//! `plugin.toml`, the file at the root of an external plugin's folder that says what it is and how to start it.

use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use serde::Deserialize;

use super::spec::{ParamSpec, check_params};
use crate::plugins::{schema, tab};

/// The protocol version this build of the backend speaks.
pub const SUPPORTED_PROTOCOL: u32 = 1;

#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub name: String,
    pub subname: String,
    pub version: String,
    pub protocol: u32,
    /// One line for the Plugins list
    pub summary: String,
    /// A longer text for the plugin's page
    #[serde(default)]
    pub help_message: String,
    /// Whether the plugin is switched on the first time the backend sees it. Not for a plugin with settings: it has nothing to
    /// run with until the owner fills them in.
    #[serde(default)]
    pub default_enabled: bool,
    /// What the owner fills in before the plugin runs, in the shape of a tool's parameters: the backend draws the form,
    /// checks what is entered against it and gives the plugin only settings that fit
    #[serde(default)]
    pub settings: Vec<ParamSpec>,
    /// A tab of the plugin's own in Settings, shown to every user while the plugin is enabled
    #[serde(default)]
    pub tab: Option<Tab>,
    /// Present when the plugin keeps its data in the database: it gets a schema and a role of its own
    #[serde(default)]
    pub database: Option<Database>,
    /// Informational: which SDK the plugin was written with
    #[serde(default)]
    pub sdk: Option<String>,
    pub kind: Kind,
    /// `kind = "source"`: the command that builds it. Not run by the backend at start-up, only when a plugin is installed.
    #[serde(default)]
    #[allow(dead_code)]
    pub build: Option<String>,
    /// `kind = "source"`: the command that starts it
    #[serde(default)]
    pub run: Option<String>,
    /// `kind = "prebuilt"`: one binary per target, `<arch>-<os>` such as `x86_64-linux`
    #[serde(default)]
    pub prebuilt: HashMap<String, Prebuilt>,
    /// A command that exits 0 when what the plugin needs is there; its output says what is missing when it is not
    #[serde(default)]
    pub check: Option<String>,
    /// The oldest backend version the plugin works with
    #[serde(default)]
    pub min_tulpa: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Database {
    /// The folder, inside the plugin's own, whose `.sql` files are its migrations, one step each, applied in the order of
    /// their names
    #[serde(default = "default_migrations")]
    pub migrations: String,
}

