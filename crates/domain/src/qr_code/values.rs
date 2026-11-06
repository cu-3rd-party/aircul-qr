use std::collections::BTreeMap;
use std::fmt;

use crate::DomainError;

/// Подпись кода для людей («Холл, 3 этаж»): непустая, без пробелов по краям,
/// не длиннее [`QrTitle::MAX_LEN`] символов.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QrTitle(String);

impl QrTitle {
    pub const MAX_LEN: usize = 200;

    pub fn parse(value: impl AsRef<str>) -> Result<Self, DomainError> {
        let value = value.as_ref().trim();
        if value.is_empty() {
            return Err(DomainError::validation("title", "must not be empty"));
        }
        if value.chars().count() > Self::MAX_LEN {
            return Err(DomainError::validation(
                "title",
                format!("must be at most {} characters", Self::MAX_LEN),
            ));
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for QrTitle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Публичный короткий код: часть ссылки скана `/s/{code}`. Случайный base62, чтобы коды
/// нельзя было перебрать; без `_` и `-`, поэтому годится в payload deep link'а Telegram.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QrCodeShortCode(String);

impl QrCodeShortCode {
    /// 12 символов base62 — около 71 бита случайности.
    pub const LEN: usize = 12;
    const ALPHABET: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

    pub fn generate() -> Self {
        // 248 = 62 * 4: байты выше отбрасываются, чтобы символы были равновероятны
        let mut code = String::with_capacity(Self::LEN);
        while code.len() < Self::LEN {
            let byte: u8 = rand::random();
            if byte < 248 {
                code.push(Self::ALPHABET[usize::from(byte % 62)] as char);
            }
        }
        Self(code)
    }

    pub fn parse(value: impl AsRef<str>) -> Result<Self, DomainError> {
        let value = value.as_ref();
        if value.len() != Self::LEN || !value.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
            return Err(DomainError::validation(
                "code",
                format!("must be {} latin letters or digits", Self::LEN),
            ));
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for QrCodeShortCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Имя микросервиса-владельца (`sub` его JWT): `aircul-bot`, `aircul-site`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ServiceName(String);

impl ServiceName {
    pub const MAX_LEN: usize = 64;

    pub fn parse(value: impl AsRef<str>) -> Result<Self, DomainError> {
        let value = value.as_ref();
        let valid_chars = value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
        });
        if value.is_empty() || value.len() > Self::MAX_LEN || !valid_chars {
            return Err(DomainError::validation(
                "owner",
                format!(
                    "must be 1..{} characters: lowercase latin letters, digits, '-' or '_'",
                    Self::MAX_LEN
                ),
            ));
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ServiceName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Куда ведёт скан: абсолютный http(s) URL. В шаблоне можно использовать
/// [`TargetUrl::CODE_PLACEHOLDER`] — он заменяется коротким кодом при создании
/// (`https://t.me/bot?start=qr_{code}`), так получатель редиректа узнаёт, какой код сканировали.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetUrl(String);

impl TargetUrl {
    pub const MAX_LEN: usize = 2048;
    pub const CODE_PLACEHOLDER: &str = "{code}";

    /// Шаблон с подстановкой кода (при создании).
    pub fn from_template(
        template: impl AsRef<str>,
        code: &QrCodeShortCode,
    ) -> Result<Self, DomainError> {
        Self::parse(
            template
                .as_ref()
                .trim()
                .replace(Self::CODE_PLACEHOLDER, code.as_str()),
        )
    }

    /// Готовый URL (восстановление из хранилища).
    pub fn parse(value: impl AsRef<str>) -> Result<Self, DomainError> {
        let value = value.as_ref().trim();
        let lower = value.to_ascii_lowercase();
        let rest = lower
            .strip_prefix("https://")
            .or_else(|| lower.strip_prefix("http://"));
        let Some(rest) = rest else {
            return Err(DomainError::validation(
                "targetUrl",
                "must be an absolute http(s) URL",
            ));
        };
        if rest.is_empty() || rest.starts_with('/') {
            return Err(DomainError::validation("targetUrl", "must contain a host"));
        }
        if value.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(DomainError::validation(
                "targetUrl",
                "must not contain whitespace",
            ));
        }
        if value.len() > Self::MAX_LEN {
            return Err(DomainError::validation(
                "targetUrl",
                format!("must be at most {} bytes", Self::MAX_LEN),
            ));
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TargetUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Произвольные строковые метки владельца (`{"kind": "minigame", "game": "minesweeper"}`).
/// Сервис их не интерпретирует: хранит и отдаёт тем, кто читает код.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Metadata(BTreeMap<String, String>);

impl Metadata {
    pub const MAX_ENTRIES: usize = 16;
    pub const MAX_KEY_LEN: usize = 32;
    pub const MAX_VALUE_LEN: usize = 256;

    pub fn parse(entries: BTreeMap<String, String>) -> Result<Self, DomainError> {
        if entries.len() > Self::MAX_ENTRIES {
            return Err(DomainError::validation(
                "metadata",
                format!("must have at most {} entries", Self::MAX_ENTRIES),
            ));
        }
        for (key, value) in &entries {
            let mut bytes = key.bytes();
            let valid_key = bytes
                .next()
                .is_some_and(|first| first.is_ascii_alphabetic())
                && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                && key.len() <= Self::MAX_KEY_LEN;
            if !valid_key {
                return Err(DomainError::validation(
                    "metadata",
                    format!(
                        "key {key:?} must start with a latin letter, contain only latin letters, digits or '_' \
                         and be at most {} characters",
                        Self::MAX_KEY_LEN
                    ),
                ));
            }
            if value.chars().count() > Self::MAX_VALUE_LEN {
                return Err(DomainError::validation(
                    "metadata",
                    format!(
                        "value of {key:?} must be at most {} characters",
                        Self::MAX_VALUE_LEN
                    ),
                ));
            }
        }
        Ok(Self(entries))
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    pub fn entries(&self) -> &BTreeMap<String, String> {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_code_is_parseable_and_random() {
        let a = QrCodeShortCode::generate();
        let b = QrCodeShortCode::generate();

        assert_eq!(QrCodeShortCode::parse(a.as_str()).unwrap(), a);
        assert_eq!(a.as_str().len(), QrCodeShortCode::LEN);
        assert_ne!(a, b);
    }

    #[test]
    fn short_code_rejects_wrong_alphabet_and_length() {
        assert!(QrCodeShortCode::parse("abc").is_err());
        assert!(QrCodeShortCode::parse("abcdefghij_-").is_err());
        assert!(QrCodeShortCode::parse("abcdefghijkl").is_ok());
    }

    #[test]
    fn service_name_rules() {
        assert!(ServiceName::parse("aircul-bot").is_ok());
        assert!(ServiceName::parse("").is_err());
        assert!(ServiceName::parse("Aircul").is_err());
        assert!(ServiceName::parse("a".repeat(65)).is_err());
    }

    #[test]
    fn target_url_substitutes_code() {
        let code = QrCodeShortCode::parse("abcdefghijkl").unwrap();

        let url = TargetUrl::from_template(" https://t.me/bot?start=qr_{code} ", &code).unwrap();

        assert_eq!(url.as_str(), "https://t.me/bot?start=qr_abcdefghijkl");
    }

    #[test]
    fn target_url_rejects_non_http() {
        assert!(TargetUrl::parse("javascript:alert(1)").is_err());
        assert!(TargetUrl::parse("ftp://example.com").is_err());
        assert!(TargetUrl::parse("https://").is_err());
        assert!(TargetUrl::parse("https:///path").is_err());
        assert!(TargetUrl::parse("https://exa mple.com").is_err());
        assert!(TargetUrl::parse("HTTPS://example.com/x").is_ok());
    }

    #[test]
    fn metadata_limits() {
        let ok = BTreeMap::from([("game".to_owned(), "minesweeper".to_owned())]);
        assert_eq!(
            Metadata::parse(ok).unwrap().get("game"),
            Some("minesweeper")
        );

        let bad_key = BTreeMap::from([("1game".to_owned(), String::new())]);
        assert!(Metadata::parse(bad_key).is_err());

        let long_value =
            BTreeMap::from([("k".to_owned(), "x".repeat(Metadata::MAX_VALUE_LEN + 1))]);
        assert!(Metadata::parse(long_value).is_err());

        let many = (0..=Metadata::MAX_ENTRIES)
            .map(|i| (format!("k{i}"), String::new()))
            .collect();
        assert!(Metadata::parse(many).is_err());
    }
}
