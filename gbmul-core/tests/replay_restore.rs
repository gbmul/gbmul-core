//! Tier 4 — replay restore must not false-positive misdrop on manifest fixtures.

#![cfg(feature = "emulator_tests")]

use base64::Engine;
use gbmul_core::bot::fixture_manifest::{ClaimKind, ManifestContract};
use gbmul_core::bot::fixtures::{emulator_from_savestate, misdrop_fixture};
use gbmul_core::bot::{ori_info, piece_left_col, piece_min_row, ADDR_CUR_ORI, BotState, TetrisBot};
use gbmul_core::emulator::joypad::GbButton;
use gbmul_core::state::EmulatorState;

const MANIFEST: &str = include_str!("fixtures/misdrop/manifest.json");

fn load_state(name: &str) -> EmulatorState {
    let path = misdrop_fixture(name);
    let b64 = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim().trim_matches('"'))
        .unwrap_or_else(|e| panic!("b64 {}: {e}", path.display()));
    bincode::deserialize(&bytes).expect("EmulatorState")
}

fn apply_actions(emu: &mut gbmul_core::emulator::Emulator, actions: &[(u8, bool)]) {
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

#[test]
#[ignore = "planner_s_spin_r16_c5 restore still misdrops — enable when spawn pairing + S→L restore fixed"]
fn replay_restore_must_not_misdrop_on_manifest_fixtures() {
    let manifest: ManifestContract = serde_json::from_str(MANIFEST).expect("manifest");
    for entry in manifest.fixtures.iter().filter(|e| {
        e.has_ci_claim(ClaimKind::PlannerReachability)
            && !matches!(e.claim(ClaimKind::AuxiliaryBoard), Some(_))
    }) {
        let state = load_state(&entry.b64);
        let mut emu = emulator_from_savestate(&state);
        let mut bot = TetrisBot::new();
        bot.set_pps(f64::INFINITY);
        bot.set_soft_drop_mode(true);
        bot.begin_replay_restore();

        let mut saw_path = false;
        for _ in 0..12_000 {
            let (_gs, actions) = bot.tick(
                |a| emu.memory.read(a),
                |s, l| (0..l).map(|i| emu.memory.read(s.wrapping_add(i))).collect(),
            );
            apply_actions(&mut emu, &actions);
            emu.run_frame();
            if matches!(bot.bot_state(), BotState::Path) {
                saw_path = true;
            }
            let _ = (
                piece_min_row(|a| emu.memory.read(a)),
                piece_left_col(|a| emu.memory.read(a)),
                ori_info(emu.memory.read(ADDR_CUR_ORI)),
            );
            if saw_path && bot.debug_get_move_path().is_empty() && bot.misdrop_stats().0 > 0 {
                break;
            }
        }
        assert_eq!(
            bot.misdrop_stats().0,
            0,
            "{}: replay restore must not misdrop",
            entry.id
        );
    }
}