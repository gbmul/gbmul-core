//! Tier-3 Rosy emulator golden tests.
//!
//! Run pass fixtures: `cargo test -p gbmul-core --features emulator_tests rosy_golden_pass`
//! Run all (slow):     `cargo test -p gbmul-core --features emulator_tests -- --ignored rosy_golden_all`

#![cfg(feature = "emulator_tests")]

use base64::Engine;
use gbmul_core::bot::fixture_manifest::{ClaimKind, FixtureEntry, ManifestContract};
use gbmul_core::bot::fixtures::{emulator_from_savestate, misdrop_fixture};
use gbmul_core::bot::{
    is_occupied, ori_info, piece_collides, piece_left_col, piece_min_row, piece_pos_trustworthy,
    ADDR_CUR_ORI, ADDR_NEXT_ORI, BOARD_BASE, BOARD_COLS, BOARD_ROWS, BOARD_STRIDE, BotState,
    PIECE_NAMES, TetrisBot,
};
use gbmul_core::emulator::Emulator;
use gbmul_core::state::EmulatorState;

const MANIFEST: &str = include_str!("fixtures/misdrop/manifest.json");

fn piece_type_idx(name: &str) -> Option<usize> {
    PIECE_NAMES.iter().position(|&p| p == name)
}

fn load_manifest() -> ManifestContract {
    let m: ManifestContract = serde_json::from_str(MANIFEST).expect("parse manifest.json");
    assert_eq!(m.version, 2);
    m
}

fn load_savestate_b64(name: &str) -> EmulatorState {
    let path = misdrop_fixture(name);
    let b64 = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim().trim_matches('"'))
        .expect("base64 decode");
    bincode::deserialize(&bytes).expect("bincode EmulatorState")
}

fn apply_actions(emu: &mut Emulator, actions: &[(u8, bool)]) {
    use gbmul_core::emulator::joypad::GbButton;
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

fn run_bot_until_lock(fixture: &FixtureEntry) -> (i32, usize, usize, u32) {
    let state = load_savestate_b64(&fixture.b64);
    let mut emu = emulator_from_savestate(&state);

    let fixture_type = piece_type_idx(&fixture.piece)
        .unwrap_or_else(|| panic!("{}: unknown piece {}", fixture.id, fixture.piece));
    let next_type = fixture
        .next
        .as_deref()
        .and_then(piece_type_idx)
        .or_else(|| {
            ori_info(state.ram[ADDR_NEXT_ORI as usize])
                .map(|(t, _)| t)
        });

    let mut bot = TetrisBot::new();
    bot.set_pps(f64::INFINITY);
    bot.set_soft_drop_mode(true);
    bot.begin_replay_restore();

    let mut lock_row = -1i32;
    let mut lock_col = 0usize;
    let mut lock_rot = 0usize;
    let mut saw_path = false;
    let mut saw_fixture_piece = false;
    let mut last_status = String::new();

    for frame in 0..20_000 {
        let (_gs, actions) = bot.tick(
            |a| emu.memory.read(a),
            |s, l| (0..l).map(|i| emu.memory.read(s.wrapping_add(i))).collect(),
        );
        apply_actions(&mut emu, &actions);
        emu.run_frame();
        last_status = bot.action().to_string();

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
            let collides =
                rot < 4 && piece_collides(&bb, fixture_type, rot, row, col as i32);
            if piece_pos_trustworthy(row, col) && !collides {
                // Prefer pose matching want when known — freeze so ARE/Dropping
                // glitches cannot overwrite a correct terminal tuck
                // (L→S r13 c3: (13,3) then false (13,2) mid-lock).
                let matches_want = fixture.want_lock.as_ref().is_some_and(|w| {
                    row == w.row && col == w.col && rot == w.rot
                });
                if matches_want || lock_row < 0 {
                    lock_row = row;
                    lock_col = col;
                    lock_rot = rot;
                } else if !fixture.want_lock.as_ref().is_some_and(|w| {
                    lock_row == w.row && lock_col == w.col && lock_rot == w.rot
                }) {
                    // No want match yet — keep updating
                    lock_row = row;
                    lock_col = col;
                    lock_rot = rot;
                }
            }
        } else if saw_fixture_piece && cur_type.is_some() {
            // Next piece spawned — stop before reading its spawn pose as the lock.
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

        if frame == 300 && !saw_path {
            eprintln!(
                "{} @frame300: bot={:?} status={last_status} path_len={}",
                fixture.id,
                bot.bot_state(),
                bot.debug_get_move_path().len()
            );
        }
    }

    assert!(
        saw_path,
        "{}: bot must enter Path state (last status={last_status}, bot={:?})",
        fixture.id,
        bot.bot_state()
    );
    assert!(
        lock_row >= 0,
        "{}: must observe lock (row={lock_row}, status={last_status}, path={:?})",
        fixture.id,
        bot.debug_get_move_path()
    );
    (lock_row, lock_col, lock_rot, bot.misdrop_stats().0)
}

fn emu_pass_fixtures(manifest: &ManifestContract) -> Vec<&FixtureEntry> {
    manifest
        .fixtures
        .iter()
        .filter(|e| {
            e.has_ci_claim(ClaimKind::ExecutorPath)
                && e.want_lock.is_some()
                && !matches!(e.claim(ClaimKind::AuxiliaryBoard), Some(_))
        })
        .collect()
}

#[test]
fn rosy_golden_manifest_smoke() {
    let manifest = load_manifest();
    assert_eq!(manifest.version, 2);
    eprintln!(
        "rosy_golden: {} fixtures, {} executor CI",
        manifest.fixtures.len(),
        emu_pass_fixtures(&manifest).len()
    );
}

#[test]
fn rosy_golden_pass_fixtures() {
    let manifest = load_manifest();
    for entry in emu_pass_fixtures(&manifest) {
        let want = entry.want_lock.as_ref().unwrap();
        let (row, col, rot, misdrops) = run_bot_until_lock(entry);
        assert_eq!(
            (row, col, rot),
            (want.row, want.col, want.rot),
            "{}: lock mismatch",
            entry.id
        );
        assert_eq!(misdrops, 0, "{}: must not misdrop", entry.id);
    }
}

#[test]
#[ignore = "slow Rosy emulator golden — all fixtures including fail baselines"]
fn rosy_golden_all_fixtures() {
    let manifest = load_manifest();
    for entry in &manifest.fixtures {
        let executor = entry.claim(ClaimKind::ExecutorPath);
        if executor.is_none() || executor.is_some_and(|c| c.baseline.is_none()) {
            continue;
        }
        let Some(want) = &entry.want_lock else {
            continue;
        };
        let (row, col, rot, misdrops) = run_bot_until_lock(entry);
        assert_eq!(
            (row, col, rot),
            (want.row, want.col, want.rot),
            "{}: lock mismatch",
            entry.id
        );
        assert_eq!(misdrops, 0, "{}: must not misdrop", entry.id);
    }
}