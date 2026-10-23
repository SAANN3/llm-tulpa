//! The Skills tab in Settings: what the user has, a drop zone to add more, and a button on each skill to read or delete it.
//! The user may delete any skill, including one the model made; the model's tools can only delete its own.

use serde_json::Value;

use super::pack::{self, SKILL_FILE};
use super::store::{Origin, SkillStore, SkillStoreError};
use crate::plugins::base::PluginError;
use crate::plugins::tab::{Block, Row, RowButton, Section, TabFile, TabLayout, TabPopup};

const UPLOAD: &str = "upload";
const VIEW: &str = "view";
const DELETE: &str = "delete";

fn failed(e: SkillStoreError) -> PluginError {
    PluginError::FailedUnknown(e.to_string())
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
            switch: None,
            buttons: vec![
                RowButton { action: VIEW.to_string(), label: "Read".to_string(), confirm: None },
                RowButton {
                    action: DELETE.to_string(),
                    label: "Delete".to_string(),
                    confirm: Some(format!("Delete the skill '{}' and all its files?", skill.name)),
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
                               The model sees each skill's name and description and reads the rest when a task matches."
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

/// A folder or a zip dropped on the drop zone. What is wrong with it is told in the tab's message line, not as an error,
/// since it is the person's file that needs fixing.
pub async fn upload(store: &SkillStore, user_id: i64, action: &str, files: Vec<TabFile>) -> Result<TabLayout, PluginError> {
    if action != UPLOAD {
        return Err(PluginError::NotFound(format!("the skills tab has no drop zone '{action}'")));
    }
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
