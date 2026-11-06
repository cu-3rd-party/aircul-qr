//! Публичные адреса без токена: скан (`/s/{code}`) и картинка (`/i/{code}.png|svg`).

use axum::extract::State;
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use serde::Deserialize;
use template_application::qr_codes::{ImageFormat, ScanOutcome};
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::AppState;
use crate::error::{ApiResult, ProblemDetails};
use crate::extract::{Path, Query};

const TAG: &str = "Public";

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(scan))
        .routes(routes!(image))
}

/// Scan: count and redirect to the target
///
/// `HEAD` перенаправляет, но не засчитывается. Истёкший код — 410, неизвестный — 404.
#[utoipa::path(get, path = "/s/{code}", tag = TAG, params(("code" = String, Path)),
    responses((status = 302, description = "Redirect to targetUrl"), (status = 404), (status = 410)))]
async fn scan(
    State(state): State<AppState>,
    method: Method,
    Path(code): Path<String>,
) -> ApiResult<Response> {
    let outcome = state.qr_codes.scan(&code, method != Method::HEAD).await?;
    let mut response = match outcome {
        ScanOutcome::Redirect(target) => match HeaderValue::from_str(&target) {
            Ok(location) => (StatusCode::FOUND, [(header::LOCATION, location)]).into_response(),
            Err(_) => page(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Ошибка",
                "Не удалось открыть ссылку QR-кода.",
            ),
        },
        ScanOutcome::NotFound => page(
            StatusCode::NOT_FOUND,
            "QR-код не найден",
            "Такого QR-кода нет. Проверьте, что сканируете код организаторов.",
        ),
        ScanOutcome::Expired => page(
            StatusCode::GONE,
            "QR-код устарел",
            "Срок действия этого QR-кода закончился.",
        ),
    };
    let headers = response.headers_mut();
    // каждый скан должен дойти до сервера, а адрес назначения — не утечь в Referer
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    Ok(response)
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ImageQuery {
    /// Минимальная сторона в пикселях, 64..4096, по умолчанию 512.
    pub size: Option<u32>,
}

/// QR code image (`{code}.png` or `{code}.svg`)
#[utoipa::path(get, path = "/i/{file}", tag = TAG, params(("file" = String, Path), ImageQuery),
    responses((status = 200, description = "image/png or image/svg+xml"),
        (status = 400, body = ProblemDetails), (status = 404, body = ProblemDetails)))]
async fn image(
    State(state): State<AppState>,
    Path(file): Path<String>,
    Query(query): Query<ImageQuery>,
) -> ApiResult<Response> {
    let parsed = file
        .rsplit_once('.')
        .and_then(|(code, extension)| Some((code, extension.parse::<ImageFormat>().ok()?)));
    let Some((code, format)) = parsed else {
        return Err(template_application::AppError::not_found("QrCode", file).into());
    };
    let image = state.qr_codes.image(code, format, query.size).await?;
    Ok((
        [
            (header::CONTENT_TYPE, image.format.content_type()),
            // картинка кода не меняется: в ней только ссылка скана
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        image.bytes,
    )
        .into_response())
}

fn page(status: StatusCode, title: &str, text: &str) -> Response {
    let html = format!(
        r#"<!doctype html><html lang="ru"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>{title}</title><style>body{{font-family:system-ui,sans-serif;margin:0;min-height:100vh;display:grid;place-items:center;background:#f4f4f5;color:#18181b}}main{{max-width:28rem;padding:1.5rem;text-align:center}}h1{{font-size:1.4rem}}@media(prefers-color-scheme:dark){{body{{background:#18181b;color:#f4f4f5}}}}</style></head><body><main><h1>{title}</h1><p>{text}</p></main></body></html>"#
    );
    (status, Html(html)).into_response()
}
