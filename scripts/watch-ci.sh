#!/usr/bin/env bash
# 等 CI 跑完并只在**状态变化**时打印 —— 替代「每轮查 CI」。
#
# **为什么需要**（第 975 轮）：861/893 轮那条规矩说「一个 run 在跑时别派发」，
# 883 轮那条说「不要每轮查 CI」（单片 criteria 可达 19 分钟，而轮次间隔只有 12-80 秒）。
# 而我**一直每轮查** ⇒ 违反了自己写的规矩。
# ⇒ 按 971 轮的观察：能被脚本强制的才会生效 ⇒ 做成这个脚本。
#
# **∴ 已知局限（第 1067 轮记）**：
#   ① 只监视 **HEAD** 的 run ⇒ 想看**别的提交**的 run 要手动：
#        gh api "repos/gradetwo/yanshi/actions/runs/<id>/jobs" \
#          --jq '.jobs[] | select(.status=="in_progress") | "\(.name) 起于 \(.started_at)"'
#   ② `--once` 对 **pending** 的 run 只打印一行（pending 的 run **没有 job**，看不到 started_at）
#
# 用法：
#   bash scripts/watch-ci.sh            # 等当前 HEAD 的那个 run 跑完
#   bash scripts/watch-ci.sh 15         # 15 秒轮询（缺省 60）
#
# 行为：**只在某 job 的结论变化时打印一行**，其余时间静默；全部完成后退出。

set -uo pipefail
cd "$(dirname "$0")/.."

# **不要有假参数**（第 988 轮实测）：原来这里有个 `LIMIT`，只赋值**从未使用**
# ⇒ `--limit 3` 会被接受而**毫无效果** ⇒ 那是「静默降级」（用户以为改了行为，其实没有）✗
# ⇒ 所以删掉它，只留一个**真的会生效**的参数：轮询间隔。
# **`--once`**（第 1035 轮加）：查一次就退出，**不阻塞** ✓
#   **∴ 为什么**：这个脚本原本是**阻塞式**（run 完成才退出）✗
#     ⇒ 适合 `nohup ... &` 后台跑，**不适合每轮前台调**（会挂住一轮）✗
#     ⇒ 而我 1034 轮正因此又回到「每轮手查 CI」⇒ 违反 883 轮那条 ✓
# **查询频率提醒**（第 1133 轮）：**∴ 为什么** —— 我在 1124–1131 轮**共 8 轮**反复查同一个
#   job，而 8 轮里信息增量只有 **2 分 30 秒**（轮次间隔 8–22 秒 vs CI 17+ 分钟）
#   ⇒ **∴ 「每 5 轮查一次」也不够 ⇒ 应该每 30 轮** ✗
#   ⇒ **∴ 所以**把「上次查询时间」记下来，**每次都报出来** ✓（**∴ 不阻塞，因为有时确实该查**）
STAMP="${TMPDIR:-/tmp}/watch-ci-last-query"
NOW=$(date +%s)
if [ -f "$STAMP" ]; then
  LAST=$(cat "$STAMP" 2>/dev/null || echo 0)
  GAP=$(( NOW - LAST ))
  if [ "$GAP" -lt 300 ]; then
    echo "  ⚠ 距上次查询只有 ${GAP} 秒（轮次间隔通常 8–22 秒，而 CI 要 17+ 分钟）"
    echo "     ⇒ 每轮查的信息量接近 0；建议每 30 轮才查一次"
  else
    echo "  距上次查询 ${GAP} 秒"
  fi
fi
printf '%s' "$NOW" > "$STAMP" 2>/dev/null || true
ONCE=0
if [ "${1:-}" = "--once" ]; then ONCE=1; shift; fi
INTERVAL="${1:-60}"
case "$INTERVAL" in
  ''|*[!0-9]*) echo "  ⊘ 用法：$0 [--once] [轮询间隔秒数，缺省 60]" >&2; exit 2 ;;
esac
[ "$INTERVAL" -lt 5 ] && { echo "  ⊘ 间隔至少 5 秒（别把 API 打爆）" >&2; exit 2; }

