use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use nezha_utils::ip::GeoIP;

use super::host::{Host, HostState};

/// 服务器模型（数据库存储）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Server {
    pub id: i64,
    pub created_at: Option<NaiveDateTime>,
    pub updated_at: Option<NaiveDateTime>,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub uuid: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub public_note: String,
    #[serde(default)]
    pub display_index: i32,
    #[serde(default)]
    pub hide_for_guest: bool,
    #[serde(default)]
    pub enable_ddns: bool,

    // 运行时字段（不入库）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<Host>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<HostState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geoip: Option<GeoIP>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_active: Option<NaiveDateTime>,

    // DDNS 配置（JSON 解析）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ddns_profiles: Vec<u64>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub override_ddns_domains: HashMap<u64, Vec<String>>,

    // 流量快照（运行时）
    #[serde(skip)]
    pub prev_transfer_in_snapshot: u64,
    #[serde(skip)]
    pub prev_transfer_out_snapshot: u64,
    #[serde(skip)]
    pub last_tsdb_write: Option<NaiveDateTime>,
}

impl Default for Server {
    fn default() -> Self {
        Self {
            id: 0,
            created_at: None,
            updated_at: None,
            name: String::new(),
            uuid: String::new(),
            note: String::new(),
            public_note: String::new(),
            display_index: 0,
            hide_for_guest: false,
            enable_ddns: false,
            host: Some(Host::default()),
            state: Some(HostState::default()),
            geoip: Some(GeoIP::default()),
            last_active: None,
            ddns_profiles: Vec::new(),
            override_ddns_domains: HashMap::new(),
            prev_transfer_in_snapshot: 0,
            prev_transfer_out_snapshot: 0,
            last_tsdb_write: None,
        }
    }
}

impl Server {
    /// 从运行中的服务器复制运行时数据
    pub fn copy_from_running(&mut self, old: &Server) {
        self.host = old.host.clone();
        self.state = old.state.clone();
        self.geoip = old.geoip.clone();
        self.last_active = old.last_active;
        self.prev_transfer_in_snapshot = old.prev_transfer_in_snapshot;
        self.prev_transfer_out_snapshot = old.prev_transfer_out_snapshot;
        self.last_tsdb_write = old.last_tsdb_write;
    }

    /// 对游客是否可见
    pub fn has_permission_for_guest(&self) -> bool {
        !self.hide_for_guest
    }
}

/// 服务器 API 请求
#[derive(Debug, Deserialize)]
pub struct ServerForm {
    pub name: Option<String>,
    pub note: Option<String>,
    pub public_note: Option<String>,
    pub display_index: Option<i32>,
    pub hide_for_guest: Option<bool>,
    pub enable_ddns: Option<bool>,
    pub ddns_profiles: Option<Vec<u64>>,
    pub override_ddns_domains: Option<HashMap<u64, Vec<String>>>,
}
