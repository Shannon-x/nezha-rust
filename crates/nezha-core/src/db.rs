use crate::config::DatabaseConfig;
use sqlx::{AnyPool, any::AnyPoolOptions};
use tracing::info;

/// 数据库连接封装
#[derive(Clone)]
pub struct Database {
    pub pool: AnyPool,
    pub db_type: String,
}

impl Database {
    /// 根据配置初始化数据库连接池
    pub async fn connect(config: &DatabaseConfig) -> anyhow::Result<Self> {
        let dsn = config.dsn();
        let db_type = config.r#type.clone();

        info!("Connecting to {} database...", db_type);

        // 对于 SQLite, 确保父目录存在
        if db_type == "sqlite" || db_type.is_empty() {
            if let Some(parent) = std::path::Path::new(&config.path).parent() {
                std::fs::create_dir_all(parent)?;
            }
        }

        // 安装所有数据库驱动
        sqlx::any::install_default_drivers();

        // 对于 SQLite, 添加 create_if_missing 参数
        let connect_dsn = if (db_type == "sqlite" || db_type.is_empty()) && !dsn.contains('?') {
            format!("{}?mode=rwc", dsn)
        } else {
            dsn
        };

        let pool = AnyPoolOptions::new()
            .max_connections(50)
            .min_connections(5)
            .acquire_timeout(std::time::Duration::from_secs(30))
            .idle_timeout(std::time::Duration::from_secs(600))
            .connect(&connect_dsn)
            .await?;

        info!("Database connected: {}", db_type);

        let db = Self { pool, db_type };
        db.run_migrations().await?;

        Ok(db)
    }

