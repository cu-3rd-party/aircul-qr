//! Инфраструктурный слой: всё, что знает про SeaORM и PostgreSQL.
//! Наружу отдаёт доменные трейты (`Arc<dyn QrCodeRepository>`, `Arc<dyn UnitOfWork>`) и рендер QR.

pub mod persistence;
pub mod rendering;

use std::sync::Arc;

use sea_orm::{DatabaseConnection, DbErr};
use template_application::qr_codes::QrRenderer;
use template_domain::qr_code::QrCodeRepository;
use template_domain::unit_of_work::UnitOfWork;

use crate::persistence::repositories::SeaQrCodeRepository;
use crate::persistence::unit_of_work::SeaUnitOfWork;
use crate::rendering::QrCodeRenderer;

/// Настройки инфраструктуры (собираются в web из переменных окружения).
#[derive(Debug, Clone)]
pub struct InfrastructureConfig {
    pub database_url: String,
    /// Применять миграции при старте.
    pub initialise_database: bool,
}

/// Реализации доменных абстракций — аналог `AddInfrastructureServices`.
#[derive(Clone)]
pub struct Infrastructure {
    pub db: DatabaseConnection,
    pub qr_codes: Arc<dyn QrCodeRepository>,
    pub unit_of_work: Arc<dyn UnitOfWork>,
    pub renderer: Arc<dyn QrRenderer>,
}

impl Infrastructure {
    pub async fn init(config: &InfrastructureConfig) -> Result<Self, DbErr> {
        let db = persistence::connect(&config.database_url).await?;
        if config.initialise_database {
            persistence::initialise(&db).await?;
        }
        Ok(Self::from_connection(db))
    }

    /// Проверка доступности БД для `/health`.
    pub async fn is_healthy(&self) -> bool {
        self.db.ping().await.is_ok()
    }

    pub fn from_connection(db: DatabaseConnection) -> Self {
        Self {
            qr_codes: Arc::new(SeaQrCodeRepository::new(db.clone())),
            unit_of_work: Arc::new(SeaUnitOfWork::new(db.clone())),
            renderer: Arc::new(QrCodeRenderer),
            db,
        }
    }
}
