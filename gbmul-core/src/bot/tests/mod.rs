use super::*;

#[test]
fn synth_normal_execution_path_rot_and_slide() {
    assert_eq!(
        synth_normal_execution_path(0, 1, 3, 5),
        vec!["CW", "R", "R"]
    );
    assert_eq!(synth_normal_execution_path(2, 2, 4, 2), vec!["L", "L"]);
}

#[test]
fn path_terminal_mtype_last_non_drop() {
    let spin: Vec<String> = ["D", "D", "CCW"].iter().map(|s| s.to_string()).collect();
    assert_eq!(path_terminal_mtype(&spin), "spin");

    let tuck: Vec<String> = ["D", "CW", "L", "D", "L", "L"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(path_terminal_mtype(&tuck), "tuck");

    let j_suffix: Vec<String> = ["D", "CCW", "D", "R"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(path_terminal_mtype(&j_suffix), "tuck");
}

/// cargo test j_spin_r15_c4_r3_replay -- --nocapture
#[test]
fn j_spin_r15_c4_r3_replay() {
    use crate::bot::fixtures::emulator_from_savestate;
    use base64::Engine;
    use crate::state::EmulatorState;

    let b64 = std::fs::read_to_string(
        super::fixtures::misdrop_fixture("misdrop_j_spin_r15_c4_r3_state.b64"),
    )
    .unwrap();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim().trim_matches('"'))
        .unwrap();
    let state: EmulatorState = bincode::deserialize(&bytes).unwrap();
    let mut emu = emulator_from_savestate(&state);
    let mut bot = TetrisBot::new();
    bot.set_pps(f64::INFINITY);
    bot.set_soft_drop_mode(true);
    bot.begin_replay_restore();

    for frame in 0..25_000 {
        let (_, actions) = bot.tick(
            |a| emu.memory.read(a),
            |s, l| (0..l).map(|i| emu.memory.read(s.wrapping_add(i))).collect(),
        );
        apply_bot_actions(&actions, &mut emu);
        emu.run_frame();
        bot.tick_post_frame(
            |a| emu.memory.read(a),
            |s, l| (0..l).map(|i| emu.memory.read(s.wrapping_add(i))).collect(),
        );
        let (mis, _) = bot.misdrop_stats();
        if mis > 0 {
            eprintln!(
                "frame={frame} MISDROP mis={mis} sprite=({},{},r{}) mode={}",
                piece_min_row(|a| emu.memory.read(a)),
                piece_left_col(|a| emu.memory.read(a)),
                ori_info(emu.memory.read(ADDR_CUR_ORI)).map(|i| i.1).unwrap_or(99),
                bot.mode()
            );
            eprintln!("path trace:\n{}", bot.debug_take_path_trace());
            break;
        }
        if frame > 400
            && matches!(bot.bot_state(), BotState::Idle)
            && bot.debug_get_move_path().is_empty()
        {
            eprintln!("frame={frame} lock OK mode={}", bot.mode());
            eprintln!("path trace:\n{}", bot.debug_take_path_trace());
            break;
        }
    }
}

/// cargo test j_spin_r15_c4_r3_probe -- --nocapture
#[test]
fn j_spin_r15_c4_r3_probe() {
    use base64::Engine;
    use crate::state::EmulatorState;
    use crate::bot::srs::{srs_try_rotate_auto, srs_try_rotate_detailed};

    let b64 = std::fs::read_to_string(
        super::fixtures::misdrop_fixture("misdrop_j_spin_r15_c4_r3_state.b64"),
    )
    .unwrap();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim().trim_matches('"'))
        .unwrap();
    let state: EmulatorState = bincode::deserialize(&bytes).unwrap();
    let bb = read_board_bitboard_from_ram(&state.ram);
    eprintln!("\n=== J spin board rows 11-17 ===");
    for row in 11..BOARD_ROWS {
        let bits = bb[row];
        let cols: String = (0..10)
            .map(|c| if (bits >> c) & 1 == 1 { '#' } else { '.' })
            .collect();
        eprintln!("  row {row}: {cols}");
    }

    let path: Vec<&str> = std::iter::repeat_n("D", 16)
        .chain(["CCW", "CCW", "D", "L", "CW"])
        .collect();
    let path_strings: Vec<String> = path.iter().map(|s| s.to_string()).collect();

    for (label, sr, sc) in [("spawn (-2,3,r0)", -2i32, 3i32)] {
        eprintln!("\n--- {label} ---");
        if let Some(end) = simulate_path_stepwise(&bb, 6, sr, sc, 0, &path_strings) {
            eprintln!("recorded path sim => ({},{},r{}) want (15,4,r3)", end.0, end.1, end.2);
        } else {
            eprintln!("recorded path sim => FAIL");
        }
        for n in [16, 17, 18, 19, 20, 21] {
            if let Some(end) = simulate_path_stepwise(&bb, 6, sr, sc, 0, &path_strings[..n]) {
                eprintln!("  prefix {n}: ({},{},r{})", end.0, end.1, end.2);
            }
        }
        let moves = bfs_moves(&bb, 6, sr, sc as usize, 0);
        let want = moves.iter().find(|m| m.row == 15 && m.col == 4 && m.rot == 3);
        eprintln!(
            "BFS (15,4,r3): {} path={:?}",
            want.is_some(),
            want.map(|m| m.path.as_slice())
        );
        for m in moves.iter().filter(|m| m.col == 4 && m.rot == 3) {
            eprintln!("  lock col4 r3 row{} path={:?}", m.row, m.path);
        }
    }

    let emu = emu_run_path(&b64, &path);
    eprintln!(
        "\nemu full path => ({},{},r{}) want (15,4,r3) got meta (13,4,r3)",
        emu.0, emu.1, emu.2
    );
    for n in [16, 17, 18, 19, 20, 21] {
        let pre: Vec<&str> = path[..n].to_vec();
        let sim_p = simulate_path_stepwise(&bb, 6, -2, 3, 0, &pre.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        let emu_p = emu_run_path(&b64, &pre);
        if sim_p.map(|(r, c, rot)| (r, c as usize, rot)) != Some(emu_p) {
            eprintln!(
                "DIVERGE step {n} {:?} sim={sim_p:?} emu=({},{},r{})",
                pre.last(),
                emu_p.0,
                emu_p.1,
                emu_p.2
            );
        }
    }

    for &(r, c, rot, lbl) in &[
        (14, 3, 0, "after-16D-sim"),
        (13, 3, 0, "after-16D-replay"),
        (12, 3, 3, "after-CCW1-sim"),
        (15, 4, 2, "pre-CCW#2"),
    ] {
        eprintln!(
            "CCW auto {lbl} ({r},{c},r{rot}): {:?}",
            srs_try_rotate_auto(&bb, 6, r, c, rot, false)
        );
        eprintln!(
            "CW auto {lbl} ({r},{c},r{rot}): {:?}",
            srs_try_rotate_auto(&bb, 6, r, c, rot, true)
        );
    }
    {
        use crate::bot::fixtures::emulator_from_savestate;
        let mut emu = emulator_from_savestate(&state);
        let pre16: Vec<&str> = path[..16].to_vec();
        emu_run_path_on(&mut emu, &pre16);
        eprintln!("emu pre-CCW#1 cells: {:?}", read_active_piece(|a| emu.memory.read(a)));
        emu_run_path_on(&mut emu, &["CCW"]);
        eprintln!(
            "emu post-CCW#1: {:?} ori=0x{:02x}",
            read_active_piece(|a| emu.memory.read(a)),
            read_current_ori(|a| emu.memory.read(a))
        );
    }
}

/// Diagnostic only (no asserts). Red test: `misdrop_real_detection::t_spin_r16_c5_r2_executor_reaches_want_lock`
/// cargo test t_spin_r16_c5_r2_replay -- --ignored --nocapture
#[test]
#[ignore = "diagnostic — use misdrop_real_detection::t_spin_r16_c5_r2_executor_reaches_want_lock"]
fn t_spin_r16_c5_r2_replay() {
    use crate::bot::fixtures::emulator_from_savestate;
    use base64::Engine;
    use crate::state::EmulatorState;

    let b64 = std::fs::read_to_string(
        super::fixtures::misdrop_fixture("misdrop_t_spin_r16_c5_r2_state.b64"),
    )
    .unwrap();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim().trim_matches('"'))
        .unwrap();
    let state: EmulatorState = bincode::deserialize(&bytes).unwrap();
    let mut emu = emulator_from_savestate(&state);
    let mut bot = TetrisBot::new();
    bot.set_pps(f64::INFINITY);
    bot.set_soft_drop_mode(true);
    bot.begin_replay_restore();

    for frame in 0..25_000 {
        let (_, actions) = bot.tick(
            |a| emu.memory.read(a),
            |s, l| (0..l).map(|i| emu.memory.read(s.wrapping_add(i))).collect(),
        );
        apply_bot_actions(&actions, &mut emu);
        emu.run_frame();
        bot.tick_post_frame(
            |a| emu.memory.read(a),
            |s, l| (0..l).map(|i| emu.memory.read(s.wrapping_add(i))).collect(),
        );
        let (mis, _) = bot.misdrop_stats();
        if mis > 0 {
            eprintln!(
                "frame={frame} MISDROP mis={mis} sprite=({},{},r{}) mode={}",
                piece_min_row(|a| emu.memory.read(a)),
                piece_left_col(|a| emu.memory.read(a)),
                ori_info(emu.memory.read(ADDR_CUR_ORI)).map(|i| i.1).unwrap_or(99),
                bot.mode()
            );
            eprintln!("path trace:\n{}", bot.debug_take_path_trace());
            break;
        }
        if frame > 400
            && matches!(bot.bot_state(), BotState::Idle)
            && bot.debug_get_move_path().is_empty()
        {
            eprintln!("frame={frame} lock OK mode={}", bot.mode());
            eprintln!("path trace:\n{}", bot.debug_take_path_trace());
            break;
        }
    }
}

/// cargo test t_spin_r16_c5_r2_probe -- --nocapture
#[test]
fn t_spin_r16_c5_r2_probe() {
    use base64::Engine;
    use crate::state::EmulatorState;
    use crate::bot::srs::srs_try_rotate_auto;

    let b64 = std::fs::read_to_string(
        super::fixtures::misdrop_fixture("misdrop_t_spin_r16_c5_r2_state.b64"),
    )
    .unwrap();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim().trim_matches('"'))
        .unwrap();
    let state: EmulatorState = bincode::deserialize(&bytes).unwrap();
    let bb = read_board_bitboard_from_ram(&state.ram);
    eprintln!("\n=== T spin board rows 11-17 ===");
    for row in 11..BOARD_ROWS {
        let bits = bb[row];
        let cols: String = (0..10)
            .map(|c| if (bits >> c) & 1 == 1 { '#' } else { '.' })
            .collect();
        eprintln!("  row {row}: {cols}");
    }

    let path: Vec<&str> = std::iter::repeat_n("D", 15)
        .chain(["R", "R", "CCW", "D", "D", "D", "L", "CCW"])
        .collect();
    let path_strings: Vec<String> = path.iter().map(|s| s.to_string()).collect();
    let type_t = 2usize;

    eprintln!("\n--- spawn (-2,3,r0) ---");
    if let Some(end) = simulate_path_stepwise(&bb, type_t, -2, 3, 0, &path_strings) {
        eprintln!("recorded path sim => ({},{},r{}) want (16,5,r2)", end.0, end.1, end.2);
    } else {
        eprintln!("recorded path sim => FAIL");
    }
    for n in [15, 16, 17, 18, 19, 20, 21, 22, 23] {
        if let Some(end) = simulate_path_stepwise(&bb, type_t, -2, 3, 0, &path_strings[..n]) {
            eprintln!("  prefix {n}: ({},{},r{})", end.0, end.1, end.2);
        }
    }
    let moves = bfs_moves(&bb, type_t, -2, 3, 0);
    let want = moves.iter().find(|m| m.row == 16 && m.col == 5 && m.rot == 2);
    eprintln!(
        "BFS (16,5,r2): {} path={:?}",
        want.is_some(),
        want.map(|m| m.path.as_slice())
    );
    for m in moves.iter().filter(|m| m.col == 5 && (m.rot == 2 || m.rot == 3)) {
        eprintln!("  lock col5 r{} row{} path={:?}", m.rot, m.row, m.path);
    }

    let emu = emu_run_path(&b64, &path);
    eprintln!(
        "\nemu full path => ({},{},r{}) want (16,5,r2) got meta (13,5,r3)",
        emu.0, emu.1, emu.2
    );
    for n in [15, 16, 17, 18, 19, 20, 21, 22, 23] {
        let pre: Vec<&str> = path[..n].to_vec();
        let sim_p = simulate_path_stepwise(
            &bb,
            type_t,
            -2,
            3,
            0,
            &pre.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        );
        let emu_p = emu_run_path(&b64, &pre);
        if sim_p.map(|(r, c, rot)| (r, c as usize, rot)) != Some(emu_p) {
            eprintln!(
                "DIVERGE step {n} {:?} sim={sim_p:?} emu=({},{},r{})",
                pre.last(),
                emu_p.0,
                emu_p.1,
                emu_p.2
            );
        }
    }

    let row13: Vec<&str> = std::iter::repeat_n("D", 15)
        .chain(["R", "CCW", "D"])
        .collect();
    eprintln!(
        "row-13 alt: sim={:?} emu={:?}",
        simulate_path_stepwise(
            &bb,
            type_t,
            -2,
            3,
            0,
            &row13.iter().map(|s| s.to_string()).collect::<Vec<_>>()
        ),
        emu_run_path(&b64, &row13)
    );

    for &(r, c, rot, lbl) in &[
        (13, 5, 0, "after-17R-sim"),
        (14, 5, 3, "replay-snap"),
        (12, 6, 3, "after-CCW-sim"),
        (15, 5, 3, "pre-final-CCW"),
    ] {
        eprintln!(
            "{lbl} ({r},{c},r{rot}) CCW={:?} CW={:?}",
            srs_try_rotate_auto(&bb, type_t, r, c, rot, false),
            srs_try_rotate_auto(&bb, type_t, r, c, rot, true)
        );
    }
    eprintln!("brute CCW r0 → (13,5,r3) or (14,5,r3):");
    for r in 11..=16 {
        for c in 0..=8 {
            if let Some(end) = srs_try_rotate_auto(&bb, type_t, r, c, 0, false) {
                if end.2 == 3 && end.1 >= 4 && end.1 <= 6 && (end.0 == 13 || end.0 == 14) {
                    eprintln!("  CCW from ({r},{c},r0) => {end:?}");
                }
            }
        }
    }
}

/// Prove whether (15,4,r3) is reachable on Rosy hardware from this savestate.
/// cargo test j_spin_r15_c4_r3_proof -- --nocapture
#[test]
fn j_spin_r15_c4_r3_proof() {
    use crate::bot::fixtures::emulator_from_savestate;
    use base64::Engine;
    use crate::state::EmulatorState;

    let b64 = std::fs::read_to_string(
        super::fixtures::misdrop_fixture("misdrop_j_spin_r15_c4_r3_state.b64"),
    )
    .unwrap();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim().trim_matches('"'))
        .unwrap();
    let state: EmulatorState = bincode::deserialize(&bytes).unwrap();
    let bb = read_board_bitboard_from_ram(&state.ram);

    let planned: Vec<&str> = std::iter::repeat_n("D", 16)
        .chain(["CCW", "CCW", "D", "L", "CW"])
        .collect();
    let planned_strings: Vec<String> = planned.iter().map(|s| s.to_string()).collect();

    eprintln!("\n=== PROOF: can hardware reach (15,4,r3)? ===\n");

    // 1) Sim says yes — document that.
    let sim_end = simulate_path_stepwise(&bb, 6, -2, 3, 0, &planned_strings);
    eprintln!("1) Sim planned path end: {sim_end:?} (want Some((15,4,3)))");

    // 2) Open-loop batch execution (same as live bot path runner).
    let emu_batch = emu_run_path(&b64, &planned);
    eprintln!(
        "2) Emu batch full path end: ({},{},r{})",
        emu_batch.0, emu_batch.1, emu_batch.2
    );
    for n in [16, 17, 18, 21] {
        let end = emu_run_path(&b64, &planned[..n]);
        eprintln!("   prefix {n}: ({},{},r{})", end.0, end.1, end.2);
    }

    // 3) Same session: after CCW#1, do steps 18–21 individually.
    let mut emu2 = emulator_from_savestate(&state);
    let prefix17: Vec<&str> = planned[..17].to_vec();
    emu_run_path_on(&mut emu2, &prefix17);
    let after_ccw1 = emu_piece_pos(&emu2);
    eprintln!(
        "\n3) Same-session continuation after CCW#1: ({},{},r{})",
        after_ccw1.0, after_ccw1.1, after_ccw1.2
    );
    for (i, act) in planned[17..].iter().enumerate() {
        let before = emu_piece_pos(&emu2);
        emu_run_path_on(&mut emu2, &[act]);
        let after = emu_piece_pos(&emu2);
        eprintln!(
            "   step {} {:3}: ({},{},r{}) -> ({},{},r{})",
            18 + i,
            act,
            before.0,
            before.1,
            before.2,
            after.0,
            after.1,
            after.2
        );
    }

    // 4) Gravity soak after step 17: does pose drift toward row 15?
    let mut emu3 = emulator_from_savestate(&state);
    emu_run_path_on(&mut emu3, &prefix17);
    let soak_start = emu_piece_pos(&emu3);
    for f in 0..600 {
        emu3.run_frame();
        let p = emu_piece_pos(&emu3);
        if p != soak_start {
            eprintln!("   gravity soak frame {f}: ({},{},r{})", p.0, p.1, p.2);
            break;
        }
    }
    let soak_end = emu_piece_pos(&emu3);
    eprintln!(
        "4) 600-frame gravity soak after CCW#1: ({},{},r{}) -> ({},{},r{})",
        soak_start.0, soak_start.1, soak_start.2, soak_end.0, soak_end.1, soak_end.2
    );

    // 5) Every BFS path claiming (15,4,r3) — verify on emulator.
    let moves = bfs_moves(&bb, 6, -2, 3, 0);
    let row15_paths: Vec<_> = moves
        .iter()
        .filter(|m| m.row == 15 && m.col == 4 && m.rot == 3)
        .collect();
    eprintln!(
        "\n5) BFS paths to (15,4,r3): {} candidate(s)",
        row15_paths.len()
    );
    for (pi, m) in row15_paths.iter().enumerate() {
        let path_refs: Vec<&str> = m.path.iter().map(|s| s.as_str()).collect();
        let end = emu_run_path(&b64, &path_refs);
        eprintln!("   path #{pi} len={} emu_end=({},{},r{})", m.path.len(), end.0, end.1, end.2);
        assert_ne!(
            end,
            (15, 4, 3),
            "emulator must NOT reach (15,4,r3) via BFS path #{pi}"
        );
    }

    // 6) Sim vs emu at each prefix — planned row-15 only exists in sim after step 17.
    eprintln!("\n6) Prefix divergence (sim row-15 chain vs emu):");
    for n in [17, 18, 19, 20, 21] {
        let sim_p = simulate_path_stepwise(&bb, 6, -2, 3, 0, &planned_strings[..n]);
        let emu_p = emu_run_path(&b64, &planned[..n]);
        let mark = if sim_p.map(|t| (t.0, t.1 as usize, t.2)) == Some(emu_p) {
            "match"
        } else {
            "DIVERGE"
        };
        eprintln!("   step {n}: sim={sim_p:?} emu={emu_p:?} {mark}");
    }

    // Final verdict assertions.
    assert_eq!(
        sim_end,
        Some((13, 4, 3)),
        "sim must match hardware lock after floor SRS fix"
    );
    assert_eq!(
        emu_batch,
        (13, 4, 3),
        "hardware lands at (13,4,r3) matching live misdrop got"
    );
    // 7) Variant paths — any way to lock at row 15?
    let variants: Vec<(&str, Vec<&str>)> = vec![
        ("skip L: ...D,CW", std::iter::repeat_n("D", 16).chain(["CCW","CCW","D","CW"]).collect()),
        ("extra D before CW", std::iter::repeat_n("D", 16).chain(["CCW","CCW","D","L","D","CW"]).collect()),
        ("CW before last D", std::iter::repeat_n("D", 16).chain(["CCW","CCW","D","CW","L"]).collect()),
        ("single CCW tail", std::iter::repeat_n("D", 16).chain(["CCW","D","L","CW"]).collect()),
    ];
    eprintln!("\n7) Variant paths to (15,4,r3):");
    for (name, path) in &variants {
        let end = emu_run_path(&b64, path);
        let sim = simulate_path_stepwise(
            &bb,
            6,
            -2,
            3,
            0,
            &path.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        );
        eprintln!("   {name}: sim={sim:?} emu={end:?}");
    }

    let row13: Vec<&str> = std::iter::repeat_n("D", 16)
        .chain(["R", "CCW", "D"])
        .collect();
    let row13_emu = emu_run_path(&b64, &row13);
    let row13_sim = simulate_path_stepwise(
        &bb,
        6,
        -2,
        3,
        0,
        &row13.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
    );
    eprintln!(
        "8) Row-13 path: sim={row13_sim:?} emu={row13_emu:?}"
    );

    eprintln!("\n=== VERDICT: spin to row 15 is NOT achievable on hardware ===\n");
}

