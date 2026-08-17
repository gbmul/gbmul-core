use gbmul_core::emulator::Emulator;
use gbmul_core::state::EmulatorState;

macro_rules! log_ts {
    ($($arg:tt)*) => {{
        let now = chrono::Local::now();
        let msg = format!("[{}] {}", now.format("%Y-%m-%d %H:%M:%S"), format!($($arg)*));
        // Direct write + flush ONLY to file. Never touch stdout to avoid "broken pipe" panics
        // caused by the launcher's `exec > >(tee ...)` redirection + process substitution.
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("log.txt")
        {
            use std::io::Write;
            let _ = writeln!(f, "{}", msg);
            let _ = f.flush();
        }
    }};
}

fn main() {
    env_logger::init();

    let debug_mode = std::env::var_os("GBMUL_DEBUG").is_some();
    if debug_mode {
        log_ts!("[DEBUG] GBMUL_DEBUG enabled - verbose logging active");
    }

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: gbmul-sdl2 <rom.gb> [frames]");
        std::process::exit(1);
    }

    let rom_path = args[1].clone();
    let rom = std::fs::read(&rom_path).expect("Failed to read ROM");
    log_ts!("GBMUL build {} — ROM: {} ({} bytes)", BUILD_ID, rom_path, rom.len());

    #[cfg(feature = "sdl2")]
    run_sdl2(&rom_path, &rom);

    #[cfg(not(feature = "sdl2"))]
    run_bench(&rom, args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300));
}

// ---------------------------------------------------------------------------
// Headless benchmark
// ---------------------------------------------------------------------------

#[cfg(not(feature = "sdl2"))]
fn run_bench(rom: &[u8], frames: u32) {
    use std::time::Instant;
    let mut emu = Emulator::new();
    emu.load_rom(rom).expect("Failed to load ROM");
    let t0 = Instant::now();
    for _ in 0..frames {
        emu.run_frame();
    }
    let elapsed = t0.elapsed();
    println!(
        "{} frames in {:.2}s → {:.1} fps (target 59.7)",
        frames, elapsed.as_secs_f64(), frames as f64 / elapsed.as_secs_f64()
    );
}

// ---------------------------------------------------------------------------
// Menu
// ---------------------------------------------------------------------------

#[cfg(feature = "sdl2")]
const MAIN_MENU: &[&str] = &["SAVE", "LOAD", "RESET", "A.LOAD", "A.SAVE", "MISC", "QUIT"];
const MISC_MENU: &[&str] = &["FPS", "SHOT", "PALETTE", "BOT", "SOUND", "ABOUT", "BACK"];
const ABOUT_MENU: &[&str] = &["BACK"];
/// Compile-time build id from `build.rs` (`YYYYMMDDHHMM` local).
const BUILD_ID: &str = env!("BUILD_ID");

// Game state probes (shared with web Rust bot)
const PROBE_C000: u16 = 0xC000;
const PROBE_C201: u16 = 0xC201;
const PROBE_INGAME_ADDR: u16 = 0xC204;
const PROBE_INGAME_MASK: u8 = 0x80;
const PROBE_PAUSE_ADDR: u16 = 0xCFFC;
const PROBE_VRAM_GO_ADDR: u16 = 0x9885;
const PROBE_VRAM_GO_TILE: u8 = 0x10;
const PROBE_VRAM_WIN_TILE: u8 = 0x26;

fn detect_game_state(emu: &Emulator) -> &'static str {
  let c000 = emu.memory.read(PROBE_C000);
  let c201 = emu.memory.read(PROBE_C201);
  if c000 == 0x80 { return "title"; }
  if (emu.memory.read(PROBE_INGAME_ADDR) & PROBE_INGAME_MASK) != 0 {
    let cffc = emu.memory.read(PROBE_PAUSE_ADDR);
    let vram = emu.memory.read(PROBE_VRAM_GO_ADDR);
    if cffc == 0x01 { return "paused"; }
    if vram == PROBE_VRAM_GO_TILE { return "game-over"; }
    if vram == PROBE_VRAM_WIN_TILE { return "win"; }
    if c201 == 0x40 { return "demo"; }
    return "in-game";
  }
  if c000 == 0 && c201 == 0 { return "splash"; }
  if c201 != 0 {
    if c000 == 0x70 { return "submenu-gametype"; }
    return "submenu-level";
  }
  "title"
}

#[cfg(feature = "sdl2")]
struct Menu {
    open: bool,    // logical: game paused, input captured
    slide_x: f32, // visual: animates toward open or closed target
    selected: usize,
    current_menu: u8, // 0=main, 1=misc, 2=about
}

#[cfg(feature = "sdl2")]
impl Menu {
    fn new() -> Self {
        Menu { open: false, slide_x: 0.0, selected: 0, current_menu: 0 }
    }

    fn show(&mut self, win_w: u32) {
        if !self.open {
            self.slide_x = win_w as f32; // start from off-screen right
            self.current_menu = 0;
        }
        self.open = true;
        self.selected = 0;
    }

    fn hide(&mut self) {
        self.open = false;
        self.current_menu = 0;
        self.selected = 0;
        // slide_x will animate back to win_w on next update()
    }

