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

