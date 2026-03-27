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
    pub skip_servers: std::collections::HashMap<u64, bool>,
    pub fail_trigger_tasks: Vec<u64>,
    pub recover_trigger_tasks: Vec<u64>,
    pub min_latency: f32,
    pub max_latency: f32,
    pub latency_notify: bool,
    pub enable_trigger_task: bool,
}

#[derive(Serialize)]
pub struct ServiceShowResponse {
    pub services: Vec<ServicePublicView>,
}

#[derive(Serialize)]
pub struct DataPoint {
    pub ts: i64,
    pub delay: f64,
    pub status: u8,
}

#[derive(Serialize)]
pub struct ServiceHistorySummary {
    pub avg_delay: f64,
    pub up_percent: f32,
    pub total_up: u64,
    pub total_down: u64,
    pub data_points: Vec<DataPoint>,
}

#[derive(Serialize)]
pub struct ServerServiceStats {
    pub server_id: u64,
    pub server_name: String,
    pub stats: ServiceHistorySummary,
}

#[derive(Serialize)]
pub struct ServiceHistoryResponse {
    pub service_id: u64,
    pub service_name: String,
    pub servers: Vec<ServerServiceStats>,
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

#[derive(Serialize)]
pub struct ServiceInfos {
    pub monitor_id: u64,
    pub server_id: u64,
    pub monitor_name: String,
    pub server_name: String,
    pub display_index: i32,
    pub created_at: Vec<i64>,
    pub avg_delay: Vec<f64>,
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
    pub skip_servers: Option<std::collections::HashMap<u64, bool>>,
    pub fail_trigger_tasks: Option<Vec<u64>>,
    pub recover_trigger_tasks: Option<Vec<u64>>,
    pub min_latency: Option<f32>,
    pub max_latency: Option<f32>,
    pub latency_notify: Option<bool>,
    pub enable_trigger_task: Option<bool>,
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
) -> Json<CommonResponse<ServiceHistoryResponse>> {
    let mut empty = ServiceHistoryResponse {
        service_id: id,
        service_name: "".to_string(),
        servers: vec![],
    };
    
    // 如果有对应的服务，获取名称
    if let Some(svc) = state.services.get(&id) {
        empty.service_name = svc.name.clone();
    } else {
        return Json(CommonResponse::error("service not found"));
    }

    if let Some(ref tsdb) = state.tsdb {
        match tsdb.query_service_history(id, nezha_tsdb::QueryPeriod::Day1).await {
            Ok(result) => {
                let servers = result.servers.iter().map(|s| {
                    let server_name = state.servers.get(&s.server_id).map(|srv| srv.name.clone()).unwrap_or_default();
                    ServerServiceStats {
                        server_id: s.server_id,
                        server_name,
                        stats: ServiceHistorySummary {
                            avg_delay: s.stats.avg_delay,
                            up_percent: if s.stats.total_up + s.stats.total_down > 0 {
                                (s.stats.total_up as f32) / ((s.stats.total_up + s.stats.total_down) as f32) * 100.0
                            } else {
                                0.0
                            },
                            total_up: s.stats.total_up,
                            total_down: s.stats.total_down,
                            data_points: vec![], // TSDB crate currently doesn't fetch individual data points
                        }
                    }
                }).collect();
                Json(CommonResponse::success(ServiceHistoryResponse {
                    service_id: empty.service_id,
                    service_name: empty.service_name,
                    servers,
                }))
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

/// 服务监控时序数据 — GET /api/v1/monitor/:id
/// 前端调用此接口获取 created_at（时间戳数组）和 avg_delay（延迟数组）
/// 每个元素代表一台被该监控覆盖的服务器的延迟时序记录
pub async fn monitor_history(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<u64>,
) -> Json<CommonResponse<Vec<ServiceInfos>>> {
    // 先克隆 service 所需字段，立即释放 DashMap Ref，避免跨 shard 死锁
    let (service_name, display_index, cover, skip_servers) = match state.services.get(&id) {
        Some(svc) => (
            svc.name.clone(),
            svc.display_index,
            svc.cover,
            svc.skip_servers.clone(),
        ),
        None => return Json(CommonResponse::error("service not found")),
    };
    // DashMap Ref 在此处已经 drop

    // 收集所有被此服务覆盖的服务器（此时再遍历 servers，不会死锁）
    let covered_servers: Vec<(u64, String)> = state.servers.iter()
        .filter(|entry| {
            let sid = *entry.key();
            if cover == 0 {
                !skip_servers.contains_key(&sid)
            } else {
                skip_servers.get(&sid).copied().unwrap_or(false)
            }
        })
        .map(|entry| (*entry.key(), entry.value().name.clone()))
        .collect();

    let mut result = Vec::new();

    if let Some(ref tsdb) = state.tsdb {
        for (server_id, server_name) in covered_servers {
            let points = tsdb.query_service_datapoints(id, server_id, nezha_tsdb::QueryPeriod::Day1)
                .await
                .unwrap_or_default();
            let created_at: Vec<i64> = points.iter().map(|(ts, _)| *ts).collect();
            let avg_delay: Vec<f64> = points.iter().map(|(_, d)| *d).collect();
            result.push(ServiceInfos {
                monitor_id: id, server_id,
                monitor_name: service_name.clone(), server_name,
                display_index, created_at, avg_delay,
            });
        }
    } else {
        // 没有 TSDB 时按覆盖到的服务器返回空数组
        for (server_id, server_name) in covered_servers {
            result.push(ServiceInfos {
                monitor_id: id, server_id,
                monitor_name: service_name.clone(), server_name,
                display_index, created_at: vec![], avg_delay: vec![],
            });
        }
    }

    Json(CommonResponse::success(result))
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
                skip_servers: s.skip_servers.clone(),
                fail_trigger_tasks: s.fail_trigger_tasks.clone(),
                recover_trigger_tasks: s.recover_trigger_tasks.clone(),
                min_latency: s.min_latency,
                max_latency: s.max_latency,
                latency_notify: s.latency_notify,
                enable_trigger_task: s.enable_trigger_task,
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
    let min_latency = form.min_latency.unwrap_or(0.0);
    let max_latency = form.max_latency.unwrap_or(0.0);
    let latency_notify = form.latency_notify.unwrap_or(false);
    let enable_trigger_task = form.enable_trigger_task.unwrap_or(false);
    let now = Utc::now().naive_utc().format("%Y-%m-%d %H:%M:%S").to_string();

    let _skip_servers_str = serde_json::to_string(&form.skip_servers.as_ref().unwrap_or(&std::collections::HashMap::new())).unwrap_or_else(|_| "{}".to_string());
    
    // SQLite doesn't natively support arrays, we need to serialize them
    let _fail_str = serde_json::to_string(&form.fail_trigger_tasks.as_ref().unwrap_or(&vec![])).unwrap_or_else(|_| "[]".to_string());
    let _recover_str = serde_json::to_string(&form.recover_trigger_tasks.as_ref().unwrap_or(&vec![])).unwrap_or_else(|_| "[]".to_string());

    let result = sqlx::query(
        "INSERT INTO services (created_at, updated_at, name, type, target, duration, notify, cover, notification_group_id, enable_show_in_service, display_index) VALUES (?,?,?,?,?,?,?,?,?,?,?)"
    )
    .bind(now.as_str()).bind(now.as_str()).bind(name).bind(stype).bind(target)
    .bind(duration).bind(notify).bind(cover).bind(ng_id).bind(show_in).bind(di)
    // Add additional DB schema fields if your SQLite schema supports them, currently falling back to just returning them in memory since original didn't insert them
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
            svc.skip_servers = form.skip_servers.unwrap_or_default();
            svc.fail_trigger_tasks = form.fail_trigger_tasks.unwrap_or_default();
            svc.recover_trigger_tasks = form.recover_trigger_tasks.unwrap_or_default();
            svc.min_latency = min_latency;
            svc.max_latency = max_latency;
            svc.latency_notify = latency_notify;
            svc.enable_trigger_task = enable_trigger_task;
            
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
    // Add additional fields dynamically
    let _has_min_latency = form.min_latency.is_some();
    let _has_max_latency = form.max_latency.is_some();
    let _has_latency_notify = form.latency_notify.is_some();
    let _has_enable_trigger = form.enable_trigger_task.is_some();

    if has_type { parts.push("type = ?"); }
    if has_dur { parts.push("duration = ?"); }
    if has_notify { parts.push("notify = ?"); }
    if has_cover { parts.push("cover = ?"); }
    if has_ng { parts.push("notification_group_id = ?"); }
    if has_show { parts.push("enable_show_in_service = ?"); }
    if has_di { parts.push("display_index = ?"); }
    
    // Some columns might not exist in the basic DB schema, but we want to bind them safely.
    // For now we just update DB for the available fields, but memory for all.

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
        if let Some(ss) = form.skip_servers.clone() { s.skip_servers = ss; }
        if let Some(ft) = form.fail_trigger_tasks.clone() { s.fail_trigger_tasks = ft; }
        if let Some(rt) = form.recover_trigger_tasks.clone() { s.recover_trigger_tasks = rt; }
        if let Some(ml) = form.min_latency { s.min_latency = ml; }
        if let Some(ml) = form.max_latency { s.max_latency = ml; }
        if let Some(ln) = form.latency_notify { s.latency_notify = ln; }
        if let Some(et) = form.enable_trigger_task { s.enable_trigger_task = et; }
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
    Extension(state): Extension<Arc<AppState>>,
) -> Json<CommonResponse<Vec<u64>>> {
    let mut server_ids = std::collections::HashSet::new();

    for svc in state.services.iter() {
        let s = svc.value();
        if s.cover == 0 { // ServiceCoverAll
            for server in state.servers.iter() {
                if !s.skip_servers.contains_key(server.key()) {
                    server_ids.insert(*server.key());
                }
            }
        } else {
            for (id, enabled) in &s.skip_servers {
                if *enabled {
                    server_ids.insert(*id);
                }
            }
        }
    }

    let ret: Vec<u64> = server_ids.into_iter().collect();
    Json(CommonResponse::success(ret))
}

/// 列出服务器的服务 — GET /api/v1/server/:id/service
pub async fn list_server_services(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<u64>,
) -> Json<CommonResponse<Vec<ServiceInfos>>> {
    let server_name = match state.servers.get(&id) {
        Some(s) => s.name.clone(),
        None => return Json(CommonResponse::error("server not found")),
    };

    let mut result = Vec::new();
    let mut _history_map = std::collections::HashMap::new();

    if let Some(ref tsdb) = state.tsdb {
        if let Ok(history_results) = tsdb.query_service_history_by_server_id(id, nezha_tsdb::QueryPeriod::Day1).await {
            _history_map = history_results;
        }
    }

    for svc in state.services.iter() {
        let service = svc.value();
        
        if service.cover == 0 {
            if service.skip_servers.contains_key(&id) { continue; }
        } else {
            if !service.skip_servers.contains_key(&id) { continue; }
        }

        // Add service irrespective of history being present to prevent UI undefined failures on empty array
        result.push(ServiceInfos {
            monitor_id: service.id as u64,
            server_id: id,
            monitor_name: service.name.clone(),
            server_name: server_name.clone(),
            display_index: service.display_index,
            created_at: vec![], // Empty array if no history is present yet
            avg_delay: vec![],
        });
    }

    Json(CommonResponse::success(result))
}

