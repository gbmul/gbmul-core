//! BFS planning entry: `plan()` and safe-normal fallback.

use super::board::{ori_info, ADDR_NEXT_ORI};
use super::memory::{piece_left_col, piece_min_row, read_board_bitboard};
use super::planning::{
    find_best_move_with_bfs, find_best_move_with_bfs_1ply, find_bfs_path_to_lock,
    find_safe_normal_placement,
};
use super::{
    bfs_plan_acceptable, classify_move, fix_s_spin_cw_terminal, fix_sz_spin_path,
    piece_pos_trustworthy, plan_row_before_final_action, prefer_simplest_equivalent_path,
    synth_normal_execution_path, trim_redundant_setup_d, BotState, TetrisBot,
};

impl TetrisBot {
    pub(super) fn plan_safe_normal(
        &mut self,
        read: &impl Fn(u16) -> u8,
        read_r: &impl Fn(u16, u16) -> Vec<u8>,
        ori: u8,
        actions: &mut Vec<(u8, bool)>,
        info: &(usize, u8),
        actual_col: usize,
    ) -> bool {
        let Some((rot, col)) =
            find_safe_normal_placement(|a| read(a), |b, l| read_r(b, l), actual_col)
        else {
            return false;
        };
        self.meat_mode = "1ply-safe".into();
        self.intended_lock = None;
        self.clear_lock_verify_state();
        self.planned_path =
            synth_normal_execution_path(info.1 as usize, rot, actual_col, col);
        self.move_path.clear();
        self.path_step = 0;
        self.path_resync_stuck_frames = 0;
        self.target_rot = rot;
        self.target_left = col;
        self.rot_attempts = 0;
        self.trans_attempts = 0;
        let cur_r = info.1 as usize;
        let needed = (rot + 4 - cur_r) % 4;
        if needed > 0 {
            self.state = BotState::Rotating;
            self.status_msg = "safe-normal".to_string();
        } else {
            self.state = BotState::Translating;
            self.status_msg = "safe-normal".to_string();
        }
        let _ = (read, ori, actions);
        true
    }