fn default_migrations() -> String {
    "migrations".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct Tab {
    /// The tab's place in the address (`/settings/<id>`)
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Source,
    Prebuilt,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Prebuilt {
    /// Relative to the plugin's folder
    pub path: String,
    pub sha256: String,
}

#[derive(Debug)]
pub struct ManifestError(pub String);

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// `<arch>-<os>` of the machine the backend runs on, the key of `[prebuilt.*]`
pub fn current_target() -> String {
    format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS)
}

/// `1.2.3` (anything after a `-` or `+` is ignored) as numbers, `None` when it is not that shape
pub fn version_numbers(version: &str) -> Option<[u64; 3]> {
    let core = version.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let numbers = [parts.next()?.parse().ok()?, parts.next()?.parse().ok()?, parts.next()?.parse().ok()?];
    parts.next().is_none().then_some(numbers)
}

impl Manifest {
    /// Reads and checks `plugin.toml` in `plugin_dir`, which must be the folder `<name>/<subname>` the plugin says it is.
    pub fn load(plugin_dir: &Path) -> Result<Manifest, ManifestError> {
        let path = plugin_dir.join("plugin.toml");
        let text = std::fs::read_to_string(&path).map_err(|e| ManifestError(format!("cannot read {}: {e}", path.display())))?;
        let manifest = Self::parse(&text)?;
        let mut parts = plugin_dir.iter().rev();
        let (folder_sub, folder_name) = (parts.next().and_then(|p| p.to_str()), parts.next().and_then(|p| p.to_str()));
        if folder_name != Some(manifest.name.as_str()) || folder_sub != Some(manifest.subname.as_str()) {
            return Err(ManifestError(format!(
                "it says it is {}/{} but lives in {}; its folder must be <name>/<subname>",
                manifest.name,
                manifest.subname,
                plugin_dir.display()
            )));
        }
        Ok(manifest)
    }

    pub fn parse(text: &str) -> Result<Manifest, ManifestError> {
        let manifest: Manifest = toml::from_str(text).map_err(|e| ManifestError(format!("plugin.toml is not valid: {e}")))?;
        manifest.validate()?;
        Ok(manifest)
    }

    fn validate(&self) -> Result<(), ManifestError> {
        schema::prefix(&self.name, &self.subname).map_err(|e| ManifestError(e.to_string()))?;
        if self.protocol != SUPPORTED_PROTOCOL {
            return Err(ManifestError(format!("it speaks protocol {}, this backend speaks {SUPPORTED_PROTOCOL}", self.protocol)));
        }
        if version_numbers(&self.version).is_none() {
            return Err(ManifestError(format!("version '{}' is not of the form 1.2.3", self.version)));
        }
        if self.summary.trim().is_empty() {
            return Err(ManifestError("summary is empty".to_string()));
        }
        check_params("settings", &self.settings).map_err(ManifestError)?;
        if self.default_enabled && !self.settings.is_empty() {
            return Err(ManifestError("default_enabled cannot be set for a plugin with settings".to_string()));
        }
        if let Some(database) = &self.database {
            let folder = Path::new(&database.migrations);
            if database.migrations.is_empty() || !folder.is_relative() || folder.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
                return Err(ManifestError(format!("the migrations folder '{}' must be a plain folder name inside the plugin's own", database.migrations)));
            }
            schema::role_name(&self.name, &self.subname).map_err(|e| ManifestError(e.to_string()))?;
        }
        if let Some(tab) = &self.tab {
            if !tab::valid_tab_id(&tab.id) || tab::RESERVED_TAB_IDS.contains(&tab.id.as_str()) {
                return Err(ManifestError(format!("tab id '{}' is not usable: lowercase letters, digits and hyphens, and not one of the Settings page's own", tab.id)));
            }
            if tab.title.trim().is_empty() {
                return Err(ManifestError("the tab has no title".to_string()));
            }
        }
        if let Some(min) = &self.min_tulpa {
            let wanted = version_numbers(min).ok_or_else(|| ManifestError(format!("min_tulpa '{min}' is not of the form 1.2.3")))?;
            let have = version_numbers(env!("CARGO_PKG_VERSION")).unwrap_or_default();
            if wanted > have {
                return Err(ManifestError(format!("it needs llm-tulpa {min} or newer, this is {}", env!("CARGO_PKG_VERSION"))));
            }
        }
        match self.kind {
            Kind::Source => {
                if self.run.as_deref().is_none_or(|run| run.trim().is_empty()) {
                    return Err(ManifestError("kind = \"source\" needs a run command".to_string()));
                }
            }
            Kind::Prebuilt => {
                for (target, binary) in &self.prebuilt {
                    if binary.sha256.len() != 64 || !binary.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
                        return Err(ManifestError(format!("prebuilt.{target}: sha256 is not 64 hex digits")));
                    }
                }
                if !self.prebuilt.contains_key(&current_target()) {
                    return Err(ManifestError(format!("it has no prebuilt binary for {}", current_target())));
                }
            }
        }
        Ok(())
    }

    /// The command line that starts the plugin: `run`, or the binary built for this machine
    pub fn run_command(&self) -> String {
        match self.kind {
            Kind::Source => self.run.clone().unwrap_or_default(),
            Kind::Prebuilt => self.prebuilt.get(&current_target()).map(|b| format!("\"{{plugin_dir}}/{}\"", b.path)).unwrap_or_default(),
        }
    }
}

