//! Misdrop fixture paths for unit tests (`tests/fixtures/misdrop/`).

/// Path to a file under `gbmul-core/tests/fixtures/misdrop/`.
pub fn misdrop_fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/misdrop")
        .join(name)
}

/// Rosy ROM used by Tier-3 emulator golden tests (`www/game.gb`).
#[cfg(any(test, feature = "emulator_tests"))]
pub fn rosy_rom_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../www/game.gb")
}

/// Load Rosy ROM bytes for emulator tests.
#[cfg(any(test, feature = "emulator_tests"))]
pub fn load_rosy_rom() -> Vec<u8> {
    let path = rosy_rom_path();
    std::fs::read(&path).unwrap_or_else(|e| panic!("read Rosy ROM {}: {e}", path.display()))
}

/// Restore savestate on an emulator with Rosy ROM mapped (required for `run_frame` to execute game code).
#[cfg(any(test, feature = "emulator_tests"))]
pub fn emulator_from_savestate(state: &crate::state::EmulatorState) -> crate::emulator::Emulator {
    let mut emu = crate::emulator::Emulator::new();
    emu.load_rom(&load_rosy_rom()).expect("load Rosy ROM");
    emu.restore_state(state);
    emu
}