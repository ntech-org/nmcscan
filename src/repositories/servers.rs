use sea_orm::*;
use sea_orm::sea_query::Expr;
use crate::models::entities::{servers, server_players, server_history};
use chrono::{NaiveDateTime, Utc};

#[derive(Clone)]
pub struct ServerRepository {
    db: DatabaseConnection,
}

impl ServerRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn get_server(&self, ip: &str, port: i32) -> Result<Option<servers::Model>, DbErr> {
        servers::Entity::find_by_id((ip.to_string(), port))
            .one(&self.db)
            .await
    }

    pub async fn upsert_server(&self, model: servers::ActiveModel) -> Result<servers::Model, DbErr> {
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
                    .value(servers::Column::Favicon, Expr::cust("COALESCE(excluded.favicon, servers.favicon)"))
                    .value(servers::Column::Brand, Expr::cust("COALESCE(excluded.brand, servers.brand)"))
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
        port: i32,
        server_type: &str,
        players_online: i32,
        players_max: i32,
        motd: Option<String>,
        version: Option<String>,
        players_sample: Option<Vec<crate::network::slp::PlayerSample>>,
        favicon: Option<String>,
        brand: Option<String>,
        asn: Option<String>,
        country: Option<String>,
    ) -> Result<bool, DbErr> {
        let existing = self.get_server(ip, port).await?;
        let is_new = existing.is_none();

        let txn = self.db.begin().await?;

        let server = servers::ActiveModel {
            ip: Set(ip.to_string()),
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
                    .value(servers::Column::Asn, Expr::cust("COALESCE(servers.asn, excluded.asn)"))
                    .value(servers::Column::Country, Expr::cust("COALESCE(servers.country, excluded.country)"))
                    .value(servers::Column::Favicon, Expr::cust("COALESCE(excluded.favicon, servers.favicon)"))
                    .value(servers::Column::Brand, Expr::cust("COALESCE(excluded.brand, servers.brand)"))
                    .to_owned(),
            )
            .exec(&txn)
            .await?;

        // History
        let history = server_history::ActiveModel {
            ip: Set(ip.to_string()),
            port: Set(port),
            players_online: Set(players_online),
            timestamp: Set(Utc::now().naive_utc()),
            ..Default::default()
        };
        server_history::Entity::insert(history).exec(&txn).await?;

        // Players
        if let Some(sample) = players_sample {
            for player in sample {
                let name = player.name.trim();
                if !name.is_empty() {
                    let p_model = server_players::ActiveModel {
                        ip: Set(ip.to_string()),
                        port: Set(port),
                        player_name: Set(name.to_string()),
                        player_uuid: Set(Some(player.id)),
                        last_seen: Set(Utc::now().naive_utc()),
                        ..Default::default()
                    };
                    server_players::Entity::insert(p_model)
                        .on_conflict(
                            sea_query::OnConflict::columns([server_players::Column::Ip, server_players::Column::Port, server_players::Column::PlayerName])
                                .update_columns([server_players::Column::PlayerUuid, server_players::Column::LastSeen])
                                .to_owned()
                        )
                        .exec(&txn)
                        .await?;
                }
            }
        }

        txn.commit().await?;
        Ok(is_new)
    }

    pub async fn mark_offline(
        &self,
        ip: &str,
        port: i32,
        server_type: &str,
        asn: Option<String>,
        country: Option<String>,
    ) -> Result<(), DbErr> {
        let server = servers::ActiveModel {
            ip: Set(ip.to_string()),
            port: Set(port),
            server_type: Set(server_type.to_string()),
            status: Set("ignored".to_string()),
            priority: Set(3),
            last_seen: Set(Some(Utc::now().naive_utc())),
            consecutive_failures: Set(1),
            asn: Set(asn),
            country: Set(country),
            ..Default::default()
        };

        servers::Entity::insert(server)
            .on_conflict(
                sea_query::OnConflict::columns([servers::Column::Ip, servers::Column::Port])
                    .value(servers::Column::Status, Expr::cust("CASE WHEN servers.motd IS NOT NULL OR servers.status = 'online' THEN 'offline' ELSE 'ignored' END"))
                    .value(servers::Column::ConsecutiveFailures, Expr::cust("servers.consecutive_failures + 1"))
                    .value(servers::Column::LastSeen, Expr::cust("CURRENT_TIMESTAMP"))
                    .value(servers::Column::Priority, Expr::cust("CASE WHEN servers.consecutive_failures >= 5 THEN 3 ELSE servers.priority END"))
                    .value(servers::Column::Asn, Expr::cust("COALESCE(servers.asn, excluded.asn)"))
                    .value(servers::Column::Country, Expr::cust("COALESCE(servers.country, excluded.country)"))
                    .to_owned()
            )
            .exec(&self.db)
            .await?;
        Ok(())
    }

    pub async fn insert_server_if_new(&self, ip: &str, port: i32, server_type: &str) -> Result<(), DbErr> {
        let server = servers::ActiveModel {
            ip: Set(ip.to_string()),
            port: Set(port),
            server_type: Set(server_type.to_string()),
            ..Default::default()
        };
        servers::Entity::insert(server)
            .on_conflict(
                sea_query::OnConflict::columns([servers::Column::Ip, servers::Column::Port])
                    .do_nothing()
                    .to_owned()
            )
            .exec(&self.db)
            .await?;
        Ok(())
    }

    pub async fn batch_update_results(&self, results: Vec<crate::network::ScanResult>) -> Result<(), DbErr> {
        if results.is_empty() {
            return Ok(());
        }

        let txn = self.db.begin().await?;

        for res in results {
            if res.online {
                let server = servers::ActiveModel {
                    ip: Set(res.ip.clone()),
                    port: Set(res.port as i32),
                    server_type: Set(res.server_type.clone()),
                    status: Set("online".to_string()),
                    players_online: Set(res.players_online),
                    players_max: Set(res.players_max),
                    motd: Set(res.motd),
                    version: Set(res.version),
                    favicon: Set(res.favicon),
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
                            .to_owned()
                    )
                    .exec(&txn)
                    .await?;

                // Insert into history
                let history_model = server_history::ActiveModel {
                    ip: Set(res.ip.clone()),
                    port: Set(res.port as i32),
                    timestamp: Set(res.timestamp),
                    players_online: Set(res.players_online),
                    ..Default::default()
                };
                server_history::Entity::insert(history_model).exec(&txn).await?;

                // Update players
                if let Some(samples) = res.players_sample {
                    for p in samples {
                        let p_model = server_players::ActiveModel {
                            ip: Set(res.ip.clone()),
                            port: Set(res.port as i32),
                            player_name: Set(p.name),
                            player_uuid: Set(Some(p.uuid)),
                            last_seen: Set(res.timestamp),
                            ..Default::default()
                        };
                        server_players::Entity::insert(p_model)
                            .on_conflict(
                                sea_query::OnConflict::columns([server_players::Column::Ip, server_players::Column::Port, server_players::Column::PlayerName])
                                    .update_columns([server_players::Column::PlayerUuid, server_players::Column::LastSeen])
                                    .to_owned()
                            )
                            .exec(&txn)
                            .await?;
                    }
                }
            } else {
                let server = servers::ActiveModel {
                    ip: Set(res.ip.clone()),
                    port: Set(res.port as i32),
                    server_type: Set(res.server_type.clone()),
                    status: Set("ignored".to_string()),
                    priority: Set(3),
                    last_seen: Set(Some(res.timestamp)),
                    consecutive_failures: Set(1),
                    asn: Set(res.asn),
                    country: Set(res.country.unwrap_or(None)),
                    ..Default::default()
                };

                servers::Entity::insert(server)
                    .on_conflict(
                        sea_query::OnConflict::columns([servers::Column::Ip, servers::Column::Port])
                            .value(servers::Column::Status, Expr::cust("CASE WHEN servers.motd IS NOT NULL OR servers.status = 'online' THEN 'offline' ELSE 'ignored' END"))
                            .value(servers::Column::ConsecutiveFailures, Expr::cust("servers.consecutive_failures + 1"))
                            .value(servers::Column::LastSeen, res.timestamp)
                            .value(servers::Column::Priority, Expr::cust("CASE WHEN servers.consecutive_failures >= 5 THEN 3 ELSE servers.priority END"))
                            .value(servers::Column::Asn, Expr::cust("COALESCE(servers.asn, excluded.asn)"))
                            .value(servers::Column::Country, Expr::cust("COALESCE(servers.country, excluded.country)"))
                            .to_owned()
                    )
                    .exec(&txn)
                    .await?;
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
    ) -> Result<Vec<servers::Model>, DbErr> {
        let mut query = servers::Entity::find()
            .filter(servers::Column::Status.ne("ignored"));

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
            query = query.filter(
                Condition::any()
                    .add(servers::Column::Ip.contains(search))
                    .add(servers::Column::Motd.contains(search))
                    .add(servers::Column::Version.contains(search))
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
                            .into_query()
                    )
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
        if let Some(c_ip) = cursor_ip {
            match sort_by {
                Some("players") => {
                    if let Some(c_val) = cursor_players {
                        if order == Order::Desc {
                            query = query.filter(
                                Condition::any()
                                    .add(servers::Column::PlayersOnline.lt(c_val))
                                    .add(Condition::all()
                                        .add(servers::Column::PlayersOnline.eq(c_val))
                                        .add(servers::Column::Ip.gt(c_ip)))
                            );
                        } else {
                            query = query.filter(
                                Condition::any()
                                    .add(servers::Column::PlayersOnline.gt(c_val))
                                    .add(Condition::all()
                                        .add(servers::Column::PlayersOnline.eq(c_val))
                                        .add(servers::Column::Ip.gt(c_ip)))
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
                                    .add(Condition::all()
                                        .add(servers::Column::LastSeen.eq(c_val))
                                        .add(servers::Column::Ip.gt(c_ip)))
                            );
                        } else {
                            query = query.filter(
                                Condition::any()
                                    .add(servers::Column::LastSeen.gt(c_val))
                                    .add(Condition::all()
                                        .add(servers::Column::LastSeen.eq(c_val))
                                        .add(servers::Column::Ip.gt(c_ip)))
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

    pub async fn get_server_players(&self, ip: &str, port: i32) -> Result<Vec<server_players::Model>, DbErr> {
        server_players::Entity::find()
            .filter(server_players::Column::Ip.eq(ip))
            .filter(server_players::Column::Port.eq(port))
            .order_by_desc(server_players::Column::LastSeen)
            .limit(100)
            .all(&self.db)
            .await
    }

    pub async fn get_server_history(&self, ip: &str, port: i32, limit: u64) -> Result<Vec<server_history::Model>, DbErr> {
        server_history::Entity::find()
            .filter(server_history::Column::Ip.eq(ip))
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

    pub async fn get_servers_for_refill(&self, priority: i32, interval_hours: i64, limit: u64) -> Result<Vec<servers::Model>, DbErr> {
        let stale_time = Utc::now() - chrono::Duration::hours(interval_hours);
        servers::Entity::find()
            .filter(servers::Column::Priority.eq(priority))
            .filter(servers::Column::Status.ne("ignored"))
            .filter(
                Condition::any()
                    .add(servers::Column::LastSeen.is_null())
                    .add(servers::Column::LastSeen.lt(stale_time))
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
                    .add(servers::Column::Motd.is_not_null())
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
        server_players::Entity::find()
            .filter(server_players::Column::PlayerName.contains(name))
            .order_by_desc(server_players::Column::LastSeen)
            .limit(50)
            .all(&self.db)
            .await
    }

    pub async fn get_existing_ips(&self, ips: Vec<String>) -> Result<std::collections::HashSet<String>, DbErr> {
        let mut known_ips = std::collections::HashSet::new();
        for chunk in ips.chunks(500) {
            let chunk_vec: Vec<String> = chunk.to_vec();
            let found = servers::Entity::find()
                .filter(servers::Column::Ip.is_in(chunk_vec))
                .select_only()
                .column(servers::Column::Ip)
                .all(&self.db)
                .await?;
            for model in found {
                known_ips.insert(model.ip);
            }
        }
        Ok(known_ips)
    }

    pub async fn link_servers_to_asns(&self) -> Result<u64, DbErr> {
        // This one is complex and might be better left as raw SQL if it's a one-off,
        // but let's try to implement the logic from db.rs or use raw SQL.
        // The original logic used a loop over ranges.
        
        // Since this is a specialized maintenance task, I'll use raw SQL for it to keep it simple and efficient.
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
        
        let result = self.db.execute(Statement::from_string(self.db.get_database_backend(), sql.to_string())).await?;
        Ok(result.rows_affected())
    }
}
