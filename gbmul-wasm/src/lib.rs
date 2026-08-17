use wasm_bindgen::prelude::*;

use gbmul_core::bot::TetrisBot;
use gbmul_core::emulator;
use gbmul_core::state;
use gbmul_core::utils;

// ---------------------------------------------------------------------------
// RunResult — return value for run_until_event()
// ---------------------------------------------------------------------------

#[wasm_bindgen]
pub enum RunResult {
    FrameComplete,
    SerialPending,
    Yielded,
}

// ---------------------------------------------------------------------------
// GbEmu — single-player (unchanged, kept for backwards compatibility)
// ---------------------------------------------------------------------------

/// WASM-exposed Game Boy emulator.
///
/// JS usage:
///   const emu = new GbEmu();
///   emu.load_rom(romBytes);          // Uint8Array
///   const pixels = emu.run_frame();  // Uint8Array – 160×144 RGBA
///   emu.key_down(0);                 // GbButton enum value
///   emu.key_up(0);
#[wasm_bindgen]
pub struct GbEmu {
    emulator: emulator::Emulator,
    palette_index: usize,
    /// T-cycle counter within the current frame for run_until_event().
    wgl_frame_cycles: u32,
}

/// Numeric constants matching the Game Boy button enum, exposed to JS.
#[allow(non_snake_case)]
#[wasm_bindgen]
pub struct GbButton;

#[allow(non_snake_case)]
#[wasm_bindgen]
impl GbButton {
    #[wasm_bindgen(getter)] pub fn A()      -> u8 { 0 }
    #[wasm_bindgen(getter)] pub fn B()      -> u8 { 1 }
    #[wasm_bindgen(getter)] pub fn Select() -> u8 { 2 }
    #[wasm_bindgen(getter)] pub fn Start()  -> u8 { 3 }
    #[wasm_bindgen(getter)] pub fn Up()     -> u8 { 4 }
    #[wasm_bindgen(getter)] pub fn Down()   -> u8 { 5 }
    #[wasm_bindgen(getter)] pub fn Left()   -> u8 { 6 }
    #[wasm_bindgen(getter)] pub fn Right()  -> u8 { 7 }
}

