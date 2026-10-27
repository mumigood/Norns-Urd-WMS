#![cfg(feature = "server")]

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use tower::ServiceExt;
use urdwms::{
    db, seed,
    service::{self, ItemInput, ItemOrder, Store},
    web,
};

async fn request(app: &Router, method: &str, path: &str, token: Option<&str>, body: Value) -> (StatusCode, Vec<u8>) {
    let mut request = Request::builder().method(method).uri(path).header("content-type", "application/json");
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    let response = app.clone().oneshot(request.body(Body::from(body.to_string())).unwrap()).await.unwrap();
    let status = response.status();
    (status, to_bytes(response.into_body(), 1024 * 1024).await.unwrap().to_vec())
}

async fn json_request(app: &Router, method: &str, path: &str, token: Option<&str>, body: Value) -> Value {
    let (status, bytes) = request(app, method, path, token, body).await;
    assert_eq!(status, StatusCode::OK, "{method} {path}: {}", String::from_utf8_lossy(&bytes));
    serde_json::from_slice(&bytes).unwrap()
}

async fn login(app: &Router) -> String {
    json_request(app, "POST", "/api/auth/login", None, json!({"username":"peter", "password":"peter0821"}))
        .await["token"].as_str().unwrap().to_owned()
}

fn data_version(conn: &Connection) -> i64 {
    conn.query_row("PRAGMA data_version", [], |r| r.get(0)).unwrap()
}

#[tokio::test]
async fn unversioned_desktop_data_survives_upgrade_and_is_available_through_web() {
    let dir = tempfile::tempdir().unwrap();
    let original = Connection::open(dir.path().join("urdwms.db")).unwrap();
    // The desktop predecessor had no migration version. Build its schema
    // directly, never through Store::open (which is the upgrade under test).
    original.execute_batch(db::SCHEMA).unwrap();
    seed::demo(&original).unwrap();
    original.execute("UPDATE app_user SET name='原有管理員' WHERE username='peter'", []).unwrap();
    original.execute("UPDATE app_role SET label='原有系統管理' WHERE code='admin'", []).unwrap();
    original
        .execute("UPDATE inventory_item SET name='原有包材自訂名稱' WHERE id=(SELECT min(id) FROM inventory_item)", [])
        .unwrap();
    original
        .execute_batch(
            "DROP TABLE app_session; ALTER TABLE inventory_item DROP COLUMN sort_order; PRAGMA user_version=0;",
        )
        .unwrap();
    let hash: String =
        original.query_row("SELECT password_hash FROM app_user WHERE username='peter'", [], |r| r.get(0)).unwrap();
    let item_count: i64 = original.query_row("SELECT count(*) FROM inventory_item", [], |r| r.get(0)).unwrap();
    let user_count: i64 = original.query_row("SELECT count(*) FROM app_user", [], |r| r.get(0)).unwrap();
    original
        .execute(
            "INSERT INTO app_setting(key,value,updated_at,updated_by) VALUES('alerts',?,'2026-01-01','existing-user')",
            [json!({"expiry_days":47,"stale_days":91,"pending_hours":13}).to_string()],
        )
        .unwrap();
    std::fs::create_dir(dir.path().join("uploads")).unwrap();
    let image = b"\xff\xd8\xff\xd9";
    std::fs::write(dir.path().join("uploads/original-photo.jpg"), image).unwrap();
    let item_id: i64 = original.query_row("SELECT min(id) FROM inventory_item", [], |r| r.get(0)).unwrap();
    original.execute("INSERT INTO material_usage_scan(item_id,qty,status,captured_at,captured_by,image_path,field_values,created_at) VALUES(?,1,'blocked_unreadable','2026-01-01','existing-user','/uploads/original-photo.jpg','{}','2026-01-01')", [item_id]).unwrap();
    drop(original);

    let desktop = Store::open(dir.path()).unwrap();
    assert_eq!(desktop.conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0)).unwrap(), 1);
    assert_eq!(
        desktop
            .conn
            .query_row("SELECT password_hash FROM app_user WHERE username='peter'", [], |r| r.get::<_, String>(0))
            .unwrap(),
        hash
    );
    assert_eq!(
        desktop.conn.query_row("SELECT count(*) FROM app_user", [], |r| r.get::<_, i64>(0)).unwrap(),
        user_count
    );
    let app = web::router(dir.path().to_path_buf());
    let token = login(&app).await;
    let me = json_request(&app, "GET", "/api/auth/me", Some(&token), Value::Null).await;
    assert_eq!(me["username"], "peter");
    assert_eq!(me["name"], "原有管理員");
    assert_eq!(me["role_label"], "原有系統管理");
    let items = json_request(&app, "GET", "/api/items", Some(&token), Value::Null).await;
    assert_eq!(items.as_array().unwrap().len() as i64, item_count);
    assert!(items.as_array().unwrap().iter().any(|i| i["id"] == item_id && i["name"] == "原有包材自訂名稱"));
    let thresholds = json_request(&app, "GET", "/api/settings/alerts", Some(&token), Value::Null).await;
    assert_eq!(thresholds, json!({"expiry_days":47,"stale_days":91,"pending_hours":13}));
    let scans = json_request(&app, "GET", "/api/scans", Some(&token), Value::Null).await;
    assert!(
        scans
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["image_path"] == "/uploads/original-photo.jpg" && s["captured_by"] == "existing-user")
    );
    let (status, served) = request(&app, "GET", "/uploads/original-photo.jpg", Some(&token), Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(served, image);
    assert_eq!(std::fs::read(desktop.resolve_image("/uploads/original-photo.jpg")).unwrap(), image);
    // Starting/reopening the second client must neither reseed nor duplicate.
    let reopened = Store::open(dir.path()).unwrap();
    assert_eq!(service::items(&reopened.conn, ItemOrder::Sort).unwrap().len() as i64, item_count);
}

