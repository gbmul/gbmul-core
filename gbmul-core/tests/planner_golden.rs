//! Phase 2 — planner parity tests driven by manifest v2 claims.

use base64::Engine;
use gbmul_core::bot::fixture_manifest::{ClaimKind, ManifestContract};
use gbmul_core::bot::{
    bfs_moves, fixtures::misdrop_fixture, is_occupied, ori_info, piece_left_col, piece_min_row,
    simulate_path_stepwise, ADDR_CUR_ORI, BOARD_COLS, BOARD_ROWS, BOARD_STRIDE, BfsLockedMove,
    PIECE_NAMES,
};
use gbmul_core::state::EmulatorState;
use serde::Deserialize;

const MANIFEST: &str = include_str!("fixtures/misdrop/manifest.json");

#[derive(Debug, Deserialize)]
struct WantLock {
    row: i32,
    col: usize,
    rot: usize,
}

#[derive(Debug, Deserialize)]
struct SpawnParams {
    row: i32,
    col: usize,
    rot: usize,
}

#[derive(Debug, Deserialize)]
struct FixtureEntry {
    id: String,
    b64: String,
    piece: String,
    recorded_path: Option<Vec<String>>,
    want_lock: Option<WantLock>,
    spawn: Option<SpawnParams>,
    claims: Vec<gbmul_core::bot::fixture_manifest::Claim>,
}

#[derive(Debug, Deserialize)]
struct Manifest {
    version: u32,
    fixtures: Vec<FixtureEntry>,
}

fn piece_type_idx(name: &str) -> Option<usize> {
    PIECE_NAMES.iter().position(|&p| p == name)
}

fn read_board_bitboard_from_ram(ram: &[u8]) -> [u16; BOARD_ROWS] {
    let mut bb = [0u16; BOARD_ROWS];
    for row in 0..BOARD_ROWS {
        let base = 0x800 + row * BOARD_STRIDE + 2;
        for col in 0..BOARD_COLS {
            if is_occupied(ram.get(base + col).copied().unwrap_or(0)) {
                bb[row] |= 1 << col;
            }
        }
    }
    bb
}

fn load_savestate(name: &str) -> EmulatorState {
    let path = misdrop_fixture(name);
    let b64 = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim().trim_matches('"'))
        .expect("base64");
    bincode::deserialize(&bytes).expect("EmulatorState")
}

fn spawn_from_state(state: &EmulatorState) -> (i32, usize, usize) {
    let read = |a: u16| *state.ram.get(a as usize).unwrap_or(&0);
    let row = piece_min_row(&read);
    let col = piece_left_col(&read);
    let rot = ori_info(read(ADDR_CUR_ORI))
        .map(|(_, r)| r as usize)
        .unwrap_or(0);
    (row, col, rot)
}

fn bfs_has_lock(moves: &[BfsLockedMove], want: &WantLock) -> bool {
    moves.iter().any(|m| {
        m.row == want.row && m.col == want.col as i32 && m.rot == want.rot
    })
}

#[test]
fn planner_manifest_fixtures_have_bfs_lock_or_recorded_path() {
    let manifest: Manifest = serde_json::from_str(MANIFEST).expect("manifest");
    assert_eq!(manifest.version, 2);
    let mut checked = 0usize;

    for entry in &manifest.fixtures {
        let planner = entry.claims.iter().find(|c| c.kind == ClaimKind::PlannerReachability);
        let Some(planner) = planner else {
            continue;
        };
        let Some(want) = &entry.want_lock else {
            continue;
        };
        let Some(type_idx) = piece_type_idx(&entry.piece) else {
            continue;
        };

        let state = load_savestate(&entry.b64);
        let bb = read_board_bitboard_from_ram(&state.ram);
        let (live_row, live_col, live_rot) = spawn_from_state(&state);
        let (spawn_row, spawn_col, spawn_rot) = entry
            .spawn
            .as_ref()
            .map(|s| (s.row, s.col, s.rot))
            .unwrap_or((live_row, live_col, live_rot));

        let moves = bfs_moves(&bb, type_idx, spawn_row, spawn_col, spawn_rot);

        let expect_pass = planner.enforced_in_ci && planner.baseline.as_deref() == Some("pass");
        let expect_unreachable =
            planner.enforced_in_ci && planner.baseline.as_deref() == Some("fail_unreachable");
        if expect_pass {
            assert!(
                bfs_has_lock(&moves, want),
                "{}: BFS must reach want_lock ({},{},r{}) from spawn ({},{},r{})",
                entry.id, want.row, want.col, want.rot, spawn_row, spawn_col, spawn_rot
            );
        } else if expect_unreachable {
            assert!(
                !bfs_has_lock(&moves, want),
                "{}: BFS must not reach want_lock ({},{},r{}) under floor SRS",
                entry.id, want.row, want.col, want.rot
            );
        } else if bfs_has_lock(&moves, want) {
            eprintln!(
                "{}: BFS now reaches want_lock ({},{},r{}) — consider promoting planner claim to proven",
                entry.id, want.row, want.col, want.rot
            );
        }

        if expect_pass || expect_unreachable || planner.baseline.is_some() {
            if let Some(path) = &entry.recorded_path {
                let sim_ok = simulate_path_stepwise(
                    &bb, type_idx, spawn_row, spawn_col as i32, spawn_rot, path,
                )
                .is_some();
                if expect_pass {
                    assert!(
                        sim_ok,
                        "{}: recorded_path must be simulatable from spawn ({},{},r{})",
                        entry.id, spawn_row, spawn_col, spawn_rot
                    );
                } else if expect_unreachable {
                    // Path may still sim to a *different* hardware lock (Z floor up-same-col);
                    // it must not end at the fiction want.
                    let end = simulate_path_stepwise(
                        &bb, type_idx, spawn_row, spawn_col as i32, spawn_rot, path,
                    );
                    assert!(
                        end.is_none_or(|(r, c, rot)| {
                            r != want.row || c != want.col as i32 || rot != want.rot
                        }),
                        "{}: recorded_path must not reach fiction want under floor SRS (got {end:?})",
                        entry.id
                    );
                } else if sim_ok {
                    eprintln!(
                        "{}: recorded_path now simulates — planner parity improved",
                        entry.id
                    );
                }
            }
        }

        checked += 1;
    }

    if checked == 0 {
        eprintln!("planner_golden: no planner_reachability fixtures yet (fresh catalog)");
    }
}

#[test]
fn planner_ci_fixture_count() {
    let manifest: ManifestContract = serde_json::from_str(MANIFEST).expect("manifest");
    let n = manifest
        .fixtures
        .iter()
        .filter(|e| e.has_ci_claim(ClaimKind::PlannerReachability))
        .count();
    eprintln!("planner CI fixtures: {n}");
}