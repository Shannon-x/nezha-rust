//! 核心模块单元测试

#[cfg(test)]
mod config_tests {
    use nezha_core::config::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.listen_port, 8008);
        assert_eq!(config.language, "en_US");
        assert_eq!(config.database.r#type, "sqlite");
        assert_eq!(config.tsdb.retention_days, 30);
    }

    #[test]
    fn test_dsn_sqlite() {
        let db = DatabaseConfig::default();
        assert!(db.dsn().starts_with("sqlite://"));
    }

    #[test]
    fn test_dsn_mysql() {
        let mut db = DatabaseConfig::default();
        db.r#type = "mysql".to_string();
        db.host = "127.0.0.1".to_string();
        db.username = "root".to_string();
        db.password = "pass".to_string();
        db.dbname = "nz".to_string();
        assert_eq!(db.dsn(), "mysql://root:pass@127.0.0.1:3306/nz");
    }

    #[test]
    fn test_dsn_postgres_with_ssl() {
        let mut db = DatabaseConfig::default();
        db.r#type = "postgres".to_string();
        db.host = "pg.local".to_string();
        db.username = "u".to_string();
        db.password = "p".to_string();
        db.dbname = "db".to_string();
        db.sslmode = "require".to_string();
        assert!(db.dsn().contains("sslmode=require"));
    }

    #[test]
    fn test_yaml_roundtrip() {
        let yaml = r#"
language: "zh_CN"
listen_port: 9090
database:
  type: "mysql"
  host: "localhost"
  username: "root"
  password: "test"
  dbname: "nezha"
tsdb:
  type: "mysql"
  retention_days: 60
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.language, "zh_CN");
        assert_eq!(config.listen_port, 9090);
        assert_eq!(config.tsdb.retention_days, 60);
    }

    #[test]
    fn test_oauth2_config() {
        let yaml = r#"
oauth2:
  github:
    client_id: "abc"
    client_secret: "def"
    endpoint: "https://github.com"
    redirect_url: "http://localhost/callback"
    scopes: ["user:email"]
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert!(config.oauth2.contains_key("github"));
        assert_eq!(config.oauth2["github"].scopes.len(), 1);
    }
}

#[cfg(test)]
mod model_tests {
    use nezha_core::models::server::Server;
    use nezha_core::models::service::Service;
    use nezha_core::models::host::{Host, HostState};

    #[test]
    fn test_server_default() {
        let s = Server::default();
        assert_eq!(s.id, 0);
        assert!(s.name.is_empty());
        assert!(!s.hide_for_guest);
    }

    #[test]
    fn test_service_default_duration() {
        let s = Service::default();
        assert_eq!(s.duration, 30);
        assert!(!s.notify);
    }

    #[test]
    fn test_host_state_default_zeros() {
        let s = HostState::default();
        assert_eq!(s.cpu, 0.0);
        assert_eq!(s.mem_used, 0);
        assert_eq!(s.load_1, 0.0);
    }

    #[test]
    fn test_host_default() {
        let h = Host::default();
        assert!(h.platform.is_empty());
        assert_eq!(h.mem_total, 0);
    }
}

#[cfg(test)]
mod common_tests {
    use nezha_core::models::common::CommonResponse;

    #[test]
    fn test_common_response_success() {
        let resp = CommonResponse::success(42);
        assert!(resp.success);
        assert_eq!(resp.data, Some(42));
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_common_response_error() {
        let resp = CommonResponse::<()>::error("bad");
        assert!(!resp.success);
        assert!(resp.data.is_none());
        assert_eq!(resp.error, Some("bad".to_string()));
    }
}

#[cfg(test)]
mod jwt_tests {
    use nezha_api::handlers::auth::Claims;

    #[test]
    fn test_jwt_encode_decode() {
        use jsonwebtoken::{encode, decode, EncodingKey, DecodingKey, Header, Validation};
        let secret = "test_secret_key_for_unit_testing";
        let claims = Claims {
            sub: 1, role: 1, username: "admin".to_string(),
            exp: (chrono::Utc::now().timestamp() + 3600) as usize,
            iat: chrono::Utc::now().timestamp() as usize,
        };

        let token = encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes())).unwrap();
        assert!(!token.is_empty());

