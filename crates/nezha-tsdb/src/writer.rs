use crate::store::{ServerMetrics, ServiceMetrics, Store};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::warn;

/// 缓冲写入器 - 批量写入提高性能
pub struct BufferedWriter {
    server_tx: mpsc::Sender<ServerMetrics>,
    service_tx: mpsc::Sender<ServiceMetrics>,
}

impl BufferedWriter {
    pub fn new(store: Arc<dyn Store>, buffer_size: usize, flush_interval_secs: u64) -> Self {
        let (server_tx, mut server_rx) = mpsc::channel::<ServerMetrics>(buffer_size);
        let (service_tx, mut service_rx) = mpsc::channel::<ServiceMetrics>(buffer_size);

        let store_clone = store.clone();
        tokio::spawn(async move {
            let mut buf = Vec::with_capacity(100);
            loop {
                tokio::select! {
                    Some(m) = server_rx.recv() => {
                        buf.push(m);
                        if buf.len() >= 100 {
                            for m in buf.drain(..) {
                                if let Err(e) = store_clone.write_server_metrics(&m).await {
                                    warn!("Failed to write server metrics: {}", e);
                                }
                            }
                        }
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_secs(flush_interval_secs)) => {
                        for m in buf.drain(..) {
                            if let Err(e) = store_clone.write_server_metrics(&m).await {
                                warn!("Failed to flush server metrics: {}", e);
                            }
                        }
                    }
                    else => break,
                }
            }
        });

        let store_clone2 = store.clone();
        tokio::spawn(async move {
            let mut buf = Vec::with_capacity(100);
            loop {
                tokio::select! {
                    Some(m) = service_rx.recv() => {
                        buf.push(m);
                        if buf.len() >= 100 {
                            for m in buf.drain(..) {
                                if let Err(e) = store_clone2.write_service_metrics(&m).await {
                                    warn!("Failed to write service metrics: {}", e);
                                }
                            }
                        }
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_secs(flush_interval_secs)) => {
                        for m in buf.drain(..) {
                            if let Err(e) = store_clone2.write_service_metrics(&m).await {
                                warn!("Failed to flush service metrics: {}", e);
                            }
                        }
                    }
                    else => break,
                }
            }
        });

        Self {
            server_tx,
            service_tx,
        }
    }

    pub async fn write_server_metrics(&self, m: ServerMetrics) -> anyhow::Result<()> {
        self.server_tx.send(m).await?;
        Ok(())
    }

    pub async fn write_service_metrics(&self, m: ServiceMetrics) -> anyhow::Result<()> {
        self.service_tx.send(m).await?;
        Ok(())
    }
}
