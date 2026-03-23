use maxminddb::{geoip2, Reader};
use std::net::IpAddr;
use std::path::Path;
use std::sync::OnceLock;
use tracing::warn;

static GEOIP_DB: OnceLock<Option<Reader<Vec<u8>>>> = OnceLock::new();

/// 初始化 GeoIP 数据库
pub fn init_geoip(path: &str) {
    GEOIP_DB.get_or_init(|| {
        if !Path::new(path).exists() {
            warn!("GeoIP database not found at {}", path);
            return None;
        }
        match Reader::open_readfile(path) {
            Ok(reader) => {
                tracing::info!("GeoIP database loaded from {}", path);
                Some(reader)
            }
            Err(e) => {
                warn!("Failed to open GeoIP database: {}", e);
                None
            }
        }
    });
}

/// 查询 IP 地理位置，返回国家代码
pub fn lookup(ip: IpAddr) -> Option<String> {
    let db = GEOIP_DB.get()?.as_ref()?;
    match db.lookup::<geoip2::Country>(ip) {
        Ok(result) => result
            .country
            .and_then(|c| c.iso_code)
            .map(|s| s.to_string()),
        Err(e) => {
            warn!("GeoIP lookup failed for {}: {}", ip, e);
            None
        }
    }
}
