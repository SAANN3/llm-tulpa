pub mod cpu_usage;
pub mod env_read;
pub mod env_write;
pub mod execute_command;
pub mod get_date;
pub mod get_disk_space;
pub mod get_hardware;
pub mod get_network_info;
pub mod get_process_list;
pub mod get_user_info;
pub mod job_kill;
pub mod job_output;
mod jobs;
pub mod list_jobs;
pub mod shell;
pub mod start_job;

use super::base::Tool;

/// Every tool in the `os` domain (function names prefixed `os.`), for `main.rs` to
/// register alongside every other domain's `collect()`.
pub fn collect() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(get_hardware::GetHardwareTool),
        Box::new(get_disk_space::GetDiskSpaceTool),
        Box::new(get_process_list::GetProcessListTool),
        Box::new(get_network_info::GetNetworkInfoTool),
        Box::new(execute_command::ExecuteCommandTool),
        Box::new(start_job::StartJobTool),
        Box::new(job_output::JobOutputTool),
        Box::new(job_kill::JobKillTool),
        Box::new(list_jobs::ListJobsTool),
        Box::new(cpu_usage::CpuUsageTool),
        Box::new(get_user_info::GetUserInfoTool),
        Box::new(env_read::EnvReadTool),
        Box::new(env_write::EnvWriteTool),
        Box::new(get_date::GetDateTool),
    ]
}

/// Shared by every `os` tool that reports a byte count — keeps units consistent
/// across the domain (e.g. hardware RAM/VRAM and disk space read the same way).
fn format_gb(bytes: u64) -> String {
    format!("{:.1}gb", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
}
