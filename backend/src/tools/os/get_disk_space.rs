use std::collections::HashSet;
use std::path::Path;

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;
use sysinfo::Disks;

use crate::tools::base::{PropertyInfo, Tool, ToolError};

use super::format_gb;

pub struct GetDiskSpaceTool;

#[derive(Serialize)]
struct DiskOut {
    filesystem: Option<String>,
    mount_point: String,
    size: String,
    used: String,
    available: String,
}

#[async_trait]
impl Tool for GetDiskSpaceTool {
    fn function_name(&self) -> &str {
        "os.get_disk_space"
    }

    fn description(&self) -> &str {
        "Lists every mounted disk/volume on the machine this backend runs on, like the \
         `df` command: filesystem type, mount point, total size, used space, and \
         available space. Pseudo filesystems reporting zero capacity are omitted. Takes \
         no arguments."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        vec![]
    }

    async fn call_untyped(&self, _data: Value) -> Result<Value, ToolError> {
        let disks = Disks::new_with_refreshed_list();
        // When dockerized, the host's real root filesystem is bind-mounted read-only at
        // this path purely so this tool can see genuine host disk stats instead of the
        // container's own (irrelevant to the user) overlay storage — see compose.yaml.
        // Unset on a native run, where paths are already the real ones.
        let host_root = std::env::var("HOST_ROOT").ok();

        let mut seen_mount_points = HashSet::new();
        let out: Vec<DiskOut> = disks
            .iter()
            .filter(|disk| disk.total_space() > 0)
            // Docker's own storage layer for the container's writable root — never
            // meaningful to report as "disk space on this machine".
            .filter(|disk| disk.file_system().to_str() != Some("overlay"))
            .map(|disk| {
                let total = disk.total_space();
                let available = disk.available_space();

                DiskOut {
                    filesystem: disk.file_system().to_str().map(|s| s.to_string()),
                    mount_point: display_mount_point(disk.mount_point(), host_root.as_deref()),
                    size: format_gb(total),
                    used: format_gb(total.saturating_sub(available)),
                    available: format_gb(available),
                }
            })
            // The host-root passthrough (see `display_mount_point`) can surface the same
            // real host mount twice — once directly, once as a submount discovered under
            // `HOST_ROOT` — collapse those back to one entry.
            .filter(|disk| seen_mount_points.insert(disk.mount_point.clone()))
            // Also picked up via the same recursive submount discovery: Docker's own data
            // directory (each running container's storage backing directory) — real paths
            // on the host disk, but never meaningful as "disk space on this machine".
            .filter(|disk| !disk.mount_point.starts_with("/var/lib/docker"))
            .collect();

        Ok(serde_json::to_value(out)?)
    }
}

/// Maps a mount point as seen from inside the container back to its real host path.
/// `host_root` is where the host's `/` is bind-mounted (e.g. `/hostfs`) — that path
/// itself maps back to `/`, and anything under it has that prefix stripped, so the
/// tool's output reads the same whether it's running natively or dockerized.
fn display_mount_point(mount_point: &Path, host_root: Option<&str>) -> String {
    let raw = mount_point.to_string_lossy();
    match host_root {
        Some(root) if raw == root => "/".to_string(),
        Some(root) => raw.strip_prefix(root).map(str::to_string).unwrap_or_else(|| raw.to_string()),
        None => raw.to_string(),
    }
}
