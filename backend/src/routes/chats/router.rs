use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};
use utoipa::OpenApi;

use crate::state::AppState;

use super::create::*;
use super::delete::*;
use super::find::*;
use super::get::*;
use super::messages::*;
use super::rename::*;
use super::search::*;
use super::set_folder::*;
use super::set_model::*;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(get_chats).post(create_chat).delete(delete_chat))
        .route("/rename", post(rename_chat))
        .route("/model", post(set_model))
        .route("/folder", post(set_folder))
        .route("/messages", get(get_messages))
        .route("/search", get(search_messages))
        .route("/find", get(find_chats))
}

#[derive(OpenApi)]
#[openapi(
    paths(get_chats, create_chat, delete_chat, rename_chat, set_model, set_folder, get_messages, search_messages, find_chats),
    components(schemas(
        ChatOut,
        ChatListOut,
        GetChatsResponse,
        CreateChatRequest,
        RenameChatRequest,
        SetModelRequest,
        SetFolderRequest,
        MessageToolCallOut,
        MessageOut,
        MessagesResponse,
        MessageSearchOut,
        MessageSearchResponse,
        ChatFindMessageOut,
        ChatFindOut,
        FindChatsResponse,
    )),
)]
pub struct ApiDoc;
