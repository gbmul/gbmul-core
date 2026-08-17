//! BFS search and SRS-aligned path simulation (`bfs_moves` expansion rules).

use super::board::{
    bfs_row_idx, Bitboard, BOARD_COLS, BOARD_ROWS, BFS_ROW_COUNT, SHAPES,
};
use super::srs;

/// Max grounded moves before lock (Rosy: 30f delay, 15 move resets).
const MAX_GROUNDED_RUN: u32 = 15;

#[derive(Clone)]
struct BfsPred {
    action: String,
    from_row: i32,
    from_col: i32,
    from_rot: usize,
}

/// Immediate CW↔CCW undoes rotation and only wastes grounded budget / path steps.
fn is_opposite_rotation_after_last(
    pred: &[Vec<Vec<Option<BfsPred>>>],
    r: i32,
    c: i32,
    rot: usize,
    action: &str,
) -> bool {
    let (Some(r_idx), Some(c_idx)) = (bfs_row_idx(r), usize::try_from(c).ok()) else {
        return false;
    };
    if c_idx >= BOARD_COLS {
        return false;
    }
    let Some(p) = &pred[r_idx][c_idx][rot] else {
        return false;
    };
    matches!(
        (p.action.as_str(), action),
        ("CW", "CCW") | ("CCW", "CW")
    )
}

pub fn piece_collides(bb: &Bitboard, type_idx: usize, rot: usize, row: i32, col: i32) -> bool {
    for &[dr, dc] in &SHAPES[type_idx][rot] {
        let r = row + dr as i32;
        let c = col + dc as i32;
        if c < 0 || c >= BOARD_COLS as i32 {
            return true;
        }
        if r >= BOARD_ROWS as i32 {
            return true;
        }
        if r >= 0 && (bb[r as usize] & (1 << c)) != 0 {
            return true;
        }
    }
    false
}

/// BFS over (row, col, rot) for reachable locked positions (tucks/spins).
/// Returns list of reachable locked states with their action paths.
pub fn bfs_moves(
    bb: &Bitboard,
    type_idx: usize,
    spawn_row: i32,
    spawn_col: usize,
    spawn_rot: usize,
) -> Vec<BfsLockedMove> {
    let mut visited = vec![vec![vec![false; 4]; BOARD_COLS]; BFS_ROW_COUNT];
    let mut pred: Vec<Vec<Vec<Option<BfsPred>>>> = vec![vec![vec![None; 4]; BOARD_COLS]; BFS_ROW_COUNT];
    let mut grounded_run = vec![vec![vec![0u32; 4]; BOARD_COLS]; BFS_ROW_COUNT];

    let mut locked: Vec<BfsLockedMove> = Vec::new();
    let mut queue: Vec<(i32, i32, usize)> = Vec::new();

    if piece_collides(bb, type_idx, spawn_rot, spawn_row, spawn_col as i32) {
        return locked;
    }

    let Some(sr_idx) = bfs_row_idx(spawn_row) else {
        return locked;
    };
    visited[sr_idx][spawn_col][spawn_rot] = true;
    queue.push((spawn_row, spawn_col as i32, spawn_rot));

    let mut head = 0usize;
    while head < queue.len() {
        let (r, c, rot) = queue[head];
        head += 1;
        let (r_idx, c_idx) = match (bfs_row_idx(r), usize::try_from(c).ok()) {
            (Some(ri), Some(ci)) if ci < BOARD_COLS => (ri, ci),
            _ => continue,
        };
        let run = grounded_run[r_idx][c_idx][rot];

        let can_down = !piece_collides(bb, type_idx, rot, r + 1, c);

        if !can_down {
            let mut path: Vec<String> = Vec::new();
            let mut pr = r;
            let mut pc = c;
            let mut prot = rot;
            while let (Some(pri), Some(pci)) = (bfs_row_idx(pr), usize::try_from(pc).ok()) {
                let Some(p) = &pred[pri][pci][prot] else {
                    break;
                };
                path.push(p.action.clone());
                pr = p.from_row;
                pc = p.from_col;
                prot = p.from_rot;
            }
            path.reverse();
            locked.push(BfsLockedMove {
                row: r,
                col: c,
                rot,
                path,
            });
            if run >= MAX_GROUNDED_RUN {
                continue;
            }
        }

        let moves: [(&str, i32, i32, usize); 3] = [
            ("D", r + 1, c, rot),
            ("L", r, c - 1, rot),
            ("R", r, c + 1, rot),
        ];

        for (action, nr, nc, nrot) in moves {
            if bfs_row_idx(nr).is_none() || nc < 0 || nc >= BOARD_COLS as i32 {
                continue;
            }
            let (nr_idx, nc_idx) = (bfs_row_idx(nr).unwrap(), nc as usize);
            if visited[nr_idx][nc_idx][nrot] {
                continue;
            }
            if piece_collides(bb, type_idx, nrot, nr, nc) {
                continue;
            }

            visited[nr_idx][nc_idx][nrot] = true;
            pred[nr_idx][nc_idx][nrot] = Some(BfsPred {
                action: action.to_string(),
                from_row: r,
                from_col: c,
                from_rot: rot,
            });
            grounded_run[nr_idx][nc_idx][nrot] = if can_down { 0 } else { run + 1 };
            queue.push((nr, nc, nrot));
        }

        for &(action, cw) in &[("CW", true), ("CCW", false)] {
            if is_opposite_rotation_after_last(&pred, r, c, rot, action) {
                continue;
            }

            let rot_result = srs::srs_try_rotate_auto(bb, type_idx, r, c, rot, cw);
            let Some((nr, nc, nrot)) = rot_result else {
                continue;
            };
            let Some(nr_idx) = bfs_row_idx(nr) else {
                continue;
            };
            let nc_idx = nc as usize;
            if nc_idx >= BOARD_COLS {
                continue;
            }

            if !can_down {
                if run >= MAX_GROUNDED_RUN {
                    continue;
                }
                if visited[nr_idx][nc_idx][nrot] {
                    continue;
                }
                visited[nr_idx][nc_idx][nrot] = true;
                pred[nr_idx][nc_idx][nrot] = Some(BfsPred {
                    action: action.to_string(),
                    from_row: r,
                    from_col: c,
                    from_rot: rot,
                });
                let kicked = nr != r || nc != c;
                grounded_run[nr_idx][nc_idx][nrot] = if kicked { 0 } else { run + 1 };
                queue.push((nr, nc, nrot));
                continue;
            }

            if piece_collides(bb, type_idx, nrot, nr + 1, nc) {
                continue;
            }
            if visited[nr_idx][nc_idx][nrot] {
                continue;
            }

            visited[nr_idx][nc_idx][nrot] = true;
            pred[nr_idx][nc_idx][nrot] = Some(BfsPred {
                action: action.to_string(),
                from_row: r,
                from_col: c,
                from_rot: rot,
            });
            grounded_run[nr_idx][nc_idx][nrot] = 0;
            queue.push((nr, nc, nrot));
        }
    }

    locked
}

