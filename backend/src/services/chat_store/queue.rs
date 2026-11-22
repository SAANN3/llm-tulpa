use sea_orm::{ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};

use super::entities::queued_prompts;
use super::{ChatStore, ChatStoreErrors};
use crate::services::llm::ThinkChoice;

/// A prompt waiting for its chat's turn to finish, in the order it was sent.
/// What a page is shown for one waiting prompt. The image bytes stay in the row.
pub struct QueuedMessageView {
    pub id: i64,
    pub prompt: String,
    pub images: u32,
    pub file_ids: Vec<i64>,
}

pub struct QueuedPrompt {
    pub prompt: String,
    pub images: Vec<String>,
    pub file_ids: Vec<i64>,
    pub think: Option<ThinkChoice>,
}

impl ChatStore {
    /// Puts a prompt behind any already waiting for this chat.
    pub async fn enqueue_prompt(
        &self,
        chat_id: i64,
        prompt: String,
        images: Vec<String>,
        file_ids: Vec<i64>,
        think: Option<ThinkChoice>,
    ) -> Result<(), ChatStoreErrors> {
        let position = queued_prompts::Entity::find()
            .filter(queued_prompts::Column::ChatId.eq(chat_id))
            .order_by_desc(queued_prompts::Column::Position)
            .one(&self.db)
            .await?
            .map(|row| row.position.saturating_add(1))
            .unwrap_or(0);
        queued_prompts::Entity::insert(queued_prompts::ActiveModel {
            chat_id: Set(chat_id),
            position: Set(position),
            prompt: Set(prompt),
            images: Set(serde_json::to_value(&images).unwrap_or(serde_json::json!([]))),
            file_ids: Set(serde_json::to_value(&file_ids).unwrap_or(serde_json::json!([]))),
            think: Set(think.map(|think| serde_json::to_string(&think).unwrap_or_default())),
            ..Default::default()
        })
        .exec(&self.db)
        .await?;
        Ok(())
    }

    /// Puts a prompt ahead of the ones already waiting. Used when starting the oldest one failed.
    pub async fn requeue_prompt_front(
        &self,
        chat_id: i64,
        prompt: String,
        images: Vec<String>,
        file_ids: Vec<i64>,
        think: Option<ThinkChoice>,
    ) -> Result<(), ChatStoreErrors> {
        let position = queued_prompts::Entity::find()
            .filter(queued_prompts::Column::ChatId.eq(chat_id))
            .order_by_asc(queued_prompts::Column::Position)
            .one(&self.db)
            .await?
            .map(|row| row.position.saturating_sub(1))
            .unwrap_or(0);
        queued_prompts::Entity::insert(queued_prompts::ActiveModel {
            chat_id: Set(chat_id),
            position: Set(position),
            prompt: Set(prompt),
            images: Set(serde_json::to_value(&images).unwrap_or(serde_json::json!([]))),
            file_ids: Set(serde_json::to_value(&file_ids).unwrap_or(serde_json::json!([]))),
            think: Set(think.map(|think| serde_json::to_string(&think).unwrap_or_default())),
            ..Default::default()
        })
        .exec(&self.db)
        .await?;
        Ok(())
    }

    /// The waiting prompts, oldest first, without the image bytes.
    pub async fn queued_messages(&self, chat_id: i64) -> Result<Vec<QueuedMessageView>, ChatStoreErrors> {
        let rows = queued_prompts::Entity::find()
            .filter(queued_prompts::Column::ChatId.eq(chat_id))
            .order_by_asc(queued_prompts::Column::Position)
            .order_by_asc(queued_prompts::Column::Id)
            .all(&self.db)
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| QueuedMessageView {
                id: row.id,
                prompt: row.prompt,
                images: row.images.as_array().map(|items| items.len() as u32).unwrap_or(0),
                file_ids: serde_json::from_value(row.file_ids).unwrap_or_default(),
            })
            .collect())
    }

    /// Takes the oldest waiting prompt. A second caller taking the same row finds it already gone.
    pub async fn pop_queued_prompt(&self, chat_id: i64) -> Result<Option<QueuedPrompt>, ChatStoreErrors> {
        let Some(row) = queued_prompts::Entity::find()
            .filter(queued_prompts::Column::ChatId.eq(chat_id))
            .order_by_asc(queued_prompts::Column::Position)
            .order_by_asc(queued_prompts::Column::Id)
            .one(&self.db)
            .await?
        else {
            return Ok(None);
        };
        let gone = queued_prompts::Entity::delete_by_id(row.id).exec(&self.db).await?;
        if gone.rows_affected == 0 {
            return Ok(None);
        }
        let images = serde_json::from_value(row.images).unwrap_or_default();
        let file_ids = serde_json::from_value(row.file_ids).unwrap_or_default();
        let think = row.think.and_then(|text| serde_json::from_str::<ThinkChoice>(&text).ok());
        Ok(Some(QueuedPrompt { prompt: row.prompt, images, file_ids, think }))
    }

    /// Drops every prompt waiting for this chat.
    pub async fn clear_queued_prompts(&self, chat_id: i64) -> Result<(), ChatStoreErrors> {
        queued_prompts::Entity::delete_many()
            .filter(queued_prompts::Column::ChatId.eq(chat_id))
            .exec(&self.db)
            .await?;
        Ok(())
    }

    /// Drops one waiting prompt. `false` when that row is not in this chat's queue.
    pub async fn delete_queued_prompt(&self, chat_id: i64, id: i64) -> Result<bool, ChatStoreErrors> {
        let gone = queued_prompts::Entity::delete_many()
            .filter(queued_prompts::Column::ChatId.eq(chat_id))
            .filter(queued_prompts::Column::Id.eq(id))
            .exec(&self.db)
            .await?;
        Ok(gone.rows_affected > 0)
    }

    /// Chats that still have a prompt waiting, so a restart can start them.
    pub async fn chats_with_queued_prompts(&self) -> Result<Vec<i64>, ChatStoreErrors> {
        let rows = queued_prompts::Entity::find()
            .select_only()
            .column(queued_prompts::Column::ChatId)
            .distinct()
            .into_tuple::<i64>()
            .all(&self.db)
            .await?;
        Ok(rows)
    }
}
