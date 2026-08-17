//! Soft-drop input: original Tetris.gb (and Rosy) only soft-drop on *pure* Down.
//! Holding Left/Right with Down is accepted by the joypad, but the ROM cancels
//! soft-drop via `and $B0 / cp $80` (must equal Down alone).

use gbmul_core::emulator::joypad::{GbButton, Joypad};

#[test]
fn joypad_reports_left_and_down_together() {
    let mut j = Joypad::new();
    j.press(GbButton::Left);
    j.press(GbButton::Down);
    j.write(0x20); // select d-pad
    let r = j.read();
    // Active-low: bit0=Right, bit1=Left, bit2=Up, bit3=Down
    assert_eq!(r & 0x0F, 0b0101, "Left+Down → bits 1 and 3 low, got {:04b}", r & 0x0F);
}

/// Tetris packs directions into the high nibble of $FF80 after poll:
/// bit7=Down, bit6=Up, bit5=Left, bit4=Right.
#[test]
fn tetris_soft_drop_mask_rejects_diagonal() {
    // Pure Down
    assert_eq!(0x80u8 & 0xB0, 0x80);
    // Left+Down
    assert_ne!(0xA0u8 & 0xB0, 0x80);
    // Right+Down
    assert_ne!(0x90u8 & 0xB0, 0x80);
    // Left only
    assert_ne!(0x20u8 & 0xB0, 0x80);
}
