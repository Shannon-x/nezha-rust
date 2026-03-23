use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

/// 通用基础模型（对应 Go 的 Common struct）
#[derive(Debug, Clone, Default, Serialize, Deserialize, sqlx::FromRow)]
pub struct Common {
    pub id: i64,
    pub created_at: Option<NaiveDateTime>,
    pub updated_at: Option<NaiveDateTime>,
}

/// 通用 API 响应
#[derive(Debug, Serialize, Deserialize)]
pub struct CommonResponse<T: Serialize> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T: Serialize> CommonResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(msg.into()),
        }
    }
}

/// 分页响应
#[derive(Debug, Serialize, Deserialize)]
pub struct PaginatedResponse<T: Serialize> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<PaginatedData<T>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PaginatedData<T: Serialize> {
    pub items: Vec<T>,
    pub total: i64,
}

/// 任务类型常量
pub const TASK_TYPE_COMMAND: u64 = 0;
pub const TASK_TYPE_HTTP_GET: u64 = 1;
pub const TASK_TYPE_ICMP_PING: u64 = 2;
pub const TASK_TYPE_TCP_PING: u64 = 3;
pub const TASK_TYPE_REPORT_CONFIG: u64 = 10;

/// 检查是否是服务监控相关的任务类型
pub fn is_service_sentinel_needed(task_type: u64) -> bool {
    matches!(
        task_type,
        TASK_TYPE_HTTP_GET | TASK_TYPE_ICMP_PING | TASK_TYPE_TCP_PING
    )
}

/// 规则覆盖范围
pub const RULE_COVER_ALL: u64 = 0;
pub const RULE_COVER_IGNORE_ALL: u64 = 1;

/// 上下文键
pub const CTX_KEY_AUTHORIZED_USER: &str = "authorized_user";
pub const CTX_KEY_REAL_IP: &str = "real_ip";
pub const CTX_KEY_CONNECTING_IP: &str = "connecting_ip";
