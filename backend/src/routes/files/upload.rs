use std::sync::Arc;

use axum::{extract::Multipart, extract::State, http::StatusCode, Json};
use utoipa::ToSchema;

use crate::{services::error::ErrorService, state::AppState};

use super::get::FileOut;

/// Doc-only shape of the multipart body — never actually deserialized, `upload_file`
/// reads the real `Multipart` stream field by field instead. Exists purely so the
/// generated OpenAPI doc shows what a client needs to send.
#[derive(ToSchema)]
#[allow(dead_code)]
pub(crate) struct UploadFileRequest {
    /// The chat this file belongs to. Omit for a file uploaded before a chat exists
    /// yet (the home page's case) — it gets claimed by whichever chat it's actually
    /// attached to a message in, once that happens.
    chat_id: Option<i64>,
    /// Defaults to `true` if omitted.
    read_only: Option<bool>,
    /// The file itself — its multipart filename becomes the stored `file_name`.
    #[schema(value_type = String)]
    file: Vec<u8>,
}

/// Uploads a file as a normal multipart form (the same shape an HTML `<form
/// enctype="multipart/form-data">` posts), not JSON: a `file` part carrying the bytes
/// (its filename becomes the stored, UI-facing `file_name`), an optional `chat_id`
/// text field (omit it to upload before a chat exists yet — see `UploadFileRequest`),
/// and an optional `read_only` text field (`"true"`/`"false"`, defaults to `true`).
#[utoipa::path(
    post,
    path = "/api/files/upload",
    tag = "files",
    request_body(content = UploadFileRequest, content_type = "multipart/form-data"),
    responses(
        (status = 200, description = "File stored", body = FileOut),
        (status = 400, description = "Malformed multipart body, or a required field is missing/invalid", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed, or the file couldn't be written to disk", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn upload_file(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Json<FileOut>, ErrorService> {
    let mut chat_id: Option<i64> = None;
    let mut read_only: Option<bool> = None;
    let mut file_name: Option<String> = None;
    let mut file_bytes: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ErrorService::new(StatusCode::BAD_REQUEST, format!("invalid multipart body: {e}")))?
    {
        let field_name = field.name().unwrap_or("").to_string();

        match field_name.as_str() {
            "chat_id" => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| ErrorService::new(StatusCode::BAD_REQUEST, format!("invalid 'chat_id' field: {e}")))?;
                chat_id = Some(
                    text.trim()
                        .parse()
                        .map_err(|_| ErrorService::new(StatusCode::BAD_REQUEST, "'chat_id' must be an integer"))?,
                );
            }
            "read_only" => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| ErrorService::new(StatusCode::BAD_REQUEST, format!("invalid 'read_only' field: {e}")))?;
                read_only = Some(
                    text.trim()
                        .parse()
                        .map_err(|_| ErrorService::new(StatusCode::BAD_REQUEST, "'read_only' must be 'true' or 'false'"))?,
                );
            }
            "file" => {
                file_name = Some(field.file_name().unwrap_or("file").to_string());
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| ErrorService::new(StatusCode::BAD_REQUEST, format!("invalid 'file' field: {e}")))?;
                file_bytes = Some(bytes.to_vec());
            }
            // Unknown fields are ignored rather than rejected — lets a caller send
            // extra form fields (a browser form's own bookkeeping, e.g.) without this
            // route needing to know about every one of them.
            _ => {}
        }
    }

    let file_name = file_name.ok_or_else(|| ErrorService::new(StatusCode::BAD_REQUEST, "missing 'file' field"))?;
    let bytes = file_bytes.ok_or_else(|| ErrorService::new(StatusCode::BAD_REQUEST, "missing 'file' field"))?;

    let record = state.file_store.store_bytes(chat_id, &file_name, &bytes, read_only).await?;

    Ok(Json(record.into()))
}
