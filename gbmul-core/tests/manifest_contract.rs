//! Manifest v2 contract — ids, board_id fingerprints, claim/knowledge consistency.

use base64::Engine;
use gbmul_core::bot::fixture_manifest::{
    board_id_from_ram, ClaimKind, FixtureEntry, Knowledge, ManifestContract,
};
use gbmul_core::bot::fixtures::misdrop_fixture;
use gbmul_core::state::EmulatorState;

const MANIFEST: &str = include_str!("fixtures/misdrop/manifest.json");

fn load_manifest() -> ManifestContract {
    let m: ManifestContract = serde_json::from_str(MANIFEST).expect("parse manifest.json");
    assert_eq!(m.version, 2, "manifest must be version 2");
    m
}

fn load_board_id(b64_name: &str) -> Option<String> {
    let path = misdrop_fixture(b64_name);
    let b64 = std::fs::read_to_string(&path).ok()?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim().trim_matches('"'))
        .ok()?;
    let state: EmulatorState = bincode::deserialize(&bytes).ok()?;
    Some(board_id_from_ram(&state.ram))
}

#[test]
fn manifest_v2_unique_ids_and_board_ids() {
    let manifest = load_manifest();
    let mut ids = std::collections::HashSet::new();
    for entry in &manifest.fixtures {
        assert!(ids.insert(entry.id.clone()), "duplicate fixture id: {}", entry.id);
        assert!(
            !entry.id.contains("_to_"),
            "{}: fixture id must not use piece-pair slug (use legacy_id / pair_label)",
            entry.id
        );
        assert!(!entry.claims.is_empty(), "{}: must declare at least one claim", entry.id);
    }
    // Fresh catalog may be empty until new captures are imported.
}

#[test]
fn manifest_board_id_matches_savestate() {
    let manifest = load_manifest();
    for entry in &manifest.fixtures {
        let Some(want) = &entry.board_id else {
            continue;
        };
        let computed = load_board_id(&entry.b64).unwrap_or_else(|| {
            panic!(
                "{}: board_id {want} declared but b64 {} is not bincode EmulatorState",
                entry.id, entry.b64
            )
        });
        assert_eq!(
            computed, *want,
            "{}: board_id mismatch (computed {computed}, manifest {want})",
            entry.id
        );
    }
}

#[test]
fn manifest_proven_claims_have_ci_or_test_hook() {
    let manifest = load_manifest();
    for entry in &manifest.fixtures {
        for claim in &entry.claims {
            if claim.knowledge != Knowledge::Proven {
                continue;
            }
            match claim.kind {
                ClaimKind::SrsNegative => {}
                ClaimKind::SpawnCapture => {
                    assert!(
                        claim.enforced_in_ci,
                        "{}: proven spawn_capture must be enforced_in_ci",
                        entry.id
                    );
                }
                ClaimKind::PlannerReachability => {
                    assert!(
                        claim.enforced_in_ci,
                        "{}: proven planner_reachability must be enforced_in_ci",
                        entry.id
                    );
                    assert!(
                        matches!(
                            claim.baseline.as_deref(),
                            Some("pass") | Some("fail_unreachable")
                        ),
                        "{}: proven planner must baseline pass or fail_unreachable",
                        entry.id
                    );
                }
                ClaimKind::MisdropDetection | ClaimKind::ExecutorPath => {
                    // May be proven in unit tests without emu CI yet.
                    if claim.enforced_in_ci {
                        assert!(
                            claim.baseline.as_deref() == Some("pass"),
                            "{}: CI-enforced {:?} must baseline pass",
                            entry.id,
                            claim.kind
                        );
                    }
                }
                ClaimKind::AuxiliaryBoard => {
                    panic!("{}: auxiliary_board cannot be proven", entry.id);
                }
            }
        }
    }
}

#[test]
fn manifest_srs_negative_does_not_imply_executor() {
    let manifest = load_manifest();
    for entry in &manifest.fixtures {
        let srs_only = entry.claims.len() == 1
            && entry.claims[0].kind == ClaimKind::SrsNegative;
        if srs_only {
            assert!(
                entry.generalizes != Some(true),
                "{}: srs_negative must not generalize",
                entry.id
            );
            assert!(
                !entry.has_ci_claim(ClaimKind::ExecutorPath),
                "{}: srs_negative must not claim executor_path in CI",
                entry.id
            );
        }
    }
}

/// Filter used by planner_golden and replay_restore.
pub fn planner_ci_fixtures(manifest: &ManifestContract) -> impl Iterator<Item = &FixtureEntry> {
    manifest.fixtures.iter().filter(|e| {
        e.has_ci_claim(ClaimKind::PlannerReachability) && e.want_lock.is_some()
    })
}

/// Filter used by rosy_golden_pass.
pub fn executor_ci_fixtures(manifest: &ManifestContract) -> impl Iterator<Item = &FixtureEntry> {
    manifest.fixtures.iter().filter(|e| {
        e.has_ci_claim(ClaimKind::ExecutorPath)
            && e.want_lock.is_some()
            && !matches!(e.claim(ClaimKind::AuxiliaryBoard), Some(_))
    })
}

/// Dev helper after import: `cargo test manifest_board_id_for -- --nocapture`
#[test]
fn manifest_board_id_for() {
    let manifest = load_manifest();
    for entry in &manifest.fixtures {
        if let Some(computed) = load_board_id(&entry.b64) {
            eprintln!("{} ({}) -> {}", entry.id, entry.b64, computed);
        } else {
            eprintln!("{} ({}) -> SKIP invalid savestate", entry.id, entry.b64);
        }
    }
}