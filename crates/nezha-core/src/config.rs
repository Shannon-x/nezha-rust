use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// 数据库类型常量
pub const DB_TYPE_SQLITE: &str = "sqlite";
pub const DB_TYPE_MYSQL: &str = "mysql";
pub const DB_TYPE_POSTGRES: &str = "postgres";
pub const DB_TYPE_SQLSERVER: &str = "sqlserver";

/// 配置覆盖范围
pub const CONFIG_COVER_ALL: u8 = 0;
pub const CONFIG_COVER_IGNORE_ALL: u8 = 1;

/// 数据库配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    #[serde(default = "default_db_type")]
    pub r#type: String,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub dbname: String,
    #[serde(default)]
    pub sslmode: String,
    #[serde(default = "default_db_path")]
    pub path: String,
}

fn default_db_type() -> String {
    DB_TYPE_SQLITE.to_string()
}

fn default_db_path() -> String {
    "data/sqlite.db".to_string()
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            r#type: DB_TYPE_SQLITE.to_string(),
            host: String::new(),
            port: 0,
            username: String::new(),
            password: String::new(),
            dbname: String::new(),
            sslmode: String::new(),
            path: "data/sqlite.db".to_string(),
        }
    }
}

impl DatabaseConfig {
    /// 生成数据源名称（DSN）
    pub fn dsn(&self) -> String {
        match self.r#type.as_str() {
            DB_TYPE_MYSQL => self.mysql_dsn(),
            DB_TYPE_POSTGRES => self.postgres_dsn(),
            DB_TYPE_SQLSERVER => self.sqlserver_dsn(),
            _ => format!("sqlite://{}", self.path),
        }
    }

    fn mysql_dsn(&self) -> String {
        let port = if self.port == 0 { 3306 } else { self.port };
        format!(
            "mysql://{}:{}@{}:{}/{}",
            self.username, self.password, self.host, port, self.dbname
        )
    }

    fn postgres_dsn(&self) -> String {
        let port = if self.port == 0 { 5432 } else { self.port };
        let sslmode = if self.sslmode.is_empty() {
            "disable"
        } else {
            &self.sslmode
        };
        format!(
            "postgres://{}:{}@{}:{}/{}?sslmode={}",
            self.username, self.password, self.host, port, self.dbname, sslmode
        )
    }

    fn sqlserver_dsn(&self) -> String {
        let port = if self.port == 0 { 1433 } else { self.port };
        format!(
            "sqlserver://{}:{}@{}:{}/{}",
            self.username, self.password, self.host, port, self.dbname
        )
    }
}

/// HTTPS 配置
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HttpsConfig {
    #[serde(default)]
    pub insecure_tls: bool,
    #[serde(default)]
    pub listen_port: u16,
    #[serde(default)]
    pub tls_cert_path: String,
    #[serde(default)]
    pub tls_key_path: String,
}

/// TSDB 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TsdbConfig {
    /// "sqlite" 或 "mysql"
    #[serde(default = "default_tsdb_type")]
    pub r#type: String,
    #[serde(default = "default_tsdb_data_path")]
    pub data_path: String,
    #[serde(default = "default_retention_days")]
    pub retention_days: u16,
    #[serde(default)]
    pub min_free_disk_space_gb: f64,
    #[serde(default = "default_max_memory_mb")]
    pub max_memory_mb: i64,
    #[serde(default = "default_write_buffer_size")]
    pub write_buffer_size: usize,
    #[serde(default = "default_write_buffer_flush_interval")]
    pub write_buffer_flush_interval: u64,
    /// MySQL TSDB 专用配置（可选，不设则复用主数据库配置）
    #[serde(default)]
    pub mysql: Option<DatabaseConfig>,
    /// PostgreSQL TSDB 专用配置（可选，不设则复用主数据库配置）
    #[serde(default)]
    pub postgres: Option<DatabaseConfig>,
}

fn default_tsdb_type() -> String {
    "sqlite".to_string()
}

fn default_tsdb_data_path() -> String {
    "data/tsdb".to_string()
}

fn default_retention_days() -> u16 {
    30
}

fn default_max_memory_mb() -> i64 {
    256
}

fn default_write_buffer_size() -> usize {
    1000
}

fn default_write_buffer_flush_interval() -> u64 {
    5
}