#[wasm_bindgen]
impl GbEmu {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        console_error_panic_hook::set_once();
        let _ = console_log::init_with_level(log::Level::Warn);
        GbEmu {
            emulator: emulator::Emulator::new(),
            palette_index: 0,
            wgl_frame_cycles: 0,
        }
    }

    /// Load ROM without the random warm-up frames used in solo mode.
    /// Both sides must use this in networked play so they start from the same state.
    pub fn load_rom_sync(&mut self, rom: &[u8]) -> Result<(), JsValue> {
        self.emulator.memory.load_rom(rom);
        self.emulator.running = true;
        self.wgl_frame_cycles = 0;
        Ok(())
    }

    /// Run until a frame completes or this emulator becomes the serial MASTER
    /// waiting for the peer's response.
    ///
    /// Returns FrameComplete when 70 224 T-cycles have elapsed (call get_screen()
    /// + get_audio_buffer(), then await requestAnimationFrame before the next call).
    ///
    /// Returns SerialPending ONLY when this emulator initiated a master-mode
    /// transfer and is waiting for the slave's byte. The CPU is parked exactly at
    /// that point. Call pending_serial_byte() for the byte to send, exchange it
    /// over WebRTC, then resume_with_serial(slaveByte) before calling again.
    ///
    /// A SLAVE-armed serial port does NOT stop the loop: real hardware keeps the
    /// CPU running while the slave waits for the master's clock. The transfer
    /// stays pending until JS injects the master's byte via resume_with_serial().
    /// Two slaves therefore never complete an exchange (matching real hardware),
    /// which is essential for the Tetris 2-player master/slave negotiation.
    ///
    /// `max_cycles` bounds how many T-cycles run before returning Yielded (0 =
    /// run the whole frame). A small budget lets JS check for the peer's serial
    /// byte and yield to the event loop many times per frame, so a slave can
    /// respond to the master within ~1 ms instead of waiting a full ~16 ms
    /// frame — this is what keeps the master smooth and the handshake fast.
    pub fn run_until_event(&mut self, max_cycles: u32) -> RunResult {
        const FRAME: u32 = 70224;
        const SLICE: u32 = 512;

        if self.wgl_frame_cycles == 0 {
            self.emulator.frame_start_polls();
        }

        let start = self.wgl_frame_cycles;

        while self.wgl_frame_cycles < FRAME {
            let to_run = SLICE.min(FRAME - self.wgl_frame_cycles);
            let ran = self.emulator.run_slice(to_run);
            self.wgl_frame_cycles += ran;

            if self.emulator.serial.transferring && self.emulator.serial.master_waiting {
                return RunResult::SerialPending;
            }

            if max_cycles != 0 && self.wgl_frame_cycles - start >= max_cycles {
                return RunResult::Yielded;
            }
        }

        self.wgl_frame_cycles = 0;
        RunResult::FrameComplete
    }

    /// The byte this emulator's serial port wants to send (valid after
    /// SerialPending, or when serial_slave_pending() is true).
    pub fn pending_serial_byte(&self) -> u8 {
        self.emulator.serial.sb
    }

    /// True when this emulator's serial port is armed as a slave, waiting for the
    /// master's clock. JS polls this each loop iteration: when a master byte has
    /// arrived from the peer, it injects it via resume_with_serial().
    pub fn serial_slave_pending(&self) -> bool {
        self.emulator.serial.slave_pending
    }

    /// Inject the byte received from the peer and allow emulation to continue.
    pub fn resume_with_serial(&mut self, received: u8) {
        if self.emulator.serial.slave_pending {
            self.emulator.serial.slave_pending = false;
        }
        self.emulator.serial.receive_byte(received);
    }

    /// Return the current framebuffer as 160×144 RGBA pixels without advancing
    /// emulation. Use after run_until_event() returns FrameComplete.
    pub fn get_screen(&self) -> Vec<u8> {
        framebuffer_to_rgba(self.emulator.get_framebuffer(), self.palette_index)
    }

    pub fn load_rom(&mut self, rom: &[u8]) -> Result<(), JsValue> {
        self.emulator
            .load_rom(rom)
            .map_err(|e| JsValue::from_str(&e))
    }

    /// Run one Game Boy frame (~60 fps).
    /// Returns a flat Uint8Array of 160×144 RGBA pixels (92 160 bytes).
    pub fn run_frame(&mut self) -> Vec<u8> {
        self.emulator.run_frame();
        framebuffer_to_rgba(self.emulator.get_framebuffer(), self.palette_index)
    }

    pub fn key_down(&mut self, button: u8) {
        if let Some(btn) = map_button(button) {
            self.emulator.joypad.press(btn);
        }
    }

    pub fn key_up(&mut self, button: u8) {
        if let Some(btn) = map_button(button) {
            self.emulator.joypad.release(btn);
        }
    }

    pub fn set_palette(&mut self, index: u8) {
        self.palette_index = index as usize % utils::palette::PALETTES.len();
    }

    pub fn palette_count() -> u8 {
        utils::palette::PALETTES.len() as u8
    }

    pub fn read_mem(&self, addr: u16) -> u8 {
        self.emulator.memory.read(addr)
    }

    pub fn read_mem_range(&self, start: u16, length: u16) -> Vec<u8> {
        (0..length)
            .map(|i| self.emulator.memory.read(start.wrapping_add(i)))
            .collect()
    }

    pub fn save_state(&self) -> Result<Vec<u8>, JsValue> {
        let s = self.emulator.save_state();
        bincode::serialize(&s).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    pub fn load_state(&mut self, data: &[u8]) -> Result<(), JsValue> {
        let s: state::EmulatorState = bincode::deserialize(data)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.emulator.restore_state(&s);
        Ok(())
    }

    /// Drain and return the APU sample buffer (interleaved L/R Float32).
    /// Call once per frame after run_frame().
    pub fn get_audio_buffer(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.emulator.apu.sample_buffer)
    }

    pub fn get_sram(&self) -> Vec<u8> {
        self.emulator.memory.eram.to_vec()
    }

    pub fn set_sram(&mut self, data: &[u8]) {
        let len = data.len().min(self.emulator.memory.eram.len());
        self.emulator.memory.eram[..len].copy_from_slice(&data[..len]);
    }

    pub fn sram_status(&self) -> String {
        if self.emulator.memory.is_battery_backed() {
            format!("battery-backed, {} bytes", self.emulator.memory.eram.len())
        } else {
            "not battery-backed".to_string()
        }
    }
}

// ---------------------------------------------------------------------------
// GbEmuPair — two emulators linked by a virtual link cable
// ---------------------------------------------------------------------------

