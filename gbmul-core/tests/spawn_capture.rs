//! Phase 4 — spawn capture parity with www/index.js `rememberSpawnFullState`.
//!
//! Pairing contract (see BOT_REFACTOR_AND_TEST_PLAN.md §6.2):
//! - Misdrop export uses `_misdropPairingState` frozen at current-piece spawn.
//! - Rust `has_pending_misdrop_pairing()` must stay true until replay drains / path ends.
//! - Savestates attached to `spawn_capture: proven` fixtures must pass `at_true_spawn`.

use base64::Engine;
use gbmul_core::bot::fixture_manifest::{ClaimKind, ManifestContract};
use gbmul_core::bot::fixtures::misdrop_fixture;
use gbmul_core::bot::{at_true_spawn, piece_min_row};
use gbmul_core::state::EmulatorState;
use serde::Deserialize;

const MANIFEST: &str = include_str!("fixtures/misdrop/manifest.json");

#[derive(Debug, Deserialize)]
struct SpawnParams {
    row: i32,
}

#[derive(Debug, Deserialize)]
struct FixtureEntry {
    id: String,
    b64: String,
    spawn: Option<SpawnParams>,
    claims: Vec<gbmul_core::bot::fixture_manifest::Claim>,
}

#[derive(Debug, Deserialize)]
struct Manifest {
    version: u32,
    fixtures: Vec<FixtureEntry>,
}

fn load_savestate(name: &str) -> EmulatorState {
    let path = misdrop_fixture(name);
    let mut b64 = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
        .trim()
        .trim_matches('"')
        .to_string();
    while !b64.is_empty() && b64.len() % 4 != 0 {
        b64.push('=');
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&b64)
        .unwrap_or_else(|e| panic!("b64 {}: {e}", path.display()));
    bincode::deserialize(&bytes).expect("EmulatorState")
}

#[test]
fn spawn_role_fixtures_are_at_true_spawn() {
    let manifest: Manifest = serde_json::from_str(MANIFEST).expect("manifest");
    assert_eq!(manifest.version, 2);
    for entry in manifest.fixtures.iter().filter(|e| {
        e.claims
            .iter()
            .any(|c| c.kind == ClaimKind::SpawnCapture && c.enforced_in_ci)
    }) {
        let state = load_savestate(&entry.b64);
        let read = |a: u16| *state.ram.get(a as usize).unwrap_or(&0);
        assert!(
            at_true_spawn(&read),
            "{}: spawn_capture fixture must satisfy at_true_spawn",
            entry.id
        );
    }
}

#[test]
fn negative_spawn_fixtures_match_manifest_row() {
    let manifest: Manifest = serde_json::from_str(MANIFEST).expect("manifest");
    for entry in manifest
        .fixtures
        .iter()
        .filter(|e| e.spawn.as_ref().is_some_and(|s| s.row < 0))
    {
        let state = load_savestate(&entry.b64);
        let read = |a: u16| *state.ram.get(a as usize).unwrap_or(&0);
        let live_row = piece_min_row(&read);
        let want_row = entry.spawn.as_ref().unwrap().row;
        assert_eq!(
            live_row, want_row,
            "{}: live spawn row must match manifest (captured above field)",
            entry.id
        );
    }
}

#[test]
fn misdrop_fixtures_with_spawn_claim_have_board_id() {
    let manifest: ManifestContract =
        serde_json::from_str(MANIFEST).expect("manifest");
    for entry in manifest.fixtures.iter().filter(|e| {
        e.claims
            .iter()
            .any(|c| c.kind == ClaimKind::SpawnCapture)
    }) {
        assert!(
            entry.board_id.is_some(),
            "{}: spawn_capture fixture must declare board_id",
            entry.id
        );
    }
}