    pub(super) fn plan(&mut self, read: &impl Fn(u16)->u8, read_r: &impl Fn(u16,u16)->Vec<u8>, ori: u8, actions: &mut Vec<(u8,bool)>) {
        let info = ori_info(ori);
        if info.is_none() { self.begin_drop(read, read_r, ori, actions); return; }
        let info = info.unwrap();

        let actual_row = piece_min_row(read);
        let actual_col = piece_left_col(read);
        let bfs_row = actual_row;

        let next_ori = read(ADDR_NEXT_ORI);
        let next_info = ori_info(next_ori);
        let next_idx = next_info.map(|i| i.0);

        let bb_for_class = read_board_bitboard(|s,l| read_r(s,l));

        let mut replay_want_used = false;
        let mut bfs_res = if let Some((want_row, want_col, want_rot)) = self.replay_want_lock.take() {
            find_bfs_path_to_lock(
                &bb_for_class,
                info.0,
                actual_row,
                actual_col,
                info.1 as usize,
                want_row,
                want_col,
                want_rot,
            )
        } else {
            None
        };
        if bfs_res.is_some() {
            replay_want_used = true;
        } else if bfs_res.is_none() {
            bfs_res = if let Some(nt) = next_idx {
                find_best_move_with_bfs(|a|read(a), |b,l|read_r(b,l), actual_col, bfs_row, Some(nt))
            } else {
                find_best_move_with_bfs_1ply(|a|read(a), |b,l|read_r(b,l), bfs_row, actual_col, info.1 as usize)
            };
        }

        let bfs_usable = bfs_res.as_ref().is_some_and(|(rot, col, path, row)| {
            bfs_plan_acceptable(
                &bb_for_class,
                info.0,
                actual_row,
                actual_col,
                info.1 as usize,
                0,
                bfs_row,
                *row,
                *col,
                *rot,
                path,
            )
        });

        if !bfs_usable {
            if self.plan_safe_normal(read, read_r, ori, actions, &info, actual_col) {
                return;
            }
            self.target_rot = info.1 as usize;
            self.target_left = actual_col;
            self.begin_drop(read, read_r, ori, actions);
            return;
        }

        self.meat_mode = if replay_want_used {
            "replay-want".into()
        } else if next_idx.is_some() {
            "2ply-bfs".into()
        } else {
            "1ply-bfs".into()
        };

        let (rot, col, path, row) = bfs_res.unwrap();
        let sim_mtype = classify_move(&bb_for_class, info.0, row, col as i32, rot, &path, 0);
        let mtype = if replay_want_used {
            match self.replay_want_mtype.take().as_deref() {
                Some("tuck") => "tuck",
                Some("spin") | Some("tspin") => "spin",
                _ => sim_mtype,
            }
        } else {
            sim_mtype
        };

        self.target_rot = rot;
        self.target_left = col;
        let mut full_path = path.clone();
        full_path = fix_sz_spin_path(
            &bb_for_class,
            info.0,
            actual_row,
            actual_col,
            info.1 as usize,
            mtype,
            &full_path,
        );
        full_path = fix_s_spin_cw_terminal(
            &bb_for_class,
            info.0,
            actual_row,
            actual_col,
            info.1 as usize,
            row,
            col as usize,
            rot,
            mtype,
            &full_path,
        );
        full_path = trim_redundant_setup_d(
            &bb_for_class,
            info.0,
            actual_row,
            actual_col as i32,
            info.1 as usize,
            full_path,
        );
        full_path = prefer_simplest_equivalent_path(
            &bb_for_class,
            info.0,
            actual_row,
            actual_col as i32,
            info.1 as usize,
            row,
            col as i32,
            rot,
            &full_path,
        );

        self.plan_intended_row = plan_row_before_final_action(
            &bb_for_class,
            info.0,
            actual_row,
            actual_col as i32,
            info.1 as usize,
            &full_path,
            row,
            mtype,
            rot,
        );
        self.path_resync_stuck_frames = 0;
        self.path_rot_wait_frames = 0;
        self.rot_retry_settle_frames = 0;
        self.clear_lock_verify_state();
        self.intended_lock = Some((row, col as i32, rot, mtype.to_string()));
        self.planned_path = full_path.clone();

        if mtype == "normal" {
            // fast simple execution for normal placements (rot then trans then drop)
            self.move_path.clear();
            self.path_step = 0;
            self.rot_attempts = 0;
            self.trans_attempts = 0;
            let cur_r = info.1 as usize;
            let needed = (rot + 4 - cur_r) % 4;
            if needed > 0 {
                self.state = BotState::Rotating;
                self.status_msg = "rotating".to_string();
            } else {
                self.state = BotState::Translating;
                self.status_msg = "translating".to_string();
            }
            return;
        }

        self.move_path = full_path;
        self.path_step = 0;
        self.path_start_row = actual_row;
        self.path_start_col = actual_col;
        self.path_start_rot = info.1 as usize;
        self.path_commit_row = actual_row;
        self.path_commit_col = actual_col;
        self.path_commit_rot = info.1 as usize;
        self.path_pending_action = None;
        self.holding_down = false;
        self.path_down_min_frames = 0;
        self.path_down_release_armed = false;
        self.path_held_btn = None;
        actions.push((5, false));
        self.last_valid_snap = if piece_pos_trustworthy(actual_row, actual_col) {
            Some((actual_row, actual_col, info.1 as usize))
        } else {
            None
        };
        self.post_rot_sync = false;
        self.row_wait_count = 0;
        self.col_drift_count = 0;
        self.rot_settle_frames = 0;
        self.lateral_settle_frames = 0;
        self.path_trace.clear();
        self.state = BotState::Path;
        // Fresh plan after savestate restore.
        self.replay_restore_suppress = false;
        self.state_restore_replan = false;
        // Pre-lock board for mid-path verify (active piece is not in the bitboard yet).
        self.lock_verify_board_before = Some(bb_for_class);

        self.status_msg = format!("path({} steps)", self.move_path.len());
        let (piece, next) = self.placement_piece_labels();
        self.trace_path(format!(
            "plan {} steps {:?} piece={piece} next={next} want=({},{},r{}) mtype={}",
            self.move_path.len(),
            self.move_path,
            row,
            col,
            rot,
            mtype
        ));
    }
}
