/// WebSocket 服务端推送 — 使用与 REST API 一致的 ServerView 结构
use axum::{
    extract::{ws::{Message, WebSocket}, Extension, Query, WebSocketUpgrade},
    response::IntoResponse,
};
use chrono::Utc;
use nezha_service::AppState;
use serde::Deserialize;
use std::sync::Arc;
use tokio::time::Duration;

use crate::handlers::server::ServerView;

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

/// 从 AppState 构建完整的 ServerView 列表（与 REST API list() 完全一致的结构）
fn build_server_views(state: &AppState) -> Vec<ServerView> {
    state.servers
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
        .collect()
}

/// 用于增量比较的轻量快照（仅比较变化的字段）
#[derive(Clone, PartialEq)]
struct ChangeKey {
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
}

fn change_key(v: &ServerView) -> ChangeKey {
    ChangeKey {
        online: v.online,
        cpu: v.state.cpu,
        mem_used: v.state.mem_used,
        swap_used: v.state.swap_used,
        disk_used: v.state.disk_used,
        net_in_speed: v.state.net_in_speed,
        net_out_speed: v.state.net_out_speed,
        net_in_transfer: v.state.net_in_transfer,
        net_out_transfer: v.state.net_out_transfer,
        load_1: v.state.load_1,
        uptime: v.state.uptime,
    }
}

async fn handle_ws(mut socket: WebSocket, state: Arc<AppState>, use_delta: bool) {
    let mut interval = tokio::time::interval(Duration::from_secs(2));
    let mut seq: u64 = 0;
    let mut prev_keys: std::collections::HashMap<i64, ChangeKey> = std::collections::HashMap::new();

    loop {
        tokio::select! {
            _ = interval.tick() => {
                seq += 1;

                let views = build_server_views(&state);

                if !use_delta {
                    // ── Go 前端全量兼容模式 ──
                    let payload = serde_json::json!({
                        "type": "servers",
                        "servers": views,
                    });
                    if socket.send(Message::Text(payload.to_string().into())).await.is_err() { break; }
                } else if seq == 1 {
                    // ── 增量模式：首次全量 ──
                    let payload = serde_json::json!({
                        "type": "servers_full",
                        "seq": seq,
                        "data": views,
                    });
                    if socket.send(Message::Text(payload.to_string().into())).await.is_err() { break; }
                } else {
                    // ── 增量模式：仅差异 ──
                    let mut changed: Vec<&ServerView> = Vec::new();
                    let mut removed: Vec<i64> = Vec::new();
                    let mut current_keys: std::collections::HashMap<i64, ChangeKey> = std::collections::HashMap::new();

                    for v in &views {
                        let key = change_key(v);
                        let is_changed = match prev_keys.get(&v.id) {
                            Some(old) => old != &key,
                            None => true,
                        };
                        if is_changed {
                            changed.push(v);
                        }
                        current_keys.insert(v.id, key);
                    }
                    for id in prev_keys.keys() {
                        if !current_keys.contains_key(id) { removed.push(*id); }
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

                    prev_keys = current_keys;
                    continue;
                }

                // 更新 prev_keys（全量模式 + 增量首次）
                prev_keys = views.iter().map(|v| (v.id, change_key(v))).collect();
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
