use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 服务监控模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Service {
    pub id: i64,
    pub created_at: Option<NaiveDateTime>,
    pub updated_at: Option<NaiveDateTime>,
    pub name: String,
    #[serde(default)]
    pub r#type: i32,
    #[serde(default)]
    pub target: String,
    #[serde(default = "default_duration")]
    pub duration: i32,
    #[serde(default)]
    pub notification_group_id: u64,
    #[serde(default)]
    pub cover: i32,
    #[serde(default)]
    pub notify: bool,
    #[serde(default)]
    pub skip_servers: HashMap<u64, bool>,
    #[serde(default)]
    pub fail_trigger_tasks: Vec<u64>,
    #[serde(default)]
    pub recover_trigger_tasks: Vec<u64>,
    #[serde(default)]
    pub min_latency: f32,
    #[serde(default)]
    pub max_latency: f32,
    #[serde(default)]
    pub latency_notify: bool,
    #[serde(default)]
    pub enable_trigger_task: bool,
    #[serde(default)]
    pub enable_show_in_service: bool,
    #[serde(default)]
    pub display_index: i32,

    // 运行时字段（不入库）
    #[serde(skip)]
    pub cron_job_id: Option<u64>,
    #[serde(skip)]
    pub last_check: Option<chrono::NaiveDateTime>,
    #[serde(skip)]
    pub current_up: bool,
    #[serde(skip)]
    pub current_down: bool,
    #[serde(skip)]
    pub delay: f64,
}

fn default_duration() -> i32 { 30 }

impl Default for Service {
    fn default() -> Self {
        Self {
            id: 0,
            created_at: None,
            updated_at: None,
            name: String::new(),
            r#type: 0,
            target: String::new(),
            duration: 30,
            notification_group_id: 0,
            cover: 0,
            notify: false,
            skip_servers: HashMap::new(),
            fail_trigger_tasks: Vec::new(),
            recover_trigger_tasks: Vec::new(),
            min_latency: 0.0,
            max_latency: 0.0,
            latency_notify: false,
            enable_trigger_task: false,
            enable_show_in_service: false,
            display_index: 0,
            cron_job_id: None,
            last_check: None,
            current_up: false,
            current_down: false,
            delay: 0.0,
        }
    }
}

impl Service {
    /// 生成 cron 调度表达式
    pub fn cron_spec(&self) -> String {
        format!("@every {}s", self.duration)
    }
}

/// 服务响应数据项
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServiceResponseItem {
    #[serde(default)]
    pub service_name: String,
    pub delay: Box<[f64; 30]>,
    pub up: Box<[u64; 30]>,
    pub down: Box<[u64; 30]>,
    #[serde(default)]
    pub total_up: u64,
    #[serde(default)]
    pub total_down: u64,
    #[serde(default)]
    pub current_up: u64,
    #[serde(default)]
    pub current_down: u64,
}

/// 服务 API 请求
#[derive(Debug, Deserialize)]
pub struct ServiceForm {
    pub name: Option<String>,
    pub r#type: Option<i32>,
    pub target: Option<String>,
    pub duration: Option<i32>,
    pub notification_group_id: Option<u64>,
    pub cover: Option<i32>,
    pub notify: Option<bool>,
    pub skip_servers: Option<HashMap<u64, bool>>,
    pub fail_trigger_tasks: Option<Vec<u64>>,
    pub recover_trigger_tasks: Option<Vec<u64>>,
    pub min_latency: Option<f32>,
    pub max_latency: Option<f32>,
    pub latency_notify: Option<bool>,
    pub enable_trigger_task: Option<bool>,
    pub enable_show_in_service: Option<bool>,
    pub display_index: Option<i32>,
}
