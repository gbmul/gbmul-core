//! Board-diff lock pose inference and lock verify (`begin_drop`, deferred board check).

use super::board::{
    ori_info, ADDR_RNG_PTR, Bitboard, BOARD_COLS, BOARD_ROWS, PIECE_NAMES,
    SHAPES, SQ_ADDRS,
};
use super::bfs::piece_collides;
use super::memory::{
    at_top_shape_matches_ori, piece_left_col, piece_min_row, read_board_bitboard,
};
use super::misdrop::{
    evaluate_board_lock, misdrop_check_row, resolve_mtype_for_row, LockActual, LockExpectation,
    MisdropSite,
};
use super::{
    path_terminal_mtype, piece_pos_trustworthy, piece_trustworthily_grounded, BotState, TetrisBot,
};

const POST_LOCK_DELAY: u32 = 4;
const DROPPING_TIMEOUT: u32 = 50;

/// In-board cells for a lock pose (I vertical at row 17 has only 3 visible cells).
fn visible_lock_cells(type_idx: usize, rot: usize, row: i32, col: i32) -> Vec<(i32, i32)> {
    SHAPES[type_idx][rot]
        .iter()
        .filter_map(|&[dr, dc]| {
            let r = row + dr as i32;
            let c = col + dc as i32;
            if r >= 0 && r < BOARD_ROWS as i32 && c >= 0 && c < BOARD_COLS as i32 {
                Some((r, c))
            } else {
                None
            }
        })
        .collect()
}

/// Every in-board shape cell occupied (OOB-below cells skipped for bottom locks).
pub(super) fn lock_anchor_filled(bb: &Bitboard, type_idx: usize, rot: usize, row: i32, col: i32) -> bool {
    let shape = &SHAPES[type_idx][rot];
    let mut in_board = 0u32;
    let mut filled = 0u32;
    for &[dr, dc] in shape {
        let r = row + dr as i32;
        let c = col + dc as i32;
        if r < 0 || c < 0 || c >= BOARD_COLS as i32 {
            return false;
        }
        if r >= BOARD_ROWS as i32 {
            continue;
        }
        in_board += 1;
        if (bb[r as usize] & (1 << c)) != 0 {
            filled += 1;
        }
    }
    if filled != in_board || in_board == 0 {
        return false;
    }
    let expected_in_board = shape
        .iter()
        .filter(|&&[dr, dc]| {
            let r = row + dr as i32;
            let c = col + dc as i32;
            r >= 0 && r < BOARD_ROWS as i32 && c >= 0 && c < BOARD_COLS as i32
        })
        .count() as u32;
    filled == expected_in_board
}

fn lock_pose_matches_new_cells(
    type_idx: usize,
    rot: usize,
    row: i32,
    col: i32,
    new_cells: &[(i32, i32)],
) -> bool {
    let mut shape_cells = visible_lock_cells(type_idx, rot, row, col);
    if shape_cells.is_empty() {
        return false;
    }
    shape_cells.sort();
    shape_cells == *new_cells
}

/// BFS anchor row for a locked piece at `col`/`rot` (not the bottom cell row).
pub(crate) fn find_board_lock_row(bb: &Bitboard, type_idx: usize, rot: usize, col: i32) -> Option<i32> {
    let max_visible = (0..BOARD_ROWS as i32)
        .map(|row| visible_lock_cells(type_idx, rot, row, col).len())
        .max()
        .unwrap_or(0);
    for row in 0..BOARD_ROWS as i32 {
        if visible_lock_cells(type_idx, rot, row, col).len() < max_visible {
            continue;
        }
        if lock_anchor_filled(bb, type_idx, rot, row, col) {
            return Some(row);
        }
    }
    None
}

pub(super) fn board_new_cells(before: &Bitboard, after: &Bitboard) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    for row in 0..BOARD_ROWS {
        for col in 0..BOARD_COLS {
            let was = (before[row] & (1 << col)) != 0;
            let now = (after[row] & (1 << col)) != 0;
            if now && !was {
                out.push((row as i32, col as i32));
            }
        }
    }
    out
}

