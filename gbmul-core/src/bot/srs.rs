//! Guideline SRS wall-kick tables and rotation for the BFS planner.
//!
//! Kick offsets use wiki convention: x = right, y = up.
//! Board coords: row increases downward → kick_row = -kick_y.

use super::{board::BFS_ROW_MIN, BOARD_COLS, BOARD_ROWS, Bitboard, SHAPES};

/// Guideline SRS mino positions in the 4×4 rotation matrix (row, col).
const SRS_CELLS: [[[(i8, i8); 4]; 4]; 7] = [
    // I — four distinct 4×4 states (0/2 horizontal at rows 1/2, R/L vertical at cols 0/2)
    [
        [(1, 0), (1, 1), (1, 2), (1, 3)], // 0 spawn
        [(0, 0), (1, 0), (2, 0), (3, 0)], // R
        [(2, 0), (2, 1), (2, 2), (2, 3)], // 2
        [(0, 2), (1, 2), (2, 2), (3, 2)], // L
    ],
    // O
    [
        [(1, 0), (1, 1), (2, 0), (2, 1)],
        [(1, 0), (1, 1), (2, 0), (2, 1)],
        [(1, 0), (1, 1), (2, 0), (2, 1)],
        [(1, 0), (1, 1), (2, 0), (2, 1)],
    ],
    // T
    [
        [(1, 1), (2, 0), (2, 1), (2, 2)],
        [(0, 0), (1, 0), (1, 1), (2, 0)],
        [(0, 0), (0, 1), (0, 2), (1, 1)],
        [(0, 1), (1, 0), (1, 1), (2, 1)],
    ],
    // S
    [
        [(1, 1), (1, 2), (2, 0), (2, 1)],
        [(0, 0), (1, 0), (1, 1), (2, 1)],
        [(1, 1), (1, 2), (2, 0), (2, 1)],
        [(0, 0), (1, 0), (1, 1), (2, 1)],
    ],
    // Z
    [
        [(1, 0), (1, 1), (2, 1), (2, 2)],
        [(0, 1), (1, 0), (1, 1), (2, 0)],
        [(1, 0), (1, 1), (2, 1), (2, 2)],
        [(0, 1), (1, 0), (1, 1), (2, 0)],
    ],
    // L
    [
        [(1, 2), (2, 0), (2, 1), (2, 2)],
        [(0, 0), (1, 0), (2, 0), (2, 1)],
        [(0, 0), (0, 1), (0, 2), (1, 0)],
        [(0, 0), (0, 1), (1, 1), (2, 1)],
    ],
    // J
    [
        [(1, 0), (2, 0), (2, 1), (2, 2)],
        [(0, 0), (0, 1), (1, 0), (2, 0)],
        [(0, 0), (0, 1), (0, 2), (1, 2)],
        [(0, 1), (1, 1), (2, 0), (2, 1)],
    ],
];

/// SRS true-rotation center within the 4×4 matrix (row, col); y-down rows.
/// JLSTZ: center on a mino (integer). I/O: intersection of gridlines per guideline.
const SRS_CENTER: [[(f64, f64); 4]; 7] = [
    [(0.5, 1.5), (1.5, 0.5), (2.5, 1.5), (1.5, 2.5)], // I
    [(1.5, 1.5); 4], // O
    [(1.0, 1.0), (0.0, 1.0), (1.0, 1.0), (2.0, 1.0)], // T
    [(1.0, 1.0), (1.0, 0.0), (1.0, 1.0), (1.0, 1.0)], // S
    // Z: per-orientation centers from free-space A/B on Rosy
    // (`misdrop_z_tuck_r16_c2_r2_20260718`). Old all-(1,0) flipped r0 CW left
    // (sim col−1) vs hardware right (col+1). Floor S/Z up-same-col policy is
    // separate — this is true-rotation before kicks.
    [(2.0, 1.0), (1.0, 0.0), (1.0, 1.0), (1.0, 1.0)], // Z
    [(1.0, 1.0), (1.0, 0.0), (1.0, 1.0), (1.0, 1.0)], // L
    [(1.0, 0.0), (1.0, 0.0), (1.0, 1.0), (1.0, 2.0)], // J
];

/// JLSTZ kick tests (x right, y up). Index = from_rot for CW / CCW tables.
const JLSTZ_CW: [[(i32, i32); 5]; 4] = [
    [(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)], // 0→R
    [(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)],     // R→2
    [(0, 0), (1, 0), (1, 1), (0, -2), (1, -2)],    // 2→L
    [(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)],  // L→0
];

const JLSTZ_CCW: [[(i32, i32); 5]; 4] = [
    [(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)],      // R→0
    [(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)],  // 2→R
    [(0, 0), (1, 0), (1, 1), (0, -2), (1, -2)],     // L→2
    [(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)],   // 0→L
];

const I_CW: [[(i32, i32); 5]; 4] = [
    [(0, 0), (-2, 0), (1, 0), (-2, -1), (1, 2)],
    [(0, 0), (-1, 0), (2, 0), (-1, 2), (2, -1)],
    [(0, 0), (2, 0), (-1, 0), (2, 1), (-1, -2)],
    [(0, 0), (1, 0), (-2, 0), (1, -2), (-2, 1)],
];

const I_CCW: [[(i32, i32); 5]; 4] = [
    [(0, 0), (2, 0), (-1, 0), (2, 1), (-1, -2)],
    [(0, 0), (1, 0), (-2, 0), (1, -2), (-2, 1)],
    [(0, 0), (-2, 0), (1, 0), (-2, -1), (1, 2)],
    [(0, 0), (-1, 0), (2, 0), (-1, 2), (2, -1)],
];

/// Offset from bounding-box top-left to SRS 4×4 matrix origin (row, col).
fn matrix_origin_offset(type_idx: usize, rot: usize) -> (i8, i8) {
    let srs = &SRS_CELLS[type_idx][rot];
    let shape = &SHAPES[type_idx][rot];
    let srs_min_r = srs.iter().map(|c| c.0).min().unwrap();
    let srs_min_c = srs.iter().map(|c| c.1).min().unwrap();
    let shape_min_r = shape.iter().map(|c| c[0]).min().unwrap();
    let shape_min_c = shape.iter().map(|c| c[1]).min().unwrap();
    // srs_origin + srs_cell = bbox + shape_cell  →  origin = bbox + (shape_min - srs_min)
    (
        shape_min_r - srs_min_r,
        shape_min_c - srs_min_c,
    )
}

fn kicks(type_idx: usize, from_rot: usize, cw: bool) -> &'static [(i32, i32); 5] {
    let idx = if cw { from_rot } else { (from_rot + 3) % 4 };
    match type_idx {
        0 => if cw { &I_CW[idx] } else { &I_CCW[idx] },
        1 => &[(0, 0); 5], // O: no kicks
        _ => if cw { &JLSTZ_CW[idx] } else { &JLSTZ_CCW[idx] },
    }
}

fn rotate_point(r: i32, c: i32, cr: f64, cc: f64, cw: bool) -> (i32, i32) {
    let dr = r as f64 - cr;
    let dc = c as f64 - cc;
    if cw {
        let nr = (cr + dc).round() as i32;
        let nc = (cc - dr).round() as i32;
        (nr, nc)
    } else {
        let nr = (cr - dc).round() as i32;
        let nc = (cc + dr).round() as i32;
        (nr, nc)
    }
}

/// Board-space cells for piece at bounding-box anchor.
fn board_cells(type_idx: usize, rot: usize, row: i32, col: i32) -> [(i32, i32); 4] {
    let mut out = [(0, 0); 4];
    for (i, &[dr, dc]) in SHAPES[type_idx][rot].iter().enumerate() {
        out[i] = (row + dr as i32, col + dc as i32);
    }
    out
}

/// SRS true rotation without kicks: new bounding-box anchor after rotating minos.
fn srs_basic_rotate_anchor(
    type_idx: usize,
    row: i32,
    col: i32,
    from_rot: usize,
    to_rot: usize,
    cw: bool,
) -> (i32, i32) {
    let (mor, moc) = matrix_origin_offset(type_idx, from_rot);
    let srs_row = row + mor as i32;
    let srs_col = col + moc as i32;
    let (cr, cc) = SRS_CENTER[type_idx][from_rot];
    let center_r = srs_row as f64 + cr;
    let center_c = srs_col as f64 + cc;

    let cells = board_cells(type_idx, from_rot, row, col);
    let mut min_r = i32::MAX;
    let mut min_c = i32::MAX;
    for &(r, c) in &cells {
        let (nr, nc) = rotate_point(r, c, center_r, center_c, cw);
        min_r = min_r.min(nr);
        min_c = min_c.min(nc);
    }

    // Convert min board cell back to bbox anchor for to_rot.
    let shape = &SHAPES[type_idx][to_rot];
    let shape_min_r = shape.iter().map(|c| c[0] as i32).min().unwrap();
    let shape_min_c = shape.iter().map(|c| c[1] as i32).min().unwrap();
    (
        min_r - shape_min_r,
        min_c - shape_min_c,
    )
}

/// True when the piece cannot move one row down (resting on floor or stack).
pub fn piece_grounded(bb: &Bitboard, type_idx: usize, rot: usize, row: i32, col: i32) -> bool {
    super::piece_collides(bb, type_idx, rot, row + 1, col)
}

/// Lowest mino row (larger = deeper on the board) for a placed piece.
fn piece_max_row(type_idx: usize, rot: usize, row: i32, col: i32) -> i32 {
    SHAPES[type_idx][rot]
        .iter()
        .map(|c| row + c[0] as i32)
        .max()
        .unwrap_or(row)
}

/// True when a mino sits in a cavity under a lip (empty cells below, then a block further down).
/// Matches SRS “basic rotation obstructed” floor scenarios ([Tetris Wiki wall-kick J example](https://tetris.wiki/Super_Rotation_System#Wall_Kicks)).
pub fn under_overhang(bb: &Bitboard, type_idx: usize, rot: usize, row: i32, col: i32) -> bool {
    for &[dr, dc] in &SHAPES[type_idx][rot] {
        let r = row + dr as i32;
        let c = col + dc as i32;
        if c < 0 || c >= BOARD_COLS as i32 {
            continue;
        }
        let mut gap = false;
        for scan in (r + 1).max(0)..BOARD_ROWS as i32 {
            if (bb[scan as usize] >> c) & 1 == 1 {
                return gap;
            }
            gap = true;
        }
    }
    false
}

/// True when SRS floor-kick planning rules apply: resting on stack/floor, or any mino
/// is already under a lip (cavity). See [Tetris Wiki — Basic Rotation](https://tetris.wiki/Super_Rotation_System#Basic_Rotation):
/// JLSTZ spawn states are “floating”; the bounding box can sit below the stack surface,
/// so floor rotations need wall kicks.
pub fn srs_floor_policy(bb: &Bitboard, type_idx: usize, rot: usize, row: i32, col: i32) -> bool {
    piece_grounded(bb, type_idx, rot, row, col) || under_overhang(bb, type_idx, rot, row, col)
}

/// Guideline SRS kick selection with optional **floor** policy for BFS planning.
///
/// References (floor / wall kick behaviour):
/// - [Tetris Wiki — SRS Wall Kicks](https://tetris.wiki/Super_Rotation_System#Wall_Kicks):
///   five tests in order; Test 1 `(0,0)` is basic rotation; **if obstructed**, kicks apply;
///   if all five fail, rotation fails. J/L floor example: Test 1 fails, Tests 2–4 fail,
///   Test 5 `(+1,-2)` succeeds.
/// - [Hard Drop — SRS](https://harddrop.com/wiki/SRS): same tables; R→2 / R→0 include
///   Test 4 `(0,+2)` and Test 5 `(+1,+2)` — **upward** floor kicks (positive y).
///
/// **Floor-kick policy** (`floor_policy == true`):
/// 1. Reject downward kicks (`ky < 0`) while on the floor ([Hard Drop SRS](https://harddrop.com/wiki/SRS)).
/// 2. **Grounded under a lip** (`under_overhang` from-pose): same-row / deeper basic
///    rotation and non-climbing kicks are rejected even if cells are empty
///    (`misdrop_l_spin_r13_c6_r1` test-0, `misdrop_l_spin_r15_c3_r2` kick (1,1)).
///    Climbing rotations under a lip stay legal (J spin setup).
/// 3. When **grounded**, test 0 that shifts the piece **downward** (`nr > row`) is blocked,
///    and test 0 that digs minos onto the **bottom floor row** is blocked even if cells fit
///    (`misdrop_l_spin_r15_c2_r1_20260717` — fiction floor spin; mid-stack digs stay legal).
/// 4. When **grounded** and test 0 is blocked, **only** upward kicks (`ky > 0`) are eligible —
///    horizontal test-1 floor wins are false positives (J spin `misdrop_j_spin_r16_c5_r2`).
/// 5. If test 0 is blocked and any upward kick (`ky > 0`) fits, **only** upward kicks
///    are eligible (R→2 tests 4–5). When test 0 fits (and is not cavity-obstructed), it wins.
/// 6. S/Z grounded horizontal (r0/r2): GB floor spin is up-one same-col when it fits
///    (`misdrop_z_spin_r15_c1_r1`, `…_20260718-103523` CW@(16,3)→(15,3) not (15,1)).
/// 7. I grounded vertical: r0≡r2 / r1≡r3 visually but A≠B on floor — CCW r1→r0 test-0
///    is false when CW r1→r2 kick `(-1,0)` lands higher (`misdrop_i_tuck_r16_c1_r0`).
/// 8. If no kick fits, rotation is impossible (wiki: “rotation fails completely”).
/// 9. J/L grounded floor: CW r2→r3 test-0 false positive — use CCW-B from `(row,col+1,r0)`
///    when it lands higher on the stack (`misdrop_j_spin_r15_c4_r3` final CW).
fn srs_try_rotate_inner(
    bb: &Bitboard,
    type_idx: usize,
    row: i32,
    col: i32,
    from_rot: usize,
    cw: bool,
    floor_policy: bool,
) -> Option<(i32, i32, usize, usize, (i32, i32))> {
    srs_try_rotate_inner_ex(bb, type_idx, row, col, from_rot, cw, floor_policy, true)
}

