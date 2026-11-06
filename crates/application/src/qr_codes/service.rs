use std::sync::Arc;

use chrono::{DateTime, Utc};
use template_domain::qr_code::{
    Metadata, NewQrCode, QrCode, QrCodeId, QrCodeRepository, QrCodeShortCode, QrTitle, ScanState,
    TargetUrl,
};
use template_domain::unit_of_work::UnitOfWork;
use uuid::Uuid;

use super::{CreateQrCode, ImageFormat, QrCodeDto, QrRenderer, RenderedImage, ScanOutcome};
use crate::auth::{Caller, Scope};
use crate::error::ValidationErrors;
use crate::{AppError, AppResult};

const ENTITY: &str = "QrCode";

/// Источник текущего времени (в тестах — фиксированный).
pub type Clock = Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>;

/// Публичный адрес сервиса: из него собираются ссылка скана (её содержит QR) и ссылки на картинки.
#[derive(Debug, Clone)]
pub struct PublicLinks {
    base_url: String,
}

impl PublicLinks {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_owned(),
        }
    }

    pub fn scan(&self, code: &str) -> String {
        format!("{}/s/{code}", self.base_url)
    }

    pub fn image(&self, code: &str, format: ImageFormat) -> String {
        format!("{}/i/{code}.{}", self.base_url, format.extension())
    }
}

#[derive(Clone)]
pub struct QrCodeService {
    qr_codes: Arc<dyn QrCodeRepository>,
    unit_of_work: Arc<dyn UnitOfWork>,
    renderer: Arc<dyn QrRenderer>,
    links: PublicLinks,
    clock: Clock,
}

impl QrCodeService {
    pub const MAX_BATCH: usize = 500;
    pub const MAX_PAGE: u64 = 500;
    pub const DEFAULT_IMAGE_SIZE: u32 = 512;
    pub const MIN_IMAGE_SIZE: u32 = 64;
    pub const MAX_IMAGE_SIZE: u32 = 4096;

    pub fn new(
        qr_codes: Arc<dyn QrCodeRepository>,
        unit_of_work: Arc<dyn UnitOfWork>,
        renderer: Arc<dyn QrRenderer>,
        links: PublicLinks,
    ) -> Self {
        Self {
            qr_codes,
            unit_of_work,
            renderer,
            links,
            clock: Arc::new(Utc::now),
        }
    }

    pub fn with_clock(mut self, clock: Clock) -> Self {
        self.clock = clock;
        self
    }

    pub async fn create(&self, caller: &Caller, request: CreateQrCode) -> AppResult<QrCodeDto> {
        require(caller, Scope::CodesWrite)?;
        let code = self.build(caller, &request)?;
        self.qr_codes.insert(&code).await?;
        Ok(self.to_dto(&code))
    }

    /// `count` одинаковых по шаблону кодов в одной транзакции (печать пачкой).
    pub async fn create_batch(
        &self,
        caller: &Caller,
        count: usize,
        request: CreateQrCode,
    ) -> AppResult<Vec<QrCodeDto>> {
        require(caller, Scope::CodesWrite)?;
        if !(1..=Self::MAX_BATCH).contains(&count) {
            return Err(validation(
                "count",
                format!("must be between 1 and {}", Self::MAX_BATCH),
            ));
        }
        let codes = (0..count)
            .map(|_| self.build(caller, &request))
            .collect::<AppResult<Vec<_>>>()?;

        let txn = self.unit_of_work.begin().await?;
        for code in &codes {
            txn.qr_codes().insert(code).await?;
        }
        txn.commit().await?;
        Ok(codes.iter().map(|code| self.to_dto(code)).collect())
    }

    pub async fn list(
        &self,
        caller: &Caller,
        offset: u64,
        limit: u64,
    ) -> AppResult<Vec<QrCodeDto>> {
        require(caller, Scope::CodesRead)?;
        let limit = limit.clamp(1, Self::MAX_PAGE);
        let codes = self
            .qr_codes
            .list_by_owner(&caller.service, offset, limit)
            .await?;
        Ok(codes.iter().map(|code| self.to_dto(code)).collect())
    }

    pub async fn get(&self, caller: &Caller, id: Uuid) -> AppResult<QrCodeDto> {
        require_read(caller)?;
        let code = self.qr_codes.find_by_id(QrCodeId::from(id)).await?;
        self.readable(caller, code, id.to_string())
    }

