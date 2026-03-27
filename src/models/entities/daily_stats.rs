#![allow(dead_code)]
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "daily_stats")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub date: Date,
    pub scans_total: i32,
    pub scans_hot: i32,
    pub scans_warm: i32,
    pub scans_cold: i32,
    pub discoveries: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
