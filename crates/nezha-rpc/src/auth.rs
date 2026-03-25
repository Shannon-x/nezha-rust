use nezha_core::models::server::Server;
use nezha_service::AppState;
use std::sync::Arc;
use tonic::{metadata::MetadataMap, Status};

/// 从 gRPC metadata 中验证 Agent 身份
/// 支持两种模式：
/// - client_uuid: Agent-Rust 发送 UUID 字符串，按 server.uuid 查找，找不到则自动注册
/// - client_id: 旧版 Agent 发送数字 ID，直接按 key 查找
pub async fn check_auth(metadata: &MetadataMap, state: &Arc<AppState>) -> Result<u64, Status> {
    let secret = metadata
        .get("client_secret")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| Status::unauthenticated("missing client_secret"))?;

    let agent_key = state.config.read().await.agent_secret_key.clone();
    if secret != agent_key {
        return Err(Status::unauthenticated("invalid secret key"));
    }

    // 优先读 client_uuid（Agent-Rust），fallback 到 client_id（旧版）
    let uuid_str = metadata.get("client_uuid").and_then(|v| v.to_str().ok());
    let id_str = metadata.get("client_id").and_then(|v| v.to_str().ok());

    let client_id = if let Some(uuid) = uuid_str {
        // 按 UUID 查找服务器，找不到则自动注册
        match state.servers.iter().find(|e| e.value().uuid == uuid).map(|e| *e.key()) {
            Some(id) => id,
            None => auto_register_server(state, uuid).await?,
        }
    } else if let Some(id) = id_str {
        let id = id.parse::<u64>()
            .map_err(|_| Status::unauthenticated("invalid client_id"))?;
        if !state.servers.contains_key(&id) {
            return Err(Status::not_found("server not found"));
        }
        id
    } else {
        return Err(Status::unauthenticated("missing client_uuid or client_id"));
    };

    Ok(client_id)
}

/// 自动注册一个新服务器（Agent 首次上报时调用）
/// 使用 UUID 前 8 位作为默认名称
async fn auto_register_server(state: &Arc<AppState>, uuid: &str) -> Result<u64, Status> {
    let name = format!("Agent-{}", &uuid[..8.min(uuid.len())]);
    let now = chrono::Utc::now().naive_utc().format("%Y-%m-%d %H:%M:%S").to_string();

    tracing::info!("自动注册新服务器: name={}, uuid={}", name, uuid);

    let result = sqlx::query(
        "INSERT INTO servers (created_at, updated_at, name, uuid, note, display_index, hide_for_guest, enable_ddns) VALUES (?,?,?,?,?,?,?,?)"
    )
    .bind(now.as_str())
    .bind(now.as_str())
    .bind(&name)
    .bind(uuid)
    .bind("")
    .bind(0i32)
    .bind(0i32)
    .bind(0i32)
    .execute(&state.db.pool)
    .await
    .map_err(|e| {
        tracing::error!("自动注册服务器失败: {}", e);
        Status::internal(format!("failed to auto-register server: {}", e))
    })?;

    let id = result.last_insert_id().unwrap_or(0) as u64;

    // 插入内存
    let mut server = Server::default();
    server.id = id as i64;
    server.name = name;
    server.uuid = uuid.to_string();
    state.servers.insert(id, server);

    tracing::info!("服务器自动注册成功: id={}, uuid={}", id, uuid);
    Ok(id)
}