    fn current_items(&self) -> &'static [&'static str] {
        match self.current_menu {
            1 => MISC_MENU,
            2 => ABOUT_MENU,
            _ => MAIN_MENU,
        }
    }

    /// Pop one menu level (about→misc, misc→main, main→close). Returns true if menu closed.
    fn nav_back(&mut self) -> bool {
        match self.current_menu {
            2 => {
                self.current_menu = 1;
                // index of ABOUT in MISC_MENU
                self.selected = MISC_MENU.iter().position(|&s| s == "ABOUT").unwrap_or(0);
                false
            }
            1 => {
                self.current_menu = 0;
                self.selected = 5; // MISC in main
                false
            }
            _ => {
                self.hide();
                true
            }
        }
    }

    // target_open = win_w - panel_w, target_closed = win_w
    fn update(&mut self, target_open: f32, win_w: f32) {
        let target = if self.open { target_open } else { win_w };
        let diff = target - self.slide_x;

        if diff.abs() > 0.2 {
            // Non-linear ease-out: stronger movement when far from target,
            // then progressively decelerates as we get close (curvy feel).
            let dist = diff.abs();
            let k = 0.15 + (dist / 380.0).clamp(0.0, 0.13);
            self.slide_x += diff * k;

            // Snap when very close to avoid micro-movement / jitter
            if (target - self.slide_x).abs() < 0.2 {
                self.slide_x = target;
            }
        } else {
            self.slide_x = target;
        }
    }

    fn is_visible(&self, win_w: u32) -> bool {
        self.slide_x < win_w as f32 - 1.0
    }

    fn nav_up(&mut self) {
        let len = self.current_items().len();
        if self.selected > 0 {
            self.selected -= 1;
        } else {
            self.selected = len - 1;
        }
    }

    fn nav_down(&mut self) {
        let len = self.current_items().len();
        if self.selected + 1 < len {
            self.selected += 1;
        } else {
            self.selected = 0;
        }
    }
}

// ---------------------------------------------------------------------------
// SDL2 display + input
// ---------------------------------------------------------------------------

