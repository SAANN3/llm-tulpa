use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};
use utoipa::OpenApi;

use crate::state::AppState;

use super::create::*;
use super::delete::*;
use super::get::*;
use super::messages::*;
use super::rename::*;
use super::set_model::*;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(get_chats).post(create_chat).delete(delete_chat))
        .route("/rename", post(rename_chat))
        .route("/model", post(set_model))
        .route("/messages", get(get_messages))
}

#[derive(OpenApi)]
#[openapi(
    paths(get_chats, create_chat, delete_chat, rename_chat, set_model, get_messages),
    components(schemas(
        ChatOut,
        ChatListOut,
        GetChatsResponse,
        CreateChatRequest,
        RenameChatRequest,
        SetModelRequest,
        MessageToolCallOut,
        MessageOut,
        MessagesResponse,
    )),
)]
pub struct ApiDoc;
