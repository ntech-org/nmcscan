pub use sea_orm_migration::prelude::*;

mod m20220101_000001_create_table;
mod m20260328_000002_add_performance_indexes;
mod m20260329_000003_create_api_keys;
mod m20260329_000004_cleanup_ignored_servers;
mod m20260331_000005_add_scan_epoch;
mod m20260402_000006_add_login_and_flags;
mod m20260406_000007_create_minecraft_accounts;
mod m20260406_000008_optimize_database_types;
mod m20260408_000009_add_scaling_indexes;
mod m20260413_000010_add_created_at;
mod m20260413_000011_create_exclusions;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20220101_000001_create_table::Migration),
            Box::new(m20260328_000002_add_performance_indexes::Migration),
            Box::new(m20260329_000003_create_api_keys::Migration),
            Box::new(m20260329_000004_cleanup_ignored_servers::Migration),
            Box::new(m20260331_000005_add_scan_epoch::Migration),
            Box::new(m20260402_000006_add_login_and_flags::Migration),
            Box::new(m20260406_000007_create_minecraft_accounts::Migration),
            Box::new(m20260406_000008_optimize_database_types::Migration),
            Box::new(m20260408_000009_add_scaling_indexes::Migration),
            Box::new(m20260413_000010_add_created_at::Migration),
            Box::new(m20260413_000011_create_exclusions::Migration),
        ]
    }
}
