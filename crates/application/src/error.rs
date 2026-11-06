use std::collections::BTreeMap;

use template_domain::{DomainError, RepositoryError};

/// Ошибки по полям: `{"title": ["must not be empty"]}`.
pub type ValidationErrors = BTreeMap<String, Vec<String>>;

/// Ошибка сценария. Web-слой переводит её в ProblemDetails с нужным статусом.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("One or more validation failures have occurred.")]
    Validation(ValidationErrors),

    #[error("{entity} with id {id} was not found.")]
    NotFound { entity: &'static str, id: String },

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Forbidden")]
    Forbidden,

    #[error(transparent)]
    Repository(#[from] RepositoryError),

    /// Сбой не из хранилища (например, рендер картинки).
    #[error("{0}")]
    Internal(String),
}

impl AppError {
    pub fn not_found(entity: &'static str, id: impl ToString) -> Self {
        Self::NotFound {
            entity,
            id: id.to_string(),
        }
    }
}

impl From<DomainError> for AppError {
    fn from(error: DomainError) -> Self {
        match error {
            DomainError::Validation { field, message } => {
                Self::Validation(BTreeMap::from([(field.into(), vec![message])]))
            }
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;
