//! Аутентификация микросервисов: JWT (HS256), подписанные ключом этого сервиса (`JWT_SECRET`).
//! Токены выпускает сам сервис: `template-web issue-token <service> <scope>...`.

use axum::extract::FromRequestParts;
use axum::http::header;
use axum::http::request::Parts;
use chrono::{Duration, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use template_application::AppError;
use template_application::auth::{Caller, Scope};

use crate::AppState;
use crate::error::ApiError;

/// `iss` и `aud` токенов: токен от другого сервиса с тем же ключом не подойдёт.
pub const ISSUER: &str = "aircul-qr";

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    /// Имя сервиса-клиента: владелец создаваемых кодов.
    pub sub: String,
    pub iss: String,
    pub aud: String,
    pub iat: i64,
    pub exp: i64,
    /// Права через пробел: `codes:write codes:read codes:read:any`.
    #[serde(default)]
    pub scope: String,
}

pub struct JwtAuth {
    encoding: EncodingKey,
    decoding: DecodingKey,
    validation: Validation,
}

impl JwtAuth {
    pub fn new(secret: &[u8]) -> Self {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_issuer(&[ISSUER]);
        validation.set_audience(&[ISSUER]);
        validation.set_required_spec_claims(&["exp", "sub", "iss", "aud"]);
        Self {
            encoding: EncodingKey::from_secret(secret),
            decoding: DecodingKey::from_secret(secret),
            validation,
        }
    }

    pub fn issue(
        &self,
        service: &str,
        scopes: &[Scope],
        ttl: Duration,
    ) -> Result<String, jsonwebtoken::errors::Error> {
        let now = Utc::now();
        let claims = Claims {
            sub: service.to_owned(),
            iss: ISSUER.to_owned(),
            aud: ISSUER.to_owned(),
            iat: now.timestamp(),
            exp: (now + ttl).timestamp(),
            scope: scopes
                .iter()
                .map(|scope| scope.as_str())
                .collect::<Vec<_>>()
                .join(" "),
        };
        encode(&Header::new(Algorithm::HS256), &claims, &self.encoding)
    }

    /// `None` — подпись, срок, `iss`/`aud` или имя сервиса не прошли проверку.
    pub fn verify(&self, token: &str) -> Option<Caller> {
        let data = decode::<Claims>(token, &self.decoding, &self.validation).ok()?;
        Caller::from_claims(&data.claims.sub, &data.claims.scope)
    }
}

/// Экстрактор: `Authorization: Bearer <jwt>`, иначе 401.
pub struct Authenticated(pub Caller);

impl FromRequestParts<AppState> for Authenticated {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| {
                value
                    .strip_prefix("Bearer ")
                    .or_else(|| value.strip_prefix("bearer "))
            })
            .map(str::trim)
            .ok_or(AppError::Unauthorized)?;
        let caller = state.auth.verify(token).ok_or(AppError::Unauthorized)?;
        Ok(Self(caller))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &[u8] = b"0123456789abcdef0123456789abcdef";

    #[test]
    fn issued_token_round_trips() {
        let auth = JwtAuth::new(SECRET);
        let token = auth
            .issue(
                "aircul-bot",
                &[Scope::CodesWrite, Scope::CodesRead],
                Duration::days(1),
            )
            .unwrap();

        let caller = auth.verify(&token).unwrap();

        assert_eq!(caller.service.as_str(), "aircul-bot");
        assert!(caller.has(Scope::CodesWrite) && caller.has(Scope::CodesRead));
        assert!(!caller.has(Scope::CodesReadAny));
    }

    #[test]
    fn rejects_foreign_key_expired_and_bad_subject() {
        let auth = JwtAuth::new(SECRET);
        let other = JwtAuth::new(b"another-secret-another-secret-xx");

        let foreign = other.issue("aircul-bot", &[], Duration::days(1)).unwrap();
        assert!(auth.verify(&foreign).is_none());

        let expired = auth.issue("aircul-bot", &[], Duration::days(-1)).unwrap();
        assert!(auth.verify(&expired).is_none());

        let bad_subject = auth.issue("Not A Service", &[], Duration::days(1)).unwrap();
        assert!(auth.verify(&bad_subject).is_none());

        assert!(auth.verify("garbage").is_none());
    }

    #[test]
    fn rejects_wrong_audience() {
        let key = EncodingKey::from_secret(SECRET);
        let claims = Claims {
            sub: "aircul-bot".into(),
            iss: ISSUER.into(),
            aud: "someone-else".into(),
            iat: Utc::now().timestamp(),
            exp: (Utc::now() + Duration::days(1)).timestamp(),
            scope: "codes:write".into(),
        };
        let token = encode(&Header::new(Algorithm::HS256), &claims, &key).unwrap();

        assert!(JwtAuth::new(SECRET).verify(&token).is_none());
    }
}
