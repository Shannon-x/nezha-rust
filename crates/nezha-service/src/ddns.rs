use std::sync::Arc;
use nezha_core::models::server::Server;
use nezha_core::models::ddns::DdnsProfile;
use nezha_core::models::host::GeoIP;
use reqwest::{Client, Method, header::{HeaderName, HeaderValue}};
use tracing::{info, warn, error};
use std::time::Duration;

/// DDNS 管理模块
pub struct DdnsManager;

impl DdnsManager {
    /// 触发特定服务器的 DDNS 更新
    pub async fn update(
        state: Arc<crate::state::AppState>,
        server: &Server,
        new_geoip: &GeoIP,
    ) {
        if !server.enable_ddns {
            return;
        }
        
        let ipv4 = new_geoip.ip.ipv4_addr.clone();
        let ipv6 = new_geoip.ip.ipv6_addr.clone();
        
        if ipv4.is_empty() && ipv6.is_empty() {
            return;
        }

        let profiles_ids = server.ddns_profiles.clone();
        if profiles_ids.is_empty() {
            return;
        }
        
        tokio::spawn(async move {
            let client = Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .unwrap_or_default();

            for profile_id in profiles_ids {
                // 读取完整配置
                let q = "SELECT id, name, provider, access_id, access_secret, webhook_url, webhook_method, webhook_request_type, webhook_request_body, webhook_headers, domains, max_retries, enable_ipv4, enable_ipv6 FROM ddns_profiles WHERE id = ?";
                let row: Result<(i64, String, String, String, String, String, String, i32, String, String, String, i32, bool, bool), _> = 
                    sqlx::query_as(q).bind(profile_id as i64).fetch_one(&state.db.pool).await;

                let (id, name, provider, access_id, access_secret, w_url, w_method, w_type, w_body, w_headers, doms_json, max_retries, e_v4, e_v6) = match row {
                    Ok(r) => r,
                    Err(e) => {
                        error!("DdnsManager: Failed to fetch profile {}: {}", profile_id, e);
                        continue;
                    }
                };

                let domains: Vec<String> = serde_json::from_str(&doms_json).unwrap_or_default();
                
                // 获取需要覆盖的域名配置
                let active_domains = if let Some(override_doms) = server.override_ddns_domains.get(&(id as u64)) {
                    if !override_doms.is_empty() {
                        override_doms.clone()
                    } else {
                        domains
                    }
                } else {
                    domains
                };

                if active_domains.is_empty() {
                    continue;
                }

                for domain in active_domains {
                    let mut success = false;
                    for attempt in 1..=(max_retries.max(1)) {
                        info!("NEZHA>> Updating DNS Record of domain {}: {}/{}", domain, attempt, max_retries);
                        
                        let res = match provider.as_str() {
                            "webhook" => Self::update_webhook(&client, &w_url, &w_method, w_type, &w_body, &w_headers, &domain, &ipv4, &ipv6).await,
                            "cloudflare" => Self::update_cloudflare(&client, &access_secret, &domain, &ipv4, &ipv6, e_v4, e_v6).await,
                            "he" => Self::update_he(&client, &domain, &access_secret, &ipv4, &ipv6, e_v4, e_v6).await,
                            "tencentcloud" => { warn!("TencentCloud DDNS not currently supported in Rust agent"); Err(anyhow::anyhow!("Unsupported")) },
                            _ => { warn!("DdnsManager: Unsupported provider {}", provider); Err(anyhow::anyhow!("Unsupported")) }
                        };

                        if let Err(e) = res {
                            warn!("NEZHA>> Failed to update DNS record of domain {}: {}", domain, e);
                        } else {
                            info!("NEZHA>> Update DNS record of domain {} succeeded", domain);
                            success = true;
                            break;
                        }
                    }
                    if !success {
                        error!("DdnsManager: Exhausted retries for {} on provider {}", domain, name);
                    }
                }
            }
        });
    }

