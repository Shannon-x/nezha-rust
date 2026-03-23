#!/usr/bin/env bash
#
# Nezha Dashboard (Rust) — 一键安装/更新/卸载脚本
#
# 用法:
#   curl -fsSL https://raw.githubusercontent.com/Shannon-x/nezha-rust/main/install.sh | bash -s -- install
#   或: bash install.sh install|update|uninstall|status
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
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
BLUE='\033[0;34m'; CYAN='\033[0;36m'; NC='\033[0m'

info()    { echo -e "${GREEN}[INFO]${NC} $*"; }
warn()    { echo -e "${YELLOW}[WARN]${NC} $*"; }
error()   { echo -e "${RED}[ERROR]${NC} $*" >&2; }
header()  { echo -e "\n${CYAN}═══════════════════════════════════════${NC}"; echo -e "${CYAN}  $*${NC}"; echo -e "${CYAN}═══════════════════════════════════════${NC}\n"; }

check_root() {
    if [[ $EUID -ne 0 ]]; then
        error "请使用 root 权限运行: sudo bash $0 ${1:-}"
        exit 1
    fi
}

# ── 平台检测 ──
detect_os() {
    local os
    os=$(uname -s | tr '[:upper:]' '[:lower:]')
    case "$os" in
        linux)   echo "linux" ;;
        darwin)  echo "darwin" ;;
        freebsd) echo "freebsd" ;;
        *)       error "不支持的操作系统: $os"; exit 1 ;;
    esac
}

detect_arch() {
    local arch
    arch=$(uname -m)
    case "$arch" in
        x86_64|amd64)           echo "amd64" ;;
        aarch64|arm64)          echo "arm64" ;;
        armv7*|armhf)           echo "armv7" ;;
        *)                      error "不支持的架构: $arch"; exit 1 ;;
    esac
}

detect_libc() {
    if [[ "$(detect_os)" != "linux" ]]; then
        echo "native"
        return
    fi
    if ldd --version 2>&1 | grep -qi musl 2>/dev/null; then
        echo "musl"
    elif [ -f /etc/alpine-release ]; then
        echo "musl"
    else
        echo "gnu"
    fi
}

# 构建二进制名称
get_artifact_name() {
    local os arch libc
    os=$(detect_os)
    arch=$(detect_arch)
    libc=$(detect_libc)

    local artifact="${BIN_NAME}-${os}-${arch}"
    if [[ "$libc" == "musl" ]]; then
        artifact="${artifact}-musl"
    fi
    echo "$artifact"
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
        "$BIN_PATH" --version 2>/dev/null | head -1 || echo "unknown"
    else
        echo "未安装"
    fi
}

download_binary() {
    local version="$1"
    local artifact
    artifact=$(get_artifact_name)
    local url="${GITHUB_DL}/${version}/${artifact}.tar.gz"

    info "平台:  $(detect_os)/$(detect_arch) ($(detect_libc))"
    info "文件:  ${artifact}.tar.gz"
    info "版本:  ${version}"
    info "URL:   ${url}"

    local tmp_file tmp_dir
    tmp_file=$(mktemp)
    tmp_dir=$(mktemp -d)

    if ! curl -fSL --progress-bar -o "$tmp_file" "$url"; then
        rm -f "$tmp_file" && rm -rf "$tmp_dir"
        # 尝试不带 .tar.gz 的裸二进制（老版本兼容）
        url="${GITHUB_DL}/${version}/${artifact}"
        info "尝试裸二进制: ${url}"
        if ! curl -fSL --progress-bar -o "$tmp_file" "$url"; then
            rm -f "$tmp_file" && rm -rf "$tmp_dir"
            error "下载失败！"
            error "可用下载: https://github.com/${REPO}/releases/latest"
            exit 1
        fi
        chmod +x "$tmp_file"
        mv -f "$tmp_file" "$BIN_PATH"
        info "已安装到 ${BIN_PATH}（无前端资源）"
        return
    fi

    # 解压 tar.gz
    tar xzf "$tmp_file" -C "$tmp_dir"
    rm -f "$tmp_file"

    # 安装二进制
    if [[ -f "$tmp_dir/nezha-dashboard" ]]; then
        chmod +x "$tmp_dir/nezha-dashboard"
        mv -f "$tmp_dir/nezha-dashboard" "$BIN_PATH"
        info "二进制已安装: ${BIN_PATH}"
    fi

    # 安装前端资源
    if [[ -d "$tmp_dir/resource" ]]; then
        mkdir -p "${INSTALL_DIR}/resource"
        cp -r "$tmp_dir/resource/"* "${INSTALL_DIR}/resource/" 2>/dev/null || true
        info "前端已安装: ${INSTALL_DIR}/resource/"
    fi

    rm -rf "$tmp_dir"
}

create_default_config() {
    if [[ -f "$CONFIG_FILE" ]]; then
        info "配置文件已存在: ${CONFIG_FILE}"
        return
    fi

    local secret agent_key
    secret=$(head -c 32 /dev/urandom | base64 | tr -d '=/+' | head -c 32)
    agent_key=$(head -c 16 /dev/urandom | base64 | tr -d '=/+' | head -c 16)

    mkdir -p "$(dirname "$CONFIG_FILE")"
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
    info "默认配置生成: ${CONFIG_FILE}"
    warn "⚠️  请修改 jwt_secret_key 和 agent_secret_key！"
}

create_systemd_service() {
    if [[ "$(detect_os)" != "linux" ]]; then
        warn "非 Linux 系统，跳过 systemd 服务创建"
        return
    fi

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
Environment=NZ_RESOURCE_DIR=${INSTALL_DIR}/resource
Restart=always
RestartSec=5
LimitNOFILE=65535
MemoryMax=256M
NoNewPrivileges=true
ProtectSystem=strict
ReadWritePaths=${INSTALL_DIR}
PrivateTmp=true

[Install]
WantedBy=multi-user.target
EOF
    systemctl daemon-reload
    info "systemd 服务已创建"
}

