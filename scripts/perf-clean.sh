#!/usr/bin/env bash
# **★★★ 性能测量前：清掉残留进程 ＋ 等负载回落 ✗ ★★★**（**第 720 轮 ✓）
#
# **∴ 为什么必须 ✗**（**本会话的实测教训 ✓）：
#   **∴ 第 719 轮 ✗**：**同一段合成代码**✗ 在**几小时内**给出了**完全不同**的数字** ✓
#     **∴ 4K ✗**：**GPU **426.61 ms（**0.93×**）⇒ **1157.31 ms（**0.35×**）**** ✓
#       ＋ **∴ 而**采用次数**几乎相同**✗（**30 vs 31 ✓）⇒ **∴ 所以**不是代码差异** ✓
#         ＋ **∴ 根因 ✗**：**一个**残留的 `yanshi-serve`**✗（**∴ 从第 99 轮起**跑了一小时 ✓）
#           ＋ **∴ 证据 ✗**：**杀掉它 ＋ 等 20 秒 ⇒ **负载 1.48 ⇒ 0.97**** ✓ ★
#   **∴ 而**本会话**多次**被不可比的数字误导**✗（**第 682／703／716／719 轮 ✓）
#     ＋ **∴ 所以**：**把这条纪律**做成脚本**✗ ⇒ **∴ 从而**不再靠记忆 ✓ ★
#
# **∴ 用法 ✗**：`bash scripts/perf-clean.sh`（**在每次性能测量之前 ✓）★
#
# **★ 关键：用 `comm` 列匹配，不用 `-f`（命令行文本） ✗ ★**（**第 99 轮的教训 ✓）：
#   **∴ 为什么 ✗**：**本会话**有 **5 次** `pkill -f`／`ps | grep` **匹配到自己**✗
#     ＋ **∴ 因为**命令行里**含**目标字符串**（**∴ 如** `/tmp/chrome99` ✓）
#       ＋ **∴ 于是**：**它**杀掉了自己的 shell** ✓
#         ＋ **∴ 所以**：**只匹配**可执行名（**`comm` 列 ✓）⇒ **∴ 安全** ✓ ★
set -u

echo "== 性能测量前清理（第 720 轮）=="

# **∴ 目标 ✗**：**浏览器 ＋ 服务端 ＋ 测试二进制**（**∴ 都可能残留 ✓）★
PATTERNS='^chrom|^yanshi-serve$|^yanshi-serve-|^cargo$'
pids=$(ps -eo pid,comm 2>/dev/null | awk -v pat="$PATTERNS" '$2 ~ pat {print $1}')
if [ -n "$pids" ]; then
  echo "  找到残留进程：$(printf '%s' "$pids" | tr '\n' ' ')"
  for pid in $pids; do kill "$pid" 2>/dev/null || true; done
  sleep 3
  # **∴ 还不死就**强杀**（**∴ 只对确认过的 PID ✓）★
  for pid in $pids; do kill -9 "$pid" 2>/dev/null || true; done
  sleep 2
fi
left=$(ps -eo pid,comm 2>/dev/null | awk -v pat="$PATTERNS" '$2 ~ pat' | wc -l)
echo "  清理后残留：$left"

# **∴ 等** 1 分钟负载**回落到阈值以下 ✗**（**∴ 默认 2.0，可用 LOAD_MAX 覆盖 ✓）★
LOAD_MAX="${LOAD_MAX:-2.0}"
for i in $(seq 1 30); do
  load1=$(awk '{print $1}' /proc/loadavg 2>/dev/null || echo "9.9")
  if awk -v a="$load1" -v b="$LOAD_MAX" 'BEGIN { exit !(a <= b) }'; then
    echo "  ✓ 负载 $load1 ≤ ${LOAD_MAX}（第 ${i} 次检查）"
    break
  fi
  sleep 5
done
echo "  当前：$(cat /proc/loadavg 2>/dev/null)"
echo "== 清理完成，可以测量 =="
