//! Pending tool calls as the HTTP shape, and the user's answer to one (`Decision`).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

/// Facade-owned mirror of `tools::base::ScopeGrant` — the tools layer's type is not the HTTP shape.
#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct AgentScopeGrant {
    #[schema(value_type = Object)]
    pub scope: Value,
    pub ui_message: String,
}

/// Whether this call is allowed to run under the chat's grants.
#[derive(Clone, Debug, Serialize, ToSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AgentToolPermission {
    Allowed,
    Denied {
        reason: String,
        escalation: Option<AgentScopeGrant>,
    },
}

#[derive(Serialize, ToSchema)]
pub struct AgentToolCall {
    pub permission: AgentToolPermission,
    pub name: String,
    #[schema(value_type = Object)]
    pub arguments: Value,
}

impl AgentToolCall {
    pub fn offered_grant(&self) -> Option<&AgentScopeGrant> {
        match &self.permission {
            AgentToolPermission::Denied { escalation: Some(grant), .. } => Some(grant),
            _ => None,
        }
    }

    /// A grant the user could give, or a plain refusal of a call that offers nothing.
    pub fn check_decision(&self, decision: &Decision) -> Result<(), String> {
        if self.offered_grant().is_some() || decision.allowance == Allowance::Deny {
            Ok(())
        } else {
            Err("this pending call offers nothing that could be granted".to_string())
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Allowance {
    Permanent,
    OnlyNow,
    Deny,
}

/// The answer to one pending tool call: `index` is its position in `TurnState::pending`.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, ToSchema)]
pub struct Decision {
    pub index: usize,
    pub allowance: Allowance,
}
