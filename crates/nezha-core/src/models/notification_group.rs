use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NotificationGroup {
    pub id: i64,
    pub created_at: Option<NaiveDateTime>,
    pub updated_at: Option<NaiveDateTime>,
    pub name: String,
    #[serde(default)]
    pub notifications: Vec<u64>,
}

#[derive(Debug, Deserialize)]
pub struct NotificationGroupForm {
    pub name: Option<String>,
    pub notifications: Option<Vec<u64>>,
}
