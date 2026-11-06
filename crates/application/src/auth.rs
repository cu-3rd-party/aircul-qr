//! Кто вызывает API: микросервис из проверенного JWT (проверка подписи — в web-слое).

use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

use template_domain::qr_code::ServiceName;

/// Право из claim'а `scope` (через пробел, как в OAuth 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Scope {
    /// Создавать и удалять свои коды.
    CodesWrite,
    /// Читать свои коды (список, по id, по короткому коду).
    CodesRead,
    /// Читать любые коды: бот читает метки кодов, выданных сайтом.
    CodesReadAny,
}

impl Scope {
    pub const ALL: [Scope; 3] = [Scope::CodesWrite, Scope::CodesRead, Scope::CodesReadAny];

    pub fn as_str(self) -> &'static str {
        match self {
            Scope::CodesWrite => "codes:write",
            Scope::CodesRead => "codes:read",
            Scope::CodesReadAny => "codes:read:any",
        }
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Scope {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|scope| scope.as_str() == value)
            .ok_or_else(|| format!("unknown scope {value:?}"))
    }
}

/// Аутентифицированный вызывающий сервис.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caller {
    pub service: ServiceName,
    pub scopes: BTreeSet<Scope>,
}

impl Caller {
    pub fn new(service: ServiceName, scopes: impl IntoIterator<Item = Scope>) -> Self {
        Self {
            service,
            scopes: scopes.into_iter().collect(),
        }
    }

    /// Неизвестные права игнорируются: старый сервис принимает токены с новыми правами.
    pub fn from_claims(subject: &str, scope: &str) -> Option<Self> {
        let service = ServiceName::parse(subject).ok()?;
        Some(Self::new(
            service,
            scope.split_whitespace().filter_map(|s| s.parse().ok()),
        ))
    }

    pub fn has(&self, scope: Scope) -> bool {
        self.scopes.contains(&scope)
    }
}
