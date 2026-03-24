use axum::{
    extract::{Extension, Query},
    response::{IntoResponse, Redirect},
    Json,
};
use chrono::{Duration, Utc};
use jsonwebtoken::{encode, EncodingKey, Header};
use nezha_core::models::common::CommonResponse;
use nezha_service::AppState;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, error};

use super::auth::Claims;

/// OAuth2 查询参数
#[derive(Debug, Deserialize)]
pub struct OAuth2CallbackParams {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

/// OAuth2 登录 — 获取可用的 OAuth2 提供者列表
pub async fn list_providers(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<CommonResponse<Vec<serde_json::Value>>> {
    let cfg = state.config.read().await;
    let providers: Vec<serde_json::Value> = cfg.oauth2
        .iter()
        .map(|(name, c)| {
            serde_json::json!({
                "name": name,
                "redirect_url": c.redirect_url,
                "has_config": !c.client_id.is_empty(),
            })
        })
        .collect();
    Json(CommonResponse::success(providers))
}

/// OAuth2 授权跳转 — 重定向到 OAuth2 Provider
pub async fn authorize(
    Extension(state): Extension<Arc<AppState>>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let provider = params.get("provider").map(|s| s.as_str()).unwrap_or("");

    let cfg = state.config.read().await;
    let oauth2_config = match cfg.oauth2.get(provider) {
        Some(c) if !c.client_id.is_empty() => c,
        _ => return Redirect::to("/api/v1/oauth2/error?msg=invalid_provider").into_response(),
    };

    // 生成 state 防 CSRF
    let csrf_state = nezha_utils::generate_random_string(32);

    // 构建授权 URL
    let scopes = if oauth2_config.scopes.is_empty() {
        "openid profile email".to_string()
    } else {
        oauth2_config.scopes.join(" ")
    };

    let auth_url = format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}",
        oauth2_config.endpoint,
        urlencoding::encode(&oauth2_config.client_id),
        urlencoding::encode(&oauth2_config.redirect_url),
        urlencoding::encode(&scopes),
        csrf_state,
    );

    info!("OAuth2 authorize redirect: provider={}", provider);
    Redirect::to(&auth_url).into_response()
}

/// OAuth2 回调 — 用 code 换取 token，获取用户信息，生成 JWT
pub async fn callback(
    Extension(state): Extension<Arc<AppState>>,
    Query(params): Query<OAuth2CallbackParams>,
) -> Json<CommonResponse<serde_json::Value>> {
    // 检查错误
    if let Some(ref err) = params.error {
        return Json(CommonResponse::error(format!("OAuth2 error: {}", err)));
    }

    let code = match params.code {
        Some(ref c) => c.clone(),
        None => return Json(CommonResponse::error("Missing authorization code")),
    };

    // 查找匹配的 OAuth2 provider（通过 state 或默认第一个）
    let cfg = state.config.read().await;
    let (provider_name, oauth2_config) = match cfg.oauth2.iter().next() {
        Some((name, c)) => (name.clone(), c.clone()),
        None => return Json(CommonResponse::error("No OAuth2 provider configured")),
    };

    // Step 1: 用 code 换取 access_token
    let token_url = format!("{}/token", oauth2_config.endpoint.trim_end_matches("/authorize"));

    let client = reqwest::Client::new();
    let token_resp = client.post(&token_url)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("client_id", &oauth2_config.client_id),
            ("client_secret", &oauth2_config.client_secret),
            ("redirect_uri", &oauth2_config.redirect_url),
        ])
        .send()
        .await;

    let access_token = match token_resp {
        Ok(resp) => {
            let body: serde_json::Value = resp.json().await.unwrap_or_default();
            match body["access_token"].as_str() {
                Some(t) => t.to_string(),
                None => return Json(CommonResponse::error(format!("Token exchange failed: {}", body))),
            }
        }
        Err(e) => return Json(CommonResponse::error(format!("Token request failed: {}", e))),
    };

    // Step 2: 用 access_token 获取用户信息
    let user_info_url = if oauth2_config.user_info_url.is_empty() {
        format!("{}/userinfo", oauth2_config.endpoint.trim_end_matches("/authorize"))
    } else {
        oauth2_config.user_info_url.clone()
    };

    let user_info_resp = client.get(&user_info_url)
        .bearer_auth(&access_token)
        .send()
        .await;

    let user_info: serde_json::Value = match user_info_resp {
        Ok(resp) => resp.json().await.unwrap_or_default(),
        Err(e) => return Json(CommonResponse::error(format!("User info request failed: {}", e))),
    };

    // Step 3: 提取用户 ID
    let user_id_path = if oauth2_config.user_id_path.is_empty() {
        "sub"
    } else {
        &oauth2_config.user_id_path
    };
    let oauth2_user_id = user_info[user_id_path].as_str()
        .or_else(|| user_info[user_id_path].as_i64().map(|_| ""))
        .unwrap_or("")
        .to_string();

    let oauth2_user_id = if oauth2_user_id.is_empty() {
        user_info[user_id_path].to_string()
    } else {
        oauth2_user_id
    };

    if oauth2_user_id.is_empty() || oauth2_user_id == "null" {
        return Json(CommonResponse::error("Could not extract user ID from OAuth2"));
    }

    // Step 4: 查找或创建对应的本地用户
    let oauth_key = format!("{}:{}", provider_name, oauth2_user_id);
    let row: Option<(i64, String, i32)> = sqlx::query_as(
        "SELECT id, username, role FROM users WHERE oauth_id = ?"
    )
    .bind(&oauth_key)
    .fetch_optional(&state.db.pool)
    .await
    .unwrap_or(None);

    let (user_id, username, role) = match row {
        Some((id, name, r)) => (id, name, r),
        None => {
            // 自动创建用户
            let username = user_info["name"].as_str()
                .or_else(|| user_info["login"].as_str())
                .or_else(|| user_info["email"].as_str())
                .unwrap_or(&oauth2_user_id)
                .to_string();

            let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
            let result = sqlx::query(
                "INSERT INTO users (created_at, updated_at, username, password, role, oauth_id) VALUES (?,?,?,?,?,?)"
            )
            .bind(now.as_str()).bind(now.as_str())
            .bind(&username).bind("").bind(0i32).bind(&oauth_key)
            .execute(&state.db.pool).await;

            match result {
                Ok(r) => (r.last_insert_id().unwrap_or(0), username, 0),
                Err(e) => return Json(CommonResponse::error(format!("Create user failed: {}", e))),
            }
        }
    };

    // Step 5: 生成 JWT
    let expire_hours = if cfg.jwt_timeout > 0 {
        cfg.jwt_timeout as i64 * 24
    } else {
        24
    };
    let now = Utc::now();
    let claims = Claims {
        sub: user_id,
        role: role as u8,
        username: username.clone(),
        exp: (now + Duration::hours(expire_hours)).timestamp() as usize,
        iat: now.timestamp() as usize,
    };

    let token = match encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(cfg.jwt_secret_key.as_bytes()),
    ) {
        Ok(t) => t,
        Err(e) => return Json(CommonResponse::error(format!("JWT生成失败: {}", e))),
    };

    info!("OAuth2 login success: provider={} user={}", provider_name, username);

    Json(CommonResponse::success(serde_json::json!({
        "token": token,
        "expire": (now + Duration::hours(expire_hours)).to_rfc3339(),
        "user": {
            "id": user_id,
            "username": username,
            "role": role,
            "oauth_provider": provider_name,
        }
    })))
}
