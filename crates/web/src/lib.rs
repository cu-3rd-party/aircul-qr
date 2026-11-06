//! Web-слой: HTTP-контракт (request/response модели, OpenAPI), маппинг ошибок в ProblemDetails
//! и сборка зависимостей. Бизнес-логики здесь нет — только вызовы сервисов приложения.

pub mod auth;
pub mod config;
pub mod error;
pub mod extract;
pub mod routes;
pub mod state;

use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};
use utoipa_axum::router::OpenApiRouter;
use utoipa_scalar::{Scalar, Servable};

pub use crate::state::AppState;

#[derive(OpenApi)]
#[openapi(info(title = "AirCul QR API", version = "v1"), modifiers(&BearerAuth))]
struct ApiDoc;

/// Схема `bearer` для OpenAPI: JWT микросервиса.
struct BearerAuth;

impl Modify for BearerAuth {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "bearer",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .build(),
            ),
        );
    }
}

/// Полный роутер приложения: API, публичные `/s/{code}` и `/i/{file}`, `/health`, `/openapi/v1.json`, `/scalar`.
pub fn app(state: AppState) -> Router {
    let (router, openapi) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .nest("/api/v1", routes::v1::router())
        .merge(routes::public::router())
        .split_for_parts();

    router
        .merge(routes::health::router())
        .route(
            "/openapi/v1.json",
            axum::routing::get({
                let openapi = openapi.clone();
                move || async move { axum::Json(openapi) }
            }),
        )
        .merge(Scalar::with_url("/scalar", openapi))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}
