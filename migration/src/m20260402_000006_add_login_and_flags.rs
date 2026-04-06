use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // Add login-related columns to servers table
        db.execute_unprepared(
            "ALTER TABLE servers ADD COLUMN IF NOT EXISTS login_obstacle TEXT DEFAULT NULL",
        )
        .await?;

        db.execute_unprepared(
            "ALTER TABLE servers ADD COLUMN IF NOT EXISTS last_login_at TIMESTAMP DEFAULT NULL",
        )
        .await?;

        // Add flags column (comma-separated tags for fast filtering)
        db.execute_unprepared("ALTER TABLE servers ADD COLUMN IF NOT EXISTS flags TEXT DEFAULT ''")
            .await?;

        // GIN trigram index on flags for LIKE-based filtering
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_servers_flags ON servers USING GIN (flags gin_trgm_ops)"
        ).await?;

        // Trigger function to auto-compute flags from brand, login_obstacle, players_online
        db.execute_unprepared(r#"
            CREATE OR REPLACE FUNCTION compute_server_flags() RETURNS trigger AS $$
            BEGIN
              NEW.flags := '';

              -- Server software type
              IF NEW.brand IS NULL OR NEW.brand IN ('Vanilla', 'Paper', 'Spigot', 'Purpur', 'Bukkit') THEN
                IF NEW.flags != '' THEN NEW.flags := NEW.flags || ','; END IF;
                NEW.flags := NEW.flags || 'vanilla';
              ELSIF NEW.brand IN ('Forge', 'Fabric', 'NeoForge') THEN
                IF NEW.flags != '' THEN NEW.flags := NEW.flags || ','; END IF;
                NEW.flags := NEW.flags || 'modded';
              ELSIF NEW.brand = 'Proxy' OR NEW.brand = 'Velocity' OR NEW.brand = 'BungeeCord' THEN
                IF NEW.flags != '' THEN NEW.flags := NEW.flags || ','; END IF;
                NEW.flags := NEW.flags || 'proxy';
              END IF;

              -- Activity
              IF NEW.players_online > 0 THEN
                IF NEW.flags != '' THEN NEW.flags := NEW.flags || ','; END IF;
                NEW.flags := NEW.flags || 'active';
              END IF;

              -- Login results
              IF NEW.login_obstacle IS NOT NULL THEN
                IF NEW.flags != '' THEN NEW.flags := NEW.flags || ','; END IF;
                NEW.flags := NEW.flags || 'login_tested';

                IF NEW.login_obstacle = 'success' THEN
                  NEW.flags := NEW.flags || ',cracked';
                ELSIF NEW.login_obstacle = 'premium' THEN
                  NEW.flags := NEW.flags || ',premium';
                ELSIF NEW.login_obstacle = 'whitelist' THEN
                  NEW.flags := NEW.flags || ',whitelisted';
                ELSIF NEW.login_obstacle = 'banned' THEN
                  NEW.flags := NEW.flags || ',banned';
                END IF;
              END IF;

              RETURN NEW;
            END;
            $$ LANGUAGE plpgsql;
        "#).await?;

        // Drop trigger if exists, then create
        db.execute_unprepared("DROP TRIGGER IF EXISTS trg_compute_server_flags ON servers")
            .await?;
        db.execute_unprepared(
            r#"
            CREATE TRIGGER trg_compute_server_flags
              BEFORE INSERT OR UPDATE OF brand, login_obstacle, players_online
              ON servers
              FOR EACH ROW
              EXECUTE FUNCTION compute_server_flags()
        "#,
        )
        .await?;

        // Backfill flags for existing rows
        db.execute_unprepared(
            r#"
            UPDATE servers SET flags = flags
              WHERE brand IS NOT NULL OR login_obstacle IS NOT NULL OR players_online > 0
        "#,
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        db.execute_unprepared("DROP TRIGGER IF EXISTS trg_compute_server_flags ON servers")
            .await?;
        db.execute_unprepared("DROP FUNCTION IF EXISTS compute_server_flags()")
            .await?;
        db.execute_unprepared("DROP INDEX IF EXISTS idx_servers_flags")
            .await?;
        db.execute_unprepared("ALTER TABLE servers DROP COLUMN IF EXISTS flags")
            .await?;
        db.execute_unprepared("ALTER TABLE servers DROP COLUMN IF EXISTS last_login_at")
            .await?;
        db.execute_unprepared("ALTER TABLE servers DROP COLUMN IF EXISTS login_obstacle")
            .await?;

        Ok(())
    }
}
