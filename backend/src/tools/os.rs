pub mod get_disk_space;
pub mod get_hardware;

use super::base::Tool;

/// Every tool in the `os` domain (function names prefixed `os.`), for `main.rs` to
/// register alongside every other domain's `collect()`.
pub fn collect() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(get_hardware::GetHardwareTool),
        Box::new(get_disk_space::GetDiskSpaceTool),
    ]
}

/// Shared by every `os` tool that reports a byte count — keeps units consistent
/// across the domain (e.g. hardware RAM/VRAM and disk space read the same way).
fn format_gb(bytes: u64) -> String {
    format!("{:.1}gb", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
}
