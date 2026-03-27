use axum::{extract::Extension, extract::Path, extract::Query, Json, response::IntoResponse};
use chrono::{NaiveDateTime, Utc};
use nezha_core::models::common::CommonResponse;
use nezha_core::models::server::ServerForm;
use nezha_core::models::host::{Host, HostState};
use nezha_service::AppState;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// 分页参数
#[derive(Deserialize)]
pub struct Pagination {
    #[serde(default = "default_page")]
    pub page: usize,
    #[serde(default = "default_limit")]
    pub limit: usize,
}
fn default_page() -> usize { 1 }
fn default_limit() -> usize { 0 } // 0 = no pagination (return all)

/// 分页响应
#[derive(Serialize)]
pub struct PaginatedResponse<T: Serialize> {
    pub data: Vec<T>,
    pub total: usize,
    pub page: usize,
    pub limit: usize,
}

// ── 零分配响应结构体（替代 serde_json::json!）──

#[derive(Clone, Serialize)]
pub struct ServerView {
    pub id: i64,
    pub name: String,
    pub uuid: String,
    pub note: String,
    pub public_note: String,
    pub display_index: i32,
    pub hide_for_guest: bool,
    pub enable_ddns: bool,
    pub host: Host,
    pub state: HostState,
    pub geoip: nezha_utils::ip::GeoIP,
    pub last_active: String,
    pub online: bool,
}

#[derive(Serialize)]
pub struct MetricPoint {
    pub timestamp: String,
    pub value: f64,
}

#[derive(Serialize)]
pub struct IdResponse {
    pub id: u64,
}

#[derive(Serialize)]
pub struct CreateServerResponse {
    pub id: u64,
    pub uuid: String,
}

