//! The Skills tab in Settings: what the user has, a drop zone to add more, and on each skill a switch that shows it to the
//! model or hides it, and buttons to read it, add files to it or delete it.
//! The user may delete any skill, including one the model made; the model's tools can only delete its own.

use serde_json::Value;

use super::pack::{self, SKILL_FILE};
use super::store::{Origin, SkillStore, SkillStoreError};
use crate::plugins::base::PluginError;
use crate::plugins::tab::{Block, Row, RowButton, RowSwitch, Section, TabFile, TabLayout, TabPopup};

const UPLOAD: &str = "upload";
const ADD_FILES: &str = "add_files";
const ENABLE: &str = "enable";
const VIEW: &str = "view";
const DELETE: &str = "delete";

/// A skill that is not there is a 404 and a refused request a 400; only a failure of the disk or the database is a 500.
fn failed(e: SkillStoreError) -> PluginError {
    match e {
        SkillStoreError::NotFound(_) => PluginError::NotFound(e.to_string()),
        SkillStoreError::Refused(_) | SkillStoreError::Invalid(_) => PluginError::InvalidState(e.to_string()),
        SkillStoreError::Io(_) | SkillStoreError::Db(_) => PluginError::FailedUnknown(e.to_string()),
    }
}

/// The tab as it is for `user_id`, with a line about what was just done and perhaps a text to show.
pub async fn layout(store: &SkillStore, user_id: i64, message: Option<String>, popup: Option<TabPopup>) -> Result<TabLayout, PluginError> {
    let rows = store
        .list(user_id)
        .await
        .map_err(failed)?
        .into_iter()
        .map(|skill| Row {
            id: skill.name.clone(),
            title: skill.name.clone(),
            subtitle: Some(skill.description),
            badge: Some(match skill.origin {
                Origin::User => "added by you".to_string(),
                Origin::Model => "made by the model".to_string(),
            }),
            switch: Some(RowSwitch { action: ENABLE.to_string(), on: skill.enabled }),
            buttons: vec![
                RowButton { action: VIEW.to_string(), label: "Read".to_string(), confirm: None, upload: false },
                RowButton { action: ADD_FILES.to_string(), label: "Add files".to_string(), confirm: None, upload: true },
                RowButton {
                    action: DELETE.to_string(),
                    label: "Delete".to_string(),
                    confirm: Some(format!("Delete the skill '{}' and all its files?", skill.name)),
                    upload: false,
                },
            ],
        })
        .collect();
    Ok(TabLayout {
        sections: vec![
            Section {
                title: Some("Add a skill".to_string()),
                blocks: vec![
                    Block::Text {
                        text: "A skill is a folder with a SKILL.md that says when to use it and how, and any files it refers to. \
                               The model sees each skill's name and description and reads the rest when a task matches. \
                               A skill that is switched off is hidden from the model and stays here."
                            .to_string(),
                        dim: true,
                    },
                    Block::Drop {
                        action: UPLOAD.to_string(),
                        label: "Drop a skill folder or a .zip here".to_string(),
                        help: Some("A skill with a name you already have replaces it.".to_string()),
                        accept: vec![],
                    },
                ],
            },
            Section { title: Some("Your skills".to_string()), blocks: vec![Block::Items { empty: "No skills yet.".to_string(), rows }] },
        ],
        message,
        popup,
    })
}

/// A button on a skill's row.
pub async fn action(store: &SkillStore, user_id: i64, action: &str, args: Value) -> Result<TabLayout, PluginError> {
    let name = args
        .get("row")
        .and_then(Value::as_str)
        .ok_or_else(|| PluginError::InvalidState("the button did not say which skill".to_string()))?;
    match action {
        ENABLE => {
            let on = args
                .get("on")
                .and_then(Value::as_bool)
                .ok_or_else(|| PluginError::InvalidState("the switch did not say on or off".to_string()))?;
            store.set_enabled(user_id, name, on, Origin::User).await.map_err(failed)?;
            layout(store, user_id, Some(format!("'{name}' is {} for the model.", if on { "on" } else { "off" })), None).await
        }
        VIEW => {
            let bytes = store.read_file(user_id, name, SKILL_FILE).await.map_err(failed)?;
            let text = String::from_utf8_lossy(&bytes).to_string();
            layout(store, user_id, None, Some(TabPopup { title: name.to_string(), text })).await
        }
        DELETE => {
            store.delete(user_id, name, Origin::User).await.map_err(failed)?;
            layout(store, user_id, Some(format!("Deleted '{name}'.")), None).await
        }
        other => Err(PluginError::NotFound(format!("the skills tab has no action '{other}'"))),
    }
}

/// Files that arrive in the tab: a folder or a zip dropped on the drop zone, or files chosen with a skill's own button.
/// What is wrong with them is told in the tab's message line, not as an error, since it is the person's files that need
/// fixing.
pub async fn upload(store: &SkillStore, user_id: i64, action: &str, row: Option<String>, files: Vec<TabFile>) -> Result<TabLayout, PluginError> {
    match (action, row) {
        (UPLOAD, _) => add_skill(store, user_id, files).await,
        (ADD_FILES, Some(name)) => add_files(store, user_id, &name, files).await,
        (ADD_FILES, None) => Err(PluginError::InvalidState("the files did not say which skill they are for".to_string())),
        (other, _) => Err(PluginError::NotFound(format!("the skills tab has no drop zone '{other}'"))),
    }
}

/// Files for a skill that is already there. They are written one by one, so when one is refused the ones before it stay.
async fn add_files(store: &SkillStore, user_id: i64, name: &str, files: Vec<TabFile>) -> Result<TabLayout, PluginError> {
    let count = files.len();
    for file in files {
        match store.add_file(user_id, name, &file.path, file.bytes, Origin::User).await {
            Ok(()) => {}
            Err(SkillStoreError::Invalid(why)) => return layout(store, user_id, Some(format!("Not added: {why}")), None).await,
            Err(other) => return Err(failed(other)),
        }
    }
    let what = if count == 1 { "1 file".to_string() } else { format!("{count} files") };
    layout(store, user_id, Some(format!("Added {what} to '{name}'.")), None).await
}

/// A folder or a zip dropped on the drop zone: a whole skill.
async fn add_skill(store: &SkillStore, user_id: i64, files: Vec<TabFile>) -> Result<TabLayout, PluginError> {
    // Unpacking a zip is CPU work, kept off the async runtime
    let parsed = tokio::task::spawn_blocking(move || {
        let files = files.into_iter().map(|file| (file.path, file.bytes)).collect();
        pack::from_files(files).and_then(pack::Pack::into_skill)
    })
    .await
    .map_err(|e| PluginError::FailedUnknown(format!("reading the files failed: {e}")))?;
    let skill = match parsed {
        Ok(skill) => skill,
        Err(why) => return layout(store, user_id, Some(format!("Not added: {why}")), None).await,
    };
    let replaced = store.get(user_id, &skill.name).await.is_ok();
    let (name, count) = (skill.name.clone(), skill.files.len());
    match store.save(user_id, &skill.name, &skill.description, skill.files, Origin::User).await {
        Ok(_) => {
            let verb = if replaced { "Replaced" } else { "Added" };
            layout(store, user_id, Some(format!("{verb} '{name}' ({count} files).")), None).await
        }
        Err(SkillStoreError::Invalid(why)) => layout(store, user_id, Some(format!("Not added: {why}")), None).await,
        Err(other) => Err(failed(other)),
    }
}
