// Color palettes – no SDL2 dependency; colors are plain (R, G, B) tuples.

/// A palette is 4 shades from lightest (0) to darkest (3).
pub type Palette = [(u8, u8, u8); 4];

pub const PALETTES: [Palette; 5] = [
    // 0 – Default lighter grayscale
    [
        (255, 255, 255),
        (221, 221, 221),
        (170, 170, 170),
        (119, 119, 119),
    ],
    // 1 – Grayscale
    [
        (255, 255, 255),
        (170, 170, 170),
        (85,  85,  85),
        (0,   0,   0),
    ],
    // 2 – Autumn
    [
        (255, 246, 211),
        (255, 183, 119),
        (170, 85,  59),
        (85,  34,  0),
    ],
    // 3 – Ice Blue
    [
        (224, 248, 255),
        (139, 207, 230),
        (68,  137, 172),
        (27,  54,  68),
    ],
    // 4 – Cherry
    [
        (255, 224, 230),
        (255, 140, 158),
        (170, 56,  74),
        (85,  0,   19),
    ],
];

pub fn map_color(shade: u8, palette_index: usize) -> (u8, u8, u8) {
    PALETTES[palette_index % PALETTES.len()][(shade & 0x03) as usize]
}
