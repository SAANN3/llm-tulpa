//! A plugin's own tab in Settings, described as data. The plugin says that it has a tab (`Plugin::settings_tab`) and what
//! the tab holds for the user who opens it (`tab_layout`); the frontend draws that with one component, in the style of the
//! other Settings pages, so a plugin needs no frontend code of its own. What the user does in it goes back to the plugin
//! (`tab_action`, `tab_upload`), which answers with the layout as it is now.
//!
//! The vocabulary is small on purpose: a titled section holds text, a switch, a list of rows that carry buttons (and a
//! switch of their own), and a drop zone for files. A plugin that needs more adds a block here and the one component learns
//! to draw it.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

/// The ids the Settings page uses for its own tabs: a plugin's tab cannot take one.
pub const RESERVED_TAB_IDS: [&str; 5] = ["behaviour", "appearance", "profile", "models", "voice"];

/// What a plugin says about its tab.
#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct TabInfo {
    /// The tab's place in the address (`/settings/<id>`): lowercase letters, digits and hyphens
    pub id: String,
    pub title: String,
}

/// A tab of an enabled plugin, as the Settings page lists it.
#[derive(Serialize, ToSchema)]
pub struct PluginTab {
    pub plugin_name: String,
    pub plugin_subname: String,
    pub id: String,
    pub title: String,
}

/// Everything a tab shows, for one user.
#[derive(Default, Serialize, ToSchema)]
pub struct TabLayout {
    pub sections: Vec<Section>,
    /// A line after an action: what was done, or why it was not
    pub message: Option<String>,
    /// A text to show in a popup, such as a skill's instructions
    pub popup: Option<TabPopup>,
}

#[derive(Serialize, ToSchema)]
pub struct Section {
    pub title: Option<String>,
    pub blocks: Vec<Block>,
}

#[derive(Serialize, ToSchema)]
pub struct TabPopup {
    pub title: String,
    pub text: String,
}

/// One thing in a section. Every control carries an `action`, the word it sends back to the plugin.
#[derive(Serialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Block {
    /// A line of text; `dim` draws it as a hint
    Text { text: String, dim: bool },
    /// A labeled switch. Sends `action` with `{"on": bool}`.
    Switch { action: String, label: String, help: Option<String>, on: bool },
    /// A list of rows, each with a title, buttons and perhaps a switch; `empty` is said when there are none
    Items { empty: String, rows: Vec<Row> },
    /// A drop zone for files and folders. Sends `action` with the files; `accept` lists the file endings it takes
    /// (`.zip`, `.md`), or all when empty.
    Drop { action: String, label: String, help: Option<String>, accept: Vec<String> },
}

#[derive(Serialize, ToSchema)]
pub struct Row {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    /// A short label beside the title, such as where the item came from
    pub badge: Option<String>,
    pub switch: Option<RowSwitch>,
    pub buttons: Vec<RowButton>,
}

/// A row's switch. Sends `action` with `{"row": id, "on": bool}`.
#[derive(Serialize, ToSchema)]
pub struct RowSwitch {
    pub action: String,
    pub on: bool,
}

/// A row's button. Sends `action` with `{"row": id}`; `confirm`, when set, is asked in a popup first. An `upload` button
/// opens a file picker instead and sends the chosen files to the plugin's upload (`tab_upload`) with the row's id.
#[derive(Serialize, ToSchema)]
pub struct RowButton {
    pub action: String,
    pub label: String,
    pub confirm: Option<String>,
    pub upload: bool,
}

/// What a control sends back (`POST /api/plugins/tab/action`).
#[derive(Deserialize, ToSchema)]
pub struct TabActionBody {
    pub plugin_name: String,
    pub plugin_subname: String,
    pub action: String,
    #[schema(value_type = Object)]
    #[serde(default)]
    pub args: Value,
}

/// One file of an upload: its path as the user's browser gave it (a folder's files keep their folder), and its bytes.
pub struct TabFile {
    pub path: String,
    pub bytes: Vec<u8>,
}

/// Whether `id` can be a tab's place in the address.
pub fn valid_tab_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 40 && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tab_id_is_a_plain_slug() {
        assert!(valid_tab_id("skills"));
        assert!(valid_tab_id("my-tab-2"));
        assert!(!valid_tab_id(""));
        assert!(!valid_tab_id("Skills"));
        assert!(!valid_tab_id("a b"));
        assert!(!valid_tab_id("a/b"));
        assert!(!valid_tab_id(&"x".repeat(41)));
    }

    #[test]
    fn a_block_is_sent_with_its_type() {
        let block = Block::Switch { action: "on".into(), label: "L".into(), help: None, on: true };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "switch");
        assert_eq!(json["on"], true);
        assert_eq!(serde_json::to_value(Block::Text { text: "t".into(), dim: false }).unwrap()["type"], "text");
    }
}
