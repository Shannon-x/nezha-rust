use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

/// 用户角色
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[repr(u8)]
pub enum UserRole {
    #[default]
    Member = 0,
    Admin = 1,
}

impl UserRole {
    pub fn is_admin(&self) -> bool {
        matches!(self, UserRole::Admin)
    }
}

impl From<i32> for UserRole {
    fn from(v: i32) -> Self {
        match v {
            1 => UserRole::Admin,
            _ => UserRole::Member,
        }
    }
}

/// 用户模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub created_at: Option<NaiveDateTime>,
    pub updated_at: Option<NaiveDateTime>,
    pub username: String,
    #[serde(skip_serializing)]
    pub password: String,
    pub role: UserRole,
    #[serde(default)]
    pub agent_secret: String,
}

impl Default for User {
    fn default() -> Self {
        Self {
            id: 0,
            created_at: None,
            updated_at: None,
            username: String::new(),
            password: String::new(),
            role: UserRole::Member,
            agent_secret: String::new(),
        }
    }
}

/// 用户 API 请求
#[derive(Debug, Deserialize)]
pub struct UserForm {
    pub username: Option<String>,
    pub password: Option<String>,
    pub role: Option<u8>,
}

/// 登录请求
#[derive(Debug, Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
}

/// 登录响应
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub expire: String,
}

/// 用户资料响应
#[derive(Debug, Serialize)]
pub struct ProfileResponse {
    pub user: User,
}
