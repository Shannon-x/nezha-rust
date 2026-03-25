use std::net::SocketAddr;

use anyhow::Context;
use nezha_api::create_router;
use nezha_core::Config;

use nezha_proto::nezha_service_server::NezhaServiceServer;
use nezha_rpc::NezhaHandler;
use nezha_service::AppState;
use nezha_service::sentinel::ServiceSentinel;
use nezha_service::alert::AlertSentinel;
use nezha_service::cron::CronManager;
use tokio::signal;
use tracing::info;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 解析命令行参数
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "-v" || a == "--version") {
        println!("nezha-dashboard v{}", VERSION);
        return Ok(());
    }

    let config_path = args
        .windows(2)
        .find(|w| w[0] == "-c" || w[0] == "--config")
        .map(|w| w[1].as_str())
        .unwrap_or("data/config.yaml");

    // 初始化日志
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sqlx=warn".into()),
        )
        .init();

    info!("NEZHA>> Starting Nezha Dashboard v{}", VERSION);

    // 加载配置
    let config = Config::load(config_path)
        .with_context(|| format!("Failed to load config from '{}'", config_path))?;
    let host = if config.listen_host.is_empty() { "0.0.0.0" } else { &config.listen_host };
    let listen_addr: SocketAddr =
        format!("{}:{}", host, config.listen_port).parse()?;

    // 初始化 i18n
    nezha_utils::i18n::init_i18n(&config.language);

    // 初始化应用状态
    let state = AppState::new(config).await
        .context("Failed to initialize application state (check database/TSDB path permissions)")?;

    // 创建默认管理员账户
    nezha_api::handlers::auth::ensure_admin(&state).await?;

    // 启动 ServiceSentinel 监控引擎
    let sentinel = ServiceSentinel::new(state.clone());
    tokio::spawn(async move { sentinel.start().await });

    // 启动 AlertSentinel 告警引擎
    let alert = AlertSentinel::new(state.clone());
    tokio::spawn(async move { alert.start().await });

    // 启动 CronManager 定时任务
    let cron = CronManager::new(state.clone());
    tokio::spawn(async move { cron.start().await });

    // TSDB 定期维护
    if let Some(ref tsdb) = state.tsdb {
        let tsdb_clone = tsdb.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
            loop {
                interval.tick().await;
                tsdb_clone.maintenance().await;
            }
        });
    }

    // 创建 gRPC 服务（Agent 通信）
    let grpc_handler = NezhaHandler::new(state.clone());
    let grpc_service = NezhaServiceServer::new(grpc_handler);

    // 创建 HTTP 路由（Web API + 前端）
    let http_app = create_router(state.clone());

    info!("NEZHA>> Dashboard started on {}", listen_addr);
    info!("NEZHA>> API: http://{}/api/v1/", listen_addr);
    info!("NEZHA>> gRPC: {} (same port, multiplexed)", listen_addr);

    // 使用 tonic 的 multiplex 同时服务 gRPC 和 HTTP
    // gRPC 请求 (content-type: application/grpc) 走 tonic
    // 其他请求走 axum HTTP
    let listener = tokio::net::TcpListener::bind(listen_addr).await?;

    // 使用 axum 的路由嵌入 tonic gRPC
    use tower::ServiceExt;
    let grpc_service_cloned = grpc_service.clone();

    // 构建混合服务：gRPC + HTTP 在同一端口
    let combined = axum::Router::new()
        .merge(http_app)
        .route_service(
            &format!("/{}/{{method}}", <NezhaServiceServer<NezhaHandler> as tonic::server::NamedService>::NAME),
            grpc_service_cloned,
        )
        .into_make_service();

    // 使用 hyper 启动混合服务
    axum::serve(listener, combined)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    // 优雅关闭
    info!("NEZHA>> Graceful shutdown...");
    if let Some(ref tsdb) = state.tsdb {
        let _ = tsdb.close().await;
    }
    info!("NEZHA>> Shutdown complete");

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