fn srs_try_rotate_inner_ex(
    bb: &Bitboard,
    type_idx: usize,
    row: i32,
    col: i32,
    from_rot: usize,
    cw: bool,
    floor_policy: bool,
    allow_jl_alt: bool,
) -> Option<(i32, i32, usize, usize, (i32, i32))> {
    let to_rot = if cw { (from_rot + 1) % 4 } else { (from_rot + 3) % 4 };

    if type_idx == 1 {
        if piece_fits(bb, type_idx, to_rot, row, col) {
            return Some((row, col, to_rot, 0, (0, 0)));
        }
        return None;
    }

    let (base_row, base_col) = srs_basic_rotate_anchor(type_idx, row, col, from_rot, to_rot, cw);
    let table = kicks(type_idx, from_rot, cw);

    let kick_fits = |kx: i32, ky: i32| -> Option<(i32, i32)> {
        let nr = base_row - ky;
        let nc = base_col + kx;
        if nr < BFS_ROW_MIN || nr >= BOARD_ROWS as i32 || nc < 0 || nc >= BOARD_COLS as i32 {
            return None;
        }
        if piece_fits(bb, type_idx, to_rot, nr, nc) {
            Some((nr, nc))
        } else {
            None
        }
    };

    let grounded = piece_grounded(bb, type_idx, from_rot, row, col);
    // Under a lip: basic rot is obstructed even if cells empty; kicks that stay in the
    // cavity are false positives (L floor spins r13/r15). See floor policy §2.
    let from_cavity = floor_policy && under_overhang(bb, type_idx, from_rot, row, col);
    // Grounded floor: basic rotation must not shift the piece downward (ky=0 but nr>row).
    // J spin final CCW at (14,4,r3)→(16,5,r2) is a false positive via this path.
    // Grounded + cavity: test-0 is never a free win (must kick out of the lip).
    let test_0_fits = kick_fits(0, 0).is_some_and(|(nr, nc)| {
        if !floor_policy {
            return true;
        }
        if grounded && nr > row {
            return false;
        }
        // Grounded under a lip: same-row / deeper basic rot is a false free win
        // (L floor spins). Climbing basic rot remains legal (J spin setup CCW).
        if grounded && from_cavity {
            return nr < row;
        }
        // Grounded basic rot that digs minos onto the bottom floor row is a false free
        // spin (L fiction (15,2,r1)). Mid-stack digs that stay above the floor remain
        // legal (J spin setup CW). Cavity climb rule covers higher false spins.
        if grounded
            && piece_max_row(type_idx, to_rot, nr, nc)
                > piece_max_row(type_idx, from_rot, row, col)
            && piece_max_row(type_idx, to_rot, nr, nc) >= BOARD_ROWS as i32 - 1
        {
            return false;
        }
        true
    });

    let up_kick_available = floor_policy
        && table
            .iter()
            .any(|&(kx, ky)| ky > 0 && kick_fits(kx, ky).is_some());
    let wall_locked = col <= 0 || col >= BOARD_COLS as i32 - 2;
    let any_non_down_kick = floor_policy
        && grounded
        && table.iter().any(|&(kx, ky)| {
            !(kx == 0 && ky == 0) && ky >= 0 && kick_fits(kx, ky).is_some()
        });
    // Grounded floor spin/tuck: if basic rotation (test 0) is blocked, only upward
    // kicks are legal. Horizontal test-1 wins like J (14,3,r3) CCW→(16,5,r2) are false
    // positives — real SRS has no valid kick (wiki: rotation fails completely).
    let floor_basic_blocked = floor_policy && grounded && !test_0_fits;

    // S/Z grounded horizontal (r0/r2): first *fitting* kick that changes column is a
    // GB false positive — hardware uses up-one same-col when that pose fits.
    // - misdrop_z_spin_r15_c1_r1: CW@(15,2) → (14,2)
    // - …_20260718-103523: CW@(16,3) → (15,3) not fiction (15,1) [may not be test-0]
    // Same-col climbs (S A/B asymmetry tuck) keep the full kick table.
    if floor_policy
        && grounded
        && (type_idx == 3 || type_idx == 4)
        && (from_rot == 0 || from_rot == 2)
    {
        let nr_up = row - 1;
        let nc_up = col;
        let up_ok = nr_up >= BFS_ROW_MIN && piece_fits(bb, type_idx, to_rot, nr_up, nc_up);
        let mut first_fit: Option<(i32, i32)> = None;
        for &(kx, ky) in table.iter() {
            if floor_policy && grounded && ky < 0 {
                continue; // floor: no downward kicks
            }
            if let Some((nr, nc)) = kick_fits(kx, ky) {
                first_fit = Some((nr, nc));
                break;
            }
        }
        if let Some((nr, nc)) = first_fit {
            if nc != col {
                if up_ok {
                    return Some((nr_up, nc_up, to_rot, 5, (0, 1)));
                }
                if under_overhang(bb, type_idx, from_rot, row, col) && nr >= row {
                    return None;
                }
            }
        }
    }

    // I piece: duplicate horizontal/vertical visuals, four SRS tables — floor CCW r1→r0
    // test-0 false positive; hardware lands like CW r1→r2 kick (-1,0) (map to r0 cells).
    let i_floor = floor_policy && srs_floor_policy(bb, type_idx, from_rot, row, col);
    if i_floor && type_idx == 0 && !cw && from_rot == 1 && to_rot == 0 {
        let (cw_br, cw_bc) = srs_basic_rotate_anchor(type_idx, row, col, 1, 2, true);
        let (nr, nc) = (cw_br, cw_bc - 1);
        if piece_fits(bb, type_idx, 0, nr, nc) {
            let ccw_test0_lower = kick_fits(0, 0).is_some_and(|(ccw_nr, _)| ccw_nr > nr);
            if ccw_test0_lower {
                return Some((nr, nc, 0, 5, (-1, 0)));
            }
        }
    }
    // Mirror: CW r3→r0 test-0 false positive when CCW r3→r2 kick (-2,0) lands higher.
    if i_floor && type_idx == 0 && cw && from_rot == 3 && to_rot == 0 {
        let (ccw_br, ccw_bc) = srs_basic_rotate_anchor(type_idx, row, col, 3, 2, false);
        let (nr, nc) = (ccw_br, ccw_bc - 2);
        if piece_fits(bb, type_idx, 0, nr, nc) {
            let cw_test0_lower = kick_fits(0, 0).is_some_and(|(cw_nr, _)| cw_nr > nr);
            if cw_test0_lower {
                return Some((nr, nc, 0, 5, (-2, 0)));
            }
        }
    }

    // J/L: grounded CW r2→r3 test-0 false positive — hardware lands like CCW-B from
    // (row,col+1,r0), higher on the stack (misdrop_j_spin_r15_c4_r3 final CW).
    if allow_jl_alt
        && floor_policy
        && (type_idx == 5 || type_idx == 6)
        && cw
        && from_rot == 2
        && to_rot == 3
        && grounded
        && srs_floor_policy(bb, type_idx, from_rot, row, col)
    {
        if let Some((test_nr, _)) = kick_fits(0, 0) {
            let alt_col = col + 1;
            if alt_col < BOARD_COLS as i32 {
                if let Some(alt) = srs_try_rotate_inner_ex(
                    bb,
                    type_idx,
                    row,
                    alt_col,
                    0,
                    false,
                    true,
                    false,
                ) {
                    if alt.2 == 3 && alt.1 == alt_col && alt.0 < test_nr {
                        return Some((alt.0, alt.1, 3, 5, (0, 0)));
                    }
                }
            }
        }
    }

    for (ti, &(kx, ky)) in table.iter().enumerate() {
        if floor_policy {
            if ky < 0 {
                continue;
            }
            // Floor + wall: if every wall kick failed, rotation fails — do not fall back
            // to in-place test 0 (L→Z #1 / Tetris Wiki floor J/L example).
            if grounded && wall_locked && !any_non_down_kick && kx == 0 && ky == 0 {
                continue;
            }
            if floor_basic_blocked && ky <= 0 {
                continue;
            }
            // Prefer upward kicks only when test 0 does not fit; if test 0 fits,
            // use it even when an upward kick is also available (J spin setup (15,5,r0)).
            if up_kick_available && !test_0_fits && ky <= 0 {
                continue;
            }
        }
        if let Some((nr, nc)) = kick_fits(kx, ky) {
            if kx == 0 && ky == 0 && test_0_fits {
                // Test 0 fits and is not a grounded-cavity false free win.
                return Some((nr, nc, to_rot, ti, (kx, ky)));
            }
            // Grounded under a lip: reject kicks that do not raise the piece
            // (L (1,1) same-row floor false positive; climbing setup kicks stay legal).
            if floor_policy && grounded && from_cavity && nr >= row {
                continue;
            }
            if floor_policy
                && ky <= 0
                && !test_0_fits
                && under_overhang(bb, type_idx, to_rot, nr, nc)
            {
                continue;
            }
            return Some((nr, nc, to_rot, ti, (kx, ky)));
        }
    }
    None
}

/// Standard guideline SRS — no floor-kick policy (piece high in the well).
pub fn srs_try_rotate(
    bb: &Bitboard,
    type_idx: usize,
    row: i32,
    col: i32,
    from_rot: usize,
    cw: bool,
) -> Option<(i32, i32, usize)> {
    srs_try_rotate_inner(bb, type_idx, row, col, from_rot, cw, false)
        .map(|(r, c, rot, _, _)| (r, c, rot))
}

/// Floor SRS for BFS — applies floor-kick policy (see `srs_try_rotate_inner`).
pub fn srs_try_rotate_grounded(
    bb: &Bitboard,
    type_idx: usize,
    row: i32,
    col: i32,
    from_rot: usize,
    cw: bool,
) -> Option<(i32, i32, usize)> {
    srs_try_rotate_inner(bb, type_idx, row, col, from_rot, cw, true)
        .map(|(r, c, rot, _, _)| (r, c, rot))
}

/// Like `srs_try_rotate` but also returns kick test index (0..4) and SRS offset (kx, ky).
pub fn srs_try_rotate_detailed(
    bb: &Bitboard,
    type_idx: usize,
    row: i32,
    col: i32,
    from_rot: usize,
    cw: bool,
    floor_policy: bool,
) -> Option<(i32, i32, usize, usize, (i32, i32))> {
    srs_try_rotate_inner(bb, type_idx, row, col, from_rot, cw, floor_policy)
}

/// SRS rotation with automatic floor-policy detection for BFS / path simulation.
pub fn srs_try_rotate_auto(
    bb: &Bitboard,
    type_idx: usize,
    row: i32,
    col: i32,
    from_rot: usize,
    cw: bool,
) -> Option<(i32, i32, usize)> {
    let floor = srs_floor_policy(bb, type_idx, from_rot, row, col);
    srs_try_rotate_inner_ex(bb, type_idx, row, col, from_rot, cw, floor, true)
        .map(|(r, c, rot, _, _)| (r, c, rot))
}

