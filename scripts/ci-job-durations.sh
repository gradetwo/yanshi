#!/bin/bash
# 算一个 run 里每个 job 的**实际**时长（用 started_at，不用 run 的 created_at）
SELF="$(basename "$0")"
[ $# -lt 1 ] && { echo "用法：${SELF} <run-id>" >&2; exit 2; }
command -v gh >/dev/null || { echo "${SELF}：缺 gh" >&2; exit 3; }
NOW=$(date -u +%s)
# **★ 显式初始化**（第 1155 轮）：`shell_scripts.rs:158` 只读 `read ` 后的**前 3 个词** ✗
#   ⇒ 所以 `read -r name st stt end` 里的 `stt`/`end` **不算「赋值」** ⇒ 报「被引用但从未赋值」✗
#   ⇒ 与第 1057 轮同一个坑 ⇒ 修法同：显式赋空串 ✓
stt=""
end=""
gh api "repos/gradetwo/yanshi/actions/runs/$1/jobs" --jq '
  .jobs[] | "\(.name)\t\(.status)\t\(.started_at // "-")\t\(.completed_at // "-")"' 2>/dev/null \
| while IFS=$'\t' read -r name st stt end; do
    [ "$stt" = "-" ] && { printf "  ⏳ %-28s 未开始\n" "$name"; continue; }
    S=$(date -u -d "$stt" +%s 2>/dev/null) || continue
    if [ "$end" = "-" ]; then E=$NOW; mark="⏳"; else E=$(date -u -d "$end" +%s 2>/dev/null) || continue; mark="✓"; fi
    D=$((E-S)); [ $D -lt 0 ] && D=0; printf "  %s %-28s %2d分%02d秒\n" "$mark" "$name" $((D/60)) $((D%60))
  done
