//! BFS path validation, classification, post-planner fixes, and path execution.

use super::board::{Bitboard, BOARD_COLS, BOARD_ROWS};
use super::lock_verify::lock_anchor_filled;
use super::bfs::{
    bfs_path_reaches_lock, max_matching_path_step, piece_collides, simulate_path_prefix,
    simulate_path_stepwise, BfsLockedMove,
};
use super::find_bfs_path_to_lock;
use super::srs;
use std::cmp::Ordering;

/// True when a BFS plan should be executed (vs safe-normal fallback).
/// - Normal: full path must simulate from actual spawn (incl. `extra_d` prefix).
/// - Tuck/spin: path must simulate from `bfs_row` via SRS BFS rules.
pub fn bfs_plan_acceptable(
    bb: &Bitboard,
    type_idx: usize,
    actual_row: i32,
    actual_col: usize,
    spawn_rot: usize,
    extra_d: usize,
    bfs_row: i32,
    lock_row: i32,
    lock_col: usize,
    lock_rot: usize,
    path: &[String],
) -> bool {
    let mtype = classify_move(bb, type_idx, lock_row, lock_col as i32, lock_rot, path, 0);
    let m = BfsLockedMove {
        row: lock_row,
        col: lock_col as i32,
        rot: lock_rot,
        path: path.to_vec(),
    };
    if !bfs_path_reaches_lock(bb, type_idx, bfs_row, actual_col as i32, spawn_rot, &m) {
        return false;
    }
    if mtype == "normal" {
        return planned_bfs_path_valid(
            bb,
            type_idx,
            actual_row,
            actual_col,
            spawn_rot,
            extra_d,
            lock_row,
            lock_col,
            lock_rot,
            path,
        );
    }
    bfs_path_reaches_lock(bb, type_idx, actual_row, actual_col as i32, spawn_rot, &m)
}

/// Back-compat alias used in tests.
pub fn bfs_result_trustworthy(
    bb: &Bitboard,
    type_idx: usize,
    actual_row: i32,
    actual_col: usize,
    spawn_rot: usize,
    extra_d: usize,
    lock_row: i32,
    lock_col: usize,
    lock_rot: usize,
    path: &[String],
) -> bool {
    bfs_plan_acceptable(
        bb,
        type_idx,
        actual_row,
        actual_col,
        spawn_rot,
        extra_d,
        actual_row,
        lock_row,
        lock_col,
        lock_rot,
        path,
    )
}

/// True when `path` (plus spawn `extra_d` prefix) simulates to the BFS lock cell.
pub fn planned_bfs_path_valid(
    bb: &Bitboard,
    type_idx: usize,
    actual_row: i32,
    actual_col: usize,
    spawn_rot: usize,
    extra_d: usize,
    lock_row: i32,
    lock_col: usize,
    lock_rot: usize,
    path: &[String],
) -> bool {
    let full_path: Vec<String> = if extra_d > 0 {
        let mut p = vec!["D".to_string(); extra_d];
        p.extend(path.iter().cloned());
        p
    } else {
        path.to_vec()
    };
    if full_path.is_empty() {
        return true;
    }
    simulate_path_prefix(bb, type_idx, actual_row, actual_col as i32, spawn_rot, &full_path)
        .is_some_and(|(r, c, rot)| r == lock_row && c == lock_col as i32 && rot == lock_rot)
}

pub fn piece_pos_trustworthy(row: i32, col: usize) -> bool {
    row >= 0 && row < BOARD_ROWS as i32 && col < BOARD_COLS
}

/// True when the piece cannot move down due to blocks (not WRAM garbage / off-board reads).
pub fn piece_trustworthily_grounded(
    bb: &Bitboard,
    type_idx: usize,
    rot: usize,
    min_row: i32,
    col: usize,
) -> bool {
    if !piece_pos_trustworthy(min_row, col) {
        return false;
    }
    let below = min_row + 1;
    if below >= BOARD_ROWS as i32 {
        return false;
    }
    piece_collides(bb, type_idx, rot, below, col as i32)
}

/// Drop a lone `D` immediately before setup `CW`/`CCW` when sim lock is unchanged
/// (Z→T: emu gravity during `RRR` already reaches the setup row).
pub fn trim_redundant_setup_d(
    bb: &Bitboard,
    type_idx: usize,
    spawn_row: i32,
    spawn_col: i32,
    spawn_rot: usize,
    path: Vec<String>,
) -> Vec<String> {
    let want = simulate_path_prefix(bb, type_idx, spawn_row, spawn_col, spawn_rot, &path);
    for i in 0..path.len() {
        if path.get(i).map(|s| s.as_str()) != Some("D") {
            continue;
        }
        if !matches!(
            path.get(i + 1).map(|s| s.as_str()),
            Some("CW") | Some("CCW")
        ) {
            continue;
        }
        let shorter: Vec<String> = path
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, s)| s.clone())
            .collect();
        if simulate_path_prefix(bb, type_idx, spawn_row, spawn_col, spawn_rot, &shorter) == want
        {
            return trim_redundant_setup_d(
                bb, type_idx, spawn_row, spawn_col, spawn_rot, shorter,
            );
        }
    }
    path
}

/// Lower rank = preferred. normal > tuck > spin > tspin.
pub fn mtype_preference_rank(mtype: &str) -> u32 {
    match mtype {
        "normal" => 0,
        "tuck" => 1,
        "spin" => 2,
        "tspin" => 3,
        _ => 4,
    }
}

/// Grounded SRS rotations where the kick table displaces the piece (not test 0).
pub fn count_floor_kicks_in_path(
    bb: &Bitboard,
    type_idx: usize,
    start_row: i32,
    start_col: i32,
    start_rot: usize,
    path: &[String],
) -> u32 {
    let mut r = start_row;
    let mut c = start_col;
    let mut rot = start_rot;
    let mut kicks = 0u32;
    for action in path {
        let can_down = !piece_collides(bb, type_idx, rot, r + 1, c);
        match action.as_str() {
            "D" => {
                if can_down {
                    r += 1;
                }
            }
            "L" => {
                if c > 0 && !piece_collides(bb, type_idx, rot, r, c - 1) {
                    c -= 1;
                }
            }
            "R" => {
                if c < BOARD_COLS as i32 - 1 && !piece_collides(bb, type_idx, rot, r, c + 1) {
                    c += 1;
                }
            }
            "CW" | "CCW" => {
                let cw = action == "CW";
                if !can_down {
                    if let Some((_, _, _, _, (kx, ky))) =
                        srs::srs_try_rotate_detailed(bb, type_idx, r, c, rot, cw, true)
                    {
                        if kx != 0 || ky != 0 {
                            kicks += 1;
                        }
                        if let Some((nr, nc, nrot)) =
                            srs::srs_try_rotate_auto(bb, type_idx, r, c, rot, cw)
                        {
                            r = nr;
                            c = nc;
                            rot = nrot;
                        }
                    }
                } else if let Some((nr, nc, nrot)) =
                    srs::srs_try_rotate_auto(bb, type_idx, r, c, rot, cw)
                {
                    r = nr;
                    c = nc;
                    rot = nrot;
                }
            }
            _ => {}
        }
    }
    kicks
}

/// True when setup is ordered rotations → laterals → D's (terminal tuck L/R allowed last).
pub fn is_rotate_first_path(path: &[String], mtype: &str) -> bool {
    if path.is_empty() {
        return true;
    }
    if mtype == "spin" || mtype == "tspin" {
        return true;
    }
    let n = path.len();
    let terminal = if mtype == "tuck" {
        matches!(path.last().map(|s| s.as_str()), Some("L") | Some("R"))
    } else {
        false
    };
    let end = if terminal { n - 1 } else { n };
    let mut phase = 0u8; // 0=rot, 1=lat, 2=D
    for act in path.iter().take(end) {
        match act.as_str() {
            "CW" | "CCW" => {
                if phase > 0 {
                    return false;
                }
            }
            "L" | "R" => {
                if phase > 1 {
                    return false;
                }
                phase = 1;
            }
            "D" => {
                phase = 2;
            }
            _ => return false,
        }
    }
    true
}

/// Path-order penalties for normal/tuck (spins keep terminal rots on purpose).
///
/// - Late rotation (after L/R or D): bad for normals.
/// - Early setup L/R before descent on terminal tucks: soft-drop overshoots the well
///   (T→Z `misdrop_t_tuck_r12_c7_r1_20260718`). Prefer BFS D-first then terminal slide.
fn path_order_penalty(path: &[String], mtype: &str) -> u32 {
    if path.is_empty() || mtype == "spin" || mtype == "tspin" {
        return 0;
    }

    let first_rot = path.iter().position(|a| *a == "CW" || *a == "CCW");
    let first_lat = path.iter().position(|a| *a == "L" || *a == "R");
    let first_d = path.iter().position(|a| *a == "D");
    let mut penalty = 0u32;

    // Late rotation: normals only. Tucks often rotate mid-descent into the well
    // (L→S `misdrop_l_tuck_r13_c3_r1_20260718` — spawn CW kicks differently than mid-stack).
    if mtype == "normal" {
        if let Some(ri) = first_rot {
            if first_lat.is_some_and(|li| ri > li) || first_d.is_some_and(|di| ri > di) {
                penalty += 5_000;
            }
        } else if path.iter().any(|a| *a == "CW" || *a == "CCW") {
            penalty += 5_000;
        }
    }

    // Terminal tuck: setup laterals before any D overshoot the well under soft-drop.
    if mtype == "tuck" && path.len() >= 2 {
        if let Some(term) = path.last() {
            if *term == "L" || *term == "R" {
                let body = &path[..path.len() - 1];
                let body_lat = body.iter().position(|a| *a == "L" || *a == "R");
                let body_d = body.iter().position(|a| *a == "D");
                match (body_lat, body_d) {
                    (Some(li), Some(di)) if li < di => penalty += 3_500,
                    (Some(_), None) => penalty += 3_500,
                    _ => {}
                }
            }
        }
    }

    penalty
}

/// Lower = simpler. Penalizes move class, floor kicks, length, path order, D-before-setup.
pub fn path_simplicity_score(
    bb: &Bitboard,
    type_idx: usize,
    start_row: i32,
    start_col: i32,
    start_rot: usize,
    lock_row: i32,
    lock_col: i32,
    lock_rot: usize,
    path: &[String],
) -> u32 {
    let mtype = classify_move(bb, type_idx, lock_row, lock_col, lock_rot, path, 0);
    let kicks = count_floor_kicks_in_path(bb, type_idx, start_row, start_col, start_rot, path);
    let rots = path
        .iter()
        .filter(|a| *a == "CW" || *a == "CCW")
        .count() as u32;
    let leading_d = path
        .iter()
        .take_while(|a| *a == "D")
        .count() as u32;
    mtype_preference_rank(mtype) * 10_000
        + kicks * 1_000
        + path_order_penalty(path, mtype)
        + if is_rotate_first_path(path, mtype) {
            0
        } else {
            2_000
        }
        + path.len() as u32 * 10
        + rots * 5
        + leading_d * 2
}

/// Reorder a normal/tuck path to rotations → laterals → D's → optional terminal tuck.
pub fn canonicalize_rotate_first_path(path: &[String], mtype: &str) -> Vec<String> {
    if path.is_empty() || mtype == "spin" || mtype == "tspin" {
        return path.to_vec();
    }
    let terminal = if mtype == "tuck" {
        path.last()
            .filter(|a| *a == "L" || *a == "R")
            .cloned()
    } else {
        None
    };
    let body = if terminal.is_some() {
        &path[..path.len() - 1]
    } else {
        path
    };
    let mut rots = Vec::new();
    let mut lats = Vec::new();
    let mut ds = Vec::new();
    for a in body {
        match a.as_str() {
            "CW" | "CCW" => rots.push(a.clone()),
            "L" | "R" => lats.push(a.clone()),
            "D" => ds.push(a.clone()),
            _ => {}
        }
    }
    let mut out = rots;
    out.extend(lats);
    out.extend(ds);
    if let Some(t) = terminal {
        out.push(t);
    }
    out
}

/// Compare paths to the same lock. Returns Less when `a` is preferred.
pub fn compare_path_preference(
    bb: &Bitboard,
    type_idx: usize,
    start_row: i32,
    start_col: i32,
    start_rot: usize,
    lock_row: i32,
    lock_col: i32,
    lock_rot: usize,
    a: &[String],
    b: &[String],
) -> Ordering {
    let sa = path_simplicity_score(
        bb, type_idx, start_row, start_col, start_rot, lock_row, lock_col, lock_rot, a,
    );
    let sb = path_simplicity_score(
        bb, type_idx, start_row, start_col, start_rot, lock_row, lock_col, lock_rot, b,
    );
    sa.cmp(&sb)
}

/// Tie-break near-equal Meatfighter scores: fewer cavities, then tuck > normal > spin, then path.
pub fn compare_placement_preference(
    bb: &Bitboard,
    type_idx: usize,
    spawn_row: i32,
    spawn_col: i32,
    spawn_rot: usize,
    a: &(i32, i32, usize, &[String]),
    b: &(i32, i32, usize, &[String]),
) -> Ordering {
    use super::planning::{count_cavities, simulate_place_and_clear};

    let (bba, _, _) = simulate_place_and_clear(bb, type_idx, a.2, a.1 as usize, a.0);
    let (bbb, _, _) = simulate_place_and_clear(bb, type_idx, b.2, b.1 as usize, b.0);
    let ca = count_cavities(&bba);
    let cb = count_cavities(&bbb);
    if ca.abs_diff(cb) > 1 {
        return ca.cmp(&cb);
    }

    let ma = classify_move(bb, type_idx, a.0, a.1, a.2, a.3, 0);
    let mb = classify_move(bb, type_idx, b.0, b.1, b.2, b.3, 0);
    let ra = placement_shape_rank(ma);
    let rb = placement_shape_rank(mb);
    match ra.cmp(&rb) {
        Ordering::Equal => compare_path_preference(
            bb, type_idx, spawn_row, spawn_col, spawn_rot, a.0, a.1, a.2, a.3, b.3,
        ),
        o => o,
    }
}