/// cargo test i_tuck_r16_c1_r0_replay -- --nocapture
#[test]
fn i_tuck_r16_c1_r0_replay() {
    use crate::bot::fixtures::emulator_from_savestate;
    use base64::Engine;
    use crate::state::EmulatorState;

    let b64 = std::fs::read_to_string(
        super::fixtures::misdrop_fixture("misdrop_i_tuck_r16_c1_r0_state.b64"),
    )
    .unwrap();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim().trim_matches('"'))
        .unwrap();
    let state: EmulatorState = bincode::deserialize(&bytes).unwrap();
    let mut emu = emulator_from_savestate(&state);
    let mut bot = TetrisBot::new();
    bot.set_pps(f64::INFINITY);
    bot.set_soft_drop_mode(true);
    bot.begin_replay_restore();

    for frame in 0..25_000 {
        let (_, actions) = bot.tick(
            |a| emu.memory.read(a),
            |s, l| (0..l).map(|i| emu.memory.read(s.wrapping_add(i))).collect(),
        );
        for &(btn, down) in &actions {
            if down && btn == 5 {
                // count soft drops
            }
        }
        apply_bot_actions(&actions, &mut emu);
        emu.run_frame();
        bot.tick_post_frame(
            |a| emu.memory.read(a),
            |s, l| (0..l).map(|i| emu.memory.read(s.wrapping_add(i))).collect(),
        );
        let (mis, _) = bot.misdrop_stats();
        let row = piece_min_row(|a| emu.memory.read(a));
        let col = piece_left_col(|a| emu.memory.read(a));
        let rot = ori_info(emu.memory.read(ADDR_CUR_ORI))
            .map(|i| i.1 as usize)
            .unwrap_or(99);
        if mis > 0 {
            eprintln!(
                "frame={frame} MISDROP mis={mis} sprite=({row},{col},r{rot}) mode={}",
                bot.mode()
            );
            eprintln!("path trace:\n{}", bot.debug_take_path_trace());
            break;
        }
        if frame > 500
            && matches!(bot.bot_state(), BotState::Idle)
            && bot.debug_get_move_path().is_empty()
        {
            eprintln!(
                "frame={frame} lock OK sprite=({row},{col},r{rot}) mode={}",
                bot.mode()
            );
            eprintln!("path trace:\n{}", bot.debug_take_path_trace());
            break;
        }
    }
}

fn apply_bot_actions(actions: &[(u8, bool)], emu: &mut crate::emulator::Emulator) {
    use crate::emulator::joypad::GbButton;
    for &(btn, down) in actions {
        let b = match btn {
            0 => GbButton::A,
            1 => GbButton::B,
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

/// cargo test i_tuck_r16_c1_r0_probe -- --nocapture
#[test]
fn i_tuck_r16_c1_r0_probe() {
    use base64::Engine;
    use crate::state::EmulatorState;

    let b64 = std::fs::read_to_string(
        super::fixtures::misdrop_fixture("misdrop_i_tuck_r16_c1_r0_state.b64"),
    )
    .unwrap();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim().trim_matches('"'))
        .unwrap();
    let state: EmulatorState = bincode::deserialize(&bytes).unwrap();
    let bb = read_board_bitboard_from_ram(&state.ram);
    eprintln!("\n=== I tuck board rows 12-17 ===");
    for row in 12..BOARD_ROWS {
        let bits = bb[row];
        let cols: String = (0..10)
            .map(|c| if (bits >> c) & 1 == 1 { '#' } else { '.' })
            .collect();
        eprintln!("  row {row}: {cols}");
    }

    let path: Vec<&str> = std::iter::repeat_n("D", 14)
        .chain(["L", "L", "CCW", "D", "D", "L"])
        .collect();
    let path_strings: Vec<String> = path.iter().map(|s| s.to_string()).collect();

    for (label, sr, sc, srot) in [
        ("manifest (-2,0,r0)", -2i32, 0i32, 0usize),
        ("meta (-2,5,r1)", -2, 5, 1),
        ("meta (0,5,r1)", 0, 5, 1),
    ] {
        eprintln!("\n--- spawn {label} ---");
        if let Some(end) = simulate_path_stepwise(&bb, 0, sr, sc, srot, &path_strings) {
            eprintln!("recorded path sim => ({},{},r{}) want (16,1,r0)", end.0, end.1, end.2);
        } else {
            eprintln!("recorded path sim => FAIL");
        }
        for n in [14, 15, 16, 17, 18, 19, 20] {
            if let Some(end) = simulate_path_stepwise(&bb, 0, sr, sc, srot, &path_strings[..n]) {
                eprintln!("  prefix {n}: ({},{},r{})", end.0, end.1, end.2);
            }
        }
        let moves = bfs_moves(&bb, 0, sr, sc as usize, srot);
        let want = moves.iter().find(|m| m.row == 16 && m.col == 1 && m.rot == 0);
        eprintln!(
            "BFS (16,1,r0): {} path={:?}",
            want.is_some(),
            want.map(|m| m.path.as_slice())
        );
        for m in moves.iter().filter(|m| m.col == 1 && m.rot == 0) {
            eprintln!("  lock col1 r0 row{} path={:?}", m.row, m.path);
        }
    }

    {
        use crate::bot::fixtures::emulator_from_savestate;
        let mut emu = emulator_from_savestate(&state);
        let actual_col = piece_left_col(|a| emu.memory.read(a));
        let safe = find_safe_normal_placement(
            &|a: u16| emu.memory.read(a),
            &|b: u16, l: u16| {
                (0..l)
                    .map(|i| emu.memory.read(b.wrapping_add(i)))
                    .collect::<Vec<u8>>()
            },
            actual_col,
        );
        eprintln!(
            "find_safe_normal_placement live spawn col={actual_col}: {safe:?}"
        );
    }

    let emu = emu_run_path(&b64, &path);
    eprintln!(
        "\nemu full path => ({},{},r{}) want (16,1,r0) got meta (14,1,r0)",
        emu.0, emu.1, emu.2
    );
    for i in 0..=path.len() {
        let prefix: Vec<&str> = path[..i].to_vec();
        let sim_p = simulate_path_stepwise(
            &bb,
            0,
            -2,
            5,
            1,
            &prefix.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        );
        let emu_p = if prefix.is_empty() {
            (-2i32, 5usize, 1usize)
        } else {
            emu_run_path(&b64, &prefix)
        };
        if sim_p.map(|(r, c, rot)| (r, c as usize, rot)) != Some(emu_p) {
            eprintln!(
                "DIVERGE step {i} {:?} sim={sim_p:?} emu=({},{},r{})",
                prefix.last(),
                emu_p.0,
                emu_p.1,
                emu_p.2
            );
        }
    }

    use crate::bot::srs::{srs_try_rotate_auto, srs_try_rotate_detailed};
    for &(r, c, rot) in &[(12, 3, 1), (12, 2, 1)] {
        for &cw in &[false, true] {
            let btn = if cw { "CW" } else { "CCW" };
            eprintln!(
                "I {btn} ({r},{c},r{rot}) auto: {:?}",
                srs_try_rotate_auto(&bb, 0, r, c, rot, cw)
            );
            eprintln!(
                "I {btn} ({r},{c},r{rot}) floor: {:?}",
                srs_try_rotate_detailed(&bb, 0, r, c, rot, cw, true)
            );
        }
    }
    // Step-confirm emu through path prefix 17 (through CCW).
    for n in [16, 17] {
        let pre: Vec<&str> = path[..n].to_vec();
        let emu_p = emu_run_path(&b64, &pre);
        let sim_p = simulate_path_stepwise(&bb, 0, -2, 5, 1, &pre.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        eprintln!("prefix {n}: sim={sim_p:?} emu=({},{},r{})", emu_p.0, emu_p.1, emu_p.2);
    }
    {
        use crate::bot::fixtures::emulator_from_savestate;
        let mut emu = emulator_from_savestate(&state);
        let pre16: Vec<&str> = path[..16].to_vec();
        emu_run_path_on(&mut emu, &pre16);
        let cells_pre = read_active_piece(|a| emu.memory.read(a));
        eprintln!("emu cells before CCW: {cells_pre:?}");
        emu_run_path_on(&mut emu, &["CCW"]);
        let cells_post = read_active_piece(|a| emu.memory.read(a));
        let ori = read_current_ori(|a| emu.memory.read(a));
        eprintln!("emu cells after CCW: {cells_post:?} ori=0x{ori:02x}");
    }
}

/// cargo test z_spin_r15_c1_r1_emu_vs_sim -- --nocapture
#[test]
fn z_spin_r15_c1_r1_emu_vs_sim() {
    use base64::Engine;
    use crate::state::EmulatorState;

    let b64 = std::fs::read_to_string(
        super::fixtures::misdrop_fixture("misdrop_z_spin_r15_c1_r1_state.b64"),
    )
    .unwrap();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim().trim_matches('"'))
        .unwrap();
    let state: EmulatorState = bincode::deserialize(&bytes).unwrap();
    let bb = read_board_bitboard_from_ram(&state.ram);
    let path: Vec<&str> = std::iter::repeat_n("D", 17)
        .chain(["L", "CW"])
        .collect();
    let path_strings: Vec<String> = path.iter().map(|s| s.to_string()).collect();
    let sim = simulate_path_stepwise(&bb, 4, -2, 3, 0, &path_strings);
    let emu = emu_run_path(&b64, &path);
    assert_eq!(sim, Some((14, 2, 1)), "sim must match hardware after S/Z floor fix");
    assert_eq!((emu.0, emu.1, emu.2), (14, 2, 1));
}

/// cargo test i_tuck_r16_c1_r0_emu_vs_sim -- --nocapture
#[test]
fn i_tuck_r16_c1_r0_emu_vs_sim() {
    use base64::Engine;
    use crate::state::EmulatorState;

    let b64 = std::fs::read_to_string(
        super::fixtures::misdrop_fixture("misdrop_i_tuck_r16_c1_r0_state.b64"),
    )
    .unwrap();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim().trim_matches('"'))
        .unwrap();
    let state: EmulatorState = bincode::deserialize(&bytes).unwrap();
    let bb = read_board_bitboard_from_ram(&state.ram);
    let path: Vec<&str> = std::iter::repeat_n("D", 14)
        .chain(["L", "L", "CCW", "D", "D", "L"])
        .collect();
    let path_strings: Vec<String> = path.iter().map(|s| s.to_string()).collect();
    let sim = simulate_path_stepwise(&bb, 0, -2, 5, 1, &path_strings);
    // After CCW fix, sim no longer falsely reaches row-16 want.
    assert_ne!(sim, Some((16, 1, 0)));
    // Hardware CCW from ~(12,2,r1) lands (13,0,r0) — prefix through step 17.
    let pre17: Vec<&str> = path[..17].to_vec();
    let emu17 = emu_run_path(&b64, &pre17);
    use crate::bot::srs::srs_try_rotate_auto;
    assert_eq!(
        srs_try_rotate_auto(&bb, 0, 12, 2, 1, false),
        Some((13, 0, 0))
    );
    assert_eq!(
        (emu17.0, emu17.1, emu17.2),
        (13, 0, 0),
        "emu CCW must match floor I A≠B policy"
    );

    // Recorded misdrop path (2×L before CCW) must not sim-reach want.
    let recorded: Vec<&str> = std::iter::repeat_n("D", 14)
        .chain(["L", "L", "CCW", "D", "D", "L"])
        .collect();
    assert_ne!(
        simulate_path_stepwise(
            &bb,
            0,
            -2,
            5,
            1,
            &recorded.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        ),
        Some((16, 1, 0))
    );
}

#[test]
#[ignore = "legacy s_to_l_misdrop_state.b64 removed; rewrite against manifest fixture"]
fn fix_sz_spin_path_rewrites_s_to_l_tuck() {
    use crate::state::EmulatorState;
    use base64::Engine;

    let b64 = std::fs::read_to_string(
        super::fixtures::misdrop_fixture("s_to_l_misdrop_state.b64"),
    )
    .unwrap();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim().trim_matches('"'))
        .unwrap();
    let state: EmulatorState = bincode::deserialize(&bytes).unwrap();
    let bb = read_board_bitboard_from_ram(&state.ram);

    let bfs: Vec<String> = std::iter::repeat("D")
        .take(14)
        .map(|s| s.to_string())
        .chain(
            ["R", "CCW", "D", "D", "D", "CCW"]
                .iter()
                .map(|s| s.to_string()),
        )
        .collect();
    let fixed = fix_sz_spin_path(&bb, 3, 0, 3, 0, "spin", &bfs);
    assert_eq!(
        fixed.iter().filter(|a| *a == "R").count(),
        2,
        "user path has two R's: {:?}",
        fixed
    );
    assert_eq!(fixed[14], "CCW");
    assert_eq!(fixed[15], "R");
    assert_eq!(fixed[fixed.len() - 2], "R");
    assert_eq!(fixed[fixed.len() - 1], "CCW");
    assert_eq!(
        simulate_path_prefix(&bb, 3, 0, 3, 0, &fixed),
        Some((16, 5, 2))
    );

    // Piece at row 5: BFS returns D×9 + tuck suffix (UI: DDDDDDDDDRCCWDDDCCW).
    let bfs_from5: Vec<String> = std::iter::repeat("D")
        .take(9)
        .map(|s| s.to_string())
        .chain(
            ["R", "CCW", "D", "D", "D", "CCW"]
                .iter()
                .map(|s| s.to_string()),
        )
        .collect();
    let fixed9 = fix_sz_spin_path(&bb, 3, 5, 3, 0, "spin", &bfs_from5);
    assert_eq!(fixed9.iter().filter(|a| *a == "R").count(), 2);
    assert_eq!(fixed9[9], "CCW");
    assert_eq!(fixed9[10], "R");
    assert_eq!(fixed9[fixed9.len() - 2], "R");
    assert_eq!(fixed9[fixed9.len() - 1], "CCW");
    assert_eq!(
        simulate_path_prefix(&bb, 3, 5, 3, 0, &fixed9),
        Some((16, 5, 2))
    );
}

#[test]
#[ignore = "awaiting fresh fixture capture"]
fn fix_s_spin_cw_terminal_leaves_path_when_cw_sim_misses_lock() {
    use crate::state::EmulatorState;
    use base64::Engine;

    let b64 = std::fs::read_to_string(
        super::fixtures::misdrop_fixture("s_to_l_misdrop_state.b64"),
    )
    .unwrap();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim().trim_matches('"'))
        .unwrap();
    let state: EmulatorState = bincode::deserialize(&bytes).unwrap();
    let bb = read_board_bitboard_from_ram(&state.ram);

    let bfs: Vec<String> = std::iter::repeat("D")
        .take(14)
        .map(|s| s.to_string())
        .chain(
            ["R", "CCW", "D", "D", "D", "CCW"]
                .iter()
                .map(|s| s.to_string()),
        )
        .collect();
    let cw_path: Vec<String> = std::iter::repeat("D")
        .take(14)
        .map(|s| s.to_string())
        .chain(["R", "CW"].iter().map(|s| s.to_string()))
        .collect();
    eprintln!(
        "cw_path sim={:?}",
        simulate_path_stepwise(&bb, 3, 0, 3, 0, &cw_path)
    );
    let fixed = fix_s_spin_cw_terminal(&bb, 3, 0, 3, 0, 16, 5, 2, "spin", &bfs);
    if simulate_path_stepwise(&bb, 3, 0, 3, 0, &cw_path)
        .is_some_and(|(r, c, rot)| r == 16 && c == 5 && rot == 2)
    {
        assert_eq!(fixed, cw_path, "must prefer D×n,R,CW when sim reaches lock");
    } else {
        assert_eq!(fixed, bfs, "keep BFS CCW tail when CW shortcut fails sim");
    }
}

#[test]
#[ignore = "awaiting fresh fixture capture"]
fn s_to_l_spin_remains_trustworthy() {
    use crate::state::EmulatorState;
    use base64::Engine;

    let b64 = std::fs::read_to_string(
        super::fixtures::misdrop_fixture("s_to_l_misdrop_state.b64"),
    )
    .unwrap();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim().trim_matches('"'))
        .unwrap();
    let state: EmulatorState = bincode::deserialize(&bytes).unwrap();
    let bb = read_board_bitboard_from_ram(&state.ram);

    let path: Vec<String> = std::iter::repeat("D")
        .take(14)
        .map(|s| s.to_string())
        .chain(
            ["R", "CCW", "D", "D", "D", "CCW"]
                .iter()
                .map(|s| s.to_string()),
        )
        .collect();
    assert!(
        bfs_plan_acceptable(&bb, 3, 0, 3, 0, 0, 0, 16, 5, 2, &path),
        "S→L spin must stay BFS-acceptable"
    );
}

#[test]
fn mid_path_tuck_ori_glitch_not_misdrop() {
    let mut bot = TetrisBot::new();
    bot.state = BotState::Path;
    bot.last_ori = 0x18; // T
    bot.move_path = vec!["D".into(); 16];
    bot.move_path.extend(["L", "L", "D", "D", "R"].iter().map(|s| s.to_string()));
    bot.path_step = 14;
    bot.intended_lock = Some((16, 2, 0, "tuck".to_string()));
    bot.last_valid_snap = Some((14, 2, 0));
    bot.last_placement = Some(PlacementReplay {
        version: "1".into(),
        timestamp: String::new(),
        source: None,
        current_piece: PieceInfo {
            piece_type: 2,
            rot: 0,
            spawn_col: 3,
        },
        next_piece: NextPiece {
            piece_type: 3,
            ori: 0x14,
        },
        misdrop: None,
        strategy: None,
        mode: None,
        pps: None,
        note: String::new(),
    });

    // Brief Z-piece ori glitch mid-tuck (not spawn T, not next S).
    let read = |a: u16| -> u8 {
        match a {
            ADDR_CUR_ORI => 0x10,
            0xC010 | 0xC014 | 0xC018 | 0xC01C => 128,
            0xC011 | 0xC015 | 0xC019 | 0xC01D => 24,
            _ => 0,
        }
    };
    let read_r = |_: u16, _: u16| vec![0u8; BOARD_ROWS * BOARD_STRIDE];
    let mut actions = Vec::new();
    bot.handle_path(&read, &read_r, &mut actions, 0x10);
    assert_eq!(bot.misdrop_count, 0);
    assert_eq!(bot.state, BotState::Path);
}

#[test]
fn handle_path_type_mismatch_never_counts_misdrop() {
    let mut bot = TetrisBot::new();
    bot.state = BotState::Path;
    bot.last_ori = 0x18; // T
    bot.move_path = vec!["L".into(), "D".into()];
    bot.path_step = 0;
    bot.intended_lock = Some((16, 2, 0, "normal".to_string()));

    let read = |_: u16| 0u8;
    let read_r = |_: u16, _: u16| vec![0u8; BOARD_ROWS * BOARD_STRIDE];
    let mut actions = Vec::new();
    bot.handle_path(&read, &read_r, &mut actions, 0x14); // S ori, hard mismatch
    assert_eq!(bot.misdrop_count, 0, "handle_path must never increment misdrop_count");
    assert_eq!(bot.state, BotState::Idle);
}

#[test]
fn state_restore_clears_cached_path_and_replans() {
    let mut bot = TetrisBot::new();
    bot.move_path = vec!["D".into(); 22];
    bot.planned_path = bot.move_path.clone();
    bot.path_step = 10;
    bot.intended_lock = Some((16, 5, 2, "spin".to_string()));
    bot.state = BotState::Path;

    bot.begin_state_restore();
    assert!(bot.move_path.is_empty());
    assert!(bot.planned_path.is_empty());
    assert_eq!(bot.path_step, 0);
    assert!(bot.intended_lock.is_none());
    assert!(bot.state_restore_replan);
    assert!(!bot.replay_restore_suppress);
    assert_eq!(bot.state, BotState::Idle);
}

#[test]
fn replay_restore_suppress_survives_multiple_glitches() {
    let mut bot = TetrisBot::new();
    bot.replay_restore_suppress = true;
    bot.state = BotState::Path;
    bot.last_ori = 0x18;
    bot.move_path = vec!["L".into()];
    bot.path_step = 0;
    bot.intended_lock = Some((16, 2, 0, "normal".to_string()));

    let read = |_: u16| 0u8;
    let read_r = |_: u16, _: u16| vec![0u8; BOARD_ROWS * BOARD_STRIDE];
    let mut actions = Vec::new();
    bot.handle_path(&read, &read_r, &mut actions, 0x14); // S ori, type mismatch
    assert_eq!(bot.misdrop_count, 0);
    assert!(bot.replay_restore_suppress, "suppress must survive first glitch");
    bot.handle_path(&read, &read_r, &mut actions, 0x14);
    assert_eq!(bot.misdrop_count, 0, "second glitch must not misdrop while suppress active");
    assert!(bot.replay_restore_suppress);
}

#[test]
fn mid_path_next_piece_spawn_lock_transition() {
    let mut bot = TetrisBot::new();
    bot.state = BotState::Path;
    bot.last_ori = 0x00; // L r0
    bot.move_path = vec!["D".into(); 22];
    bot.path_step = 5;
    bot.intended_lock = Some((15, 6, 1, "spin".to_string()));
    bot.last_placement = Some(PlacementReplay {
        version: "1".into(),
        timestamp: String::new(),
        source: None,
        current_piece: PieceInfo {
            piece_type: 5,
            rot: 0,
            spawn_col: 3,
        },
        next_piece: NextPiece {
            piece_type: 1,
            ori: 0x0c,
        },
        misdrop: None,
        strategy: None,
        mode: None,
        pps: None,
        note: String::new(),
    });

    let read = |a: u16| -> u8 {
        match a {
            ADDR_CUR_ORI => 0x0c, // O r0 — next piece after L locked early
            _ => 0,
        }
    };
    let read_r = |_: u16, _: u16| vec![0u8; BOARD_ROWS * BOARD_STRIDE];
    let mut actions = Vec::new();
    bot.handle_path(&read, &read_r, &mut actions, 0x0c);
    assert_eq!(bot.misdrop_count, 0);
    assert_eq!(bot.state, BotState::Idle);
    assert!(bot.move_path.is_empty());
}

