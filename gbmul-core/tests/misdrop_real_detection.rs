//! Dual-lock RGB fixtures: sim (planner reaches want) + emu (executor locks want).
//! Only fixtures with both halves live here.

#![cfg(feature = "emulator_tests")]

use base64::Engine;
use gbmul_core::bot::fixtures::{emulator_from_savestate, misdrop_fixture};
use gbmul_core::bot::{
    is_occupied, ori_info, piece_collides, piece_left_col, piece_min_row, piece_pos_trustworthy,
    ADDR_CUR_ORI, ADDR_NEXT_ORI, BOARD_BASE, BOARD_COLS, BOARD_ROWS, BOARD_STRIDE, BotState,
    PIECE_NAMES, TetrisBot,
};
use gbmul_core::emulator::joypad::GbButton;
use gbmul_core::state::EmulatorState;

fn apply(actions: &[(u8, bool)], emu: &mut gbmul_core::emulator::Emulator) {
    for &(btn, down) in actions {
        let b = match btn {
            0 => GbButton::A,
            1 => GbButton::B,
            4 => GbButton::Up,
            5 => GbButton::Down,
            6 => GbButton::Left,
            7 => GbButton::Right,
            _ => continue,
        };
        if down {
            emu.joypad.press(b);
        } else {
            emu.joypad.release(b);
        }
    }
}

fn tick(emu: &mut gbmul_core::emulator::Emulator, bot: &mut TetrisBot) {
    let (_, actions) = bot.tick(
        |a| emu.memory.read(a),
        |s, l| (0..l).map(|i| emu.memory.read(s.wrapping_add(i))).collect(),
    );
    apply(&actions, emu);
    emu.run_frame();
    bot.tick_post_frame(
        |a| emu.memory.read(a),
        |s, l| (0..l).map(|i| emu.memory.read(s.wrapping_add(i))).collect(),
    );
}

fn load_savestate_b64(name: &str) -> EmulatorState {
    let path = misdrop_fixture(name);
    let b64 = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim().trim_matches('"'))
        .expect("base64 decode");
    bincode::deserialize(&bytes).expect("bincode EmulatorState")
}

fn piece_type_idx(name: &str) -> Option<usize> {
    PIECE_NAMES.iter().position(|&p| p == name)
}

fn board_from_state(state: &EmulatorState) -> [u16; gbmul_core::bot::BOARD_ROWS] {
    let mut bb = [0u16; gbmul_core::bot::BOARD_ROWS];
    for row in 0..gbmul_core::bot::BOARD_ROWS {
        let base = 0x800 + row * gbmul_core::bot::BOARD_STRIDE + 2;
        for col in 0..gbmul_core::bot::BOARD_COLS {
            if gbmul_core::bot::is_occupied(state.ram.get(base + col).copied().unwrap_or(0)) {
                bb[row] |= 1 << col;
            }
        }
    }
    bb
}

