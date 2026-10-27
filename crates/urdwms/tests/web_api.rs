#![cfg(feature = "server")]
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::ServiceExt;
use urdwms::{seed, service::Store, web};

fn app(demo: bool) -> (tempfile::TempDir, Router) {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(dir.path()).unwrap();
    if demo {
        seed::demo(&s.conn).unwrap();
    }
    seed::first_run(&s.conn).unwrap();
    (dir, web::router(s.data_dir.clone()))
}
async fn req(app: &Router, method: &str, path: &str, token: Option<&str>, body: Value) -> (StatusCode, Value) {
    let mut r = Request::builder().method(method).uri(path).header("content-type", "application/json");
    if let Some(t) = token {
        r = r.header("authorization", format!("Bearer {t}"));
    }
    let r = r.body(Body::from(body.to_string())).unwrap();
    let response = app.clone().oneshot(r).await.unwrap();
    let status = response.status();
    let data = to_bytes(response.into_body(), 16 * 1024 * 1024).await.unwrap();
    (status, serde_json::from_slice(&data).unwrap_or(Value::Null))
}
async fn login(app: &Router, name: &str, password: &str) -> String {
    let (code, data) = req(app, "POST", "/api/auth/login", None, json!({"username":name,"password":password})).await;
    assert_eq!(code, StatusCode::OK, "{data}");
    data["token"].as_str().unwrap().to_owned()
}
#[tokio::test]
async fn authentication_logout_and_expiry() {
    let (dir, app) = app(true);
    assert_eq!(req(&app, "GET", "/api/health", None, Value::Null).await.0, StatusCode::OK);
    assert_eq!(req(&app, "GET", "/api/items", None, Value::Null).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(
        req(&app, "POST", "/api/auth/login", None, json!({"username":"peter","password":"wrong"})).await.0,
        StatusCode::UNAUTHORIZED
    );
    let t = login(&app, "peter", "peter0821").await;
    let me = req(&app, "GET", "/api/auth/me", Some(&t), Value::Null).await;
    assert_eq!(me.1["role"], "admin");
    assert_eq!(req(&app, "POST", "/api/auth/logout", Some(&t), Value::Null).await.0, StatusCode::OK);
    assert_eq!(req(&app, "GET", "/api/auth/me", Some(&t), Value::Null).await.0, StatusCode::UNAUTHORIZED);
    let t = login(&app, "peter", "peter0821").await;
    let s = Store::open(dir.path()).unwrap();
    s.conn.execute("UPDATE app_session SET expires_at='2000-01-01T00:00:00+00:00'", []).unwrap();
    assert_eq!(req(&app, "GET", "/api/auth/me", Some(&t), Value::Null).await.0, StatusCode::UNAUTHORIZED);
}
#[tokio::test]
async fn initial_password_gate_and_other_sessions_revoked() {
    let (_dir, app) = app(false);
    let t = login(&app, "admin", seed::DEFAULT_ADMIN_PASSWORD).await;
    let other = login(&app, "admin", seed::DEFAULT_ADMIN_PASSWORD).await;
    assert_eq!(req(&app, "GET", "/api/items", Some(&t), Value::Null).await.0, StatusCode::FORBIDDEN);
    assert_eq!(
        req(
            &app,
            "POST",
            "/api/auth/password",
            Some(&t),
            json!({"current_password":seed::DEFAULT_ADMIN_PASSWORD,"new_password":"new-password-2026"})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(req(&app, "GET", "/api/auth/me", Some(&t), Value::Null).await.1["must_change"], false);
    assert_eq!(req(&app, "GET", "/api/items", Some(&t), Value::Null).await.0, StatusCode::OK);
    assert_eq!(req(&app, "GET", "/api/auth/me", Some(&other), Value::Null).await.0, StatusCode::UNAUTHORIZED);
}
#[tokio::test]
async fn permissions_refresh_and_reset_revokes_session() {
    let (_dir, app) = app(true);
    let admin = login(&app, "peter", "peter0821").await;
    let worker = login(&app, "operator", "urdwms2026").await;
    assert_eq!(
        req(&app, "POST", "/api/items", Some(&worker), json!({"name":"forbidden"})).await.0,
        StatusCode::FORBIDDEN
    );
    let id = req(&app, "GET", "/api/auth/me", Some(&worker), Value::Null).await.1["id"].as_i64().unwrap();
    assert_eq!(
        req(&app, "PATCH", &format!("/api/users/{id}"), Some(&admin), json!({"role":"manager"})).await.0,
        StatusCode::OK
    );
    assert_eq!(req(&app, "GET", "/api/auth/me", Some(&worker), Value::Null).await.1["role"], "manager");
    assert_eq!(
        req(&app, "PATCH", &format!("/api/users/{id}"), Some(&admin), json!({"password":"replacement-2026"})).await.0,
        StatusCode::OK
    );
    assert_eq!(req(&app, "GET", "/api/auth/me", Some(&worker), Value::Null).await.0, StatusCode::UNAUTHORIZED);
}
#[tokio::test]
async fn uploads_require_auth_and_reject_traversal() {
    let (dir, app) = app(true);
    std::fs::write(dir.path().join("uploads/photo.jpg"), b"\xff\xd8\xff\xd9").unwrap();
    assert_eq!(req(&app, "GET", "/uploads/photo.jpg", None, Value::Null).await.0, StatusCode::UNAUTHORIZED);
    let t = login(&app, "peter", "peter0821").await;
    assert_eq!(req(&app, "GET", "/uploads/photo.jpg", Some(&t), Value::Null).await.0, StatusCode::OK);
    for path in ["/uploads/../urdwms.db", "/uploads/%2e%2e%2furdwms.db", "/uploads/C:%5csecret", "/uploads/missing.jpg"]
    {
        assert_eq!(req(&app, "GET", path, Some(&t), Value::Null).await.0, StatusCode::NOT_FOUND, "{path}");
    }
}

#[tokio::test]
async fn concurrent_issue_cannot_overdraw_the_last_box() {
    let (_dir, app) = app(true);
    let t = login(&app, "peter", "peter0821").await;
    let (status, item) = req(&app, "POST", "/api/items", Some(&t), json!({"name":"last box"})).await;
    assert_eq!(status, StatusCode::OK, "{item}");
    let id = item["id"].as_i64().unwrap();
    let (status, lot) =
        req(&app, "POST", "/api/lots", Some(&t), json!({"item_id":id,"receipt_date":"2026-10-27","qty":1})).await;
    assert_eq!(status, StatusCode::OK, "{lot}");
    let body = json!({"item_id":id,"lot_id":lot["id"],"qty":1});
    let (a, b) = tokio::join!(
        req(&app, "POST", "/api/scans", Some(&t), body.clone()),
        req(&app, "POST", "/api/scans", Some(&t), body)
    );
    assert_eq!(
        [&a, &b].iter().filter(|r| r.0 == StatusCode::OK && r.1["status"] == "posted").count(),
        1,
        "{a:?} {b:?}"
    );
    let lots = req(&app, "GET", &format!("/api/lots?item_id={id}"), Some(&t), Value::Null).await.1;
    assert_eq!(lots[0]["qty_on_hand"], 0);
    assert_eq!(lots[0]["qty_drawn"], 1);
}

#[tokio::test]
async fn malformed_and_oversize_requests_return_json_errors() {
    let (_dir, app) = app(true);
    let t = login(&app, "peter", "peter0821").await;
    for (content, expected) in [("{".into(), 400), ("x".repeat(1024 * 1024 + 1), 413)] {
        let request = Request::builder()
            .method("POST")
            .uri("/api/items")
            .header("authorization", format!("Bearer {t}"))
            .header("content-type", "application/json")
            .body(Body::from(content))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status().as_u16(), expected);
        let value: Value = serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
        assert!(value["detail"].is_string());
    }
    let request=Request::builder().method("POST").uri("/api/recognize")
        .header("authorization",format!("Bearer {t}"))
        .header("content-type","multipart/form-data; boundary=x")
        .body(Body::from("--x\r\nContent-Disposition: form-data; name=\"image\"; filename=\"bad.jpg\"\r\nContent-Type: image/jpeg\r\n\r\nnot-an-image\r\n--x--\r\n")).unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