/// 2-ply placement tie-break: tuck setups beat flat normals when scores are near-equal.
fn placement_shape_rank(mtype: &str) -> u32 {
    match mtype {
        "tuck" => 0,
        "normal" => 1,
        "spin" => 2,
        "tspin" => 3,
        _ => 4,
    }
}

fn synth_simple_path_candidates(
    spawn_row: i32,
    spawn_col: i32,
    spawn_rot: usize,
    lock_row: i32,
    lock_col: i32,
    lock_rot: usize,
) -> Vec<Vec<String>> {
    let cw = (lock_rot + 4 - spawn_rot) % 4;
    let ccw = (spawn_rot + 4 - lock_rot) % 4;
    let col_delta = lock_col - spawn_col;
    let mut out = Vec::new();

    for &(n_rot, use_cw) in &[(cw, true), (ccw, false)] {
        let rot_path: Vec<String> = if n_rot == 0 {
            vec![]
        } else if use_cw {
            vec!["CW".to_string(); n_rot]
        } else {
            vec!["CCW".to_string(); n_rot]
        };
        let lat_path: Vec<String> = if col_delta > 0 {
            vec!["R".to_string(); col_delta as usize]
        } else if col_delta < 0 {
            vec!["L".to_string(); (-col_delta) as usize]
        } else {
            vec![]
        };

        for d_count in 0..=28u32 {
            let mut setup_first = rot_path.clone();
            setup_first.extend(lat_path.clone());
            setup_first.extend(std::iter::repeat_n("D".to_string(), d_count as usize));
            out.push(setup_first);

            for term in ["L", "R"] {
                let mut tuck = rot_path.clone();
                tuck.extend(lat_path.clone());
                tuck.extend(std::iter::repeat_n("D".to_string(), d_count as usize));
                tuck.push(term.to_string());
                out.push(tuck);
            }
        }
    }
    out
}

/// Same lock cell: prefer rotate-first setup (normals), no floor kicks, shortest path.
///
/// **Tucks with mid-descent rotation** keep BFS order: rewriting to spawn-height CW/CCW
/// can kick differently on hardware than the mid-stack rot BFS planned
/// (`misdrop_l_tuck_r13_c3_r1_20260718` — spawn CW → col4, sim → col3).
pub fn prefer_simplest_equivalent_path(
    bb: &Bitboard,
    type_idx: usize,
    spawn_row: i32,
    spawn_col: i32,
    spawn_rot: usize,
    lock_row: i32,
    lock_col: i32,
    lock_rot: usize,
    current: &[String],
) -> Vec<String> {
    let mtype = classify_move(bb, type_idx, lock_row, lock_col, lock_rot, current, 0);
    let spin_like = mtype == "spin" || mtype == "tspin";
    // Tuck with first rotation *after* some descent: keep BFS order.
    // Rewriting to spawn-height CW/CCW kicks differently on GB hardware than the
    // mid-stack rot BFS planned:
    // - L→S r13 c3: spawn CW → col4; mid L+CW → well col2
    // - L→S r15 c4: spawn CW → col4 grounded short; mid CW after D×16 → (15,4) tuck
    let first_d = current.iter().position(|a| a == "D");
    let first_rot = current
        .iter()
        .position(|a| a == "CW" || a == "CCW");
    let tuck_keep_bfs_mid_rot = mtype == "tuck"
        && first_d.is_some_and(|di| first_rot.is_some_and(|ri| di < ri));

    let mut best = current.to_vec();
    let mut best_score = path_simplicity_score(
        bb, type_idx, spawn_row, spawn_col, spawn_rot, lock_row, lock_col, lock_rot, &best,
    );

    let mut try_candidate = |path: Vec<String>| {
        // Normals: only rotate-first. Tucks may keep BFS mid-descent rot order.
        if !spin_like && mtype == "normal" && !is_rotate_first_path(&path, mtype) {
            return;
        }
        if simulate_path_stepwise(bb, type_idx, spawn_row, spawn_col, spawn_rot, &path)
            .is_some_and(|(r, c, rot)| r == lock_row && c == lock_col && rot == lock_rot)
        {
            let score = path_simplicity_score(
                bb, type_idx, spawn_row, spawn_col, spawn_rot, lock_row, lock_col, lock_rot, &path,
            );
            if score < best_score {
                best_score = score;
                best = path;
            }
        }
    };

    try_candidate(current.to_vec());
    if !spin_like && !tuck_keep_bfs_mid_rot {
        try_candidate(canonicalize_rotate_first_path(current, mtype));
        for candidate in synth_simple_path_candidates(
            spawn_row, spawn_col, spawn_rot, lock_row, lock_col, lock_rot,
        ) {
            try_candidate(candidate);
        }
    }
    best
}

/// S/Z grounded spin: BFS often plans `R,CCW,D…,CCW` but GB Tetris needs
/// `CCW,R,D…,R,CCW` (duplicate SRS orientation — tuck button order matters).
pub fn fix_sz_spin_path(
    bb: &Bitboard,
    type_idx: usize,
    spawn_row: i32,
    spawn_col: usize,
    spawn_rot: usize,
    mtype: &str,
    path: &[String],
) -> Vec<String> {
    if type_idx != 3 && type_idx != 4 || mtype != "spin" || path.len() < 6 {
        return path.to_vec();
    }
    let n = path.len();
    if path[n - 1] != "CCW" {
        return path.to_vec();
    }
    let mut d_end = 0usize;
    let mut i = n - 2;
    while path[i] == "D" {
        d_end += 1;
        if i == 0 {
            return path.to_vec();
        }
        i -= 1;
    }
    if d_end == 0 || path[i] != "CCW" || path[i - 1] != "R" {
        return path.to_vec();
    }
    let mut out: Vec<String> = path[..i - 1].to_vec();
    out.push("CCW".into());
    out.push("R".into());
    for _ in 0..d_end {
        out.push("D".into());
    }
    out.push("R".into());
    out.push("CCW".into());
    if simulate_path_prefix(bb, type_idx, spawn_row, spawn_col as i32, spawn_rot, &out).is_some() {
        out
    } else {
        path.to_vec()
    }
}

/// S-piece spin: BFS often emits `D×n,R,CCW,D×k,CCW` but Rosy needs `D×n,R,CW` on
/// well boards (CCW tail lands on T overhang). Only rewrite when CW path sim-reaches lock.
pub fn fix_s_spin_cw_terminal(
    bb: &Bitboard,
    type_idx: usize,
    spawn_row: i32,
    spawn_col: usize,
    spawn_rot: usize,
    lock_row: i32,
    lock_col: usize,
    lock_rot: usize,
    mtype: &str,
    path: &[String],
) -> Vec<String> {
    if type_idx != 3 || mtype != "spin" || path.len() < 4 {
        return path.to_vec();
    }
    let n = path.len();
    if path[n - 1] != "CCW" {
        return path.to_vec();
    }
    let mut d_tail = 0usize;
    let mut i = n - 2;
    while path[i] == "D" {
        d_tail += 1;
        if i == 0 {
            return path.to_vec();
        }
        i -= 1;
    }
    if d_tail == 0 || path[i] != "CCW" || i < 1 || path[i - 1] != "R" {
        return path.to_vec();
    }
    let mut cw_path: Vec<String> = path[..i - 1].to_vec();
    cw_path.push("R".into());
    cw_path.push("CW".into());
    if let Some((r, c, rot)) = simulate_path_prefix(
        bb,
        type_idx,
        spawn_row,
        spawn_col as i32,
        spawn_rot,
        &cw_path,
    ) {
        if r == lock_row && c == lock_col as i32 && rot == lock_rot {
            return cw_path;
        }
    }
    path.to_vec()
}

/// Synthesize the rot/trans portion of a normal (non-path) execution for replay metadata.
pub fn synth_normal_execution_path(
    cur_rot: usize,
    target_rot: usize,
    cur_col: usize,
    target_col: usize,
) -> Vec<String> {
    let mut path = Vec::new();
    let cw = (target_rot + 4 - cur_rot) % 4;
    let ccw = (cur_rot + 4 - target_rot) % 4;
    if cw <= ccw {
        path.extend(std::iter::repeat_n("CW".to_string(), cw));
    } else {
        path.extend(std::iter::repeat_n("CCW".to_string(), ccw));
    }
    if target_col < cur_col {
        path.extend(std::iter::repeat_n("L".to_string(), cur_col - target_col));
    } else if target_col > cur_col {
        path.extend(std::iter::repeat_n("R".to_string(), target_col - cur_col));
    }
    path
}

/// Classify path ending from the last non-drop action before lock.
pub fn path_terminal_mtype(path: &[String]) -> &'static str {
    if path.is_empty() {
        return "normal";
    }
    match path.iter().rev().find(|a| *a != "D").map(|s| s.as_str()) {
        Some("CW") | Some("CCW") => "spin",
        Some("L") | Some("R") => "tuck",
        _ => "normal",
    }
}

pub fn classify_move(
    bb: &Bitboard,
    type_idx: usize,
    row: i32,
    col: i32,
    rot: usize,
    path: &[String],
    clears: u32,
) -> &'static str {
    let last = path.last().map(|s| s.as_str()).unwrap_or("");
    let is_rot = last == "CW" || last == "CCW";
    if is_rot && piece_collides(bb, type_idx, rot, row - 1, col) {
        return if type_idx == 2 && clears > 0 {
            "tspin"
        } else {
            "spin"
        };
    }
    if path_terminal_mtype(path) == "spin" {
        let mut grav = 0i32;
        while !piece_collides(bb, type_idx, rot, grav + 1, col) {
            grav += 1;
        }
        if grav != row {
            return "spin";
        }
    }
    let mut grav = 0i32;
    while !piece_collides(bb, type_idx, rot, grav + 1, col) {
        grav += 1;
    }
    if grav != row {
        "tuck"
    } else {
        "normal"
    }
}

/// Simulate path from `start_row` and return the row immediately before the last
/// action fires (used as spin/tuck execution milestone).
pub fn row_before_final_action(start_row: i32, path: &[String]) -> Option<i32> {
    if path.is_empty() {
        return None;
    }
    let mut r = start_row;
    for act in &path[..path.len() - 1] {
        match act.as_str() {
            "D" => r += 1,
            _ => {}
        }
    }
    Some(r)
}

/// `plan_intended_row` for path execution: row before the terminal action for
/// spin/tuck paths, otherwise the BFS lock row.
pub fn plan_row_before_final_action(
    bb: &Bitboard,
    type_idx: usize,
    start_row: i32,
    start_col: i32,
    start_rot: usize,
    path: &[String],
    lock_row: i32,
    mtype: &str,
    target_rot: usize,
) -> i32 {
    if mtype == "spin" || mtype == "tspin" || mtype == "tuck" {
        if mtype == "tuck" && !path.is_empty() {
            if let Some((r, _, _)) = simulate_path_prefix(
                bb,
                type_idx,
                start_row,
                start_col,
                start_rot,
                &path[..path.len() - 1],
            ) {
                return r;
            }
        }
        if let Some(mut plan_row) = row_before_final_action(start_row, path) {
            if (mtype == "spin" || mtype == "tspin") && type_idx == 2 {
                if let Some(last) = path.last().map(|s| s.as_str()) {
                    if last == "CW" || last == "CCW" {
                        let t_cw_off: [[i32; 2]; 4] = [[0, 1], [1, -1], [-1, 0], [0, 0]];
                        let t_ccw_off: [[i32; 2]; 4] = [[0, 0], [0, -1], [-1, 1], [1, 0]];
                        let from_rot = if last == "CW" {
                            (target_rot + 3) % 4
                        } else {
                            (target_rot + 1) % 4
                        };
                        let off = if last == "CW" {
                            t_cw_off[from_rot]
                        } else {
                            t_ccw_off[from_rot]
                        };
                        plan_row -= off[0];
                    }
                }
            }
            return plan_row;
        }
    }
    lock_row
}
use super::memory::{piece_left_col, piece_min_row, read_board_bitboard};
use super::misdrop::MisdropReason;
use super::planning::find_best_move_with_bfs_1ply;
use super::{BotState, TetrisBot, FRAME_DELAY, ori_info};

pub(crate) const IMPLICIT_DESCENT: &str = "__descent__";
pub(crate) const MIN_BTN_HOLD_FRAMES: u32 = 8;
pub(crate) const LATERAL_CHAIN_SETTLE_FRAMES: u32 = 4;
pub(crate) const PATH_DOWN_HOLD_FRAMES: u32 = 1;

/// Count cells that differ between two bitboards (for garbage detection logging).
pub fn count_board_diff(a: &Bitboard, b: &Bitboard) -> u32 {
    let mut n = 0u32;
    for r in 0..super::BOARD_ROWS {
        n += (a[r] ^ b[r]).count_ones();
    }
    n
}

impl TetrisBot {
    fn refresh_path_expected_from_sim(&mut self, bb: &Bitboard, type_idx: usize) {
        if let Some((r, c, rot)) = simulate_path_prefix(
            bb,
            type_idx,
            self.path_start_row,
            self.path_start_col as i32,
            self.path_start_rot,
            &self.move_path[..self.path_step],
        ) {
            self.path_expected_row = r;
            self.path_expected_col = c as usize;
            self.path_expected_rot = rot;
        }
    }

