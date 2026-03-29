pub use sea_orm_migration::prelude::*;

mod m20220101_000001_create_table;
mod m20260328_000002_add_performance_indexes;
mod m20260329_000003_create_api_keys;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20220101_000001_create_table::Migration),
            Box::new(m20260328_000002_add_performance_indexes::Migration),
            Box::new(m20260329_000003_create_api_keys::Migration),
        ]
    }
}
