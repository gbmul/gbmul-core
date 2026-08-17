// SM83 CPU implementation (Game Boy CPU, Z80-like)
// Implements all 8-bit and 16-bit instructions

use bitflags::bitflags;

bitflags! {
    /// CPU flags register
    pub struct Flags: u8 {
        const ZERO       = 0b1000_0000; // Z flag
        const SUBTRACT   = 0b0100_0000; // N flag
        const HALF_CARRY = 0b0010_0000; // H flag
        const CARRY      = 0b0001_0000; // C flag
    }
}

pub struct Cpu {
    // 8-bit registers
    pub a: u8,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    pub f: Flags,
    
    // 16-bit registers
    pub sp: u16, // Stack pointer
    pub pc: u16, // Program counter
    
    // Control
    pub ime: bool,  // Interrupt master enable
    pub ime_scheduled: bool,  // IME will be enabled after next instruction
    pub halted: bool,
}

impl Cpu {
    pub fn new() -> Self {
        Cpu {
            a: 0x01,
            b: 0x00,
            c: 0x13,
            d: 0x00,
            e: 0xD8,
            h: 0x01,
            l: 0x4D,
            f: Flags::from_bits_truncate(0xB0),
            sp: 0xFFFE,
            pc: 0x0100,
            ime: false,
            ime_scheduled: false,
            halted: false,
        }
    }
    
    /// Check and handle interrupts. Returns true if an interrupt was handled.
    pub fn check_interrupts(&mut self, memory: &mut super::Memory) -> bool {
        // Check if IME is enabled
        if !self.ime {
            // Even if IME is disabled, check if any interrupt is pending to un-halt
            let if_reg = memory.read(0xFF0F);
            let ie_reg = memory.read(0xFFFF);
            if (if_reg & ie_reg & 0x1F) != 0 && self.halted {
                self.halted = false;
            }
            return false;
        }
        
        // Read interrupt flags (IF) and interrupt enable (IE)
        let if_reg = memory.read(0xFF0F);
        let ie_reg = memory.read(0xFFFF);
        
        // Check which interrupts are both requested and enabled
        let triggered = if_reg & ie_reg & 0x1F;
        
        if triggered == 0 {
            return false;
        }
        
        // Unhalt if halted
        if self.halted {
            self.halted = false;
        }
        
        // Handle interrupts in priority order: VBlank > LCD > Timer > Serial > Joypad
        let (interrupt_bit, handler_addr) = if triggered & 0x01 != 0 {
            (0x01, 0x0040) // VBlank
        } else if triggered & 0x02 != 0 {
            (0x02, 0x0048) // LCD STAT
        } else if triggered & 0x04 != 0 {
            (0x04, 0x0050) // Timer
        } else if triggered & 0x08 != 0 {
            (0x08, 0x0058) // Serial
        } else {
            (0x10, 0x0060) // Joypad
        };
        
        // Clear the interrupt flag
        memory.write(0xFF0F, if_reg & !interrupt_bit);
        
        // Disable interrupts
        self.ime = false;
        
        // Push PC onto stack
        self.sp = self.sp.wrapping_sub(1);
        memory.write(self.sp, (self.pc >> 8) as u8);
        self.sp = self.sp.wrapping_sub(1);
        memory.write(self.sp, (self.pc & 0xFF) as u8);
        
        // Jump to interrupt handler
        self.pc = handler_addr;
        true
    }
    
    pub fn step(&mut self, memory: &mut super::Memory) -> u32 {
        // Handle scheduled IME enable (EI has 1-instruction delay)
        if self.ime_scheduled {
            self.ime = true;
            self.ime_scheduled = false;
        }
        
        if self.halted {
            return 4;
        }
        
        let opcode = memory.read(self.pc);
        
        // Targeted logging - only around problem areas or when SUBTRACT flag changes
        let _has_subtract = self.f.contains(Flags::SUBTRACT);
        let _old_f = self.f.bits();
        let _log_pc = self.pc;
        
        self.pc = self.pc.wrapping_add(1);
        
        let cycles = self.execute(opcode, memory);
        
        // Trace key ROM addresses related to FF98 writes and title state machine
        // if log_pc == 0x2323 || log_pc == 0x23D8 || log_pc == 0x2496 {
        //     println!("[PC_TRACE] PC:{:04X} Op:{:02X} A:{:02X} F:{:02X} -> FF98 write site!", log_pc, opcode, self.a, old_f);
        // }
        // if log_pc == 0x64D3 {
        //     let df7f = memory.read(0xDF7F);
        //     let df7e = memory.read(0xDF7E);
        //     let ffe4 = memory.read(0xFFE4);
        //     let ffa6 = memory.read(0xFFA6);
        //     println!("[PC_TRACE] PC:{:04X} (title update enter) DF7F={:02X} DF7E={:02X} FFE4={:02X} FFA6={:02X} A:{:02X} F:{:02X}", log_pc, df7f, df7e, ffe4, ffa6, self.a, old_f);
        // }
        // if log_pc == 0x0BD0 {
        //     println!("[PC_TRACE] PC:{:04X} -> LD A,$01; LD ($DF7F),A site! A:{:02X} F:{:02X}", log_pc, self.a, old_f);
        // }
        // Trace RST $28 dispatch (first 5 times only)
        static RST28_COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        // if log_pc == 0x0028 {
        //     let n = RST28_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        //     if n < 5 {
        //         println!("[RST28] dispatch #{}: A={:02X} H={:02X} L={:02X} SP={:04X}", n, self.a, self.h, self.l, self.sp);
        //     }
        // }
        // Trace execution from WRAM/Echo RAM (dynamically copied code)
        static E0FA_COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        // if log_pc == 0xE0FA || log_pc == 0xC0FA {
        //     let n = E0FA_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        //     if n < 3 {
        //         let b0 = memory.read(0xC0FA);
        //         let b1 = memory.read(0xC0FB);
        //         let b2 = memory.read(0xC0FC);
        //         let b3 = memory.read(0xC0FD);
        //         println!("[WRAM_EXEC] #{}: PC={:04X} bytes@C0FA={:02X} {:02X} {:02X} {:02X}", n, log_pc, b0, b1, b2, b3);
        //     }
        // }
        
        // Log if SUBTRACT flag changed or F becomes C0 or we're in problem area or at specific PCs of interest
        /*
        let new_has_subtract = self.f.contains(Flags::SUBTRACT);
        if (has_subtract != new_has_subtract) || 
           (self.f.bits() == 0xC0) ||
           (log_pc >= 0x2A8F && log_pc <= 0x2AA0) || 
           (log_pc >= 0x64D0 && log_pc <= 0x64D4) ||
           (log_pc >= 0x6520 && log_pc <= 0x6525) {
            println!("[EXEC] PC:{:04X} Op:{:02X} A:{:02X} F:{:02X}->{:02X} (Z:{} N:{} H:{} C:{})", 
                log_pc, opcode, self.a, old_f, self.f.bits(),
                if self.f.contains(Flags::ZERO) { 1 } else { 0 },
                if self.f.contains(Flags::SUBTRACT) { 1 } else { 0 },
                if self.f.contains(Flags::HALF_CARRY) { 1 } else { 0 },
                if self.f.contains(Flags::CARRY) { 1 } else { 0 });
        }
        */
        
        cycles
    }
    
