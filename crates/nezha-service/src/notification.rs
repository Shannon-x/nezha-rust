use crate::state::AppState;
use std::sync::Arc;
use std::time::Duration;
use once_cell::sync::Lazy;
use tokio::sync::Semaphore;
use tracing::{info, warn, error};

// ────────────────────────────────────────────
// 高性能通知系统
//
// 性能设计：
// 1. 全局 reqwest::Client 单例（连接池复用）
// 2. Semaphore 限制并发通知数（防止对外请求风暴）
// 3. 指数退避重试（最多 3 次）
// 4. 字符串模板替换使用 Cow 避免不必要的分配
// 5. query_as 结果直接映射，零中间层
// ────────────────────────────────────────────

/// 全局 HTTP 客户端（进程生命周期复用）
static NOTIFY_CLIENT: Lazy<reqwest::Client> = Lazy::new(|| {
    reqwest::Client::builder()
        .pool_max_idle_per_host(10)
        .pool_idle_timeout(Duration::from_secs(90))
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .tcp_keepalive(Duration::from_secs(30))
        .build()
        .unwrap_or_default()
});

/// 全局 HTTP 客户端（跳过 TLS 验证）
static NOTIFY_CLIENT_INSECURE: Lazy<reqwest::Client> = Lazy::new(|| {
    reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .pool_max_idle_per_host(5)
        .pool_idle_timeout(Duration::from_secs(60))
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .build()
        .unwrap_or_default()
});

/// 并发限制（全局共享）
static NOTIFY_SEMAPHORE: Lazy<Arc<Semaphore>> = Lazy::new(|| Arc::new(Semaphore::new(16)));

/// 发送通知到指定通知组（高性能版本）
pub async fn send_notification(
    state: &AppState,
    notification_group_id: i64,
    title: &str,
    message: &str,
) {
    // 查询通知组 → 通知 ID 列表
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT COALESCE(notifications,'[]') FROM notification_groups WHERE id = ?"
    )
    .bind(notification_group_id)
    .fetch_optional(&state.db.pool).await.ok().flatten();

    let notification_ids: Vec<i64> = match row {
        Some((json,)) => serde_json::from_str(&json).unwrap_or_default(),
        None => return,
    };

    if notification_ids.is_empty() { return; }

    // 批量查询所有通知配置（单次 DB 查询）
    let placeholders: String = notification_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let query = format!(
        "SELECT id, tag, url, request_method, request_type, COALESCE(request_header,''), COALESCE(request_body,''), CAST(verify_tls AS INTEGER) FROM notifications WHERE id IN ({})",
        placeholders
    );

    let mut q = sqlx::query_as::<_, (i64, String, String, i32, i32, String, String, i32)>(&query);
    for id in &notification_ids {
        q = q.bind(id);
    }

    let configs: Vec<(i64, String, String, i32, i32, String, String, i32)> = 
        q.fetch_all(&state.db.pool).await.unwrap_or_default();

    // 并发发送所有通知
    let title = title.to_string();
    let message = message.to_string();

    for (_id, tag, url, method, _req_type, headers_str, body_tpl, verify_tls_i) in configs {
        let verify_tls = verify_tls_i != 0;
        let title = title.clone();
        let message = message.clone();
        let sem = NOTIFY_SEMAPHORE.clone();

        tokio::spawn(async move {
            let _permit = sem.acquire().await;

            let result = send_webhook_with_retry(
                &url, method, &headers_str, &body_tpl, &title, &message, verify_tls,
            ).await;

            match result {
                Ok(()) => info!("通知已发送: {} → {}", tag, url),
                Err(e) => error!("通知失败: {} → {} : {}", tag, url, e),
            }
        });
    }
}

/// 带重试的 Webhook 发送（指数退避，最多 3 次）
async fn send_webhook_with_retry(
    url: &str,
    method: i32,
    headers_str: &str,
    body_tpl: &str,
    title: &str,
    message: &str,
    verify_tls: bool,
) -> anyhow::Result<()> {
    let mut last_err = anyhow::anyhow!("unknown");
    let delays = [0u64, 1000, 3000]; // 退避：0, 1s, 3s

    for (attempt, delay_ms) in delays.iter().enumerate() {
        if *delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(*delay_ms)).await;
        }

        match send_webhook(url, method, headers_str, body_tpl, title, message, verify_tls).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_err = e;
                if attempt < 2 {
                    warn!("通知重试 {}/3: {}", attempt + 1, url);
                }
            }
        }
    }

    Err(last_err)
}

/// 发送单次 Webhook
#[inline]
async fn send_webhook(
    url: &str,
    method: i32,
    headers_str: &str,
    body_tpl: &str,
    title: &str,
    message: &str,
    verify_tls: bool,
) -> anyhow::Result<()> {
    // 模板替换（仅在有占位符时才分配）
    let url = if url.contains("#TITLE#") || url.contains("#MSG#") {
        std::borrow::Cow::Owned(url.replace("#TITLE#", title).replace("#MSG#", message))
    } else {
        std::borrow::Cow::Borrowed(url)
    };

    let body = if body_tpl.contains("#TITLE#") || body_tpl.contains("#MSG#") {
        body_tpl.replace("#TITLE#", title).replace("#MSG#", message)
    } else {
        body_tpl.to_string()
    };

    let client = if verify_tls { &*NOTIFY_CLIENT } else { &*NOTIFY_CLIENT_INSECURE };

    let mut request = match method {
        1 => client.get(url.as_ref()),
        2 => {
            let json_body = if body.is_empty() {
                serde_json::json!({"title": title, "message": message})
            } else {
                serde_json::from_str(&body).unwrap_or(serde_json::json!({"text": message}))
            };
            client.post(url.as_ref()).json(&json_body)
        }
        3 => {
            client.post(url.as_ref())
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body(body)
        }
        _ => {
            client.post(url.as_ref()).json(&serde_json::json!({"title": title, "message": message}))
        }
    };

    // 解析自定义 Headers（仅非空时开销）
    if !headers_str.is_empty() {
        if let Ok(headers) = serde_json::from_str::<std::collections::HashMap<String, String>>(headers_str) {
            for (k, v) in headers {
                request = request.header(&k, &v);
            }
        }
    }

    let resp = request.send().await?;
    if resp.status().is_success() || resp.status().is_redirection() {
        Ok(())
    } else {
        anyhow::bail!("HTTP {}", resp.status())
    }
}
