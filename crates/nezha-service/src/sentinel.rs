use crate::state::AppState;
use chrono::{NaiveDateTime, Utc};
use nezha_core::models::service::Service;
use nezha_tsdb::{ServiceMetrics, Store};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tracing::{info, debug};

// ────────────────────────────────────────────
// 高性能 ServiceSentinel
//
// 性能设计：
// 1. 全局 reqwest::Client 复用（连接池+keep-alive）
// 2. Semaphore 限制并发检测数（防止 fd 耗尽）
// 3. 批量 TSDB 写入通过 mpsc channel
// 4. 零分配热路径：检测结果用栈上结构
// 5. 按服务 duration 精确调度，避免无效唤醒
// ────────────────────────────────────────────

/// 检测结果（栈分配，零堆开销）
#[derive(Debug, Clone, Copy)]
struct CheckResult {
    success: bool,
    delay_ms: f64,
}

/// 批量 TSDB 写入消息
struct TsdbBatch {
    service_id: u64,
    server_id: u64,
    timestamp: NaiveDateTime,
    delay: f64,
    successful: bool,
}

pub struct ServiceSentinel {
    state: Arc<AppState>,
}

impl ServiceSentinel {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    /// 启动监控引擎
    pub async fn start(self) {
        let state = self.state.clone();
        info!("NEZHA>> ServiceSentinel 启动 [高性能模式]");

        // 加载服务列表到内存
        Self::load_services(&state).await;

        // 共享 HTTP 客户端（连接池复用）
        let http_client = Arc::new(
            reqwest::Client::builder()
                .pool_max_idle_per_host(20)
                .pool_idle_timeout(Duration::from_secs(90))
                .timeout(Duration::from_secs(10))
                .connect_timeout(Duration::from_secs(5))
                .tcp_keepalive(Duration::from_secs(30))
                .redirect(reqwest::redirect::Policy::limited(3))
                .build()
                .unwrap_or_default(),
        );

        // 跳过 TLS 验证的客户端
        let http_client_insecure = Arc::new(
            reqwest::Client::builder()
                .danger_accept_invalid_certs(true)
                .pool_max_idle_per_host(10)
                .pool_idle_timeout(Duration::from_secs(90))
                .timeout(Duration::from_secs(10))
                .connect_timeout(Duration::from_secs(5))
                .build()
                .unwrap_or_default(),
        );

        // 并发限制信号量（防止 fd 耗尽）
        let semaphore = Arc::new(Semaphore::new(64));

        // TSDB 批量写入 channel
        let (tsdb_tx, mut tsdb_rx) = tokio::sync::mpsc::channel::<TsdbBatch>(2048);

        // 启动 TSDB 批量写入任务
        if let Some(ref tsdb) = state.tsdb {
            let tsdb_clone = tsdb.clone();
            tokio::spawn(async move {
                let mut batch = Vec::with_capacity(128);
                let mut flush_interval = tokio::time::interval(Duration::from_secs(3));

                loop {
                    tokio::select! {
                        Some(item) = tsdb_rx.recv() => {
                            batch.push(item);
                            if batch.len() >= 128 {
                                Self::flush_batch(&tsdb_clone, &mut batch).await;
                            }
                        }
                        _ = flush_interval.tick() => {
                            if !batch.is_empty() {
                                Self::flush_batch(&tsdb_clone, &mut batch).await;
                            }
                        }
                    }
                }
            });
        }

        // 主调度循环（5 秒精度）
        let mut tick = tokio::time::interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            let now = Utc::now().naive_utc();

            // 快照服务列表（减少锁持有时间）
            let tasks: Vec<(u64, Service)> = state.services
                .iter()
                .filter_map(|e| {
                    let svc = e.value();
                    // 精确判断是否到达检测间隔
                    let should_run = match svc.last_check {
                        Some(last) => (now - last).num_seconds() >= svc.duration as i64,
                        None => true,
                    };
                    if should_run { Some((*e.key(), svc.clone())) } else { None }
                })
                .collect();

            if tasks.is_empty() { continue; }
            debug!("ServiceSentinel: 调度 {} 个检测任务", tasks.len());

            for (id, svc) in tasks {
                let state_c = state.clone();
                let sem = semaphore.clone();
                let client = http_client.clone();
                let client_insecure = http_client_insecure.clone();
                let tx = tsdb_tx.clone();

                tokio::spawn(async move {
                    // 获取并发许可
                    let _permit = sem.acquire().await;

                    let result = match svc.r#type {
                        0 => Self::check_http(&client, &svc.target).await,
                        4 => Self::check_http(&client_insecure, &svc.target).await,
                        1 => Self::check_tcp(&svc.target).await,
                        2 | 3 => Self::check_icmp(&svc.target).await,
                        _ => CheckResult { success: false, delay_ms: 0.0 },
                    };

                    let now = Utc::now().naive_utc();

                    // 原子更新内存状态
                    if let Some(mut s) = state_c.services.get_mut(&id) {
                        s.last_check = Some(now);
                        s.current_up = result.success;
                        s.current_down = !result.success;
                        s.delay = result.delay_ms;
                    }

                    // 异步写入 TSDB（非阻塞）
                    let _ = tx.try_send(TsdbBatch {
                        service_id: id,
                        server_id: 0,
                        timestamp: now,
                        delay: result.delay_ms,
                        successful: result.success,
                    });
                });
            }
        }
    }

    /// 批量刷写 TSDB
    #[inline]
    async fn flush_batch(tsdb: &Arc<dyn Store>, batch: &mut Vec<TsdbBatch>) {
        for item in batch.drain(..) {
            let _ = tsdb.write_service_metrics(&ServiceMetrics {
                service_id: item.service_id,
                server_id: item.server_id,
                timestamp: item.timestamp,
                delay: item.delay,
                successful: item.successful,
            }).await;
        }
    }

    /// HTTP 检测（复用连接池）
    #[inline]
    async fn check_http(client: &reqwest::Client, target: &str) -> CheckResult {
        let start = std::time::Instant::now();
        match client.get(target).send().await {
            Ok(resp) => {
                let delay = start.elapsed().as_secs_f64() * 1000.0;
                CheckResult {
                    success: resp.status().is_success() || resp.status().is_redirection(),
                    delay_ms: delay,
                }
            }
            Err(_) => CheckResult {
                success: false,
                delay_ms: start.elapsed().as_secs_f64() * 1000.0,
            },
        }
    }

    /// TCP 检测（直接系统调用，极低开销）
    #[inline]
    async fn check_tcp(target: &str) -> CheckResult {
        let start = std::time::Instant::now();
        match tokio::time::timeout(
            Duration::from_secs(10),
            tokio::net::TcpStream::connect(target),
        ).await {
            Ok(Ok(stream)) => {
                drop(stream); // 立即释放 fd
                CheckResult {
                    success: true,
                    delay_ms: start.elapsed().as_secs_f64() * 1000.0,
                }
            }
            _ => CheckResult {
                success: false,
                delay_ms: start.elapsed().as_secs_f64() * 1000.0,
            },
        }
    }

    /// ICMP Ping（降级为 TCP:80，无需 root）
    #[inline]
    async fn check_icmp(target: &str) -> CheckResult {
        let addr = if target.contains(':') {
            target.to_string()
        } else {
            format!("{}:80", target)
        };
        Self::check_tcp(&addr).await
    }

    /// 加载服务列表
    async fn load_services(state: &Arc<AppState>) {
        let rows: Vec<(
            i64, String, i32, String, i32, bool, i32, bool, i32, i64,
            String, String, String, f64, f64, bool, bool
        )> = sqlx::query_as(
            "SELECT id, name, type, target, duration, notify, cover, enable_show_in_service, display_index, notification_group_id, COALESCE(skip_servers_raw,'{}'), COALESCE(fail_trigger_tasks_raw,'[]'), COALESCE(recover_trigger_tasks_raw,'[]'), min_latency, max_latency, latency_notify, enable_trigger_task FROM services"
        )
        .fetch_all(&state.db.pool).await.unwrap_or_default();

        for row in rows {
            let skip_servers: std::collections::HashMap<u64, bool> = serde_json::from_str(&row.10).unwrap_or_default();
            let fail_trigger_tasks: Vec<u64> = serde_json::from_str(&row.11).unwrap_or_default();
            let recover_trigger_tasks: Vec<u64> = serde_json::from_str(&row.12).unwrap_or_default();

            state.services.insert(row.0 as u64, Service {
                id: row.0,
                name: row.1,
                r#type: row.2,
                target: row.3,
                duration: if row.4 < 5 { 30 } else { row.4 },
                notify: row.5,
                cover: row.6,
                enable_show_in_service: row.7,
                display_index: row.8,
                notification_group_id: row.9 as u64,
                skip_servers,
                fail_trigger_tasks,
                recover_trigger_tasks,
                min_latency: row.13 as f32,
                max_latency: row.14 as f32,
                latency_notify: row.15,
                enable_trigger_task: row.16,
            });
        }
        info!("ServiceSentinel: 加载 {} 个监控服务", state.services.len());
    }
}