SHA=$(git rev-parse HEAD)
# **★ 用 `/actions/runs` 端点，不要 `gh run list`** ✓（第 1278 轮实测）
#   **为什么**：`gh run list` 内部请求 `/actions/workflows?per_page=100`
#   ⇒ 那个端点有**独立的、低得多的次级限额** ⇒ 即使 `core` 还有 3806/5000 也会 403 ✗
#   ⇒ 而 `/actions/runs` 同一时刻可用 ✓
RUN=$(gh api "repos/gradetwo/yanshi/actions/runs?per_page=20" \
  --jq "[.workflow_runs[] | select(.head_sha==\"$SHA\")] | .[0].id // empty" 2>/dev/null)

if [ -z "$RUN" ]; then
  echo "  ⊘ 不可判：HEAD 还没有对应的 CI run（刚 push？等 GitHub 创建）"
  exit 3
fi
echo "  监视 run ${RUN}（HEAD ${SHA:0:8}）"

# **不用 `declare -A`**（第 1017 轮）：那是 **bash 4+** 的特性 ✗
# ⇒ macOS 自带 bash 3.2 ⇒ 会直接报 `declare: -A: invalid option` ✗
# ⇒ 而 `script_portability` 测试**只查**「$VAR 紧跟非 ASCII」⇒ 抓不到这一条 ✗
# ⇒ 所以改用**临时文件**存「name<TAB>concl」对（bash 3.2 兼容）
PREV_FILE="$(mktemp "${TMPDIR:-/tmp}/watch-ci-prev.XXXXXX")"
trap 'rm -f "$PREV_FILE"' EXIT
while :; do
  STATE=$(gh api "repos/gradetwo/yanshi/actions/runs/$RUN" --jq '.status + ":" + (.conclusion // "-")' 2>/dev/null)
  # 逐 job 比对结论
  # **先初始化**（第 1057 轮）：shell_scripts 测试只取 \`read \` 后**前 3 个词**（含 \`-r\`）
  #   ⇒ 只能覆盖 2 个变量 ⇒ 第 3 个起必须**显式初始化**（否则报『被引用但从未赋值』）
  started=""; concl=""
  #   ⇒ 原来在第 4 位 ⇒ 被报『${started} 被引用但从未赋值』✗
  while IFS=$'\t' read -r name started concl; do
    key="$name"
    grep -qxF "${key}	${concl}" "$PREV_FILE" 2>/dev/null && continue
    printf '%s\t%s\n' "$key" "$concl" >> "$PREV_FILE"
    case "$concl" in
      success)   echo "  ✓ $name" ;;
      failure)   echo "  ✗ $name" ;;
      cancelled) echo "  ⊘ ${name}（取消）" ;;
      skipped)   echo "  － ${name}（跳过）" ;;
      *)         echo "  ⏳ ${name}（${concl}${started:+，起于 ${started}}）" ;;
    esac
  # **带上已跑时长**（第 1043 轮加）：**∴ 为什么** —— 第 1042 轮我凭「轮次数」估
  #   `shard 3/6` 跑了 32 分钟，而用 `date` 对照 `started_at` 实际只有 **14 分钟** ✗
  #   ⇒ 轮次间隔不固定（12–80 秒）⇒ 凭轮次估时长**必然不准** ✗
  done < <(gh api "repos/gradetwo/yanshi/actions/runs/$RUN/jobs" \
            --jq '.jobs[] | "\(.name)\t\(.conclusion // .status)\t\(.started_at // "-")"' 2>/dev/null)

  case "$STATE" in
    completed:*) echo "  ⇒ run 完成：${STATE#completed:}"; exit 0 ;;
  esac
  # `--once`：打印完当前各 job 状态就退出（不阻塞）
  [ "$ONCE" = "1" ] && { echo "  ⇒ run 仍在 ${STATE%%:*}$([ "$ONCE" = "1" ] && echo "（--once，不等待）")"; exit 0; }
  sleep "$INTERVAL"
done