/// Two Game Boy emulators connected via a virtual link cable (SharedLink).
///
/// JS usage:
///   const pair = new GbEmuPair();
///   pair.load_rom(romBytes);           // loads same ROM into both
///   const pixelsA = pair.run_frame_a(); // human (left)
///   const pixelsB = pair.run_frame_b(); // bot   (right)
///   pair.key_down_a(btn);              // human input
///   pair.key_down_b(btn);              // bot input
///   const v = pair.read_mem_a(addr);   // inspect human emulator
///   const v = pair.read_mem_b(addr);   // inspect bot emulator
/// Tetris GB (and Rosy 2P) link-cable protocol phase.
///
/// Pre-round the master sends a 256-byte piece table (IDs `0x00/04/08/0C/10/14/18`).
/// Only after that stream ends do bytes mean stack heights. Interpreting piece IDs
/// as heights is the main source of false garbage at match start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LinkPhase {
    /// Handshake / menu / idle — ignore height semantics.
    PreGame,
    /// Master is streaming the 256-piece table.
    PieceSync,
    /// In-round: low bytes are stack heights; drops imply garbage.
    Gameplay,
}

/// Classic GB Tetris piece-table encoding (index × 4).
#[inline]
fn is_tetris_piece_id(b: u8) -> bool {
    matches!(b, 0x00 | 0x04 | 0x08 | 0x0C | 0x10 | 0x14 | 0x18)
}

/// Round-end / desync markers seen in serial captures.
#[inline]
fn is_tetris_round_end(b: u8) -> bool {
    matches!(b, 0x94 | 0xAA | 0xFF)
}

/// Log to the browser console (visible even when log crate level is filtered).
fn console_link(msg: &str) {
    web_sys::console::log_1(&JsValue::from_str(msg));
}

#[wasm_bindgen]
pub struct GbEmuPair {
    emu_a: emulator::Emulator,
    emu_b: emulator::Emulator,
    palette_index: usize,
    /// Last height A sent during gameplay (for garbage detection).
    link_prev_a_height: u8,
    /// Accumulated garbage lines detected via link height drops.
    link_garbage_total: u32,
    /// Current Tetris link protocol phase.
    link_phase: LinkPhase,
    /// Consecutive piece-ID bytes while still in PreGame (enter PieceSync).
    link_piece_streak: u16,
    /// Piece-table bytes counted in PieceSync (target 256).
    link_piece_count: u16,
    /// Candidate height awaiting confirmation (must repeat to count).
    link_height_pending: u8,
    /// How many consecutive samples equal `link_height_pending`.
    link_height_pending_n: u8,
    /// Last *confirmed* (stable) height of A.
    link_height_stable: u8,
    /// How long we've held `link_height_stable` (extra confirm ticks).
    /// Real stack height sits for many frames; post-sync heartbeat
    /// oscillates `0x00`↔`0x02` too quickly to accumulate dwell.
    link_height_dwell: u16,
}

/// Consecutive identical height samples required before a value is trusted.
const LINK_HEIGHT_STABLE_N: u8 = 4;

/// Extra confirm ticks at the high height before a drop can mean garbage.
/// A real double (height 2) sits for a long time before the clear; the false
/// `2→0` at match start only holds `0x02` for a handful of exchanges.
const LINK_HEIGHT_MIN_DWELL: u16 = 20;

#[wasm_bindgen]
impl GbEmuPair {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        console_error_panic_hook::set_once();
        let _ = console_log::init_with_level(log::Level::Debug);

        let (link_a, link_b) = emulator::link::SharedLink::pair();
        let mut emu_a = emulator::Emulator::new();
        let mut emu_b = emulator::Emulator::new();
        emu_a.set_link(Box::new(link_a));
        emu_b.set_link(Box::new(link_b));

