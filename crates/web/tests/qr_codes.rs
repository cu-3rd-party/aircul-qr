mod common;

use axum::http::{Method, StatusCode, header};
use common::{BASE_URL, TestApp};
use serde_json::json;
use template_application::auth::Scope;
use uuid::Uuid;

const URL: &str = "/api/v1/qr-codes";

fn rw() -> [Scope; 2] {
    [Scope::CodesWrite, Scope::CodesRead]
}

#[tokio::test]
async fn requests_without_valid_token_are_401() {
    let app = TestApp::spawn().await;

    let missing = app.request(Method::GET, URL, None, None).await;
    assert_eq!(missing.status, StatusCode::UNAUTHORIZED);
    assert_eq!(missing.header(header::WWW_AUTHENTICATE), Some("Bearer"));
    assert_eq!(
        missing.header(header::CONTENT_TYPE),
        Some("application/problem+json")
    );

    let garbage = app.request(Method::GET, URL, Some("not-a-jwt"), None).await;
    assert_eq!(garbage.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn missing_scope_is_403() {
    let app = TestApp::spawn().await;
    let reader = app.token("aircul-bot", &[Scope::CodesRead]);

    let response = app
        .request(
            Method::POST,
            URL,
            Some(&reader),
            Some(json!({ "targetUrl": "https://example.com" })),
        )
        .await;

    assert_eq!(response.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn create_scan_redirects_and_counts() {
    let app = TestApp::spawn().await;
    let token = app.token("aircul-site", &rw());

    let created = app
        .request(
            Method::POST,
            URL,
            Some(&token),
            Some(json!({
                "targetUrl": "https://t.me/aircul_bot?start=mg_{code}",
                "title": "Сапёр",
                "metadata": { "kind": "minigame", "game": "minesweeper" },
            })),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.body);
    let code = created.body["code"].as_str().unwrap().to_owned();
    let id = created.body["id"].as_str().unwrap().to_owned();
    assert_eq!(created.body["owner"], "aircul-site");
    assert_eq!(created.body["scanUrl"], format!("{BASE_URL}/s/{code}"));
    assert_eq!(
        created.body["targetUrl"],
        format!("https://t.me/aircul_bot?start=mg_{code}")
    );
    assert_eq!(
        created.header(header::LOCATION),
        Some(format!("{URL}/{id}").as_str())
    );

    for _ in 0..2 {
        let scan = app
            .request(Method::GET, &format!("/s/{code}"), None, None)
            .await;
        assert_eq!(scan.status, StatusCode::FOUND);
        assert_eq!(
            scan.header(header::LOCATION),
            Some(format!("https://t.me/aircul_bot?start=mg_{code}").as_str())
        );
        assert_eq!(scan.header(header::CACHE_CONTROL), Some("no-store"));
    }
    // HEAD (префетчеры) не считается
    let head = app
        .request(Method::HEAD, &format!("/s/{code}"), None, None)
        .await;
    assert_eq!(head.status, StatusCode::FOUND);

    let fetched = app
        .request(Method::GET, &format!("{URL}/{id}"), Some(&token), None)
        .await;
    assert_eq!(fetched.status, StatusCode::OK);
    assert_eq!(fetched.body["scans"], 2);
    assert!(fetched.body["lastScannedAt"].is_string());
    assert_eq!(fetched.body["metadata"]["game"], "minesweeper");
}

#[tokio::test]
async fn unknown_scan_is_404_page() {
    let app = TestApp::spawn().await;

    let response = app
        .request(Method::GET, "/s/AAAAAAAAAAAA", None, None)
        .await;

    assert_eq!(response.status, StatusCode::NOT_FOUND);
    assert!(
        response
            .header(header::CONTENT_TYPE)
            .unwrap()
            .starts_with("text/html")
    );
}

#[tokio::test]
async fn invalid_create_is_400_with_field_errors() {
    let app = TestApp::spawn().await;
    let token = app.token("aircul-bot", &rw());

    let response = app
        .request(
            Method::POST,
            URL,
            Some(&token),
            Some(
                json!({ "targetUrl": "javascript:alert(1)", "expiresAt": "2000-01-01T00:00:00Z" }),
            ),
        )
        .await;

    assert_eq!(response.status, StatusCode::BAD_REQUEST);
    assert!(response.body["errors"]["targetUrl"].is_array());
    assert!(response.body["errors"]["expiresAt"].is_array());
}

#[tokio::test]
async fn batch_creates_distinct_codes_and_list_is_per_owner() {
    let app = TestApp::spawn().await;
    let bot = app.token("aircul-bot", &rw());
    let site = app.token("aircul-site", &rw());

    let batch = app
        .request(
            Method::POST,
            &format!("{URL}/batch"),
            Some(&bot),
            Some(json!({ "count": 3, "targetUrl": "https://t.me/aircul_bot?start=qr_{code}", "title": "Кампус" })),
        )
        .await;
    assert_eq!(batch.status, StatusCode::CREATED, "{:?}", batch.body);
    let codes = batch.body.as_array().unwrap();
    assert_eq!(codes.len(), 3);
    assert_ne!(codes[0]["code"], codes[1]["code"]);
    app.request(
        Method::POST,
        URL,
        Some(&site),
        Some(json!({ "targetUrl": "https://example.com" })),
    )
    .await;

    let bot_list = app.request(Method::GET, URL, Some(&bot), None).await;
    assert_eq!(bot_list.body.as_array().unwrap().len(), 3);
    let page = app
        .request(
            Method::GET,
            &format!("{URL}?offset=2&limit=5"),
            Some(&bot),
            None,
        )
        .await;
    assert_eq!(page.body.as_array().unwrap().len(), 1);
    assert_eq!(page.body[0]["code"], codes[2]["code"]);
    let site_list = app.request(Method::GET, URL, Some(&site), None).await;
    assert_eq!(site_list.body.as_array().unwrap().len(), 1);

    let too_many = app
        .request(
            Method::POST,
            &format!("{URL}/batch"),
            Some(&bot),
            Some(json!({ "count": 501, "targetUrl": "https://example.com" })),
        )
        .await;
    assert_eq!(too_many.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn foreign_codes_need_read_any() {
    let app = TestApp::spawn().await;
    let site = app.token("aircul-site", &rw());
    let bot = app.token("aircul-bot", &rw());
    let bot_reader = app.token("aircul-bot", &[Scope::CodesReadAny]);
    let created = app
        .request(
            Method::POST,
            URL,
            Some(&site),
            Some(json!({ "targetUrl": "https://t.me/aircul_bot?start=mg_{code}", "metadata": { "game": "hex_patch" } })),
        )
        .await;
    let code = created.body["code"].as_str().unwrap();
    let id = created.body["id"].as_str().unwrap();
    let by_code = format!("{URL}/by-code/{code}");

    assert_eq!(
        app.request(Method::GET, &by_code, Some(&bot), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    let read = app
        .request(Method::GET, &by_code, Some(&bot_reader), None)
        .await;
    assert_eq!(read.status, StatusCode::OK);
    assert_eq!(read.body["owner"], "aircul-site");
    assert_eq!(read.body["metadata"]["game"], "hex_patch");

    // удалить чужой нельзя даже с read:any
    let delete = app
        .request(Method::DELETE, &format!("{URL}/{id}"), Some(&bot), None)
        .await;
    assert_eq!(delete.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_removes_code_and_scan() {
    let app = TestApp::spawn().await;
    let token = app.token("aircul-bot", &rw());
    let created = app
        .request(
            Method::POST,
            URL,
            Some(&token),
            Some(json!({ "targetUrl": "https://example.com" })),
        )
        .await;
    let id = created.body["id"].as_str().unwrap();
    let code = created.body["code"].as_str().unwrap();

    let url = format!("{URL}/{id}");
    assert_eq!(
        app.request(Method::DELETE, &url, Some(&token), None)
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        app.request(Method::GET, &url, Some(&token), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.request(Method::GET, &format!("/s/{code}"), None, None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.request(
            Method::GET,
            &format!("{URL}/{}", Uuid::now_v7()),
            Some(&token),
            None
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn images_are_public() {
    let app = TestApp::spawn().await;
    let token = app.token("aircul-bot", &rw());
    let created = app
        .request(
            Method::POST,
            URL,
            Some(&token),
            Some(json!({ "targetUrl": "https://example.com" })),
        )
        .await;
    let code = created.body["code"].as_str().unwrap();

    let png = app
        .request(Method::GET, &format!("/i/{code}.png?size=128"), None, None)
        .await;
    assert_eq!(png.status, StatusCode::OK);
    assert_eq!(png.header(header::CONTENT_TYPE), Some("image/png"));
    assert!(png.bytes.starts_with(b"\x89PNG"));

    let svg = app
        .request(Method::GET, &format!("/i/{code}.svg"), None, None)
        .await;
    assert_eq!(svg.header(header::CONTENT_TYPE), Some("image/svg+xml"));

    assert_eq!(
        app.request(Method::GET, &format!("/i/{code}.gif"), None, None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.request(Method::GET, "/i/AAAAAAAAAAAA.png", None, None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.request(Method::GET, &format!("/i/{code}.png?size=10"), None, None)
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn health_and_openapi_are_served() {
    let app = TestApp::spawn().await;

    assert_eq!(
        app.request(Method::GET, "/health", None, None).await.status,
        StatusCode::OK
    );
    let openapi = app
        .request(Method::GET, "/openapi/v1.json", None, None)
        .await;
    assert_eq!(openapi.status, StatusCode::OK);
    assert!(openapi.body["paths"]["/api/v1/qr-codes/{id}"].is_object());
    assert!(openapi.body["paths"]["/s/{code}"].is_object());
    assert!(openapi.body["components"]["securitySchemes"]["bearer"].is_object());
}