#[test]
fn path_complete_ori_transition_not_misdrop() {
    let mut bot = TetrisBot::new();
    bot.state = BotState::Path;
    bot.last_ori = 0x18; // T
    bot.move_path = vec!["D".into(); 16];
    bot.move_path.extend(["L", "L", "D", "D", "R"].iter().map(|s| s.to_string()));
    bot.path_step = bot.move_path.len();
    bot.intended_lock = Some((16, 2, 0, "tuck".to_string()));
    bot.last_valid_snap = Some((14, 2, 0));
    bot.last_placement = Some(PlacementReplay {
        version: "1".into(),
        timestamp: String::new(),
        source: None,
        current_piece: PieceInfo {
            piece_type: 2,
            rot: 0,
            spawn_col: 3,
        },
        next_piece: NextPiece {
            piece_type: 3,
            ori: 0x14,
        },
        misdrop: None,
        strategy: None,
        mode: None,
        pps: None,
        note: String::new(),
    });

    // Next-piece ori (S) appears right after tuck path completes.
    let read = |a: u16| -> u8 {
        match a {
            ADDR_CUR_ORI => 0x14,
            0xC010 | 0xC014 | 0xC018 | 0xC01C => 128,
            0xC011 | 0xC015 | 0xC019 | 0xC01D => 24,
            _ => 0,
        }
    };
    let read_r = |_: u16, _: u16| vec![0u8; BOARD_ROWS * BOARD_STRIDE];
    let mut actions = Vec::new();
    bot.handle_path(&read, &read_r, &mut actions, 0x14);
    assert_eq!(bot.misdrop_count, 0, "T→S lock transition must not misdrop");
    assert!(
        matches!(bot.state, BotState::Dropping | BotState::Idle),
        "path complete must enter lock verify, got {:?}",
        bot.state
    );
}

#[test]
fn i_spin_mid_path_wrong_lock_detected_via_board_diff() {
    use super::simulate_place_and_clear;

    let mut bot = TetrisBot::new();
    bot.intended_lock = Some((16, 3, 2, "spin".to_string()));
    bot.planned_path = vec!["D".into(); 17];
    bot.planned_path
        .extend(["CW", "D", "L", "CW", "D", "D", "CCW"].iter().map(|s| s.to_string()));
    bot.move_path = bot.planned_path.clone();
    bot.path_step = 19;
    bot.lock_verify_path_incomplete = true;
    bot.last_placement = Some(PlacementReplay {
        version: "1".into(),
        timestamp: String::new(),
        source: None,
        current_piece: PieceInfo {
            piece_type: 0,
            rot: 1,
            spawn_col: 4,
        },
        next_piece: NextPiece {
            piece_type: 2,
            ori: 0x0c,
        },
        misdrop: None,
        strategy: None,
        mode: None,
        pps: None,
        note: String::new(),
    });

    let bb_before = [0u16; BOARD_ROWS];
    // Vertical I locked in the stack (wrong rot) while path still had L,CW,D,D,CCW left.
    let (bb_after, _, _) = simulate_place_and_clear(&bb_before, 0, 1, 3, 11);
    bot.lock_verify_board_before = Some(bb_before);
    bot.lock_verify_post_frame = true;
    bot.pending_lock_verify = true;

    let read_r = move |_: u16, _: u16| {
        let mut raw = vec![0u8; BOARD_ROWS * BOARD_STRIDE];
        for row in 0..BOARD_ROWS {
            let base = row * BOARD_STRIDE + 2;
            for col in 0..BOARD_COLS {
                if (bb_after[row] & (1 << col)) != 0 {
                    raw[base + col] = 0x81;
                }
            }
        }
        raw
    };
    bot.tick_post_frame(|_: u16| 0u8, &read_r);
    assert_eq!(
        bot.misdrop_count, 1,
        "incomplete I spin with vertical lock must misdrop"
    );
}

#[test]
fn verify_lock_board_diff_detects_z_spin_col_misdrop() {
    use super::simulate_place_and_clear;

    let bb_before = [0u16; BOARD_ROWS];
    let (bb_after, _, _) = simulate_place_and_clear(&bb_before, 4, 1, 3, 15);

    let mut bot = TetrisBot::new();
    bot.intended_lock = Some((15, 2, 1, "spin".to_string()));
    bot.move_path = vec!["D".into(); 17];
    bot.move_path.push("CW".into());
    bot.last_placement = Some(PlacementReplay {
        version: "1".into(),
        timestamp: String::new(),
        source: None,
        current_piece: PieceInfo {
            piece_type: 4,
            rot: 0,
            spawn_col: 3,
        },
        next_piece: NextPiece {
            piece_type: 1,
            ori: 4,
        },
        misdrop: None,
        strategy: None,
        mode: None,
        pps: None,
        note: String::new(),
    });
    bot.pending_lock_verify = true;
    bot.lock_verify_post_frame = true;
    bot.lock_verify_board_before = Some(bb_before);
    bot.start_lock_audit(15, 3, 1, true);

    let read_r = move |_: u16, _: u16| {
        let mut raw = vec![0u8; BOARD_ROWS * BOARD_STRIDE];
        for row in 0..BOARD_ROWS {
            let base = row * BOARD_STRIDE + 2;
            for col in 0..BOARD_COLS {
                if (bb_after[row] & (1 << col)) != 0 {
                    raw[base + col] = 0x81;
                }
            }
        }
        raw
    };
    bot.tick_post_frame(|_: u16| 0u8, &read_r);
    assert_eq!(
        bot.misdrop_count, 1,
        "Z spin col3 vs want col2 must misdrop via board diff"
    );
}

#[test]
fn stale_lock_snapshot_empty_diff_is_pose_unknown() {
    use super::simulate_place_and_clear;

    let bb_before = [0u16; BOARD_ROWS];
    let (bb_after, _, _) = simulate_place_and_clear(&bb_before, 4, 1, 3, 15);

    let mut bot = TetrisBot::new();
    bot.intended_lock = Some((15, 2, 1, "spin".to_string()));
    bot.last_placement = Some(PlacementReplay {
        version: "1".into(),
        timestamp: String::new(),
        source: None,
        current_piece: PieceInfo {
            piece_type: 4,
            rot: 0,
            spawn_col: 3,
        },
        next_piece: NextPiece {
            piece_type: 1,
            ori: 4,
        },
        misdrop: None,
        strategy: None,
        mode: None,
        pps: None,
        note: String::new(),
    });
    bot.pending_lock_verify = true;
    bot.lock_verify_post_frame = true;
    // Stale snapshot: piece already merged when "before" was taken (old handle_dropping bug).
    bot.lock_verify_board_before = Some(bb_after);

    let read_r = move |_: u16, _: u16| {
        let mut raw = vec![0u8; BOARD_ROWS * BOARD_STRIDE];
        for row in 0..BOARD_ROWS {
            let base = row * BOARD_STRIDE + 2;
            for col in 0..BOARD_COLS {
                if (bb_after[row] & (1 << col)) != 0 {
                    raw[base + col] = 0x81;
                }
            }
        }
        raw
    };
    bot.tick_post_frame(|_: u16| 0u8, &read_r);
    assert_eq!(
        bot.misdrop_count, 0,
        "before==after yields empty diff — pose unknown, no misdrop"
    );
}

#[test]
fn verify_pending_lock_spin_row_off_by_one_not_misdrop() {
    let mut bot = TetrisBot::new();
    bot.intended_lock = Some((15, 1, 0, "spin".to_string()));
    bot.move_path = vec!["D".into(); 16];
    bot.move_path.extend(["CCW", "R", "D", "D", "D", "L", "CW"].iter().map(|s| s.to_string()));
    bot.last_valid_snap = Some((15, 1, 0));
    bot.pending_lock_verify = true;
    bot.lock_verify_col_rot_ok = false;
    bot.last_placement = Some(PlacementReplay {
        version: "1".into(),
        timestamp: String::new(),
        source: None,
        current_piece: PieceInfo {
            piece_type: 4,
            rot: 0,
            spawn_col: 3,
        },
        next_piece: NextPiece {
            piece_type: 6,
            ori: 4,
        },
        misdrop: None,
        strategy: None,
        mode: None,
        pps: None,
        note: String::new(),
    });

    let read = |_: u16| 0u8;
    let read_r = |_: u16, _: u16| vec![0u8; BOARD_ROWS * BOARD_STRIDE];
    bot.verify_pending_lock(&read, &read_r);
    assert_eq!(
        bot.misdrop_count, 0,
        "Z spin col/rot match with gravity row +1 must not misdrop"
    );
}

#[test]
fn l_to_i_normal_intent_tuck_path_not_misdrop() {
    let path: Vec<String> = std::iter::repeat_n("D".into(), 17)
        .chain(["L".into(), "L".into()])
        .collect();
    let exp = LockExpectation {
        want_col: 5,
        want_rot: 1,
        want_row: Some(14),
        mtype_for_row: resolve_mtype_for_row("normal", "tuck"),
    };
    let act = LockActual {
        col: 5,
        rot: 1,
        eff_row: 16,
    };
    assert_eq!(evaluate_lock(&exp, &act, &path), None);
}

#[test]
fn verify_pending_lock_skips_when_col_rot_matched_at_begin_drop() {
    let mut bot = TetrisBot::new();
    bot.intended_lock = Some((14, 5, 1, "tuck".to_string()));
    bot.move_path = vec!["D".into(); 17];
    bot.move_path.extend(["R", "R", "R"].iter().map(|s| s.to_string()));
    bot.last_valid_snap = Some((15, 6, 0));
    bot.pending_lock_verify = true;
    bot.lock_verify_col_rot_ok = true;
    bot.last_placement = Some(PlacementReplay {
        version: "1".into(),
        timestamp: String::new(),
        source: None,
        current_piece: PieceInfo {
            piece_type: 4,
            rot: 1,
            spawn_col: 3,
        },
        next_piece: NextPiece {
            piece_type: 0,
            ori: 0,
        },
        misdrop: None,
        strategy: None,
        mode: None,
        pps: None,
        note: String::new(),
    });

    let read = |_: u16| 0u8;
    let read_r = |_: u16, _: u16| vec![0u8; BOARD_ROWS * BOARD_STRIDE];
    bot.verify_pending_lock(&read, &read_r);
    assert_eq!(bot.misdrop_count, 0, "stale snap must not re-trigger misdrop");
    assert!(!bot.pending_lock_verify);
}

#[test]
fn has_pending_misdrop_pairing_true_during_active_path() {
    let mut bot = TetrisBot::new();
    assert!(!bot.has_pending_misdrop_pairing());
    bot.state = BotState::Path;
    bot.move_path = vec!["D".into(), "L".into(), "R".into()];
    bot.path_step = 1;
    assert!(
        bot.has_pending_misdrop_pairing(),
        "active path must freeze JS spawn pairing until lock/misdrop"
    );
    // Path state alone freezes even after steps exhaust (mid-path high ori still possible).
    bot.path_step = 3;
    assert!(
        bot.has_pending_misdrop_pairing(),
        "BotState::Path must keep pairing frozen until Idle"
    );
    bot.state = BotState::Idle;
    bot.move_path.clear();
    bot.path_step = 0;
    assert!(!bot.has_pending_misdrop_pairing());
    // intended_lock survives until the next plan — must NOT freeze Idle spawn
    // capture for the following piece (empty-board misdrop attach regression).
    bot.intended_lock = Some((15, 5, 1, "tuck".into()));
    assert!(
        !bot.has_pending_misdrop_pairing(),
        "Idle + stale intended_lock must allow next-piece plan-time capture"
    );
    bot.pending_lock_verify = true;
    assert!(
        bot.has_pending_misdrop_pairing(),
        "pending_lock_verify must freeze pairing through lock settle"
    );
}

#[test]
fn board_verify_defers_o_partial_merge_not_spurious_row16() {
    use super::simulate_place_and_clear;

    let mut bot = TetrisBot::new();
    bot.intended_lock = Some((15, 8, 0, "normal".to_string()));
    bot.move_path = vec!["D".into(); 3];
    bot.last_placement = Some(PlacementReplay {
        version: "1".into(),
        timestamp: String::new(),
        source: None,
        current_piece: PieceInfo {
            piece_type: 1,
            rot: 0,
            spawn_col: 8,
        },
        next_piece: NextPiece {
            piece_type: 0,
            ori: 8,
        },
        misdrop: None,
        strategy: None,
        mode: None,
        pps: None,
        note: String::new(),
    });

    // Stack under col 8–9 at row 17; O anchor 15 only bottom half merged so far.
    let mut bb = [0u16; BOARD_ROWS];
    bb[17] |= (1 << 8) | (1 << 9);
    let (mut bb_partial, _, _) = simulate_place_and_clear(&bb, 1, 0, 8, 15);
    // Drop top row of O to simulate one-frame partial merge.
    bb_partial[15] &= !((1 << 8) | (1 << 9));

    bot.lock_verify_board_before = Some(bb);
    bot.lock_verify_post_frame = true;
    bot.pending_lock_verify = true;

    let read_r = move |_: u16, _: u16| {
        let mut raw = vec![0u8; BOARD_ROWS * BOARD_STRIDE];
        for row in 0..BOARD_ROWS {
            let base = row * BOARD_STRIDE + 2;
            for col in 0..BOARD_COLS {
                if (bb_partial[row] & (1 << col)) != 0 {
                    raw[base + col] = 0x81;
                }
            }
        }
        raw
    };
    bot.tick_post_frame(|_: u16| 0u8, &read_r);
    assert_eq!(
        bot.misdrop_count, 0,
        "partial O merge must defer, not spurious row-16 misdrop"
    );
    assert!(bot.pending_lock_verify, "partial merge must re-schedule verify");
}

#[test]
fn board_diff_detects_tuck_row_misdrop() {
    use super::simulate_place_and_clear;

    let mut bot = TetrisBot::new();
    bot.intended_lock = Some((16, 5, 0, "tuck".to_string()));
    bot.planned_path = vec!["D".into(); 17];
    bot.planned_path.extend(["CW", "D", "R"].iter().map(|s| s.to_string()));
    bot.move_path = bot.planned_path.clone();
    bot.last_placement = Some(PlacementReplay {
        version: "1".into(),
        timestamp: String::new(),
        source: None,
        current_piece: PieceInfo {
            piece_type: 2,
            rot: 0,
            spawn_col: 3,
        },
        next_piece: NextPiece {
            piece_type: 5,
            ori: 0,
        },
        misdrop: None,
        strategy: None,
        mode: None,
        pps: None,
        note: String::new(),
    });

    let bb = [0u16; BOARD_ROWS];
    bot.lock_verify_board_before = Some(bb);
    bot.lock_verify_post_frame = true;
    bot.pending_lock_verify = true;

    // T tuck correct col/rot but one row high — want row 16, landed row 15.
    let (bb_wrong, _, _) = simulate_place_and_clear(&bb, 2, 0, 5, 15);
    let read_r = move |_: u16, _: u16| {
        let mut raw = vec![0u8; BOARD_ROWS * BOARD_STRIDE];
        for row in 0..BOARD_ROWS {
            let base = row * BOARD_STRIDE + 2;
            for col in 0..BOARD_COLS {
                if (bb_wrong[row] & (1 << col)) != 0 {
                    raw[base + col] = 0x81;
                }
            }
        }
        raw
    };
    bot.tick_post_frame(|_: u16| 0u8, &read_r);
    assert_eq!(
        bot.misdrop_count, 1,
        "board diff must detect tuck row misdrop"
    );
}

#[test]
fn board_diff_detects_tuck_col_misdrop() {
    use super::simulate_place_and_clear;

    let mut bot = TetrisBot::new();
    bot.intended_lock = Some((16, 5, 0, "tuck".to_string()));
    bot.planned_path = vec!["D".into(); 17];
    bot.planned_path.extend(["CW", "D", "R"].iter().map(|s| s.to_string()));
    bot.move_path = bot.planned_path.clone();
    bot.last_placement = Some(PlacementReplay {
        version: "1".into(),
        timestamp: String::new(),
        source: None,
        current_piece: PieceInfo {
            piece_type: 2,
            rot: 0,
            spawn_col: 3,
        },
        next_piece: NextPiece {
            piece_type: 5,
            ori: 0,
        },
        misdrop: None,
        strategy: None,
        mode: None,
        pps: None,
        note: String::new(),
    });

    let bb = [0u16; BOARD_ROWS];
    bot.lock_verify_board_before = Some(bb);
    bot.lock_verify_post_frame = true;
    bot.pending_lock_verify = true;

    // T tuck landed col 3 rot 3 (meta misdrop_t_tuck_r16_c5_r0) — want col 5 rot 0.
    let (bb_wrong, _, _) = simulate_place_and_clear(&bb, 2, 3, 3, 15);
    let read_r = move |_: u16, _: u16| {
        let mut raw = vec![0u8; BOARD_ROWS * BOARD_STRIDE];
        for row in 0..BOARD_ROWS {
            let base = row * BOARD_STRIDE + 2;
            for col in 0..BOARD_COLS {
                if (bb_wrong[row] & (1 << col)) != 0 {
                    raw[base + col] = 0x81;
                }
            }
        }
        raw
    };
    bot.tick_post_frame(|_: u16| 0u8, &read_r);
    assert_eq!(
        bot.misdrop_count, 1,
        "board diff must detect tuck col/rot misdrop"
    );
}

#[test]
fn board_diff_detects_spin_col_misdrop() {
    use super::simulate_place_and_clear;

    let mut bot = TetrisBot::new();
    bot.intended_lock = Some((15, 4, 1, "spin".to_string()));
    bot.planned_path = vec!["D".into(); 16];
    bot.move_path = bot.planned_path.clone();
    bot.last_placement = Some(PlacementReplay {
        version: "1".into(),
        timestamp: String::new(),
        source: None,
        current_piece: PieceInfo {
            piece_type: 5,
            rot: 0,
            spawn_col: 3,
        },
        next_piece: NextPiece {
            piece_type: 6,
            ori: 4,
        },
        misdrop: None,
        strategy: None,
        mode: None,
        pps: None,
        note: String::new(),
    });

    let bb = [0u16; BOARD_ROWS];
    bot.lock_verify_board_before = Some(bb);
    bot.lock_verify_post_frame = true;
    bot.pending_lock_verify = true;

    // L landed col 3 rot 2 (meta: misdrop_l_spin) — want col 4 rot 1.
    let (bb_wrong, _, _) = simulate_place_and_clear(&bb, 5, 2, 3, 14);
    let read_r = move |_: u16, _: u16| {
        let mut raw = vec![0u8; BOARD_ROWS * BOARD_STRIDE];
        for row in 0..BOARD_ROWS {
            let base = row * BOARD_STRIDE + 2;
            for col in 0..BOARD_COLS {
                if (bb_wrong[row] & (1 << col)) != 0 {
                    raw[base + col] = 0x81;
                }
            }
        }
        raw
    };
    bot.tick_post_frame(|_: u16| 0u8, &read_r);
    assert_eq!(
        bot.misdrop_count, 1,
        "board diff must detect spin col/rot misdrop"
    );
}

#[test]
fn board_diff_detects_spin_row_short_two_rows() {
    use super::simulate_place_and_clear;

    let mut bot = TetrisBot::new();
    bot.intended_lock = Some((15, 3, 1, "spin".to_string()));
    bot.planned_path = vec!["D".into(); 16];
    bot.planned_path.extend(["CW", "D", "D", "D", "CCW"].iter().map(|s| s.to_string()));
    bot.move_path = bot.planned_path.clone();
    bot.last_placement = Some(PlacementReplay {
        version: "1".into(),
        timestamp: String::new(),
        source: None,
        current_piece: PieceInfo {
            piece_type: 5,
            rot: 1,
            spawn_col: 4,
        },
        next_piece: NextPiece {
            piece_type: 2,
            ori: 24,
        },
        misdrop: None,
        strategy: None,
        mode: None,
        pps: None,
        note: String::new(),
    });

    let bb = [0u16; BOARD_ROWS];
    bot.lock_verify_board_before = Some(bb);
    bot.lock_verify_post_frame = true;
    bot.pending_lock_verify = true;

    // L spin: want row 15, floor kick blocked — landed row 13, col/rot match (meta l_spin).
    let (bb_wrong, _, _) = simulate_place_and_clear(&bb, 5, 1, 3, 13);
    let read_r = move |_: u16, _: u16| {
        let mut raw = vec![0u8; BOARD_ROWS * BOARD_STRIDE];
        for row in 0..BOARD_ROWS {
            let base = row * BOARD_STRIDE + 2;
            for col in 0..BOARD_COLS {
                if (bb_wrong[row] & (1 << col)) != 0 {
                    raw[base + col] = 0x81;
                }
            }
        }
        raw
    };
    bot.tick_post_frame(|_: u16| 0u8, &read_r);
    assert_eq!(
        bot.misdrop_count, 1,
        "spin 2 rows short must misdrop via board col/row/rot check"
    );
}

#[test]
fn grounded_spin_always_schedules_board_verify() {
    let mut bot = TetrisBot::new();
    bot.intended_lock = Some((15, 3, 1, "spin".to_string()));
    bot.move_path = vec!["D".into(); 16];
    bot.target_left = 3;
    bot.target_rot = 1;

    let read = |_: u16| 0u8;
    let read_r = |_: u16, _: u16| vec![0u8; BOARD_ROWS * BOARD_STRIDE];
    let mut actions = Vec::new();
    bot.begin_drop(&read, &read_r, 0x14, &mut actions);
    assert!(
        bot.pending_lock_verify,
        "spin must always schedule board verify even when sprite col/rot match"
    );
}

