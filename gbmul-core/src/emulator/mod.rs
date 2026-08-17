// Game Boy emulator core – WASM build.
// /dev/urandom replaced with getrandom; no SDL2 / filesystem usage.

pub mod cpu;
pub mod memory;
pub mod cartridge;
pub mod gpu;
pub mod joypad;
pub mod timer;
pub mod serial;
pub mod link;
pub mod apu;

use cpu::Cpu;
use memory::Memory;
use gpu::Gpu;
pub use joypad::Joypad;
use timer::Timer;
use serial::Serial;
use link::{LinkCable, LoopbackLink};
use apu::Apu;

pub struct Emulator {
    pub cpu: Cpu,
    pub memory: Memory,
    pub gpu: Gpu,
    pub joypad: Joypad,
    pub timer: Timer,
    pub serial: Serial,
    pub link: Box<dyn LinkCable>,
    pub apu: Apu,
    pub running: bool,
    pub cycles: u64,
    pub audio_enabled: bool,
}

impl Emulator {
    pub fn new() -> Self {
        Emulator {
            cpu: Cpu::new(),
            memory: Memory::new(),
            gpu: Gpu::new(),
            joypad: Joypad::new(),
            timer: Timer::new(),
            serial: Serial::new(),
            link: Box::new(LoopbackLink),
            apu: Apu::new(),
            running: false,
            cycles: 0,
            audio_enabled: true,
        }
    }

    /// Replace the link-cable backend. Call before starting emulation.
    pub fn set_link(&mut self, link: Box<dyn LinkCable>) {
        self.link = link;
    }

    /// Enable or disable audio emulation (APU stepping and sample generation).
    /// Disabling saves CPU and is useful for silent play on handheld.
    pub fn set_audio_enabled(&mut self, enabled: bool) {
        self.audio_enabled = enabled;
        if !enabled {
            self.apu.sample_buffer.clear();
        }
    }

    pub fn load_rom(&mut self, rom_data: &[u8]) -> Result<(), String> {
        self.memory.load_rom(rom_data);
        self.running = true;
        log::info!("ROM loaded ({} bytes)", rom_data.len());

        // Warm up with a random number of frames to seed the ROM's RNG.
        // Skip for battery-backed carts: their boot sequences (e.g. LSDJ) are
        // timing-sensitive and warmup frames corrupt their SRAM test / checksum.
        // Note: GbEmuPair::load_rom skips this to keep both emulators in sync.
        if !self.memory.is_battery_backed() {
            let mut buf = [0u8; 1];
            let _ = getrandom::getrandom(&mut buf);
            for _ in 0..buf[0] {
                self.run_frame();
            }
        }

        Ok(())
    }

    pub fn reset(&mut self) {
        self.cpu = Cpu::new();
        self.gpu = Gpu::new();
        self.joypad = Joypad::new();
        self.timer = Timer::new();
        self.serial = Serial::new();
        // self.link is intentionally NOT reset: two instances connected via
        // SharedLink remain paired across a reset.
        // eram (cartridge SRAM) intentionally NOT cleared: battery-backed ROMs
        // retain save data across resets.
        self.memory.ram.fill(0x00);
        self.memory.oam.fill(0x00);
        self.memory.io.fill(0x00);
        self.memory.hram.fill(0x00);
        self.memory.mbc1_rom_bank = 1;
        self.memory.mbc1_ram_bank = 0;
        self.memory.mbc1_ram_enable = false;
        self.memory.mbc1_mode = false;
        self.memory.apu_trigger_ch1 = false;
        self.memory.apu_trigger_ch2 = false;
        self.memory.apu_trigger_ch3 = false;
        self.memory.apu_trigger_ch4 = false;
        self.apu = Apu::new();
        self.running = true;
        self.cycles = 0;
    }

    /// Run one frame (~70 224 T-cycles = 59.7 Hz).
    pub fn run_frame(&mut self) {
        if !self.running { return; }
        self.frame_start_polls();
        self.run_slice(70224);
    }

