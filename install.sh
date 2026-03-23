#!/usr/bin/env bash
#
# Nezha Dashboard (Rust) — 一键安装/更新/卸载脚本
# 用法: curl -fsSL https://raw.githubusercontent.com/Shannon-x/nezha-rust/main/install.sh | bash -s -- install
#       或: bash install.sh install|update|uninstall|status
#

set -euo pipefail

# ── 配置 ──
REPO="Shannon-x/nezha-rust"
INSTALL_DIR="/opt/nezha"
BIN_NAME="nezha-dashboard"
BIN_PATH="/usr/local/bin/${BIN_NAME}"
SERVICE_NAME="nezha"
DATA_DIR="${INSTALL_DIR}/data"
CONFIG_FILE="${DATA_DIR}/config.yaml"
GITHUB_API="https://api.github.com/repos/${REPO}/releases/latest"
GITHUB_DL="https://github.com/${REPO}/releases/download"

# ── 颜色 ──
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# ── 辅助函数 ──
info()    { echo -e "${GREEN}[INFO]${NC} $*"; }
warn()    { echo -e "${YELLOW}[WARN]${NC} $*"; }
error()   { echo -e "${RED}[ERROR]${NC} $*" >&2; }
header()  { echo -e "\n${CYAN}═══════════════════════════════════════${NC}"; echo -e "${CYAN}  $*${NC}"; echo -e "${CYAN}═══════════════════════════════════════${NC}\n"; }

check_root() {
    if [[ $EUID -ne 0 ]]; then
        error "请使用 root 权限运行此脚本"
        error "sudo bash $0 $*"
        exit 1
    fi
}

detect_arch() {
    local arch
    arch=$(uname -m)
    case "$arch" in
        x86_64|amd64)   echo "amd64" ;;
        aarch64|arm64)   echo "arm64" ;;
        *)               error "不支持的架构: $arch"; exit 1 ;;
    esac
}

detect_libc() {
    # 检测是否为 musl（Alpine/OpenWrt）
    if ldd --version 2>&1 | grep -qi musl; then
        echo "musl"
    else
        echo "gnu"
    fi
}

