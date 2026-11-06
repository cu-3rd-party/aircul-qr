use axum::Json;
use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use template_application::AppError;
use template_application::error::ValidationErrors;
use utoipa::ToSchema;

/// RFC 9457 Problem Details.
#[derive(Debug, Serialize, ToSchema)]
pub struct ProblemDetails {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub title: &'static str,
    pub status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<ValidationErrors>,
}

/// Ошибка обработчика: всё, что может пойти не так, превращается в ProblemDetails.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error(transparent)]
    App(#[from] AppError),

    #[error("{0}")]
    BadRequest(String),
}

impl From<JsonRejection> for ApiError {
    fn from(rejection: JsonRejection) -> Self {
        Self::BadRequest(rejection.body_text())
    }
}

impl From<PathRejection> for ApiError {
    fn from(rejection: PathRejection) -> Self {
        Self::BadRequest(rejection.body_text())
    }
}

impl From<QueryRejection> for ApiError {
    fn from(rejection: QueryRejection) -> Self {
        Self::BadRequest(rejection.body_text())
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let problem = match self {
            ApiError::App(AppError::Validation(errors)) => ProblemDetails {
                errors: Some(errors),
                ..problem(
                    StatusCode::BAD_REQUEST,
                    "One or more validation errors occurred.",
                    None,
                )
            },
            ApiError::BadRequest(detail) => {
                problem(StatusCode::BAD_REQUEST, "Bad Request", Some(detail))
            }
            ApiError::App(error @ AppError::NotFound { .. }) => problem(
                StatusCode::NOT_FOUND,
                "The specified resource was not found.",
                Some(error.to_string()),
            ),
            ApiError::App(AppError::Unauthorized) => {
                problem(StatusCode::UNAUTHORIZED, "Unauthorized", None)
            }
            ApiError::App(AppError::Forbidden) => problem(StatusCode::FORBIDDEN, "Forbidden", None),
            ApiError::App(AppError::Internal(error)) => {
                tracing::error!(%error, "Unhandled internal error");
                problem(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "An error occurred while processing your request.",
                    None,
                )
            }
            ApiError::App(AppError::Repository(error)) => {
                tracing::error!(%error, "Unhandled repository error");
                problem(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "An error occurred while processing your request.",
                    None,
                )
            }
        };

        let status =
            StatusCode::from_u16(problem.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let mut response = (
            status,
            [(header::CONTENT_TYPE, "application/problem+json")],
            Json(problem),
        )
            .into_response();
        if status == StatusCode::UNAUTHORIZED {
            response.headers_mut().insert(
                header::WWW_AUTHENTICATE,
                header::HeaderValue::from_static("Bearer"),
            );
        }
        response
    }
}

fn problem(status: StatusCode, title: &'static str, detail: Option<String>) -> ProblemDetails {
    ProblemDetails {
        kind: type_uri(status),
        title,
        status: status.as_u16(),
        detail,
        errors: None,
    }
}

fn type_uri(status: StatusCode) -> &'static str {
    match status {
        StatusCode::BAD_REQUEST => "https://tools.ietf.org/html/rfc9110#section-15.5.1",
        StatusCode::UNAUTHORIZED => "https://tools.ietf.org/html/rfc9110#section-15.5.2",
        StatusCode::FORBIDDEN => "https://tools.ietf.org/html/rfc9110#section-15.5.4",
        StatusCode::NOT_FOUND => "https://tools.ietf.org/html/rfc9110#section-15.5.5",
        _ => "https://tools.ietf.org/html/rfc9110#section-15.6.1",
    }
}
