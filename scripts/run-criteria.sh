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

if [ "${SKIP_BROWSER:-0}" != "1" ] && command -v chromium >/dev/null 2>&1; then
  chromium --headless=new --no-sandbox --disable-gpu --remote-debugging-port="$CDP_PORT" \
    --user-data-dir="$PROFILE" about:blank >"$ROOT_DIR/chrome.log" 2>&1 &
  CHROME_PID=$!
  for _ in $(seq 1 40); do curl -sf "http://127.0.0.1:$CDP_PORT/json/version" >/dev/null 2>&1 && break; sleep 0.5; done
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
    echo "  ⊘ $name（没有 chromium，跳过）"; skipped=$((skipped+1)); continue
  fi
  doc="crit_$(echo "$name" | tr -cd 'a-z0-9')"
  tok="$(token_for "$doc")"
  case "$name" in
    kernel-brush-parity.mjs)
      timeout 900 node "$script" "$BASE" "$doc" "$tok" "$ROOT/crates/yanshi-wasm/pkg/yanshi_wasm.js" >"$ROOT_DIR/out.txt" 2>&1 ;;
    browser-*)
      CDP_PORT="$CDP_PORT" timeout 900 node "$script" "$BASE/?doc=$doc&token=$tok" >"$ROOT_DIR/out.txt" 2>&1 ;;
    *)
      timeout 900 node "$script" "$BASE" >"$ROOT_DIR/out.txt" 2>&1 ;;
  esac
  code=$?
  if [ "$code" = 0 ]; then
    echo "  ✓ $name"; pass=$((pass+1))
  elif is_known_red "$name"; then
    echo "  ⚠ $name（**已知红，按记录不阻塞 CI**）: $(grep -m1 -E '^     - ' "$ROOT_DIR/out.txt" | cut -c1-120)"
    expected_red=$((expected_red+1))
  else
    echo "  ✗ $name (EXIT=$code)"; tail -6 "$ROOT_DIR/out.txt" | sed 's/^/       /'; fail=$((fail+1))
  fi
done

echo "  —— 通过 $pass｜意外失败 $fail｜已知红 $expected_red｜跳过 $skipped"
[ "$fail" = 0 ] || exit 1
