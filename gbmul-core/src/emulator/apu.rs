// APU register offsets within memory.io[] (io[0] = 0xFF00):
//   CH1: NR10=0x10 NR11=0x11 NR12=0x12 NR13=0x13 NR14=0x14
//   CH2: NR21=0x16 NR22=0x17 NR23=0x18 NR24=0x19
//   CH3: NR30=0x1A NR31=0x1B NR32=0x1C NR33=0x1D NR34=0x1E
//   CH4: NR41=0x20 NR42=0x21 NR43=0x22 NR44=0x23
//   NR50=0x24  NR51=0x25  NR52=0x26
//   Wave RAM: io[0x30..0x40]
//
// Trigger detection: memory.write() sets Memory::apu_trigger_chN when NRx4 bit 7
// is written. run_slice() reads and clears the flag before calling Apu::step().

const SAMPLE_RATE: f32 = 44_100.0;
const CPU_FREQ: f32 = 4_194_304.0;
const CYCLES_PER_SAMPLE: f32 = CPU_FREQ / SAMPLE_RATE; // ≈ 95.1

// Duty-cycle waveforms: DUTY_TABLE[duty][step] = output bit (0 or 1).
const DUTY_TABLE: [[u8; 8]; 4] = [
    [0, 0, 0, 0, 0, 0, 0, 1], // 12.5%
    [1, 0, 0, 0, 0, 0, 0, 1], // 25%
    [1, 0, 0, 0, 0, 1, 1, 1], // 50%
    [0, 1, 1, 1, 1, 1, 1, 0], // 75%
];

// HPF simulates the GB output capacitor. Mainly handles CH3 wave DC offset;
// CH1/CH2/CH4 use a centred formula that already produces ~0 DC.
const HPF_CHARGE: f32 = 0.999_77;

// LPF simulates the analog rolloff of the GB's output stage (amp + capacitors).
// Real hardware naturally rounds square-wave edges; without this, direct digital
// generation sounds harsher than original hardware. Alpha = 2π·fc / (2π·fc + sr).
// fc ≈ 7 kHz at 44100 Hz → warm without being muffled. Raise toward 1.0 for brighter.
const LPF_ALPHA: f32 = 0.5;

// ── CH1 & CH2 — Pulse ─────────────────────────────────────────────────────────

struct PulseChannel {
    freq_timer:  u32,
    freq_period: u32,  // sweep-managed reload value; bypasses register when sweep active
    duty_step:   u8,

    volume:      u8,
    env_period:  u8,
    env_timer:   u8,
    env_amplify: bool,

    length_counter: u16,
    length_enable:  bool,

    // Frequency sweep — CH1 only; sweep_enabled is always false for CH2.
    sweep_shadow:  u16, // shadow copy of 11-bit frequency register
    sweep_timer:   u8,  // countdown timer (1–8)
    sweep_enabled: bool,

    enabled: bool,
    dac_on:  bool,
}

impl PulseChannel {
    fn new() -> Self {
        PulseChannel {
            freq_timer:     8,
            freq_period:    8,
            duty_step:      0,
            volume:         0,
            env_period:     0,
            env_timer:      0,
            env_amplify:    false,
            length_counter: 0,
            length_enable:  false,
            sweep_shadow:   0,
            sweep_timer:    8,
            sweep_enabled:  false,
            enabled:        false,
            dac_on:         false,
        }
    }

    // nr10 = NR10 value for CH1, 0x00 for CH2 (no sweep).
    fn trigger(&mut self, nr1: u8, nr2: u8, nr3: u8, nr4: u8, nr10: u8) {
        if self.length_counter == 0 {
            self.length_counter = 64 - (nr1 & 0x3F) as u16;
        }
        self.dac_on      = (nr2 & 0xF8) != 0;
        self.volume      = (nr2 >> 4) & 0x0F;
        self.env_amplify = (nr2 & 0x08) != 0;
        self.env_period  = nr2 & 0x07;
        self.env_timer   = self.env_period;

        let freq_reg     = ((nr4 as u32 & 0x07) << 8) | nr3 as u32;
        self.freq_period = (2048 - freq_reg) * 4;
        self.freq_timer  = self.freq_period;

        // Sweep initialisation (harmless for CH2: nr10=0 → sp=0, ss=0).
        let sp = (nr10 >> 4) & 0x07;
        let ss = nr10 & 0x07;
        self.sweep_shadow  = freq_reg as u16;
        self.sweep_timer   = if sp == 0 { 8 } else { sp };
        self.sweep_enabled = sp != 0 || ss != 0;

        // Enable before overflow check so the check can disable if needed.
        self.enabled = self.dac_on;

        // Trigger overflow check: if new frequency would exceed 2047, disable immediately.
        if ss != 0 {
            let _ = self.sweep_compute(nr10);
        }
    }

