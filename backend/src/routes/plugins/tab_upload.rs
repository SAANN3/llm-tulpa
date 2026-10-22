use std::sync::Arc;

use axum::{extract::Multipart, extract::State, http::StatusCode, Json};
use utoipa::ToSchema;

use crate::{
    plugins::tab::{TabFile, TabLayout},
    routes::auth::AuthUser,
    services::error::ErrorService,
    state::AppState,
};

/// Doc-only shape of the multipart body, like `UploadFileRequest`: the handler reads the stream field by field.
#[derive(ToSchema)]
#[allow(dead_code)]
pub(crate) struct TabUploadRequest {
    plugin_name: String,
    plugin_subname: String,
    /// The drop zone's action
    action: String,
    /// Each file under the field name `file`; its multipart filename is its path, a folder's files keeping their folder
    #[schema(value_type = String)]
    file: Vec<u8>,
}

/// Files the caller dropped on a drop zone of a plugin's tab, as a multipart form: the plugin, the zone's action and one
/// `file` part per file. Answers with the layout as it is after.
#[utoipa::path(
    post,
    path = "/api/plugins/tab/upload",
    tag = "plugins",
    request_body(content = TabUploadRequest, content_type = "multipart/form-data"),
    responses(
        (status = 200, description = "The tab's layout after the upload", body = TabLayout),
        (status = 400, description = "Malformed form, or the plugin refused the files", body = crate::services::error::ErrorBody),
        (status = 404, description = "No such tab, or its plugin is not enabled", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn plugin_tab_upload(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    mut multipart: Multipart,
) -> Result<Json<TabLayout>, ErrorService> {
    let bad = |what: String| ErrorService::new(StatusCode::BAD_REQUEST, what);
    let (mut plugin_name, mut plugin_subname, mut action) = (None, None, None);
    let mut files = Vec::new();
    while let Some(field) = multipart.next_field().await.map_err(|e| bad(format!("invalid multipart body: {e}")))? {
        match field.name().unwrap_or("") {
            "plugin_name" => plugin_name = Some(field.text().await.map_err(|e| bad(e.to_string()))?),
            "plugin_subname" => plugin_subname = Some(field.text().await.map_err(|e| bad(e.to_string()))?),
            "action" => action = Some(field.text().await.map_err(|e| bad(e.to_string()))?),
            "file" => {
                let path = field.file_name().unwrap_or("").to_string();
                let bytes = field.bytes().await.map_err(|e| bad(format!("could not read a file: {e}")))?.to_vec();
                files.push(TabFile { path, bytes });
            }
            _ => {}
        }
    }
    let (Some(plugin_name), Some(plugin_subname), Some(action)) = (plugin_name, plugin_subname, action) else {
        return Err(bad("the plugin, its subname and the action are needed".to_string()));
    };
    if files.is_empty() {
        return Err(bad("no file was sent".to_string()));
    }
    let layout = state.services().await?.plugin_registry.tab_upload(&plugin_name, &plugin_subname, auth.id, &action, files).await?;
    Ok(Json(layout))
}
