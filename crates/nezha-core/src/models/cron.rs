use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Cron {
    pub id: i64,
    pub created_at: Option<NaiveDateTime>,
    pub updated_at: Option<NaiveDateTime>,
    pub name: String,
    #[serde(default)]
    pub task_type: i32,
    #[serde(default)]
    pub scheduler: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub servers: Vec<u64>,
    #[serde(default)]
    pub cover: i32,
    #[serde(default)]
    pub push_successful: bool,
    #[serde(default)]
    pub notification_group_id: u64,
    pub last_executed_at: Option<NaiveDateTime>,
    #[serde(default)]
    pub last_result: bool,
    #[serde(skip)]
    pub cron_job_id: Option<u64>,
}

impl Cron {
    pub fn cron_spec(&self) -> &str { &self.scheduler }
}

#[derive(Debug, Deserialize)]
pub struct CronForm {
    pub name: Option<String>,
    pub task_type: Option<i32>,
    pub scheduler: Option<String>,
    pub command: Option<String>,
    pub servers: Option<Vec<u64>>,
    pub cover: Option<i32>,
    pub push_successful: Option<bool>,
    pub notification_group_id: Option<u64>,
}