    fn step(&mut self, nr2: u8, nr3: u8, nr4: u8, cycles: u32) {
        self.dac_on = (nr2 & 0xF8) != 0;
        if !self.dac_on { self.enabled = false; }
        self.length_enable = (nr4 & 0x40) != 0;
        if !self.enabled { return; }

        // Sweep-active channels use the shadow-derived period (updated by tick_sweep).
        // Inactive channels read the current register value (allows mid-note pitch writes).
        let period = if self.sweep_enabled {
            self.freq_period
        } else {
            let freq_reg = ((nr4 as u32 & 0x07) << 8) | nr3 as u32;
            (2048 - freq_reg) * 4
        };

        if period > 0 {
            let mut remaining = cycles;
            while remaining >= self.freq_timer {
                remaining       -= self.freq_timer;
                self.freq_timer  = period;
                self.duty_step   = (self.duty_step + 1) & 7;
            }
            self.freq_timer -= remaining;
        }
    }

    fn tick_length(&mut self) {
        if self.length_enable && self.length_counter > 0 {
            self.length_counter -= 1;
            if self.length_counter == 0 { self.enabled = false; }
        }
    }

    fn tick_envelope(&mut self) {
        if self.env_period == 0 { return; }
        if self.env_timer > 0 { self.env_timer -= 1; }
        if self.env_timer == 0 {
            self.env_timer = self.env_period;
            if self.env_amplify  { if self.volume < 15 { self.volume += 1; } }
            else                 { if self.volume >  0 { self.volume -= 1; } }
        }
    }

    // CH1 only — called at 128 Hz (frame sequencer steps 2 and 6).
    // NR10: bits 6–4 = sweep period (0–7), bit 3 = negate, bits 2–0 = shift.
    fn tick_sweep(&mut self, nr10: u8) {
        let period = (nr10 >> 4) & 0x07;
        let shift  = nr10 & 0x07;

        if self.sweep_timer > 0 { self.sweep_timer -= 1; }
        if self.sweep_timer == 0 {
            self.sweep_timer = if period == 0 { 8 } else { period };
            if self.sweep_enabled && period != 0 {
                if let Some(new_freq) = self.sweep_compute(nr10) {
                    if shift != 0 {
                        self.sweep_shadow = new_freq;
                        self.freq_period  = (2048 - new_freq as u32) * 4;
                        // Second overflow check with the updated shadow.
                        let _ = self.sweep_compute(nr10);
                    }
                }
            }
        }
    }

    // Compute next sweep frequency. Returns None and disables channel on overflow.
    fn sweep_compute(&mut self, nr10: u8) -> Option<u16> {
        let shift  = nr10 & 0x07;
        let negate = (nr10 & 0x08) != 0;
        let delta  = self.sweep_shadow >> shift;
        let new_freq = if negate {
            self.sweep_shadow.saturating_sub(delta)
        } else {
            self.sweep_shadow + delta
        };
        if new_freq > 2047 {
            self.enabled = false;
            None
        } else {
            Some(new_freq)
        }
    }

    // Centred DAC output: (bit − 0.5) × volume/7.5 → [−1.0, +1.0].
    // volume=0 → 0.0 (true silence, no DC offset accumulation).
    fn sample(&self, nr1: u8) -> f32 {
        if !self.enabled || !self.dac_on { return 0.0; }
        let duty = (nr1 >> 6) & 0x03;
        let bit  = DUTY_TABLE[duty as usize][self.duty_step as usize];
        (bit as f32 - 0.5) * (self.volume as f32 / 7.5)
    }
}

