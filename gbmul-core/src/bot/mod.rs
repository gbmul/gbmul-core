//! Core data models for the Tetris bot (NRS ROM).
//!
//! These are the pure, emulator-agnostic constants and utilities extracted
//! from the original JS implementation (bot-pieces.js + bot-board.js).
//!
//! Goal (M1): single source of truth in Rust so the same definitions are used
//! by the Web (WASM) client and the native handheld (SDL2) without duplication.

mod board;
mod bfs;
mod game_state;
mod memory;
mod lock_verify;
mod misdrop_hooks;
mod path_exec;
mod plan;
mod planning;
mod replay;

pub use board::*;
pub use bfs::*;
pub use game_state::*;
pub use memory::*;
pub use path_exec::*;
pub(crate) use path_exec::{
    IMPLICIT_DESCENT, LATERAL_CHAIN_SETTLE_FRAMES, MIN_BTN_HOLD_FRAMES, PATH_DOWN_HOLD_FRAMES,
};
pub use planning::*;
pub use replay::*;

#[cfg(test)]
pub(crate) use bfs::bfs_path_is_reachable;
pub(crate) use planning::simulate_place_and_clear;

mod srs;

pub mod fixture_manifest;
pub mod fixtures;
pub mod misdrop;

pub(crate) use lock_verify::find_board_lock_row;
pub(crate) use misdrop::{
    evaluate_lock, misdrop_check_row, resolve_mtype_for_row, LockActual, LockExpectation,
};


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BotState {
    Idle,
    Rotating,
    Translating,
    Dropping,
    Path,
}

const MAX_ROT_ATTEMPTS: u32 = 8;
const MAX_TRANS_ATTEMPTS: u32 = 16;
pub(super) const FRAME_DELAY: u32 = 1;
const NAV_START_INTERVAL: u32 = 40;
// ── First Rust bot version (M4+ integration) ────────────────────────────────

pub struct TetrisBot {
    pub nav_timer: u32,
    pending_release: Option<u8>,
    frame_delay: u32,

    state: BotState,
    last_ori: u8,
    prev_ori: u8,
    target_rot: usize,
    target_left: usize,
    rot_attempts: u32,
    trans_attempts: u32,
    replan_attempts: u32,
    dropping_wait: u32,
    pre_drop_rng_ptr: Option<u8>,
    pre_drop_sq_snapshot: Option<[u16; 4]>,
    soft_drop_mode: bool,
    holding_down: bool,
    /// Countdown before path soft-drop may release (min press duration).
    path_down_min_frames: u32,
    path_down_release_armed: bool,
    /// Generic held tap for path L/R (and other short taps).
    path_held_btn: Option<(u8, u32)>,
    frame_count: u32,
    last_drop_frame: u32,
    pps_limit: f64,
    total_drops: u32,
    misdrop_count: u32,
    pause_requested: bool,
    status_msg: String,
    /// Transient notification shown in GUI when garbage is detected/sent.
    garbage_notification: Option<String>,
    /// Frames remaining for the current garbage notification.
    garbage_notif_frames: u8,
    input_delay: u32,

    // Meatfighter BFS path execution fields
    /// BFS plan at `plan()` time — stored for misdrop replay even when `mtype == "normal"`
    /// uses fast rot/trans execution instead of `move_path` stepping.
    planned_path: Vec<String>,
    move_path: Vec<String>,
    path_step: usize,
    /// Anchor when path execution started (for simulation-based step sync).
    path_start_row: i32,
    path_start_col: usize,
    path_start_rot: usize,
    /// Last trustworthy on-board piece position (for misdrop got when ARE garbles reads).
    last_valid_snap: Option<(i32, usize, usize)>,
    path_expected_row: i32,
    path_expected_col: usize,
    path_expected_rot: usize,
    /// Position when path_step last advanced (confirm-only sync).
    path_commit_row: i32,
    path_commit_col: usize,
    path_commit_rot: usize,
    /// Action sent, waiting for emu to reflect it before path_step++.
    path_pending_action: Option<String>,
    post_rot_sync: bool,
    intended_lock: Option<(i32, i32, usize, String)>, // row, col, rot, moveType
    plan_intended_row: i32,
    piece_count: u32,
    row_wait_count: u32,
    col_drift_count: u32,
    /// Frames to wait after CW/CCW before treating rot mismatch as real (emu lag).
    rot_settle_frames: u32,
    /// Frames to wait after L/R confirm before CW/CCW (S→L: R then tuck CCW).
    lateral_settle_frames: u32,
    /// Frames stuck in tuck/spin block_replan without path_step progress.
    path_resync_stuck_frames: u32,
    /// Frames blocked on pending CW/CCW while gravity advances (idle-glitch stall).
    path_rot_wait_frames: u32,
    /// Cooldown after rot-confirm stall before re-issuing CW/CCW (avoid gravity-only fall).
    rot_retry_settle_frames: u32,
    /// Path finished while still falling — verify lock after hard-drop settles.
    pending_lock_verify: bool,
    /// Col/rot already matched live reads at begin_drop; deferred verify must not
    /// re-check against a stale last_valid_snap (tuck R,R,R finish false positives).
    lock_verify_col_rot_ok: bool,
    /// Deferred lock verify after run_frame (board diff — cells merged).
    lock_verify_post_frame: bool,
    /// Board snapshot from when verify was scheduled (before lock merge).
    lock_verify_board_before: Option<Bitboard>,
    /// Garbage tile (0x28) count from previous frame — detect garbage arrivals.
    prev_garbage_count: u32,
    /// Highest 0x28 count seen during current arrival burst (debounce).
    garbage_peak: u32,
    /// Frames remaining before garbage count is considered stable.
    garbage_debounce: u8,
    /// Last stable garbage count that was reported via log / replan trigger.
    garbage_last_reported: u32,
    /// Path did not finish before lock (mid-path transition).
    lock_verify_path_incomplete: bool,

