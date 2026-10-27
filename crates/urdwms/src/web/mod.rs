//! HTTP transport over the same services used by egui. Each request owns a
//! connection, so a camera timeout never holds a global database mutex.
mod dto;
mod routes;
mod sessions;

use crate::{
    db,
    service::{self, Store},
};
use axum::{
    Json, Router,
    body::{Body, to_bytes},
    extract::{DefaultBodyLimit, FromRequest, Multipart, Path, Query, Request, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::{Value, json};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::sync::Semaphore;

const JSON_LIMIT: usize = 1024 * 1024;
const IMAGE_LIMIT: usize = urdwms_core::camera::MAX_BYTES;

#[derive(Debug)]
pub struct ApiError {
    pub status: u16,
    pub detail: Value,
}
impl ApiError {
    pub fn new(status: u16, message: impl Into<String>) -> Self {
        Self { status, detail: Value::String(message.into()) }
    }
    fn internal(error: impl std::fmt::Display) -> Self {
        crate::applog::error(error.to_string());
        Self::new(500, "伺服器處理失敗，請查看主機日誌")
    }
}
impl From<crate::error::AppError> for ApiError {
    fn from(error: crate::error::AppError) -> Self {
        if error.0.starts_with("資料庫錯誤：") || error.0.starts_with("檔案錯誤：") {
            Self::internal(error)
        } else {
            Self::new(400, error.0)
        }
    }
}
impl From<serde_json::Error> for ApiError {
    fn from(error: serde_json::Error) -> Self {
        Self::new(400, format!("資料格式錯誤：{error}"))
    }
}
impl From<rusqlite::Error> for ApiError {
    fn from(error: rusqlite::Error) -> Self {
        Self::internal(error)
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            Json(json!({"detail": self.detail})),
        )
            .into_response()
    }
}

#[derive(Clone)]
struct AppState {
    data_dir: PathBuf,
    workers: Arc<Semaphore>,
}
impl AppState {
    async fn run<T, F>(&self, work: F) -> Result<T, ApiError>
    where
        T: Send + 'static,
        F: FnOnce(Store) -> Result<T, ApiError> + Send + 'static,
    {
        let permit =
            self.workers.clone().try_acquire_owned().map_err(|_| ApiError::new(503, "主機忙碌中，請稍後重試"))?;
        let dir = self.data_dir.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            // Schema initialization happens only at startup, not per request.
            let store = Store { conn: db::open(&dir.join("urdwms.db"))?, data_dir: dir };
            work(store)
        })
        .await
        .map_err(ApiError::internal)?
    }
}

/// Initialize Store/seed before constructing the router. Static files are
/// served by the deployment's web server, never from the data directory.
pub fn router(data_dir: PathBuf) -> Router {
    Router::new()
        .route("/api/health", get(|| async { Json(json!({"ok": true, "version": env!("CARGO_PKG_VERSION")})) }))
        .route("/api/recognize", post(recognize))
        .route("/uploads/{*path}", get(upload))
        .fallback(api)
        .layer(DefaultBodyLimit::max(IMAGE_LIMIT + 64 * 1024))
        .layer(axum::middleware::map_response(|mut response: Response| async move {
            response.headers_mut().insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
            response.headers_mut().insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
            response
        }))
        .with_state(AppState { data_dir, workers: Arc::new(Semaphore::new(16)) })
}

async fn api(State(state): State<AppState>, request: Request) -> Result<Json<Value>, ApiError> {
    let method = request.method().as_str().to_string();
    let path = request.uri().path().trim_end_matches('/').to_string();
    let Query(query) = Query::<HashMap<String, String>>::try_from_uri(request.uri())
        .map_err(|_| ApiError::new(400, "查詢參數格式錯誤"))?;
    let headers = request.headers().clone();
    let bytes = to_bytes(request.into_body(), JSON_LIMIT).await.map_err(|_| ApiError::new(413, "請求內容過大"))?;
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        if headers
            .get(header::CONTENT_TYPE)
            .and_then(|h| h.to_str().ok())
            .is_none_or(|h| h.split(';').next() != Some("application/json"))
        {
            return Err(ApiError::new(415, "請使用 application/json"));
        }
        serde_json::from_slice(&bytes)?
    };
    state
        .run(move |store| {
            if method == "POST" && path == "/api/auth/login" {
                return sessions::login(&store, &body);
            }
            let (user, token) = sessions::authenticate(&store, &headers)?;
            match (method.as_str(), path.as_str()) {
                ("GET", "/api/auth/me") => Ok(sessions::user_json(&user)),
                ("POST", "/api/auth/logout") => {
                    store.conn.execute("DELETE FROM app_session WHERE token = ?", [&token])?;
                    service::logout(&store, &user);
                    Ok(json!({"ok": true}))
                }
                ("POST", "/api/auth/password") => sessions::change_password(&store, &user, &token, &body),
                _ => {
                    sessions::require_password_changed(&user)?;
                    routes::dispatch(&store, &user, &method, &path, &query, body)
                }
            }
        })
        .await
        .map(Json)
}

