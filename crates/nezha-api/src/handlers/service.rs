use axum::{extract::Extension, extract::Path, extract::Query, Json};
use chrono::Utc;
use nezha_core::models::common::CommonResponse;
use nezha_service::AppState;
use serde::{Deserialize, Serialize};

/// 分页参数
#[derive(Deserialize)]
pub struct Pagination {
    #[serde(default = "default_page")]
    pub page: usize,
    #[serde(default = "default_limit")]
    pub limit: usize,
}
fn default_page() -> usize { 1 }
fn default_limit() -> usize { 0 }
use std::sync::Arc;

// ── 零分配响应结构体 ──

#[derive(Serialize)]
pub struct ServicePublicView {
    pub id: i64,
    pub name: String,
    pub r#type: i32,
    pub target: String,
    pub duration: i32,
}

#[derive(Clone, Serialize)]
pub struct ServiceListView {
    pub id: i64,
    pub name: String,
    pub r#type: i32,
    pub target: String,
    pub duration: i32,
    pub notify: bool,
    pub cover: i32,
    pub notification_group_id: u64,
    pub enable_show_in_service: bool,
    pub display_index: i32,
}

#[derive(Serialize)]
pub struct ServiceShowResponse {
    pub services: Vec<ServicePublicView>,
}

#[derive(Serialize)]
pub struct HistoryServerView {
    pub server_id: u64,
    pub total_up: u64,
    pub total_down: u64,
    pub avg_delay: f64,
}

#[derive(Serialize)]
pub struct HistoryResponse {
    pub servers: Vec<HistoryServerView>,
}

#[derive(Serialize)]
pub struct DailyStatsView {
    pub up: u64,
    pub down: u64,
    pub delay: f64,
}

#[derive(Serialize)]
pub struct IdResponse {
    pub id: u64,
}

#[derive(Deserialize)]
pub struct ServiceForm {
    pub name: Option<String>,
    pub r#type: Option<i32>,
    pub target: Option<String>,
    pub duration: Option<i32>,
    pub notify: Option<bool>,
    pub cover: Option<i32>,
    pub notification_group_id: Option<i64>,
    pub enable_show_in_service: Option<bool>,
    pub display_index: Option<i32>,
}