#[cfg(feature = "sdl2")]
fn run_sdl2(rom_path: &str, rom: &[u8]) {
    use gbmul_core::utils::palette::PALETTES;
    use sdl2::audio::{AudioQueue, AudioSpecDesired};
    use sdl2::event::Event;
    use sdl2::pixels::{Color, PixelFormatEnum};
    use sdl2::rect::Rect;
    use sdl2::render::BlendMode;
    use std::time::{Duration, Instant};

    const GB_W: u32 = 160;
    const GB_H: u32 = 144;
    const FRAME_NS: u64 = 16_742_706; // 59.7275 fps

    let sdl = sdl2::init().expect("SDL2 init failed");
    let video = sdl.video().expect("SDL2 video init failed");

    let mut play_sound = load_play_sound();

    let audio_subsystem = sdl.audio().expect("SDL2 audio init failed");
    let desired_spec = AudioSpecDesired {
        freq: Some(44100),
        channels: Some(2),
        samples: Some(512),
    };
    let audio_queue: AudioQueue<f32> = audio_subsystem
        .open_queue(None, &desired_spec)
        .expect("Failed to open audio queue");
    audio_queue.resume();
    log_ts!("[AUDIO] SDL2 audio queue ready @ 44100 Hz stereo, buffer 512");
    if !play_sound {
        log_ts!("[AUDIO] Sound disabled (APU emulation skipped)");
    }

    let window = video
        .window("GBmul", GB_W * 3, GB_H * 3)
        .fullscreen_desktop()
        .build()
        .expect("Window creation failed");

    let mut canvas = window
        .into_canvas()
        .present_vsync()
        .build()
        .expect("Canvas creation failed");

    canvas.set_blend_mode(BlendMode::Blend);

    // Ensure panics are logged with location even on crash
    std::panic::set_hook(Box::new(|info| {
        log_ts!("!!! PANIC: {}", info);
    }));

    let tc = canvas.texture_creator();
    let mut tex = tc
        .create_texture_streaming(PixelFormatEnum::RGB24, GB_W, GB_H)
        .expect("Texture creation failed");

    // Cute TERM font is now fully embedded as bitmaps (see glyph() below).
    // No PNG, no image crate at runtime.

    let gc_sys = sdl.game_controller().ok();
    let _controller = gc_sys.as_ref().and_then(|gc| {
        (0..gc.num_joysticks().unwrap_or(0)).find_map(|i| gc.open(i).ok())
    });

    let mut emu = Emulator::new();
    emu.set_audio_enabled(play_sound);
    emu.load_rom(rom).expect("Failed to load ROM");

    // Discard any samples generated during the RNG warmup in load_rom
    let _ = std::mem::take(&mut emu.apu.sample_buffer);

    // Auto-restore state on launch if enabled
    let mut auto_restore = load_auto_restore();
    let mut auto_save = load_auto_save();
    let mut show_fps = load_show_fps();
    let mut palette_index: usize = load_palette();
    let mut bot_enabled = load_bot();
    let mut bot_controller: Option<gbmul_core::bot::TetrisBot> = if bot_enabled {
        Some(gbmul_core::bot::TetrisBot::new())
    } else {
        None
    };
    log_ts!(
        "[BOT] enabled={} (human input still accepted when on)",
        bot_enabled
    );
    if auto_restore {
        let state_file = get_state_path(rom_path);
        log_ts!("[AUTO] Attempting auto-restore from {}", state_file);
        match std::fs::read(&state_file) {
            Ok(bytes) => {
                log_ts!("[AUTO] Read {} bytes from state file", bytes.len());
                match bincode::deserialize::<EmulatorState>(&bytes) {
                    Ok(state) => {
                        // Safe restore with length checks
                        if state.vram.len() == emu.memory.vram.len() &&
                           state.eram.len() == emu.memory.eram.len() &&
                           state.ram.len() == emu.memory.ram.len() {
                            emu.restore_state(&state);
                            log_ts!("[AUTO] Successfully auto-restored state");
                        } else {
                            log_ts!("[AUTO] State size mismatch, skipping restore");
                        }
                    }
                    Err(e) => log_ts!("[AUTO] Deserialize failed: {}", e),
                }
            }
            Err(e) => log_ts!("[AUTO] No state file or read error: {}", e),
        }
    } else {
        log_ts!("[AUTO] Auto-restore disabled");
    }

    let mut event_pump = sdl.event_pump().expect("Event pump failed");
    let mut menu = Menu::new();

    // FPS tracking
    let mut frame_count: u32 = 0;
    let mut fps_last = Instant::now();
    let mut fps: u32 = 0;

    let mut logged_first_audio = false;
    let mut last_bot_st: &str = "";
    let mut last_ori: u8 = 0xff;

    'main: loop {
        let t0 = Instant::now();

        let (win_w, win_h) = canvas.output_size().unwrap();
        let panel_w = win_w / 3;
        let target_open = (win_w - panel_w) as f32;

        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => break 'main,

                Event::KeyDown { keycode: Some(k), repeat: false, .. } => {
                    use sdl2::keyboard::Keycode;
                    match k {
                        Keycode::Escape | Keycode::M => {
                            if menu.open {
                                menu.nav_back();
                            } else { menu.show(win_w); }
                            log_ts!("[MENU] Toggled via key, open={}", menu.open);
                        }
                        _ if menu.open => {
                            log_ts!("[MENU] Key while menu open: {:?}", k);
                            match k {
                                Keycode::Up         => menu.nav_up(),
                                Keycode::Down       => menu.nav_down(),
                                Keycode::Return | Keycode::Z => {
                                    if handle_menu_selection(&mut menu, &mut emu, &mut auto_restore, &mut auto_save, &mut show_fps, &mut palette_index, &mut bot_enabled, &mut bot_controller, &mut play_sound, rom_path) { break 'main; }
                                }
                                _ => {}
                            }
                        },
                        _ => {
                            // Human input always reaches the joypad (same as web).
                            // Bot may also drive buttons; last write per frame wins for shared keys.
                            if let Some(btn) = key_to_gb(k) { emu.joypad.press(btn); }
                        }
                    }
                }

                Event::KeyUp { keycode: Some(k), .. } => {
                    if !menu.open {
                        if let Some(btn) = key_to_gb(k) { emu.joypad.release(btn); }
                    }
                }

                Event::ControllerButtonDown { button, .. } => {
                    use sdl2::controller::Button;
                    match button {
                        Button::Guide => {
                            if menu.open { menu.hide(); } else { menu.show(win_w); }
                            log_ts!("[MENU] Toggled via controller Guide, open={}", menu.open);
                        }
                        _ if menu.open => {
                            log_ts!("[MENU] Controller button while menu open: {:?}", button);
                            match button {
                                Button::DPadUp   => menu.nav_up(),
                                Button::DPadDown => menu.nav_down(),
                                Button::B | Button::Start => {
                                    if handle_menu_selection(&mut menu, &mut emu, &mut auto_restore, &mut auto_save, &mut show_fps, &mut palette_index, &mut bot_enabled, &mut bot_controller, &mut play_sound, rom_path) { break 'main; }
                                }
                                Button::A => {
                                    menu.nav_back();
                                }
                                _ => {}
                            }
                        },
                        _ => {
                            // Human pad input always accepted while bot is on (matches web).
                            if let Some(btn) = pad_to_gb(button) { emu.joypad.press(btn); }
                        }
                    }
                }

                Event::ControllerButtonUp { button, .. } => {
                    if !menu.open {
                        if let Some(btn) = pad_to_gb(button) { emu.joypad.release(btn); }
                    }
                }

                Event::ControllerAxisMotion { axis, value, .. } => {
                    if menu.open {
                        log_ts!("[MENU] Axis while menu open: {:?} = {}", axis, value);
                        // Many handhelds report dpad via axes (try common vertical axes)
                        if value < -8000 {
                            menu.nav_up();
                        } else if value > 8000 {
                            menu.nav_down();
                        }
                    } else {
                        // normal game input from axes if needed (left stick etc.)
                    }
                }

                _ => {}
            }
        }

        if bot_enabled {
            let st = detect_game_state(&emu);
            if st != last_bot_st {
                log_ts!("[BOT] state: {}", st);
                last_bot_st = st;
            }
        }

        if !menu.open {
            emu.run_frame();
            let samples: Vec<f32> = std::mem::take(&mut emu.apu.sample_buffer);
            if play_sound && !samples.is_empty() {
                if !logged_first_audio {
                    log_ts!("[AUDIO] draining first {} samples (~{} stereo pairs)", samples.len(), samples.len() / 2);
                    logged_first_audio = true;
                }
                if let Err(e) = audio_queue.queue_audio(&samples) {
                    log_ts!("[AUDIO] queue_audio error (dropped?): {}", e);
                }
            }
            if bot_enabled {
                // log bot data on piece change (ori change)
                let ori = emu.memory.read(0xC203);
                if ori != last_ori {
                    last_ori = ori;
                    let y1 = emu.memory.read(0xC010) as i32;
                    let x1 = emu.memory.read(0xC011) as i32;
                    let y2 = emu.memory.read(0xC014) as i32;
                    let x2 = emu.memory.read(0xC015) as i32;
                    let y3 = emu.memory.read(0xC018) as i32;
                    let x3 = emu.memory.read(0xC019) as i32;
                    let y4 = emu.memory.read(0xC01C) as i32;
                    let x4 = emu.memory.read(0xC01D) as i32;
                    let r1 = (y1 - 16) / 8;
                    let c1 = (x1 - 24) / 8;
                    let r2 = (y2 - 16) / 8;
                    let c2 = (x2 - 24) / 8;
                    let r3 = (y3 - 16) / 8;
                    let c3 = (x3 - 24) / 8;
                    let r4 = (y4 - 16) / 8;
                    let c4 = (x4 - 24) / 8;
                    log_ts!("[BOT DATA] ori=0x{:02x} piece=[({},{}) ({},{}) ({},{}) ({},{})]", ori, r1, c1, r2, c2, r3, c3, r4, c4);
                }
            }
        }

        // --- Bot control (drives joypad when enabled) ---
        if bot_enabled {
            if bot_controller.is_none() {
                bot_controller = Some(gbmul_core::bot::TetrisBot::new());
            }
            if let Some(b) = &mut bot_controller {
                let ( _gs, actions ) = b.tick(
                    |addr| emu.memory.read(addr),
                    |start, len| (0..len).map(|i| emu.memory.read(start.wrapping_add(i))).collect(),
                );
                for (btn, is_down) in actions {
                    let gb_btn = match btn {
                        0 => gbmul_core::emulator::joypad::GbButton::A,
                        1 => gbmul_core::emulator::joypad::GbButton::B,
                        2 => gbmul_core::emulator::joypad::GbButton::Select,
                        3 => gbmul_core::emulator::joypad::GbButton::Start,
                        4 => gbmul_core::emulator::joypad::GbButton::Up,
                        5 => gbmul_core::emulator::joypad::GbButton::Down,
                        6 => gbmul_core::emulator::joypad::GbButton::Left,
                        7 => gbmul_core::emulator::joypad::GbButton::Right,
                        _ => continue,
                    };
                    if is_down {
                        emu.joypad.press(gb_btn);
                    } else {
                        emu.joypad.release(gb_btn);
                    }
                }
            }
        }

        menu.update(target_open, win_w as f32);

        // --- Render game frame ---
        let fb = emu.get_framebuffer();
        let pal = &PALETTES[palette_index % PALETTES.len()];
        tex.with_lock(None, |buf: &mut [u8], pitch: usize| {
            for (i, &shade) in fb.iter().enumerate() {
                let (r, g, b) = pal[(shade & 0x03) as usize];
                let x = i % GB_W as usize;
                let y = i / GB_W as usize;
                let o = y * pitch + x * 3;
                buf[o] = r; buf[o + 1] = g; buf[o + 2] = b;
            }
        }).expect("Texture lock failed");

        let scale = (win_w / GB_W).min(win_h / GB_H).max(1);
        let sw = GB_W * scale;
        let sh = GB_H * scale;
        let dst = Rect::new(((win_w - sw) / 2) as i32, ((win_h - sh) / 2) as i32, sw, sh);

        canvas.set_draw_color(Color::RGB(0, 0, 0));
        canvas.clear();
        canvas.copy(&tex, None, Some(dst)).unwrap();

        // --- Render menu overlay ---
        if menu.is_visible(win_w) {
            let px = menu.slide_x as i32;
            let visible_w = (win_w as i32 - px).max(0) as u32;

            // Dim the game behind the panel
            canvas.set_draw_color(Color::RGBA(0, 0, 0, 80));
            canvas.fill_rect(Rect::new(0, 0, win_w, win_h)).unwrap();

            if visible_w > 0 {
                // Panel — white background
                canvas.set_draw_color(Color::RGB(255, 255, 255));
                canvas.fill_rect(Rect::new(px, 0, visible_w, win_h)).unwrap();

                // Left-edge separator
                canvas.set_draw_color(Color::RGB(170, 170, 170));
                canvas.fill_rect(Rect::new(px, 0, 2, win_h)).unwrap();

                // Menu items - medium font size (between original and previous smaller version)
                let font_scale = ((win_h / 150) as i32).max(2);
                let label_scale = (font_scale - 1).max(1);
                let char_h = 8 * font_scale;  // TERM font is 8x8 glyphs
                let item_h = char_h + font_scale * 4;
                let items_top = win_h as i32 / 2 - (menu.current_items().len() as i32 * item_h) / 2;

                // About screen: build id above the BACK item
                if menu.current_menu == 2 {
                    let info_color = Color::RGB(50, 50, 50);
                    let muted = Color::RGB(119, 119, 119);
                    let line_gap = item_h;
                    // Center a small info block above the single BACK row
                    let block_h = line_gap * 3;
                    let block_top = items_top - block_h - font_scale * 2;
                    let label_x = px + font_scale * 12;
                    draw_str(&mut canvas, "GBMUL", label_x, block_top, label_scale, info_color);
                    draw_str(
                        &mut canvas,
                        "BUILD",
                        label_x,
                        block_top + line_gap,
                        label_scale,
                        muted,
                    );
                    draw_str(
                        &mut canvas,
                        BUILD_ID,
                        label_x,
                        block_top + line_gap * 2,
                        label_scale,
                        info_color,
                    );
                }

                for (i, &item) in menu.current_items().iter().enumerate() {
                    let iy = items_top + i as i32 * item_h;
                    let selected = i == menu.selected;

                    if selected {
                        // Highlight row — light grey (palette shade 1)
                        canvas.set_draw_color(Color::RGB(221, 221, 221));
                        canvas.fill_rect(Rect::new(
                            px + 2, iy - font_scale,
                            (visible_w as i32 - 4).max(0) as u32,
                            (item_h - font_scale) as u32,
                        )).unwrap();
                    }

                    // Center text vertically inside the highlight zone
                    let text_h = 8 * label_scale;
                    let text_y = iy - font_scale + ((item_h - font_scale) - text_h) / 2;

                    // Arrow — dark grey (palette shade 3), slightly smaller than labels
                    let arrow_color = Color::RGB(119, 119, 119);
                    draw_str(&mut canvas, ">", px + font_scale * 2, text_y, label_scale, arrow_color);

                    // Label
                    let text_color = if selected { Color::RGB(50, 50, 50) } else { Color::RGB(170, 170, 170) };
                    let label_x = px + font_scale * 12;
                    draw_str(&mut canvas, item, label_x, text_y, label_scale, text_color);

                    // Checkbox for A.LOAD / A.SAVE / FPS / BOT - draw cute box, overlay a simple X when checked
                    if item == "A.LOAD" || item == "A.SAVE" || item == "FPS" || item == "BOT" || item == "SOUND" {
                        let checked = if item == "A.LOAD" { auto_restore } else if item == "A.SAVE" { auto_save } else if item == "FPS" { show_fps } else if item == "BOT" { bot_enabled } else { play_sound };
                        // Position relative to the full panel width (from px), so checkboxes slide with the menu like labels
                        let cb_x = px + panel_w as i32 - font_scale * 10;
                        let cb_y = text_y;
                        let bs = label_scale * 8;  // box size ~glyph height
                        // Only draw if within the visible area during animation
                        if cb_x + bs as i32 <= win_w as i32 {
                            // draw box border
                            canvas.set_draw_color(text_color);
                            canvas.draw_rect(Rect::new(cb_x, cb_y, bs as u32, (label_scale*8) as u32 )).ok();
                            if checked {
                                // overlay a simple X using lines (independent of font glyphs)
                                let pad = label_scale;
                                let left = cb_x + pad;
                                let right = cb_x + bs as i32 - pad;
                                let top = cb_y + pad;
                                let bottom = cb_y + (label_scale * 8) as i32 - pad;
                                canvas.draw_line((left, top), (right, bottom)).ok();
                                canvas.draw_line((right, top), (left, bottom)).ok();
                            }
                        }
                    } else if item == "PALETTE" {
                        // Draw 4-color swatch preview of current palette (instead of checkbox)
                        let pal = &PALETTES[palette_index % PALETTES.len()];
                        let sw_x = px + panel_w as i32 - font_scale * 14;
                        let sw_y = text_y;
                        let sw_size = (label_scale * 6) as u32;
                        let gap = label_scale as i32;
                        // only if it fits in visible panel
                        if sw_x + (sw_size as i32 * 4 + gap * 3) <= win_w as i32 {
                            for (j, &(r, g, b)) in pal.iter().enumerate() {
                                let sx = sw_x + j as i32 * (sw_size as i32 + gap);
                                canvas.set_draw_color(Color::RGB(r, g, b));
                                canvas.fill_rect(Rect::new(sx, sw_y, sw_size, sw_size)).ok();
                                // subtle outline using text_color
                                canvas.set_draw_color(text_color);
                                canvas.draw_rect(Rect::new(sx, sw_y, sw_size, sw_size)).ok();
                            }
                        }
                    }
                }
            }
        }

        // Draw FPS at top-left using the embedded TERM font (8x8 glyphs)
        if show_fps {
            let fps_text = format!("FPS:{}", fps);
            let fps_scale: i32 = 2;
            let pad: i32 = 3;
            let adv: i32 = (8 + 1) * fps_scale; // advance per char (glyph 8 +1 spacing)
            let num_chars = fps_text.chars().count() as i32;
            let text_w = if num_chars > 0 {
                ((num_chars - 1) * adv + 8 * fps_scale) as u32
            } else { 0 };
            let text_h = (8 * fps_scale) as u32;
            let x: i32 = 6;
            let y: i32 = 6;

            // dark rounded-ish bg plate for readability over gameplay
            let bg_x = x - pad;
            let bg_y = y - pad;
            let bg_w = text_w + (pad as u32 * 2);
            let bg_h = text_h + (pad as u32 * 2);
            canvas.set_draw_color(Color::RGBA(0, 0, 0, 210));
            canvas.fill_rect(Rect::new(bg_x, bg_y, bg_w, bg_h)).ok();

            // subtle border/outline for nicer look with the cute font
            canvas.set_draw_color(Color::RGBA(70, 70, 70, 180));
            canvas.draw_rect(Rect::new(bg_x, bg_y, bg_w, bg_h)).ok();

            // main text (white for contrast). TERM font via atlas.
            draw_str(&mut canvas, &fps_text, x, y, fps_scale, Color::RGB(255, 255, 255));
        }



        canvas.present();

        // Update FPS counter
        frame_count += 1;
        if fps_last.elapsed() >= Duration::from_secs(1) {
            fps = frame_count;
            frame_count = 0;
            fps_last = Instant::now();
        }

        let elapsed = t0.elapsed().as_nanos() as u64;
        if elapsed < FRAME_NS {
            std::thread::sleep(Duration::from_nanos(FRAME_NS - elapsed));
        }
    }
}

