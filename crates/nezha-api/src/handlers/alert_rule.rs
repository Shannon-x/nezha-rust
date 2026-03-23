use axum::{extract::Extension, extract::Path, Json};
use chrono::Utc;
use nezha_core::models::common::CommonResponse;
use nezha_service::AppState;
use std::sync::Arc;

/// 列出告警规则
pub async fn list(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<CommonResponse<Vec<serde_json::Value>>> {
    let rows: Vec<(i64, String, String, bool, String, i64)> = sqlx::query_as(
        "SELECT id, name, COALESCE(rules_raw,'[]'), enabled, COALESCE(trigger_mode,'any'), notification_group_id FROM alert_rules ORDER BY id DESC"
    )
    .fetch_all(&state.db.pool).await.unwrap_or_default();

    let data: Vec<serde_json::Value> = rows.iter().map(|(id, name, rules_raw, enabled, trigger_mode, ng_id)| {
        serde_json::json!({
            "id": id, "name": name, "rules_raw": rules_raw,
            "enabled": enabled, "trigger_mode": trigger_mode,
            "notification_group_id": ng_id,
        })
    }).collect();
    Json(CommonResponse::success(data))
}

/// 创建告警规则
pub async fn create(
    Extension(state): Extension<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> Json<CommonResponse<serde_json::Value>> {
    let name = body["name"].as_str().unwrap_or("New Alert Rule");
    let rules_raw = body.get("rules_raw").map(|v| v.to_string()).unwrap_or("[]".to_string());
    let enabled = body["enabled"].as_bool().unwrap_or(true);
    let trigger_mode = body["trigger_mode"].as_str().unwrap_or("any");
    let ng_id = body["notification_group_id"].as_i64().unwrap_or(0);
    let now = Utc::now().naive_utc().format("%Y-%m-%d %H:%M:%S").to_string();

    let result = sqlx::query(
        "INSERT INTO alert_rules (created_at, updated_at, name, rules_raw, enabled, trigger_mode, notification_group_id) VALUES (?,?,?,?,?,?,?)"
    )
    .bind(now.as_str()).bind(now.as_str()).bind(name).bind(&rules_raw).bind(enabled).bind(trigger_mode).bind(ng_id)
    .execute(&state.db.pool).await;

    match result {
        Ok(r) => Json(CommonResponse::success(serde_json::json!({"id": r.last_insert_id()}))),
        Err(e) => Json(CommonResponse::error(format!("创建失败: {}", e))),
    }
}

/// 更新告警规则
pub async fn update(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<u64>,
    Json(body): Json<serde_json::Value>,
) -> Json<CommonResponse<serde_json::Value>> {
    let now = Utc::now().naive_utc().format("%Y-%m-%d %H:%M:%S").to_string();
    if let Some(name) = body["name"].as_str() {
        sqlx::query("UPDATE alert_rules SET name = ?, updated_at = ? WHERE id = ?")
            .bind(name).bind(now.as_str()).bind(id as i64).execute(&state.db.pool).await.ok();
    }
    if let Some(rules) = body.get("rules_raw") {
        sqlx::query("UPDATE alert_rules SET rules_raw = ?, updated_at = ? WHERE id = ?")
            .bind(rules.to_string()).bind(now.as_str()).bind(id as i64).execute(&state.db.pool).await.ok();
    }
    if let Some(enabled) = body["enabled"].as_bool() {
        sqlx::query("UPDATE alert_rules SET enabled = ?, updated_at = ? WHERE id = ?")
            .bind(enabled).bind(now.as_str()).bind(id as i64).execute(&state.db.pool).await.ok();
    }
    Json(CommonResponse::success(serde_json::json!({"id": id})))
}

/// 批量删除告警规则
pub async fn batch_delete(
    Extension(state): Extension<Arc<AppState>>,
    Json(ids): Json<Vec<u64>>,
) -> Json<CommonResponse<()>> {
    for id in &ids {
        sqlx::query("DELETE FROM alert_rules WHERE id = ?")
            .bind(*id as i64).execute(&state.db.pool).await.ok();
    }
    Json(CommonResponse::success(()))
}