    pub meat_mode: String,

    // For misdrop replay metadata capture (labels only; full state on host)
    last_placement: Option<PlacementReplay>,
    pending_replay: Option<PlacementReplay>,

    /// When true (set via begin_replay_restore after loading a misdrop savestate),
    /// the next misdrop detection is ignored: do not increment counters and do not
    /// produce a pending replay. Cleared after use. Prevents spurious misdrops
    /// caused by stale bot plan state right after a full state restore.
    replay_restore_suppress: bool,
    /// One-shot want lock for misdrop replay (row, col, rot). Consumed on first plan().
    replay_want_lock: Option<(i32, usize, usize)>,
    /// Captured move_type from misdrop meta ("tuck", "spin", …). Consumed on first plan().
    replay_want_mtype: Option<String>,
    /// After generic load_state: discard cached paths and run a fresh BFS plan.
    state_restore_replan: bool,
    /// Pending lock audit started at begin_drop; finalized in verify_lock_post_frame.
    lock_audit_pending: Option<replay::LockAuditEntry>,
    /// Drained by host via take_lock_audit_json → localStorage lock log.
    lock_audit_queue: Vec<replay::LockAuditEntry>,

    /// When false (default), do not press Start in splash/title/submenus.
    auto_menu_nav: bool,

    /// Recent path-execution trace lines (for browser console / WASM debug export).
    path_trace: Vec<String>,
    /// path_step when spin emu-kick BFS replan replaced the tail (disables post-setup-rot D wait).
    spin_emu_replan_at_step: Option<usize>,
}

impl TetrisBot {
    pub fn new() -> Self {
        TetrisBot {
            nav_timer: 128,
            pending_release: None,
            frame_delay: 0,
            state: BotState::Idle,
            last_ori: 0xff,
            prev_ori: 0xff,
            target_rot: 0,
            target_left: 0,
            rot_attempts: 0,
            trans_attempts: 0,
            replan_attempts: 0,
            dropping_wait: 0,
            pre_drop_rng_ptr: None,
            pre_drop_sq_snapshot: None,
            soft_drop_mode: false,
            holding_down: false,
            path_down_min_frames: 0,
            path_down_release_armed: false,
            path_held_btn: None,
            frame_count: 0,
            last_drop_frame: 0,
            pps_limit: f64::INFINITY,
            total_drops: 0,
            misdrop_count: 0,
            pause_requested: false,
            status_msg: "idle".into(),
            garbage_notification: None,
            garbage_notif_frames: 0,
            input_delay: 0,
            planned_path: Vec::new(),
            move_path: Vec::new(),
            path_step: 0,
            path_start_row: 0,
            path_start_col: 3,
            path_start_rot: 0,
            last_valid_snap: None,
            path_expected_row: 0,
            path_expected_col: 3,
            path_expected_rot: 0,
            path_commit_row: 0,
            path_commit_col: 3,
            path_commit_rot: 0,
            path_pending_action: None,
            post_rot_sync: false,
            intended_lock: None,
            plan_intended_row: 0,
            piece_count: 0,
            row_wait_count: 0,
            col_drift_count: 0,
            rot_settle_frames: 0,
            lateral_settle_frames: 0,
            path_resync_stuck_frames: 0,
            path_rot_wait_frames: 0,
            rot_retry_settle_frames: 0,
            pending_lock_verify: false,
            lock_verify_col_rot_ok: false,
            lock_verify_post_frame: false,
            lock_verify_board_before: None,
            prev_garbage_count: 0,
            garbage_peak: 0,
            garbage_debounce: 0,
            garbage_last_reported: 0,
            lock_verify_path_incomplete: false,
            meat_mode: "2ply-bfs".into(),
            last_placement: None,
            pending_replay: None,
            replay_restore_suppress: false,
            replay_want_lock: None,
            replay_want_mtype: None,
            state_restore_replan: false,
            lock_audit_pending: None,
            lock_audit_queue: Vec::new(),
            auto_menu_nav: false,
            path_trace: Vec::new(),
            spin_emu_replan_at_step: None,
        }
    }