    pub async fn get_by_short_code(&self, caller: &Caller, code: &str) -> AppResult<QrCodeDto> {
        require_read(caller)?;
        let found = match QrCodeShortCode::parse(code) {
            Ok(short_code) => self.qr_codes.find_by_short_code(&short_code).await?,
            Err(_) => None,
        };
        self.readable(caller, found, code.to_owned())
    }

    pub async fn delete(&self, caller: &Caller, id: Uuid) -> AppResult<()> {
        require(caller, Scope::CodesWrite)?;
        let id = QrCodeId::from(id);
        let code = self.qr_codes.find_by_id(id).await?;
        match code {
            Some(code) if code.is_owned_by(&caller.service) => {
                if !self.qr_codes.delete(id).await? {
                    return Err(AppError::not_found(ENTITY, id));
                }
                Ok(())
            }
            // чужой код не отличается от несуществующего
            _ => Err(AppError::not_found(ENTITY, id)),
        }
    }

    /// Публичный скан. `count = false` — не засчитывать (HEAD, префетчеры).
    pub async fn scan(&self, code: &str, count: bool) -> AppResult<ScanOutcome> {
        let Ok(short_code) = QrCodeShortCode::parse(code) else {
            return Ok(ScanOutcome::NotFound);
        };
        let Some(code) = self.qr_codes.find_by_short_code(&short_code).await? else {
            return Ok(ScanOutcome::NotFound);
        };
        let now = (self.clock)();
        match code.scan_state(now) {
            ScanState::Expired => Ok(ScanOutcome::Expired),
            ScanState::Active => {
                if count {
                    self.qr_codes.record_scan(code.id(), now).await?;
                }
                Ok(ScanOutcome::Redirect(code.target_url().to_string()))
            }
        }
    }

    /// Публичная картинка существующего кода; в ней — ссылка скана.
    pub async fn image(
        &self,
        code: &str,
        format: ImageFormat,
        size: Option<u32>,
    ) -> AppResult<RenderedImage> {
        let short_code =
            QrCodeShortCode::parse(code).map_err(|_| AppError::not_found(ENTITY, code))?;
        if self
            .qr_codes
            .find_by_short_code(&short_code)
            .await?
            .is_none()
        {
            return Err(AppError::not_found(ENTITY, code));
        }
        let size = size.unwrap_or(Self::DEFAULT_IMAGE_SIZE);
        if !(Self::MIN_IMAGE_SIZE..=Self::MAX_IMAGE_SIZE).contains(&size) {
            return Err(validation(
                "size",
                format!(
                    "must be between {} and {}",
                    Self::MIN_IMAGE_SIZE,
                    Self::MAX_IMAGE_SIZE
                ),
            ));
        }
        self.renderer
            .render(&self.links.scan(short_code.as_str()), format, size)
            .map_err(|error| AppError::Internal(error.to_string()))
    }

    fn build(&self, caller: &Caller, request: &CreateQrCode) -> AppResult<QrCode> {
        let mut errors = ValidationErrors::new();
        let short_code = QrCodeShortCode::generate();

        let target_url = collect(
            &mut errors,
            TargetUrl::from_template(&request.target_url, &short_code),
        );
        let title = match request.title.as_deref().map(str::trim) {
            None | Some("") => None,
            Some(title) => collect(&mut errors, QrTitle::parse(title)),
        };
        let metadata = collect(&mut errors, Metadata::parse(request.metadata.clone()));
        if request
            .expires_at
            .is_some_and(|expires_at| expires_at <= (self.clock)())
        {
            errors
                .entry("expiresAt".into())
                .or_default()
                .push("must be in the future".into());
        }

        match (target_url, metadata) {
            (Some(target_url), Some(metadata)) if errors.is_empty() => Ok(QrCode::new(NewQrCode {
                owner: caller.service.clone(),
                short_code,
                target_url,
                title,
                metadata,
                expires_at: request.expires_at,
            })),
            _ => Err(AppError::Validation(errors)),
        }
    }

    fn readable(&self, caller: &Caller, code: Option<QrCode>, id: String) -> AppResult<QrCodeDto> {
        match code {
            Some(code) if caller.has(Scope::CodesReadAny) || code.is_owned_by(&caller.service) => {
                Ok(self.to_dto(&code))
            }
            _ => Err(AppError::not_found(ENTITY, id)),
        }
    }

