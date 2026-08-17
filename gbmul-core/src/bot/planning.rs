//! Move evaluation and 1-ply / 2-ply BFS planning (Meatfighter weights).

use super::board::{
    column_heights, ori_info, Bitboard, Shape, BOARD_COLS, BOARD_ROWS, ADDR_CUR_ORI, SHAPES,
};

use super::bfs::{bfs_moves, bfs_path_reaches_lock, BfsLockedMove};
use super::memory::read_board_bitboard;

/// Weights for evaluation (from original JS).
struct Weights {
    clears: i32,
    holes: i32,
    wells: i32,
    row_trans: i32,
    col_trans: i32,
    quad_well: i32,
    landing: i32,
}

const W: Weights = Weights {
    clears: 10,
    holes: -35,
    wells: -8,
    row_trans: -5,
    col_trans: -12,
    quad_well: 20,
    landing: -2,
};

/// Core board evaluator (ported from JS evaluate).
pub fn evaluate(bb: &Bitboard, heights: &[u8], clears: u32) -> i32 {
    use super::board::popcount;

    let mut row_trans = 0i32;
    let mut col_trans = 0i32;
    let mut holes = 0i32;
    let mut wells = 0i32;
    let mut quad_well = 0i32;

    for r in 0..BOARD_ROWS {
        if bb[r] != 0 {
            row_trans += popcount((bb[r] >> 1) ^ bb[r]) as i32;
        }
    }

    for col in 0..BOARD_COLS {
        let mut prev = false;
        let top_row = BOARD_ROWS - heights[col] as usize;
        for row in top_row..BOARD_ROWS {
            let filled = (bb[row] & (1 << col)) != 0;
            if filled != prev {
                col_trans += 1;
                if !filled {
                    holes += 1;
                }
            }
            prev = filled;
        }
    }

    let mut lowest_h = i32::MAX;
    for col in 0..BOARD_COLS {
        let h = heights[col] as i32;
        if h < lowest_h {
            lowest_h = h;
        }
        let lh = if col == 0 {
            BOARD_ROWS as i32
        } else {
            heights[col - 1] as i32
        };
        let rh = if col == BOARD_COLS - 1 {
            BOARD_ROWS as i32
        } else {
            heights[col + 1] as i32
        };
        let mut n = h;
        let mut count = 0;
        while n < lh && n < rh {
            count = count + count + 1;
            n += 1;
        }
        wells += count;
    }

    let mut q_row = BOARD_ROWS as i32 - lowest_h - 1;
    while q_row >= 0 && popcount(bb[q_row as usize]) >= (BOARD_COLS - 1) as u32 {
        quad_well += 1;
        q_row -= 1;
    }

    (clears * clears * W.clears as u32) as i32
        + holes * W.holes
        + wells * W.wells
        + row_trans * W.row_trans
        + col_trans * W.col_trans
        + quad_well * W.quad_well
}

/// Default evaluator with landing penalty.
pub fn default_evaluate(bb: &Bitboard, heights: &[u8], clears: u32, land_row: usize) -> i32 {
    evaluate(bb, heights, clears) + (BOARD_ROWS as i32 - land_row as i32) * W.landing
}

/// Check if a piece shape can reach targetCol from spawnCol by sliding at row 0.
pub fn is_reachable(bb: &Bitboard, shape: &Shape, spawn_col: usize, target_col: usize) -> bool {
    let step = if target_col > spawn_col { 1 } else { -1 };
    let mut col = spawn_col as i32;
    while col != target_col as i32 {
        col += step;
        for &[dr, dc] in shape {
            let r = dr;
            let c = col + step + dc as i32;
            if c < 0 || c >= BOARD_COLS as i32 {
                return false;
            }
            if r >= 0 && (r as i32) < BOARD_ROWS as i32 && (bb[r as usize] & (1 << c)) != 0 {
                return false;
            }
        }
    }
    true
}

