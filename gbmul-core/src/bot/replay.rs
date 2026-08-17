//! Misdrop replay metadata (host pairs with full emulator savestate).

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PieceInfo {
    pub piece_type: usize,
    pub rot: usize,
    pub spawn_col: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NextPiece {
    pub piece_type: usize,
    pub ori: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MisdropContext {
    pub num: u32,
    pub total: u32,
    pub wanted_col: usize,
    pub wanted_rot: usize,
    #[serde(default)]
    pub wanted_row: Option<i32>,
    pub actual_col: usize,
    pub actual_rot: usize,
    #[serde(default)]
    pub actual_row: Option<i32>,
    #[serde(default)]
    pub got_valid: bool,
    pub move_type: String,
    pub path_len: usize,
    #[serde(default)]
    pub path: Option<Vec<String>>,
    pub critical: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PlacementReplay {
    pub version: String,
    pub timestamp: String,
    #[serde(default)]
    pub source: Option<String>,
    pub current_piece: PieceInfo,
    pub next_piece: NextPiece,
    #[serde(default)]
    pub misdrop: Option<MisdropContext>,
    #[serde(default)]
    pub strategy: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub pps: Option<String>,
    #[serde(default)]
    pub note: String,
}

impl PlacementReplay {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}

/// One piece lock — persisted by host for MCP/debug (survives page reload).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LockAuditEntry {
    pub drop_num: u32,
    pub frame: u32,
    pub piece: String,
    pub mtype: String,
    #[serde(default)]
    pub want_row: Option<i32>,
    #[serde(default)]
    pub want_col: Option<usize>,
    #[serde(default)]
    pub want_rot: Option<usize>,
    pub sprite_row: i32,
    pub sprite_col: usize,
    pub sprite_rot: usize,
    #[serde(default)]
    pub board_row: Option<i32>,
    #[serde(default)]
    pub board_col: Option<i32>,
    #[serde(default)]
    pub board_rot: Option<usize>,
    pub verify: String,
    pub misdrop_detected: bool,
    #[serde(default)]
    pub path: Vec<String>,
}