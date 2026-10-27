//! Expiring bearer sessions. Only a digest of the bearer token is stored.
use super::ApiError;
use crate::{
    auth::{self, Session},
    service::{self, Store},
};
use axum::http::{HeaderMap, header};
use chrono::{Duration, Utc};
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn text<'a>(body: &'a Value, key: &str) -> Result<&'a str, ApiError> {
    body.get(key).and_then(Value::as_str).ok_or_else(|| ApiError::new(400, format!("缺少欄位：{key}")))
}
fn digest(token: &str) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(token.as_bytes())))
}
pub fn user_json(user: &Session) -> Value {
    json!({"id":user.id,"username":user.username,"name":user.name,"title":user.title,
        "role":user.role,"role_label":user.role_label,"must_change":user.must_change,"permissions":auth::permissions(&user.role)})
}
pub fn login(store: &Store, body: &Value) -> Result<Value, ApiError> {
    let username = text(body, "username")?;
    let password = text(body, "password")?;
    let tx = rusqlite::Transaction::new_unchecked(&store.conn, rusqlite::TransactionBehavior::Immediate)?;
    let user = service::login(store, username, password).map_err(|_| ApiError::new(401, "帳號或密碼不正確"))?;
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(ApiError::internal)?;
    let token = hex::encode(bytes);
    let expires = (Utc::now() + Duration::hours(12)).to_rfc3339();
    store.conn.execute("DELETE FROM app_session WHERE expires_at < ?", [Utc::now().to_rfc3339()])?;
    store.conn.execute(
        "INSERT INTO app_session(token,user_id,created_at,expires_at) VALUES (?,?,?,?)",
        params![digest(&token), user.id, Utc::now().to_rfc3339(), expires],
    )?;
    tx.commit()?;
    Ok(json!({"token":token,"expires_at":expires,"user":user_json(&user)}))
}
pub fn authenticate(store: &Store, headers: &HeaderMap) -> Result<(Session, String), ApiError> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .filter(|t| t.len() == 64 && t.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or_else(|| ApiError::new(401, "請先登入"))?;
    let key = digest(token);
    let row: Option<(String,String)> = store.conn.query_row(
        "SELECT u.username,s.expires_at FROM app_session s JOIN app_user u ON u.id=s.user_id WHERE s.token=? AND u.active=1",
        [&key], |r| Ok((r.get(0)?,r.get(1)?))).optional()?;
    let (username, expires) = row.ok_or_else(|| ApiError::new(401, "登入已失效，請重新登入"))?;
    if !chrono::DateTime::parse_from_rfc3339(&expires).is_ok_and(|t| t > Utc::now()) {
        store.conn.execute("DELETE FROM app_session WHERE token=?", [&key])?;
        return Err(ApiError::new(401, "登入逾時，請重新登入"));
    }
    let user = service::session_for(&store.conn, &username).map_err(|_| ApiError::new(401, "登入已失效"))?;
    Ok((user, key))
}
pub fn require_password_changed(user: &Session) -> Result<(), ApiError> {
    if user.must_change { Err(ApiError::new(403, "請先更改初始密碼")) } else { Ok(()) }
}
pub fn change_password(store: &Store, user: &Session, key: &str, body: &Value) -> Result<Value, ApiError> {
    service::change_password_retaining_session(
        store,
        user,
        text(body, "current_password")?,
        text(body, "new_password")?,
        Some(key),
    )?;
    Ok(json!({"ok":true}))
}
