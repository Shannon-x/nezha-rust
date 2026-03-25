use crate::state::AppState;
use chrono::Utc;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn, error};

/// Cron 任务调度器
///
/// 管理定时任务：向指定服务器下发命令执行
pub struct CronManager {
    state: Arc<AppState>,
}

impl CronManager {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    /// 启动 Cron 调度循环
    pub async fn start(self) {
        info!("NEZHA>> CronManager 已启动");

        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            self.tick().await;
        }
    }

    /// 每分钟 tick，检查需要执行的任务
    async fn tick(&self) {
        let rows: Vec<(i64, String, i32, String, String, String, i32, i64, i32)> = sqlx::query_as(
            "SELECT id, name, task_type, scheduler, command, COALESCE(servers,'[]'), cover, notification_group_id, CAST(push_successful AS INTEGER) FROM crons"
        )
        .fetch_all(&self.state.db.pool).await.unwrap_or_default();

        let now = Utc::now();

        for (id, name, task_type, scheduler, command, servers_str, cover, _ng_id, _push_ok) in &rows {
            // 简化的 cron 调度匹配（完整实现需要 cron 表达式解析）
            if !self.should_run(&scheduler, &now) {
                continue;
            }

            info!("CronManager: 执行任务 {} ({})", name, command);

            // 获取要执行的服务器列表
            let server_ids: Vec<u64> = serde_json::from_str(servers_str).unwrap_or_default();

            match *task_type {
                // 命令执行任务 - 通过 gRPC 下发到 Agent
                0 => {
                    self.dispatch_command(&server_ids, *cover, command).await;
                }
                // HTTP GET 回调任务
                1 => {
                    self.http_callback(command).await;
                }
                _ => {
                    warn!("CronManager: 未知任务类型 {}", task_type);
                }
            }

            // 更新最后执行时间
            sqlx::query("UPDATE crons SET last_executed_at = ?, last_result = '' WHERE id = ?")
                .bind(now.naive_utc().format("%Y-%m-%d %H:%M:%S").to_string()).bind(id)
                .execute(&self.state.db.pool).await.ok();
        }
    }

    /// 简化的 cron 表达式匹配
    fn should_run(&self, scheduler: &str, now: &chrono::DateTime<Utc>) -> bool {
        // 极简实现：假设每 N 分钟执行一次
        // 真正实现需要使用 croner 或 cron crate 解析
        // 这里简化为：如果当前分钟是 0 则执行 "0 * * * *" 类似的规则
        if scheduler.starts_with("@every") {
            // @every 5m 格式
            if let Some(s) = scheduler.strip_prefix("@every ") {
                let s = s.trim();
                if let Some(mins) = s.strip_suffix('m') {
                    if let Ok(n) = mins.parse::<u32>() {
                        return now.timestamp() as u32 % (n * 60) < 60;
                    }
                }
                if let Some(hours) = s.strip_suffix('h') {
                    if let Ok(n) = hours.parse::<u32>() {
                        return now.timestamp() as u32 % (n * 3600) < 60;
                    }
                }
            }
        }

        // 标准 cron 表达式：简化解析
        let parts: Vec<&str> = scheduler.split_whitespace().collect();
        if parts.len() >= 5 {
            let minute = now.format("%M").to_string();
            let hour = now.format("%H").to_string();

            let min_match = parts[0] == "*" || parts[0] == &minute
                || parts[0].parse::<u32>().ok() == minute.parse::<u32>().ok();
            let hour_match = parts[1] == "*" || parts[1] == &hour
                || parts[1].parse::<u32>().ok() == hour.parse::<u32>().ok();

            return min_match && hour_match;
        }

        false
    }

    /// 通过 gRPC 下发命令到 Agent
    async fn dispatch_command(&self, server_ids: &[u64], cover: i32, command: &str) {
        // cover=0 忽略特定服务器, cover=1 只在特定服务器执行
        let target_servers = if cover == 1 {
            server_ids.to_vec()
        } else {
            // 所有服务器除了排除列表
            self.state.servers.iter()
                .filter(|e| !server_ids.contains(e.key()))
                .map(|e| *e.key())
                .collect()
        };

        for sid in target_servers {
            if let Some(sender) = self.state.task_senders.get(&sid) {
                let task = nezha_proto::Task {
                    id: 0,
                    r#type: 0,
                    data: command.to_string(),
                };
                if let Err(e) = sender.send(Ok(task)).await {
                    warn!("CronManager: 向服务器 {} 下发任务失败: {}", sid, e);
                    // 通道已关闭，移除
                    self.state.task_senders.remove(&sid);
                } else {
                    info!("CronManager: 向服务器 {} 下发命令成功", sid);
                }
            } else {
                warn!("CronManager: 服务器 {} 未连接，跳过", sid);
            }
        }
    }

    /// HTTP 回调任务
    async fn http_callback(&self, url: &str) {
        match reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default()
            .get(url)
            .send()
            .await
        {
            Ok(resp) => {
                info!("CronManager: HTTP 回调 {} -> {}", url, resp.status());
            }
            Err(e) => {
                error!("CronManager: HTTP 回调失败 {}: {}", url, e);
            }
        }
    }
}