/// Simulate a path using the same SRS + collision rules as `bfs_moves` expansion.
pub fn simulate_path_stepwise(
    bb: &Bitboard,
    type_idx: usize,
    start_row: i32,
    start_col: i32,
    start_rot: usize,
    path: &[String],
) -> Option<(i32, i32, usize)> {
    let mut r = start_row;
    let mut c = start_col;
    let mut rot = start_rot;
    let mut grounded_run = 0u32;

    for action in path {
        let can_down = !piece_collides(bb, type_idx, rot, r + 1, c);
        match action.as_str() {
            "D" => {
                if can_down {
                    r += 1;
                    grounded_run = 0;
                } else {
                    return None;
                }
            }
            "L" => {
                if c <= 0 || piece_collides(bb, type_idx, rot, r, c - 1) {
                    return None;
                }
                c -= 1;
                grounded_run = if can_down { 0 } else { grounded_run + 1 };
            }
            "R" => {
                if c >= BOARD_COLS as i32 - 1 || piece_collides(bb, type_idx, rot, r, c + 1) {
                    return None;
                }
                c += 1;
                grounded_run = if can_down { 0 } else { grounded_run + 1 };
            }
            "CW" | "CCW" => {
                let cw = action == "CW";
                if !can_down {
                    if grounded_run >= MAX_GROUNDED_RUN {
                        return None;
                    }
                    let (nr, nc, nrot) = srs::srs_try_rotate_auto(bb, type_idx, r, c, rot, cw)?;
                    let kicked = nr != r || nc != c;
                    r = nr;
                    c = nc;
                    rot = nrot;
                    grounded_run = if kicked { 0 } else { grounded_run + 1 };
                } else {
                    let (nr, nc, nrot) = srs::srs_try_rotate_auto(bb, type_idx, r, c, rot, cw)?;
                    if piece_collides(bb, type_idx, nrot, nr + 1, nc) {
                        return None;
                    }
                    r = nr;
                    c = nc;
                    rot = nrot;
                    grounded_run = 0;
                }
            }
            _ => return None,
        }
    }
    Some((r, c, rot))
}

/// True if `path` is reachable from spawn via BFS edge rules (for planner debugging).
#[cfg(test)]
pub(crate) fn bfs_path_is_reachable(
    bb: &Bitboard,
    type_idx: usize,
    spawn_row: i32,
    spawn_col: usize,
    spawn_rot: usize,
    path: &[String],
) -> bool {
    simulate_path_stepwise(bb, type_idx, spawn_row, spawn_col as i32, spawn_rot, path).is_some()
}

#[derive(Clone, Debug)]
pub struct BfsLockedMove {
    pub row: i32,
    pub col: i32,
    pub rot: usize,
    pub path: Vec<String>,
}

/// Simulate a path prefix with SRS rotations; returns (row, col, rot) after `path`.
pub fn simulate_path_prefix(
    bb: &Bitboard,
    type_idx: usize,
    start_row: i32,
    start_col: i32,
    start_rot: usize,
    path: &[String],
) -> Option<(i32, i32, usize)> {
    simulate_path_stepwise(bb, type_idx, start_row, start_col, start_rot, path)
}

/// True when simulating `m.path` from spawn reaches the BFS lock cell.
pub fn bfs_path_reaches_lock(
    bb: &Bitboard,
    type_idx: usize,
    start_row: i32,
    start_col: i32,
    start_rot: usize,
    m: &BfsLockedMove,
) -> bool {
    simulate_path_prefix(bb, type_idx, start_row, start_col, start_rot, &m.path)
        .is_some_and(|(r, c, rot)| r == m.row && c == m.col as i32 && rot == m.rot)
}

/// Longest path prefix whose simulated position matches the piece's actual position.
/// Gravity-free D's are included: if the piece fell further than taps sent, a longer
/// D-prefix may still match.
pub fn max_matching_path_step(
    bb: &Bitboard,
    type_idx: usize,
    start_row: i32,
    start_col: i32,
    start_rot: usize,
    path: &[String],
    actual_row: i32,
    actual_col: usize,
    actual_rot: usize,
) -> usize {
    let mut best = 0usize;
    for k in 0..=path.len() {
        if let Some((sr, sc, srot)) =
            simulate_path_prefix(bb, type_idx, start_row, start_col, start_rot, &path[..k])
        {
            if sr == actual_row && sc == actual_col as i32 && srot == actual_rot {
                best = k;
            }
        }
    }
    best
}