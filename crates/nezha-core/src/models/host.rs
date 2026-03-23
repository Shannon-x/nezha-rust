use serde::{Deserialize, Serialize};
use nezha_utils::ip::{GeoIP, IpAddr};

/// 传感器温度
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SensorTemperature {
    pub name: String,
    pub temperature: f64,
}

/// 主机状态（运行时数据，不入库）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HostState {
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub cpu: f64,
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub mem_used: u64,
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub swap_used: u64,
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub disk_used: u64,
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub net_in_transfer: u64,
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub net_out_transfer: u64,
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub net_in_speed: u64,
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub net_out_speed: u64,
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub uptime: u64,
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub load_1: f64,
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub load_5: f64,
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub load_15: f64,
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub tcp_conn_count: u64,
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub udp_conn_count: u64,
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub process_count: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub temperatures: Vec<SensorTemperature>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gpu: Vec<f64>,
}

fn is_zero_f64(v: &f64) -> bool { *v == 0.0 }
fn is_zero_u64(v: &u64) -> bool { *v == 0 }

/// 主机信息（运行时数据，不入库）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Host {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub platform: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub platform_version: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cpu: Vec<String>,
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub mem_total: u64,
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub disk_total: u64,
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub swap_total: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub arch: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub virtualization: String,
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub boot_time: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gpu: Vec<String>,
}

impl Host {
    /// 返回过滤后的 Host（去除敏感字段）
    pub fn filtered(&self) -> Host {
        Host {
            platform: self.platform.clone(),
            platform_version: String::new(),
            cpu: self.cpu.clone(),
            mem_total: self.mem_total,
            disk_total: self.disk_total,
            swap_total: self.swap_total,
            arch: self.arch.clone(),
            virtualization: self.virtualization.clone(),
            boot_time: self.boot_time,
            version: String::new(),
            gpu: self.gpu.clone(),
        }
    }
}

/// 从 protobuf 转换
impl HostState {
    pub fn from_pb(state: &nezha_proto::State) -> Self {
        Self {
            cpu: state.cpu,
            mem_used: state.mem_used,
            swap_used: state.swap_used,
            disk_used: state.disk_used,
            net_in_transfer: state.net_in_transfer,
            net_out_transfer: state.net_out_transfer,
            net_in_speed: state.net_in_speed,
            net_out_speed: state.net_out_speed,
            uptime: state.uptime,
            load_1: state.load1,
            load_5: state.load5,
            load_15: state.load15,
            tcp_conn_count: state.tcp_conn_count,
            udp_conn_count: state.udp_conn_count,
            process_count: state.process_count,
            temperatures: state
                .temperatures
                .iter()
                .map(|t| SensorTemperature {
                    name: t.name.clone(),
                    temperature: t.temperature,
                })
                .collect(),
            gpu: state.gpu.clone(),
        }
    }
}

impl Host {
    pub fn from_pb(h: &nezha_proto::Host) -> Self {
        Self {
            platform: h.platform.clone(),
            platform_version: h.platform_version.clone(),
            cpu: h.cpu.clone(),
            mem_total: h.mem_total,
            disk_total: h.disk_total,
            swap_total: h.swap_total,
            arch: h.arch.clone(),
            virtualization: h.virtualization.clone(),
            boot_time: h.boot_time,
            version: h.version.clone(),
            gpu: h.gpu.clone(),
        }
    }
}

/// 从 protobuf 转换 GeoIP
pub fn geoip_from_pb(g: &nezha_proto::GeoIp) -> GeoIP {
    let ip = g.ip.as_ref().map(|ip| IpAddr {
        ipv4_addr: ip.ipv4.clone(),
        ipv6_addr: ip.ipv6.clone(),
    }).unwrap_or_default();
    GeoIP {
        ip,
        country_code: g.country_code.clone(),
    }
}