async fn upload(
    State(state): State<AppState>,
    Path(path): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    state
        .run(move |store| {
            let (user, _) = sessions::authenticate(&store, &headers)?;
            sessions::require_password_changed(&user)?;
            // Windows separators, drive prefixes, alternate streams and encoded
            // traversal are rejected even when running tests on Unix.
            if path.is_empty()
                || !path.bytes().all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
                || path.starts_with('.')
            {
                return Err(ApiError::new(404, "找不到影像"));
            }
            let extension = std::path::Path::new(&path).extension().and_then(|e| e.to_str()).unwrap_or("");
            let media = match extension {
                "jpg" | "jpeg" => "image/jpeg",
                "png" => "image/png",
                "bmp" => "image/bmp",
                "webp" => "image/webp",
                _ => return Err(ApiError::new(404, "找不到影像")),
            };
            let root = store.uploads().canonicalize().map_err(|_| ApiError::new(404, "找不到影像"))?;
            let file = root.join(&path).canonicalize().map_err(|_| ApiError::new(404, "找不到影像"))?;
            if file.parent() != Some(root.as_path()) {
                return Err(ApiError::new(404, "找不到影像"));
            }
            let bytes = std::fs::read(file).map_err(ApiError::internal)?;
            Ok(([(header::CONTENT_TYPE, media)], Body::from(bytes)).into_response())
        })
        .await
}

async fn recognize(State(state): State<AppState>, request: Request) -> Result<Json<Value>, ApiError> {
    let headers = request.headers().clone();
    state
        .run(move |store| {
            let (user, _) = sessions::authenticate(&store, &headers)?;
            sessions::require_password_changed(&user)?;
            user.require(&store.conn, "issue.create").map_err(|e| ApiError::new(403, e.0))
        })
        .await?;
    let mut multipart =
        Multipart::from_request(request, &state).await.map_err(|_| ApiError::new(400, "請上傳 image 欄位的照片"))?;
    let mut image = None;
    while let Some(field) = multipart.next_field().await.map_err(|_| ApiError::new(413, "照片過大或上傳格式錯誤"))?
    {
        if field.name() != Some("image") || image.is_some() {
            return Err(ApiError::new(400, "只接受一張 image 照片"));
        }
        let bytes = field.bytes().await.map_err(|_| ApiError::new(413, "照片過大"))?;
        if bytes.is_empty() || bytes.len() > IMAGE_LIMIT {
            return Err(ApiError::new(413, "照片不可為空或超過 12 MB"));
        }
        image = Some(bytes);
    }
    let bytes = image.ok_or_else(|| ApiError::new(400, "請選擇照片"))?;
    state
        .run(move |store| {
            let reader = image::ImageReader::new(std::io::Cursor::new(&bytes))
                .with_guessed_format()
                .map_err(|_| ApiError::new(400, "無法讀取照片格式"))?;
            let (w, h) = reader.into_dimensions().map_err(|_| ApiError::new(400, "影像格式不正確"))?;
            if u64::from(w) * u64::from(h) > 40_000_000 {
                return Err(ApiError::new(400, "照片尺寸超過 4000 萬畫素"));
            }
            let provider = service::provider(&store.conn);
            let proposal = service::recognise_and_store(&store, provider.as_ref(), &bytes, "-web")?;
            dto::proposal_json(&store, &proposal)
        })
        .await
        .map(Json)
}
