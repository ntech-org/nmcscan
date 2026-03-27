use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "asn_stats")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub asn: String,
    pub org: String,
    pub category: String,
    pub country: Option<String>,
    pub tags: Option<String>,
    pub last_updated: Option<DateTimeWithTimeZone>,
    pub server_count: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
