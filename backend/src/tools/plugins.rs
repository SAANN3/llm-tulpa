//! The `plugins` tool domain: what the model does with external plugins (separate programs the backend runs, see
//! `plugins/external`). It can look at what is installed and fetch a plugin to read what it is, but installing or removing one needs
//! the owner's approval, every time, and the owner has to be the user whose chat it is: an installed plugin runs as the owner, so
//! this is the owner's decision and nobody else's.
//!
//! The tools are made before the database exists (they are part of the tool list the backend starts with), and the plugins' manager
//! is made when it does, so the tools reach it through `install`, called by `bootstrap`; until then they say plugins are not available.

mod install;
mod list;
mod read_file;
mod stage;
mod uninstall;

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use serde_json::{Value, json};

use super::base::{ResolvedScope, ScopeGrant, Tool, ToolError, ToolPermission};
use crate::plugins::external::manager::ExternalPlugins;
use crate::services::user_store::UserStore;

pub fn collect() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(list::ListTool),
        Box::new(stage::StageTool),
        Box::new(read_file::ReadFileTool),
        Box::new(install::InstallTool),
        Box::new(uninstall::UninstallTool),
    ]
}

struct Link {
    plugins: Arc<ExternalPlugins>,
    users: Arc<UserStore>,
    /// The owner's id when it was last asked: a tool's permission check is synchronous and cannot ask the database, so it goes by
    /// this, and the call itself (which can) is the check that counts
    owner: RwLock<Option<i64>>,
}

static LINK: RwLock<Option<Arc<Link>>> = RwLock::new(None);

/// Hands the tools the plugins' manager, once the services exist (and again when they are made anew)
pub fn install(plugins: Arc<ExternalPlugins>, users: Arc<UserStore>) {
    *LINK.write().unwrap() = Some(Arc::new(Link { plugins, users, owner: RwLock::new(None) }));
}

fn link() -> Result<Arc<Link>, ToolError> {
    LINK.read().unwrap().clone().ok_or_else(|| ToolError::FailedUnknown("plugins are not available yet: the database is not set up".to_string()))
}

/// The manager, for a call by `user_id`: only the owner may use these tools, which is asked of the database each time
async fn plugins_for_owner(user_id: i64) -> Result<Arc<ExternalPlugins>, ToolError> {
    let link = link()?;
    let owner = link.users.owner_id().await.map_err(|e| ToolError::FailedUnknown(format!("could not tell who the owner is: {e:?}")))?;
    *link.owner.write().unwrap() = owner;
    if owner != Some(user_id) {
        return Err(ToolError::FailedUnknown("only the owner of this installation can use plugin tools: an installed plugin runs with the owner's rights".to_string()));
    }
    Ok(link.plugins.clone())
}

/// The permission side of an owner-only tool: refused outright for a user known not to be the owner (what is not yet known is
/// settled by the call), else the caller's own rule
fn refuse_non_owner(user_id: i64) -> Option<ToolPermission> {
    let owner = LINK.read().unwrap().as_ref().and_then(|link| *link.owner.read().unwrap());
    match owner {
        Some(owner) if owner != user_id => Some(ToolPermission::Denied {
            reason: "only the owner of this installation can use plugin tools: an installed plugin runs with the owner's rights".to_string(),
            escalation: None,
        }),
        _ => None,
    }
}

/// Where the staged files of `id` are, for the approval's text
fn staged_tree(id: &str) -> Option<PathBuf> {
    LINK.read().unwrap().as_ref().and_then(|link| link.plugins.staged_tree(id))
}

/// An approval for one thing, kept apart from approvals for any other: `kind` and `what` become `{kind: {what: true}}` in the
/// chat's grants, so allowing the install of one plugin never covers another, nor removing one cover removing it with its data
fn approval(kind: &str, what: &str, scope: &ResolvedScope, reason: String, ui_message: String) -> ToolPermission {
    let granted = scope.own.as_ref().and_then(|own| own.get(kind)).and_then(|kinds| kinds.get(what)).and_then(Value::as_bool) == Some(true);
    if granted {
        return ToolPermission::Allowed;
    }
    ToolPermission::Denied {
        reason,
        escalation: Some(ScopeGrant { scope: ResolvedScope { own: Some(json!({ kind: { what: true } })), shared: Default::default() }, ui_message }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowed(permission: &ToolPermission) -> bool {
        matches!(permission, ToolPermission::Allowed)
    }

    #[test]
    fn an_approval_covers_only_what_was_approved() {
        let none = ResolvedScope::default();
        let asked = approval("install", "0123456789abcdef", &none, "r".to_string(), "Install it?".to_string());
        let ToolPermission::Denied { escalation: Some(grant), .. } = asked else { panic!("a plugin not approved yet is asked about") };
        assert_eq!(grant.ui_message, "Install it?");
        let granted = grant.scope;
        assert!(allowed(&approval("install", "0123456789abcdef", &granted, "r".to_string(), "m".to_string())));
        // Another staged plugin, and the same name under another kind of approval, are asked about again
        assert!(!allowed(&approval("install", "fedcba9876543210", &granted, "r".to_string(), "m".to_string())));
        assert!(!allowed(&approval("uninstall", "0123456789abcdef", &granted, "r".to_string(), "m".to_string())));
    }

    #[test]
    fn removing_a_plugin_with_its_data_is_not_covered_by_removing_it() {
        let ToolPermission::Denied { escalation: Some(grant), .. } = approval("uninstall", "a/b", &ResolvedScope::default(), "r".to_string(), "m".to_string()) else {
            panic!("asked")
        };
        assert!(allowed(&approval("uninstall", "a/b", &grant.scope, "r".to_string(), "m".to_string())));
        assert!(!allowed(&approval("uninstall_with_data", "a/b", &grant.scope, "r".to_string(), "m".to_string())));
    }
}