    /// Fast-forward path_step when gravity/emu drift left us behind the sim prefix.
    /// Only skips gravity `D` steps — never lateral/rot (DAS can match two R's after one press).
    pub(crate) fn resync_path_step_to_actual(
        &mut self,
        bb: &Bitboard,
        type_idx: usize,
        actual_row: i32,
        actual_col: usize,
        actual_rot: usize,
    ) -> bool {
        if self.intended_lock.as_ref().is_some_and(|(_, _, _, t)| {
            matches!(t.as_str(), "tuck" | "spin" | "tspin")
        }) {
            return false;
        }
        let matched = max_matching_path_step(
            bb,
            type_idx,
            self.path_start_row,
            self.path_start_col as i32,
            self.path_start_rot,
            &self.move_path,
            actual_row,
            actual_col,
            actual_rot,
        );
        if matched <= self.path_step {
            return false;
        }
        let mut target = self.path_step;
        while target < matched {
            let act = match self.move_path.get(target).map(|s| s.as_str()) {
                Some(a) => a,
                None => break,
            };
            if act != "D" {
                break;
            }
            let next = target + 1;
            let Some((sr, sc, srot)) = simulate_path_prefix(
                bb,
                type_idx,
                self.path_start_row,
                self.path_start_col as i32,
                self.path_start_rot,
                &self.move_path[..next],
            ) else {
                break;
            };
            if sr == actual_row && sc == actual_col as i32 && srot == actual_rot {
                target = next;
            } else {
                break;
            }
        }
        if target > self.path_step {
            self.trace_path(format!(
                "resync step {}→{} @({},{},r{})",
                self.path_step, target, actual_row, actual_col, actual_rot
            ));
            self.path_step = target;
            self.path_commit_row = actual_row;
            self.path_commit_col = actual_col;
            self.path_commit_rot = actual_rot;
            self.path_pending_action = None;
            self.path_resync_stuck_frames = 0;
            self.refresh_path_expected_from_sim(bb, type_idx);
            true
        } else {
            false
        }
    }

    /// Clear stale pending path input and resync path_step to the live pose.
    /// Idle-glitch resume can leave path_pending_action set while button holds were
    /// released — handle_path then blocks on rot confirm and the piece falls on gravity only.
    pub(super) fn recover_stalled_path_pending(
        &mut self,
        actions: &mut Vec<(u8, bool)>,
        bb: &Bitboard,
        type_idx: usize,
        actual_row: i32,
        actual_col: usize,
        actual_rot: usize,
        reason: &str,
    ) -> bool {
        let step_before = self.path_step;
        let pending = self.path_pending_action.take();
        self.post_rot_sync = false;
        self.rot_settle_frames = 0;
        self.lateral_settle_frames = 0;
        self.path_rot_wait_frames = 0;
        self.frame_delay = 0;
        self.release_path_btn_hold(actions);

        if let Some(ref act) = pending {
            if matches!(act.as_str(), "CW" | "CCW")
                && self.path_step < self.move_path.len()
                && self.move_path[self.path_step] == act.as_str()
            {
                let expected_rot = if act == "CW" {
                    (self.path_commit_rot + 1) % 4
                } else {
                    (self.path_commit_rot + 3) % 4
                };
                if actual_rot == expected_rot {
                    self.path_step += 1;
                    self.path_commit_row = actual_row;
                    self.path_commit_col = actual_col;
                    self.path_commit_rot = actual_rot;
                }
            }
        }

        self.resync_path_step_to_actual(bb, type_idx, actual_row, actual_col, actual_rot);
        // Trust live pose — gravity may have advanced rows/cols while pending rot blocked
        // soft-drop; sim prefix can lag and would re-trigger col-drift early returns.
        self.path_expected_row = actual_row;
        self.path_expected_col = actual_col;
        self.path_expected_rot = actual_rot;
        self.trace_path(format!(
            "recover stalled pending ({reason}) @({},{},r{}) step→{}/{} was={:?}",
            actual_row,
            actual_col,
            actual_rot,
            self.path_step + 1,
            self.move_path.len(),
            pending
        ));
        if reason == "rot confirm stall" && self.path_step == step_before {
            self.rot_retry_settle_frames = 4;
            return true;
        }
        false
    }

    pub(crate) fn begin_path_soft_drop(&mut self, actions: &mut Vec<(u8, bool)>) {
        // key_down only — host applies inputs before run_frame; release on row confirm.
        self.holding_down = true;
        self.path_down_min_frames = PATH_DOWN_HOLD_FRAMES;
        self.path_down_release_armed = false;
        actions.push((5, true));
    }

    /// Release Down immediately after one path D-step (never hold through landing).
    fn finish_path_soft_drop(&mut self, actions: &mut Vec<(u8, bool)>) {
        self.holding_down = false;
        self.path_down_release_armed = false;
        self.path_down_min_frames = 0;
        actions.push((5, false));
    }

    fn arm_path_soft_drop_release(&mut self, actions: &mut Vec<(u8, bool)>) {
        if self.path_down_min_frames == 0 && !self.holding_down {
            return;
        }
        if self.path_down_min_frames == 0 {
            self.finish_path_soft_drop(actions);
        } else {
            self.path_down_release_armed = true;
        }
    }

    /// Rosy instant-locks if Down is held while grounded — cancel every path frame.
    pub(crate) fn cancel_soft_drop_if_grounded(
        &mut self,
        actions: &mut Vec<(u8, bool)>,
        bb: &Bitboard,
        type_idx: usize,
        actual_row: i32,
        actual_col: usize,
        actual_rot: usize,
    ) {
        if Self::piece_can_soft_drop(bb, type_idx, actual_rot, actual_row, actual_col as i32) {
            return;
        }
        self.holding_down = false;
        self.path_down_release_armed = false;
        self.path_down_min_frames = 0;
        actions.push((5, false));
    }

    fn begin_path_btn_hold(&mut self, btn: u8, actions: &mut Vec<(u8, bool)>) {
        self.path_held_btn = Some((btn, MIN_BTN_HOLD_FRAMES));
        actions.push((btn, true));
    }

    fn release_path_btn_hold(&mut self, actions: &mut Vec<(u8, bool)>) {
        if let Some((btn, _)) = self.path_held_btn.take() {
            actions.push((btn, false));
        }
    }

    /// Keep path Down/L/R pressed for MIN_BTN_HOLD_FRAMES; release when countdown ends.
    /// Rotation (CW/CCW) stays held until `try_confirm_pending_path_step` clears pending —
    /// Rosy grounded kicks can outlast MIN_BTN_HOLD_FRAMES (Z→T CW @ (10,6,r0)).
    pub(super) fn tick_path_button_holds(&mut self, actions: &mut Vec<(u8, bool)>) {
        if self.holding_down {
            actions.push((5, true));
            if self.path_down_min_frames > 0 {
                self.path_down_min_frames -= 1;
            }
            if self.path_down_release_armed && self.path_down_min_frames == 0 {
                self.holding_down = false;
                self.path_down_release_armed = false;
                actions.push((5, false));
            }
        }
        let pending_rot = matches!(
            self.path_pending_action.as_deref(),
            Some("CW") | Some("CCW")
        );
        if let Some((btn, rem)) = self.path_held_btn {
            actions.push((btn, true));
            if pending_rot {
                if rem > 1 {
                    self.path_held_btn = Some((btn, rem - 1));
                }
            } else if rem <= 1 {
                self.path_held_btn = None;
                actions.push((btn, false));
            } else {
                self.path_held_btn = Some((btn, rem - 1));
            }
        } else if pending_rot {
            let btn = if self.path_pending_action.as_deref() == Some("CW") {
                0
            } else {
                1
            };
            actions.push((btn, true));
        }
    }

    fn piece_can_soft_drop(
        bb: &Bitboard,
        type_idx: usize,
        rot: usize,
        row: i32,
        col: i32,
    ) -> bool {
        !piece_collides(bb, type_idx, rot, row + 1, col)
    }

    /// True when one more soft-drop row lands on the stack (S→L: D onto row 14 then R).
    pub(crate) fn soft_drop_lands_grounded(
        bb: &Bitboard,
        type_idx: usize,
        rot: usize,
        row: i32,
        col: i32,
    ) -> bool {
        Self::piece_can_soft_drop(bb, type_idx, rot, row, col)
            && !Self::piece_can_soft_drop(bb, type_idx, rot, row + 1, col)
    }

    fn path_action_frame_delay(&self, action: &str, urgent_terminal_slide: bool) -> u32 {
        if urgent_terminal_slide {
            0
        } else if action == "D" || action == IMPLICIT_DESCENT {
            if self.pps_limit.is_infinite() && self.input_delay == 0 {
                0
            } else {
                1 + self.input_delay
            }
        } else {
            FRAME_DELAY + self.input_delay
        }
    }

    /// Soft-drop one row toward target_row without advancing path_step.
    /// Returns true when handle_path should return early.
    fn try_implicit_soft_drop(
        &mut self,
        bb: &Bitboard,
        type_idx: usize,
        actual_row: i32,
        actual_col: usize,
        actual_rot: usize,
        target_row: i32,
        actions: &mut Vec<(u8, bool)>,
    ) -> bool {
        if actual_row >= target_row {
            self.row_wait_count = 0;
            return false;
        }
        if self.path_pending_action.is_some()
            || self.holding_down
            || self.path_held_btn.is_some()
            || self.rot_settle_frames > 0
        {
            return true;
        }
        if !Self::piece_can_soft_drop(bb, type_idx, actual_rot, actual_row, actual_col as i32) {
            // Rosy: Down while grounded locks immediately — wait for sync/gravity edge.
            self.row_wait_count += 1;
            if self.row_wait_count > 180 {
                self.row_wait_count = 0;
            }
            return true;
        }
        self.row_wait_count = 0;
        self.begin_path_soft_drop(actions);
        self.path_pending_action = Some(IMPLICIT_DESCENT.to_string());
        self.path_commit_row = actual_row;
        self.path_commit_col = actual_col;
        self.path_commit_rot = actual_rot;
        self.frame_delay = 0;
        true
    }

