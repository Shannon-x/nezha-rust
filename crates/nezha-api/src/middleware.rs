use axum::{
    extract::{Extension, Request},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use dashmap::DashMap;
use nezha_core::models::common::CommonResponse;
use nezha_service::AppState;
use std::sync::Arc;
use std::time::Instant;

use crate::handlers::auth::{Claims, decode_token};

/// JWT 认证中间件 — 必须登录
pub async fn auth_required(
    Extension(state): Extension<Arc<AppState>>,
    mut request: Request,
    next: Next,
) -> Response {
    let token = extract_token(&request);

    let token = match token {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(CommonResponse::<()>::error("未授权，请先登录")),
            )
                .into_response();
        }
    };

    let jwt_secret = state.config.read().await.jwt_secret_key.clone();
    match decode_token(&token, &jwt_secret) {
        Some(claims) => {
            request.extensions_mut().insert(claims);
            next.run(request).await
        }
        None => (
            StatusCode::UNAUTHORIZED,
            Json(CommonResponse::<()>::error("Token 无效或已过期")),
        )
            .into_response(),
    }
}

/// JWT 认证中间件 — 可选认证（有 token 就解析，没有也放行）
pub async fn auth_optional(
    Extension(state): Extension<Arc<AppState>>,
    mut request: Request,
    next: Next,
) -> Response {
    if let Some(token) = extract_token(&request) {
        let jwt_secret = state.config.read().await.jwt_secret_key.clone();
        if let Some(claims) = decode_token(&token, &jwt_secret) {
            request.extensions_mut().insert(claims);
        }
    }
    next.run(request).await
}

/// 管理员权限中间件
pub async fn admin_required(
    request: Request,
    next: Next,
) -> Response {
    let claims = request.extensions().get::<Claims>().cloned();
    match claims {
        Some(c) if c.role == 0 => next.run(request).await,  // Go 版 role=0 为管理员
        _ => (
            StatusCode::FORBIDDEN,
            Json(CommonResponse::<()>::error("需要管理员权限")),
        )
            .into_response(),
    }
}

/// IP 限频中间件（简化版 WAF）
pub struct RateLimiter {
    requests: DashMap<String, (u32, Instant)>,
    max_requests: u32,
    window_secs: u64,
}

impl RateLimiter {
    pub fn new(max_requests: u32, window_secs: u64) -> Self {
        Self {
            requests: DashMap::new(),
            max_requests,
            window_secs,
        }
    }

    pub fn check(&self, ip: &str) -> bool {
        let now = Instant::now();
        let mut entry = self.requests.entry(ip.to_string()).or_insert((0, now));
        let (count, start) = entry.value_mut();

        if now.duration_since(*start).as_secs() > self.window_secs {
            *count = 1;
            *start = now;
            true
        } else if *count < self.max_requests {
            *count += 1;
            true
        } else {
            false
        }
    }
}

/// 从请求中提取 JWT Token
/// 查找顺序与 Go 版 gin-jwt 一致：header: Authorization, query: token, cookie: nz-jwt
fn extract_token(request: &Request) -> Option<String> {
    // 1. 从 Authorization header 获取 (Bearer token)
    if let Some(auth) = request.headers().get("Authorization") {
        if let Ok(auth_str) = auth.to_str() {
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                return Some(token.to_string());
            }
            // 也支持 "Token xxx" 格式（API Token）
            if let Some(token) = auth_str.strip_prefix("Token ") {
                return Some(token.to_string());
            }
        }
    }
    // 2. 从 query string 获取 token 参数
    if let Some(query) = request.uri().query() {
        for pair in query.split('&') {
            if let Some(token) = pair.strip_prefix("token=") {
                return Some(token.to_string());
            }
        }
    }
    // 3. 从 nz-jwt cookie 获取（与 Go 版 gin-jwt 兼容）
    if let Some(cookie_header) = request.headers().get("cookie") {
        if let Ok(cookies) = cookie_header.to_str() {
            for cookie in cookies.split(';') {
                let cookie = cookie.trim();
                if let Some(token) = cookie.strip_prefix("nz-jwt=") {
                    if !token.is_empty() {
                        return Some(token.to_string());
                    }
                }
            }
        }
    }
    None
}