#[test]
fn board_diff_detects_spin_row_short_floor_kick() {
    use super::simulate_place_and_clear;

    let mut bot = TetrisBot::new();
    bot.intended_lock = Some((16, 5, 2, "spin".to_string()));
    bot.planned_path = vec!["D".into(); 18];
    bot.planned_path.push("CCW".into());
    bot.move_path = bot.planned_path.clone();
    bot.last_placement = Some(PlacementReplay {
        version: "1".into(),
        timestamp: String::new(),
        source: None,
        current_piece: PieceInfo {
            piece_type: 3,
            rot: 0,
            spawn_col: 3,
        },
        next_piece: NextPiece {
            piece_type: 6,
            ori: 4,
        },
        misdrop: None,
        strategy: None,
        mode: None,
        pps: None,
        note: String::new(),
    });

    let bb = [0u16; BOARD_ROWS];
    bot.lock_verify_board_before = Some(bb);
    bot.lock_verify_post_frame = true;
    bot.pending_lock_verify = true;

    // S spin: want row 16, floor kick landed row 15 (too high) — col/rot match.
    let (bb_wrong, _, _) = simulate_place_and_clear(&bb, 3, 2, 5, 15);
    let read_r = move |_: u16, _: u16| {
        let mut raw = vec![0u8; BOARD_ROWS * BOARD_STRIDE];
        for row in 0..BOARD_ROWS {
            let base = row * BOARD_STRIDE + 2;
            for col in 0..BOARD_COLS {
                if (bb_wrong[row] & (1 << col)) != 0 {
                    raw[base + col] = 0x81;
                }
            }
        }
        raw
    };
    bot.tick_post_frame(|_: u16| 0u8, &read_r);
    assert_eq!(
        bot.misdrop_count, 1,
        "spin row-short (floor kick too high) must misdrop via board diff"
    );
}

#[test]
fn board_find_lock_row_i_vertical_bottom_is_anchor_not_bottom_cell() {
    use super::simulate_place_and_clear;

    let bb = [0u16; BOARD_ROWS];
    let (bb_locked, _, _) = simulate_place_and_clear(&bb, 0, 1, 0, 14);
    let gr = super::find_board_lock_row(&bb_locked, 0, 1, 0);
    assert_eq!(
        gr,
        Some(14),
        "I vertical bottom lock anchor is row 14, not bottom cell row 17"
    );
}

#[test]
fn board_diff_detects_normal_i_row_above_floor() {
    use super::simulate_place_and_clear;

    let mut bot = TetrisBot::new();
    // I horizontal (rot 0) should lock bottom row 17; floated to row 15.
    bot.intended_lock = Some((17, 3, 0, "normal".to_string()));
    bot.move_path.clear();
    bot.last_placement = Some(PlacementReplay {
        version: "1".into(),
        timestamp: String::new(),
        source: None,
        current_piece: PieceInfo {
            piece_type: 0,
            rot: 0,
            spawn_col: 3,
        },
        next_piece: NextPiece {
            piece_type: 1,
            ori: 8,
        },
        misdrop: None,
        strategy: None,
        mode: None,
        pps: None,
        note: String::new(),
    });

    let bb = [0u16; BOARD_ROWS];
    bot.lock_verify_board_before = Some(bb);
    bot.lock_verify_post_frame = true;
    bot.pending_lock_verify = true;

    let (bb_wrong, _, _) = simulate_place_and_clear(&bb, 0, 0, 3, 15);
    let read_r = move |_: u16, _: u16| {
        let mut raw = vec![0u8; BOARD_ROWS * BOARD_STRIDE];
        for row in 0..BOARD_ROWS {
            let base = row * BOARD_STRIDE + 2;
            for col in 0..BOARD_COLS {
                if (bb_wrong[row] & (1 << col)) != 0 {
                    raw[base + col] = 0x81;
                }
            }
        }
        raw
    };
    bot.tick_post_frame(|_: u16| 0u8, &read_r);
    assert_eq!(
        bot.misdrop_count, 1,
        "I horizontal above floor row must misdrop"
    );
}

#[test]
fn board_diff_passes_correct_normal_lock() {
    use super::simulate_place_and_clear;

    let mut bot = TetrisBot::new();
    bot.intended_lock = Some((15, 7, 1, "normal".to_string()));
    bot.move_path.clear();
    bot.last_placement = Some(PlacementReplay {
        version: "1".into(),
        timestamp: String::new(),
        source: None,
        current_piece: PieceInfo {
            piece_type: 1,
            rot: 1,
            spawn_col: 3,
        },
        next_piece: NextPiece {
            piece_type: 0,
            ori: 8,
        },
        misdrop: None,
        strategy: None,
        mode: None,
        pps: None,
        note: String::new(),
    });

    let bb = [0u16; BOARD_ROWS];
    bot.lock_verify_board_before = Some(bb);
    bot.lock_verify_post_frame = true;
    bot.pending_lock_verify = true;

    let (bb_locked, _, _) = simulate_place_and_clear(&bb, 1, 1, 7, 15);
    let read_r = move |_: u16, _: u16| {
        let mut raw = vec![0u8; BOARD_ROWS * BOARD_STRIDE];
        for row in 0..BOARD_ROWS {
            let base = row * BOARD_STRIDE + 2;
            for col in 0..BOARD_COLS {
                if (bb_locked[row] & (1 << col)) != 0 {
                    raw[base + col] = 0x81;
                }
            }
        }
        raw
    };
    bot.tick_post_frame(|_: u16| 0u8, &read_r);
    assert_eq!(bot.misdrop_count, 0, "correct normal lock must pass board verify");
}

#[test]
fn normal_deferred_verify_uses_post_frame_snap_not_begin_drop_row() {
    let mut bot = TetrisBot::new();
    bot.intended_lock = Some((15, 7, 1, "normal".to_string()));
    bot.move_path.clear();
    bot.last_valid_snap = Some((15, 7, 1));
    bot.pending_lock_verify = true;
    bot.lock_verify_col_rot_ok = true;
    bot.last_placement = Some(PlacementReplay {
        version: "1".into(),
        timestamp: String::new(),
        source: None,
        current_piece: PieceInfo {
            piece_type: 1,
            rot: 1,
            spawn_col: 3,
        },
        next_piece: NextPiece {
            piece_type: 0,
            ori: 8,
        },
        misdrop: None,
        strategy: None,
        mode: None,
        pps: None,
        note: String::new(),
    });

    let read = |_: u16| 0u8;
    let read_r = |_: u16, _: u16| vec![0u8; BOARD_ROWS * BOARD_STRIDE];
    bot.verify_pending_lock(&read, &read_r);
    assert_eq!(
        bot.misdrop_count, 0,
        "post-frame snap at want row must pass deferred normal verify"
    );
}

#[test]
fn normal_fast_begin_drop_defers_row_only_mismatch() {
    let mut bot = TetrisBot::new();
    bot.intended_lock = Some((15, 7, 1, "normal".to_string()));
    bot.move_path.clear();
    bot.target_left = 7;
    bot.target_rot = 1;

    let read = |_: u16| 0u8;
    let read_r = |_: u16, _: u16| vec![0u8; BOARD_ROWS * BOARD_STRIDE];
    let mut actions = Vec::new();
    bot.begin_drop(&read, &read_r, 0x18, &mut actions);
    assert!(!bot.pending_lock_verify, "matching col/rot at begin_drop needs no verify");
    assert_eq!(bot.misdrop_count, 0);
}

#[test]
fn normal_board_verify_passes_when_footprint_on_board() {
    use super::simulate_place_and_clear;

    let mut bot = TetrisBot::new();
    bot.intended_lock = Some((15, 7, 1, "normal".to_string()));
    bot.move_path.clear();
    bot.last_placement = Some(PlacementReplay {
        version: "1".into(),
        timestamp: String::new(),
        source: None,
        current_piece: PieceInfo {
            piece_type: 1,
            rot: 1,
            spawn_col: 3,
        },
        next_piece: NextPiece {
            piece_type: 0,
            ori: 8,
        },
        misdrop: None,
        strategy: None,
        mode: None,
        pps: None,
        note: String::new(),
    });
    bot.pending_lock_verify = true;

    let bb = [0u16; BOARD_ROWS];
    let (bb_locked, _, _) = simulate_place_and_clear(&bb, 1, 1, 7, 15);
    let read_r = move |_: u16, _: u16| {
        let mut raw = vec![0u8; BOARD_ROWS * BOARD_STRIDE];
        for row in 0..BOARD_ROWS {
            let base = row * BOARD_STRIDE + 2;
            for col in 0..BOARD_COLS {
                if (bb_locked[row] & (1 << col)) != 0 {
                    raw[base + col] = 0x81;
                }
            }
        }
        raw
    };
    bot.verify_normal_fast_lock_from_board(&read_r);
    assert_eq!(bot.misdrop_count, 0, "board footprint match must not misdrop");
}

#[test]
fn normal_board_verify_fires_on_real_row_misdrop() {
    use super::simulate_place_and_clear;

    let mut bot = TetrisBot::new();
    bot.intended_lock = Some((15, 7, 1, "normal".to_string()));
    bot.move_path.clear();
    bot.last_placement = Some(PlacementReplay {
        version: "1".into(),
        timestamp: String::new(),
        source: None,
        current_piece: PieceInfo {
            piece_type: 1,
            rot: 1,
            spawn_col: 3,
        },
        next_piece: NextPiece {
            piece_type: 0,
            ori: 8,
        },
        misdrop: None,
        strategy: None,
        mode: None,
        pps: None,
        note: String::new(),
    });
    bot.pending_lock_verify = true;

    let bb = [0u16; BOARD_ROWS];
    let (bb_locked, _, _) = simulate_place_and_clear(&bb, 1, 1, 7, 14);
    let read_r = move |_: u16, _: u16| {
        let mut raw = vec![0u8; BOARD_ROWS * BOARD_STRIDE];
        for row in 0..BOARD_ROWS {
            let base = row * BOARD_STRIDE + 2;
            for col in 0..BOARD_COLS {
                if (bb_locked[row] & (1 << col)) != 0 {
                    raw[base + col] = 0x81;
                }
            }
        }
        raw
    };
    bot.verify_normal_fast_lock_from_board(&read_r);
    assert_eq!(bot.misdrop_count, 1, "wrong board row must still misdrop");
}

#[test]
fn tuck_grounded_lock_defers_row_not_col_rot() {
    let bb = [0u16; BOARD_ROWS];
    let mut bot = TetrisBot::new();
    bot.target_left = 1;
    bot.target_rot = 0;
    bot.intended_lock = Some((16, 1, 0, "tuck".to_string()));
    bot.move_path = vec!["D".into(), "L".into()];
    bot.last_valid_snap = Some((15, 1, 0));

    let read = |_: u16| 0u8;
    let read_r = |_: u16, _: u16| vec![0u8; BOARD_ROWS * BOARD_STRIDE];
    // min_row=15 grounded, want row 16 col 1 rot 0 — col/rot match, row differs.
    let mut actions = Vec::new();
    bot.begin_drop(&read, &read_r, 0x18, &mut actions);
    assert!(
        bot.pending_lock_verify,
        "tuck with matching col/rot but row off-by-one must defer, not misdrop"
    );
    assert_eq!(bot.misdrop_count, 0);
}

#[test]
#[ignore = "awaiting fresh fixture capture"]
fn s_to_l_bfs_misses_ccw_before_r_path() {
    use crate::state::EmulatorState;
    use base64::Engine;

    let b64 = std::fs::read_to_string(
        super::fixtures::misdrop_fixture("s_to_l_misdrop_state.b64"),
    )
    .unwrap();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim().trim_matches('"'))
        .unwrap();
    let state: EmulatorState = bincode::deserialize(&bytes).unwrap();
    let bb = read_board_bitboard_from_ram(&state.ram);

    let rccw: Vec<String> = std::iter::repeat("D")
        .take(14)
        .map(|s| s.to_string())
        .chain(
            ["R", "CCW", "D", "D", "D", "CCW"]
                .iter()
                .map(|s| s.to_string()),
        )
        .collect();
    let ccr: Vec<String> = std::iter::repeat("D")
        .take(14)
        .map(|s| s.to_string())
        .chain(
            ["CCW", "R", "D", "D", "D", "CCW"]
                .iter()
                .map(|s| s.to_string()),
        )
        .collect();
    let user: Vec<String> = std::iter::repeat("D")
        .take(14)
        .map(|s| s.to_string())
        .chain(
            ["CCW", "R", "D", "D", "D", "R", "CCW"]
                .iter()
                .map(|s| s.to_string()),
        )
        .collect();

    let r_ok = bfs_path_is_reachable(&bb, 3, 0, 3, 0, &rccw);
    let c_ok = bfs_path_is_reachable(&bb, 3, 0, 3, 0, &ccr);
    let u_ok = bfs_path_is_reachable(&bb, 3, 0, 3, 0, &user);
    eprintln!("bfs_reachable R,CCW={r_ok} CCW,R={c_ok} user(CCWR,DDDR,CCW)={u_ok}");
    assert!(r_ok);
    eprintln!(
        "sim R,CCW {:?}",
        simulate_path_prefix(&bb, 3, 0, 3, 0, &rccw)
    );
    eprintln!(
        "sim CCW,R {:?}",
        simulate_path_prefix(&bb, 3, 0, 3, 0, &ccr)
    );

    let moves = bfs_moves(&bb, 3, 0, 3, 0);
    let hits: Vec<_> = moves
        .iter()
        .filter(|m| m.col == 5 && m.rot == 2 && m.row == 16)
        .collect();
    eprintln!("{} BFS locks at (16,5,r2)", hits.len());
    for m in &hits {
        eprintln!("  {:?}", m.path);
    }
}

#[test]
fn ori_info_works() {
    assert_eq!(ori_info(0x00), Some((5, 0))); // L spawn
    assert_eq!(ori_info(0x03), Some((5, 3)));
    assert_eq!(ori_info(0x08), Some((0, 0))); // I
    assert_eq!(ori_info(0x1B), Some((2, 3))); // T last rot
    assert_eq!(ori_info(0xFF), None);
}

#[test]
fn shapes_have_correct_lengths() {
    for t in 0..7 {
        for r in 0..4 {
            assert_eq!(SHAPES[t][r].len(), 4);
        }
    }
}

#[test]
fn spin_final_rot_waits_on_row_not_lateral_suffix() {
    let path: Vec<String> = (0..3)
        .map(|_| "D".to_string())
        .chain(["CW".to_string(), "L".to_string()])
        .collect();
    assert!(TetrisBot::path_suffix_is_lateral_only(&path, 3));
    assert!(!TetrisBot::path_suffix_is_lateral_only(&path, 4));
    // Terminal spin CCW (nothing after) must NOT trigger row-wait via empty suffix.
    let spin: Vec<String> = (0..20)
        .map(|_| "D".to_string())
        .chain(["R".to_string(), "CCW".to_string(), "D".to_string(), "CCW".to_string()])
        .collect();
    assert!(!TetrisBot::path_suffix_is_lateral_only(&spin, spin.len() - 1));
}

#[test]
fn s_to_t_replay_path_matches_bfs() {
    let mut bb = [0u16; BOARD_ROWS];
    bb[15] = 896;
    bb[16] = 927;
    bb[17] = 911;

    let moves = bfs_moves(&bb, 3, 0, 4, 1);
    let target = moves
        .iter()
        .find(|m| m.col == 4 && m.rot == 2)
        .expect("S lock col4 rot2");
    let late = bfs_moves(&bb, 3, 5, 4, 1)
        .into_iter()
        .find(|m| m.col == 4 && m.rot == 2)
        .expect("row-5 plan");
    // Floor SRS: path from row 5 may tuck earlier (R before final CW) vs naive D×9,CW.
    let mut r = 5i32;
    let mut c = 4i32;
    let mut rot = 1usize;
    for act in &late.path {
        match act.as_str() {
            "D" => r += 1,
            "L" => c -= 1,
            "R" => c += 1,
            "CW" => {
                let got = srs::srs_try_rotate_auto(&bb, 3, r, c, rot, true);
                assert!(got.is_some(), "CW at ({r},{c},r{rot})");
                let (nr, nc, nrot) = got.unwrap();
                r = nr;
                c = nc;
                rot = nrot;
            }
            "CCW" => {
                let got = srs::srs_try_rotate_auto(&bb, 3, r, c, rot, false);
                assert!(got.is_some(), "CCW at ({r},{c},r{rot})");
                let (nr, nc, nrot) = got.unwrap();
                r = nr;
                c = nc;
                rot = nrot;
            }
            _ => {}
        }
    }
    assert_eq!((r, c, rot), (late.row, late.col as i32, late.rot));
    let plan_row = plan_row_before_final_action(&bb, 3, 5, 4, 1, &late.path, late.row, "spin", 2);
    assert!(
        plan_row == 12 || plan_row == 13,
        "floor SRS spin milestone: got {plan_row} path={:?}",
        late.path
    );

    let mut r2 = 0i32;
    let mut c2 = 4i32;
    let mut rot2 = 1usize;
    for act in &target.path {
        match act.as_str() {
            "D" => r2 += 1,
            "L" => c2 -= 1,
            "R" => c2 += 1,
            "CW" => {
                let got = srs::srs_try_rotate_auto(&bb, 3, r2, c2, rot2, true);
                assert!(got.is_some(), "CW at ({r2},{c2},r{rot2})");
                let (nr, nc, nrot) = got.unwrap();
                r2 = nr;
                c2 = nc;
                rot2 = nrot;
            }
            "CCW" => {
                let got = srs::srs_try_rotate_auto(&bb, 3, r2, c2, rot2, false);
                assert!(got.is_some(), "CCW at ({r2},{c2},r{rot2})");
                let (nr, nc, nrot) = got.unwrap();
                r2 = nr;
                c2 = nc;
                rot2 = nrot;
            }
            _ => {}
        }
    }
    assert_eq!((r2, c2, rot2), (target.row, target.col as i32, target.rot));
}

#[test]
fn srs_kick_col_sync_does_not_undo_planned_kick() {
    // Z CCW at (6,3,r1) kicks left to col 2 (BFS-intended). Old post_rot_sync
    // inserted R because path_expected_col was still 3 — misdrop +1 col right.
    let path_expected_col = 3usize;
    let actual_col = 2usize;
    let il_col = 2i32;
    let delta_vs_expected = actual_col as i32 - path_expected_col as i32;
    assert_eq!(delta_vs_expected, -1, "kick left vs in-place expectation");
    let delta_vs_intended = il_col - actual_col as i32;
    assert_eq!(delta_vs_intended, 0, "no compensation toward intended lock col");
}

