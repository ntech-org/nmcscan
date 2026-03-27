#![allow(dead_code)]
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "verification_token")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub identifier: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub token: String,
    pub expires: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
