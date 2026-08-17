//! Misdrop taxonomy and lock evaluation (Phase 1).

mod evaluate;

pub use evaluate::{
    evaluate_board_lock, evaluate_lock, misdrop_check_row, resolve_mtype_for_row,
    should_defer_row_verify, LockActual, LockExpectation,
};

/// Why a misdrop was recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MisdropReason {
    /// Final lock col/rot differs from `intended_lock` (authoritative check).
    LockColRotMismatch {
        want_col: usize,
        want_rot: usize,
        got_col: usize,
        got_rot: usize,
    },
    /// Row differs from intended lock.
    LockRowMismatch { want_row: i32, got_row: i32 },
    /// Ori byte glitch or lock-transition noise — log only, never count.
    PathAbortedGarbageOri,
    /// BFS planned an unreachable lock — test failure, not runtime misdrop.
    PlannerUnreachable,
}

/// Code path that requested a lock evaluation (for traces).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MisdropSite {
    /// `begin_drop`: grounded lock vs `intended_lock`.
    BeginDrop,
    /// `verify_pending_lock`: deferred tuck/spin settle vs snap.
    VerifyPendingLock,
}

impl MisdropSite {
    pub fn label(self) -> &'static str {
        match self {
            Self::BeginDrop => "begin_drop",
            Self::VerifyPendingLock => "verify_pending_lock",
        }
    }
}