#[test]
fn tuck_terminal_descent_suffix_detection() {
    let path: Vec<String> = ["D", "D", "D", "R"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert!(TetrisBot::path_suffix_is_d_then_terminal_lateral(&path, 0));
    assert!(!TetrisBot::path_suffix_is_d_then_terminal_lateral(&path, 3));
    let misdrop: Vec<String> = [
        "D", "D", "D", "D", "D", "D", "D", "D", "L", "L", "L", "D", "D", "D", "R",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert!(TetrisBot::path_suffix_is_d_then_terminal_lateral(&misdrop, 11));
    assert!(!TetrisBot::path_suffix_is_d_then_terminal_lateral(&misdrop, 8));
}

#[test]
#[ignore = "awaiting fresh fixture capture"]
fn tuck_gravity_sync_advances_d_steps_before_terminal_r() {
    use base64::Engine;
    use crate::state::EmulatorState;

    let b64 = std::fs::read_to_string(
        super::fixtures::misdrop_fixture("t_to_o_misdrop_state.b64"),
    )
    .unwrap()
    .trim()
    .trim_matches('"')
    .to_string();
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64).unwrap();
    let state: EmulatorState = bincode::deserialize(&bytes).unwrap();
    let bb = read_board_bitboard_from_ram(&state.ram);

    let path: Vec<String> = [
        "D", "D", "D", "D", "D", "D", "D", "D", "L", "L", "L", "D", "D", "D", "R",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    let mut bot = TetrisBot::new();
    bot.move_path = path;
    bot.path_start_row = 5;
    bot.path_start_col = 3;
    bot.path_start_rot = 0;
    bot.path_step = 11;
    bot.path_commit_row = 13;
    bot.path_commit_col = 0;
    bot.path_commit_rot = 0;
    bot.intended_lock = Some((16, 1, 0, "tuck".to_string()));
    bot.plan_intended_row = 16;

    // Gravity must not consume well D's — path_step stays until plan row skip.
    bot.sync_tuck_path_step(&bb, 2, 15, 0, 0);
    assert_eq!(
        bot.path_step, 11,
        "passive gravity must not advance path_step on D-prefix"
    );

    bot.sync_tuck_path_step(&bb, 2, 16, 0, 0);
    assert_eq!(bot.path_step, 14, "at tuck row skip remaining D's to terminal R");
}

#[test]
#[ignore = "awaiting fresh fixture capture"]
fn tuck_sync_skips_well_d_when_gravity_reaches_plan_row_first() {
    use base64::Engine;
    use crate::state::EmulatorState;

    let b64 = std::fs::read_to_string(
        super::fixtures::misdrop_fixture("t_to_o_misdrop_state.b64"),
    )
    .unwrap()
    .trim()
    .trim_matches('"')
    .to_string();
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64).unwrap();
    let state: EmulatorState = bincode::deserialize(&bytes).unwrap();
    let bb = read_board_bitboard_from_ram(&state.ram);

    let path: Vec<String> = [
        "D", "D", "D", "D", "D", "D", "D", "D", "L", "L", "L", "D", "D", "D", "R",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    let mut bot = TetrisBot::new();
    bot.move_path = path;
    bot.path_start_row = 5;
    bot.path_start_col = 3;
    bot.path_start_rot = 0;
    bot.path_step = 12;
    bot.path_commit_row = 15;
    bot.path_commit_col = 0;
    bot.path_commit_rot = 0;
    bot.intended_lock = Some((16, 1, 0, "tuck".to_string()));
    bot.plan_intended_row = 16;

    // Gravity beat us to row 16 while path_step still on a well D.
    bot.sync_tuck_path_step(&bb, 2, 16, 0, 0);
    assert_eq!(
        bot.path_step, 14,
        "must skip to terminal R, never leave path_step on a D at plan row"
    );
    assert_eq!(bot.move_path[bot.path_step], "R");
}

#[test]
#[ignore = "awaiting fresh fixture capture"]
fn tuck_sync_catches_gravity_during_lateral_prefix() {
    use base64::Engine;
    use crate::state::EmulatorState;

    let b64 = std::fs::read_to_string(
        super::fixtures::misdrop_fixture("t_to_o_misdrop_state.b64"),
    )
    .unwrap()
    .trim()
    .trim_matches('"')
    .to_string();
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64).unwrap();
    let state: EmulatorState = bincode::deserialize(&bytes).unwrap();
    let bb = read_board_bitboard_from_ram(&state.ram);

    let path: Vec<String> = [
        "D", "D", "D", "D", "D", "D", "D", "D", "L", "L", "L", "D", "D", "D", "R",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    let mut bot = TetrisBot::new();
    bot.move_path = path.clone();
    bot.path_start_row = 5;
    bot.path_start_col = 3;
    bot.path_start_rot = 0;
    bot.path_step = 8;
    bot.path_commit_row = 13;
    bot.path_commit_col = 3;
    bot.path_commit_rot = 0;
    bot.path_pending_action = None;
    bot.intended_lock = Some((16, 1, 0, "tuck".to_string()));
    bot.plan_intended_row = 16;

    // Stale L pending cleared; gravity brought piece to lock row before L's finished.
    bot.path_pending_action = Some("L".into());
    let mut actions = Vec::new();
    bot.try_confirm_pending_path_step(&mut actions, &bb, 2, 16, 0, 0);
    assert_eq!(bot.path_pending_action, None, "stale lateral pending cleared");
    bot.sync_tuck_path_step(&bb, 2, 16, 0, 0);
    assert_eq!(
        bot.path_step, 8,
        "lateral steps are not fast-forwarded — each L/R needs its own tap"
    );
}

#[test]
fn resync_does_not_skip_consecutive_lateral_steps() {
    let bb = [0u16; BOARD_ROWS];
    let mut bot = TetrisBot::new();
    bot.move_path = vec!["R".into(), "R".into(), "D".into()];
    bot.path_start_row = 14;
    bot.path_start_col = 2;
    bot.path_start_rot = 0;
    bot.path_step = 1;
    bot.path_commit_row = 14;
    bot.path_commit_col = 3;
    bot.path_commit_rot = 0;
    // DAS landed at col 4 (two R's in sim) after only one confirmed tap.
    let jumped = bot.resync_path_step_to_actual(&bb, 2, 14, 4, 0);
    assert!(!jumped, "resync must not skip the second R on col drift");
    assert_eq!(bot.path_step, 1);
}

#[test]
fn sync_tuck_does_not_skip_consecutive_lateral_steps() {
    let bb = [0u16; BOARD_ROWS];
    let mut bot = TetrisBot::new();
    bot.move_path = vec!["R".into(), "R".into(), "D".into()];
    bot.path_start_row = 14;
    bot.path_start_col = 2;
    bot.path_start_rot = 0;
    bot.path_step = 0;
    bot.path_commit_row = 14;
    bot.path_commit_col = 2;
    bot.path_commit_rot = 0;
    bot.intended_lock = Some((15, 4, 0, "tuck".to_string()));
    bot.plan_intended_row = 15;
    // Sim matches two R's at col 4, but only the first tap was sent.
    bot.sync_tuck_path_step(&bb, 2, 14, 4, 0);
    assert_eq!(
        bot.path_step, 0,
        "must not skip the second R when DAS/sim absorbed two columns in one press"
    );
}

#[test]
fn lateral_confirm_sets_settle_before_consecutive_lateral() {
    let bb = [0u16; BOARD_ROWS];
    let mut bot = TetrisBot::new();
    bot.move_path = vec!["L".into(), "L".into(), "D".into()];
    bot.path_start_row = 12;
    bot.path_start_col = 3;
    bot.path_start_rot = 0;
    bot.path_pending_action = Some("L".into());
    bot.path_commit_row = 12;
    bot.path_commit_col = 3;
    bot.path_commit_rot = 0;
    bot.path_held_btn = Some((6, MIN_BTN_HOLD_FRAMES));
    let mut actions = Vec::new();
    bot.try_confirm_pending_path_step(&mut actions, &bb, 6, 12, 2, 0);
    assert_eq!(bot.path_step, 1);
    assert_eq!(
        bot.lateral_settle_frames, LATERAL_CHAIN_SETTLE_FRAMES,
        "consecutive L must pause so DAS sees a fresh tap"
    );
}

#[test]
fn lateral_confirm_releases_path_btn_hold() {
    let bb = [0u16; BOARD_ROWS];
    let mut bot = TetrisBot::new();
    bot.move_path = vec!["R".into(), "CCW".into()];
    bot.path_start_row = 14;
    bot.path_start_col = 3;
    bot.path_start_rot = 0;
    bot.path_pending_action = Some("R".into());
    bot.path_commit_row = 14;
    bot.path_commit_col = 3;
    bot.path_commit_rot = 0;
    bot.path_held_btn = Some((7, MIN_BTN_HOLD_FRAMES));
    let mut actions = Vec::new();
    bot.try_confirm_pending_path_step(&mut actions, &bb, 3, 14, 4, 0);
    assert!(bot.path_held_btn.is_none(), "lateral confirm must end hold");
    assert!(
        actions.iter().any(|&(b, d)| b == 7 && !d),
        "R release must be in same-frame actions"
    );
    assert_eq!(bot.path_step, 1);
    assert_eq!(
        bot.lateral_settle_frames, 3,
        "must wait before tuck CCW after R"
    );
}

#[test]
fn implicit_descent_confirms_row_without_advancing_path_step() {
    let bb = [0u16; BOARD_ROWS];
    let mut bot = TetrisBot::new();
    bot.move_path = vec!["CW".to_string()];
    bot.path_step = 0;
    bot.path_commit_row = 10;
    bot.path_commit_col = 4;
    bot.path_commit_rot = 1;
    bot.plan_intended_row = 14;
    bot.holding_down = true;
    bot.path_down_min_frames = MIN_BTN_HOLD_FRAMES;
    bot.path_pending_action = Some(IMPLICIT_DESCENT.to_string());

    let mut actions = Vec::new();
    bot.try_confirm_pending_path_step(&mut actions, &bb, 3, 11, 4, 1);
    assert_eq!(bot.path_step, 0, "implicit descent must not advance path_step");
    assert!(bot.path_pending_action.is_none());
    assert_eq!(bot.path_commit_row, 11);
    assert!(bot.path_down_release_armed, "release waits for min hold");
    assert!(bot.holding_down, "Down still held until min frames elapse");
}

#[test]
fn grounded_path_d_skips_soft_drop_tap() {
    let mut bb = [0u16; BOARD_ROWS];
    bb[15] = 0b1111000111;
    bb[16] = 0b1000001111;
    bb[17] = 0b1110011111;
    let path: Vec<String> = (0..9)
        .map(|_| "D".to_string())
        .chain(["R".to_string()])
        .collect();
    let mut bot = TetrisBot::new();
    bot.move_path = path;
    bot.path_step = 8;
    bot.path_start_row = 5;
    bot.path_start_col = 3;
    bot.path_start_rot = 0;
    bot.path_commit_row = 13;
    bot.path_commit_col = 3;
    bot.path_commit_rot = 0;
    bot.intended_lock = Some((16, 5, 2, "spin".to_string()));
    bot.plan_intended_row = 16;
    bot.state = BotState::Path;
    bot.path_pending_action = None;
    bot.holding_down = false;

    let mut actions = Vec::new();
    // S→L: at row 14 grounded, must cancel any held Down before R/CCW tuck chain.
    bot.cancel_soft_drop_if_grounded(&mut actions, &bb, 3, 14, 3, 0);
    assert_eq!(actions, vec![(5, false)]);
    assert!(!bot.holding_down);
    bot.holding_down = true;
    let mut actions2 = Vec::new();
    bot.cancel_soft_drop_if_grounded(&mut actions2, &bb, 3, 14, 3, 0);
    assert_eq!(actions2, vec![(5, false)]);

    assert!(TetrisBot::soft_drop_lands_grounded(&bb, 3, 0, 13, 3));
    assert!(!TetrisBot::soft_drop_lands_grounded(&bb, 3, 0, 12, 3));
}

#[test]
fn path_soft_drop_releases_immediately_on_row_confirm() {
    let mut bot = TetrisBot::new();
    bot.move_path = vec!["D".to_string(); 4];
    bot.path_step = 0;
    bot.path_start_row = 5;
    bot.path_start_col = 3;
    bot.path_start_rot = 0;
    let mut actions = Vec::new();
    bot.begin_path_soft_drop(&mut actions);
    assert_eq!(actions, vec![(5, true)]);
    bot.path_pending_action = Some("D".into());
    bot.path_commit_row = 5;
    bot.path_commit_col = 3;
    bot.path_commit_rot = 0;

    let bb = [0u16; BOARD_ROWS];
    bot.try_confirm_pending_path_step(&mut actions, &bb, 3, 6, 3, 0);
    assert_eq!(bot.path_step, 1);
    assert!(!bot.holding_down, "Down must release on row confirm");
    assert!(
        actions.iter().any(|&(b, d)| b == 5 && !d),
        "Down release must be in same-frame actions"
    );
}

#[test]
fn lands_grounded_d_waits_for_gravity_not_down() {
    let mut bb = [0u16; BOARD_ROWS];
    bb[15] = 0b1111000111;
    bb[16] = 0b1000001111;
    bb[17] = 0b1110011111;
    assert!(TetrisBot::soft_drop_lands_grounded(&bb, 3, 0, 13, 3));
}

#[test]
fn confirm_only_d_step_ignores_passive_gravity() {
    let mut bb = [0u16; BOARD_ROWS];
    bb[15] = 391;
    bb[16] = 963;
    bb[17] = 967;
    let path: Vec<String> = (0..15)
        .map(|_| "D".to_string())
        .chain(["CW".to_string(), "D".to_string(), "CCW".to_string()])
        .collect();
    // max_matching would jump to 11 after passive gravity; confirm-only stays at 9.
    assert_eq!(max_matching_path_step(&bb, 0, 0, 3, 0, &path, 11, 3, 0), 11);
    let mut bot = TetrisBot::new();
    bot.move_path = path;
    bot.path_start_row = 0;
    bot.path_start_col = 3;
    bot.path_start_rot = 0;
    bot.path_step = 9;
    bot.path_commit_row = 8;
    bot.path_commit_col = 3;
    bot.path_commit_rot = 0;
    bot.path_pending_action = Some("D".into());
    let mut actions = Vec::new();
    bot.try_confirm_pending_path_step(&mut actions, &bb, 0, 9, 3, 0);
    assert_eq!(bot.path_step, 10, "one D tap confirms one step");
    bot.path_pending_action = None;
    bot.try_confirm_pending_path_step(&mut actions, &bb, 0, 11, 3, 0);
    assert_eq!(bot.path_step, 10, "gravity alone must not advance path_step");
}

#[test]
fn max_matching_path_step_syncs_d_prefix_and_rewinds() {
    let mut bb = [0u16; BOARD_ROWS];
    bb[15] = 391;
    bb[16] = 963;
    bb[17] = 967;
    let path: Vec<String> = (0..15)
        .map(|_| "D".to_string())
        .chain(["CW".to_string(), "D".to_string(), "CCW".to_string()])
        .collect();
    // Only 9 D's worth of descent — must not be at step 15 (I→J failure mode)
    assert_eq!(max_matching_path_step(&bb, 0, 0, 3, 0, &path, 9, 3, 0), 9);
    // Gravity-free: fell to row 11 without 11 taps
    assert_eq!(max_matching_path_step(&bb, 0, 0, 3, 0, &path, 11, 3, 0), 11);
}

#[test]
fn row_before_final_action_counts_d_steps() {
    let path: Vec<String> = (0..3)
        .map(|_| "D".to_string())
        .chain(["CW".to_string()])
        .collect();
    assert_eq!(row_before_final_action(5, &path), Some(8));
}

#[test]
fn popcount_basic() {
    assert_eq!(popcount(0b0), 0);
    assert_eq!(popcount(0b1010), 2);
    assert_eq!(popcount(0b1111111111), 10); // full row (10 cols)
}

#[test]
fn is_occupied_logic() {
    assert!(is_occupied(0x80));
    assert!(is_occupied(0x8F));
    assert!(!is_occupied(0x8E)); // wall
    assert!(!is_occupied(0x00));
    assert!(!is_occupied(0x70));
}

#[test]
fn column_heights_example() {
    let mut bb = [0u16; BOARD_ROWS];
    bb[17] = 0b0000_0000_01; // bottom row, col 0 filled
    bb[16] = 0b0000_0000_11; // col 0 and 1

    let h = column_heights(&bb);
    assert_eq!(h[0], 2);
    assert_eq!(h[1], 2);
    assert_eq!(h[2], 0);
}

#[test]
#[ignore = "awaiting fresh fixture capture"]
fn t_to_o_misdrop_v2_replay_analysis() {
    use crate::bot::srs::srs_try_rotate_auto;
    use crate::state::EmulatorState;
    use base64::Engine;

    let b64 = std::fs::read_to_string(
        super::fixtures::misdrop_fixture("t_to_o_misdrop_state.b64"),
    )
    .expect("t_to_o_misdrop_state.b64");
    let b64 = b64.trim().trim_matches('"');
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .expect("base64");
    let state: EmulatorState = bincode::deserialize(&bytes).expect("state");
    let bb = read_board_bitboard_from_ram(&state.ram);

    let type_t = 2usize;
    let spawn_col = 3usize;
    let spawn_rot = 0usize;
    let path: Vec<String> = [
        "D", "D", "D", "D", "D", "D", "D", "D", "L", "L", "L", "D", "D", "D", "R",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    let moves = bfs_moves(&bb, type_t, 0, spawn_col, spawn_rot);
    let moves5 = bfs_moves(&bb, type_t, 5, spawn_col, spawn_rot);
    eprintln!("BFS moves from spawn0: {} spawn5: {}", moves.len(), moves5.len());
    if let Some(m) = moves5.iter().find(|m| m.col == 1 && m.rot == 0 && m.row == 16) {
        eprintln!("BFS row5 want path {:?} len={}", m.path, m.path.len());
    }
    for m in moves.iter().filter(|m| m.col <= 2 && m.rot == 0) {
        eprintln!(
            "  col{} row{} path={:?} classify={}",
            m.col,
            m.row,
            m.path,
            classify_move(&bb, type_t, m.row, m.col, m.rot, &m.path, 0)
        );
    }

    let winner = moves
        .iter()
        .find(|m| m.col == 1 && m.rot == 0 && m.row == 16);
    eprintln!("BFS has want (16,1,r0): {:?}", winner.map(|m| &m.path));

    // Simulate misdrop path from spawn row 5 (typical when plan() runs)
    let mut r = 5i32;
    let mut c = spawn_col as i32;
    let mut rot = spawn_rot;
    for (i, act) in path.iter().enumerate() {
        let grounded = piece_collides(&bb, type_t, rot, r + 1, c);
        eprintln!("  step {i} pre=({r},{c},r{rot}) grounded={grounded} act={act}");
        match act.as_str() {
            "D" => r += 1,
            "L" => c -= 1,
            "R" => c += 1,
            "CW" => {
                let got = srs_try_rotate_auto(&bb, type_t, r, c, rot, true);
                eprintln!("  step {i} CW at ({r},{c},r{rot}) => {got:?}");
                if let Some((nr, nc, nrot)) = got {
                    r = nr;
                    c = nc;
                    rot = nrot;
                }
            }
            "CCW" => {
                let got = srs_try_rotate_auto(&bb, type_t, r, c, rot, false);
                if let Some((nr, nc, nrot)) = got {
                    r = nr;
                    c = nc;
                    rot = nrot;
                }
            }
            _ => {}
        }
    }
    eprintln!("path sim end ({r},{c},r{rot}) want (16,1,r0)");

    let mtype = classify_move(&bb, type_t, 16, 1, 0, &path, 0);
    let plan_row = plan_row_before_final_action(
        &bb, type_t, 5, spawn_col as i32, spawn_rot, &path, 16, mtype, 0,
    );
    eprintln!("classify={mtype} plan_intended_row(from row5)={plan_row}");

    // After prefix before final R
    let mut r2 = 0i32;
    let mut c2 = spawn_col as i32;
    for act in &path[..path.len() - 1] {
        match act.as_str() {
            "D" => r2 += 1,
            "L" => c2 -= 1,
            "R" => c2 += 1,
            _ => {}
        }
    }
    eprintln!("pre-final-R sim ({r2},{c2},r0) can_R={}", !piece_collides(&bb, type_t, 0, r2, c2 + 1));

    assert!(
        moves.iter().any(|m| m.col == 1 && m.rot == 0 && m.row == 16),
        "BFS should reach tuck target col1 row16"
    );
    assert_eq!((r, c, rot), (16, 1, 0), "misdrop path from row5 should sim to col1");

}

fn read_board_bitboard_from_ram(ram: &[u8]) -> [u16; BOARD_ROWS] {
    let mut bb = [0u16; BOARD_ROWS];
    for row in 0..BOARD_ROWS {
        let base = 0x800 + row * BOARD_STRIDE + 2;
        let mut bits = 0u16;
        for col in 0..BOARD_COLS {
            let v = ram.get(base + col).copied().unwrap_or(0);
            if is_occupied(v) {
                bits |= 1 << col;
            }
        }
        bb[row] = bits;
    }
    bb
}

fn read_board_bitboard_from_emulator(emu: &crate::emulator::Emulator) -> [u16; BOARD_ROWS] {
    read_board_bitboard(|addr, len| {
        (0..len)
            .map(|i| emu.memory.read(addr.wrapping_add(i as u16)))
            .collect()
    })
}

#[test]
fn placement_replay_serde_roundtrip() {
    // Note: board serialization removed. Replay JSON is now small metadata only.
    let replay = PlacementReplay {
        version: "1".to_string(),
        timestamp: "2026-06-30T20:00:00Z".to_string(),
        source: Some("test".to_string()),
        current_piece: PieceInfo { piece_type: 2, rot: 1, spawn_col: 3 },
        next_piece: NextPiece { piece_type: 5, ori: 0x00 },
        misdrop: Some(MisdropContext {
            num: 3,
            total: 42,
            wanted_col: 2,
            wanted_rot: 1,
            wanted_row: Some(14),
            actual_col: 3,
            actual_rot: 1,
            actual_row: Some(14),
            got_valid: true,
            move_type: "tuck".to_string(),
            path_len: 5,
            path: Some(vec!["CW".into(), "L".into(), "D".into()]),
            critical: false,
        }),
        strategy: Some("meatfighter".to_string()),
        mode: Some("2ply-bfs".to_string()),
        pps: Some("inf".to_string()),
        note: "interesting T tuck case".to_string(),
    };

    let json = replay.to_json().expect("serialize");
    assert!(json.contains("\"current_piece\""));
    assert!(json.contains("2ply-bfs"));
    // No board in the JSON anymore
    assert!(!json.contains("\"board\""));

    let restored: PlacementReplay = PlacementReplay::from_json(&json).expect("deserialize");
    assert_eq!(replay, restored);
}

fn emu_piece_pos(emu: &crate::emulator::Emulator) -> (i32, usize, usize) {
    let read = |a: u16| emu.memory.read(a);
    (
        piece_min_row(&read),
        piece_left_col(&read),
        ori_info(read(ADDR_CUR_ORI))
            .map(|i| i.1 as usize)
            .unwrap_or(99),
    )
}

/// Execute `path` on an already-loaded Rosy emu (step-confirm semantics).
fn emu_run_path_on(emu: &mut crate::emulator::Emulator, path: &[&str]) -> (i32, usize, usize) {
    use crate::emulator::joypad::GbButton;

    const MAX_STEP_FRAMES: u32 = 180;
    const SETTLE_FRAMES: u32 = 4;

    for act in path {
        let (r0, c0, rot0) = emu_piece_pos(&emu);
        match *act {
            "D" => {
                // One row per D-step: brief tap (PATH_DOWN_HOLD_FRAMES), then wait for +1 row.
                emu.joypad.press(GbButton::Down);
                for _ in 0..PATH_DOWN_HOLD_FRAMES {
                    emu.run_frame();
                }
                emu.joypad.release(GbButton::Down);
                let mut frames = 0u32;
                while frames < MAX_STEP_FRAMES {
                    let (r, c, rot) = emu_piece_pos(&emu);
                    if r > r0 && c == c0 && rot == rot0 {
                        break;
                    }
                    emu.run_frame();
                    frames += 1;
                }
            }
            "L" | "R" => {
                let btn = if *act == "L" {
                    GbButton::Left
                } else {
                    GbButton::Right
                };
                emu.joypad.press(btn);
                for _ in 0..MIN_BTN_HOLD_FRAMES {
                    emu.run_frame();
                }
                let mut frames = 0u32;
                loop {
                    let (_, c, rot) = emu_piece_pos(&emu);
                    let moved = match *act {
                        "L" => (c as i32) < c0 as i32,
                        "R" => (c as i32) > c0 as i32,
                        _ => false,
                    };
                    if moved && rot == rot0 {
                        break;
                    }
                    if frames >= MAX_STEP_FRAMES {
                        break;
                    }
                    emu.run_frame();
                    frames += 1;
                }
                emu.joypad.release(btn);
            }
            "CW" | "CCW" => {
                let btn = if *act == "CW" {
                    GbButton::A
                } else {
                    GbButton::B
                };
                let expected_rot = if *act == "CW" {
                    (rot0 + 1) % 4
                } else {
                    (rot0 + 3) % 4
                };
                let mut frames = 0u32;
                loop {
                    emu.joypad.press(btn);
                    emu.run_frame();
                    let (_, _, rot) = emu_piece_pos(&emu);
                    if rot == expected_rot {
                        break;
                    }
                    frames += 1;
                    if frames >= MAX_STEP_FRAMES {
                        break;
                    }
                }
                emu.joypad.release(btn);
                for _ in 0..SETTLE_FRAMES {
                    emu.run_frame();
                }
            }
            _ => {}
        }
    }

    // Brief settle only — long gravity tails desync prefix checks and open-loop paths.
    for _ in 0..SETTLE_FRAMES {
        emu.run_frame();
    }
    emu_piece_pos(emu)
}

/// Execute `path` on Rosy emu with step-confirm semantics (one D = one row, L/R/CW/CCW
/// confirmed by position change). Matches TetrisBot path execution, not fixed 8-frame taps.
fn emu_run_path(state_b64: &str, path: &[&str]) -> (i32, usize, usize) {
    use crate::bot::fixtures::emulator_from_savestate;
    use crate::state::EmulatorState;
    use base64::Engine;

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(state_b64.trim().trim_matches('"'))
        .unwrap();
    let state: EmulatorState = bincode::deserialize(&bytes).unwrap();
    let mut emu = emulator_from_savestate(&state);
    emu_run_path_on(&mut emu, path)
}

/// cargo test j_to_i_2_false_positive -- --nocapture
#[test]
#[ignore = "awaiting fresh fixture capture"]
fn j_to_i_2_false_positive_probe() {
    use base64::Engine;
    use crate::state::EmulatorState;

    let b64 = std::fs::read_to_string(
        super::fixtures::misdrop_fixture("J_to_I_2_misdrop_state.b64"),
    )
    .unwrap();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim().trim_matches('"'))
        .unwrap();
    let state: EmulatorState = bincode::deserialize(&bytes).unwrap();
    let bb = read_board_bitboard_from_ram(&state.ram);

    let path: Vec<String> = [
        "D", "D", "D", "D", "D", "D", "D", "CW", "L", "L", "L", "L", "L", "L", "D", "CCW",
        "D", "R",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    for sr in [0i32, -2] {
        eprintln!(
            "recorded sim row {sr}: {:?}",
            simulate_path_prefix(&bb, 6, sr, 3, 0, &path)
        );
    }

    let moves = bfs_moves(&bb, 6, 0, 3, 0);
    for want in [(14, 2, 0), (16, 0, 0), (14, 0, 0), (16, 2, 0)] {
        if let Some(m) = moves
            .iter()
            .find(|m| m.row == want.0 && m.col == want.1 && m.rot == want.2)
        {
            eprintln!("BFS {:?}: {:?}", want, m.path);
            eprintln!(
                "  classify={}",
                classify_move(&bb, 6, m.row, m.col, m.rot, &m.path, 0)
            );
            eprintln!(
                "  sim {:?}",
                simulate_path_prefix(&bb, 6, 0, 3, 0, &m.path)
            );
        }
    }
    assert_eq!(path_terminal_mtype(&path), "tuck");
    eprintln!(
        "replay meta classify_move_type={}",
        TetrisBot::classify_move_type(&path)
    );
    eprintln!(
        "classify_move@want(14,2)={}",
        classify_move(&bb, 6, 14, 2, 0, &path, 0)
    );
    eprintln!(
        "classify_move@got(16,0)={}",
        classify_move(&bb, 6, 16, 0, 0, &path, 0)
    );
    for m in moves.iter().filter(|m| m.path.iter().any(|a| a == "CCW" || a == "CW")) {
        eprintln!(
            "spin cand ({},{},r{}) {:?}",
            m.row, m.col, m.rot, m.path
        );
    }
}

/// I→S #1: col/rot matched but garbage row 29 triggered false misdrop.
#[test]
fn i_to_s_normal_false_positive_guards() {
    let bb = [0u16; BOARD_ROWS];
    // Off-board row reads must not count as grounded.
    assert!(!piece_trustworthily_grounded(&bb, 0, 0, 29, 6));
    assert!(!piece_pos_trustworthy(29, 6));
    // Simple normal placement only checks col/rot, not lock row.
    assert!(!misdrop_check_row("normal", &[]));
    assert!(misdrop_check_row("tuck", &[]));
    assert!(misdrop_check_row("normal", &["D".to_string()]));
}

/// cargo test t_to_l_misdrop_probe -- --nocapture
#[test]
#[ignore = "awaiting fresh fixture capture"]
fn t_to_l_misdrop_probe() {
    use base64::Engine;
    use crate::bot::srs::srs_try_rotate_auto;
    use crate::state::EmulatorState;

    let b64 = std::fs::read_to_string(
        super::fixtures::misdrop_fixture("T_to_L_1_spawn_state.b64"),
    )
    .unwrap_or_else(|_| {
        std::fs::read_to_string(
            super::fixtures::misdrop_fixture("T_to_L_1_misdrop_state.b64"),
        )
        .unwrap()
    });
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim().trim_matches('"'))
        .unwrap();
    let state: EmulatorState = bincode::deserialize(&bytes).unwrap();
    let mut emu = super::fixtures::emulator_from_savestate(&state);
    let bb_ram = read_board_bitboard_from_ram(&state.ram);
    let bb_emu = read_board_bitboard_from_emulator(&emu);
    // Live board captured from replay restore while in-game (__emu read_mem 0xC800).
    let live_bb: [u16; BOARD_ROWS] = [
        520, 550, 1021, 591, 136, 252, 706, 204, 729, 576, 30, 731, 882, 457, 590, 819,
        146, 713,
    ];
    let empty_bb = [0u16; BOARD_ROWS];
    let bb = if bb_emu.iter().any(|&r| r != 0) {
        bb_emu
    } else if bb_ram.iter().any(|&r| r != 0) {
        bb_ram
    } else {
        live_bb
    };
    eprintln!(
        "bb_ram sum={} bb_emu sum={} using_live={}",
        bb_ram.iter().sum::<u16>(),
        bb_emu.iter().sum::<u16>(),
        bb == live_bb
    );

    let type_t = 2usize;
    let spawn_col = 3usize;
    let spawn_rot = 0usize;
    let want = (13i32, 3i32, 3usize);

    let recorded: Vec<String> = std::iter::repeat("D")
        .take(13)
        .map(|s| s.to_string())
        .chain(["R", "D", "D", "D", "CCW", "D", "L"].iter().map(|s| s.to_string()))
        .collect();

    for (label, test_bb) in [("LIVE", &bb), ("EMPTY", &empty_bb)] {
        eprintln!("=== BFS on {label} board sum={} ===", test_bb.iter().sum::<u16>());
        let mv = bfs_moves(test_bb, type_t, 0, spawn_col, spawn_rot);
        eprintln!("  moves from row0: {}", mv.len());
        for m in mv.iter().filter(|m| m.row == 13 && m.col == 3 && m.rot == 3) {
            eprintln!(
                "  (13,3,r3) path={:?} valid={}",
                m.path,
                bfs_path_reaches_lock(test_bb, type_t, 0, spawn_col as i32, spawn_rot, m)
            );
        }
        if let Some(m) = mv.iter().find(|m| m.path == recorded) {
            eprintln!("  recorded path match ({},{},r{})", m.row, m.col, m.rot);
        } else {
            eprintln!("  recorded path: NOT in BFS");
        }
        let cw_count = mv
            .iter()
            .filter(|m| {
                m.path.windows(3)
                    .any(|w| w.iter().all(|a| a == "CW"))
                    && m.rot == 3
            })
            .count();
        eprintln!("  CW-chain r3 count: {cw_count}");
        for m in mv
            .iter()
            .filter(|m| {
                m.path.windows(3)
                    .any(|w| w.iter().all(|a| a == "CW"))
                    && m.rot == 3
            })
            .take(5)
        {
            eprintln!("    ({},{},r{}) {:?}", m.row, m.col, m.rot, m.path);
        }
    }

    eprintln!("board: {:?}", bb);

    for sr in -2..=5 {
        let blocked = piece_collides(&bb, type_t, spawn_rot, sr, spawn_col as i32);
        eprintln!("spawn row {sr} blocked={blocked}");
    }

    let moves = bfs_moves(&bb, type_t, 0, spawn_col, spawn_rot);
    eprintln!("BFS total moves from row0: {}", moves.len());
    let moves = moves;

    for m in moves
        .iter()
        .filter(|m| m.rot == want.2 && (m.col == want.1 || m.col == 5))
    {
        eprintln!(
            "  cand ({},{},r{}) path={:?}",
            m.row, m.col, m.rot, m.path
        );
    }

    for m in moves.iter().filter(|m| m.col == want.1 && m.rot == want.2) {
        eprintln!(
            "BFS want lock ({},{},r{}) path_len={} path={:?}",
            m.row, m.col, m.rot, m.path.len(), m.path
        );
        eprintln!(
            "  sim={:?} classify={}",
            simulate_path_prefix(&bb, type_t, 0, spawn_col as i32, spawn_rot, &m.path),
            classify_move(&bb, type_t, m.row, m.col, m.rot, &m.path, 0)
        );
    }

    if let Some(m) = moves.iter().find(|m| m.path == recorded) {
        eprintln!("recorded path in BFS → ({},{},r{})", m.row, m.col, m.rot);
    } else {
        eprintln!("recorded path NOT exact match in BFS");
        if let Some(sim) = simulate_path_prefix(&bb, type_t, 0, spawn_col as i32, spawn_rot, &recorded) {
            eprintln!("recorded sim → {:?}", sim);
        } else {
            eprintln!("recorded path does NOT sim from spawn");
        }
    }

    // All locks at want row/col/rot
    for m in moves.iter().filter(|m| m.row == want.0 && m.col == want.1 && m.rot == want.2) {
        let ok = bfs_path_reaches_lock(&bb, type_t, 0, spawn_col as i32, spawn_rot, m);
        eprintln!(
            "  exact want {:?} path_len={} reaches_lock={ok} path={:?}",
            want,
            m.path.len(),
            m.path
        );
    }

    // CW-chain candidates (3+ consecutive CW)
    for m in moves.iter().filter(|m| {
        m.path.windows(3).any(|w| w.iter().all(|a| a == "CW"))
    }) {
        if m.col == want.1 || m.col == 5 {
            eprintln!(
                "  CW-chain ({},{},r{}) reaches={} {:?}",
                m.row,
                m.col,
                m.rot,
                bfs_path_reaches_lock(&bb, type_t, 0, spawn_col as i32, spawn_rot, m),
                m.path
            );
        }
    }

    // 2-ply pick (T then L) with path validation
    let mut best_score = i32::MIN;
    let mut best = None;
    for m in &moves {
        if !bfs_path_reaches_lock(&bb, type_t, 0, spawn_col as i32, spawn_rot, m) {
            continue;
        }
        let (bb1, c1, h1) = simulate_place_and_clear(&bb, type_t, m.rot, m.col as usize, m.row);
        let mv2 = bfs_moves(&bb1, 5, 0, 3, 0);
        let mut inner = i32::MIN;
        for m2 in &mv2 {
            if !bfs_path_reaches_lock(&bb1, 5, 0, 3, 0, m2) {
                continue;
            }
            let (bb2, c2, h2) =
                simulate_place_and_clear(&bb1, 5, m2.rot, m2.col as usize, m2.row);
            inner = inner.max(meatfighter_evaluate(
                &bb2,
                &column_heights(&bb2),
                c1 + c2,
                h1 + h2,
            ));
        }
        if inner > best_score {
            best_score = inner;
            best = Some(m);
        }
    }
    if let Some(m) = best {
        eprintln!(
            "2-ply validated best T ({},{},r{}) score={} path={:?}",
            m.row, m.col, m.rot, best_score, m.path
        );
    } else {
        eprintln!("2-ply validated best: NONE");
    }

    let recorded_ok =
        bfs_path_reaches_lock(&bb, type_t, -2, spawn_col as i32, spawn_rot, &BfsLockedMove {
            row: want.0,
            col: want.1,
            rot: want.2,
            path: recorded.clone(),
        });
    eprintln!("recorded path reaches_lock from row-2: {recorded_ok}");

    // User manual: early CW then D's then CW chain (SRS kick weirdness).
    let manual_candidates: Vec<Vec<&str>> = vec![
        vec!["CW"],
        vec!["CW", "D", "D", "D", "CW", "CW", "CW", "CW", "CW"],
        vec!["CW", "D", "D", "D", "D", "D", "D", "D", "D", "D", "D", "D", "D", "CW", "CW", "CW", "CW", "CW"],
    ];
    for cand in manual_candidates {
        let path: Vec<String> = cand.iter().map(|s| s.to_string()).collect();
        if let Some(end) = simulate_path_prefix(&bb, type_t, 0, spawn_col as i32, spawn_rot, &path) {
            eprintln!("manual {:?} → {:?}", path, end);
        }
    }

    // Step through recorded path at CCW
    let prefix: Vec<String> = recorded[..recorded.len() - 2].iter().cloned().collect();
    if let Some((r, c, rot)) =
        simulate_path_prefix(&bb, type_t, 0, spawn_col as i32, spawn_rot, &prefix)
    {
        eprintln!("pre-CCW @({r},{c},r{rot})");
        let ccw = srs_try_rotate_auto(&bb, type_t, r, c, rot, false);
        let cw = srs_try_rotate_auto(&bb, type_t, r, c, rot, true);
        eprintln!("  CCW auto → {ccw:?}");
        eprintln!("  CW  auto → {cw:?}");
    }

    // Recorded path step-by-step on empty board
    let mut r = 0i32;
    let mut c = spawn_col as i32;
    let mut rot = spawn_rot;
    for (i, act) in recorded.iter().enumerate() {
        match act.as_str() {
            "D" => r += 1,
            "L" => c -= 1,
            "R" => c += 1,
            "CW" => {
                if let Some((nr, nc, nrot)) = srs_try_rotate_auto(&bb, type_t, r, c, rot, true) {
                    eprintln!("step {i} CW ({r},{c},r{rot})→({nr},{nc},r{nrot})");
                    r = nr; c = nc; rot = nrot;
                } else {
                    eprintln!("step {i} CW FAIL at ({r},{c},r{rot})");
                }
            }
            "CCW" => {
                if let Some((nr, nc, nrot)) = srs_try_rotate_auto(&bb, type_t, r, c, rot, false) {
                    eprintln!("step {i} CCW ({r},{c},r{rot})→({nr},{nc},r{nrot})");
                    r = nr; c = nc; rot = nrot;
                } else {
                    eprintln!("step {i} CCW FAIL at ({r},{c},r{rot})");
                }
            }
            _ => {}
        }
    }
    eprintln!("recorded end ({r},{c},r{rot}) want {:?}", want);

}

/// Browser-paused T→L #1 board captured Jul 8 2026 via chrome devtools.
#[test]
fn t_to_l_live_pause_board_rejects_spin_path() {
    let pause_bb: [u16; BOARD_ROWS] =
        [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 15, 903, 903, 1015, 511];
    let type_t = 2usize;
    let spawn_col = 3usize;
    let spawn_rot = 0usize;
    let recorded: Vec<String> = std::iter::repeat("D")
        .take(13)
        .map(|s| s.to_string())
        .chain(["R", "D", "D", "D", "CCW", "D", "L"].iter().map(|s| s.to_string()))
        .collect();

    let path_ok = bfs_path_reaches_lock(
        &pause_bb,
        type_t,
        -2,
        spawn_col as i32,
        spawn_rot,
        &BfsLockedMove {
            row: 13,
            col: 3,
            rot: 3,
            path: recorded.clone(),
        },
    );
    eprintln!("pause board sum={}", pause_bb.iter().sum::<u16>());
    eprintln!(
        "recorded path path_ok={path_ok} sim(-2)={:?} sim(0)={:?}",
        simulate_path_stepwise(&pause_bb, type_t, -2, spawn_col as i32, spawn_rot, &recorded),
        simulate_path_stepwise(&pause_bb, type_t, 0, spawn_col as i32, spawn_rot, &recorded),
    );
    eprintln!("bfs_moves row0={}", bfs_moves(&pause_bb, type_t, 0, spawn_col, spawn_rot).len());

    // Simulate plan() BFS pick on this board (T + next L, 2-ply).
    let mut best_score = i32::MIN;
    let mut best: Option<(usize, usize, Vec<String>, i32)> = None;
    for m in bfs_moves(&pause_bb, type_t, 0, spawn_col, spawn_rot) {
        if !bfs_path_reaches_lock(&pause_bb, type_t, 0, spawn_col as i32, spawn_rot, &m) {
            continue;
        }
        let (bb1, c1, h1) =
            simulate_place_and_clear(&pause_bb, type_t, m.rot, m.col as usize, m.row);
        let mut inner = i32::MIN;
        for m2 in bfs_moves(&bb1, 5, 0, 3, 0) {
            if !bfs_path_reaches_lock(&bb1, 5, 0, 3, 0, &m2) {
                continue;
            }
            let (bb2, c2, h2) =
                simulate_place_and_clear(&bb1, 5, m2.rot, m2.col as usize, m2.row);
            inner = inner.max(meatfighter_evaluate(
                &bb2,
                &column_heights(&bb2),
                c1 + c2,
                h1 + h2,
            ));
        }
        if inner > best_score {
            best_score = inner;
            best = Some((m.rot, m.col as usize, m.path.clone(), m.row));
        }
    }
    if let Some((rot, col, path, row)) = best {
        let acceptable = bfs_plan_acceptable(
            &pause_bb,
            type_t,
            -2,
            spawn_col,
            spawn_rot,
            0,
            -2,
            row,
            col,
            rot,
            &path,
        );
        let winner_sim = simulate_path_stepwise(
            &pause_bb, type_t, -2, spawn_col as i32, spawn_rot, &path,
        );
        eprintln!(
            "2-ply best ({row},{col},r{rot}) acceptable={acceptable} winner_sim(-2)={winner_sim:?}"
        );
        assert!(
            !acceptable,
            "pause board: 2-ply BFS winner must not be acceptable from spawn row -2"
        );
    }

    // Recorded path simulates from spawn row -2 in SRS model but not from row 0
    // (gravity-advanced pose). plan() uses actual_row, so this is allowed in BFS;
    // emulator golden on aux_t_midgame_pause catches ROM mismatch.
    assert!(
        simulate_path_stepwise(&pause_bb, type_t, 0, spawn_col as i32, spawn_rot, &recorded)
            .is_none(),
        "recorded spin path must not simulate from row 0 on pause board"
    );
}

/// cargo test t_to_l_emulator_plan_probe -- --nocapture
#[test]
#[ignore = "awaiting fresh fixture capture"]
fn t_to_l_emulator_plan_probe() {
    use base64::Engine;
    use crate::emulator::Emulator;
    use crate::state::EmulatorState;

    for (label, file) in [
        ("spawn", "T_to_L_1_spawn_state.b64"),
        ("misdrop", "T_to_L_1_misdrop_state.b64"),
    ] {
        let b64 = std::fs::read_to_string(
            super::fixtures::misdrop_fixture(file),
        )
        .unwrap_or_else(|e| panic!("missing {file}: {e}"));
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.trim().trim_matches('"'))
            .unwrap();
        let state: EmulatorState = bincode::deserialize(&bytes).unwrap();
        let mut emu = super::fixtures::emulator_from_savestate(&state);

        let read = |a: u16| emu.memory.read(a);
        let read_r = |a: u16, len: u16| {
            (0..len)
                .map(|i| emu.memory.read(a.wrapping_add(i as u16)))
                .collect::<Vec<_>>()
        };

        let bb_ram = read_board_bitboard_from_ram(&state.ram);
        let bb_emu = read_board_bitboard_from_emulator(&emu);
        let ori = read(ADDR_CUR_ORI);
        let next_ori = read(ADDR_NEXT_ORI);
        let info = ori_info(ori);
        let next_info = ori_info(next_ori);
        let actual_row = piece_min_row(&read);
        let actual_col = piece_left_col(&read);
        let bfs_row = actual_row.max(0);
        let extra_d = if actual_row < 0 {
            (-actual_row) as usize
        } else {
            0
        };

        eprintln!("\n=== T→L {label} ({file}) ===");
        eprintln!(
            "bb_ram sum={} bb_emu sum={}",
            bb_ram.iter().sum::<u16>(),
            bb_emu.iter().sum::<u16>()
        );
        eprintln!(
            "ori=0x{ori:02X} {:?} next=0x{next_ori:02X} {:?}",
            info, next_info
        );
        eprintln!(
            "piece @ row={actual_row} col={actual_col} bfs_row={bfs_row} extra_d={extra_d}"
        );

        let Some((type_idx, spawn_rot)) = info else {
            eprintln!("  no current piece info — splash/menu?");
            continue;
        };
        let next_idx = next_info.map(|i| i.0);

        let bfs_res = if let Some(nt) = next_idx {
            find_best_move_with_bfs(&read, &read_r, actual_col, bfs_row, Some(nt))
        } else {
            find_best_move_with_bfs_1ply(&read, &read_r, bfs_row, actual_col, spawn_rot as usize)
        };

        match &bfs_res {
            None => eprintln!("  BFS result: NONE"),
            Some((rot, col, path, row)) => {
                let bb = read_board_bitboard(&read_r);
                let mtype = classify_move(&bb, type_idx, *row, *col as i32, *rot, path, 0);
                let path_ok = planned_bfs_path_valid(
                    &bb,
                    type_idx,
                    actual_row,
                    actual_col,
                    spawn_rot as usize,
                    extra_d,
                    *row,
                    *col,
                    *rot,
                    path,
                );
                eprintln!(
                    "  BFS best: ({row},{col},r{rot}) mtype={mtype} path_ok={path_ok} path={path:?}"
                );
                eprintln!("  bfs_usable={path_ok}");
            }
        }

        let safe = find_safe_normal_placement(&read, &read_r, actual_col);
        eprintln!("  safe_normal: {safe:?}");

        let mut bot = TetrisBot::new();
        bot.pps_limit = f64::INFINITY;
        let mut actions = Vec::new();
        bot.plan(&read, &read_r, ori, &mut actions);
        eprintln!(
            "  bot plan: mode={} status={} path={:?} target=({},{},r{})",
            bot.meat_mode,
            bot.status_msg,
            bot.debug_get_move_path(),
            bot.target_left,
            bot.plan_intended_row,
            bot.target_rot
        );
    }
}

/// Live T→L board: BFS spawn blocked at row 0 → plan() must use safe 1-ply normal.
#[test]
fn t_to_l_live_board_falls_back_to_safe_normal() {
    let live_bb: [u16; BOARD_ROWS] = [
        520, 550, 1021, 591, 136, 252, 706, 204, 729, 576, 30, 731, 882, 457, 590, 819,
        146, 713,
    ];
    let type_t = 2usize;
    let spawn_col = 3usize;
    let spawn_rot = 0usize;

    // plan() uses bfs_row = actual_row.max(0); at spawn actual_row is often 0 or negative.
    let bfs_at_row0 = bfs_moves(&live_bb, type_t, 0, spawn_col, spawn_rot);
    assert_eq!(
        bfs_at_row0.len(),
        0,
        "T spawn blocked at row 0 on live board — BFS must not return tuck/spin plans"
    );

    // Recorded misdrop path does not simulate to intended lock.
    let recorded: Vec<String> = std::iter::repeat("D")
        .take(13)
        .map(|s| s.to_string())
        .chain(["R", "D", "D", "D", "CCW", "D", "L"].iter().map(|s| s.to_string()))
        .collect();
    assert!(
        !planned_bfs_path_valid(
            &live_bb,
            type_t,
            0,
            spawn_col,
            spawn_rot,
            0,
            13,
            3,
            3,
            &recorded,
        ),
        "recorded T-spin path must fail path validation"
    );

    // Classic 1-ply normal search still finds a placement.
    let mut found_safe = false;
    for rot in 0..4 {
        let shape = &SHAPES[type_t][rot];
        let max_dc = shape.iter().map(|&[_, c]| c).max().unwrap_or(0);
        let max_dr = shape.iter().map(|&[r, _]| r).max().unwrap_or(0);
        let left_col_max = BOARD_COLS - 1 - max_dc as usize;
        for left_col in 0..=left_col_max {
            if !is_reachable(&live_bb, shape, spawn_col, left_col) {
                continue;
            }
            let mut land_row = -1i32;
            'find_land: for start_row in 0..=(BOARD_ROWS as i32 - 1 - max_dr as i32) {
                for &[dr, dc] in shape {
                    let r = start_row + dr as i32;
                    let c = left_col as i32 + dc as i32;
                    if r >= 0 && r < BOARD_ROWS as i32 && (live_bb[r as usize] & (1 << c)) != 0
                    {
                        land_row = start_row - 1;
                        break 'find_land;
                    }
                }
                land_row = start_row;
            }
            if land_row >= 0 {
                found_safe = true;
                break;
            }
        }
        if found_safe {
            break;
        }
    }
    assert!(
        found_safe,
        "safe 1-ply normal placement must exist on live board"
    );
}