    fn to_dto(&self, code: &QrCode) -> QrCodeDto {
        let short_code = code.short_code().as_str();
        QrCodeDto {
            id: code.id().into_inner(),
            code: short_code.to_owned(),
            owner: code.owner().to_string(),
            title: code.title().map(ToString::to_string),
            target_url: code.target_url().to_string(),
            metadata: code.metadata().entries().clone(),
            expires_at: code.expires_at(),
            is_active: code.scan_state((self.clock)()) == ScanState::Active,
            scans: code.scans(),
            last_scanned_at: code.last_scanned_at(),
            created: code.audit().created(),
            scan_url: self.links.scan(short_code),
            image_svg_url: self.links.image(short_code, ImageFormat::Svg),
            image_png_url: self.links.image(short_code, ImageFormat::Png),
        }
    }
}

fn require(caller: &Caller, scope: Scope) -> AppResult<()> {
    if caller.has(scope) {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

fn require_read(caller: &Caller) -> AppResult<()> {
    if caller.has(Scope::CodesRead) || caller.has(Scope::CodesReadAny) {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

fn validation(field: &str, message: String) -> AppError {
    AppError::Validation(ValidationErrors::from([(field.to_owned(), vec![message])]))
}

/// Копит ошибки всех полей, чтобы клиент увидел их разом.
fn collect<T>(
    errors: &mut ValidationErrors,
    result: Result<T, template_domain::DomainError>,
) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(template_domain::DomainError::Validation { field, message }) => {
            errors.entry(field.into()).or_default().push(message);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use chrono::Duration;
    use mockall::mock;
    use template_domain::error::RepositoryResult;
    use template_domain::qr_code::ServiceName;
    use template_domain::unit_of_work::Transaction;

    use super::*;
    use crate::qr_codes::RenderError;

    mock! {
        Repo {}

        #[async_trait]
        impl QrCodeRepository for Repo {
            async fn find_by_id(&self, id: QrCodeId) -> RepositoryResult<Option<QrCode>>;
            async fn find_by_short_code(&self, code: &QrCodeShortCode) -> RepositoryResult<Option<QrCode>>;
            async fn list_by_owner(&self, owner: &ServiceName, offset: u64, limit: u64) -> RepositoryResult<Vec<QrCode>>;
            async fn insert(&self, code: &QrCode) -> RepositoryResult<()>;
            async fn delete(&self, id: QrCodeId) -> RepositoryResult<bool>;
            async fn record_scan(&self, id: QrCodeId, at: DateTime<Utc>) -> RepositoryResult<bool>;
        }
    }

    /// UnitOfWork, который пишет вставки в общий список.
    #[derive(Default, Clone)]
    struct RecordingUow(Arc<Mutex<Vec<QrCode>>>);

    struct RecordingTxn(Arc<Mutex<Vec<QrCode>>>);

    struct TxnRepo(Arc<Mutex<Vec<QrCode>>>);

    #[async_trait]
    impl UnitOfWork for RecordingUow {
        async fn begin(&self) -> RepositoryResult<Box<dyn Transaction>> {
            Ok(Box::new(RecordingTxn(self.0.clone())))
        }
    }

    #[async_trait]
    impl Transaction for RecordingTxn {
        fn qr_codes(&self) -> Box<dyn QrCodeRepository + '_> {
            Box::new(TxnRepo(self.0.clone()))
        }

        async fn commit(self: Box<Self>) -> RepositoryResult<()> {
            Ok(())
        }

        async fn rollback(self: Box<Self>) -> RepositoryResult<()> {
            Ok(())
        }
    }

    #[async_trait]
    impl QrCodeRepository for TxnRepo {
        async fn find_by_id(&self, _: QrCodeId) -> RepositoryResult<Option<QrCode>> {
            unimplemented!()
        }
        async fn find_by_short_code(
            &self,
            _: &QrCodeShortCode,
        ) -> RepositoryResult<Option<QrCode>> {
            unimplemented!()
        }
        async fn list_by_owner(
            &self,
            _: &ServiceName,
            _: u64,
            _: u64,
        ) -> RepositoryResult<Vec<QrCode>> {
            unimplemented!()
        }
        async fn insert(&self, code: &QrCode) -> RepositoryResult<()> {
            self.0.lock().unwrap().push(code.clone());
            Ok(())
        }
        async fn delete(&self, _: QrCodeId) -> RepositoryResult<bool> {
            unimplemented!()
        }
        async fn record_scan(&self, _: QrCodeId, _: DateTime<Utc>) -> RepositoryResult<bool> {
            unimplemented!()
        }
    }

    struct FakeRenderer;

    impl QrRenderer for FakeRenderer {
        fn render(
            &self,
            content: &str,
            format: ImageFormat,
            _: u32,
        ) -> Result<RenderedImage, RenderError> {
            Ok(RenderedImage {
                format,
                bytes: content.as_bytes().to_vec(),
            })
        }
    }

    fn service_with(repo: MockRepo, uow: RecordingUow) -> QrCodeService {
        QrCodeService::new(
            Arc::new(repo),
            Arc::new(uow),
            Arc::new(FakeRenderer),
            PublicLinks::new("https://qr.example/"),
        )
    }

    fn service(repo: MockRepo) -> QrCodeService {
        service_with(repo, RecordingUow::default())
    }

    fn caller(name: &str, scopes: &[Scope]) -> Caller {
        Caller::new(ServiceName::parse(name).unwrap(), scopes.iter().copied())
    }

    fn bot() -> Caller {
        caller("aircul-bot", &[Scope::CodesWrite, Scope::CodesRead])
    }

    fn existing(owner: &str, expires_at: Option<DateTime<Utc>>) -> QrCode {
        let short_code = QrCodeShortCode::generate();
        QrCode::new(NewQrCode {
            owner: ServiceName::parse(owner).unwrap(),
            target_url: TargetUrl::from_template("https://t.me/bot?start=mg_{code}", &short_code)
                .unwrap(),
            short_code,
            title: None,
            metadata: Metadata::parse(BTreeMap::from([("game".into(), "minesweeper".into())]))
                .unwrap(),
            expires_at,
        })
    }

    fn request(target: &str) -> CreateQrCode {
        CreateQrCode {
            target_url: target.into(),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn create_substitutes_code_and_builds_links() {
        let mut repo = MockRepo::new();
        repo.expect_insert()
            .withf(|code| code.owner().as_str() == "aircul-bot" && code.scans() == 0)
            .times(1)
            .returning(|_| Ok(()));

        let dto = service(repo)
            .create(&bot(), request("https://t.me/bot?start=qr_{code}"))
            .await
            .unwrap();

        assert_eq!(
            dto.target_url,
            format!("https://t.me/bot?start=qr_{}", dto.code)
        );
        assert_eq!(dto.scan_url, format!("https://qr.example/s/{}", dto.code));
        assert_eq!(
            dto.image_png_url,
            format!("https://qr.example/i/{}.png", dto.code)
        );
        assert!(dto.is_active);
    }

    #[tokio::test]
    async fn create_without_write_scope_is_forbidden() {
        let mut repo = MockRepo::new();
        repo.expect_insert().never();

        let error = service(repo)
            .create(
                &caller("aircul-bot", &[Scope::CodesRead]),
                request("https://example.com"),
            )
            .await
            .unwrap_err();

        assert!(matches!(error, AppError::Forbidden));
    }

    #[tokio::test]
    async fn create_reports_all_invalid_fields() {
        let mut repo = MockRepo::new();
        repo.expect_insert().never();

        let error = service(repo)
            .create(
                &bot(),
                CreateQrCode {
                    target_url: "javascript:alert(1)".into(),
                    title: Some("x".repeat(QrTitle::MAX_LEN + 1)),
                    metadata: BTreeMap::from([("bad key".into(), String::new())]),
                    expires_at: Some(Utc::now() - Duration::minutes(1)),
                },
            )
            .await
            .unwrap_err();

        let AppError::Validation(errors) = error else {
            panic!("expected validation error, got {error:?}");
        };
        for field in ["targetUrl", "title", "metadata", "expiresAt"] {
            assert!(errors.contains_key(field), "missing {field}: {errors:?}");
        }
    }

    #[tokio::test]
    async fn create_batch_inserts_distinct_codes_in_transaction() {
        let mut repo = MockRepo::new();
        repo.expect_insert().never();
        let uow = RecordingUow::default();

        let codes = service_with(repo, uow.clone())
            .create_batch(&bot(), 3, request("https://t.me/bot?start=qr_{code}"))
            .await
            .unwrap();

        assert_eq!(codes.len(), 3);
        assert_eq!(uow.0.lock().unwrap().len(), 3);
        assert_ne!(codes[0].code, codes[1].code);
    }

    #[tokio::test]
    async fn create_batch_rejects_bad_count() {
        let error = service(MockRepo::new())
            .create_batch(
                &bot(),
                QrCodeService::MAX_BATCH + 1,
                request("https://example.com"),
            )
            .await
            .unwrap_err();

        assert!(matches!(error, AppError::Validation(errors) if errors.contains_key("count")));
    }

    #[tokio::test]
    async fn get_foreign_code_is_not_found_without_read_any() {
        let code = existing("aircul-site", None);
        let mut repo = MockRepo::new();
        repo.expect_find_by_short_code()
            .returning(move |_| Ok(Some(code.clone())));
        let service = service(repo);
        let short = QrCodeShortCode::generate();

        let error = service
            .get_by_short_code(&bot(), short.as_str())
            .await
            .unwrap_err();
        assert!(matches!(error, AppError::NotFound { .. }));

        let reader = caller("aircul-bot", &[Scope::CodesReadAny]);
        let dto = service
            .get_by_short_code(&reader, short.as_str())
            .await
            .unwrap();
        assert_eq!(dto.owner, "aircul-site");
        assert_eq!(
            dto.metadata.get("game").map(String::as_str),
            Some("minesweeper")
        );
    }

    #[tokio::test]
    async fn get_without_read_scopes_is_forbidden() {
        let error = service(MockRepo::new())
            .get(&caller("aircul-bot", &[Scope::CodesWrite]), Uuid::now_v7())
            .await
            .unwrap_err();

        assert!(matches!(error, AppError::Forbidden));
    }

    #[tokio::test]
    async fn delete_foreign_code_is_not_found() {
        let code = existing("aircul-site", None);
        let mut repo = MockRepo::new();
        repo.expect_find_by_id()
            .returning(move |_| Ok(Some(code.clone())));
        repo.expect_delete().never();

        let error = service(repo)
            .delete(&bot(), Uuid::now_v7())
            .await
            .unwrap_err();

        assert!(matches!(error, AppError::NotFound { .. }));
    }

    #[tokio::test]
    async fn scan_active_code_counts_and_redirects() {
        let code = existing("aircul-site", None);
        let target = code.target_url().to_string();
        let id = code.id();
        let mut repo = MockRepo::new();
        repo.expect_find_by_short_code()
            .returning(move |_| Ok(Some(code.clone())));
        repo.expect_record_scan()
            .withf(move |scanned, _| *scanned == id)
            .times(1)
            .returning(|_, _| Ok(true));

        let outcome = service(repo)
            .scan(QrCodeShortCode::generate().as_str(), true)
            .await
            .unwrap();

        assert_eq!(outcome, ScanOutcome::Redirect(target));
    }

    #[tokio::test]
    async fn scan_without_counting_does_not_record() {
        let code = existing("aircul-site", None);
        let mut repo = MockRepo::new();
        repo.expect_find_by_short_code()
            .returning(move |_| Ok(Some(code.clone())));
        repo.expect_record_scan().never();

        let outcome = service(repo)
            .scan(QrCodeShortCode::generate().as_str(), false)
            .await
            .unwrap();

        assert!(matches!(outcome, ScanOutcome::Redirect(_)));
    }

    #[tokio::test]
    async fn scan_expired_code_is_not_counted() {
        let now = Utc::now();
        let code = existing("aircul-site", Some(now + Duration::minutes(5)));
        let mut repo = MockRepo::new();
        repo.expect_find_by_short_code()
            .returning(move |_| Ok(Some(code.clone())));
        repo.expect_record_scan().never();

        let outcome = service(repo)
            .with_clock(Arc::new(move || now + Duration::minutes(10)))
            .scan(QrCodeShortCode::generate().as_str(), true)
            .await
            .unwrap();

        assert_eq!(outcome, ScanOutcome::Expired);
    }

    #[tokio::test]
    async fn scan_malformed_code_skips_storage() {
        let mut repo = MockRepo::new();
        repo.expect_find_by_short_code().never();

        assert_eq!(
            service(repo).scan("../etc", true).await.unwrap(),
            ScanOutcome::NotFound
        );
    }

    #[tokio::test]
    async fn image_encodes_scan_url_and_checks_size() {
        let code = existing("aircul-site", None);
        let mut repo = MockRepo::new();
        repo.expect_find_by_short_code()
            .returning(move |_| Ok(Some(code.clone())));
        let service = service(repo);
        let short = QrCodeShortCode::generate();

        let image = service
            .image(short.as_str(), ImageFormat::Svg, None)
            .await
            .unwrap();
        assert_eq!(
            image.bytes,
            format!("https://qr.example/s/{short}").into_bytes()
        );

        let error = service
            .image(short.as_str(), ImageFormat::Png, Some(1))
            .await
            .unwrap_err();
        assert!(matches!(error, AppError::Validation(errors) if errors.contains_key("size")));
    }
}
