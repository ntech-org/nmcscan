use crate::models::entities::{server_history, server_players, servers};
use chrono::{NaiveDateTime, Utc};
use sea_orm::prelude::IpNetwork;
use sea_orm::sea_query::Expr;
use sea_orm::*;
use std::str::FromStr;

const MAX_HISTORY_ENTRIES: u64 = 500;
const MAX_FAVICON_SIZE: usize = 2048; // Truncate favicons larger than 2KB

/// Parse an IP string into IpNetwork, panicking on invalid IPs (they should already be validated).
fn parse_ip(ip: &str) -> IpNetwork {
    IpNetwork::from_str(ip).unwrap_or_else(|e| panic!("Invalid IP '{}': {}", ip, e))
}

#[derive(Clone)]
pub struct ServerRepository {
    db: DatabaseConnection,
}

impl ServerRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn get_server(&self, ip: &str, port: i16) -> Result<Option<servers::Model>, DbErr> {
        servers::Entity::find_by_id((parse_ip(ip), port))
            .one(&self.db)
            .await
    }

    pub async fn upsert_server(
        &self,
        model: servers::ActiveModel,
    ) -> Result<servers::Model, DbErr> {
        servers::Entity::insert(model)
            .on_conflict(
                sea_query::OnConflict::columns([servers::Column::Ip, servers::Column::Port])
                    .update_columns([
                        servers::Column::ServerType,
                        servers::Column::Status,
                        servers::Column::PlayersOnline,
                        servers::Column::PlayersMax,
                        servers::Column::Motd,
                        servers::Column::Version,
                        servers::Column::Priority,
                        servers::Column::LastSeen,
                        servers::Column::ConsecutiveFailures,
                        servers::Column::WhitelistProb,
                        servers::Column::Asn,
                        servers::Column::Country,
                    ])
                    .value(
                        servers::Column::Favicon,
                        Expr::cust("COALESCE(excluded.favicon, servers.favicon)"),
                    )
                    .value(
                        servers::Column::Brand,
                        Expr::cust("COALESCE(excluded.brand, servers.brand)"),
                    )
                    .to_owned(),
            )
            .exec_with_returning(&self.db)
            .await
    }

    pub async fn get_servers_by_priority(&self, limit: u64) -> Result<Vec<servers::Model>, DbErr> {
        servers::Entity::find()
            .order_by_asc(servers::Column::Priority)
            .order_by_asc(servers::Column::LastSeen)
            .limit(limit)
            .all(&self.db)
            .await
    }

    pub async fn get_online_servers(&self, limit: u64) -> Result<Vec<servers::Model>, DbErr> {
        servers::Entity::find()
            .filter(servers::Column::Status.eq("online"))
            .order_by_desc(servers::Column::PlayersOnline)
            .limit(limit)
            .all(&self.db)
            .await
    }

    pub async fn mark_online(
        &self,
        ip: &str,
        port: i16,
        server_type: &str,
        players_online: i32,
        players_max: i32,
        motd: Option<String>,
        version: Option<String>,
        players_sample: Option<Vec<crate::network::PlayerSample>>,
        favicon: Option<String>,
        brand: Option<String>,
        asn: Option<String>,
        country: Option<String>,
    ) -> Result<bool, DbErr> {
        let existing = self.get_server(ip, port).await?;
        let is_new = existing.is_none();

        let favicon = truncate_favicon(favicon);
        let txn = self.db.begin().await?;

        let server = servers::ActiveModel {
            ip: Set(parse_ip(ip)),
            port: Set(port),
            server_type: Set(server_type.to_string()),
            status: Set("online".to_string()),
            players_online: Set(players_online),
            players_max: Set(players_max),
            motd: Set(motd),
            version: Set(version),
            priority: Set(1),
            last_seen: Set(Some(Utc::now().naive_utc())),
            consecutive_failures: Set(0),
            asn: Set(asn),
            country: Set(country),
            favicon: Set(favicon),
            brand: Set(brand),
            ..Default::default()
        };

        servers::Entity::insert(server)
            .on_conflict(
                sea_query::OnConflict::columns([servers::Column::Ip, servers::Column::Port])
                    .update_columns([
                        servers::Column::ServerType,
                        servers::Column::PlayersOnline,
                        servers::Column::PlayersMax,
                        servers::Column::Motd,
                        servers::Column::Version,
                    ])
                    .value(servers::Column::Status, "online")
                    .value(servers::Column::Priority, 1)
                    .value(servers::Column::LastSeen, Expr::cust("CURRENT_TIMESTAMP"))
                    .value(servers::Column::ConsecutiveFailures, 0)
                    .value(
                        servers::Column::Asn,
                        Expr::cust("COALESCE(servers.asn, excluded.asn)"),
                    )
                    .value(
                        servers::Column::Country,
                        Expr::cust("COALESCE(servers.country, excluded.country)"),
                    )
                    .value(
                        servers::Column::Favicon,
                        Expr::cust("COALESCE(excluded.favicon, servers.favicon)"),
                    )
                    .value(
                        servers::Column::Brand,
                        Expr::cust("COALESCE(excluded.brand, servers.brand)"),
                    )
                    .to_owned(),
            )
            .exec(&txn)
            .await?;

        // History (capped)
        self.insert_history_capped(&txn, ip, port, Utc::now().naive_utc(), players_online)
            .await?;

        // Players
        if let Some(sample) = players_sample {
            for player in sample {
                let name = player.name.trim();
                if !name.is_empty() {
                    let p_model = server_players::ActiveModel {
                        ip: Set(parse_ip(ip)),
                        port: Set(port),
                        player_name: Set(name.to_string()),
                        player_uuid: Set(Some(player.uuid)),
                        last_seen: Set(Utc::now().naive_utc()),
                        ..Default::default()
                    };
                    server_players::Entity::insert(p_model)
                        .on_conflict(
                            sea_query::OnConflict::columns([
                                server_players::Column::Ip,
                                server_players::Column::Port,
                                server_players::Column::PlayerName,
                            ])
                            .update_columns([
                                server_players::Column::PlayerUuid,
                                server_players::Column::LastSeen,
                            ])
                            .to_owned(),
                        )
                        .exec(&txn)
                        .await?;
                }
            }
        }

        txn.commit().await?;
        Ok(is_new)
    }

    /// Offline scans are no longer stored in the database.
    /// Scan tracking is handled by the Redis bitset (ScanTracker).
    /// For previously-known servers that go offline, we update their status in-place.
    pub async fn mark_offline(
        &self,
        ip: &str,
        port: i16,
        _server_type: &str,
        _asn: Option<String>,
        _country: Option<String>,
    ) -> Result<(), DbErr> {
        // Only update existing servers (ones that were previously online).
        // Discovery IPs that fail are NOT inserted into the DB at all.
        let existing = self.get_server(ip, port).await?;
        if let Some(model) = existing {
            if model.status == "online" || model.motd.is_some() {
                let failures = model.consecutive_failures + 1;
                let server = servers::ActiveModel {
                    ip: Set(parse_ip(ip)),
                    port: Set(port),
                    status: Set("offline".to_string()),
                    consecutive_failures: Set(failures),
                    last_seen: Set(Some(Utc::now().naive_utc())),
                    priority: Set(if failures >= 5 { 3 } else { model.priority }),
                    ..Default::default()
                };
                servers::Entity::update(server)
                    .filter(servers::Column::Ip.eq(parse_ip(ip)))
                    .filter(servers::Column::Port.eq(port))
                    .exec(&self.db)
                    .await?;
            }
        }
        Ok(())
    }

    pub async fn insert_server_if_new(
        &self,
        ip: &str,
        port: i16,
        server_type: &str,
    ) -> Result<(), DbErr> {
        let server = servers::ActiveModel {
            ip: Set(parse_ip(ip)),
            port: Set(port),
            server_type: Set(server_type.to_string()),
            ..Default::default()
        };
        servers::Entity::insert(server)
            .on_conflict(
                sea_query::OnConflict::columns([servers::Column::Ip, servers::Column::Port])
                    .do_nothing()
                    .to_owned(),
            )
            .exec(&self.db)
            .await?;
        Ok(())
    }

    pub async fn batch_update_results(
        &self,
        results: Vec<crate::network::ScanResult>,
    ) -> Result<(), DbErr> {
        if results.is_empty() {
            return Ok(());
        }

        let txn = self.db.begin().await?;

        for res in results {
            if !res.online {
                // Skip offline results entirely - they are tracked in Redis bitset only.
                // Only update existing servers that were previously online.
                let port_i16: i16 = res.port.try_into().unwrap_or(25565);
                let existing = servers::Entity::find_by_id((parse_ip(&res.ip), res.port as i16))
                    .one(&txn)
                    .await?;
                if let Some(model) = existing {
                    if model.status == "online" || model.motd.is_some() {
                        let failures = model.consecutive_failures + 1;
                        let mut am: servers::ActiveModel = model.into();
                        am.status = Set("offline".to_string());
                        am.consecutive_failures = Set(failures);
                        am.last_seen = Set(Some(res.timestamp));
                        am.priority = Set(if failures >= 5 {
                            3
                        } else {
                            am.priority.unwrap()
                        });
                        am.update(&txn).await?;
                    }
                }
                continue;
            }

            // Online server - store/update in database
            let favicon = truncate_favicon(res.favicon);
            let server = servers::ActiveModel {
                ip: Set(parse_ip(&res.ip)),
                port: Set(res.port as i16),
                server_type: Set(res.server_type.clone()),
                status: Set("online".to_string()),
                players_online: Set(res.players_online),
                players_max: Set(res.players_max),
                motd: Set(res.motd),
                version: Set(res.version),
                favicon: Set(favicon),
                brand: Set(res.brand),
                priority: Set(1),
                last_seen: Set(Some(res.timestamp)),
                consecutive_failures: Set(0),
                asn: Set(res.asn),
                country: Set(res.country.unwrap_or(None)),
                ..Default::default()
            };

            servers::Entity::insert(server)
                .on_conflict(
                    sea_query::OnConflict::columns([servers::Column::Ip, servers::Column::Port])
                        .update_columns([
                            servers::Column::Status,
                            servers::Column::PlayersOnline,
                            servers::Column::PlayersMax,
                            servers::Column::Motd,
                            servers::Column::Version,
                            servers::Column::Favicon,
                            servers::Column::Brand,
                            servers::Column::LastSeen,
                            servers::Column::Priority,
                            servers::Column::ConsecutiveFailures,
                            servers::Column::Asn,
                            servers::Column::Country,
                        ])
                        .to_owned(),
                )
                .exec(&txn)
                .await?;

            // Insert into history (capped to prevent unbounded growth)
            self.insert_history_capped(
                &txn,
                &res.ip,
                res.port.try_into().unwrap(),
                res.timestamp,
                res.players_online,
            )
            .await?;

            // Update players
            if let Some(samples) = res.players_sample {
                for p in samples {
                    let p_model = server_players::ActiveModel {
                        ip: Set(parse_ip(&res.ip)),
                        port: Set(res.port as i16),
                        player_name: Set(p.name),
                        player_uuid: Set(Some(p.uuid)),
                        last_seen: Set(res.timestamp),
                        ..Default::default()
                    };
                    server_players::Entity::insert(p_model)
                        .on_conflict(
                            sea_query::OnConflict::columns([
                                server_players::Column::Ip,
                                server_players::Column::Port,
                                server_players::Column::PlayerName,
                            ])
                            .update_columns([
                                server_players::Column::PlayerUuid,
                                server_players::Column::LastSeen,
                            ])
                            .to_owned(),
                        )
                        .exec(&txn)
                        .await?;
                }
            }
        }

        txn.commit().await?;
        Ok(())
    }

    pub async fn get_all_servers(
        &self,
        status_filter: Option<&str>,
        search_query: Option<&str>,
        limit: u64,
        min_players: Option<i32>,
        max_players: Option<i32>,
        version: Option<&str>,
        asn_category: Option<&str>,
        whitelist_prob_min: Option<f64>,
        country: Option<&str>,
        brand: Option<&str>,
        server_type_filter: Option<&str>,
        sort_by: Option<&str>,
        sort_order: Option<&str>,
        cursor_players: Option<i32>,
        cursor_ip: Option<&str>,
        cursor_last_seen: Option<NaiveDateTime>,
        asn_filter: Option<&str>,
        min_max_players: Option<i32>,
        max_max_players: Option<i32>,
        flags_filter: Vec<String>,
        login_obstacle_filter: Option<&str>,
    ) -> Result<Vec<servers::Model>, DbErr> {
        let mut query = servers::Entity::find().filter(servers::Column::Status.ne("ignored"));

        if let Some(status) = status_filter {
            if status != "all" {
                query = query.filter(servers::Column::Status.eq(status));
            }
        }

        if let Some(st) = server_type_filter {
            if st != "all" {
                query = query.filter(servers::Column::ServerType.eq(st));
            }
        }

        if let Some(search) = search_query {
            let pattern = format!("%{}%", search.replace('\'', "''"));
            query = query.filter(
                Condition::any()
                    .add(Expr::cust(format!("ip::text ILIKE '{}'", pattern)))
                    .add(Expr::cust(format!("motd ILIKE '{}'", pattern)))
                    .add(Expr::cust(format!("version ILIKE '{}'", pattern))),
            );
        }

        if let Some(min_p) = min_players {
            query = query.filter(servers::Column::PlayersOnline.gte(min_p));
        }

        if let Some(max_p) = max_players {
            query = query.filter(servers::Column::PlayersOnline.lte(max_p));
        }

        if let Some(min_mp) = min_max_players {
            query = query.filter(servers::Column::PlayersMax.gte(min_mp));
        }

        if let Some(max_mp) = max_max_players {
            query = query.filter(servers::Column::PlayersMax.lte(max_mp));
        }

        if let Some(ver) = version {
            query = query.filter(servers::Column::Version.contains(ver));
        }

        if let Some(prob) = whitelist_prob_min {
            query = query.filter(servers::Column::WhitelistProb.gte(prob));
        }

        if let Some(cat) = asn_category {
            if cat != "all" {
                query = query.filter(
                    servers::Column::Asn.in_subquery(
                        crate::models::entities::asns::Entity::find()
                            .select_only()
                            .column(crate::models::entities::asns::Column::Asn)
                            .filter(crate::models::entities::asns::Column::Category.eq(cat))
                            .into_query(),
                    ),
                );
            }
        }

        if let Some(asn) = asn_filter {
            query = query.filter(servers::Column::Asn.eq(asn));
        }

        if let Some(c) = country {
            query = query.filter(servers::Column::Country.eq(c));
        }

        if let Some(b) = brand {
            query = query.filter(servers::Column::Brand.contains(b));
        }

        if let Some(lo) = login_obstacle_filter {
            query = query.filter(servers::Column::LoginObstacle.eq(lo));
        }

        // Flags filter
        for flag in flags_filter {
            let flag = flag.trim();
            if flag.is_empty() {
                continue;
            }
            let pattern = format!("%,{},%", flag);
            let pattern_start = format!("{},%", flag);
            let pattern_end = format!("%,{}", flag);
            query = query.filter(
                Condition::any()
                    .add(servers::Column::Flags.like(&pattern))
                    .add(servers::Column::Flags.like(&pattern_start))
                    .add(servers::Column::Flags.like(&pattern_end))
                    .add(servers::Column::Flags.eq(flag)),
            );
        }

        let order = match sort_order {
            Some("asc") => Order::Asc,
            _ => Order::Desc,
        };

        let sort_col = match sort_by {
            Some("players") => servers::Column::PlayersOnline,
            Some("last_seen") => servers::Column::LastSeen,
            Some("ip") => servers::Column::Ip,
            _ => servers::Column::PlayersOnline,
        };

        // Cursor-based pagination
        if let Some(c_ip_str) = cursor_ip {
            let c_ip = parse_ip(c_ip_str);
            match sort_by {
                Some("players") => {
                    if let Some(c_val) = cursor_players {
                        if order == Order::Desc {
                            query = query.filter(
                                Condition::any()
                                    .add(servers::Column::PlayersOnline.lt(c_val))
                                    .add(
                                        Condition::all()
                                            .add(servers::Column::PlayersOnline.eq(c_val))
                                            .add(servers::Column::Ip.gt(c_ip)),
                                    ),
                            );
                        } else {
                            query = query.filter(
                                Condition::any()
                                    .add(servers::Column::PlayersOnline.gt(c_val))
                                    .add(
                                        Condition::all()
                                            .add(servers::Column::PlayersOnline.eq(c_val))
                                            .add(servers::Column::Ip.gt(c_ip)),
                                    ),
                            );
                        }
                    }
                }
                Some("last_seen") => {
                    if let Some(c_val) = cursor_last_seen {
                        if order == Order::Desc {
                            query = query.filter(
                                Condition::any()
                                    .add(servers::Column::LastSeen.lt(c_val))
                                    .add(
                                        Condition::all()
                                            .add(servers::Column::LastSeen.eq(c_val))
                                            .add(servers::Column::Ip.gt(c_ip)),
                                    ),
                            );
                        } else {
                            query = query.filter(
                                Condition::any()
                                    .add(servers::Column::LastSeen.gt(c_val))
                                    .add(
                                        Condition::all()
                                            .add(servers::Column::LastSeen.eq(c_val))
                                            .add(servers::Column::Ip.gt(c_ip)),
                                    ),
                            );
                        }
                    }
                }
                Some("ip") => {
                    if order == Order::Desc {
                        query = query.filter(servers::Column::Ip.lt(c_ip));
                    } else {
                        query = query.filter(servers::Column::Ip.gt(c_ip));
                    }
                }
                _ => {}
            }
        }

        query
            .order_by(sort_col, order)
            .order_by_asc(servers::Column::Ip)
            .limit(limit)
            .all(&self.db)
            .await
    }

    pub async fn get_server_players(
        &self,
        ip: &str,
        port: i16,
    ) -> Result<Vec<server_players::Model>, DbErr> {
        server_players::Entity::find()
            .filter(server_players::Column::Ip.eq(parse_ip(ip)))
            .filter(server_players::Column::Port.eq(port))
            .order_by_desc(server_players::Column::LastSeen)
            .limit(100)
            .all(&self.db)
            .await
    }

    pub async fn get_server_history(
        &self,
        ip: &str,
        port: i16,
        limit: u64,
    ) -> Result<Vec<server_history::Model>, DbErr> {
        server_history::Entity::find()
            .filter(server_history::Column::Ip.eq(parse_ip(ip)))
            .filter(server_history::Column::Port.eq(port))
            .order_by_desc(server_history::Column::Timestamp)
            .limit(limit)
            .all(&self.db)
            .await
            .map(|mut v| {
                v.reverse(); // Chronological order
                v
            })
    }

    pub async fn get_servers_for_refill(
        &self,
        priority: i32,
        interval_hours: i64,
        limit: u64,
    ) -> Result<Vec<servers::Model>, DbErr> {
        let stale_time = Utc::now() - chrono::Duration::hours(interval_hours);
        servers::Entity::find()
            .filter(servers::Column::Priority.eq(priority))
            .filter(servers::Column::Status.ne("ignored"))
            .filter(
                Condition::any()
                    .add(servers::Column::LastSeen.is_null())
                    .add(servers::Column::LastSeen.lt(stale_time)),
            )
            .order_by_asc(servers::Column::LastSeen)
            .limit(limit)
            .all(&self.db)
            .await
    }

    pub async fn get_servers_for_load(&self, limit: u64) -> Result<Vec<servers::Model>, DbErr> {
        servers::Entity::find()
            .filter(servers::Column::Status.ne("unknown"))
            .filter(
                Condition::any()
                    .add(servers::Column::Status.eq("online"))
                    .add(servers::Column::Motd.is_not_null()),
            )
            .order_by_asc(servers::Column::Priority)
            .order_by_asc(servers::Column::LastSeen)
            .limit(limit)
            .all(&self.db)
            .await
    }

    pub async fn get_dead_servers(&self, limit: u64) -> Result<Vec<servers::Model>, DbErr> {
        servers::Entity::find()
            .filter(servers::Column::Priority.eq(3))
            .order_by_asc(servers::Column::LastSeen)
            .limit(limit)
            .all(&self.db)
            .await
    }

    pub async fn search_players(&self, name: &str) -> Result<Vec<server_players::Model>, DbErr> {
        let pattern = format!("%{}%", name.replace('\'', "''"));
        server_players::Entity::find()
            .filter(Expr::cust(format!("player_name ILIKE '{}'", pattern)))
            .order_by_desc(server_players::Column::LastSeen)
            .limit(50)
            .all(&self.db)
            .await
    }

    pub async fn get_existing_ips(
        &self,
        _ips: Vec<String>,
    ) -> Result<std::collections::HashSet<String>, DbErr> {
        // Deprecated: Scan tracking moved to Redis bitset (ScanTracker).
        // This method is kept as a no-op for backwards compatibility.
        Ok(std::collections::HashSet::new())
    }

    pub async fn link_servers_to_asns(&self) -> Result<u64, DbErr> {
        let sql = r#"
            UPDATE servers 
            SET asn = r.asn, 
                country = COALESCE(servers.country, a.country)
            FROM asn_ranges r
            JOIN asns a ON r.asn = a.asn
            WHERE servers.asn IS NULL 
            AND (
                (r.cidr LIKE '%/32' AND servers.ip = REPLACE(r.cidr, '/32', '')) OR
                (r.cidr LIKE '%/24' AND servers.ip LIKE REPLACE(r.cidr, '/24', '.%')) OR
                (r.cidr LIKE '%/16' AND servers.ip LIKE REPLACE(r.cidr, '/16', '.%'))
            )
        "#;

        let result = self
            .db
            .execute(Statement::from_string(
                self.db.get_database_backend(),
                sql.to_string(),
            ))
            .await?;
        Ok(result.rows_affected())
    }

    /// Insert history entry but cap total entries per server to prevent unbounded growth.
    async fn insert_history_capped(
        &self,
        txn: &DatabaseTransaction,
        ip: &str,
        port: i16,
        timestamp: chrono::NaiveDateTime,
        players_online: i32,
    ) -> Result<(), DbErr> {
        // Check current count
        let count = server_history::Entity::find()
            .filter(server_history::Column::Ip.eq(parse_ip(ip)))
            .filter(server_history::Column::Port.eq(port))
            .count(txn)
            .await?;

        // If at cap, delete oldest entries
        if count >= MAX_HISTORY_ENTRIES {
            let excess = count - MAX_HISTORY_ENTRIES + 1;
            let oldest: Vec<server_history::Model> = server_history::Entity::find()
                .filter(server_history::Column::Ip.eq(parse_ip(ip)))
                .filter(server_history::Column::Port.eq(port))
                .order_by_asc(server_history::Column::Timestamp)
                .limit(excess)
                .all(txn)
                .await?;

            for entry in oldest {
                server_history::Entity::delete_by_id((entry.ip, entry.port, entry.timestamp))
                    .exec(txn)
                    .await?;
            }
        }

        let history = server_history::ActiveModel {
            ip: Set(parse_ip(ip)),
            port: Set(port),
            timestamp: Set(timestamp),
            players_online: Set(players_online),
            ..Default::default()
        };
        server_history::Entity::insert(history).exec(txn).await?;
        Ok(())
    }

    /// Delete all servers with status 'ignored' to reclaim storage.
    /// Call this once after migrating to Redis bitset tracking.
    pub async fn purge_ignored_servers(&self) -> Result<u64, DbErr> {
        let result = servers::Entity::delete_many()
            .filter(servers::Column::Status.eq("ignored"))
            .exec(&self.db)
            .await?;
        Ok(result.rows_affected)
    }

    /// Update login obstacle result for a server.
    /// The PostgreSQL trigger will automatically recompute flags.
    pub async fn update_login_result(
        &self,
        ip: &str,
        port: i16,
        obstacle: &str,
    ) -> Result<(), DbErr> {
        let server = servers::ActiveModel {
            ip: Set(parse_ip(ip)),
            port: Set(port),
            login_obstacle: Set(Some(obstacle.to_string())),
            last_login_at: Set(Some(Utc::now().naive_utc())),
            ..Default::default()
        };
        servers::Entity::update(server)
            .filter(servers::Column::Ip.eq(parse_ip(ip)))
            .filter(servers::Column::Port.eq(port))
            .exec(&self.db)
            .await?;
        Ok(())
    }

    /// Get servers filtered by a flag (e.g., "cracked", "vanilla", "active").
    /// Uses LIKE with comma delimiters for reliable matching.
    pub async fn get_servers_by_flag(
        &self,
        flag: &str,
        limit: u64,
    ) -> Result<Vec<servers::Model>, DbErr> {
        // Match flag surrounded by commas or at start/end of string
        let pattern = format!("%,{},%", flag);
        let pattern_start = format!("{},%", flag);
        let pattern_end = format!("%,{}", flag);

        servers::Entity::find()
            .filter(
                Condition::any()
                    .add(servers::Column::Flags.like(&pattern))
                    .add(servers::Column::Flags.like(&pattern_start))
                    .add(servers::Column::Flags.like(&pattern_end))
                    .add(servers::Column::Flags.eq(flag)),
            )
            .filter(servers::Column::Status.eq("online"))
            .order_by_desc(servers::Column::PlayersOnline)
            .limit(limit)
            .all(&self.db)
            .await
    }
}

/// Truncate oversized favicon data to save storage.
/// Favicons are base64-encoded PNGs that can be 5-10KB each.
fn truncate_favicon(favicon: Option<String>) -> Option<String> {
    let f = favicon?;
    if f.len() > MAX_FAVICON_SIZE {
        // Keep just the first portion to avoid massive storage bloat
        // but preserve enough for a recognizable icon
        None // Drop oversized favicons entirely
    } else {
        Some(f)
    }
}
