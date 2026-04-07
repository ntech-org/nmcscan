use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "asns")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub asn: String,
    pub org: String,
    pub category: String,
    pub country: Option<String>,
    pub tags: Option<String>,
    pub last_updated: Option<DateTimeWithTimeZone>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::asn_ranges::Entity")]
    AsnRanges,
}

impl Related<super::asn_ranges::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::AsnRanges.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