pub(super) fn infer_lock_pose_from_new_cells(
    before: &Bitboard,
    after: &Bitboard,
    type_idx: usize,
) -> Option<(i32, i32, usize)> {
    let mut new_cells = board_new_cells(before, after);
    if new_cells.is_empty() {
        return None;
    }
    new_cells.sort();
    for rot in 0..4 {
        for col in 0..BOARD_COLS as i32 {
            for row in (0..BOARD_ROWS as i32).rev() {
                if lock_pose_matches_new_cells(type_idx, rot, row, col, &new_cells) {
                    return Some((row, col, rot));
                }
            }
        }
    }
    None
}

fn infer_lock_row_at_pose(
    before: &Bitboard,
    after: &Bitboard,
    type_idx: usize,
    rot: usize,
    col: i32,
) -> Option<i32> {
    let mut new_cells = board_new_cells(before, after);
    if new_cells.is_empty() {
        return None;
    }
    new_cells.sort();
    for row in (0..BOARD_ROWS as i32).rev() {
        if lock_pose_matches_new_cells(type_idx, rot, row, col, &new_cells) {
            return Some(row);
        }
    }
    None
}

/// Newly locked piece pose: diff inference first, then anchor appeared since `before`.
pub(super) fn find_new_lock_pose(
    before: Option<&Bitboard>,
    after: &Bitboard,
    type_idx: usize,
) -> Option<(i32, i32, usize)> {
    if let Some(b) = before {
        if let Some(pose) = infer_lock_pose_from_new_cells(b, after, type_idx) {
            return Some(pose);
        }
        let mut best: Option<(i32, i32, usize)> = None;
        for rot in 0..4 {
            for col in 0..BOARD_COLS as i32 {
                for row in (0..BOARD_ROWS as i32).rev() {
                    if lock_anchor_filled(after, type_idx, rot, row, col)
                        && !lock_anchor_filled(b, type_idx, rot, row, col)
                    {
                        best = Some((row, col, rot));
                    }
                }
            }
        }
        return best;
    }
    None
}

impl TetrisBot {
    fn is_normal_fast_path(&self) -> bool {
        self.intended_lock
            .as_ref()
            .is_some_and(|(_, _, _, t)| t == "normal")
            && self.move_path.is_empty()
    }
    fn effective_lock_row(
        bb: &Bitboard,
        type_idx: usize,
        rot: usize,
        min_row: i32,
        col: usize,
        mtype: Option<&str>,
    ) -> i32 {
        if mtype == Some("tuck") {
            return min_row;
        }
        let mut grav = min_row;
        while !piece_collides(bb, type_idx, rot, grav + 1, col as i32) {
            grav += 1;
        }
        grav
    }
    pub(super) fn note_valid_piece_snap(&mut self, row: i32, col: usize, rot: usize) {
        if piece_pos_trustworthy(row, col) {
            self.last_valid_snap = Some((row, col, rot));
        }
    }

    pub(super) fn clear_lock_verify_state(&mut self) {
        self.pending_lock_verify = false;
        self.lock_verify_col_rot_ok = false;
        self.lock_verify_post_frame = false;
        self.lock_verify_board_before = None;
        self.lock_verify_path_incomplete = false;
    }

    pub(super) fn placement_piece_labels(&self) -> (String, String) {
        match self.last_placement.as_ref() {
            Some(lp) => (
                PIECE_NAMES
                    .get(lp.current_piece.piece_type)
                    .copied()
                    .unwrap_or("?")
                    .to_string(),
                PIECE_NAMES
                    .get(lp.next_piece.piece_type)
                    .copied()
                    .unwrap_or("?")
                    .to_string(),
            ),
            None => ("?".into(), "?".into()),
        }
    }

    pub(super) fn schedule_lock_verify(&mut self, read_r: &impl Fn(u16, u16) -> Vec<u8>, col_rot_ok: bool) {
        if self.lock_verify_board_before.is_none() {
            self.lock_verify_board_before = Some(read_board_bitboard(read_r));
        }
        self.pending_lock_verify = true;
        self.lock_verify_col_rot_ok = self.lock_verify_col_rot_ok || col_rot_ok;
    }