impl Default for TsdbConfig {
    fn default() -> Self {
        Self {
            r#type: default_tsdb_type(),
            data_path: default_tsdb_data_path(),
            retention_days: default_retention_days(),
            min_free_disk_space_gb: 1.0,
            max_memory_mb: default_max_memory_mb(),
            write_buffer_size: default_write_buffer_size(),
            write_buffer_flush_interval: default_write_buffer_flush_interval(),
            mysql: None,
            postgres: None,
        }
    }
}

/// 内存配置
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryConfig {
    #[serde(default)]
    pub go_mem_limit_mb: i64,
}

/// OAuth2 配置
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Oauth2Config {
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub client_secret: String,
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub redirect_url: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub user_info_url: String,
    #[serde(default)]
    pub user_id_path: String,
}

/// 前端模板
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontendTemplate {
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub is_admin: bool,
    #[serde(default)]
    pub repository: String,
}

/// 主配置结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    // 通用设置
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default)]
    pub site_name: String,
    #[serde(default)]
    pub custom_code: String,
    #[serde(default)]
    pub custom_code_dashboard: String,

    // 面板设置
    #[serde(default)]
    pub install_host: String,
    #[serde(default)]
    pub tls: bool,
    #[serde(default)]
    pub web_real_ip_header: String,
    #[serde(default)]
    pub agent_real_ip_header: String,
    #[serde(default = "default_user_template")]
    pub user_template: String,
    #[serde(default = "default_admin_template")]
    pub admin_template: String,

    #[serde(default)]
    pub enable_plain_ip_in_notification: bool,
    #[serde(default)]
    pub enable_ip_change_notification: bool,
    #[serde(default)]
    pub ip_change_notification_group_id: u64,
    #[serde(default = "default_cover")]
    pub cover: u8,
    #[serde(default)]
    pub ignored_ip_notification: String,
    #[serde(default)]
    pub dns_servers: String,

    // 核心设置
    #[serde(default = "default_avg_ping_count")]
    pub avg_ping_count: i32,
    #[serde(default)]
    pub debug: bool,
    #[serde(default = "default_location")]
    pub location: String,
    #[serde(default)]
    pub force_auth: bool,
    #[serde(default)]
    pub agent_secret_key: String,
    #[serde(default = "default_jwt_timeout")]
    pub jwt_timeout: i32,
    #[serde(default)]
    pub jwt_secret_key: String,
    #[serde(default = "default_listen_port")]
    pub listen_port: u16,
    #[serde(default)]
    pub listen_host: String,

    // OAuth2
    #[serde(default)]
    pub oauth2: HashMap<String, Oauth2Config>,

    // HTTPS
    #[serde(default)]
    pub https: HttpsConfig,

    // TSDB
    #[serde(default)]
    pub tsdb: TsdbConfig,

    // 内存
    #[serde(default)]
    pub memory: MemoryConfig,

    // 数据库
    #[serde(default)]
    pub database: DatabaseConfig,

    /// 配置文件路径（运行时字段，不序列化）
    #[serde(skip)]
    pub file_path: PathBuf,

    /// 解析后的 IP 通知忽略列表
    #[serde(skip)]
    pub ignored_ip_notification_server_ids: HashMap<u64, bool>,
}

fn default_language() -> String {
    "en_US".to_string()
}

fn default_user_template() -> String {
    "user-dist".to_string()
}

fn default_admin_template() -> String {
    "admin-dist".to_string()
}

fn default_cover() -> u8 {
    1
}

fn default_avg_ping_count() -> i32 {
    2
}

fn default_location() -> String {
    "Asia/Shanghai".to_string()
}

fn default_jwt_timeout() -> i32 {
    1
}

fn default_listen_port() -> u16 {
    8008
}

impl Default for Config {
    fn default() -> Self {
        Self {
            language: default_language(),
            site_name: String::new(),
            custom_code: String::new(),
            custom_code_dashboard: String::new(),
            install_host: String::new(),
            tls: false,
            web_real_ip_header: String::new(),
            agent_real_ip_header: String::new(),
            user_template: default_user_template(),
            admin_template: default_admin_template(),
            enable_plain_ip_in_notification: false,
            enable_ip_change_notification: false,
            ip_change_notification_group_id: 0,
            cover: default_cover(),
            ignored_ip_notification: String::new(),
            dns_servers: String::new(),
            avg_ping_count: default_avg_ping_count(),
            debug: false,
            location: default_location(),
            force_auth: false,
            agent_secret_key: String::new(),
            jwt_timeout: default_jwt_timeout(),
            jwt_secret_key: String::new(),
            listen_port: default_listen_port(),
            listen_host: String::new(),
            oauth2: HashMap::new(),
            https: HttpsConfig::default(),
            tsdb: TsdbConfig::default(),
            memory: MemoryConfig::default(),
            database: DatabaseConfig::default(),
            file_path: PathBuf::new(),
            ignored_ip_notification_server_ids: HashMap::new(),
        }
    }
}