    fn execute(&mut self, opcode: u8, memory: &mut super::Memory) -> u32 {
        match opcode {
            // NOP
            0x00 => 4,
            
            // LD BC, nn
            0x01 => {
                self.c = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.b = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                12
            }
            
            // LD (BC), A
            0x02 => {
                let addr = ((self.b as u16) << 8) | (self.c as u16);
                memory.write(addr, self.a);
                8
            }
            
            // INC BC
            0x03 => {
                let bc = (((self.b as u16) << 8) | (self.c as u16)).wrapping_add(1);
                self.b = (bc >> 8) as u8;
                self.c = bc as u8;
                8
            }
            
            // INC B
            0x04 => {
                self.b = self.inc8(self.b);
                4
            }
            
            // DEC BC
            0x0B => {
                let bc = (((self.b as u16) << 8) | (self.c as u16)).wrapping_sub(1);
                self.b = (bc >> 8) as u8;
                self.c = bc as u8;
                8
            }
            
            // DEC B
            0x05 => {
                self.b = self.dec8(self.b);
                4
            }
            
            // LD B, n
            0x06 => {
                self.b = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                8
            }
            
            // RLCA (Rotate A left circular)
            0x07 => {
                let carry = (self.a >> 7) & 1;
                self.a = (self.a << 1) | carry;
                self.f = Flags::empty();
                self.f.set(Flags::CARRY, carry != 0);
                4
            }
            
            // LD (nn), SP
            0x08 => {
                let low = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                let high = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                let addr = ((high as u16) << 8) | (low as u16);
                memory.write(addr, self.sp as u8);
                memory.write(addr.wrapping_add(1), (self.sp >> 8) as u8);
                20
            }
            
            // ADD HL, BC
            0x09 => {
                let hl = ((self.h as u16) << 8) | (self.l as u16);
                let bc = ((self.b as u16) << 8) | (self.c as u16);
                let result = hl.wrapping_add(bc);
                self.f.remove(Flags::SUBTRACT);
                self.f.set(Flags::HALF_CARRY, (hl & 0x0FFF) + (bc & 0x0FFF) > 0x0FFF);
                self.f.set(Flags::CARRY, result < hl);
                self.h = (result >> 8) as u8;
                self.l = result as u8;
                8
            }
            
            // LD A, (BC)
            0x0A => {
                let addr = ((self.b as u16) << 8) | (self.c as u16);
                self.a = memory.read(addr);
                8
            }
            
            // INC C
            0x0C => {
                self.c = self.inc8(self.c);
                4
            }
            
            // DEC C
            0x0D => {
                self.c = self.dec8(self.c);
                4
            }
            
            // LD C, n
            0x0E => {
                self.c = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                8
            }
            
            // RRCA (Rotate A right circular)
            0x0F => {
                let carry = self.a & 0x01;
                self.a = (self.a >> 1) | (carry << 7);
                self.f = Flags::empty();
                self.f.set(Flags::CARRY, carry != 0);
                4
            }
            
            // STOP
            0x10 => {
                // On DMG/non-CGB, STOP just halts. Skip the next byte.
                self.pc = self.pc.wrapping_add(1);
                4
            }
            
            // LD DE, nn
            0x11 => {
                self.e = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.d = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                12
            }
            
            // LD (DE), A
            0x12 => {
                let addr = ((self.d as u16) << 8) | (self.e as u16);
                memory.write(addr, self.a);
                8
            }
            
            // INC DE
            0x13 => {
                let de = (((self.d as u16) << 8) | (self.e as u16)).wrapping_add(1);
                self.d = (de >> 8) as u8;
                self.e = de as u8;
                8
            }
            
            // INC D
            0x14 => {
                self.d = self.inc8(self.d);
                4
            }
            
            // DEC D
            0x15 => {
                self.d = self.dec8(self.d);
                4
            }
            
            // LD D, n
            0x16 => {
                self.d = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                8
            }
            
            // RLA (Rotate A left through carry)
            0x17 => {
                let old_carry = if self.f.contains(Flags::CARRY) { 1 } else { 0 };
                let new_carry = (self.a >> 7) & 1;
                self.a = (self.a << 1) | old_carry;
                self.f = Flags::empty();
                self.f.set(Flags::CARRY, new_carry != 0);
                4
            }
            
            // DEC DE
            0x1B => {
                let de = (((self.d as u16) << 8) | (self.e as u16)).wrapping_sub(1);
                self.d = (de >> 8) as u8;
                self.e = de as u8;
                8
            }
            
            // DEC E
            0x1D => {
                self.e = self.dec8(self.e);
                4
            }
            
            // LD E, n
            0x1E => {
                self.e = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                8
            }
            
            // JR n (relative jump)
            0x18 => {
                let offset = memory.read(self.pc) as i8;
                self.pc = self.pc.wrapping_add(1);
                self.pc = (self.pc as i32 + offset as i32) as u16;
                12
            }
            
            // JR NZ, n
            0x20 => {
                let offset = memory.read(self.pc) as i8;
                self.pc = self.pc.wrapping_add(1);
                if !self.f.contains(Flags::ZERO) {
                    self.pc = (self.pc as i32 + offset as i32) as u16;
                    12
                } else {
                    8
                }
            }
            
            // LD HL, nn
            0x21 => {
                self.l = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.h = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                12
            }
            
            // LD (HL+), A
            0x22 => {
                let addr = ((self.h as u16) << 8) | (self.l as u16);
                memory.write(addr, self.a);
                let hl = addr.wrapping_add(1);
                self.h = (hl >> 8) as u8;
                self.l = hl as u8;
                8
            }
            
            // LD A, (HL+)
            0x2A => {
                let addr = ((self.h as u16) << 8) | (self.l as u16);
                self.a = memory.read(addr);
                let hl = addr.wrapping_add(1);
                self.h = (hl >> 8) as u8;
                self.l = hl as u8;
                8
            }
            
            // DEC HL
            0x2B => {
                let hl = (((self.h as u16) << 8) | (self.l as u16)).wrapping_sub(1);
                self.h = (hl >> 8) as u8;
                self.l = hl as u8;
                8
            }
            
            // INC HL
            0x23 => {
                let hl = (((self.h as u16) << 8) | (self.l as u16)).wrapping_add(1);
                self.h = (hl >> 8) as u8;
                self.l = hl as u8;
                8
            }
            
            // DAA (Decimal Adjust Accumulator)
            0x27 => {
                let mut a = self.a as u16;
                if !self.f.contains(Flags::SUBTRACT) {
                    if self.f.contains(Flags::CARRY) || a > 0x99 {
                        a = a.wrapping_add(0x60);
                        self.f.insert(Flags::CARRY);
                    }
                    if self.f.contains(Flags::HALF_CARRY) || (a & 0x0F) > 0x09 {
                        a = a.wrapping_add(0x06);
                    }
                } else {
                    if self.f.contains(Flags::CARRY) {
                        a = a.wrapping_sub(0x60);
                    }
                    if self.f.contains(Flags::HALF_CARRY) {
                        a = a.wrapping_sub(0x06);
                    }
                }
                self.a = a as u8;
                self.f.set(Flags::ZERO, self.a == 0);
                self.f.remove(Flags::HALF_CARRY);
                4
            }
            
            // ADD HL, HL
            0x29 => {
                let hl = ((self.h as u16) << 8) | (self.l as u16);
                let result = hl.wrapping_add(hl);
                self.f.remove(Flags::SUBTRACT);
                self.f.set(Flags::HALF_CARRY, (hl & 0x0FFF) + (hl & 0x0FFF) > 0x0FFF);
                self.f.set(Flags::CARRY, hl > 0x7FFF); // Check if result would overflow
                self.h = (result >> 8) as u8;
                self.l = result as u8;
                8
            }
            
            // INC L
            0x2C => {
                self.l = self.inc8(self.l);
                4
            }
            
            // DEC L
            0x2D => {
                self.l = self.dec8(self.l);
                4
            }
            
            // LD L, n
            0x2E => {
                self.l = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                8
            }
            
            // CPL (Complement A - flip all bits)
            0x2F => {
                self.a = !self.a;
                self.f.insert(Flags::SUBTRACT);
                self.f.insert(Flags::HALF_CARRY);
                4
            }
            
            // JR NC, n (jump relative if no carry)
            0x30 => {
                let offset = memory.read(self.pc) as i8;
                self.pc = self.pc.wrapping_add(1);
                if !self.f.contains(Flags::CARRY) {
                    self.pc = (self.pc as i32 + offset as i32) as u16;
                    12
                } else {
                    8
                }
            }
            
            // INC SP
            0x33 => {
                self.sp = self.sp.wrapping_add(1);
                8
            }
            
            // INC (HL)
            0x34 => {
                let addr = ((self.h as u16) << 8) | (self.l as u16);
                let value = memory.read(addr);
                let result = self.inc8(value);
                memory.write(addr, result);
                12
            }
            
            // DEC (HL)
            0x35 => {
                let addr = ((self.h as u16) << 8) | (self.l as u16);
                let value = memory.read(addr);
                let result = self.dec8(value);
                memory.write(addr, result);
                12
            }
            
            // LD (HL), n
            0x36 => {
                let value = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                let addr = ((self.h as u16) << 8) | (self.l as u16);
                memory.write(addr, value);
                12
            }
            
            // DEC H
            0x25 => {
                self.h = self.dec8(self.h);
                4
            }
            
            // LD H, n
            0x26 => {
                self.h = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                8
            }
            
            // JR Z, n
            0x28 => {
                let offset = memory.read(self.pc) as i8;
                self.pc = self.pc.wrapping_add(1);
                if self.f.contains(Flags::ZERO) {
                    self.pc = (self.pc as i32 + offset as i32) as u16;
                    12
                } else {
                    8
                }
            }
            
            // LD SP, nn
            0x31 => {
                let low = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                let high = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.sp = ((high as u16) << 8) | (low as u16);
                12
            }
            
            // ADC A, r (add with carry)
            0x88..=0x8F => {
                let src = opcode & 0x07;
                let value = self.get_r8(src, memory);
                let carry = if self.f.contains(Flags::CARRY) { 1 } else { 0 };
                let result = (self.a as u16) + (value as u16) + carry;
                self.f.set(Flags::ZERO, (result & 0xFF) == 0);
                self.f.remove(Flags::SUBTRACT);
                self.f.set(Flags::HALF_CARRY, (self.a & 0x0F) + (value & 0x0F) + carry as u8 > 0x0F);
                self.f.set(Flags::CARRY, result > 0xFF);
                self.a = result as u8;
                if src == 6 { 8 } else { 4 }
            }
            
            // SBC A, r (subtract with carry)
            0x98..=0x9F => {
                let src = opcode & 0x07;
                let value = self.get_r8(src, memory);
                let carry = if self.f.contains(Flags::CARRY) { 1 } else { 0 };
                let result = (self.a as i16) - (value as i16) - (carry as i16);
                self.f.set(Flags::ZERO, (result & 0xFF) == 0);
                self.f.insert(Flags::SUBTRACT);
                self.f.set(Flags::HALF_CARRY, (self.a & 0x0F) < (value & 0x0F) + carry as u8);
                self.f.set(Flags::CARRY, result < 0);
                self.a = result as u8;
                if src == 6 { 8 } else { 4 }
            }
            
            // LD (HL-), A
            0x32 => {
                let addr = ((self.h as u16) << 8) | (self.l as u16);
                memory.write(addr, self.a);
                let hl = addr.wrapping_sub(1);
                self.h = (hl >> 8) as u8;
                self.l = hl as u8;
                8
            }
            
            // LD A, (HL-) - Load A from (HL) then decrement HL
            0x3A => {
                let addr = ((self.h as u16) << 8) | (self.l as u16);
                self.a = memory.read(addr);
                let hl = addr.wrapping_sub(1);
                self.h = (hl >> 8) as u8;
                self.l = hl as u8;
                8
            }
            
            // INC A
            0x3C => {
                self.a = self.inc8(self.a);
                4
            }
            
            // DEC A
            0x3D => {
                self.a = self.dec8(self.a);
                4
            }
            
            // LD A, n (load immediate byte into A)
            0x3E => {
                self.a = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                8
            }
            
            // RET NZ
            0xC0 => {
                if !self.f.contains(Flags::ZERO) {
                    let low = memory.read(self.sp);
                    self.sp = self.sp.wrapping_add(1);
                    let high = memory.read(self.sp);
                    self.sp = self.sp.wrapping_add(1);
                    self.pc = ((high as u16) << 8) | (low as u16);
                    20
                } else {
                    8
                }
            }
            
            // POP BC
            0xC1 => {
                self.c = memory.read(self.sp);
                self.sp = self.sp.wrapping_add(1);
                self.b = memory.read(self.sp);
                self.sp = self.sp.wrapping_add(1);
                12
            }
            
            // JP NZ, nn
            0xC2 => {
                let low = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                let high = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                if !self.f.contains(Flags::ZERO) {
                    self.pc = ((high as u16) << 8) | (low as u16);
                    16
                } else {
                    12
                }
            }
            
            // JP nn (absolute jump)
            0xC3 => {
                let low = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                let high = memory.read(self.pc);
                self.pc = ((high as u16) << 8) | (low as u16);
                16
            }

            // CALL NZ, nn
            0xC4 => {
                let low = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                let high = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                if !self.f.contains(Flags::ZERO) {
                    let addr = ((high as u16) << 8) | (low as u16);
                    self.sp = self.sp.wrapping_sub(1);
                    memory.write(self.sp, (self.pc >> 8) as u8);
                    self.sp = self.sp.wrapping_sub(1);
                    memory.write(self.sp, self.pc as u8);
                    self.pc = addr;
                    24
                } else {
                    12
                }
            }

            // JP Z, nn
            0xCA => {
                let low = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                let high = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                if self.f.contains(Flags::ZERO) {
                    self.pc = ((high as u16) << 8) | (low as u16);
                    16
                } else {
                    12
                }
            }
            
            // CALL Z, nn
            0xCC => {
                let low = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                let high = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                if self.f.contains(Flags::ZERO) {
                    let addr = ((high as u16) << 8) | (low as u16);
                    self.sp = self.sp.wrapping_sub(1);
                    memory.write(self.sp, (self.pc >> 8) as u8);
                    self.sp = self.sp.wrapping_sub(1);
                    memory.write(self.sp, self.pc as u8);
                    self.pc = addr;
                    24
                } else {
                    12
                }
            }
            
            // RET (unconditional)
            0xC9 => {
                let low = memory.read(self.sp);
                self.sp = self.sp.wrapping_add(1);
                let high = memory.read(self.sp);
                self.sp = self.sp.wrapping_add(1);
                self.pc = ((high as u16) << 8) | (low as u16);
                16
            }
            
            // AND n
            0xE6 => {
                let value = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.a &= value;
                self.f = Flags::HALF_CARRY;
                if self.a == 0 {
                    self.f.insert(Flags::ZERO);
                }
                8
            }
            
            // POP HL
            0xE1 => {
                let low = memory.read(self.sp);
                self.sp = self.sp.wrapping_add(1);
                let high = memory.read(self.sp);
                self.sp = self.sp.wrapping_add(1);
                self.h = high;
                self.l = low;
                12
            }
            
            // ADD SP, n
            0xE8 => {
                let offset = memory.read(self.pc) as i8 as i16;
                self.pc = self.pc.wrapping_add(1);
                let sp = self.sp as i16;
                let result = sp.wrapping_add(offset);
                self.f = Flags::empty();
                self.f.set(Flags::HALF_CARRY, ((sp & 0x0F) + (offset & 0x0F)) > 0x0F);
                self.f.set(Flags::CARRY, ((sp & 0xFF) + (offset & 0xFF)) > 0xFF);
                self.sp = result as u16;
                16
            }
            
            // JP (HL)
            0xE9 => {
                self.pc = ((self.h as u16) << 8) | (self.l as u16);
                4
            }
            
            // XOR n
            0xEE => {
                let value = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.a ^= value;
                self.f = Flags::empty();
                if self.a == 0 {
                    self.f.insert(Flags::ZERO);
                }
                8
            }
            
            // LD HL, SP+n
            0xF8 => {
                let offset = memory.read(self.pc) as i8;
                self.pc = self.pc.wrapping_add(1);
                let result = self.sp.wrapping_add(offset as i16 as u16);
                self.f = Flags::empty();
                let offset_u = offset as u8;
                self.f.set(Flags::HALF_CARRY, (self.sp & 0x0F) + ((offset_u as u16) & 0x0F) > 0x0F);
                self.f.set(Flags::CARRY, (self.sp & 0xFF) + ((offset_u as u16) & 0xFF) > 0xFF);
                self.h = (result >> 8) as u8;
                self.l = result as u8;
                12
            }
            
            // RET NC
            0xD0 => {
                if !self.f.contains(Flags::CARRY) {
                    let low = memory.read(self.sp);
                    self.sp = self.sp.wrapping_add(1);
                    let high = memory.read(self.sp);
                    self.sp = self.sp.wrapping_add(1);
                    self.pc = ((high as u16) << 8) | (low as u16);
                    20
                } else {
                    8
                }
            }
            
            // SUB n
            0xD6 => {
                let value = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.a = self.sub8(self.a, value);
                8
            }
            
            // RET C
            0xD8 => {
                if self.f.contains(Flags::CARRY) {
                    let low = memory.read(self.sp);
                    self.sp = self.sp.wrapping_add(1);
                    let high = memory.read(self.sp);
                    self.sp = self.sp.wrapping_add(1);
                    self.pc = ((high as u16) << 8) | (low as u16);
                    20
                } else {
                    8
                }
            }
            
            // CALL C, nn
            0xDC => {
                let low = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                let high = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                if self.f.contains(Flags::CARRY) {
                    let addr = ((high as u16) << 8) | (low as u16);
                    self.sp = self.sp.wrapping_sub(1);
                    memory.write(self.sp, (self.pc >> 8) as u8);
                    self.sp = self.sp.wrapping_sub(1);
                    memory.write(self.sp, self.pc as u8);
                    self.pc = addr;
                    24
                } else {
                    12
                }
            }
            
            // SBC A, n
            0xDE => {
                let value = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                let carry = if self.f.contains(Flags::CARRY) { 1 } else { 0 };
                let result = (self.a as i16) - (value as i16) - (carry as i16);
                self.f.set(Flags::ZERO, (result & 0xFF) == 0);
                self.f.insert(Flags::SUBTRACT);
                self.f.set(Flags::HALF_CARRY, (self.a & 0x0F) < (value & 0x0F) + carry as u8);
                self.f.set(Flags::CARRY, result < 0);
                self.a = result as u8;
                8
            }
            
            // LD B, B through LD A, A (0x40-0x75 and 0x77-0x7F, excluding 0x76 which is HALT)
            0x40..=0x75 | 0x77..=0x7F => {
                let dest = (opcode >> 3) & 0x07;
                let src = opcode & 0x07;
                let value = self.get_r8(src, memory);
                self.set_r8(dest, value, memory);
                if src == 6 || dest == 6 { 8 } else { 4 }
            }
            
            // HALT (0x76)
            0x76 => {
                self.halted = true;
                4
            }
            
            // ADD A, r
            0x80..=0x87 => {
                let src = opcode & 0x07;
                let value = self.get_r8(src, memory);
                self.a = self.add8(self.a, value);
                if src == 6 { 8 } else { 4 }
            }
            
            // SUB r
            0x90..=0x97 => {
                let src = opcode & 0x07;
                let value = self.get_r8(src, memory);
                self.a = self.sub8(self.a, value);
                if src == 6 { 8 } else { 4 }
            }
            
            // AND r
            0xA0..=0xA7 => {
                let src = opcode & 0x07;
                let value = self.get_r8(src, memory);
                self.a &= value;
                self.f = Flags::HALF_CARRY;
                if self.a == 0 {
                    self.f.insert(Flags::ZERO);
                }
                if src == 6 { 8 } else { 4 }
            }
            
            // XOR r
            0xA8..=0xAF => {
                let src = opcode & 0x07;
                let value = self.get_r8(src, memory);
                self.a ^= value;
                self.f = Flags::empty();
                if self.a == 0 {
                    self.f.insert(Flags::ZERO);
                }
                if src == 6 { 8 } else { 4 }
            }
            
            // OR r
            0xB0..=0xB7 => {
                let src = opcode & 0x07;
                let value = self.get_r8(src, memory);
                self.a |= value;
                self.f = Flags::empty();
                if self.a == 0 {
                    self.f.insert(Flags::ZERO);
                }
                if src == 6 { 8 } else { 4 }
            }
            
            // CP r
            0xB8..=0xBF => {
                let src = opcode & 0x07;
                let value = self.get_r8(src, memory);
                self.sub8(self.a, value); // Sets flags but discards result
                if src == 6 { 8 } else { 4 }
            }
            
            // CB PREFIX - Extended instruction set
            0xCB => {
                let cb_opcode = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.execute_cb(cb_opcode, memory)
            }
            
            // CALL nn
            0xCD => {
                let low = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                let high = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                let addr = ((high as u16) << 8) | (low as u16);
                
                // Push return address
                self.sp = self.sp.wrapping_sub(1);
                memory.write(self.sp, (self.pc >> 8) as u8);
                self.sp = self.sp.wrapping_sub(1);
                memory.write(self.sp, self.pc as u8);
                
                self.pc = addr;
                24
            }
            
            // RST vectors
            0xC7 | 0xCF | 0xD7 | 0xDF | 0xE7 | 0xEF | 0xF7 | 0xFF => {
                let addr = (opcode & 0x38) as u16;
                self.sp = self.sp.wrapping_sub(1);
                memory.write(self.sp, (self.pc >> 8) as u8);
                self.sp = self.sp.wrapping_sub(1);
                memory.write(self.sp, self.pc as u8);
                self.pc = addr;
                16
            }
            
            // LD (0xFF00+n), A
            0xE0 => {
                let offset = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                let addr = 0xFF00 + offset as u16;
                // Debug logging disabled
                // if self.pc - 2 == 0x2A95 {
                //     println!("[LDH (n),A] PC:{:04X} Writing A:{:02X} to address {:04X} (FF00+{:02X})", 
                //         self.pc - 2, self.a, addr, offset);
                // }
               memory.write(addr, self.a);
                12
            }
            
            // LD (0xFF00+C), A
            0xE2 => {
                memory.write(0xFF00 + self.c as u16, self.a);
                8
            }
            
            // LD A, (0xFF00+n)
            0xF0 => {
                let offset = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.a = memory.read(0xFF00 + offset as u16);
                12
            }
            
            // LD A, (0xFF00+C)
            0xF2 => {
                self.a = memory.read(0xFF00 + self.c as u16);
                8
            }
            
            // OR n
            0xF6 => {
                let value = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.a |= value;
                self.f = Flags::empty();
                if self.a == 0 {
                    self.f.insert(Flags::ZERO);
                }
                8
            }
            
            // 0xFA: LD A, (nn)
            0xFA => {
                let low = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                let high = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                let addr = ((high as u16) << 8) | (low as u16);
                self.a = memory.read(addr);
                16
            }
            
            // DI (disable interrupts)
            0xF3 => {
                self.ime = false;
                4
            }

            // EI (enable interrupts)
            0xFB => {
                self.ime_scheduled = true;
                4
            }
            
            // POP AF
            0xF1 => {
                let f_value = memory.read(self.sp);
                self.sp = self.sp.wrapping_add(1);
                let a_value = memory.read(self.sp);
                self.sp = self.sp.wrapping_add(1);
                self.f = Flags::from_bits_truncate(f_value & 0xF0);
                self.a = a_value;
                // Debug logging disabled
                // println!("[POP AF] PC:{:04X} Stack[{:04X}]=F:{:02X} Stack[{:04X}]=A:{:02X} | OldA:{:02X} OldF:{:02X} -> NewA:{:02X} NewF:{:02X} (Z:{} N:{} H:{} C:{}) SP:{:04X}", 
                //     self.pc - 1, sp_f, f_value, sp_a, a_value, old_a, old_f, self.a, self.f.bits(),
                //     if self.f.contains(Flags::ZERO) { 1 } else { 0 },
                //     if self.f.contains(Flags::SUBTRACT) { 1 } else { 0 },
                //     if self.f.contains(Flags::HALF_CARRY) { 1 } else { 0 },
                //     if self.f.contains(Flags::CARRY) { 1 } else { 0 },
                //     self.sp);
                12
            }
            
            // FE: CP n
            0xFE => {
                let value = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                let _old_f = self.f.bits();
                self.sub8(self.a, value);
                /*
                if self.pc - 2 == 0x2A8D {
                    println!("[CP n] PC:{:04X} Comparing A:{:02X} with n:{:02X} | F:{:02X}->{:02X} (Z:{} means {})", 
                        self.pc - 2, self.a, value, old_f, self.f.bits(),
                        if self.f.contains(Flags::ZERO) { 1 } else { 0 },
                        if self.f.contains(Flags::ZERO) { "EQUAL" } else { "NOT EQUAL" });
                }
                */
                8
            }
            
            // 0x19: ADD HL, DE
            0x19 => {
                let hl = ((self.h as u16) << 8) | (self.l as u16);
                let de = ((self.d as u16) << 8) | (self.e as u16);
                let result = hl.wrapping_add(de);
                self.h = (result >> 8) as u8;
                self.l = result as u8;
                self.f.remove(Flags::SUBTRACT);
                self.f.set(Flags::CARRY, result < hl);
                self.f.set(Flags::HALF_CARRY, (hl & 0x0FFF) + (de & 0x0FFF) > 0x0FFF);
                8
            }
            
            // 0x1A: LD A, (DE)
            0x1A => {
                let addr = ((self.d as u16) << 8) | (self.e as u16);
                self.a = memory.read(addr);
                8
            }
            
            // 0x1C: INC E
            0x1C => {
                self.e = self.inc8(self.e);
                4
            }
            
            // 0x38: JR C, r8
            0x38 => {
                let offset = memory.read(self.pc) as i8;
                self.pc = self.pc.wrapping_add(1);
                if self.f.contains(Flags::CARRY) {
                    self.pc = ((self.pc as i32) + (offset as i32)) as u16;
                    12
                } else {
                    8
                }
            }
            
            // 0xC5: PUSH BC
            0xC5 => {
                self.sp = self.sp.wrapping_sub(1);
                memory.write(self.sp, self.b);
                self.sp = self.sp.wrapping_sub(1);
                memory.write(self.sp, self.c);
                16
            }
            
            // 0xC6: ADD A, n
            0xC6 => {
                let value = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.a = self.add8(self.a, value);
                8
            }
            
            // 0xC8: RET Z
            0xC8 => {
                if self.f.contains(Flags::ZERO) {
                    let low = memory.read(self.sp);
                    self.sp = self.sp.wrapping_add(1);
                    let high = memory.read(self.sp);
                    self.sp = self.sp.wrapping_add(1);
                    self.pc = ((high as u16) << 8) | (low as u16);
                    20
                } else {
                    8
                }
            }
            
            // 0xD1: POP DE
            0xD1 => {
                self.e = memory.read(self.sp);
                self.sp = self.sp.wrapping_add(1);
                self.d = memory.read(self.sp);
                self.sp = self.sp.wrapping_add(1);
                12
            }
            
            // 0xD5: PUSH DE
            0xD5 => {
                self.sp = self.sp.wrapping_sub(1);
                memory.write(self.sp, self.d);
                self.sp = self.sp.wrapping_sub(1);
                memory.write(self.sp, self.e);
                16
            }
            
            // 0xD9: RETI (Return from interrupt)
            0xD9 => {
                let low = memory.read(self.sp);
                self.sp = self.sp.wrapping_add(1);
                let high = memory.read(self.sp);
                self.sp = self.sp.wrapping_add(1);
                self.pc = ((high as u16) << 8) | (low as u16);
                self.ime = true;
                16
            }
            
            // 0xE5: PUSH HL
            0xE5 => {
                self.sp = self.sp.wrapping_sub(1);
                memory.write(self.sp, self.h);
                self.sp = self.sp.wrapping_sub(1);
                memory.write(self.sp, self.l);
                16
            }
            
            // 0xF5: PUSH AF
            0xF5 => {
                self.sp = self.sp.wrapping_sub(1);
                memory.write(self.sp, self.a);
                self.sp = self.sp.wrapping_sub(1);
                memory.write(self.sp, self.f.bits());
                // Debug logging disabled
                // println!("[PUSH AF] PC:{:04X} A:{:02X} F:{:02X} (Z:{} N:{} H:{} C:{}) -> Stack[{:04X}]=F Stack[{:04X}]=A SP:{:04X}", 
                //     self.pc - 1, self.a, self.f.bits(),
                //     if self.f.contains(Flags::ZERO) { 1 } else { 0 },
                //     if self.f.contains(Flags::SUBTRACT) { 1 } else { 0 },
                //     if self.f.contains(Flags::HALF_CARRY) { 1 } else { 0 },
                //     if self.f.contains(Flags::CARRY) { 1 } else { 0 },
                //     sp_f, sp_a, self.sp);
                16
            }
            
            // 0xEA: LD (nn), A
            0xEA => {
                let low = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                let high = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                let addr = ((high as u16) << 8) | (low as u16);
                memory.write(addr, self.a);
                16
            }
            
            // 0x1F: RRA - Rotate A right through carry
            0x1F => {
                let old_carry = if self.f.contains(Flags::CARRY) { 1u8 } else { 0 };
                let new_carry = self.a & 0x01;
                self.a = (self.a >> 1) | (old_carry << 7);
                self.f = Flags::empty();
                self.f.set(Flags::CARRY, new_carry != 0);
                4
            }

            // 0x24: INC H
            0x24 => {
                self.h = self.inc8(self.h);
                4
            }

            // 0x39: ADD HL, SP
            0x39 => {
                let hl = ((self.h as u16) << 8) | (self.l as u16);
                let result = hl.wrapping_add(self.sp);
                self.f.remove(Flags::SUBTRACT);
                self.f.set(Flags::HALF_CARRY, (hl & 0x0FFF) + (self.sp & 0x0FFF) > 0x0FFF);
                self.f.set(Flags::CARRY, result < hl);
                self.h = (result >> 8) as u8;
                self.l = result as u8;
                8
            }

            // 0x3B: DEC SP
            0x3B => {
                self.sp = self.sp.wrapping_sub(1);
                8
            }

            // 0x3F: CCF - Complement Carry Flag
            0x3F => {
                let c = self.f.contains(Flags::CARRY);
                self.f.remove(Flags::SUBTRACT);
                self.f.remove(Flags::HALF_CARRY);
                self.f.set(Flags::CARRY, !c);
                4
            }

            // 0xCE: ADC A, n8 - Add immediate with carry
            0xCE => {
                let value = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                let carry = if self.f.contains(Flags::CARRY) { 1u16 } else { 0 };
                let result = (self.a as u16) + (value as u16) + carry;
                self.f.set(Flags::ZERO, (result & 0xFF) == 0);
                self.f.remove(Flags::SUBTRACT);
                self.f.set(Flags::HALF_CARRY, (self.a & 0x0F) + (value & 0x0F) + carry as u8 > 0x0F);
                self.f.set(Flags::CARRY, result > 0xFF);
                self.a = result as u8;
                8
            }

            // 0xD2: JP NC, a16 - Jump if carry NOT set
            0xD2 => {
                let low = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                let high = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                if !self.f.contains(Flags::CARRY) {
                    self.pc = ((high as u16) << 8) | (low as u16);
                    16
                } else {
                    12
                }
            }

            // 0xD4: CALL NC, a16 - Call if carry NOT set
            0xD4 => {
                let low = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                let high = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                if !self.f.contains(Flags::CARRY) {
                    let addr = ((high as u16) << 8) | (low as u16);
                    self.sp = self.sp.wrapping_sub(1);
                    memory.write(self.sp, (self.pc >> 8) as u8);
                    self.sp = self.sp.wrapping_sub(1);
                    memory.write(self.sp, self.pc as u8);
                    self.pc = addr;
                    24
                } else {
                    12
                }
            }

            // 0xDA: JP C, a16 - Jump if carry set
            0xDA => {
                let low = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                let high = memory.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                if self.f.contains(Flags::CARRY) {
                    self.pc = ((high as u16) << 8) | (low as u16);
                    16
                } else {
                    12
                }
            }

            _ => {
                // Unknown opcode - just skip it
                log::warn!("Unknown opcode: 0x{:02X} at PC: 0x{:04X}", opcode, self.pc.wrapping_sub(1));
                4
            }
        }
    }
    
