//! What the model can do with skills. Every tool works on the skills of the user whose chat the call is in (`ctx.user_id`),
//! and the ones that change anything change only what the model made itself (`Origin::Model`): a skill the user uploaded
//! is read-only to it, which `SkillStore` enforces. The tools that change files say so in their spec (`access_folder`), so
//! the backend asks the user to approve the folder before a call runs.
//!
//! The tools are named `example_skills.*`, not `skills.*`, so that this port and the built-in plugin can both be on.

use std::path::PathBuf;

use tulpa_plugin::{Bucket, Failure, Param, ToolSpec, Value, json};

use crate::format;
use crate::pack::{self, SKILL_FILE};
use crate::store::{Origin, SkillStore};

/// Where a user's skills are: the folder the tools that change them ask the user to approve
const USER_FOLDER: &str = "{data_dir}/{user_id}";

pub fn specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec::new(
            "example_skills.list",
            "Lists the user's skills, each with its description. The list in your instructions was made when this chat started, so \
             call this to see a skill that was added, written or deleted since.",
        ),
        ToolSpec::new(
            "example_skills.read",
            "Reads a skill's instructions (its SKILL.md) and lists the skill's other files. Check the list of skills in your \
             instructions before starting a task; when a description matches the task, read that skill first and follow it. Its \
             other files are opened with example_skills.read_file, only when the instructions send you to them.",
        )
        .param(Param::string("name", "The skill's name, as in the list of skills in your instructions.")),
        ToolSpec::new(
            "example_skills.read_file",
            "Reads one text file of a skill, by the path example_skills.read listed. Only for a file the skill's instructions refer to.",
        )
        .param(Param::string("name", "The skill's name, as in the list of skills in your instructions."))
        .param(Param::string("path", "The file's path inside the skill, as example_skills.read listed it (for example scripts/run.sh).")),
        ToolSpec::new(
            "example_skills.write",
            "Saves a skill: instructions for a kind of task, to read the next time one comes up. Before writing, check \
             example_skills.list for a name already in use: writing a name you made earlier replaces that skill, and a skill the user \
             added cannot be changed, so it needs another name. Other files for the skill are added with example_skills.add_file.",
        )
        .param(Param::string("name", "The skill's name: lowercase letters and digits, in words joined by hyphens (for example lab-report)."))
        .param(Param::string(
            "description",
            "One or two sentences on when to use the skill. This is all that is shown in your instructions, so it has to be enough to \
             tell whether a task matches.",
        ))
        .param(Param::string("content", "The skill's instructions, in markdown, without a header: the steps to follow and what to watch for."))
        .access_folder(Bucket::Write, USER_FOLDER),
        ToolSpec::new(
            "example_skills.add_file",
            "Adds a text file to a skill you wrote (a script, a template, notes the instructions refer to), or replaces the file of \
             that path. The skill's instructions are changed with example_skills.write, not here.",
        )
        .param(Param::string("name", "The name of a skill you wrote."))
        .param(Param::string("path", "Where the file goes inside the skill, for example scripts/run.sh or notes/format.md. Not SKILL.md."))
        .param(Param::string("content", "The file's text."))
        .access_folder(Bucket::Write, USER_FOLDER),
        ToolSpec::new(
            "example_skills.delete",
            "Deletes a skill you wrote, with all its files. A skill the user added cannot be deleted by you.",
        )
        .param(Param::string("name", "The name of a skill you wrote."))
        .access_folder(Bucket::Delete, USER_FOLDER),
        ToolSpec::new(
            "example_skills.disable",
            "Turns off a skill you wrote, for when it is outdated or wrong but the user may want it back: it leaves your list of \
             skills and you cannot read it, while its files are kept and the user can turn it on again in Settings. A skill the \
             user added cannot be turned off by you. To remove a skill for good, use example_skills.delete.",
        )
        .param(Param::string("name", "The name of a skill you wrote."))
        .access_folder(Bucket::Write, USER_FOLDER),
        // Reading the source and writing into the skills folder are two approvals, asked one at a time
        ToolSpec::new(
            "example_skills.install",
            "Installs a skill from a folder or a .zip on this machine, for example one that was downloaded. The SKILL.md in it \
             names the skill and says when to use it. A skill the user added under the same name is not replaced.",
        )
        .param(Param::string("path", "Path of a folder that holds a SKILL.md (with the files it refers to), or of a .zip of one, on this machine."))
        .access_path_arg(Bucket::Read, "path")
        .access_folder(Bucket::Write, USER_FOLDER),
    ]
}

