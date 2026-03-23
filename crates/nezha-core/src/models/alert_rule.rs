use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 告警规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    pub id: i64,
    pub created_at: Option<NaiveDateTime>,
    pub updated_at: Option<NaiveDateTime>,
    pub name: String,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub fail_trigger_tasks: Vec<u64>,
    #[serde(default)]
    pub recover_trigger_tasks: Vec<u64>,
    #[serde(default)]
    pub notification_group_id: u64,
    #[serde(default)]
    pub trigger_mode: i32,
    #[serde(default = "default_true")]
    pub enable: bool,
}

fn default_true() -> bool { true }

impl Default for AlertRule {
    fn default() -> Self {
        Self {
            id: 0, created_at: None, updated_at: None,
            name: String::new(), rules: Vec::new(),
            fail_trigger_tasks: Vec::new(), recover_trigger_tasks: Vec::new(),
            notification_group_id: 0, trigger_mode: 0, enable: true,
        }
    }
}

/// 告警规则项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    #[serde(default)]
    pub r#type: String,
    #[serde(default)]
    pub min: f64,
    #[serde(default)]
    pub max: f64,
    #[serde(default)]
    pub cycle_start: Option<NaiveDateTime>,
    #[serde(default)]
    pub cycle_interval: u64,
    #[serde(default)]
    pub cycle_unit: String,
    #[serde(default)]
    pub duration: u64,
    #[serde(default)]
    pub cover: u64,
    #[serde(default)]
    pub ignore: HashMap<u64, bool>,
    #[serde(default)]
    pub next_transfer_at: HashMap<u64, NaiveDateTime>,
    #[serde(default)]
    pub last_cycle_status: HashMap<u64, bool>,
}

impl Rule {
    pub fn is_transfer_duration_rule(&self) -> bool {
        self.r#type == "transfer_in_cycle" || self.r#type == "transfer_out_cycle" || self.r#type == "transfer_all_cycle"
    }
}

#[derive(Debug, Deserialize)]
pub struct AlertRuleForm {
    pub name: Option<String>,
    pub rules: Option<Vec<Rule>>,
    pub fail_trigger_tasks: Option<Vec<u64>>,
    pub recover_trigger_tasks: Option<Vec<u64>>,
    pub notification_group_id: Option<u64>,
    pub trigger_mode: Option<i32>,
    pub enable: Option<bool>,
}
