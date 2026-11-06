pub mod qr_codes;

use utoipa_axum::router::OpenApiRouter;

use crate::AppState;

/// Все эндпоинты `/api/v1`. Новая фича — модуль рядом и строка здесь.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().nest("/qr-codes", qr_codes::router())
}
