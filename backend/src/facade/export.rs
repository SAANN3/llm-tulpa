//! Turning a chat into a file: Markdown or a PDF, optionally with its tool calls and its
//! attachments. The chat is first read into a plain `Transcript`, so both formats are rendered
//! from the same entries and can't disagree about what a chat contains.

mod markdown;
mod pdf;

use std::io::{Cursor, Write};
use std::sync::Arc;

use base64::Engine as _;
use chrono::{DateTime, FixedOffset, Utc};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

use crate::services::{
    chat_store::{ChatStore, Message},
    error::ErrorService,
    file_store::FileStore,
    settings_store::SettingsStore,
};

/// Tool output can be a whole file's text; past this much it only bloats the export
const MAX_TOOL_OUTPUT_CHARS: usize = 4000;
const MAX_TOOL_ARGUMENT_CHARS: usize = 2000;
/// Text files past this size aren't printed into a document
const MAX_EMBEDDED_TEXT_BYTES: usize = 2 * 1024 * 1024;
const MAX_EMBEDDED_TEXT_CHARS: usize = 20_000;
/// Widest and tallest an image is set in a PDF, in millimetres
const MAX_IMAGE_WIDTH_MM: f32 = 120.0;
const MAX_IMAGE_HEIGHT_MM: f32 = 90.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Markdown,
    Pdf,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AttachmentMode {
    /// Left out; a message that had some says how many
    None,
    /// Put in the document itself: images shown in place, text files printed. What can't be (a
    /// binary file, a very large one) is left out, and the document says so.
    Embed,
    /// Every attachment kept as a file in a zip next to the document; images are shown in the
    /// PDF as well
    Zip,
}

pub struct ExportOptions {
    pub format: ExportFormat,
    /// Tool calls, their results and job notices; left out they are simply absent
    pub tools: bool,
    pub attachments: AttachmentMode,
}

pub struct ExportFile {
    pub file_name: String,
    pub content_type: &'static str,
    pub bytes: Vec<u8>,
}

pub(crate) enum Speaker {
    User,
    Assistant,
    /// A tool's result, by the tool's name
    Tool(String),
    Notice,
}

pub(crate) struct ExportCall {
    pub name: String,
    pub arguments: String,
}

pub(crate) enum AttachmentBody {
    /// An image the document shows in place. The PDF finds its bytes by `key`; Markdown points at
    /// `markdown_source`, which is a path inside the zip or, when embedded, a data URI.
    Image { key: String, width_mm: f32, markdown_source: String },
    /// A text file, printed in the document
    Text(String),
    /// A file that is only named in the document; its bytes are in the zip at `path`
    Listed { path: String },
}

pub(crate) struct ExportAttachment {
    pub name: String,
    pub body: AttachmentBody,
}

pub(crate) struct ExportEntry {
    pub speaker: Speaker,
    pub time: DateTime<FixedOffset>,
    pub text: String,
    pub calls: Vec<ExportCall>,
    pub attachments: Vec<ExportAttachment>,
    /// How many attachments the message has that were left out, so the reader knows
    pub omitted_attachments: usize,
}

pub(crate) struct Transcript {
    pub title: String,
    pub exported_at: DateTime<FixedOffset>,
    pub entries: Vec<ExportEntry>,
}

/// An attachment as read from the chat, before it is placed in the export
struct Loaded {
    name: String,
    bytes: Vec<u8>,
}

/// Everything the export needs besides the entries: the images the PDF shows by key, and the
/// files that go in the zip
struct Payload {
    inline_images: Vec<(String, Vec<u8>)>,
    zip_files: Vec<(String, Vec<u8>)>,
}

#[derive(Clone)]
pub struct ExportFacade {
    chat_store: Arc<ChatStore>,
    file_store: Arc<FileStore>,
    settings_store: Arc<SettingsStore>,
}

impl ExportFacade {
    pub fn new(chat_store: Arc<ChatStore>, file_store: Arc<FileStore>, settings_store: Arc<SettingsStore>) -> Self {
        Self { chat_store, file_store, settings_store }
    }

