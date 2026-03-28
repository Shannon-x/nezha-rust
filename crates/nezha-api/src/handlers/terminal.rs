use axum::{extract::Extension, Json};
use nezha_core::models::common::CommonResponse;
use nezha_service::AppState;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Go 版 TerminalForm — 前端发送的终端创建请求
#[derive(Deserialize)]
pub struct TerminalForm {
    pub server_id: u64,
}

/// Go 版 CreateTerminalResponse — 返回给前端的终端会话信息
#[derive(Serialize)]
pub struct CreateTerminalResponse {
    pub session_id: String,
    pub server_id: u64,
    pub server_name: String,
}

/// 创建终端会话 — POST /api/v1/terminal
/// 与 Go 版 createTerminal 一致：
/// 1. 验证服务器存在且 Agent 已连接
/// 2. 生成 session UUID
/// 3. 保存 session → server_id 映射
/// 4. 返回 { session_id, server_id, server_name }
/// WS 连接后由 ws handler 实际创建 stream 和发 task
pub async fn create(
    Extension(state): Extension<Arc<AppState>>,
    Json(form): Json<TerminalForm>,
) -> Json<CommonResponse<CreateTerminalResponse>> {
    // 查找服务器
    let server_name = match state.servers.get(&form.server_id) {
        Some(s) => s.name.clone(),
        None => return Json(CommonResponse::error("server not found or not connected")),
    };

    // 检查 Agent 是否已连接（通过 task_senders 判断）
    if !state.task_senders.contains_key(&form.server_id) {
        return Json(CommonResponse::error("server not found or not connected"));
    }

    // 生成 session UUID
    let session_id = uuid::Uuid::new_v4().to_string();

    // 保存 session → server_id 映射，供 WS handler 使用
    state.pending_terminals.insert(session_id.clone(), form.server_id);

    Json(CommonResponse::success(CreateTerminalResponse {
        session_id,
        server_id: form.server_id,
        server_name,
    }))
}

/// 创建文件管理会话 — GET /api/v1/file
/// 与终端类似，session 用于后续 WS 连接
pub async fn create_fm(
    Extension(state): Extension<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<TerminalForm>,
) -> Json<CommonResponse<CreateTerminalResponse>> {
    let server_name = match state.servers.get(&params.server_id) {
        Some(s) => s.name.clone(),
        None => return Json(CommonResponse::error("server not found or not connected")),
    };

    if !state.task_senders.contains_key(&params.server_id) {
        return Json(CommonResponse::error("server not found or not connected"));
    }

    let session_id = uuid::Uuid::new_v4().to_string();
    state.pending_terminals.insert(session_id.clone(), params.server_id);

    Json(CommonResponse::success(CreateTerminalResponse {
        session_id,
        server_id: params.server_id,
        server_name,
    }))
}
