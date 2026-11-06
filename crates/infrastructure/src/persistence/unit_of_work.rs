use async_trait::async_trait;
use sea_orm::{DatabaseConnection, DatabaseExecutor, DatabaseTransaction, TransactionTrait};
use template_domain::RepositoryError;
use template_domain::error::RepositoryResult;
use template_domain::qr_code::QrCodeRepository;
use template_domain::unit_of_work::{Transaction, UnitOfWork};

use super::repositories::SeaQrCodeRepository;

pub struct SeaUnitOfWork {
    db: DatabaseConnection,
}

impl SeaUnitOfWork {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait]
impl UnitOfWork for SeaUnitOfWork {
    async fn begin(&self) -> RepositoryResult<Box<dyn Transaction>> {
        let txn = self.db.begin().await.map_err(RepositoryError::new)?;
        Ok(Box::new(SeaTransaction { txn }))
    }
}

pub struct SeaTransaction {
    txn: DatabaseTransaction,
}

#[async_trait]
impl Transaction for SeaTransaction {
    fn qr_codes(&self) -> Box<dyn QrCodeRepository + '_> {
        Box::new(SeaQrCodeRepository::new(DatabaseExecutor::from(&self.txn)))
    }

    async fn commit(self: Box<Self>) -> RepositoryResult<()> {
        self.txn.commit().await.map_err(RepositoryError::new)
    }

    async fn rollback(self: Box<Self>) -> RepositoryResult<()> {
        self.txn.rollback().await.map_err(RepositoryError::new)
    }
}