    /// Drain new lock-audit entries as JSON array (host persists to localStorage).
    pub fn take_lock_audit_json(&mut self) -> String {
        if self.lock_audit_queue.is_empty() {
            return "[]".to_string();
        }
        let drained: Vec<_> = self.lock_audit_queue.drain(..).collect();
        serde_json::to_string(&drained).unwrap_or_else(|_| "[]".to_string())
    }

    pub(super) fn start_lock_audit(
        &mut self,
        sprite_row: i32,
        sprite_col: usize,
        sprite_rot: usize,
        verify_scheduled: bool,
    ) {
        let piece = self
            .last_placement
            .as_ref()
            .and_then(|lp| PIECE_NAMES.get(lp.current_piece.piece_type).copied())
            .unwrap_or("?")
            .to_string();
        let (want_row, want_col, want_rot, mtype) = match self.intended_lock.as_ref() {
            Some((r, c, rot, t)) => (Some(*r), Some(*c as usize), Some(*rot), t.clone()),
            None => (None, None, None, "normal".to_string()),
        };
        let path = if !self.move_path.is_empty() {
            self.move_path.clone()
        } else {
            self.planned_path.clone()
        };
        self.lock_audit_pending = Some(replay::LockAuditEntry {
            drop_num: self.total_drops,
            frame: self.frame_count,
            piece,
            mtype,
            want_row,
            want_col,
            want_rot,
            sprite_row,
            sprite_col,
            sprite_rot,
            board_row: None,
            board_col: None,
            board_rot: None,
            verify: if verify_scheduled {
                "pending".into()
            } else {
                "sprite_only".into()
            },
            misdrop_detected: false,
            path,
        });
        if !verify_scheduled {
            self.flush_lock_audit();
        }
    }

    pub(super) fn finalize_lock_audit(
        &mut self,
        board: Option<(i32, i32, usize)>,
        verify: &str,
        misdrop_detected: bool,
    ) {
        let Some(mut entry) = self.lock_audit_pending.take() else {
            return;
        };
        if let Some((r, c, rot)) = board {
            entry.board_row = Some(r);
            entry.board_col = Some(c);
            entry.board_rot = Some(rot);
        }
        entry.verify = verify.to_string();
        entry.misdrop_detected = misdrop_detected;
        if self.lock_audit_queue.len() >= 50 {
            self.lock_audit_queue.remove(0);
        }
        self.lock_audit_queue.push(entry);
    }

    fn flush_lock_audit(&mut self) {
        if let Some(entry) = self.lock_audit_pending.take() {
            if self.lock_audit_queue.len() >= 50 {
                self.lock_audit_queue.remove(0);
            }
            self.lock_audit_queue.push(entry);
        }
    }

    pub fn set_auto_menu_nav(&mut self, enabled: bool) {
        self.auto_menu_nav = enabled;
    }

    pub fn set_soft_drop_mode(&mut self, enabled: bool) { self.soft_drop_mode = enabled; }
    pub fn set_pps(&mut self, pps: f64) {
        self.pps_limit = if pps.is_infinite() { f64::INFINITY } else if pps > 0.0 { pps } else { 0.0 };
    }
    pub fn set_input_delay(&mut self, delay: u32) { self.input_delay = delay; }
    pub fn consume_pause_request(&mut self) -> bool {
        let v = self.pause_requested;
        self.pause_requested = false;
        v
    }

    pub fn reset(&mut self) {
        let tot = self.total_drops;
        let mis = self.misdrop_count;
        let auto_nav = self.auto_menu_nav;
        let pps = self.pps_limit;
        let input_delay = self.input_delay;
        let soft = self.soft_drop_mode;
        *self = Self::new();
        self.total_drops = tot;
        self.misdrop_count = mis;
        self.auto_menu_nav = auto_nav;
        self.pps_limit = pps;
        self.input_delay = input_delay;
        self.soft_drop_mode = soft;
    }
    pub fn reset_stats(&mut self) {
        self.total_drops = 0; self.misdrop_count = 0; self.last_drop_frame = 0;
    }

