use nezha_proto::nezha_service_server::NezhaService;
use nezha_proto::*;
use nezha_service::AppState;
use std::sync::Arc;
use tonic::{Request, Response, Status, Streaming};
use tracing::{info, warn};

/// gRPC 请求处理器
pub struct NezhaHandler {
    pub state: Arc<AppState>,
}

impl NezhaHandler {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl NezhaService for NezhaHandler {
    type ReportSystemStateStream =
        tokio_stream::wrappers::ReceiverStream<Result<Receipt, Status>>;

    async fn report_system_state(
        &self,
        request: Request<Streaming<State>>,
    ) -> Result<Response<Self::ReportSystemStateStream>, Status> {
        let (tx, rx) = tokio::sync::mpsc::channel(128);
        let state = self.state.clone();

        // 验证 Agent 身份
        let client_id = crate::auth::check_auth(request.metadata(), &state).await?;

        let mut stream = request.into_inner();
        tokio::spawn(async move {
            while let Ok(Some(pb_state)) = stream.message().await {
                let inner_state =
                    nezha_core::models::host::HostState::from_pb(&pb_state);

                if let Some(mut server) = state.servers.get_mut(&client_id) {
                    server.last_active = Some(chrono::Utc::now().naive_utc());
                    server.state = Some(inner_state);

                    // TSDB 写入（节流：每分钟每服务器一条）
                    if state.tsdb_enabled() {
                        let should_write = server
                            .last_tsdb_write
                            .map(|t| {
                                chrono::Utc::now().naive_utc() - t
                                    >= chrono::Duration::minutes(1)
                            })
                            .unwrap_or(true);

                        if should_write {
                            server.last_tsdb_write =
                                Some(chrono::Utc::now().naive_utc());
                            let s = server.state.as_ref().unwrap();
                            if let Some(ref tsdb) = state.tsdb {
                                let _ = tsdb
                                    .write_server_metrics(
                                        &nezha_tsdb::ServerMetrics {
                                            server_id: client_id,
                                            timestamp: chrono::Utc::now()
                                                .naive_utc(),
                                            cpu: s.cpu,
                                            mem_used: s.mem_used,
                                            swap_used: s.swap_used,
                                            disk_used: s.disk_used,
                                            net_in_speed: s.net_in_speed,
                                            net_out_speed: s.net_out_speed,
                                            net_in_transfer: s.net_in_transfer,
                                            net_out_transfer: s.net_out_transfer,
                                            load1: s.load_1,
                                            load5: s.load_5,
                                            load15: s.load_15,
                                            tcp_conn_count: s.tcp_conn_count,
                                            udp_conn_count: s.udp_conn_count,
                                            process_count: s.process_count,
                                            temperature: s
                                                .temperatures
                                                .iter()
                                                .map(|t| t.temperature)
                                                .fold(0.0f64, f64::max),
                                            uptime: s.uptime,
                                            gpu: s
                                                .gpu
                                                .iter()
                                                .cloned()
                                                .fold(0.0f64, f64::max),
                                        },
                                    )
                                    .await;
                            }
                        }
                    }

                    // 初始化流量快照
                    if server.prev_transfer_in_snapshot == 0
                        || server.prev_transfer_out_snapshot == 0
                    {
                        server.prev_transfer_in_snapshot =
                            pb_state.net_in_transfer;
                        server.prev_transfer_out_snapshot =
                            pb_state.net_out_transfer;
                    }
                }

                let _ = tx.send(Ok(Receipt { proced: true })).await;
            }
        });

        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            rx,
        )))
    }

    async fn report_system_info(
        &self,
        request: Request<Host>,
    ) -> Result<Response<Receipt>, Status> {
        let client_id = crate::auth::check_auth(request.metadata(), &self.state).await?;
        let host = nezha_core::models::host::Host::from_pb(request.get_ref());

        if let Some(mut server) = self.state.servers.get_mut(&client_id) {
            server.host = Some(host);
        }

        Ok(Response::new(Receipt { proced: true }))
    }

    async fn report_system_info2(
        &self,
        request: Request<Host>,
    ) -> Result<Response<Uint64Receipt>, Status> {
        let client_id = crate::auth::check_auth(request.metadata(), &self.state).await?;
        let host = nezha_core::models::host::Host::from_pb(request.get_ref());

        if let Some(mut server) = self.state.servers.get_mut(&client_id) {
            server.host = Some(host);
        }

        Ok(Response::new(Uint64Receipt {
            data: self.state.boot_time,
        }))
    }

    type RequestTaskStream =
        tokio_stream::wrappers::ReceiverStream<Result<Task, Status>>;

    async fn request_task(
        &self,
        request: Request<Streaming<TaskResult>>,
    ) -> Result<Response<Self::RequestTaskStream>, Status> {
        let client_id = crate::auth::check_auth(request.metadata(), &self.state).await?;
        let (tx, rx) = tokio::sync::mpsc::channel(128);
        let mut stream = request.into_inner();

        // 保存发送端，保持通道存活
        self.state.task_senders.insert(client_id, tx);
        let state = self.state.clone();

        tokio::spawn(async move {
            while let Ok(Some(result)) = stream.message().await {
                info!(
                    "Task result from client {}: type={}, success={}",
                    client_id, result.r#type, result.successful
                );
            }
            // Agent 断连，清理通道
            state.task_senders.remove(&client_id);
            info!("Agent {} task stream closed, sender removed", client_id);
        });

        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            rx,
        )))
    }

    type IOStreamStream =
        tokio_stream::wrappers::ReceiverStream<Result<IoStreamData, Status>>;

    async fn io_stream(
        &self,
        request: Request<Streaming<IoStreamData>>,
    ) -> Result<Response<Self::IOStreamStream>, Status> {
        let _client_id = crate::auth::check_auth(request.metadata(), &self.state).await?;
        let mut stream = request.into_inner();

        let first_msg = match stream.message().await {
            Ok(Some(msg)) => msg,
            _ => return Err(Status::invalid_argument("Failed to read first IOStream data")),
        };

        if first_msg.data.len() < 4 || first_msg.data[0..4] != [0xff, 0x05, 0xff, 0x05] {
            return Err(Status::invalid_argument("Invalid stream magic bytes"));
        }

        let stream_id = String::from_utf8_lossy(&first_msg.data[4..]).to_string();

        let active_stream = match self.state.active_streams.remove(&stream_id) {
            Some(s) => s.1,
            None => return Err(Status::not_found("StreamID not found or already bound")),
        };

        let (tx, rx) = tokio::sync::mpsc::channel(128);

        let mut rx_from_ws = active_stream.rx_from_ws;
        tokio::spawn(async move {
            while let Some(data) = rx_from_ws.recv().await {
                if tx.send(Ok(IoStreamData { data })).await.is_err() {
                    break;
                }
            }
        });

        let tx_to_ws = active_stream.tx_to_ws;
        tokio::spawn(async move {
            while let Ok(Some(msg)) = stream.message().await {
                if msg.data.is_empty() {
                    continue; // 忽略 keep-alive 心跳包
                }
                if tx_to_ws.send(msg.data).await.is_err() {
                    break;
                }
            }
        });

        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    async fn report_geo_ip(
        &self,
        request: Request<GeoIp>,
    ) -> Result<Response<GeoIp>, Status> {
        let client_id = crate::auth::check_auth(request.metadata(), &self.state).await?;
        let pb_geoip = request.into_inner();
        let geoip = nezha_core::models::host::geoip_from_pb(&pb_geoip);

        // 查找国家代码
        let mut country_code = String::new();
        let ip_str = if !geoip.ip.ipv6_addr.is_empty()
            && (pb_geoip.use6 || geoip.ip.ipv4_addr.is_empty())
        {
            &geoip.ip.ipv6_addr
        } else {
            &geoip.ip.ipv4_addr
        };

        if let Ok(ip) = ip_str.parse::<std::net::IpAddr>() {
            if let Some(code) = nezha_utils::geoip::lookup(ip) {
                country_code = code;
            }
        }

        if let Some(mut server) = self.state.servers.get_mut(&client_id) {
            let mut geo = geoip;
            geo.country_code = country_code.clone();

            // 检查并更新DDNS
            if server.enable_ddns {
                let ip_changed = server.geoip.as_ref().map(|g| &g.ip) != Some(&geo.ip);
                if ip_changed {
                    nezha_service::ddns::DdnsManager::update(
                        self.state.clone(),
                        &server.clone(),
                        &geo
                    ).await;
                }
            }

            server.geoip = Some(geo);
        }

        Ok(Response::new(GeoIp {
            use6: false,
            ip: None,
            country_code,
            dashboard_boot_time: self.state.boot_time,
        }))
    }
}
