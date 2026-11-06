//! API для микросервисов (JWT): создание, чтение и удаление своих кодов.

use std::collections::BTreeMap;

use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use template_application::qr_codes::{CreateQrCode, QrCodeDto};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::AppState;
use crate::auth::Authenticated;
use crate::error::{ApiResult, ProblemDetails};
use crate::extract::{Json, Path, Query};

const TAG: &str = "QrCodes";

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list, create))
        .routes(routes!(create_batch))
        .routes(routes!(get_by_code))
        .routes(routes!(get, delete))
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct QrCodeResponse {
    pub id: Uuid,
    /// Короткий код: `/s/{code}`.
    pub code: String,
    /// Сервис-владелец (`sub` его токена).
    pub owner: String,
    pub title: Option<String>,
    /// Куда перенаправляет скан (с подставленным кодом).
    pub target_url: String,
    pub metadata: BTreeMap<String, String>,
    pub expires_at: Option<DateTime<Utc>>,
    /// Не истёк: скан засчитывается и перенаправляет.
    pub is_active: bool,
    pub scans: u64,
    pub last_scanned_at: Option<DateTime<Utc>>,
    pub created: DateTime<Utc>,
    /// Ссылка, зашитая в QR.
    pub scan_url: String,
    pub image_svg_url: String,
    pub image_png_url: String,
}

impl From<QrCodeDto> for QrCodeResponse {
    fn from(dto: QrCodeDto) -> Self {
        Self {
            id: dto.id,
            code: dto.code,
            owner: dto.owner,
            title: dto.title,
            target_url: dto.target_url,
            metadata: dto.metadata,
            expires_at: dto.expires_at,
            is_active: dto.is_active,
            scans: dto.scans,
            last_scanned_at: dto.last_scanned_at,
            created: dto.created,
            scan_url: dto.scan_url,
            image_svg_url: dto.image_svg_url,
            image_png_url: dto.image_png_url,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateQrCodeRequest {
    /// Абсолютный http(s) URL; `{code}` заменяется коротким кодом:
    /// `https://t.me/aircul_bot?start=qr_{code}`.
    pub target_url: String,
    pub title: Option<String>,
    /// Строковые метки, до 16 штук: `{"kind": "minigame", "game": "minesweeper"}`.
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
    /// После этого момента скан не засчитывается и не перенаправляет (410).
    pub expires_at: Option<DateTime<Utc>>,
}

impl From<CreateQrCodeRequest> for CreateQrCode {
    fn from(request: CreateQrCodeRequest) -> Self {
        Self {
            target_url: request.target_url,
            title: request.title,
            metadata: request.metadata,
            expires_at: request.expires_at,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateQrCodeBatchRequest {
    /// Сколько кодов создать, 1..500.
    pub count: usize,
    #[serde(flatten)]
    pub code: CreateQrCodeRequest,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListQuery {
    #[serde(default)]
    pub offset: u64,
    /// 1..500, по умолчанию 100.
    pub limit: Option<u64>,
}

/// List own QR codes (oldest first)
#[utoipa::path(get, path = "/", tag = TAG, params(ListQuery), security(("bearer" = [])),
    responses((status = 200, body = Vec<QrCodeResponse>), (status = 401, body = ProblemDetails),
        (status = 403, body = ProblemDetails)))]
async fn list(
    State(state): State<AppState>,
    Authenticated(caller): Authenticated,
    Query(query): Query<ListQuery>,
) -> ApiResult<Json<Vec<QrCodeResponse>>> {
    let codes = state
        .qr_codes
        .list(&caller, query.offset, query.limit.unwrap_or(100))
        .await?;
    Ok(Json(codes.into_iter().map(Into::into).collect()))
}

/// Create QR code
#[utoipa::path(post, path = "/", tag = TAG, request_body = CreateQrCodeRequest, security(("bearer" = [])),
    responses((status = 201, body = QrCodeResponse), (status = 400, body = ProblemDetails),
        (status = 401, body = ProblemDetails), (status = 403, body = ProblemDetails)))]
async fn create(
    State(state): State<AppState>,
    Authenticated(caller): Authenticated,
    Json(request): Json<CreateQrCodeRequest>,
) -> ApiResult<impl IntoResponse> {
    let code = state.qr_codes.create(&caller, request.into()).await?;
    let location = format!("/api/v1/qr-codes/{}", code.id);
    Ok((
        StatusCode::CREATED,
        [(header::LOCATION, location)],
        Json(QrCodeResponse::from(code)),
    ))
}

/// Create a batch of QR codes from one template (atomic)
#[utoipa::path(post, path = "/batch", tag = TAG, request_body = CreateQrCodeBatchRequest, security(("bearer" = [])),
    responses((status = 201, body = Vec<QrCodeResponse>), (status = 400, body = ProblemDetails),
        (status = 401, body = ProblemDetails), (status = 403, body = ProblemDetails)))]
async fn create_batch(
    State(state): State<AppState>,
    Authenticated(caller): Authenticated,
    Json(request): Json<CreateQrCodeBatchRequest>,
) -> ApiResult<impl IntoResponse> {
    let codes = state
        .qr_codes
        .create_batch(&caller, request.count, request.code.into())
        .await?;
    let body: Vec<QrCodeResponse> = codes.into_iter().map(Into::into).collect();
    Ok((StatusCode::CREATED, Json(body)))
}

/// Get QR code by id (own, or any with `codes:read:any`)
#[utoipa::path(get, path = "/{id}", tag = TAG, params(("id" = Uuid, Path)), security(("bearer" = [])),
    responses((status = 200, body = QrCodeResponse), (status = 401, body = ProblemDetails),
        (status = 403, body = ProblemDetails), (status = 404, body = ProblemDetails)))]
async fn get(
    State(state): State<AppState>,
    Authenticated(caller): Authenticated,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<QrCodeResponse>> {
    Ok(Json(state.qr_codes.get(&caller, id).await?.into()))
}

/// Get QR code by short code (own, or any with `codes:read:any`)
#[utoipa::path(get, path = "/by-code/{code}", tag = TAG, params(("code" = String, Path)), security(("bearer" = [])),
    responses((status = 200, body = QrCodeResponse), (status = 401, body = ProblemDetails),
        (status = 403, body = ProblemDetails), (status = 404, body = ProblemDetails)))]
async fn get_by_code(
    State(state): State<AppState>,
    Authenticated(caller): Authenticated,
    Path(code): Path<String>,
) -> ApiResult<Json<QrCodeResponse>> {
    Ok(Json(
        state
            .qr_codes
            .get_by_short_code(&caller, &code)
            .await?
            .into(),
    ))
}

/// Delete own QR code
#[utoipa::path(delete, path = "/{id}", tag = TAG, params(("id" = Uuid, Path)), security(("bearer" = [])),
    responses((status = 204), (status = 401, body = ProblemDetails), (status = 403, body = ProblemDetails),
        (status = 404, body = ProblemDetails)))]
async fn delete(
    State(state): State<AppState>,
    Authenticated(caller): Authenticated,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    state.qr_codes.delete(&caller, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
