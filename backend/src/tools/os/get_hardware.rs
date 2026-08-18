use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;
use sysinfo::System;

use crate::tools::base::{PropertyInfo, Tool, ToolError};

use super::format_gb;

pub struct GetHardwareTool;

#[derive(Serialize)]
struct HardwareOut {
    cpu: String,
    gpu: Option<String>,
    os_name: String,
    max_ram_memory: String,
    current_ram_memory: String,
    max_gpu_memory: Option<String>,
    current_gpu_memory: Option<String>,
}

#[async_trait]
impl Tool for GetHardwareTool {
    fn function_name(&self) -> &str {
        "os.get_hardware"
    }

    fn description(&self) -> &str {
        "Reads hardware info for the machine this backend runs on: CPU model, GPU model, \
         total and currently-used system RAM, and total and currently-used GPU VRAM, plus \
         the OS name. GPU fields come back null if no supported GPU could be detected. \
         Takes no arguments."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        vec![]
    }

    async fn call_untyped(&self, _data: Value) -> Result<Value, ToolError> {
        let mut sys = System::new_all();
        sys.refresh_all();

        let cpu = sys
            .cpus()
            .first()
            .map(|cpu| cpu.brand().to_string())
            .unwrap_or_default();

        let os_name = System::name().unwrap_or_else(|| "unknown".to_string());

        let gpu = gfxinfo::active_gpu().ok();
        let (max_gpu_memory, current_gpu_memory) = gpu
            .as_ref()
            .map(|gpu| {
                let info = gpu.info();
                (Some(format_gb(info.total_vram())), Some(format_gb(info.used_vram())))
            })
            .unwrap_or((None, None));

        let out = HardwareOut {
            cpu,
            gpu: gpu.as_ref().map(|gpu| gpu.model().to_string()),
            os_name,
            max_ram_memory: format_gb(sys.total_memory()),
            current_ram_memory: format_gb(sys.used_memory()),
            max_gpu_memory,
            current_gpu_memory,
        };

        Ok(serde_json::to_value(out)?)
    }
}
