// Emulator state – WASM build.
// File I/O removed; serialise/deserialise from JS via bincode bytes.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct EmulatorState {
    pub cpu_a: u8, pub cpu_b: u8, pub cpu_c: u8,
    pub cpu_d: u8, pub cpu_e: u8, pub cpu_h: u8, pub cpu_l: u8,
    pub cpu_f: u8, pub cpu_sp: u16, pub cpu_pc: u16,
    pub cpu_ime: bool, pub cpu_ime_scheduled: bool, pub cpu_halted: bool,

    pub vram: Vec<u8>, pub eram: Vec<u8>, pub ram: Vec<u8>,
    pub oam: Vec<u8>,  pub io: Vec<u8>,   pub hram: Vec<u8>,
    pub ie: u8,

    pub mbc1_rom_bank: usize, pub mbc1_ram_bank: usize,
    pub mbc1_ram_enable: bool, pub mbc1_mode: bool,

    pub timer_div: u16, pub timer_tima: u8, pub timer_tma: u8,
    pub timer_tac: u8,  pub timer_tima_cycles: u32,

    pub gpu_mode: u8, pub gpu_line: u8, pub gpu_cycles: u32,
    pub cycles: u64,
}