/// Run replay restore until the fixture piece locks; returns observed lock pose + misdrop count.
fn run_bot_until_lock_pose(
    b64: &str,
    piece: &str,
    next: Option<&str>,
    want: Option<(i32, usize, usize, &str)>,
) -> (i32, usize, usize, u32) {
    let state = load_savestate_b64(b64);
    let mut emu = emulator_from_savestate(&state);

    let fixture_type = piece_type_idx(piece)
        .unwrap_or_else(|| panic!("unknown piece {piece}"));
    let next_type = next.and_then(piece_type_idx).or_else(|| {
        ori_info(state.ram[ADDR_NEXT_ORI as usize]).map(|(t, _)| t)
    });

    let mut bot = TetrisBot::new();
    bot.set_pps(f64::INFINITY);
    bot.set_soft_drop_mode(true);
    if let Some((row, col, rot, mtype)) = want {
        bot.begin_replay_restore_with_want(row, col, rot, Some(mtype));
    } else {
        bot.begin_replay_restore();
    }

    let mut lock_row = -1i32;
    let mut lock_col = 0usize;
    let mut lock_rot = 0usize;
    let mut saw_path = false;
    let mut saw_fixture_piece = false;
    // Freeze pose once Dropping starts — ARE/lock-delay sprites can glitch col
    // after a correct terminal tuck (L→S r13 c3: (13,3) → false (13,2)).
    let mut pose_frozen = false;

    for frame in 0..20_000 {
        tick(&mut emu, &mut bot);

        if matches!(bot.bot_state(), BotState::Path) {
            saw_path = true;
        }

        let cur_type = ori_info(emu.memory.read(ADDR_CUR_ORI)).map(|i| i.0);
        let row = piece_min_row(|a| emu.memory.read(a));
        let col = piece_left_col(|a| emu.memory.read(a));
        let rot = ori_info(emu.memory.read(ADDR_CUR_ORI))
            .map(|i| i.1 as usize)
            .unwrap_or(99);

        if cur_type == Some(fixture_type) {
            saw_fixture_piece = true;
            // Ignore post-merge / ARE sprites that collide with the board (false row+1).
            let mut bb = [0u16; BOARD_ROWS];
            for r in 0..BOARD_ROWS {
                for c in 0..BOARD_COLS {
                    let a = BOARD_BASE + (r as u16) * BOARD_STRIDE as u16 + 2 + c as u16;
                    if is_occupied(emu.memory.read(a)) {
                        bb[r] |= 1 << c;
                    }
                }
            }
            let collides = rot < 4
                && piece_collides(&bb, fixture_type, rot, row, col as i32);
            if !pose_frozen && piece_pos_trustworthy(row, col) && !collides {
                lock_row = row;
                lock_col = col;
                lock_rot = rot;
                // Freeze when live pose matches forced want (terminal tuck reached).
                if let Some((wr, wc, wrot, _)) = want {
                    if row == wr && col == wc && rot == wrot {
                        pose_frozen = true;
                    }
                }
            }
            // Free replan: freeze first Dropping pose after Path (path vec may still
            // hold steps with path_step at end — use Path→Dropping transition).
            if !pose_frozen
                && saw_path
                && matches!(bot.bot_state(), BotState::Dropping)
                && lock_row >= 0
            {
                pose_frozen = true;
            }
        } else if saw_fixture_piece && cur_type.is_some() {
            if next_type.is_none() || cur_type == next_type {
                break;
            }
        }

        if saw_fixture_piece
            && lock_row >= 0
            && matches!(bot.bot_state(), BotState::Idle)
            && bot.debug_get_move_path().is_empty()
        {
            break;
        }

        if frame == 19_999 {
            panic!(
                "{piece}: timeout — saw_path={saw_path} lock=({lock_row},{lock_col},r{lock_rot})"
            );
        }
    }

    assert!(saw_path, "{piece}: bot must enter Path state");
    assert!(
        lock_row >= 0,
        "{piece}: must observe trustworthy lock pose"
    );
    (lock_row, lock_col, lock_rot, bot.misdrop_stats().0)
}

// ── j_spin_r16_c1_r0 ────────────────────────────────────────────────────────

#[test]
fn j_spin_r16_c1_r0_planner_reaches_want() {
    use gbmul_core::bot::{classify_move, find_bfs_path_to_lock, simulate_path_stepwise};

    let state = load_savestate_b64("misdrop_j_spin_r16_c1_r0_state.b64");
    let bb = board_from_state(&state);
    // J=6; spawn col 3. Path: D×14,L,CW,D,D,CCW,D
    let path: Vec<String> = std::iter::repeat_n("D".into(), 14)
        .chain(
            ["L", "CW", "D", "D", "CCW", "D"]
                .iter()
                .map(|s| s.to_string()),
        )
        .collect();
    assert!(
        find_bfs_path_to_lock(&bb, 6, -2, 3, 0, 16, 1, 0).is_some(),
        "BFS must reach want_lock (16,1,r0)"
    );
    assert_eq!(
        simulate_path_stepwise(&bb, 6, -2, 3, 0, &path),
        Some((16, 1, 0)),
        "recorded path must sim to want"
    );
    assert_eq!(
        classify_move(&bb, 6, 16, 1, 0, &path, 0),
        "spin",
        "want must be spin"
    );
}

#[test]
fn j_spin_r16_c1_r0_executor_reaches_want_lock() {
    let (row, col, rot, mis) = run_bot_until_lock_pose(
        "misdrop_j_spin_r16_c1_r0_state.b64",
        "J",
        Some("O"),
        Some((16, 1, 0, "spin")),
    );
    assert_eq!(
        (row, col, rot),
        (16, 1, 0),
        "J spin executor must reach want_lock"
    );
    assert_eq!(mis, 0, "must not misdrop");
}

// ── z_tuck_r13_c6_r1 ────────────────────────────────────────────────────────

