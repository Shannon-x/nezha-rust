#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    // ──────── Config 测试 ────────
    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.listen_port, 8008);
        assert_eq!(config.language, "en_US");
        assert_eq!(config.location, "Asia/Shanghai");
        assert_eq!(config.database.r#type, "sqlite");
        assert_eq!(config.jwt_timeout, 1);
        assert_eq!(config.tsdb.retention_days, 30);
    }

    #[test]
    fn test_sqlite_dsn() {
        let db = DatabaseConfig::default();
        assert_eq!(db.dsn(), "sqlite://data/sqlite.db");
    }

    #[test]
    fn test_mysql_dsn() {
        let mut db = DatabaseConfig::default();
        db.r#type = "mysql".to_string();
        db.host = "127.0.0.1".to_string();
        db.username = "root".to_string();
        db.password = "secret".to_string();
        db.dbname = "nezha".to_string();
        assert_eq!(db.dsn(), "mysql://root:secret@127.0.0.1:3306/nezha");
    }

    #[test]
    fn test_postgres_dsn() {
        let mut db = DatabaseConfig::default();
        db.r#type = "postgres".to_string();
        db.host = "db.host".to_string();
        db.username = "admin".to_string();
        db.password = "pass".to_string();
        db.dbname = "nz".to_string();
        db.sslmode = "require".to_string();
        assert_eq!(db.dsn(), "postgres://admin:pass@db.host:5432/nz?sslmode=require");
    }

    #[test]
    fn test_tsdb_config_defaults() {
        let tsdb = TsdbConfig::default();
        assert_eq!(tsdb.r#type, "sqlite");
        assert_eq!(tsdb.retention_days, 30);
        assert_eq!(tsdb.write_buffer_size, 1000);
        assert_eq!(tsdb.write_buffer_flush_interval, 5);
        assert!(tsdb.mysql.is_none());
    }

    #[test]
    fn test_config_yaml_parse() {
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
        let config: Config = serde_yaml::from_str(yaml).expect("parse yaml");
        assert_eq!(config.language, "zh_CN");
        assert_eq!(config.listen_port, 9090);
        assert_eq!(config.database.r#type, "mysql");
        assert_eq!(config.tsdb.r#type, "mysql");
        assert_eq!(config.tsdb.retention_days, 60);
    }

    #[test]
    fn test_oauth2_config() {
        let yaml = r#"
oauth2:
  github:
    client_id: "abc"
    client_secret: "def"
    endpoint: "https://github.com/login/oauth/authorize"
    redirect_url: "http://localhost/callback"
    scopes: ["user:email"]
"#;
        let config: Config = serde_yaml::from_str(yaml).expect("parse oauth2");
        assert!(config.oauth2.contains_key("github"));
        let gh = &config.oauth2["github"];
        assert_eq!(gh.client_id, "abc");
        assert_eq!(gh.scopes.len(), 1);
    }
}
