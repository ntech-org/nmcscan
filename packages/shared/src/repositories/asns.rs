use crate::models::entities::{asn_ranges, asn_stats, asns};
use chrono::Utc;
use sea_orm::sea_query::Expr;
use sea_orm::*;

#[derive(Clone)]
pub struct AsnRepository {
    db: DatabaseConnection,
}

impl AsnRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn upsert_asn(
        &self,
        asn: &str,
        org: &str,
        category: &str,
        country: Option<&str>,
        tags: Option<Vec<String>>,
    ) -> Result<(), DbErr> {
        let tags_str = tags.map(|t| t.join(","));
        let model = asns::ActiveModel {
            asn: Set(asn.to_string()),
            org: Set(org.to_string()),
            category: Set(category.to_string()),
            country: Set(country.map(|s| s.to_string())),
            tags: Set(tags_str),
            last_updated: Set(Some(Utc::now().into())),
        };

        asns::Entity::insert(model)
            .on_conflict(
                sea_query::OnConflict::column(asns::Column::Asn)
                    .update_columns([asns::Column::Org, asns::Column::Category])
                    .value(
                        asns::Column::Country,
                        Expr::cust("COALESCE(excluded.country, asns.country)"),
                    )
                    .value(
                        asns::Column::Tags,
                        Expr::cust("COALESCE(excluded.tags, asns.tags)"),
                    )
                    .value(asns::Column::LastUpdated, Expr::cust("CURRENT_TIMESTAMP"))
                    .to_owned(),
            )
            .exec(&self.db)
            .await?;
        Ok(())
    }

    pub async fn upsert_asn_range(&self, cidr: &str, asn: &str) -> Result<(), DbErr> {
        let model = asn_ranges::ActiveModel {
            cidr: Set(cidr.to_string()),
            asn: Set(asn.to_string()),
            ..Default::default()
        };

        asn_ranges::Entity::insert(model)
            .on_conflict(
                sea_query::OnConflict::column(asn_ranges::Column::Cidr)
                    .update_column(asn_ranges::Column::Asn)
                    .to_owned(),
            )
            .exec(&self.db)
            .await?;
        Ok(())
    }

    pub async fn get_all_asns(&self) -> Result<Vec<asns::Model>, DbErr> {
        asns::Entity::find().all(&self.db).await
    }

    pub async fn get_all_asn_ranges(&self) -> Result<Vec<asn_ranges::Model>, DbErr> {
        asn_ranges::Entity::find().all(&self.db).await
    }

    pub async fn get_asns_by_category(&self, category: &str) -> Result<Vec<asns::Model>, DbErr> {
        asns::Entity::find()
            .filter(asns::Column::Category.eq(category))
            .order_by_asc(asns::Column::Org)
            .all(&self.db)
            .await
    }

    pub async fn get_stale_asns(&self, days: i64) -> Result<Vec<asns::Model>, DbErr> {
        let stale_time: chrono::DateTime<chrono::FixedOffset> =
            (Utc::now() - chrono::Duration::days(days)).into();
        asns::Entity::find()
            .filter(
                Condition::any()
                    .add(asns::Column::LastUpdated.is_null())
                    .add(asns::Column::LastUpdated.lt(stale_time)),
            )
            .order_by_asc(asns::Column::LastUpdated)
            .all(&self.db)
            .await
    }

    pub async fn get_asn_list_with_counts(&self) -> Result<Vec<asn_stats::Model>, DbErr> {
        asn_stats::Entity::find()
            .order_by_desc(asn_stats::Column::ServerCount)
            .order_by_asc(asn_stats::Column::Org)
            .all(&self.db)
            .await
    }

    pub async fn get_asn_list_paginated(
        &self,
        page: u64,
        limit: u64,
    ) -> Result<(Vec<asn_stats::Model>, u64), DbErr> {
        let paginator = asn_stats::Entity::find()
            .order_by_desc(asn_stats::Column::ServerCount)
            .order_by_asc(asn_stats::Column::Org)
            .paginate(&self.db, limit);

        let total = paginator.num_items().await?;
        let items = paginator.fetch_page(page).await?;

        Ok((items, total))
    }

    pub async fn get_asn_stats_counts(&self) -> Result<(i64, i64, i64, i64), DbErr> {
        let hosting = asns::Entity::find()
            .filter(asns::Column::Category.eq("hosting"))
            .count(&self.db)
            .await?;
        let residential = asns::Entity::find()
            .filter(asns::Column::Category.eq("residential"))
            .count(&self.db)
            .await?;
        let excluded = asns::Entity::find()
            .filter(asns::Column::Category.eq("excluded"))
            .count(&self.db)
            .await?;
        let unknown = asns::Entity::find()
            .filter(asns::Column::Category.eq("unknown"))
            .count(&self.db)
            .await?;

        Ok((
            hosting as i64,
            residential as i64,
            excluded as i64,
            unknown as i64,
        ))
    }

    pub async fn get_asn_count(&self) -> Result<u64, DbErr> {
        asns::Entity::find().count(&self.db).await
    }

    pub async fn get_hosting_ranges(&self) -> Result<Vec<(String, String)>, DbErr> {
        // Find ranges where the linked ASN has category 'hosting'
        let ranges = asn_ranges::Entity::find()
            .join(JoinType::InnerJoin, asn_ranges::Relation::Asns.def())
            .filter(asns::Column::Category.eq("hosting"))
            .order_by_asc(asns::Column::Org)
            .all(&self.db)
            .await?;

        Ok(ranges.into_iter().map(|r| (r.cidr, r.asn)).collect())
    }

    pub async fn get_ranges_to_scan(
        &self,
        category: &str,
        limit: u64,
    ) -> Result<Vec<asn_ranges::Model>, DbErr> {
        // Select ranges with fair round-robin scheduling, then randomize selection.
        //
        // Strategy:
        // 1. Inner query selects a pool (10x limit) ordered by last scanned time.
        //    This ensures ALL ranges eventually get picked without starvation.
        // 2. Outer query randomly picks from that pool.
        // 3. CRITICAL: Epoch cooldown filter prevents ranges from being rescanned
        //    too frequently. Hosting: 12h min (2x/day), Residential: 56h min (3x/week).
        //
        // This means the scanner progresses through the entire category evenly,
        // preventing large ASNs from monopolizing the queue AND preventing the same
        // range from being rescanned immediately after epoch reset.
        let pool_size = limit * 10;

        // Epoch cooldown: prevent ranges from being picked again too soon
        let min_hours = if category == "hosting" { 12 } else { 56 };

        let sql = format!(
            r#"
            SELECT cidr, asn, scan_offset, last_scanned_at, scan_epoch FROM (
                SELECT r.cidr, r.asn, r.scan_offset, r.last_scanned_at, r.scan_epoch
                FROM asn_ranges r
                JOIN asns a ON r.asn = a.asn
                WHERE a.category = '{}'
                  AND (r.last_scanned_at IS NULL 
                       OR r.last_scanned_at < NOW() - INTERVAL '{} HOURS')
                ORDER BY
                    r.last_scanned_at ASC NULLS FIRST,
                    r.scan_offset ASC
                LIMIT {}
            ) pool
            ORDER BY random()
            LIMIT {}
        "#,
            category, min_hours, pool_size, limit
        );

        let stmt = Statement::from_string(self.db.get_database_backend(), sql);
        let rows = self.db.query_all(stmt).await?;

        use crate::models::entities::asn_ranges::Model as AsnRangeModel;
        let models: Vec<AsnRangeModel> = rows
            .into_iter()
            .map(|row| AsnRangeModel {
                cidr: row.try_get("", "cidr").unwrap_or_default(),
                asn: row.try_get("", "asn").unwrap_or_default(),
                scan_offset: row.try_get("", "scan_offset").unwrap_or(0),
                last_scanned_at: row.try_get("", "last_scanned_at").ok(),
                scan_epoch: row.try_get("", "scan_epoch").unwrap_or(0),
            })
            .collect();

        Ok(models)
    }

    pub async fn update_range_progress(
        &self,
        cidr: &str,
        new_offset: i64,
        reset: bool,
    ) -> Result<(), DbErr> {
        let mut model: asn_ranges::ActiveModel = asn_ranges::Entity::find_by_id(cidr.to_string())
            .one(&self.db)
            .await?
            .ok_or(DbErr::RecordNotFound(format!("Range not found: {}", cidr)))?
            .into();

        if reset {
            model.scan_offset = Set(0);
            model.last_scanned_at = Set(Some(Utc::now().naive_utc()));
        } else {
            model.scan_offset = Set(new_offset);
        }

        model.update(&self.db).await?;
        Ok(())
    }

    pub async fn update_batch_range_progress(
        &self,
        updates: Vec<(String, i64, bool, bool)>,
    ) -> Result<(), DbErr> {
        if updates.is_empty() {
            return Ok(());
        }

        // Single bulk UPDATE using PostgreSQL VALUES / unnest to avoid per-row transactions.
        // Uses CASE expressions to handle the three different update patterns:
        // 1. reset + bump_epoch: offset=0, last_scanned_at=now, epoch+1
        // 2. reset only: offset=0, last_scanned_at=now
        // 3. normal: offset=new_value
        let sql = r#"
            UPDATE asn_ranges
            SET
                scan_offset = CASE WHEN v.reset THEN 0 ELSE v.offset END,
                last_scanned_at = CASE WHEN v.reset THEN CURRENT_TIMESTAMP ELSE asn_ranges.last_scanned_at END,
                scan_epoch = CASE WHEN v.reset AND v.bump_epoch THEN asn_ranges.scan_epoch + 1 ELSE asn_ranges.scan_epoch END
            FROM (
                SELECT unnest($1::text[]) AS cidr,
                       unnest($2::bigint[]) AS offset,
                       unnest($3::boolean[]) AS reset,
                       unnest($4::boolean[]) AS bump_epoch
            ) v
            WHERE asn_ranges.cidr = v.cidr
        "#;

        let cidrs: Vec<String> = updates.iter().map(|(c, _, _, _)| c.clone()).collect();
        let offsets: Vec<i64> = updates.iter().map(|(_, o, _, _)| *o).collect();
        let resets: Vec<bool> = updates.iter().map(|(_, _, r, _)| *r).collect();
        let bumps: Vec<bool> = updates.iter().map(|(_, _, _, b)| *b).collect();

        self.db
            .execute(Statement::from_sql_and_values(
                self.db.get_database_backend(),
                sql,
                [cidrs.into(), offsets.into(), resets.into(), bumps.into()],
            ))
            .await?;

        Ok(())
    }

    /// Get scan progress per category: total ranges, scanned ranges, total epochs.
    pub async fn get_scan_progress(&self) -> Result<Vec<CategoryProgress>, DbErr> {
        let sql = r#"
            SELECT a.category,
                   COUNT(r.cidr)::bigint as total_ranges,
                   SUM(CASE WHEN r.scan_offset > 0 THEN 1 ELSE 0 END)::bigint as scanned_ranges,
                   SUM(r.scan_epoch)::bigint as total_epochs
            FROM asn_ranges r
            JOIN asns a ON r.asn = a.asn
            GROUP BY a.category
            ORDER BY a.category
        "#;

        let stmt = Statement::from_string(self.db.get_database_backend(), sql.to_string());
        let rows = self.db.query_all(stmt).await?;

        let progress: Vec<CategoryProgress> = rows
            .into_iter()
            .map(|row| {
                let total: i64 = row.try_get("", "total_ranges").unwrap_or(0);
                let scanned: i64 = row.try_get("", "scanned_ranges").unwrap_or(0);
                CategoryProgress {
                    category: row.try_get("", "category").unwrap_or_default(),
                    total_ranges: total,
                    scanned_ranges: scanned,
                    total_epochs: row.try_get("", "total_epochs").unwrap_or(0),
                    cycle_progress_pct: if total > 0 {
                        (scanned as f64 / total as f64) * 100.0
                    } else {
                        0.0
                    },
                }
            })
            .collect();

        Ok(progress)
    }
}

/// Per-category scan progress.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CategoryProgress {
    pub category: String,
    pub total_ranges: i64,
    pub scanned_ranges: i64,
    pub total_epochs: i64,
    pub cycle_progress_pct: f64,
}
