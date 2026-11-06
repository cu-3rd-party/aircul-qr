use async_trait::async_trait;

use crate::error::RepositoryResult;
use crate::qr_code::QrCodeRepository;

/// Единица работы: открывает транзакцию, внутри которой репозитории работают атомарно.
/// Реализация (SeaORM) — в `template-infrastructure`.
#[async_trait]
pub trait UnitOfWork: Send + Sync {
    async fn begin(&self) -> RepositoryResult<Box<dyn Transaction>>;
}

/// Открытая транзакция. Без явного `commit` изменения откатываются при drop.
#[async_trait]
pub trait Transaction: Send + Sync {
    /// Репозиторий, работающий внутри этой транзакции.
    fn qr_codes(&self) -> Box<dyn QrCodeRepository + '_>;

    async fn commit(self: Box<Self>) -> RepositoryResult<()>;

    async fn rollback(self: Box<Self>) -> RepositoryResult<()>;
}
