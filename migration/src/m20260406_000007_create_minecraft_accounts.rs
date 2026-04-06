use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(MinecraftAccounts::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(MinecraftAccounts::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(MinecraftAccounts::Email)
                            .string()
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(MinecraftAccounts::Password).string())
                    .col(ColumnDef::new(MinecraftAccounts::AccessToken).string())
                    .col(ColumnDef::new(MinecraftAccounts::RefreshToken).string())
                    .col(ColumnDef::new(MinecraftAccounts::ExpiresAt).timestamp())
                    .col(
                        ColumnDef::new(MinecraftAccounts::Status)
                            .string()
                            .not_null()
                            .default("active"),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(MinecraftAccounts::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum MinecraftAccounts {
    Table,
    Id,
    Email,
    Password,
    AccessToken,
    RefreshToken,
    ExpiresAt,
    Status,
}