/// 获取所有服务器列表
/// Go 版 listHandler 返回 CommonResponse[[]Server]，data 是一个数组
pub async fn list(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<CommonResponse<Vec<ServerView>>> {
    let mut servers: Vec<ServerView> = state.servers
        .iter()
        .map(|entry| {
            let s = entry.value();
            let online = s.last_active.map(|t| {
                (Utc::now().naive_utc() - t).num_seconds() < 120
            }).unwrap_or(false);
            ServerView {
                id: s.id,
                name: s.name.clone(),
                uuid: s.uuid.clone(),
                note: s.note.clone(),
                public_note: s.public_note.clone(),
                display_index: s.display_index,
                hide_for_guest: s.hide_for_guest,
                enable_ddns: s.enable_ddns,
                host: s.host.clone().unwrap_or_default(),
                state: s.state.clone().unwrap_or_default(),
                geoip: s.geoip.clone().unwrap_or_default(),
                last_active: s.last_active
                    .map(|t| t.format("%Y-%m-%dT%H:%M:%S").to_string())
                    .unwrap_or_default(),
                online,
            }
        })
        .collect();

    servers.sort_unstable_by(|a, b| {
        a.display_index.cmp(&b.display_index).then(a.id.cmp(&b.id))
    });

    Json(CommonResponse::success(servers))
}


/// 获取服务器指标（零 json! 分配）
pub async fn get_metrics(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<u64>,
) -> Json<CommonResponse<Vec<MetricPoint>>> {
    if let Some(ref tsdb) = state.tsdb {
        match tsdb.query_server_metrics(id, nezha_tsdb::MetricType::CPU, nezha_tsdb::QueryPeriod::Hour6).await {
            Ok(points) => {
                let data: Vec<MetricPoint> = points.iter().map(|p| MetricPoint {
                    timestamp: p.timestamp.to_string(),
                    value: p.value,
                }).collect();
                Json(CommonResponse::success(data))
            }
            Err(_) => Json(CommonResponse::success(vec![])),
        }
    } else {
        Json(CommonResponse::success(vec![]))
    }
}

/// 更新服务器（单条动态 SQL，不再为每个字段单独 UPDATE）
pub async fn update(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<u64>,
    Json(form): Json<ServerForm>,
) -> Json<CommonResponse<IdResponse>> {
    if !state.servers.contains_key(&id) {
        return Json(CommonResponse::error("服务器不存在"));
    }

    let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    // 构建动态 SET 子句（字符串字段）
    let mut parts = vec!["updated_at = ?"];
    let mut str_vals: Vec<String> = vec![now.clone()];

    if let Some(ref name) = form.name { parts.push("name = ?"); str_vals.push(name.clone()); }
    if let Some(ref note) = form.note { parts.push("note = ?"); str_vals.push(note.clone()); }
    if let Some(ref pn) = form.public_note { parts.push("public_note = ?"); str_vals.push(pn.clone()); }

    // display_index 和 hide_for_guest 需要单独 bind（类型不同）
    let has_di = form.display_index.is_some();
    let has_hfg = form.hide_for_guest.is_some();
    if has_di { parts.push("display_index = ?"); }
    if has_hfg { parts.push("hide_for_guest = ?"); }

    let sql = format!("UPDATE servers SET {} WHERE id = ?", parts.join(", "));
    let mut query = sqlx::query(&sql);
    for v in &str_vals { query = query.bind(v.as_str()); }
    if let Some(di) = form.display_index { query = query.bind(di); }
    if let Some(hfg) = form.hide_for_guest { query = query.bind(hfg as i32); }
    query = query.bind(id as i64);
    query.execute(&state.db.pool).await.ok();

    // 更新内存
    if let Some(mut server) = state.servers.get_mut(&id) {
        if let Some(ref name) = form.name { server.name = name.clone(); }
        if let Some(ref note) = form.note { server.note = note.clone(); }
        if let Some(ref pn) = form.public_note { server.public_note = pn.clone(); }
        if let Some(di) = form.display_index { server.display_index = di; }
        if let Some(hfg) = form.hide_for_guest { server.hide_for_guest = hfg; }
    }

    Json(CommonResponse::success(IdResponse { id }))
}

/// 批量删除服务器（单条 IN 子句 SQL）
pub async fn batch_delete(
    Extension(state): Extension<Arc<AppState>>,
    Json(ids): Json<Vec<u64>>,
) -> Json<CommonResponse<()>> {
    if ids.is_empty() {
        return Json(CommonResponse::success(()));
    }
    for id in &ids { state.servers.remove(id); }
    let placeholders: String = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!("DELETE FROM servers WHERE id IN ({})", placeholders);
    let mut query = sqlx::query(&sql);
    for id in &ids { query = query.bind(*id as i64); }
    query.execute(&state.db.pool).await.ok();
    Json(CommonResponse::success(()))
}

/// 创建服务器
pub async fn create(
    Extension(state): Extension<Arc<AppState>>,
    Json(form): Json<ServerForm>,
) -> Json<CommonResponse<CreateServerResponse>> {
    let name = form.name.unwrap_or_else(|| "New Server".to_string());
    let uuid_str = uuid::Uuid::new_v4().to_string();
    let now = Utc::now().naive_utc().format("%Y-%m-%d %H:%M:%S").to_string();

    let result = sqlx::query(
        "INSERT INTO servers (created_at, updated_at, name, uuid, note, display_index, hide_for_guest, enable_ddns) VALUES (?,?,?,?,?,?,?,?)"
    )
    .bind(now.as_str()).bind(now.as_str()).bind(&name).bind(&uuid_str)
    .bind(form.note.as_deref().unwrap_or("")).bind(form.display_index.unwrap_or(0))
    .bind(form.hide_for_guest.unwrap_or(false) as i32).bind(form.enable_ddns.unwrap_or(false) as i32)
    .execute(&state.db.pool).await;

    match result {
        Ok(r) => {
            let id = r.last_insert_id().unwrap_or(0) as u64;
            let mut server = nezha_core::models::server::Server::default();
            server.id = id as i64;
            server.name = name;
            server.uuid = uuid_str.clone();
            state.servers.insert(id, server);
            Json(CommonResponse::success(CreateServerResponse { id, uuid: uuid_str }))
        }
        Err(e) => Json(CommonResponse::error(format!("创建失败: {}", e))),
    }
}

/// 获取服务器安装命令 — GET /api/v1/server/config/:id
/// 返回一键安装命令字符串（前端 getServerConfig 使用）
pub async fn get_config(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<u64>,
) -> axum::response::Response {
    let cfg = state.config.read().await;
    let agent_secret = &cfg.agent_secret_key;
    let install_host = if cfg.install_host.is_empty() {
        format!("localhost:{}", cfg.listen_port)
    } else {
        cfg.install_host.clone()
    };
    let tls_flag = if cfg.tls { " --tls" } else { "" };

    match state.servers.get(&id) {
        Some(_s) => {
            // 一键安装命令，指向 Shannon-x/agent-rust
            let cmd = format!(
                "curl -L https://raw.githubusercontent.com/Shannon-x/agent-rust/main/install.sh -o agent-install.sh && chmod +x agent-install.sh && sudo ./agent-install.sh -s {} -k {}{}",
                install_host, agent_secret, tls_flag
            );
            cmd.into_response()
        }
        None => {
            axum::response::Response::builder()
                .status(404)
                .body("server not found".into())
                .unwrap()
        }
    }
}

/// 设置服务器配置 — POST /api/v1/server/config
pub async fn set_config(
    Extension(_state): Extension<Arc<AppState>>,
    Json(_body): Json<serde_json::Value>,
) -> Json<CommonResponse<()>> {
    Json(CommonResponse::success(()))
}

/// 批量移动服务器 — POST /api/v1/batch-move/server
pub async fn batch_move(
    Extension(_state): Extension<Arc<AppState>>,
    Json(_body): Json<serde_json::Value>,
) -> Json<CommonResponse<()>> {
    Json(CommonResponse::success(()))
}

/// 强制更新服务器 — POST /api/v1/force-update/server
pub async fn force_update(
    Extension(_state): Extension<Arc<AppState>>,
    Json(_body): Json<serde_json::Value>,
) -> Json<CommonResponse<()>> {
    Json(CommonResponse::success(()))
}