#[cfg(feature = "sdl2")]
fn handle_menu_selection(menu: &mut Menu, emu: &mut Emulator, auto_restore: &mut bool, auto_save: &mut bool, show_fps: &mut bool, palette_index: &mut usize, bot_enabled: &mut bool, bot_controller: &mut Option<gbmul_core::bot::TetrisBot>, play_sound: &mut bool, rom_path: &str) -> bool {
    let item = menu.current_items().get(menu.selected).copied().unwrap_or("");
    log_ts!("[MENU] Confirm on item[{}]: '{}'", menu.selected, item);

    match item {
        "SAVE" => {
            save_state(emu, rom_path);
            menu.hide();
            false
        }
        "LOAD" => {
            load_state(emu, rom_path);
            menu.hide();
            false
        }
        "RESET" => {
            emu.reset();
            menu.hide();
            false
        }
        "A.LOAD" => {
            let old = *auto_restore;
            *auto_restore = !old;
            save_auto_restore(*auto_restore);
            log_ts!("[MENU] A.LOAD toggled {} -> {} (saved to {:?})", old, *auto_restore, settings_path());
            // stay in menu so user sees the checkbox change
            false
        }
        "A.SAVE" => {
            let old = *auto_save;
            *auto_save = !old;
            save_auto_save(*auto_save);
            log_ts!("[MENU] A.SAVE toggled {} -> {} (saved to {:?})", old, *auto_save, auto_save_path());
            // stay in menu so user sees the checkbox change
            false
        }
        "FPS" => {
            let old = *show_fps;
            *show_fps = !old;
            save_show_fps(*show_fps);
            log_ts!("[MENU] Show FPS toggled {} -> {}", old, *show_fps);
            // stay in menu so user sees the checkbox change
            false
        }
        "PALETTE" => {
            let len = gbmul_core::utils::palette::PALETTES.len();
            let old = *palette_index;
            *palette_index = (old + 1) % len;
            save_palette(*palette_index);
            log_ts!("[MENU] Palette cycled {} -> {} ({} of {})", old, *palette_index, *palette_index + 1, len);
            // stay in menu; live recolor of game + swatch happens on next render
            false
        }
        "BOT" => {
            let old = *bot_enabled;
            *bot_enabled = !old;
            if *bot_enabled {
                *bot_controller = Some(gbmul_core::bot::TetrisBot::new());
            } else {
                *bot_controller = None;
                use gbmul_core::emulator::joypad::GbButton::*;
                for b in [A, B, Select, Start, Up, Down, Left, Right] {
                    emu.joypad.release(b);
                }
            }
            save_bot(*bot_enabled);
            log_ts!("[MENU] BOT data log toggled {} -> {}", old, *bot_enabled);
            // stay in menu so user sees the checkbox change
            false
        }
        "SOUND" => {
            let old = *play_sound;
            *play_sound = !old;
            emu.set_audio_enabled(*play_sound);
            save_play_sound(*play_sound);
            log_ts!("[MENU] Sound emulation toggled {} -> {}", old, *play_sound);
            // stay in menu
            false
        }
        "SHOT" => {
            // Manual screenshot via raw fb dump (640x480 32bpp)
            let path = "/mnt/SDCARD/screenshot.raw";
            if let Ok(mut f) = std::fs::File::open("/dev/fb0") {
                let mut buf = vec![0u8; 640 * 480 * 4];
                if std::io::Read::read_exact(&mut f, &mut buf).is_ok() {
                    let _ = std::fs::write(path, &buf);
                    log_ts!("[SHOT] Raw saved to {}. On host: scp flip:{} ~/shot.raw && ffmpeg -f rawvideo -pix_fmt bgra -s 640x480 -i ~/shot.raw ~/shot.png", path, path);
                }
            }
            menu.hide();
            false
        }
        "MISC" => {
            menu.current_menu = 1;
            menu.selected = 0;
            false
        }
        "ABOUT" => {
            menu.current_menu = 2;
            menu.selected = 0;
            log_ts!("[MENU] About — build {}", BUILD_ID);
            false
        }
        "BACK" => {
            menu.nav_back();
            false
        }
        "QUIT" => {
            if *auto_save {
                save_state(emu, rom_path);
                log_ts!("[AUTO] Auto-saved on exit to {}", get_state_path(rom_path));
            }
            true
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Bitmap font — 8×8 (TERM.png), MSB = leftmost pixel
// ---------------------------------------------------------------------------

#[cfg(feature = "sdl2")]
fn glyph(ch: char) -> [u8; 8] {
    // Hardcoded cute TERM font glyphs (8x8), extracted from TERM.png
    // Verified visually via the HTML tester.
    match ch.to_ascii_uppercase() {
        ' ' => [0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b00000000],
        '>' => [0b00010000, 0b00001000, 0b00000100, 0b00000010, 0b00000100, 0b00001000, 0b00010000, 0b00000000],
        'A' => [0b00000000, 0b00000000, 0b00111100, 0b01000100, 0b01000100, 0b01001100, 0b00110100, 0b00000000],
        'B' => [0b01000000, 0b01000000, 0b01111000, 0b01000100, 0b01000100, 0b01000100, 0b01111000, 0b00000000],
        'C' => [0b00000000, 0b00000000, 0b00111000, 0b01000100, 0b01000000, 0b01000100, 0b00111000, 0b00000000],
        'D' => [0b00000100, 0b00000100, 0b00111100, 0b01000100, 0b01000100, 0b01000100, 0b00111100, 0b00000000],
        'E' => [0b00000000, 0b00000000, 0b00111000, 0b01000100, 0b01111100, 0b01000000, 0b00111000, 0b00000000],
        'F' => [0b00001100, 0b00010000, 0b00111000, 0b00010000, 0b00010000, 0b00010000, 0b00010000, 0b00000000],
        'G' => [0b00000000, 0b00000000, 0b00111100, 0b01000100, 0b01000100, 0b00111100, 0b00000100, 0b00111000],
        'H' => [0b01000000, 0b01000000, 0b01111000, 0b01000100, 0b01000100, 0b01000100, 0b01000100, 0b00000000],
        'I' => [0b00010000, 0b00000000, 0b00110000, 0b00010000, 0b00010000, 0b00010000, 0b00111000, 0b00000000],
        'J' => [0b00000100, 0b00000000, 0b00001100, 0b00000100, 0b00000100, 0b00000100, 0b00100100, 0b00011000],
        'K' => [0b00100000, 0b00100000, 0b00100100, 0b00101000, 0b00110000, 0b00101000, 0b00100100, 0b00000000],
        'L' => [0b00110000, 0b00010000, 0b00010000, 0b00010000, 0b00010000, 0b00010000, 0b00001100, 0b00000000],
        'M' => [0b00000000, 0b00000000, 0b01111000, 0b01010100, 0b01010100, 0b01010100, 0b01010100, 0b00000000],
        'N' => [0b00000000, 0b00000000, 0b01111000, 0b01000100, 0b01000100, 0b01000100, 0b01000100, 0b00000000],
        'O' => [0b00000000, 0b00000000, 0b00111000, 0b01000100, 0b01000100, 0b01000100, 0b00111000, 0b00000000],
        'P' => [0b00000000, 0b00000000, 0b01111000, 0b01000100, 0b01000100, 0b01111000, 0b01000000, 0b01000000],
        'Q' => [0b00000000, 0b00000000, 0b00111100, 0b01000100, 0b01000100, 0b00111100, 0b00000100, 0b00000100],
        'R' => [0b00000000, 0b00000000, 0b01011100, 0b01100000, 0b01000000, 0b01000000, 0b01000000, 0b00000000],
        'S' => [0b00000000, 0b00000000, 0b00111100, 0b01000000, 0b00111000, 0b00000100, 0b01111000, 0b00000000],
        'T' => [0b00010000, 0b00010000, 0b00111000, 0b00010000, 0b00010000, 0b00010000, 0b00001100, 0b00000000],
        'U' => [0b00000000, 0b00000000, 0b01000100, 0b01000100, 0b01000100, 0b01000100, 0b00111100, 0b00000000],
        'V' => [0b00000000, 0b00000000, 0b01000100, 0b01000100, 0b00101000, 0b00101000, 0b00010000, 0b00000000],
        'W' => [0b00000000, 0b00000000, 0b01000100, 0b01000100, 0b01010100, 0b01010100, 0b00111000, 0b00000000],
        'X' => [0b00000000, 0b00000000, 0b01000100, 0b00101000, 0b00010000, 0b00101000, 0b01000100, 0b00000000],
        'Y' => [0b00000000, 0b00000000, 0b01000100, 0b01000100, 0b01000100, 0b00111100, 0b00000100, 0b00111000],
        'Z' => [0b00000000, 0b00000000, 0b01111100, 0b00001000, 0b00010000, 0b00100000, 0b01111100, 0b00000000],
        '0' => [0b00111000, 0b01000100, 0b01001100, 0b01010100, 0b01100100, 0b01000100, 0b00111000, 0b00000000],
        '1' => [0b00010000, 0b00110000, 0b00010000, 0b00010000, 0b00010000, 0b00010000, 0b00111000, 0b00000000],
        '2' => [0b00111000, 0b01000100, 0b00000100, 0b00011000, 0b00100000, 0b01000000, 0b01111100, 0b00000000],
        '3' => [0b00111000, 0b01000100, 0b00000100, 0b00011000, 0b00000100, 0b01000100, 0b00111000, 0b00000000],
        '4' => [0b00000100, 0b00001100, 0b00010100, 0b00100100, 0b01000100, 0b01111100, 0b00000100, 0b00000000],
        '5' => [0b01111100, 0b01000000, 0b01111000, 0b00000100, 0b00000100, 0b01000100, 0b00111000, 0b00000000],
        '6' => [0b00111000, 0b01000000, 0b01000000, 0b01111000, 0b01000100, 0b01000100, 0b00111000, 0b00000000],
        '7' => [0b01111100, 0b00000100, 0b00001000, 0b00001000, 0b00010000, 0b00010000, 0b00010000, 0b00000000],
        '8' => [0b00111000, 0b01000100, 0b01000100, 0b00111000, 0b01000100, 0b01000100, 0b00111000, 0b00000000],
        '9' => [0b00111000, 0b01000100, 0b01000100, 0b00111100, 0b00000100, 0b00000100, 0b00111000, 0b00000000],
        '-' => [0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b01111100, 0b00000000, 0b00000000, 0b00000000],
        '#' => [0b00101000, 0b00101000, 0b01111100, 0b00101000, 0b00101000, 0b01111100, 0b00101000, 0b00101000],
        '!' => [0b00010000, 0b00010000, 0b00010000, 0b00010000, 0b00010000, 0b00000000, 0b00010000, 0b00010000],
        '?' => [0b00111000, 0b01000100, 0b01000100, 0b00001000, 0b00010000, 0b00000000, 0b00010000, 0b00010000],
        ',' => [0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b01000000, 0b01000000, 0b10000000],
        '.' => [0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b01000000, 0b01000000, 0b00000000],
        ':' => [0b00000000, 0b01000000, 0b01000000, 0b00000000, 0b00000000, 0b01000000, 0b01000000, 0b00000000],
        '=' => [0b00000000, 0b10000001, 0b01000001, 0b00100001, 0b00010001, 0b00001001, 0b00000101, 0b00000011],
        _ => [0; 8],
    }
}

#[cfg(feature = "sdl2")]
fn draw_char(
    canvas: &mut sdl2::render::Canvas<sdl2::video::Window>,
    ch: char, x: i32, y: i32, scale: i32,
    color: sdl2::pixels::Color,
) {
    use sdl2::rect::Rect;
    let g = glyph(ch);
    canvas.set_draw_color(color);
    for (row, &byte) in g.iter().enumerate() {
        for col in 0i32..8 {
            if byte & (0x80 >> col as u32) != 0 {
                canvas.fill_rect(Rect::new(
                    x + col * scale,
                    y + row as i32 * scale,
                    scale as u32,
                    scale as u32,
                )).ok();
            }
        }
    }
}

#[cfg(feature = "sdl2")]
fn draw_str(
    canvas: &mut sdl2::render::Canvas<sdl2::video::Window>,
    s: &str, x: i32, y: i32, scale: i32,
    color: sdl2::pixels::Color,
) {
    for (i, ch) in s.chars().enumerate() {
        draw_char(canvas, ch, x + i as i32 * (8 + 1) * scale, y, scale, color);
    }
}

// ---------------------------------------------------------------------------
// State save/load + settings (for SDL2 native handheld)
// ---------------------------------------------------------------------------

#[cfg(feature = "sdl2")]
fn get_state_path(rom_path: &str) -> String {
    let p = std::path::Path::new(rom_path);
    if let Some(_stem) = p.file_stem() {
        let mut new_path = p.to_path_buf();
        new_path.set_extension("state");
        new_path.to_string_lossy().into_owned()
    } else {
        format!("{}.state", rom_path)
    }
}

#[cfg(feature = "sdl2")]
fn settings_path() -> std::path::PathBuf {
    // Store setting next to the executable (port folder)
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("auto_restore")))
        .unwrap_or_else(|| std::path::PathBuf::from("auto_restore"))
}

#[cfg(feature = "sdl2")]
fn load_auto_restore() -> bool {
    std::fs::read_to_string(settings_path())
        .map(|s| s.trim() == "1")
        .unwrap_or(false)
}

#[cfg(feature = "sdl2")]
fn save_auto_restore(enabled: bool) {
    let _ = std::fs::write(settings_path(), if enabled { "1" } else { "0" });
}

#[cfg(feature = "sdl2")]
fn auto_save_path() -> std::path::PathBuf {
    // Store setting next to the executable (port folder)
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("auto_save")))
        .unwrap_or_else(|| std::path::PathBuf::from("auto_save"))
}

