//! Миграции схемы. Применяются при старте web (`Migrator::up`), руками — через CLI этого крейта.

pub use sea_orm_migration::prelude::*;

mod m20261106_000001_create_qr_code;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(m20261106_000001_create_qr_code::Migration)]
    }
}
