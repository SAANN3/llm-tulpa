//! The PDF form of a transcript, typeset with Typst. Message text is Markdown, so it is
//! converted to Typst markup first. Every piece of text goes in as a string literal (`#"..."`),
//! never as markup, so nothing a message says can be read as Typst syntax.

use pulldown_cmark::{Alignment, CodeBlockKind, Event, Options, Parser, Tag};
use typst_as_lib::{typst_kit_options::TypstKitFontOptions, TypstEngine};

use super::{AttachmentBody, ExportEntry, Speaker, Transcript};

/// A run of non-space characters longer than this gets break opportunities inserted. Typst, like
/// a browser without `overflow-wrap`, lets a word wider than the page run off its edge, and chats
/// hold such words (URLs, paths, base64).
const MAX_UNBROKEN_CHARS: usize = 32;

const PREAMBLE: &str = r##"#set page(paper: "a4", margin: (x: 18mm, y: 20mm), footer: context align(center, text(size: 8pt, fill: luma(120), counter(page).display("1 / 1", both: true))))
#set text(size: 10pt)
#set par(leading: 0.62em, spacing: 1em)
#set block(spacing: 0.95em)
#set raw(tab-size: 4)
#show raw: set text(size: 8.5pt)
#show raw.where(block: true): it => block(width: 100%, fill: luma(236), inset: 6pt, radius: 2pt, breakable: true, it)
#show heading: set text(size: 11pt)
#show heading: set block(above: 0.9em, below: 0.5em)
#show link: set text(fill: rgb("#1a4f9c"))
#show table.cell.where(y: 0): strong
// An attachment stands apart from message text: framed, tagged with what it is, its name in the
// monospace font. `dashed` is for the note that some were left out.
#let attachment(kind, name, body: none, dashed: false) = block(
  width: 100%, breakable: true, fill: white, radius: 3pt, inset: (x: 8pt, y: 6pt),
  stroke: (paint: luma(130), thickness: 0.7pt, dash: if dashed { "dashed" } else { none }),
)[
  #text(size: 7pt, weight: "bold", tracking: 0.1em, fill: luma(90))[#upper(kind)]#h(7pt)#text(font: "DejaVu Sans Mono", size: 8.5pt)[#name]
  #if body != none [
    #v(4pt)
    #body
  ]
]
#let entry(who, when, fill, body) = block(width: 100%, breakable: true, inset: (x: 9pt, y: 7pt), radius: 3pt, fill: fill)[
  #block(sticky: true, below: 7pt, grid(columns: (1fr, auto), text(weight: "bold", size: 9pt)[#who], text(size: 8pt, fill: luma(110))[#when]))
  #body
]
"##;

/// Typesets the transcript. `inline_images` are the images the entries refer to by key.
pub(super) fn render<'a>(
    transcript: &Transcript,
    inline_images: impl IntoIterator<Item = (&'a str, &'a [u8])>,
) -> Result<Vec<u8>, String> {
    let engine = TypstEngine::builder()
        .main_file(source(transcript))
        .with_static_file_resolver(inline_images)
        // The fonts compiled into the binary only: nothing is read from the machine, so the
        // same chat gives the same PDF wherever the backend runs
        .search_fonts_with(TypstKitFontOptions::default().include_system_fonts(false).include_embedded_fonts(true))
        .build();

    let document = engine
        .compile()
        .output
        .map_err(|e| format!("{e:?}").chars().take(600).collect::<String>())?;
    typst_pdf::pdf(&document, &Default::default()).map_err(|e| format!("{e:?}").chars().take(600).collect::<String>())
}

fn source(transcript: &Transcript) -> String {
    let mut out = String::from(PREAMBLE);
    out.push_str(&format!("#set document(title: {})\n", typst_str(&transcript.title)));
    out.push_str(&format!("#text(size: 17pt, weight: \"bold\")[{}]\n\n", piece(&transcript.title)));
    let count = transcript.entries.len();
    out.push_str(&format!(
        "#text(size: 8.5pt, fill: luma(110))[{}]\n\n#v(4pt)\n\n",
        piece(&format!(
            "Exported {} · {count} message{}",
            transcript.exported_at.format("%Y-%m-%d %H:%M %:z"),
            if count == 1 { "" } else { "s" },
        )),
    ));

    for entry in &transcript.entries {
        out.push_str(&entry_markup(entry));
    }
    out
}