/// Splits a command line into words: spaces separate, single quotes keep everything, double quotes keep everything but
/// `\"` and `\\`. A backslash outside quotes is an ordinary character so a Windows path survives. No shell is involved,
/// so nothing in the line is expanded or run except by the program it starts.
pub fn split_command(line: &str) -> Result<Vec<String>, ManifestError> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            c if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(c) => word.push(c),
                        None => return Err(ManifestError(format!("an opening ' is never closed in: {line}"))),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(c @ ('"' | '\\')) => word.push(c),
                            Some(c) => {
                                word.push('\\');
                                word.push(c);
                            }
                            None => return Err(ManifestError(format!("an opening \" is never closed in: {line}"))),
                        },
                        Some(c) => word.push(c),
                        None => return Err(ManifestError(format!("an opening \" is never closed in: {line}"))),
                    }
                }
            }
            c => {
                in_word = true;
                word.push(c);
            }
        }
    }
    if in_word {
        words.push(word);
    }
    Ok(words)
}

/// The words of `line` with `{plugin_dir}` and `{data_dir}` replaced. Expanding after splitting keeps a folder with a space
/// in its name one word.
pub fn expand_command(line: &str, plugin_dir: &Path, data_dir: &Path) -> Result<Vec<String>, ManifestError> {
    let words = split_command(line)?;
    Ok(words
        .into_iter()
        .map(|w| w.replace("{plugin_dir}", &plugin_dir.to_string_lossy()).replace("{data_dir}", &data_dir.to_string_lossy()))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"
name = "example"
subname = "hello"
version = "0.1.0"
protocol = 1
summary = "Says hello"
kind = "source"
run = "{plugin_dir}/target/release/tulpa-example-hello"
"#;

    #[test]
    fn a_complete_manifest_is_accepted() {
        let m = Manifest::parse(GOOD).unwrap();
        assert_eq!((m.name.as_str(), m.subname.as_str(), m.kind), ("example", "hello", Kind::Source));
        assert_eq!(m.run_command(), "{plugin_dir}/target/release/tulpa-example-hello");
        assert_eq!((m.help_message.as_str(), m.default_enabled), ("", false));
        let described = Manifest::parse(&GOOD.replace("kind =", "help_message = \"More\"\ndefault_enabled = true\nkind =")).unwrap();
        assert_eq!((described.help_message.as_str(), described.default_enabled), ("More", true));
    }

    #[test]
    fn settings_and_a_tab_are_read() {
        let text = format!("{GOOD}\n[[settings]]\nname = \"greeting\"\ntype = \"string\"\ndescription = \"x\"\n\n[tab]\nid = \"hello\"\ntitle = \"Hello\"\n");
        let m = Manifest::parse(&text).unwrap();
        assert_eq!((m.settings.len(), m.settings[0].required), (1, false));
        assert_eq!(m.tab.map(|t| t.id), Some("hello".to_string()));
    }

    #[test]
    fn a_database_is_declared_with_the_default_migrations_folder() {
        let m = Manifest::parse(&format!("{GOOD}\n[database]\n")).unwrap();
        assert_eq!(m.database.map(|d| d.migrations), Some("migrations".to_string()));
        assert!(Manifest::parse(GOOD).unwrap().database.is_none());
        let named = Manifest::parse(&format!("{GOOD}\n[database]\nmigrations = \"sql/steps\"\n")).unwrap();
        assert_eq!(named.database.map(|d| d.migrations), Some("sql/steps".to_string()));
    }

    #[test]
    fn what_cannot_work_is_refused_with_the_reason() {
        let cases = [
            (GOOD.replace("protocol = 1", "protocol = 2"), "protocol"),
            (GOOD.replace("0.1.0", "one"), "1.2.3"),
            (GOOD.replace("example", "Bad-Name"), "lowercase"),
            (GOOD.replace("Says hello", "  "), "summary"),
            (GOOD.replace("run = ", "# run = "), "run command"),
            (format!("{GOOD}min_tulpa = \"999.0.0\"\n"), "999.0.0"),
            (GOOD.replace("kind = \"source\"", "kind = \"prebuilt\""), "prebuilt binary"),
            (format!("{GOOD}\n[[settings]]\nname = \"a\"\ntype = \"string\"\ndescription = \"x\"\n[[settings]]\nname = \"a\"\ntype = \"string\"\ndescription = \"x\"\n"), "listed twice"),
            (GOOD.replace("kind =", "default_enabled = true\nkind =") + "[[settings]]\nname = \"a\"\ntype = \"string\"\ndescription = \"x\"\n", "default_enabled"),
            (format!("{GOOD}\n[database]\nmigrations = \"../sql\"\n"), "plain folder name"),
            (format!("{GOOD}\n[database]\nmigrations = \"/sql\"\n"), "plain folder name"),
            (format!("{GOOD}\n[database]\nmigrations = \"\"\n"), "plain folder name"),
            (format!("{}\n[database]\n", GOOD.replace("\"example\"", &format!("\"{}\"", "n".repeat(40))).replace("\"hello\"", &format!("\"{}\"", "s".repeat(30)))), "63 bytes"),
            (format!("{GOOD}\n[tab]\nid = \"models\"\ntitle = \"T\"\n"), "tab id"),
            (format!("{GOOD}\n[tab]\nid = \"ok\"\ntitle = \" \"\n"), "no title"),
        ];
        for (text, expected) in cases {
            let err = Manifest::parse(&text).unwrap_err().to_string();
            assert!(err.contains(expected), "{err:?} should mention {expected:?}");
        }
    }

    #[test]
    fn the_folder_must_be_name_slash_subname() {
        let dir = std::env::temp_dir().join(format!("tulpa-manifest-{}", std::process::id()));
        let right = dir.join("example").join("hello");
        let wrong = dir.join("example").join("other");
        std::fs::create_dir_all(&right).unwrap();
        std::fs::create_dir_all(&wrong).unwrap();
        std::fs::write(right.join("plugin.toml"), GOOD).unwrap();
        std::fs::write(wrong.join("plugin.toml"), GOOD).unwrap();
        assert!(Manifest::load(&right).is_ok());
        assert!(Manifest::load(&wrong).unwrap_err().to_string().contains("<name>/<subname>"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn versions_compare_as_numbers() {
        assert_eq!(version_numbers("0.10.2"), Some([0, 10, 2]));
        assert_eq!(version_numbers("1.2.3-beta+4"), Some([1, 2, 3]));
        assert_eq!(version_numbers("1.2"), None);
        assert_eq!(version_numbers("1.2.3.4"), None);
        assert!(version_numbers("0.10.0") > version_numbers("0.9.9"));
    }

    #[test]
    fn a_command_line_splits_into_words_without_a_shell() {
        let words = |l: &str| split_command(l).unwrap();
        assert_eq!(words("which grim"), ["which", "grim"]);
        assert_eq!(words("  python3   main.py "), ["python3", "main.py"]);
        assert_eq!(words(r#"run "a b" 'c d' e"f g"h"#), ["run", "a b", "c d", "ef gh"]);
        assert_eq!(words(r#""say \"hi\"""#), [r#"say "hi""#]);
        assert_eq!(words(r"C:\tools\x.exe"), [r"C:\tools\x.exe"]);
        assert_eq!(words("a && b"), ["a", "&&", "b"]);
        assert_eq!(words(""), Vec::<String>::new());
        assert!(split_command("a 'b").is_err());
        assert!(split_command("a \"b").is_err());
    }

    #[test]
    fn placeholders_are_expanded_per_word_so_a_space_in_a_folder_stays_one_word() {
        let words = expand_command("{plugin_dir}/bin/run --data {data_dir}", Path::new("/opt/my plugins/x"), Path::new("/data/x y")).unwrap();
        assert_eq!(words, ["/opt/my plugins/x/bin/run", "--data", "/data/x y"]);
    }
}
