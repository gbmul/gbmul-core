// Custom logger utilities
// Enhanced logging for debugging and telemetry

use log::LevelFilter;
use env_logger::Builder;
use std::io::Write;

pub fn init_logger() {
    Builder::new()
        .filter_level(LevelFilter::Info)
        .format(|buf, record| {
            writeln!(
                buf,
                "[{} {} {}:{}] {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                record.file().unwrap_or("unknown"),
                record.line().unwrap_or(0),
                record.args()
            )
        })
        .init();
}
