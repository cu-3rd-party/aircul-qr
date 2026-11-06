//! Доменный слой. Не зависит ни от SeaORM, ни от axum: только сущности, value objects,
//! доменные ошибки и контракты репозиториев (реализации — в `template-infrastructure`).

pub mod common;
pub mod error;
pub mod qr_code;
pub mod unit_of_work;

pub use error::{DomainError, RepositoryError};