        GbEmuPair {
            emu_a,
            emu_b,
            palette_index: 0,
            link_prev_a_height: 0,
            link_garbage_total: 0,
            link_phase: LinkPhase::PreGame,
            link_piece_streak: 0,
            link_piece_count: 0,
            link_height_pending: 0,
            link_height_pending_n: 0,
            link_height_stable: 0,
            link_height_dwell: 0,
        }
    }

    /// Returns accumulated garbage lines detected via link height drops, and resets.
    #[wasm_bindgen(js_name = takeLinkGarbage)]
    pub fn take_link_garbage(&mut self) -> u32 {
        let v = self.link_garbage_total;
        self.link_garbage_total = 0;
        v
    }

    /// Load the same ROM into both emulators. No RNG warm-up so both start
    /// in identical state and the link cable protocol can synchronise them.
    pub fn load_rom(&mut self, rom: &[u8]) -> Result<(), JsValue> {
        self.emu_a.memory.load_rom(rom);
        self.emu_a.running = true;
        self.emu_b.memory.load_rom(rom);
        self.emu_b.running = true;
        self.reset_link_protocol();
        Ok(())
    }

    pub fn run_frame_a(&mut self) -> Vec<u8> {
        self.emu_a.run_frame();
        self.advance_link();
        framebuffer_to_rgba(self.emu_a.get_framebuffer(), self.palette_index)
    }

    pub fn run_frame_b(&mut self) -> Vec<u8> {
        self.emu_b.run_frame();
        self.advance_link();
        framebuffer_to_rgba(self.emu_b.get_framebuffer(), self.palette_index)
    }

    /// Run both emulators interleaved in 512-cycle slices so that serial
    /// link-cable exchanges complete within A's own frame rather than waiting
    /// a full rAF tick.  On real hardware a serial round-trip takes 512 T-cycles
    /// (~0.12 ms); without interleaving each round-trip costs one JS frame
    /// (~16 ms), stretching the ~840-exchange 2P pre-game handshake to ~14 s.
    /// With 512-cycle slices each exchange takes ~1 024 cycles (~2 slices),
    /// so all 840 exchanges fit within ~12 GB frames → ~0.2 s of real time.
    pub fn run_frame_pair(&mut self) -> Vec<u8> {
        const FRAME: u32 = 70224;
        const SLICE: u32 = 512;

        self.emu_a.frame_start_polls();
        self.emu_b.frame_start_polls();

        let mut fa: u32 = 0;
        let mut fb: u32 = 0;

        while fa < FRAME || fb < FRAME {
            if fa < FRAME {
                fa += self.emu_a.run_slice(SLICE.min(FRAME - fa));
                self.advance_link();
            }
            if fb < FRAME {
                fb += self.emu_b.run_slice(SLICE.min(FRAME - fb));
                self.advance_link();
            }
        }

        let mut out = framebuffer_to_rgba(self.emu_a.get_framebuffer(), self.palette_index);
        out.extend(framebuffer_to_rgba(self.emu_b.get_framebuffer(), self.palette_index));
        out
    }

    /// Set side A's entire button state from a bit mask (bit 0 = A … 7 = Right).
    /// Used by the lockstep netplay loop to apply a frame's synchronized input.
    pub fn set_input_a(&mut self, mask: u8) { self.emu_a.joypad.set_from_mask(mask); }
    /// Set side B's entire button state from a bit mask (bit 0 = A … 7 = Right).
    pub fn set_input_b(&mut self, mask: u8) { self.emu_b.joypad.set_from_mask(mask); }

    /// Drain side B's APU sample buffer (the guest renders/plays side B).
    pub fn get_audio_buffer_b(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.emu_b.apu.sample_buffer)
    }

    pub fn key_down_a(&mut self, button: u8) {
        if let Some(btn) = map_button(button) { self.emu_a.joypad.press(btn); }
    }
    pub fn key_up_a(&mut self, button: u8) {
        if let Some(btn) = map_button(button) { self.emu_a.joypad.release(btn); }
    }
    pub fn key_down_b(&mut self, button: u8) {
        if let Some(btn) = map_button(button) { self.emu_b.joypad.press(btn); }
    }
    pub fn key_up_b(&mut self, button: u8) {
        if let Some(btn) = map_button(button) { self.emu_b.joypad.release(btn); }
    }

    pub fn read_mem_a(&self, addr: u16) -> u8 { self.emu_a.memory.read(addr) }
    pub fn read_mem_b(&self, addr: u16) -> u8 { self.emu_b.memory.read(addr) }

    pub fn read_mem_range_a(&self, start: u16, length: u16) -> Vec<u8> {
        (0..length).map(|i| self.emu_a.memory.read(start.wrapping_add(i))).collect()
    }
    pub fn read_mem_range_b(&self, start: u16, length: u16) -> Vec<u8> {
        (0..length).map(|i| self.emu_b.memory.read(start.wrapping_add(i))).collect()
    }

    pub fn set_palette(&mut self, index: u8) {
        self.palette_index = index as usize % utils::palette::PALETTES.len();
    }

    pub fn palette_count() -> u8 {
        utils::palette::PALETTES.len() as u8
    }

    /// True when the link-cable serial handshake is actively in progress on
    /// either emulator — master waiting for slave response, or slave armed.
    /// Used by JS to detect the 2P pre-game handshake and run extra frame
    /// pairs per rAF tick to speed up the ~840-exchange sequence.
    pub fn link_transferring(&self) -> bool {
        (self.emu_a.serial.transferring && self.emu_a.serial.master_waiting)
            || self.emu_a.serial.slave_pending
            || (self.emu_b.serial.transferring && self.emu_b.serial.master_waiting)
            || self.emu_b.serial.slave_pending
    }

    pub fn save_state_a(&self) -> Result<Vec<u8>, JsValue> {
        let s = self.emu_a.save_state();
        bincode::serialize(&s).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    pub fn load_state_a(&mut self, data: &[u8]) -> Result<(), JsValue> {
        let s: state::EmulatorState = bincode::deserialize(data)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.emu_a.restore_state(&s);
        Ok(())
    }

    /// Drain and return emu_a's APU sample buffer (interleaved L/R Float32).
    pub fn get_audio_buffer_a(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.emu_a.apu.sample_buffer)
    }
}

