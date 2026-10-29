#!/usr/bin/env bash
# PWA 内联工具表一键重生成（scripts/regen-pwa-table.sh）。
#
# 为什么需要它：内联工具表（// <gen:tool-specs> 块）必须与服务端
# 工具清单逐项一致。改了工具面（tools_registry.rs / tools.rs）却没重生成，
# CI 的 tool-pwa-suggestion-accept 判据就红——而那时离提交已过去半小时。
# 本脚本把「解析二进制（按需构建）→ 起临时服务 → --write/--check → 清理」包成一条命令。
#
# 为什么必须管两份：表有两份拷贝，缺一不可——
#   ① web/api-local.js（生成器 gen-pwa-tool-specs.mjs 的 TARGET，判据 §表只查它）；
#   ② crates/yanshi-http/assets/api-local.js（服务端 include_str! 内嵌、实际 serve 的那份，
#      判据的页内 PWA 比对跑的就是它）。
#   两份在基线是逐字相同的（diff 为空）。实测教训（2026-10-29）：四支跟进只改了 web/，
#   本地 --check 通过，但 CI 仍红（batch-wait 新 run 仍倒在 tool-pwa-suggestion-accept），
#   因为 served 的 embedded 份还是旧表。所以 --write 必须两份同写，--check 必须两份同查；
#   只写 web/ 的重生成是不完整的重生成。
#
# 用法：
#   bash scripts/regen-pwa-table.sh            # 重生成并写回两份 api-local.js
#   bash scripts/regen-pwa-table.sh --check    # 只比对两份，不写回（pre-commit 用）
#   YANSHI_SERVE_BIN=/path/to/yanshi-serve bash scripts/regen-pwa-table.sh [--check]
#
# 退出码（与 gen-pwa-tool-specs.mjs 同口径）：
#   0 一致 / 写回成功；1 表过期（只在 --check 下出现，或 --write 时 embedded 份无法同步）；
#   3 不可判（服务起不来 / 构建失败）。
#   不可判 ≠ 表过期：拿不到结论时不要当成表过期处理（pre-commit 收到 3 只警告不拦提交）。
#
# 二进制解析（按顺序，第一个命中即用）：
#   ① ${YANSHI_SERVE_BIN}（显式指定，直接用，不验新鲜度——调用方负责）；
#   ② target/release/yanshi-serve（可执行且比全部 .rs 新）；
#   ③ target/debug/yanshi-serve（同上）；
#   ④ 现场构建 debug 版（cargo build -p yanshi-http）。
# 为什么构建 debug 而不是 release：表只读 /api/tools 的 JSON 内容，与优化级别无关；
#   debug 构建快得多，而重生成与提交前检查都不该为一张表付 release 构建的代价。
# 新鲜度规则照抄 run-criteria-local.sh 的教训：任一 crates/ 或 vendor/ 下的 .rs
#   比二进制新 ⇒ 二进制陈旧（第 1308 轮：只扫 crates 漏了 vendor 就判错新鲜度）。
#   JS 资产不纳入：/api/tools 只吐注册表，不含 api-local.js；run-criteria-local 扫 JS
#   是因为它跑的判据要测嵌进二进制的 viewer 资产，本脚本不测那个。
#
# 同步口径（只动表块，不动块外代码）：
#   --write 先由生成器写 web/，再把 web/ 的表块原样搬进 embedded 份（只换标记之间的行）。
#   块外若有差异只警告不拦（两份基线相同；块外差异是另一回事，提示开发者自行 cp 对齐）。

set -uo pipefail
cd "$(dirname "$0")/.."

WEB="web/api-local.js"
EMBEDDED="crates/yanshi-http/assets/api-local.js"
BEGIN="// <gen:tool-specs>"
END="// </gen:tool-specs>"

MODE="write"
if [[ "${1:-}" == "--check" ]]; then MODE="check"; fi

# 取出文件里标记之间的表块（不含标记行本身），打到 stdout；缺文件或缺标记则退出 1。
extract_block() {
  local file="$1"
  [ -f "$file" ] || return 1
  awk -v b="$BEGIN" -v e="$END" '
    $0 == b { inblock = 1; found_b = 1; next }
    $0 == e { if (inblock) found_e = 1; inblock = 0; next }
    inblock { print }
    END { if (!found_b || !found_e) exit 1 }
  ' "$file"
}

