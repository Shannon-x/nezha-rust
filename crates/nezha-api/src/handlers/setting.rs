use axum::{extract::Extension, extract::Path, Json};
use chrono::Utc;
use nezha_core::models::common::CommonResponse;
use nezha_service::AppState;
use std::sync::Arc;

/// 列出设置
pub async fn get_config(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<CommonResponse<serde_json::Value>> {
    Json(CommonResponse::success(serde_json::json!({
        "language": state.config.language,
        "site_name": state.config.site_name,
        "agent_secret_key": state.config.agent_secret_key,
        "listen_port": state.config.listen_port,
        "tls": state.config.tls,
        "jwt_timeout": state.config.jwt_timeout,
        "enable_plain_ip_in_notification": state.config.enable_plain_ip_in_notification,
    })))
}

/// 更新设置
pub async fn update_config(
    Extension(state): Extension<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> Json<CommonResponse<()>> {
    // 配置更新需要重启生效，这里仅存到数据库
    let now = Utc::now().naive_utc().format("%Y-%m-%d %H:%M:%S").to_string();
    if let Some(config_json) = serde_json::to_string(&body).ok() {
        sqlx::query("INSERT OR REPLACE INTO config (key, value, updated_at) VALUES ('settings', ?, ?)")
            .bind(&config_json).bind(now.as_str())
            .execute(&state.db.pool).await.ok();
    }
    Json(CommonResponse::success(()))
}

/// 列出通知组
pub async fn list_notification_groups(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<CommonResponse<Vec<serde_json::Value>>> {
    let rows: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT id, name, COALESCE(notifications,'[]') FROM notification_groups ORDER BY id"
    )
    .fetch_all(&state.db.pool).await.unwrap_or_default();

    let data: Vec<serde_json::Value> = rows.iter().map(|(id, name, notifications)| {
        serde_json::json!({"id": id, "name": name, "notifications": notifications})
    }).collect();
    Json(CommonResponse::success(data))
}

/// 用户列表
pub async fn list_users(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<CommonResponse<Vec<serde_json::Value>>> {
    let rows: Vec<(i64, String, i32)> = sqlx::query_as(
        "SELECT id, username, role FROM users ORDER BY id"
    )
    .fetch_all(&state.db.pool).await.unwrap_or_default();

    let data: Vec<serde_json::Value> = rows.iter().map(|(id, username, role)| {
        serde_json::json!({"id": id, "username": username, "role": role})
    }).collect();
    Json(CommonResponse::success(data))
}
