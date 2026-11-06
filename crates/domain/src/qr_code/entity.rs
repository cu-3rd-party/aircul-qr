use std::fmt;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::{Metadata, QrCodeShortCode, QrTitle, ServiceName, TargetUrl};
use crate::common::Audit;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct QrCodeId(Uuid);

impl QrCodeId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    pub fn into_inner(self) -> Uuid {
        self.0
    }
}

impl Default for QrCodeId {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Uuid> for QrCodeId {
    fn from(value: Uuid) -> Self {
        Self(value)
    }
}

impl fmt::Display for QrCodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Что делать со сканом кода.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanState {
    /// Засчитать и перенаправить на `target_url`.
    Active,
    /// Срок действия вышел: не считать, не перенаправлять.
    Expired,
}

/// Поля нового кода (всё, кроме того, что выдаёт сам агрегат).
#[derive(Debug, Clone)]
pub struct NewQrCode {
    pub owner: ServiceName,
    pub short_code: QrCodeShortCode,
    pub target_url: TargetUrl,
    pub title: Option<QrTitle>,
    pub metadata: Metadata,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Агрегат «QR-код»: принадлежит микросервису, который его создал, ведёт на `target_url`
/// и считает сканы.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QrCode {
    id: QrCodeId,
    owner: ServiceName,
    short_code: QrCodeShortCode,
    target_url: TargetUrl,
    title: Option<QrTitle>,
    metadata: Metadata,
    expires_at: Option<DateTime<Utc>>,
    scans: u64,
    last_scanned_at: Option<DateTime<Utc>>,
    audit: Audit,
}

impl QrCode {
    pub fn new(fields: NewQrCode) -> Self {
        Self {
            id: QrCodeId::new(),
            owner: fields.owner,
            short_code: fields.short_code,
            target_url: fields.target_url,
            title: fields.title,
            metadata: fields.metadata,
            expires_at: fields.expires_at,
            scans: 0,
            last_scanned_at: None,
            audit: Audit::new(),
        }
    }

    /// Восстановление из хранилища (без проверок и без изменения аудита).
    pub fn restore(
        id: QrCodeId,
        fields: NewQrCode,
        scans: u64,
        last_scanned_at: Option<DateTime<Utc>>,
        audit: Audit,
    ) -> Self {
        Self {
            id,
            owner: fields.owner,
            short_code: fields.short_code,
            target_url: fields.target_url,
            title: fields.title,
            metadata: fields.metadata,
            expires_at: fields.expires_at,
            scans,
            last_scanned_at,
            audit,
        }
    }

    pub fn scan_state(&self, now: DateTime<Utc>) -> ScanState {
        match self.expires_at {
            Some(expires_at) if expires_at <= now => ScanState::Expired,
            _ => ScanState::Active,
        }
    }

    pub fn is_owned_by(&self, service: &ServiceName) -> bool {
        &self.owner == service
    }

    /// Скан в памяти; в хранилище счётчик увеличивается атомарно (`QrCodeRepository::record_scan`).
    pub fn register_scan(&mut self, now: DateTime<Utc>) {
        self.scans += 1;
        self.last_scanned_at = Some(now);
    }

    pub fn id(&self) -> QrCodeId {
        self.id
    }

    pub fn owner(&self) -> &ServiceName {
        &self.owner
    }

    pub fn short_code(&self) -> &QrCodeShortCode {
        &self.short_code
    }

    pub fn target_url(&self) -> &TargetUrl {
        &self.target_url
    }

    pub fn title(&self) -> Option<&QrTitle> {
        self.title.as_ref()
    }

    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    pub fn expires_at(&self) -> Option<DateTime<Utc>> {
        self.expires_at
    }

    pub fn scans(&self) -> u64 {
        self.scans
    }

    pub fn last_scanned_at(&self) -> Option<DateTime<Utc>> {
        self.last_scanned_at
    }

    pub fn audit(&self) -> &Audit {
        &self.audit
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;

    fn code(expires_at: Option<DateTime<Utc>>) -> QrCode {
        let short_code = QrCodeShortCode::generate();
        QrCode::new(NewQrCode {
            owner: ServiceName::parse("aircul-bot").unwrap(),
            target_url: TargetUrl::from_template("https://t.me/bot?start=qr_{code}", &short_code)
                .unwrap(),
            short_code,
            title: None,
            metadata: Metadata::default(),
            expires_at,
        })
    }

    #[test]
    fn new_code_has_no_scans() {
        let code = code(None);

        assert_eq!(code.scans(), 0);
        assert!(code.last_scanned_at().is_none());
        assert!(
            code.target_url()
                .as_str()
                .ends_with(code.short_code().as_str())
        );
    }

    #[test]
    fn expiry_controls_scan_state() {
        let now = Utc::now();

        assert_eq!(code(None).scan_state(now), ScanState::Active);
        assert_eq!(
            code(Some(now + Duration::minutes(1))).scan_state(now),
            ScanState::Active
        );
        assert_eq!(code(Some(now)).scan_state(now), ScanState::Expired);
    }

    #[test]
    fn register_scan_counts() {
        let mut code = code(None);
        let now = Utc::now();

        code.register_scan(now);
        code.register_scan(now);

        assert_eq!(code.scans(), 2);
        assert_eq!(code.last_scanned_at(), Some(now));
    }

    #[test]
    fn ownership() {
        let code = code(None);

        assert!(code.is_owned_by(&ServiceName::parse("aircul-bot").unwrap()));
        assert!(!code.is_owned_by(&ServiceName::parse("aircul-site").unwrap()));
    }
}
