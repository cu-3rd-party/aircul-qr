use std::sync::Arc;

use template_application::qr_codes::{PublicLinks, QrCodeService};
use template_infrastructure::Infrastructure;

use crate::auth::JwtAuth;
use crate::config::AppSettings;

/// Composition root: связывает реализации из инфраструктуры с сервисами приложения.
/// Клонируется на каждый запрос, поэтому внутри только `Arc`/дешёвые хэндлы.
#[derive(Clone)]
pub struct AppState {
    pub infrastructure: Infrastructure,
    pub qr_codes: QrCodeService,
    pub auth: Arc<JwtAuth>,
}

impl AppState {
    pub fn new(infrastructure: Infrastructure, settings: &AppSettings) -> Self {
        Self {
            qr_codes: QrCodeService::new(
                infrastructure.qr_codes.clone(),
                infrastructure.unit_of_work.clone(),
                infrastructure.renderer.clone(),
                PublicLinks::new(&settings.public_base_url),
            ),
            auth: Arc::new(JwtAuth::new(&settings.jwt_secret)),
            infrastructure,
        }
    }
}