    /// Post-frame hook (call after `run_frame`). Board verify + sprite snap.
    pub fn tick_post_frame(
        &mut self,
        read: impl Fn(u16) -> u8,
        read_r: impl Fn(u16, u16) -> Vec<u8>,
    ) {
        if self.lock_verify_post_frame {
            self.lock_verify_post_frame = false;
            self.verify_lock_post_frame(&read_r);
            return;
        }
        if self.state == BotState::Dropping && self.pending_lock_verify {
            let ori = read(ADDR_CUR_ORI);
            if let Some((_, rot)) = ori_info(ori) {
                let min_row = piece_min_row(&read);
                let col = piece_left_col(&read);
                if piece_pos_trustworthy(min_row, col) {
                    self.note_valid_piece_snap(min_row, col, rot as usize);
                }
            }
        }
    }


    fn clear_planning_state_for_restore(&mut self) {
        self.pending_replay = None;
        self.last_placement = None;
        self.intended_lock = None;
        self.holding_down = false;
        self.path_down_min_frames = 0;
        self.path_down_release_armed = false;
        self.path_held_btn = None;
        self.state = BotState::Idle;
        self.last_ori = 0xff;
        self.prev_ori = 0xff;
        self.planned_path.clear();
        self.move_path.clear();
        self.path_step = 0;
        self.path_expected_col = 3;
        self.path_expected_rot = 0;
        self.path_commit_row = 0;
        self.path_commit_col = 3;
        self.path_commit_rot = 0;
        self.path_pending_action = None;
        self.spin_emu_replan_at_step = None;
        self.post_rot_sync = false;
        self.last_valid_snap = None;
        self.clear_lock_verify_state();
        self.path_trace.clear();
        self.replan_attempts = 0;
    }

    /// Inform the bot that we just restored a full emulator savestate corresponding
    /// to a captured misdrop replay. The bot must clear its planning state and
    /// suppress misdrop counting/persistence for the upcoming (replayed) piece.
    /// This avoids spurious "new" misdrops caused purely by stale internal targets/paths.
    pub fn begin_replay_restore(&mut self) {
        self.clear_planning_state_for_restore();
        self.replay_restore_suppress = true;
        self.replay_want_lock = None;
        self.replay_want_mtype = None;
        self.state_restore_replan = false;
    }

    /// Misdrop replay: restore savestate and plan the captured want lock (not fresh 2-ply).
    pub fn begin_replay_restore_with_want(
        &mut self,
        row: i32,
        col: usize,
        rot: usize,
        mtype: Option<&str>,
    ) {
        self.begin_replay_restore();
        self.replay_want_lock = Some((row, col, rot));
        self.replay_want_mtype = mtype.map(str::to_string);
    }

    /// Generic emulator savestate restore (load state, page reload). Clears all
    /// cached paths/targets and forces a fresh BFS plan on the restored piece.
    pub fn begin_state_restore(&mut self) {
        self.clear_planning_state_for_restore();
        self.replay_restore_suppress = false;
        self.state_restore_replan = true;
    }

    /// Debug helpers for studying misdrop replays.
    /// Returns the remaining move path (shrinks as execution advances).
    pub fn debug_get_move_path(&self) -> Vec<String> {
        self.move_path.clone()
    }

    /// Full BFS path as planned at lock time (stable for the whole piece).
    pub fn debug_get_planned_path(&self) -> Vec<String> {
        self.planned_path.clone()
    }

    /// Landing type from BFS `intended_lock`: "normal", "tuck", or "spin".
    pub fn debug_get_landing_type(&self) -> String {
        match self.intended_lock.as_ref() {
            Some((_, _, _, mtype)) => mtype.clone(),
            None => "no-plan".to_string(),
        }
    }

    /// Classify the *intention* for the current planned path using the rule:
    /// Look at the last non-drop movement before hard drop.
    /// - If last movement is rotation (CW/CCW): "spin"
    /// - If last movement is side (L/R): "tuck"
    /// - Otherwise: "normal" or "unknown"
    pub fn debug_classify_intention(&self) -> String {
        let path = &self.move_path;
        if path.is_empty() {
            return "no-plan".to_string();
        }
        // Find the last action that is a movement (not "D")
        match path_terminal_mtype(path) {
            "spin" => "spin".to_string(),
            "tuck" => "tuck".to_string(),
            "normal" => "normal".to_string(),
            _ => "other".to_string(),
        }
    }

    pub fn debug_get_target(&self) -> (usize, usize, usize, String, String) {
        let (piece, next) = self.placement_piece_labels();
        (self.target_left, self.target_rot, self.path_step, piece, next)
    }

    pub fn debug_get_pending_action(&self) -> Option<String> {
        self.path_pending_action.clone()
    }

    pub fn debug_path_flags(&self) -> (bool, bool, u32) {
        (
            self.holding_down,
            self.path_down_release_armed,
            self.path_down_min_frames,
        )
    }

    pub fn debug_take_path_trace(&mut self) -> String {
        self.path_trace.join("\n")
    }