fn text_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str, Failure> {
    args.get(key).and_then(Value::as_str).ok_or_else(|| Failure::bad_args(format!("the argument '{key}' is missing or is not text")))
}

/// A file's text, or why the model cannot have it as text.
fn as_text(path: &str, bytes: Vec<u8>) -> Result<String, Failure> {
    String::from_utf8(bytes).map_err(|e| Failure::from(format!("'{path}' is not text ({} bytes)", e.as_bytes().len())))
}

pub async fn call(store: &SkillStore, user_id: i64, tool: &str, args: Value) -> Result<Value, Failure> {
    match tool {
        "example_skills.list" => {
            let skills: Vec<Value> = store
                .list(user_id)
                .await?
                .into_iter()
                .filter(|s| s.enabled)
                // `user` for one the user added, `model` for one made through these tools
                .map(|s| json!({"name": s.name, "description": s.description, "origin": s.origin.as_str()}))
                .collect();
            Ok(Value::Array(skills))
        }
        "example_skills.read" => {
            let name = text_arg(&args, "name")?;
            store.get_enabled(user_id, name).await?;
            let content = as_text(SKILL_FILE, store.read_file(user_id, name, SKILL_FILE).await?)?;
            let other_files: Vec<String> = store.files(user_id, name).await?.into_iter().filter(|path| path != SKILL_FILE).collect();
            Ok(json!({"name": name, "content": content, "other_files": other_files}))
        }
        "example_skills.read_file" => {
            let (name, path) = (text_arg(&args, "name")?, text_arg(&args, "path")?);
            store.get_enabled(user_id, name).await?;
            let content = as_text(path, store.read_file(user_id, name, path).await?)?;
            Ok(json!({"path": path, "content": content}))
        }
        "example_skills.write" => {
            let (name, description, content) = (text_arg(&args, "name")?, text_arg(&args, "description")?, text_arg(&args, "content")?);
            let text = format::render(name, description, content);
            store.save(user_id, name, description, vec![(SKILL_FILE.to_string(), text.into_bytes())], Origin::Model).await?;
            // What to do next, said in the result itself where it is read more reliably than in the description
            Ok(json!({"name": name, "note": "saved; it shows in the list of skills from the next chat on, and example_skills.read opens it now"}))
        }
        "example_skills.add_file" => {
            let (name, path, content) = (text_arg(&args, "name")?, text_arg(&args, "path")?, text_arg(&args, "content")?);
            store.add_file(user_id, name, path, content.as_bytes().to_vec(), Origin::Model).await?;
            Ok(json!({"name": name, "path": path}))
        }
        "example_skills.delete" => {
            let name = text_arg(&args, "name")?;
            store.delete(user_id, name, Origin::Model).await?;
            Ok(json!({"deleted": name}))
        }
        "example_skills.disable" => {
            let name = text_arg(&args, "name")?;
            store.set_enabled(user_id, name, false, Origin::Model).await?;
            Ok(json!({"disabled": name, "note": "turned off; it is out of your list from the next chat on and only the user can turn it back on"}))
        }
        "example_skills.install" => {
            let source = PathBuf::from(text_arg(&args, "path")?);
            // Reading a folder or a zip is blocking work, kept off the async runtime
            let skill = tokio::task::spawn_blocking(move || pack::from_path(&source).and_then(pack::Pack::into_skill))
                .await
                .map_err(|e| Failure::from(format!("reading the skill failed: {e}")))?
                .map_err(Failure::from)?;
            let files = skill.files.len();
            store.save(user_id, &skill.name, &skill.description, skill.files, Origin::Model).await?;
            Ok(json!({"name": skill.name, "description": skill.description, "files": files}))
        }
        other => Err(Failure::not_found(format!("no such tool: {other}"))),
    }
}
