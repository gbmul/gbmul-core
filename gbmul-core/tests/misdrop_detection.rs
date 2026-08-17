//! Misdrop detection golden tests — manifest `misdrop_detection` claims + meta sidecar.

use std::fs;

use gbmul_core::bot::fixture_manifest::{ClaimKind, Knowledge, ManifestContract};
use gbmul_core::bot::fixtures::misdrop_fixture;
use gbmul_core::bot::misdrop::{evaluate_lock, resolve_mtype_for_row, LockActual, LockExpectation};
use serde::Deserialize;

const MANIFEST: &str = include_str!("fixtures/misdrop/manifest.json");

#[derive(Debug, Deserialize)]
struct MetaMisdrop {
    move_type: Option<String>,
    wanted_col: Option<usize>,
    wanted_rot: Option<usize>,
    wanted_row: Option<i32>,
    actual_col: Option<usize>,
    actual_rot: Option<usize>,
    actual_row: Option<i32>,
    path: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct SidecarMeta {
    misdrop: Option<MetaMisdrop>,
}

fn load_meta(name: &str) -> SidecarMeta {
    let path = misdrop_fixture(name);
    let raw = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&raw).expect("parse meta json")
}

#[test]
fn manifest_misdrop_detection_contract() {
    let manifest: ManifestContract = serde_json::from_str(MANIFEST).expect("manifest");
    assert_eq!(manifest.version, 2);
    for entry in &manifest.fixtures {
        for claim in &entry.claims {
            if claim.kind != ClaimKind::MisdropDetection {
                continue;
            }
            if claim.enforced_in_ci {
                assert!(
                    entry.want_lock.is_some() && entry.recorded_path.is_some(),
                    "{}: CI misdrop_detection needs want_lock + recorded_path",
                    entry.id
                );
                assert!(
                    entry.meta.is_some(),
                    "{}: CI misdrop_detection needs meta sidecar with got pose",
                    entry.id
                );
            }
        }
    }
}

#[test]
fn misdrop_detection_golden_from_manifest() {
    let manifest: ManifestContract = serde_json::from_str(MANIFEST).expect("manifest");

    for entry in &manifest.fixtures {
        let Some(claim) = entry.claim(ClaimKind::MisdropDetection) else {
            continue;
        };
        if claim.knowledge != Knowledge::Proven || !claim.enforced_in_ci {
            continue;
        }

        let want = entry.want_lock.as_ref().expect("want_lock");
        let path = entry.recorded_path.clone().expect("recorded_path");
        let meta_name = entry.meta.as_ref().expect("meta");
        let sidecar = load_meta(meta_name);
        let m = sidecar.misdrop.expect("misdrop in meta");
        let mtype = m.move_type.as_deref().unwrap_or("unknown");
        let path_mtype = if mtype == "normal" {
            "normal"
        } else {
            gbmul_core::bot::path_terminal_mtype(&path)
        };
        let mtype_for_row = resolve_mtype_for_row(mtype, path_mtype);

        let got_col = m.actual_col.expect("actual_col in meta");
        let got_rot = m.actual_rot.expect("actual_rot in meta");
        let got_row = m.actual_row.expect("actual_row in meta");

        let exp = LockExpectation {
            want_col: want.col,
            want_rot: want.rot,
            want_row: Some(want.row),
            mtype_for_row,
        };
        let act = LockActual {
            col: got_col,
            rot: got_rot,
            eff_row: got_row,
        };

        assert_eq!(
            evaluate_lock(&exp, &act, &path),
            None,
            "{}: false-positive misdrop — evaluate_lock must not fire (want {:?} got {:?})",
            entry.id,
            (want.row, want.col, want.rot),
            (got_row, got_col, got_rot)
        );
    }
}

/// Dev: `cargo test misdrop_fixture_diagnosis -- --nocapture`
#[test]
fn misdrop_fixture_diagnosis() {
    use base64::Engine;
    use gbmul_core::bot::{
        bfs_moves, simulate_path_stepwise, BOARD_COLS, BOARD_ROWS, BOARD_STRIDE, PIECE_NAMES,
    };
    use gbmul_core::state::EmulatorState;

    let manifest: ManifestContract = serde_json::from_str(MANIFEST).expect("manifest");
    for entry in &manifest.fixtures {
        let want = entry.want_lock.as_ref().expect("want_lock");
        let path = entry.recorded_path.clone().unwrap_or_default();
        let type_idx = PIECE_NAMES.iter().position(|p| *p == entry.piece.as_str()).unwrap();
        let spawn = entry.spawn.as_ref().expect("spawn");

        let b64 = fs::read_to_string(misdrop_fixture(&entry.b64)).expect("b64");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.trim().trim_matches('"'))
            .expect("decode");
        let state: EmulatorState = bincode::deserialize(&bytes).expect("state");
        let mut bb = [0u16; BOARD_ROWS];
        for row in 0..BOARD_ROWS {
            let base = 0x800 + row * BOARD_STRIDE + 2;
            for col in 0..BOARD_COLS {
                if gbmul_core::bot::is_occupied(state.ram.get(base + col).copied().unwrap_or(0)) {
                    bb[row] |= 1 << col;
                }
            }
        }

        let moves = bfs_moves(&bb, type_idx, spawn.row, spawn.col, spawn.rot);
        let bfs_hit = moves.iter().any(|m| {
            m.row == want.row && m.col == want.col as i32 && m.rot == want.rot
        });
        let sim = simulate_path_stepwise(
            &bb,
            type_idx,
            spawn.row,
            spawn.col as i32,
            spawn.rot,
            &path,
        );

        eprintln!("=== {} (board {}) ===", entry.id, entry.board_id.as_deref().unwrap_or("?"));
        eprintln!("  spawn ({},{},r{}) want ({},{},r{})", spawn.row, spawn.col, spawn.rot, want.row, want.col, want.rot);
        eprintln!("  BFS reaches want: {bfs_hit} ({} locks total)", moves.len());
        eprintln!("  recorded_path sim: {sim:?}");
        if let Some(m) = moves.iter().find(|m| m.col == want.col as i32 && m.rot == want.rot) {
            eprintln!("  BFS path to want rot/col: {:?}", m.path);
        }
    }
}