// ── CH3 — Wave ───────────────────────────────────────────────────────────────

struct WaveChannel {
    freq_timer:  u32,
    wave_pos:    u8,    // 0–31 nibble position

    length_counter: u16,
    length_enable:  bool,

    enabled: bool,
    dac_on:  bool,
}

impl WaveChannel {
    fn new() -> Self {
        WaveChannel {
            freq_timer:     8,
            wave_pos:       0,
            length_counter: 0,
            length_enable:  false,
            enabled:        false,
            dac_on:         false,
        }
    }

    fn trigger(&mut self, nr30: u8, nr31: u8, nr33: u8, nr34: u8) {
        self.dac_on = (nr30 & 0x80) != 0;
        if self.length_counter == 0 {
            self.length_counter = 256 - nr31 as u16;
        }
        let freq_reg    = ((nr34 as u32 & 0x07) << 8) | nr33 as u32;
        self.freq_timer = (2048 - freq_reg) * 2;
        self.wave_pos   = 0;
        self.enabled    = self.dac_on;
    }

    fn step(&mut self, nr30: u8, nr33: u8, nr34: u8, cycles: u32) {
        self.dac_on = (nr30 & 0x80) != 0;
        if !self.dac_on { self.enabled = false; }
        self.length_enable = (nr34 & 0x40) != 0;
        if !self.enabled { return; }

        let freq_reg = ((nr34 as u32 & 0x07) << 8) | nr33 as u32;
        let period   = (2048 - freq_reg) * 2;
        if period > 0 {
            let mut remaining = cycles;
            while remaining >= self.freq_timer {
                remaining       -= self.freq_timer;
                self.freq_timer  = period;
                self.wave_pos    = (self.wave_pos + 1) & 31;
            }
            self.freq_timer -= remaining;
        }
    }

    fn tick_length(&mut self) {
        if self.length_enable && self.length_counter > 0 {
            self.length_counter -= 1;
            if self.length_counter == 0 { self.enabled = false; }
        }
    }

    fn sample(&self, nr32: u8, wave_ram: &[u8]) -> f32 {
        if !self.enabled || !self.dac_on { return 0.0; }
        let byte   = wave_ram[(self.wave_pos / 2) as usize];
        let nibble = if self.wave_pos & 1 == 0 { byte >> 4 } else { byte & 0x0F };
        let shifted = match (nr32 >> 5) & 0x03 {
            0 => 0,
            1 => nibble,
            2 => nibble >> 1,
            _ => nibble >> 2,
        };
        shifted as f32 / 7.5 - 1.0
    }
}

// ── CH4 — Noise (LFSR) ────────────────────────────────────────────────────────

struct NoiseChannel {
    lfsr:       u16,   // 15-bit shift register
    freq_timer: u32,

    volume:      u8,
    env_period:  u8,
    env_timer:   u8,
    env_amplify: bool,

    length_counter: u16,
    length_enable:  bool,

    enabled: bool,
    dac_on:  bool,
}

impl NoiseChannel {
    fn new() -> Self {
        NoiseChannel {
            lfsr:           0x7FFF,
            freq_timer:     8,
            volume:         0,
            env_period:     0,
            env_timer:      0,
            env_amplify:    false,
            length_counter: 0,
            length_enable:  false,
            enabled:        false,
            dac_on:         false,
        }
    }

    fn lfsr_period(nr43: u8) -> u32 {
        let r = (nr43 & 0x07) as u32;
        let s = ((nr43 >> 4) & 0x0F) as u32;
        (if r == 0 { 8 } else { r * 16 }) << s
    }

    fn trigger(&mut self, nr41: u8, nr42: u8, nr43: u8) {
        if self.length_counter == 0 {
            self.length_counter = 64 - (nr41 & 0x3F) as u16;
        }
        self.dac_on      = (nr42 & 0xF8) != 0;
        self.volume      = (nr42 >> 4) & 0x0F;
        self.env_amplify = (nr42 & 0x08) != 0;
        self.env_period  = nr42 & 0x07;
        self.env_timer   = self.env_period;
        self.freq_timer  = Self::lfsr_period(nr43).max(1);
        self.lfsr        = 0x7FFF;
        self.enabled     = self.dac_on;
    }