    pub(super) fn trace_path(&mut self, msg: impl std::fmt::Display) {
        if !matches!(self.state, BotState::Path) {
            return;
        }
        let line = msg.to_string();
        log::warn!("[path] {}", line);
        if self.path_trace.len() >= 80 {
            self.path_trace.remove(0);
        }
        self.path_trace.push(line);
    }

    pub fn action(&self) -> &str {
        if self.garbage_notif_frames > 0 {
            if let Some(ref notif) = self.garbage_notification {
                return notif;
            }
        }
        &self.status_msg
    }

    /// Feed garbage lines from link-cable height-drop detection.
    /// Sets notification and triggers replan if currently in Path state.
    pub fn add_garbage_lines(&mut self, added_lines: u32) {
        self.garbage_notification = Some(format!("garbage {} lines sent", added_lines));
        self.garbage_notif_frames = 30;
        if self.state == BotState::Path {
            self.trace_path("garbage arrived mid-path — transitioning to Idle for replan");
            self.move_path.clear();
            self.path_trace.clear();
            self.state = BotState::Idle;
        }
    }
    pub fn mode(&self) -> &str { &self.meat_mode }
    pub fn misdrop_stats(&self) -> (u32, u32) { (self.misdrop_count, self.total_drops) }
    pub fn bot_state(&self) -> BotState { self.state }

    /// Returns the last captured placement state (board + current piece + next piece)
    /// if we have one (updated on every piece spawn).
    pub fn last_placement(&self) -> Option<&PlacementReplay> {
        self.last_placement.as_ref()
    }


    /// Take the pending misdrop metadata JSON (cur/next + misdrop info) and clear.
    /// Returns empty string when no new misdrop. Host pairs with full savestate.
    pub fn take_pending_replay_json(&mut self) -> String {
        self.pending_replay.take()
            .and_then(|r| r.to_json().ok())
            .unwrap_or_default()
    }

    /// True while JS must not overwrite plan-time spawn pairing.
    ///
    /// Freeze during active execution (Path / Rotating / Translating / Dropping),
    /// lock verify, and undrained misdrop replay so mid-path high rotations and
    /// post-lock next-piece spawn cannot replace the plan-time board.
    ///
    /// **Do not** gate on `intended_lock.is_some()`: that field is set at plan and
    /// kept until the *next* plan overwrites it, so it is still Some on every
    /// subsequent piece's spawn frame (capture runs *before* tick/plan). Freezing
    /// on it made only the first piece of a game ever enter the spawn ring —
    /// later misdrops reattached the empty early-game savestate (2026-07-18
    /// z_tuck regression).
    pub fn has_pending_misdrop_pairing(&self) -> bool {
        self.pending_replay.is_some()
            || self.pending_lock_verify
            || matches!(
                self.state,
                BotState::Path | BotState::Rotating | BotState::Translating | BotState::Dropping
            )
            || (!self.move_path.is_empty() && self.path_step < self.move_path.len())
    }

    fn capture_last_placement(
        &mut self,
        read: &impl Fn(u16) -> u8,
        cur_ori: u8,
    ) {
        if ori_info(cur_ori).is_none() {
            return;
        }

        let next_ori = read(ADDR_NEXT_ORI);

        let cur_info = ori_info(cur_ori).unwrap();
        let next_info = ori_info(next_ori).unwrap_or((0, 0));

        let current_piece = PieceInfo {
            piece_type: cur_info.0,
            rot: cur_info.1 as usize,
            spawn_col: piece_left_col(|a| read(a)),
        };

        let next_piece = NextPiece {
            piece_type: next_info.0,
            ori: next_ori,
        };

        log::info!(
            "[replay] captured spawn placement — current type={} rot={} col={}, next type={} ori=0x{:02x}",
            current_piece.piece_type,
            current_piece.rot,
            current_piece.spawn_col,
            next_piece.piece_type,
            next_piece.ori
        );

        let replay = PlacementReplay {
            version: "1".to_string(),
            timestamp: format!("f{}", self.frame_count),
            source: None,
            current_piece,
            next_piece,
            misdrop: None,
            strategy: Some(self.meat_mode.clone()),
            mode: None,
            pps: Some(if self.pps_limit.is_infinite() { "inf".to_string() } else { self.pps_limit.to_string() }),
            note: String::new(),
        };

        self.last_placement = Some(replay);
    }

