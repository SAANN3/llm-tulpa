use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sysinfo::System;

use crate::tools::base::{PropertyInfo, Tool, ToolError, ToolParams};

#[derive(Deserialize, tool_derive::ToolParams)]
struct CpuUsageArgs {}

pub struct CpuUsageTool;

#[derive(Serialize)]
struct CpuUsageOut {
    usage_percent: f64,
    num_cpus: u32,
    load_avg_1min: Option<f64>,
    load_avg_5min: Option<f64>,
    load_avg_15min: Option<f64>,
}

#[async_trait]
impl Tool for CpuUsageTool {
    fn function_name(&self) -> &str {
        "os.cpu_usage"
    }

    fn description(&self) -> &str {
        "Returns current CPU usage percentage, total number of logical CPUs, and system load \
         averages (1min, 5min, 15min). Useful for checking if the machine is under heavy load."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        CpuUsageArgs::tool_properties()
    }

    async fn call_untyped(&self, _data: Value) -> Result<Value, ToolError> {
        let mut sys = System::new_all();
        // A single refresh right after `new_all()` has no prior sample to diff against, so
        // `cpu_usage()` always reads back 0 — sysinfo needs two refreshes spaced at least
        // `MINIMUM_CPU_UPDATE_INTERVAL` apart to compute a real delta.
        sys.refresh_cpu_usage();
        tokio::time::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL).await;
        sys.refresh_cpu_usage();

        // Calculate average CPU usage across all cores
        let cpu_percent = if sys.cpus().is_empty() {
            0.0
        } else {
            let total: f64 = sys.cpus().iter().map(|c| c.cpu_usage() as f64).sum();
            total / sys.cpus().len() as f64
        };

        // Get load averages using sysinfo (platform-aware)
        let load_avg = System::load_average();

        Ok(serde_json::to_value(CpuUsageOut {
            usage_percent: cpu_percent,
            num_cpus: sys.cpus().len() as u32,
            load_avg_1min: Some(load_avg.one),
            load_avg_5min: Some(load_avg.five),
            load_avg_15min: Some(load_avg.fifteen),
        })?)
    }
}
