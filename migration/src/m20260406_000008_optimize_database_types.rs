use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        println!("Starting database optimization migration...");

        // 1. Convert IP columns from TEXT to INET
        println!("Converting IP columns from TEXT to INET...");
        
        // servers.ip
        db.execute_unprepared("
            ALTER TABLE servers 
            ALTER COLUMN ip TYPE INET USING ip::INET
        ").await?;

        // server_players.ip
        db.execute_unprepared("
            ALTER TABLE server_players 
            ALTER COLUMN ip TYPE INET USING ip::INET
        ").await?;

        // server_history.ip
        db.execute_unprepared("
            ALTER TABLE server_history 
            ALTER COLUMN ip TYPE INET USING ip::INET
        ").await?;

        println!("IP columns converted to INET.");

        // 2. Convert port columns from INTEGER to SMALLINT
        println!("Converting port columns from INTEGER to SMALLINT...");
        
        // servers.port
        db.execute_unprepared("
            ALTER TABLE servers 
            ALTER COLUMN port TYPE SMALLINT USING port::SMALLINT
        ").await?;

        // server_players.port
        db.execute_unprepared("
            ALTER TABLE server_players 
            ALTER COLUMN port TYPE SMALLINT USING port::SMALLINT
        ").await?;

        // server_history.port
        db.execute_unprepared("
            ALTER TABLE server_history 
            ALTER COLUMN port TYPE SMALLINT USING port::SMALLINT
        ").await?;

        println!("Port columns converted to SMALLINT.");

        // 3. Add trigram index for player name substring searches
        println!("Creating optimized indexes...");
        
        db.execute_unprepared("
            CREATE INDEX IF NOT EXISTS trgm_idx_server_players_player_name 
            ON server_players USING GIN (player_name gin_trgm_ops)
        ").await?;

        // 4. Add GIN index for ASN tags (improve tag-based filtering)
        db.execute_unprepared("
            CREATE INDEX IF NOT EXISTS idx_asns_tags_gin 
            ON asns USING GIN (to_tsvector('simple', COALESCE(tags, '')))
        ").await?;

        // 5. Add composite index for server flags filtering
        db.execute_unprepared("
            CREATE INDEX IF NOT EXISTS idx_servers_flags_status 
            ON servers (status) WHERE flags IS NOT NULL AND flags != ''
        ").await?;

        println!("Database optimization complete!");
        println!("Storage savings:");
        println!("  - IP columns: ~7-19 bytes per row (INET vs variable TEXT)");
        println!("  - Port columns: 2 bytes saved per row (SMALLINT vs INTEGER)");
        println!("  - Estimated savings: ~10 bytes × millions of rows");
        println!("Performance improvements:");
        println!("  - Native INET type: IP validation + future CIDR containment support");
        println!("  - Player search: Trigram index for substring matching");
        println!("  - ASN tags: Full-text search index on tags");
        println!("  - Flags: Partial index for faster flag-based filtering");

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        println!("Rolling back database optimization...");

        // Revert ports back to INTEGER
        db.execute_unprepared("ALTER TABLE server_history ALTER COLUMN port TYPE INTEGER USING port::INTEGER").await?;
        db.execute_unprepared("ALTER TABLE server_players ALTER COLUMN port TYPE INTEGER USING port::INTEGER").await?;
        db.execute_unprepared("ALTER TABLE servers ALTER COLUMN port TYPE INTEGER USING port::INTEGER").await?;

        // Revert IPs back to TEXT
        db.execute_unprepared("ALTER TABLE server_history ALTER COLUMN ip TYPE TEXT USING ip::TEXT").await?;
        db.execute_unprepared("ALTER TABLE server_players ALTER COLUMN ip TYPE TEXT USING ip::TEXT").await?;
        db.execute_unprepared("ALTER TABLE servers ALTER COLUMN ip TYPE TEXT USING ip::TEXT").await?;

        // Drop new indexes
        db.execute_unprepared("DROP INDEX IF EXISTS trgm_idx_server_players_player_name").await?;
        db.execute_unprepared("DROP INDEX IF EXISTS idx_asns_tags_gin").await?;
        db.execute_unprepared("DROP INDEX IF EXISTS idx_servers_flags_status").await?;

        println!("Database optimization rollback complete.");

        Ok(())
    }
}
