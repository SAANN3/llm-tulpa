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

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(get_chats).post(create_chat).delete(delete_chat))
        .route("/rename", post(rename_chat))
        .route("/messages", get(get_messages))
}

#[derive(OpenApi)]
#[openapi(
    paths(get_chats, create_chat, delete_chat, rename_chat, get_messages),
    components(schemas(
        ChatOut,
        ChatListOut,
        GetChatsResponse,
        CreateChatRequest,
        RenameChatRequest,
        MessageToolCallOut,
        MessageOut,
        MessagesResponse,
    )),
)]
pub struct ApiDoc;
