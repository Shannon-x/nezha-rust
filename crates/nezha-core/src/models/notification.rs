use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Notification {
    pub id: i64,
    pub created_at: Option<NaiveDateTime>,
    pub updated_at: Option<NaiveDateTime>,
    pub name: String,
    #[serde(default)]
    pub url: String,
    #[serde(default = "default_one")]
    pub request_method: i32,
    #[serde(default = "default_one")]
    pub request_type: i32,
    #[serde(default)]
    pub request_header: String,
    #[serde(default)]
    pub request_body: String,
    #[serde(default)]
    pub skip_check: bool,
}

fn default_one() -> i32 { 1 }

#[derive(Debug, Deserialize)]
pub struct NotificationForm {
    pub name: Option<String>,
    pub url: Option<String>,
    pub request_method: Option<i32>,
    pub request_type: Option<i32>,
    pub request_header: Option<String>,
    pub request_body: Option<String>,
    pub skip_check: Option<bool>,
}
