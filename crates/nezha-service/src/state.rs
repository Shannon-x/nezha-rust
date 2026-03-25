use dashmap::DashMap;
use moka::future::Cache;
use nezha_core::config::Config;
use nezha_core::db::Database;
use nezha_core::models::server::Server;
use nezha_core::models::service::Service;
use nezha_proto::Task;
use nezha_tsdb::Store;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tonic::Status;
use chrono::Utc;

/// 全局应用状态（替代 Go 的全局 singleton 变量）
pub struct AppState {
    /// 配置（RwLock 支持运行时修改 + 持久化）
    pub config: RwLock<Config>,
    pub db: Database,

    /// 服务器列表（并发安全 map）
    pub servers: DashMap<u64, Server>,
    /// 服务监控列表
    pub services: DashMap<u64, Service>,

    /// 通用缓存（5 分钟 TTL）
    pub cache: Cache<String, String>,

    /// TSDB 存储后端
    pub tsdb: Option<Arc<dyn Store>>,

    /// Dashboard 启动时间
    pub boot_time: u64,

    /// 已连接 Agent 的任务通道 (server_id → Task sender)
    pub task_senders: DashMap<u64, mpsc::Sender<Result<Task, Status>>>,

    /// 服务监控任务通道
    pub service_dispatch_tx: mpsc::Sender<Service>,
}

impl AppState {
    pub async fn new(config: Config) -> anyhow::Result<Arc<Self>> {
        // 初始化数据库
        let db = Database::connect(&config.database).await?;

        // 初始化 TSDB
        let tsdb: Option<Arc<dyn Store>> = match config.tsdb.r#type.as_str() {
            "mysql" => {
                let dsn = if let Some(ref mysql_conf) = config.tsdb.mysql {
                    mysql_conf.dsn()
                } else {
                    config.database.dsn()
                };
                let store = nezha_tsdb::mysql::MysqlStore::new(&dsn, config.tsdb.retention_days).await?;
                Some(Arc::new(store))
            }
            "postgres" | "postgresql" => {
                let dsn = if let Some(ref pg_conf) = config.tsdb.postgres {
                    pg_conf.dsn()
                } else {
                    config.database.dsn()
                };
                let store = nezha_tsdb::postgres::PostgresStore::new(&dsn, config.tsdb.retention_days).await?;
                Some(Arc::new(store))
            }
            "sqlite" | "" => {
                let store = nezha_tsdb::sqlite::SqliteStore::new(
                    &config.tsdb.data_path,
                    config.tsdb.retention_days,
                ).await?;
                Some(Arc::new(store))
            }
            other => {
                tracing::warn!("Unknown TSDB type: {}, TSDB disabled", other);
                None
            }
        };

        let cache = Cache::builder()
            .time_to_live(Duration::from_secs(300))
            .time_to_idle(Duration::from_secs(600))
            .max_capacity(10_000)
            .build();

        let (tx, _rx) = mpsc::channel::<Service>(200);

        let state = Arc::new(Self {
            config: RwLock::new(config),
            db,
            servers: DashMap::new(),
            services: DashMap::new(),
            cache,
            tsdb,
            boot_time: Utc::now().timestamp() as u64,
            task_senders: DashMap::new(),
            service_dispatch_tx: tx,
        });

        // 从数据库加载服务器列表
        state.load_servers().await?;

        Ok(state)
    }

    /// 从数据库加载所有服务器
    async fn load_servers(&self) -> anyhow::Result<()> {
        let rows: Vec<(i64, String, String, String, String, i32, bool, bool)> = sqlx::query_as(
            "SELECT id, name, COALESCE(uuid,''), COALESCE(note,''), COALESCE(public_note,''), display_index, hide_for_guest, enable_ddns FROM servers"
        )
        .fetch_all(&self.db.pool).await?;

        for (id, name, uuid, note, public_note, display_index, hide_for_guest, enable_ddns) in rows {
            let mut server = Server::default();
            server.id = id;
            server.name = name;
            server.uuid = uuid;
            server.note = note;
            server.public_note = public_note;
            server.display_index = display_index;
            server.hide_for_guest = hide_for_guest;
            server.enable_ddns = enable_ddns;
            self.servers.insert(id as u64, server);
        }

        tracing::info!("Loaded {} servers from database", self.servers.len());
        Ok(())
    }

    /// TSDB 是否已启用
    pub fn tsdb_enabled(&self) -> bool {
        self.tsdb.is_some()
    }

    /// IP 脱敏
    pub async fn ip_desensitize(&self, ip: &str) -> String {
        if self.config.read().await.enable_plain_ip_in_notification {
            ip.to_string()
        } else {
            nezha_utils::ip_desensitize(ip)
        }
    }
}