        let decoded = decode::<Claims>(&token, &DecodingKey::from_secret(secret.as_bytes()), &Validation::default()).unwrap();
        assert_eq!(decoded.claims.sub, 1);
        assert_eq!(decoded.claims.username, "admin");
    }

    #[test]
    fn test_jwt_expired_fails() {
        use jsonwebtoken::{encode, decode, EncodingKey, DecodingKey, Header, Validation};
        let secret = "test_secret";
        let claims = Claims {
            sub: 1, role: 0, username: "u".to_string(),
            exp: 1000, iat: 999,
        };
        let token = encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes())).unwrap();
        assert!(decode::<Claims>(&token, &DecodingKey::from_secret(secret.as_bytes()), &Validation::default()).is_err());
    }

    #[test]
    fn test_rate_limiter() {
        use nezha_api::middleware::RateLimiter;
        let limiter = RateLimiter::new(3, 60);
        assert!(limiter.check("1.2.3.4"));
        assert!(limiter.check("1.2.3.4"));
        assert!(limiter.check("1.2.3.4"));
        assert!(!limiter.check("1.2.3.4")); // blocked
        assert!(limiter.check("5.6.7.8")); // different IP ok
    }
}

#[cfg(test)]
mod tsdb_tests {
    use nezha_tsdb::*;

    #[test]
    fn test_server_metrics_struct() {
        let m = ServerMetrics {
            server_id: 1,
            timestamp: chrono::Utc::now().naive_utc(),
            cpu: 45.5, mem_used: 1024, swap_used: 0, disk_used: 5000,
            net_in_speed: 100, net_out_speed: 200,
            net_in_transfer: 1_000_000, net_out_transfer: 2_000_000,
            load1: 0.5, load5: 0.3, load15: 0.1,
            tcp_conn_count: 50, udp_conn_count: 10, process_count: 100,
            temperature: 55.0, uptime: 86400, gpu: 30.0,
        };
        assert_eq!(m.server_id, 1);
        assert_eq!(m.cpu, 45.5);
    }

    #[test]
    fn test_daily_stats_default() {
        let s = DailyServiceStats::default();
        assert_eq!(s.up, 0);
        assert_eq!(s.delay, 0.0);
    }

    #[tokio::test]
    async fn test_sqlite_store_write_query() {
        let tmp = tempfile::tempdir().unwrap();
        let store = nezha_tsdb::sqlite::SqliteStore::new(tmp.path().to_str().unwrap(), 7).await.unwrap();
        assert!(!store.is_closed());

        // Write + Query server metrics
        let m = ServerMetrics {
            server_id: 1, timestamp: chrono::Utc::now().naive_utc(),
            cpu: 50.0, mem_used: 2048, swap_used: 0, disk_used: 10000,
            net_in_speed: 1000, net_out_speed: 2000,
            net_in_transfer: 1_000_000, net_out_transfer: 2_000_000,
            load1: 1.0, load5: 0.5, load15: 0.3,
            tcp_conn_count: 100, udp_conn_count: 20, process_count: 200,
            temperature: 60.0, uptime: 3600, gpu: 0.0,
        };
        store.write_server_metrics(&m).await.unwrap();

        let points = store.query_server_metrics(1, MetricType::CPU, QueryPeriod::Hour1).await.unwrap();
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].value, 50.0);

        // Write + Query service metrics
        let sm = ServiceMetrics {
            service_id: 1, server_id: 1,
            timestamp: chrono::Utc::now().naive_utc(),
            delay: 15.5, successful: true,
        };
        store.write_service_metrics(&sm).await.unwrap();

        let h = store.query_service_history(1, QueryPeriod::Hour1).await.unwrap();
        assert_eq!(h.servers.len(), 1);
        assert_eq!(h.servers[0].stats.total_up, 1);

        store.maintenance().await;
        store.close().await.unwrap();
        assert!(store.is_closed());
    }
}

#[cfg(test)]
mod utils_tests {
    #[test]
    fn test_generate_random_string() {
        let s1 = nezha_utils::generate_random_string(32);
        let s2 = nezha_utils::generate_random_string(32);
        assert_eq!(s1.len(), 32);
        assert_ne!(s1, s2);
    }

    #[test]
    fn test_ip_desensitize() {
        let masked = nezha_utils::ip_desensitize("192.168.1.100");
        assert!(!masked.contains("100"));
    }
}
