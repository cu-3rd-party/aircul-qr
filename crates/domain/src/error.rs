use std::error::Error;

/// Нарушение инварианта доменной модели.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DomainError {
    #[error("{field}: {message}")]
    Validation {
        field: &'static str,
        message: String,
    },
}

impl DomainError {
    pub fn validation(field: &'static str, message: impl Into<String>) -> Self {
        Self::Validation {
            field,
            message: message.into(),
        }
    }
}

/// Ошибка хранилища. Домен не знает, что внутри (SeaORM, sqlx, ...), — только что операция не удалась.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct RepositoryError(Box<dyn Error + Send + Sync>);

impl RepositoryError {
    pub fn new(source: impl Error + Send + Sync + 'static) -> Self {
        Self(Box::new(source))
    }
}

pub type RepositoryResult<T> = Result<T, RepositoryError>;
