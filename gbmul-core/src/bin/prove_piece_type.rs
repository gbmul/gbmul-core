//! One-shot: prove piece_type index from a misdrop savestate.
//!   cargo run -p gbmul-core --bin prove_piece_type -- <state.b64>

use std::env;
use std::fs;

use base64::Engine;
use gbmul_core::bot::{
    ori_info, piece_left_col, piece_min_row, PIECE_NAMES, SHAPES, SQ_ADDRS, ADDR_CUR_ORI,
    ADDR_NEXT_ORI,
};
use gbmul_core::state::EmulatorState;

fn fixture_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/misdrop")
}

fn main() {
    let name = env::args().nth(1).expect("usage: prove_piece_type <state.b64>");
    let path = fixture_dir().join(&name);
    let b64 = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim().trim_matches('"'))
        .expect("b64");
    let state: EmulatorState = bincode::deserialize(&bytes).expect("state");
    let read = |a: u16| *state.ram.get(a as usize).unwrap_or(&0);

    let cur_ori = read(ADDR_CUR_ORI);
    let next_ori = read(ADDR_NEXT_ORI);
    let (type_idx, rot) = ori_info(cur_ori).expect("valid cur ori");
    let (next_idx, _) = ori_info(next_ori).expect("valid next ori");

    println!("savestate: {name}");
    println!("  RAM[$C203] cur_ori  = 0x{cur_ori:02X} → type_idx={type_idx} ({}) rot={rot}", PIECE_NAMES[type_idx]);
    println!("  RAM[$C213] next_ori = 0x{next_ori:02X} → type_idx={next_idx} ({})", PIECE_NAMES[next_idx]);
    println!("  spawn row={} col={}", piece_min_row(&read), piece_left_col(&read));

    // Geometry: active piece squares in pixel RAM → cell offsets → match SHAPES[type_idx][rot]
    let mut cells = Vec::new();
    for &[y_a, x_a] in &SQ_ADDRS {
        let py = read(y_a) as i32;
        let px = read(x_a) as i32;
        let row = (py - 16) / 8;
        let col = (px - 24) / 8;
        cells.push((row, col));
    }
    cells.sort();
    let min_r = cells[0].0;
    let min_c = cells.iter().map(|c| c.1).min().unwrap();
    let norm: Vec<(i8, i8)> = cells
        .iter()
        .map(|(r, c)| ((r - min_r) as i8, (c - min_c) as i8))
        .collect();
    let shape = &SHAPES[type_idx][rot as usize];
    let mut shape_norm: Vec<(i8, i8)> = shape.iter().map(|&[dr, dc]| (dr, dc)).collect();
    shape_norm.sort();
    let mut norm_sorted = norm.clone();
    norm_sorted.sort();
    println!("  live cells (norm): {norm_sorted:?}");
    println!("  SHAPES[{type_idx}][r{rot}] (norm): {shape_norm:?}");
    println!(
        "  geometry matches type_idx {type_idx} ({}): {}",
        PIECE_NAMES[type_idx],
        norm_sorted == shape_norm
    );

    // Wrong import mapping would use type 0 = O
    let wrong_o_shape: Vec<(i8, i8)> = SHAPES[1][0].iter().map(|&[dr, dc]| (dr, dc)).collect();
    println!(
        "  matches O (wrong import idx 0): {}",
        norm_sorted == {
            let mut s = wrong_o_shape;
            s.sort();
            s
        }
    );
}