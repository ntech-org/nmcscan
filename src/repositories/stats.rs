use sea_orm::*;
use chrono::Utc;

#[derive(Clone)]
pub struct StatsRepository {
    db: DatabaseConnection,
}

impl StatsRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn refresh_materialized_views(&self) -> Result<(), DbErr> {
        self.db.execute_unprepared("REFRESH MATERIALIZED VIEW CONCURRENTLY asn_stats").await?;
        self.db.execute_unprepared("REFRESH MATERIALIZED VIEW CONCURRENTLY global_stats").await?;
        Ok(())
    }

    pub async fn increment_stats(&self, tier: i32, found_new: bool) -> Result<(), DbErr> {
        let date = Utc::now().date_naive();
        let tier_col = match tier { 1 => "scans_hot", 2 => "scans_warm", _ => "scans_cold" };
        let found_val = if found_new { 1 } else { 0 };
        
        let sql = format!(
            "INSERT INTO daily_stats (date, scans_total, {}, discoveries) \
             VALUES ($1, 1, 1, $2) \
             ON CONFLICT(date) DO UPDATE SET \
             scans_total = daily_stats.scans_total + 1, \
             {} = daily_stats.{} + 1, \
             discoveries = daily_stats.discoveries + $3",
            tier_col, tier_col, tier_col
        );
        
        self.db.execute(Statement::from_sql_and_values(
            self.db.get_database_backend(),
            &sql,
            [date.into(), found_val.into(), found_val.into()]
        )).await?;
        
        Ok(())
    }

    pub async fn increment_batch_stats(&self, hot: i32, warm: i32, cold: i32, discoveries: i32) -> Result<(), DbErr> {
        if hot == 0 && warm == 0 && cold == 0 && discoveries == 0 { return Ok(()); }
        let date = Utc::now().date_naive();
        let total = hot + warm + cold;
        
        let sql = "INSERT INTO daily_stats (date, scans_total, scans_hot, scans_warm, scans_cold, discoveries) \
                   VALUES ($1, $2, $3, $4, $5, $6) \
                   ON CONFLICT(date) DO UPDATE SET \
                   scans_total = daily_stats.scans_total + EXCLUDED.scans_total, \
                   scans_hot = daily_stats.scans_hot + EXCLUDED.scans_hot, \
                   scans_warm = daily_stats.scans_warm + EXCLUDED.scans_warm, \
                   scans_cold = daily_stats.scans_cold + EXCLUDED.scans_cold, \
                   discoveries = daily_stats.discoveries + EXCLUDED.discoveries";
                   
        self.db.execute(Statement::from_sql_and_values(
            self.db.get_database_backend(),
            sql,
            [date.into(), total.into(), hot.into(), warm.into(), cold.into(), discoveries.into()]
        )).await?;
        
        Ok(())
    }

    pub async fn get_global_stats(&self) -> Result<(i64, i64, i64), DbErr> {
        let res = self.db.query_one(Statement::from_string(
            self.db.get_database_backend(),
            "SELECT server_count, online_count, total_players FROM global_stats WHERE id = 1".to_string()
        )).await?;
        
        match res {
            Some(row) => {
                let server_count: i64 = row.try_get("", "server_count")?;
                let online_count: i64 = row.try_get("", "online_count")?;
                let total_players: i64 = row.try_get("", "total_players")?;
                Ok((server_count, online_count, total_players))
            },
            None => Ok((0, 0, 0))
        }
    }
}