    /// Resolve got values for misdrop metadata; prefer last_valid_snap over ARE garbage.
    fn resolve_got_actual(
        &self,
        raw_row: i32,
        raw_col: usize,
        raw_rot: usize,
        eff_row: Option<i32>,
    ) -> (usize, usize, Option<i32>, bool) {
        if piece_pos_trustworthy(raw_row, raw_col) {
            return (raw_col, raw_rot, eff_row.or(Some(raw_row)), true);
        }
        if let Some((r, c, rot)) = self.last_valid_snap {
            return (c, rot, Some(r), true);
        }
        (raw_col, raw_rot, eff_row, false)
    }
    pub(super) fn begin_drop(&mut self, read: &impl Fn(u16)->u8, read_r: &impl Fn(u16,u16)->Vec<u8>, ori: u8, actions: &mut Vec<(u8,bool)>) {
        let al = piece_left_col(|a| read(a));
        let ar = ori_info(ori).map(|i| i.1 as usize).unwrap_or(0xff);
        let min_row = piece_min_row(|a| read(a));
        self.total_drops += 1;

        let pinfo = ori_info(ori);
        let bb = read_board_bitboard(|s, l| read_r(s, l));
        let grounded = pinfo.is_some_and(|i| {
            piece_trustworthily_grounded(&bb, i.0, ar, min_row, al)
        });
        let pos_trust = piece_pos_trustworthy(min_row, al);

        if grounded && pos_trust {
            let path_mtype = if !self.move_path.is_empty() {
                path_terminal_mtype(&self.move_path).to_string()
            } else {
                String::new()
            };
            let intended = self.intended_lock.clone();
            let (want_col, want_rot, want_row, mtype_for_row) = match intended {
                Some((il_row, il_col, il_rot, il_mtype)) => {
                    let mtype_for_row = resolve_mtype_for_row(&il_mtype, &path_mtype);
                    (il_col as usize, il_rot, Some(il_row), mtype_for_row)
                }
                None => (self.target_left, self.target_rot, None, "normal"),
            };

            let eff_row = pinfo.map(|i| {
                Self::effective_lock_row(&bb, i.0, ar, min_row, al, Some(mtype_for_row))
            }).unwrap_or(min_row);

            let expectation = LockExpectation {
                want_col,
                want_rot,
                want_row,
                mtype_for_row,
            };
            let actual = LockActual {
                col: al,
                rot: ar,
                eff_row,
            };
            let col_rot_ok = actual.col == expectation.want_col
                && actual.rot == expectation.want_rot;
            // Always board-verify grounded locks — sprite can match while board pose is wrong
            // (T-tuck col/rot lie, spin floor-kick row-short, I horizontal above floor).
            self.schedule_lock_verify(read_r, col_rot_ok);
            if mtype_for_row != "normal" && piece_pos_trustworthy(min_row, al) {
                self.note_valid_piece_snap(min_row, al, ar);
            }
        } else if self.intended_lock.is_some() {
            let path_mtype = if !self.move_path.is_empty() {
                path_terminal_mtype(&self.move_path)
            } else {
                "normal"
            };
            let il_mtype = self
                .intended_lock
                .as_ref()
                .map(|(_, _, _, t)| t.as_str())
                .unwrap_or("normal");
            let mtype_for_row = resolve_mtype_for_row(il_mtype, path_mtype);
            if misdrop_check_row(mtype_for_row, &self.move_path) {
                let col_rot_ok = self
                    .intended_lock
                    .as_ref()
                    .is_some_and(|(_, wc, wr, _)| al == *wc as usize && ar == *wr);
                self.schedule_lock_verify(read_r, col_rot_ok);
            }
        }
        if self.soft_drop_mode {
            actions.push((5, true)); self.holding_down = true; self.frame_delay = 0;
        } else {
            actions.push((4, true)); self.pending_release = Some(4); self.frame_delay = POST_LOCK_DELAY;
        }
        self.last_ori = ori;
        let mut snap = [0u16;4];
        for (i, &[y,x]) in SQ_ADDRS.iter().enumerate() {
            let py = read(y) as u16; let px = read(x) as u16; snap[i] = (py<<8) | px;
        }
        self.pre_drop_sq_snapshot = Some(snap);
        self.pre_drop_rng_ptr = Some(read(ADDR_RNG_PTR));
        self.state = BotState::Dropping;
        self.status_msg = "dropping".to_string();

        let sprite_row = if grounded && pos_trust {
            let path_mtype = if !self.move_path.is_empty() {
                path_terminal_mtype(&self.move_path)
            } else {
                "normal"
            };
            let mtype_for_row = self
                .intended_lock
                .as_ref()
                .map(|(_, _, _, t)| resolve_mtype_for_row(t, path_mtype))
                .unwrap_or("normal");
            pinfo
                .map(|i| {
                    Self::effective_lock_row(&bb, i.0, ar, min_row, al, Some(mtype_for_row))
                })
                .unwrap_or(min_row)
        } else {
            min_row
        };
        self.start_lock_audit(sprite_row, al, ar, self.pending_lock_verify);

        // If we were suppressing for a just-restored replay piece, the placement decision
        // for it is now complete (this begin_drop). Clear so future pieces are evaluated normally.
        if self.replay_restore_suppress {
            self.replay_restore_suppress = false;
        }
    }