    /// Process pending serial bytes at the start of a frame.
    /// Must be called once before run_slice() at each new frame boundary.
    pub fn frame_start_polls(&mut self) {
        if self.serial.transferring && self.serial.master_waiting {
            if let Some(incoming) = self.link.poll_incoming() {
                self.serial.receive_byte(incoming);
            }
        }
        if self.serial.slave_pending && !self.serial.transferring {
            let sb_out = self.serial.sb;
            let sc_saved = self.serial.sc;
            if let Some(incoming) = self.link.slave_exchange(sb_out) {
                self.serial.start_transfer(sb_out, sc_saved);
                let _ = self.serial.take_outgoing();
                self.serial.receive_byte(incoming);
            }
        }
    }

    /// Run up to `budget` T-cycles of CPU/peripheral execution.
    /// Does NOT do frame-start serial polls — call frame_start_polls() first.
    /// Returns the number of T-cycles actually executed (≥ budget due to
    /// instruction boundaries; the caller should track total against 70 224).
    pub fn run_slice(&mut self, budget: u32) -> u32 {
        if !self.running { return 0; }
        let mut done = 0u32;
        while done < budget {
            self.memory.sync_joypad_to(&self.joypad);
            self.timer.sync_to_memory(&mut self.memory);
            self.memory.div_was_written = false;
            self.memory.sc_write_flag = false;

            let cycles = self.cpu.step(&mut self.memory);

            if self.memory.div_was_written {
                self.timer.write_div();
                self.memory.write_direct(0xFF04, self.timer.read_div());
            }

            // ── Serial: handle SC write (game started a transfer) ─────────────
            if self.memory.sc_write_flag {
                self.memory.sc_write_flag = false;
                let sb = self.memory.read_direct(0xFF01);
                let sc = self.memory.read_direct(0xFF02);
                let is_master = sc & 0x01 != 0;

                if is_master {
                    self.serial.start_transfer(sb, sc);
                    let outgoing = self.serial.take_outgoing().unwrap_or(0xFF);
                    self.link.exchange(outgoing);
                    if let Some(incoming) = self.link.poll_incoming() {
                        self.serial.receive_byte(incoming);
                    } else {
                        self.serial.master_waiting = true;
                    }
                } else {
                    if let Some(incoming) = self.link.slave_exchange(sb) {
                        self.serial.start_transfer(sb, sc);
                        let _ = self.serial.take_outgoing();
                        self.serial.receive_byte(incoming);
                    } else {
                        self.serial.sc = sc;
                        self.serial.sb = sb;
                        self.serial.slave_pending = true;
                    }
                }
            }

            self.memory.sync_joypad_from(&mut self.joypad);
            self.timer.sync_from_memory(&self.memory);

            if self.cpu.check_interrupts(&mut self.memory) {
                done += 20;
            }

            if self.timer.step(cycles) {
                let if_reg = self.memory.read(0xFF0F);
                self.memory.write(0xFF0F, if_reg | 0x04);
            }

            if self.serial.step(cycles) {
                self.serial.sync_to_memory(&mut self.memory);
                let if_reg = self.memory.read(0xFF0F);
                self.memory.write(0xFF0F, if_reg | 0x08);
            }

            // Inline sub-frame polls — deliver partner bytes between instructions.
            if self.serial.transferring && self.serial.master_waiting {
                if let Some(incoming) = self.link.poll_incoming() {
                    self.serial.receive_byte(incoming);
                }
            }
            if self.serial.slave_pending && !self.serial.transferring {
                let sb_saved = self.serial.sb;
                let sc_saved = self.serial.sc;
                if let Some(incoming) = self.link.slave_exchange(sb_saved) {
                    self.serial.start_transfer(sb_saved, sc_saved);
                    let _ = self.serial.take_outgoing();
                    self.serial.receive_byte(incoming);
                }
            }

            self.gpu.step(cycles, &mut self.memory);
            if self.audio_enabled {
                let trigger_ch1 = self.memory.apu_trigger_ch1;
                let trigger_ch2 = self.memory.apu_trigger_ch2;
                let trigger_ch3 = self.memory.apu_trigger_ch3;
                let trigger_ch4 = self.memory.apu_trigger_ch4;
                self.memory.apu_trigger_ch1 = false;
                self.memory.apu_trigger_ch2 = false;
                self.memory.apu_trigger_ch3 = false;
                self.memory.apu_trigger_ch4 = false;
                self.apu.step(cycles, &self.memory.io, trigger_ch1, trigger_ch2, trigger_ch3, trigger_ch4);
            } else {
                // still clear triggers to avoid accumulation
                self.memory.apu_trigger_ch1 = false;
                self.memory.apu_trigger_ch2 = false;
                self.memory.apu_trigger_ch3 = false;
                self.memory.apu_trigger_ch4 = false;
                self.apu.sample_buffer.clear();
            }

            done += cycles;
            self.cycles += cycles as u64;
        }
        done
    }