# 检查某文件是否含一对标记；缺失则退出 1。
has_markers() {
  local file="$1"
  [ -f "$file" ] || return 1
  grep -qF "$BEGIN" "$file" && grep -qF "$END" "$file"
}

# 把源文件的表块搬进目标文件（只换标记之间的行，其余行原样保留）；失败退出 1。
sync_block() {
  local src="$1" dst="$2" tmp=""
  tmp=$(mktemp)
  awk -v b="$BEGIN" -v e="$END" -v src="$src" '
    BEGIN {
      n = 0
      while ((getline line < src) > 0) {
        if (line == b) { inblock = 1; continue }
        if (line == e) { inblock = 0; continue }
        if (inblock) block[++n] = line
      }
      if (n == 0) exit 1
    }
    $0 == b { print; for (i = 1; i <= n; i++) print block[i]; skip = 1; next }
    $0 == e { skip = 0; print; next }
    !skip { print }
  ' "$dst" > "$tmp" || { rm -f "$tmp"; return 1; }
  mv "$tmp" "$dst"
}

is_fresh() {
  local bin="$1"
  [ -x "$bin" ] || return 1
  [ -z "$(find crates vendor -name '*.rs' -newer "$bin" 2>/dev/null | head -5)" ]
}

# --write 之前先确认两份都有标记，避免 web/ 已写、embedded 无法同步的半截状态。
if [[ "$MODE" == "write" ]]; then
  if ! has_markers "$WEB"; then
    echo "  ✗ 写回失败：${WEB} 里找不到生成标记" >&2
    exit 1
  fi
  if ! has_markers "$EMBEDDED"; then
    echo "  ✗ 写回失败：${EMBEDDED} 里找不到生成标记（两份须同写，半截不同步）" >&2
    exit 1
  fi
fi

BIN=""
BIN_HOW=""
if [ -n "${YANSHI_SERVE_BIN:-}" ]; then
  BIN="${YANSHI_SERVE_BIN}"
  BIN_HOW="显式指定（不验新鲜度）"
  if [ ! -x "$BIN" ]; then
    echo "  ⊘ 不可判：YANSHI_SERVE_BIN 指向的二进制不存在或不可执行：${BIN}" >&2
    exit 3
  fi
elif is_fresh target/release/yanshi-serve; then
  BIN="target/release/yanshi-serve"
  BIN_HOW="复用 release（比源码新）"
elif is_fresh target/debug/yanshi-serve; then
  BIN="target/debug/yanshi-serve"
  BIN_HOW="复用 debug（比源码新）"
else
  echo "  ⟳ 没有新鲜二进制 ⇒ 构建 debug 版（cargo build -p yanshi-http）"
  BUILD_LOG=$(mktemp)
  if ! cargo build -p yanshi-http > "$BUILD_LOG" 2>&1; then
    echo "  ⊘ 不可判：构建失败（详见 ${BUILD_LOG}，尾部如下）" >&2
    tail -5 "$BUILD_LOG" | sed 's/^/      /' >&2
    rm -f "$BUILD_LOG"
    exit 3
  fi
  rm -f "$BUILD_LOG"
  if [ ! -x target/debug/yanshi-serve ]; then
    echo "  ⊘ 不可判：构建成功但没有二进制（target/debug/yanshi-serve 不存在）" >&2
    exit 3
  fi
  BIN="target/debug/yanshi-serve"
  BIN_HOW="刚构建的 debug 版"
fi
echo "  服务端二进制：${BIN}（${BIN_HOW}）"

ROOT=$(mktemp -d "${TMPDIR:-/tmp}/yanshi-pwa-table-XXXXXX")
PORT=$(( 25000 + RANDOM % 2000 ))
YANSHI_EXPORT_DIR="${ROOT}/exports" \
  "$BIN" --root "$ROOT" --doc _bootstrap --width 320 --height 240 --bind "127.0.0.1:${PORT}" > "${ROOT}/s.log" 2>&1 &
PID=$!
trap 'kill "${PID}" 2>/dev/null; rm -rf "${ROOT}"' EXIT

for _ in $(seq 1 60); do
  curl -s -m 1 -o /dev/null "http://127.0.0.1:${PORT}/health" && break
  sleep 0.5
