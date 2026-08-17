//! CLI: metadata for a misdrop fixture savestate + optional sidecar meta JSON.
//!
//!   cargo run -p gbmul-core --bin fixture_meta -- <state.b64> [meta.json]

use std::env;
use std::fs;
use std::path::PathBuf;

use base64::Engine;
use gbmul_core::bot::fixture_manifest::board_id_from_ram;
use gbmul_core::bot::misdrop::{evaluate_lock, resolve_mtype_for_row, LockActual, LockExpectation};
use gbmul_core::bot::{at_true_spawn, ori_info, piece_left_col, piece_min_row, ADDR_CUR_ORI};
use gbmul_core::state::EmulatorState;
use serde::Deserialize;
use serde_json::json;

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

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/misdrop")
}

fn load_state(name: &str) -> EmulatorState {
    let path = fixture_dir().join(name);
    let b64 = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim().trim_matches('"'))
        .expect("base64 decode");
    bincode::deserialize(&bytes).expect("EmulatorState bincode")
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let b64_name = args.get(1).expect("usage: fixture_meta <state.b64> [meta.json]");
    let state = load_state(b64_name);
    let read = |a: u16| *state.ram.get(a as usize).unwrap_or(&0);

    let spawn_row = piece_min_row(&read);
    let spawn_col = piece_left_col(&read);
    let spawn_rot = ori_info(read(ADDR_CUR_ORI))
        .map(|(_, r)| r as usize)
        .unwrap_or(0);

    let mut out = json!({
        "board_id": board_id_from_ram(&state.ram),
        "spawn": { "row": spawn_row, "col": spawn_col, "rot": spawn_rot },
        "at_true_spawn": at_true_spawn(&read),
    });

    if let Some(meta_name) = args.get(2) {
        let meta_path = fixture_dir().join(meta_name);
        let raw = fs::read_to_string(&meta_path).expect("read meta");
        let sidecar: SidecarMeta = serde_json::from_str(&raw).expect("parse meta");
        if let Some(m) = sidecar.misdrop {
            let mtype = m.move_type.as_deref().unwrap_or("unknown");
            let path = m.path.clone().unwrap_or_default();
            // Normal fast-path: BFS plan is stored for replay but row verify used mtype normal.
            let path_mtype = if path.is_empty() || mtype == "normal" {
                mtype
            } else {
                gbmul_core::bot::path_terminal_mtype(&path)
            };
            let mtype_for_row = resolve_mtype_for_row(mtype, path_mtype);

            if let (Some(wc), Some(wr), Some(wrow)) = (m.wanted_col, m.wanted_rot, m.wanted_row)
            {
                if let (Some(gc), Some(gr), Some(grow)) =
                    (m.actual_col, m.actual_rot, m.actual_row)
                {
                    let exp = LockExpectation {
                        want_col: wc,
                        want_rot: wr,
                        want_row: Some(wrow),
                        mtype_for_row,
                    };
                    let act = LockActual {
                        col: gc,
                        rot: gr,
                        eff_row: grow,
                    };
                    let verdict = evaluate_lock(&exp, &act, &path);
                    out["evaluate_lock"] = json!(format!("{verdict:?}"));
                    out["should_count_misdrop"] = json!(verdict.is_some());
                }
            }
            out["move_type"] = json!(if path.is_empty() { mtype } else { path_mtype });
            out["stored_move_type"] = json!(mtype);
            out["path_terminal_mtype"] = json!(path_mtype);
            out["wanted"] = json!({
                "row": m.wanted_row, "col": m.wanted_col, "rot": m.wanted_rot
            });
            out["actual"] = json!({
                "row": m.actual_row, "col": m.actual_col, "rot": m.actual_rot
            });
        }
    }

    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}