#[test]
fn z_tuck_r13_c6_r1_planner_reaches_want() {
    use gbmul_core::bot::{
        classify_move, find_bfs_path_to_lock, prefer_simplest_equivalent_path, simulate_path_stepwise,
    };

    let state = load_savestate_b64("misdrop_z_tuck_r13_c6_r1_state.b64");
    let bb = board_from_state(&state);
    // Z=4; spawn col 3. After Z true-rotation fix, spawn CW steps right (col+1), so the
    // old import path CW,R,R,R,D×15,R no longer sims — BFS finds mid-descent rot instead.
    let bfs = find_bfs_path_to_lock(&bb, 4, -2, 3, 0, 13, 6, 1)
        .expect("BFS must reach want_lock (13,6,r1)");
    let path = prefer_simplest_equivalent_path(&bb, 4, -2, 3, 0, 13, 6, 1, &bfs.2);
    assert_eq!(
        simulate_path_stepwise(&bb, 4, -2, 3, 0, &path),
        Some((13, 6, 1)),
        "preferred path must sim to want: {path:?}"
    );
    assert_eq!(
        classify_move(&bb, 4, 13, 6, 1, &path, 0),
        "tuck",
        "want must be a tuck into the L-well"
    );
}

#[test]
fn z_tuck_r13_c6_r1_executor_reaches_want_lock() {
    let (row, col, rot, mis) = run_bot_until_lock_pose(
        "misdrop_z_tuck_r13_c6_r1_state.b64",
        "Z",
        Some("J"),
        Some((13, 6, 1, "tuck")),
    );
    assert_eq!(
        (row, col, rot),
        (13, 6, 1),
        "Z tuck executor must reach want_lock into L-well"
    );
    assert_eq!(mis, 0, "must not misdrop");
}

// ── j_tuck_r13_c3_r3 ────────────────────────────────────────────────────────

#[test]
fn j_tuck_r13_c3_r3_planner_reaches_want() {
    use gbmul_core::bot::{find_bfs_path_to_lock, simulate_path_stepwise};

    let state = load_savestate_b64("misdrop_j_tuck_r13_c3_r3_state.b64");
    let bb = board_from_state(&state);
    // J=6; meta spawn_col 3. Path: CCW,R,D×17,L
    let path: Vec<String> = ["CCW".into(), "R".into()]
        .into_iter()
        .chain(std::iter::repeat_n("D".into(), 17))
        .chain(std::iter::once("L".into()))
        .collect();
    assert!(
        find_bfs_path_to_lock(&bb, 6, -2, 3, 0, 13, 3, 3).is_some(),
        "BFS must reach want_lock (13,3,r3)"
    );
    assert_eq!(
        simulate_path_stepwise(&bb, 6, -2, 3, 0, &path),
        Some((13, 3, 3)),
        "recorded path must sim to want"
    );
    let without_l: Vec<String> = path[..path.len() - 1].to_vec();
    assert_eq!(
        simulate_path_stepwise(&bb, 6, -2, 3, 0, &without_l),
        Some((13, 4, 3)),
        "without final L must match browser got (13,4,r3) — terminal slide never ran"
    );
}

#[test]
fn j_tuck_r13_c3_r3_executor_reaches_want_lock() {
    let (row, col, rot, mis) = run_bot_until_lock_pose(
        "misdrop_j_tuck_r13_c3_r3_state.b64",
        "J",
        Some("Z"),
        Some((13, 3, 3, "tuck")),
    );
    assert_eq!(
        (row, col, rot),
        (13, 3, 3),
        "J tuck executor must reach want_lock (terminal L)"
    );
    assert_eq!(mis, 0, "must not misdrop");
}

// ── t_spin_r16_c5_r2 ────────────────────────────────────────────────────────

#[test]
fn t_spin_r16_c5_r2_planner_reaches_want() {
    use gbmul_core::bot::{bfs_moves, find_bfs_path_to_lock, simulate_path_stepwise};

    let state = load_savestate_b64("misdrop_t_spin_r16_c5_r2_state.b64");
    let bb = board_from_state(&state);
    let path: Vec<String> = std::iter::repeat_n("D".into(), 15)
        .chain(["R", "R", "CCW", "D", "D", "D", "L", "CCW"].iter().map(|s| s.to_string()))
        .collect();
    let spawn_col = 3usize; // meta spawn_col; manifest spawn fixed to match
    assert!(
        find_bfs_path_to_lock(&bb, 2, -2, spawn_col, 0, 16, 5, 2).is_some(),
        "BFS must reach want_lock (16,5,r2)"
    );
    assert_eq!(
        simulate_path_stepwise(&bb, 2, -2, spawn_col as i32, 0, &path),
        Some((16, 5, 2)),
        "recorded path must sim to want under floor SRS"
    );
    let moves = bfs_moves(&bb, 2, -2, spawn_col, 0);
    assert!(
        moves.iter().any(|m| m.row == 16 && m.col == 5 && m.rot == 2),
        "bfs_moves must list (16,5,r2)"
    );
}