/// cargo test s_to_l_plan_probe -- --nocapture
#[test]
#[ignore = "awaiting fresh fixture capture"]
fn s_to_l_plan_probe() {
    use base64::Engine;
    use crate::emulator::Emulator;
    use crate::state::EmulatorState;

    let b64 = std::fs::read_to_string(
        super::fixtures::misdrop_fixture("s_to_l_misdrop_state.b64"),
    )
    .unwrap();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim().trim_matches('"'))
        .unwrap();
    let state: EmulatorState = bincode::deserialize(&bytes).unwrap();
    let mut emu = super::fixtures::emulator_from_savestate(&state);
    for _ in 0..30 {
        emu.run_frame();
    }
    let read = |a: u16| emu.memory.read(a);
    let read_r = |a: u16, len: u16| {
        (0..len)
            .map(|i| emu.memory.read(a.wrapping_add(i as u16)))
            .collect::<Vec<_>>()
    };
    let bb = read_board_bitboard(&read_r);
    let actual_row = piece_min_row(&read);
    let actual_col = piece_left_col(&read);
    let spawn_rot = ori_info(read(ADDR_CUR_ORI)).map(|i| i.1 as usize).unwrap_or(0);
    eprintln!("spawn ({actual_row},{actual_col},r{spawn_rot})");
    for col in [actual_col, 3usize] {
        let moves = bfs_moves(&bb, 3, actual_row, col, spawn_rot);
        let hit = moves.iter().find(|m| m.row == 16 && m.col == 5 && m.rot == 2);
        eprintln!("BFS col {col}: hits={} want={:?}", moves.len(), hit.map(|m| &m.path));
        if let Some(m) = hit {
            let ok = bfs_plan_acceptable(
                &bb, 3, actual_row, col, spawn_rot, 0, actual_row,
                m.row, m.col as usize, m.rot, &m.path,
            );
            eprintln!("  acceptable={ok} sim={:?}", simulate_path_stepwise(
                &bb, 3, actual_row, col as i32, spawn_rot, &m.path,
            ));
        }
    }
    let next_ori = read(ADDR_NEXT_ORI);
    let bfs_res = find_best_move_with_bfs(&read, &read_r, actual_col, actual_row, Some(5));
    if let Some((rot, col, path, row)) = &bfs_res {
        let ok = bfs_plan_acceptable(
            &bb, 3, actual_row, actual_col, spawn_rot, 0, actual_row,
            *row, *col, *rot, path,
        );
        eprintln!("find_best_move_with_bfs: ({row},{col},r{rot}) acceptable={ok}");
    } else {
        eprintln!("find_best_move_with_bfs: NONE");
    }
    let mut bot = TetrisBot::new();
    bot.set_pps(f64::INFINITY);
    bot.set_soft_drop_mode(true);
    bot.begin_replay_restore();
    let mut actions = Vec::new();
    bot.plan(&read, &read_r, read(ADDR_CUR_ORI), &mut actions);
    eprintln!(
        "plan: state={:?} status={} path={:?} next=0x{next_ori:02X}",
        bot.state, bot.status_msg, bot.move_path
    );
    if let Some((rot, col, path, row)) = &bfs_res {
        let mt = classify_move(&bb, 3, *row, *col as i32, *rot, path, 0);
        eprintln!("2-ply pick mtype={mt} want spin to (16,5,r2)");
    }
}

