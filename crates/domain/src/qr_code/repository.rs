use async_trait::async_trait;
use chrono::{DateTime, Utc};

use super::{QrCode, QrCodeId, QrCodeShortCode, ServiceName};
use crate::error::RepositoryResult;

/// Репозиторий агрегата. Реализация (SeaORM) — в `template-infrastructure`.
#[async_trait]
pub trait QrCodeRepository: Send + Sync {
    async fn find_by_id(&self, id: QrCodeId) -> RepositoryResult<Option<QrCode>>;

    async fn find_by_short_code(&self, code: &QrCodeShortCode) -> RepositoryResult<Option<QrCode>>;

    /// Коды владельца от старых к новым.
    async fn list_by_owner(
        &self,
        owner: &ServiceName,
        offset: u64,
        limit: u64,
    ) -> RepositoryResult<Vec<QrCode>>;

    async fn insert(&self, code: &QrCode) -> RepositoryResult<()>;

    /// `false`, если записи с таким id нет.
    async fn delete(&self, id: QrCodeId) -> RepositoryResult<bool>;

    /// Атомарно увеличивает счётчик сканов (параллельные сканы не теряются).
    /// `false`, если записи с таким id нет.
    async fn record_scan(&self, id: QrCodeId, at: DateTime<Utc>) -> RepositoryResult<bool>;
}
