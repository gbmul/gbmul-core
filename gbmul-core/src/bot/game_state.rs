//! High-level game state detection (menu / in-game / paused).

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameState {
    Splash,
    Title,
    Demo,
    SubmenuGametype,
    SubmenuLevel,
    InGame,
    Paused,
    GameOver,
    Win,
    Rocket,
    HighScoreEntry,
    /// 2P result cutscene — this side won the round; series not over yet.
    VsRoundWin,
    /// 2P result cutscene — this side lost the round; series not over yet.
    VsRoundLoss,
    /// 2P result cutscene — series finished (first to 4); this side won the match.
    VsMatchWin,
    /// 2P result cutscene — series finished (first to 4); this side lost the match.
    VsMatchLoss,
}

impl GameState {
    pub fn as_str(&self) -> &'static str {
        match self {
            GameState::Splash => "splash",
            GameState::Title => "title",
            GameState::Demo => "demo",
            GameState::SubmenuGametype => "submenu-gametype",
            GameState::SubmenuLevel => "submenu-level",
            GameState::InGame => "in-game",
            GameState::Paused => "paused",
            GameState::GameOver => "game-over",
            GameState::Win => "win",
            GameState::Rocket => "rocket",
            GameState::HighScoreEntry => "high-score-entry",
            GameState::VsRoundWin => "2p-round-win",
            GameState::VsRoundLoss => "2p-round-loss",
            GameState::VsMatchWin => "2p-match-win",
            GameState::VsMatchLoss => "2p-match-loss",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            GameState::Splash => "Splash",
            GameState::Title => "Title",
            GameState::Demo => "Demo",
            GameState::SubmenuGametype => "Game Type",
            GameState::SubmenuLevel => "Level Select",
            GameState::InGame => "In Game",
            GameState::Paused => "Paused",
            GameState::GameOver => "Game Over",
            GameState::Win => "Type-B Win!",
            GameState::Rocket => "Rocket!",
            GameState::HighScoreEntry => "High Score Entry",
            GameState::VsRoundWin => "2P Round Win",
            GameState::VsRoundLoss => "2P Round Loss",
            GameState::VsMatchWin => "2P Game Win",
            GameState::VsMatchLoss => "2P Game Loss",
        }
    }

    /// True for any 2P Mario/Luigi result cutscene (round or full match).
    pub fn is_vs_result(self) -> bool {
        matches!(
            self,
            GameState::VsRoundWin
                | GameState::VsRoundLoss
                | GameState::VsMatchWin
                | GameState::VsMatchLoss
        )
    }
}

pub const PROBE_C000: u16 = 0xC000;
pub const PROBE_C001: u16 = 0xC001;
pub const PROBE_C201: u16 = 0xC201;
pub const PROBE_MENU_PHASE: u16 = 0xFFE1;
pub const PROBE_INGAME_ADDR: u16 = 0xC204;
pub const PROBE_INGAME_MASK: u8 = 0x80;
pub const PROBE_PAUSE_ADDR: u16 = 0xCFFC;
pub const PROBE_VRAM_GO_ADDR: u16 = 0x9885;
pub const PROBE_VRAM_GO_TILE: u8 = 0x10;
pub const PROBE_VRAM_WIN_TILE: u8 = 0x26;
pub const PROBE_VRAM_TOPOUT_LO: u8 = 0x87; // solid-grid top-out artefact (not post-game)
pub const PROBE_VRAM_TOPOUT_HI: u8 = 0x88;
pub const PROBE_SCOREBOARD_C001: u8 = 0x50; // post-GO scoreboard (level select uses 0x30)
pub const MENU_HRAM_POSTGAME_NAME: u8 = 0x32; // E1 ≥ 0x32 → 3-letter name entry (empirical)
/// Multiplayer / link-cable mode flag (Data Crystal: multiplayer).
pub const PROBE_MP_FLAG: u16 = 0xFFC5;
/// 2P series: this side's win count (0–4 UI boxes; RAM may briefly read 5).
pub const PROBE_MP_WINS: u16 = 0xFFD7;
/// 2P series: opponent's win count.
pub const PROBE_MP_LOSSES: u16 = 0xFFD8;
/// First-to-N match length (Tetris GB has 4 score boxes per player).
pub const VS_MATCH_WINS: u8 = 4;
/// 2P result cutscene: this GB won the round (live probe, Mario celebrate).
pub const E1_VS_RESULT_WIN: u8 = 0x20;
/// 2P result cutscene: this GB lost the round (live probe, Luigi cry).
pub const E1_VS_RESULT_LOSS: u8 = 0x21;

