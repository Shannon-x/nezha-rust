use axum::{extract::Extension, Json};
use bcrypt::{hash, verify, DEFAULT_COST};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use nezha_core::models::common::CommonResponse;
use nezha_core::models::user::{LoginForm, LoginResponse, User};
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

/// 登录
pub async fn login(
    Extension(state): Extension<Arc<AppState>>,
    Json(form): Json<LoginForm>,
) -> Json<CommonResponse<LoginResponse>> {
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
        None => return Json(CommonResponse::error("用户名或密码错误")),
    };

    // 验证密码
    match verify(&form.password, &password_hash) {
        Ok(true) => {}
        _ => return Json(CommonResponse::error("用户名或密码错误")),
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
        Err(e) => return Json(CommonResponse::error(format!("JWT生成失败: {}", e))),
    };

    Json(CommonResponse::success(LoginResponse {
        token,
        expire: (now + Duration::hours(expire_hours)).to_rfc3339(),
    }))
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
        .bind(1i32)
        .execute(&state.db.pool)
        .await?;
        tracing::info!("NEZHA>> 已创建默认管理员账户 admin/admin");
    }
    Ok(())
}
