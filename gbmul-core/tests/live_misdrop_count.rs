#![cfg(feature = "emulator_tests")]
use gbmul_core::bot::{ori_info, ADDR_CUR_ORI, TetrisBot};
use gbmul_core::emulator::joypad::GbButton;

fn apply(actions: &[(u8, bool)], emu: &mut gbmul_core::emulator::Emulator) {
    for &(btn, down) in actions {
        let b = match btn { 0=>GbButton::A,1=>GbButton::B,4=>GbButton::Up,5=>GbButton::Down,6=>GbButton::Left,7=>GbButton::Right,_=>continue };
        if down { emu.joypad.press(b); } else { emu.joypad.release(b); }
    }
}

fn tick(emu: &mut gbmul_core::emulator::Emulator, bot: &mut TetrisBot) {
    let (_, actions) = bot.tick(|a| emu.memory.read(a), |s,l| (0..l).map(|i| emu.memory.read(s.wrapping_add(i))).collect());
    apply(&actions, emu);
    emu.run_frame();
    bot.tick_post_frame(|a| emu.memory.read(a), |s,l| (0..l).map(|i| emu.memory.read(s.wrapping_add(i))).collect());
}

#[test]
fn live_play_first_ten_piece_locks_no_misdrop() {
    let rom = std::fs::read("../www/game.gb").expect("game.gb");
    let mut emu = gbmul_core::emulator::Emulator::new();
    emu.memory.load_rom(&rom);
    emu.running = true;
    let mut bot = TetrisBot::new();
    bot.set_pps(f64::INFINITY);
    bot.set_soft_drop_mode(true);
    bot.set_auto_menu_nav(true);
    for _ in 0..8_000 { tick(&mut emu, &mut bot); }
    let mut locks = 0u32;
    let mut last_ori = 0xffu8;
    for _ in 0..80_000 {
        tick(&mut emu, &mut bot);
        let ori = emu.memory.read(ADDR_CUR_ORI);
        if ori != last_ori && ori_info(ori).is_some() && last_ori != 0xff { locks += 1; }
        last_ori = ori;
        if locks >= 10 { break; }
    }
    let (mis, total) = bot.misdrop_stats();
    eprintln!("locks={locks} misdrops={mis} total_drops={total}");
    assert_eq!(mis, 0);
}