    /// Post-frame lock verify: board diff is ground truth for col/rot/row.
    pub(super) fn verify_lock_post_frame(&mut self, read_r: &impl Fn(u16, u16) -> Vec<u8>) {
        if self.replay_restore_suppress {
            self.finalize_lock_audit(None, "replay_suppressed", false);
            self.clear_lock_verify_state();
            return;
        }
        let before = self.lock_verify_board_before.take();
        self.pending_lock_verify = false;
        self.lock_verify_col_rot_ok = false;

        let Some((il_row, il_col, il_rot, il_mtype)) = self.intended_lock.clone() else {
            self.finalize_lock_audit(None, "no_intended_lock", false);
            return;
        };
        let type_idx = self
            .last_placement
            .as_ref()
            .map(|lp| lp.current_piece.piece_type)
            .unwrap_or(99);
        if type_idx > 6 {
            self.finalize_lock_audit(None, "invalid_piece_type", false);
            return;
        }

        let after = read_board_bitboard(read_r);
        let want_col = il_col as usize;
        let path = if self.move_path.is_empty() {
            self.planned_path.clone()
        } else {
            self.move_path.clone()
        };
        let path_mtype = if path.is_empty() {
            "normal"
        } else {
            path_terminal_mtype(&path)
        };
        let mtype_for_row = resolve_mtype_for_row(&il_mtype, path_mtype);

        let expectation = LockExpectation {
            want_col,
            want_rot: il_rot,
            want_row: Some(il_row),
            mtype_for_row,
        };

        let path_incomplete = self.lock_verify_path_incomplete;
        self.lock_verify_path_incomplete = false;

        if !path_incomplete
            && lock_anchor_filled(&after, type_idx, il_rot, il_row, il_col)
        {
            self.finalize_lock_audit(
                Some((il_row, il_col, il_rot)),
                "board_anchor_ok",
                false,
            );
            return;
        }

        // Piece merges one cell per frame — unconstrained find_board_lock_row can spuriously
        // match anchor+1 (O on stack: row-16 shape filled before row-15 top cells merge).
        if let Some(ref before) = before {
            let new_cells = board_new_cells(before, &after);
            if !new_cells.is_empty() && new_cells.len() < 4 {
                self.lock_verify_board_before = Some(*before);
                self.pending_lock_verify = true;
                self.lock_verify_post_frame = true;
                self.lock_verify_path_incomplete = path_incomplete;
                return;
            }
        }

        // Landed pose from board diff (evaluate_board_lock is single col/row/rot authority).
        let landed = find_new_lock_pose(before.as_ref(), &after, type_idx);

        if let Some((got_row, got_col, got_rot)) = landed {
            let actual = LockActual {
                col: got_col as usize,
                rot: got_rot,
                eff_row: got_row,
            };
            let misdrop = evaluate_board_lock(&expectation, &actual, &path).is_some();
            if misdrop {
                self.record_board_lock_misdrop(
                    MisdropSite::VerifyPendingLock,
                    expectation,
                    actual,
                    got_col as usize,
                    got_rot,
                    Some(got_row),
                    true,
                    path,
                );
            }
            self.finalize_lock_audit(
                Some((got_row, got_col, got_rot)),
                if misdrop { "board_misdrop" } else { "board_ok" },
                misdrop,
            );
        } else if path_incomplete && matches!(mtype_for_row, "tuck" | "spin" | "tspin") {
            self.finalize_lock_audit(None, "path_incomplete_no_board_pose", false);
        } else {
            log::warn!(
                "[lock-verify] board_pose_unknown piece_type={type_idx} had_before={}",
                before.is_some()
            );
            self.finalize_lock_audit(None, "board_pose_unknown", false);
        }
    }

