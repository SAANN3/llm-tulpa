use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sysinfo::System;

use crate::tools::base::{PropertyInfo, PropertyType, Tool, ToolError, ToolParams};

#[derive(Deserialize, tool_derive::ToolParams)]
struct GetProcessListArgs {
    #[tool(description = "Optional PID to filter — only returns this specific process. Omit for all processes.")]
    pid: Option<u32>,
    #[tool(description = "Maximum number of processes to return (defaults to 100 if omitted). Use a higher value for long lists, but be aware very large results may exceed context limits.")]
    limit: Option<usize>,
}

pub struct GetProcessListTool;

#[derive(Serialize)]
struct ProcessInfo {
    pid: u32,
    name: String,
    cpu_percent: f64,
    memory_bytes: u64,
    status: String,
}

#[async_trait]
impl Tool for GetProcessListTool {
    fn function_name(&self) -> &str {
        "os.get_process_list"
    }

    fn description(&self) -> &str {
        "Lists running processes on the machine, sorted by CPU usage (highest first). Returns \
         PID, name, CPU percentage, memory usage in bytes, and status for each process. Can \
         optionally filter to a single PID or limit the number of results."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        GetProcessListArgs::tool_properties()
    }

    async fn call_untyped(&self, data: Value) -> Result<Value, ToolError> {
        let args: GetProcessListArgs = serde_json::from_value(data)?;
        let limit = args.limit.unwrap_or(100);

        let mut sys = System::new_all();
        sys.refresh_all();

        let processes: Vec<ProcessInfo> = if let Some(target_pid) = args.pid {
            sys.process(sysinfo::Pid::from(target_pid as usize))
                .map(|proc| {
                    vec![ProcessInfo {
                        pid: target_pid,
                        name: proc.name().to_string_lossy().to_string(),
                        cpu_percent: proc.cpu_usage() as f64,
                        memory_bytes: proc.memory(),
                        status: format!("{:?}", proc.status()),
                    }]
                })
                .unwrap_or_default()
        } else {
            let mut list: Vec<ProcessInfo> = sys
                .processes()
                .iter()
                .map(|(pid, proc)| ProcessInfo {
                    pid: pid.as_u32(),
                    name: proc.name().to_string_lossy().to_string(),
                    cpu_percent: proc.cpu_usage() as f64,
                    memory_bytes: proc.memory(),
                    status: format!("{:?}", proc.status()),
                })
                .collect();
            // `sys.processes()` is a HashMap — iteration order is arbitrary, so without an
            // explicit sort `.take(limit)` was returning an effectively random subset rather
            // than the processes actually worth surfacing.
            list.sort_by(|a, b| b.cpu_percent.partial_cmp(&a.cpu_percent).unwrap_or(std::cmp::Ordering::Equal));
            list.truncate(limit);
            list
        };

        Ok(serde_json::to_value(processes)?)
    }
}