fn entry_markup(entry: &ExportEntry) -> String {
    let (who, fill) = match &entry.speaker {
        Speaker::User => ("You".to_string(), "luma(236)"),
        Speaker::Assistant => ("Assistant".to_string(), "white"),
        Speaker::Tool(name) => (format!("Tool result · {name}"), "luma(246)"),
        Speaker::Notice => ("Notice".to_string(), "luma(246)"),
    };
    let when = entry.time.format("%Y-%m-%d %H:%M").to_string();

    let mut body = String::new();
    if !entry.text.is_empty() {
        match entry.speaker {
            // A tool's output is data, not prose: set verbatim
            Speaker::Tool(_) => body.push_str(&format!("#raw({}, block: true)\n\n", typst_str(&entry.text))),
            _ => body.push_str(&markdown_to_typst(&entry.text)),
        }
    }
    for call in &entry.calls {
        body.push_str(&format!(
            "#text(weight: \"bold\", size: 9pt)[Tool call #raw({})]\n\n#raw({}, block: true, lang: \"json\")\n\n",
            typst_str(&call.name),
            typst_str(&call.arguments),
        ));
    }
    for attachment in &entry.attachments {
        let name = piece(&attachment.name);
        match &attachment.body {
            AttachmentBody::Image { key, width_mm, .. } => body.push_str(&format!(
                "#attachment(\"Image\", [{name}], body: image({}, width: {width_mm:.1}mm))\n\n",
                typst_str(key),
            )),
            AttachmentBody::Text(text) => body.push_str(&format!(
                "#attachment(\"File\", [{name}], body: raw({}, block: true))\n\n",
                typst_str(text),
            )),
            AttachmentBody::Listed { .. } => body.push_str(&format!("#attachment(\"Attachment\", [{name}])\n\n")),
        }
    }
    if entry.omitted_attachments > 0 {
        body.push_str(&format!(
            "#attachment(\"Not included\", [{}], dashed: true)\n\n",
            piece(&format!(
                "{} attachment{}",
                entry.omitted_attachments,
                if entry.omitted_attachments == 1 { "" } else { "s" },
            )),
        ));
    }

    format!("#entry({}, {}, {fill})[\n{body}]\n\n", typst_str(&who), typst_str(&when))
}

/// A Typst string literal holding exactly `text`
fn typst_str(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Text as a piece of markup (`#"..."`), with a break opportunity (a zero-width space) wherever a
/// word runs on past `MAX_UNBROKEN_CHARS`
fn piece(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let mut spaced = String::with_capacity(text.len());
    let mut run = 0;
    for c in text.chars() {
        if c.is_whitespace() {
            run = 0;
        } else {
            run += 1;
            if run > MAX_UNBROKEN_CHARS {
                spaced.push('\u{200B}');
                run = 1;
            }
        }
        spaced.push(c);
    }
    format!("#{}", typst_str(&spaced))
}

enum Kind {
    Root,
    Paragraph,
    Heading(u8),
    Emphasis,
    Strong,
    Strikethrough,
    Link(String),
    Image,
    BlockQuote,
    CodeBlock(Option<String>),
    List(Option<u64>),
    Item,
    Table(Vec<Alignment>),
    TableHead,
    TableRow,
    TableCell,
    /// Something this converter has no markup for: its content is kept, its wrapper dropped
    Passthrough,
}

/// One open element while converting: what it holds so far (`out`), and for lists and tables the
/// finished children (`items`)
struct Frame {
    kind: Kind,
    out: String,
    items: Vec<String>,
    head_cells: usize,
}

impl Frame {
    fn new(kind: Kind) -> Self {
        Self { kind, out: String::new(), items: vec![], head_cells: 0 }
    }
}

fn kind_of(tag: Tag) -> Kind {
    match tag {
        Tag::Paragraph => Kind::Paragraph,
        Tag::Heading { level, .. } => Kind::Heading(level as u8),
        Tag::BlockQuote(_) => Kind::BlockQuote,
        Tag::CodeBlock(CodeBlockKind::Fenced(language)) => {
            let language = language.split_whitespace().next().unwrap_or("");
            let usable = !language.is_empty() && language.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '#' | '-' | '_'));
            Kind::CodeBlock(usable.then(|| language.to_string()))
        }
        Tag::CodeBlock(CodeBlockKind::Indented) => Kind::CodeBlock(None),
        Tag::List(start) => Kind::List(start),
        Tag::Item => Kind::Item,
        Tag::Table(alignments) => Kind::Table(alignments),
        Tag::TableHead => Kind::TableHead,
        Tag::TableRow => Kind::TableRow,
        Tag::TableCell => Kind::TableCell,
        Tag::Emphasis => Kind::Emphasis,
        Tag::Strong => Kind::Strong,
        Tag::Strikethrough => Kind::Strikethrough,
        Tag::Link { dest_url, .. } => Kind::Link(dest_url.to_string()),
        Tag::Image { .. } => Kind::Image,
        _ => Kind::Passthrough,
    }
}