#[test]
fn j_tuck_prefers_simple_path_over_floor_kicks() {
    use base64::Engine;
    use crate::state::EmulatorState;

    // Dual-lock green fixture (spawn matches meta J). Old r15_c6 had shifted next-piece state.
    let b64 = std::fs::read_to_string(
        super::fixtures::misdrop_fixture("misdrop_j_tuck_r13_c3_r3_state.b64"),
    )
    .unwrap();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim().trim_matches('"'))
        .unwrap();
    let state: EmulatorState = bincode::deserialize(&bytes).unwrap();
    let mut emu = super::fixtures::emulator_from_savestate(&state);
    let read = |a: u16| emu.memory.read(a);
    let read_r = |a: u16, len: u16| {
        (0..len)
            .map(|i| read(a.wrapping_add(i as u16)))
            .collect::<Vec<_>>()
    };
    let bb = read_board_bitboard(&read_r);
    let spawn_row = piece_min_row(&read);
    let spawn_col = piece_left_col(&read) as i32;
    let spawn_rot = ori_info(read(ADDR_CUR_ORI)).map(|i| i.1 as usize).unwrap_or(0);
    let type_j = 6usize;
    let want_row = 13i32;
    let want_col = 3i32;
    let want_rot = 3usize;

    let moves = bfs_moves(&bb, type_j, spawn_row, spawn_col as usize, spawn_rot);
    let m = moves
        .iter()
        .find(|m| m.row == want_row && m.col == want_col && m.rot == want_rot)
        .expect("BFS must reach want lock");
    let kicks = count_floor_kicks_in_path(&bb, type_j, spawn_row, spawn_col, spawn_rot, &m.path);
    assert!(
        kicks <= 1,
        "BFS path should minimize floor kicks (got {kicks}): {:?}",
        m.path
    );
    let simplified = prefer_simplest_equivalent_path(
        &bb,
        type_j,
        spawn_row,
        spawn_col,
        spawn_rot,
        want_row,
        want_col,
        want_rot,
        &m.path,
    );
    assert!(
        count_floor_kicks_in_path(&bb, type_j, spawn_row, spawn_col, spawn_rot, &simplified) <= 1
    );
    assert!(simulate_path_stepwise(
        &bb, type_j, spawn_row, spawn_col, spawn_rot, &simplified,
    )
    .is_some_and(|(r, c, rot)| r == want_row && c == want_col && rot == want_rot));
    let mtype = classify_move(&bb, type_j, want_row, want_col, want_rot, &simplified, 0);
    // May be rotate-first or BFS mid-descent (D… then rot) — both valid after L-tuck
    // keep-BFS rule for lat→rot mid-stack paths.
    assert!(
        is_rotate_first_path(&simplified, mtype)
            || simplified.first().map(|s| s.as_str()) == Some("D"),
        "J tuck path should rotate-first or descend-first: {simplified:?}"
    );
    assert!(
        simplified.iter().any(|a| a == "R" || a == "L"),
        "path should include lateral setup or terminal slide: {simplified:?}"
    );
}

#[test]
fn j_tuck_r13_c3_replay_want_lock_plans_tuck() {
    use base64::Engine;
    use crate::state::EmulatorState;

    let b64 = std::fs::read_to_string(
        super::fixtures::misdrop_fixture("misdrop_j_tuck_r13_c3_r3_state.b64"),
    )
    .unwrap();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim().trim_matches('"'))
        .unwrap();
    let state: EmulatorState = bincode::deserialize(&bytes).unwrap();
    let emu = super::fixtures::emulator_from_savestate(&state);
    let read = |a: u16| emu.memory.read(a);
    let read_r = |a: u16, len: u16| {
        (0..len)
            .map(|i| emu.memory.read(a.wrapping_add(i as u16)))
            .collect::<Vec<_>>()
    };

    let mut bot = TetrisBot::new();
    bot.set_pps(f64::INFINITY);
    bot.set_soft_drop_mode(true);
    bot.begin_replay_restore_with_want(13, 3, 3, Some("tuck"));
    let mut actions = Vec::new();
    bot.plan(&read, &read_r, read(ADDR_CUR_ORI), &mut actions);

    assert_eq!(bot.meat_mode, "replay-want");
    assert_eq!(bot.debug_get_landing_type(), "tuck");
    assert_eq!(
        bot.intended_lock
            .as_ref()
            .map(|(r, c, rot, t)| (*r, *c as usize, *rot, t.clone())),
        Some((13, 3, 3, "tuck".to_string()))
    );
    assert!(
        matches!(
            bot.planned_path.first().map(|s| s.as_str()),
            Some("CCW") | Some("D")
        ),
        "tuck replay path starts with CCW or D: {:?}",
        bot.planned_path
    );
    assert!(matches!(bot.bot_state(), BotState::Path));
}

/// cargo test j_tuck_r13_c3_plan_probe -- --nocapture
#[test]
fn j_tuck_r13_c3_plan_probe() {
    use base64::Engine;
    use crate::state::EmulatorState;

    let b64 = std::fs::read_to_string(
        super::fixtures::misdrop_fixture("misdrop_j_tuck_r13_c3_r3_state.b64"),
    )
    .unwrap();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim().trim_matches('"'))
        .unwrap();
    let state: EmulatorState = bincode::deserialize(&bytes).unwrap();
    for (label, emu_frames) in [("raw", 0u32), ("emu30", 30)] {
        let mut emu = super::fixtures::emulator_from_savestate(&state);
        for _ in 0..emu_frames {
            emu.run_frame();
        }
        let read = |a: u16| emu.memory.read(a);
        let read_r = |a: u16, len: u16| {
            (0..len)
                .map(|i| emu.memory.read(a.wrapping_add(i as u16)))
                .collect::<Vec<_>>()
        };
        probe_j_tuck_board(label, &read, &read_r);
    }
}

fn probe_j_tuck_board(
    label: &str,
    read: &impl Fn(u16) -> u8,
    read_r: &impl Fn(u16, u16) -> Vec<u8>,
) {
    let bb = read_board_bitboard(read_r);
    let actual_row = piece_min_row(read);
    let actual_col = piece_left_col(read);
    let spawn_rot = ori_info(read(ADDR_CUR_ORI)).map(|i| i.1 as usize).unwrap_or(0);
    let next_ori = read(ADDR_NEXT_ORI);
    let next_idx = ori_info(next_ori).map(|i| i.0);

    eprintln!("\n=== {label} ===");
    eprintln!("spawn ({actual_row},{actual_col},r{spawn_rot}) next=0x{next_ori:02X} idx={next_idx:?}");

    let want_row = 13i32;
    let want_col = 3i32;
    let want_rot = 3usize;
    let type_j = 6usize; // J in PIECE_NAMES

    for probe_col in [actual_col, 3usize] {
        let moves = bfs_moves(&bb, type_j, actual_row, probe_col, spawn_rot);
        if let Some(m) = moves.iter().find(|m| m.row == want_row && m.col == want_col && m.rot == want_rot) {
            eprintln!("col{probe_col} BFS path to want: {:?}", m.path);
        } else {
            eprintln!("col{probe_col}: no BFS hit for want lock");
        }
    }

    let moves = bfs_moves(&bb, type_j, actual_row, actual_col, spawn_rot);
    let hits: Vec<_> = moves
        .iter()
        .filter(|m| m.row == want_row && m.col == want_col && m.rot == want_rot)
        .collect();
    eprintln!("BFS hits for ({want_row},{want_col},r{want_rot}): {}", hits.len());
    for m in &hits {
        eprintln!("  path={:?}", m.path);
        eprintln!("  sim={:?}", simulate_path_stepwise(
            &bb, type_j, actual_row, actual_col as i32, spawn_rot, &m.path,
        ));
    }

    let human_paths: &[&[&str]] = &[
        &["CCW", "R", "L"],
        &["CCW", "R", "D", "D", "D", "D", "D", "D", "D", "D", "D", "D", "D", "D", "D", "D", "D", "D", "D", "L"],
    ];
    for hp in human_paths {
        let p: Vec<String> = hp.iter().map(|s| s.to_string()).collect();
        eprintln!(
            "human {:?} -> {:?}",
            hp,
            simulate_path_stepwise(&bb, type_j, actual_row, actual_col as i32, spawn_rot, &p),
        );
    }

    let recorded: Vec<String> = [
        "CCW", "R",
        "D", "D", "D", "D", "D", "D", "D", "D", "D", "D", "D", "D", "D", "D", "D", "D", "D",
        "L",
    ].iter().map(|s| s.to_string()).collect();
    eprintln!(
        "recorded sim={:?}",
        simulate_path_stepwise(&bb, type_j, actual_row, actual_col as i32, spawn_rot, &recorded),
    );

    let bfs_res = find_best_move_with_bfs(&read, &read_r, actual_col, actual_row, next_idx);
    let bfs_res_o = find_best_move_with_bfs(&read, &read_r, actual_col, actual_row, Some(1));
    for (tag, res) in [("next_ram", &bfs_res), ("next_O", &bfs_res_o)] {
        if let Some((rot, col, path, row)) = res {
            let mtype = classify_move(&bb, type_j, *row, *col as i32, *rot, path, 0);
            let ok = bfs_plan_acceptable(
                &bb, type_j, actual_row, actual_col, spawn_rot, 0, actual_row,
                *row, *col, *rot, path,
            );
            eprintln!("2-ply {tag}: ({row},{col},r{rot}) mtype={mtype} acceptable={ok}");
            eprintln!("  raw path={path:?}");
            if *row == want_row && *col == want_col as usize && *rot == want_rot {
                eprintln!("  *** THIS IS THE WANT LOCK ***");
            }
        } else {
            eprintln!("2-ply {tag}: NONE");
        }
    }

    let mut bot = TetrisBot::new();
    bot.set_pps(f64::INFINITY);
    bot.set_soft_drop_mode(true);
    bot.begin_replay_restore();
    let mut actions = Vec::new();
    bot.plan(&read, &read_r, read(ADDR_CUR_ORI), &mut actions);
    eprintln!(
        "bot plan: mode={} path={:?} intended={:?}",
        bot.meat_mode, bot.move_path, bot.intended_lock,
    );

    if let Some(m) = hits.first() {
        trace_path_kicks(label, &bb, type_j, actual_row, actual_col as i32, spawn_rot, &m.path);
    }
    trace_path_kicks(
        label,
        &bb,
        type_j,
        actual_row,
        actual_col as i32,
        spawn_rot,
        &recorded,
    );
    let human_ok: Vec<String> = ["CCW", "R", "R", "R", "R", "D", "D", "D", "D", "D", "D", "D", "D", "D", "D", "D", "D", "D", "L"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    if actual_row >= 4 {
        trace_path_kicks(label, &bb, type_j, actual_row, actual_col as i32, spawn_rot, &human_ok);
    }
}

fn trace_path_kicks(
    label: &str,
    bb: &Bitboard,
    type_idx: usize,
    start_row: i32,
    start_col: i32,
    start_rot: usize,
    path: &[String],
) {
    use super::srs::srs_try_rotate_auto;
    let mut r = start_row;
    let mut c = start_col;
    let mut rot = start_rot;
    let mut grounded_run = 0u32;
    eprintln!("--- kick trace [{label}] from ({start_row},{start_col},r{start_rot}) ---");
    for (i, action) in path.iter().enumerate() {
        let can_down = !super::bfs::piece_collides(bb, type_idx, rot, r + 1, c);
        let before = (r, c, rot);
        match action.as_str() {
            "D" => {
                if can_down {
                    r += 1;
                    grounded_run = 0;
                }
            }
            "L" => {
                c -= 1;
                grounded_run = if can_down { 0 } else { grounded_run + 1 };
            }
            "R" => {
                c += 1;
                grounded_run = if can_down { 0 } else { grounded_run + 1 };
            }
            "CW" | "CCW" => {
                let cw = action == "CW";
                if let Some((nr, nc, nrot, ti, (kx, ky))) =
                    super::srs::srs_try_rotate_detailed(bb, type_idx, r, c, rot, cw, !can_down)
                {
                    let kicked = nr != r || nc != c;
                    eprintln!(
                        "  [{i}] {action} @({},{},r{}) -> ({},{},r{}) kick#{ti} ({kx},{ky}) grounded={}",
                        before.0, before.1, before.2, nr, nc, nrot, !can_down
                    );
                    r = nr;
                    c = nc;
                    rot = nrot;
                    grounded_run = if kicked { 0 } else if can_down { 0 } else { grounded_run + 1 };
                } else if let Some((nr, nc, nrot)) = srs_try_rotate_auto(bb, type_idx, r, c, rot, cw) {
                    eprintln!(
                        "  [{i}] {action} @({},{},r{}) -> ({},{},r{}) (auto, no detail)",
                        before.0, before.1, before.2, nr, nc, nrot
                    );
                    r = nr;
                    c = nc;
                    rot = nrot;
                    grounded_run = 0;
                }
            }
            _ => {}
        }
        if action != "CW" && action != "CCW" {
            eprintln!(
                "  [{i}] {action} @({},{},r{}) -> ({},{},r{})",
                before.0, before.1, before.2, r, c, rot
            );
        }
    }
    eprintln!("  final: ({r},{c},r{rot})");
}

/// cargo test z_to_t_bot_probe -- --nocapture
#[test]
#[ignore = "awaiting fresh fixture capture"]
fn z_to_t_bot_probe() {
    use base64::Engine;
    use crate::emulator::joypad::GbButton;
    use crate::state::EmulatorState;

    let b64 = std::fs::read_to_string(
        super::fixtures::misdrop_fixture("Z_to_T_1_misdrop_state.b64"),
    )
    .unwrap();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim().trim_matches('"'))
        .unwrap();
    let state: EmulatorState = bincode::deserialize(&bytes).unwrap();
    let mut emu = super::fixtures::emulator_from_savestate(&state);

    let mut bot = TetrisBot::new();
    bot.set_pps(f64::INFINITY);
    bot.set_soft_drop_mode(true);
    bot.begin_replay_restore();

    let apply = |emu: &mut crate::emulator::Emulator, actions: &[(u8, bool)]| {
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
    };

    let mut saw_path = false;
    let mut lock_z = (-1i32, 0usize, 0usize);
    let mut saw_z = false;
    let mut plan_path: Vec<String> = Vec::new();
    for frame in 0..15_000 {
        let (_gs, actions) = bot.tick(
            |a| emu.memory.read(a),
            |s, l| (0..l).map(|i| emu.memory.read(s.wrapping_add(i))).collect(),
        );
        apply(&mut emu, &actions);
        emu.run_frame();

        if matches!(bot.bot_state(), BotState::Path) {
            if !saw_path {
                plan_path = bot.debug_get_move_path();
            }
            saw_path = true;
        }

        let cur_type = ori_info(emu.memory.read(ADDR_CUR_ORI)).map(|i| i.0);
        let p = emu_piece_pos(&emu);
        if cur_type == Some(4) {
            saw_z = true;
            if piece_pos_trustworthy(p.0, p.1) {
                lock_z = p;
            }
        } else if saw_z && cur_type == Some(2) {
            eprintln!(
                "Z locked @({},{},r{}) misdrops={} planned_len={} step={}/{}",
                lock_z.0,
                lock_z.1,
                lock_z.2,
                bot.misdrop_stats().0,
                plan_path.len(),
                bot.debug_get_target().2,
                plan_path.len()
            );
            eprintln!("planned path ({}) {:?}", plan_path.len(), plan_path);
            eprintln!("path trace:\n{}", bot.debug_take_path_trace());
            break;
        }

        if frame == 80 && saw_path {
            eprintln!(
                "frame {frame} bot={:?} step={}/{} pos={p:?} tgt={:?}",
                bot.bot_state(),
                bot.debug_get_target().2,
                bot.debug_get_move_path().len(),
                bot.debug_get_target()
            );
        }
    }
    assert!(saw_path, "bot must enter Path for Z→T tuck");
    eprintln!(
        "Z→T probe lock ({},{},r{}) want (11,3,r1) — emu_golden still fail_path until lock-delay setup-CW fix",
        lock_z.0, lock_z.1, lock_z.2
    );
}

