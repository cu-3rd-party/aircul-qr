use std::env;
use std::net::SocketAddr;

use anyhow::{Context, ensure};
use template_infrastructure::InfrastructureConfig;

/// Настройки из переменных окружения (+ `.env` при локальном запуске).
#[derive(Debug, Clone)]
pub struct Config {
    /// `LISTEN_ADDR`, по умолчанию `0.0.0.0:5213`.
    pub listen_addr: SocketAddr,
    /// `DATABASE_URL`.
    pub database_url: String,
    pub app: AppSettings,
}

/// То, что нужно `AppState` (в тестах собирается руками).
#[derive(Clone)]
pub struct AppSettings {
    /// `PUBLIC_BASE_URL`: внешний адрес сервиса, из него строятся ссылки в QR (`{base}/s/{code}`).
    pub public_base_url: String,
    /// `JWT_SECRET`: ключ HS256, которым сервис подписывает и проверяет токены микросервисов.
    pub jwt_secret: Vec<u8>,
}

impl std::fmt::Debug for AppSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppSettings")
            .field("public_base_url", &self.public_base_url)
            .field("jwt_secret", &"<redacted>")
            .finish()
    }
}

impl Config {
    pub const DEFAULT_DATABASE_URL: &str = "postgres://admin:password@localhost:5432/db";
    pub const DEFAULT_LISTEN_ADDR: &str = "0.0.0.0:5213";
    pub const DEFAULT_PUBLIC_BASE_URL: &str = "http://localhost:5213";
    /// HS256: ключ не короче выхода SHA-256.
    pub const MIN_JWT_SECRET_LEN: usize = 32;

    pub fn from_env() -> anyhow::Result<Self> {
        let listen_addr =
            env::var("LISTEN_ADDR").unwrap_or_else(|_| Self::DEFAULT_LISTEN_ADDR.into());
        Ok(Self {
            listen_addr: listen_addr
                .parse()
                .with_context(|| format!("invalid LISTEN_ADDR: {listen_addr}"))?,
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| Self::DEFAULT_DATABASE_URL.into()),
            app: AppSettings::from_env()?,
        })
    }

    pub fn infrastructure(&self) -> InfrastructureConfig {
        InfrastructureConfig {
            database_url: self.database_url.clone(),
            initialise_database: true,
        }
    }
}

impl AppSettings {
    pub fn from_env() -> anyhow::Result<Self> {
        let public_base_url =
            env::var("PUBLIC_BASE_URL").unwrap_or_else(|_| Config::DEFAULT_PUBLIC_BASE_URL.into());
        ensure!(
            public_base_url.starts_with("https://") || public_base_url.starts_with("http://"),
            "PUBLIC_BASE_URL must be an absolute http(s) URL, got {public_base_url:?}"
        );
        let jwt_secret = env::var("JWT_SECRET")
            .context("JWT_SECRET is not set")?
            .into_bytes();
        ensure!(
            jwt_secret.len() >= Config::MIN_JWT_SECRET_LEN,
            "JWT_SECRET must be at least {} bytes",
            Config::MIN_JWT_SECRET_LEN
        );
        Ok(Self {
            public_base_url,
            jwt_secret,
        })
    }
}