    pub fn tick(&mut self, read_mem: impl Fn(u16) -> u8, read_range: impl Fn(u16, u16) -> Vec<u8>) -> (GameState, Vec<(u8, bool)>) {
        let mut actions: Vec<(u8, bool)> = Vec::new();
        if let Some(btn) = self.pending_release.take() {
            actions.push((btn, false));
        }
        // Decrement garbage notification counter, clear when expired.
        if self.garbage_notif_frames > 0 {
            self.garbage_notif_frames -= 1;
            if self.garbage_notif_frames == 0 {
                self.garbage_notification = None;
            }
        }
        let gs = detect_game_state(&read_mem);
        if gs == GameState::Paused || gs == GameState::Win || gs.is_vs_result() {
            self.status_msg = match gs {
                GameState::Win => "type-b win (idle)".to_string(),
                GameState::VsRoundWin => "2p round win (idle)".to_string(),
                GameState::VsRoundLoss => "2p round loss (idle)".to_string(),
                GameState::VsMatchWin => "2p match win (idle)".to_string(),
                GameState::VsMatchLoss => "2p match loss (idle)".to_string(),
                _ => "paused".to_string(),
            };
            self.tick_path_button_holds(&mut actions);
            return (gs, actions);
        }
        if gs != GameState::InGame {
            self.handle_nav(&read_mem, &mut actions, gs);
            self.tick_path_button_holds(&mut actions);
            return (gs, actions);
        }
        // Advance bot frame clock every in-game frame (including ARE / frame_delay).
        // PPS throttle: elapsed = frame_count - last_drop_frame vs 60/pps.
        // This line was accidentally removed during garbage work; without it
        // last_drop_frame stayed 0 and the PPS gate never engaged.
        self.frame_count = self.frame_count.wrapping_add(1);
        if (read_mem(ADDR_C204) & 0x80) == 0 {
            self.tick_path_button_holds(&mut actions);
            return (gs, actions);
        }
        // Garbage tile detection: runs every frame, even during frame_delay.
        // Debounce: garbage tiles are written to WRAM gradually over several frames,
        // so we wait for the count to stabilise before logging / triggering replan.
        //
        // Baseline: the first non-zero snapshot is treated as pre-existing
        // (Type-B start height, round setup) — not as "lines just sent".
        // Only subsequent *increases* after that baseline count as attacks.
        let garbage_count = count_garbage_tiles(|s, l| read_range(s, l));
        if garbage_count > self.prev_garbage_count {
            self.garbage_peak = self.garbage_peak.max(garbage_count);
            self.garbage_debounce = 6;
        }
        self.prev_garbage_count = garbage_count;
        if self.garbage_debounce > 0 {
            self.garbage_debounce -= 1;
            if self.garbage_debounce == 0 && self.garbage_peak > self.garbage_last_reported {
                let added = self.garbage_peak - self.garbage_last_reported;
                let added_lines = added / 9u32;
                // First observation only seeds the baseline (start-of-round fill).
                if self.garbage_last_reported == 0 && self.garbage_peak > 0 {
                    log::warn!(
                        "[bot] garbage baseline set: {} tile(s) ({} line(s)) — not an attack",
                        self.garbage_peak, self.garbage_peak / 9u32
                    );
                    self.garbage_last_reported = self.garbage_peak;
                    self.garbage_peak = 0;
                } else if added_lines > 0 {
                    let total_lines = self.garbage_peak / 9u32;
                    log::warn!(
                        "[bot] garbage detected: {} new line(s) (total={}, state={:?})",
                        added_lines, total_lines, self.state
                    );
                    // WRAM on the bot board = lines *received* from the human.
                    self.garbage_notification = Some(format!(
                        "garbage {} lines received", added_lines
                    ));
                    self.garbage_notif_frames = 30;
                    if self.state == BotState::Path {
                        self.trace_path("garbage arrived mid-path — transitioning to Idle for replan");
                        self.move_path.clear();
                        self.path_trace.clear();
                        self.state = BotState::Idle;
                    }
                    self.garbage_last_reported = self.garbage_peak;
                    self.garbage_peak = 0;
                } else {
                    // Partial line (< 9 tiles) — keep waiting for more tiles.
                    self.garbage_last_reported = self.garbage_peak;
                    self.garbage_peak = 0;
                }
            }
        }
        if self.frame_delay > 0 {
            self.frame_delay -= 1;
            self.tick_path_button_holds(&mut actions);
            return (gs, actions);
        }
        let ori = read_mem(ADDR_CUR_ORI);
        match self.state {
            BotState::Idle => self.handle_idle(&read_mem, &read_range, &mut actions, ori),
            BotState::Rotating => self.handle_rotating(&read_mem, &read_range, &mut actions, ori),
            BotState::Translating => self.handle_translating(&read_mem, &read_range, &mut actions, ori),
            BotState::Dropping => self.handle_dropping(&read_mem, &read_range, ori, &mut actions),
            BotState::Path => self.handle_path(&read_mem, &read_range, &mut actions, ori),
        }
        // After handle_path so Down releases from D-confirm are not overridden by holds.
        self.tick_path_button_holds(&mut actions);
        (gs, actions)
    }