    /// Advance path_step only after the emu reflects the action we just sent.
    /// Unlike max_matching_path_step, passive gravity cannot skip D-prefix steps.
    pub(crate) fn try_confirm_pending_path_step(
        &mut self,
        actions: &mut Vec<(u8, bool)>,
        bb: &Bitboard,
        type_idx: usize,
        actual_row: i32,
        actual_col: usize,
        actual_rot: usize,
    ) {
        let action = match self.path_pending_action.clone() {
            Some(a) => a,
            None => return,
        };
        let is_tuck = self
            .intended_lock
            .as_ref()
            .is_some_and(|(_, _, _, t)| t == "tuck");

        let confirmed = match action.as_str() {
            IMPLICIT_DESCENT => {
                if actual_row > self.path_commit_row + 1 {
                    self.path_pending_action = None;
                    self.arm_path_soft_drop_release(actions);
                    return;
                }
                if actual_row > self.path_commit_row
                    && actual_col == self.path_commit_col
                    && actual_rot == self.path_commit_rot
                {
                    self.path_commit_row = actual_row;
                    self.path_pending_action = None;
                    self.arm_path_soft_drop_release(actions);
                    if self.path_step < self.move_path.len()
                        && self.move_path[self.path_step] == "D"
                        && matches!(
                            self.move_path.get(self.path_step + 1).map(|s| s.as_str()),
                            Some("CW") | Some("CCW")
                        )
                    {
                        self.trace_path(format!(
                            "implicit descent before setup rot @({},{},r{}) — skip D",
                            actual_row, actual_col, actual_rot
                        ));
                        self.path_step += 1;
                        if self.path_step < self.move_path.len() {
                            match self.move_path[self.path_step].as_str() {
                                "CW" => {
                                    self.begin_path_btn_hold(0, actions);
                                    self.path_pending_action = Some("CW".into());
                                    self.post_rot_sync = true;
                                    self.rot_settle_frames = 0;
                                    self.frame_delay = 0;
                                }
                                "CCW" => {
                                    self.begin_path_btn_hold(1, actions);
                                    self.path_pending_action = Some("CCW".into());
                                    self.post_rot_sync = true;
                                    self.rot_settle_frames = 0;
                                    self.frame_delay = 0;
                                }
                                _ => {}
                            }
                        }
                    }
                }
                return;
            }
            "D" => {
                if actual_row > self.path_commit_row + 1
                    && actual_col == self.path_commit_col
                    && actual_rot == self.path_commit_rot
                {
                    // Soft-drop held too long — piece outran one D-step; catch up.
                    let extra = (actual_row - self.path_commit_row) as usize;
                    self.path_pending_action = None;
                    self.finish_path_soft_drop(actions);
                    self.trace_path(format!(
                        "D catch-up +{} rows @({},{}) step→{}",
                        extra, actual_row, actual_col, self.path_step
                    ));
                    for _ in 0..extra {
                        if self.path_step < self.move_path.len()
                            && self.move_path[self.path_step] == "D"
                        {
                            self.path_step += 1;
                        } else {
                            break;
                        }
                    }
                    self.path_commit_row = actual_row;
                    self.path_commit_col = actual_col;
                    self.path_commit_rot = actual_rot;
                    self.refresh_path_expected_from_sim(bb, type_idx);
                    return;
                }
                actual_row > self.path_commit_row
                    && actual_col == self.path_commit_col
                    && actual_rot == self.path_commit_rot
            }
            "L" | "R" => {
                // One path step = one column (DAS +2 must not confirm two R/L steps).
                let col_step_ok = match action.as_str() {
                    "L" => (actual_col as i32) == self.path_commit_col as i32 - 1,
                    "R" => (actual_col as i32) == self.path_commit_col as i32 + 1,
                    _ => false,
                };
                if self.spin_replan_tail_active()
                    && self.path_prefix_pose_matches(
                        bb,
                        type_idx,
                        self.path_step,
                        actual_row,
                        actual_col,
                        actual_rot,
                    )
                {
                    true
                } else if actual_row != self.path_commit_row {
                    if self.spin_replan_tail_active() {
                        if self.spin_replan_pose_sync(
                            bb, type_idx, actual_row, actual_col, actual_rot,
                        ) {
                            self.release_path_btn_hold(actions);
                            self.chain_spin_replan_action(
                                actions, bb, type_idx, actual_row, actual_col, actual_rot,
                            );
                        } else {
                            self.path_pending_action = None;
                            self.release_path_btn_hold(actions);
                        }
                        return;
                    }
                    // Gravity during mid-path lateral (I→J tuck RR between D-runs): confirm
                    // when col already took the intended ±1 step; else resync D's only.
                    // Without this we re-press R forever and overshoot (want c4 → lock c6).
                    if col_step_ok && actual_rot == self.path_commit_rot {
                        self.trace_path(format!(
                            "lateral+gravity confirm {action} @({},{},r{}) was commit c{}",
                            actual_row, actual_col, actual_rot, self.path_commit_col
                        ));
                        true
                    } else {
                        self.path_pending_action = None;
                        self.resync_path_step_to_actual(
                            bb, type_idx, actual_row, actual_col, actual_rot,
                        );
                        return;
                    }
                } else if !col_step_ok {
                    // Terminal tuck slide at plan row: retry next frame if tap was missed.
                    if is_tuck && actual_row >= self.plan_intended_row {
                        self.path_pending_action = None;
                    }
                    return;
                } else {
                    col_step_ok && actual_rot == self.path_commit_rot
                }
            }
            "CW" | "CCW" => {
                let expected_rot = if action == "CW" {
                    (self.path_commit_rot + 1) % 4
                } else {
                    (self.path_commit_rot + 3) % 4
                };
                if actual_rot != expected_rot {
                    false
                } else if self.is_setup_spin_rot_step() {
                    // Setup spin: sim kick can differ from ROM (T-spin CCW @(13,5,r0)).
                    // Hold rot until sim pose or a gravity-ready terminal pose (e.g. (14,5,r3)).
                    let sim_pose = simulate_path_prefix(
                        bb,
                        type_idx,
                        self.path_start_row,
                        self.path_start_col as i32,
                        self.path_start_rot,
                        &self.move_path[..=self.path_step],
                    );
                    sim_pose.is_some_and(|(sr, sc, srot)| {
                        sr == actual_row && sc == actual_col as i32 && srot == actual_rot
                    }) || self.spin_emu_kick_can_bfs_replan(
                        bb, type_idx, actual_row, actual_col, actual_rot,
                    )
                } else {
                    // Terminal spin / normal: trust emu kicks once rotation state changed.
                    true
                }
            }
            _ => false,
        };
        let setup_spin_emu_kick = matches!(action.as_str(), "CW" | "CCW")
            && self.is_setup_spin_rot_step()
            && !simulate_path_prefix(
                bb,
                type_idx,
                self.path_start_row,
                self.path_start_col as i32,
                self.path_start_rot,
                &self.move_path[..=self.path_step],
            )
            .is_some_and(|(sr, sc, srot)| {
                sr == actual_row && sc == actual_col as i32 && srot == actual_rot
            });

        if confirmed {
            self.path_step += 1;
            self.path_commit_row = actual_row;
            self.path_commit_col = actual_col;
            self.path_commit_rot = actual_rot;
            self.path_pending_action = None;
            self.path_resync_stuck_frames = 0;
            if action == "CW" || action == "CCW" {
                self.release_path_btn_hold(actions);
            }
            if setup_spin_emu_kick {
                let replan_base = self.path_step;
                self.spin_emu_kick_bfs_replan(bb, type_idx, actual_row, actual_col, actual_rot);
                let tail = &self.move_path[replan_base..];
                let matched = max_matching_path_step(
                    bb,
                    type_idx,
                    self.path_start_row,
                    self.path_start_col as i32,
                    self.path_start_rot,
                    tail,
                    actual_row,
                    actual_col,
                    actual_rot,
                );
                if matched > 0 {
                    self.trace_path(format!(
                        "spin replan pose sync step {}→{} @({},{},r{})",
                        self.path_step,
                        replan_base + matched,
                        actual_row,
                        actual_col,
                        actual_rot
                    ));
                    self.path_step = replan_base + matched;
                    self.path_commit_row = actual_row;
                    self.path_commit_col = actual_col;
                    self.path_commit_rot = actual_rot;
                }
                self.chain_spin_replan_action(
                    actions, bb, type_idx, actual_row, actual_col, actual_rot,
                );
            }
            if action == "L" || action == "R" {
                // L/R confirm must end the hold before the next path step (S→L: R+CCW
                // overlap prevented the vertical tuck kick).
                self.release_path_btn_hold(actions);
                if matches!(
                    self.move_path.get(self.path_step).map(|s| s.as_str()),
                    Some("CW") | Some("CCW")
                ) {
                    self.lateral_settle_frames = 3;
                } else if self.path_step < self.move_path.len()
                    && matches!(self.move_path[self.path_step].as_str(), "L" | "R")
                {
                    self.lateral_settle_frames = LATERAL_CHAIN_SETTLE_FRAMES;
                } else if self.path_step < self.move_path.len()
                    && self.move_path[self.path_step] == "D"
                    && matches!(
                        self.move_path.get(self.path_step + 1).map(|s| s.as_str()),
                        Some("CW") | Some("CCW")
                    )
                {
                    // Only skip a D before rot when the rot is the *last* path action and
                    // the piece is already grounded — otherwise the D is needed height
                    // (J spin replan D,L,D,CCW,D: skipping the mid D before CCW locked short).
                    let rot_is_last = self.path_step + 2 >= self.move_path.len();
                    let grounded = !Self::piece_can_soft_drop(
                        bb, type_idx, actual_rot, actual_row, actual_col as i32,
                    );
                    if rot_is_last && grounded {
                        self.trace_path(format!(
                            "R/L confirm — skip grounded D before terminal rot @({},{},r{})",
                            actual_row, actual_col, actual_rot
                        ));
                        self.path_step += 1;
                        if self.path_step < self.move_path.len() {
                            match self.move_path[self.path_step].as_str() {
                                "CW" => {
                                    self.begin_path_btn_hold(0, actions);
                                    self.path_pending_action = Some("CW".into());
                                    self.post_rot_sync = true;
                                    self.rot_settle_frames = 0;
                                    self.frame_delay = 0;
                                }
                                "CCW" => {
                                    self.begin_path_btn_hold(1, actions);
                                    self.path_pending_action = Some("CCW".into());
                                    self.post_rot_sync = true;
                                    self.rot_settle_frames = 0;
                                    self.frame_delay = 0;
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            if action == "D" {
                self.trace_path(format!(
                    "D confirmed @({},{},r{}) step→{}/{}",
                    actual_row,
                    actual_col,
                    actual_rot,
                    self.path_step,
                    self.move_path.len()
                ));
                if !self.spin_replan_tail_active()
                    && self.spin_terminal_rot_reaches_want(
                        bb, type_idx, actual_row, actual_col, actual_rot,
                    )
                {
                    if let Some(term_idx) = self.terminal_spin_rot_index() {
                        if self.path_step <= term_idx {
                            self.trace_path(format!(
                                "spin terminal-ready after D — skip to step {}/{} @({},{},r{})",
                                term_idx + 1,
                                self.move_path.len(),
                                actual_row,
                                actual_col,
                                actual_rot
                            ));
                            self.path_step = term_idx;
                            self.path_commit_row = actual_row;
                            self.path_commit_col = actual_col;
                            self.path_commit_rot = actual_rot;
                            self.finish_path_soft_drop(actions);
                            if self.chain_terminal_spin_rot(
                                actions, actual_row, actual_col, actual_rot,
                            ) {
                                self.refresh_path_expected_from_sim(bb, type_idx);
                                return;
                            }
                        }
                    }
                }
                // Terminal spin rot before Down release — Rosy instant-locks grounded+Down.
                let chained_terminal = self.advance_past_satisfied_spin_ds_chain_terminal(
                    actions,
                    bb,
                    type_idx,
                    actual_row,
                    actual_col,
                    actual_rot,
                );
                self.finish_path_soft_drop(actions);
                if chained_terminal {
                    self.refresh_path_expected_from_sim(bb, type_idx);
                    return;
                }
                // Closed-loop tuck: if this D landed us grounded with only more Ds + L/R left,
                // skip leftover Ds and start the terminal slide same frame (lock-delay budget).
                if self
                    .intended_lock
                    .as_ref()
                    .is_some_and(|(_, _, _, t)| t == "tuck")
                    && Self::path_suffix_is_d_then_terminal_lateral(&self.move_path, self.path_step)
                    && !Self::piece_can_soft_drop(bb, type_idx, actual_rot, actual_row, actual_col as i32)
                    && actual_row >= self.plan_intended_row
                {
                    let d_run = self.move_path[self.path_step..]
                        .iter()
                        .take_while(|a| a.as_str() == "D")
                        .count();
                    if d_run > 0 {
                        self.trace_path(format!(
                            "tuck D-confirm grounded — skip {d_run} well D @({},{},r{}) → terminal",
                            actual_row, actual_col, actual_rot
                        ));
                        self.path_step += d_run;
                        self.path_commit_row = actual_row;
                        self.path_commit_col = actual_col;
                        self.path_commit_rot = actual_rot;
                    }
                }
                // Same-frame terminal slide after grounded well D (L→S tuck R into col).
                if self.path_step < self.move_path.len() {
                    match self.move_path[self.path_step].as_str() {
                        "CW" => {
                            self.begin_path_btn_hold(0, actions);
                            self.path_pending_action = Some("CW".into());
                            self.post_rot_sync = true;
                            self.rot_settle_frames = 0;
                            self.frame_delay = 0;
                        }
                        "CCW" => {
                            self.begin_path_btn_hold(1, actions);
                            self.path_pending_action = Some("CCW".into());
                            self.post_rot_sync = true;
                            self.rot_settle_frames = 0;
                            self.frame_delay = 0;
                        }
                        "L" => {
                            self.begin_path_btn_hold(6, actions);
                            self.path_pending_action = Some("L".into());
                            self.frame_delay = 0;
                        }
                        "R" => {
                            self.begin_path_btn_hold(7, actions);
                            self.path_pending_action = Some("R".into());
                            self.frame_delay = 0;
                        }
                        _ => {}
                    }
                }
            }
            if setup_spin_emu_kick {
                self.path_expected_row = actual_row;
                self.path_expected_col = actual_col;
                self.path_expected_rot = actual_rot;
            } else {
                self.refresh_path_expected_from_sim(bb, type_idx);
            }
            if self.spin_replan_tail_active() {
                self.chain_spin_replan_action(
                    actions, bb, type_idx, actual_row, actual_col, actual_rot,
                );
            }
        }
    }
    /// True when path[step..] contains L/R before the next CW/CCW (Z→T: RRR before tuck rot).
    /// True when only L/R remain after path_step (final spin rotation).
    pub(crate) fn path_suffix_is_lateral_only(move_path: &[String], path_step: usize) -> bool {
        let suffix = &move_path[path_step + 1..];
        !suffix.is_empty() && suffix.iter().all(|a| a == "L" || a == "R")
    }

    /// After D confirm on spin paths: skip sim-satisfied D's before terminal rot, chain rot same frame.
    fn advance_past_satisfied_spin_ds_chain_terminal(
        &mut self,
        actions: &mut Vec<(u8, bool)>,
        bb: &Bitboard,
        type_idx: usize,
        actual_row: i32,
        actual_col: usize,
        actual_rot: usize,
    ) -> bool {
        let il_mtype = self
            .intended_lock
            .as_ref()
            .map(|(_, _, _, t)| t.as_str())
            .unwrap_or("");
        if !matches!(il_mtype, "spin" | "tspin") {
            return false;
        }
        while self.path_step < self.move_path.len() && self.move_path[self.path_step] == "D" {
            let terminal_d_before_rot = self.path_step + 2 == self.move_path.len()
                && matches!(
                    self.move_path.get(self.path_step + 1).map(|s| s.as_str()),
                    Some("CW") | Some("CCW")
                );
            if !terminal_d_before_rot {
                break;
            }
            let Some((sr, sc, srot)) = simulate_path_prefix(
                bb,
                type_idx,
                self.path_start_row,
                self.path_start_col as i32,
                self.path_start_rot,
                &self.move_path[..=self.path_step],
            ) else {
                break;
            };
            if sr != actual_row || sc != actual_col as i32 || srot != actual_rot {
                break;
            }
            self.trace_path(format!(
                "D confirm skip redundant spin D step {}/{} @({},{},r{})",
                self.path_step + 1,
                self.move_path.len(),
                actual_row,
                actual_col,
                actual_rot
            ));
            self.path_step += 1;
            self.path_commit_row = actual_row;
            self.path_commit_col = actual_col;
            self.path_commit_rot = actual_rot;
        }
        self.chain_terminal_spin_rot(actions, actual_row, actual_col, actual_rot)
    }

    fn terminal_spin_rot_pending(&self) -> bool {
        matches!(
            self.intended_lock.as_ref().map(|(_, _, _, t)| t.as_str()),
            Some("spin" | "tspin")
        ) && self.path_step + 1 == self.move_path.len()
            && matches!(
                self.path_pending_action.as_deref(),
                Some("CW") | Some("CCW")
            )
    }

    /// Fire terminal spin CW/CCW immediately — grounded lock window is one frame.
    fn chain_terminal_spin_rot(
        &mut self,
        actions: &mut Vec<(u8, bool)>,
        actual_row: i32,
        actual_col: usize,
        actual_rot: usize,
    ) -> bool {
        let il_mtype = self
            .intended_lock
            .as_ref()
            .map(|(_, _, _, t)| t.as_str())
            .unwrap_or("");
        if !matches!(il_mtype, "spin" | "tspin")
            || self.path_step >= self.move_path.len()
            || self.path_step + 1 != self.move_path.len()
        {
            return false;
        }
        match self.move_path[self.path_step].as_str() {
            "CW" => {
                self.trace_path(format!(
                    "chain terminal CW step {}/{} @({},{},r{})",
                    self.path_step + 1,
                    self.move_path.len(),
                    actual_row,
                    actual_col,
                    actual_rot
                ));
                self.begin_path_btn_hold(0, actions);
                self.path_pending_action = Some("CW".into());
                self.post_rot_sync = true;
                self.rot_settle_frames = 0;
                self.frame_delay = 0;
                true
            }
            "CCW" => {
                self.trace_path(format!(
                    "chain terminal CCW step {}/{} @({},{},r{})",
                    self.path_step + 1,
                    self.move_path.len(),
                    actual_row,
                    actual_col,
                    actual_rot
                ));
                self.begin_path_btn_hold(1, actions);
                self.path_pending_action = Some("CCW".into());
                self.post_rot_sync = true;
                self.rot_settle_frames = 0;
                self.frame_delay = 0;
                true
            }
            _ => false,
        }
    }

    fn spin_replan_tail_active(&self) -> bool {
        self.spin_emu_replan_at_step
            .is_some_and(|s| self.path_step >= s)
    }

    /// True when `move_path[..=step]` simulates to the live pose from path_start.
    fn path_prefix_pose_matches(
        &self,
        bb: &Bitboard,
        type_idx: usize,
        step: usize,
        row: i32,
        col: usize,
        rot: usize,
    ) -> bool {
        simulate_path_prefix(
            bb,
            type_idx,
            self.path_start_row,
            self.path_start_col as i32,
            self.path_start_rot,
            &self.move_path[..=step],
        )
        .is_some_and(|(sr, sc, srot)| sr == row && sc == col as i32 && srot == rot)
    }

    /// Fast-forward path_step on emu-kick replan tail when live pose matches sim prefix.
    fn spin_replan_pose_sync(
        &mut self,
        bb: &Bitboard,
        type_idx: usize,
        row: i32,
        col: usize,
        rot: usize,
    ) -> bool {
        let Some(replan_at) = self.spin_emu_replan_at_step else {
            return false;
        };
        if self.path_step < replan_at {
            return false;
        }
        let matched = max_matching_path_step(
            bb,
            type_idx,
            self.path_start_row,
            self.path_start_col as i32,
            self.path_start_rot,
            &self.move_path,
            row,
            col,
            rot,
        );
        if matched <= self.path_step {
            return false;
        }
        self.trace_path(format!(
            "spin replan pose sync step {}→{} @({},{},r{})",
            self.path_step, matched, row, col, rot
        ));
        self.path_step = matched;
        self.path_commit_row = row;
        self.path_commit_col = col;
        self.path_commit_rot = rot;
        self.path_pending_action = None;
        self.path_resync_stuck_frames = 0;
        self.refresh_path_expected_from_sim(bb, type_idx);
        true
    }

    /// Issue the next replan-tail input immediately (zero frame_delay).
    fn chain_spin_replan_action(
        &mut self,
        actions: &mut Vec<(u8, bool)>,
        bb: &Bitboard,
        type_idx: usize,
        row: i32,
        col: usize,
        rot: usize,
    ) {
        if !self.spin_replan_tail_active() || self.path_step >= self.move_path.len() {
            return;
        }
        let action = self.move_path[self.path_step].clone();
        self.path_commit_row = row;
        self.path_commit_col = col;
        self.path_commit_rot = rot;
        self.lateral_settle_frames = 0;
        self.frame_delay = 0;
        match action.as_str() {
            "R" => {
                self.trace_path(format!(
                    "chain replan R step {}/{} @({},{},r{})",
                    self.path_step + 1,
                    self.move_path.len(),
                    row,
                    col,
                    rot
                ));
                self.begin_path_btn_hold(7, actions);
                self.path_pending_action = Some("R".into());
            }
            "L" => {
                self.trace_path(format!(
                    "chain replan L step {}/{} @({},{},r{})",
                    self.path_step + 1,
                    self.move_path.len(),
                    row,
                    col,
                    rot
                ));
                self.begin_path_btn_hold(6, actions);
                self.path_pending_action = Some("L".into());
            }
            "D" => {
                if Self::piece_can_soft_drop(bb, type_idx, rot, row, col as i32) {
                    self.trace_path(format!(
                        "chain replan D step {}/{} @({},{},r{})",
                        self.path_step + 1,
                        self.move_path.len(),
                        row,
                        col,
                        rot
                    ));
                    self.begin_path_soft_drop(actions);
                    self.path_pending_action = Some("D".into());
                }
            }
            "CW" => {
                self.trace_path(format!(
                    "chain replan CW step {}/{} @({},{},r{})",
                    self.path_step + 1,
                    self.move_path.len(),
                    row,
                    col,
                    rot
                ));
                self.begin_path_btn_hold(0, actions);
                self.path_pending_action = Some("CW".into());
                self.post_rot_sync = true;
                self.rot_settle_frames = 0;
            }
            "CCW" => {
                self.trace_path(format!(
                    "chain replan CCW step {}/{} @({},{},r{})",
                    self.path_step + 1,
                    self.move_path.len(),
                    row,
                    col,
                    rot
                ));
                self.begin_path_btn_hold(1, actions);
                self.path_pending_action = Some("CCW".into());
                self.post_rot_sync = true;
                self.rot_settle_frames = 0;
            }
            _ => {}
        }
    }

    fn spin_emu_kick_can_bfs_replan(
        &self,
        bb: &Bitboard,
        type_idx: usize,
        row: i32,
        col: usize,
        rot: usize,
    ) -> bool {
        let Some((want_row, want_col, want_rot, _)) = self.intended_lock.as_ref() else {
            return false;
        };
        find_bfs_path_to_lock(
            bb,
            type_idx,
            row,
            col,
            rot,
            *want_row,
            *want_col as usize,
            *want_rot,
        )
        .is_some()
    }

    /// BFS a new path from live pose to `intended_lock`.
    /// `replace_all`: grounded tuck mid-rot stall (drop dead prefix).
    /// `!replace_all`: spin emu-kick (keep confirmed prefix, append tail).
    fn replan_path_from_live_pose(
        &mut self,
        bb: &Bitboard,
        type_idx: usize,
        row: i32,
        col: usize,
        rot: usize,
        replace_all: bool,
        reason: &str,
    ) -> bool {
        let Some((want_row, want_col, want_rot, mtype)) = self.intended_lock.clone() else {
            return false;
        };
        let Some((_, _, path, _)) = find_bfs_path_to_lock(
            bb,
            type_idx,
            row,
            col,
            rot,
            want_row,
            want_col as usize,
            want_rot,
        ) else {
            return false;
        };
        let path = if replace_all {
            prefer_simplest_equivalent_path(
                bb,
                type_idx,
                row,
                col as i32,
                rot,
                want_row,
                want_col,
                want_rot,
                &path,
            )
        } else {
            path.into_iter().map(|s| s.to_string()).collect()
        };
        self.trace_path(format!(
            "{reason} {:?} @({},{},r{}) → ({},{},r{})",
            path, row, col, rot, want_row, want_col, want_rot
        ));
        if replace_all {
            self.move_path = path;
            self.path_step = 0;
            self.plan_intended_row = plan_row_before_final_action(
                bb, type_idx, row, col as i32, rot, &self.move_path, want_row, &mtype, want_rot,
            );
        } else {
            self.move_path.truncate(self.path_step);
            self.move_path.extend(path);
            self.spin_emu_replan_at_step = Some(self.path_step);
        }
        self.path_start_row = row;
        self.path_start_col = col;
        self.path_start_rot = rot;
        self.path_commit_row = row;
        self.path_commit_col = col;
        self.path_commit_rot = rot;
        self.path_pending_action = None;
        self.path_rot_wait_frames = 0;
        self.path_resync_stuck_frames = 0;
        self.rot_settle_frames = 0;
        self.rot_retry_settle_frames = 0;
        true
    }

    fn spin_emu_kick_bfs_replan(
        &mut self,
        bb: &Bitboard,
        type_idx: usize,
        row: i32,
        col: usize,
        rot: usize,
    ) -> bool {
        self.replan_path_from_live_pose(
            bb, type_idx, row, col, rot, false, "spin emu-kick BFS replan",
        )
    }

    fn is_setup_spin_rot_step(&self) -> bool {
        // After one emu-kick BFS replan, remaining rots use normal confirm — a second
        // replan thrash locked J spin short (misdrop_j_spin_r16_c1_r0: CW replan then
        // CCW replan → (12,1,r0) instead of (16,1,r0)).
        if self.spin_emu_replan_at_step.is_some() {
            return false;
        }
        let mtype = self
            .intended_lock
            .as_ref()
            .map(|(_, _, _, t)| t.as_str())
            .unwrap_or("");
        if !matches!(mtype, "spin" | "tspin") || self.path_step + 1 >= self.move_path.len() {
            return false;
        }
        matches!(
            self.move_path.get(self.path_step).map(|s| s.as_str()),
            Some("CW") | Some("CCW")
        )
    }

    fn terminal_spin_rot_index(&self) -> Option<usize> {
        if !self
            .intended_lock
            .as_ref()
            .is_some_and(|(_, _, _, t)| matches!(t.as_str(), "spin" | "tspin"))
        {
            return None;
        }
        self.move_path
            .iter()
            .rposition(|a| a == "CW" || a == "CCW")
    }

    /// True when the final spin rotation in the plan reaches want_lock from this pose.
    fn spin_terminal_rot_reaches_want(
        &self,
        bb: &Bitboard,
        type_idx: usize,
        row: i32,
        col: usize,
        rot: usize,
    ) -> bool {
        let Some((want_row, want_col, want_rot, _)) = self.intended_lock.as_ref() else {
            return false;
        };
        let Some(term_idx) = self.terminal_spin_rot_index() else {
            return false;
        };
        let cw = self.move_path[term_idx] == "CW";
        srs::srs_try_rotate_grounded(bb, type_idx, row, col as i32, rot, cw)
            .is_some_and(|(r, c, r2)| r == *want_row && c == *want_col as i32 && r2 == *want_rot)
    }

    /// True when executing D's after a tuck/spin rotation (S→L: D×3 after vertical CCW).
    fn path_in_spin_post_rotation_descent(&self, actual_rot: usize) -> bool {
        if self
            .spin_emu_replan_at_step
            .is_some_and(|s| self.path_step >= s)
        {
            return false;
        }
        if actual_rot == 0 || self.path_step >= self.move_path.len() {
            return false;
        }
        if self.move_path[self.path_step] != "D" {
            return false;
        }
        let mtype = self
            .intended_lock
            .as_ref()
            .map(|(_, _, _, t)| t.as_str())
            .unwrap_or("");
        if !matches!(mtype, "spin" | "tspin") {
            return false;
        }
        self.move_path[..self.path_step]
            .iter()
            .any(|a| a == "CW" || a == "CCW")
    }

    /// True when the next lateral group has no D-steps after it (tuck terminal slide).
    fn path_rest_is_terminal_lateral(move_path: &[String], path_step: usize) -> bool {
        let rest = &move_path[path_step..];
        match rest.iter().position(|a| a != "L" && a != "R") {
            None => true,
            Some(idx) => !rest[idx..].iter().any(|a| a == "D"),
        }
    }

    /// True when path[step..] is one or more D's then only L/R (tuck well descent).
    pub(crate) fn path_suffix_is_d_then_terminal_lateral(move_path: &[String], path_step: usize) -> bool {
        let rest = &move_path[path_step..];
        let d_run = rest.iter().take_while(|a| *a == "D").count();
        if d_run == 0 || d_run >= rest.len() {
            return false;
        }
        rest[d_run..].iter().all(|a| a == "L" || a == "R")
    }

    /// Tuck well: D-run after setup rot, before terminal slide — each D needs soft-drop.
    fn tuck_in_well_descent(&self) -> bool {
        if self.path_step >= self.move_path.len() {
            return false;
        }
        if self
            .intended_lock
            .as_ref()
            .map(|(_, _, _, t)| t.as_str())
            != Some("tuck")
        {
            return false;
        }
        if self.move_path[self.path_step] != "D" {
            return false;
        }
        if !Self::path_suffix_is_d_then_terminal_lateral(&self.move_path, self.path_step) {
            return false;
        }
        self.move_path[..self.path_step]
            .iter()
            .any(|a| a == "CW" || a == "CCW")
    }

    /// Insert R/L at `path_step` when emu well column differs from BFS sim but plan row is reached.
    fn ensure_tuck_terminal_col_reach(&mut self, actual_col: usize) {
        let Some((_, il_col, _, mtype)) = self.intended_lock.clone() else {
            return;
        };
        if mtype != "tuck" || self.path_step >= self.move_path.len() {
            return;
        }
        if !Self::path_rest_is_terminal_lateral(&self.move_path, self.path_step) {
            return;
        }
        let target_col = il_col as usize;
        if actual_col == target_col {
            return;
        }
        let delta = il_col - actual_col as i32;
        if delta == 0 {
            return;
        }
        let act = if delta > 0 { "R" } else { "L" };
        let needed = delta.unsigned_abs() as usize;
        let mut net = 0i32;
        for a in &self.move_path[self.path_step..] {
            match a.as_str() {
                "R" => net += 1,
                "L" => net -= 1,
                _ => break,
            }
        }
        let path_reach = if delta > 0 { net.max(0) } else { (-net).max(0) };
        let missing = needed.saturating_sub(path_reach as usize);
        if missing == 0 {
            return;
        }
        for _ in 0..missing {
            self.move_path.insert(self.path_step, act.to_string());
        }
        self.trace_path(format!(
            "tuck terminal col compensate +{missing} {act} @col{actual_col} want col{target_col}"
        ));
    }

    /// Tuck path_step sync while idle: if grounded at plan row with D* then L/R left,
    /// skip leftover well D's (path goals, not D-tape). Airborne: do not skip D's.
    pub(crate) fn sync_tuck_path_step(
        &mut self,
        bb: &Bitboard,
        type_idx: usize,
        actual_row: i32,
        actual_col: usize,
        actual_rot: usize,
    ) {
        if self.path_pending_action.is_some() || self.holding_down || self.path_held_btn.is_some() {
            return;
        }
        let il_mtype = self
            .intended_lock
            .as_ref()
            .map(|(_, _, _, t)| t.as_str())
            .unwrap_or("");
        if il_mtype != "tuck" {
            return;
        }
        if self.path_step >= self.move_path.len() {
            return;
        }
        if !Self::path_suffix_is_d_then_terminal_lateral(&self.move_path, self.path_step) {
            return;
        }
        if actual_row < self.plan_intended_row {
            return;
        }
        if Self::piece_can_soft_drop(bb, type_idx, actual_rot, actual_row, actual_col as i32) {
            return;
        }
        let d_run = self.move_path[self.path_step..]
            .iter()
            .take_while(|a| a.as_str() == "D")
            .count();
        if d_run == 0 {
            return;
        }
        self.trace_path(format!(
            "tuck sync skip {d_run} well D @({},{},r{}) → terminal",
            actual_row, actual_col, actual_rot
        ));
        self.path_step += d_run;
        self.path_commit_row = actual_row;
        self.path_commit_col = actual_col;
        self.path_commit_rot = actual_rot;
    }
    pub(super) fn handle_path(&mut self, read: &impl Fn(u16)->u8, read_r: &impl Fn(u16,u16)->Vec<u8>, actions: &mut Vec<(u8,bool)>, ori: u8) {
        let info = ori_info(ori);
        let cur_type = info.map(|i| i.0);
        let last_type = ori_info(self.last_ori).map(|i| i.0);
        let type_mismatch = match (cur_type, last_type) {
            (Some(c), Some(l)) => c != l,
            (None, _) => true,
            (Some(_), None) => true,
        };
        // Brief ori/piece-type glitches mid-path are common during ARE / lock — trust spawn metadata.
        let spawn_piece = self
            .last_placement
            .as_ref()
            .map(|lp| lp.current_piece.piece_type);
        let next_piece = self.last_placement.as_ref().map(|lp| lp.next_piece.piece_type);
        let path_done = !self.move_path.is_empty() && self.path_step >= self.move_path.len();
        let snap_resume = !self.move_path.is_empty()
            && self.path_step < self.move_path.len()
            && self.last_valid_snap.is_some_and(|(sr, sc, _)| piece_pos_trustworthy(sr, sc))
            && self.intended_lock.as_ref().is_some_and(|(_, _, _, t)| {
                matches!(t.as_str(), "tuck" | "spin" | "tspin")
            });

        // Path finished: ori may already show the next piece — verify lock via begin_drop.
        if path_done && (info.is_none() || type_mismatch) {
            self.trace_path("path complete — ori transition → begin_drop");
            self.holding_down = false;
            self.row_wait_count = 0;
            self.piece_count += 1;
            self.state = BotState::Idle;
            // Next-piece ori is visible; lock verify must use the piece we executed.
            self.begin_drop(read, read_r, self.last_ori, actions);
            return;
        }

        // Piece locked before path finished (e.g. L spin interrupted by garbage row 29
        // then ARE shows next piece). Must not spin on "ori glitch ignored" forever.
        if type_mismatch
            && !self.move_path.is_empty()
            && cur_type.is_some_and(|c| next_piece == Some(c))
            && last_type.is_some_and(|l| spawn_piece == Some(l))
        {
            let raw_row = piece_min_row(|a| read(a));
            let raw_col = piece_left_col(|a| read(a));
            let falling_piece_still_active = self.last_valid_snap.is_some_and(|(sr, sc, srot)| {
                if !piece_pos_trustworthy(sr, sc) || self.path_step >= self.move_path.len() {
                    return false;
                }
                if !piece_pos_trustworthy(raw_row, raw_col) {
                    // Garbage/off-board read — snap is authoritative; next-piece ori is ARE noise.
                    return true;
                }
                if raw_row != sr || raw_col != sc {
                    return false;
                }
                let bb = read_board_bitboard(|s, l| read_r(s, l));
                let type_idx = self
                    .last_placement
                    .as_ref()
                    .map(|lp| lp.current_piece.piece_type)
                    .unwrap_or(99);
                type_idx <= 6 && !lock_anchor_filled(&bb, type_idx, srot, sr, sc as i32)
            });
            if falling_piece_still_active {
                // resume path from snap (no log)
            } else {
                let (piece, next) = self.placement_piece_labels();
                self.trace_path(format!(
                    "next piece spawned mid-path ({}/{}) piece={piece} next={next} — lock transition",
                    self.path_step,
                    self.move_path.len()
                ));
                self.lock_verify_path_incomplete = self.path_step < self.move_path.len();
                // Incomplete tuck/spin: sprite snap can match an intermediate pose — board only.
                self.schedule_lock_verify(read_r, false);
                self.lock_verify_post_frame = true;
                if let Some((row, col, rot)) = self.last_valid_snap {
                    self.start_lock_audit(row, col as usize, rot, true);
                }
                self.move_path.clear();
                self.holding_down = false;
                self.state = BotState::Idle;
                self.last_ori = 0xff;
                return;
            }
        }

        // Brief flicker back to the spawn piece type mid-path (ARE noise).
        if type_mismatch
            && !self.move_path.is_empty()
            && cur_type.is_some_and(|c| spawn_piece == Some(c))
        {
            self.trace_path(format!(
                "ori glitch ignored (cur={cur_type:?} last={last_type:?})"
            ));
            return;
        } else if (info.is_none() || type_mismatch) && !snap_resume {
            if info.is_none() && !self.move_path.is_empty() {
                self.trace_path("ori read none mid-path — wait");
                return;
            }
            let special_path = self.intended_lock.as_ref().is_some_and(|(_, _, _, t)| {
                matches!(t.as_str(), "tuck" | "spin" | "tspin")
            });
            if special_path && !self.move_path.is_empty() && self.path_step < self.move_path.len() {
                self.trace_path(format!(
                    "mid-path special ori glitch (cur={cur_type:?} last={last_type:?}) — wait"
                ));
                return;
            }
            if self.skip_misdrop_if_replay_restore() {
                return;
            }
            if self.last_valid_snap.is_some() && self.path_step < self.move_path.len() {
                self.trace_path(format!(
                    "ori glitch — wait for resync (cur={cur_type:?} last={last_type:?})"
                ));
                return;
            }
            self.trace_path(format!(
                "ori abort — not misdrop ({:?})",
                MisdropReason::PathAbortedGarbageOri
            ));
            self.holding_down = false;
            self.move_path.clear();
            self.state = BotState::Idle;
            self.last_ori = 0xff;
            return;
        }

        let raw_row = piece_min_row(|a| read(a));
        let (type_idx, mut actual_row, mut actual_col, mut actual_rot) =
            if snap_resume && type_mismatch {
                let (sr, sc, srot) = self.last_valid_snap.unwrap();
                (spawn_piece.unwrap(), sr, sc, srot)
            } else {
                let i = info.unwrap();
                (
                    i.0,
                    raw_row,
                    piece_left_col(|a| read(a)),
                    i.1 as usize,
                )
            };
        if snap_resume && type_mismatch {
            self.path_expected_row = actual_row;
            self.path_expected_col = actual_col;
            self.path_expected_rot = actual_rot;
            self.path_resync_stuck_frames = 0;
        }

        let mid_path = !self.move_path.is_empty() && self.path_step < self.move_path.len();
        if raw_row >= BOARD_ROWS as i32 {
            if mid_path {
                let pending_rot = matches!(
                    self.path_pending_action.as_deref(),
                    Some("CW") | Some("CCW")
                );
                if let Some((sr, sc, srot)) = self.last_valid_snap {
                    if piece_pos_trustworthy(sr, sc) {
                        self.trace_path(format!(
                            "garbage row {raw_row} mid-path — snap ({sr},{sc},r{srot}) live_r{actual_rot} pending_rot={pending_rot}"
                        ));
                        actual_row = sr;
                        actual_col = sc;
                        if !pending_rot {
                            actual_rot = srot;
                        }
                    } else if self.skip_misdrop_if_replay_restore() {
                        self.trace_path(format!(
                            "garbage row {raw_row} during replay restore — wait"
                        ));
                        return;
                    } else {
                        self.trace_path(format!(
                            "garbage row {raw_row} — wait for valid pose"
                        ));
                        return;
                    }
                } else if self.skip_misdrop_if_replay_restore() {
                    self.trace_path(format!(
                        "garbage row {raw_row} during replay restore — wait"
                    ));
                    return;
                } else {
                    self.trace_path(format!("garbage row {raw_row} — wait for valid pose"));
                    return;
                }
            } else if self.skip_misdrop_if_replay_restore() {
                self.trace_path(format!(
                    "garbage row {raw_row} during replay restore — wait"
                ));
                return;
            } else {
                self.trace_path(format!("garbage row {raw_row} — wait for valid pose"));
                return;
            }
        }

        self.note_valid_piece_snap(actual_row, actual_col, actual_rot);

        let bb = read_board_bitboard(|s, l| read_r(s, l));
        if self.spin_replan_tail_active()
            && self.path_pending_action.is_none()
            && !self.holding_down
            && self.path_held_btn.is_none()
            && self.rot_settle_frames == 0
            && self.spin_replan_pose_sync(&bb, type_idx, actual_row, actual_col, actual_rot)
        {
            self.chain_spin_replan_action(
                actions, &bb, type_idx, actual_row, actual_col, actual_rot,
            );
            return;
        }
        if self.path_pending_action.is_none()
            && !self.holding_down
            && self.path_held_btn.is_none()
            && self.rot_settle_frames == 0
        {
            let step_before = self.path_step;
            if self.resync_path_step_to_actual(
                &bb, type_idx, actual_row, actual_col, actual_rot,
            ) && self.path_step > step_before
                && self.path_step < self.move_path.len()
                && step_before < self.move_path.len()
                && self.move_path[step_before] == "D"
                && matches!(
                    self.move_path[self.path_step].as_str(),
                    "CW" | "CCW"
                )
            {
                match self.move_path[self.path_step].as_str() {
                    "CW" => {
                        self.trace_path(format!(
                            "resync chain CW step {}/{} @({},{},r{})",
                            self.path_step + 1,
                            self.move_path.len(),
                            actual_row,
                            actual_col,
                            actual_rot
                        ));
                        self.begin_path_btn_hold(0, actions);
                        self.path_pending_action = Some("CW".into());
                        self.post_rot_sync = true;
                        self.rot_settle_frames = 0;
                        self.frame_delay = 0;
                        return;
                    }
                    "CCW" => {
                        self.trace_path(format!(
                            "resync chain CCW step {}/{} @({},{},r{})",
                            self.path_step + 1,
                            self.move_path.len(),
                            actual_row,
                            actual_col,
                            actual_rot
                        ));
                        self.begin_path_btn_hold(1, actions);
                        self.path_pending_action = Some("CCW".into());
                        self.post_rot_sync = true;
                        self.rot_settle_frames = 0;
                        self.frame_delay = 0;
                        return;
                    }
                    _ => {}
                }
            }
        }
        if self.rot_settle_frames > 0 {
            self.rot_settle_frames -= 1;
        } else {
            self.try_confirm_pending_path_step(
                actions,
                &bb,
                type_idx,
                actual_row,
                actual_col,
                actual_rot,
            );
        }
        self.cancel_soft_drop_if_grounded(
            actions, &bb, type_idx, actual_row, actual_col, actual_rot,
        );
        // Well D's after setup rot: sim col can differ from emu SRS kick (S tuck col5);
        // anchoring expected to actual keeps soft-drop issuing every D instead of gravity.
        if self.tuck_in_well_descent() && self.path_pending_action.is_none()
        {
            self.path_expected_row = actual_row;
            self.path_expected_col = actual_col;
            self.path_expected_rot = actual_rot;
        } else {
            self.refresh_path_expected_from_sim(&bb, type_idx);
        }
        self.sync_tuck_path_step(&bb, type_idx, actual_row, actual_col, actual_rot);

        // post rot sync (SRS) — BFS paths already use srs_try_rotate kicks; sync
        // expected position to actual. Do NOT undo kicks via L/R (Z→S CCW col 3→2
        // was wrongly compensated with R, landing 1 col too far right).
        if self.post_rot_sync {
            self.post_rot_sync = false;
            self.path_expected_row = actual_row;
            self.path_expected_col = actual_col;
            if self.path_step >= self.move_path.len() {
                if let Some((_, il_col, _, _)) = self.intended_lock.clone() {
                    let delta = il_col - actual_col as i32;
                    if delta != 0 {
                        let act = if delta > 0 { "R" } else { "L" };
                        let n = delta.abs() as usize;
                        for _ in 0..n {
                            self.move_path.insert(self.path_step, act.to_string());
                        }
                    }
                }
                return;
            }
        }

        // Wait for pending rotation confirm — don't block_replan loop (S→L vertical tuck CCW).
        // Always tick wait frames: grounded mid-path rots never increase row, so the old
        // "only count when row advances" guard hung forever (J tuck r15 c6: 2nd CCW).
        if matches!(
            self.path_pending_action.as_deref(),
            Some("CW") | Some("CCW")
        ) {
            self.path_rot_wait_frames = self.path_rot_wait_frames.saturating_add(1);
            if self.path_rot_wait_frames > 24
                || actual_row > self.path_commit_row.saturating_add(2)
            {
                // Grounded tuck: failed mid-path rot — replan from live pose to want.
                // Before recover_stalled (retries same rot forever — j_tuck r15 c6).
                let grounded_tuck = self
                    .intended_lock
                    .as_ref()
                    .is_some_and(|(_, _, _, t)| t == "tuck")
                    && !Self::piece_can_soft_drop(
                        &bb, type_idx, actual_rot, actual_row, actual_col as i32,
                    );
                if grounded_tuck
                    && self.replan_path_from_live_pose(
                        &bb,
                        type_idx,
                        actual_row,
                        actual_col,
                        actual_rot,
                        true,
                        "tuck mid-rot stall replan",
                    )
                {
                    self.release_path_btn_hold(actions);
                    return;
                }
                if self.recover_stalled_path_pending(
                    actions,
                    &bb,
                    type_idx,
                    actual_row,
                    actual_col,
                    actual_rot,
                    "rot confirm stall",
                ) {
                    return;
                }
            } else {
                let special = self.intended_lock.as_ref().is_some_and(|(_, _, _, t)| {
                    matches!(t.as_str(), "tuck" | "spin" | "tspin")
                });
                if special
                    && actual_row > self.path_commit_row
                    && actual_rot == self.path_commit_rot
                    && self.path_rot_wait_frames >= 2
                    && Self::piece_can_soft_drop(
                        &bb, type_idx, actual_rot, actual_row, actual_col as i32,
                    )
                {
                    self.trace_path(format!(
                        "rot wait gravity seize — soft-drop @({},{},r{}) pending={:?}",
                        actual_row,
                        actual_col,
                        actual_rot,
                        self.path_pending_action
                    ));
                    self.path_pending_action = None;
                    self.release_path_btn_hold(actions);
                    self.path_rot_wait_frames = 0;
                    self.begin_path_soft_drop(actions);
                    self.path_pending_action = Some(IMPLICIT_DESCENT.to_string());
                    self.path_commit_row = actual_row;
                    self.path_commit_col = actual_col;
                    self.path_commit_rot = actual_rot;
                    return;
                }
                return;
            }
        } else {
            self.path_rot_wait_frames = 0;
        }

        // deviation
        if actual_col != self.path_expected_col || actual_rot != self.path_expected_rot {
            if actual_rot == self.path_expected_rot {
                let path_done = self.path_step >= self.move_path.len();
                let is_tuck = self.intended_lock.as_ref().map(|(_, _, _, t)| t == "tuck").unwrap_or(false);
                // Gravity/ori-glitch stall: live pose descended past sim prefix while
                // soft-drop was blocked — accept actual and keep executing (S tuck col7).
                // Col guard only during well D* (Z tuck passive-gravity fix).
                if !path_done
                    && is_tuck
                    && actual_row >= self.path_expected_row
                    && (!self.tuck_in_well_descent()
                        || actual_col == self.path_expected_col)
                {
                    self.path_expected_row = actual_row;
                    self.path_expected_col = actual_col;
                    self.path_expected_rot = actual_rot;
                    let matched = max_matching_path_step(
                        &bb,
                        type_idx,
                        self.path_start_row,
                        self.path_start_col as i32,
                        self.path_start_rot,
                        &self.move_path,
                        actual_row,
                        actual_col,
                        actual_rot,
                    );
                    // Live pose already matches a longer path prefix than path_step.
                    // Consume setup L/R/CW/CCW that DAS/gravity already applied — otherwise
                    // leftover R's overshoot the tuck well (Z→J: col 6 plan → col 7 lock).
                    // Never gravity-skip well D* (PathPlanVsExecution / J-tuck well D rule).
                    if matched > self.path_step {
                        let advanced = &self.move_path[self.path_step..matched];
                        let only_setup = advanced
                            .iter()
                            .all(|a| matches!(a.as_str(), "L" | "R" | "CW" | "CCW"));
                        if only_setup {
                            self.trace_path(format!(
                                "gravity-ahead consume setup step {}→{}/{} @({},{},r{})",
                                self.path_step,
                                matched,
                                self.move_path.len(),
                                actual_row,
                                actual_col,
                                actual_rot
                            ));
                            self.path_step = matched;
                            self.path_commit_row = actual_row;
                            self.path_commit_col = actual_col;
                            self.path_commit_rot = actual_rot;
                        } else if matched == self.move_path.len() {
                            // Full plan pose reached — never mid-path well D fast-forward.
                            self.trace_path(format!(
                                "gravity-ahead path complete step {}→{}/{}",
                                self.path_step,
                                matched,
                                self.move_path.len()
                            ));
                            self.path_step = matched;
                            self.path_commit_row = actual_row;
                            self.path_commit_col = actual_col;
                            self.path_commit_rot = actual_rot;
                        }
                    }
                } else {
                    // Tuck safety-wait: path complete but slide blocked above plan row.
                    if path_done && is_tuck && actual_row < self.plan_intended_row {
                        if self.try_implicit_soft_drop(
                            &bb, type_idx, actual_row, actual_col, actual_rot,
                            self.plan_intended_row, actions,
                        ) {
                            if self.row_wait_count > 180 {
                                self.row_wait_count = 0;
                                self.state = BotState::Idle;
                                self.last_ori = 0xff;
                                return;
                            }
                            return;
                        }
                    }
                    if path_done {
                        self.col_drift_count += 1;
                        if self.col_drift_count > 8 {
                            self.col_drift_count = 0;
                            self.state = BotState::Idle;
                            self.begin_drop(read, read_r, ori, actions);
                            return;
                        }
                    } else {
                        self.col_drift_count = 0;
                    }
                    if self.spin_replan_tail_active()
                        && self.spin_replan_pose_sync(
                            &bb, type_idx, actual_row, actual_col, actual_rot,
                        )
                    {
                        self.chain_spin_replan_action(
                            actions, &bb, type_idx, actual_row, actual_col, actual_rot,
                        );
                        return;
                    }
                    if self.resync_path_step_to_actual(
                        &bb, type_idx, actual_row, actual_col, actual_rot,
                    ) {
                        return;
                    }
                    // Col drift without path mutation — wait for pending confirm or replan.
                    return;
                }
            } else if self.rot_settle_frames > 0 {
                return;
            } else {
                // Mid-path 1-ply replan replaces the BFS path with a short suffix from the
                // sim position; SRS kick / soft-drop drift leaves the piece off (J→O, S→L).
                let block_replan = self.intended_lock.as_ref().is_some_and(|(_, _, _, t)| {
                    matches!(t.as_str(), "tuck" | "spin" | "tspin")
                });
                if block_replan {
                    self.path_resync_stuck_frames += 1;
                    self.trace_path(format!(
                        "block_replan rot drift @({},{},r{}) exp=({},{},r{}) step={}/{} stuck={}",
                        actual_row,
                        actual_col,
                        actual_rot,
                        self.path_expected_row,
                        self.path_expected_col,
                        self.path_expected_rot,
                        self.path_step,
                        self.move_path.len(),
                        self.path_resync_stuck_frames
                    ));
                    if !self.resync_path_step_to_actual(
                        &bb, type_idx, actual_row, actual_col, actual_rot,
                    ) {
                        self.path_expected_row = actual_row;
                        self.path_expected_col = actual_col;
                        self.path_expected_rot = actual_rot;
                    }
                    if self.path_resync_stuck_frames > 24 {
                        if let Some((r, l, p, row)) = find_best_move_with_bfs_1ply(
                            |a| read(a),
                            |b, ll| read_r(b, ll),
                            actual_row,
                            actual_col,
                            actual_rot,
                        ).filter(|(r, l, p, row)| {
                            bfs_plan_acceptable(
                                &bb,
                                type_idx,
                                actual_row,
                                actual_col,
                                actual_rot,
                                0,
                                actual_row,
                                *row,
                                *l,
                                *r,
                                p,
                            )
                        }) {
                            let mtype = classify_move(
                                &bb, type_idx, row, l as i32, r, &p, 0,
                            );
                            let full_path = prefer_simplest_equivalent_path(
                                &bb,
                                type_idx,
                                actual_row,
                                actual_col as i32,
                                actual_rot,
                                row,
                                l as i32,
                                r,
                                &trim_redundant_setup_d(
                                    &bb,
                                    type_idx,
                                    actual_row,
                                    actual_col as i32,
                                    actual_rot,
                                    p.clone(),
                                ),
                            );
                            let plan_row = plan_row_before_final_action(
                                &bb,
                                type_idx,
                                actual_row,
                                actual_col as i32,
                                actual_rot,
                                &full_path,
                                row,
                                mtype,
                                r,
                            );
                            self.trace_path(format!(
                                "tuck recovery replan @({},{},r{}) → ({},{},r{}) {:?}",
                                actual_row, actual_col, actual_rot, row, l, r, full_path
                            ));
                            self.move_path = full_path;
                            self.path_step = 0;
                            self.path_start_row = actual_row;
                            self.path_start_col = actual_col;
                            self.path_start_rot = actual_rot;
                            self.path_commit_row = actual_row;
                            self.path_commit_col = actual_col;
                            self.path_commit_rot = actual_rot;
                            self.path_pending_action = None;
                            self.target_rot = r;
                            self.target_left = l;
                            self.plan_intended_row = plan_row;
                            self.intended_lock =
                                Some((row, l as i32, r, mtype.to_string()));
                            self.path_resync_stuck_frames = 0;
                            self.col_drift_count = 0;
                            self.rot_settle_frames = 0;
                            self.lateral_settle_frames = 0;
                            return;
                        }
                    }
                    self.col_drift_count = 0;
                    return;
                }
                // rot mismatch: replan 1ply normal only (tuck/spin → safe normal via filter)
                if let Some((r, l, p, row)) = find_best_move_with_bfs_1ply(|a|read(a), |b,ll|read_r(b,ll), actual_row, actual_col, actual_rot).filter(|(r,l,p,row)| {
                    bfs_plan_acceptable(&bb, type_idx, actual_row, actual_col, actual_rot, 0, actual_row, *row, *l, *r, p)
                }) {
                    let type_idx = info.unwrap().0;
                    let bb = read_board_bitboard(|s, l| read_r(s, l));
                    let mtype = classify_move(&bb, type_idx, row, l as i32, r, &p, 0);
                    let full_path = prefer_simplest_equivalent_path(
                        &bb,
                        type_idx,
                        actual_row,
                        actual_col as i32,
                        actual_rot,
                        row,
                        l as i32,
                        r,
                        &trim_redundant_setup_d(
                            &bb,
                            type_idx,
                            actual_row,
                            actual_col as i32,
                            actual_rot,
                            p.clone(),
                        ),
                    );
                    let plan_row = plan_row_before_final_action(
                        &bb, type_idx, actual_row, actual_col as i32, actual_rot, &full_path, row,
                        mtype, r,
                    );
                    self.move_path = full_path;
                    self.path_step = 0;
                    self.path_start_row = actual_row;
                    self.path_start_col = actual_col;
                    self.path_start_rot = actual_rot;
                    self.path_commit_row = actual_row;
                    self.path_commit_col = actual_col;
                    self.path_commit_rot = actual_rot;
                    self.path_pending_action = None;
                    self.note_valid_piece_snap(actual_row, actual_col, actual_rot);
                    self.post_rot_sync = false;
                    self.target_rot = r;
                    self.target_left = l;
                    self.plan_intended_row = plan_row;
                    self.intended_lock = Some((row, l as i32, r, mtype.to_string()));
                    self.row_wait_count = 0;
                    self.col_drift_count = 0;
                    self.rot_settle_frames = 0;
                    self.lateral_settle_frames = 0;
                    return;
                } else {
                    self.state = BotState::Idle;
                    self.begin_drop(read, read_r, ori, actions);
                    return;
                }
            }
        }

        // path complete -> descend to plan row if needed, verify lock, then hard drop
        if self.path_step >= self.move_path.len() {
            if actual_row < self.plan_intended_row {
                if self.try_implicit_soft_drop(
                    &bb, type_idx, actual_row, actual_col, actual_rot,
                    self.plan_intended_row, actions,
                ) {
                    if self.row_wait_count > 180 {
                        self.row_wait_count = 0;
                        self.state = BotState::Idle;
                        self.last_ori = 0xff;
                        return;
                    }
                    return;
                }
            }
            self.row_wait_count = 0;
            self.piece_count += 1;
            // Misdrop is checked in begin_drop once the piece is grounded — not here
            // while still falling above the lock row (J-spin false positives).
            self.state = BotState::Idle;
            self.begin_drop(read, read_r, ori, actions);
            return;
        }

        // Row approach before spin final-rot or tuck terminal-lateral (controlled soft-drop).
        let next_a = &self.move_path[self.path_step];
        let il_row = self.plan_intended_row;
        let il_mtype = self
            .intended_lock
            .as_ref()
            .map(|(_, _, _, t)| t.clone())
            .unwrap_or_default();
        let is_final_rot = (next_a == "CW" || next_a == "CCW")
            && (il_mtype.as_str() == "spin" || il_mtype.as_str() == "tspin")
            && Self::path_suffix_is_lateral_only(&self.move_path, self.path_step)
            && actual_row < il_row;
        if is_final_rot {
            if self.try_implicit_soft_drop(
                &bb, type_idx, actual_row, actual_col, actual_rot, il_row, actions,
            ) {
                return;
            }
        } else if (next_a == "L" || next_a == "R")
            && il_mtype.as_str() == "tuck"
            && actual_row < il_row
            && Self::path_rest_is_terminal_lateral(&self.move_path, self.path_step)
        {
            if self.try_implicit_soft_drop(
                &bb, type_idx, actual_row, actual_col, actual_rot, il_row, actions,
            ) {
                return;
            }
        } else if self.row_wait_count > 0 {
            self.row_wait_count = 0;
        }

        if self.rot_settle_frames > 0 {
            return;
        }

        if self.lateral_settle_frames > 0 {
            self.lateral_settle_frames -= 1;
            return;
        }

        if self.rot_retry_settle_frames > 0 {
            self.rot_retry_settle_frames -= 1;
            return;
        }

        if self.path_pending_action.is_some() || self.path_held_btn.is_some() {
            return;
        }

        // execute — path_step advances only after try_confirm_pending_path_step
        let action = self.move_path[self.path_step].clone();

        // Never stack inputs on stale Down; release before lateral/rot or next D.
        if self.holding_down || self.path_down_release_armed {
            let grounded =
                !Self::piece_can_soft_drop(&bb, type_idx, actual_rot, actual_row, actual_col as i32);
            if action == "D" && !grounded {
                self.finish_path_soft_drop(actions);
            } else if grounded || action == "CW" || action == "CCW" || action == "L" || action == "R" {
                self.finish_path_soft_drop(actions);
                self.trace_path(format!(
                    "release Down before {} @({},{},r{}) grounded={}",
                    action, actual_row, actual_col, actual_rot, grounded
                ));
            } else {
                self.trace_path(format!(
                    "blocked {} — Down still held @({},{},r{})",
                    action, actual_row, actual_col, actual_rot
                ));
                return;
            }
        }

        // Tuck well D*: plan D×N means "reach tuck row", not "press D that many times".
        // Once grounded at/ past plan row with only L/R left after remaining D's, skip the
        // leftover D's and run the terminal slide in the lock-delay window
        // closed-loop, not blind D-tape.
        let mut action = action;
        if action == "D"
            && il_mtype.as_str() == "tuck"
            && Self::path_suffix_is_d_then_terminal_lateral(&self.move_path, self.path_step)
            && actual_row >= self.plan_intended_row
            && !Self::piece_can_soft_drop(&bb, type_idx, actual_rot, actual_row, actual_col as i32)
        {
            let d_run = self.move_path[self.path_step..]
                .iter()
                .take_while(|a| a.as_str() == "D")
                .count();
            self.trace_path(format!(
                "tuck at plan row grounded — skip {d_run} well D @({},{},r{}) → terminal slide",
                actual_row, actual_col, actual_rot
            ));
            self.path_step += d_run;
            self.path_commit_row = actual_row;
            self.path_commit_col = actual_col;
            self.path_commit_rot = actual_rot;
            self.finish_path_soft_drop(actions);
            self.refresh_path_expected_from_sim(&bb, type_idx);
            self.row_wait_count = 0;
            if self.path_step >= self.move_path.len() {
                return;
            }
            action = self.move_path[self.path_step].clone();
        }

        if matches!(action.as_str(), "L" | "R")
            && il_mtype.as_str() == "tuck"
            && Self::path_rest_is_terminal_lateral(&self.move_path, self.path_step)
        {
            self.ensure_tuck_terminal_col_reach(actual_col);
            // ensure_tuck may insert L/R at path_step — re-read
            if self.path_step < self.move_path.len() {
                action = self.move_path[self.path_step].clone();
            }
        }

        match action.as_str() {
            "L" => {
                let terminal_l_before_ccw = matches!(
                    self.move_path.get(self.path_step + 1).map(|s| s.as_str()),
                    Some("CCW")
                ) && matches!(il_mtype.as_str(), "spin" | "tspin");
                if terminal_l_before_ccw
                    && !Self::piece_can_soft_drop(
                        &bb, type_idx, actual_rot, actual_row, actual_col as i32,
                    )
                    && self.spin_terminal_rot_reaches_want(
                        &bb, type_idx, actual_row, actual_col, actual_rot,
                    )
                {
                    self.trace_path(format!(
                        "spin terminal L skipped (grounded) @({},{},r{}) step {}/{}",
                        actual_row,
                        actual_col,
                        actual_rot,
                        self.path_step + 1,
                        self.move_path.len()
                    ));
                    self.path_step += 1;
                    self.path_commit_row = actual_row;
                    self.path_commit_col = actual_col;
                    self.path_commit_rot = actual_rot;
                    self.release_path_btn_hold(actions);
                    self.path_pending_action = None;
                    if self.path_step < self.move_path.len()
                        && self.move_path[self.path_step] == "CCW"
                    {
                        self.trace_path(format!(
                            "chain CCW step {}/{} @({},{},r{})",
                            self.path_step + 1,
                            self.move_path.len(),
                            actual_row,
                            actual_col,
                            actual_rot
                        ));
                        self.begin_path_btn_hold(1, actions);
                        self.path_pending_action = Some("CCW".into());
                        self.post_rot_sync = true;
                        self.rot_settle_frames = 0;
                        self.frame_delay = 0;
                    }
                    return;
                }
                self.trace_path(format!(
                    "exec L step {}/{} @({},{},r{})",
                    self.path_step + 1,
                    self.move_path.len(),
                    actual_row,
                    actual_col,
                    actual_rot
                ));
                self.begin_path_btn_hold(6, actions);
            }
            "R" => {
                self.trace_path(format!(
                    "exec R step {}/{} @({},{},r{})",
                    self.path_step + 1,
                    self.move_path.len(),
                    actual_row,
                    actual_col,
                    actual_rot
                ));
                self.begin_path_btn_hold(7, actions);
            }
            "D" => {
                let d_before_setup_rot = matches!(
                    self.move_path.get(self.path_step + 1).map(|s| s.as_str()),
                    Some("CW") | Some("CCW")
                ) && matches!(il_mtype.as_str(), "spin" | "tuck" | "tspin")
                    && self.path_step + 2 < self.move_path.len();
                if d_before_setup_rot {
                    let chain_setup_rot = |bot: &mut TetrisBot,
                                           actions: &mut Vec<(u8, bool)>,
                                           actual_row,
                                           actual_col,
                                           actual_rot| {
                        if bot.path_step >= bot.move_path.len() {
                            return false;
                        }
                        match bot.move_path[bot.path_step].as_str() {
                            "CW" => {
                                bot.release_path_btn_hold(actions);
                                bot.trace_path(format!(
                                    "chain CW step {}/{} @({},{},r{})",
                                    bot.path_step + 1,
                                    bot.move_path.len(),
                                    actual_row,
                                    actual_col,
                                    actual_rot
                                ));
                                bot.begin_path_btn_hold(0, actions);
                                bot.path_pending_action = Some("CW".into());
                                bot.post_rot_sync = true;
                                bot.rot_settle_frames = 0;
                                bot.frame_delay = 0;
                                true
                            }
                            "CCW" => {
                                bot.release_path_btn_hold(actions);
                                bot.trace_path(format!(
                                    "chain CCW step {}/{} @({},{},r{})",
                                    bot.path_step + 1,
                                    bot.move_path.len(),
                                    actual_row,
                                    actual_col,
                                    actual_rot
                                ));
                                bot.begin_path_btn_hold(1, actions);
                                bot.path_pending_action = Some("CCW".into());
                                bot.post_rot_sync = true;
                                bot.rot_settle_frames = 0;
                                true
                            }
                            _ => false,
                        }
                    };
                    if let Some((sr, sc, srot)) = simulate_path_prefix(
                        &bb,
                        type_idx,
                        self.path_start_row,
                        self.path_start_col as i32,
                        self.path_start_rot,
                        &self.move_path[..=self.path_step],
                    ) {
                        if sr == actual_row
                            && sc == actual_col as i32
                            && srot == actual_rot
                        {
                            self.trace_path(format!(
                                "skip D before setup rot step {}/{} @({},{},r{})",
                                self.path_step + 1,
                                self.move_path.len(),
                                actual_row,
                                actual_col,
                                actual_rot
                            ));
                            self.path_step += 1;
                            self.path_commit_row = actual_row;
                            self.path_commit_col = actual_col;
                            self.path_commit_rot = actual_rot;
                            self.refresh_path_expected_from_sim(&bb, type_idx);
                            if chain_setup_rot(
                                self, actions, actual_row, actual_col, actual_rot,
                            ) {
                                return;
                            }
                            return;
                        }
                    }
                    if !Self::piece_can_soft_drop(
                        &bb, type_idx, actual_rot, actual_row, actual_col as i32,
                    ) {
                        return;
                    }
                    // Last resort: tap Down once (fall through).
                }
                if !matches!(il_mtype.as_str(), "tuck" | "spin" | "tspin") {
                    if let Some((sr, sc, srot)) = simulate_path_prefix(
                        &bb,
                        type_idx,
                        self.path_start_row,
                        self.path_start_col as i32,
                        self.path_start_rot,
                        &self.move_path[..=self.path_step],
                    ) {
                        if sr == actual_row
                            && sc == actual_col as i32
                            && srot == actual_rot
                        {
                            self.trace_path(format!(
                                "gravity-satisfied D step {}/{} @({},{},r{})",
                                self.path_step + 1,
                                self.move_path.len(),
                                actual_row,
                                actual_col,
                                actual_rot
                            ));
                            self.path_step += 1;
                            self.path_commit_row = actual_row;
                            self.path_commit_col = actual_col;
                            self.path_commit_rot = actual_rot;
                            self.refresh_path_expected_from_sim(&bb, type_idx);
                            return;
                        }
                    }
                }
                let spin_post_rot_d = self.path_in_spin_post_rotation_descent(actual_rot);
                if !Self::piece_can_soft_drop(
                    &bb, type_idx, actual_rot, actual_row, actual_col as i32,
                ) {
                    if il_mtype.as_str() == "tuck" {
                        return;
                    }
                    if spin_post_rot_d
                        && actual_row > self.path_commit_row
                        && actual_col == self.path_commit_col
                        && actual_rot == self.path_commit_rot
                    {
                        self.trace_path(format!(
                            "post-rot gravity sync D step {}/{} @({},{},r{})",
                            self.path_step + 1,
                            self.move_path.len(),
                            actual_row,
                            actual_col,
                            actual_rot
                        ));
                        self.path_step += 1;
                        self.path_commit_row = actual_row;
                        self.path_commit_col = actual_col;
                        self.path_commit_rot = actual_rot;
                        self.refresh_path_expected_from_sim(&bb, type_idx);
                        return;
                    }
                    if actual_rot != 0 {
                        let sim_ok = simulate_path_prefix(
                            &bb,
                            type_idx,
                            self.path_start_row,
                            self.path_start_col as i32,
                            self.path_start_rot,
                            &self.move_path[..=self.path_step],
                        )
                        .is_some_and(|(sr, sc, srot)| {
                            sr == actual_row
                                && sc == actual_col as i32
                                && srot == actual_rot
                        });
                        if sim_ok || !spin_post_rot_d {
                            self.trace_path(format!(
                                "grounded skip D step {}/{} @({},{},r{}) sim_ok={sim_ok}",
                                self.path_step + 1,
                                self.move_path.len(),
                                actual_row,
                                actual_col,
                                actual_rot
                            ));
                            self.path_step += 1;
                            self.path_commit_row = actual_row;
                            self.path_commit_col = actual_col;
                            self.path_commit_rot = actual_rot;
                            self.refresh_path_expected_from_sim(&bb, type_idx);
                            if self.chain_terminal_spin_rot(
                                actions, actual_row, actual_col, actual_rot,
                            ) {
                                return;
                            }
                            return;
                        }
                        self.trace_path(format!(
                            "spin grounded wait gravity @({},{},r{}) step {}/{}",
                            actual_row,
                            actual_col,
                            actual_rot,
                            self.path_step + 1,
                            self.move_path.len()
                        ));
                        return;
                    }
                    // BFS D-step satisfied by prior descent; Down would instant-lock (S→L).
                    self.trace_path(format!(
                        "grounded skip D step {}/{} @({},{},r{})",
                        self.path_step + 1,
                        self.move_path.len(),
                        actual_row,
                        actual_col,
                        actual_rot
                    ));
                    self.path_step += 1;
                    self.path_commit_row = actual_row;
                    self.path_commit_col = actual_col;
                    self.path_commit_rot = actual_rot;
                    self.refresh_path_expected_from_sim(&bb, type_idx);
                    if self.chain_terminal_spin_rot(
                        actions, actual_row, actual_col, actual_rot,
                    ) {
                        return;
                    }
                    return;
                }
                if !spin_post_rot_d
                    && actual_rot == 0
                    && !d_before_setup_rot
                    && il_mtype.as_str() == "normal"
                    && Self::soft_drop_lands_grounded(
                        &bb, type_idx, actual_rot, actual_row, actual_col as i32,
                    )
                {
                    // Last row before stack: passive gravity only (Rosy locks if Down
                    // is held when the piece lands — S→L dies at (14,3) before R/CCW).
                    self.trace_path(format!(
                        "lands_grounded wait gravity @({},{},r{}) step={}/{}",
                        actual_row,
                        actual_col,
                        actual_rot,
                        self.path_step + 1,
                        self.move_path.len()
                    ));
                    if actual_row > self.path_commit_row {
                        self.path_step += 1;
                        self.path_commit_row = actual_row;
                        self.path_commit_col = actual_col;
                        self.path_commit_rot = actual_rot;
                        self.refresh_path_expected_from_sim(&bb, type_idx);
                    }
                    return;
                }
                self.trace_path(format!(
                    "exec D step {}/{} @({},{},r{})",
                    self.path_step + 1,
                    self.move_path.len(),
                    actual_row,
                    actual_col,
                    actual_rot
                ));
                self.begin_path_soft_drop(actions);
            }
            "CW" => {
                self.release_path_btn_hold(actions);
                self.trace_path(format!(
                    "exec CW step {}/{} @({},{},r{}) btn=A",
                    self.path_step + 1,
                    self.move_path.len(),
                    actual_row,
                    actual_col,
                    actual_rot
                ));
                self.begin_path_btn_hold(0, actions);
                self.post_rot_sync = true;
                let terminal_spin_rot_step = matches!(il_mtype.as_str(), "spin" | "tspin")
                    && self.path_step + 1 == self.move_path.len();
                let setup_tuck_rot = matches!(il_mtype.as_str(), "tuck" | "spin" | "tspin")
                    && self.path_step + 1 < self.move_path.len();
                self.rot_settle_frames =
                    if setup_tuck_rot || terminal_spin_rot_step { 0 } else { 4 };
            }
            "CCW" => {
                self.release_path_btn_hold(actions);
                self.trace_path(format!(
                    "exec CCW step {}/{} @({},{},r{}) btn=B",
                    self.path_step + 1,
                    self.move_path.len(),
                    actual_row,
                    actual_col,
                    actual_rot
                ));
                self.begin_path_btn_hold(1, actions);
                self.post_rot_sync = true;
                let terminal_spin_rot_step = matches!(il_mtype.as_str(), "spin" | "tspin")
                    && self.path_step + 1 == self.move_path.len();
                let setup_tuck_rot = matches!(il_mtype.as_str(), "tuck" | "spin" | "tspin")
                    && self.path_step + 1 < self.move_path.len();
                self.rot_settle_frames =
                    if setup_tuck_rot || terminal_spin_rot_step { 0 } else { 4 };
            }
            _ => {}
        }
        self.path_pending_action = Some(action.clone());
        let terminal_spin_rot = (action == "CW" || action == "CCW")
            && self.path_step + 1 == self.move_path.len()
            && matches!(il_mtype.as_str(), "spin" | "tspin");
        let setup_tuck_rot_exec = (action == "CW" || action == "CCW")
            && self.path_step + 1 < self.move_path.len()
            && matches!(il_mtype.as_str(), "tuck" | "spin" | "tspin");
        let urgent_terminal_slide = self.spin_replan_tail_active()
            || ((action == "L" || action == "R")
                && matches!(il_mtype.as_str(), "tuck" | "spin" | "tspin")
                && (actual_row >= il_row
                    || !Self::piece_can_soft_drop(
                        &bb, type_idx, actual_rot, actual_row, actual_col as i32,
                    )));
        self.frame_delay = if action == "D"
            || terminal_spin_rot
            || setup_tuck_rot_exec
            || self.spin_replan_tail_active()
        {
            0
        } else {
            self.path_action_frame_delay(&action, urgent_terminal_slide)
        };
        self.status_msg = format!("path({}/{})", self.path_step + 1, self.move_path.len());
    }
}
