#!/usr/bin/env bash
# **起一个"肯定是新的"服务端** ✓ —— 这条脚本存在，是因为我**同一个坑踩了三次** ✗：
#
# **那个坑** ✓：`yanshi-serve` 这个**二进制属于 `crates/yanshi-http`** ✓，
# 而我改完 `crates/yanshi-server`（库 ✓）后经常只跑
# `cargo build --release -p yanshi-server` ✗ ⇒ **二进制还是旧的** ✓
# ⇒ 用 curl 验工具时看到的是**旧行为** ✗（例如"不接受参数 color" ✓，而代码里明明已经加了 ✓）。
# ⇒ **凡是"用 curl 验工具"之前，都必须先重编 `-p yanshi-http`** ✓ —— 这就是这条脚本的全部意义 ✓。
#
# 用法：
#   scripts/serve.sh                 # 重编 + 起在 127.0.0.1:8110，数据在 ~/.local/share/yanshi/workspace
#   scripts/serve.sh --port 8300     # 换端口
#   PORT=8300 ROOT=/tmp/ws scripts/serve.sh
set -euo pipefail
cd "$(dirname "$0")/.."

PORT="${PORT:-8110}"
ROOT="${ROOT:-$HOME/.local/share/yanshi/workspace}"
DOC="${DOC:-yanshi}"

while [ $# -gt 0 ]; do
  case "$1" in
    --port) PORT="$2"; shift 2 ;;
    --root) ROOT="$2"; shift 2 ;;
    --no-build) shift ;;   # **只给"我确定刚编过"的场合** ✓（默认一定重编 ✓）
    -h|--help) sed -n '2,12p' "$0"; exit 0 ;;
    *) echo "未知参数：${1}（可用 --port/--root/--no-build）" >&2; exit 2 ;;
  esac
done

echo "== 重编服务端二进制（-p yanshi-http ✓ —— 它才是 yanshi-serve 的归属 ✓）"
cargo build --release -p yanshi-http
# **把身份打出来** ✓ —— 这样"这一版是哪一版"当场可见 ✓（与打包里的 commit 断言同一个思路 ✓）。
echo -n "   身份："; ./target/release/yanshi-serve --version

mkdir -p "$ROOT"
echo "== 起在 http://127.0.0.1:$PORT/ （数据目录 ${ROOT}）"
exec ./target/release/yanshi-serve --bind "127.0.0.1:$PORT" --root "$ROOT" --doc "$DOC" \
  --assets-dir "$(cd "$(dirname "$0")/.." && pwd)/assets"
