# AirCul модуль QR кодов

Микросервис QR-кодов для остальных сервисов AirCul (бот «Централь», сайт):

- **генерирует** коды по запросу микросервисов (по одному или пачкой) и **отдаёт картинки** (PNG/SVG);
- **считает сканы**: каждый скан идёт через `GET /s/{code}`, засчитывается и **перенаправляет** (302)
  на `targetUrl` кода;
- **проверяет, что запросы идут от микросервисов**: API принимает только JWT (HS256), подписанные ключом
  этого сервиса (`JWT_SECRET`).

## Как это работает

```
сервис ──JWT──▶ POST /api/v1/qr-codes {targetUrl: "https://t.me/aircul_bot?start=mg_{code}", metadata}
               ◀── {code, scanUrl, imagePngUrl, imageSvgUrl, ...}
человек ─скан─▶ GET /s/{code}  → scans += 1 → 302 Location: https://t.me/aircul_bot?start=mg_{code}
```

- `{code}` в `targetUrl` заменяется коротким кодом (12 символов base62, ~71 бит случайности) — так получатель
  редиректа узнаёт, какой код сканировали, и может прочитать его `metadata` через API.
- Код принадлежит сервису, который его создал (`owner` = `sub` токена). Чужие коды не видны, если у токена нет
  права `codes:read:any`; удалить чужой код нельзя.
- `expiresAt`: после него скан не засчитывается и не перенаправляет (410).
- `HEAD /s/{code}` перенаправляет, но не засчитывается (префетчеры ссылок).
- `metadata` — до 16 строковых меток, сервис их не интерпретирует.

| Метод                              | Право                                   | Что делает                         |
|------------------------------------|-----------------------------------------|------------------------------------|
| `POST /api/v1/qr-codes`            | `codes:write`                           | создать код                        |
| `POST /api/v1/qr-codes/batch`      | `codes:write`                           | `count` кодов (1..500) атомарно    |
| `GET /api/v1/qr-codes`             | `codes:read`                            | свои коды, `?offset=&limit=`       |
| `GET /api/v1/qr-codes/{id}`        | `codes:read` / `codes:read:any`         | код по id                          |
| `GET /api/v1/qr-codes/by-code/{c}` | `codes:read` / `codes:read:any`         | код по короткому коду              |
| `DELETE /api/v1/qr-codes/{id}`     | `codes:write`                           | удалить свой код                   |
| `GET /s/{code}`                    | —                                       | скан: +1 и 302 на `targetUrl`      |
| `GET /i/{code}.png` / `.svg`       | —                                       | картинка, `?size=64..4096` (512)   |

Полная схема — `/scalar`, `/openapi/v1.json`. Ошибки — RFC 9457 ProblemDetails.

## Токены микросервисов

Токены выпускает сам сервис тем же ключом, которым проверяет:

```bash
JWT_SECRET=... cargo run -p template-web -- issue-token aircul-bot codes:write codes:read codes:read:any
JWT_SECRET=... cargo run -p template-web -- issue-token aircul-site codes:write codes:read --ttl-days 90
# в контейнере: /app/template-web issue-token <service> <scope>...
```

Claims: `sub` — имя сервиса (`[a-z0-9_-]`, оно же `owner` кодов), `iss` = `aud` = `aircul-qr`, `exp` обязателен
(по умолчанию 365 дней), `scope` — права через пробел. Отозвать все токены — сменить `JWT_SECRET`.

Кому что нужно:

- **aircul-bot** — `codes:write codes:read codes:read:any`: печатает коды кампуса и читает метки кодов сайта
  (какая мини-игра пройдена).
- **aircul-site** — `codes:write codes:read`: выдаёт код после прохождения мини-игры.

## Запуск

```bash
cp .env.example .env                 # JWT_SECRET: openssl rand -base64 48
docker compose up -d db
cargo run -p template-web            # http://localhost:5213/scalar, health — /health
```

Переменные: `DATABASE_URL` (по умолч. `postgres://admin:password@localhost:5432/db`), `LISTEN_ADDR`
(`0.0.0.0:5213`, в Docker `0.0.0.0:8080`), `PUBLIC_BASE_URL` (внешний адрес, из него ссылки в QR;
по умолч. `http://localhost:5213`), `JWT_SECRET` (обязателен, ≥ 32 байт), `RUST_LOG`. Локально читается `.env`.
Запросы для IDE — `crates/web/api.http`.

## Добавить фичу

1. **domain**: `src/foo/` — `entity.rs` (агрегат + `FooId`), value objects, `repository.rs` (`trait FooRepository`);
   при нужде — метод `foos()` в `unit_of_work::Transaction`.
2. **migration**: `cargo run -p template-migration -- generate create_foo`, описать таблицу (snake_case, ед. число),
   добавить в `Migrator::migrations`.
3. **infrastructure**: `persistence/entities/foo.rs` (SeaORM-модель), `persistence/repositories/foo_repository.rs`
   (`SeaFooRepository<C>` + маппинг), поле в `Infrastructure`.
4. **application**: `src/foo/` — `service.rs` (`FooService`), `dto.rs`; юнит-тесты с моком репозитория.
5. **web**: `routes/v1/foo.rs` (модели, хендлеры с `#[utoipa::path]`, `router()`), строка `.nest("/foos", …)`
   в `routes/v1/mod.rs`, сервис в `AppState`. Защищённый хендлер берёт `Authenticated(caller)`.
6. Интеграционный тест в `crates/web/tests/foo.rs` (`TestApp::spawn()`, токен — `app.token(...)`).

## Интеграционные тесты

`template_test_support::TestDatabase::create()` — отдельная пустая мигрированная БД на тест.
Сервер PostgreSQL — один на тестовый бинарь:

- задан `TEST_DATABASE_URL` → используется он, контейнеров нет (так в CI: `services: postgres`);
- иначе → один контейнер Testcontainers (нужен Docker), удаляется при выходе из бинаря.

При первом обращении в сервере создаётся БД-шаблон с применёнными миграциями, дальше каждый тест получает копию
(`CREATE DATABASE ... TEMPLATE`, единицы миллисекунд). Тесты изолированы и идут параллельно, сбрасывать БД не нужно.
На внешнем сервере созданные БД удаляются в конце прогона.

```bash
cargo test --workspace                                         # контейнер на бинарь
docker compose up -d db && TEST_DATABASE_URL=postgres://admin:password@localhost:5432/db cargo test --workspace
```

## Миграции

Применяются автоматически при старте `web`. Вручную (берёт `DATABASE_URL`):

```bash
cargo run -p template-migration -- status
cargo run -p template-migration -- up
cargo run -p template-migration -- down
```

## Docker

Контекст сборки — корень репозитория (`docker build .`), фильтр — `Dockerfile.dockerignore`.
Зависимости кэшируются отдельным слоем через cargo-chef; рантайм — distroless (`cc-debian13:nonroot`).
