use serde::{Deserialize, Serialize};
use std::fmt;

/// IP 地址对（IPv4 + IPv6）
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpAddr {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ipv4_addr: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ipv6_addr: String,
}

impl IpAddr {
    pub fn join(&self) -> String {
        if !self.ipv4_addr.is_empty() && !self.ipv6_addr.is_empty() {
            format!("{}/{}", self.ipv4_addr, self.ipv6_addr)
        } else if !self.ipv4_addr.is_empty() {
            self.ipv4_addr.clone()
        } else {
            self.ipv6_addr.clone()
        }
    }

    pub fn is_empty(&self) -> bool {
        self.ipv4_addr.is_empty() && self.ipv6_addr.is_empty()
    }
}

impl fmt::Display for IpAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.join())
    }
}

/// GeoIP 信息
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GeoIP {
    #[serde(default)]
    pub ip: IpAddr,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub country_code: String,
}
