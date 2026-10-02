//! Reading a whole chat back, for exporting it.

use sea_orm::{prelude::*, QueryOrder};

use super::entities::messages;
use super::{ChatStore, ChatStoreErrors, Message};

impl ChatStore {
    /// Every message of a chat, oldest first, each with its tool calls, images and files.
    pub async fn all_messages(&self, chat_id: i64) -> Result<Vec<Message>, ChatStoreErrors> {
        self.chat(chat_id).await?;

        let rows = messages::Entity::find()
            .filter(messages::Column::ChatId.eq(chat_id))
            .order_by_asc(messages::Column::Id)
            .all(&self.db)
            .await?;

        self.hydrate_messages(rows).await
    }
}
