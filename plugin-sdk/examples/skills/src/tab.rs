//! The Skills tab in Settings: what the user has, a drop zone to add more, and on each skill a switch that shows it to the
//! model or hides it, and buttons to read it, add files to it or delete it.
//! The user may delete any skill, including one the model made; the model's tools can only delete its own.

use tulpa_plugin::{Block, Failure, Row, Section, Tab, UploadedFile, Value};

use crate::pack::{self, SKILL_FILE};
use crate::store::{Origin, SkillStore, SkillStoreError};

const UPLOAD: &str = "upload";
const ADD_FILES: &str = "add_files";
const ENABLE: &str = "enable";
const VIEW: &str = "view";
const DELETE: &str = "delete";

/// The tab as it is for `user_id`, with a line about what was just done and perhaps a text to show (a title and a text).
pub async fn layout(store: &SkillStore, user_id: i64, message: Option<String>, popup: Option<(String, String)>) -> Result<Tab, Failure> {
    let rows = store
        .list(user_id)
        .await?
        .into_iter()
        .map(|skill| {
            Row::new(&skill.name, &skill.name)
                .subtitle(skill.description)
                .badge(match skill.origin {
                    Origin::User => "added by you",
                    Origin::Model => "made by the model",
                })
                .switch(ENABLE, skill.enabled)
                .button(VIEW, "Read")
                .upload_button(ADD_FILES, "Add files")
                .confirm_button(DELETE, "Delete", format!("Delete the skill '{}' and all its files?", skill.name))
        })
        .collect();
    let mut tab = Tab::new()
        .section(
            Section::new("Add a skill")
                .block(Block::hint(
                    "A skill is a folder with a SKILL.md that says when to use it and how, and any files it refers to. \
                     The model sees each skill's name and description and reads the rest when a task matches. \
                     A skill that is switched off is hidden from the model and stays here.",
                ))
                .block(Block::drop_zone(UPLOAD, "Drop a skill folder or a .zip here", &[]).help("A skill with a name you already have replaces it.")),
        )
        .section(Section::new("Your skills").block(Block::items("No skills yet.", rows)));
    if let Some(message) = message {
        tab = tab.message(message);
    }
    if let Some((title, text)) = popup {
        tab = tab.popup(title, text);
    }
    Ok(tab)
}

/// A button on a skill's row.
pub async fn action(store: &SkillStore, user_id: i64, action: &str, args: Value) -> Result<Tab, Failure> {
    let name = args.get("row").and_then(Value::as_str).ok_or_else(|| Failure::bad_args("the button did not say which skill"))?;
    match action {
        ENABLE => {
            let on = args.get("on").and_then(Value::as_bool).ok_or_else(|| Failure::bad_args("the switch did not say on or off"))?;
            store.set_enabled(user_id, name, on, Origin::User).await?;
            layout(store, user_id, Some(format!("'{name}' is {} for the model.", if on { "on" } else { "off" })), None).await
        }
        VIEW => {
            let bytes = store.read_file(user_id, name, SKILL_FILE).await?;
            let text = String::from_utf8_lossy(&bytes).to_string();
            layout(store, user_id, None, Some((name.to_string(), text))).await
        }
        DELETE => {
            store.delete(user_id, name, Origin::User).await?;
            layout(store, user_id, Some(format!("Deleted '{name}'.")), None).await
        }
        other => Err(Failure::not_found(format!("the skills tab has no action '{other}'"))),
    }
}

/// Files that arrive in the tab: a folder or a zip dropped on the drop zone, or files chosen with a skill's own button.
/// What is wrong with them is told in the tab's message line, not as an error, since it is the person's files that need
/// fixing.
pub async fn upload(store: &SkillStore, user_id: i64, action: &str, row: Option<String>, files: Vec<UploadedFile>) -> Result<Tab, Failure> {
    // The files wait on disk only until this call returns, so they are read now
    let mut read = Vec::with_capacity(files.len());
    for file in files {
        let bytes = tokio::fs::read(&file.file).await.map_err(|e| Failure::from(format!("could not read an uploaded file: {e}")))?;
        read.push((file.path, bytes));
    }
    match (action, row) {
        (UPLOAD, _) => add_skill(store, user_id, read).await,
        (ADD_FILES, Some(name)) => add_files(store, user_id, &name, read).await,
        (ADD_FILES, None) => Err(Failure::bad_args("the files did not say which skill they are for")),
        (other, _) => Err(Failure::not_found(format!("the skills tab has no drop zone '{other}'"))),
    }
}

/// Files for a skill that is already there. They are written one by one, so when one is refused the ones before it stay.
async fn add_files(store: &SkillStore, user_id: i64, name: &str, files: Vec<(String, Vec<u8>)>) -> Result<Tab, Failure> {
    let count = files.len();
    for (path, bytes) in files {
        match store.add_file(user_id, name, &path, bytes, Origin::User).await {
            Ok(()) => {}
            Err(SkillStoreError::Invalid(why)) => return layout(store, user_id, Some(format!("Not added: {why}")), None).await,
            Err(other) => return Err(other.into()),
        }
    }
    let what = if count == 1 { "1 file".to_string() } else { format!("{count} files") };
    layout(store, user_id, Some(format!("Added {what} to '{name}'.")), None).await
}

/// A folder or a zip dropped on the drop zone: a whole skill.
async fn add_skill(store: &SkillStore, user_id: i64, files: Vec<(String, Vec<u8>)>) -> Result<Tab, Failure> {
    // Unpacking a zip is CPU work, kept off the async runtime
    let parsed = tokio::task::spawn_blocking(move || pack::from_files(files).and_then(pack::Pack::into_skill))
        .await
        .map_err(|e| Failure::from(format!("reading the files failed: {e}")))?;
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
        Err(other) => Err(other.into()),
    }
}
