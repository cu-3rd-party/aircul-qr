use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QrCodeDto {
    pub id: Uuid,
    pub code: String,
    pub owner: String,
    pub title: Option<String>,
    pub target_url: String,
    pub metadata: BTreeMap<String, String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub is_active: bool,
    pub scans: u64,
    pub last_scanned_at: Option<DateTime<Utc>>,
    pub created: DateTime<Utc>,
    /// Что зашито в QR: публичная ссылка скана.
    pub scan_url: String,
    pub image_svg_url: String,
    pub image_png_url: String,
}

#[derive(Debug, Clone, Default)]
pub struct CreateQrCode {
    /// Шаблон, `{code}` заменяется коротким кодом.
    pub target_url: String,
    pub title: Option<String>,
    pub metadata: BTreeMap<String, String>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Чем ответить на скан.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanOutcome {
    Redirect(String),
    NotFound,
    Expired,
}
