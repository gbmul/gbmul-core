// Serial port — Link Cable hardware (SB / SC)
//
// Hardware model:
//   0xFF01  SB — Serial transfer data (shift register)
//   0xFF02  SC — Serial transfer control
//              Bit 7: Start/busy flag (1 = transfer in progress; written 1 to start)
//              Bit 0: Clock select (1 = internal/master, 0 = external/slave)
//
// Transfer timing:
//   Master mode (SC bit 0 = 1): 8 bits × 64 T-cycles per bit = 512 T-cycles total
//   Slave  mode (SC bit 0 = 0): transfer only completes when the master clocks us;
//                                if no byte arrives we stay pending (no spurious IRQ).
//
// Interrupt: When a transfer completes the Serial interrupt (IF bit 3) is raised.
// Serial::step() returns true as the signal to the caller.
//
// Link layer interface:
//   take_outgoing()  — called by the link layer after a transfer starts; returns
//                      the byte the local GB wants to send, consuming it.
//   receive_byte()   — called by the link layer to inject the byte arriving from
//                      the remote GB; the value lands in SB when the transfer completes.

use super::memory::Memory;

const TRANSFER_CYCLES: u32 = 512;

pub struct Serial {
    pub sb: u8,
    pub sc: u8,

    pub transferring: bool,
    transfer_cycles: u32,

    outgoing: Option<u8>,
    incoming: Option<u8>,

    /// Slave armed (SC bit 7 set, bit 0 clear) but no master byte arrived yet.
    /// The frame-start poll retries each frame until the master byte arrives.
    pub slave_pending: bool,

    /// Master sent its byte and is waiting for the slave response.
    /// The frame-start poll calls link.poll_incoming() until it arrives.
    pub master_waiting: bool,

    /// Number of times the master timer has been extended while waiting for
    /// the slave response. Capped at 600 (600 × 512 cycles ≈ 73 ms) before
    /// giving up with 0xFF. LocalLink bytes arrive in nanoseconds so the cap
    /// is never reached in local dual-emulator mode.
    pub master_extend_count: u32,
}

impl Serial {
    pub fn new() -> Self {
        Serial {
            sb: 0x00,
            sc: 0x00,
            transferring: false,
            transfer_cycles: 0,
            outgoing: None,
            incoming: None,
            slave_pending: false,
            master_waiting: false,
            master_extend_count: 0,
        }
    }

    /// Called when the game writes SC with bit 7 set (start transfer).
    pub fn start_transfer(&mut self, sb: u8, sc: u8) {
        self.sb = sb;
        self.sc = sc | 0x80;
        self.transferring = true;
        self.transfer_cycles = TRANSFER_CYCLES;
        self.outgoing = Some(sb);
        self.slave_pending = false;
        self.master_waiting = false;
        self.master_extend_count = 0;
    }

    /// Advance the shift register by `cycles` T-cycles.
    /// Returns `true` when a transfer just completed → caller must set IF bit 3.
    pub fn step(&mut self, cycles: u32) -> bool {
        if !self.transferring {
            return false;
        }

        if cycles >= self.transfer_cycles {
            // Master mode: if the slave response hasn't arrived yet, extend the
            // timer rather than completing with 0xFF. Cap at 600 extensions
            // (~73 ms) so we never stall forever if the partner never responds.
            if self.master_waiting && self.incoming.is_none() {
                if self.master_extend_count < 600 {
                    self.master_extend_count += 1;
                    self.transfer_cycles = TRANSFER_CYCLES;
                    return false;
                }
                log::warn!("[serial] master timed out after {} extensions — completing with 0xFF", self.master_extend_count);
                self.master_waiting = false;
                self.master_extend_count = 0;
            }

            self.transfer_cycles = 0;
            self.transferring = false;
            self.master_waiting = false;
            self.master_extend_count = 0;

            // Place the received byte (or 0xFF if no cable) into SB.
            let received = self.incoming.take().unwrap_or(0xFF);
            self.sb = received;
            self.sc &= !0x80; // hardware clears bit 7 on completion

            true
        } else {
            self.transfer_cycles -= cycles;
            false
        }
    }

    /// Returns the byte the local GB is trying to send, consuming it.
    pub fn take_outgoing(&mut self) -> Option<u8> {
        self.outgoing.take()
    }

    /// Injects a byte from the link partner into the incoming slot.
    /// If the serial is a slave waiting for an external clock, also kicks
    /// off the transfer.
    pub fn receive_byte(&mut self, byte: u8) {
        self.incoming = Some(byte);
        self.master_waiting = false;
        self.master_extend_count = 0;
        // Slave was armed (SC bit 7 set, bit 0 clear) and the master's byte
        // acts as the external clock — start the transfer now if idle.
        if !self.transferring && (self.sc & 0x80 != 0) {
            self.transferring = true;
            self.transfer_cycles = TRANSFER_CYCLES;
        }
    }

    /// Push SB / SC into the I/O register array so the game can read them.
    pub fn sync_to_memory(&self, memory: &mut Memory) {
        memory.write_direct(0xFF01, self.sb);
        let sc_in_mem = (memory.read_direct(0xFF02) & 0x01) | (self.sc & 0xFE);
        memory.write_direct(0xFF02, sc_in_mem);
    }
}