/// cargo test z_to_t_cw_from_10_6 -- --nocapture
#[test]
#[ignore = "awaiting fresh fixture capture"]
fn z_to_t_cw_from_10_6() {
    let b64 = std::fs::read_to_string(
        super::fixtures::misdrop_fixture("Z_to_T_1_misdrop_state.b64"),
    )
    .unwrap();
    let prefix: Vec<&str> = vec![
        "D", "D", "D", "D", "D", "D", "D", "D", "D", "D", "D", "R", "R", "R",
    ];
    use base64::Engine;
    use crate::bot::fixtures::emulator_from_savestate;
    use crate::emulator::joypad::GbButton;
    use crate::state::EmulatorState;

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim().trim_matches('"'))
        .unwrap();
    let state: EmulatorState = bincode::deserialize(&bytes).unwrap();
    let mut emu = emulator_from_savestate(&state);
    for (label, d_n, r_n) in [("11D+RRR", 11, 3), ("11D+RR", 11, 2)] {
        let state2: EmulatorState = bincode::deserialize(&bytes).unwrap();
        let mut emu2 = emulator_from_savestate(&state2);
        let mut emu2 = emulator_from_savestate(&state);
        let d_prefix: Vec<&str> = std::iter::repeat("D").take(d_n).collect();
        emu_run_path_on(&mut emu2, &d_prefix);
        for _ in 0..r_n {
            emu2.joypad.press(GbButton::Right);
            for _ in 0..MIN_BTN_HOLD_FRAMES {
                emu2.run_frame();
            }
            emu2.joypad.release(GbButton::Right);
            for _ in 0..4 {
                emu2.run_frame();
            }
        }
        let p2 = emu_piece_pos(&emu2);
        eprintln!("{label} pos: ({},{},r{})", p2.0, p2.1, p2.2);
        for f in 0..15 {
            emu2.joypad.press(GbButton::A);
            emu2.run_frame();
            let q = emu_piece_pos(&emu2);
            if q.2 != p2.2 {
                eprintln!("  {label} CW ok frame {f}: ({},{},r{})", q.0, q.1, q.2);
                break;
            }
        }
        emu2.joypad.release(GbButton::A);
    }
    let d_prefix: Vec<&str> = std::iter::repeat("D").take(11).collect();
    emu_run_path_on(&mut emu, &d_prefix);
    for _ in 0..3 {
        emu.joypad.press(GbButton::Right);
        for _ in 0..MIN_BTN_HOLD_FRAMES {
            emu.run_frame();
        }
        emu.joypad.release(GbButton::Right);
        for _ in 0..4 {
            emu.run_frame();
        }
    }
    let p = emu_piece_pos(&emu);
    eprintln!("fast RRR pos: ({},{},r{})", p.0, p.1, p.2);
    let bb = read_board_bitboard_from_emulator(&emu);
    eprintln!(
        "srs CW: {:?}",
        crate::bot::srs::srs_try_rotate_auto(&bb, 4, p.0, p.1 as i32, p.2, true)
    );
    for _ in 0..30 {
        emu.run_frame();
    }
    let delayed = emu_piece_pos(&emu);
    eprintln!("after 30f delay: ({},{},r{})", delayed.0, delayed.1, delayed.2);
    emu.joypad.press(GbButton::Left);
    for _ in 0..MIN_BTN_HOLD_FRAMES {
        emu.run_frame();
    }
    emu.joypad.release(GbButton::Left);
    for _ in 0..4 {
        emu.run_frame();
    }
    eprintln!("after L nudge: {:?}", emu_piece_pos(&emu));
    for f in 0..30 {
        emu.joypad.press(GbButton::A);
        emu.run_frame();
        let q = emu_piece_pos(&emu);
        if q.2 != p.2 {
            eprintln!("CW ok frame {f}: ({},{},r{})", q.0, q.1, q.2);
            break;
        }
    }
    emu.joypad.release(GbButton::A);
    let end = emu_piece_pos(&emu);
    eprintln!("after fast CW: ({},{},r{})", end.0, end.1, end.2);
}

/// cargo test z_to_t_prefix_steps -- --nocapture
#[test]
#[ignore = "awaiting fresh fixture capture"]
fn z_to_t_prefix_steps() {
    use base64::Engine;
    use crate::state::EmulatorState;

    let b64 = std::fs::read_to_string(
        super::fixtures::misdrop_fixture("Z_to_T_1_misdrop_state.b64"),
    )
    .unwrap();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim().trim_matches('"'))
        .unwrap();
    let state: EmulatorState = bincode::deserialize(&bytes).unwrap();
    let bb = read_board_bitboard_from_ram(&state.ram);
    let spawn_row = -2i32;
    let spawn_col = 3usize;
    let spawn_rot = 0usize;
    let path: Vec<&str> = vec![
        "D", "D", "D", "D", "D", "D", "D", "D", "D", "D", "D", "R", "R", "R", "D", "CW",
        "D", "L", "D",
    ];
    for i in 0..=path.len() {
        let prefix: Vec<&str> = path[..i].to_vec();
        let sim = simulate_path_prefix(&bb, 4, spawn_row, spawn_col as i32, spawn_rot, &prefix.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        let emu = if prefix.is_empty() {
            (spawn_row, spawn_col, spawn_rot)
        } else {
            emu_run_path(&b64, &prefix)
        };
        eprintln!("step {i:02} {:?} sim={sim:?} emu=({},{},r{})", prefix.last(), emu.0, emu.1, emu.2);
    }
}

/// cargo test z_to_t_emu_run_path -- --nocapture
#[test]
#[ignore = "awaiting fresh fixture capture"]
fn z_to_t_emu_run_path() {
    use crate::state::EmulatorState;
    use base64::Engine;

    let b64 = std::fs::read_to_string(
        super::fixtures::misdrop_fixture("Z_to_T_1_misdrop_state.b64"),
    )
    .unwrap();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim().trim_matches('"'))
        .unwrap();
    let state: EmulatorState = bincode::deserialize(&bytes).unwrap();
    let mut emu = super::fixtures::emulator_from_savestate(&state);
    let bb = read_board_bitboard_from_emulator(&emu);
    let spawn_row = piece_min_row(|a| emu.memory.read(a));
    let spawn_col = piece_left_col(|a| emu.memory.read(a));
    let spawn_rot = ori_info(emu.memory.read(ADDR_CUR_ORI))
        .map(|i| i.1 as usize)
        .unwrap_or(0);
    eprintln!("live spawn ({spawn_row},{spawn_col},r{spawn_rot})");

    let bfs_owned = bfs_moves(&bb, 4, spawn_row, spawn_col, spawn_rot)
        .into_iter()
        .find(|m| m.row == 11 && m.col == 3 && m.rot == 1)
        .expect("BFS path")
        .path;
    let mtype = classify_move(&bb, 4, 11, 3, 1, &bfs_owned, 0);
    let fixed = fix_sz_spin_path(&bb, 4, spawn_row, spawn_col, spawn_rot, mtype, &bfs_owned);
    eprintln!("BFS path len={} mtype={mtype} {:?}", fixed.len(), fixed);
    let bfs_path: Vec<&str> = fixed.iter().map(|s| s.as_str()).collect();
    let got_bfs = emu_run_path(&b64, &bfs_path);
    let sim_bfs = simulate_path_prefix(
        &bb, 4, spawn_row, spawn_col as i32, spawn_rot, &fixed,
    );
    eprintln!("sim BFS => {sim_bfs:?}  emu open-loop => ({},{},r{})", got_bfs.0, got_bfs.1, got_bfs.2);
    // Open-loop emu_run_path drifts on passive gravity; Tier-3 uses TetrisBot (rosy_golden).
    assert_eq!(sim_bfs, Some((11, 3, 1)), "BFS path must simulate to want");
}

/// cargo test o_to_s_planner_analysis -- --nocapture
#[test]
#[ignore = "awaiting fresh fixture capture"]
fn o_to_s_planner_analysis() {
    use base64::Engine;
    use crate::state::EmulatorState;

    let b64 = std::fs::read_to_string(
        super::fixtures::misdrop_fixture("O_to_S_4_misdrop_state.b64"),
    )
    .unwrap();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim().trim_matches('"'))
        .unwrap();
    let state: EmulatorState = bincode::deserialize(&bytes).unwrap();
    let mut emu = super::fixtures::emulator_from_savestate(&state);
    let bb_ram = read_board_bitboard_from_ram(&state.ram);
    let bb_emu = read_board_bitboard_from_emulator(&emu);
    eprintln!(
        "bb_ram sum={} bb_emu sum={}",
        bb_ram.iter().sum::<u16>(),
        bb_emu.iter().sum::<u16>(),
    );
    let heights = column_heights(&bb_emu);
    eprintln!("col heights: {:?}", &heights[..]);
    for sr in [-2i32, 0, 2, 4, 5] {
        let moves = bfs_moves(&bb_emu, 1, sr, 4, 0);
        let hit = moves.iter().find(|m| m.row == 16 && m.col == 1 && m.rot == 0);
        eprintln!("spawn row {sr}: bfs={} hit(16,1,r0)={}", moves.len(), hit.is_some());
        if let Some(m) = hit {
            eprintln!("  path={:?}", m.path);
        }
    }
    eprintln!("col1-3 locks row>=10:");
    for m in bfs_moves(&bb_emu, 1, -2, 4, 0)
        .into_iter()
        .filter(|m| m.col <= 3 && m.row >= 10)
    {
        eprintln!("  ({},{},r{}) {:?}", m.row, m.col, m.rot, m.path);
    }
    for want in [(16, 1, 0), (14, 3, 0), (14, 1, 0)] {
        let hit = bfs_moves(&bb_emu, 1, -2, 4, 0)
            .iter()
            .any(|m| m.row == want.0 && m.col == want.1 && m.rot == want.2);
        eprintln!("reachable {:?}: {hit}", want);
    }
    for (name, bb) in [("ram", &bb_ram), ("emu", &bb_emu)] {
        let moves = bfs_moves(bb, 1, -2, 4, 0);
        let hit = moves.iter().find(|m| m.row == 16 && m.col == 1 && m.rot == 0);
        eprintln!("{name}: bfs_moves={} hit(16,1,r0)={}", moves.len(), hit.is_some());
        if let Some(m) = hit {
            eprintln!("  path={:?}", m.path);
            eprintln!(
                "  sim={:?}",
                simulate_path_stepwise(bb, 1, -2, 4, 0, &m.path)
            );
        }
    }
    let path: Vec<String> = std::iter::repeat("D")
        .take(15)
        .map(|_| "D".to_string())
        .chain(["L", "D", "D", "D", "L", "L"].iter().map(|s| s.to_string()))
        .collect();
    let bb = if bb_emu.iter().any(|&r| r != 0) {
        bb_emu
    } else {
        bb_ram
    };
    for (i, act) in path.iter().enumerate() {
        let prefix = &path[..=i];
        let sim = simulate_path_stepwise(&bb, 1, -2, 4, 0, prefix);
        if sim.is_none() {
            eprintln!("recorded fails at step {i} act={act} prefix={prefix:?}");
            break;
        }
        if i + 1 == path.len() {
            eprintln!("recorded full sim {sim:?}");
        }
    }
}

/// cargo test recent_tuck_misdrop_analysis -- --nocapture
#[test]
#[ignore = "awaiting fresh fixture capture"]
fn recent_tuck_misdrop_analysis() {
    use base64::Engine;
    use crate::state::EmulatorState;

    for (name, b64_file, type_idx, spawn_col, spawn_rot, want, recorded) in [
        (
            "O→S #4",
            "O_to_S_4_misdrop_state.b64",
            1usize,
            4usize,
            0usize,
            (16i32, 1i32, 0usize),
            vec!["D"; 15]
                .into_iter()
                .map(|_| "D".to_string())
                .chain(["L", "D", "D", "D", "L", "L"].iter().map(|s| s.to_string()))
                .collect::<Vec<_>>(),
        ),
        (
            "Z→T #1",
            "Z_to_T_1_misdrop_state.b64",
            4usize,
            3usize,
            0usize,
            (11i32, 3i32, 1usize),
            vec![
                "D", "D", "D", "R", "R", "R", "D", "D", "CW", "D", "L", "D",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        ),
    ] {
        let b64 = std::fs::read_to_string(
            super::fixtures::misdrop_fixture(b64_file),
        )
        .unwrap_or_else(|_| panic!("missing {b64_file}"));
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.trim().trim_matches('"'))
            .unwrap();
        let state: EmulatorState = bincode::deserialize(&bytes).unwrap();
        let bb = read_board_bitboard_from_ram(&state.ram);

        use crate::emulator::Emulator;
        let mut emu = super::fixtures::emulator_from_savestate(&state);
        for _ in 0..30 {
            emu.run_frame();
        }
        let read = |a: u16| emu.memory.read(a);
        let spawn_row = piece_min_row(&read);
        let spawn_col_live = piece_left_col(&read);
        let spawn_rot_live = ori_info(read(ADDR_CUR_ORI)).map(|i| i.1 as usize).unwrap_or(99);
        eprintln!("live spawn ({spawn_row},{spawn_col_live},r{spawn_rot_live}) meta col {spawn_col}");

        let moves = bfs_moves(&bb, type_idx, spawn_row, spawn_col_live, spawn_rot_live);
        let best = moves
            .iter()
            .find(|m| m.row == want.0 && m.col == want.1 && m.rot == want.2);
        eprintln!("\n=== {name} ===");
        eprintln!("recorded path ({})", recorded.len());
        eprintln!("  {:?}", recorded);
        if let Some(m) = best {
            let mtype = classify_move(&bb, type_idx, m.row, m.col as i32, m.rot, &m.path, 0);
            eprintln!("BFS winner ({},{},r{}) type={mtype}", m.row, m.col, m.rot);
            eprintln!("  {:?}", m.path);
            eprintln!(
                "  sim {:?}",
                simulate_path_prefix(
                    &bb,
                    type_idx,
                    spawn_row,
                    spawn_col_live as i32,
                    spawn_rot_live,
                    &m.path,
                )
            );
            eprintln!("paths match: {}", m.path == recorded);
        } else {
            eprintln!("no BFS hit for want {want:?}");
            for m in moves.iter().take(8) {
                eprintln!("  cand ({},{},r{}) {:?}", m.row, m.col, m.rot, m.path);
            }
        }
        eprintln!(
            "recorded sim {:?}",
            simulate_path_prefix(
                &bb,
                type_idx,
                spawn_row,
                spawn_col_live as i32,
                spawn_rot_live,
                &recorded,
            )
        );
        let plan_row = plan_row_before_final_action(
            &bb,
            type_idx,
            spawn_row,
            spawn_col_live as i32,
            spawn_rot_live,
            &recorded,
            want.0,
            "tuck",
            want.2,
        );
        eprintln!("plan_intended_row={plan_row} lock_row={}", want.0);
        let pre = simulate_path_prefix(
            &bb,
            type_idx,
            spawn_row,
            spawn_col_live as i32,
            spawn_rot_live,
            &recorded[..recorded.len().saturating_sub(1)],
        );
        eprintln!("sim before final action {pre:?}");
        if name.contains('Z') {
            for sr in [0i32, 5, 7] {
                let m5 = bfs_moves(&bb, type_idx, sr, 3, 0);
                if let Some(m) = m5.iter().find(|m| m.row == want.0 && m.col == want.1 && m.rot == want.2) {
                    eprintln!("BFS from row {sr}: {:?}", m.path);
                    eprintln!(
                        "  sim {:?}",
                        simulate_path_prefix(&bb, type_idx, sr, 3, 0, &m.path)
                    );
                }
            }
        }

        let got = emu_run_path(&b64, &recorded.iter().map(|s| s.as_str()).collect::<Vec<_>>());
        eprintln!("emu recorded path => ({},{},r{})", got.0, got.1, got.2);
    }
}

/// cargo test s_to_l_path_compare -- --nocapture
#[test]
#[ignore = "awaiting fresh fixture capture"]
fn s_to_l_path_compare_emulator() {
    let b64 = std::fs::read_to_string(
        super::fixtures::misdrop_fixture("s_to_l_misdrop_state.b64"),
    )
    .unwrap();

    let bfs: Vec<&str> = std::iter::repeat("D")
        .take(14)
        .chain(["R", "CCW", "D", "D", "D", "CCW"].iter().copied())
        .collect();

    let user_guess: Vec<&str> = ["CCW", "R"]
        .iter()
        .copied()
        .chain(std::iter::repeat("D").take(12))
        .chain(["R", "CCW"].iter().copied())
        .collect();

    let fixed: Vec<&str> = std::iter::repeat("D")
        .take(14)
        .chain(["CCW", "R", "D", "D", "D", "R", "CCW"].iter().copied())
        .collect();

    let extra_r: Vec<&str> = std::iter::repeat("D")
        .take(14)
        .chain(["R", "R", "CCW", "D", "D", "D", "CW"].iter().copied())
        .collect();

    let d11: Vec<&str> = std::iter::repeat("D")
        .take(11)
        .chain(["R", "R", "CCW", "D", "D", "D", "CW"].iter().copied())
        .collect();

    for (name, path) in [
        ("bfs_srs", bfs),
        ("fix_sz_spin", fixed),
        ("user_CCWR..RCCW", user_guess),
        ("extra_R_end_CW", extra_r),
        ("d11_RR_CCW_DDD_CW", d11),
    ] {
        let got = emu_run_path(&b64, &path);
        eprintln!("{name}: R={} path_len={} => ({},{},r{})", 
            path.iter().filter(|a| **a == "R").count(),
            path.len(), got.0, got.1, got.2);
    }
}

/// Full emulator replay — run with `cargo test s_to_l_savestate_spin -- --ignored --nocapture`
#[test]
#[ignore = "awaiting fresh fixture capture"]
fn s_to_l_savestate_spin_emulator_execution() {
    use crate::emulator::{joypad::GbButton, Emulator};
    use crate::state::EmulatorState;
    use base64::Engine;

    let b64 = std::fs::read_to_string(
        super::fixtures::misdrop_fixture("s_to_l_misdrop_state.b64"),
    )
    .expect("s_to_l_misdrop_state.b64");
    let b64 = b64.trim().trim_matches('"');
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64).unwrap();
    let state: EmulatorState = bincode::deserialize(&bytes).unwrap();

    let mut emu = super::fixtures::emulator_from_savestate(&state);

    let mut bot = TetrisBot::new();
    bot.set_soft_drop_mode(true);
    bot.begin_replay_restore();

    let apply_actions = |emu: &mut Emulator, actions: &[(u8, bool)]| {
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
    };

    let mut saw_path = false;
    let mut planned_len = 0usize;
    let mut lock_col = 0usize;
    let mut lock_rot = 0usize;
    let mut lock_row = -1i32;
    let mut trace_dump = String::new();

    for _frame in 0..15000 {
        let (_gs, actions) = bot.tick(
            |a| emu.memory.read(a),
            |s, l| (0..l).map(|i| emu.memory.read(s.wrapping_add(i))).collect(),
        );
        apply_actions(&mut emu, &actions);
        emu.run_frame();

        if matches!(bot.state, BotState::Path) {
            if !saw_path {
                saw_path = true;
                planned_len = bot.move_path.len();
            }
            if !bot.path_trace.is_empty() {
                trace_dump = bot.path_trace.join("\n");
            }
        }

        if saw_path && matches!(bot.state, BotState::Dropping) {
            let row = piece_min_row(|a| emu.memory.read(a));
            let col = piece_left_col(|a| emu.memory.read(a));
            let rot = ori_info(emu.memory.read(ADDR_CUR_ORI))
                .map(|i| i.1 as usize)
                .unwrap_or(99);
            if row >= 13 {
                lock_row = row;
                lock_col = col;
                lock_rot = rot;
                break;
            }
        }
    }

    assert!(saw_path, "bot must plan S→L spin path from savestate");
    assert!(
        planned_len >= 20,
        "must plan full BFS path from spawn (got {} steps, not 15-step suffix); trace:\n{}",
        planned_len,
        bot.path_trace.join("\n")
    );
    assert_eq!(
        (lock_row, lock_col, lock_rot),
        (16, 5, 2),
        "S→L spin must lock at (16,5,r2); trace:\n{}",
        trace_dump
    );
    assert_eq!(bot.misdrop_count, 0, "must not misdrop S→L spin");
}
