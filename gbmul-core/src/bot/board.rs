//! Board geometry, shapes, and bitboard helpers.

/// Board geometry.
pub const BOARD_ROWS: usize = 18;
pub const BOARD_COLS: usize = 10;

/// BFS row range: Rosy spawns above visible row 0 (up to 4 hidden rows).
pub const BFS_ROW_MIN: i32 = -4;
pub const BFS_ROW_COUNT: usize = (BOARD_ROWS as i32 - BFS_ROW_MIN) as usize;

#[inline]
pub(crate) fn bfs_row_idx(row: i32) -> Option<usize> {
    if row < BFS_ROW_MIN || row >= BOARD_ROWS as i32 {
        None
    } else {
        Some((row - BFS_ROW_MIN) as usize)
    }
}

/// WRAM layout for the playfield (locked pieces).
pub const BOARD_BASE: u16 = 0xC800;
pub const BOARD_STRIDE: usize = 32;

pub const ADDR_CUR_ORI: u16 = 0xC203;
pub const ADDR_NEXT_ORI: u16 = 0xC213;
pub const ADDR_RNG_PTR: u16 = 0xFFB0;
pub const ADDR_RNG_RESULT: u16 = 0xFFAE;
pub const ADDR_C204: u16 = 0xC204;

pub const SQ_ADDRS: [[u16; 2]; 4] = [
    [0xC010, 0xC011],
    [0xC014, 0xC015],
    [0xC018, 0xC019],
    [0xC01C, 0xC01D],
];

pub const PIX_X_OFF: i32 = 24;
pub const PIX_Y_OFF: i32 = 16;
pub const PIX_CELL: i32 = 8;

pub const PIECE_NAMES: [&'static str; 7] = ["I", "O", "T", "S", "Z", "L", "J"];

pub type Shape = [[i8; 2]; 4];

pub const SHAPES: [[Shape; 4]; 7] = [
    [
        [[0, 0], [0, 1], [0, 2], [0, 3]],
        [[0, 0], [1, 0], [2, 0], [3, 0]],
        [[0, 0], [0, 1], [0, 2], [0, 3]],
        [[0, 0], [1, 0], [2, 0], [3, 0]],
    ],
    [
        [[0, 0], [0, 1], [1, 0], [1, 1]],
        [[0, 0], [0, 1], [1, 0], [1, 1]],
        [[0, 0], [0, 1], [1, 0], [1, 1]],
        [[0, 0], [0, 1], [1, 0], [1, 1]],
    ],
    [
        [[0, 1], [1, 0], [1, 1], [1, 2]],
        [[0, 0], [1, 0], [1, 1], [2, 0]],
        [[0, 0], [0, 1], [0, 2], [1, 1]],
        [[0, 1], [1, 0], [1, 1], [2, 1]],
    ],
    [
        [[0, 1], [0, 2], [1, 0], [1, 1]],
        [[0, 0], [1, 0], [1, 1], [2, 1]],
        [[0, 1], [0, 2], [1, 0], [1, 1]],
        [[0, 0], [1, 0], [1, 1], [2, 1]],
    ],
    [
        [[0, 0], [0, 1], [1, 1], [1, 2]],
        [[0, 1], [1, 0], [1, 1], [2, 0]],
        [[0, 0], [0, 1], [1, 1], [1, 2]],
        [[0, 1], [1, 0], [1, 1], [2, 0]],
    ],
    [
        [[0, 2], [1, 0], [1, 1], [1, 2]],
        [[0, 0], [1, 0], [2, 0], [2, 1]],
        [[0, 0], [0, 1], [0, 2], [1, 0]],
        [[0, 0], [0, 1], [1, 1], [2, 1]],
    ],
    [
        [[0, 0], [1, 0], [1, 1], [1, 2]],
        [[0, 0], [0, 1], [1, 0], [2, 0]],
        [[0, 0], [0, 1], [0, 2], [1, 2]],
        [[0, 1], [1, 1], [2, 0], [2, 1]],
    ],
];

const ORI_TABLE: [(u8, usize); 7] = [
    (0x00, 5),
    (0x04, 6),
    (0x08, 0),
    (0x0C, 1),
    (0x10, 4),
    (0x14, 3),
    (0x18, 2),
];

pub fn ori_info(ori: u8) -> Option<(usize, u8)> {
    for &(spawn_ori, type_idx) in &ORI_TABLE {
        if ori >= spawn_ori && ori < spawn_ori + 4 {
            return Some((type_idx, ori - spawn_ori));
        }
    }
    None
}

#[inline]
pub fn popcount(x: u16) -> u32 {
    let mut x = x as u32;
    x = x - ((x >> 1) & 0x55555555);
    x = (x & 0x33333333) + ((x >> 2) & 0x33333333);
    x = (x + (x >> 4)) & 0x0f0f0f0f;
    x = x + (x >> 8);
    x = x + (x >> 16);
    x & 0x3f
}

#[inline]
pub fn is_occupied(v: u8) -> bool {
    ((v & 0xF0) == 0x80 && v != 0x8E) || v == 0x28
}

#[inline]
pub fn is_garbage_tile(v: u8) -> bool {
    v == 0x28
}

pub type Bitboard = [u16; BOARD_ROWS];

pub fn column_heights(bb: &Bitboard) -> [u8; BOARD_COLS] {
    let mut h = [0u8; BOARD_COLS];
    for col in 0..BOARD_COLS {
        for row in 0..BOARD_ROWS {
            if bb[row] & (1 << col) != 0 {
                h[col] = (BOARD_ROWS - row) as u8;
                break;
            }
        }
    }
    h
}