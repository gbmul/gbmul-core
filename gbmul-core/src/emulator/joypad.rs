// Joypad input handler – no SDL2 dependency in WASM build.
// GbButton is defined here and used by lib.rs via map_button().

use bitflags::bitflags;

#[derive(Debug, Clone, Copy)]
pub enum GbButton {
    A,
    B,
    Select,
    Start,
    Up,
    Down,
    Left,
    Right,
}

bitflags! {
    pub struct JoypadState: u8 {
        const A      = 0b0000_0001;
        const B      = 0b0000_0010;
        const SELECT = 0b0000_0100;
        const START  = 0b0000_1000;
        const RIGHT  = 0b0001_0000;
        const LEFT   = 0b0010_0000;
        const UP     = 0b0100_0000;
        const DOWN   = 0b1000_0000;
    }
}

pub struct Joypad {
    state: JoypadState,
    prev_state: JoypadState,
    select_buttons: bool,
    select_directions: bool,
}

impl Joypad {
    pub fn new() -> Self {
        Joypad {
            state: JoypadState::empty(),
            prev_state: JoypadState::empty(),
            select_buttons: true,
            select_directions: true,
        }
    }

    pub fn press(&mut self, button: GbButton) {
        let flag = Self::to_flag(button);
        self.state.insert(flag);
    }

    pub fn release(&mut self, button: GbButton) {
        let flag = Self::to_flag(button);
        self.state.remove(flag);
    }

    /// Set the entire button state from a bit mask using the lib.rs button-index
    /// order (bit 0 = A, 1 = B, 2 = Select, 3 = Start, 4 = Up, 5 = Down,
    /// 6 = Left, 7 = Right). Used by the lockstep netplay loop to apply a frame's
    /// synchronized input in one call.
    pub fn set_from_mask(&mut self, mask: u8) {
        let mut s = JoypadState::empty();
        if mask & (1 << 0) != 0 { s.insert(JoypadState::A); }
        if mask & (1 << 1) != 0 { s.insert(JoypadState::B); }
        if mask & (1 << 2) != 0 { s.insert(JoypadState::SELECT); }
        if mask & (1 << 3) != 0 { s.insert(JoypadState::START); }
        if mask & (1 << 4) != 0 { s.insert(JoypadState::UP); }
        if mask & (1 << 5) != 0 { s.insert(JoypadState::DOWN); }
        if mask & (1 << 6) != 0 { s.insert(JoypadState::LEFT); }
        if mask & (1 << 7) != 0 { s.insert(JoypadState::RIGHT); }
        self.state = s;
    }

    /// Returns true if any button transitioned from not-pressed to pressed since last update_prev().
    pub fn has_new_press(&self) -> bool {
        self.state.bits() & !self.prev_state.bits() != 0
    }

    /// Snapshot current state so next call to has_new_press() detects new transitions.
    pub fn update_prev(&mut self) {
        self.prev_state = JoypadState::from_bits_truncate(self.state.bits());
    }

    fn to_flag(button: GbButton) -> JoypadState {
        match button {
            GbButton::A      => JoypadState::A,
            GbButton::B      => JoypadState::B,
            GbButton::Select => JoypadState::SELECT,
            GbButton::Start  => JoypadState::START,
            GbButton::Up     => JoypadState::UP,
            GbButton::Down   => JoypadState::DOWN,
            GbButton::Left   => JoypadState::LEFT,
            GbButton::Right  => JoypadState::RIGHT,
        }
    }

    /// Read joypad register (0xFF00).
    pub fn read(&self) -> u8 {
        let mut result = 0xC0;
        if !self.select_buttons    { result |= 0x20; }
        if !self.select_directions { result |= 0x10; }

        let button_bits = if self.select_buttons {
            let mut bits = 0x0F;
            if self.state.contains(JoypadState::START)  { bits &= !0x08; }
            if self.state.contains(JoypadState::SELECT) { bits &= !0x04; }
            if self.state.contains(JoypadState::B)      { bits &= !0x02; }
            if self.state.contains(JoypadState::A)      { bits &= !0x01; }
            bits
        } else if self.select_directions {
            let mut bits = 0x0F;
            if self.state.contains(JoypadState::DOWN)   { bits &= !0x08; }
            if self.state.contains(JoypadState::UP)     { bits &= !0x04; }
            if self.state.contains(JoypadState::LEFT)   { bits &= !0x02; }
            if self.state.contains(JoypadState::RIGHT)  { bits &= !0x01; }
            bits
        } else {
            0x0F
        };

        result | button_bits
    }

    /// Write to joypad register (selects which group of buttons to read).
    pub fn write(&mut self, value: u8) {
        self.select_buttons    = (value & 0x20) == 0;
        self.select_directions = (value & 0x10) == 0;
    }
}