    pub async fn export(&self, user_id: i64, chat_id: i64, options: ExportOptions) -> Result<ExportFile, ErrorService> {
        let chat = self.chat_store.owned_chat(user_id, chat_id).await?;
        let timezone_hours = self.settings_store.settings(user_id).await?.timezone.unwrap_or(0);
        // An out-of-range offset falls back to UTC rather than failing the export
        let offset = FixedOffset::east_opt(timezone_hours.saturating_mul(3600)).unwrap_or_else(|| FixedOffset::east_opt(0).expect("zero is a valid offset"));

        let messages = self.chat_store.all_messages(chat_id).await?;
        let (entries, payload) = self.entries(user_id, messages, &options, offset).await?;
        let transcript = Transcript {
            title: chat.name.clone(),
            exported_at: Utc::now().with_timezone(&offset),
            entries,
        };

        let slug = Self::file_slug(&chat.name);
        let (extension, content_type) = match options.format {
            ExportFormat::Markdown => ("md", "text/markdown; charset=utf-8"),
            ExportFormat::Pdf => ("pdf", "application/pdf"),
        };
        let format = options.format;

        // Typst compiling and deflating are CPU work that can take a while on a long chat
        let (file_name, content_type, bytes) = tokio::task::spawn_blocking(move || -> Result<_, ErrorService> {
            let document = match format {
                ExportFormat::Markdown => markdown::render(&transcript).into_bytes(),
                ExportFormat::Pdf => {
                    let inline = payload.inline_images.iter().map(|(key, bytes)| (key.as_str(), bytes.as_slice()));
                    pdf::render(&transcript, inline)
                        .map_err(|e| ErrorService::internal(format!("couldn't build the PDF: {e}")))?
                }
            };
            if payload.zip_files.is_empty() {
                return Ok((format!("{slug}.{extension}"), content_type, document));
            }

            let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
            let zip_options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            let zipped = (|| -> Result<(), zip::result::ZipError> {
                zip.start_file(format!("{slug}.{extension}"), zip_options)?;
                zip.write_all(&document)?;
                for (path, bytes) in &payload.zip_files {
                    zip.start_file(path.as_str(), zip_options)?;
                    zip.write_all(bytes)?;
                }
                Ok(())
            })();
            zipped.map_err(|e| ErrorService::internal(format!("couldn't build the archive: {e}")))?;
            let archive = zip
                .finish()
                .map_err(|e| ErrorService::internal(format!("couldn't build the archive: {e}")))?
                .into_inner();
            Ok((format!("{slug}.zip"), "application/zip", archive))
        })
        .await
        .map_err(|e| ErrorService::internal(format!("the export task failed: {e}")))??;

        Ok(ExportFile { file_name, content_type, bytes })
    }

    /// The entries of the transcript, plus the bytes of every attachment they point at
    async fn entries(
        &self,
        user_id: i64,
        messages: Vec<Message>,
        options: &ExportOptions,
        offset: FixedOffset,
    ) -> Result<(Vec<ExportEntry>, Payload), ErrorService> {
        let mut entries = Vec::new();
        let mut payload = Payload { inline_images: vec![], zip_files: vec![] };

        for message in messages {
            let speaker = match message.role.as_str() {
                "user" => Speaker::User,
                "assistant" => Speaker::Assistant,
                "tool" if options.tools => Speaker::Tool(message.tool_name.clone().unwrap_or_else(|| "tool".to_string())),
                "notice" if options.tools => Speaker::Notice,
                _ => continue,
            };
            let is_conversation = matches!(speaker, Speaker::User | Speaker::Assistant);

            let text = match speaker {
                Speaker::Tool(_) => Self::cut(message.content.trim(), MAX_TOOL_OUTPUT_CHARS),
                _ => message.content.trim().to_string(),
            };
            let calls: Vec<ExportCall> = if options.tools {
                message
                    .tool_calls
                    .iter()
                    .map(|call| ExportCall {
                        name: call.tool_name.clone(),
                        arguments: Self::cut(&serde_json::to_string_pretty(&call.arguments).unwrap_or_default(), MAX_TOOL_ARGUMENT_CHARS),
                    })
                    .collect()
            } else {
                vec![]
            };

            let mut attachments = Vec::new();
            let mut omitted = 0;
            if is_conversation {
                let carried = message.images.len() + message.file_ids.len();
                if options.attachments == AttachmentMode::None {
                    omitted = carried;
                } else {
                    let loaded = self.load_attachments(user_id, &message).await?;
                    // Whatever couldn't be read (a file gone from the disk) is left out as well
                    omitted += carried - loaded.len();
                    for (index, item) in loaded.into_iter().enumerate() {
                        match Self::place(&message, index, item, options.attachments, &mut payload) {
                            Some(attachment) => attachments.push(attachment),
                            None => omitted += 1,
                        }
                    }
                }
            }

            if text.is_empty() && calls.is_empty() && attachments.is_empty() && omitted == 0 {
                continue;
            }
            entries.push(ExportEntry {
                speaker,
                time: message.created_at.with_timezone(&offset),
                text,
                calls,
                attachments,
                omitted_attachments: omitted,
            });
        }

        Ok((entries, payload))
    }

    /// The images and files of one message, read in, images first. A file that is gone from the
    /// disk or the store is skipped rather than failing the whole export.
    async fn load_attachments(&self, user_id: i64, message: &Message) -> Result<Vec<Loaded>, ErrorService> {
        let mut loaded = Vec::new();

        for (index, encoded) in message.images.iter().enumerate() {
            let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(encoded) else {
                continue;
            };
            let extension = infer::get(&bytes).map(|kind| kind.extension()).unwrap_or("bin");
            loaded.push(Loaded { name: format!("image-{}.{extension}", index + 1), bytes });
        }

        for &file_id in &message.file_ids {
            let Ok(record) = self.file_store.get(file_id).await else {
                continue;
            };
            // A file id in a message is the user's own, but nothing else enforces it
            if record.user_id != user_id {
                continue;
            }
            let Ok(bytes) = tokio::fs::read(&record.full_path).await else {
                continue;
            };
            loaded.push(Loaded { name: Self::safe_file_name(&record.file_name), bytes });
        }

        Ok(loaded)
    }

