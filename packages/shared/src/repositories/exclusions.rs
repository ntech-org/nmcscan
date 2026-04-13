use crate::models::entities::exclusions;
use sea_orm::*;
use std::collections::HashSet;

#[derive(Clone)]
pub struct ExclusionRepository {
    db: DatabaseConnection,
}

impl ExclusionRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// Get all exclusions with pagination.
    pub async fn get_all(
        &self,
        page: u64,
        limit: u64,
    ) -> Result<Vec<exclusions::Model>, DbErr> {
        exclusions::Entity::find()
            .order_by_asc(exclusions::Column::Id)
            .offset(page * limit)
            .limit(limit)
            .all(&self.db)
            .await
    }

    /// Get all exclusion networks as a HashSet for fast lookup.
    pub async fn get_all_networks(&self) -> Result<HashSet<String>, DbErr> {
        let exclusions = exclusions::Entity::find()
            .select_only()
            .column(exclusions::Column::Network)
            .all(&self.db)
            .await?;
        Ok(exclusions.into_iter().map(|e| e.network).collect())
    }

    /// Count total exclusions.
    pub async fn count(&self) -> Result<u64, DbErr> {
        exclusions::Entity::find().count(&self.db).await
    }

    /// Add a new exclusion. Returns error if network already exists.
    pub async fn insert(
        &self,
        network: &str,
        comment: Option<&str>,
        source: &str,
    ) -> Result<(), DbErr> {
        let exclusion = exclusions::ActiveModel {
            network: Set(network.to_string()),
            comment: Set(comment.map(|c| c.to_string())),
            source: Set(source.to_string()),
            ..Default::default()
        };
        exclusions::Entity::insert(exclusion)
            .on_conflict(
                sea_query::OnConflict::column(exclusions::Column::Network)
                    .do_nothing()
                    .to_owned(),
            )
            .exec(&self.db)
            .await?;
        Ok(())
    }

    /// Delete an exclusion by ID.
    pub async fn delete(&self, id: i32) -> Result<u64, DbErr> {
        let result = exclusions::Entity::delete_by_id(id)
            .exec(&self.db)
            .await?;
        Ok(result.rows_affected)
    }

    /// Seed exclusions from the static config file at startup.
    /// Parses each line and upserts it with source='config_file' or source='honeypot'.
    pub async fn seed_from_config(
        &self,
        file_content: &str,
        source: &str,
    ) -> Result<usize, DbErr> {
        let mut count = 0;
        for line in file_content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            // Extract comment if present
            let (network, comment) = if let Some(idx) = line.find('#') {
                let net = line[..idx].trim();
                let cmt = line[idx + 1..].trim();
                (net, Some(cmt))
            } else {
                (line, None)
            };

            if network.is_empty() {
                continue;
            }

            let _ = self.insert(network, comment, source).await;
            count += 1;
        }
        Ok(count)
    }
}