    fn step(&mut self, nr42: u8, nr43: u8, nr44: u8, cycles: u32) {
        self.dac_on = (nr42 & 0xF8) != 0;
        if !self.dac_on { self.enabled = false; }
        self.length_enable = (nr44 & 0x40) != 0;
        if !self.enabled { return; }

        let period = Self::lfsr_period(nr43).max(1);
        let short  = (nr43 & 0x08) != 0;
        let mut remaining = cycles;
        while remaining >= self.freq_timer {
            remaining       -= self.freq_timer;
            self.freq_timer  = period;
            let xor = ((self.lfsr ^ (self.lfsr >> 1)) & 1) as u16;
            self.lfsr = (self.lfsr >> 1) | (xor << 14);
            if short {
                self.lfsr = (self.lfsr & !(1 << 6)) | (xor << 6);
            }
        }
        self.freq_timer -= remaining;
    }

    fn tick_length(&mut self) {
        if self.length_enable && self.length_counter > 0 {
            self.length_counter -= 1;
            if self.length_counter == 0 { self.enabled = false; }
        }
    }

    fn tick_envelope(&mut self) {
        if self.env_period == 0 { return; }
        if self.env_timer > 0 { self.env_timer -= 1; }
        if self.env_timer == 0 {
            self.env_timer = self.env_period;
            if self.env_amplify  { if self.volume < 15 { self.volume += 1; } }
            else                 { if self.volume >  0 { self.volume -= 1; } }
        }
    }

    fn sample(&self) -> f32 {
        if !self.enabled || !self.dac_on { return 0.0; }
        let bit = ((self.lfsr & 1) ^ 1) as u8;
        (bit as f32 - 0.5) * (self.volume as f32 / 7.5)
    }
}

// ── APU ───────────────────────────────────────────────────────────────────────

pub struct Apu {
    frame_seq_cycles: u32,
    frame_seq_step:   u8,

    ch1: PulseChannel,
    ch2: PulseChannel,
    ch3: WaveChannel,
    ch4: NoiseChannel,

    sample_cycles: f32,
    hpf_cap_l:     f32,
    hpf_cap_r:     f32,
    lpf_cap_l:     f32,
    lpf_cap_r:     f32,
    pub sample_buffer: Vec<f32>, // interleaved L/R, drained each frame by JS
}

impl Apu {
    pub fn new() -> Self {
        Apu {
            frame_seq_cycles: 0,
            frame_seq_step:   0,
            ch1:              PulseChannel::new(),
            ch2:              PulseChannel::new(),
            ch3:              WaveChannel::new(),
            ch4:              NoiseChannel::new(),
            sample_cycles:    0.0,
            hpf_cap_l:        0.0,
            hpf_cap_r:        0.0,
            lpf_cap_l:        0.0,
            lpf_cap_r:        0.0,
            sample_buffer:    Vec::with_capacity(800 * 2),
        }
    }

