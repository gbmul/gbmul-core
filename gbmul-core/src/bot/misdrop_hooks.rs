//! Misdrop recording and replay persistence (Phase 1 hooks).

use super::misdrop::{
    evaluate_board_lock, evaluate_lock, LockActual, LockExpectation, MisdropSite,
};
use super::replay::MisdropContext;
use super::{path_terminal_mtype, TetrisBot};

impl TetrisBot {
    fn is_critical_stack(&self) -> bool {
        // Board no longer stored in PlacementReplay. Critical detection via full
        // state on host or can be reimplemented with a live board read at persist time.
        // For now we don't mark entries critical (simpler, and full savestate has the info).
        false
    }

    pub(crate) fn classify_move_type(path: &[String]) -> &'static str {
        if path.is_empty() {
            return "normal";
        }
        match path_terminal_mtype(path) {
            "spin" => "spin",
            "tuck" => "tuck",
            _ => "path",
        }
    }

    /// Replay restore: skip misdrop sites until `begin_drop` finishes the restored piece.
    /// Do not clear the suppress flag here — one suppressed glitch must not unblock the next frame.
    pub(super) fn skip_misdrop_if_replay_restore(&mut self) -> bool {
        if self.replay_restore_suppress {
            self.pending_replay = None;
            true
        } else {
            false
        }
    }

    /// Single misdrop authority (Phase 1): only `begin_drop` and `verify_pending_lock` call this.
    pub(super) fn record_lock_misdrop(
        &mut self,
        site: MisdropSite,
        expectation: LockExpectation,
        actual: LockActual,
        got_col: usize,
        got_rot: usize,
        got_row: Option<i32>,
        got_valid: bool,
        path: Vec<String>,
    ) {
        if self.replay_restore_suppress {
            self.pending_replay = None;
            return;
        }
        let Some(reason) = evaluate_lock(&expectation, &actual, &path) else {
            return;
        };
        self.misdrop_count += 1;
        log::warn!(
            "[misdrop] site={} reason={reason:?}",
            site.label()
        );
        self.persist_misdrop_replay(
            expectation.want_col,
            expectation.want_rot,
            expectation.want_row,
            got_col,
            got_rot,
            got_row,
            got_valid,
            path,
        );
    }

    /// Board-diff verify: strict spin/tspin row (no +1 gravity tolerance).
    pub(super) fn record_board_lock_misdrop(
        &mut self,
        site: MisdropSite,
        expectation: LockExpectation,
        actual: LockActual,
        got_col: usize,
        got_rot: usize,
        got_row: Option<i32>,
        got_valid: bool,
        path: Vec<String>,
    ) {
        if self.replay_restore_suppress {
            self.pending_replay = None;
            return;
        }
        let Some(reason) = evaluate_board_lock(&expectation, &actual, &path) else {
            return;
        };
        self.misdrop_count += 1;
        log::warn!(
            "[misdrop] site={} reason={reason:?}",
            site.label()
        );
        self.persist_misdrop_replay(
            expectation.want_col,
            expectation.want_rot,
            expectation.want_row,
            got_col,
            got_rot,
            got_row,
            got_valid,
            path,
        );
    }

    fn persist_misdrop_replay(
        &mut self,
        wanted_col: usize,
        wanted_rot: usize,
        wanted_row: Option<i32>,
        actual_col: usize,
        actual_rot: usize,
        actual_row: Option<i32>,
        got_valid: bool,
        path: Vec<String>,
    ) {
        if self.replay_restore_suppress {
            self.pending_replay = None;
            return;
        }

        let critical = self.is_critical_stack();
        let replay_path = if path.is_empty() {
            self.planned_path.clone()
        } else {
            path
        };
        let move_type = self
            .intended_lock
            .as_ref()
            .map(|(_, _, _, t)| t.clone())
            .unwrap_or_else(|| Self::classify_move_type(&replay_path).to_string());
        let path_len = replay_path.len();

        if let Some(mut replay) = self.last_placement.take() {
            log::warn!(
                "[replay] MISDROP #{}/{} — cur={} rot={} → want col={} rot={} row={:?} got col={} rot={} row={:?} valid={} | type={} path_len={}",
                self.misdrop_count, self.total_drops,
                replay.current_piece.piece_type, replay.current_piece.rot,
                wanted_col, wanted_rot, wanted_row,
                actual_col, actual_rot, actual_row, got_valid,
                move_type, path_len
            );
            replay.misdrop = Some(MisdropContext {
                num: self.misdrop_count,
                total: self.total_drops,
                wanted_col,
                wanted_rot,
                wanted_row,
                actual_col,
                actual_rot,
                actual_row,
                got_valid,
                move_type,
                path_len,
                path: if replay_path.is_empty() {
                    None
                } else {
                    Some(replay_path)
                },
                critical,
            });
            self.pending_replay = Some(replay);
        } else {
            log::warn!(
                "[replay] MISDROP #{}/{} — no spawn snapshot available",
                self.misdrop_count, self.total_drops
            );
        }
    }
}
