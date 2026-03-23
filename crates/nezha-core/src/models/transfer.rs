use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Transfer {
    pub id: i64,
    pub created_at: Option<NaiveDateTime>,
    pub server_id: i64,
    #[serde(rename = "in")]
    pub transfer_in: u64,
    #[serde(rename = "out")]
    pub transfer_out: u64,
}