    /// Get the current 160×144 framebuffer (grayscale 0–3).
    pub fn get_framebuffer(&self) -> &[u8] {
        self.gpu.get_framebuffer()
    }

    pub fn save_state(&self) -> crate::state::EmulatorState {
        crate::state::EmulatorState {
            cpu_a: self.cpu.a,
            cpu_b: self.cpu.b,
            cpu_c: self.cpu.c,
            cpu_d: self.cpu.d,
            cpu_e: self.cpu.e,
            cpu_h: self.cpu.h,
            cpu_l: self.cpu.l,
            cpu_f: self.cpu.f.bits(),
            cpu_sp: self.cpu.sp,
            cpu_pc: self.cpu.pc,
            cpu_ime: self.cpu.ime,
            cpu_ime_scheduled: self.cpu.ime_scheduled,
            cpu_halted: self.cpu.halted,

            vram: self.memory.vram.to_vec(),
            eram: self.memory.eram.to_vec(),
            ram:  self.memory.ram.to_vec(),
            oam:  self.memory.oam.to_vec(),
            io:   self.memory.io.to_vec(),
            hram: self.memory.hram.to_vec(),
            ie:   self.memory.ie,

            mbc1_rom_bank:   self.memory.mbc1_rom_bank,
            mbc1_ram_bank:   self.memory.mbc1_ram_bank,
            mbc1_ram_enable: self.memory.mbc1_ram_enable,
            mbc1_mode:       self.memory.mbc1_mode,

            timer_div:        self.timer.div,
            timer_tima:       self.timer.tima,
            timer_tma:        self.timer.tma,
            timer_tac:        self.timer.tac,
            timer_tima_cycles: self.timer.tima_cycles,

            gpu_mode:   self.gpu.mode.to_u8(),
            gpu_line:   self.gpu.line,
            gpu_cycles: self.gpu.cycles,

            cycles: self.cycles,
        }
    }

    pub fn restore_state(&mut self, s: &crate::state::EmulatorState) {
        self.cpu.a              = s.cpu_a;
        self.cpu.b              = s.cpu_b;
        self.cpu.c              = s.cpu_c;
        self.cpu.d              = s.cpu_d;
        self.cpu.e              = s.cpu_e;
        self.cpu.h              = s.cpu_h;
        self.cpu.l              = s.cpu_l;
        self.cpu.f              = cpu::Flags::from_bits_truncate(s.cpu_f);
        self.cpu.sp             = s.cpu_sp;
        self.cpu.pc             = s.cpu_pc;
        self.cpu.ime            = s.cpu_ime;
        self.cpu.ime_scheduled  = s.cpu_ime_scheduled;
        self.cpu.halted         = s.cpu_halted;

        self.memory.vram.copy_from_slice(&s.vram);
        self.memory.eram.copy_from_slice(&s.eram);
        self.memory.ram.copy_from_slice(&s.ram);
        self.memory.oam.copy_from_slice(&s.oam);
        self.memory.io.copy_from_slice(&s.io);
        self.memory.hram.copy_from_slice(&s.hram);
        self.memory.ie             = s.ie;
        self.memory.mbc1_rom_bank  = s.mbc1_rom_bank;
        self.memory.mbc1_ram_bank  = s.mbc1_ram_bank;
        self.memory.mbc1_ram_enable = s.mbc1_ram_enable;
        self.memory.mbc1_mode      = s.mbc1_mode;

        self.timer.div         = s.timer_div;
        self.timer.tima        = s.timer_tima;
        self.timer.tma         = s.timer_tma;
        self.timer.tac         = s.timer_tac;
        self.timer.tima_cycles = s.timer_tima_cycles;

        self.gpu.mode   = gpu::GpuMode::from_u8(s.gpu_mode);
        self.gpu.line   = s.gpu_line;
        self.gpu.cycles = s.gpu_cycles;

        self.cycles = s.cycles;
        // Serial state is not saved — the link protocol re-syncs on reconnect.
    }
}
