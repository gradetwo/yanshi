#!/usr/bin/env bash
# 用**引擎自己**渲染桌面条目图标（dogfooding）：起一个临时服务端，画一枚「偃师印」，
# 取回 256×256 PNG 写到 deploy/icons/yanshi.png。
#
# 用法：deploy/omarchy/make-icon.sh [输出路径]
# 依赖：已构建的 target/release/yanshi-serve（cargo build --release -p yanshi-http）

set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT="${1:-$REPO_DIR/deploy/icons/yanshi.png}"
PORT="${YANSHI_ICON_PORT:-8123}"
ROOT="$(mktemp -d)"
TOKEN=""

cleanup() {
  [[ -n ${SERVER_PID:-} ]] && kill "$SERVER_PID" 2>/dev/null || true
  rm -rf "$ROOT"
}
trap cleanup EXIT

"$REPO_DIR/target/release/yanshi-serve" --bind "127.0.0.1:$PORT" --root "$ROOT" --doc icon >/dev/null 2>&1 &
SERVER_PID=$!
for _ in $(seq 1 40); do
  curl -sf "http://127.0.0.1:$PORT/health" >/dev/null 2>&1 && break
  sleep 0.25
done

TOKEN=$(curl -sf -X POST "http://127.0.0.1:$PORT/api/documents" \
  -d '{"doc_id":"icon","width":256,"height":256,"actor":"human:icon"}' |
  python3 -c 'import sys,json;print(json.load(sys.stdin)["token"])')

tool() {
  curl -sf -X POST "http://127.0.0.1:$PORT/api/tools/$1?doc=icon&token=$TOKEN" -d "$2" >/dev/null
}

# 印面：深底 + 琥珀圆 + 「人」字笔触（与默认主题的暖色一致）。
tool create_layer '{"layer_id":"L","name":"seal"}'
tool draw_shape '{"layer_id":"L","data":{"geometry":{"kind":"rect","bbox":{"x":0,"y":0,"w":256,"h":256}},"color":"#16161a"}}'
tool draw_shape '{"layer_id":"L","data":{"geometry":{"kind":"ellipse","bbox":{"x":36,"y":36,"w":184,"h":184}},"color":"#e8a33d"}}'
tool draw_stroke '{"layer_id":"L","data":{"points":[[86,150],[128,92],[170,150]],"size":11,"color":[22,22,26,255]}}'
tool draw_stroke '{"layer_id":"L","data":{"points":[[92,178],[164,178]],"size":7,"color":[22,22,26,255]}}'

URL=$(curl -sf -X POST "http://127.0.0.1:$PORT/api/tools/render_region?doc=icon&token=$TOKEN" \
  -d '{"region":{"x":0,"y":0,"w":256,"h":256}}' |
  python3 -c 'import sys,json;print(json.load(sys.stdin)["thumb_url"])')

mkdir -p "$(dirname "$OUT")"
curl -sf "http://127.0.0.1:$PORT$URL" -o "$OUT"
echo "已写出 $OUT（$(file -b "$OUT")）"
