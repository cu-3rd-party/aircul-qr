//! Интеграционные тесты репозиториев и UnitOfWork на настоящем PostgreSQL (своя БД на тест).

use std::collections::BTreeMap;

use chrono::{Duration, Utc};
use template_domain::qr_code::{
    Metadata, NewQrCode, QrCode, QrCodeShortCode, QrTitle, ServiceName, TargetUrl,
};
use template_infrastructure::{Infrastructure, persistence};
use template_test_support::TestDatabase;

async fn setup() -> Infrastructure {
    let db = TestDatabase::create().await;
    Infrastructure::from_connection(persistence::connect(&db.url).await.unwrap())
}

fn code(owner: &str) -> QrCode {
    let short_code = QrCodeShortCode::generate();
    QrCode::new(NewQrCode {
        owner: ServiceName::parse(owner).unwrap(),
        target_url: TargetUrl::from_template("https://t.me/bot?start=qr_{code}", &short_code)
            .unwrap(),
        short_code,
        title: Some(QrTitle::parse("Холл").unwrap()),
        metadata: Metadata::parse(BTreeMap::from([("game".into(), "minesweeper".into())])).unwrap(),
        expires_at: Some(Utc::now() + Duration::hours(1)),
    })
}

#[tokio::test]
async fn repository_round_trip() {
    let infra = setup().await;
    let code = code("aircul-bot");

    infra.qr_codes.insert(&code).await.unwrap();

    let by_id = infra.qr_codes.find_by_id(code.id()).await.unwrap().unwrap();
    let by_code = infra
        .qr_codes
        .find_by_short_code(code.short_code())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(by_id.id(), by_code.id());
    assert_eq!(by_id.target_url(), code.target_url());
    assert_eq!(by_id.metadata(), code.metadata());
    assert_eq!(by_id.title(), code.title());
    // PostgreSQL хранит микросекунды
    assert_eq!(
        by_id.audit().created().timestamp_micros(),
        code.audit().created().timestamp_micros()
    );
}

#[tokio::test]
async fn record_scan_is_atomic_under_concurrency() {
    let infra = setup().await;
    let code = code("aircul-bot");
    infra.qr_codes.insert(&code).await.unwrap();

    let scans = (0..20).map(|_| {
        let repo = infra.qr_codes.clone();
        let id = code.id();
        tokio::spawn(async move { repo.record_scan(id, Utc::now()).await.unwrap() })
    });
    for scan in scans {
        assert!(scan.await.unwrap());
    }

    let loaded = infra.qr_codes.find_by_id(code.id()).await.unwrap().unwrap();
    assert_eq!(loaded.scans(), 20);
    assert!(loaded.last_scanned_at().is_some());
}

#[tokio::test]
async fn list_by_owner_pages_in_creation_order() {
    let infra = setup().await;
    let mine: Vec<_> = (0..3).map(|_| code("aircul-bot")).collect();
    for code in &mine {
        infra.qr_codes.insert(code).await.unwrap();
    }
    infra.qr_codes.insert(&code("aircul-site")).await.unwrap();
    let owner = ServiceName::parse("aircul-bot").unwrap();

    let all = infra.qr_codes.list_by_owner(&owner, 0, 10).await.unwrap();
    let page = infra.qr_codes.list_by_owner(&owner, 1, 1).await.unwrap();

    assert_eq!(all.len(), 3);
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].id(), mine[1].id());
}

#[tokio::test]
async fn unit_of_work_commit_and_rollback() {
    let infra = setup().await;
    let committed = code("aircul-bot");
    let rolled_back = code("aircul-bot");

    let txn = infra.unit_of_work.begin().await.unwrap();
    txn.qr_codes().insert(&committed).await.unwrap();
    txn.commit().await.unwrap();

    let txn = infra.unit_of_work.begin().await.unwrap();
    txn.qr_codes().insert(&rolled_back).await.unwrap();
    assert!(
        txn.qr_codes()
            .find_by_id(rolled_back.id())
            .await
            .unwrap()
            .is_some()
    );
    txn.rollback().await.unwrap();

    assert!(
        infra
            .qr_codes
            .find_by_id(committed.id())
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        infra
            .qr_codes
            .find_by_id(rolled_back.id())
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn delete_and_missing_scan() {
    let infra = setup().await;
    let code = code("aircul-bot");
    infra.qr_codes.insert(&code).await.unwrap();

    assert!(infra.qr_codes.delete(code.id()).await.unwrap());
    assert!(!infra.qr_codes.delete(code.id()).await.unwrap());
    assert!(
        !infra
            .qr_codes
            .record_scan(code.id(), Utc::now())
            .await
            .unwrap()
    );
}