    /// Board-based verify helper (unit tests).
    pub(crate) fn verify_normal_fast_lock_from_board(&mut self, read_r: &impl Fn(u16, u16) -> Vec<u8>) {
        if !self.pending_lock_verify || !self.is_normal_fast_path() {
            return;
        }
        let col_rot_ok = self.lock_verify_col_rot_ok;
        self.pending_lock_verify = false;
        self.lock_verify_col_rot_ok = false;

        let Some((il_row, il_col, il_rot, _)) = self.intended_lock.clone() else {
            return;
        };
        let type_idx = self
            .last_placement
            .as_ref()
            .map(|lp| lp.current_piece.piece_type)
            .unwrap_or(99);
        if type_idx > 6 {
            return;
        }

        let bb = read_board_bitboard(read_r);
        let want_col = il_col as usize;
        if lock_anchor_filled(&bb, type_idx, il_rot, il_row, il_col) {
            return;
        }
        if let Some(gr) = find_board_lock_row(&bb, type_idx, il_rot, il_col) {
            if gr == il_row {
                return;
            }
        } else if col_rot_ok {
            // Board not merged yet (timing / line clear) — trust begin_drop col/rot.
            return;
        }

        let (got_col, got_rot, got_row) =
            if let Some(gr) = find_board_lock_row(&bb, type_idx, il_rot, il_col) {
                (want_col, il_rot, gr)
            } else if let Some((sr, sc, srot)) = self.last_valid_snap {
                (sc, srot, sr)
            } else {
                (want_col, il_rot, il_row)
            };

        let expectation = LockExpectation {
            want_col,
            want_rot: il_rot,
            want_row: Some(il_row),
            mtype_for_row: "normal",
        };
        let actual = LockActual {
            col: got_col,
            rot: got_rot,
            eff_row: got_row,
        };
        let path = self.planned_path.clone();
        self.record_lock_misdrop(
            MisdropSite::VerifyPendingLock,
            expectation,
            actual,
            got_col,
            got_rot,
            Some(got_row),
            true,
            path,
        );
    }