    fn handle_nav(&mut self, read_mem: &impl Fn(u16)->u8, actions: &mut Vec<(u8,bool)>, gs: GameState) {
        if !self.auto_menu_nav {
            self.state = BotState::Idle;
            self.last_ori = 0xff;
            self.frame_delay = 0;
            self.status_msg = match gs {
                GameState::Splash => "splash (manual)".to_string(),
                GameState::Title => "title (manual)".to_string(),
                GameState::SubmenuGametype => "game type (manual)".to_string(),
                GameState::SubmenuLevel => "level select (manual)".to_string(),
                GameState::GameOver => "game over (manual)".to_string(),
                _ => "menu (manual)".to_string(),
            };
            return;
        }

        // Extra guard: if the in-game flag is set, don't send Start even if high-level gs says otherwise.
        // This prevents accidental pauses during transitions/line clears/ARE when probes are flaky.
        let ingame_flag = (read_mem(PROBE_INGAME_ADDR) & PROBE_INGAME_MASK) != 0;
        if ingame_flag
            && gs != GameState::Paused
            && gs != GameState::Win
            && !gs.is_vs_result()
        {
            self.state = BotState::Idle;
            self.last_ori = 0xff;
            return;
        }

        if gs == GameState::Splash {
            self.status_msg = "navigating (splash)".to_string();
            self.state = BotState::Idle; self.last_ori = 0xff;
            if self.nav_timer > 0 { self.nav_timer -= 1; }
            if self.nav_timer == 0 { self.nav_timer = NAV_START_INTERVAL; actions.push((3,true)); self.pending_release = Some(3); self.frame_delay = 1; }
            return;
        }
        self.state = BotState::Idle; self.last_ori = 0xff; self.frame_delay = 0;
        self.status_msg = match gs { GameState::Title => "navigating (title)", GameState::SubmenuGametype => "navigating (game type)", GameState::SubmenuLevel => "navigating (level select)", GameState::GameOver => "restarting (game over)", _ => "navigating" }.to_string();
        if self.nav_timer > 0 { self.nav_timer -= 1; }
        if self.nav_timer == 0 { self.nav_timer = NAV_START_INTERVAL; actions.push((3,true)); self.pending_release = Some(3); self.frame_delay = 1; }
    }

    fn handle_idle(&mut self, read: &impl Fn(u16)->u8, read_r: &impl Fn(u16,u16)->Vec<u8>, actions: &mut Vec<(u8,bool)>, ori: u8) {
        self.status_msg = "idle".to_string();
        // Restored savestate: plan immediately (skip ori debounce + stale path resume).
        if (self.replay_restore_suppress || self.state_restore_replan)
            && self.last_placement.is_none()
            && ori_info(ori).is_some()
        {
            self.last_ori = ori;
            self.prev_ori = ori;
            self.replan_attempts = 0;
            self.capture_last_placement(read, ori);
            self.plan(read, read_r, ori, actions);
            return;
        }
        if ori != self.last_ori {
            if ori != self.prev_ori { self.prev_ori = ori; return; }
            self.prev_ori = ori;
            if ori_info(ori).is_none() {
                self.last_ori = ori;
                return;
            }
            // WRAM ori glitch aborted path → same piece still active: resume, never replan
            // from a gravity-advanced row (S→L: 22-step plan became 15-step suffix).
            if !self.move_path.is_empty() && self.path_step < self.move_path.len() {
                if let (Some(info), Some(lp)) = (ori_info(ori), self.last_placement.as_ref()) {
                    if info.0 == lp.current_piece.piece_type {
                        self.last_ori = ori;
                        self.state = BotState::Path;
                        self.holding_down = false;
                        self.path_down_min_frames = 0;
                        self.path_down_release_armed = false;
                        actions.push((5, false));
                        let type_idx = info.0;
                        let actual_rot = info.1 as usize;
                        let raw_row = piece_min_row(read);
                        let actual_col = piece_left_col(read);
                        let mut actual_row = raw_row;
                        if raw_row >= BOARD_ROWS as i32 {
                            if let Some((sr, sc, _)) = self.last_valid_snap {
                                if piece_pos_trustworthy(sr, sc) {
                                    actual_row = sr;
                                }
                            }
                        }
                        let bb = read_board_bitboard(read_r);
                        self.recover_stalled_path_pending(
                            actions,
                            &bb,
                            type_idx,
                            actual_row,
                            actual_col,
                            actual_rot,
                            "idle glitch",
                        );
                        self.trace_path(format!(
                            "resume path step {}/{} after idle glitch @({},{},r{})",
                            self.path_step + 1,
                            self.move_path.len(),
                            actual_row,
                            actual_col,
                            actual_rot
                        ));
                        return;
                    }
                }
            }
            if self.pps_limit == 0.0 {
                self.status_msg = "idle (paused)".to_string();
                return;
            }
            if self.pps_limit.is_finite() && self.last_drop_frame > 0 {
                let el = self.frame_count.wrapping_sub(self.last_drop_frame);
                let minf = if self.pps_limit > 0.0 {
                    (60.0 / self.pps_limit).ceil() as u32
                } else {
                    u32::MAX
                };
                if el < minf {
                    self.status_msg = format!("idle (pps {:.1})", self.pps_limit);
                    return;
                }
            }
            self.last_ori = ori; self.replan_attempts = 0;

            // Capture spawn metadata (piece types) for misdrop replay labeling.
            // Full state is captured on the JS side via save_state at spawn Y.
            self.capture_last_placement(read, ori);
            // Plan immediately (same frame as JS bot) — no extra plan_pending frame.
            self.plan(read, read_r, ori, actions);
        }
    }




