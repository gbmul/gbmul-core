// Timer and divider registers

use crate::emulator::memory::Memory;

pub struct Timer {
    pub div: u16,          // Internal 16-bit counter; readable DIV = upper 8 bits
    pub tima: u8,          // Timer counter
    pub tma: u8,           // Timer modulo
    pub tac: u8,           // Timer control
    pub tima_cycles: u32,  // Fractional T-cycles for TIMA
}

impl Timer {
    pub fn new() -> Self {
        // DMG post-boot state per Pan Docs / whichboot.gb measurements.
        // The internal 16-bit counter is 0xAB34 at PC=$0100, giving readable DIV=$AB.
        // Using the accurate value is essential for games (e.g. Tetris) that sample DIV
        // at a fixed cycle offset from $0100 to determine their RNG seed.  Randomising
        // this register breaks the sync completely.
        Timer {
            div: 0xAB34,
            tima: 0x00,
            tma: 0x00,
            tac: 0x00, // timer disabled post-boot (TAC=$F8 but bits 2-0 all 0)
            tima_cycles: 0,
        }
    }
    
    pub fn step(&mut self, cycles: u32) -> bool {
        // Advance the internal 16-bit counter by the raw T-cycle count.
        // The readable DIV ($FF04) = upper 8 bits, so it increments every 256 T-cycles
        // = 4194304 / 256 = 16384 Hz, which matches real hardware.
        self.div = self.div.wrapping_add(cycles as u16);

        // Update TIMA counter if enabled
        if self.tac & 0b100 != 0 {
            self.tima_cycles += cycles;
            
            // Determine divider based on TAC bits 0-1
            let divider = match self.tac & 0b11 {
                0 => 1024,  // 4096 Hz
                1 => 16,    // 262144 Hz
                2 => 64,    // 65536 Hz
                3 => 256,   // 16384 Hz
                _ => unreachable!(),
            };
            
            while self.tima_cycles >= divider {
                self.tima = self.tima.wrapping_add(1);
                self.tima_cycles -= divider;
                
                // Check for overflow
                if self.tima == 0 {
                    self.tima = self.tma;
                    return true; // Timer interrupt requested
                }
            }
        }
        
        false
    }
    
    pub fn read_div(&self) -> u8 {
        (self.div >> 8) as u8
    }
    
    pub fn write_div(&mut self) {
        // Any write to $FF04 resets the entire internal 16-bit counter to 0 (real HW).
        log::info!("DIV reset to 0 (was 0x{:02X})", self.read_div());
        self.div = 0;
        self.tima_cycles = 0;
    }
    
    pub fn read_tima(&self) -> u8 {
        self.tima
    }
    
    pub fn write_tima(&mut self, value: u8) {
        self.tima = value;
    }
    
    pub fn read_tma(&self) -> u8 {
        self.tma
    }
    
    pub fn write_tma(&mut self, value: u8) {
        self.tma = value;
    }
    
    pub fn read_tac(&self) -> u8 {
        self.tac
    }
    
    pub fn write_tac(&mut self, value: u8) {
        self.tac = value & 0b111;
    }
    
    /// Sync timer state to memory
    pub fn sync_to_memory(&self, memory: &mut Memory) {
        memory.write_direct(0xFF04, self.read_div());
        memory.write_direct(0xFF05, self.tima);
        memory.write_direct(0xFF06, self.tma);
        memory.write_direct(0xFF07, self.tac);
    }
    
    /// Sync timer state from memory writes
    pub fn sync_from_memory(&mut self, memory: &Memory) {
        // Only sync writable registers
        self.tima = memory.read_direct(0xFF05);
        self.tma = memory.read_direct(0xFF06);
        self.tac = memory.read_direct(0xFF07) & 0b111;
    }
}
