use axum::{
    extract::Extension,
    middleware,
    routing::{delete, get, patch, post},
    Router,
};
use nezha_service::AppState;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

use crate::handlers;

/// 创建 HTTP API 路由（含静态文件服务）
pub fn create_router(state: Arc<AppState>) -> Router {
    // ── 公开路由 ──
    let public = Router::new()
        .route("/api/v1/login", post(handlers::auth::login))
        .route("/api/v1/setting", get(handlers::setting::get_config))
        .route("/api/v1/oauth2/providers", get(handlers::oauth2::list_providers))
        .route("/api/v1/oauth2/authorize", get(handlers::oauth2::authorize))
        .route("/api/v1/oauth2/callback", get(handlers::oauth2::callback));

    // ── 可选认证路由 ──
    let optional_auth = Router::new()
        .route("/api/v1/ws/server", get(crate::ws::server_stream))
        .route("/api/v1/server-group", get(handlers::server_group::list))
        .route("/api/v1/service", get(handlers::service::show))
        .route("/api/v1/service/{id}", get(handlers::service::history))
        .route("/api/v1/service/{id}/history", get(handlers::service::get_history))
        .route("/api/v1/server/{id}/metrics", get(handlers::server::get_metrics))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            |ext: Extension<Arc<AppState>>, req, next| crate::middleware::auth_optional(ext, req, next),
        ));

    // ── 需认证路由 ──
    let auth = Router::new()
        .route("/api/v1/profile", get(handlers::auth::get_profile))
        // 服务器
        .route("/api/v1/server", get(handlers::server::list).post(handlers::server::create))
        .route("/api/v1/server/{id}", patch(handlers::server::update))
        .route("/api/v1/batch-delete/server", post(handlers::server::batch_delete))
        // 服务监控
        .route("/api/v1/service/list", get(handlers::service::list))
        .route("/api/v1/service/create", post(handlers::service::create))
        .route("/api/v1/service/update/{id}", patch(handlers::service::update))
        .route("/api/v1/batch-delete/service", post(handlers::service::batch_delete))
        // 通知
        .route("/api/v1/notification", get(handlers::notification::list).post(handlers::notification::create))
        .route("/api/v1/notification/{id}", patch(handlers::notification::update))
        .route("/api/v1/batch-delete/notification", post(handlers::notification::batch_delete))
        .route("/api/v1/notification-group", get(handlers::setting::list_notification_groups))
        // 告警
        .route("/api/v1/alert-rule", get(handlers::alert_rule::list).post(handlers::alert_rule::create))
        .route("/api/v1/alert-rule/{id}", patch(handlers::alert_rule::update))
        .route("/api/v1/batch-delete/alert-rule", post(handlers::alert_rule::batch_delete))
        // 定时任务
        .route("/api/v1/cron", get(handlers::cron::list).post(handlers::cron::create))
        .route("/api/v1/cron/{id}", patch(handlers::cron::update))
        .route("/api/v1/batch-delete/cron", post(handlers::cron::batch_delete))
        // DDNS / NAT
        .route("/api/v1/ddns", get(handlers::ddns::list).post(handlers::ddns::create))
        .route("/api/v1/ddns/{id}", patch(handlers::ddns::update))
        .route("/api/v1/batch-delete/ddns", post(handlers::ddns::batch_delete))
        .route("/api/v1/nat", get(handlers::nat::list).post(handlers::nat::create))
        .route("/api/v1/nat/{id}", patch(handlers::nat::update))
        .route("/api/v1/batch-delete/nat", post(handlers::nat::batch_delete))
        // 用户管理
        .route("/api/v1/user", get(handlers::setting::list_users).post(handlers::user::create))
        .route("/api/v1/user/{id}", patch(handlers::user::update))
        .route("/api/v1/batch-delete/user", post(handlers::user::batch_delete))
        // 设置
        .route("/api/v1/setting/update", patch(handlers::setting::update_config))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            |ext: Extension<Arc<AppState>>, req, next| crate::middleware::auth_required(ext, req, next),
        ));

    // ── 静态文件服务 ──
    let resource_dir = std::env::var("NZ_RESOURCE_DIR").unwrap_or_else(|_| "resource".to_string());

    // Admin 前端 SPA: /dashboard/* → resource/admin/
    // 对所有非静态文件路由回退到 index.html（SPA 客户端路由）
    let admin_dir = format!("{}/admin", resource_dir);
    let admin_index = format!("{}/admin/index.html", resource_dir);
    let admin_spa = ServeDir::new(&admin_dir)
        .append_index_html_on_directories(true)
        .fallback(ServeFile::new(&admin_index));

    // User 前端 SPA: /* → resource/ (回退到 index.html)
    let user_index = format!("{}/index.html", resource_dir);
    let user_spa = ServeDir::new(&resource_dir)
        .append_index_html_on_directories(true)
        .fallback(ServeFile::new(&user_index));

    Router::new()
        .merge(public)
        .merge(optional_auth)
        .merge(auth)
        .nest_service("/dashboard", admin_spa)
        .fallback_service(user_spa)
        .layer(CorsLayer::permissive())
        .layer(Extension(state))
}

