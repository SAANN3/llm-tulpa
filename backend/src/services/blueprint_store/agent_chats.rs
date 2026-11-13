use super::{BlueprintStore, BlueprintStoreError, entities::blueprint_agent_chats as bindings};
use sea_orm::{ActiveValue::Set, SqlErr, TransactionTrait, prelude::*};

impl BlueprintStore {
    pub async fn agent_chat(
        &self,
        user_id: i64,
        id: i64,
        node: &str,
    ) -> Result<Option<i64>, BlueprintStoreError> {
        let txn = self.db.begin().await?;
        self.lock_blueprint(&txn, user_id, id).await?;
        let chat = bindings::Entity::find_by_id((id, node.to_owned()))
            .one(&txn)
            .await?
            .map(|b| b.chat_id);
        txn.commit().await?;
        Ok(chat)
    }
    pub async fn bind_agent_chat(
        &self,
        user_id: i64,
        id: i64,
        node: &str,
        chat_id: Option<i64>,
    ) -> Result<(), BlueprintStoreError> {
        let txn = self.db.begin().await?;
        self.lock_blueprint(&txn, user_id, id).await?;
        if let Some(chat_id) = chat_id {
            bindings::ActiveModel {
                blueprint_id: Set(id),
                node_id: Set(node.into()),
                chat_id: Set(chat_id),
            }
            .insert(&txn)
            .await
            .map_err(|error| match error.sql_err() {
                Some(SqlErr::UniqueConstraintViolation(_)) => BlueprintStoreError::Conflict,
                _ => BlueprintStoreError::Database(error),
            })?;
        } else {
            bindings::Entity::delete_by_id((id, node.to_owned()))
                .exec(&txn)
                .await?;
        }
        txn.commit().await?;
        Ok(())
    }
}
