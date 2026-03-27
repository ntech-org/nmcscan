use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "server_history")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub ip: String,
    #[sea_orm(primary_key)]
    pub port: i32,
    #[sea_orm(primary_key)]
    pub timestamp: DateTime,
    pub players_online: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::servers::Entity",
        from = "(Column::Ip, Column::Port)",
        to = "(super::servers::Column::Ip, super::servers::Column::Port)",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Servers,
}

impl Related<super::servers::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Servers.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
