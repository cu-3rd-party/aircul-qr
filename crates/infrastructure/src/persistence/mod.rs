pub mod entities;
pub mod repositories;
pub mod unit_of_work;

use std::time::Duration;

use sea_orm::{ConnectOptions, Database, DatabaseConnection, DbErr};
use template_migration::{Migrator, MigratorTrait};

pub async fn connect(database_url: &str) -> Result<DatabaseConnection, DbErr> {
    let mut options = ConnectOptions::new(database_url);
    options
        .max_connections(20)
        .connect_timeout(Duration::from_secs(5))
        .sqlx_logging_level(tracing::log::LevelFilter::Debug);
    Database::connect(options).await
}

/// Применяет миграции.
pub async fn initialise(db: &DatabaseConnection) -> Result<(), DbErr> {
    migrate(db).await.inspect_err(|error| {
        tracing::error!(%error, "An error occurred while migrating the database.");
    })
}

pub async fn migrate(db: &DatabaseConnection) -> Result<(), DbErr> {
    Migrator::up(db, None).await
}