#[tokio::test]
async fn independent_desktop_and_web_connections_share_writes_and_change_notifications() {
    let dir = tempfile::tempdir().unwrap();
    let desktop = Store::open(dir.path()).unwrap();
    seed::demo(&desktop.conn).unwrap();
    let user = service::login(&desktop, "peter", "peter0821").unwrap();
    let app = web::router(dir.path().to_path_buf());
    let token = login(&app).await;
    let before_own_write = data_version(&desktop.conn);
    let id = service::create_item(
        &desktop,
        &user,
        &ItemInput {
            name: Some("桌面新增共用包材".into()),
            model: Some("SHARED-DESKTOP".into()),
            ..Default::default()
        },
    )
    .unwrap();
    // data_version is a connection-local external-commit signal, not a count
    // of writes; a local save does not pretend to be a remote edit.
    assert_eq!(data_version(&desktop.conn), before_own_write);
    let items = json_request(&app, "GET", "/api/items", Some(&token), Value::Null).await;
    assert!(items.as_array().unwrap().iter().any(|i| i["id"] == id && i["name"] == "桌面新增共用包材"));

    let before_web_write = data_version(&desktop.conn);
    json_request(&app, "PATCH", &format!("/api/items/{id}"), Some(&token), json!({"name":"網頁修正共用包材"})).await;
    assert_ne!(data_version(&desktop.conn), before_web_write);
    let updated = service::items(&desktop.conn, ItemOrder::Sort).unwrap().into_iter().find(|i| i.id == id).unwrap();
    assert_eq!(updated.name, "網頁修正共用包材");
    service::save_alert_thresholds(
        &desktop,
        &user,
        service::AlertThresholds { expiry_days: 33, stale_days: 88, pending_hours: 7 },
    )
    .unwrap();
    assert_eq!(json_request(&app, "GET", "/api/settings/alerts", Some(&token), Value::Null).await["expiry_days"], 33);
    json_request(
        &app,
        "PUT",
        "/api/settings/alerts",
        Some(&token),
        json!({"expiry_days":44,"stale_days":99,"pending_hours":8}),
    )
    .await;
    assert_eq!(service::alert_thresholds(&desktop.conn).unwrap().expiry_days, 44);

    // A web-side account change must be visible to the existing egui session.
    json_request(&app, "PATCH", &format!("/api/users/{}", user.id), Some(&token), json!({"title":"共用帳號職位"}))
        .await;
    assert_eq!(service::refresh_session(&desktop, &user).unwrap().title.as_deref(), Some("共用帳號職位"));
    let stored: String =
        desktop.conn.query_row("SELECT name FROM inventory_item WHERE id=?", params![id], |r| r.get(0)).unwrap();
    assert_eq!(stored, "網頁修正共用包材");
}