#[cfg(feature = "sdl2")]
fn load_auto_save() -> bool {
    std::fs::read_to_string(auto_save_path())
        .map(|s| s.trim() == "1")
        .unwrap_or(false)
}

#[cfg(feature = "sdl2")]
fn save_auto_save(enabled: bool) {
    let _ = std::fs::write(auto_save_path(), if enabled { "1" } else { "0" });
}

#[cfg(feature = "sdl2")]
fn show_fps_path() -> std::path::PathBuf {
    // Store setting next to the executable (port folder)
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("show_fps")))
        .unwrap_or_else(|| std::path::PathBuf::from("show_fps"))
}

#[cfg(feature = "sdl2")]
fn load_show_fps() -> bool {
    std::fs::read_to_string(show_fps_path())
        .map(|s| s.trim() == "1")
        .unwrap_or(false)
}

#[cfg(feature = "sdl2")]
fn save_show_fps(enabled: bool) {
    let _ = std::fs::write(show_fps_path(), if enabled { "1" } else { "0" });
}

#[cfg(feature = "sdl2")]
fn play_sound_path() -> std::path::PathBuf {
    // Store setting next to the executable (port folder)
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("play_sound")))
        .unwrap_or_else(|| std::path::PathBuf::from("play_sound"))
}

#[cfg(feature = "sdl2")]
fn load_play_sound() -> bool {
    std::fs::read_to_string(play_sound_path())
        .map(|s| s.trim() == "1")
        .unwrap_or(true) // default enabled
}

