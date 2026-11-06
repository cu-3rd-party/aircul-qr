use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "qr_code")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(unique)]
    pub code: String,
    pub owner: String,
    pub title: Option<String>,
    pub target_url: String,
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: Json,
    pub expires_at: Option<DateTimeWithTimeZone>,
    pub scans: i64,
    pub last_scanned_at: Option<DateTimeWithTimeZone>,
    pub created: DateTimeWithTimeZone,
    pub last_modified: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
