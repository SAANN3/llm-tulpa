use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};
use utoipa::OpenApi;

use crate::state::AppState;

use super::create::*;
use super::delete::*;
use super::export::*;
use super::find::*;
use super::get::*;
use super::messages::*;
use super::rename::*;
use super::rewind::*;
use super::search::*;
use super::set_folder::*;
use super::set_model::*;
use super::set_profile::*;
use super::set_tools::*;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(get_chats).post(create_chat).delete(delete_chat))
        .route("/rename", post(rename_chat))
        .route("/rewind", post(rewind_chat))
        .route("/model", post(set_model))
        .route("/profile", post(set_profile))
        .route("/tools", post(set_tools))
        .route("/folder", post(set_folder))
        .route("/messages", get(get_messages))
        .route("/search", get(search_messages))
        .route("/find", get(find_chats))
        .route("/export", get(export_chat))
}

#[derive(OpenApi)]
#[openapi(
    paths(get_chats, create_chat, delete_chat, rename_chat, rewind_chat, set_model, set_profile, set_tools, set_folder, get_messages, search_messages, find_chats, export_chat),
    components(schemas(
        ChatOut,
        ChatListOut,
        GetChatsResponse,
        CreateChatRequest,
        RenameChatRequest,
        RewindChatRequest,
        RewindChatResponse,
        SetModelRequest,
        SetProfileRequest,
        SetToolsRequest,
        SetFolderRequest,
        MessageToolCallOut,
        MessageOut,
        MessagesResponse,
        MessageSearchOut,
        MessageSearchResponse,
        ChatFindMessageOut,
        ChatFindOut,
        FindChatsResponse,
        ExportFormatParam,
        AttachmentModeParam,
    )),
)]
pub struct ApiDoc;