fn piece_fits(bb: &Bitboard, type_idx: usize, rot: usize, row: i32, col: i32) -> bool {
    if row < BFS_ROW_MIN || row >= BOARD_ROWS as i32 || col < 0 || col >= BOARD_COLS as i32 {
        return false;
    }
    !super::piece_collides(bb, type_idx, rot, row, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_match_srs_cells() {
        for type_idx in 0..7 {
            for rot in 0..4 {
                let (mor, moc) = matrix_origin_offset(type_idx, rot);
                let srs = &SRS_CELLS[type_idx][rot];
                let shape = &SHAPES[type_idx][rot];
                for (i, &[dr, dc]) in shape.iter().enumerate() {
                    let sr = srs[i].0 + mor;
                    let sc = srs[i].1 + moc;
                    assert_eq!(
                        (sr, sc),
                        (dr as i8, dc as i8),
                        "shape mismatch type={} rot={} cell={}",
                        type_idx,
                        rot,
                        i
                    );
                }
            }
        }
    }

    #[test]
    fn i_rotation_states_are_distinct_in_srs_matrix() {
        let i = &SRS_CELLS[0];
        assert_eq!(i[0], [(1, 0), (1, 1), (1, 2), (1, 3)]);
        assert_eq!(i[2], [(2, 0), (2, 1), (2, 2), (2, 3)]);
        assert_eq!(i[1], [(0, 0), (1, 0), (2, 0), (3, 0)]);
        assert_eq!(i[3], [(0, 2), (1, 2), (2, 2), (3, 2)]);
        assert_ne!(i[0], i[2]);
        assert_ne!(i[1], i[3]);
    }

    #[test]
    fn i_cw_twice_changes_rot_not_just_visual() {
        let bb = [0u16; BOARD_ROWS];
        // Mid-board horizontal I (spawn row 0 is too high for 0->R without floor).
        let (r1, c1, rot1) = srs_try_rotate(&bb, 0, 8, 4, 0, true).expect("0->R");
        assert_eq!(rot1, 1);
        let (r2, c2, rot2) = srs_try_rotate(&bb, 0, r1, c1, rot1, true).expect("R->2");
        assert_eq!(rot2, 2, "second CW must reach state 2, not back to 0");
        assert_ne!((r1, c1), (r2, c2), "R->2 anchor should differ from R position");
    }

    #[test]
    fn t_spawn_cw_finds_valid_kick() {
        let bb = [0u16; BOARD_ROWS];
        let result = srs_try_rotate(&bb, 2, 0, 3, 0, true);
        assert!(result.is_some());
        let (r, c, rot) = result.unwrap();
        assert_eq!(rot, 1);
        assert!(!crate::bot::piece_collides(&bb, 2, rot, r, c));
    }

    #[test]
    fn o_rotate_stays_put() {
        let bb = [0u16; BOARD_ROWS];
        assert_eq!(srs_try_rotate(&bb, 1, 5, 4, 0, true), Some((5, 4, 1)));
    }

    /// J→L #1 board from replay savestate (read_board_bitboard +2 stride).
    fn j_to_l_board() -> [u16; BOARD_ROWS] {
        let mut bb = [0u16; BOARD_ROWS];
        bb[14] = 24;
        bb[15] = 253;
        bb[16] = 127;
        bb[17] = 255;
        bb
    }

    fn path_rot_count(path: &[String]) -> usize {
        path.iter().filter(|a| *a == "CW" || *a == "CCW").count()
    }

    fn path_has_opposite_rotation_pair(path: &[String]) -> bool {
        for w in path.windows(2) {
            if matches!((w[0].as_str(), w[1].as_str()), ("CW", "CCW") | ("CCW", "CW")) {
                return true;
            }
        }
        false
    }

    fn z_to_i_board() -> [u16; BOARD_ROWS] {
        let mut bb = [0u16; BOARD_ROWS];
        bb[14] = 7;
        bb[15] = 899;
        bb[16] = 999;
        bb
    }

    #[test]
    fn z_to_i_replay_analysis() {
        let bb = z_to_i_board();
        let moves = crate::bot::bfs_moves(&bb, 4, 0, 3, 3); // Z rot L at col 3
        let target = moves.iter().find(|m| m.col == 2 && m.rot == 0);
        eprintln!("Z locks col2 rot0: {:?}", target.map(|m| (&m.path, m.row)));
        if let Some(m) = target {
            assert!(!path_has_opposite_rotation_pair(&m.path));
        }
        // Simulate recorded path
        let mut r = 0i32;
        let mut c = 3i32;
        let mut rot = 3usize;
        for act in ["D","D","D","D","D","D","D","D","D","D","CW"] {
            match act {
                "D" => r += 1,
                "CW" => {
                    let got = srs_try_rotate(&bb, 4, r, c, rot, true);
                    eprintln!("CW at ({r},{c},r{rot}) => {got:?}");
                    if let Some((nr,nc,nr2)) = got { r=nr;c=nc;rot=nr2; }
                }
                _ => {}
            }
        }
        eprintln!("model after path: ({r},{c},r{rot}) wanted (?,2,r0)");
    }

    #[test]
    fn j_to_l_planner_why_rotations() {
        use crate::bot::{
            bfs_moves, find_best_move_with_bfs, meatfighter_evaluate, simulate_place_and_clear,
            column_heights, BOARD_COLS,
        };

        let bb = j_to_l_board();
        let type_j = 6usize;
        let type_l = 5usize;
        let spawn_col = 4usize;
        let spawn_rot = 1usize;

        let moves1 = bfs_moves(&bb, type_j, 0, spawn_col, spawn_rot);
        for m in &moves1 {
            assert!(
                !path_has_opposite_rotation_pair(&m.path),
                "CW/CCW pair in path to ({},{},r{}): {:?}",
                m.row,
                m.col,
                m.rot,
                m.path
            );
        }

        eprintln!("\n=== J→L planner dump ===");
        eprintln!("BFS locked states for J @ spawn (0,{spawn_col},r{spawn_rot}): {}", moves1.len());

        // Score every J placement with 2-ply (next = L)
        let mut ranked: Vec<(i32, i32, usize, usize, i32, Vec<String>)> = Vec::new();
        for m in &moves1 {
            let (bb1, clears1, lock_h1) =
                simulate_place_and_clear(&bb, type_j, m.rot, m.col as usize, m.row);
            let moves2 = bfs_moves(&bb1, type_l, 0, 3, 0);
            let mut best_inner = i32::MIN;
            for m2 in &moves2 {
                let (bb2, clears2, lock_h2) =
                    simulate_place_and_clear(&bb1, type_l, m2.rot, m2.col as usize, m2.row);
                let h2 = column_heights(&bb2);
                let sc = meatfighter_evaluate(&bb2, &h2, clears1 + clears2, lock_h1 + lock_h2);
                if sc > best_inner {
                    best_inner = sc;
                }
            }
            ranked.push((
                best_inner,
                m.row,
                m.col as usize,
                m.rot,
                path_rot_count(&m.path) as i32,
                m.path.clone(),
            ));
        }
        ranked.sort_by(|a, b| b.0.cmp(&a.0));

        eprintln!("\nTop 8 placements by 2-ply score (J then L):");
        for (i, (sc, row, col, rot, rots, path)) in ranked.iter().take(8).enumerate() {
            eprintln!(
                "  #{i} score={sc} lock=({row},{col},r{rot}) rots={rots} len={} path={path:?}",
                path.len()
            );
        }

        // All ways to land J at col 7 rot 2 (any row)
        let c7r2: Vec<_> = moves1
            .iter()
            .filter(|m| m.col == 7 && m.rot == 2)
            .collect();
        eprintln!("\nLocked states with col=7 rot=2: {}", c7r2.len());
        for m in &c7r2 {
            eprintln!(
                "  row={} rots={} len={} path={:?}",
                m.row,
                path_rot_count(&m.path),
                m.path.len(),
                m.path
            );
        }

        // Simplest path to col 7 rot 2 (min rotations, then min length)
        if let Some(best_simple) = c7r2
            .iter()
            .min_by_key(|m| (path_rot_count(&m.path), m.path.len()))
        {
            let (bb1, c1, h1) = simulate_place_and_clear(
                &bb,
                type_j,
                best_simple.rot,
                best_simple.col as usize,
                best_simple.row,
            );
            let moves2 = bfs_moves(&bb1, type_l, 0, 3, 0);
            let mut best_inner = i32::MIN;
            for m2 in &moves2 {
                let (bb2, c2, h2) =
                    simulate_place_and_clear(&bb1, type_l, m2.rot, m2.col as usize, m2.row);
                let sc = meatfighter_evaluate(&bb2, &column_heights(&bb2), c1 + c2, h1 + h2);
                if sc > best_inner {
                    best_inner = sc;
                }
            }
            eprintln!(
                "\nSimplest col7 rot2: row={} rots={} score={best_inner} path={:?}",
                best_simple.row,
                path_rot_count(&best_simple.path),
                best_simple.path
            );
        }

        let winner_score = ranked.first().map(|r| r.0).unwrap_or(0);
        let c7_best = ranked
            .iter()
            .filter(|r| r.2 == 7 && r.3 == 2)
            .map(|r| r.0)
            .max()
            .unwrap_or(i32::MIN);
        eprintln!(
            "\nScore winner={winner_score} vs best col7rot2={c7_best} (delta={})",
            winner_score - c7_best
        );

        // Try spawn variants (replay had col 3 row 1 on restore, meta says col 4 rot 1)
        // Can we reach col7 rot2 with a naive path (no mid-air rotation zigzag)?
        let naive = ["D"; 13]
            .into_iter()
            .map(String::from)
            .chain(["R", "R", "R", "CW"].iter().map(|s| s.to_string()))
            .collect::<Vec<_>>();
        let mut r = 0i32;
        let mut c = 4i32;
        let mut rot = 1usize;
        let mut ok = true;
        for act in &naive {
            match act.as_str() {
                "D" => r += 1,
                "R" => c += 1,
                "CW" => {
                    if let Some((nr, nc, nrot)) = srs_try_rotate(&bb, type_j, r, c, rot, true) {
                        r = nr;
                        c = nc;
                        rot = nrot;
                    } else {
                        ok = false;
                        break;
                    }
                }
                _ => {}
            }
        }
        eprintln!("\nNaive D×13 R×3 CW from (0,4,r1): ok={ok} end=({r},{c},r{rot})");

        // 1-ply replan from mid-path positions (like after deviation)
        for (sr, sc, srot) in [(3, 5, 1), (5, 6, 1), (8, 4, 1), (10, 5, 0)] {
            let mv = bfs_moves(&bb, type_j, sr, sc, srot);
            if let Some(m) = mv.iter().filter(|m| m.col == 7 && m.rot == 2).next() {
                eprintln!(
                    "1-ply from ({sr},{sc},r{srot}) has col7rot2: rots={} path={:?}",
                    path_rot_count(&m.path),
                    m.path
                );
            }
        }

        // Exact replay spawn: row 1 col 3 rot 1 (piece already fell 1 row when plan() runs)
        {
            let sr = 1i32;
            let sc = 3usize;
            let srot = 1usize;
            let mv = bfs_moves(&bb, type_j, sr, sc, srot);
            let mut ranked2: Vec<(i32, i32, usize, usize, Vec<String>)> = Vec::new();
            for m in &mv {
                let (bb1, c1, h1) =
                    simulate_place_and_clear(&bb, type_j, m.rot, m.col as usize, m.row);
                let mv2 = bfs_moves(&bb1, type_l, 0, 3, 0);
                let mut inner = i32::MIN;
                for m2 in &mv2 {
                    let (bb2, c2, h2) =
                        simulate_place_and_clear(&bb1, type_l, m2.rot, m2.col as usize, m2.row);
                    inner = inner.max(meatfighter_evaluate(
                        &bb2,
                        &column_heights(&bb2),
                        c1 + c2,
                        h1 + h2,
                    ));
                }
                ranked2.push((inner, m.row, m.col as usize, m.rot, m.path.clone()));
            }
            ranked2.sort_by(|a, b| b.0.cmp(&a.0));
            eprintln!("\n=== spawn(1,3,r1) exact replay top 5 ===");
            for (i, (sc2, row, col, rot, path)) in ranked2.iter().take(5).enumerate() {
                eprintln!(
                    "  #{i} score={sc2} ({row},{col},r{rot}) rots={} path={path:?}",
                    path_rot_count(path)
                );
            }
            let c7 = ranked2.iter().find(|r| r.2 == 7 && r.3 == 2);
            let w = ranked2.first();
            eprintln!("winner vs col7rot2: {:?} vs {:?}", w.map(|x| (x.0, x.2, x.3)), c7.map(|x| (x.0, x.2, x.3, path_rot_count(&x.4))));
        }

        for (sr, sc, srot) in [(0, 3, 1), (1, 3, 1), (0, 3, 0), (0, 4, 0)] {
            let mv = bfs_moves(&bb, type_j, sr, sc, srot);
            let mut best: Option<(i32, i32, usize, usize, Vec<String>)> = None;
            for m in &mv {
                let (bb1, c1, h1) =
                    simulate_place_and_clear(&bb, type_j, m.rot, m.col as usize, m.row);
                let mv2 = bfs_moves(&bb1, type_l, 0, 3, 0);
                let mut inner = i32::MIN;
                for m2 in &mv2 {
                    let (bb2, c2, h2) =
                        simulate_place_and_clear(&bb1, type_l, m2.rot, m2.col as usize, m2.row);
                    inner = inner.max(meatfighter_evaluate(
                        &bb2,
                        &column_heights(&bb2),
                        c1 + c2,
                        h1 + h2,
                    ));
                }
                if best.as_ref().map(|b| inner > b.0).unwrap_or(true) {
                    best = Some((inner, m.row, m.col as usize, m.rot, m.path.clone()));
                }
            }
            if let Some((sc2, row, col, rot, path)) = best {
                eprintln!(
                    "spawn({sr},{sc},r{srot}) -> BEST ({row},{col},r{rot}) score={sc2} rots={} path={path:?}",
                    path_rot_count(&path)
                );
            }
        }
    }

    #[test]
    #[ignore = "awaiting fresh fixture capture"]
    fn j_to_t_misdrop_replay_analysis() {
        use crate::bot::{
            bfs_moves, classify_move, column_heights, meatfighter_evaluate,
            piece_collides, simulate_place_and_clear, BOARD_COLS, BOARD_ROWS,
        };
        use crate::state::EmulatorState;
        use base64::Engine;

        let b64 = std::fs::read_to_string(
            crate::bot::fixtures::misdrop_fixture("j_to_t_replay_state.b64"),
        )
        .expect("j_to_t_replay_state.b64 from browser export");
        let b64 = b64.trim().trim_matches('"');
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .expect("base64 decode");
        let state: EmulatorState = bincode::deserialize(&bytes).expect("bincode state");

        eprintln!("savestate ram len={}", state.ram.len());
        let slice = &state.ram[0x800..0x800 + 18 * 16];
        let nz: usize = slice.iter().filter(|&&b| b != 0).count();
        eprintln!("board slice nonzero bytes: {nz}");
        for row in 14..18 {
            let base = 0x800 + row * 32 + 2;
            let bytes: Vec<u8> = (0..10)
                .map(|c| state.ram.get(base + c).copied().unwrap_or(0))
                .collect();
            eprintln!("row {row} raw: {bytes:?}");
        }
        let bb = bitboard_from_savestate_ram(&state.ram);
        let nonempty: Vec<_> = bb
            .iter()
            .enumerate()
            .filter(|(_, r)| **r != 0)
            .map(|(i, r)| (i, *r))
            .collect();
        eprintln!("J→T board nonempty rows: {nonempty:?}");

        let type_j = 6usize;
        let spawn_col = 3usize;
        let spawn_rot = 0usize;
        let moves = bfs_moves(&bb, type_j, 0, spawn_col, spawn_rot);

        let target = moves
            .iter()
            .find(|m| m.col == 4 && m.rot == 1 && m.row == 14);
        if let Some(m) = target {
            eprintln!("BFS has want target (14,4,r1): path={:?}", m.path);
            let mtype = classify_move(&bb, type_j, m.row, m.col, m.rot, &m.path, 0);
            eprintln!("classify_move => {mtype}");
            simulate_path(&bb, type_j, 0, spawn_col as i32, spawn_rot, &m.path);
        } else {
            eprintln!("BFS has NO lock at (14,4,r1) among {} moves", moves.len());
            for m in moves.iter().filter(|m| m.col == 4 && m.rot == 1) {
                eprintln!("  col4 rot1 row={} path={:?}", m.row, m.path);
            }
        }

        // 2-ply winner (next T = type 2)
        let mut ranked: Vec<(i32, i32, usize, usize, Vec<String>, String)> = Vec::new();
        for m in &moves {
            let (bb1, c1, h1) =
                simulate_place_and_clear(&bb, type_j, m.rot, m.col as usize, m.row);
            let mv2 = bfs_moves(&bb1, 2, 0, 3, 0);
            let mut inner = i32::MIN;
            for m2 in &mv2 {
                let (bb2, c2, h2) =
                    simulate_place_and_clear(&bb1, 2, m2.rot, m2.col as usize, m2.row);
                inner = inner.max(meatfighter_evaluate(
                    &bb2,
                    &column_heights(&bb2),
                    c1 + c2,
                    h1 + h2,
                ));
            }
            let mtype = classify_move(&bb, type_j, m.row, m.col, m.rot, &m.path, 0);
            ranked.push((inner, m.row, m.col as usize, m.rot, m.path.clone(), mtype.to_string()));
        }
        ranked.sort_by(|a, b| b.0.cmp(&a.0));
        eprintln!("2-ply top 5 J placements (next T):");
        for (i, (sc, row, col, rot, path, mtype)) in ranked.iter().take(5).enumerate() {
            eprintln!("  #{i} score={sc} ({row},{col},r{rot}) type={mtype} path={path:?}");
        }

        // Recorded misdrop path cannot reach row 14 from spawn
        let recorded = ["D", "CCW", "CCW", "CCW", "D"]
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        let end = simulate_path(&bb, type_j, 0, spawn_col as i32, spawn_rot, &recorded);
        eprintln!("recorded path end {end:?} vs want (14,4,r1) (None = floor SRS rejects step)");

        // 1-ply from got position (13,4,r3) — likely replan source
        let got_moves = bfs_moves(&bb, type_j, 13, 4, 3);
        if let Some(m) = got_moves.iter().find(|m| m.path == recorded) {
            eprintln!("1-ply from got (13,4,r3) reproduces recorded path → lock ({},{},r{})", m.row, m.col, m.rot);
        } else {
            eprintln!("1-ply from (13,4,r3): {} moves, shortest:", got_moves.len());
            if let Some(m) = got_moves.iter().min_by_key(|m| m.path.len()) {
                eprintln!("  ({},{},r{}) path={:?}", m.row, m.col, m.rot, m.path);
            }
        }

        // 2-ply winner (same logic as find_best_move_with_bfs)
        let mut best_score = i32::MIN;
        let mut best: Option<(usize, usize, Vec<String>, i32, String)> = None;
        for m in &moves {
            let (bb1, c1, h1) =
                simulate_place_and_clear(&bb, type_j, m.rot, m.col as usize, m.row);
            let mv2 = bfs_moves(&bb1, 2, 0, 3, 0);
            let mut inner = i32::MIN;
            for m2 in &mv2 {
                let (bb2, c2, h2) =
                    simulate_place_and_clear(&bb1, 2, m2.rot, m2.col as usize, m2.row);
                inner = inner.max(meatfighter_evaluate(
                    &bb2,
                    &column_heights(&bb2),
                    c1 + c2,
                    h1 + h2,
                ));
            }
            let mtype = classify_move(&bb, type_j, m.row, m.col, m.rot, &m.path, 0);
            if inner > best_score {
                best_score = inner;
                best = Some((m.rot, m.col as usize, m.path.clone(), m.row, mtype.to_string()));
            }
        }
        if let Some((rot, col, path, row, mtype)) = &best {
            eprintln!("2-ply WINNER (find_best equiv): ({row},{col},r{rot}) type={mtype} score={best_score} path={path:?}");
            let end = simulate_path(&bb, type_j, 0, spawn_col as i32, spawn_rot, path);
            eprintln!("winner sim end {end:?}");
        }

        // Is (14,4,r1) a spin per classify_move? (last action must be rot for spin)
        if let Some(m) = target {
            let last = m.path.last().map(|s| s.as_str());
            eprintln!("(14,4,r1) last action = {last:?} => classify_move normal (not spin)");
            // Verify lock cells vs board
            use crate::bot::SHAPES;
            let fits = !piece_collides(&bb, type_j, 1, 14, 4);
            let blocked_below = piece_collides(&bb, type_j, 1, 15, 4);
            eprintln!("(14,4,r1) fits={fits} blocked_below={blocked_below}");
            for &[dr, dc] in &SHAPES[type_j][1] {
                let r = 14 + dr as i32;
                let c = 4 + dc as i32;
                let occ = if r >= 0 && r < BOARD_ROWS as i32 && c >= 0 && c < BOARD_COLS as i32 {
                    (bb[r as usize] >> c) & 1 == 1
                } else {
                    false
                };
                eprintln!("  cell ({r},{c}) occupied={occ}");
            }
        }

        // Compare spawn col 4 variant (late spawn read?)
        let moves_c4 = bfs_moves(&bb, type_j, 0, 4, 0);
        if let Some(m) = moves_c4.iter().find(|m| m.col == 4 && m.rot == 1 && m.row == 14) {
            eprintln!("from spawn col4: (14,4,r1) path={:?}", m.path);
        }

        // J flat-side spawn might be rot 1 on this ROM?
        for spawn_rot in [0usize, 1] {
            let mv = bfs_moves(&bb, type_j, 0, spawn_col, spawn_rot);
            let mut best_score = i32::MIN;
            let mut best_line = String::new();
            for m in &mv {
                let (bb1, c1, h1) =
                    simulate_place_and_clear(&bb, type_j, m.rot, m.col as usize, m.row);
                let mv2 = bfs_moves(&bb1, 2, 0, 3, 0);
                let mut inner = i32::MIN;
                for m2 in &mv2 {
                    let (bb2, c2, h2) =
                        simulate_place_and_clear(&bb1, 2, m2.rot, m2.col as usize, m2.row);
                    inner = inner.max(meatfighter_evaluate(
                        &bb2,
                        &column_heights(&bb2),
                        c1 + c2,
                        h1 + h2,
                    ));
                }
                if inner > best_score {
                    best_score = inner;
                    let mt = classify_move(&bb, type_j, m.row, m.col, m.rot, &m.path, 0);
                    best_line = format!(
                        "spawn_rot={spawn_rot} → ({},{},r{}) type={mt} score={inner}",
                        m.row, m.col, m.rot
                    );
                }
            }
            eprintln!("{best_line}");
        }

        // Analyze 2-ply winner spin (14,3,r3) — true spawn plan on this board
        if let Some(m) = moves.iter().find(|m| m.col == 3 && m.rot == 3 && m.row == 14) {
            eprintln!("(14,3,r3) path={:?}", m.path);
            eprintln!("(14,3,r3) classify={}", classify_move(&bb, type_j, m.row, m.col, m.rot, &m.path, 0));
            let end = simulate_path(&bb, type_j, 0, spawn_col as i32, 0, &m.path);
            eprintln!("(14,3,r3) sim end {end:?}");
            // spin check: grounded + last rot?
            let last = m.path.last().map(|s| s.as_str()).unwrap_or("");
            let grounded_spin = last == "CW" || last == "CCW";
            let cant_up = piece_collides(&bb, type_j, 3, 13, 3);
            eprintln!("(14,3,r3) last={last} cant_up_before_lock={cant_up} (spin needs rot last + blocked above)");
        }

        // 1-ply at spawn — does it pick (14,4,r1)?
        let one_ply = bfs_moves(&bb, type_j, 0, spawn_col, 0);
        let mut best1 = i32::MIN;
        let mut best1m = None;
        for m in &one_ply {
            let (bb1, c1, h1) =
                simulate_place_and_clear(&bb, type_j, m.rot, m.col as usize, m.row);
            let sc = meatfighter_evaluate(&bb1, &column_heights(&bb1), c1, h1);
            if sc > best1 {
                best1 = sc;
                best1m = Some(m);
            }
        }
        if let Some(m) = best1m {
            let mt = classify_move(&bb, type_j, m.row, m.col, m.rot, &m.path, 0);
            eprintln!("1-ply at spawn: ({},{},r{}) type={mt} path={:?}", m.row, m.col, m.rot, m.path);
        }

        // Does any 1-ply state match misdrop want (14,4,r1)?
        for m in &one_ply {
            if m.col == 4 && m.rot == 1 && m.row == 14 {
                eprintln!("1-ply lists (14,4,r1) type={}", classify_move(&bb, type_j, m.row, m.col, m.rot, &m.path, 0));
            }
        }

        // 2-ply if spawn read was col 4 (off-by-one spawn capture?)
        let mv4 = bfs_moves(&bb, type_j, 0, 4, 0);
        let mut bs = i32::MIN;
        let mut bm = None;
        for m in &mv4 {
            let (bb1, c1, h1) =
                simulate_place_and_clear(&bb, type_j, m.rot, m.col as usize, m.row);
            let mv2 = bfs_moves(&bb1, 2, 0, 3, 0);
            let mut inner = i32::MIN;
            for m2 in &mv2 {
                let (bb2, c2, h2) =
                    simulate_place_and_clear(&bb1, 2, m2.rot, m2.col as usize, m2.row);
                inner = inner.max(meatfighter_evaluate(
                    &bb2,
                    &column_heights(&bb2),
                    c1 + c2,
                    h1 + h2,
                ));
            }
            if inner > bs {
                bs = inner;
                bm = Some(m);
            }
        }
        if let Some(m) = bm {
            let mt = classify_move(&bb, type_j, m.row, m.col, m.rot, &m.path, 0);
            eprintln!("2-ply from spawn col4: ({},{},r{}) type={mt}", m.row, m.col, m.rot);
        }

        // Trace (14,4,r1) path — are late CCWs grounded (game may forbid)?
        if let Some(m) = target {
            let mut r = 0i32;
            let mut c = spawn_col as i32;
            let mut rot = 0usize;
            for (i, act) in m.path.iter().enumerate() {
                let can_down = !piece_collides(&bb, type_j, rot, r + 1, c);
                eprintln!("  (14,4,r1) step {i} pre=({r},{c},r{rot}) grounded={}", !can_down);
                match act.as_str() {
                    "D" => r += 1,
                    "CW" => {
                        if let Some((nr, nc, nrot)) = srs_try_rotate(&bb, type_j, r, c, rot, true) {
                            r = nr;
                            c = nc;
                            rot = nrot;
                        }
                    }
                    "CCW" => {
                        if let Some((nr, nc, nrot)) = srs_try_rotate(&bb, type_j, r, c, rot, false) {
                            r = nr;
                            c = nc;
                            rot = nrot;
                        }
                    }
                    _ => {}
                }
            }
        }

        // Rosy: Down locks immediately when grounded — flag D-steps after grounded states.
        if let Some(m) = moves.iter().find(|m| m.col == 3 && m.rot == 3 && m.row == 14) {
            let mut r = 0i32;
            let mut c = spawn_col as i32;
            let mut rot = 0usize;
            for (i, act) in m.path.iter().enumerate() {
                let grounded = piece_collides(&bb, type_j, rot, r + 1, c);
                let rosy_down_invalid = grounded && act == "D";
                eprintln!(
                    "  spin trace step {i}: pre=({r},{c},r{rot}) grounded={grounded} act={act} rosy_D_invalid={rosy_down_invalid}"
                );
                match act.as_str() {
                    "D" => r += 1,
                    "L" => c -= 1,
                    "R" => c += 1,
                    "CW" => {
                        if let Some((nr, nc, nrot)) = srs_try_rotate(&bb, type_j, r, c, rot, true) {
                            r = nr;
                            c = nc;
                            rot = nrot;
                        }
                    }
                    "CCW" => {
                        if let Some((nr, nc, nrot)) = srs_try_rotate(&bb, type_j, r, c, rot, false) {
                            r = nr;
                            c = nc;
                            rot = nrot;
                        }
                    }
                    _ => {}
                }
            }
        }

        // Which SRS kick test wins on grounded CWs? ky>0 => kick UP (row decreases).
        for &(r, c, rot, label) in &[(12i32, 3, 1usize, "CW#1 grounded"), (14, 2, 2, "CW#2 grounded")] {
            let to_rot = (rot + 1) % 4;
            let (br, bc) = srs_basic_rotate_anchor(type_j, r, c, rot, to_rot, true);
            eprintln!("  {label} at ({r},{c},r{rot}) basic_anchor=({br},{bc},r{to_rot})");
            for (ti, &(kx, ky)) in kicks(type_j, rot, true).iter().enumerate() {
                let nr = br - ky;
                let nc = bc + kx;
                let fits = nr >= 0
                    && nr < BOARD_ROWS as i32
                    && nc >= 0
                    && nc < BOARD_COLS as i32
                    && piece_fits(&bb, type_j, to_rot, nr, nc);
                eprintln!("    test {ti} (kx={kx},ky={ky}) → ({nr},{nc}) fits={fits}");
            }
            if let Some((nr, nc, nrot, ti, (kx, ky))) =
                srs_try_rotate_detailed(&bb, type_j, r, c, rot, true, true)
            {
                let row_delta = nr - r;
                eprintln!(
                    "  => winner test {ti} (kx={kx},ky={ky}) → ({nr},{nc},r{nrot}) row_delta={row_delta}"
                );
            }
        }
        // If test 0 is wrong and UP kick (test 3) wins at CW#1 → (11,2,r2), can BFS still reach (14,3,r3)?
        let up_kick_state = (11i32, 2, 2usize);
        let from_up = bfs_moves(&bb, type_j, up_kick_state.0, up_kick_state.1 as usize, up_kick_state.2);
        let reaches = from_up.iter().any(|m| m.row == 14 && m.col == 3 && m.rot == 3);
        eprintln!(
            "  from UP-kick state (11,2,r2): {} locks, reaches (14,3,r3)={reaches}",
            from_up.len()
        );
        if let Some(m) = from_up.iter().find(|m| m.row == 14 && m.col == 3 && m.rot == 3) {
            eprintln!("  UP-kick → (14,3,r3) path: {:?}", m.path);
        }
        // Print J cells at contested positions
        use crate::bot::SHAPES;
        for &(r, c, rot, lbl) in &[(12, 3, 1, "pre-CW#1"), (13, 2, 2, "test0"), (11, 2, 2, "test3-UP")] {
            eprint!("  {lbl} J r{rot} at ({r},{c}) cells:");
            for &[dr, dc] in &SHAPES[type_j][rot] {
                let cr = r + dr as i32;
                let cc = c + dc as i32;
                let occ = if cr >= 0 && cr < BOARD_ROWS as i32 && cc >= 0 && cc < BOARD_COLS as i32 {
                    (bb[cr as usize] >> cc) & 1 == 1
                } else {
                    false
                };
                eprint!(" ({cr},{cc}){}", if occ { "#" } else { "" });
            }
            eprintln!();
        }

        // Board profile cols 2-5 rows 12-17
        for row in 12..=17 {
            let bits = bb[row];
            let cols: String = (0..10)
                .map(|c| if (bits >> c) & 1 == 1 { '#' } else { '.' })
                .collect();
            eprintln!("  board row {row}: {cols}");
        }

        // Why is (14,3,r3) spin possibly impossible? Check final CW SRS at lock approach.
        if let Some(m) = moves.iter().find(|m| m.col == 3 && m.rot == 3 && m.row == 14) {
            let mut r = 0i32;
            let mut c = spawn_col as i32;
            let mut rot = 0usize;
            for (i, act) in m.path.iter().enumerate() {
                match act.as_str() {
                    "D" => r += 1,
                    "L" => c -= 1,
                    "R" => c += 1,
                    "CW" => {
                        let got = srs_try_rotate(&bb, type_j, r, c, rot, true);
                        eprintln!("  step {i} CW at ({r},{c},r{rot}) => {got:?}");
                        if let Some((nr, nc, nrot)) = got {
                            r = nr;
                            c = nc;
                            rot = nrot;
                        }
                    }
                    "CCW" => {
                        let got = srs_try_rotate(&bb, type_j, r, c, rot, false);
                        eprintln!("  step {i} CCW at ({r},{c},r{rot}) => {got:?}");
                        if let Some((nr, nc, nrot)) = got {
                            r = nr;
                            c = nc;
                            rot = nrot;
                        }
                    }
                    _ => {}
                }
            }
        }

        // Floor policy: J spin (14,3,r3) must not be in BFS — 2→L test 0 lands under lip,
        // no upward kick in that table, rotation fails (wiki: all 5 tests fail).
        assert!(
            !moves.iter().any(|m| m.col == 3 && m.rot == 3 && m.row == 14),
            "false-positive J spin (14,3,r3) must be rejected"
        );
        assert_eq!(
            srs_try_rotate_grounded(&bb, type_j, 14, 2, 2, true),
            None,
            "2→L at (14,2,r2): test 0 under overhang, no UP kick → rotation impossible"
        );
        // R→2 at (12,3,r1) on floor: SRS table test 4 = (0,+2) lifts to (11,2,r2).
        assert_eq!(
            srs_try_rotate_grounded(&bb, type_j, 12, 3, 1, true),
            Some((11, 2, 2)),
            "R→2 floor: SRS test 4 (0,+2) lifts piece above overhang"
        );
        // Airborne rotation unchanged — first CW in path still (11,3,r1).
        assert_eq!(
            srs_try_rotate(&bb, type_j, 11, 4, 0, true),
            Some((11, 3, 1))
        );
    }

    fn bitboard_from_savestate_ram(ram: &[u8]) -> [u16; BOARD_ROWS] {
        let mut bb = [0u16; BOARD_ROWS];
        for row in 0..BOARD_ROWS {
            let base = 0x800 + row * 32 + 2;
            let mut bits = 0u16;
            for col in 0..BOARD_COLS {
                let v = ram.get(base + col).copied().unwrap_or(0);
                if (v & 0x80) != 0 && v != 0x8E {
                    bits |= 1 << col;
                }
            }
            bb[row] = bits;
        }
        bb
    }

    fn simulate_path(
        bb: &[u16; BOARD_ROWS],
        type_idx: usize,
        start_row: i32,
        start_col: i32,
        start_rot: usize,
        path: &[String],
    ) -> Option<(i32, i32, usize)> {
        let mut r = start_row;
        let mut c = start_col;
        let mut rot = start_rot;
        for act in path {
            match act.as_str() {
                "D" => r += 1,
                "L" => c -= 1,
                "R" => c += 1,
                "CW" => {
                    let (nr, nc, nrot) = srs_try_rotate_auto(bb, type_idx, r, c, rot, true)?;
                    r = nr;
                    c = nc;
                    rot = nrot;
                }
                "CCW" => {
                    let (nr, nc, nrot) = srs_try_rotate_auto(bb, type_idx, r, c, rot, false)?;
                    r = nr;
                    c = nc;
                    rot = nrot;
                }
                _ => {}
            }
        }
        Some((r, c, rot))
    }

    #[test]
    fn j_to_l_replay_analysis() {
        // J→L #1 misdrop savestate (rows 14-17 from emu).
        let mut bb = [0u16; BOARD_ROWS];
        bb[14] = 0b0001100000;
        bb[15] = 0b1111110100;
        bb[16] = 0b0111111100;
        bb[17] = 0b1111111100;

        let moves = crate::bot::bfs_moves(&bb, 6, 0, 4, 1);
        let target = moves.iter().find(|m| m.col == 7 && m.rot == 2);
        if let Some(m) = target {
            eprintln!("J→L BFS col7 rot2 path: {:?}", m.path);
            let last = m.path.iter().rev().find(|a| *a != "D").map(|s| s.as_str());
            eprintln!("last_non_d={last:?} row={} col={}", m.row, m.col);
        } else {
            eprintln!("J→L: no col7 rot2 in BFS ({} moves)", moves.len());
        }
        // Recorded path final CW from emu step 13: (6,4,r1)
        let cw = srs_try_rotate(&bb, 6, 6, 4, 1, true);
        eprintln!("emu pre-final CW (6,4,r1) => model {cw:?}, emu landed (7,3,r2)");
        assert_eq!(cw, Some((7, 3, 2)));

        // Simulate BFS winning path to col7 rot2 step-by-step
        if let Some(m) = target {
            let mut r: i32 = 0;
            let mut c: i32 = 4;
            let mut rot: usize = 1;
            for (i, act) in m.path.iter().enumerate() {
                match act.as_str() {
                    "D" => r += 1,
                    "L" => c -= 1,
                    "R" => c += 1,
                    "CW" => {
                        let Some((nr, nc, nrot)) = srs_try_rotate_auto(&bb, 6, r, c, rot, true) else {
                            panic!("step {i} CW fail at ({r},{c},r{rot})");
                        };
                        r = nr; c = nc; rot = nrot;
                    }
                    "CCW" => {
                        let Some((nr, nc, nrot)) = srs_try_rotate_auto(&bb, 6, r, c, rot, false) else {
                            panic!("step {i} CCW fail at ({r},{c},r{rot})");
                        };
                        r = nr; c = nc; rot = nrot;
                    }
                    _ => {}
                }
            }
            eprintln!("sim end ({r},{c},r{rot}) target row={} col={} rot={}", m.row, m.col, m.rot);
            assert_eq!((r, c, rot), (m.row, m.col, m.rot));
        }
    }

    #[test]
    fn i_to_j_misdrop_spin_analysis() {
        use crate::bot::{
            bfs_moves, classify_move, column_heights, meatfighter_evaluate,
            simulate_place_and_clear,
        };

        // I→J #1 replay board (rows 15-17)
        let mut bb = [0u16; BOARD_ROWS];
        bb[15] = 391;
        bb[16] = 963;
        bb[17] = 967;

        let moves = bfs_moves(&bb, 0, 0, 3, 0);
        for m in moves.iter().filter(|m| m.col == 2 && m.rot == 0) {
            eprintln!("I col2 rot0 lock row={} path={:?}", m.row, m.path);
        }
        let spin_paths: Vec<_> = moves
            .iter()
            .filter(|m| {
                m.path
                    .last()
                    .map(|a| a == "CW" || a == "CCW")
                    .unwrap_or(false)
            })
            .take(8)
            .collect();
        eprintln!("sample spin-ending I moves:");
        for m in &spin_paths {
            eprintln!("  ({},{},r{}) path={:?}", m.row, m.col, m.rot, m.path);
        }

        let type_j = 6usize;
        let mut ranked: Vec<(i32, i32, usize, usize, Vec<String>)> = Vec::new();
        for m in &moves {
            let (bb1, c1, h1) =
                simulate_place_and_clear(&bb, 0, m.rot, m.col as usize, m.row);
            let mv2 = bfs_moves(&bb1, type_j, 0, 3, 0);
            let mut inner = i32::MIN;
            for m2 in &mv2 {
                let (bb2, c2, h2) =
                    simulate_place_and_clear(&bb1, type_j, m2.rot, m2.col as usize, m2.row);
                inner = inner.max(meatfighter_evaluate(
                    &bb2,
                    &column_heights(&bb2),
                    c1 + c2,
                    h1 + h2,
                ));
            }
            ranked.push((inner, m.row, m.col as usize, m.rot, m.path.clone()));
        }
        ranked.sort_by(|a, b| b.0.cmp(&a.0));
        eprintln!("2-ply top 5 I placements (next J):");
        for (i, (sc, row, col, rot, path)) in ranked.iter().take(5).enumerate() {
            eprintln!("  #{i} score={sc} ({row},{col},r{rot}) path={path:?}");
        }
        let target = ranked.first().unwrap();
        let target = moves
            .iter()
            .find(|m| m.row == target.1 && m.col as usize == target.2 && m.rot == target.3)
            .unwrap();
        eprintln!("2-ply winner: row={} path={:?}", target.row, target.path);
        let mtype = classify_move(&bb, 0, target.row, target.col, target.rot, &target.path, 0);
        eprintln!("classify_move => {mtype}");

        let recorded = ["D"; 9]
            .iter()
            .map(|s| s.to_string())
            .chain(["CW".to_string(), "D".to_string(), "CCW".to_string()])
            .collect::<Vec<_>>();
        eprintln!("recorded misdrop path: {recorded:?}");

        let mut r = 0i32;
        let mut c = 3i32;
        let mut rot = 0usize;
        for act in &recorded {
            match act.as_str() {
                "D" => r += 1,
                "CW" => {
                    let got = srs_try_rotate(&bb, 0, r, c, rot, true);
                    eprintln!("CW at ({r},{c},r{rot}) => {got:?}");
                    if let Some((nr, nc, nrot)) = got {
                        r = nr;
                        c = nc;
                        rot = nrot;
                    }
                }
                "CCW" => {
                    let got = srs_try_rotate(&bb, 0, r, c, rot, false);
                    eprintln!("CCW at ({r},{c},r{rot}) => {got:?}");
                    if let Some((nr, nc, nrot)) = got {
                        r = nr;
                        c = nc;
                        rot = nrot;
                    }
                }
                _ => {}
            }
        }
        eprintln!("recorded path end ({r},{c},r{rot}) vs target ({},{},r0)", target.row, target.col);

        // Full BFS winning path
        let mut r2 = 0i32;
        let mut c2 = 3i32;
        let mut rot2 = 0usize;
        for act in &target.path {
            match act.as_str() {
                "D" => r2 += 1,
                "CW" => {
                    let got = srs_try_rotate(&bb, 0, r2, c2, rot2, true);
                    let (nr, nc, nrot) = got.expect("BFS CW");
                    r2 = nr;
                    c2 = nc;
                    rot2 = nrot;
                }
                "CCW" => {
                    let got = srs_try_rotate(&bb, 0, r2, c2, rot2, false);
                    let (nr, nc, nrot) = got.expect("BFS CCW");
                    r2 = nr;
                    c2 = nc;
                    rot2 = nrot;
                }
                _ => {}
            }
        }
        eprintln!("2-ply winner sim end ({r2},{c2},r{rot2})");
        assert_eq!((r2, c2, rot2), (target.row, target.col, target.rot));
    }

    #[test]
    fn t_to_o_misdrop_tuck_analysis() {
        use crate::bot::{bfs_moves, classify_move, piece_collides};

        // T→O #1 replay board (from savestate bitboard rows 15-17)
        let mut bb = [0u16; BOARD_ROWS];
        bb[15] = 944;
        bb[16] = 995;
        bb[17] = 507;

        let type_t = 2usize;
        let spawn_col = 1usize;
        let spawn_rot = 3usize;

        let moves = bfs_moves(&bb, type_t, 0, spawn_col, spawn_rot);
        let target = moves
            .iter()
            .find(|m| m.col == 2 && m.rot == 0)
            .expect("BFS should reach col2 rot0");

        eprintln!("BFS path to (2,r0): {:?} lock row={}", target.path, target.row);
        let mtype = classify_move(&bb, type_t, target.row, target.col, target.rot, &target.path, 0);
        eprintln!("classify_move => {mtype}");

        // Gravity-only drop test for tuck semantics
        let mut grav = 0i32;
        while !piece_collides(&bb, type_t, 0, grav + 1, 2) {
            grav += 1;
        }
        eprintln!("straight drop col2 rot0 lands row={grav}, lock row={}", target.row);

        // Simulate recorded misdrop path prefix (before col-drift R spam)
        let mut r = 0i32;
        let mut c = spawn_col as i32;
        let mut rot = spawn_rot;
        for act in ["D", "D", "D", "D", "D", "D", "D", "D", "D", "CW"] {
            match act {
                "D" => r += 1,
                "CW" => {
                    let got = srs_try_rotate(&bb, type_t, r, c, rot, true);
                    eprintln!("CW at ({r},{c},r{rot}) => {got:?}");
                    if let Some((nr, nc, nrot)) = got {
                        r = nr;
                        c = nc;
                        rot = nrot;
                    }
                }
                _ => {}
            }
        }
        eprintln!("after D×9,CW: ({r},{c},r{rot}) target ({},{},r0)", target.row, target.col);

        // Can we slide R from there to col 2?
        let can_r = !piece_collides(&bb, type_t, rot, r, c + 1);
        eprintln!("R from post-CW position blocked={}", !can_r);

        // Full BFS winning path simulation
        let mut r2 = 0i32;
        let mut c2 = spawn_col as i32;
        let mut rot2 = spawn_rot;
        let sim_end = simulate_path(&bb, type_t, 0, spawn_col as i32, spawn_rot, &target.path);
        eprintln!("BFS path sim {sim_end:?} target ({},{},r{})", target.row, target.col, target.rot);
        assert_eq!(sim_end, Some((target.row, target.col as i32, target.rot)));

        // Misdrop path D×9,CW,R is NOT the BFS path — R×9 is col-drift compensation junk
        assert_ne!(target.path.len(), 19, "BFS path should not need 9 R compensation");
    }

    #[test]
    fn z_replay_final_ccw_matches_emu_position() {
        // Z→S #1: before final CCW in recorded path — emu (row 6, col 3, rot 1) → (6, 2, rot 0).
        let mut bb = [0u16; BOARD_ROWS];
        bb[17] = 0b0111011100;
        assert_eq!(srs_try_rotate(&bb, 4, 6, 3, 1, false), Some((6, 2, 0)));
    }

    #[test]
    fn i_tuck_r16_c1_r0_floor_ccw_matches_hardware() {
        use crate::bot::fixtures::misdrop_fixture;
        use crate::state::EmulatorState;
        use base64::Engine;

        let b64 = std::fs::read_to_string(misdrop_fixture("misdrop_i_tuck_r16_c1_r0_state.b64"))
            .expect("i_tuck state");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.trim().trim_matches('"'))
            .expect("b64");
        let state: EmulatorState = bincode::deserialize(&bytes).expect("state");
        let bb = bitboard_from_savestate_ram(&state.ram);

        // Hardware CCW from vertical r1 at ~(12,2) lands horizontal (13,0,r0), not test-0 (14,1).
        assert_eq!(
            srs_try_rotate_grounded(&bb, 0, 12, 2, 1, false),
            Some((13, 0, 0)),
            "CCW r1→r0 must use CW-table (-1,0) kick on floor"
        );
        assert_eq!(
            srs_try_rotate_grounded(&bb, 0, 12, 3, 1, false),
            Some((13, 1, 0)),
            "CCW from col 3 shifts with kick"
        );

        let live_path: Vec<String> = std::iter::repeat("D".to_string())
            .take(14)
            .chain(["L", "L", "CCW", "D", "D", "L"].iter().map(|s| s.to_string()))
            .collect();
        let end = simulate_path(&bb, 0, -2, 5, 1, &live_path);
        assert_ne!(
            end,
            Some((16, 1, 0)),
            "recorded misdrop path must not sim to row-16 lock"
        );
        let moves = crate::bot::bfs_moves(&bb, 0, -2, 5, 1);
        let two_l_before_ccw = moves.iter().any(|m| {
            m.row == 16
                && m.col == 1
                && m.rot == 0
                && m.path.windows(3).any(|w| w == ["L", "L", "CCW"])
        });
        assert!(
            !two_l_before_ccw,
            "BFS must not offer row-16 via L,L,CCW tuck (false-positive CCW from col 3)"
        );
    }

    /// I: r0≡r2 / r1≡r3 visually; floor A≠B (like S/Z).
    #[test]
    fn i_ab_rotation_asymmetry_at_floor() {
        use crate::bot::fixtures::misdrop_fixture;
        use crate::state::EmulatorState;
        use base64::Engine;

        let b64 = std::fs::read_to_string(misdrop_fixture("misdrop_i_tuck_r16_c1_r0_state.b64"))
            .expect("i_tuck state");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.trim().trim_matches('"'))
            .expect("b64");
        let state: EmulatorState = bincode::deserialize(&bytes).expect("state");
        let bb = bitboard_from_savestate_ram(&state.ram);
        let r = 12i32;
        let c = 2i32;

        let ccw_r1 = srs_try_rotate_auto(&bb, 0, r, c, 1, false).expect("r1 CCW/B");
        let cw_r1 = srs_try_rotate_auto(&bb, 0, r, c, 1, true).expect("r1 CW/A");
        assert_eq!(ccw_r1, (13, 0, 0), "r1 CCW/B → horizontal r0 at (13,0)");
        assert_eq!(cw_r1.2, 2, "r1 CW/A → horizontal r2, not r0");
        assert!(
            ccw_r1.0 < cw_r1.0 || ccw_r1.1 != cw_r1.1,
            "same visual vertical: B vs A land differently on floor"
        );
    }

    #[test]
    fn i_tuck_r16_c1_r0_ccw_kick_probe() {
        use crate::bot::fixtures::misdrop_fixture;
        use crate::state::EmulatorState;
        use base64::Engine;

        let b64 = std::fs::read_to_string(misdrop_fixture("misdrop_i_tuck_r16_c1_r0_state.b64"))
            .expect("i_tuck state");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.trim().trim_matches('"'))
            .expect("b64");
        let state: EmulatorState = bincode::deserialize(&bytes).expect("state");
        let bb = bitboard_from_savestate_ram(&state.ram);

        // Hardware: vertical I at ~(12,2,r1), CCW → horizontal (13,0,r0).
        for &(r, c) in &[(12, 2), (12, 3)] {
            let (br, bc) = srs_basic_rotate_anchor(0, r, c, 1, 0, false);
            eprintln!("CCW r1→r0 from ({r},{c}) base=({br},{bc})");
            for (ti, &(kx, ky)) in kicks(0, 1, false).iter().enumerate() {
                let nr = br - ky;
                let nc = bc + kx;
                let fits = piece_fits(&bb, 0, 0, nr, nc);
                if fits {
                    eprintln!("  kick {ti} ({kx},{ky}) => ({nr},{nc},r0)");
                }
            }
            eprintln!("  auto: {:?}", srs_try_rotate_auto(&bb, 0, r, c, 1, false));
        }
        // CW reaches r2 horizontal — A vs B asymmetry (same visual, different SRS state).
        for &(r, c) in &[(12, 2), (12, 3)] {
            let (br, bc) = srs_basic_rotate_anchor(0, r, c, 1, 2, true);
            eprintln!("CW r1→r2 from ({r},{c}) base=({br},{bc})");
            for (ti, &(kx, ky)) in kicks(0, 1, true).iter().enumerate() {
                let nr = br - ky;
                let nc = bc + kx;
                if piece_fits(&bb, 0, 2, nr, nc) {
                    eprintln!("  kick {ti} ({kx},{ky}) => ({nr},{nc},r2)");
                }
            }
        }
        eprintln!("r0@(13,0) fits: {}", piece_fits(&bb, 0, 0, 13, 0));
        eprintln!("r2@(13,0) fits: {}", piece_fits(&bb, 0, 2, 13, 0));
    }

    #[test]
    fn z_spin_r15_c1_r1_blocked_by_floor_policy() {
        use crate::bot::fixtures::misdrop_fixture;
        use crate::state::EmulatorState;
        use base64::Engine;

        let b64 = std::fs::read_to_string(misdrop_fixture("misdrop_z_spin_r15_c1_r1_state.b64"))
            .expect("z_spin state");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.trim().trim_matches('"'))
            .expect("b64");
        let state: EmulatorState = bincode::deserialize(&bytes).expect("state");
        let bb = bitboard_from_savestate_ram(&state.ram);
        let type_z = 4usize;

        let live_path: Vec<String> = std::iter::repeat("D".to_string())
            .take(17)
            .chain(["L", "CW"].iter().map(|s| s.to_string()))
            .collect();

        let moves = crate::bot::bfs_moves(&bb, type_z, -2, 3, 0);

        assert!(
            !moves.iter().any(|m| m.row == 15 && m.col == 1 && m.rot == 1),
            "Z spin (15,1,r1) must be unreachable — same-row tuck false positive"
        );

        assert_eq!(
            srs_try_rotate_grounded(&bb, type_z, 15, 2, 0, true),
            Some((14, 2, 1)),
            "grounded CW must kick up to (14,2,r1) matching hardware"
        );

        let end = simulate_path(&bb, type_z, -2, 3, 0, &live_path);
        assert_eq!(
            end,
            Some((14, 2, 1)),
            "recorded path sim must match hardware lock, not planner want"
        );
    }

    /// 2026-07-18 board: D×18+CW fiction (15,1,r1); hardware CW@(16,3) → (15,3,r1).
    #[test]
    fn z_spin_r15_c1_r1_20260718_floor_up_same_col() {
        use crate::bot::fixtures::misdrop_fixture;
        use crate::state::EmulatorState;
        use base64::Engine;

        let b64 = std::fs::read_to_string(misdrop_fixture(
            "misdrop_z_spin_r15_c1_r1_20260718-103523_state.b64",
        ))
        .expect("z_spin 20260718 state");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.trim().trim_matches('"'))
            .expect("b64");
        let state: EmulatorState = bincode::deserialize(&bytes).expect("state");
        let bb = bitboard_from_savestate_ram(&state.ram);
        let type_z = 4usize;

        assert_eq!(
            srs_try_rotate_grounded(&bb, type_z, 16, 3, 0, true),
            Some((15, 3, 1)),
            "grounded CW@(16,3) must match hardware (15,3,r1), not fiction (15,1)"
        );
        assert!(
            !crate::bot::bfs_moves(&bb, type_z, -2, 3, 0)
                .iter()
                .any(|m| m.row == 15 && m.col == 1 && m.rot == 1),
            "BFS must not offer fiction (15,1,r1)"
        );
    }


    /// GREEN: Z true-rotation — CW@(13,3,r0) → (13,4,r1) on this board (not (13,2,r1)).
    #[test]
    fn z_tuck_r16_c2_r2_cw_at_row13_matches_hardware() {
        use crate::bot::fixtures::misdrop_fixture;
        use crate::bot::simulate_path_stepwise;
        use crate::state::EmulatorState;
        use base64::Engine;

        let b64 = std::fs::read_to_string(misdrop_fixture(
            "misdrop_z_tuck_r16_c2_r2_20260718-162308_state.b64",
        ))
        .expect("z_tuck state");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.trim().trim_matches('"'))
            .expect("b64");
        let state: EmulatorState = bincode::deserialize(&bytes).expect("state");
        let bb = bitboard_from_savestate_ram(&state.ram);
        let type_z = 4usize;

        assert_eq!(
            srs_try_rotate_auto(&bb, type_z, 13, 3, 0, true),
            Some((13, 4, 1)),
            "CW@(13,3,r0) must match hardware (13,4,r1), not fiction (13,2,r1)"
        );

        // Free-space chain at col 3 (no stack): r0 CW steps right one col.
        assert_eq!(
            srs_try_rotate_auto(&bb, type_z, 8, 3, 0, true),
            Some((8, 4, 1)),
            "free-space Z CW r0→r1 steps right"
        );
        assert_eq!(
            srs_try_rotate_auto(&bb, type_z, 8, 3, 0, false),
            Some((8, 3, 3)),
            "free-space Z CCW r0→r3 stays col"
        );

        // Old browser path used fiction left kick and must not reach want.
        let recorded: Vec<String> = std::iter::repeat_n("D".into(), 15)
            .chain(["CW".into(), "D".into(), "CW".into(), "D".into(), "R".into()])
            .collect();
        assert_ne!(
            simulate_path_stepwise(&bb, type_z, -2, 3, 0, &recorded),
            Some((16, 2, 2)),
            "recorded fiction path must not reach want after SRS fix"
        );
    }

    /// Emu: soft-drop to row 13 then A — hardware lands (13,4,r1).
    #[test]
    fn z_tuck_r16_c2_r2_emu_cw_at_row13() {
        use crate::bot::fixtures::{emulator_from_savestate, misdrop_fixture};
        use crate::bot::{ori_info, piece_left_col, piece_min_row, ADDR_CUR_ORI};
        use crate::emulator::joypad::GbButton;
        use crate::state::EmulatorState;
        use base64::Engine;

        let b64 = std::fs::read_to_string(misdrop_fixture(
            "misdrop_z_tuck_r16_c2_r2_20260718-162308_state.b64",
        ))
        .expect("state");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.trim().trim_matches('"'))
            .expect("b64");
        let state: EmulatorState = bincode::deserialize(&bytes).expect("state");
        let mut emu = emulator_from_savestate(&state);

        let mut landed = None;
        for _ in 0..400 {
            let row = piece_min_row(|a| emu.memory.read(a));
            let col = piece_left_col(|a| emu.memory.read(a));
            let rot = ori_info(emu.memory.read(ADDR_CUR_ORI))
                .map(|i| i.1 as usize)
                .unwrap_or(99);
            if row >= 13 && rot == 0 {
                emu.joypad.press(GbButton::A);
                emu.run_frame();
                emu.joypad.release(GbButton::A);
                emu.run_frame();
                let r = piece_min_row(|a| emu.memory.read(a));
                let c = piece_left_col(|a| emu.memory.read(a));
                let ro = ori_info(emu.memory.read(ADDR_CUR_ORI))
                    .map(|i| i.1 as usize)
                    .unwrap_or(99);
                landed = Some((r, c, ro));
                break;
            }
            emu.joypad.press(GbButton::Down);
            emu.run_frame();
            emu.joypad.release(GbButton::Down);
            emu.run_frame();
        }
        assert_eq!(
            landed,
            Some((13, 4, 1)),
            "hardware CW at row13 must land (13,4,r1)"
        );
    }




    #[test]
    fn j_spin_r15_c4_r3_kick_probe() {
        use crate::bot::fixtures::misdrop_fixture;
        use crate::state::EmulatorState;
        use base64::Engine;

        let b64 = std::fs::read_to_string(misdrop_fixture("misdrop_j_spin_r15_c4_r3_state.b64"))
            .expect("j_spin state");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.trim().trim_matches('"'))
            .expect("b64");
        let state: EmulatorState = bincode::deserialize(&bytes).expect("state");
        let bb = bitboard_from_savestate_ram(&state.ram);
        let type_j = 6usize;

        for &(r, c, rot, lbl) in &[
            (14, 3, 0, "after-16D-sim"),
            (15, 3, 0, "gravity+1-before-CCW"),
            (15, 4, 0, "gravity+1+R-equiv"),
            (14, 4, 0, "after-16D+R"),
            (13, 3, 0, "emu-16D-anchor"),
        ] {
            eprintln!(
                "\n=== {lbl} ({r},{c},r{rot}) grounded={} under_lip={} floor_policy={}",
                piece_grounded(&bb, type_j, rot, r, c),
                under_overhang(&bb, type_j, rot, r, c),
                srs_floor_policy(&bb, type_j, rot, r, c),
            );
            let to_rot = (rot + 3) % 4;
            let (br, bc) = srs_basic_rotate_anchor(type_j, r, c, rot, to_rot, false);
            eprintln!("  CCW base=({br},{bc})");
            for (ti, &(kx, ky)) in kicks(type_j, rot, false).iter().enumerate() {
                let nr = br - ky;
                let nc = bc + kx;
                let fits = piece_fits(&bb, type_j, to_rot, nr, nc);
                eprintln!("  kick {ti} ({kx},{ky}) => ({nr},{nc},r{to_rot}) fits={fits}");
            }
            eprintln!(
                "  auto={:?} grounded_detailed={:?} no_floor={:?}",
                srs_try_rotate_auto(&bb, type_j, r, c, rot, false),
                srs_try_rotate_detailed(&bb, type_j, r, c, rot, false, true),
                srs_try_rotate_detailed(&bb, type_j, r, c, rot, false, false),
            );
        }

        for &(r, c, rot, lbl, cw) in &[
            (13, 4, 3, "hardware-got", false),
            (12, 3, 3, "sim-after-CCW1", false),
            (15, 3, 2, "pre-final-CW-sim", true),
            (15, 4, 2, "after-D-sim", false),
        ] {
            eprintln!(
                "  {lbl} ({r},{c},r{rot}) {} auto={:?}",
                if cw { "CW" } else { "CCW" },
                srs_try_rotate_auto(&bb, type_j, r, c, rot, cw)
            );
        }
        let row13: Vec<String> = std::iter::repeat("D".to_string())
            .take(16)
            .chain(["R", "CCW", "D"].iter().map(|s| s.to_string()))
            .collect();
        eprintln!(
            "row-13 path sim end {:?}",
            crate::bot::simulate_path_stepwise(&bb, type_j, -2, 3, 0, &row13)
        );

        eprintln!("\n--- CW from (15,3,r2) kick table ---");
        let (br, bc) = srs_basic_rotate_anchor(type_j, 15, 3, 2, 3, true);
        for (ti, &(kx, ky)) in kicks(type_j, 2, true).iter().enumerate() {
            let nr = br - ky;
            let nc = bc + kx;
            let fits = piece_fits(&bb, type_j, 3, nr, nc);
            eprintln!("  kick {ti} ({kx},{ky}) => ({nr},{nc},r3) fits={fits}");
        }

        eprintln!("\n--- brute: rotations → (13,4,r3) or (15,4,r3) ---");
        for r in 11..=16 {
            for c in 0..=6 {
                for rot in 0..4 {
                    for &cw in &[false, true] {
                        if let Some(end) = srs_try_rotate_auto(&bb, type_j, r, c, rot, cw) {
                            if end == (13, 4, 3) || end == (15, 4, 3) {
                                let lbl = if cw { "CW" } else { "CCW" };
                                eprintln!("  {lbl} from ({r},{c},r{rot}) => {end:?}");
                            }
                        }
                    }
                }
            }
        }

        let moves = crate::bot::bfs_moves(&bb, type_j, -2, 3, 0);
        assert!(
            moves.iter().any(|m| m.row == 13 && m.col == 4 && m.rot == 3),
            "row-13 lock must remain reachable"
        );
    }

    #[test]
    fn j_spin_r15_c4_r3_blocked_by_floor_policy() {
        use crate::bot::fixtures::misdrop_fixture;
        use crate::state::EmulatorState;
        use base64::Engine;

        let b64 = std::fs::read_to_string(misdrop_fixture("misdrop_j_spin_r15_c4_r3_state.b64"))
            .expect("j_spin state");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.trim().trim_matches('"'))
            .expect("b64");
        let state: EmulatorState = bincode::deserialize(&bytes).expect("state");
        let bb = bitboard_from_savestate_ram(&state.ram);
        let type_j = 6usize;

        let moves = crate::bot::bfs_moves(&bb, type_j, -2, 3, 0);
        assert!(
            !moves.iter().any(|m| m.row == 15 && m.col == 4 && m.rot == 3),
            "J spin (15,4,r3) must be unreachable under floor SRS"
        );
        assert!(
            moves.iter().any(|m| m.row == 13 && m.col == 4 && m.rot == 3),
            "J spin (13,4,r3) row-13 lock must remain reachable"
        );

        assert_eq!(
            srs_try_rotate_grounded(&bb, type_j, 15, 3, 2, true),
            Some((13, 4, 3)),
            "grounded final CW must match hardware lock, not sim (15,4,r3)"
        );

        let live_path: Vec<String> = std::iter::repeat("D".to_string())
            .take(16)
            .chain(["CCW", "CCW", "D", "L", "CW"].iter().map(|s| s.to_string()))
            .collect();
        let end = crate::bot::simulate_path_stepwise(&bb, type_j, -2, 3, 0, &live_path);
        assert_eq!(
            end,
            Some((13, 4, 3)),
            "recorded misdrop path must sim to hardware lock row 13"
        );
    }

    #[test]
    fn j_spin_r16_c5_r2_blocked_by_floor_policy() {
        use crate::bot::fixtures::misdrop_fixture;
        use crate::state::EmulatorState;
        use base64::Engine;

        let b64 = std::fs::read_to_string(misdrop_fixture("misdrop_j_spin_r16_c5_r2_state.b64"))
            .expect("j_spin state");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.trim().trim_matches('"'))
            .expect("b64");
        let state: EmulatorState = bincode::deserialize(&bytes).expect("state");
        let bb = bitboard_from_savestate_ram(&state.ram);
        let type_j = 6usize;

        let moves = crate::bot::bfs_moves(&bb, type_j, -2, 3, 0);
        assert!(
            !moves.iter().any(|m| m.row == 16 && m.col == 5 && m.rot == 2),
            "J spin (16,5,r2) must be unreachable under floor SRS"
        );

        // Final grounded CCW: test 0 shifts down (14,4,r3)→(16,5,r2) — blocked on floor.
        assert_eq!(
            srs_try_rotate_grounded(&bb, type_j, 14, 4, 3, false),
            None,
            "grounded final CCW must fail: downward basic rotation + no upward kick"
        );
        // Setup CCW uses test 0 (not mandatory upward kick) when basic rotation fits above.
        assert_eq!(
            srs_try_rotate_grounded(&bb, type_j, 15, 5, 0, false),
            Some((13, 5, 3))
        );

        let live_path: Vec<String> = std::iter::repeat("D".to_string())
            .take(17)
            .chain(["R", "CCW", "D", "D", "CCW"].iter().map(|s| s.to_string()))
            .collect();
        let end = simulate_path(&bb, type_j, -2, 3, 0, &live_path);
        assert_ne!(
            end,
            Some((16, 5, 2)),
            "recorded live path must not sim to want lock under floor SRS"
        );
    }

    #[test]
    #[ignore = "awaiting fresh fixture capture"]
    fn j_to_t_spin_blocked_by_floor_policy() {
        use crate::bot::bfs_moves;
        use crate::state::EmulatorState;
        use base64::Engine;

        let b64 = std::fs::read_to_string(
            crate::bot::fixtures::misdrop_fixture("j_to_t_replay_state.b64"),
        )
        .unwrap()
        .trim()
        .trim_matches('"')
        .to_string();
        let bytes = base64::engine::general_purpose::STANDARD.decode(b64).unwrap();
        let state: EmulatorState = bincode::deserialize(&bytes).unwrap();
        let bb = bitboard_from_savestate_ram(&state.ram);
        let type_j = 6usize;
        let moves = bfs_moves(&bb, type_j, 0, 3, 0);
        assert!(
            !moves.iter().any(|m| m.col == 3 && m.rot == 3 && m.row == 14),
            "J spin (14,3,r3) must be unreachable"
        );
        assert_eq!(srs_try_rotate_grounded(&bb, type_j, 14, 2, 2, true), None);
        assert_eq!(
            srs_try_rotate_grounded(&bb, type_j, 12, 3, 1, true),
            Some((11, 2, 2))
        );
    }

    #[test]
    fn grounded_l_ccw_blocked_by_wall() {
        // Narrow well: in-place NRS would allow CCW; SRS kicks cannot fit.
        let mut bb = [0u16; BOARD_ROWS];
        for row in 10..BOARD_ROWS {
            bb[row] = 0b11; // cols 0-1 filled
        }
        // L R0 in well at col 0, grounded on stack
        assert!(srs_try_rotate(&bb, 5, 16, 0, 0, false).is_none());
    }

    #[test]
    #[ignore = "awaiting fresh fixture capture"]
    fn l_to_z_spin_blocked_by_floor_policy() {
        use crate::bot::{bfs_moves, classify_move};
        use crate::state::EmulatorState;
        use base64::Engine;

        let b64 = std::fs::read_to_string(
            crate::bot::fixtures::misdrop_fixture("l_to_z_misdrop_state.b64"),
        )
        .expect("l_to_z_misdrop_state.b64");
        let b64 = b64.trim().trim_matches('"');
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .expect("base64");
        let state: EmulatorState = bincode::deserialize(&bytes).expect("state");
        let bb = bitboard_from_savestate_ram(&state.ram);
        let type_l = 5usize;
        let spawn_col = 2usize;
        let spawn_rot = 1usize;

        let moves = bfs_moves(&bb, type_l, 0, spawn_col, spawn_rot);
        eprintln!("BFS locks from spawn: {}", moves.len());
        for m in moves.iter().filter(|m| m.col <= 1 && m.rot <= 1) {
            let mtype = classify_move(&bb, type_l, m.row, m.col, m.rot, &m.path, 0);
            eprintln!(
                "  ({},{},r{}) type={mtype} path={:?}",
                m.row, m.col, m.rot, m.path
            );
        }

        assert!(
            !moves.iter().any(|m| m.col == 0 && m.rot == 1 && m.row == 15),
            "L spin lock (15,0,r1) must be unreachable under floor SRS"
        );
        assert_eq!(
            srs_try_rotate_auto(&bb, type_l, 15, 0, 2, false),
            None,
            "grounded final CCW at lock row must fail (no valid floor kick)"
        );
    }

    #[test]
    fn t_spin_suffix_from_emulator_setup_ccw_pose() {
        use crate::bot::fixtures::misdrop_fixture;
        use crate::state::EmulatorState;
        use base64::Engine;

        let b64 = std::fs::read_to_string(misdrop_fixture("misdrop_t_spin_r16_c5_r2_state.b64"))
            .expect("t_spin state");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.trim().trim_matches('"'))
            .expect("b64");
        let state: EmulatorState = bincode::deserialize(&bytes).expect("state");
        let bb = bitboard_from_savestate_ram(&state.ram);
        let type_t = 2usize;
        let tail: Vec<String> = ["D", "D", "D", "L", "CCW"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        assert_eq!(
            crate::bot::simulate_path_stepwise(&bb, type_t, 12, 6, 3, &tail),
            Some((16, 5, 2)),
            "sim setup CCW pose must complete planned suffix"
        );
        assert!(
            crate::bot::simulate_path_stepwise(&bb, type_t, 13, 5, 3, &tail).is_none(),
            "emu setup CCW @(13,5,r3) cannot run sim suffix — executor must recover"
        );
        assert_eq!(
            crate::bot::simulate_path_stepwise(&bb, type_t, 14, 6, 3, &["D".into()]),
            Some((15, 6, 3)),
            "via row6: D from (14,6,r3)"
        );
        assert_eq!(
            crate::bot::simulate_path_stepwise(&bb, type_t, 15, 6, 3, &["L".into()]),
            Some((15, 5, 3)),
            "via row6: L from (15,6,r3)"
        );
        let via_row6: Vec<String> = ["D", "L", "CCW"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            crate::bot::simulate_path_stepwise(&bb, type_t, 14, 6, 3, &via_row6),
            Some((16, 5, 2)),
            "emu row6 recovery suffix reaches want"
        );
        assert_eq!(
            srs_try_rotate_grounded(&bb, type_t, 15, 5, 3, false),
            Some((16, 5, 2)),
            "pre-terminal @(15,5,r3) CCW reaches want in sim"
        );
        let replan: Vec<String> = ["R", "D", "D", "L", "CCW"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            crate::bot::simulate_path_stepwise(&bb, type_t, 13, 5, 3, &replan),
            Some((16, 5, 2)),
            "BFS replan tail from @(13,5,r3) must sim to want"
        );
        for n in 1..=5 {
            eprintln!(
                "replan prefix {n}: {:?}",
                crate::bot::simulate_path_stepwise(&bb, type_t, 13, 5, 3, &replan[..n])
            );
        }
    }

    #[test]
    fn t_spin_r16_c5_r2_terminal_ccw_reachable() {
        use crate::bot::fixtures::misdrop_fixture;
        use crate::state::EmulatorState;
        use base64::Engine;

        let b64 = std::fs::read_to_string(misdrop_fixture("misdrop_t_spin_r16_c5_r2_state.b64"))
            .expect("t_spin state");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.trim().trim_matches('"'))
            .expect("b64");
        let state: EmulatorState = bincode::deserialize(&bytes).expect("state");
        let bb = bitboard_from_savestate_ram(&state.ram);
        let type_t = 2usize;

        // Hardware-valid terminal poses (unlike misdrop_l_spin want).
        assert_eq!(
            srs_try_rotate_grounded(&bb, type_t, 15, 5, 3, false),
            Some((16, 5, 2)),
            "pre-terminal CCW @(15,5,r3) must reach want"
        );
        assert_eq!(
            srs_try_rotate_grounded(&bb, type_t, 14, 5, 3, false),
            Some((16, 5, 2)),
            "browser got pose CCW @(14,5,r3) must reach want"
        );
    }

    #[test]
    fn l_spin_r15_c3_r2_terminal_ccw_srs() {
        use crate::bot::fixtures::misdrop_fixture;
        use crate::state::EmulatorState;
        use base64::Engine;

        let b64 = std::fs::read_to_string(misdrop_fixture("misdrop_l_spin_r15_c3_r2_state.b64"))
            .expect("l_spin state");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.trim().trim_matches('"'))
            .expect("b64");
        let state: EmulatorState = bincode::deserialize(&bytes).expect("state");
        let bb = bitboard_from_savestate_ram(&state.ram);
        let type_l = 5usize;

        for &(r, c, rot, cw) in &[(15, 2, 3, false), (15, 2, 3, true)] {
            let lbl = if cw { "CW" } else { "CCW" };
            eprintln!("\n=== L {lbl} @({r},{c},r{rot}) ===");
            eprintln!(
                "  grounded={} floor_policy={}",
                piece_grounded(&bb, type_l, rot, r, c),
                srs_floor_policy(&bb, type_l, rot, r, c),
            );
            let to_rot = if cw { (rot + 1) % 4 } else { (rot + 3) % 4 };
            let (br, bc) = srs_basic_rotate_anchor(type_l, r, c, rot, to_rot, cw);
            eprintln!("  base=({br},{bc}) to_rot={to_rot}");
            for (ti, &(kx, ky)) in kicks(type_l, rot, cw).iter().enumerate() {
                let nr = br - ky;
                let nc = bc + kx;
                let fits = piece_fits(&bb, type_l, to_rot, nr, nc);
                eprintln!("  kick {ti} ({kx},{ky}) => ({nr},{nc}) fits={fits}");
            }
            eprintln!(
                "  auto={:?} grounded={:?} detailed_floor={:?}",
                srs_try_rotate_auto(&bb, type_l, r, c, rot, cw),
                srs_try_rotate_grounded(&bb, type_l, r, c, rot, cw),
                srs_try_rotate_detailed(&bb, type_l, r, c, rot, cw, true),
            );
        }
        // Emulator manual test: CCW/B at (15,2,r3) does not rotate; CW/A → (15,2,r0).
        assert_eq!(
            srs_try_rotate_grounded(&bb, type_l, 15, 2, 3, false),
            None,
            "L final CCW r3→r2 at (15,2) must be blocked under hardware floor SRS"
        );
    }

    /// RED→GREEN: L floor spin want (13,6,r1) is sim-only under cavity floor policy.
    /// Live got (11,6,r1). Covered by generalized under-lip rule (not a from_rot special-case).
    #[test]
    fn l_spin_r13_c6_r1_want_blocked_by_floor_srs() {
        use crate::bot::fixtures::misdrop_fixture;
        use crate::bot::{find_bfs_path_to_lock, simulate_path_stepwise};
        use crate::state::EmulatorState;
        use base64::Engine;

        let b64 = std::fs::read_to_string(misdrop_fixture("misdrop_l_spin_r13_c6_r1_state.b64"))
            .expect("l_spin r13 state");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.trim().trim_matches('"'))
            .expect("b64");
        let state: EmulatorState = bincode::deserialize(&bytes).expect("state");
        let bb = bitboard_from_savestate_ram(&state.ram);
        let type_l = 5usize;
        let path: Vec<String> = [
            "D", "D", "D", "D", "D", "D", "D", "D", "D", "D", "D", "D", "D", "R", "R", "CCW",
            "CCW", "D", "D", "CCW",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        assert_eq!(
            srs_try_rotate_grounded(&bb, type_l, 13, 6, 2, false),
            None,
            "terminal CCW @ (13,6,r2) must fail under floor cavity policy"
        );
        assert!(
            find_bfs_path_to_lock(&bb, type_l, -2, 3, 0, 13, 6, 1).is_none(),
            "BFS must not plan unreachable want (13,6,r1)"
        );
        assert!(
            simulate_path_stepwise(&bb, type_l, -2, 3, 0, &path)
                .is_none_or(|p| p != (13, 6, 1)),
            "recorded path must not sim to want under floor SRS"
        );
    }


    /// RED→GREEN: L floor spin want (15,2,r1) is fiction — hardware rejects terminal CCW.
    #[test]
    fn l_spin_r15_c2_r1_20260717_want_blocked_by_floor_srs() {
        use crate::bot::fixtures::misdrop_fixture;
        use crate::bot::{find_bfs_path_to_lock, simulate_path_stepwise};
        use crate::state::EmulatorState;
        use base64::Engine;

        let b64 = std::fs::read_to_string(misdrop_fixture(
            "misdrop_l_spin_r15_c2_r1_20260717-234304_state.b64",
        ))
        .expect("state");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.trim().trim_matches('"'))
            .expect("b64");
        let state: EmulatorState = bincode::deserialize(&bytes).expect("state");
        let bb = bitboard_from_savestate_ram(&state.ram);
        let type_l = 5usize;
        let path: Vec<String> = std::iter::repeat_n("D".into(), 15)
            .chain(["CW", "CW", "D", "D", "CCW"].iter().map(|s| (*s).into()))
            .collect();

        assert_eq!(
            srs_try_rotate_grounded(&bb, type_l, 15, 2, 2, false),
            None,
            "terminal CCW @(15,2,r2) must fail (fiction floor dig spin)"
        );
        assert!(
            find_bfs_path_to_lock(&bb, type_l, -2, 3, 0, 15, 2, 1).is_none(),
            "BFS must not plan unreachable want (15,2,r1)"
        );
        assert!(
            simulate_path_stepwise(&bb, type_l, -2, 3, 0, &path)
                .is_none_or(|p| p != (15, 2, 1)),
            "recorded path must not sim to want"
        );
    }

    /// S/Z have duplicate visual orientations (r0≡r2 horizontal, r1≡r3 vertical) but
    /// SRS uses four distinct kick tables — A (CW) and B (CCW) are not interchangeable.
    #[test]
    fn s_sz_ab_rotation_asymmetry_at_tuck() {
        let mut bb = [0u16; BOARD_ROWS];
        bb[15] = 0b1111000111;
        bb[16] = 0b1000001111;
        bb[17] = 0b1110011111;

        let type_s = 3usize;
        let r = 14i32;
        let c = 4i32;

        // Horizontal r0: B (CCW) tucks left; A (CW) kicks up to vertical r1.
        let ccw_r0 = srs_try_rotate_auto(&bb, type_s, r, c, 0, false).expect("r0 CCW");
        let cw_r0 = srs_try_rotate_auto(&bb, type_s, r, c, 0, true).expect("r0 CW");
        assert_eq!(ccw_r0, (12, 4, 3), "r0 CCW/B → vertical tuck (12,4,r3)");
        assert_eq!(cw_r0.2, 1, "r0 CW/A → vertical r1, not r3");
        assert!(
            cw_r0.0 < r,
            "r0 CW/A must kick upward (row {} < {})",
            cw_r0.0,
            r
        );

        // Horizontal r2: opposite buttons reach vertical r3 vs r1 (distinct kick tables).
        let cw_r2 = srs_try_rotate_auto(&bb, type_s, r, c, 2, true).expect("r2 CW");
        let ccw_r2 = srs_try_rotate_auto(&bb, type_s, r, c, 2, false).expect("r2 CCW");
        assert_eq!(cw_r2.2, 3, "r2 CW/A → vertical r3 (not r1)");
        assert_eq!(ccw_r2.2, 1, "r2 CCW/B → vertical r1 (not r3)");
        assert_ne!(ccw_r0, cw_r0, "same visual horizontal: B vs A land differently");
        assert_ne!(ccw_r0.2, ccw_r2.2, "r0 CCW/B and r2 CCW/B reach different vertical states");
        assert_ne!(cw_r0.2, cw_r2.2, "r0 CW/A and r2 CW/A reach different vertical states");
    }

    #[test]
    #[ignore = "awaiting fresh fixture capture"]
    fn s_to_l_user_path_vs_bfs_plan() {
        use crate::bot::{
            bfs_moves, simulate_path_prefix, BOARD_ROWS,
        };
        use crate::state::EmulatorState;
        use base64::Engine;

        let b64 = std::fs::read_to_string(
            crate::bot::fixtures::misdrop_fixture("s_to_l_misdrop_state.b64"),
        )
        .unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.trim().trim_matches('"'))
            .unwrap();
        let state: EmulatorState = bincode::deserialize(&bytes).unwrap();
        let mut bb = [0u16; BOARD_ROWS];
        for row in 0..BOARD_ROWS {
            let base = 0x800 + row * 32 + 2;
            let mut bits = 0u16;
            for col in 0..10 {
                let v = state.ram.get(base + col).copied().unwrap_or(0);
                if (v & 0x80) != 0 && v != 0x8E {
                    bits |= 1 << col;
                }
            }
            bb[row] = bits;
        }

        let type_s = 3usize;
        let spawn_col = 3i32;
        let spawn_rot = 0usize;

        let moves = bfs_moves(&bb, type_s, 0, spawn_col as usize, spawn_rot);
        let want = moves
            .iter()
            .find(|m| m.col == 5 && m.rot == 2 && m.row == 16)
            .expect("(16,5,r2)");

        eprintln!("BFS shortest path: {:?}", want.path);
        eprintln!(
            "R count={}",
            want.path.iter().filter(|a| *a == "R").count()
        );

        // All distinct paths BFS finds to (16,5,r2)
        let alts: Vec<_> = moves
            .iter()
            .filter(|m| m.col == 5 && m.rot == 2 && m.row == 16)
            .collect();
        eprintln!("{} BFS locks at (16,5,r2)", alts.len());
        for (i, m) in alts.iter().take(12).enumerate() {
            let rs = m.path.iter().filter(|a| *a == "R").count();
            let last = m.path.iter().rev().find(|a| *a != "D").map(|s| s.as_str());
            eprintln!(
                "  #{i} R={rs} rots={} len={} last={last:?} path={:?}",
                m.path.iter().filter(|a| *a == "CW" || *a == "CCW").count(),
                m.path.len(),
                m.path
            );
        }

        // User-reported manual path shapes (one extra R, ends CW not CCW)
        let candidates: Vec<Vec<&str>> = vec![
            // CCW,R, D×14, R, CW
            std::iter::repeat("D").take(0)
                .chain(["CCW", "R"].iter().copied())
                .chain(std::iter::repeat("D").take(14))
                .chain(["R", "CW"].iter().copied())
                .collect(),
            // D×14, R, R, CCW, D×3, CW
            std::iter::repeat("D").take(14)
                .chain(["R", "R", "CCW", "D", "D", "D", "CW"].iter().copied())
                .collect(),
            // D×14, R, CCW, D×3, R, CW
            std::iter::repeat("D").take(14)
                .chain(["R", "CCW", "D", "D", "D", "R", "CW"].iter().copied())
                .collect(),
            // D×9, R, CCW, D×5, R, CW  (recorded misdrop prefix style + extra R + CW)
            std::iter::repeat("D").take(9)
                .chain(["R", "CCW", "D", "D", "D", "D", "D", "R", "CW"].iter().copied())
                .collect(),
            // D×14, R, CCW, D×3, CCW (BFS plan)
            std::iter::repeat("D").take(14)
                .chain(["R", "CCW", "D", "D", "D", "CCW"].iter().copied())
                .collect(),
        ];

        for path in &candidates {
            let path_s: Vec<String> = path.iter().map(|s| s.to_string()).collect();
            let end = simulate_path_prefix(&bb, type_s, 0, spawn_col, spawn_rot, &path_s);
            let rs = path.iter().filter(|a| **a == "R").count();
            eprintln!(
                "sim R={rs} {:?} => {end:?} target (16,5,r2)",
                path_s
            );
        }

        // Spin/tuck suffix variants from grounded (14,3,r0) — user: CCWR…DRCW vs plan R,CCW,DDD,CCW
        let grounded = (14i32, 3, 0usize);
        let suffixes: &[&[&str]] = &[
            &["R", "CCW", "D", "D", "D", "CCW"],
            &["CCW", "R", "D", "D", "D", "R", "CW"],
            &["R", "R", "CCW", "D", "D", "D", "CW"],
            &["CCW", "R", "D", "D", "D", "CCW"],
            &["R", "CCW", "D", "D", "D", "R", "CW"],
            &["CW", "R", "D", "D", "D", "R", "CW"],
        ];
        for suf in suffixes {
            let path_s: Vec<String> = suf.iter().map(|s| s.to_string()).collect();
            let end = simulate_path_prefix(&bb, type_s, grounded.0, grounded.1, grounded.2, &path_s);
            let rs = suf.iter().filter(|a| **a == "R").count();
            eprintln!(
                "from (14,3,r0) R={rs} {:?} => {end:?}",
                path_s
            );
        }

        // Brute: D×n + suffix to (16,5,r2) with 2 R's and final CW
        let mut found = 0usize;
        for d1 in 9..=15usize {
            for d2 in 0..=6usize {
                let path: Vec<String> = std::iter::repeat("D")
                    .take(d1)
                    .map(|s| s.to_string())
                    .chain(
                        ["R", "R", "CCW", "D", "D", "D", "CW"]
                            .iter()
                            .map(|s| s.to_string()),
                    )
                    .chain(std::iter::repeat("D").take(d2).map(|s| s.to_string()))
                    .collect();
                if let Some(end) = simulate_path_prefix(&bb, type_s, 0, spawn_col, spawn_rot, &path)
                {
                    if end == (16, 5, 2) {
                        eprintln!("HIT D×{d1}+RR,CCW,DDD,CW+D×{d2}: {:?}", path);
                        found += 1;
                    }
                }
            }
        }
        eprintln!("brute RR+CW hits: {found}");

        let alt: Vec<String> = std::iter::repeat("D")
            .take(14)
            .map(|s| s.to_string())
            .chain(
                ["CCW", "R", "D", "D", "D", "CCW"]
                    .iter()
                    .map(|s| s.to_string()),
            )
            .collect();
        let end = simulate_path_prefix(&bb, type_s, 0, spawn_col, spawn_rot, &alt);
        eprintln!("D×14,CCW,R,DDD,CCW => {end:?}");

        let user_extra_r: Vec<String> = std::iter::repeat("D")
            .take(14)
            .map(|s| s.to_string())
            .chain(
                ["CCW", "R", "D", "D", "D", "R", "CCW"]
                    .iter()
                    .map(|s| s.to_string()),
            )
            .collect();
        eprintln!(
            "user D×14,CCW,R,DDD,R,CCW => {:?}",
            simulate_path_prefix(&bb, type_s, 0, spawn_col, spawn_rot, &user_extra_r)
        );

        // Does BFS graph include CCW,R tuck ordering?
        let has_ccw_r = alts.iter().any(|m| {
            m.path.windows(2).any(|w| w[0] == "CCW" && w[1] == "R")
        });
        eprintln!("BFS has CCW,R pair in path to (16,5,r2): {has_ccw_r}");

        let rccw: Vec<String> = std::iter::repeat("D")
            .take(14)
            .map(|s| s.to_string())
            .chain(
                ["R", "CCW", "D", "D", "D", "CCW"]
                    .iter()
                    .map(|s| s.to_string()),
            )
            .collect();
        let ccr: Vec<String> = std::iter::repeat("D")
            .take(14)
            .map(|s| s.to_string())
            .chain(
                ["CCW", "R", "D", "D", "D", "CCW"]
                    .iter()
                    .map(|s| s.to_string()),
            )
            .collect();
        for d in 9..=16usize {
            for (name, suf) in [
                ("R,CCW,DDD,CCW", vec!["R", "CCW", "D", "D", "D", "CCW"]),
                ("CCW,R,DDD,CCW", vec!["CCW", "R", "D", "D", "D", "CCW"]),
                ("CCW,R,DDD,R,CW", vec!["CCW", "R", "D", "D", "D", "R", "CW"]),
                ("R,R,CCW,DDD,CW", vec!["R", "R", "CCW", "D", "D", "D", "CW"]),
            ] {
                let path: Vec<String> = std::iter::repeat("D")
                    .take(d)
                    .map(|s| s.to_string())
                    .chain(suf.iter().map(|s| s.to_string()))
                    .collect();
                if let Some(end) =
                    simulate_path_prefix(&bb, type_s, 0, spawn_col, spawn_rot, &path)
                {
                    eprintln!("D×{d} {name} => {end:?}");
                }
            }
        }
    }

    #[test]
    #[ignore = "awaiting fresh fixture capture"]
    fn s_to_l_misdrop_replay_analysis() {
        use crate::bot::{
            bfs_moves, classify_move, column_heights, meatfighter_evaluate,
            piece_collides, simulate_place_and_clear,
        };
        use crate::state::EmulatorState;
        use base64::Engine;

        let b64 = std::fs::read_to_string(
            crate::bot::fixtures::misdrop_fixture("s_to_l_misdrop_state.b64"),
        )
        .expect("s_to_l_misdrop_state.b64");
        let b64 = b64.trim().trim_matches('"');
        let bytes = base64::engine::general_purpose::STANDARD.decode(b64).unwrap();
        let state: EmulatorState = bincode::deserialize(&bytes).unwrap();
        let bb = bitboard_from_savestate_ram(&state.ram);

        let type_s = 3usize;
        let type_l = 5usize;
        let spawn_col = 3usize;
        let spawn_rot = 0usize;
        let recorded: Vec<String> = [
            "D", "D", "D", "D", "D", "D", "D", "D", "D", "R", "CCW", "D", "D", "D", "CCW",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        eprintln!("S→L board nonempty:");
        for (i, row) in bb.iter().enumerate().filter(|(_, r)| **r != 0) {
            eprintln!("  row {i}: {row:010b}");
        }

        let end = simulate_path(&bb, type_s, 0, spawn_col as i32, spawn_rot, &recorded);
        eprintln!("recorded path end {end:?} vs want (16,5,r2) got (14,3,r0)");

        let moves = bfs_moves(&bb, type_s, 0, spawn_col, spawn_rot);
        let want = moves
            .iter()
            .find(|m| m.col == 5 && m.rot == 2 && m.row == 16);
        if let Some(m) = want {
            eprintln!("BFS want (16,5,r2): path={:?}", m.path);
            eprintln!(
                "classify={}",
                classify_move(&bb, type_s, m.row, m.col, m.rot, &m.path, 0)
            );
            let sim = simulate_path(&bb, type_s, 0, spawn_col as i32, spawn_rot, &m.path);
            eprintln!("BFS want sim end {sim:?}");
            let plan = crate::bot::plan_row_before_final_action(
                &bb, type_s, 0, spawn_col as i32, spawn_rot, &m.path, m.row, "spin", m.rot,
            );
            eprintln!("plan_intended_row={plan}");
            let mut r = 0i32;
            let mut c = spawn_col as i32;
            let mut rot = spawn_rot;
            for (i, act) in m.path.iter().enumerate() {
                let grounded = piece_collides(&bb, type_s, rot, r + 1, c);
                eprintln!(
                    "  win step {i}: pre=({r},{c},r{rot}) grounded={grounded} act={act}"
                );
                match act.as_str() {
                    "D" => {
                        if !grounded {
                            r += 1;
                        }
                    }
                    "L" => c -= 1,
                    "R" => c += 1,
                    "CW" => {
                        if let Some((nr, nc, nrot)) =
                            srs_try_rotate_auto(&bb, type_s, r, c, rot, true)
                        {
                            r = nr;
                            c = nc;
                            rot = nrot;
                        }
                    }
                    "CCW" => {
                        if let Some((nr, nc, nrot)) =
                            srs_try_rotate_auto(&bb, type_s, r, c, rot, false)
                        {
                            r = nr;
                            c = nc;
                            rot = nrot;
                        }
                    }
                    _ => {}
                }
            }
            eprintln!("  win trace end: ({r},{c},r{rot})");
        } else {
            eprintln!("BFS has NO (16,5,r2)");
            for m in moves.iter().filter(|m| m.rot == 2 && (14..=17).contains(&m.row)) {
                eprintln!("  ({},{},r{}) path={:?}", m.row, m.col, m.rot, m.path);
            }
        }

        if let Some(m) = moves.iter().find(|m| m.path == recorded) {
            eprintln!(
                "recorded path in spawn BFS → ({},{},r{})",
                m.row, m.col, m.rot
            );
        }

        let mut best_score = i32::MIN;
        let mut best = None;
        for m in &moves {
            let (bb1, c1, h1) =
                simulate_place_and_clear(&bb, type_s, m.rot, m.col as usize, m.row);
            let mv2 = bfs_moves(&bb1, type_l, 0, 3, 0);
            let mut inner = i32::MIN;
            for m2 in &mv2 {
                let (bb2, c2, h2) =
                    simulate_place_and_clear(&bb1, type_l, m2.rot, m2.col as usize, m2.row);
                inner = inner.max(meatfighter_evaluate(
                    &bb2,
                    &column_heights(&bb2),
                    c1 + c2,
                    h1 + h2,
                ));
            }
            let mtype = classify_move(&bb, type_s, m.row, m.col, m.rot, &m.path, 0);
            if inner > best_score {
                best_score = inner;
                best = Some((m.row, m.col as usize, m.rot, m.path.clone(), mtype.to_string()));
            }
        }
        if let Some((row, col, rot, path, mtype)) = &best {
            eprintln!(
                "2-ply WINNER: ({row},{col},r{rot}) type={mtype} score={best_score} path={path:?}"
            );
        }

        // Trace recorded path with grounded / rosy-down checks
        let mut r = 0i32;
        let mut c = spawn_col as i32;
        let mut rot = spawn_rot;
        for (i, act) in recorded.iter().enumerate() {
            let grounded = piece_collides(&bb, type_s, rot, r + 1, c);
            eprintln!(
                "  rec step {i}: pre=({r},{c},r{rot}) grounded={grounded} act={act}"
            );
            match act.as_str() {
                "D" => {
                    if !grounded {
                        r += 1;
                    }
                }
                "L" => c -= 1,
                "R" => c += 1,
                "CW" => {
                    if let Some((nr, nc, nrot)) =
                        srs_try_rotate_auto(&bb, type_s, r, c, rot, true)
                    {
                        r = nr;
                        c = nc;
                        rot = nrot;
                    }
                }
                "CCW" => {
                    if let Some((nr, nc, nrot)) =
                        srs_try_rotate_auto(&bb, type_s, r, c, rot, false)
                    {
                        r = nr;
                        c = nc;
                        rot = nrot;
                    }
                }
                _ => {}
            }
        }
        eprintln!("  rec trace end: ({r},{c},r{rot})");

        // Does got (14,3,r0) appear on winner path?
        if let Some(m) = want {
            let mut r = 0i32;
            let mut c = spawn_col as i32;
            let mut rot = spawn_rot;
            for (i, act) in m.path.iter().enumerate() {
                match act.as_str() {
                    "D" => r += 1,
                    "L" => c -= 1,
                    "R" => c += 1,
                    "CW" => {
                        if let Some((nr, nc, nrot)) =
                            srs_try_rotate_auto(&bb, type_s, r, c, rot, true)
                        {
                            r = nr;
                            c = nc;
                            rot = nrot;
                        }
                    }
                    "CCW" => {
                        if let Some((nr, nc, nrot)) =
                            srs_try_rotate_auto(&bb, type_s, r, c, rot, false)
                        {
                            r = nr;
                            c = nc;
                            rot = nrot;
                        }
                    }
                    _ => {}
                }
                if r == 14 && c == 3 && rot == 0 {
                    eprintln!("  winner hits got at step {i} act={act}");
                }
            }
        }

        // Search who uses recorded path as suffix
        for start_row in 0..=16i32 {
            for start_col in 0..=6usize {
                for start_rot in 0..4usize {
                    let mv = bfs_moves(&bb, type_s, start_row, start_col, start_rot);
                    if let Some(m) = mv.iter().find(|m| m.path == recorded) {
                        eprintln!(
                            "recorded path from ({start_row},{start_col},r{start_rot}) → ({},{},r{})",
                            m.row, m.col, m.rot
                        );
                    }
                }
            }
        }
    }

    #[test]
    #[ignore = "awaiting fresh fixture capture"]
    fn j_to_o_misdrop_replay_analysis() {
        use crate::bot::{
            bfs_moves, classify_move, column_heights, meatfighter_evaluate,
            piece_collides, simulate_place_and_clear, BOARD_COLS, BOARD_ROWS,
        };
        use crate::state::EmulatorState;
        use base64::Engine;

        let b64 = std::fs::read_to_string(
            crate::bot::fixtures::misdrop_fixture("j_to_o_misdrop_state.b64"),
        )
        .expect("j_to_o_misdrop_state.b64");
        let b64 = b64.trim().trim_matches('"');
        let bytes = base64::engine::general_purpose::STANDARD.decode(b64).unwrap();
        let state: EmulatorState = bincode::deserialize(&bytes).unwrap();
        let bb = bitboard_from_savestate_ram(&state.ram);

        let type_j = 6usize;
        let type_o = 1usize;
        let spawn_col = 3usize;
        let spawn_rot = 0usize;
        let recorded: Vec<String> = ["L", "L", "D", "L", "D"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        eprintln!("J→O board nonempty:");
        for (i, row) in bb.iter().enumerate().filter(|(_, r)| **r != 0) {
            eprintln!("  row {i}: {row:010b}");
        }

        let end = simulate_path(&bb, type_j, 0, spawn_col as i32, spawn_rot, &recorded);
        eprintln!("recorded path end {end:?} vs want (15,2,r3) got (13,4,r3)");

        let moves = bfs_moves(&bb, type_j, 0, spawn_col, spawn_rot);
        let want = moves
            .iter()
            .find(|m| m.col == 2 && m.rot == 3 && m.row == 15);
        if let Some(m) = want {
            eprintln!("BFS want (15,2,r3): path={:?}", m.path);
            eprintln!("classify={}", classify_move(&bb, type_j, m.row, m.col, m.rot, &m.path, 0));
            let sim = simulate_path(&bb, type_j, 0, spawn_col as i32, spawn_rot, &m.path);
            eprintln!("BFS want sim end {sim:?}");
        } else {
            eprintln!("BFS has NO (15,2,r3); nearby locks:");
            for m in moves.iter().filter(|m| m.rot == 3 && (12..=16).contains(&m.row)) {
                eprintln!("  ({},{},r{}) path={:?}", m.row, m.col, m.rot, m.path);
            }
        }

        if let Some(m) = moves.iter().find(|m| m.path == recorded) {
            eprintln!(
                "recorded path in BFS → lock ({},{},r{})",
                m.row, m.col, m.rot
            );
        } else {
            eprintln!("recorded path NOT found in BFS from spawn");
        }

        // 2-ply winner (next O)
        let mut best_score = i32::MIN;
        let mut best = None;
        for m in &moves {
            let (bb1, c1, h1) =
                simulate_place_and_clear(&bb, type_j, m.rot, m.col as usize, m.row);
            let mv2 = bfs_moves(&bb1, type_o, 0, 3, 0);
            let mut inner = i32::MIN;
            for m2 in &mv2 {
                let (bb2, c2, h2) =
                    simulate_place_and_clear(&bb1, type_o, m2.rot, m2.col as usize, m2.row);
                inner = inner.max(meatfighter_evaluate(
                    &bb2,
                    &column_heights(&bb2),
                    c1 + c2,
                    h1 + h2,
                ));
            }
            let mtype = classify_move(&bb, type_j, m.row, m.col, m.rot, &m.path, 0);
            if inner > best_score {
                best_score = inner;
                best = Some((m.row, m.col as usize, m.rot, m.path.clone(), mtype.to_string()));
            }
        }
        if let Some((row, col, rot, path, mtype)) = &best {
            eprintln!(
                "2-ply WINNER: ({row},{col},r{rot}) type={mtype} score={best_score} path={path:?}"
            );
        }

        // Where does got (13,4,r3) come from?
        let got_moves = bfs_moves(&bb, type_j, 13, 4, 3);
        eprintln!("1-ply from got (13,4,r3): {} moves", got_moves.len());
        if let Some(m) = got_moves.iter().min_by_key(|m| m.path.len()) {
            eprintln!("  shortest from got: ({},{},r{}) path={:?}", m.row, m.col, m.rot, m.path);
        }
        if let Some(m) = got_moves.iter().find(|m| m.path == recorded) {
            eprintln!(
                "recorded path matches 1-ply from got → lock ({},{},r{})",
                m.row, m.col, m.rot
            );
        }
        for m in moves.iter().filter(|m| m.path == recorded) {
            eprintln!(
                "spawn BFS exact recorded path → ({},{},r{})",
                m.row, m.col, m.rot
            );
        }
        for start_row in 0..=15i32 {
            for start_col in 0..=6usize {
                for start_rot in 0..4usize {
                    let mv = bfs_moves(&bb, type_j, start_row, start_col, start_rot);
                    if let Some(m) = mv.iter().find(|m| m.path == recorded) {
                        eprintln!(
                            "recorded path from ({start_row},{start_col},r{start_rot}) → ({},{},r{})",
                            m.row, m.col, m.rot
                        );
                    }
                }
            }
        }
        // Simulate winner path — where is (13,4,r3) on the way?
        if let Some(m) = want {
            let mut r = 0i32;
            let mut c = spawn_col as i32;
            let mut rot = spawn_rot;
            for (i, act) in m.path.iter().enumerate() {
                match act.as_str() {
                    "D" => r += 1,
                    "L" => c -= 1,
                    "R" => c += 1,
                    "CW" => {
                        if let Some((nr, nc, nrot)) =
                            srs_try_rotate_auto(&bb, type_j, r, c, rot, true)
                        {
                            r = nr;
                            c = nc;
                            rot = nrot;
                        }
                    }
                    "CCW" => {
                        if let Some((nr, nc, nrot)) =
                            srs_try_rotate_auto(&bb, type_j, r, c, rot, false)
                        {
                            r = nr;
                            c = nc;
                            rot = nrot;
                        }
                    }
                    _ => {}
                }
                if r == 13 && c == 4 && rot == 3 {
                    eprintln!("  winner path hits got position at step {i} act={act}");
                }
            }
        }

        // Recorded path is the BFS suffix from (13,5,r3) to finish tuck (15,2,r3)
        let suffix_start = simulate_path(
            &bb,
            type_j,
            13,
            5,
            3,
            &recorded,
        );
        eprintln!("recorded suffix from (13,5,r3) → {suffix_start:?}");
        let from_got = simulate_path(&bb, type_j, 13, 4, 3, &recorded);
        eprintln!("same suffix from got (13,4,r3) → {from_got:?}");

        if let Some(m) = want {
            let mut r = 0i32;
            let mut c = spawn_col as i32;
            let mut rot = spawn_rot;
            for (i, act) in m.path.iter().enumerate() {
                match act.as_str() {
                    "D" => r += 1,
                    "L" => c -= 1,
                    "R" => c += 1,
                    "CW" => {
                        if let Some((nr, nc, nrot)) =
                            srs_try_rotate_auto(&bb, type_j, r, c, rot, true)
                        {
                            r = nr;
                            c = nc;
                            rot = nrot;
                        }
                    }
                    "CCW" => {
                        if let Some((nr, nc, nrot)) =
                            srs_try_rotate_auto(&bb, type_j, r, c, rot, false)
                        {
                            r = nr;
                            c = nc;
                            rot = nrot;
                        }
                    }
                    _ => {}
                }
                if r == 13 && rot == 3 && (c == 4 || c == 5) {
                    eprintln!("  winner at step {i}: ({r},{c},r{rot}) act={act}");
                }
            }
        }

        // Trace recorded path step-by-step with grounded checks
        let mut r = 0i32;
        let mut c = spawn_col as i32;
        let mut rot = spawn_rot;
        for (i, act) in recorded.iter().enumerate() {
            let grounded = piece_collides(&bb, type_j, rot, r + 1, c);
            eprintln!(
                "  rec step {i}: pre=({r},{c},r{rot}) grounded={grounded} act={act}"
            );
            match act.as_str() {
                "D" => {
                    if grounded {
                        eprintln!("    Rosy: D while grounded would instant-lock!");
                    } else {
                        r += 1;
                    }
                }
                "L" => c -= 1,
                "R" => c += 1,
                _ => {}
            }
        }
        eprintln!("  rec trace end: ({r},{c},r{rot})");

        // Check if partial path execution could explain got
        for prefix_len in 1..=recorded.len() {
            if let Some((r, c, rot)) =
                simulate_path(&bb, type_j, 0, spawn_col as i32, spawn_rot, &recorded[..prefix_len])
            {
                let fits = !piece_collides(&bb, type_j, rot, r + 1, c);
                eprintln!(
                    "  prefix[{prefix_len}] end=({r},{c},r{rot}) can_soft_drop={}",
                    !fits
                );
            }
        }
    }














}
