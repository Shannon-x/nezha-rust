use axum::{extract::Extension, extract::Path, Json};
use nezha_core::models::common::CommonResponse;
use nezha_service::AppState;
use std::sync::Arc;

pub async fn list(Extension(_state): Extension<Arc<AppState>>) -> Json<CommonResponse<Vec<serde_json::Value>>> {
    Json(CommonResponse::success(vec![]))
}

pub async fn create(Extension(_state): Extension<Arc<AppState>>, Json(_body): Json<serde_json::Value>) -> Json<CommonResponse<serde_json::Value>> {
    Json(CommonResponse::success(serde_json::Value::Null))
}

pub async fn update(Extension(_state): Extension<Arc<AppState>>, Path(_id): Path<u64>, Json(_body): Json<serde_json::Value>) -> Json<CommonResponse<serde_json::Value>> {
    Json(CommonResponse::success(serde_json::Value::Null))
}

pub async fn batch_delete(Extension(_state): Extension<Arc<AppState>>, Json(_ids): Json<Vec<u64>>) -> Json<CommonResponse<()>> {
    Json(CommonResponse::success(()))
}

/// DDNS 供应商列表 — GET /api/v1/ddns/providers
pub async fn list_providers(Extension(_state): Extension<Arc<AppState>>) -> Json<CommonResponse<Vec<serde_json::Value>>> {
    // 与 Go 版 ddns.ListProviders 兼容
    let providers = vec![
        serde_json::json!({"name": "cloudflare", "display_name": "Cloudflare"}),
        serde_json::json!({"name": "aliyun", "display_name": "Aliyun"}),
        serde_json::json!({"name": "dnspod", "display_name": "DNSPod"}),
        serde_json::json!({"name": "namesilo", "display_name": "NameSilo"}),
    ];
    Json(CommonResponse::success(providers))
}