    /// 执行数据库迁移
    async fn run_migrations(&self) -> anyhow::Result<()> {
        info!("Running database migrations...");

        // 创建所有表
        let create_tables = vec![
            // users 表
            r#"CREATE TABLE IF NOT EXISTS users (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at DATETIME,
                updated_at DATETIME,
                username VARCHAR(255) NOT NULL UNIQUE,
                password VARCHAR(255) NOT NULL,
                role INTEGER DEFAULT 0,
                agent_secret VARCHAR(255) DEFAULT ''
            )"#,
            // servers 表
            r#"CREATE TABLE IF NOT EXISTS servers (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at DATETIME,
                updated_at DATETIME,
                name VARCHAR(255) NOT NULL,
                uuid VARCHAR(36) UNIQUE,
                note TEXT DEFAULT '',
                public_note TEXT DEFAULT '',
                display_index INTEGER DEFAULT 0,
                hide_for_guest INTEGER DEFAULT 0,
                enable_ddns INTEGER DEFAULT 0,
                ddns_profiles_raw TEXT DEFAULT '[]',
                override_ddns_domains_raw TEXT DEFAULT '{}'
            )"#,
            // server_groups 表
            r#"CREATE TABLE IF NOT EXISTS server_groups (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at DATETIME,
                updated_at DATETIME,
                name VARCHAR(255) NOT NULL
            )"#,
            // server_group_servers 表
            r#"CREATE TABLE IF NOT EXISTS server_group_servers (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                server_group_id INTEGER,
                server_id INTEGER
            )"#,
            // notifications 表
            r#"CREATE TABLE IF NOT EXISTS notifications (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at DATETIME,
                updated_at DATETIME,
                name VARCHAR(255) NOT NULL,
                tag VARCHAR(255) DEFAULT '',
                url TEXT NOT NULL,
                request_method INTEGER DEFAULT 1,
                request_type INTEGER DEFAULT 1,
                request_header TEXT DEFAULT '',
                request_body TEXT DEFAULT '',
                verify_tls INTEGER DEFAULT 1,
                skip_check INTEGER DEFAULT 0
            )"#,
            // notification_groups 表
            r#"CREATE TABLE IF NOT EXISTS notification_groups (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at DATETIME,
                updated_at DATETIME,
                name VARCHAR(255) NOT NULL
            )"#,
            // notification_group_notifications 表
            r#"CREATE TABLE IF NOT EXISTS notification_group_notifications (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                notification_group_id INTEGER,
                notification_id INTEGER
            )"#,
            // alert_rules 表
            r#"CREATE TABLE IF NOT EXISTS alert_rules (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at DATETIME,
                updated_at DATETIME,
                name VARCHAR(255) NOT NULL,
                rules_raw TEXT DEFAULT '[]',
                fail_trigger_tasks_raw TEXT DEFAULT '[]',
                recover_trigger_tasks_raw TEXT DEFAULT '[]',
                notification_group_id INTEGER DEFAULT 0,
                trigger_mode INTEGER DEFAULT 0,
                enable INTEGER DEFAULT 1
            )"#,
            // services 表
            r#"CREATE TABLE IF NOT EXISTS services (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at DATETIME,
                updated_at DATETIME,
                name VARCHAR(255) NOT NULL,
                type INTEGER DEFAULT 0,
                target TEXT DEFAULT '',
                duration INTEGER DEFAULT 30,
                notification_group_id INTEGER DEFAULT 0,
                cover INTEGER DEFAULT 0,
                notify INTEGER DEFAULT 0,
                skip_servers_raw TEXT DEFAULT '{}',
                fail_trigger_tasks_raw TEXT DEFAULT '[]',
                recover_trigger_tasks_raw TEXT DEFAULT '[]',
                min_latency REAL DEFAULT 0,
                max_latency REAL DEFAULT 0,
                latency_notify INTEGER DEFAULT 0,
                enable_trigger_task INTEGER DEFAULT 0,
                enable_show_in_service INTEGER DEFAULT 0,
                display_index INTEGER DEFAULT 0
            )"#,
            // service_histories 表
            r#"CREATE TABLE IF NOT EXISTS service_histories (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at DATETIME,
                service_id INTEGER,
                server_id INTEGER DEFAULT 0,
                avg_delay REAL DEFAULT 0,
                up INTEGER DEFAULT 0,
                down INTEGER DEFAULT 0,
                data TEXT DEFAULT ''
            )"#,
            // crons 表
            r#"CREATE TABLE IF NOT EXISTS crons (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at DATETIME,
                updated_at DATETIME,
                name VARCHAR(255) NOT NULL,
                task_type INTEGER DEFAULT 0,
                scheduler TEXT DEFAULT '',
                command TEXT DEFAULT '',
                servers_raw TEXT DEFAULT '[]',
                cover INTEGER DEFAULT 0,
                push_successful INTEGER DEFAULT 0,
                notification_group_id INTEGER DEFAULT 0,
                last_executed_at DATETIME,
                last_result INTEGER DEFAULT 0
            )"#,
            // transfers 表
            r#"CREATE TABLE IF NOT EXISTS transfers (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at DATETIME,
                server_id INTEGER,
                "in" BIGINT DEFAULT 0,
                "out" BIGINT DEFAULT 0
            )"#,
            // nats 表
            r#"CREATE TABLE IF NOT EXISTS nats (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at DATETIME,
                updated_at DATETIME,
                name VARCHAR(255) DEFAULT '',
                server_id INTEGER DEFAULT 0,
                host TEXT DEFAULT '',
                domain TEXT DEFAULT '',
                enabled INTEGER DEFAULT 1
            )"#,
            // ddns_profiles 表
            r#"CREATE TABLE IF NOT EXISTS ddns_profiles (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at DATETIME,
                updated_at DATETIME,
                name VARCHAR(255) NOT NULL,
                provider VARCHAR(255) DEFAULT '',
                access_id VARCHAR(255) DEFAULT '',
                access_secret VARCHAR(255) DEFAULT '',
                webhook_url TEXT DEFAULT '',
                webhook_method VARCHAR(16) DEFAULT '',
                webhook_request_type INTEGER DEFAULT 0,
                webhook_request_body TEXT DEFAULT '',
                webhook_headers TEXT DEFAULT '',
                domains_raw TEXT DEFAULT '[]',
                max_retries INTEGER DEFAULT 3,
                enable_ipv4 INTEGER DEFAULT 1,
                enable_ipv6 INTEGER DEFAULT 0
            )"#,
            // wafs 表
            r#"CREATE TABLE IF NOT EXISTS wafs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at DATETIME,
                updated_at DATETIME,
                ip VARCHAR(255) NOT NULL,
                blocked_at DATETIME,
                blocked_reason TEXT DEFAULT '',
                count INTEGER DEFAULT 0
            )"#,
            // oauth2_binds 表
            r#"CREATE TABLE IF NOT EXISTS oauth2_binds (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at DATETIME,
                updated_at DATETIME,
                user_id INTEGER,
                provider VARCHAR(255),
                open_id VARCHAR(255)
            )"#,
        ];

        for sql in create_tables {
            sqlx::query(sql).execute(&self.pool).await?;
        }

        info!("Database migrations completed");
        Ok(())
    }
}
