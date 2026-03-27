use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "asn_ranges")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub cidr: String,
    pub asn: String,
    pub scan_offset: i64,
    pub last_scanned_at: Option<DateTime>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::asns::Entity",
        from = "Column::Asn",
        to = "super::asns::Column::Asn",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Asns,
}

impl Related<super::asns::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Asns.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