impl Config {
    /// 从文件路径和环境变量加载配置
    /// 优先级：环境变量 > 配置文件 > 默认值
    pub fn load(path: &str) -> anyhow::Result<Self> {
        let mut config = if Path::new(path).exists() {
            let content = fs::read_to_string(path)?;
            serde_yaml::from_str::<Config>(&content)?
        } else {
            Config::default()
        };

        config.file_path = PathBuf::from(path);

        // 环境变量覆盖（NZ_ 前缀）
        config.apply_env_overrides();

        // 确保必要的密钥存在
        let mut need_save = false;
        if config.jwt_secret_key.is_empty() {
            config.jwt_secret_key = nezha_utils::generate_random_string(1024);
            need_save = true;
        }
        if config.agent_secret_key.is_empty() {
            config.agent_secret_key = nezha_utils::generate_random_string(32);
            need_save = true;
        }
        if need_save {
            if let Err(e) = config.save() {
                tracing::warn!(
                    "Failed to persist auto-generated keys to config file '{}': {}. \
                     The generated keys will be used in memory but will not survive a restart. \
                     Please ensure the config directory is writable.",
                    path, e
                );
            }
        }

        // 解析 ignored_ip_notification
        if !config.ignored_ip_notification.is_empty() {
            for id_str in config.ignored_ip_notification.split(',') {
                if let Ok(id) = id_str.trim().parse::<u64>() {
                    config.ignored_ip_notification_server_ids.insert(id, true);
                }
            }
        }

        Ok(config)
    }

    /// 应用环境变量覆盖
    fn apply_env_overrides(&mut self) {
        if let Ok(v) = std::env::var("NZ_DATABASE_TYPE") {
            self.database.r#type = v;
        }
        if let Ok(v) = std::env::var("NZ_DATABASE_HOST") {
            self.database.host = v;
        }
        if let Ok(v) = std::env::var("NZ_DATABASE_PORT") {
            if let Ok(port) = v.parse() {
                self.database.port = port;
            }
        }
        if let Ok(v) = std::env::var("NZ_DATABASE_USERNAME") {
            self.database.username = v;
        }
        if let Ok(v) = std::env::var("NZ_DATABASE_PASSWORD") {
            self.database.password = v;
        }
        if let Ok(v) = std::env::var("NZ_DATABASE_DBNAME") {
            self.database.dbname = v;
        }
        if let Ok(v) = std::env::var("NZ_DATABASE_SSLMODE") {
            self.database.sslmode = v;
        }
        if let Ok(v) = std::env::var("NZ_DATABASE_PATH") {
            self.database.path = v;
        }
        if let Ok(v) = std::env::var("NZ_LISTEN_PORT") {
            if let Ok(port) = v.parse() {
                self.listen_port = port;
            }
        }
        if let Ok(v) = std::env::var("NZ_LANGUAGE") {
            self.language = v;
        }
        if let Ok(v) = std::env::var("NZ_AGENT_SECRET_KEY") {
            self.agent_secret_key = v;
        }
        if let Ok(v) = std::env::var("NZ_DEBUG") {
            self.debug = v == "true" || v == "1";
        }
    }

    /// 保存配置到文件
    pub fn save(&self) -> anyhow::Result<()> {
        if let Some(dir) = self.file_path.parent() {
            fs::create_dir_all(dir)?;
        }
        let data = serde_yaml::to_string(self)?;
        fs::write(&self.file_path, data)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.listen_port, 8008);
        assert_eq!(config.language, "en_US");
        assert_eq!(config.location, "Asia/Shanghai");
        assert_eq!(config.database.r#type, "sqlite");
    }

    #[test]
    fn test_database_dsn() {
        let mut db = DatabaseConfig::default();
        db.r#type = "mysql".to_string();
        db.host = "localhost".to_string();
        db.username = "root".to_string();
        db.password = "pass".to_string();
        db.dbname = "nezha".to_string();
        assert_eq!(db.dsn(), "mysql://root:pass@localhost:3306/nezha");
    }
}