#[cfg(feature = "sdl2")]
fn save_play_sound(enabled: bool) {
    let _ = std::fs::write(play_sound_path(), if enabled { "1" } else { "0" });
}

#[cfg(feature = "sdl2")]
fn palette_path() -> std::path::PathBuf {
    // Store setting next to the executable (port folder)
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("palette")))
        .unwrap_or_else(|| std::path::PathBuf::from("palette"))
}

#[cfg(feature = "sdl2")]
fn load_palette() -> usize {
    std::fs::read_to_string(palette_path())
        .ok()
        .and_then(|s| s.trim().parse::<usize>().ok())
        .unwrap_or(0)
}

#[cfg(feature = "sdl2")]
fn save_palette(index: usize) {
    let _ = std::fs::write(palette_path(), index.to_string());
}

#[cfg(feature = "sdl2")]
fn bot_path() -> std::path::PathBuf {
    // Store setting next to the executable (port folder)
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("bot")))
        .unwrap_or_else(|| std::path::PathBuf::from("bot"))
}

#[cfg(feature = "sdl2")]
fn load_bot() -> bool {
    std::fs::read_to_string(bot_path())
        .map(|s| s.trim() == "1")
        .unwrap_or(false)
}

#[cfg(feature = "sdl2")]
fn save_bot(enabled: bool) {
    let _ = std::fs::write(bot_path(), if enabled { "1" } else { "0" });
}

