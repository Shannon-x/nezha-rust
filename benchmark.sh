#!/usr/bin/env bash
# Nezha Rust vs Go 性能基准对比脚本
# 用法: ./benchmark.sh [rust_binary] [go_binary]

set -euo pipefail

RUST_BIN="${1:-./target/release/nezha-dashboard}"
GO_BIN="${2:-}"
DURATION=30
CONCURRENCY=100
PORT_RUST=18008
PORT_GO=18009

RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}╔═══════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║  Nezha 性能基准对比 — Rust vs Go              ║${NC}"
echo -e "${BLUE}╚═══════════════════════════════════════════════╝${NC}"

# ──────── 1. 内存用量对比 ────────
echo -e "\n${GREEN}=== 1. 内存用量对比 ===${NC}"

echo "Starting Rust binary..."
NZ_LISTEN_PORT=$PORT_RUST $RUST_BIN -c /tmp/nz_bench_config.yaml &
RUST_PID=$!
sleep 3

RUST_RSS=$(ps -o rss= -p $RUST_PID 2>/dev/null || echo "0")
RUST_RSS_MB=$(echo "scale=1; $RUST_RSS / 1024" | bc)
echo -e "  Rust 启动内存: ${GREEN}${RUST_RSS_MB} MB${NC} (RSS)"

# 检查二进制大小
RUST_SIZE=$(stat -c%s "$RUST_BIN" 2>/dev/null || echo "0")
RUST_SIZE_MB=$(echo "scale=1; $RUST_SIZE / 1048576" | bc)
echo -e "  Rust 二进制大小: ${GREEN}${RUST_SIZE_MB} MB${NC}"

if [ -n "$GO_BIN" ] && [ -f "$GO_BIN" ]; then
    NZ_LISTEN_PORT=$PORT_GO $GO_BIN &
    GO_PID=$!
    sleep 3
    GO_RSS=$(ps -o rss= -p $GO_PID 2>/dev/null || echo "0")
    GO_RSS_MB=$(echo "scale=1; $GO_RSS / 1024" | bc)
    echo -e "  Go   启动内存: ${RED}${GO_RSS_MB} MB${NC} (RSS)"

    GO_SIZE=$(stat -c%s "$GO_BIN" 2>/dev/null || echo "0")
    GO_SIZE_MB=$(echo "scale=1; $GO_SIZE / 1048576" | bc)
    echo -e "  Go   二进制大小: ${RED}${GO_SIZE_MB} MB${NC}"
fi

# ──────── 2. 吞吐量/延迟测试 ────────
echo -e "\n${GREEN}=== 2. API 吞吐量/延迟测试 ===${NC}"

if command -v wrk &> /dev/null; then
    echo -e "\n  ${BLUE}--- Rust (GET /api/v1/setting) ---${NC}"
    wrk -t4 -c${CONCURRENCY} -d${DURATION}s "http://127.0.0.1:${PORT_RUST}/api/v1/setting" 2>&1 | tail -8

    if [ -n "${GO_PID:-}" ]; then
        echo -e "\n  ${BLUE}--- Go (GET /api/v1/setting) ---${NC}"
        wrk -t4 -c${CONCURRENCY} -d${DURATION}s "http://127.0.0.1:${PORT_GO}/api/v1/setting" 2>&1 | tail -8
    fi
elif command -v ab &> /dev/null; then
    echo -e "\n  ${BLUE}--- Rust (GET /api/v1/setting) ---${NC}"
    ab -n 10000 -c ${CONCURRENCY} -q "http://127.0.0.1:${PORT_RUST}/api/v1/setting" 2>&1 | grep -E "Requests per second|Time per request|Transfer rate|Percentage"

    if [ -n "${GO_PID:-}" ]; then
        echo -e "\n  ${BLUE}--- Go (GET /api/v1/setting) ---${NC}"
        ab -n 10000 -c ${CONCURRENCY} -q "http://127.0.0.1:${PORT_GO}/api/v1/setting" 2>&1 | grep -E "Requests per second|Time per request|Transfer rate|Percentage"
    fi
else
    echo "  ⚠ wrk/ab 未安装，使用 curl 简单测试"
    echo -e "\n  ${BLUE}--- Rust 单请求延迟 ---${NC}"
    for i in 1 2 3 4 5; do
        curl -s -o /dev/null -w "  请求 $i: %{time_total}s\n" "http://127.0.0.1:${PORT_RUST}/api/v1/setting"
    done
fi

# ──────── 3. 负载后内存 ────────
echo -e "\n${GREEN}=== 3. 负载后内存 ===${NC}"
sleep 2
RUST_RSS_AFTER=$(ps -o rss= -p $RUST_PID 2>/dev/null || echo "0")
RUST_RSS_AFTER_MB=$(echo "scale=1; $RUST_RSS_AFTER / 1024" | bc)
echo -e "  Rust 负载后内存: ${GREEN}${RUST_RSS_AFTER_MB} MB${NC} (RSS)"

if [ -n "${GO_PID:-}" ]; then
    GO_RSS_AFTER=$(ps -o rss= -p $GO_PID 2>/dev/null || echo "0")
    GO_RSS_AFTER_MB=$(echo "scale=1; $GO_RSS_AFTER / 1024" | bc)
    echo -e "  Go   负载后内存: ${RED}${GO_RSS_AFTER_MB} MB${NC} (RSS)"
fi

# ──────── 清理 ────────
echo -e "\n${GREEN}=== 清理 ===${NC}"
kill $RUST_PID 2>/dev/null || true
kill ${GO_PID:-0} 2>/dev/null || true
echo "  Done."

echo -e "\n${BLUE}╔═══════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║  基准测试完成                                  ║${NC}"
echo -e "${BLUE}╚═══════════════════════════════════════════════╝${NC}"
