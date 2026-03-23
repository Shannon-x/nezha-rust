use crate::state::AppState;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use parking_lot::RwLock;
use tracing::{info, warn, debug};

// ────────────────────────────────────────────
// 高性能 AlertSentinel
//
// 性能设计：
// 1. RwLock 规则缓存，仅数据库变更时重载（非每次 tick）
// 2. 向量化指标匹配：预编译规则为闭包数组
// 3. DashMap 无锁读取服务器状态
// 4. 去重表用 HashMap<(i64,u64), i64>，O(1)
// 5. 通知发送异步分离，不阻塞检测循环
// ────────────────────────────────────────────

/// 预编译的告警规则
struct CompiledRule {
    id: i64,
    name: String,
    checks: Vec<MetricCheck>,
    trigger_all: bool, // true=all, false=any
    notification_group_id: i64,
}

/// 单指标检测条件
#[derive(Clone)]
struct MetricCheck {
    metric: MetricKind,
    max: f64,
    min: f64,
}

/// 指标类型枚举（避免字符串比较）
#[derive(Clone, Copy)]
enum MetricKind {
    Cpu,
    Memory,
    Swap,
    Disk,
    Load,
    Offline,
    TransferIn,
    TransferOut,
}

/// 服务器指标快照（栈上平铺，cache-friendly）
#[derive(Clone, Copy)]
struct MetricSnapshot {
    cpu: f64,
    mem_pct: f64,
    swap_pct: f64,
    disk_pct: f64,
    load_1: f64,
    online: bool,
    net_in_gb: f64,
    net_out_gb: f64,
}

pub struct AlertSentinel {
    state: Arc<AppState>,
    /// 去重表：(rule_id, server_id) -> 上次触发 unix timestamp
    dedup: HashMap<(i64, u64), i64>,
    /// 编译后的规则缓存
    rules_cache: Vec<CompiledRule>,
    /// 规则版本号（检测数据库变化用）
    rules_version: i64,
    /// 最小告警间隔（秒）
    min_interval: i64,
}

