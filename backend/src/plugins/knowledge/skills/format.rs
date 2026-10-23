//! The text of a `SKILL.md`: a header between two lines of `---` that names the skill and says in a sentence or two when to
//! use it, and then the instructions. The header is the subset of YAML that skills are written in (the same as Claude's own
//! skills, so a skill made for those can be uploaded as it is): `key: value` lines, a value plain, in quotes, or a block
//! (`>` folds its lines into one, `|` keeps them); keys other than `name` and `description`, and anything indented under
//! them, are ignored.

/// What a `SKILL.md` says about itself.
#[derive(Debug, PartialEq)]
pub struct Parsed {
    /// The `name` in the header, when there is one
    pub name: Option<String>,
    /// When to use the skill, on one line
    pub description: String,
    /// The instructions: everything after the header
    pub body: String,
}

/// The longest a skill's name may be
pub const MAX_NAME_CHARS: usize = 64;

/// Whether `name` is a skill's name: lowercase letters and digits, in words joined by single hyphens.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME_CHARS
        && name.split('-').all(|word| !word.is_empty() && word.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()))
}

/// A name made from whatever a folder, a file or a zip is called: lowercased, anything but letters and digits turned into
/// a hyphen, the hyphens tidied. `None` when nothing is left.
pub fn name_from(text: &str) -> Option<String> {
    let mut out = String::new();
    for c in text.to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').chars().take(MAX_NAME_CHARS).collect::<String>();
    let out = out.trim_end_matches('-').to_string();
    valid_name(&out).then_some(out)
}

/// Reads a `SKILL.md`. Errors say what to fix, since they are shown to the person who uploaded it.
pub fn parse(text: &str) -> Result<Parsed, String> {
    let text = text.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    let mut lines = text.lines();
    let first = lines.by_ref().find(|line| !line.trim().is_empty());
    if first.map(str::trim) != Some("---") {
        return Err("SKILL.md has to start with a header between two lines of ---, with a name and a description".to_string());
    }
    let mut header = Vec::new();
    let mut closed = false;
    for line in lines.by_ref() {
        if line.trim_end() == "---" {
            closed = true;
            break;
        }
        header.push(line);
    }
    if !closed {
        return Err("the header of SKILL.md is not closed: it needs a second line of ---".to_string());
    }
    let body = lines.collect::<Vec<_>>().join("\n").trim().to_string();

    let (mut name, mut description) = (None, None);
    let mut at = 0;
    while at < header.len() {
        let line = header[at];
        at += 1;
        // Anything indented belongs to the key above it; a key this does not read takes its lines with it
        if line.trim().is_empty() || line.starts_with(' ') || line.starts_with('\t') || line.trim_start().starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else { continue };
        let key = key.trim();
        let value = value.trim();
        let block = matches!(value.chars().next(), Some('>') | Some('|')).then(|| value.starts_with('>'));
        let mut text = String::new();
        if let Some(fold) = block {
            let mut lines_of_block = Vec::new();
            while at < header.len() && (header[at].trim().is_empty() || header[at].starts_with(' ') || header[at].starts_with('\t')) {
                lines_of_block.push(header[at].trim());
                at += 1;
            }
            text = lines_of_block.join(if fold { " " } else { "\n" });
        } else if value.starts_with('"') || value.starts_with('\'') {
            let quote = value.chars().next().unwrap();
            let inner = value[1..].strip_suffix(quote).unwrap_or(&value[1..]);
            text = if quote == '"' { inner.replace("\\\"", "\"").replace("\\\\", "\\") } else { inner.replace("''", "'") };
        } else {
            text = value.to_string();
        }
        match key {
            "name" => name = Some(one_line(&text)),
            "description" => description = Some(one_line(&text)),
            _ => {}
        }
    }
    let description = description.filter(|d| !d.is_empty()).ok_or_else(|| {
        "the header of SKILL.md needs a description: a sentence or two on when to use the skill".to_string()
    })?;
    Ok(Parsed { name: name.filter(|n| !n.is_empty()), description, body })
}