/// Find best (rot, left_col) placement for the current piece.
pub fn find_best_move(
    read_mem: impl Fn(u16) -> u8,
    read_range: impl Fn(u16, u16) -> Vec<u8>,
    from_col: Option<usize>,
    force_rot: i32,
    evaluate_fn: fn(&Bitboard, &[u8], u32, usize) -> i32,
) -> Option<(usize, usize)> {
    let bb = read_board_bitboard(read_range);
    let ori = read_mem(ADDR_CUR_ORI);
    let info = ori_info(ori)?;

    const FULL_ROW: u16 = (1 << BOARD_COLS) - 1;
    let spawn_col = from_col.unwrap_or(3);

    let mut best_score = i32::MIN;
    let mut best_move = None;

    for rot in 0..4 {
        if force_rot >= 0 && rot != force_rot as usize {
            continue;
        }
        let shape = &SHAPES[info.0][rot];
        let max_dc = shape.iter().map(|&[_, c]| c).max().unwrap_or(0);
        let max_dr = shape.iter().map(|&[r, _]| r).max().unwrap_or(0);
        let left_col_max = BOARD_COLS - 1 - max_dc as usize;

        for left_col in 0..=left_col_max {
            if !is_reachable(&bb, shape, spawn_col, left_col) {
                continue;
            }

            let mut land_row = -1i32;
            'find_land: for start_row in 0..=(BOARD_ROWS as i32 - 1 - max_dr as i32) {
                for &[dr, dc] in shape {
                    let r = start_row + dr as i32;
                    let c = left_col as i32 + dc as i32;
                    if r >= 0 && r < BOARD_ROWS as i32 && (bb[r as usize] & (1 << c)) != 0 {
                        land_row = start_row - 1;
                        break 'find_land;
                    }
                }
                land_row = start_row;
            }
            if land_row < 0 {
                continue;
            }

            let mut test = bb;
            for &[dr, dc] in shape {
                let r = land_row + dr as i32;
                if r >= 0 && r < BOARD_ROWS as i32 {
                    test[r as usize] |= 1 << (left_col as i32 + dc as i32);
                }
            }

            let mut clears = 0u32;
            let mut kept = vec![];
            for r in 0..BOARD_ROWS {
                if test[r] == FULL_ROW {
                    clears += 1;
                } else {
                    kept.push(test[r]);
                }
            }
            if clears > 0 {
                let mut cleared = [0u16; BOARD_ROWS];
                for (i, &k) in kept.iter().enumerate() {
                    cleared[BOARD_ROWS - kept.len() + i] = k;
                }
                test = cleared;
            }

            let test_heights = column_heights(&test);
            let score = evaluate_fn(&test, &test_heights, clears, land_row as usize);
            if score > best_score {
                best_score = score;
                best_move = Some((rot, left_col));
            }
        }
    }

    best_move
}

/// Empty cells directly under a block with a filled lateral neighbor (overhang dent).
pub fn count_cavities(bb: &Bitboard) -> i32 {
    let mut n = 0i32;
    for r in 0..BOARD_ROWS.saturating_sub(1) {
        for c in 0..BOARD_COLS {
            if bb[r] & (1 << c) != 0 {
                continue;
            }
            if bb[r + 1] & (1 << c) == 0 {
                continue;
            }
            let left = c > 0 && bb[r] & (1 << (c - 1)) != 0;
            let right = c + 1 < BOARD_COLS && bb[r] & (1 << (c + 1)) != 0;
            if left || right {
                n += 1;
            }
        }
    }
    n
}