    async fn update_webhook(client: &Client, url: &str, method: &str, req_type: i32, body: &str, headers: &str, domain: &str, ipv4: &str, ipv6: &str) -> anyhow::Result<()> {
        let replace_macros = |s: &str| -> String {
            s.replace("#IPv4#", ipv4)
             .replace("#IPv6#", ipv6)
             .replace("#DOMAIN#", domain)
        };

        let parsed_url = replace_macros(url);
        let parsed_body = replace_macros(body);

        let mut req = client.request(
            Method::from_bytes(method.to_uppercase().as_bytes()).unwrap_or(Method::GET),
            &parsed_url
        );

        if req_type == 1 {
            req = req.header("Content-Type", "application/json");
        } else if req_type == 2 {
            req = req.header("Content-Type", "application/x-www-form-urlencoded");
        }

        if !headers.is_empty() {
            let hm: std::collections::HashMap<String, String> = serde_json::from_str(headers).unwrap_or_default();
            for (k, v) in hm {
                let parsed_v = replace_macros(&v);
                if let (Ok(name), Ok(val)) = (HeaderName::from_bytes(k.as_bytes()), HeaderValue::from_str(&parsed_v)) {
                    req = req.header(name, val);
                }
            }
        }

        if !parsed_body.is_empty() {
            req = req.body(parsed_body);
        }

        let resp = req.send().await?;
        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Webhook HTTP {}", resp.status()));
        }
        Ok(())
    }

    async fn update_he(client: &Client, domain: &str, password: &str, ipv4: &str, ipv6: &str, e_v4: bool, e_v6: bool) -> anyhow::Result<()> {
        if e_v4 && !ipv4.is_empty() {
            let url = format!("https://dyn.dns.he.net/nic/update?hostname={}&password={}&myip={}", domain, password, ipv4);
            let resp = client.get(&url).send().await?;
            if !resp.status().is_success() { return Err(anyhow::anyhow!("HE v4 fail")); }
        }
        if e_v6 && !ipv6.is_empty() {
            let url = format!("https://dyn.dns.he.net/nic/update?hostname={}&password={}&myip={}", domain, password, ipv6);
            let resp = client.get(&url).send().await?;
            if !resp.status().is_success() { return Err(anyhow::anyhow!("HE v6 fail")); }
        }
        Ok(())
    }

    async fn update_cloudflare(client: &Client, token: &str, domain: &str, ipv4: &str, ipv6: &str, e_v4: bool, e_v6: bool) -> anyhow::Result<()> {
        let auth_header = format!("Bearer {}", token);
        
        let parts: Vec<&str> = domain.split('.').collect();
        let mut zone_id = String::new();
        
        for i in 0..(parts.len().saturating_sub(1)) {
            let test_zone = parts[i..].join(".");
            let url = format!("https://api.cloudflare.com/client/v4/zones?name={}", test_zone);
            let resp: serde_json::Value = client.get(&url).header("Authorization", &auth_header).send().await?.json().await?;
            
            if resp["success"].as_bool().unwrap_or(false) {
                if let Some(zones) = resp["result"].as_array() {
                    if !zones.is_empty() {
                        zone_id = zones[0]["id"].as_str().unwrap_or("").to_string();
                        break;
                    }
                }
            }
        }
        
        if zone_id.is_empty() {
            return Err(anyhow::anyhow!("Cloudflare: Zone not found for domain"));
        }

        let do_update = |ip: &str, record_type: &str| -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>> {
            let client = client.clone();
            let auth = auth_header.clone();
            let zone = zone_id.clone();
            let dom = domain.to_string();
            let ip = ip.to_string();
            let rt = record_type.to_string();
            Box::pin(async move {
                let url = format!("https://api.cloudflare.com/client/v4/zones/{}/dns_records?name={}&type={}", zone, dom, rt);
                let resp: serde_json::Value = client.get(&url).header("Authorization", &auth).send().await?.json().await?;
                
                let mut record_id = String::new();
                if let Some(recs) = resp["result"].as_array() {
                    if !recs.is_empty() {
                        record_id = recs[0]["id"].as_str().unwrap_or("").to_string();
                    }
                }
                
                let payload = serde_json::json!({
                    "type": rt,
                    "name": dom,
                    "content": ip,
                    "ttl": 1,
                    "proxied": false
                });

                if record_id.is_empty() {
                    let url = format!("https://api.cloudflare.com/client/v4/zones/{}/dns_records", zone);
                    client.post(&url).header("Authorization", &auth).json(&payload).send().await?;
                } else {
                    let url = format!("https://api.cloudflare.com/client/v4/zones/{}/dns_records/{}", zone, record_id);
                    client.put(&url).header("Authorization", &auth).json(&payload).send().await?;
                }
                Ok(())
            })
        };

        if e_v4 && !ipv4.is_empty() { do_update(ipv4, "A").await?; }
        if e_v6 && !ipv6.is_empty() { do_update(ipv6, "AAAA").await?; }

        Ok(())
    }
}
