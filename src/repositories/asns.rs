use sea_orm::*;
use sea_orm::sea_query::Expr;
use crate::models::entities::{asns, asn_ranges, asn_stats};
use chrono::Utc;

#[derive(Clone)]
pub struct AsnRepository {
    db: DatabaseConnection,
}

impl AsnRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn upsert_asn(&self, asn: &str, org: &str, category: &str, country: Option<&str>, tags: Option<Vec<String>>) -> Result<(), DbErr> {
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
                    .value(asns::Column::Country, Expr::cust("COALESCE(excluded.country, asns.country)"))
                    .value(asns::Column::Tags, Expr::cust("COALESCE(excluded.tags, asns.tags)"))
                    .value(asns::Column::LastUpdated, Expr::cust("CURRENT_TIMESTAMP"))
                    .to_owned()
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
                    .to_owned()
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
        let stale_time: chrono::DateTime<chrono::FixedOffset> = (Utc::now() - chrono::Duration::days(days)).into();
        asns::Entity::find()
            .filter(
                Condition::any()
                    .add(asns::Column::LastUpdated.is_null())
                    .add(asns::Column::LastUpdated.lt(stale_time))
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

    pub async fn get_asn_list_paginated(&self, page: u64, limit: u64) -> Result<(Vec<asn_stats::Model>, u64), DbErr> {
        let paginator = asn_stats::Entity::find()
            .order_by_desc(asn_stats::Column::ServerCount)
            .order_by_asc(asn_stats::Column::Org)
            .paginate(&self.db, limit);

        let total = paginator.num_items().await?;
        let items = paginator.fetch_page(page).await?;

        Ok((items, total))
    }

    pub async fn get_asn_stats_counts(&self) -> Result<(i64, i64, i64, i64), DbErr> {
        let hosting = asns::Entity::find().filter(asns::Column::Category.eq("hosting")).count(&self.db).await?;
        let residential = asns::Entity::find().filter(asns::Column::Category.eq("residential")).count(&self.db).await?;
        let excluded = asns::Entity::find().filter(asns::Column::Category.eq("excluded")).count(&self.db).await?;
        let unknown = asns::Entity::find().filter(asns::Column::Category.eq("unknown")).count(&self.db).await?;
        
        Ok((hosting as i64, residential as i64, excluded as i64, unknown as i64))
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

    pub async fn get_ranges_to_scan(&self, category: &str, limit: u64) -> Result<Vec<asn_ranges::Model>, DbErr> {
        asn_ranges::Entity::find()
            .join(JoinType::InnerJoin, asn_ranges::Relation::Asns.def())
            .filter(asns::Column::Category.eq(category))
            .order_by_asc(asn_ranges::Column::LastScannedAt)
            .order_by_asc(asn_ranges::Column::ScanOffset)
            .limit(limit)
            .all(&self.db)
            .await
    }

    pub async fn update_range_progress(&self, cidr: &str, new_offset: i64, reset: bool) -> Result<(), DbErr> {
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

    pub async fn update_batch_range_progress(&self, updates: Vec<(String, i64, bool)>) -> Result<(), DbErr> {
        let txn = self.db.begin().await?;
        for (cidr, offset, reset) in updates {
            let sql = if reset {
                "UPDATE asn_ranges SET scan_offset = 0, last_scanned_at = CURRENT_TIMESTAMP WHERE cidr = $1"
            } else {
                "UPDATE asn_ranges SET scan_offset = $1 WHERE cidr = $2"
            };
            
            let stmt = if reset {
                Statement::from_sql_and_values(self.db.get_database_backend(), sql, [cidr.into()])
            } else {
                Statement::from_sql_and_values(self.db.get_database_backend(), sql, [offset.into(), cidr.into()])
            };
            
            txn.execute(stmt).await?;
        }
        txn.commit().await?;
        Ok(())
    }
}