// ---------------------------------------------------------------------------
// RustBot — thin wrapper so the Rust TetrisBot can drive one side
// ---------------------------------------------------------------------------

#[wasm_bindgen]
pub struct MisdropStats {
    pub count: u32,
    pub total: u32,
}

#[wasm_bindgen]
pub struct RustBot {
    bot: TetrisBot,
}

#[wasm_bindgen]
impl RustBot {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        console_error_panic_hook::set_once();
        let _ = console_log::init_with_level(log::Level::Warn);
        RustBot {
            bot: TetrisBot::new(),
        }
    }

    pub fn reset(&mut self) {
        self.bot.reset();
    }

    /// Tell the bot we restored a misdrop replay savestate. It will reset
    /// planning state and suppress misdrop evaluation for the restored piece.
    pub fn begin_replay_restore(&mut self) {
        self.bot.begin_replay_restore();
    }

    /// Misdrop replay: plan the saved want lock instead of re-running 2-ply.
    #[wasm_bindgen(js_name = beginReplayRestoreWithWant)]
    pub fn begin_replay_restore_with_want(
        &mut self,
        row: i32,
        col: usize,
        rot: usize,
        mtype: Option<String>,
    ) {
        self.bot
            .begin_replay_restore_with_want(row, col, rot, mtype.as_deref());
    }

    /// Generic savestate restore (load state / page reload). Clears cached paths
    /// and forces a fresh BFS plan on the restored active piece.
    #[wasm_bindgen(js_name = beginStateRestore)]
    pub fn begin_state_restore(&mut self) {
        self.bot.begin_state_restore();
    }

    /// Feed garbage lines from link-cable height-drop detection.
    #[wasm_bindgen(js_name = addGarbageLines)]
    pub fn add_garbage_lines(&mut self, lines: u32) {
        self.bot.add_garbage_lines(lines);
    }

    /// Debug: get the bot's current planned move path as a comma-joined string.
    #[wasm_bindgen(js_name = "debugGetMovePath")]
    pub fn debug_get_move_path(&self) -> String {
        self.bot.debug_get_move_path().join(",")
    }

    /// Debug: full BFS path from plan time (does not shrink during execution).
    #[wasm_bindgen(js_name = "debugGetPlannedPath")]
    pub fn debug_get_planned_path(&self) -> String {
        self.bot.debug_get_planned_path().join(",")
    }

    /// Debug: landing type from BFS intended lock ("normal", "tuck", "spin").
    #[wasm_bindgen(js_name = "debugGetLandingType")]
    pub fn debug_get_landing_type(&self) -> String {
        self.bot.debug_get_landing_type()
    }

    /// Debug: classify intention per user's rule (last movement before drop).
    #[wasm_bindgen(js_name = "debugClassifyIntention")]
    pub fn debug_classify_intention(&self) -> String {
        self.bot.debug_classify_intention()
    }

    /// Debug: (target_col, target_rot, path_step)
    #[wasm_bindgen(js_name = "debugGetTarget")]
    pub fn debug_get_target(&self) -> String {
        let (l, r, step, piece, next) = self.bot.debug_get_target();
        format!(
            "{{\"col\":{},\"rot\":{},\"step\":{},\"piece\":\"{}\",\"next\":\"{}\"}}",
            l, r, step, piece, next
        )
    }

    #[wasm_bindgen(js_name = "debugGetPendingAction")]
    pub fn debug_get_pending_action(&self) -> String {
        self.bot
            .debug_get_pending_action()
            .unwrap_or_default()
    }

    #[wasm_bindgen(js_name = "debugPathFlags")]
    pub fn debug_path_flags(&self) -> String {
        let (hold, armed, min) = self.bot.debug_path_flags();
        format!(
            "{{\"holdingDown\":{},\"releaseArmed\":{},\"downMinFrames\":{}}}",
            hold, armed, min
        )
    }

    #[wasm_bindgen(js_name = "debugTakePathTrace")]
    pub fn debug_take_path_trace(&mut self) -> String {
        self.bot.debug_take_path_trace()
    }

    /// Drain lock-audit entries since last call (host persists to localStorage).
    #[wasm_bindgen(js_name = "takeLockAuditJson")]
    pub fn take_lock_audit_json(&mut self) -> String {
        self.bot.take_lock_audit_json()
    }

    /// Sample falling-piece pose after `run_frame` (deferred lock verify timing).
    #[wasm_bindgen(js_name = "tickPostFrame")]
    pub fn tick_post_frame(&mut self, emu: &mut GbEmu) {
        self.bot.tick_post_frame(
            |addr| emu.read_mem(addr),
            |start, len| emu.read_mem_range(start, len),
        );
    }

    /// Sample falling-piece pose after `run_frame_pair` (dual mode, bot side B).
    #[wasm_bindgen(js_name = "tickPostFramePairB")]
    pub fn tick_post_frame_pair_b(&mut self, pair: &mut GbEmuPair) {
        self.bot.tick_post_frame(
            |addr| pair.read_mem_b(addr),
            |start, len| pair.read_mem_range_b(start, len),
        );
    }

    /// Call once per frame in solo mode (side A / single GbEmu).
    pub fn tick(&mut self, emu: &mut GbEmu) {
        let ( _gs, actions ) = self.bot.tick(
            |addr| emu.read_mem(addr),
            |start, len| emu.read_mem_range(start, len),
        );
        for (btn, is_down) in actions {
            if is_down {
                emu.key_down(btn);
            } else {
                emu.key_up(btn);
            }
        }
    }

    /// Call once per frame in local dual mode — bot drives side B of the pair.
    #[wasm_bindgen(js_name = "tickPairB")]
    pub fn tick_pair_b(&mut self, pair: &mut GbEmuPair) {
        let ( _gs, actions ) = self.bot.tick(
            |addr| pair.read_mem_b(addr),
            |start, len| pair.read_mem_range_b(start, len),
        );
        for (btn, is_down) in actions {
            if is_down {
                pair.key_down_b(btn);
            } else {
                pair.key_up_b(btn);
            }
        }
    }

    #[wasm_bindgen(js_name = "setPps")]
    pub fn set_pps(&mut self, pps: f64) {
        self.bot.set_pps(pps);
    }

    #[wasm_bindgen(js_name = "setInputDelay")]
    pub fn set_input_delay(&mut self, delay: u32) {
        self.bot.set_input_delay(delay);
    }

    #[wasm_bindgen(js_name = "setSoftDropMode")]
    pub fn set_soft_drop_mode(&mut self, enabled: bool) {
        self.bot.set_soft_drop_mode(enabled);
    }

    #[wasm_bindgen(js_name = "setAutoMenuNav")]
    pub fn set_auto_menu_nav(&mut self, enabled: bool) {
        self.bot.set_auto_menu_nav(enabled);
    }

    #[wasm_bindgen(js_name = "resetStats")]
    pub fn reset_stats(&mut self) {
        self.bot.reset_stats();
    }

    #[wasm_bindgen(js_name = "consumePauseRequest")]
    pub fn consume_pause_request(&mut self) -> bool {
        self.bot.consume_pause_request()
    }

    #[wasm_bindgen(getter)]
    pub fn action(&self) -> String {
        self.bot.action().to_string()
    }

    #[wasm_bindgen(getter)]
    pub fn mode(&self) -> String {
        self.bot.mode().to_string()
    }

    #[wasm_bindgen(js_name = "misdropStats")]
    pub fn misdrop_stats(&self) -> MisdropStats {
        let (count, total) = self.bot.misdrop_stats();
        MisdropStats { count, total }
    }

    /// Returns small JSON metadata for the most recent misdrop (piece types + context).
    /// Host (JS) pairs it with a full emulator savestate captured at spawn.
    /// Returns empty string when nothing pending.
    #[wasm_bindgen(js_name = "takePendingReplayJson")]
    pub fn take_pending_replay_json(&mut self) -> String {
        self.bot.take_pending_replay_json()
    }

    #[wasm_bindgen(js_name = "hasPendingMisdropPairing")]
    pub fn has_pending_misdrop_pairing(&self) -> bool {
        self.bot.has_pending_misdrop_pairing()
    }
}