/// Appends a finished element to its parent in Typst markup
fn close(frame: Frame, parent: &mut Frame) {
    let Frame { kind, out, items, head_cells } = frame;
    match kind {
        Kind::Root | Kind::Passthrough => parent.out.push_str(&out),
        Kind::Paragraph => parent.out.push_str(&format!("{out}\n\n")),
        Kind::Heading(level) => parent.out.push_str(&format!("#heading(level: {level}, outlined: false)[{out}]\n\n")),
        Kind::Emphasis => parent.out.push_str(&format!("#emph[{out}]")),
        Kind::Strong => parent.out.push_str(&format!("#strong[{out}]")),
        Kind::Strikethrough => parent.out.push_str(&format!("#strike[{out}]")),
        Kind::Link(url) => parent.out.push_str(&format!("#link({})[{out}]", typst_str(&url))),
        Kind::Image => parent.out.push_str(&format!("#emph[(image: {out})]")),
        Kind::BlockQuote => parent.out.push_str(&format!(
            "#block(inset: (left: 9pt), stroke: (left: 1.5pt + luma(170)))[{out}]\n\n"
        )),
        Kind::CodeBlock(language) => {
            let language = language.map(|l| format!(", lang: {}", typst_str(&l))).unwrap_or_default();
            parent.out.push_str(&format!("#raw({}, block: true{language})\n\n", typst_str(out.trim_end_matches('\n'))));
        }
        Kind::List(start) => {
            if items.is_empty() {
                return;
            }
            let items = items.join(", ");
            match start {
                Some(start) => parent.out.push_str(&format!("#enum(start: {start}, tight: true, {items})\n\n")),
                None => parent.out.push_str(&format!("#list(tight: true, {items})\n\n")),
            }
        }
        Kind::Item | Kind::TableCell => parent.items.push(format!("[{out}]")),
        Kind::TableHead => {
            parent.head_cells = items.len();
            parent.items.extend(items);
        }
        Kind::TableRow => parent.items.extend(items),
        Kind::Table(alignments) => {
            if items.is_empty() {
                return;
            }
            let columns = alignments.len().max(1);
            let aligns: Vec<&str> = alignments
                .iter()
                .map(|a| match a {
                    Alignment::Center => "center",
                    Alignment::Right => "right",
                    _ => "left",
                })
                .collect();
            let (head, body) = items.split_at(head_cells.min(items.len()));
            let header = if head.is_empty() { String::new() } else { format!("table.header({}), ", head.join(", ")) };
            parent.out.push_str(&format!(
                "#table(columns: {columns}, align: ({},), stroke: 0.5pt + luma(170), inset: 5pt, {header}{})\n\n",
                aligns.join(", "),
                body.join(", "),
            ));
        }
    }
}

/// Message Markdown as Typst markup: paragraphs, headings, emphasis, links, lists, quotes, code
/// and tables. Line breaks inside a paragraph are kept as breaks, as the chat page shows them.
fn markdown_to_typst(markdown: &str) -> String {
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut stack = vec![Frame::new(Kind::Root)];

    for event in Parser::new_ext(markdown, options) {
        match event {
            Event::Start(tag) => stack.push(Frame::new(kind_of(tag))),
            Event::End(_) => {
                if stack.len() > 1 {
                    let frame = stack.pop().expect("more than the root is open");
                    close(frame, stack.last_mut().expect("the root stays open"));
                }
            }
            Event::Text(text) => {
                let top = stack.last_mut().expect("the root stays open");
                match top.kind {
                    Kind::CodeBlock(_) => top.out.push_str(&text),
                    _ => top.out.push_str(&piece(&text)),
                }
            }
            Event::Code(code) => stack.last_mut().expect("the root stays open").out.push_str(&format!("#raw({})", typst_str(&code))),
            Event::Html(html) | Event::InlineHtml(html) => stack.last_mut().expect("the root stays open").out.push_str(&piece(&html)),
            Event::SoftBreak | Event::HardBreak => stack.last_mut().expect("the root stays open").out.push_str("#linebreak()"),
            Event::Rule => stack
                .last_mut()
                .expect("the root stays open")
                .out
                .push_str("#line(length: 100%, stroke: 0.5pt + luma(170))\n\n"),
            Event::FootnoteReference(name) => stack.last_mut().expect("the root stays open").out.push_str(&piece(&format!("[{name}]"))),
            Event::TaskListMarker(checked) => {
                stack.last_mut().expect("the root stays open").out.push_str(&piece(if checked { "[x] " } else { "[ ] " }))
            }
            _ => {}
        }
    }

    // Anything left open (malformed input can't produce it, but a partial result beats a panic)
    while stack.len() > 1 {
        let frame = stack.pop().expect("more than the root is open");
        close(frame, stack.last_mut().expect("the root stays open"));
    }
    stack.pop().map(|root| root.out).unwrap_or_default()
}
