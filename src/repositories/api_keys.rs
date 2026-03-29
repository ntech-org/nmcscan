use sea_orm::*;
use sea_orm::sea_query::Expr;
use std::sync::Arc;
use crate::models::entities::api_keys;

#[derive(Clone)]
pub struct ApiKeyRepository {
    db: DatabaseConnection,
}

impl ApiKeyRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// List active keys for a user.
    pub async fn list_for_user(&self, user_id: i32) -> Result<Vec<api_keys::Model>, DbErr> {
        api_keys::Entity::find()
            .filter(api_keys::Column::UserId.eq(user_id))
            .filter(api_keys::Column::Revoked.eq(false))
            .order_by_desc(api_keys::Column::CreatedAt)
            .all(&self.db)
            .await
    }

    /// Create a new API key for a user.
    pub async fn create_key(
        &self,
        user_id: i32,
        name: &str,
        key_hash: &str,
    ) -> Result<api_keys::Model, DbErr> {
        let new_key = api_keys::ActiveModel {
            user_id: Set(user_id),
            name: Set(name.to_string()),
            key_hash: Set(key_hash.to_string()),
            created_at: Set(chrono::Utc::now().into()),
            revoked: Set(false),
            ..Default::default()
        };

        new_key.insert(&self.db).await
    }

    /// Revoke an API key by ID and User ID (to ensure ownership).
    pub async fn revoke_key(&self, user_id: i32, key_id: i32) -> Result<bool, DbErr> {
        let update_result = api_keys::Entity::update_many()
            .col_expr(api_keys::Column::Revoked, Expr::value(true))
            .filter(api_keys::Column::Id.eq(key_id))
            .filter(api_keys::Column::UserId.eq(user_id))
            .exec(&self.db)
            .await?;

        Ok(update_result.rows_affected > 0)
    }

    /// Validate a key hash and return the user_id if valid.
    pub async fn validate_key(&self, key_hash: &str) -> Result<Option<i32>, DbErr> {
        let key = api_keys::Entity::find()
            .filter(api_keys::Column::KeyHash.eq(key_hash))
            .filter(api_keys::Column::Revoked.eq(false))
            .one(&self.db)
            .await?;

        if let Some(key_model) = key {
            // Update last_used_at
            let mut active_key: api_keys::ActiveModel = key_model.clone().into();
            active_key.last_used_at = Set(Some(chrono::Utc::now().into()));
            if let Err(e) = active_key.update(&self.db).await {
                tracing::warn!("Failed to update last_used_at for api_key: {}", e);
            }
            
            return Ok(Some(key_model.user_id));
        }

        Ok(None)
    }
}