// Non-wasm helper impl (private, not exported).
impl GbEmuPair {
    fn reset_link_protocol(&mut self) {
        self.link_phase = LinkPhase::PreGame;
        self.link_piece_streak = 0;
        self.link_piece_count = 0;
        self.link_prev_a_height = 0;
        self.link_garbage_total = 0;
        self.link_height_pending = 0;
        self.link_height_pending_n = 0;
        self.link_height_stable = 0;
        self.link_height_dwell = 0;
    }

    /// Drive the Tetris link protocol state machine from the **master** TX byte.
    ///
    /// PreGame → PieceSync after a short streak of piece IDs (filters menu noise).
    /// PieceSync → Gameplay after 256 piece-table bytes (or ≥200 if stream breaks).
    /// Gameplay → PreGame on round-end markers.
    ///
    /// Height / garbage is handled separately via [`track_a_height`] so we always
    /// use side A's stack (human → bot), regardless of who clocks the cable.
    fn observe_protocol_byte(&mut self, master_tx: u8) {
        match self.link_phase {
            LinkPhase::PreGame => {
                if is_tetris_piece_id(master_tx) {
                    self.link_piece_streak = self.link_piece_streak.saturating_add(1);
                    // Menu bytes occasionally look like piece IDs; require a real burst.
                    if self.link_piece_streak >= 8 {
                        self.link_phase = LinkPhase::PieceSync;
                        self.link_piece_count = self.link_piece_streak;
                        self.link_piece_streak = 0;
                        console_link(&format!(
                            "[link] piece-table sync started (count={})",
                            self.link_piece_count
                        ));
                    }
                } else {
                    self.link_piece_streak = 0;
                }
            }
            LinkPhase::PieceSync => {
                if is_tetris_piece_id(master_tx) {
                    self.link_piece_count = self.link_piece_count.saturating_add(1);
                    if self.link_piece_count >= 256 {
                        self.enter_gameplay("256 piece bytes");
                    }
                } else {
                    // Only accept end-of-stream after most of the table arrived.
                    // Early non-piece noise must not arm height tracking.
                    if self.link_piece_count >= 200 {
                        console_link(&format!(
                            "[link] piece-table sync ended at {} bytes (next=0x{:02X})",
                            self.link_piece_count, master_tx
                        ));
                        self.enter_gameplay("stream end");
                        if is_tetris_round_end(master_tx) {
                            self.link_phase = LinkPhase::PreGame;
                            self.link_prev_a_height = 0;
                            self.link_height_dwell = 0;
                        }
                    } else {
                        // Menu / pre-table noise while still short of 200 — stay put (quiet).
                    }
                }
            }
            LinkPhase::Gameplay => {
                if is_tetris_round_end(master_tx) {
                    console_link(&format!(
                        "[link] round end marker 0x{:02X} — back to PreGame",
                        master_tx
                    ));
                    self.link_phase = LinkPhase::PreGame;
                    self.link_piece_streak = 0;
                    self.link_piece_count = 0;
                    self.link_prev_a_height = 0;
                    self.link_height_dwell = 0;
                }
            }
        }
    }

