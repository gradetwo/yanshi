#!/usr/bin/env bash
# **在 CI 上跑全部判据**（本地只做小改动 ✓，重活交给 GitHub ✓）。
# 约定：tool-*.mjs 收 <base-url>；browser-*.mjs 收 <viewer-url> 并读 CDP_PORT；
#       kernel-brush-parity.mjs 收 <base-url> <doc> <token> <内核 js 路径>。
# **已知红名单**（scripts/criteria-known-red.txt）里的脚本仍会跑、结果照印，但不让本脚本失败 ✓。
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
PORT="${PORT:-13990}"
CDP_PORT="${CDP_PORT:-9490}"
ROOT_DIR="$(mktemp -d)"
PROFILE="$(mktemp -d)"
KNOWN="$ROOT/scripts/criteria-known-red.txt"
is_known_red() { [ -f "$KNOWN" ] && grep -qE "^$1[[:space:]]" "$KNOWN"; }

cleanup() {
  [ -n "${SERVER_PID:-}" ] && kill "$SERVER_PID" 2>/dev/null
  [ -n "${CHROME_PID:-}" ] && kill "$CHROME_PID" 2>/dev/null
  rm -rf "$ROOT_DIR" "$PROFILE"
}
trap cleanup EXIT

case "${SKIP_SERVER:-0}" in
  1) : ;;
  *) ./target/debug/yanshi-serve --bind "127.0.0.1:$PORT" --root "$ROOT_DIR/work" \
       --doc boot --width 320 --height 240 --assets-dir "$ROOT/assets" >"$ROOT_DIR/server.log" 2>&1 &
     SERVER_PID=$!
     for _ in $(seq 1 40); do curl -sf "http://127.0.0.1:$PORT/api/documents" >/dev/null 2>&1 && break; sleep 0.5; done ;;
esac
BASE="http://127.0.0.1:$PORT"
# 让判据能核对落盘产物（render.png 等）；不设则相关检查自行跳过并打印 ✓。
export YANSHI_WORKSPACE="$ROOT_DIR/work"

if [ "${SKIP_BROWSER:-0}" != "1" ] && command -v chromium >/dev/null 2>&1; then
  chromium --headless=new --no-sandbox --disable-gpu --remote-debugging-port="$CDP_PORT" \
    --user-data-dir="$PROFILE" about:blank >"$ROOT_DIR/chrome.log" 2>&1 &
  CHROME_PID=$!
  for _ in $(seq 1 40); do curl -sf "http://127.0.0.1:$CDP_PORT/json/version" >/dev/null 2>&1 && break; sleep 0.5; done
  # **连不上就快速失败** ✓：以前会**逐条各等 15 分钟** ✗ ⇒ 一小时内什么都拿不到 ✓。
  if ! curl -sf "http://127.0.0.1:$CDP_PORT/json/version" >/dev/null 2>&1; then
    echo "  ⚠️ 浏览器起来了但 CDP 连不上 ⇒ **跳过全部浏览器判据**（不干等 ✓）"
    kill "$CHROME_PID" 2>/dev/null; CHROME_PID=""; SKIP_BROWSER=1
  fi
fi

token_for() {
  curl -s -X POST "$BASE/api/documents" -H 'content-type: application/json' \
    -d "{\"doc_id\":\"$1\",\"width\":320,\"height\":240}" |
    python3 -c 'import sys,json;print(json.load(sys.stdin)["token"])'
}

pass=0; fail=0; expected_red=0; skipped=0
for script in scripts/tool-*.mjs scripts/browser-*.mjs scripts/kernel-brush-parity.mjs; do
  name="$(basename "$script")"
  [ -f "$script" ] || continue
  # 浏览器判据没有 chromium 就跳过（本地环境常见；CI 里一定装了）。
  if [[ "$name" == browser-* ]] && [ "${SKIP_BROWSER:-0}" = "1" -o -z "${CHROME_PID:-}" ]; then
    echo "  ⊘ ${name}（没有 chromium，跳过）"; skipped=$((skipped+1)); continue
  fi
  doc="crit_$(echo "$name" | tr -cd 'a-z0-9')"
  tok="$(token_for "$doc")"
  # **心跳** ✓：卡住时一眼看出是**哪一条** ✓（以前只看到"Job 还在跑" ✗）。
  echo "  → $name"
  case "$name" in
    kernel-brush-parity.mjs)
      timeout 600 node "$script" "$BASE" "$doc" "$tok" "$ROOT/crates/yanshi-wasm/pkg/yanshi_wasm.js" >"$ROOT_DIR/out.txt" 2>&1 ;;
    browser-*)
      # **浏览器判据 240s** ✓（本地实测多在 1 分钟内 ✓）—— 以前一律 900s ✗ ⇒ 25 条最坏要跑几小时 ✗。
      CDP_PORT="$CDP_PORT" timeout 240 node "$script" "$BASE/?doc=$doc&token=$tok" >"$ROOT_DIR/out.txt" 2>&1 ;;
    *)
      # **统一传三个参数** ✓：有的判据要 <base> <doc> <token> ✓（如 tool-brush-tag-filter ✓），
      # 只收 base 的会忽略多余参数 ✓ ⇒ 一条约定覆盖两种 ✓（CI 第一轮就是这里漏了 ✗）。
      # **工具判据 180s** ✓（其中含浏览器的那几条自带更长的内部等待 ✓）。
      timeout 180 node "$script" "$BASE" "$doc" "$tok" >"$ROOT_DIR/out.txt" 2>&1 ;;
  esac
  code=$?
  if [ "$code" = 0 ]; then
    echo "  ✓ $name"; pass=$((pass+1))
  elif is_known_red "$name"; then
    echo "  ⚠ ${name}（**已知红，按记录不阻塞 CI**）: $(grep -m1 -E '^     - ' "$ROOT_DIR/out.txt" | cut -c1-120)"
    expected_red=$((expected_red+1))
  else
    echo "  ✗ $name (EXIT=$code)"; tail -6 "$ROOT_DIR/out.txt" | sed 's/^/       /'; fail=$((fail+1))
  fi
done

echo "  —— 通过 ${pass}｜意外失败 ${fail}｜已知红 ${expected_red}｜跳过 $skipped"
[ "$fail" = 0 ] || exit 1
