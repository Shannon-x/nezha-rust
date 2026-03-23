use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServiceHistory {
    pub id: i64,
    pub created_at: Option<NaiveDateTime>,
    pub service_id: u64,
    #[serde(default)]
    pub server_id: u64,
    #[serde(default)]
    pub avg_delay: f64,
    #[serde(default)]
    pub up: u64,
    #[serde(default)]
    pub down: u64,
    #[serde(default)]
    pub data: String,
}
