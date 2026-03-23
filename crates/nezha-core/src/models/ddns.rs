use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DdnsProfile {
    pub id: i64,
    pub created_at: Option<NaiveDateTime>,
    pub updated_at: Option<NaiveDateTime>,
    pub name: String,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub access_id: String,
    #[serde(default)]
    pub access_secret: String,
    #[serde(default)]
    pub webhook_url: String,
    #[serde(default)]
    pub webhook_method: String,
    #[serde(default)]
    pub webhook_request_type: i32,
    #[serde(default)]
    pub webhook_request_body: String,
    #[serde(default)]
    pub webhook_headers: String,
    #[serde(default)]
    pub domains: Vec<String>,
    #[serde(default = "default_max_retries")]
    pub max_retries: i32,
    #[serde(default = "default_true")]
    pub enable_ipv4: bool,
    #[serde(default)]
    pub enable_ipv6: bool,
}

fn default_max_retries() -> i32 { 3 }
fn default_true() -> bool { true }

#[derive(Debug, Deserialize)]
pub struct DdnsForm {
    pub name: Option<String>,
    pub provider: Option<String>,
    pub access_id: Option<String>,
    pub access_secret: Option<String>,
    pub webhook_url: Option<String>,
    pub webhook_method: Option<String>,
    pub domains: Option<Vec<String>>,
    pub max_retries: Option<i32>,
    pub enable_ipv4: Option<bool>,
    pub enable_ipv6: Option<bool>,
}