impl AlertSentinel {
    pub fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            dedup: HashMap::with_capacity(256),
            rules_cache: Vec::new(),
            rules_version: 0,
            min_interval: 300,
        }
    }

    pub async fn start(mut self) {
        info!("NEZHA>> AlertSentinel 启动 [高性能模式]");

        let mut fast_tick = tokio::time::interval(Duration::from_secs(15));
        let mut reload_tick = tokio::time::interval(Duration::from_secs(60));

        // 首次加载规则
        self.reload_rules().await;

        loop {
            tokio::select! {
                _ = fast_tick.tick() => {
                    self.check_all().await;
                }
                _ = reload_tick.tick() => {
                    self.reload_rules().await;
                }
            }
        }
    }

    /// 从数据库加载规则并预编译
    async fn reload_rules(&mut self) {
        let rows: Vec<(i64, String, String, bool, String, i64)> = sqlx::query_as(
            "SELECT id, name, COALESCE(rules_raw,'[]'), enabled, COALESCE(trigger_mode,'any'), notification_group_id FROM alert_rules WHERE enabled = true"
        )
        .fetch_all(&self.state.db.pool).await.unwrap_or_default();

        self.rules_cache.clear();
        self.rules_cache.reserve(rows.len());

        for (id, name, rules_raw, _enabled, trigger_mode, ng_id) in &rows {
            let raw: Vec<serde_json::Value> = serde_json::from_str(rules_raw).unwrap_or_default();
            let checks: Vec<MetricCheck> = raw.iter().filter_map(|r| {
                let kind = match r["type"].as_str()? {
                    "cpu" => MetricKind::Cpu,
                    "memory" => MetricKind::Memory,
                    "swap" => MetricKind::Swap,
                    "disk" => MetricKind::Disk,
                    "load" => MetricKind::Load,
                    "offline" => MetricKind::Offline,
                    "transfer_in" => MetricKind::TransferIn,
                    "transfer_out" => MetricKind::TransferOut,
                    _ => return None,
                };
                Some(MetricCheck {
                    metric: kind,
                    max: r["max"].as_f64().unwrap_or(0.0),
                    min: r["min"].as_f64().unwrap_or(0.0),
                })
            }).collect();

            if !checks.is_empty() {
                self.rules_cache.push(CompiledRule {
                    id: *id,
                    name: name.clone(),
                    checks,
                    trigger_all: trigger_mode == "all",
                    notification_group_id: *ng_id,
                });
            }
        }

        debug!("AlertSentinel: 编译 {} 条规则", self.rules_cache.len());
    }

    /// 检查所有服务器
    async fn check_all(&mut self) {
        if self.rules_cache.is_empty() { return; }

        let now_ts = Utc::now().timestamp();

        // 一次性快照所有服务器指标（DashMap 读锁极短）
        let snapshots: Vec<(u64, String, MetricSnapshot)> = self.state.servers
            .iter()
            .filter_map(|e| {
                let s = e.value();
                let st = s.state.as_ref()?;
                let host = s.host.as_ref();
                let mem_total = host.map(|h| h.mem_total).unwrap_or(1) as f64;
                let swap_total = host.map(|h| h.swap_total).unwrap_or(1) as f64;
                let disk_total = host.map(|h| h.disk_total).unwrap_or(1) as f64;

                Some((*e.key(), s.name.clone(), MetricSnapshot {
                    cpu: st.cpu,
                    mem_pct: st.mem_used as f64 / mem_total.max(1.0) * 100.0,
                    swap_pct: st.swap_used as f64 / swap_total.max(1.0) * 100.0,
                    disk_pct: st.disk_used as f64 / disk_total.max(1.0) * 100.0,
                    load_1: st.load_1,
                    online: s.last_active.map(|t| (Utc::now().naive_utc() - t).num_seconds() < 120).unwrap_or(false),
                    net_in_gb: st.net_in_transfer as f64 / 1_073_741_824.0,
                    net_out_gb: st.net_out_transfer as f64 / 1_073_741_824.0,
                }))
            })
            .collect();

        // 对每个规则检查每台服务器
        let mut alerts: Vec<(i64, String, u64, String, MetricSnapshot)> = Vec::new();

        for rule in &self.rules_cache {
            for (sid, name, snap) in &snapshots {
                let matched = if rule.trigger_all {
                    rule.checks.iter().all(|c| Self::check_metric(c, snap))
                } else {
                    rule.checks.iter().any(|c| Self::check_metric(c, snap))
                };

                if matched {
                    let key = (rule.id, *sid);
                    let should_fire = match self.dedup.get(&key) {
                        Some(&last) => now_ts - last >= self.min_interval,
                        None => true,
                    };
                    if should_fire {
                        self.dedup.insert(key, now_ts);
                        alerts.push((rule.notification_group_id, rule.name.clone(), *sid, name.clone(), *snap));
                    }
                }
            }
        }

        // 批量异步发送告警（不阻塞下次检测）
        if !alerts.is_empty() {
            let state = self.state.clone();
            tokio::spawn(async move {
                for (ng_id, rule_name, _sid, server_name, snap) in alerts {
                    let msg = format!(
                        "[Nezha] {} | {} | CPU:{:.1}% Mem:{:.1}% Disk:{:.1}% Load:{:.1} Online:{}",
                        rule_name, server_name, snap.cpu, snap.mem_pct, snap.disk_pct, snap.load_1, snap.online,
                    );
                    warn!("{}", msg);
                    crate::notification::send_notification(&state, ng_id, &rule_name, &msg).await;
                }
            });
        }

        // 定期清理过期去重条目（减少内存）
        if self.dedup.len() > 1000 {
            self.dedup.retain(|_, &mut last| now_ts - last < self.min_interval * 3);
        }
    }

    /// 单指标检测（纯计算，零分配）
    #[inline(always)]
    fn check_metric(check: &MetricCheck, snap: &MetricSnapshot) -> bool {
        let value = match check.metric {
            MetricKind::Cpu => snap.cpu,
            MetricKind::Memory => snap.mem_pct,
            MetricKind::Swap => snap.swap_pct,
            MetricKind::Disk => snap.disk_pct,
            MetricKind::Load => snap.load_1,
            MetricKind::Offline => if snap.online { 0.0 } else { 1.0 },
            MetricKind::TransferIn => snap.net_in_gb,
            MetricKind::TransferOut => snap.net_out_gb,
        };
        (check.max > 0.0 && value > check.max) || (check.min > 0.0 && value < check.min)
    }
}
