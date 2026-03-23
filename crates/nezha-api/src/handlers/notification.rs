use axum::{extract::Extension, extract::Path, Json};
use chrono::Utc;
use nezha_core::models::common::CommonResponse;
use nezha_service::AppState;
use std::sync::Arc;

/// 列出通知方式
pub async fn list(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<CommonResponse<Vec<serde_json::Value>>> {
    let rows: Vec<(i64, String, String, String, bool)> = sqlx::query_as(
        "SELECT id, name, tag, url, verify_tls FROM notifications ORDER BY id DESC"
    )
    .fetch_all(&state.db.pool).await.unwrap_or_default();

    let data: Vec<serde_json::Value> = rows.iter().map(|(id, name, tag, url, verify)| {
        serde_json::json!({"id": id, "name": name, "tag": tag, "url": url, "verify_tls": verify})
    }).collect();
    Json(CommonResponse::success(data))
}

/// 创建通知方式
pub async fn create(
    Extension(state): Extension<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> Json<CommonResponse<serde_json::Value>> {
    let name = body["name"].as_str().unwrap_or("New Notification");
    let tag = body["tag"].as_str().unwrap_or("webhook");
    let url = body["url"].as_str().unwrap_or("");
    let verify = body["verify_tls"].as_bool().unwrap_or(true);
    let request_method = body["request_method"].as_i64().unwrap_or(1) as i32;
    let request_type = body["request_type"].as_i64().unwrap_or(1) as i32;
    let request_header = body.get("request_header").map(|v| v.to_string()).unwrap_or_default();
    let request_body = body["request_body"].as_str().unwrap_or("");
    let now = Utc::now().naive_utc().format("%Y-%m-%d %H:%M:%S").to_string();

    let result = sqlx::query(
       "INSERT INTO notifications (created_at, updated_at, name, tag, url, request_method, request_type, request_header, request_body, verify_tls) VALUES (?,?,?,?,?,?,?,?,?,?)"
    )
    .bind(now.as_str()).bind(now.as_str()).bind(name).bind(tag).bind(url)
    .bind(request_method).bind(request_type).bind(&request_header).bind(request_body)
    .bind(verify)
    .execute(&state.db.pool).await;

    match result {
        Ok(r) => Json(CommonResponse::success(serde_json::json!({"id": r.last_insert_id()}))),
        Err(e) => Json(CommonResponse::error(format!("创建失败: {}", e))),
    }
}

/// 更新通知方式
pub async fn update(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<u64>,
    Json(body): Json<serde_json::Value>,
) -> Json<CommonResponse<serde_json::Value>> {
    let now = Utc::now().naive_utc().format("%Y-%m-%d %H:%M:%S").to_string();
    if let Some(name) = body["name"].as_str() {
        sqlx::query("UPDATE notifications SET name = ?, updated_at = ? WHERE id = ?")
            .bind(name).bind(now.as_str()).bind(id as i64).execute(&state.db.pool).await.ok();
    }
    if let Some(url) = body["url"].as_str() {
        sqlx::query("UPDATE notifications SET url = ?, updated_at = ? WHERE id = ?")
            .bind(url).bind(now.as_str()).bind(id as i64).execute(&state.db.pool).await.ok();
    }
    Json(CommonResponse::success(serde_json::json!({"id": id})))
}

/// 批量删除通知方式
pub async fn batch_delete(
    Extension(state): Extension<Arc<AppState>>,
    Json(ids): Json<Vec<u64>>,
) -> Json<CommonResponse<()>> {
    for id in &ids {
        sqlx::query("DELETE FROM notifications WHERE id = ?")
            .bind(*id as i64).execute(&state.db.pool).await.ok();
    }
    Json(CommonResponse::success(()))
}