pub fn meatfighter_evaluate(bb: &Bitboard, heights: &[u8], total_clears: u32, total_lock_h: i32) -> i32 {
    let mut well_cells = 0i32;
    let mut col_holes = 0i32;
    let mut col_trans = 0i32;
    let mut row_trans = 0i32;

    for r in 0..BOARD_ROWS {
        if bb[r] == 0 {
            continue;
        }
        let mut prev = 1i32;
        for c in 0..BOARD_COLS {
            let cur = if (bb[r] & (1 << c)) != 0 { 1 } else { 0 };
            if cur != prev {
                row_trans += 1;
            }
            prev = cur;
        }
        if prev == 0 {
            row_trans += 1;
        }
    }

    for c in 0..BOARD_COLS {
        let h = heights[c] as i32;
        if h == 0 {
            continue;
        }
        let top_row = BOARD_ROWS as i32 - h;

        let mut prev_filled = true;
        for r in top_row..BOARD_ROWS as i32 {
            let filled = (bb[r as usize] & (1 << c)) != 0;
            if filled != prev_filled {
                col_trans += 1;
            }
            prev_filled = filled;
        }

        let mut found_filled = false;
        for r in top_row..BOARD_ROWS as i32 {
            let filled = (bb[r as usize] & (1 << c)) != 0;
            if filled {
                found_filled = true;
            } else if found_filled {
                col_holes += 1;
            }
        }

        for r in 0..top_row {
            let left_solid = if c == 0 {
                true
            } else {
                (bb[r as usize] & (1 << (c - 1))) != 0
            };
            let right_solid = if c == BOARD_COLS - 1 {
                true
            } else {
                (bb[r as usize] & (1 << (c + 1))) != 0
            };
            if left_solid && right_solid {
                well_cells += 1;
            }
        }
    }

    let cavities = count_cavities(bb);

    let raw = (total_clears as f64) * 1.000000000000000
        + (total_lock_h as f64) * 12.885008263218383
        + (well_cells as f64) * 15.842707182438396
        + (col_holes as f64) * 26.894496507795950
        + (col_trans as f64) * 27.616914062397015
        + (row_trans as f64) * 30.185110719279040
        + (cavities as f64) * 40.0;

    (-raw) as i32
}

pub fn meatfighter_evaluate_1ply(bb: &Bitboard, heights: &[u8], clears: u32, land_row: usize) -> i32 {
    let lock_h = (BOARD_ROWS as i32) - 1 - (land_row as i32);
    meatfighter_evaluate(bb, heights, clears, lock_h)
}

pub(crate) fn simulate_place_and_clear(
    bb: &Bitboard,
    type_idx: usize,
    rot: usize,
    left: usize,
    land: i32,
) -> (Bitboard, u32, i32) {
    let mut test = *bb;
    let shape = &SHAPES[type_idx][rot];
    let max_dr = shape.iter().map(|&[r, _]| r).max().unwrap_or(0) as i32;
    for &[dr, dc] in shape {
        let r = land + dr as i32;
        if r >= 0 && r < BOARD_ROWS as i32 {
            test[r as usize] |= 1 << (left as i32 + dc as i32);
        }
    }
    let full = (1u16 << BOARD_COLS) - 1;
    let mut clears = 0u32;
    let mut kept = vec![];
    for r in 0..BOARD_ROWS {
        if test[r] == full {
            clears += 1;
        } else {
            kept.push(test[r]);
        }
    }
    if clears > 0 {
        let mut cleared = [0u16; BOARD_ROWS];
        for (i, &k) in kept.iter().enumerate() {
            cleared[BOARD_ROWS - kept.len() + i] = k;
        }
        test = cleared;
    }
    let lock_h = BOARD_ROWS as i32 - 1 - (land + max_dr);
    (test, clears, lock_h)
}

/// BFS path to a specific lock cell (misdrop replay want-lock).
pub fn find_bfs_path_to_lock(
    bb: &Bitboard,
    type_idx: usize,
    spawn_row: i32,
    spawn_col: usize,
    spawn_rot: usize,
    want_row: i32,
    want_col: usize,
    want_rot: usize,
) -> Option<(usize, usize, Vec<String>, i32)> {
    let moves = bfs_moves(&bb, type_idx, spawn_row, spawn_col, spawn_rot);
    let m = moves.iter().find(|m| {
        m.row == want_row && m.col == want_col as i32 && m.rot == want_rot
    })?;
    if !bfs_path_reaches_lock(&bb, type_idx, spawn_row, spawn_col as i32, spawn_rot, m) {
        return None;
    }
    Some((want_rot, want_col, m.path.clone(), want_row))
}