    /// Track human (side A) stack height during gameplay only.
    ///
    /// False positive at match start was `0→2` then `2→0` (heartbeat), which
    /// looks like a double. A *real* height-2 stack (ready for a double clear)
    /// sits for many frames; the heartbeat only holds `0x02` briefly.
    ///
    /// Rules:
    /// 1. Height must be stable for `LINK_HEIGHT_STABLE_N` identical samples.
    /// 2. Per-step Δ capped at 4.
    /// 3. A drop only counts as garbage if the high height had enough *dwell*
    ///    (`LINK_HEIGHT_MIN_DWELL`). Height 2 is fully allowed — no min peak.
    fn track_a_height(&mut self, a_tx: u8) {
        if self.link_phase != LinkPhase::Gameplay {
            return;
        }
        if is_tetris_round_end(a_tx) || a_tx > 0x14 {
            return;
        }
        let height = a_tx;

        // Debounce: require N identical consecutive samples.
        if height == self.link_height_pending {
            self.link_height_pending_n = self.link_height_pending_n.saturating_add(1);
        } else {
            self.link_height_pending = height;
            self.link_height_pending_n = 1;
            return;
        }
        if self.link_height_pending_n < LINK_HEIGHT_STABLE_N {
            return;
        }
        // Confirmed sample of `height` (pending held long enough).
        let new_h = self.link_height_pending;
        let old_h = self.link_height_stable;

        if new_h == old_h {
            // Keep accruing dwell while height stays put.
            self.link_height_dwell = self.link_height_dwell.saturating_add(1);
            return;
        }

        if new_h > old_h {
            let rise = (new_h - old_h) as u32;
            // One piece can add at most ~4 rows of height in one update.
            if rise > 4 {
                console_link(&format!(
                    "[link] ignore height rise {}→{} (Δ{} > 4)",
                    old_h, new_h, rise
                ));
            }
            self.link_height_stable = new_h;
            self.link_prev_a_height = new_h;
            self.link_height_dwell = 0;
            return;
        }

        // new_h < old_h — possible clear / garbage.
        let drop = (old_h - new_h) as u32;
        let dwell = self.link_height_dwell;
        if dwell < LINK_HEIGHT_MIN_DWELL {
            console_link(&format!(
                "[link] ignore height drop {}→{} (dwell {} < {})",
                old_h, new_h, dwell, LINK_HEIGHT_MIN_DWELL
            ));
            self.link_height_stable = new_h;
            self.link_prev_a_height = new_h;
            self.link_height_dwell = 0;
            return;
        }
        // One clear removes at most 4 rows (tetris). Larger = protocol noise.
        if drop > 4 {
            console_link(&format!(
                "[link] ignore height drop {}→{} (Δ{} > 4)",
                old_h, new_h, drop
            ));
            self.link_height_stable = new_h;
            self.link_prev_a_height = new_h;
            self.link_height_dwell = 0;
            return;
        }
        // GB Tetris garbage: double→1, triple→2, tetris→4. Single→0.
        // Height 2 is a valid double — no minimum stack height required.
        let lines = match drop {
            1 => 0,
            2 => 1,
            3 => 2,
            4 => 4,
            _ => 0,
        };
        if lines > 0 {
            self.link_garbage_total += lines;
            console_link(&format!(
                "[link] A height {}→{} (Δ{}, dwell={}) → garbage {} line(s)",
                old_h, new_h, drop, dwell, lines
            ));
        }
        self.link_height_stable = new_h;
        self.link_prev_a_height = new_h;
        self.link_height_dwell = 0;
    }