    pub fn step(&mut self, cycles: u32, io: &[u8],
                trigger_ch1: bool, trigger_ch2: bool,
                trigger_ch3: bool, trigger_ch4: bool) {
        if io[0x26] & 0x80 == 0 {
            self.push_silence(cycles);
            return;
        }

        // Triggers: io[0x10] = NR10 (sweep config for CH1).
        if trigger_ch1 { self.ch1.trigger(io[0x11], io[0x12], io[0x13], io[0x14], io[0x10]); }
        if trigger_ch2 { self.ch2.trigger(io[0x16], io[0x17], io[0x18], io[0x19], 0x00); }
        if trigger_ch3 { self.ch3.trigger(io[0x1A], io[0x1B], io[0x1D], io[0x1E]); }
        if trigger_ch4 { self.ch4.trigger(io[0x20], io[0x21], io[0x22]); }

        // Frame sequencer — fires every 8192 T-cycles (512 Hz).
        self.frame_seq_cycles += cycles;
        while self.frame_seq_cycles >= 8192 {
            self.frame_seq_cycles -= 8192;
            let step = self.frame_seq_step;
            self.frame_seq_step = (self.frame_seq_step + 1) & 7;
            // Length counter ticks at 256 Hz (steps 0,2,4,6).
            if step % 2 == 0 {
                self.ch1.tick_length();
                self.ch2.tick_length();
                self.ch3.tick_length();
                self.ch4.tick_length();
            }
            // CH1 frequency sweep ticks at 128 Hz (steps 2 and 6).
            if step == 2 || step == 6 {
                self.ch1.tick_sweep(io[0x10]);
            }
            // Volume envelope ticks at 64 Hz (step 7).
            if step == 7 {
                self.ch1.tick_envelope();
                self.ch2.tick_envelope();
                self.ch4.tick_envelope();
            }
        }

        self.ch1.step(io[0x12], io[0x13], io[0x14], cycles);
        self.ch2.step(io[0x17], io[0x18], io[0x19], cycles);
        self.ch3.step(io[0x1A], io[0x1D], io[0x1E], cycles);
        self.ch4.step(io[0x21], io[0x22], io[0x23], cycles);

        // Sample generation: per-channel samples mixed to L/R via NR51 panning,
        // then scaled by NR50 master volume, then high-pass filtered.
        self.sample_cycles += cycles as f32;
        while self.sample_cycles >= CYCLES_PER_SAMPLE {
            self.sample_cycles -= CYCLES_PER_SAMPLE;

            let s1 = self.ch1.sample(io[0x11]);
            let s2 = self.ch2.sample(io[0x16]);
            let s3 = self.ch3.sample(io[0x1C], &io[0x30..0x40]);
            let s4 = self.ch4.sample();

            // NR51 (io[0x25]): bits 7-4 = CH4/3/2/1 right, bits 3-0 = CH4/3/2/1 left.
            let nr51 = io[0x25];
            let raw_l = 0.25 * (
                if nr51 & 0x01 != 0 { s1 } else { 0.0 }
              + if nr51 & 0x02 != 0 { s2 } else { 0.0 }
              + if nr51 & 0x04 != 0 { s3 } else { 0.0 }
              + if nr51 & 0x08 != 0 { s4 } else { 0.0 }
            );
            let raw_r = 0.25 * (
                if nr51 & 0x10 != 0 { s1 } else { 0.0 }
              + if nr51 & 0x20 != 0 { s2 } else { 0.0 }
              + if nr51 & 0x40 != 0 { s3 } else { 0.0 }
              + if nr51 & 0x80 != 0 { s4 } else { 0.0 }
            );

            // NR50 (io[0x24]): bits 6-4 = right volume (0-7), bits 2-0 = left volume (0-7).
            // Hardware range is 1–8×; normalise so vol=7 → 1.0.
            let nr50  = io[0x24];
            let vol_l = ((nr50 & 0x07) as f32 + 1.0) / 8.0;
            let vol_r = (((nr50 >> 4) & 0x07) as f32 + 1.0) / 8.0;

            // High-pass filter (DC removal) per side.
            let sl = raw_l - self.hpf_cap_l;
            self.hpf_cap_l = raw_l - sl * HPF_CHARGE;
            let sr = raw_r - self.hpf_cap_r;
            self.hpf_cap_r = raw_r - sr * HPF_CHARGE;

            // NR50 master volume.
            let out_l = sl * vol_l;
            let out_r = sr * vol_r;

            // Low-pass filter: rounds square-wave edges, matches analog output stage rolloff.
            self.lpf_cap_l += LPF_ALPHA * (out_l - self.lpf_cap_l);
            self.lpf_cap_r += LPF_ALPHA * (out_r - self.lpf_cap_r);

            self.sample_buffer.push(self.lpf_cap_l); // L
            self.sample_buffer.push(self.lpf_cap_r); // R
        }
    }

    fn push_silence(&mut self, cycles: u32) {
        self.sample_cycles += cycles as f32;
        while self.sample_cycles >= CYCLES_PER_SAMPLE {
            self.sample_cycles -= CYCLES_PER_SAMPLE;
            self.sample_buffer.push(0.0);
            self.sample_buffer.push(0.0);
        }
    }
}