#[cfg(feature = "sdl2")]
fn save_state(emu: &Emulator, rom_path: &str) {
    log_ts!("[SAVE] Starting save for ROM: {}", rom_path);
    let state = emu.save_state();
    log_ts!("[SAVE] State captured, vram.len={} eram.len={}", state.vram.len(), state.eram.len());
    match bincode::serialize(&state) {
        Ok(bytes) => {
            log_ts!("[SAVE] Serialized to {} bytes", bytes.len());
            let path = get_state_path(rom_path);
            match std::fs::write(&path, &bytes) {
                Ok(_) => log_ts!("[SAVE] SUCCESS: State written to {}", path),
                Err(e) => log_ts!("[SAVE] FAILED to write {}: {}", path, e),
            }
        }
        Err(e) => log_ts!("[SAVE] FAILED to serialize: {}", e),
    }
}

#[cfg(feature = "sdl2")]
fn load_state(emu: &mut Emulator, rom_path: &str) {
    let path = get_state_path(rom_path);
    log_ts!("[LOAD] Starting load from {}", path);
    match std::fs::read(&path) {
        Ok(bytes) => {
            log_ts!("[LOAD] Read {} bytes", bytes.len());
            match bincode::deserialize::<EmulatorState>(&bytes) {
                Ok(state) => {
                    log_ts!("[LOAD] Deserialized, checking sizes vram={} eram={}", state.vram.len(), state.eram.len());
                    if state.vram.len() == emu.memory.vram.len() &&
                       state.eram.len() == emu.memory.eram.len() &&
                       state.ram.len() == emu.memory.ram.len() &&
                       state.oam.len() == emu.memory.oam.len() {
                        emu.restore_state(&state);
                        log_ts!("[LOAD] SUCCESS: State restored");
                    } else {
                        log_ts!("[LOAD] Size mismatch - cannot restore safely");
                    }
                }
                Err(e) => log_ts!("[LOAD] Deserialize error: {}", e),
            }
        }
        Err(e) => {
            log_ts!("[LOAD] Read error (no file?): {}", e);
        }
    }
}

