//! Слой приложения: сценарии использования поверх доменных абстракций.
//! Сервисы получают репозитории как `Arc<dyn Trait>` и ничего не знают о SeaORM и HTTP.

pub mod auth;
pub mod error;
pub mod qr_codes;

pub use error::{AppError, AppResult};