    fn enter_gameplay(&mut self, reason: &str) {
        self.link_phase = LinkPhase::Gameplay;
        self.link_piece_streak = 0;
        self.link_piece_count = 0;
        self.link_prev_a_height = 0;
        self.link_height_pending = 0;
        self.link_height_pending_n = 0;
        self.link_height_stable = 0;
        self.link_height_dwell = 0;
        console_link(&format!(
            "[link] gameplay phase — height tracking active ({})",
            reason
        ));
    }

    /// Process any pending link-cable exchange immediately after a frame, so
    /// the partner's response arrives within the same JS tick rather than
    /// waiting until the next rAF callback.
    fn advance_link(&mut self) {
        // ── A is master, B is slave ───────────────────────────────────────────
        if self.emu_a.serial.transferring && self.emu_a.serial.master_waiting {
            // Capture TX *before* the exchange overwrites SB with the reply.
            let a_tx = self.emu_a.serial.sb;
            if self.emu_b.serial.slave_pending && !self.emu_b.serial.transferring {
                let sb_b = self.emu_b.serial.sb;
                if let Some(reply) = self.emu_b.link.slave_exchange(sb_b) {
                    self.emu_b.serial.slave_pending = false;
                    self.emu_b.serial.receive_byte(reply);
                    self.observe_protocol_byte(a_tx);
                    self.track_a_height(a_tx);
                }
            }
            if let Some(b_byte) = self.emu_a.link.poll_incoming() {
                self.emu_a.serial.receive_byte(b_byte);
            }
        }

        // ── B is master, A is slave (symmetric) ──────────────────────────────
        if self.emu_b.serial.transferring && self.emu_b.serial.master_waiting {
            let b_tx = self.emu_b.serial.sb;
            let a_tx = self.emu_a.serial.sb;
            if self.emu_a.serial.slave_pending && !self.emu_a.serial.transferring {
                if let Some(reply) = self.emu_a.link.slave_exchange(a_tx) {
                    self.emu_a.serial.slave_pending = false;
                    self.emu_a.serial.receive_byte(reply);
                    self.observe_protocol_byte(b_tx);
                    // A is slave: its height is in the slave reply TX.
                    self.track_a_height(a_tx);
                }
            }
            if let Some(a_byte) = self.emu_b.link.poll_incoming() {
                self.emu_b.serial.receive_byte(a_byte);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn framebuffer_to_rgba(fb: &[u8], palette_index: usize) -> Vec<u8> {
    let palette = &utils::palette::PALETTES[palette_index % utils::palette::PALETTES.len()];
    let mut rgba = Vec::with_capacity(160 * 144 * 4);
    for &shade in fb {
        let (r, g, b) = palette[(shade & 0x03) as usize];
        rgba.push(r);
        rgba.push(g);
        rgba.push(b);
        rgba.push(255);
    }
    rgba
}

fn map_button(v: u8) -> Option<emulator::joypad::GbButton> {
    use emulator::joypad::GbButton as B;
    match v {
        0 => Some(B::A),
        1 => Some(B::B),
        2 => Some(B::Select),
        3 => Some(B::Start),
        4 => Some(B::Up),
        5 => Some(B::Down),
        6 => Some(B::Left),
        7 => Some(B::Right),
        _ => None,
    }
}
