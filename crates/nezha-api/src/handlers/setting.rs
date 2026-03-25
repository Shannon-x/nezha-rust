use axum::{extract::Extension, Json};
use chrono::Utc;
use nezha_core::models::common::CommonResponse;
use nezha_service::AppState;
use std::sync::Arc;

/// Go 版 SettingResponse 兼容格式
/// 对应 model.SettingResponse
pub async fn get_config(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<CommonResponse<serde_json::Value>> {
    let config = state.config.read().await;
    let language = config.language.replace('_', "-");

    let oauth2_providers: Vec<String> = config.oauth2.keys().cloned().collect();

    let setting_response = serde_json::json!({
        "config": {
            "language": language,
            "site_name": config.site_name,
            "custom_code": config.custom_code,
            "custom_code_dashboard": config.custom_code_dashboard,
            "install_host": config.install_host,
            "agent_secret_key": config.agent_secret_key,
            "tls": config.tls,
            "web_real_ip_header": config.web_real_ip_header,
            "agent_real_ip_header": config.agent_real_ip_header,
            "user_template": config.user_template,
            "admin_template": config.admin_template,
            "enable_plain_ip_in_notification": config.enable_plain_ip_in_notification,
            "enable_ip_change_notification": config.enable_ip_change_notification,
            "ip_change_notification_group_id": config.ip_change_notification_group_id,
            "cover": config.cover,
            "ignored_ip_notification": config.ignored_ip_notification,
            "dns_servers": config.dns_servers,
            "ignored_ip_notification_server_ids": config.ignored_ip_notification_server_ids,
            "oauth2_providers": oauth2_providers,
        },
        "version": "v1.0.0",
        "frontend_templates": [
            {
                "path": "user-dist",
                "name": "Default",
                "is_admin": false,
                "is_official": true,
            },
            {
                "path": "admin-dist",
                "name": "Admin",
                "is_admin": true,
                "is_official": true,
            }
        ],
        "tsdb_enabled": true,
    });

    Json(CommonResponse::success(setting_response))
}

/// 更新设置 — PATCH /api/v1/setting
/// 从前端表单接收字段，更新内存中的 Config，并保存到 YAML 文件
pub async fn update_config(
    Extension(state): Extension<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> Json<CommonResponse<()>> {
    let mut config = state.config.write().await;

    // 逐字段更新（前端 settingFormSchema 提交的字段）
    if let Some(v) = body.get("site_name").and_then(|v| v.as_str()) {
        config.site_name = v.to_string();
    }
    if let Some(v) = body.get("language").and_then(|v| v.as_str()) {
        config.language = v.to_string();
    }
    if let Some(v) = body.get("install_host").and_then(|v| v.as_str()) {
        config.install_host = v.to_string();
    }
    if let Some(v) = body.get("custom_code").and_then(|v| v.as_str()) {
        config.custom_code = v.to_string();
    }
    if let Some(v) = body.get("custom_code_dashboard").and_then(|v| v.as_str()) {
        config.custom_code_dashboard = v.to_string();
    }
    if let Some(v) = body.get("web_real_ip_header").and_then(|v| v.as_str()) {
        config.web_real_ip_header = v.to_string();
    }
    if let Some(v) = body.get("agent_real_ip_header").and_then(|v| v.as_str()) {
        config.agent_real_ip_header = v.to_string();
    }
    if let Some(v) = body.get("user_template").and_then(|v| v.as_str()) {
        config.user_template = v.to_string();
    }
    if let Some(v) = body.get("dns_servers").and_then(|v| v.as_str()) {
        config.dns_servers = v.to_string();
    }
    if let Some(v) = body.get("ignored_ip_notification").and_then(|v| v.as_str()) {
        config.ignored_ip_notification = v.to_string();
    }
    if let Some(v) = body.get("tls").and_then(|v| v.as_bool()) {
        config.tls = v;
    }
    if let Some(v) = body.get("enable_ip_change_notification").and_then(|v| v.as_bool()) {
        config.enable_ip_change_notification = v;
    }
    if let Some(v) = body.get("enable_plain_ip_in_notification").and_then(|v| v.as_bool()) {
        config.enable_plain_ip_in_notification = v;
    }
    if let Some(v) = body.get("cover").and_then(|v| v.as_u64()) {
        config.cover = v as u8;
    }
    if let Some(v) = body.get("ip_change_notification_group_id").and_then(|v| v.as_u64()) {
        config.ip_change_notification_group_id = v;
    }

    // 保存到 YAML 文件
    if let Err(e) = config.save() {
        tracing::error!("Failed to save config: {}", e);
        return Json(CommonResponse::error(format!("保存配置失败: {}", e)));
    }

    tracing::info!("Config saved successfully");
    Json(CommonResponse::success(()))
}


/// 列出通知组 — GET /api/v1/notification-group
pub async fn list_notification_groups(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<CommonResponse<Vec<serde_json::Value>>> {
    let rows: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT id, name, COALESCE(notifications,'[]') FROM notification_groups ORDER BY id"
    )
    .fetch_all(&state.db.pool).await.unwrap_or_default();

    let data: Vec<serde_json::Value> = rows.iter().map(|(id, name, notifications)| {
        let notifs: serde_json::Value = serde_json::from_str(notifications).unwrap_or(serde_json::json!([]));
        serde_json::json!({"id": id, "name": name, "notifications": notifs})
    }).collect();
    Json(CommonResponse::success(data))
}

/// 创建通知组 — POST /api/v1/notification-group
pub async fn create_notification_group(
    Extension(state): Extension<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> Json<CommonResponse<serde_json::Value>> {
    let name = body.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let empty_arr = serde_json::json!([]);
    let notifications = body.get("notifications").unwrap_or(&empty_arr);
    let now = Utc::now().naive_utc().format("%Y-%m-%d %H:%M:%S").to_string();

    let result = sqlx::query(
        "INSERT INTO notification_groups (created_at, updated_at, name, notifications) VALUES (?, ?, ?, ?)"
    )
    .bind(now.as_str())
    .bind(now.as_str())
    .bind(name)
    .bind(serde_json::to_string(notifications).unwrap_or_default())
    .execute(&state.db.pool).await;

    match result {
        Ok(r) => Json(CommonResponse::success(serde_json::json!({"id": r.last_insert_id()}))),
        Err(e) => Json(CommonResponse::error(format!("创建通知组失败: {}", e))),
    }
}

/// 更新通知组 — PATCH /api/v1/notification-group/:id
pub async fn update_notification_group(
    Extension(state): Extension<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    Json(body): Json<serde_json::Value>,
) -> Json<CommonResponse<()>> {
    let now = Utc::now().naive_utc().format("%Y-%m-%d %H:%M:%S").to_string();
    let name = body.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let empty_arr = serde_json::json!([]);
    let notifications = body.get("notifications").unwrap_or(&empty_arr);

    let _ = sqlx::query("UPDATE notification_groups SET updated_at = ?, name = ?, notifications = ? WHERE id = ?")
        .bind(now.as_str())
        .bind(name)
        .bind(serde_json::to_string(notifications).unwrap_or_default())
        .bind(id)
        .execute(&state.db.pool).await;

    Json(CommonResponse::success(()))
}

/// 批量删除通知组 — POST /api/v1/batch-delete/notification-group
pub async fn batch_delete_notification_group(
    Extension(state): Extension<Arc<AppState>>,
    Json(ids): Json<Vec<i64>>,
) -> Json<CommonResponse<()>> {
    for id in &ids {
        let _ = sqlx::query("DELETE FROM notification_groups WHERE id = ?")
            .bind(id)
            .execute(&state.db.pool).await;
    }
    Json(CommonResponse::success(()))
}

/// 用户列表 — GET /api/v1/user
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

/// 系统维护 — POST /api/v1/maintenance
pub async fn run_maintenance(
    Extension(_state): Extension<Arc<AppState>>,
) -> Json<CommonResponse<()>> {
    Json(CommonResponse::success(()))
}

/// 在线用户列表 — GET /api/v1/online-user
/// Go 版 pCommonHandler(listOnlineUser) — 返回分页 Value
pub async fn list_online_users(
    Extension(_state): Extension<Arc<AppState>>,
) -> Json<CommonResponse<serde_json::Value>> {
    // 返回 Go 版 pCommonHandler 的 PaginatedResponse 格式
    Json(CommonResponse::success(serde_json::json!({
        "data": [],
        "total": 0
    })))
}

/// 批量封禁在线用户 — POST /api/v1/online-user/batch-block
pub async fn batch_block_online_user(
    Extension(_state): Extension<Arc<AppState>>,
    Json(_body): Json<serde_json::Value>,
) -> Json<CommonResponse<()>> {
    Json(CommonResponse::success(()))
}

/// WAF 黑名单列表 — GET /api/v1/waf
pub async fn list_waf(
    Extension(_state): Extension<Arc<AppState>>,
) -> Json<CommonResponse<serde_json::Value>> {
    Json(CommonResponse::success(serde_json::json!({
        "data": [],
        "total": 0
    })))
}

/// 批量删除 WAF 黑名单 — POST /api/v1/batch-delete/waf
pub async fn batch_delete_waf(
    Extension(_state): Extension<Arc<AppState>>,
    Json(_ids): Json<Vec<i64>>,
) -> Json<CommonResponse<()>> {
    Json(CommonResponse::success(()))
}

