/// Parsing is shared by readiness checks and execution so escaping has one meaning.
pub(super) enum Part<'a> {
    Text(&'a str),
    Argument(&'a str),
}

pub(super) fn parse(mut template: &str) -> Result<Vec<Part<'_>>, String> {
    let mut parts = Vec::new();
    while let Some(index) = template.find(['{', '}']) {
        if index > 0 {
            parts.push(Part::Text(&template[..index]));
        }
        template = &template[index..];
        if template.starts_with("{{") || template.starts_with("}}") {
            parts.push(Part::Text(&template[..1]));
            template = &template[2..];
        } else if template.starts_with('{') {
            let end = template.find('}').ok_or("unclosed template placeholder")?;
            let name = &template[1..end];
            if name.is_empty()
                || !name
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c))
            {
                return Err(
                    "expected a named placeholder such as {name}; escape braces as {{ and }}"
                        .into(),
                );
            }
            parts.push(Part::Argument(name));
            template = &template[end + 1..];
        } else {
            return Err("unmatched closing brace".into());
        }
    }
    if !template.is_empty() {
        parts.push(Part::Text(template));
    }
    Ok(parts)
}
