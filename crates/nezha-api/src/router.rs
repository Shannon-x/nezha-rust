use axum::{
    extract::Extension,
    middleware,
    routing::{get, patch, post},
    Router,
};
use nezha_service::AppState;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

use crate::handlers;

/// 创建 HTTP API 路由（含静态文件服务）
/// 路由与 Go 版 Nezha v1 controller.go 完全一致
pub fn create_router(state: Arc<AppState>) -> Router {
    // ── 公开路由 ──
    let public = Router::new()
        .route("/api/v1/login", post(handlers::auth::login))
        .route("/api/v1/oauth2/{provider}", get(handlers::oauth2::authorize))
        .route("/api/v1/oauth2/callback", get(handlers::oauth2::callback));

    // ── 可选认证路由（fallbackAuth）──
    // Go: fallbackAuth.GET("/setting", ...)
    // Go: fallbackAuth.GET("/oauth2/callback", ...)
    let fallback_auth = Router::new()
        .route("/api/v1/setting", get(handlers::setting::get_config))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            |ext: Extension<Arc<AppState>>, req, next| crate::middleware::auth_optional(ext, req, next),
        ));

    // ── 可选认证路由（optionalAuth）──
    let optional_auth = Router::new()
        .route("/api/v1/ws/server", get(crate::ws::server_stream))
        .route("/api/v1/server-group", get(handlers::server_group::list))
        .route("/api/v1/service", get(handlers::service::show))
        .route("/api/v1/service/server", get(handlers::service::list_server_with_services))
        .route("/api/v1/service/{id}/history", get(handlers::service::get_history))
        .route("/api/v1/server/{id}/service", get(handlers::service::list_server_services))
        .route("/api/v1/server/{id}/metrics", get(handlers::server::get_metrics))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            |ext: Extension<Arc<AppState>>, req, next| crate::middleware::auth_optional(ext, req, next),
        ));

    // ── 需认证路由（auth）──
    let auth = Router::new()
        // Token 刷新
        .route("/api/v1/refresh-token", get(handlers::auth::refresh_token))

        // 用户资料
        .route("/api/v1/profile", get(handlers::auth::get_profile).post(handlers::auth::update_profile))

        // 用户管理 (admin)
        .route("/api/v1/user", get(handlers::setting::list_users).post(handlers::user::create))
        .route("/api/v1/batch-delete/user", post(handlers::user::batch_delete))

        // 服务监控 — 路径与 Go 版一致
        .route("/api/v1/service/list", get(handlers::service::list))
        .route("/api/v1/service", post(handlers::service::create))
        .route("/api/v1/service/{id}", patch(handlers::service::update))
        .route("/api/v1/batch-delete/service", post(handlers::service::batch_delete))

        // 服务器组
        .route("/api/v1/server-group", post(handlers::server_group::create))
        .route("/api/v1/server-group/{id}", patch(handlers::server_group::update))
        .route("/api/v1/batch-delete/server-group", post(handlers::server_group::batch_delete))

        // 通知组
        .route("/api/v1/notification-group", get(handlers::setting::list_notification_groups).post(handlers::setting::create_notification_group))
        .route("/api/v1/notification-group/{id}", patch(handlers::setting::update_notification_group))
        .route("/api/v1/batch-delete/notification-group", post(handlers::setting::batch_delete_notification_group))

        // 服务器
        .route("/api/v1/server", get(handlers::server::list).post(handlers::server::create))
        .route("/api/v1/server/{id}", patch(handlers::server::update))
        .route("/api/v1/server/config/{id}", get(handlers::server::get_config))
        .route("/api/v1/server/config", post(handlers::server::set_config))
        .route("/api/v1/batch-delete/server", post(handlers::server::batch_delete))
        .route("/api/v1/batch-move/server", post(handlers::server::batch_move))
        .route("/api/v1/force-update/server", post(handlers::server::force_update))

        // 通知
        .route("/api/v1/notification", get(handlers::notification::list).post(handlers::notification::create))
        .route("/api/v1/notification/{id}", patch(handlers::notification::update))
        .route("/api/v1/batch-delete/notification", post(handlers::notification::batch_delete))

        // 告警规则
        .route("/api/v1/alert-rule", get(handlers::alert_rule::list).post(handlers::alert_rule::create))
        .route("/api/v1/alert-rule/{id}", patch(handlers::alert_rule::update))
        .route("/api/v1/batch-delete/alert-rule", post(handlers::alert_rule::batch_delete))

        // 定时任务
        .route("/api/v1/cron", get(handlers::cron::list).post(handlers::cron::create))
        .route("/api/v1/cron/{id}", patch(handlers::cron::update))
        .route("/api/v1/cron/{id}/manual", get(handlers::cron::manual_trigger))
        .route("/api/v1/batch-delete/cron", post(handlers::cron::batch_delete))

        // DDNS
        .route("/api/v1/ddns", get(handlers::ddns::list).post(handlers::ddns::create))
        .route("/api/v1/ddns/{id}", patch(handlers::ddns::update))
        .route("/api/v1/ddns/providers", get(handlers::ddns::list_providers))
        .route("/api/v1/batch-delete/ddns", post(handlers::ddns::batch_delete))

        // NAT
        .route("/api/v1/nat", get(handlers::nat::list).post(handlers::nat::create))
        .route("/api/v1/nat/{id}", patch(handlers::nat::update))
        .route("/api/v1/batch-delete/nat", post(handlers::nat::batch_delete))

        // WAF
        .route("/api/v1/waf", get(handlers::setting::list_waf))
        .route("/api/v1/batch-delete/waf", post(handlers::setting::batch_delete_waf))

        // 在线用户
        .route("/api/v1/online-user", get(handlers::setting::list_online_users))
        .route("/api/v1/online-user/batch-block", post(handlers::setting::batch_block_online_user))

        // 设置
        .route("/api/v1/setting", patch(handlers::setting::update_config))
        .route("/api/v1/maintenance", post(handlers::setting::run_maintenance))


        .layer(middleware::from_fn_with_state(
            state.clone(),
            |ext: Extension<Arc<AppState>>, req, next| crate::middleware::auth_required(ext, req, next),
        ));

    // ── 静态文件服务 ──
    let resource_dir = std::env::var("NZ_RESOURCE_DIR").unwrap_or_else(|_| "resource".to_string());

    // Admin 前端 SPA: /dashboard/* → resource/admin/
    let admin_dir = format!("{}/admin", resource_dir);
    let admin_index = format!("{}/admin/index.html", resource_dir);
    let admin_spa = ServeDir::new(&admin_dir)
        .append_index_html_on_directories(true)
        .fallback(ServeFile::new(&admin_index));

    // User 前端 SPA: /* → resource/
    let user_index = format!("{}/index.html", resource_dir);
    let user_spa = ServeDir::new(&resource_dir)
        .append_index_html_on_directories(true)
        .fallback(ServeFile::new(&user_index));

    Router::new()
        .merge(public)
        .merge(fallback_auth)
        .merge(optional_auth)
        .merge(auth)
        .nest_service("/dashboard", admin_spa)
        .fallback_service(user_spa)
        .layer(CorsLayer::permissive())
        .layer(Extension(state))
}
