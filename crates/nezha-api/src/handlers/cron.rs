use axum::{extract::Extension, extract::Path, Json};
use chrono::Utc;
use nezha_core::models::common::CommonResponse;
use nezha_service::AppState;
use std::sync::Arc;

/// 列出定时任务
pub async fn list(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<CommonResponse<Vec<serde_json::Value>>> {
    let rows: Vec<(i64, String, i32, String, String, String, String, i32, i64, i32)> = sqlx::query_as(
        "SELECT id, name, task_type, scheduler, command, COALESCE(servers_raw,'[]'), COALESCE(push_successful_message,''), cover, notification_group_id, CAST(push_successful AS INTEGER) FROM crons ORDER BY id DESC"
    )
    .fetch_all(&state.db.pool).await.unwrap_or_default();

    let data: Vec<serde_json::Value> = rows.iter().map(|(id, name, task_type, scheduler, command, servers, psm, cover, ng_id, ps)| {
        serde_json::json!({
            "id": id, "name": name, "task_type": task_type,
            "scheduler": scheduler, "command": command,
            "servers": servers, "push_successful_message": psm,
            "cover": cover, "notification_group_id": ng_id,
            "push_successful": ps,
        })
    }).collect();
    Json(CommonResponse::success(data))
}

/// 创建定时任务
pub async fn create(
    Extension(state): Extension<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> Json<CommonResponse<serde_json::Value>> {
    let name = body["name"].as_str().unwrap_or("New Cron");
    let task_type = body["task_type"].as_i64().unwrap_or(0) as i32;
    let scheduler = body["scheduler"].as_str().unwrap_or("0 0 * * *");
    let command = body["command"].as_str().unwrap_or("");
    let servers = body.get("servers").map(|v| v.to_string()).unwrap_or("[]".to_string());
    let cover = body["cover"].as_i64().unwrap_or(0) as i32;
    let ng_id = body["notification_group_id"].as_i64().unwrap_or(0);
    let now = Utc::now().naive_utc().format("%Y-%m-%d %H:%M:%S").to_string();

    let result = sqlx::query(
        "INSERT INTO crons (created_at, updated_at, name, task_type, scheduler, command, servers_raw, cover, notification_group_id) VALUES (?,?,?,?,?,?,?,?,?)"
    )
    .bind(now.as_str()).bind(now.as_str()).bind(name).bind(task_type).bind(scheduler)
    .bind(command).bind(&servers).bind(cover).bind(ng_id)
    .execute(&state.db.pool).await;

    match result {
        Ok(r) => Json(CommonResponse::success(serde_json::json!({"id": r.last_insert_id()}))),
        Err(e) => Json(CommonResponse::error(format!("创建失败: {}", e))),
    }
}

/// 更新定时任务
pub async fn update(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<u64>,
    Json(body): Json<serde_json::Value>,
) -> Json<CommonResponse<serde_json::Value>> {
    let now = Utc::now().naive_utc().format("%Y-%m-%d %H:%M:%S").to_string();
    if let Some(name) = body["name"].as_str() {
        sqlx::query("UPDATE crons SET name = ?, updated_at = ? WHERE id = ?")
            .bind(name).bind(now.as_str()).bind(id as i64).execute(&state.db.pool).await.ok();
    }
    if let Some(scheduler) = body["scheduler"].as_str() {
        sqlx::query("UPDATE crons SET scheduler = ?, updated_at = ? WHERE id = ?")
            .bind(scheduler).bind(now.as_str()).bind(id as i64).execute(&state.db.pool).await.ok();
    }
    if let Some(command) = body["command"].as_str() {
        sqlx::query("UPDATE crons SET command = ?, updated_at = ? WHERE id = ?")
            .bind(command).bind(now.as_str()).bind(id as i64).execute(&state.db.pool).await.ok();
    }
    Json(CommonResponse::success(serde_json::json!({"id": id})))
}

/// 批量删除定时任务
pub async fn batch_delete(
    Extension(state): Extension<Arc<AppState>>,
    Json(ids): Json<Vec<u64>>,
) -> Json<CommonResponse<()>> {
    for id in &ids {
        sqlx::query("DELETE FROM crons WHERE id = ?")
            .bind(*id as i64).execute(&state.db.pool).await.ok();
    }
    Json(CommonResponse::success(()))
}

/// 手动触发定时任务 — GET /api/v1/cron/:id/manual
pub async fn manual_trigger(
    Extension(_state): Extension<Arc<AppState>>,
    Path(_id): Path<u64>,
) -> Json<CommonResponse<()>> {
    // TODO: 实际触发定时任务执行
    Json(CommonResponse::success(()))
}

