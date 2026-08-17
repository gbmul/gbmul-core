// Memory Management Unit (MMU) – WASM build.
// /dev/urandom replaced with getrandom crate (uses browser crypto.getRandomValues).

/// Fill a fixed-size array with random bytes.
fn random_bytes<const N: usize>() -> [u8; N] {
    let mut buf = [0u8; N];
    // getrandom uses crypto.getRandomValues in WASM builds (requires the "js" feature).
    let _ = getrandom::getrandom(&mut buf);
    buf
}

pub struct Memory {
    pub rom: Vec<u8>,
    pub vram: [u8; 0x2000],
    pub eram: [u8; 0x8000],
    pub ram: [u8; 0x2000],
    pub oam: [u8; 0xA0],
    pub io: [u8; 0x80],
    pub hram: [u8; 0x7F],
    pub ie: u8,

    // MBC type read from cartridge header byte 0x0147
    mbc_type: u8,

    pub mbc1_rom_bank: usize,
    pub mbc1_ram_bank: usize,
    pub mbc1_ram_enable: bool,
    pub mbc1_mode: bool,

    // MBC5 registers
    mbc5_rom_bank_lo: u8,
    mbc5_rom_bank_hi: u8,
    mbc5_ram_bank: u8,
    mbc5_ram_enable: bool,

    pub div_was_written: bool,
    pub sc_write_flag: bool,

    // Set by write() when NRx4 bit 7 is written; cleared by APU after processing.
    // Replaces edge-detection in the APU: the hardware auto-clears bit 7 on trigger.
    pub apu_trigger_ch1: bool,
    pub apu_trigger_ch2: bool,
    pub apu_trigger_ch3: bool,
    pub apu_trigger_ch4: bool,
}

impl Memory {
    pub fn new() -> Self {
        let mut memory = Memory {
            rom: vec![0; 0x8000],
            vram: [0; 0x2000],
            eram: [0xFF; 0x8000],
            ram: random_bytes::<0x2000>(),
            oam: [0; 0xA0],
            io: [0; 0x80],
            hram: random_bytes::<0x7F>(),
            ie: 0,
            mbc_type: 0,
            mbc1_rom_bank: 1,
            mbc1_ram_bank: 0,
            mbc1_ram_enable: false,
            mbc1_mode: false,
            mbc5_rom_bank_lo: 1,
            mbc5_rom_bank_hi: 0,
            mbc5_ram_bank: 0,
            mbc5_ram_enable: false,
            div_was_written: false,
            sc_write_flag: false,
            apu_trigger_ch1: false,
            apu_trigger_ch2: false,
            apu_trigger_ch3: false,
            apu_trigger_ch4: false,
        };

        // Initialize I/O registers to post-boot ROM values
        memory.write(0xFF05, 0x00); // TIMA
        memory.write(0xFF06, 0x00); // TMA
        memory.write(0xFF07, 0x00); // TAC
        memory.write(0xFF10, 0x80); // NR10
        memory.write(0xFF11, 0xBF); // NR11
        memory.write(0xFF12, 0xF3); // NR12
        memory.write(0xFF14, 0xBF); // NR14
        memory.write(0xFF16, 0x3F); // NR21
        memory.write(0xFF17, 0x00); // NR22
        memory.write(0xFF19, 0xBF); // NR24
        memory.write(0xFF1A, 0x7F); // NR30
        memory.write(0xFF1B, 0xFF); // NR31
        memory.write(0xFF1C, 0x9F); // NR32
        memory.write(0xFF1E, 0xBF); // NR33
        memory.write(0xFF20, 0xFF); // NR41
        memory.write(0xFF21, 0x00); // NR42
        memory.write(0xFF22, 0x00); // NR43
        memory.write(0xFF23, 0xBF); // NR30
        memory.write(0xFF24, 0x77); // NR50
        memory.write(0xFF25, 0xF3); // NR51
        memory.write(0xFF26, 0xF1); // NR52
        memory.write(0xFF40, 0x91); // LCDC
        memory.write(0xFF42, 0x00); // SCY
        memory.write(0xFF43, 0x00); // SCX
        memory.write(0xFF45, 0x00); // LYC
        memory.write(0xFF47, 0xFC); // BGP
        memory.write(0xFF48, 0xFF); // OBP0
        memory.write(0xFF49, 0xFF); // OBP1
        memory.write(0xFF4A, 0x00); // WY
        memory.write(0xFF4B, 0x00); // WX
        memory.write(0xFFFF, 0x00); // IE

        memory
    }