/// 2-ply BFS version returning best placement + path for piece 1.
///
/// Picks the strict best Meatfighter score (no “prefer right tuck” / shape
/// tie-break). A Jul-16 bias toward right-side tucks made early stacks holey
/// even when flat placements scored as well or better.
pub fn find_best_move_with_bfs(
    read_mem: impl Fn(u16) -> u8,
    read_range: impl Fn(u16, u16) -> Vec<u8>,
    spawn_col: usize,
    spawn_row: i32,
    next_type_idx: Option<usize>,
) -> Option<(usize, usize, Vec<String>, i32)> {
    let bb = read_board_bitboard(read_range);
    let ori = read_mem(ADDR_CUR_ORI);
    let info = ori_info(ori)?;

    let moves1 = bfs_moves(&bb, info.0, spawn_row, spawn_col, info.1 as usize);
    let next_idx = next_type_idx.unwrap_or(0);

    let mut best_score = i32::MIN;
    let mut best: Option<(usize, usize, Vec<String>, i32)> = None;

    for m in &moves1 {
        if !bfs_path_reaches_lock(&bb, info.0, spawn_row, spawn_col as i32, info.1 as usize, m) {
            continue;
        }
        let (bb1, clears1, lock_h1) =
            simulate_place_and_clear(&bb, info.0, m.rot, m.col as usize, m.row);
        let moves2 = bfs_moves(&bb1, next_idx, 0, 3, 0);
        let mut best_inner = i32::MIN;
        for m2 in &moves2 {
            if !bfs_path_reaches_lock(&bb1, next_idx, 0, 3, 0, m2) {
                continue;
            }
            let (bb2, clears2, lock_h2) =
                simulate_place_and_clear(&bb1, next_idx, m2.rot, m2.col as usize, m2.row);
            let h2 = column_heights(&bb2);
            let sc = meatfighter_evaluate(&bb2, &h2, clears1 + clears2, lock_h1 + lock_h2);
            if sc > best_inner {
                best_inner = sc;
            }
        }

        if best_inner > best_score {
            best_score = best_inner;
            best = Some((m.rot, m.col as usize, m.path.clone(), m.row));
        }
    }

    best
}

/// Classic 1-ply column/rot search — no tuck/spin paths (safe fallback).
pub fn find_safe_normal_placement(
    read_mem: impl Fn(u16) -> u8,
    read_range: impl Fn(u16, u16) -> Vec<u8>,
    from_col: usize,
) -> Option<(usize, usize)> {
    find_best_move(read_mem, read_range, Some(from_col), -1, default_evaluate)
}

/// 1-ply BFS fallback — strict best Meatfighter score.
pub fn find_best_move_with_bfs_1ply(
    read_mem: impl Fn(u16) -> u8,
    read_range: impl Fn(u16, u16) -> Vec<u8>,
    from_row: i32,
    from_col: usize,
    from_rot: usize,
) -> Option<(usize, usize, Vec<String>, i32)> {
    let bb = read_board_bitboard(read_range);
    let ori = read_mem(ADDR_CUR_ORI);
    let info = ori_info(ori)?;

    let moves = bfs_moves(&bb, info.0, from_row, from_col, from_rot);

    let mut best_score = i32::MIN;
    let mut best = None;
    for m in &moves {
        if !bfs_path_reaches_lock(&bb, info.0, from_row, from_col as i32, from_rot, m) {
            continue;
        }
        let (bb2, clears, lock_h) =
            simulate_place_and_clear(&bb, info.0, m.rot, m.col as usize, m.row);
        let h = column_heights(&bb2);
        let sc = meatfighter_evaluate(&bb2, &h, clears, lock_h);
        if sc > best_score {
            best_score = sc;
            best = Some((m.rot, m.col as usize, m.path.clone(), m.row));
        }
    }
    best
}