fn main() {
    // Compact build id (local time): YYYYMMDDHHMM — updates on every cargo build.
    // No cargo:rerun-if-changed so Cargo re-runs this script every compile.
    let now = chrono::Local::now();
    let build_id = now.format("%Y%m%d%H%M").to_string();
    println!("cargo:rustc-env=BUILD_ID={}", build_id);
}