# ── 安装 ──
do_install() {
    header "安装 Nezha Dashboard (Rust)"
    check_root

    if ! command -v curl &>/dev/null; then
        error "缺少依赖: curl"; exit 1
    fi

    if [[ -x "$BIN_PATH" ]]; then
        warn "已安装: $(get_current_version)"
        read -rp "覆盖安装？(y/N): " confirm
        [[ "${confirm,,}" != "y" ]] && { info "取消"; exit 0; }
    fi

    local version
    version=$(get_latest_version)
    mkdir -p "$DATA_DIR"
    download_binary "$version"
    create_default_config
    create_systemd_service

    systemctl enable "$SERVICE_NAME" 2>/dev/null || true
    systemctl start "$SERVICE_NAME" 2>/dev/null || true

    header "✅ 安装完成"
    info "版本:    ${version}"
    info "二进制:  ${BIN_PATH} ($(du -h "$BIN_PATH" | awk '{print $1}'))"
    info "配置:    ${CONFIG_FILE}"
    info "数据:    ${DATA_DIR}"
    info "面板:    http://$(hostname -I 2>/dev/null | awk '{print $1}' || echo 'localhost'):8008"
    info "账号:    admin / admin"
    echo ""
    warn "⚠️  请立即修改默认密码和密钥！"
}

# ── 更新 ──
do_update() {
    header "更新 Nezha Dashboard"
    check_root

    if [[ ! -x "$BIN_PATH" ]]; then
        error "未安装，请先运行: $0 install"; exit 1
    fi

    local current latest
    current=$(get_current_version)
    latest=$(get_latest_version)
    info "当前: ${current}"
    info "最新: ${latest}"

    # 停止 → 备份 → 下载 → 启动
    systemctl stop "$SERVICE_NAME" 2>/dev/null || true
    cp -f "$BIN_PATH" "${BIN_PATH}.bak" 2>/dev/null || true
    download_binary "$latest"
    systemctl start "$SERVICE_NAME" 2>/dev/null || true

    header "✅ 更新完成"
    info "回滚命令: cp ${BIN_PATH}.bak ${BIN_PATH} && systemctl restart ${SERVICE_NAME}"
}

# ── 卸载 ──
do_uninstall() {
    header "卸载 Nezha Dashboard"
    check_root

    read -rp "确认卸载？数据将保留。(y/N): " confirm
    [[ "${confirm,,}" != "y" ]] && { info "取消"; exit 0; }

    systemctl stop "$SERVICE_NAME" 2>/dev/null || true
    systemctl disable "$SERVICE_NAME" 2>/dev/null || true
    rm -f "/etc/systemd/system/${SERVICE_NAME}.service"
    systemctl daemon-reload 2>/dev/null || true
    rm -f "$BIN_PATH" "${BIN_PATH}.bak"

    header "✅ 卸载完成"
    warn "数据保留在: ${INSTALL_DIR}"
    warn "完全清除: rm -rf ${INSTALL_DIR}"
}

# ── 状态 ──
do_status() {
    header "Nezha Dashboard 状态"

    if [[ ! -x "$BIN_PATH" ]]; then
        warn "未安装"; return
    fi

    info "平台:    $(detect_os)/$(detect_arch) ($(detect_libc))"
    info "版本:    $(get_current_version)"
    info "二进制:  ${BIN_PATH} ($(du -h "$BIN_PATH" | awk '{print $1}'))"
    [[ -f "$CONFIG_FILE" ]] && info "配置:    ${CONFIG_FILE}"
    [[ -d "$DATA_DIR" ]] && info "数据:    ${DATA_DIR} ($(du -sh "$DATA_DIR" 2>/dev/null | awk '{print $1}'))"

    echo ""
    if systemctl is-active "$SERVICE_NAME" &>/dev/null 2>&1; then
        info "服务:    ${GREEN}运行中${NC}"
        local pid rss
        pid=$(pgrep -f "$BIN_NAME" 2>/dev/null || true)
        if [[ -n "$pid" ]]; then
            rss=$(ps -o rss= -p "$pid" 2>/dev/null | awk '{printf "%.1f MB", $1/1024}')
            info "内存:    ${rss}"
            info "PID:     ${pid}"
        fi
    else
        warn "服务:    未运行"
    fi
}

# ── 主入口 ──
show_menu() {
    header "Nezha Dashboard (Rust) 管理脚本"
    echo "  1) 安装       install"
    echo "  2) 更新       update"
    echo "  3) 卸载       uninstall"
    echo "  4) 状态       status"
    echo "  0) 退出"
    echo ""
    read -rp "请选择 [0-4]: " choice
    case "$choice" in
        1) do_install ;; 2) do_update ;; 3) do_uninstall ;; 4) do_status ;; 0) exit 0 ;;
        *) error "无效选择"; show_menu ;;
    esac
}

case "${1:-}" in
    install)   do_install ;;
    update)    do_update ;;
    uninstall) do_uninstall ;;
    status)    do_status ;;
    "")        show_menu ;;
    *)
        echo "Nezha Dashboard (Rust) 管理脚本"
        echo ""
        echo "用法: $0 {install|update|uninstall|status}"
        echo ""
        echo "支持平台:"
        echo "  Linux   amd64, arm64, armv7 (glibc/musl)"
        echo "  macOS   amd64, arm64 (Apple Silicon)"
        echo "  FreeBSD amd64"
        exit 1
        ;;
esac
