use crate::models::entities::minecraft_accounts;
use sea_orm::*;

#[derive(Clone)]
pub struct MinecraftAccountRepository {
    db: DatabaseConnection,
}

impl MinecraftAccountRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn add_account(
        &self,
        email: String,
        password: Option<String>,
        access_token: Option<String>,
        refresh_token: Option<String>,
        expires_at: Option<chrono::NaiveDateTime>,
    ) -> Result<minecraft_accounts::Model, DbErr> {
        let model = minecraft_accounts::ActiveModel {
            email: Set(email),
            password: Set(password),
            access_token: Set(access_token),
            refresh_token: Set(refresh_token),
            expires_at: Set(expires_at),
            status: Set("active".to_string()),
            ..Default::default()
        };

        minecraft_accounts::Entity::insert(model)
            .exec_with_returning(&self.db)
            .await
    }

    pub async fn get_all_accounts(&self) -> Result<Vec<minecraft_accounts::Model>, DbErr> {
        minecraft_accounts::Entity::find().all(&self.db).await
    }

    pub async fn delete_account(&self, id: i32) -> Result<DeleteResult, DbErr> {
        minecraft_accounts::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
    }

    pub async fn get_active_accounts(&self) -> Result<Vec<minecraft_accounts::Model>, DbErr> {
        minecraft_accounts::Entity::find()
            .filter(minecraft_accounts::Column::Status.eq("active"))
            .all(&self.db)
            .await
    }
}