    fn handle_rotating(&mut self, read: &impl Fn(u16)->u8, read_r: &impl Fn(u16,u16)->Vec<u8>, actions: &mut Vec<(u8,bool)>, ori: u8) {
        self.status_msg = "rotating".to_string();
        let info = ori_info(ori); let linfo = ori_info(self.last_ori);
        if info.is_none() || linfo.is_none() || info.unwrap().0 != linfo.unwrap().0 {
            self.last_ori = 0xff; self.prev_ori = 0xff; self.state = BotState::Idle; return;
        }
        if info.unwrap().1 as usize == self.target_rot { self.state = BotState::Translating; self.status_msg = "translating".to_string(); return; }
        if self.rot_attempts >= MAX_ROT_ATTEMPTS {
            let cr = info.unwrap().1 as usize;
            if let Some((_,l)) = find_best_move(|a|read(a),|b,ll|read_r(b,ll), None, cr as i32, default_evaluate) { self.target_left = l; } else { self.target_left = piece_left_col(|a|read(a)); }
            self.target_rot = cr; self.trans_attempts = 0; self.state = BotState::Translating; self.status_msg = "translating".to_string(); return;
        }
        actions.push((0, true));
        self.pending_release = Some(0);
        self.frame_delay = FRAME_DELAY + self.input_delay;
        self.rot_attempts += 1;
    }

    fn handle_translating(&mut self, read: &impl Fn(u16)->u8, read_r: &impl Fn(u16,u16)->Vec<u8>, actions: &mut Vec<(u8,bool)>, ori: u8) {
        self.status_msg = "translating".to_string();
        let info = ori_info(ori); let linfo = ori_info(self.last_ori);
        let rmis = info.map(|i| i.1 as usize != self.target_rot).unwrap_or(false);
        if info.is_none() || linfo.is_none() || info.unwrap().0 != linfo.unwrap().0 || rmis {
            self.last_ori = 0xff; self.prev_ori = 0xff; self.state = BotState::Idle; return;
        }
        let cl = piece_left_col(|a| read(a));
        let dx = self.target_left as i32 - cl as i32;
        if dx == 0 { self.begin_drop(read, read_r, ori, actions); return; }
        if self.trans_attempts >= MAX_TRANS_ATTEMPTS {
            let cr = info.map(|i| i.1 as usize).unwrap_or(self.target_rot);
            if let Some((_,l)) = find_best_move(|a|read(a),|b,ll|read_r(b,ll), Some(cl), cr as i32, default_evaluate) {
                if self.replan_attempts >= 2 { self.replan_attempts=0; self.target_rot=cr; self.target_left=cl; self.begin_drop(read, read_r, ori, actions); } else { self.replan_attempts+=1; self.target_left=l; self.trans_attempts=0; }
            } else { self.target_rot=cr; self.target_left=cl; self.begin_drop(read, read_r, ori, actions); }
            return;
        }
        let btn = if dx < 0 { 6 } else { 7 };
        actions.push((btn, true)); self.pending_release = Some(btn); self.frame_delay = FRAME_DELAY + self.input_delay; self.trans_attempts += 1;
    }



    // keep old nav helpers for transitional calls in wrapper (until full migration)
    pub fn tick_for_nav(&mut self, read_mem: impl Fn(u16) -> u8) -> GameState {
        if self.frame_delay > 0 { self.frame_delay -= 1; return detect_game_state(read_mem); }
        let state = detect_game_state(read_mem);
        if state == GameState::InGame || state == GameState::Paused || state == GameState::Win { return state; }
        if self.nav_timer > 0 { self.nav_timer -= 1; }
        state
    }
    pub fn should_tap_start(&self) -> bool { self.nav_timer == 0 }
    pub fn arm_start_tap(&mut self) { self.nav_timer = 40; self.frame_delay = 1; }
    pub fn take_pending_release(&mut self) -> Option<u8> { self.pending_release.take() }
    pub fn set_pending_release(&mut self, btn: u8) { self.pending_release = Some(btn); }
}

#[cfg(test)]
mod tests;
