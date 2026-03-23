pub mod geoip;
pub mod ip;
pub mod i18n;

use rand::Rng;

/// 生成指定长度的随机字符串
pub fn generate_random_string(length: usize) -> String {
    let mut rng = rand::thread_rng();
    let chars: Vec<char> = (0..length)
        .map(|_| {
            let idx = rng.gen_range(0..62);
            match idx {
                0..=9 => (b'0' + idx) as char,
                10..=35 => (b'a' + idx - 10) as char,
                _ => (b'A' + idx - 36) as char,
            }
        })
        .collect();
    chars.into_iter().collect()
}

/// IP 脱敏处理
pub fn ip_desensitize(ip: &str) -> String {
    if let Some(pos) = ip.rfind('.') {
        // IPv4: hide last octet
        format!("{}.*", &ip[..pos])
    } else if let Some(pos) = ip.rfind(':') {
        // IPv6: hide last segment
        format!("{}:*", &ip[..pos])
    } else {
        ip.to_string()
    }
}

/// 计算两个 uint64 的差值（防下溢）
pub fn sub_uint_checked(a: u64, b: u64) -> u64 {
    a.saturating_sub(b)
}

/// 第一个错误返回
pub fn first_error<F: FnOnce() -> anyhow::Result<()>>(fns: Vec<F>) -> anyhow::Result<()> {
    for f in fns {
        f()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_random_string() {
        let s = generate_random_string(32);
        assert_eq!(s.len(), 32);
    }

    #[test]
    fn test_ip_desensitize() {
        assert_eq!(ip_desensitize("192.168.1.100"), "192.168.1.*");
        assert_eq!(ip_desensitize("2001:db8::1"), "2001:db8::*");
    }

    #[test]
    fn test_sub_uint_checked() {
        assert_eq!(sub_uint_checked(100, 50), 50);
        assert_eq!(sub_uint_checked(50, 100), 0);
    }
}
