//! Тестовое приложение: своя мигрированная БД на тест (см. `template-test-support`), запросы через `oneshot`.

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::Value;
use template_application::auth::Scope;
use template_infrastructure::{Infrastructure, InfrastructureConfig};
use template_test_support::TestDatabase;
use template_web::auth::JwtAuth;
use template_web::config::AppSettings;
use template_web::{AppState, app};
use tower::ServiceExt;

pub const BASE_URL: &str = "https://qr.test";
const SECRET: &[u8] = b"test-secret-test-secret-test-secret!";

pub struct TestApp {
    router: Router,
    auth: JwtAuth,
}

pub struct TestResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub bytes: Vec<u8>,
    pub body: Value,
}

impl TestResponse {
    pub fn header(&self, name: header::HeaderName) -> Option<&str> {
        self.headers.get(name).map(|value| value.to_str().unwrap())
    }
}

impl TestApp {
    /// Мигрированная пустая БД.
    pub async fn spawn() -> Self {
        let db = TestDatabase::create().await;
        let config = InfrastructureConfig {
            database_url: db.url,
            initialise_database: false,
        };
        let infrastructure = Infrastructure::init(&config).await.unwrap();
        let settings = AppSettings {
            public_base_url: BASE_URL.into(),
            jwt_secret: SECRET.to_vec(),
        };

        Self {
            router: app(AppState::new(infrastructure, &settings)),
            auth: JwtAuth::new(SECRET),
        }
    }

    pub fn token(&self, service: &str, scopes: &[Scope]) -> String {
        self.auth
            .issue(service, scopes, chrono::Duration::hours(1))
            .unwrap()
    }

    pub async fn request(
        &self,
        method: Method,
        uri: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> TestResponse {
        let mut request = Request::builder().method(method).uri(uri);
        if let Some(token) = token {
            request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        let body = match body {
            Some(json) => {
                request = request.header(header::CONTENT_TYPE, "application/json");
                Body::from(json.to_string())
            }
            None => Body::empty(),
        };

        let response = self
            .router
            .clone()
            .oneshot(request.body(body).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec();
        let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);

        TestResponse {
            status,
            headers,
            bytes,
            body,
        }
    }
}