fn menu_phase_is_level(e1: u8) -> bool {
    e1 == 0x11 || e1 == 0x13
}

fn menu_phase_is_high(e1: u8) -> bool {
    e1 == 0x14 || e1 == 0x15
}

fn is_topout_grid_fill(vram9885: u8) -> bool {
    vram9885 == PROBE_VRAM_TOPOUT_LO || vram9885 == PROBE_VRAM_TOPOUT_HI
}

fn is_post_game_scoreboard_family(read_mem: &impl Fn(u16) -> u8) -> bool {
    (read_mem(PROBE_INGAME_ADDR) & PROBE_INGAME_MASK) == 0
        && read_mem(PROBE_C001) == PROBE_SCOREBOARD_C001
}

/// Series is over when either side has reached first-to-4.
///
/// Live note: after the 4th point RAM can read 5 (e.g. 0–5) while VRAM still
/// shows only 4 face boxes — not an off-by-one in our detector; threshold is ≥4.
pub fn vs_match_over(wins: u8, losses: u8) -> bool {
    wins >= VS_MATCH_WINS || losses >= VS_MATCH_WINS
}

pub fn detect_game_state(read_mem: impl Fn(u16) -> u8) -> GameState {
    let c000 = read_mem(PROBE_C000);
    let c201 = read_mem(PROBE_C201);
    let e1 = read_mem(PROBE_MENU_PHASE);

    // Game-type screen (C000=0x70) before E1 — HRAM phase can persist in SRAM after B play.
    if c201 != 0 && c000 == 0x70 {
        return GameState::SubmenuGametype;
    }

    if c000 == 0x80 {
        return GameState::Title;
    }

    // Type-B win after C204 clears: C201=0x58 only valid with win-screen VRAM/CFFC.
    if c201 == 0x58 && (read_mem(PROBE_INGAME_ADDR) & PROBE_INGAME_MASK) == 0 {
        let vram9885 = read_mem(PROBE_VRAM_GO_ADDR);
        let cffc = read_mem(PROBE_PAUSE_ADDR);
        if vram9885 == PROBE_VRAM_WIN_TILE || cffc == 0x05 {
            return GameState::Win;
        }
    }

    // 2P Mario/Luigi result cutscene.
    // Live probes (2026-07): C204 often still set (would fall through as InGame);
    // FFC5=1 (multiplayer); E1=0x20 this side won, E1=0x21 this side lost.
    // Match vs round: FFD7/FFD8 series scores — first to 4 (RAM may show 5).
    // Cost: a few HRAM bytes (E1 already read). Must run before InGame.
    if read_mem(PROBE_MP_FLAG) == 1 {
        let match_over = vs_match_over(read_mem(PROBE_MP_WINS), read_mem(PROBE_MP_LOSSES));
        if e1 == E1_VS_RESULT_WIN {
            return if match_over {
                GameState::VsMatchWin
            } else {
                GameState::VsRoundWin
            };
        }
        if e1 == E1_VS_RESULT_LOSS {
            return if match_over {
                GameState::VsMatchLoss
            } else {
                GameState::VsRoundLoss
            };
        }
    }

    // C204 play family before E1 — menu HRAM phase persists in SRAM after B-type.
    if (read_mem(PROBE_INGAME_ADDR) & PROBE_INGAME_MASK) != 0 {
        let cffc = read_mem(PROBE_PAUSE_ADDR);
        let vram9885 = read_mem(PROBE_VRAM_GO_ADDR);

        if cffc == 0x01 {
            return GameState::Paused;
        }
        if vram9885 == PROBE_VRAM_GO_TILE {
            return GameState::GameOver;
        }
        // VRAM 0x26 only — C201=0x58 is piece Y during play, not a win probe.
        if vram9885 == PROBE_VRAM_WIN_TILE {
            return GameState::Win;
        }
        if c201 == 0x40 {
            return GameState::Demo;
        }
        return GameState::InGame;
    }

    if c000 == 0 && c201 == 0 {
        return GameState::Splash;
    }

    let vram9885 = read_mem(PROBE_VRAM_GO_ADDR);
    // Top-out grid fill can clear C204 before GAME OVER — not rocket/name entry.
    if (read_mem(PROBE_INGAME_ADDR) & PROBE_INGAME_MASK) == 0 && is_topout_grid_fill(vram9885) {
        return GameState::InGame;
    }

    // Type-A post game-over: C001=0x50 (level select uses 0x30).
    // Rocket: E1=0x2F–0x31. Name entry keyboard: E1≥0x32 (live probe: C000=0x52 C201=0x6A).
    if is_post_game_scoreboard_family(&read_mem)
        && !menu_phase_is_level(e1)
        && !menu_phase_is_high(e1)
    {
        if e1 < MENU_HRAM_POSTGAME_NAME {
            return GameState::Rocket;
        }
        return GameState::HighScoreEntry;
    }

    if menu_phase_is_level(e1) || menu_phase_is_high(e1) {
        return GameState::SubmenuLevel;
    }
    if c201 != 0 && read_mem(PROBE_C001) != PROBE_SCOREBOARD_C001 {
        return GameState::SubmenuLevel;
    }
    // C201 briefly clears during level-select cursor blink.
    if c000 == 0xFF || c000 == 0x40 || c000 == 0x50 {
        return GameState::SubmenuLevel;
    }
    GameState::Title
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn mem(map: &[(u16, u8)]) -> impl Fn(u16) -> u8 {
        let h: HashMap<u16, u8> = map.iter().copied().collect();
        move |a: u16| *h.get(&a).unwrap_or(&0)
    }

    #[test]
    fn topout_grid_fill_is_in_game_not_rocket() {
        let read = mem(&[
            (PROBE_C000, 0x00),
            (PROBE_C001, 0x50),
            (PROBE_C201, 0x10),
            (PROBE_INGAME_ADDR, 0x00),
            (PROBE_VRAM_GO_ADDR, PROBE_VRAM_TOPOUT_HI),
            (PROBE_MENU_PHASE, 0x2F),
        ]);
        assert_eq!(detect_game_state(read), GameState::InGame);
    }

    #[test]
    fn rocket_after_game_over() {
        let read = mem(&[
            (PROBE_C000, 0x00),
            (PROBE_C001, PROBE_SCOREBOARD_C001),
            (PROBE_C201, 0x10),
            (PROBE_INGAME_ADDR, 0x00),
            (PROBE_VRAM_GO_ADDR, 0x2F),
            (PROBE_MENU_PHASE, 0x2F),
        ]);
        assert_eq!(detect_game_state(read), GameState::Rocket);
    }

    #[test]
    fn high_score_entry_on_scoreboard_family() {
        let read = mem(&[
            (PROBE_C000, 0x52),
            (PROBE_C001, PROBE_SCOREBOARD_C001),
            (PROBE_C201, 0x6A),
            (PROBE_INGAME_ADDR, 0x00),
            (PROBE_VRAM_GO_ADDR, 0x2F),
            (PROBE_MENU_PHASE, 0x32),
        ]);
        assert_eq!(detect_game_state(read), GameState::HighScoreEntry);
    }

    #[test]
    fn level_select_not_high_score_entry() {
        let read = mem(&[
            (PROBE_C000, 0xFF),
            (PROBE_C001, 0x30),
            (PROBE_C201, 0x40),
            (PROBE_INGAME_ADDR, 0x00),
            (PROBE_VRAM_GO_ADDR, 0x2F),
            (PROBE_MENU_PHASE, 0x11),
        ]);
        assert_eq!(detect_game_state(read), GameState::SubmenuLevel);
    }

    /// Mid-series loss: E1=0x21, scores 1–2 → round loss (not match).
    #[test]
    fn vs_round_loss() {
        let read = mem(&[
            (PROBE_C000, 0x40),
            (PROBE_C001, 0x59),
            (PROBE_C201, 0x50),
            (PROBE_INGAME_ADDR, 0x80),
            (PROBE_VRAM_GO_ADDR, 0x2F),
            (PROBE_MENU_PHASE, E1_VS_RESULT_LOSS),
            (PROBE_MP_FLAG, 1),
            (PROBE_MP_WINS, 1),
            (PROBE_MP_LOSSES, 2),
            (PROBE_PAUSE_ADDR, 0x00),
        ]);
        assert_eq!(detect_game_state(read), GameState::VsRoundLoss);
    }

    /// Mid-series win: E1=0x20, scores 2–1 → round win.
    #[test]
    fn vs_round_win() {
        let read = mem(&[
            (PROBE_C000, 0x40),
            (PROBE_C001, 0x59),
            (PROBE_C201, 0x50),
            (PROBE_INGAME_ADDR, 0x80),
            (PROBE_VRAM_GO_ADDR, 0x2F),
            (PROBE_MENU_PHASE, E1_VS_RESULT_WIN),
            (PROBE_MP_FLAG, 1),
            (PROBE_MP_WINS, 2),
            (PROBE_MP_LOSSES, 1),
            (PROBE_PAUSE_ADDR, 0x00),
        ]);
        assert_eq!(detect_game_state(read), GameState::VsRoundWin);
    }

    /// Live match-loss sample: A 0–5 (VRAM shows 4 boxes; RAM can be 5).
    #[test]
    fn vs_match_loss_when_score_ge_4() {
        let read = mem(&[
            (PROBE_C000, 0x50),
            (PROBE_C001, 0x51),
            (PROBE_C201, 0x60),
            (PROBE_INGAME_ADDR, 0x80),
            (PROBE_VRAM_GO_ADDR, 0x2F),
            (PROBE_MENU_PHASE, E1_VS_RESULT_LOSS),
            (PROBE_MP_FLAG, 1),
            (PROBE_MP_WINS, 0),
            (PROBE_MP_LOSSES, 5),
            (PROBE_PAUSE_ADDR, 0x00),
        ]);
        assert_eq!(detect_game_state(read), GameState::VsMatchLoss);
    }

    /// First-to-4 win: wins == 4.
    #[test]
    fn vs_match_win_at_exactly_4() {
        let read = mem(&[
            (PROBE_C000, 0x40),
            (PROBE_C001, 0x59),
            (PROBE_C201, 0x50),
            (PROBE_INGAME_ADDR, 0x80),
            (PROBE_VRAM_GO_ADDR, 0x2F),
            (PROBE_MENU_PHASE, E1_VS_RESULT_WIN),
            (PROBE_MP_FLAG, 1),
            (PROBE_MP_WINS, 4),
            (PROBE_MP_LOSSES, 2),
            (PROBE_PAUSE_ADDR, 0x00),
        ]);
        assert_eq!(detect_game_state(read), GameState::VsMatchWin);
    }

    #[test]
    fn vs_result_requires_mp_flag() {
        let read = mem(&[
            (PROBE_C000, 0x40),
            (PROBE_C001, 0x59),
            (PROBE_C201, 0x50),
            (PROBE_INGAME_ADDR, 0x80),
            (PROBE_VRAM_GO_ADDR, 0x2F),
            (PROBE_MENU_PHASE, E1_VS_RESULT_WIN),
            (PROBE_MP_FLAG, 0),
            (PROBE_PAUSE_ADDR, 0x00),
        ]);
        assert_eq!(detect_game_state(read), GameState::InGame);
    }
}
