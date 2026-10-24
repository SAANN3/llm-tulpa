//! What a plugin's tab in Settings shows, as data. The backend draws it with one component, so a plugin needs no UI code: it
//! says what is on the page for the user who opened it, and answers what the user does there with the page as it is after.

use serde::Serialize;

/// Everything the tab shows one user
#[derive(Clone, Debug, Default, Serialize)]
pub struct Tab {
    sections: Vec<Section>,
    /// A line after an action: what was done, or why it was not
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    /// A text shown in a popup
    #[serde(skip_serializing_if = "Option::is_none")]
    popup: Option<Popup>,
}

impl Tab {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn section(mut self, section: Section) -> Self {
        self.sections.push(section);
        self
    }

    pub fn message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    pub fn popup(mut self, title: impl Into<String>, text: impl Into<String>) -> Self {
        self.popup = Some(Popup { title: title.into(), text: text.into() });
        self
    }
}

#[derive(Clone, Debug, Serialize)]
struct Popup {
    title: String,
    text: String,
}

/// A titled group of blocks
#[derive(Clone, Debug, Default, Serialize)]
pub struct Section {
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    blocks: Vec<Block>,
}

impl Section {
    pub fn new(title: impl Into<String>) -> Self {
        Self { title: Some(title.into()), blocks: vec![] }
    }

    /// A section with no title
    pub fn untitled() -> Self {
        Self::default()
    }

    pub fn block(mut self, block: Block) -> Self {
        self.blocks.push(block);
        self
    }
}

/// One thing in a section. Every control carries an `action`, the word it sends back in `tab_action` (or `tab_upload`).
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Block {
    /// A line of text; `dim` draws it as a hint
    Text { text: String, dim: bool },
    /// A labeled switch. Sends its `action` with `{"on": bool}`.
    Switch {
        action: String,
        label: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        help: Option<String>,
        on: bool,
    },
    /// A list of rows; `empty` is said when there are none
    Items { empty: String, rows: Vec<Row> },
    /// A drop zone for files and folders; `accept` lists the endings it takes (`.zip`), all when empty
    Drop {
        action: String,
        label: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        help: Option<String>,
        accept: Vec<String>,
    },
}

impl Block {
    pub fn text(text: impl Into<String>) -> Self {
        Block::Text { text: text.into(), dim: false }
    }

    /// A line drawn as a hint
    pub fn hint(text: impl Into<String>) -> Self {
        Block::Text { text: text.into(), dim: true }
    }

    pub fn switch(action: impl Into<String>, label: impl Into<String>, on: bool) -> Self {
        Block::Switch { action: action.into(), label: label.into(), help: None, on }
    }

    pub fn items(empty: impl Into<String>, rows: Vec<Row>) -> Self {
        Block::Items { empty: empty.into(), rows }
    }

    pub fn drop_zone(action: impl Into<String>, label: impl Into<String>, accept: &[&str]) -> Self {
        Block::Drop { action: action.into(), label: label.into(), help: None, accept: accept.iter().map(|a| a.to_string()).collect() }
    }

    /// A line of help under a switch or a drop zone; nothing for the other blocks
    pub fn help(mut self, text: impl Into<String>) -> Self {
        if let Block::Switch { help, .. } | Block::Drop { help, .. } = &mut self {
            *help = Some(text.into());
        }
        self
    }
}

/// A row of an `Items` block, identified by `id` in what its controls send back
#[derive(Clone, Debug, Serialize)]
pub struct Row {
    id: String,
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    subtitle: Option<String>,
    /// A short label beside the title
    #[serde(skip_serializing_if = "Option::is_none")]
    badge: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    switch: Option<RowSwitch>,
    buttons: Vec<RowButton>,
}

#[derive(Clone, Debug, Serialize)]
struct RowSwitch {
    action: String,
    on: bool,
}

#[derive(Clone, Debug, Serialize)]
struct RowButton {
    action: String,
    label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    confirm: Option<String>,
    upload: bool,
}

impl Row {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self { id: id.into(), title: title.into(), subtitle: None, badge: None, switch: None, buttons: vec![] }
    }

    pub fn subtitle(mut self, text: impl Into<String>) -> Self {
        self.subtitle = Some(text.into());
        self
    }

    pub fn badge(mut self, text: impl Into<String>) -> Self {
        self.badge = Some(text.into());
        self
    }

    /// A switch on the row; sends its `action` with `{"row": id, "on": bool}`
    pub fn switch(mut self, action: impl Into<String>, on: bool) -> Self {
        self.switch = Some(RowSwitch { action: action.into(), on });
        self
    }

    /// A button; sends its `action` with `{"row": id}`
    pub fn button(mut self, action: impl Into<String>, label: impl Into<String>) -> Self {
        self.buttons.push(RowButton { action: action.into(), label: label.into(), confirm: None, upload: false });
        self
    }

    /// A button that asks `question` in a popup before it sends anything
    pub fn confirm_button(mut self, action: impl Into<String>, label: impl Into<String>, question: impl Into<String>) -> Self {
        self.buttons.push(RowButton { action: action.into(), label: label.into(), confirm: Some(question.into()), upload: false });
        self
    }

    /// A button that opens a file picker; the files arrive in `tab_upload` with this row's id
    pub fn upload_button(mut self, action: impl Into<String>, label: impl Into<String>) -> Self {
        self.buttons.push(RowButton { action: action.into(), label: label.into(), confirm: None, upload: true });
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_tab_is_sent_in_the_shape_the_backend_reads() {
        let tab = Tab::new()
            .section(Section::new("Hello").block(Block::hint("a hint")).block(Block::switch("loud", "Loud", true).help("why")).block(Block::items(
                "none",
                vec![Row::new("r1", "Row").button("greet", "Greet").upload_button("add", "Add")],
            )))
            .message("done");
        let sent = serde_json::to_value(&tab).unwrap();
        assert_eq!(sent["message"], "done");
        assert!(sent.get("popup").is_none());
        let blocks = &sent["sections"][0]["blocks"];
        assert_eq!(blocks[0], json!({"type": "text", "text": "a hint", "dim": true}));
        assert_eq!(blocks[1], json!({"type": "switch", "action": "loud", "label": "Loud", "help": "why", "on": true}));
        assert_eq!(blocks[2]["rows"][0]["buttons"][1], json!({"action": "add", "label": "Add", "upload": true}));
    }
}
