/// WebSocket 服务端推送 — 增量模式 + Go 版全量兼容
use axum::{
    extract::{ws::{Message, WebSocket}, Extension, Query, WebSocketUpgrade},
    response::IntoResponse,
};
use nezha_service::AppState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::time::Duration;

/// 轻量服务器状态快照（用于变更检测 + 序列化）
#[derive(Clone, PartialEq, Serialize)]
struct ServerSnapshot {
    id: i64,
    name: String,
    online: bool,
    cpu: f64,
    mem_used: u64,
    swap_used: u64,
    disk_used: u64,
    net_in_speed: u64,
    net_out_speed: u64,
    net_in_transfer: u64,
    net_out_transfer: u64,
    load_1: f64,
    uptime: u64,
    country_code: String,
}

/// WebSocket 查询参数
#[derive(Deserialize)]
pub struct WsParams {
    /// 推送模式: "full"（Go 前端兼容）或 "delta"（增量）
    #[serde(default = "default_mode")]
    pub mode: String,
}

fn default_mode() -> String { "full".to_string() }

pub async fn server_stream(
    ws: WebSocketUpgrade,
    Extension(state): Extension<Arc<AppState>>,
    Query(params): Query<WsParams>,
) -> impl IntoResponse {
    let use_delta = params.mode == "delta";
    ws.on_upgrade(move |socket| handle_ws(socket, state, use_delta))
}

async fn handle_ws(mut socket: WebSocket, state: Arc<AppState>, use_delta: bool) {
    let mut interval = tokio::time::interval(Duration::from_secs(2));
    let mut seq: u64 = 0;
    let mut prev_snapshots: HashMap<i64, ServerSnapshot> = HashMap::new();

    loop {
        tokio::select! {
            _ = interval.tick() => {
                seq += 1;

                // 快照当前服务器状态
                let current: HashMap<i64, ServerSnapshot> = state.servers
                    .iter()
                    .map(|e| {
                        let s = e.value();
                        let online = s.last_active.map(|t| {
                            (chrono::Utc::now().naive_utc() - t).num_seconds() < 120
                        }).unwrap_or(false);
                        let st = s.state.as_ref();
                        let snap = ServerSnapshot {
                            id: s.id,
                            name: s.name.clone(),
                            online,
                            cpu: st.map(|x| x.cpu).unwrap_or(0.0),
                            mem_used: st.map(|x| x.mem_used).unwrap_or(0),
                            swap_used: st.map(|x| x.swap_used).unwrap_or(0),
                            disk_used: st.map(|x| x.disk_used).unwrap_or(0),
                            net_in_speed: st.map(|x| x.net_in_speed).unwrap_or(0),
                            net_out_speed: st.map(|x| x.net_out_speed).unwrap_or(0),
                            net_in_transfer: st.map(|x| x.net_in_transfer).unwrap_or(0),
                            net_out_transfer: st.map(|x| x.net_out_transfer).unwrap_or(0),
                            load_1: st.map(|x| x.load_1).unwrap_or(0.0),
                            uptime: st.map(|x| x.uptime).unwrap_or(0),
                            country_code: s.geoip.as_ref().map(|g| g.country_code.clone()).unwrap_or_default(),
                        };
                        (s.id, snap)
                    })
                    .collect();

                if !use_delta {
                    // ── Go 前端全量兼容模式 ──
                    let servers: Vec<&ServerSnapshot> = current.values().collect();
                    let payload = serde_json::json!({
                        "type": "servers",
                        "servers": servers,
                    });
                    if socket.send(Message::Text(payload.to_string().into())).await.is_err() { break; }
                } else if seq == 1 {
                    // ── 增量模式：首次全量 ──
                    let all: Vec<&ServerSnapshot> = current.values().collect();
                    let payload = serde_json::json!({
                        "type": "servers_full",
                        "seq": seq,
                        "data": all,
                    });
                    if socket.send(Message::Text(payload.to_string().into())).await.is_err() { break; }
                } else {
                    // ── 增量模式：仅差异 ──
                    let mut changed: Vec<&ServerSnapshot> = Vec::new();
                    let mut removed: Vec<i64> = Vec::new();

                    for (id, snap) in &current {
                        match prev_snapshots.get(id) {
                            Some(old) if old == snap => {}
                            _ => changed.push(snap),
                        }
                    }
                    for id in prev_snapshots.keys() {
                        if !current.contains_key(id) { removed.push(*id); }
                    }

                    if !changed.is_empty() || !removed.is_empty() {
                        let payload = serde_json::json!({
                            "type": "servers_delta",
                            "seq": seq,
                            "updated": changed,
                            "removed": removed,
                        });
                        if socket.send(Message::Text(payload.to_string().into())).await.is_err() { break; }
                    } else if seq % 10 == 0 {
                        let payload = serde_json::json!({"type": "heartbeat", "seq": seq});
                        if socket.send(Message::Text(payload.to_string().into())).await.is_err() { break; }
                    }
                }

                prev_snapshots = current;
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Ping(d))) => {
                        if socket.send(Message::Pong(d)).await.is_err() { break; }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
}