/// The whitespace of `text` collapsed into single spaces: a description is one line wherever it is shown.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A `SKILL.md` made of a name, a description and instructions, for a skill the model writes.
pub fn render(name: &str, description: &str, body: &str) -> String {
    // The description is quoted so a colon or a hash in it cannot be read as part of the header
    let description = one_line(description).replace('\\', "\\\\").replace('"', "\\\"");
    format!("---\nname: {name}\ndescription: \"{description}\"\n---\n\n{}\n", body.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_header_with_plain_values_is_read() {
        let p = parse("---\nname: lab-report\ndescription: Use for a lab report in Typst.\n---\n\n# Steps\n1. Do it\n").unwrap();
        assert_eq!(p.name.as_deref(), Some("lab-report"));
        assert_eq!(p.description, "Use for a lab report in Typst.");
        assert_eq!(p.body, "# Steps\n1. Do it");
    }

    #[test]
    fn quoted_and_block_values_are_read() {
        let quoted = parse("---\nname: \"a-b\"\ndescription: \"Says: \\\"hi\\\" # not a comment\"\n---\nbody").unwrap();
        assert_eq!(quoted.name.as_deref(), Some("a-b"));
        assert_eq!(quoted.description, "Says: \"hi\" # not a comment");
        let folded = parse("---\nname: x\ndescription: >\n  First line\n  second line\n\n  third\n---\nbody").unwrap();
        assert_eq!(folded.description, "First line second line third");
        let literal = parse("---\ndescription: |\n  one\n  two\n---\nbody").unwrap();
        assert_eq!(literal.description, "one two", "a description is one line either way");
        assert_eq!(literal.name, None);
    }

    #[test]
    fn other_keys_and_what_is_indented_under_them_are_ignored() {
        let p = parse("---\nlicense: MIT\nmetadata:\n  author: someone\n  name: not-this\nname: real\nallowed-tools: [a, b]\ndescription: When needed.\n---\nbody").unwrap();
        assert_eq!(p.name.as_deref(), Some("real"));
        assert_eq!(p.description, "When needed.");
    }

    #[test]
    fn windows_line_endings_and_a_byte_order_mark_are_fine() {
        let p = parse("\u{feff}---\r\nname: x\r\ndescription: D\r\n---\r\nbody\r\nmore").unwrap();
        assert_eq!((p.name.as_deref(), p.description.as_str(), p.body.as_str()), (Some("x"), "D", "body\nmore"));
    }

    #[test]
    fn a_missing_or_broken_header_says_what_to_fix() {
        assert!(parse("# just text").unwrap_err().contains("header"));
        assert!(parse("---\nname: x\n").unwrap_err().contains("not closed"));
        assert!(parse("---\nname: x\n---\nbody").unwrap_err().contains("description"));
        assert!(parse("---\nname: x\ndescription:\n---\nbody").unwrap_err().contains("description"));
    }

    #[test]
    fn a_name_is_lowercase_words_joined_by_hyphens() {
        assert!(valid_name("lab-report"));
        assert!(valid_name("a1"));
        assert!(!valid_name(""));
        assert!(!valid_name("Lab"));
        assert!(!valid_name("a--b"));
        assert!(!valid_name("-a"));
        assert!(!valid_name("a b"));
        assert!(!valid_name(&"a".repeat(65)));
        assert_eq!(name_from("My Lab Report!").as_deref(), Some("my-lab-report"));
        assert_eq!(name_from("  --x__y--").as_deref(), Some("x-y"));
        assert_eq!(name_from("!!!"), None);
    }

    #[test]
    fn what_is_rendered_is_read_back() {
        let text = render("my-skill", "Use it: when \"x\" # y", "Do this.\n\nThen that.");
        let p = parse(&text).unwrap();
        assert_eq!(p.name.as_deref(), Some("my-skill"));
        assert_eq!(p.description, "Use it: when \"x\" # y");
        assert_eq!(p.body, "Do this.\n\nThen that.");
    }
}
