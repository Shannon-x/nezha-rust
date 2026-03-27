use crate::store::*;
use async_trait::async_trait;
use chrono::NaiveDateTime;
use sqlx::mysql::{MySqlPool, MySqlPoolOptions};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::info;

/// MySQL TSDB 后端
pub struct MysqlStore {
    pool: MySqlPool,
    closed: AtomicBool,
    retention_days: u16,
}

impl MysqlStore {
    pub async fn new(dsn: &str, retention_days: u16) -> anyhow::Result<Self> {
        let pool = MySqlPoolOptions::new()
            .max_connections(10)
            .min_connections(2)
            .acquire_timeout(std::time::Duration::from_secs(30))
            .connect(dsn)
            .await?;

        // 创建TSDB表
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS tsdb_server_metrics (
                id BIGINT AUTO_INCREMENT PRIMARY KEY,
                server_id BIGINT NOT NULL,
                timestamp DATETIME NOT NULL,
                cpu DOUBLE, mem_used BIGINT, swap_used BIGINT, disk_used BIGINT,
                net_in_speed BIGINT, net_out_speed BIGINT,
                net_in_transfer BIGINT, net_out_transfer BIGINT,
                load1 DOUBLE, load5 DOUBLE, load15 DOUBLE,
                tcp_conn_count BIGINT, udp_conn_count BIGINT, process_count BIGINT,
                temperature DOUBLE, uptime BIGINT, gpu DOUBLE,
                INDEX idx_sid_ts (server_id, timestamp),
                INDEX idx_ts (timestamp)
            ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4"#,
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS tsdb_service_metrics (
                id BIGINT AUTO_INCREMENT PRIMARY KEY,
                service_id BIGINT NOT NULL,
                server_id BIGINT NOT NULL,
                timestamp DATETIME NOT NULL,
                delay DOUBLE,
                successful BOOLEAN,
                INDEX idx_svcid_ts (service_id, timestamp),
                INDEX idx_srvid_ts (server_id, timestamp),
                INDEX idx_ts (timestamp)
            ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4"#,
        )
        .execute(&pool)
        .await?;

        info!("MySQL TSDB connected and tables created");
        Ok(Self {
            pool,
            closed: AtomicBool::new(false),
            retention_days,
        })
    }
}

#[async_trait]
impl Store for MysqlStore {
    async fn write_server_metrics(&self, m: &ServerMetrics) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO tsdb_server_metrics (server_id,timestamp,cpu,mem_used,swap_used,disk_used,net_in_speed,net_out_speed,net_in_transfer,net_out_transfer,load1,load5,load15,tcp_conn_count,udp_conn_count,process_count,temperature,uptime,gpu) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"
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
            "INSERT INTO tsdb_service_metrics (service_id,server_id,timestamp,delay,successful) VALUES (?,?,?,?,?)"
        )
        .bind(m.service_id as i64).bind(m.server_id as i64)
        .bind(m.timestamp).bind(m.delay).bind(m.successful)
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn query_service_history(&self, service_id: u64, period: QueryPeriod) -> anyhow::Result<ServiceHistoryResult> {
        let hours = match period {
            QueryPeriod::Hour1 => 1, QueryPeriod::Hour6 => 6, QueryPeriod::Day1 => 24,
            QueryPeriod::Week1 => 168, QueryPeriod::Month1 => 720,
        };
        let cutoff = chrono::Utc::now().naive_utc() - chrono::Duration::hours(hours);

        let rows: Vec<(i64, i64, i64, f64)> = sqlx::query_as(
            "SELECT server_id, SUM(CASE WHEN successful THEN 1 ELSE 0 END), SUM(CASE WHEN NOT successful THEN 1 ELSE 0 END), AVG(delay) FROM tsdb_service_metrics WHERE service_id = ? AND timestamp > ? GROUP BY server_id"
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

        let rows: Vec<(String, i64, i64, f64)> = sqlx::query_as(
            "SELECT DATE(timestamp) as d, SUM(CASE WHEN successful THEN 1 ELSE 0 END), SUM(CASE WHEN NOT successful THEN 1 ELSE 0 END), AVG(delay) FROM tsdb_service_metrics WHERE service_id = ? AND timestamp >= ? GROUP BY d ORDER BY d"
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
        let hours = match period {
            QueryPeriod::Hour1 => 1, QueryPeriod::Hour6 => 6, QueryPeriod::Day1 => 24,
            QueryPeriod::Week1 => 168, QueryPeriod::Month1 => 720,
        };
        let cutoff = chrono::Utc::now().naive_utc() - chrono::Duration::hours(hours);
        let col = match metric {
            MetricType::CPU => "cpu", MetricType::Memory => "mem_used", MetricType::Swap => "swap_used",
            MetricType::Disk => "disk_used", MetricType::NetInSpeed => "net_in_speed",
            MetricType::NetOutSpeed => "net_out_speed", MetricType::Load => "load1",
            MetricType::Temperature => "temperature", MetricType::GPU => "gpu",
        };

        let query = format!("SELECT timestamp, {} FROM tsdb_server_metrics WHERE server_id = ? AND timestamp > ? ORDER BY timestamp", col);
        let rows: Vec<(NaiveDateTime, f64)> = sqlx::query_as(&query)
            .bind(server_id as i64).bind(cutoff)
            .fetch_all(&self.pool).await?;

        Ok(rows.iter().map(|(ts, v)| MetricDataPoint { timestamp: *ts, value: *v }).collect())
    }

    async fn query_service_history_by_server_id(&self, server_id: u64, period: QueryPeriod) -> anyhow::Result<HashMap<u64, ServiceHistoryResult>> {
        let hours = match period {
            QueryPeriod::Hour1 => 1, QueryPeriod::Hour6 => 6, QueryPeriod::Day1 => 24,
            QueryPeriod::Week1 => 168, QueryPeriod::Month1 => 720,
        };
        let cutoff = chrono::Utc::now().naive_utc() - chrono::Duration::hours(hours);

        let rows: Vec<(i64, i64, i64, f64)> = sqlx::query_as(
            "SELECT service_id, SUM(CASE WHEN successful THEN 1 ELSE 0 END), SUM(CASE WHEN NOT successful THEN 1 ELSE 0 END), AVG(delay) FROM tsdb_service_metrics WHERE server_id = ? AND timestamp > ? GROUP BY service_id"
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

    async fn query_service_datapoints(&self, service_id: u64, server_id: u64, period: QueryPeriod) -> anyhow::Result<Vec<(i64, f64)>> {
        let hours = match period {
            QueryPeriod::Hour1 => 1, QueryPeriod::Hour6 => 6, QueryPeriod::Day1 => 24,
            QueryPeriod::Week1 => 168, QueryPeriod::Month1 => 720,
        };
        let cutoff = chrono::Utc::now().naive_utc() - chrono::Duration::hours(hours);

        let rows: Vec<(NaiveDateTime, f64)> = sqlx::query_as(
            "SELECT timestamp, delay FROM tsdb_service_metrics WHERE service_id = ? AND server_id = ? AND timestamp > ? AND successful = 1 ORDER BY timestamp"
        )
        .bind(service_id as i64).bind(server_id as i64).bind(cutoff)
        .fetch_all(&self.pool).await?;

        Ok(rows.into_iter().map(|(ts, delay)| (ts.and_utc().timestamp_millis(), delay)).collect())
    }


    async fn maintenance(&self) {
        let cutoff = chrono::Utc::now().naive_utc() - chrono::Duration::days(self.retention_days as i64);
        // 分批删除，每次最多 10000 行，避免 InnoDB row-lock 风暴
        for table in &["tsdb_server_metrics", "tsdb_service_metrics"] {
            let sql = format!("DELETE FROM {} WHERE timestamp < ? LIMIT 10000", table);
            loop {
                match sqlx::query(&sql).bind(cutoff).execute(&self.pool).await {
                    Ok(r) if r.rows_affected() > 0 => continue,
                    _ => break,
                }
            }
        }
        info!("MySQL TSDB maintenance completed, removed data older than {} days", self.retention_days);
    }

    async fn flush(&self) {
        // MySQL auto-flushes via InnoDB
    }

    async fn close(&self) -> anyhow::Result<()> {
        self.closed.store(true, Ordering::SeqCst);
        self.pool.close().await;
        info!("MySQL TSDB closed");
        Ok(())
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }
}
