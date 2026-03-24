use axum::extract::Extension;
use axum::http::header::SET_COOKIE;
use axum::response::{IntoResponse, Response};
use axum::Json;
use bcrypt::{hash, verify, DEFAULT_COST};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use nezha_core::models::common::CommonResponse;
use nezha_core::models::user::{LoginForm, LoginResponse};
use nezha_service::AppState;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// JWT Claims
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: i64,       // user id
    pub role: u8,       // 0=member, 1=admin
    pub username: String,
    pub exp: usize,     // expiration timestamp
    pub iat: usize,     // issued at
}

/// 登录 — 兼容 Go 版 gin-jwt 行为：返回 JSON + 设置 nz-jwt cookie
pub async fn login(
    Extension(state): Extension<Arc<AppState>>,
    Json(form): Json<LoginForm>,
) -> Response {
    // 查询用户
    let row: Option<(i64, String, String, i32)> = sqlx::query_as(
        "SELECT id, username, password, role FROM users WHERE username = ?"
    )
    .bind(&form.username)
    .fetch_optional(&state.db.pool)
    .await
    .unwrap_or(None);

    let (user_id, username, password_hash, role) = match row {
        Some(r) => r,
        None => {
            tracing::warn!("Login failed: user '{}' not found", form.username);
            return Json(CommonResponse::<LoginResponse>::error("用户名或密码错误")).into_response();
        }
    };

    // 验证密码
    match verify(&form.password, &password_hash) {
        Ok(true) => {}
        Ok(false) => {
            tracing::warn!("Login failed: wrong password for user '{}'", username);
            return Json(CommonResponse::<LoginResponse>::error("用户名或密码错误")).into_response();
        }
        Err(e) => {
            tracing::error!("Login failed: bcrypt error for user '{}': {}", username, e);
            return Json(CommonResponse::<LoginResponse>::error("用户名或密码错误")).into_response();
        }
    }

    // 生成 JWT
    let now = Utc::now();
    let expire_hours = if state.config.jwt_timeout > 0 {
        state.config.jwt_timeout as i64 * 24
    } else {
        24
    };

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
        &EncodingKey::from_secret(state.config.jwt_secret_key.as_bytes()),
    ) {
        Ok(t) => t,
        Err(e) => {
            return Json(CommonResponse::<LoginResponse>::error(format!("JWT生成失败: {}", e))).into_response();
        }
    };

    tracing::info!("User '{}' logged in successfully", username);

    // 构建响应 JSON
    let body = CommonResponse::success(LoginResponse {
        token: token.clone(),
        expire: (now + Duration::hours(expire_hours)).to_rfc3339(),
    });

    // 设置 nz-jwt cookie（与 Go 版 gin-jwt 兼容）
    let max_age = expire_hours * 3600;
    let cookie = format!(
        "nz-jwt={}; Path=/; Max-Age={}; HttpOnly; SameSite=Lax",
        token, max_age
    );

    let mut response = Json(body).into_response();
    response.headers_mut().insert(
        SET_COOKIE,
        cookie.parse().unwrap(),
    );
    response
}

/// 获取当前用户信息
pub async fn get_profile(
    Extension(state): Extension<Arc<AppState>>,
    Extension(claims): Extension<Claims>,
) -> Json<CommonResponse<serde_json::Value>> {
    let row: Option<(i64, String, i32, String)> = sqlx::query_as(
        "SELECT id, username, role, COALESCE(agent_secret,'') FROM users WHERE id = ?"
    )
    .bind(claims.sub)
    .fetch_optional(&state.db.pool)
    .await
    .unwrap_or(None);

    match row {
        Some((id, username, role, agent_secret)) => {
            Json(CommonResponse::success(serde_json::json!({
                "id": id,
                "username": username,
                "role": role,
                "agent_secret": agent_secret,
            })))
        }
        None => Json(CommonResponse::error("用户不存在")),
    }
}

/// 从请求中提取并验证 JWT Claims
pub fn decode_token(token: &str, secret: &str) -> Option<Claims> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .ok()
    .map(|data| data.claims)
}

/// 刷新 Token — GET /api/v1/refresh-token
/// 与 Go 版 gin-jwt RefreshHandler 兼容
pub async fn refresh_token(
    Extension(state): Extension<Arc<AppState>>,
    Extension(claims): Extension<Claims>,
) -> Response {
    let now = Utc::now();
    let expire_hours = if state.config.jwt_timeout > 0 {
        state.config.jwt_timeout as i64 * 24
    } else {
        24
    };

    let new_claims = Claims {
        sub: claims.sub,
        role: claims.role,
        username: claims.username.clone(),
        exp: (now + Duration::hours(expire_hours)).timestamp() as usize,
        iat: now.timestamp() as usize,
    };

    let token = match encode(
        &Header::default(),
        &new_claims,
        &EncodingKey::from_secret(state.config.jwt_secret_key.as_bytes()),
    ) {
        Ok(t) => t,
        Err(e) => {
            return Json(CommonResponse::<LoginResponse>::error(format!("JWT刷新失败: {}", e))).into_response();
        }
    };

    let body = CommonResponse::success(LoginResponse {
        token: token.clone(),
        expire: (now + Duration::hours(expire_hours)).to_rfc3339(),
    });

    let max_age = expire_hours * 3600;
    let cookie = format!(
        "nz-jwt={}; Path=/; Max-Age={}; HttpOnly; SameSite=Lax",
        token, max_age
    );

    let mut response = Json(body).into_response();
    response.headers_mut().insert(
        SET_COOKIE,
        cookie.parse().unwrap(),
    );
    response
}

/// 更新用户资料 — POST /api/v1/profile
pub async fn update_profile(
    Extension(state): Extension<Arc<AppState>>,
    Extension(claims): Extension<Claims>,
    Json(body): Json<serde_json::Value>,
) -> Json<CommonResponse<()>> {
    let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    if let Some(new_password) = body.get("new_password").and_then(|v| v.as_str()) {
        if !new_password.is_empty() {
            if let Ok(hashed) = hash(new_password, DEFAULT_COST) {
                let _ = sqlx::query("UPDATE users SET password = ?, updated_at = ? WHERE id = ?")
                    .bind(&hashed)
                    .bind(now.as_str())
                    .bind(claims.sub)
                    .execute(&state.db.pool)
                    .await;
            }
        }
    }

    if let Some(new_username) = body.get("username").and_then(|v| v.as_str()) {
        if !new_username.is_empty() {
            let _ = sqlx::query("UPDATE users SET username = ?, updated_at = ? WHERE id = ?")
                .bind(new_username)
                .bind(now.as_str())
                .bind(claims.sub)
                .execute(&state.db.pool)
                .await;
        }
    }

    Json(CommonResponse::success(()))
}

/// 创建初始管理员账户（如果不存在）
pub async fn ensure_admin(state: &AppState) -> anyhow::Result<()> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(&state.db.pool)
        .await?;

    if count.0 == 0 {
        let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let password_hash = hash("admin", DEFAULT_COST)?;
        sqlx::query(
            "INSERT INTO users (created_at, updated_at, username, password, role) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(now.as_str())
        .bind(now.as_str())
        .bind("admin")
        .bind(&password_hash)
        .bind(0i32)  // Go 版 role=0 为管理员，前端 profile.role === 0 判断管理员
        .execute(&state.db.pool)
        .await?;
        tracing::info!("NEZHA>> 已创建默认管理员账户 admin/admin");
    }
    Ok(())
}

