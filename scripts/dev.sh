#!/usr/bin/env bash
# 一键开发启动：构建 WASM 客户端内核（可选）+ 服务端（release），然后前台启动。
#
# 用法：
#   scripts/dev.sh                # 缺省 127.0.0.1:8110，数据落在 ~/.local/share/yanshi/workspace
#   scripts/dev.sh --port 9000    # 换端口
#   PORT=9000 ROOT=/tmp/yanshi scripts/dev.sh
#
# 设计取舍：**WASM 内核缺失不阻塞启动** —— 没有 wasm-bindgen 工具链时只打印一行提示，
# 服务端照样起来（查看器自动退化为服务端渲染）；这样"第一次跑起来"不需要装一堆东西。
set -euo pipefail
cd "$(dirname "$0")/.."

PORT="${PORT:-8110}"
ROOT="${ROOT:-$HOME/.local/share/yanshi/workspace}"
DOC="${DOC:-yanshi}"

# 解析 --port/--root（保持脚本简单，只支持这两个最常用的覆盖）。
while [ $# -gt 0 ]; do
  case "$1" in
    --port) PORT="$2"; shift 2 ;;
    --root) ROOT="$2"; shift 2 ;;
    --help|-h) sed -n '2,10p' "$0"; exit 0 ;;
    *) echo "未知参数：$1（可用 --port/--root）" >&2; exit 2 ;;
  esac
done

echo "== 构建 WASM 客户端内核（可选）"
if cargo build -q -p yanshi-wasm --target wasm32-unknown-unknown --release 2>/dev/null; then
  if command -v wasm-bindgen >/dev/null 2>&1; then
    wasm-bindgen --target web --out-dir crates/yanshi-wasm/pkg --no-typescript \
      target/wasm32-unknown-unknown/release/yanshi_wasm.wasm
    echo "   ✓ crates/yanshi-wasm/pkg 已更新（浏览器端本地乐观渲染可用）"
  else
    echo "   ⚠️ 未安装 wasm-bindgen-cli，跳过 JS 胶水生成；"
    echo "      查看器仍可用（服务端渲染），需要本地乐观渲染时执行："
    echo "      cargo install wasm-bindgen-cli --version 0.2.129 --locked"
  fi
else
  echo "   ⚠️ 未安装 wasm32-unknown-unknown target，跳过；查看器将走服务端渲染。"
  echo "      需要时执行：rustup target add wasm32-unknown-unknown"
fi

echo "== 构建服务端（release）"
cargo build --release -p yanshi-http

mkdir -p "$ROOT"
echo "== 启动：http://127.0.0.1:$PORT/   （数据目录 $ROOT）"
exec target/release/yanshi-serve --bind "127.0.0.1:$PORT" --root "$ROOT" --doc "$DOC"