#[test]
fn t_spin_r16_c5_r2_executor_reaches_want_lock() {
    let (row, col, rot, mis) = run_bot_until_lock_pose(
        "misdrop_t_spin_r16_c5_r2_state.b64",
        "T",
        Some("O"),
        Some((16, 5, 2, "spin")),
    );
    assert_eq!(
        (row, col, rot),
        (16, 5, 2),
        "T spin executor must reach want_lock"
    );
    assert_eq!(mis, 0, "must not misdrop");
}

// ── t_tuck_r12_c7_r1_20260718-093702 ────────────────────────────────────────

/// Plan reaches want; prefer_simplest must not rewrite to early-R path that overshoots.
#[test]
fn t_tuck_r12_c7_r1_planner_reaches_want() {
    use gbmul_core::bot::{
        classify_move, find_bfs_path_to_lock, prefer_simplest_equivalent_path, simulate_path_stepwise,
    };

    let state = load_savestate_b64("misdrop_t_tuck_r12_c7_r1_20260718-093702_state.b64");
    let bb = board_from_state(&state);
    // T=2; meta spawn col 4 rot 1 (vertical). Live min-row is -2 on this savestate.
    let spawn_row = -2i32;
    let spawn_col = 4i32;
    let spawn_rot = 1usize;
    let bfs = find_bfs_path_to_lock(&bb, 2, spawn_row, spawn_col as usize, spawn_rot, 12, 7, 1)
        .expect("BFS must reach want_lock (12,7,r1)");
    let path = prefer_simplest_equivalent_path(
        &bb, 2, spawn_row, spawn_col, spawn_rot, 12, 7, 1, &bfs.2,
    );
    assert_eq!(
        simulate_path_stepwise(&bb, 2, spawn_row, spawn_col, spawn_rot, &path),
        Some((12, 7, 1)),
        "preferred path must sim to want"
    );
    // Do not shift all laterals before descent — soft-drop overshoots col7 well.
    let first_d = path.iter().position(|a| a == "D");
    let first_lat = path.iter().position(|a| a == "L" || a == "R");
    let term = path.last().map(|s| s.as_str());
    assert!(
        matches!(term, Some("R") | Some("L")),
        "tuck must end with terminal slide: {path:?}"
    );
    assert!(
        first_d.is_some_and(|di| first_lat.is_none_or(|li| di <= li)),
        "prefer D-before-setup-lateral for terminal tuck (got {path:?})"
    );
    assert_eq!(
        classify_move(&bb, 2, 12, 7, 1, &path, 0),
        "tuck",
        "want must be tuck"
    );
}

/// RED: executor locked (9,8,r1) — early-R path + soft-drop overshot tuck well col7.
#[test]
fn t_tuck_r12_c7_r1_executor_reaches_want_lock() {
    let (row, col, rot, mis) = run_bot_until_lock_pose(
        "misdrop_t_tuck_r12_c7_r1_20260718-093702_state.b64",
        "T",
        Some("Z"),
        Some((12, 7, 1, "tuck")),
    );
    assert_eq!(
        (row, col, rot),
        (12, 7, 1),
        "T tuck executor must reach want_lock into well"
    );
    assert_eq!(mis, 0, "must not misdrop");
}

// ── i_tuck_r17_c4_r0_20260718-102732 ─────────────────────────────────────────

