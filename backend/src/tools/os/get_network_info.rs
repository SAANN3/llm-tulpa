use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::tools::base::{PropertyInfo, Tool, ToolError, ToolParams};

#[derive(Deserialize, tool_derive::ToolParams)]
struct GetNetworkInfoArgs {}

#[derive(Serialize)]
struct NetworkInterface {
    name: String,
    ip_addresses: Vec<String>,
    mac_address: Option<String>,
    is_up: bool,
}

pub struct GetNetworkInfoTool;

#[async_trait]
impl Tool for GetNetworkInfoTool {
    fn function_name(&self) -> &str {
        "os.get_network_info"
    }

    fn description(&self) -> &str {
        "Lists network interfaces and their IP addresses on the machine. Returns interface names, all assigned IPv4/IPv6 addresses, MAC addresses where available, and whether each interface is up or down."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        GetNetworkInfoArgs::tool_properties()
    }

    async fn call_untyped(&self, _data: Value) -> Result<Value, ToolError> {
        let networks: Vec<NetworkInterface> = {
            let nets = sysinfo::Networks::new_with_refreshed_list();
            nets.iter()
                .map(|(name, data)| {
                    let mac = data.mac_address();
                    NetworkInterface {
                        name: name.to_string(),
                        ip_addresses: data
                            .ip_networks()
                            .iter()
                            .map(|net| net.addr.to_string())
                            .collect(),
                        mac_address: (!mac.is_unspecified()).then(|| mac.to_string()),
                        is_up: data.operational_state()
                            == sysinfo::InterfaceOperationalState::Up,
                    }
                })
                .collect()
        };

        Ok(serde_json::to_value(networks)?)
    }
}
