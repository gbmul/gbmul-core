// Link cable abstraction
//
// Trait-based interface over the serial transport so different backends can be
// swapped without touching the emulator core.
//
// Note: no `Send` bound — WASM is single-threaded, so `Rc<RefCell<>>` is safe.
//
// Implementations:
//   LoopbackLink — returns 0xFF for every exchange (no cable / solo play).
//   SharedLink   — two Rc-backed queues connecting two in-process Emulators.
//                  Use SharedLink::pair() to create a matched set.

use std::rc::Rc;
use std::cell::RefCell;
use std::collections::VecDeque;

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

pub trait LinkCable {
    /// Master-mode send: push `outgoing` to the partner. Returns 0xFF (the
    /// actual response arrives asynchronously via `poll_incoming`).
    fn exchange(&mut self, outgoing: u8) -> u8;

    /// Slave-mode exchange: only completes if the master has already sent a
    /// byte. Returns `Some(incoming)` and queues our reply; `None` means the
    /// transfer stays pending (no spurious IRQ).
    fn slave_exchange(&mut self, outgoing: u8) -> Option<u8> {
        Some(self.exchange(outgoing))
    }

    /// Non-blocking receive: returns the next byte the partner sent, if any.
    /// Used by the master after sending to pick up the slave's response.
    fn poll_incoming(&mut self) -> Option<u8> {
        None
    }
}

// ---------------------------------------------------------------------------
// LoopbackLink
// ---------------------------------------------------------------------------

/// No cable — every exchange returns 0xFF; slave transfers never complete.
pub struct LoopbackLink;

impl LinkCable for LoopbackLink {
    fn exchange(&mut self, _outgoing: u8) -> u8 {
        0xFF
    }
    fn slave_exchange(&mut self, _outgoing: u8) -> Option<u8> {
        None
    }
}

// ---------------------------------------------------------------------------
// SharedLink
// ---------------------------------------------------------------------------

/// One half of an in-process link cable backed by two `VecDeque` queues
/// shared via `Rc<RefCell<>>`. Safe in single-threaded WASM.
///
/// ```
/// let (link_a, link_b) = SharedLink::pair();
/// emu_a.set_link(Box::new(link_a));
/// emu_b.set_link(Box::new(link_b));
/// ```
pub struct SharedLink {
    send_q: Rc<RefCell<VecDeque<u8>>>,
    recv_q: Rc<RefCell<VecDeque<u8>>>,
}

impl SharedLink {
    pub fn pair() -> (SharedLink, SharedLink) {
        let q_ab = Rc::new(RefCell::new(VecDeque::new()));
        let q_ba = Rc::new(RefCell::new(VecDeque::new()));
        (
            SharedLink { send_q: Rc::clone(&q_ab), recv_q: Rc::clone(&q_ba) },
            SharedLink { send_q: Rc::clone(&q_ba), recv_q: Rc::clone(&q_ab) },
        )
    }
}

impl LinkCable for SharedLink {
    /// Master send: push our byte into the partner's receive queue.
    /// Does not block — response arrives via `poll_incoming`.
    fn exchange(&mut self, outgoing: u8) -> u8 {
        self.send_q.borrow_mut().push_back(outgoing);
        0xFF
    }

    /// Non-blocking receive: pick up whatever the partner has queued.
    fn poll_incoming(&mut self) -> Option<u8> {
        self.recv_q.borrow_mut().pop_front()
    }

    /// Slave path: only complete if the master already put a byte in our
    /// queue. If nothing arrived yet, return None so the transfer stays
    /// pending and no spurious serial IRQ fires.
    fn slave_exchange(&mut self, outgoing: u8) -> Option<u8> {
        if let Some(incoming) = self.recv_q.borrow_mut().pop_front() {
            self.send_q.borrow_mut().push_back(outgoing);
            Some(incoming)
        } else {
            None
        }
    }
}
