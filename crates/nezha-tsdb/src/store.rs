use async_trait::async_trait;
use chrono::NaiveDateTime;

/// 服务器指标
#[derive(Debug, Clone)]
pub struct ServerMetrics {
    pub server_id: u64,
    pub timestamp: NaiveDateTime,
    pub cpu: f64,
    pub mem_used: u64,
    pub swap_used: u64,
    pub disk_used: u64,
    pub net_in_speed: u64,
    pub net_out_speed: u64,
    pub net_in_transfer: u64,
    pub net_out_transfer: u64,
    pub load1: f64,
    pub load5: f64,
    pub load15: f64,
    pub tcp_conn_count: u64,
    pub udp_conn_count: u64,
    pub process_count: u64,
    pub temperature: f64,
    pub uptime: u64,
    pub gpu: f64,
}

/// 服务监控指标
#[derive(Debug, Clone)]
pub struct ServiceMetrics {
    pub service_id: u64,
    pub server_id: u64,
    pub timestamp: NaiveDateTime,
    pub delay: f64,
    pub successful: bool,
}

/// 查询周期
#[derive(Debug, Clone, Copy)]
pub enum QueryPeriod {
    Hour1,
    Hour6,
    Day1,
    Week1,
    Month1,
}

/// 指标类型
#[derive(Debug, Clone, Copy)]
pub enum MetricType {
    CPU,
    Memory,
    Swap,
    Disk,
    NetInSpeed,
    NetOutSpeed,
    Load,
    Temperature,
    GPU,
}

/// 指标数据点
#[derive(Debug, Clone)]
pub struct MetricDataPoint {
    pub timestamp: NaiveDateTime,
    pub value: f64,
}

/// 每日服务统计
#[derive(Debug, Clone, Default)]
pub struct DailyServiceStats {
    pub up: u64,
    pub down: u64,
    pub delay: f64,
}

/// 服务监控历史查询结果
#[derive(Debug, Clone)]
pub struct ServiceHistoryResult {
    pub servers: Vec<ServerServiceStats>,
}

#[derive(Debug, Clone)]
pub struct ServerServiceStats {
    pub server_id: u64,
    pub stats: ServiceStatsSummary,
}

#[derive(Debug, Clone)]
pub struct ServiceStatsSummary {
    pub total_up: u64,
    pub total_down: u64,
    pub avg_delay: f64,
}

/// TSDB 存储后端统一接口
#[async_trait]
pub trait Store: Send + Sync {
    /// 写入服务器指标
    async fn write_server_metrics(&self, m: &ServerMetrics) -> anyhow::Result<()>;
    /// 写入服务监控指标
    async fn write_service_metrics(&self, m: &ServiceMetrics) -> anyhow::Result<()>;

    /// 查询服务历史
    async fn query_service_history(
        &self,
        service_id: u64,
        period: QueryPeriod,
    ) -> anyhow::Result<ServiceHistoryResult>;

    /// 查询服务每日统计
    async fn query_service_daily_stats(
        &self,
        service_id: u64,
        today: NaiveDateTime,
        days: i32,
    ) -> anyhow::Result<Vec<DailyServiceStats>>;

    /// 查询服务器指标
    async fn query_server_metrics(
        &self,
        server_id: u64,
        metric: MetricType,
        period: QueryPeriod,
    ) -> anyhow::Result<Vec<MetricDataPoint>>;

    /// 按服务器 ID 查询服务历史
    async fn query_service_history_by_server_id(
        &self,
        server_id: u64,
        period: QueryPeriod,
    ) -> anyhow::Result<std::collections::HashMap<u64, ServiceHistoryResult>>;

    /// 维护（清理过期数据等）
    async fn maintenance(&self);
    /// 刷盘
    async fn flush(&self);
    /// 关闭
    async fn close(&self) -> anyhow::Result<()>;
    /// 是否已关闭
    fn is_closed(&self) -> bool;
}