    fn get_r8(&self, reg: u8, memory: &super::Memory) -> u8 {
        match reg {
            0 => self.b,
            1 => self.c,
            2 => self.d,
            3 => self.e,
            4 => self.h,
            5 => self.l,
            6 => {
                let addr = ((self.h as u16) << 8) | (self.l as u16);
                memory.read(addr)
            }
            7 => self.a,
            _ => 0,
        }
    }
    
    fn set_r8(&mut self, reg: u8, value: u8, memory: &mut super::Memory) {
        match reg {
            0 => self.b = value,
            1 => self.c = value,
            2 => self.d = value,
            3 => self.e = value,
            4 => self.h = value,
            5 => self.l = value,
            6 => {
                let addr = ((self.h as u16) << 8) | (self.l as u16);
                memory.write(addr, value);
            }
            7 => self.a = value,
            _ => {}
        }
    }
    
    fn inc8(&mut self, value: u8) -> u8 {
        let result = value.wrapping_add(1);
        self.f.set(Flags::ZERO, result == 0);
        self.f.remove(Flags::SUBTRACT);
        self.f.set(Flags::HALF_CARRY, (value & 0x0F) == 0x0F);
        result
    }
    
    fn dec8(&mut self, value: u8) -> u8 {
        let result = value.wrapping_sub(1);
        self.f.set(Flags::ZERO, result == 0);
        self.f.insert(Flags::SUBTRACT);
        self.f.set(Flags::HALF_CARRY, (value & 0x0F) == 0);
        result
    }
    