    pub fn load_rom(&mut self, rom_data: &[u8]) {
        let rom_size = rom_data.len().min(0x200000);
        self.rom = vec![0; rom_size];
        self.rom[..rom_data.len().min(rom_size)].copy_from_slice(&rom_data[..rom_data.len().min(rom_size)]);
        if rom_data.len() >= 0x150 {
            self.mbc_type = rom_data[0x0147];
        }
        // Reset MBC state on new ROM load
        self.mbc1_rom_bank = 1;
        self.mbc1_ram_bank = 0;
        self.mbc1_ram_enable = false;
        self.mbc1_mode = false;
        self.mbc5_rom_bank_lo = 1;
        self.mbc5_rom_bank_hi = 0;
        self.mbc5_ram_bank = 0;
        self.mbc5_ram_enable = false;
    }

    fn is_mbc5(&self) -> bool {
        matches!(self.mbc_type, 0x19..=0x1E)
    }

    pub fn is_battery_backed(&self) -> bool {
        matches!(self.mbc_type,
            0x03 | 0x06 | 0x09 | 0x0D | 0x0F | 0x10 | 0x13 |
            0x1B | 0x1E | 0x22
        )
    }

    fn mbc5_rom_bank(&self) -> usize {
        (self.mbc5_rom_bank_lo as usize) | ((self.mbc5_rom_bank_hi as usize) << 8)
    }

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0xFF00 => self.io[0],
            0xFF04 => self.io[4],
            0x0000..=0x3FFF => {
                if (addr as usize) < self.rom.len() { self.rom[addr as usize] } else { 0xFF }
            }
            0x4000..=0x7FFF => {
                let bank = if self.is_mbc5() {
                    self.mbc5_rom_bank()
                } else {
                    self.mbc1_rom_bank.max(1)
                };
                let a = (bank * 0x4000) + ((addr - 0x4000) as usize);
                if a < self.rom.len() { self.rom[a] } else { 0xFF }
            }
            0x8000..=0x9FFF => self.vram[(addr - 0x8000) as usize],
            0xA000..=0xBFFF => {
                if self.is_mbc5() {
                    if self.mbc5_ram_enable {
                        let a = (self.mbc5_ram_bank as usize * 0x2000) + ((addr - 0xA000) as usize);
                        if a < self.eram.len() { self.eram[a] } else { 0xFF }
                    } else { 0xFF }
                } else if self.mbc1_ram_enable {
                    let bank = if self.mbc1_mode { self.mbc1_ram_bank } else { 0 };
                    let a = (bank * 0x2000) + ((addr - 0xA000) as usize);
                    if a < self.eram.len() { self.eram[a] } else { 0xFF }
                } else { 0xFF }
            }
            0xC000..=0xDFFF => self.ram[(addr - 0xC000) as usize],
            0xE000..=0xFDFF => self.ram[(addr - 0xE000) as usize],
            0xFE00..=0xFE9F => self.oam[(addr - 0xFE00) as usize],
            0xFF00..=0xFF7F => self.io[(addr - 0xFF00) as usize],
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize],
            0xFFFF => self.ie,
            _ => 0xFF,
        }
    }

    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => {
                if self.is_mbc5() {
                    self.mbc5_ram_enable = (value & 0x0F) == 0x0A;
                } else {
                    self.mbc1_ram_enable = (value & 0x0F) == 0x0A;
                }
            }
            0x2000..=0x3FFF => {
                if self.is_mbc5() {
                    if addr <= 0x2FFF {
                        self.mbc5_rom_bank_lo = value;
                    } else {
                        self.mbc5_rom_bank_hi = value & 0x01;
                    }
                } else {
                    let bank = (value & 0x1F) as usize;
                    self.mbc1_rom_bank = if bank == 0 { 1 } else { bank };
                }
            }
            0x4000..=0x5FFF => {
                if self.is_mbc5() {
                    self.mbc5_ram_bank = value & 0x0F;
                } else {
                    self.mbc1_ram_bank = (value & 0x03) as usize;
                }
            }
            0x6000..=0x7FFF => {
                if !self.is_mbc5() {
                    self.mbc1_mode = (value & 0x01) != 0;
                }
            }
            0x8000..=0x9FFF => self.vram[(addr - 0x8000) as usize] = value,
            0xA000..=0xBFFF => {
                if self.is_mbc5() {
                    if self.mbc5_ram_enable {
                        let a = (self.mbc5_ram_bank as usize * 0x2000) + ((addr - 0xA000) as usize);
                        if a < self.eram.len() { self.eram[a] = value; }
                    }
                } else if self.mbc1_ram_enable {
                    let bank = if self.mbc1_mode { self.mbc1_ram_bank } else { 0 };
                    let a = (bank * 0x2000) + ((addr - 0xA000) as usize);
                    if a < self.eram.len() { self.eram[a] = value; }
                }
            }
            0xC000..=0xDFFF => self.ram[(addr - 0xC000) as usize] = value,
            0xE000..=0xFDFF => self.ram[(addr - 0xE000) as usize] = value,
            0xFE00..=0xFE9F => self.oam[(addr - 0xFE00) as usize] = value,
            0xFF00..=0xFF7F => {
                // OAM DMA
                if addr == 0xFF46 {
                    let source = (value as u16) << 8;
                    for i in 0..0xA0 {
                        let byte = self.read(source + i as u16);
                        self.oam[i as usize] = byte;
                    }
                }
                if addr == 0xFF04 {
                    self.div_was_written = true;
                }
                if addr == 0xFF02 && (value & 0x80) != 0 {
                    self.sc_write_flag = true;
                }
                if addr == 0xFF14 && (value & 0x80) != 0 {
                    self.apu_trigger_ch1 = true;
                }
                if addr == 0xFF19 && (value & 0x80) != 0 {
                    self.apu_trigger_ch2 = true;
                }
                if addr == 0xFF1E && (value & 0x80) != 0 {
                    self.apu_trigger_ch3 = true;
                }
                if addr == 0xFF23 && (value & 0x80) != 0 {
                    self.apu_trigger_ch4 = true;
                }
                self.io[(addr - 0xFF00) as usize] = value;
            }
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize] = value,
            0xFFFF => self.ie = value,
            _ => {}
        }
    }

    pub fn read_direct(&self, addr: u16) -> u8 {
        match addr {
            0xFF00..=0xFF7F => self.io[(addr - 0xFF00) as usize],
            _ => self.read(addr),
        }
    }

    pub fn write_direct(&mut self, addr: u16, value: u8) {
        match addr {
            0xFF00..=0xFF7F => self.io[(addr - 0xFF00) as usize] = value,
            _ => self.write(addr, value),
        }
    }

    pub fn sync_joypad_to(&mut self, joypad: &super::joypad::Joypad) {
        // Trigger joypad interrupt ($FF0F bit 4) when any button is newly pressed.
        // This fires the ROM's joypad ISR at $0060, which updates the entropy pool
        // at $FF82/$FF83/$FF84 using DIV and LY as entropy sources.
        if joypad.has_new_press() {
            self.io[0x0F] |= 0x10;
        }
        self.io[0] = joypad.read();
    }

    pub fn sync_joypad_from(&self, joypad: &mut super::joypad::Joypad) {
        joypad.write(self.io[0]);
        joypad.update_prev();
    }
}
