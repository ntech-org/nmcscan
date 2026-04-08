use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        // Index for queue refill queries: get_servers_for_refill filters by
        // (priority, status, last_seen) and orders by last_seen ASC.
        // Critical for 500K+ server scale — avoids full table scans on every refill cycle.
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_servers_priority_status_lastseen \
             ON servers(priority, status, last_seen ASC NULLS FIRST)",
        )
        .await?;

        // Index for login queue initial delay check: fetches online servers
        // and checks last_login_at for recent activity.
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_servers_status_last_login \
             ON servers(status, last_login_at ASC NULLS FIRST) \
             WHERE status = 'online'",
        )
        .await?;

        // Index for discovery range queries: get_ranges_to_scan joins
        // asn_ranges → asns and filters by category, then orders by last_scanned_at.
        // The category filter comes from the ASNs table, but indexing asn,
        // last_scanned_at helps the join and sort.
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_asn_ranges_asn_lastscanned \
             ON asn_ranges(asn, last_scanned_at ASC NULLS FIRST)",
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        db.execute_unprepared("DROP INDEX IF EXISTS idx_servers_priority_status_lastseen")
            .await?;
        db.execute_unprepared("DROP INDEX IF EXISTS idx_servers_status_last_login")
            .await?;
        db.execute_unprepared("DROP INDEX IF EXISTS idx_asn_ranges_asn_lastscanned")
            .await?;

        Ok(())
    }
}
