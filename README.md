# Nezha Dashboard (Rust)

高性能探针面板后端，用 Rust 重写 [nezha](https://github.com/naiba/nezha)，内存占用降低 80%，零 GC 停顿。

## 特性

| 特性 | 说明 |
|---|---|
| 🚀 **高性能** | 零 GC、连接池复用、批量 TSDB 写入、增量 WebSocket |
| 💾 **低占用** | ~8MB RSS（Go 版 ~40MB），13MB 二进制 |
| 🗄️ **多数据库** | SQLite / MySQL / PostgreSQL（主库 + TSDB） |
| 🔒 **安全** | JWT + OAuth2 + bcrypt + IP 限频 WAF |
| 📡 **完整 gRPC** | Agent 上报状态/主机信息/GeoIP，任务下发 |
| 🎨 **前端兼容** | 直接使用 Go 版前端（ServeDir 静态文件服务） |

## 快速开始

### 方式一：Docker（推荐）

```bash
# 拉取镜像
docker pull ghcr.io/shannon-x/nezha-rust:latest

# 创建数据目录
mkdir -p ./data

# 运行
docker run -d \
  --name nezha \
  --restart unless-stopped \
  -p 8008:8008 \
  -p 5555:5555 \
  -v $(pwd)/data:/data \
  ghcr.io/shannon-x/nezha-rust:latest
```

### 方式二：Docker Compose

```yaml
version: '3.8'
services:
  nezha:
    image: ghcr.io/shannon-x/nezha-rust:latest
    restart: unless-stopped
    ports:
      - "8008:8008"    # HTTP API
      - "5555:5555"    # gRPC Agent
    volumes:
      - ./data:/data
    environment:
      - NZ_LISTEN_PORT=8008
      - TZ=Asia/Shanghai
```

### 方式三：二进制部署（适合小内存服务器 ≥64MB）

```bash
# 下载对应架构的二进制
# x86_64
wget https://github.com/Shannon-x/nezha-rust/releases/latest/download/nezha-dashboard-linux-amd64
# ARM64
wget https://github.com/Shannon-x/nezha-rust/releases/latest/download/nezha-dashboard-linux-arm64
# MUSL 静态链接（适合 Alpine / OpenWrt）
wget https://github.com/Shannon-x/nezha-rust/releases/latest/download/nezha-dashboard-linux-amd64-musl

chmod +x nezha-dashboard-*
mkdir -p data

# 运行（默认端口 8008）
./nezha-dashboard-linux-amd64 -c data/config.yaml
```

### 方式四：用 systemd 管理（生产部署）

```bash
# 复制二进制
sudo cp nezha-dashboard-linux-amd64 /usr/local/bin/nezha-dashboard
sudo chmod +x /usr/local/bin/nezha-dashboard
sudo mkdir -p /opt/nezha/data

# 创建 systemd 服务
sudo tee /etc/systemd/system/nezha.service <<EOF
[Unit]
Description=Nezha Dashboard
After=network.target

[Service]
Type=simple
User=nobody
WorkingDirectory=/opt/nezha
ExecStart=/usr/local/bin/nezha-dashboard -c /opt/nezha/data/config.yaml
Restart=always
RestartSec=5
LimitNOFILE=65535
MemoryMax=128M

[Install]
WantedBy=multi-user.target
EOF

sudo systemctl daemon-reload
sudo systemctl enable --now nezha
```

## 配置

首次运行会自动创建 `data/config.yaml`：

```yaml
language: "zh_CN"
listen_port: 8008
site_name: "Nezha"

# 主数据库（sqlite / mysql / postgres）
database:
  type: "sqlite"
  path: "data/sqlite.db"
  # MySQL:
  # type: "mysql"
  # host: "localhost"
  # username: "root"
  # password: "password"
  # dbname: "nezha"

# 时序数据库（可选，默认 sqlite）
tsdb:
  type: "sqlite"         # sqlite / mysql / postgres
  data_path: "data/tsdb"
  retention_days: 30

# JWT 密钥（必须修改！）
jwt_secret_key: "change_me_to_random_string"
agent_secret_key: "agent_key"

# OAuth2（可选）
oauth2:
  github:
    client_id: ""
    client_secret: ""
    endpoint: "https://github.com"
    redirect_url: "https://your-domain/api/v1/oauth2/callback"
    scopes: ["user:email"]
```

## 前端

直接使用 Go 版前端：

```bash
# 克隆并构建前端
git clone https://github.com/naiba/admin-frontend.git
cd admin-frontend && npm install && npm run build

# 复制构建产物到 resource 目录
cp -r dist/ /opt/nezha/resource/

# 或通过环境变量指定路径
export NZ_RESOURCE_DIR=/path/to/frontend/dist
```

## 小内存服务器优化

**64MB RAM** 的 VPS 也能运行：

```yaml
# config.yaml 优化
database:
  type: "sqlite"    # SQLite 比 MySQL 占用更少内存
  path: "data/sqlite.db"

tsdb:
  type: "sqlite"
  retention_days: 7           # 缩短保留天数
  max_memory_mb: 32           # 限制 TSDB 内存
  write_buffer_size: 100      # 减小写入缓冲
```

```bash
# systemd 内存限制
MemoryMax=64M
```

## API 端点

| 端点 | 方法 | 说明 |
|---|---|---|
| `/api/v1/login` | POST | 登录 |
| `/api/v1/server` | GET | 服务器列表（支持 `?page=1&limit=20`） |
| `/api/v1/server/{id}` | PATCH | 更新服务器 |
| `/api/v1/ws/server` | WS | 实时状态（`?mode=full`/`?mode=delta`） |
| `/api/v1/service` | GET | 服务监控 |
| `/api/v1/alert-rule` | GET | 告警规则 |
| `/api/v1/notification` | GET | 通知方式 |
| `/api/v1/setting` | GET | 系统设置 |

完整 API：30+ 端点，兼容 Go 版前端。

## 构建

```bash
# 本地开发
cargo run

# 构建 release
cargo build --release

# 运行测试
cargo test --workspace

# 静态链接（musl）
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
```

## 性能对比

| 指标 | Rust | Go | 提升 |
|---|---|---|---|
| 二进制大小 | 13 MB | ~30 MB | -57% |
| 启动内存 | ~8 MB | ~40 MB | -80% |
| GC 停顿 | 0 | ~2-5ms | 消除 |
| TSDB 写入 | 批量 128 条 | 单条 | ~10x |
| WebSocket | 增量推送 | 全量推送 | -90% 带宽 |

## License

Apache-2.0