    fn add8(&mut self, a: u8, b: u8) -> u8 {
        let result = a.wrapping_add(b);
        self.f.set(Flags::ZERO, result == 0);
        self.f.remove(Flags::SUBTRACT);
        self.f.set(Flags::HALF_CARRY, (a & 0x0F) + (b & 0x0F) > 0x0F);
        self.f.set(Flags::CARRY, (a as u16) + (b as u16) > 0xFF);
        result
    }
    
    fn sub8(&mut self, a: u8, b: u8) -> u8 {
        let result = a.wrapping_sub(b);
        self.f.set(Flags::ZERO, result == 0);
        self.f.insert(Flags::SUBTRACT);
        self.f.set(Flags::HALF_CARRY, (a & 0x0F) < (b & 0x0F));
        self.f.set(Flags::CARRY, a < b);
        result
    }
    
    /// Execute CB-prefixed instruction
    fn execute_cb(&mut self, opcode: u8, memory: &mut super::Memory) -> u32 {
        let reg = opcode & 0x07;
        
        match opcode {
            // BIT b, r - Test bit b in register r
            0x40..=0x7F => {
                let bit = (opcode >> 3) & 0x07;
                let value = self.get_r8(reg, memory);
                let result = value & (1 << bit);
                self.f.set(Flags::ZERO, result == 0);
                self.f.remove(Flags::SUBTRACT);
                self.f.insert(Flags::HALF_CARRY);
                if reg == 6 { 12 } else { 8 }
            }
            
            // RES b, r - Reset (clear) bit b in register r
            0x80..=0xBF => {
                let bit = (opcode >> 3) & 0x07;
                let value = self.get_r8(reg, memory);
                let result = value & !(1 << bit);
                self.set_r8(reg, result, memory);
                if reg == 6 { 16 } else { 8 }
            }
            
            // SET b, r - Set bit b in register r
            0xC0..=0xFF => {
                let bit = (opcode >> 3) & 0x07;
                let value = self.get_r8(reg, memory);
                let result = value | (1 << bit);
                self.set_r8(reg, result, memory);
                if reg == 6 { 16 } else { 8 }
            }
            
            // RLC r - Rotate left circular
            0x00..=0x07 => {
                let value = self.get_r8(reg, memory);
                let carry = (value >> 7) & 1;
                let result = (value << 1) | carry;
                self.f = Flags::empty();
                self.f.set(Flags::ZERO, result == 0);
                self.f.set(Flags::CARRY, carry != 0);
                self.set_r8(reg, result, memory);
                if reg == 6 { 16 } else { 8 }
            }
            
            // RRC r - Rotate right circular
            0x08..=0x0F => {
                let value = self.get_r8(reg, memory);
                let carry = value & 1;
                let result = (value >> 1) | (carry << 7);
                self.f = Flags::empty();
                self.f.set(Flags::ZERO, result == 0);
                self.f.set(Flags::CARRY, carry != 0);
                self.set_r8(reg, result, memory);
                if reg == 6 { 16 } else { 8 }
            }
            
            // RL r - Rotate left through carry
            0x10..=0x17 => {
                let value = self.get_r8(reg, memory);
                let old_carry = if self.f.contains(Flags::CARRY) { 1 } else { 0 };
                let new_carry = (value >> 7) & 1;
                let result = (value << 1) | old_carry;
                self.f = Flags::empty();
                self.f.set(Flags::ZERO, result == 0);
                self.f.set(Flags::CARRY, new_carry != 0);
                self.set_r8(reg, result, memory);
                if reg == 6 { 16 } else { 8 }
            }
            
            // RR r - Rotate right through carry
            0x18..=0x1F => {
                let value = self.get_r8(reg, memory);
                let old_carry = if self.f.contains(Flags::CARRY) { 1 } else { 0 };
                let new_carry = value & 1;
                let result = (value >> 1) | (old_carry << 7);
                self.f = Flags::empty();
                self.f.set(Flags::ZERO, result == 0);
                self.f.set(Flags::CARRY, new_carry != 0);
                self.set_r8(reg, result, memory);
                if reg == 6 { 16 } else { 8 }
            }
            
            // SLA r - Shift left arithmetic
            0x20..=0x27 => {
                let value = self.get_r8(reg, memory);
                let carry = (value >> 7) & 1;
                let result = value << 1;
                self.f = Flags::empty();
                self.f.set(Flags::ZERO, result == 0);
                self.f.set(Flags::CARRY, carry != 0);
                self.set_r8(reg, result, memory);
                if reg == 6 { 16 } else { 8 }
            }
            
            // SRA r - Shift right arithmetic (preserve sign bit)
            0x28..=0x2F => {
                let value = self.get_r8(reg, memory);
                let carry = value & 1;
                let result = (value >> 1) | (value & 0x80);
                self.f = Flags::empty();
                self.f.set(Flags::ZERO, result == 0);
                self.f.set(Flags::CARRY, carry != 0);
                self.set_r8(reg, result, memory);
                if reg == 6 { 16 } else { 8 }
            }
            
            // SWAP r - Swap nibbles
            0x30..=0x37 => {
                let value = self.get_r8(reg, memory);
                let result = ((value & 0x0F) << 4) | ((value & 0xF0) >> 4);
                self.f = Flags::empty();
                self.f.set(Flags::ZERO, result == 0);
                self.set_r8(reg, result, memory);
                if reg == 6 { 16 } else { 8 }
            }
            
            // SRL r - Shift right logical
            0x38..=0x3F => {
                let value = self.get_r8(reg, memory);
                let carry = value & 1;
                let result = value >> 1;
                self.f = Flags::empty();
                self.f.set(Flags::ZERO, result == 0);
                self.f.set(Flags::CARRY, carry != 0);
                self.set_r8(reg, result, memory);
                if reg == 6 { 16 } else { 8 }
            }
        }
    }
}
