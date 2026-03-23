use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Nat {
    pub id: i64,
    pub created_at: Option<NaiveDateTime>,
    pub updated_at: Option<NaiveDateTime>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub server_id: i64,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub domain: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool { true }

#[derive(Debug, Deserialize)]
pub struct NatForm {
    pub name: Option<String>,
    pub server_id: Option<i64>,
    pub host: Option<String>,
    pub domain: Option<String>,
    pub enabled: Option<bool>,
}
