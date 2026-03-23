use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Waf {
    pub id: i64,
    pub created_at: Option<NaiveDateTime>,
    pub updated_at: Option<NaiveDateTime>,
    pub ip: String,
    pub blocked_at: Option<NaiveDateTime>,
    #[serde(default)]
    pub blocked_reason: String,
    #[serde(default)]
    pub count: i32,
}
