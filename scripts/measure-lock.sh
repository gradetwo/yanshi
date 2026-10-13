#!/usr/bin/env bash
# **★ 测量锁：同一时刻只允许一个 `--spawn` 测量作业 ✗ ★**
#
# **∴ 为什么造它 ✗**（**第 801 轮；**用户直接指令 ✓）★**：
#   **∴ 用户在会话中指出 ✗**：**「**经常犯的错误要考虑造工具简化自己的操作和避免错误**」
#     ＋ **∴ 而**本会话我**犯过两次**同一个错**✗：
#       ⇒ **∴ ①** 让 `be15` 与 `chunkab` **并行** ⇒ **∴ 而**它们**互相杀进程**
#         ⇒ **∴ ②** 让 `timestamps` 与 `ab45` **并行** ⇒ **∴ 同样** ✓
#   **∴ 机制（**有证据 ✓）★**：
#     ⇒ **∴ 每个 `--spawn` 作业开始时执行 `pkill -9 -x yanshi-serve`**✗
#       ⇒ **∴ 于是**它**杀掉另一个作业正在用的服务**
#         ⇒ **∴ 另一个作业读 `/proc/<pid>/stat` 得到 `ENOENT`** ✓
#           ⇒ **∴ 那**不是"变慢几成"✗ 而**是**数据全废** ✓ ★★
#
# **∴ 它做什么 ✗** ★**：
#   ⇒ **∴ ①** 取一把**独占**的 `flock`（**`/tmp/yanshi-measure.lock` ✓）
#     ⇒ **∴ ②** 默认**不等待**：**若**别人占着 ⇒ **∴ 立刻**退出 3**✗ 并**打印谁占着**
#       ＋ **∴ 加 `--wait` ⇒ **∴ 它**排队等（**有上限 ✓）
#   ⇒ **∴ ③** 拿到锁**之后**才跑命令 ⇒ **∴ 于是**不可能并行** ✓
#
# **∴ 用法 ✗**：
#   scripts/measure-lock.sh node scripts/tool-render-cost-accounts.mjs --spawn …   # 不等
#   scripts/measure-lock.sh --wait 1800 node scripts/tool-render-cost-accounts.mjs …
#   scripts/measure-lock.sh --status                                              # 看谁占着
#
# **∴ 退出码 ✗**：被跑命令的退出码｜**3** 锁被别人占着（**前置不满足 ⇒ 不是判据失败 ✓）｜2 用法错

set -u

LOCK="${YANSHI_MEASURE_LOCK:-/tmp/yanshi-measure.lock}"
SELF="$(basename "$0")"

usage() {
  echo "  ✗ ${1}" >&2
  echo "  用法：${SELF} [--wait <秒>] [--status] <命令> [参数…]" >&2
  exit 2
}

wait_secs=""
if [ "${1:-}" = "--status" ]; then
  # **∴ 看谁占着 ✗**（**∴ 用** `flock -n` 试一下 ✓）★
  if flock -n "$LOCK" true 2>/dev/null; then
    echo "  ✓ 锁空闲（${LOCK}）⇒ **∴ 现在**可以起测量** ✓"
    exit 0
  fi
  echo "  ✗ 锁被占着（${LOCK}）⇒ **∴ 现在**不能起第二个 `--spawn` 测量** ✓"
  echo "     ⇒ **∴ 那**两个作业会**互相 `pkill -9 -x yanshi-serve`** ⇒ **∴ 数据全废** ✓"
  if [ -s "${LOCK}.owner" ]; then
    echo "     ⇒ 持有者：$(cat "${LOCK}.owner" 2>/dev/null | head -1)"
  fi
  exit 3
fi
if [ "${1:-}" = "--wait" ]; then
  [ $# -ge 3 ] || usage "--wait 需要 <秒> 和一条命令"
  wait_secs="${2}"
  case "$wait_secs" in
    ''|*[!0-9]*) usage "--wait 的秒数必须是整数" ;;
  esac
  shift 2
fi
[ $# -ge 1 ] || usage "缺要跑的命令"
# **∴ 命令里必须有 `--spawn`**✗ ⇒ **∴ 否则**它不建服务 ⇒ **∴ 不需要锁** ✓
case "$*" in
  *--spawn*) : ;;
  *)
    echo "  ⇒ 命令里没有 \`--spawn\` ⇒ **∴ 它**不建服务**✗ ⇒ **∴ 不需要**这把锁**"
    echo "     （${LOCK} 只保护会 \`pkill -9 -x yanshi-serve\` 的作业）"
    exec "$@"
    ;;
esac

# **∴ owner 标记：**写清是谁 + 何时**✗ ⇒ **∴ 冲突时读的人**立刻知道** ✓
write_owner() { printf '%s\n' "pid=$$ since=$(date '+%F %T') cmd=$*" > "${LOCK}.owner" 2>/dev/null || true; }

if [ -n "$wait_secs" ]; then
  exec 9>"$LOCK"
  echo "  ⏳ 等锁（**上限 ${wait_secs} 秒 ✓）…  **∴ 若**别人在测 ⇒ **∴ 我**排队** ✓"
  if ! flock -w "$wait_secs" 9; then
    echo "  ✗ 等锁超时（${wait_secs} 秒）⇒ **∴ 本次**没有开始测量**✗ 不是判据失败** ✓" >&2
    [ -s "${LOCK}.owner" ] && echo "     ⇒ 持有者：$(head -1 "${LOCK}.owner")" >&2
    exit 3
  fi
  write_owner "$@"
  echo "  ✓ 拿到锁 ⇒ 开始测量：$*"
  "$@"
  rc=$?
  rm -f "${LOCK}.owner" 2>/dev/null || true
  echo "  ⇒ 测量结束（退出码 ${rc}）⇒ **∴ 锁**已释放** ✓"
  exit "$rc"
fi

exec 9>"$LOCK"
if ! flock -n 9; then
  echo "  ✗ 锁被占着 ⇒ **∴ 本次**没有开始测量**✗ 不是判据失败** ✓" >&2
  echo "     ⇒ **∴ 因为**两个 \`--spawn\` 作业会**互相 \`pkill -9 -x yanshi-serve\`** ⇒ **∴ 数据全废** ✓" >&2
  if [ -s "${LOCK}.owner" ]; then
    echo "     ⇒ 持有者：$(head -1 "${LOCK}.owner")" >&2
  fi
  echo "     ⇒ **∴ 要**排队 ⇒ **∴ 用** \`--wait <秒>\`** ✓" >&2
  exit 3
fi
write_owner "$@"
echo "  ✓ 拿到锁 ⇒ 开始测量：$*"
"$@"
rc=$?
rm -f "${LOCK}.owner" 2>/dev/null || true
echo "  ⇒ 测量结束（退出码 ${rc}）⇒ **∴ 锁**已释放** ✓"
exit "$rc"
