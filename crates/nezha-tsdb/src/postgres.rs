use crate::store::*;
use async_trait::async_trait;
use chrono::NaiveDateTime;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{info, warn};

// ────────────────────────────────────────────
// PostgreSQL TSDB 后端
//
// 与 MySQL 版的关键差异：
// 1. 占位符：$1, $2, $3（而非 ?）
// 2. 主键：BIGSERIAL（而非 BIGINT AUTO_INCREMENT）
// 3. 索引：CREATE INDEX IF NOT EXISTS（独立语句）
// 4. BOOL 类型：原生 BOOLEAN
// 5. 时间类型：TIMESTAMP（而非 DATETIME）
// 6. 无 ENGINE/CHARSET 子句
// ────────────────────────────────────────────

pub struct PostgresStore {
    pool: PgPool,
    closed: AtomicBool,
    retention_days: u16,
}

impl PostgresStore {
    pub async fn new(dsn: &str, retention_days: u16) -> anyhow::Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .min_connections(2)
            .acquire_timeout(std::time::Duration::from_secs(30))
            .connect(dsn)
            .await?;

        // 创建 TSDB 表（PostgreSQL 语法）
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS tsdb_server_metrics (
                id BIGSERIAL PRIMARY KEY,
                server_id BIGINT NOT NULL,
                ts TIMESTAMP NOT NULL,
                cpu DOUBLE PRECISION, mem_used BIGINT, swap_used BIGINT, disk_used BIGINT,
                net_in_speed BIGINT, net_out_speed BIGINT,
                net_in_transfer BIGINT, net_out_transfer BIGINT,
                load1 DOUBLE PRECISION, load5 DOUBLE PRECISION, load15 DOUBLE PRECISION,
                tcp_conn_count BIGINT, udp_conn_count BIGINT, process_count BIGINT,
                temperature DOUBLE PRECISION, uptime BIGINT, gpu DOUBLE PRECISION
            )"#,
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS tsdb_service_metrics (
                id BIGSERIAL PRIMARY KEY,
                service_id BIGINT NOT NULL,
                server_id BIGINT NOT NULL,
                ts TIMESTAMP NOT NULL,
                delay DOUBLE PRECISION,
                successful BOOLEAN
            )"#,
        )
        .execute(&pool)
        .await?;

        // 创建索引（PostgreSQL 不支持建表内 INDEX 语法）
        let indexes = [
            "CREATE INDEX IF NOT EXISTS idx_tsdb_sm_sid_ts ON tsdb_server_metrics (server_id, ts)",
            "CREATE INDEX IF NOT EXISTS idx_tsdb_sm_ts ON tsdb_server_metrics (ts)",
            "CREATE INDEX IF NOT EXISTS idx_tsdb_svm_svcid_ts ON tsdb_service_metrics (service_id, ts)",
            "CREATE INDEX IF NOT EXISTS idx_tsdb_svm_srvid_ts ON tsdb_service_metrics (server_id, ts)",
            "CREATE INDEX IF NOT EXISTS idx_tsdb_svm_ts ON tsdb_service_metrics (ts)",
        ];
        for idx_sql in &indexes {
            sqlx::query(idx_sql).execute(&pool).await.ok();
        }

        info!("PostgreSQL TSDB connected and tables created");
        Ok(Self {
            pool,
            closed: AtomicBool::new(false),
            retention_days,
        })
    }
}

