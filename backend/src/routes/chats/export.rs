use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::header,
    response::IntoResponse,
};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

use crate::{
    facade::export::{AttachmentMode, ExportFormat, ExportOptions},
    routes::auth::AuthUser,
    services::error::ErrorService,
    state::AppState,
};

#[derive(Deserialize, ToSchema, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ExportFormatParam {
    /// Markdown text
    Md,
    /// A PDF document
    Pdf,
}

#[derive(Deserialize, ToSchema, Clone, Copy, Default)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AttachmentModeParam {
    /// Left out; a message that had some says how many
    #[default]
    None,
    /// Put in the document: images shown in place, text files printed, anything else left out
    Embed,
    /// Every attachment as a file in a zip next to the document, images shown in the PDF too
    Zip,
}

#[derive(Deserialize, IntoParams)]
pub(crate) struct ExportChatQuery {
    chat_id: i64,
    format: ExportFormatParam,
    /// Include tool calls, their results and job notices. Default: left out.
    #[serde(default)]
    tools: bool,
    /// What to do with the images and files messages carry. Default: `none`.
    #[serde(default)]
    attachments: AttachmentModeParam,
}

/// Exports a chat's conversation as Markdown or a PDF, in the caller's timezone. Thinking traces
/// are never included. `attachments=embed` puts images and text files in the document itself;
/// `attachments=zip`, with at least one attachment, returns a zip holding the document and an
/// `attachments/` folder instead of the bare document.
#[utoipa::path(
    get,
    path = "/api/chats/export",
    tag = "chats",
    params(ExportChatQuery),
    responses(
        (status = 200, description = "The document (`text/markdown` or `application/pdf`), or a zip when attachments are included", content_type = "application/octet-stream"),
        (status = 404, description = "No such chat", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed, or the document couldn't be built", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn export_chat(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<ExportChatQuery>,
) -> Result<impl IntoResponse, ErrorService> {
    let services = state.services().await?;
    let options = ExportOptions {
        format: match query.format {
            ExportFormatParam::Md => ExportFormat::Markdown,
            ExportFormatParam::Pdf => ExportFormat::Pdf,
        },
        tools: query.tools,
        attachments: match query.attachments {
            AttachmentModeParam::None => AttachmentMode::None,
            AttachmentModeParam::Embed => AttachmentMode::Embed,
            AttachmentModeParam::Zip => AttachmentMode::Zip,
        },
    };
    let file = services.export.export(auth.id, query.chat_id, options).await?;

    let headers = [
        (header::CONTENT_TYPE, file.content_type.to_string()),
        (header::CONTENT_DISPOSITION, content_disposition(&file.file_name)),
    ];
    Ok((headers, file.bytes))
}

/// `attachment` with the name as a plain-ASCII fallback plus the full name percent-encoded
/// (RFC 6266), since a header value can't carry the non-ASCII characters a chat's name may have
fn content_disposition(file_name: &str) -> String {
    let fallback: String = file_name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '_' })
        .collect();
    let encoded: String = file_name
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
            b => format!("%{b:02X}"),
        })
        .collect();
    format!("attachment; filename=\"{fallback}\"; filename*=UTF-8''{encoded}")
}
