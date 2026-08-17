//! Emulator RAM read helpers — active piece pose and locked board bitboard.

use super::board::{
    is_garbage_tile, is_occupied, ori_info, BOARD_BASE, BOARD_COLS, BOARD_ROWS, BOARD_STRIDE,
    ADDR_CUR_ORI, PIX_CELL, PIX_X_OFF, PIX_Y_OFF, SHAPES, SQ_ADDRS,
};

#[derive(Debug, Clone, Copy)]
pub struct CellPos {
    pub row: i32,
    pub col: i32,
}

/// Read the 4 pixel positions of the active piece and convert to board cells.
pub fn read_active_piece(read_mem: impl Fn(u16) -> u8) -> [CellPos; 4] {
    let mut out = [CellPos { row: 0, col: 0 }; 4];
    for (i, &[y_addr, x_addr]) in SQ_ADDRS.iter().enumerate() {
        let py = read_mem(y_addr) as i32;
        let px = read_mem(x_addr) as i32;
        out[i] = CellPos {
            row: (py - PIX_Y_OFF) / PIX_CELL,
            col: (px - PIX_X_OFF) / PIX_CELL,
        };
    }
    out
}

pub fn read_current_ori(read_mem: impl Fn(u16) -> u8) -> u8 {
    read_mem(ADDR_CUR_ORI)
}

/// Read the locked board as bitboard (same as before, but as fn).
pub fn read_board_bitboard(read_mem_range: impl Fn(u16, u16) -> Vec<u8>) -> [u16; BOARD_ROWS] {
    let raw = read_mem_range(BOARD_BASE, (BOARD_ROWS * BOARD_STRIDE) as u16);
    let mut bb = [0u16; BOARD_ROWS];
    for row in 0..BOARD_ROWS {
        let base = row * BOARD_STRIDE + 2;
        let mut bits = 0u16;
        for col in 0..BOARD_COLS {
            if is_occupied(raw[base + col]) {
                bits |= 1 << col;
            }
        }
        bb[row] = bits;
    }
    bb
}

/// Count garbage tiles (0x28) on the board — used for 2P garbage arrival detection.
pub fn count_garbage_tiles(read_mem_range: impl Fn(u16, u16) -> Vec<u8>) -> u32 {
    let raw = read_mem_range(BOARD_BASE, (BOARD_ROWS * BOARD_STRIDE) as u16);
    let mut count = 0u32;
    for row in 0..BOARD_ROWS {
        let base = row * BOARD_STRIDE + 2;
        for col in 0..BOARD_COLS {
            if is_garbage_tile(raw[base + col]) {
                count += 1;
            }
        }
    }
    count
}

/// Returns true when the 4 active-piece pixel registers show a freshly-spawned
/// piece at the top (minRow ≤ 2, allowing negative for pieces that spawn
/// partially above the board) and all 4 positions are distinct.
/// This filters out the pre-spawn / ARE garbage state (all -2,-3 or similar).
pub fn is_piece_valid_at_top(read_mem: impl Fn(u16) -> u8) -> bool {
    let mut positions: Vec<(i32, i32)> = Vec::new();
    for &[y_addr, x_addr] in &SQ_ADDRS {
        let py = read_mem(y_addr) as i32;
        let px = read_mem(x_addr) as i32;
        let row = (py - PIX_Y_OFF) / PIX_CELL;
        let col = (px - PIX_X_OFF) / PIX_CELL;
        positions.push((row, col));
    }
    let min_row = positions.iter().map(|(r, _)| *r).min().unwrap_or(99);
    let max_row = positions.iter().map(|(r, _)| *r).max().unwrap_or(99);
    if min_row > 2 || max_row > 4 {
        return false;
    }
    for &(_, c) in &positions {
        if c < 0 || c >= BOARD_COLS as i32 {
            return false;
        }
    }
    let unique: std::collections::HashSet<_> = positions.into_iter().collect();
    unique.len() == 4
}

pub fn piece_left_col(read_mem: impl Fn(u16) -> u8) -> usize {
    let mut min_col = usize::MAX;
    for &[_ , x_addr] in &SQ_ADDRS {
        let px = read_mem(x_addr) as i32;
        let col = ((px - PIX_X_OFF) / PIX_CELL) as i32;
        if col >= 0 && (col as usize) < BOARD_COLS && (col as usize) < min_col {
            min_col = col as usize;
        }
    }
    if min_col == usize::MAX {
        0
    } else {
        min_col
    }
}

pub fn piece_min_pixel_y(read_mem: impl Fn(u16) -> u8) -> i32 {
    let mut min = i32::MAX;
    for &[y_addr, _] in &SQ_ADDRS {
        let py = read_mem(y_addr) as i32;
        if py < min {
            min = py;
        }
    }
    if min == i32::MAX {
        0
    } else {
        min
    }
}

pub fn piece_min_row(read_mem: impl Fn(u16) -> u8) -> i32 {
    let mut min = i32::MAX;
    for &[y_addr, _] in &SQ_ADDRS {
        let py = read_mem(y_addr) as i32;
        let row = (py - PIX_Y_OFF) / PIX_CELL;
        if row < min {
            min = row;
        }
    }
    if min == i32::MAX {
        0
    } else {
        min
    }
}

/// Mirrors `rememberSpawnFullState` in www/index.js — true spawn, not gravity-advanced row 1.
pub fn at_true_spawn(read_mem: impl Fn(u16) -> u8) -> bool {
    let min_row = piece_min_row(&read_mem);
    let min_y = piece_min_pixel_y(&read_mem);
    min_row <= 0 && min_y < 0x18
}

pub fn at_top_shape_matches_ori(read_mem: impl Fn(u16) -> u8, ori: u8) -> bool {
    if !is_piece_valid_at_top(|a| read_mem(a)) {
        return false;
    }
    let info = ori_info(ori);
    if info.is_none() {
        return false;
    }
    let info = info.unwrap();
    let mut squares: Vec<(i32, i32)> = Vec::new();
    for &[y_addr, x_addr] in &SQ_ADDRS {
        let py = read_mem(y_addr) as i32;
        let px = read_mem(x_addr) as i32;
        let r = (py - PIX_Y_OFF) / PIX_CELL;
        let c = (px - PIX_X_OFF) / PIX_CELL;
        squares.push((r, c));
    }
    let min_r = squares.iter().map(|(r, _)| *r).min().unwrap_or(0);
    let min_c = squares.iter().map(|(_, c)| *c).min().unwrap_or(0);
    let mut actual: Vec<(i32, i32)> = squares
        .into_iter()
        .map(|(r, c)| (r - min_r, c - min_c))
        .collect();
    actual.sort();
    let mut expected: Vec<(i32, i32)> = SHAPES[info.0][info.1 as usize]
        .iter()
        .map(|&[dr, dc]| (dr as i32, dc as i32))
        .collect();
    expected.sort();
    actual.len() == 4 && actual.iter().zip(expected.iter()).all(|(a, e)| a == e)
}