get_latest_version() {
    local version
    version=$(curl -fsSL "$GITHUB_API" 2>/dev/null | grep '"tag_name"' | head -1 | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')
    if [[ -z "$version" ]]; then
        error "无法获取最新版本号，请检查网络连接"
        exit 1
    fi
    echo "$version"
}

get_current_version() {
    if [[ -x "$BIN_PATH" ]]; then
        "$BIN_PATH" -v 2>/dev/null | awk '{print "v"$NF}' || echo "unknown"
    else
        echo "未安装"
    fi
}

download_binary() {
    local version="$1"
    local arch
    arch=$(detect_arch)
    local libc
    libc=$(detect_libc)

    local artifact
    if [[ "$libc" == "musl" && "$arch" == "amd64" ]]; then
        artifact="${BIN_NAME}-linux-amd64-musl"
    else
        artifact="${BIN_NAME}-linux-${arch}"
    fi

    local url="${GITHUB_DL}/${version}/${artifact}"
    info "下载 ${artifact} (${version})..."
    info "URL: ${url}"

    local tmp_file
    tmp_file=$(mktemp)
    if ! curl -fSL --progress-bar -o "$tmp_file" "$url"; then
        rm -f "$tmp_file"
        error "下载失败！请检查版本号和网络连接"
        error "URL: ${url}"
        exit 1
    fi

    chmod +x "$tmp_file"
    mv "$tmp_file" "$BIN_PATH"
    info "二进制文件已安装到 ${BIN_PATH}"
}

create_default_config() {
    if [[ -f "$CONFIG_FILE" ]]; then
        info "配置文件已存在: ${CONFIG_FILE}"
        return
    fi

    local secret
    secret=$(head -c 32 /dev/urandom | base64 | tr -d '=/+' | head -c 32)
    local agent_key
    agent_key=$(head -c 16 /dev/urandom | base64 | tr -d '=/+' | head -c 16)

    cat > "$CONFIG_FILE" <<EOF
# Nezha Dashboard 配置文件
language: "zh_CN"
listen_port: 8008
grpc_port: 5555
site_name: "Nezha"

database:
  type: "sqlite"
  path: "${DATA_DIR}/sqlite.db"

tsdb:
  type: "sqlite"
  data_path: "${DATA_DIR}/tsdb"
  retention_days: 30

jwt_secret_key: "${secret}"
agent_secret_key: "${agent_key}"
EOF
    info "默认配置已生成: ${CONFIG_FILE}"
    warn "请修改 jwt_secret_key 和 agent_secret_key！"
}

create_systemd_service() {
    cat > "/etc/systemd/system/${SERVICE_NAME}.service" <<EOF
[Unit]
Description=Nezha Dashboard (Rust)
After=network.target
Wants=network-online.target

[Service]
Type=simple
User=root
WorkingDirectory=${INSTALL_DIR}
ExecStart=${BIN_PATH} -c ${CONFIG_FILE}
Restart=always
RestartSec=5
LimitNOFILE=65535

# 内存限制（小内存服务器可改为 64M）
MemoryMax=256M

# 安全加固
NoNewPrivileges=true
ProtectSystem=strict
ReadWritePaths=${INSTALL_DIR}
PrivateTmp=true

[Install]
WantedBy=multi-user.target
EOF

    systemctl daemon-reload
    info "systemd 服务已创建: ${SERVICE_NAME}"
}

# ── 安装 ──
do_install() {
    header "安装 Nezha Dashboard (Rust)"

    check_root

    # 检查依赖
    for cmd in curl; do
        if ! command -v "$cmd" &>/dev/null; then
            error "缺少依赖: $cmd"
            exit 1
        fi
    done

    # 检查是否已安装
    if [[ -x "$BIN_PATH" ]]; then
        warn "Nezha Dashboard 已安装"
        local current
        current=$(get_current_version)
        info "当前版本: ${current}"
        read -rp "是否覆盖安装？(y/N): " confirm
        if [[ "${confirm,,}" != "y" ]]; then
            info "取消安装"
            exit 0
        fi
    fi

    # 获取最新版本
    local version
    version=$(get_latest_version)
    info "最新版本: ${version}"

    # 创建目录
    mkdir -p "$DATA_DIR"
    info "数据目录: ${DATA_DIR}"

    # 下载并安装
    download_binary "$version"

    # 创建默认配置
    create_default_config

    # 创建 systemd 服务
    create_systemd_service

    # 启动服务
    systemctl enable "$SERVICE_NAME"
    systemctl start "$SERVICE_NAME"

    header "安装完成！"
    info "版本:    ${version}"
    info "二进制:  ${BIN_PATH}"
    info "配置:    ${CONFIG_FILE}"
    info "数据:    ${DATA_DIR}"
    info "服务:    systemctl status ${SERVICE_NAME}"
    echo ""
    info "面板地址: http://$(hostname -I | awk '{print $1}'):8008"
    info "默认账号: admin / admin"
    echo ""
    warn "⚠️  请立即修改默认密码和 config.yaml 中的密钥！"
}

# ── 更新 ──
do_update() {
    header "更新 Nezha Dashboard"

    check_root

    if [[ ! -x "$BIN_PATH" ]]; then
        error "Nezha Dashboard 未安装，请先运行 install"
        exit 1
    fi

    local current
    current=$(get_current_version)
    local latest
    latest=$(get_latest_version)

    info "当前版本: ${current}"
    info "最新版本: ${latest}"

    if [[ "$current" == "$latest" ]]; then
        info "已是最新版本，无需更新"
        exit 0
    fi

    # 停止服务
    info "停止服务..."
    systemctl stop "$SERVICE_NAME" 2>/dev/null || true

    # 备份旧版本
    if [[ -f "$BIN_PATH" ]]; then
        cp "$BIN_PATH" "${BIN_PATH}.bak"
        info "旧版本已备份: ${BIN_PATH}.bak"
    fi

    # 下载新版本
    download_binary "$latest"

    # 重启服务
    systemctl start "$SERVICE_NAME"

    header "更新完成！"
    info "版本: ${current} → ${latest}"
    info "如有问题，可回滚: cp ${BIN_PATH}.bak ${BIN_PATH} && systemctl restart ${SERVICE_NAME}"
}

# ── 卸载 ──
do_uninstall() {
    header "卸载 Nezha Dashboard"

    check_root

    read -rp "确定要卸载 Nezha Dashboard？配置和数据将保留。(y/N): " confirm
    if [[ "${confirm,,}" != "y" ]]; then
        info "取消卸载"
        exit 0
    fi

    # 停止并禁用服务
    info "停止服务..."
    systemctl stop "$SERVICE_NAME" 2>/dev/null || true
    systemctl disable "$SERVICE_NAME" 2>/dev/null || true
    rm -f "/etc/systemd/system/${SERVICE_NAME}.service"
    systemctl daemon-reload
    info "服务已移除"

    # 删除二进制
    rm -f "$BIN_PATH" "${BIN_PATH}.bak"
    info "二进制已删除"

    # 提示数据保留
    warn "数据和配置保留在: ${INSTALL_DIR}"
    warn "如需完全清除，请运行: rm -rf ${INSTALL_DIR}"

    header "卸载完成！"
}

# ── 状态 ──
do_status() {
    header "Nezha Dashboard 状态"

    if [[ -x "$BIN_PATH" ]]; then
        local version
        version=$(get_current_version)
        info "已安装:  ${GREEN}是${NC}"
        info "版本:    ${version}"
        info "二进制:  ${BIN_PATH}"
        info "大小:    $(du -h "$BIN_PATH" | awk '{print $1}')"
    else
        warn "未安装"
        return
    fi

    if [[ -f "$CONFIG_FILE" ]]; then
        info "配置:    ${CONFIG_FILE}"
    fi

    if [[ -d "$DATA_DIR" ]]; then
        info "数据:    ${DATA_DIR} ($(du -sh "$DATA_DIR" | awk '{print $1}'))"
    fi

    echo ""
    if systemctl is-active "$SERVICE_NAME" &>/dev/null; then
        info "服务状态: ${GREEN}运行中${NC}"
        systemctl status "$SERVICE_NAME" --no-pager -l 2>/dev/null | head -10
    else
        warn "服务状态: 未运行"
    fi

    # 显示内存占用
    local pid
    pid=$(pgrep -f "$BIN_NAME" 2>/dev/null || true)
    if [[ -n "$pid" ]]; then
        local rss
        rss=$(ps -o rss= -p "$pid" 2>/dev/null | awk '{printf "%.1f MB", $1/1024}')
        info "内存占用: ${rss}"
    fi
}

# ── 主入口 ──
show_menu() {
    header "Nezha Dashboard (Rust) 管理脚本"
    echo "  1) 安装 (install)"
    echo "  2) 更新 (update)"
    echo "  3) 卸载 (uninstall)"
    echo "  4) 状态 (status)"
    echo "  0) 退出"
    echo ""
    read -rp "请选择 [0-4]: " choice
    case "$choice" in
        1) do_install ;;
        2) do_update ;;
        3) do_uninstall ;;
        4) do_status ;;
        0) exit 0 ;;
        *) error "无效选择"; show_menu ;;
    esac
}

# 命令行参数 or 交互菜单
case "${1:-}" in
    install)    do_install ;;
    update)     do_update ;;
    uninstall)  do_uninstall ;;
    status)     do_status ;;
    "")         show_menu ;;
    *)
        echo "用法: $0 {install|update|uninstall|status}"
        echo ""
        echo "  install    安装 Nezha Dashboard"
        echo "  update     更新到最新版本"
        echo "  uninstall  卸载（保留数据）"
        echo "  status     查看运行状态"
        exit 1
        ;;
esac