/// Plan reaches want (17,4,r0) via late R R D D L tuck under the shelf.
#[test]
fn i_tuck_r17_c4_r0_planner_reaches_want() {
    use gbmul_core::bot::{
        classify_move, find_bfs_path_to_lock, prefer_simplest_equivalent_path, simulate_path_stepwise,
    };

    let state = load_savestate_b64("misdrop_i_tuck_r17_c4_r0_20260718-102732_state.b64");
    let bb = board_from_state(&state);
    // I=0; live savestate is rot0 at row -1 col 3
    let spawn_row = -1i32;
    let spawn_col = 3i32;
    let spawn_rot = 0usize;
    let bfs = find_bfs_path_to_lock(&bb, 0, spawn_row, spawn_col as usize, spawn_rot, 17, 4, 0)
        .expect("BFS must reach want_lock (17,4,r0)");
    let path = prefer_simplest_equivalent_path(
        &bb, 0, spawn_row, spawn_col, spawn_rot, 17, 4, 0, &bfs.2,
    );
    assert_eq!(
        simulate_path_stepwise(&bb, 0, spawn_row, spawn_col, spawn_rot, &path),
        Some((17, 4, 0)),
        "preferred path must sim to want"
    );
    let term = path.last().map(|s| s.as_str());
    assert!(
        matches!(term, Some("R") | Some("L")),
        "tuck must end with terminal slide: {path:?}"
    );
    assert_eq!(
        classify_move(&bb, 0, 17, 4, 0, &path, 0),
        "tuck",
        "want must be tuck"
    );
}

/// RED: live locked (17,6,r0) — RR setup + soft-drop overshot; terminal L missed.
#[test]
fn i_tuck_r17_c4_r0_executor_reaches_want_lock() {
    let (row, col, rot, mis) = run_bot_until_lock_pose(
        "misdrop_i_tuck_r17_c4_r0_20260718-102732_state.b64",
        "I",
        Some("J"),
        Some((17, 4, 0, "tuck")),
    );
    assert_eq!(
        (row, col, rot),
        (17, 4, 0),
        "I tuck executor must reach want_lock col4 (not overshoot col6)"
    );
    assert_eq!(mis, 0, "must not misdrop");
}

/// Free replan (no forced want) — same dual-lock gate as live browser restore.
#[test]
fn i_tuck_r17_c4_r0_executor_free_replan_reaches_want_lock() {
    let (row, col, rot, mis) = run_bot_until_lock_pose(
        "misdrop_i_tuck_r17_c4_r0_20260718-102732_state.b64",
        "I",
        Some("J"),
        None,
    );
    assert_eq!(
        (row, col, rot),
        (17, 4, 0),
        "free replan must still lock want (17,4,r0)"
    );
    assert_eq!(mis, 0, "must not misdrop");
}

// ── z_spin_r15_c1_r1_20260718-103523 ─────────────────────────────────────────
// Planner fiction: BFS offered D×18+CW → (15,1,r1). Manual joypad CW at (16,3)
// lands (15,3,r1) on Rosy — diagonal kick is sim-only (same class as older
// misdrop_z_spin_r15_c1_r1 floor policy). Not an executor dual-lock case.

/// RED: want (15,1,r1) must not be BFS-reachable after floor SRS matches hardware.
#[test]
fn z_spin_r15_c1_r1_20260718_planner_rejects_fiction_want() {
    use gbmul_core::bot::{bfs_moves, find_bfs_path_to_lock};

    let state = load_savestate_b64("misdrop_z_spin_r15_c1_r1_20260718-103523_state.b64");
    let bb = board_from_state(&state);
    let spawn_row = -2i32;
    let spawn_col = 3usize;
    let moves = bfs_moves(&bb, 4, spawn_row, spawn_col, 0);
    assert!(
        !moves
            .iter()
            .any(|m| m.row == 15 && m.col == 1 && m.rot == 1),
        "BFS must not offer fiction Z spin (15,1,r1); hardware CW@ (16,3) → (15,3,r1)"
    );
    assert!(
        find_bfs_path_to_lock(&bb, 4, spawn_row, spawn_col, 0, 15, 1, 1).is_none(),
        "find_bfs_path_to_lock must reject fiction want"
    );
}

