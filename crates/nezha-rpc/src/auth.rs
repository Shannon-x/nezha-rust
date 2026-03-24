use nezha_service::AppState;
use std::sync::Arc;
use tonic::{metadata::MetadataMap, Status};

/// 从 gRPC metadata 中验证 Agent 身份
/// 支持两种模式：
/// - client_uuid: Agent-Rust 发送 UUID 字符串，按 server.uuid 查找
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
        // 按 UUID 查找服务器
        state.servers.iter()
            .find(|e| e.value().uuid == uuid)
            .map(|e| *e.key())
            .ok_or_else(|| Status::not_found(format!("server not found for uuid: {}", uuid)))?
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
