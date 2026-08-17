//! Misdrop fixture manifest contract (manifest.json version 2).
//!
//! Fixture **ids** name the capability + board lock target, not piece pairing.
//! `piece` / `next` are metadata only (2-ply context).

use serde::Deserialize;

use super::{is_occupied, BOARD_COLS, BOARD_ROWS, BOARD_STRIDE};

/// What a fixture actually asserts — not what piece types were active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimKind {
    /// BFS / stepwise sim reaches `want_lock` from `spawn` on this bitboard.
    PlannerReachability,
    /// Real emulator executes `recorded_path` to `want_lock` without misdrop.
    ExecutorPath,
    /// SRS / kick table rejects a specific lock (negative test).
    SrsNegative,
    /// `begin_drop` / path guards must not false-positive on this board+path.
    MisdropDetection,
    /// Savestate is a true spawn snapshot (`at_true_spawn`).
    SpawnCapture,
    /// Board capture for pairing / sim-negative probes — not an end-to-end contract.
    AuxiliaryBoard,
}

/// How much we actually know — vs what pairing labels used to imply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Knowledge {
    /// CI-enforced; test fails on regression.
    Proven,
    /// Known gap recorded via `baseline` (fail_*); not a regression signal.
    DocumentedGap,
    /// No automated check yet.
    Untested,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Claim {
    pub kind: ClaimKind,
    pub knowledge: Knowledge,
    #[serde(default)]
    pub enforced_in_ci: bool,
    pub baseline: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WantLock {
    pub row: i32,
    pub col: usize,
    pub rot: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SpawnParams {
    pub row: i32,
    pub col: usize,
    pub rot: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FixtureEntry {
    pub id: String,
    pub b64: String,
    /// Stable fingerprint of locked cells. Omitted when savestate is not bincode-valid.
    #[serde(default)]
    pub board_id: Option<String>,
    #[serde(default)]
    pub legacy_id: Option<String>,
    #[serde(default)]
    pub pair_label: Option<String>,
    #[serde(default)]
    pub meta: Option<String>,
    pub piece: String,
    #[serde(default)]
    pub next: Option<String>,
    #[serde(default)]
    pub spawn: Option<SpawnParams>,
    #[serde(default)]
    pub class: Option<String>,
    #[serde(default)]
    pub recorded_path: Option<Vec<String>>,
    #[serde(default)]
    pub want_lock: Option<WantLock>,
    pub claims: Vec<Claim>,
    #[serde(default)]
    pub generalizes: Option<bool>,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ManifestContract {
    pub version: u32,
    #[serde(default)]
    pub contract: Option<serde_json::Value>,
    pub rom_sha1: String,
    pub fixtures: Vec<FixtureEntry>,
}

impl FixtureEntry {
    pub fn claim(&self, kind: ClaimKind) -> Option<&Claim> {
        self.claims.iter().find(|c| c.kind == kind)
    }

    pub fn has_ci_claim(&self, kind: ClaimKind) -> bool {
        self.claims
            .iter()
            .any(|c| c.kind == kind && c.enforced_in_ci)
    }

    pub fn planner_baseline(&self) -> Option<&str> {
        self.claim(ClaimKind::PlannerReachability)
            .and_then(|c| c.baseline.as_deref())
    }

    pub fn executor_baseline(&self) -> Option<&str> {
        self.claim(ClaimKind::ExecutorPath)
            .and_then(|c| c.baseline.as_deref())
    }
}

/// FNV-1a 32-bit over locked playfield rows (cols 0–9 only).
pub fn board_id_from_ram(ram: &[u8]) -> String {
    let mut bb = [0u16; BOARD_ROWS];
    for row in 0..BOARD_ROWS {
        let base = 0x800 + row * BOARD_STRIDE + 2;
        for col in 0..BOARD_COLS {
            if is_occupied(ram.get(base + col).copied().unwrap_or(0)) {
                bb[row] |= 1 << col;
            }
        }
    }
    let mut hash: u32 = 0x811c9dc5;
    for (i, bits) in bb.iter().enumerate() {
        let packed = (*bits as u32) ^ (i as u32);
        hash ^= packed;
        hash = hash.wrapping_mul(0x01000193);
    }
    format!("{hash:08x}")
}

pub fn locked_bitboard_from_ram(ram: &[u8]) -> [u16; BOARD_ROWS] {
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