// ---------------------------------------------------------------------------
// Button mappings
// ---------------------------------------------------------------------------

#[cfg(feature = "sdl2")]
fn key_to_gb(k: sdl2::keyboard::Keycode) -> Option<gbmul_core::emulator::joypad::GbButton> {
    use gbmul_core::emulator::joypad::GbButton as B;
    use sdl2::keyboard::Keycode;
    match k {
        Keycode::Z | Keycode::A           => Some(B::A),
        Keycode::X | Keycode::S           => Some(B::B),
        Keycode::Return                   => Some(B::Start),
        Keycode::Backspace | Keycode::Space => Some(B::Select),
        Keycode::Up                       => Some(B::Up),
        Keycode::Down                     => Some(B::Down),
        Keycode::Left                     => Some(B::Left),
        Keycode::Right                    => Some(B::Right),
        _                                 => None,
    }
}

#[cfg(feature = "sdl2")]
fn pad_to_gb(b: sdl2::controller::Button) -> Option<gbmul_core::emulator::joypad::GbButton> {
    use gbmul_core::emulator::joypad::GbButton as B;
    use sdl2::controller::Button;
    match b {
        Button::B | Button::X => Some(B::A),
        Button::A | Button::Y => Some(B::B),
        Button::Start         => Some(B::Start),
        Button::Back          => Some(B::Select),
        Button::DPadUp        => Some(B::Up),
        Button::DPadDown      => Some(B::Down),
        Button::DPadLeft      => Some(B::Left),
        Button::DPadRight     => Some(B::Right),
        _                     => None,
    }
}
