#!/usr/bin/env bash
# 等 CI 跑完并只在**状态变化**时打印 —— 替代「每轮查 CI」。
#
# **为什么需要**（第 975 轮）：861/893 轮那条规矩说「一个 run 在跑时别派发」，
# 883 轮那条说「不要每轮查 CI」（单片 criteria 可达 19 分钟，而轮次间隔只有 12-80 秒）。
# 而我**一直每轮查** ⇒ 违反了自己写的规矩。
# ⇒ 按 971 轮的观察：能被脚本强制的才会生效 ⇒ 做成这个脚本。
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
INTERVAL="${1:-60}"
case "$INTERVAL" in
  ''|*[!0-9]*) echo "  ⊘ 用法：$0 [轮询间隔秒数，缺省 60]" >&2; exit 2 ;;
esac
[ "$INTERVAL" -lt 5 ] && { echo "  ⊘ 间隔至少 5 秒（别把 API 打爆）" >&2; exit 2; }

SHA=$(git rev-parse HEAD)
RUN=$(gh run list --limit 20 --json databaseId,headSha --jq \
  "[.[] | select(.headSha==\"$SHA\")] | .[0].databaseId // empty")

if [ -z "$RUN" ]; then
  echo "  ⊘ 不可判：HEAD 还没有对应的 CI run（刚 push？等 GitHub 创建）"
  exit 3
fi
echo "  监视 run ${RUN}（HEAD ${SHA:0:8}）"

declare -A PREV
while :; do
  STATE=$(gh api "repos/gradetwo/yanshi/actions/runs/$RUN" --jq '.status + ":" + (.conclusion // "-")' 2>/dev/null)
  # 逐 job 比对结论
  while IFS=$'\t' read -r name concl; do
    key="$name"
    [ "${PREV[$key]:-}" = "$concl" ] && continue
    PREV[$key]="$concl"
    case "$concl" in
      success)   echo "  ✓ $name" ;;
      failure)   echo "  ✗ $name" ;;
      cancelled) echo "  ⊘ ${name}（取消）" ;;
      skipped)   echo "  － ${name}（跳过）" ;;
      *)         echo "  ⏳ ${name}（${concl}）" ;;
    esac
  done < <(gh api "repos/gradetwo/yanshi/actions/runs/$RUN/jobs" \
            --jq '.jobs[] | "\(.name)\t\(.conclusion // .status)"' 2>/dev/null)

  case "$STATE" in
    completed:*) echo "  ⇒ run 完成：${STATE#completed:}"; exit 0 ;;
  esac
  sleep "$INTERVAL"
done
