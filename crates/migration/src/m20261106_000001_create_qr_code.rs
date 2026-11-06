use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(QrCode::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(QrCode::Id).uuid().not_null().primary_key())
                    .col(
                        ColumnDef::new(QrCode::Code)
                            .string_len(32)
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(QrCode::Owner).string_len(64).not_null())
                    .col(ColumnDef::new(QrCode::Title).string_len(200).null())
                    .col(
                        ColumnDef::new(QrCode::TargetUrl)
                            .string_len(2048)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(QrCode::Metadata)
                            .json_binary()
                            .not_null()
                            .default(Expr::cust("'{}'::jsonb")),
                    )
                    .col(
                        ColumnDef::new(QrCode::ExpiresAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(QrCode::Scans)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(QrCode::LastScannedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(QrCode::Created)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(QrCode::LastModified)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        // список кодов владельца
        manager
            .create_index(
                Index::create()
                    .name("ix_qr_code_owner_created")
                    .table(QrCode::Table)
                    .col(QrCode::Owner)
                    .col(QrCode::Created)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(QrCode::Table).to_owned())
            .await
    }
}

/// Таблицы и колонки — snake_case, единственное число.
#[derive(DeriveIden)]
enum QrCode {
    Table,
    Id,
    Code,
    Owner,
    Title,
    TargetUrl,
    Metadata,
    ExpiresAt,
    Scans,
    LastScannedAt,
    Created,
    LastModified,
}