/// Free replan picks a real placement (often normal, not Path) — no fiction want.
#[test]
fn z_spin_r15_c1_r1_20260718_executor_free_replan_no_fiction_misdrop() {
    // Best move after floor fix is typically normal (Rotating/Translating), not Path —
    // so do not use run_bot_until_lock_pose's saw_path assert.
    let state = load_savestate_b64("misdrop_z_spin_r15_c1_r1_20260718-103523_state.b64");
    let mut emu = emulator_from_savestate(&state);
    let mut bot = TetrisBot::new();
    bot.set_pps(f64::INFINITY);
    bot.set_soft_drop_mode(true);
    bot.begin_replay_restore();

    let mut lock_row = -1i32;
    let mut lock_col = 0usize;
    let mut lock_rot = 0usize;
    let mut saw_z = false;
    for frame in 0..20_000 {
        tick(&mut emu, &mut bot);
        let cur = ori_info(emu.memory.read(ADDR_CUR_ORI)).map(|i| i.0);
        let row = piece_min_row(|a| emu.memory.read(a));
        let col = piece_left_col(|a| emu.memory.read(a));
        let rot = ori_info(emu.memory.read(ADDR_CUR_ORI))
            .map(|i| i.1 as usize)
            .unwrap_or(99);
        if cur == Some(4) {
            saw_z = true;
            if piece_pos_trustworthy(row, col) {
                let mut bb = [0u16; BOARD_ROWS];
                for r in 0..BOARD_ROWS {
                    for c in 0..BOARD_COLS {
                        let a = BOARD_BASE + (r as u16) * BOARD_STRIDE as u16 + 2 + c as u16;
                        if is_occupied(emu.memory.read(a)) {
                            bb[r] |= 1 << c;
                        }
                    }
                }
                if rot < 4 && !piece_collides(&bb, 4, rot, row, col as i32) {
                    lock_row = row;
                    lock_col = col;
                    lock_rot = rot;
                }
            }
        } else if saw_z && cur.is_some() {
            break;
        }
        if frame == 19_999 {
            panic!("timeout lock=({lock_row},{lock_col},r{lock_rot})");
        }
    }
    assert!(lock_row >= 0, "must lock Z");
    assert_ne!(
        (lock_row, lock_col, lock_rot),
        (15, 1, 1),
        "must not lock fiction want (15,1,r1)"
    );
    assert_eq!(
        bot.misdrop_stats().0,
        0,
        "free replan must not misdrop after floor SRS fix; lock=({lock_row},{lock_col},r{lock_rot})"
    );
}

// ── l_tuck_r13_c3_r1_20260718-105850 ─────────────────────────────────────────

/// Plan reaches want (13,3,r1) via CW,L,D…,R terminal tuck.
#[test]
fn l_tuck_r13_c3_r1_planner_reaches_want() {
    use gbmul_core::bot::{
        classify_move, find_bfs_path_to_lock, prefer_simplest_equivalent_path, simulate_path_stepwise,
    };

    let state = load_savestate_b64("misdrop_l_tuck_r13_c3_r1_20260718-105850_state.b64");
    let bb = board_from_state(&state);
    // L=5; live spawn rot0 row -2 col 3
    let spawn_row = -2i32;
    let spawn_col = 3i32;
    let spawn_rot = 0usize;
    let bfs = find_bfs_path_to_lock(&bb, 5, spawn_row, spawn_col as usize, spawn_rot, 13, 3, 1)
        .expect("BFS must reach want_lock (13,3,r1)");
    let path = prefer_simplest_equivalent_path(
        &bb, 5, spawn_row, spawn_col, spawn_rot, 13, 3, 1, &bfs.2,
    );
    assert_eq!(
        simulate_path_stepwise(&bb, 5, spawn_row, spawn_col, spawn_rot, &path),
        Some((13, 3, 1)),
        "preferred path must sim to want"
    );
    assert!(
        matches!(path.last().map(|s| s.as_str()), Some("R") | Some("L")),
        "tuck must end with terminal slide: {path:?}"
    );
    // Mid-descent CW (after Ds), not spawn-height CW — hardware kicks differ.
    let first_d = path.iter().position(|a| a == "D");
    let first_rot = path.iter().position(|a| a == "CW" || a == "CCW");
    assert!(
        first_d.is_some_and(|di| first_rot.is_some_and(|ri| di < ri)),
        "keep BFS mid-descent rot (not spawn CW rewrite): {path:?}"
    );
    assert_eq!(
        classify_move(&bb, 5, 13, 3, 1, &path, 0),
        "tuck",
        "want must be tuck"
    );
}

/// RED: browser reported got (11,3,r1) vs want (13,3,r1) — row short on terminal tuck.
#[test]
fn l_tuck_r13_c3_r1_executor_reaches_want_lock() {
    let (row, col, rot, mis) = run_bot_until_lock_pose(
        "misdrop_l_tuck_r13_c3_r1_20260718-105850_state.b64",
        "L",
        Some("S"),
        Some((13, 3, 1, "tuck")),
    );
    assert_eq!(
        (row, col, rot),
        (13, 3, 1),
        "L tuck executor must reach want_lock (not short row 11)"
    );
    assert_eq!(mis, 0, "must not misdrop");
}

