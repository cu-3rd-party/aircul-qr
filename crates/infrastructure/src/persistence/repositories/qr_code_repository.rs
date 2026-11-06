use std::collections::BTreeMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_orm::sea_query::{Expr, ExprTrait};
use sea_orm::{
    ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect,
};
use template_domain::RepositoryError;
use template_domain::common::Audit;
use template_domain::error::RepositoryResult;
use template_domain::qr_code::{
    Metadata, NewQrCode, QrCode, QrCodeId, QrCodeRepository, QrCodeShortCode, QrTitle, ServiceName,
    TargetUrl,
};

use crate::persistence::entities::qr_code::{ActiveModel, Column, Entity, Model};

/// Реализация над любым соединением SeaORM: пулом (`DatabaseConnection`)
/// или транзакцией (`DatabaseExecutor` поверх `DatabaseTransaction`).
pub struct SeaQrCodeRepository<C> {
    conn: C,
}

impl<C> SeaQrCodeRepository<C> {
    pub fn new(conn: C) -> Self {
        Self { conn }
    }
}

#[async_trait]
impl<C> QrCodeRepository for SeaQrCodeRepository<C>
where
    C: ConnectionTrait + Send + Sync,
{
    async fn find_by_id(&self, id: QrCodeId) -> RepositoryResult<Option<QrCode>> {
        let model = Entity::find_by_id(id.into_inner())
            .one(&self.conn)
            .await
            .map_err(RepositoryError::new)?;
        model.map(to_domain).transpose()
    }

    async fn find_by_short_code(&self, code: &QrCodeShortCode) -> RepositoryResult<Option<QrCode>> {
        let model = Entity::find()
            .filter(Column::Code.eq(code.as_str()))
            .one(&self.conn)
            .await
            .map_err(RepositoryError::new)?;
        model.map(to_domain).transpose()
    }

    async fn list_by_owner(
        &self,
        owner: &ServiceName,
        offset: u64,
        limit: u64,
    ) -> RepositoryResult<Vec<QrCode>> {
        let models = Entity::find()
            .filter(Column::Owner.eq(owner.as_str()))
            .order_by_asc(Column::Created)
            .order_by_asc(Column::Id)
            .offset(offset)
            .limit(limit)
            .all(&self.conn)
            .await
            .map_err(RepositoryError::new)?;
        models.into_iter().map(to_domain).collect()
    }

    async fn insert(&self, code: &QrCode) -> RepositoryResult<()> {
        Entity::insert(to_active_model(code))
            .exec_without_returning(&self.conn)
            .await
            .map_err(RepositoryError::new)?;
        Ok(())
    }

    async fn delete(&self, id: QrCodeId) -> RepositoryResult<bool> {
        let result = Entity::delete_by_id(id.into_inner())
            .exec(&self.conn)
            .await
            .map_err(RepositoryError::new)?;
        Ok(result.rows_affected > 0)
    }

    async fn record_scan(&self, id: QrCodeId, at: DateTime<Utc>) -> RepositoryResult<bool> {
        // UPDATE ... SET scans = scans + 1: инкремент в базе, без read-modify-write
        let result = Entity::update_many()
            .col_expr(Column::Scans, Expr::col(Column::Scans).add(1))
            .col_expr(Column::LastScannedAt, Expr::value(at.fixed_offset()))
            .filter(Column::Id.eq(id.into_inner()))
            .exec(&self.conn)
            .await
            .map_err(RepositoryError::new)?;
        Ok(result.rows_affected > 0)
    }
}

fn to_domain(model: Model) -> RepositoryResult<QrCode> {
    // данные в БД должны удовлетворять инвариантам; если нет — это ошибка хранилища
    let metadata: BTreeMap<String, String> =
        serde_json::from_value(model.metadata).map_err(RepositoryError::new)?;
    let fields = NewQrCode {
        owner: ServiceName::parse(&model.owner).map_err(RepositoryError::new)?,
        short_code: QrCodeShortCode::parse(&model.code).map_err(RepositoryError::new)?,
        target_url: TargetUrl::parse(&model.target_url).map_err(RepositoryError::new)?,
        title: model
            .title
            .as_deref()
            .map(QrTitle::parse)
            .transpose()
            .map_err(RepositoryError::new)?,
        metadata: Metadata::parse(metadata).map_err(RepositoryError::new)?,
        expires_at: model.expires_at.map(|at| at.with_timezone(&Utc)),
    };
    Ok(QrCode::restore(
        model.id.into(),
        fields,
        u64::try_from(model.scans).unwrap_or_default(),
        model.last_scanned_at.map(|at| at.with_timezone(&Utc)),
        Audit::from_parts(
            model.created.with_timezone(&Utc),
            model.last_modified.with_timezone(&Utc),
        ),
    ))
}

fn to_active_model(code: &QrCode) -> ActiveModel {
    ActiveModel {
        id: Set(code.id().into_inner()),
        code: Set(code.short_code().to_string()),
        owner: Set(code.owner().to_string()),
        title: Set(code.title().map(ToString::to_string)),
        target_url: Set(code.target_url().to_string()),
        metadata: Set(serde_json::to_value(code.metadata().entries()).unwrap_or_default()),
        expires_at: Set(code.expires_at().map(|at| at.fixed_offset())),
        scans: Set(i64::try_from(code.scans()).unwrap_or(i64::MAX)),
        last_scanned_at: Set(code.last_scanned_at().map(|at| at.fixed_offset())),
        created: Set(code.audit().created().fixed_offset()),
        last_modified: Set(code.audit().last_modified().fixed_offset()),
    }
}
