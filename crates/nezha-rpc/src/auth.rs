use nezha_service::AppState;
use std::sync::Arc;
use tonic::{metadata::MetadataMap, Status};

/// 从 gRPC metadata 中验证 Agent 身份
pub fn check_auth(metadata: &MetadataMap, state: &Arc<AppState>) -> Result<u64, Status> {
    let secret = metadata
        .get("client_secret")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| Status::unauthenticated("missing client_secret"))?;

    if secret != state.config.agent_secret_key {
        return Err(Status::unauthenticated("invalid secret key"));
    }

    let client_id = metadata
        .get("client_id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or_else(|| Status::unauthenticated("missing or invalid client_id"))?;

    if !state.servers.contains_key(&client_id) {
        return Err(Status::not_found("server not found"));
    }

    Ok(client_id)
}
