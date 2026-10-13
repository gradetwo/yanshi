#!/usr/bin/env bash
# **★ 跑一条检查命令，并**如实返回它的退出码** ✗ ★**
#
# **∴ 为什么造它 ✗**（**第 803 轮；**用户直接指令 ✓）★**：
#   **∴ 用户在会话中指出 ✗**：**「**经常犯的错误要考虑造工具简化自己的操作和避免错误**」
#     ＋ **∴ 而**本会话我**犯过 ≥3 次**同一个错**✗：
#       ⇒ **∴ 形态 ✗**：**`node foo.mjs 2>&1 | head -3; echo "exit=$?"`**
#         ⇒ **∴ 而** `$?` **拿到的是 `head` 的 0**✗ ⇒ **∴ 于是**判据**明明红了**✗
#           ＋ **∴ 打印**却是 `exit=0`** ✓ ★★
#             ⇒ **∴ 证据（**本轮 ✓）**✗**：**我验证 `measure-lock.sh` 时**又犯了一次** ✓
#   **∴ 更隐蔽的形态 ✗**：**`if grep … | head -3; then`**
#     ＋ **∴ 而**管道退出码是**最后一个命令**（`head` ✓）的 ⇒ **∴ 它**永远真** ✓ ★★★
#
# **∴ 它做什么 ✗** ★**：
#   ⇒ **∴ ①** 用 `set -o pipefail` 跑完整命令（**∴ 于是**管道里**任一环失败**都算失败 ✓）
#     ⇒ **∴ ②** **同时**把完整输出**落盘**（**默认 `/tmp/run-check-<时间>.log` ✓）
#       ⇒ **∴ 而**只把**尾部**打到屏幕（**∴ 人**看尾 ✗ 而**文件**留全 ✓）★
#     ⇒ **∴ ③** **分开报告**三个量：**命令退出码（**判据的 ✓）｜打印是否截断｜日志路径**
#   ⇒ **∴ ④** 若**退出码非 0** ⇒ **∴ 它**自己**也退非 0**✗ 而**不是**吞掉** ✓ ★★★
#
# **∴ 用法 ✗**：
#   scripts/run-check.sh node scripts/tool-foo.mjs --self-test
#   scripts/run-check.sh --tail 40 bash scripts/bar.sh
#   scripts/run-check.sh --all node scripts/tool-baz.mjs        # 打印**全部**输出
#
# **∴ 退出码 ✗**：**被跑命令的退出码**（**原样 ✓）｜**2** 用法错
#
# **∴ 反面教材（**不要这样写 ✓）★**：
#   ```
#   node x.mjs | head -3; echo "exit=$?"       # ✗ 拿到 head 的 0
#   if grep -q pat file | head -1; then …      # ✗ 永远真
#   ```
#   **∴ 改成 ✗**：
#   ```
#   scripts/run-check.sh node x.mjs            # ✓ 原样退出码 ＋ 全量落盘
#   ```

set -u
set -o pipefail

tail_lines=20
if [ "${1:-}" = "--tail" ]; then
  [ $# -ge 3 ] || { echo "  ✗ --tail 需要 <行数> 和一条命令" >&2; exit 2; }
  case "${2}" in ''|*[!0-9]*) echo "  ✗ --tail 的行数必须是整数" >&2; exit 2 ;; esac
  tail_lines="${2}"
  shift 2
fi
show_all=0
if [ "${1:-}" = "--all" ]; then
  show_all=1
  tail_lines=100000
  shift
fi
[ $# -ge 1 ] || { echo "  ✗ 缺要跑的命令" >&2; echo "  用法：$0 [--tail <行数>|--all] <命令> [参数…]" >&2; exit 2; }

log="${YANSHI_RUN_CHECK_LOG:-/tmp/run-check-$(date +%Y%m%d-%H%M%S)-$$.log}"

# **∴ 关键 ✗**：**`set -o pipefail` 已开 ⇒ **∴ 这里**不用任何管道 ⇒ **∴ 退出码**直达** ✓ ★★★
"$@" > "$log" 2>&1
rc=$?

lines=$(wc -l < "$log" | tr -d ' ')
echo "  ── run-check ──"
echo "  ⇒ **命令退出码 ＝ ${rc}**（**∴ 那**就是判据的结果**✗ 未经过任何管道** ✓）"
echo "  ⇒ 输出 ${lines} 行 ⇒ 日志 ${log}"
if [ "$rc" != "0" ]; then
  echo "  ⇒ **本题**红**（**${rc} ✓）—— **∴ 下面**是尾部**✗ 完整内容看日志** ✓"
else
  echo "  ⇒ 本题绿（**0 ✓）"
fi
echo "  ── 输出尾部（**最多 ${tail_lines} 行 ✓）──"
if [ "$lines" -gt "$tail_lines" ]; then
  echo "  （**∴ 前面**省略 $((lines - tail_lines)) 行**✗ 看日志：${log} ✓）"
fi
tail -n "$tail_lines" "$log" | sed 's/^/  │ /'

# **∴ 最后一行必须是**真实退出码****✗ ⇒ **∴ 调用方**用 `$?` 拿到的就是它** ✓
exit "$rc"