done
if ! curl -s -m 1 -o /dev/null "http://127.0.0.1:${PORT}/health"; then
  echo "  ⊘ 不可判：临时服务没起来（尾部日志如下）" >&2
  tail -5 "${ROOT}/s.log" 2>/dev/null | sed 's/^/      /' >&2
  exit 3
fi

BASE_URL="http://127.0.0.1:${PORT}"
if [[ "$MODE" == "check" ]]; then
  echo "  比对两份 api-local.js 与服务端工具清单…"
  node scripts/gen-pwa-tool-specs.mjs "$BASE_URL" --check
  RC=$?
  if [[ "$RC" -ne 0 ]]; then
    if [[ "$RC" -eq 1 ]]; then
      echo "  ✗ 表过期 ⇒ 重生成：bash scripts/regen-pwa-table.sh" >&2
    else
      echo "  ⊘ 不可判（gen-pwa-tool-specs.mjs 退出码 ${RC}）" >&2
    fi
    exit "$RC"
  fi
  # web/ 已由生成器证实新鲜；embedded 份只要块与 web/ 逐字相同即新鲜。
  if ! has_markers "$EMBEDDED"; then
    echo "  ✗ embedded 份表过期：${EMBEDDED} 里找不到生成标记" >&2
    echo "  ✗ 表过期 ⇒ 重生成：bash scripts/regen-pwa-table.sh" >&2
    exit 1
  fi
  if ! extract_block "$WEB" > "${ROOT}/web.block"; then
    echo "  ⊘ 不可判：取不出 ${WEB} 的表块" >&2
    exit 3
  fi
  if ! extract_block "$EMBEDDED" > "${ROOT}/emb.block"; then
    echo "  ⊘ 不可判：取不出 ${EMBEDDED} 的表块" >&2
    exit 3
  fi
  if ! cmp -s "${ROOT}/web.block" "${ROOT}/emb.block"; then
    echo "  ✗ embedded 份表过期（web/ 已新、embedded 仍旧）⇒ 重生成：bash scripts/regen-pwa-table.sh" >&2
    diff "${ROOT}/web.block" "${ROOT}/emb.block" | head -6 | sed 's/^/      /' >&2
    exit 1
  fi
  echo "  ✓ 两份表一致（web/ 与 embedded）"
  echo "  ✓ 表与服务端一致"
  exit 0
else
  echo "  重生成两份 api-local.js…"
  node scripts/gen-pwa-tool-specs.mjs "$BASE_URL" --write
  RC=$?
  if [[ "$RC" -ne 0 ]]; then
    if [[ "$RC" -eq 1 ]]; then
      echo "  ✗ 表过期 ⇒ 重生成失败（生成器未写出）" >&2
    else
      echo "  ⊘ 不可判（gen-pwa-tool-specs.mjs 退出码 ${RC}）" >&2
    fi
    exit "$RC"
  fi
  if ! sync_block "$WEB" "$EMBEDDED"; then
    echo "  ✗ 同步失败：表块未能搬进 ${EMBEDDED}（web/ 已写，两份现不同步，请手动处理）" >&2
    exit 1
  fi
  if ! extract_block "$WEB" > "${ROOT}/web.block"; then
    echo "  ⊘ 不可判：取不出 ${WEB} 的表块" >&2
    exit 3
  fi
  if ! extract_block "$EMBEDDED" > "${ROOT}/emb.block"; then
    echo "  ⊘ 不可判：取不出 ${EMBEDDED} 的表块" >&2
    exit 3
  fi
  if ! cmp -s "${ROOT}/web.block" "${ROOT}/emb.block"; then
    echo "  ✗ 同步校验失败：embedded 份表块与 web/ 不一致" >&2
    exit 1
  fi
  echo "  ✓ 已写回两份 api-local.js（别忘了 git add 它们再提交）"
  if ! cmp -s "$WEB" "$EMBEDDED"; then
    echo "  ⚠ 两份在表块之外仍有差异（基线是逐字相同；若块外改动只该进 served 份，请自行 cp 对齐）" >&2
    diff "$WEB" "$EMBEDDED" | head -4 | sed 's/^/      /' >&2
  fi
  exit 0
fi
