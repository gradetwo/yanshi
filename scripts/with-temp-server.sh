#!/usr/bin/env bash
# 在**独立临时工作区**里起一个服务端，跑指定命令，然后清理。
#
# 为什么需要它：查看器/逐像素/UI 检查都会对服务端做真实的整幅或区域渲染，这些渲染产物会写进
# 工作区的 CAS，且**不被任何原子引用**（孤儿）。对着日常使用的实例跑，就会把渲染垃圾留在
# 用户的工作区里（实测一个工作区因此累积到 1.3GB）。这里用 mktemp 目录做根目录，
# 检查结束后整目录删除，日常实例的文件一个字节都不会变。
#
# 用法：scripts/with-temp-server.sh <要跑的命令…>
#   环境变量：TEMP_SERVER_PORT（缺省 8199）
set -euo pipefail
cd "$(dirname "$0")/.."

PORT="${TEMP_SERVER_PORT:-8199}"
ROOT="$(mktemp -d "${TMPDIR:-/tmp}/yanshi-check.XXXXXX")"
BIN="target/release/yanshi-serve"

if [ ! -x "$BIN" ]; then
  echo "== 构建服务端（release）"
  cargo build --release -p yanshi-http
fi

# WASM 产物守卫：浏览器里的内核是**编译产物**，改了内核源码却用旧产物，
# 会得到与原生测试相反的结论（今天因此误判"橡皮没生效"两次）。
# 只要 pkg 比内核/渲染/核心的源码旧，就自动重建。
PKG="crates/yanshi-wasm/pkg/yanshi_wasm_bg.wasm"
stale=0
if [ ! -f "$PKG" ]; then
  stale=1
elif [ -n "$(find crates/yanshi-wasm/src crates/yanshi-render/src crates/yanshi-core/src \
        -newer "$PKG" -name '*.rs' -print -quit 2>/dev/null)" ]; then
  stale=1
fi
if [ "$stale" = "1" ]; then
  echo "== WASM 产物已过期或缺失 ⇒ 自动重建（避免用旧内核得出错误结论）"
  if cargo build -q -p yanshi-wasm --target wasm32-unknown-unknown --release 2>/dev/null \
     && command -v wasm-bindgen >/dev/null 2>&1; then
    wasm-bindgen --target web --out-dir crates/yanshi-wasm/pkg --no-typescript \
      target/wasm32-unknown-unknown/release/yanshi_wasm.wasm
    echo "   ✓ WASM 内核已重建"
  else
    echo "   ⚠️ 无法重建 WASM（缺 wasm32 目标或 wasm-bindgen-cli）；浏览器检查结果可能失真" >&2
  fi
fi

cleanup() {
  if [ -n "${SERVER_PID:-}" ] && kill -0 "$SERVER_PID" 2>/dev/null; then
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
  rm -rf "$ROOT"
}
trap cleanup EXIT

echo "== 临时实例：端口 ${PORT}，根目录 ${ROOT}"
"$BIN" --bind "127.0.0.1:${PORT}" --root "$ROOT" --doc check >"$ROOT/server.log" 2>&1 &
SERVER_PID=$!

for _ in $(seq 1 40); do
  if curl -s -m 1 -o /dev/null "http://127.0.0.1:${PORT}/health"; then break; fi
  sleep 0.25
done
if ! curl -s -m 1 -o /dev/null "http://127.0.0.1:${PORT}/health"; then
  echo "服务端未就绪，日志：" >&2
  tail -20 "$ROOT/server.log" >&2
  exit 1
fi

TOKEN="$(curl -s -X POST "http://127.0.0.1:${PORT}/api/documents" \
  -d '{"doc_id":"check","width":1024,"height":1024}' \
  | python3 -c 'import sys,json;print(json.load(sys.stdin)["token"])')"

CHECK_URL="http://127.0.0.1:${PORT}/?doc=check&token=${TOKEN}"
# 查看器地址由本脚本**追加为最后一个参数**：若让调用方在命令行里写 ${CHECK_URL}，
# 外层 shell（或 make）会先把它展开成空串。
echo "== 执行：$* <viewer-url>"
status=0
"$@" "$CHECK_URL" || status=$?
# 显式保留被测命令的退出码：EXIT trap 里的 wait/rm 会覆盖隐式退出状态。
exit "$status"