    pub(crate) fn verify_pending_lock(
        &mut self,
        read: &impl Fn(u16) -> u8,
        read_r: &impl Fn(u16, u16) -> Vec<u8>,
    ) {
        if !self.pending_lock_verify {
            return;
        }
        self.pending_lock_verify = false;
        let col_rot_ok = self.lock_verify_col_rot_ok;
        self.lock_verify_col_rot_ok = false;

        let Some((il_row, il_col, il_rot, il_mtype)) = self.intended_lock.clone() else {
            return;
        };

        let path_mtype_early = path_terminal_mtype(&self.move_path).to_string();
        let mtype_early = resolve_mtype_for_row(&il_mtype, &path_mtype_early);
        if let Some(before) = self.lock_verify_board_before.take() {
            let type_idx = self
                .last_placement
                .as_ref()
                .map(|lp| lp.current_piece.piece_type)
                .unwrap_or(99);
            if type_idx <= 6 {
                let after = read_board_bitboard(read_r);
                if let Some((got_row, got_col, got_rot)) =
                    infer_lock_pose_from_new_cells(&before, &after, type_idx)
                {
                    let path = self.move_path.clone();
                    let mtype_for_row = resolve_mtype_for_row(&il_mtype, &path_mtype_early);
                    let expectation = LockExpectation {
                        want_col: il_col as usize,
                        want_rot: il_rot,
                        want_row: Some(il_row),
                        mtype_for_row,
                    };
                    let actual = LockActual {
                        col: got_col as usize,
                        rot: got_rot,
                        eff_row: got_row,
                    };
                    self.record_board_lock_misdrop(
                        MisdropSite::VerifyPendingLock,
                        expectation,
                        actual,
                        got_col as usize,
                        got_rot,
                        Some(got_row),
                        true,
                        path,
                    );
                    return;
                }
            }
        }
        if col_rot_ok && mtype_early == "tuck" {
            return;
        }
        let Some((snap_row, snap_col, snap_rot)) = self.last_valid_snap else {
            return;
        };
        if !piece_pos_trustworthy(snap_row, snap_col) {
            return;
        }

        let path_mtype = path_terminal_mtype(&self.move_path).to_string();
        let mtype_for_row = resolve_mtype_for_row(&il_mtype, &path_mtype);

        let type_idx = self
            .last_placement
            .as_ref()
            .map(|lp| lp.current_piece.piece_type)
            .unwrap_or(99);
        if type_idx > 6 {
            return;
        }

        let bb = read_board_bitboard(read_r);
        // Normal deferred verify: trust the settled sprite min_row snap (post-frame).
        // Re-running effective_lock_row here often reads one row deep on timing skew.
        let eff_row = if col_rot_ok && mtype_for_row == "normal" {
            snap_row
        } else {
            Self::effective_lock_row(
                &bb, type_idx, snap_rot, snap_row, snap_col, Some(mtype_for_row),
            )
        };
        let expectation = LockExpectation {
            want_col: il_col as usize,
            want_rot: il_rot,
            want_row: Some(il_row),
            mtype_for_row,
        };
        let actual = LockActual {
            col: snap_col,
            rot: snap_rot,
            eff_row,
        };
        let path = self.move_path.clone();
        self.record_lock_misdrop(
            MisdropSite::VerifyPendingLock,
            expectation,
            actual,
            snap_col,
            snap_rot,
            Some(eff_row),
            true,
            path,
        );
    }

    pub(super) fn handle_dropping(
        &mut self,
        read: &impl Fn(u16) -> u8,
        read_r: &impl Fn(u16, u16) -> Vec<u8>,
        ori: u8,
        actions: &mut Vec<(u8, bool)>,
    ) {
        let ori_c = ori != self.last_ori;
        // Track falling pose each frame so deferred lock verify uses a post-gravity snap.
        if !ori_c {
            if let Some((_, rot)) = ori_info(ori) {
                let min_row = piece_min_row(read);
                let col = piece_left_col(read);
                if piece_pos_trustworthy(min_row, col) {
                    self.note_valid_piece_snap(min_row, col, rot as usize);
                }
            }
        }
        let at = piece_min_row(|a| read(a)) <= 2;
        let sp = self.pre_drop_rng_ptr.map_or(false, |o| read(ADDR_RNG_PTR) != o);
        let sh = at_top_shape_matches_ori(|a| read(a), ori);
        let np = if self.soft_drop_mode {
            (ori_c && at) || (sp && sh)
        } else {
            let sqc = self.pre_drop_sq_snapshot.map_or(true, |s| SQ_ADDRS.iter().enumerate().any(|(i,&[y,x])| { let py=read(y) as u16; let px=read(x) as u16; ((py<<8)|px) != s[i] }));
            let sts = !ori_c && !sqc && sh && at && self.dropping_wait == 0;
            (ori_c && at) || ((sp || sqc) && sh) || sts
        };
        if !np && self.dropping_wait < DROPPING_TIMEOUT { self.dropping_wait += 1; return; }
        if self.holding_down { actions.push((5, false)); self.holding_down = false; }
        if self.pending_lock_verify {
            // Keep the pre-lock snapshot from begin_drop / mid-path schedule_lock_verify.
            // Overwriting here (after next-piece ARE) is too late — the piece has already
            // merged, board diff is empty, and verify returns board_pose_unknown (Z spin).
            if self.lock_verify_board_before.is_none() {
                self.lock_verify_board_before = Some(read_board_bitboard(read_r));
            }
            self.lock_verify_post_frame = true;
        }
        self.dropping_wait = 0;
        self.last_ori = 0xff;
        self.last_drop_frame = self.frame_count;
        self.state = BotState::Idle;
    }
}