#[test]
fn l_tuck_r13_c3_r1_executor_free_replan_reaches_want_lock() {
    let (row, col, rot, mis) = run_bot_until_lock_pose(
        "misdrop_l_tuck_r13_c3_r1_20260718-105850_state.b64",
        "L",
        Some("S"),
        None,
    );
    assert_eq!(
        (row, col, rot),
        (13, 3, 1),
        "free replan must lock want (13,3,r1)"
    );
    assert_eq!(mis, 0, "must not misdrop");
}

// ── l_tuck_r15_c4_r1_20260718-112848 ─────────────────────────────────────────

#[test]
fn l_tuck_r15_c4_r1_planner_reaches_want() {
    use gbmul_core::bot::{
        classify_move, find_bfs_path_to_lock, prefer_simplest_equivalent_path, simulate_path_stepwise,
    };
    let state = load_savestate_b64("misdrop_l_tuck_r15_c4_r1_20260718-112848_state.b64");
    let bb = board_from_state(&state);
    let spawn_row = -2i32;
    let spawn_col = 3i32;
    let spawn_rot = 0usize;
    let bfs = find_bfs_path_to_lock(&bb, 5, spawn_row, spawn_col as usize, spawn_rot, 15, 4, 1)
        .expect("BFS must reach want_lock (15,4,r1)");
    let path = prefer_simplest_equivalent_path(
        &bb, 5, spawn_row, spawn_col, spawn_rot, 15, 4, 1, &bfs.2,
    );
    assert_eq!(
        simulate_path_stepwise(&bb, 5, spawn_row, spawn_col, spawn_rot, &path),
        Some((15, 4, 1)),
        "preferred path must sim to want: {path:?}"
    );
    assert!(
        matches!(path.last().map(|s| s.as_str()), Some("R") | Some("L")),
        "tuck terminal slide: {path:?}"
    );
    // Must keep mid-descent CW (after Ds), not spawn CW rewrite.
    let first_d = path.iter().position(|a| a == "D");
    let first_rot = path.iter().position(|a| a == "CW" || a == "CCW");
    assert!(
        first_d.is_some_and(|di| first_rot.is_some_and(|ri| di < ri)),
        "keep BFS mid-descent rot (not spawn CW): {path:?}"
    );
    assert_eq!(classify_move(&bb, 5, 15, 4, 1, &path, 0), "tuck");
}

#[test]
fn l_tuck_r15_c4_r1_executor_reaches_want_lock() {
    let (row, col, rot, mis) = run_bot_until_lock_pose(
        "misdrop_l_tuck_r15_c4_r1_20260718-112848_state.b64",
        "L",
        Some("S"),
        Some((15, 4, 1, "tuck")),
    );
    assert_eq!((row, col, rot), (15, 4, 1), "L tuck must reach want (not short row)");
    assert_eq!(mis, 0, "must not misdrop");
}

#[test]
fn l_tuck_r15_c4_r1_executor_free_replan_reaches_want_lock() {
    let (row, col, rot, mis) = run_bot_until_lock_pose(
        "misdrop_l_tuck_r15_c4_r1_20260718-112848_state.b64",
        "L",
        Some("S"),
        None,
    );
    assert_eq!((row, col, rot), (15, 4, 1), "free replan must lock want");
    assert_eq!(mis, 0, "must not misdrop");
}

// ── z_tuck_r16_c2_r2_20260718-162308 ─────────────────────────────────────────
// Browser: want (16,2,r2) via D×15 CW…; got (13,4,r1). Root cause: Z true-rotation
// center was wrong — sim CW@(13,3,r0)→(13,2,r1), hardware →(13,4,r1).
// GREEN: SRS_CENTER[Z] fixed; free replan / forced-want no longer follow fiction.

#[test]
fn z_tuck_r16_c2_r2_recorded_path_no_longer_reaches_want() {
    use gbmul_core::bot::simulate_path_stepwise;
    let state = load_savestate_b64("misdrop_z_tuck_r16_c2_r2_20260718-162308_state.b64");
    let bb = board_from_state(&state);
    let recorded: Vec<String> = std::iter::repeat_n("D".into(), 15)
        .chain(["CW".into(), "D".into(), "CW".into(), "D".into(), "R".into()])
        .collect();
    assert_ne!(
        simulate_path_stepwise(&bb, 4, -2, 3, 0, &recorded),
        Some((16, 2, 2)),
        "fiction recorded path must not sim to want after Z center fix"
    );
}

