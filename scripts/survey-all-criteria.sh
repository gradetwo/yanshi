#!/bin/bash
# 全量普查：把 scripts/ 里每个判据都在本机跑一次，记录退出码（第 1160 轮加）
#
# **∴ 为什么**：我之前**逐轮抽 4–8 个** ⇒ 274/284 = 96% 用了 **60+ 轮** ✗
#   ⇒ 而这个脚本一次跑完 ⇒ 且结果可落盘对比 ✓
#
# **∴ 用法**：`bash scripts/survey-all-criteria.sh [输出文件]`
# **∴ 注意**：它要跑 284 个判据（每个 5–40 秒）⇒ 约 1–2 小时 ⇒ 建议后台跑
set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
OUT=${1:-/tmp/yanshi-survey.tsv}
[ -f "$OUT" ] && { echo "${0##*/}：输出已存在（${OUT}）⇒ 先删" >&2; exit 2; }
: > "$OUT"
TOTAL=0
for f in "$ROOT"/scripts/{tool,browser,mcp,serve,check}-*.mjs; do
  [ -f "$f" ] || continue
  name=$(basename "$f")
  TOTAL=$((TOTAL + 1))
  r=$(cd "$ROOT" && timeout 600 bash scripts/run-criteria-local.sh "scripts/$name" 2>/dev/null \
      | grep -oE "判据退出码=[0-9]+" | tail -1 | grep -oE "[0-9]+")
  printf "%s\t%s\n" "$name" "${r:-?}" >> "$OUT"
  printf "  [%3d] %-45s %s\n" "$TOTAL" "$name" "${r:-?}"
done
echo ""
echo "  共 $TOTAL 个 ⇒ 结果在 $OUT"
awk -F'\t' '{c[$2]++} END {for (k in c) printf "  %s = %d\n", k, c[k]}' "$OUT" | sort
