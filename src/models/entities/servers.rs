use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "servers")]
pub struct Model {
    #[sea_orm(primary_key, column_type = "Inet")]
    pub ip: IpNetwork,
    #[sea_orm(primary_key)]
    pub port: i32,
    pub server_type: String,
    pub status: String,
    pub players_online: i32,
    pub players_max: i32,
    pub motd: Option<String>,
    pub version: Option<String>,
    pub priority: i32,
    pub last_seen: Option<DateTime>,
    pub consecutive_failures: i32,
    pub whitelist_prob: f64,
    pub asn: Option<String>,
    pub country: Option<String>,
    pub favicon: Option<String>,
    pub brand: Option<String>,
    pub login_obstacle: Option<String>,
    pub last_login_at: Option<DateTime>,
    pub flags: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::server_players::Entity")]
    ServerPlayers,
    #[sea_orm(has_many = "super::server_history::Entity")]
    ServerHistory,
}

impl Related<super::server_players::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ServerPlayers.def()
    }
}

impl Related<super::server_history::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ServerHistory.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