#[async_trait]
impl Store for PostgresStore {
    async fn write_server_metrics(&self, m: &ServerMetrics) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO tsdb_server_metrics (server_id,ts,cpu,mem_used,swap_used,disk_used,net_in_speed,net_out_speed,net_in_transfer,net_out_transfer,load1,load5,load15,tcp_conn_count,udp_conn_count,process_count,temperature,uptime,gpu) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19)"
        )
        .bind(m.server_id as i64).bind(m.timestamp)
        .bind(m.cpu).bind(m.mem_used as i64).bind(m.swap_used as i64).bind(m.disk_used as i64)
        .bind(m.net_in_speed as i64).bind(m.net_out_speed as i64)
        .bind(m.net_in_transfer as i64).bind(m.net_out_transfer as i64)
        .bind(m.load1).bind(m.load5).bind(m.load15)
        .bind(m.tcp_conn_count as i64).bind(m.udp_conn_count as i64).bind(m.process_count as i64)
        .bind(m.temperature).bind(m.uptime as i64).bind(m.gpu)
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn write_service_metrics(&self, m: &ServiceMetrics) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO tsdb_service_metrics (service_id,server_id,ts,delay,successful) VALUES ($1,$2,$3,$4,$5)"
        )
        .bind(m.service_id as i64).bind(m.server_id as i64)
        .bind(m.timestamp).bind(m.delay).bind(m.successful)
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn query_service_history(&self, service_id: u64, period: QueryPeriod) -> anyhow::Result<ServiceHistoryResult> {
        let hours = period_hours(period);
        let cutoff = chrono::Utc::now().naive_utc() - chrono::Duration::hours(hours);

        let rows: Vec<(i64, i64, i64, f64)> = sqlx::query_as(
            "SELECT server_id, SUM(CASE WHEN successful THEN 1 ELSE 0 END)::BIGINT, SUM(CASE WHEN NOT successful THEN 1 ELSE 0 END)::BIGINT, COALESCE(AVG(delay),0) FROM tsdb_service_metrics WHERE service_id = $1 AND ts > $2 GROUP BY server_id"
        )
        .bind(service_id as i64).bind(cutoff)
        .fetch_all(&self.pool).await?;

        let servers = rows.iter().map(|(sid, up, down, delay)| {
            ServerServiceStats {
                server_id: *sid as u64,
                stats: ServiceStatsSummary { total_up: *up as u64, total_down: *down as u64, avg_delay: *delay },
            }
        }).collect();

        Ok(ServiceHistoryResult { servers })
    }

    async fn query_service_daily_stats(&self, service_id: u64, today: NaiveDateTime, days: i32) -> anyhow::Result<Vec<DailyServiceStats>> {
        let mut stats = vec![DailyServiceStats::default(); days as usize];
        let start = today - chrono::Duration::days(days as i64 - 1);

        // PostgreSQL: DATE() → ts::DATE, 结果需要 TEXT 转换
        let rows: Vec<(String, i64, i64, f64)> = sqlx::query_as(
            "SELECT ts::DATE::TEXT as d, SUM(CASE WHEN successful THEN 1 ELSE 0 END)::BIGINT, SUM(CASE WHEN NOT successful THEN 1 ELSE 0 END)::BIGINT, COALESCE(AVG(delay),0) FROM tsdb_service_metrics WHERE service_id = $1 AND ts >= $2 GROUP BY d ORDER BY d"
        )
        .bind(service_id as i64).bind(start)
        .fetch_all(&self.pool).await?;

        for (date_str, up, down, delay) in &rows {
            if let Ok(date) = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                let idx = (date - start.date()).num_days() as usize;
                if idx < stats.len() {
                    stats[idx] = DailyServiceStats { up: *up as u64, down: *down as u64, delay: *delay };
                }
            }
        }
        Ok(stats)
    }

    async fn query_server_metrics(&self, server_id: u64, metric: MetricType, period: QueryPeriod) -> anyhow::Result<Vec<MetricDataPoint>> {
        let hours = period_hours(period);
        let cutoff = chrono::Utc::now().naive_utc() - chrono::Duration::hours(hours);
        let col = metric_column(metric);

        // PostgreSQL: 使用 $1, $2 占位符
        let query = format!(
            "SELECT ts, {}::DOUBLE PRECISION FROM tsdb_server_metrics WHERE server_id = $1 AND ts > $2 ORDER BY ts", col
        );
        let rows: Vec<(NaiveDateTime, f64)> = sqlx::query_as(&query)
            .bind(server_id as i64).bind(cutoff)
            .fetch_all(&self.pool).await?;

        Ok(rows.iter().map(|(ts, v)| MetricDataPoint { timestamp: *ts, value: *v }).collect())
    }

    async fn query_service_history_by_server_id(&self, server_id: u64, period: QueryPeriod) -> anyhow::Result<HashMap<u64, ServiceHistoryResult>> {
        let hours = period_hours(period);
        let cutoff = chrono::Utc::now().naive_utc() - chrono::Duration::hours(hours);

        let rows: Vec<(i64, i64, i64, f64)> = sqlx::query_as(
            "SELECT service_id, SUM(CASE WHEN successful THEN 1 ELSE 0 END)::BIGINT, SUM(CASE WHEN NOT successful THEN 1 ELSE 0 END)::BIGINT, COALESCE(AVG(delay),0) FROM tsdb_service_metrics WHERE server_id = $1 AND ts > $2 GROUP BY service_id"
        )
        .bind(server_id as i64).bind(cutoff)
        .fetch_all(&self.pool).await?;

        let mut result = HashMap::new();
        for (sid, up, down, delay) in rows {
            result.insert(sid as u64, ServiceHistoryResult {
                servers: vec![ServerServiceStats {
                    server_id,
                    stats: ServiceStatsSummary { total_up: up as u64, total_down: down as u64, avg_delay: delay },
                }],
            });
        }
        Ok(result)
    }

    async fn maintenance(&self) {
        let cutoff = chrono::Utc::now().naive_utc() - chrono::Duration::days(self.retention_days as i64);
        // PostgreSQL: 分批删除避免长事务（每次最多 10000 行）
        for table in &["tsdb_server_metrics", "tsdb_service_metrics"] {
            let sql = format!("DELETE FROM {} WHERE ctid IN (SELECT ctid FROM {} WHERE ts < $1 LIMIT 10000)", table, table);
            loop {
                match sqlx::query(&sql).bind(cutoff).execute(&self.pool).await {
                    Ok(r) if r.rows_affected() > 0 => continue,
                    _ => break,
                }
            }
        }
        // VACUUM ANALYZE 以回收空间并更新统计
        let _ = sqlx::query("ANALYZE tsdb_server_metrics").execute(&self.pool).await;
        let _ = sqlx::query("ANALYZE tsdb_service_metrics").execute(&self.pool).await;
        info!("PostgreSQL TSDB maintenance completed, removed data older than {} days", self.retention_days);
    }

    async fn flush(&self) {
        // PostgreSQL auto-flushes via WAL
    }

    async fn close(&self) -> anyhow::Result<()> {
        self.closed.store(true, Ordering::SeqCst);
        self.pool.close().await;
        info!("PostgreSQL TSDB closed");
        Ok(())
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }
}

/// 查询周期 → 小时数
#[inline]
fn period_hours(period: QueryPeriod) -> i64 {
    match period {
        QueryPeriod::Hour1 => 1,
        QueryPeriod::Hour6 => 6,
        QueryPeriod::Day1 => 24,
        QueryPeriod::Week1 => 168,
        QueryPeriod::Month1 => 720,
    }
}

/// 指标类型 → 列名
#[inline]
fn metric_column(metric: MetricType) -> &'static str {
    match metric {
        MetricType::CPU => "cpu",
        MetricType::Memory => "mem_used",
        MetricType::Swap => "swap_used",
        MetricType::Disk => "disk_used",
        MetricType::NetInSpeed => "net_in_speed",
        MetricType::NetOutSpeed => "net_out_speed",
        MetricType::Load => "load1",
        MetricType::Temperature => "temperature",
        MetricType::GPU => "gpu",
    }
}