/// 服务监控公开列表（零 json! 分配）
pub async fn show(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<CommonResponse<ServiceShowResponse>> {
    let services: Vec<ServicePublicView> = state.services
        .iter()
        .filter(|e| e.value().enable_show_in_service)
        .map(|e| {
            let s = e.value();
            ServicePublicView {
                id: s.id, name: s.name.clone(), r#type: s.r#type,
                target: s.target.clone(), duration: s.duration,
            }
        })
        .collect();
    Json(CommonResponse::success(ServiceShowResponse { services }))
}

/// 服务历史（零 json!）
pub async fn history(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<u64>,
) -> Json<CommonResponse<HistoryResponse>> {
    let empty = HistoryResponse { servers: vec![] };
    if let Some(ref tsdb) = state.tsdb {
        match tsdb.query_service_history(id, nezha_tsdb::QueryPeriod::Day1).await {
            Ok(result) => {
                let servers = result.servers.iter().map(|s| HistoryServerView {
                    server_id: s.server_id,
                    total_up: s.stats.total_up,
                    total_down: s.stats.total_down,
                    avg_delay: s.stats.avg_delay,
                }).collect();
                Json(CommonResponse::success(HistoryResponse { servers }))
            }
            Err(_) => Json(CommonResponse::success(empty)),
        }
    } else {
        Json(CommonResponse::success(empty))
    }
}

/// 每日统计（零 json!）
pub async fn get_history(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<u64>,
) -> Json<CommonResponse<Vec<DailyStatsView>>> {
    if let Some(ref tsdb) = state.tsdb {
        let today = Utc::now().naive_utc();
        match tsdb.query_service_daily_stats(id, today, 30).await {
            Ok(stats) => {
                let data: Vec<DailyStatsView> = stats.iter().map(|s| DailyStatsView {
                    up: s.up, down: s.down, delay: s.delay,
                }).collect();
                Json(CommonResponse::success(data))
            }
            Err(_) => Json(CommonResponse::success(vec![])),
        }
    } else {
        Json(CommonResponse::success(vec![]))
    }
}

/// 管理列表 — 返回数组（与 Go 版 listHandler 兼容）
pub async fn list(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<CommonResponse<Vec<ServiceListView>>> {
    let mut services: Vec<ServiceListView> = state.services
        .iter()
        .map(|e| {
            let s = e.value();
            ServiceListView {
                id: s.id, name: s.name.clone(), r#type: s.r#type,
                target: s.target.clone(), duration: s.duration,
                notify: s.notify, cover: s.cover,
                notification_group_id: s.notification_group_id,
                enable_show_in_service: s.enable_show_in_service,
                display_index: s.display_index,
            }
        })
        .collect();
    services.sort_unstable_by(|a, b| a.display_index.cmp(&b.display_index).then(a.id.cmp(&b.id)));

    Json(CommonResponse::success(services))
}


/// 创建服务（强类型 form，零 json!）
pub async fn create(
    Extension(state): Extension<Arc<AppState>>,
    Json(form): Json<ServiceForm>,
) -> Json<CommonResponse<IdResponse>> {
    let name = form.name.as_deref().unwrap_or("New Service");
    let stype = form.r#type.unwrap_or(0);
    let target = form.target.as_deref().unwrap_or("");
    let duration = form.duration.unwrap_or(30);
    let notify = form.notify.unwrap_or(false);
    let cover = form.cover.unwrap_or(0);
    let ng_id = form.notification_group_id.unwrap_or(0);
    let show_in = form.enable_show_in_service.unwrap_or(false);
    let di = form.display_index.unwrap_or(0);
    let now = Utc::now().naive_utc().format("%Y-%m-%d %H:%M:%S").to_string();

    let result = sqlx::query(
        "INSERT INTO services (created_at, updated_at, name, type, target, duration, notify, cover, notification_group_id, enable_show_in_service, display_index) VALUES (?,?,?,?,?,?,?,?,?,?,?)"
    )
    .bind(now.as_str()).bind(now.as_str()).bind(name).bind(stype).bind(target)
    .bind(duration).bind(notify).bind(cover).bind(ng_id).bind(show_in).bind(di)
    .execute(&state.db.pool).await;

    match result {
        Ok(r) => {
            let id = r.last_insert_id().unwrap_or(0) as u64;
            let mut svc = nezha_core::models::service::Service::default();
            svc.id = id as i64;
            svc.name = name.to_string();
            svc.r#type = stype;
            svc.target = target.to_string();
            svc.duration = duration;
            svc.notify = notify;
            svc.cover = cover;
            svc.notification_group_id = ng_id as u64;
            svc.enable_show_in_service = show_in;
            svc.display_index = di;
            state.services.insert(id, svc);
            Json(CommonResponse::success(IdResponse { id }))
        }
        Err(e) => Json(CommonResponse::error(format!("创建失败: {}", e))),
    }
}

/// 更新服务（单条动态 SQL 合并所有字段）
pub async fn update(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<u64>,
    Json(form): Json<ServiceForm>,
) -> Json<CommonResponse<IdResponse>> {
    if !state.services.contains_key(&id) {
        return Json(CommonResponse::error("服务监控不存在"));
    }
    let now = Utc::now().naive_utc().format("%Y-%m-%d %H:%M:%S").to_string();

    // 构建动态 UPDATE — 单条 SQL
    let mut parts: Vec<&str> = vec!["updated_at = ?"];
    let mut str_vals: Vec<String> = vec![now.clone()];

    if let Some(ref n) = form.name { parts.push("name = ?"); str_vals.push(n.clone()); }
    if let Some(ref t) = form.target { parts.push("target = ?"); str_vals.push(t.clone()); }

    let has_dur = form.duration.is_some();
    let has_type = form.r#type.is_some();
    let has_notify = form.notify.is_some();
    let has_cover = form.cover.is_some();
    let has_ng = form.notification_group_id.is_some();
    let has_show = form.enable_show_in_service.is_some();
    let has_di = form.display_index.is_some();

    if has_type { parts.push("type = ?"); }
    if has_dur { parts.push("duration = ?"); }
    if has_notify { parts.push("notify = ?"); }
    if has_cover { parts.push("cover = ?"); }
    if has_ng { parts.push("notification_group_id = ?"); }
    if has_show { parts.push("enable_show_in_service = ?"); }
    if has_di { parts.push("display_index = ?"); }

    let sql = format!("UPDATE services SET {} WHERE id = ?", parts.join(", "));
    let mut query = sqlx::query(&sql);
    for v in &str_vals { query = query.bind(v.as_str()); }
    if let Some(t) = form.r#type { query = query.bind(t); }
    if let Some(d) = form.duration { query = query.bind(d); }
    if let Some(n) = form.notify { query = query.bind(n); }
    if let Some(c) = form.cover { query = query.bind(c); }
    if let Some(ng) = form.notification_group_id { query = query.bind(ng); }
    if let Some(s) = form.enable_show_in_service { query = query.bind(s); }
    if let Some(di) = form.display_index { query = query.bind(di); }
    query = query.bind(id as i64);
    query.execute(&state.db.pool).await.ok();

    // 更新内存
    if let Some(mut s) = state.services.get_mut(&id) {
        if let Some(ref n) = form.name { s.name = n.clone(); }
        if let Some(ref t) = form.target { s.target = t.clone(); }
        if let Some(tp) = form.r#type { s.r#type = tp; }
        if let Some(d) = form.duration { s.duration = d; }
        if let Some(n) = form.notify { s.notify = n; }
        if let Some(c) = form.cover { s.cover = c; }
        if let Some(ng) = form.notification_group_id { s.notification_group_id = ng as u64; }
        if let Some(sh) = form.enable_show_in_service { s.enable_show_in_service = sh; }
        if let Some(di) = form.display_index { s.display_index = di; }
    }

    Json(CommonResponse::success(IdResponse { id }))
}

/// 批量删除（单条 IN 子句）
pub async fn batch_delete(
    Extension(state): Extension<Arc<AppState>>,
    Json(ids): Json<Vec<u64>>,
) -> Json<CommonResponse<()>> {
    if ids.is_empty() { return Json(CommonResponse::success(())); }
    for id in &ids { state.services.remove(id); }
    let placeholders: String = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!("DELETE FROM services WHERE id IN ({})", placeholders);
    let mut query = sqlx::query(&sql);
    for id in &ids { query = query.bind(*id as i64); }
    query.execute(&state.db.pool).await.ok();
    Json(CommonResponse::success(()))
}

/// 列出带服务的服务器 — GET /api/v1/service/server
pub async fn list_server_with_services(
    Extension(_state): Extension<Arc<AppState>>,
) -> Json<CommonResponse<Vec<serde_json::Value>>> {
    Json(CommonResponse::success(vec![]))
}

/// 列出服务器的服务 — GET /api/v1/server/:id/service
pub async fn list_server_services(
    Extension(_state): Extension<Arc<AppState>>,
    Path(_id): Path<u64>,
) -> Json<CommonResponse<Vec<serde_json::Value>>> {
    Json(CommonResponse::success(vec![]))
}

