/// WebSocket 服务端推送 — 与 Go 版 Nezha Dashboard 完全兼容
///
/// Go 版格式: StreamServerData { now, online, servers }
/// - now:     当前时间戳（毫秒）
/// - online:  在线用户数
/// - servers: StreamServer 数组
use axum::{
    extract::{ws::{Message, WebSocket}, Extension, Query, WebSocketUpgrade},
    response::IntoResponse,
};
use chrono::Utc;
use nezha_core::models::host::{Host, HostState};
use nezha_service::AppState;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::time::Duration;

/// WebSocket 查询参数
#[derive(Deserialize)]
pub struct WsParams {
    #[serde(default = "default_mode")]
    pub mode: String,
}

fn default_mode() -> String { "full".to_string() }

/// Go 版 StreamServerData — WebSocket 推送的顶层结构
#[derive(Serialize)]
struct StreamServerData {
    now: i64,
    online: u64,
    servers: Vec<StreamServer>,
}

/// Go 版 StreamServer — 单个服务器的 WebSocket 推送格式
#[derive(Clone, Serialize)]
struct StreamServer {
    id: i64,
    name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    public_note: String,
    display_index: i32,
    host: Host,
    state: HostState,
    country_code: String,
    last_active: String,
}

pub async fn server_stream(
    ws: WebSocketUpgrade,
    Extension(state): Extension<Arc<AppState>>,
    Query(_params): Query<WsParams>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, state))
}

/// 从 AppState 构建 StreamServerData（与 Go 版 getServerStat 完全一致）
fn build_stream_data(state: &AppState) -> StreamServerData {
    let mut servers: Vec<StreamServer> = state.servers
        .iter()
        .map(|entry| {
            let s = entry.value();
            StreamServer {
                id: s.id,
                name: s.name.clone(),
                public_note: s.public_note.clone(),
                display_index: s.display_index,
                host: s.host.clone().unwrap_or_default(),
                state: s.state.clone().unwrap_or_default(),
                country_code: s.geoip.as_ref()
                    .map(|g| g.country_code.clone())
                    .unwrap_or_default(),
                last_active: s.last_active
                    .map(|t| t.and_utc().to_rfc3339())
                    .unwrap_or_default(),
            }
        })
        .collect();

    // 按 display_index 降序，再按 id 升序排列（与 Go 版一致）
    servers.sort_by(|a, b| {
        b.display_index.cmp(&a.display_index)
            .then(a.id.cmp(&b.id))
    });

    StreamServerData {
        now: Utc::now().timestamp_millis(),
        online: 0, // TODO: 在线用户计数
        servers,
    }
}

async fn handle_ws(mut socket: WebSocket, state: Arc<AppState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(2));
    let mut count: u64 = 0;

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let data = build_stream_data(&state);

                let json = match serde_json::to_string(&data) {
                    Ok(j) => j,
                    Err(_) => continue,
                };

                if socket.send(Message::Text(json.into())).await.is_err() {
                    break;
                }

                count += 1;
                // 每 4 次发一个 ping（与 Go 版一致）
                if count % 4 == 0 {
                    if socket.send(Message::Ping(vec![].into())).await.is_err() {
                        break;
                    }
                }
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