    /// Puts one loaded attachment into the export the way `mode` asks for, adding what it needs to
    /// `payload`. `None` when the mode can't hold it (a binary file in a document, say).
    fn place(message: &Message, index: usize, item: Loaded, mode: AttachmentMode, payload: &mut Payload) -> Option<ExportAttachment> {
        let Loaded { name, bytes } = item;
        let inline_key = Self::inline_key(&bytes, payload.inline_images.len());

        match mode {
            AttachmentMode::None => None,
            AttachmentMode::Embed => {
                if let Some(key) = inline_key {
                    let mime = infer::get(&bytes).map(|kind| kind.mime_type()).unwrap_or("application/octet-stream");
                    let markdown_source = format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(&bytes));
                    let width_mm = Self::display_width_mm(&bytes);
                    payload.inline_images.push((key.clone(), bytes));
                    return Some(ExportAttachment { name, body: AttachmentBody::Image { key, width_mm, markdown_source } });
                }
                Self::printable_text(&bytes).map(|text| ExportAttachment { name, body: AttachmentBody::Text(Self::cut(&text, MAX_EMBEDDED_TEXT_CHARS)) })
            }
            AttachmentMode::Zip => {
                let path = format!("attachments/{}-{}-{name}", message.id, index + 1);
                let body = match inline_key {
                    Some(key) => {
                        payload.inline_images.push((key.clone(), bytes.clone()));
                        AttachmentBody::Image {
                            key,
                            width_mm: Self::display_width_mm(&bytes),
                            markdown_source: path.replace(' ', "%20"),
                        }
                    }
                    None => AttachmentBody::Listed { path: path.replace(' ', "%20") },
                };
                payload.zip_files.push((path, bytes));
                Some(ExportAttachment { name, body })
            }
        }
    }

    /// The text of a file that is plain UTF-8 text and small enough to print, else `None`
    fn printable_text(bytes: &[u8]) -> Option<String> {
        if bytes.len() > MAX_EMBEDDED_TEXT_BYTES || bytes.contains(&0) {
            return None;
        }
        std::str::from_utf8(bytes).ok().map(|text| text.trim_end().to_string())
    }

    /// A key the PDF can reference an image by, for the formats Typst can set; `None` for the rest
    fn inline_key(bytes: &[u8], n: usize) -> Option<String> {
        let extension = match infer::get(bytes)?.mime_type() {
            "image/png" => "png",
            "image/jpeg" => "jpg",
            "image/gif" => "gif",
            "image/webp" => "webp",
            _ => return None,
        };
        Some(format!("inline-{n}.{extension}"))
    }

    /// The width to set an image at: its natural size (at 96 dpi), shrunk to fit the box above with its
    /// proportions kept. 80 mm when its size can't be read.
    fn display_width_mm(bytes: &[u8]) -> f32 {
        let dimensions = image::ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .ok()
            .and_then(|reader| reader.into_dimensions().ok());
        let Some((width, height)) = dimensions.filter(|&(w, h)| w > 0 && h > 0) else {
            return 80.0;
        };
        let (width, height) = (width as f32 * 25.4 / 96.0, height as f32 * 25.4 / 96.0);
        let scale = (MAX_IMAGE_WIDTH_MM / width).min(MAX_IMAGE_HEIGHT_MM / height).min(1.0);
        // A hair of width keeps a 1-pixel image from collapsing to nothing
        (width * scale).max(2.0)
    }

    /// A client-chosen file name made safe to put inside an archive: its last path component only,
    /// with anything that could act as a separator or control character replaced.
    fn safe_file_name(name: &str) -> String {
        let last = name.rsplit(['/', '\\']).next().unwrap_or(name);
        let cleaned: String = last.chars().map(|c| if c.is_control() || matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { c }).collect();
        let cleaned = cleaned.trim().trim_start_matches('.').to_string();
        if cleaned.is_empty() { "file".to_string() } else { cleaned }
    }

    /// The chat's name as a file name: letters and digits kept, anything else a dash
    fn file_slug(name: &str) -> String {
        let slug: String = name
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .split('-')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("-");
        let slug: String = slug.chars().take(60).collect();
        if slug.is_empty() { "chat".to_string() } else { slug }
    }

    fn cut(text: &str, max_chars: usize) -> String {
        let total = text.chars().count();
        if total <= max_chars {
            return text.to_string();
        }
        let head: String = text.chars().take(max_chars).collect();
        format!("{head}\n… ({} more characters not included)", total - max_chars)
    }
}