/// Free replan (browser restore): no misdrop; must not fail as (13,4,r1) from fiction.
#[test]
fn z_tuck_r16_c2_r2_executor_free_replan_no_misdrop() {
    let (row, col, rot, mis) = run_bot_until_lock_pose(
        "misdrop_z_tuck_r16_c2_r2_20260718-162308_state.b64",
        "Z",
        Some("S"),
        None,
    );
    assert_eq!(mis, 0, "must not misdrop after Z true-rotation fix; locked ({row},{col},r{rot})");
}

/// If BFS still offers want via a hardware-legal path, executor must lock it.
#[test]
fn z_tuck_r16_c2_r2_executor_want_if_reachable() {
    use gbmul_core::bot::find_bfs_path_to_lock;
    let state = load_savestate_b64("misdrop_z_tuck_r16_c2_r2_20260718-162308_state.b64");
    let bb = board_from_state(&state);
    if find_bfs_path_to_lock(&bb, 4, -2, 3, 0, 16, 2, 2).is_none() {
        return;
    }
    let (row, col, rot, mis) = run_bot_until_lock_pose(
        "misdrop_z_tuck_r16_c2_r2_20260718-162308_state.b64",
        "Z",
        Some("S"),
        Some((16, 2, 2, "tuck")),
    );
    assert_eq!((row, col, rot), (16, 2, 2), "executor must lock want when reachable");
    assert_eq!(mis, 0, "must not misdrop");
}

// ── j_tuck_r15_c6_r3_20260718-165150 ─────────────────────────────────────────
// Browser: want (15,6,r3) via D×15 CCW CCW D CW R DD L; got (13,3,r3).
// Planner reaches want in sim; executor locks short — path_exec / sim≠emu mid-path.

#[test]
fn j_tuck_r15_c6_r3_planner_reaches_want() {
    use gbmul_core::bot::{
        classify_move, find_bfs_path_to_lock, prefer_simplest_equivalent_path, simulate_path_stepwise,
    };
    let state = load_savestate_b64("misdrop_j_tuck_r15_c6_r3_20260718-165150_state.b64");
    let bb = board_from_state(&state);
    let spawn_row = -2i32;
    let spawn_col = 3i32;
    let spawn_rot = 0usize;
    let want_row = 15i32;
    let want_col = 6i32;
    let want_rot = 3usize;
    let bfs = find_bfs_path_to_lock(
        &bb, 6, spawn_row, spawn_col as usize, spawn_rot, want_row, want_col as usize, want_rot,
    )
    .expect("BFS must reach want_lock (15,6,r3)");
    let path = prefer_simplest_equivalent_path(
        &bb, 6, spawn_row, spawn_col, spawn_rot, want_row, want_col, want_rot, &bfs.2,
    );
    assert_eq!(
        simulate_path_stepwise(&bb, 6, spawn_row, spawn_col, spawn_rot, &path),
        Some((want_row, want_col, want_rot)),
        "preferred path must sim to want: {path:?}"
    );
    assert_eq!(
        classify_move(&bb, 6, want_row, want_col, want_rot, &path, 0),
        "tuck",
        "want must be tuck"
    );
}

/// RED: browser locked (13,3,r3) vs want (15,6,r3).
#[test]
fn j_tuck_r15_c6_r3_executor_reaches_want_lock() {
    let (row, col, rot, mis) = run_bot_until_lock_pose(
        "misdrop_j_tuck_r15_c6_r3_20260718-165150_state.b64",
        "J",
        Some("Z"),
        Some((15, 6, 3, "tuck")),
    );
    assert_eq!(
        (row, col, rot),
        (15, 6, 3),
        "J tuck executor must reach want (not short (13,3,r3))"
    );
    assert_eq!(mis, 0, "must not misdrop");
}

#[test]
fn j_tuck_r15_c6_r3_executor_free_replan_reaches_want_lock() {
    let (row, col, rot, mis) = run_bot_until_lock_pose(
        "misdrop_j_tuck_r15_c6_r3_20260718-165150_state.b64",
        "J",
        Some("Z"),
        None,
    );
    assert_eq!(
        (row, col, rot),
        (15, 6, 3),
        "free replan must lock want (15,6,r3); got ({row},{col},r{rot})"
    );
    assert_eq!(mis, 0, "must not misdrop");
}



