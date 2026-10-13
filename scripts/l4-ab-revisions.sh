#!/usr/bin/env bash
# **★ 在 L4 上对**两个修订**做 A/B 的两本账 ✗ ★**（**第 246 轮 ✓）
#
# **∴ 它解决什么 ✗**（**待办 2a ✓）★**：
#   **∴ 待办 ✗**：**「**在 L4 上量**修复忙等**前后的 CPU 占用** ✓」
#     ＋ **∴ 而**手工做法要**重复五步**✗（**检出／构建／杀旧服务／量／并排报告 ✓）
#       ⇒ **∴ 每次**都容易漏一步**（**如**忘了 `pkill -9 -x` ⇒ **∴ 量到别人的服务 ✓）★
#   **∴ 所以 ✗**：**把五步固化**✗ ⇒ **∴ 于是**：**两个修订号一条命令** ✓ ★★
#
# **∴ 判定口径（**用户已裁定 ✓）✗**：**时间账**只在**独显**上算数**✗ ⇒ **∴ 而**核显**只报 CPU 占用** ✓
#   ＋ **∴ 所以**本脚本**不打分**✗：**它只**把两份账**并排打出**✗ ⇒ **∴ 结论**由人（**或 report-accounts ✓）下 ✓ ★
#     ＋ **∴ 而**它**强制打印**适配器名**✗ ⇒ **∴ 于是**：**llvmpipe**冒充 L4**能被一眼看出** ✓ ★★
#
# **∴ 用法 ✗**（**在 L4 上跑 ✓；本地用 `scripts/l4.sh bg ab 'bash scripts/l4-ab-revisions.sh …'` ✓）**：
#   bash scripts/l4-ab-revisions.sh <修订A> <修订B> [轮数=3] [区域=3840x2160]
#   ⇒ **∴ 输出 ✗**：`${OUT_DIR:-/content/yanshi-ab}/<修订>.log` ＋ 末尾一次并排报告 ✓
#
# **∴ 退出码 ✗**：0 两份账都拿到了｜1 构建或测量失败｜**3 前提不满足**（**没有 gpu feature／检出失败 ✓）★
#
# **∴ 两面（**AGENTS.md 第 3 条 ✓）★**：
#   **∴ 收益 ✗**：**一次 A/B** 不可能漏 `pkill`**✗ ＋ **∴ 且**两份账**落盘**⇒ **∴ 可用
#     `report-accounts.mjs` 复核** ✓ ★
#   **∴ 代价 ✗**：**它**要**两次 release 构建**✗（**L4 上数分钟 ✓）
#     ＋ **∴ 且**它**改 L4 工作区的 HEAD**✗（**跑完停在 B 的检出上 ✓）
#       ⇒ **∴ 要回到分支**得**自己 `git checkout`** ✓（**∴ 本脚本**不替你决定停在哪个修订 ✓）★

set -uo pipefail

# **★★★ 先把自己**拷到 /tmp**再跑 ✗ ★★★**（**∴ 否则**本脚本会被自己**删掉** ✓）
#
# **∴ 为什么 ✗**（**∴ 这是我自己会踩的坑 ✓）★**：
#   **∴ 本脚本**要 `git checkout <修订>`**✗ ⇒ **∴ 而**它**自己就在仓库里** ✓
#     ⇒ **∴ 若**那个修订里**还没有本文件**✗ ⇒ **∴ 检出**会把脚本从工作区**删掉** ✓
#       ＋ **∴ 而** bash **是**边读边执行**✗（**不是**先整份读进内存 ✓）
#         ⇒ **∴ 于是**：**后面的行**可能读不到了 ⇒ **∴ 行为**随 inode 时机而变 ✓ ★★
#   **∴ 所以 ✗**：**先复制到一个**不随仓库变化的路径**✗ ⇒ **∴ 在那儿**再 exec** ✓ ★
if [ "${YANSHI_AB_SELF:-}" != "1" ]; then
  cp "$0" /tmp/yanshi-ab-revisions.sh || { echo "✗ 无法把自己拷到 /tmp" >&2; exit 3; }
  YANSHI_AB_SELF=1 exec bash /tmp/yanshi-ab-revisions.sh "$@"
fi

A="${1:-}"; B="${2:-}"
rounds="${3:-3}"
region="${4:-3840x2160}"
[ -n "$A" ] && [ -n "$B" ] || { echo "用法：bash scripts/l4-ab-revisions.sh <修订A> <修订B> [轮数] [区域]" >&2; exit 3; }

L4_DIR="${L4_DIR:-/content/yanshi}"
OUT_DIR="${OUT_DIR:-/content/yanshi-ab}"
# **∴ cargo 在非交互 ssh 里**不在 PATH 上**✗ ⇒ **∴ 必须自己加 ✓（**l4.sh 的教训 ✓）★
export PATH="$HOME/.cargo/bin:$PATH"
mkdir -p "$OUT_DIR"

# **∴ 适配器名先打 ✗**（**∴ 否则**把 llvmpipe 的账当 L4 的 ✓）★
echo "== 适配器 =="
vulkaninfo 2>/dev/null | grep -E "deviceName|driverInfo" | head -4 || echo "  ⚠ vulkaninfo 不可用 ⇒ 无法证明是独显"

for rev in "$A" "$B"; do
  cd "$L4_DIR" || { echo "✗ 没有 $L4_DIR" >&2; exit 3; }
  echo "== 检出 $rev =="
  git fetch -q origin || { echo "✗ git fetch 失败" >&2; exit 3; }
  git checkout -q "$rev" || { echo "✗ 检出 $rev 失败（**∴ 该修订**没推上来？✓）" >&2; exit 3; }
  git log --oneline -1
  echo "== 构建 $rev（release ＋ gpu feature）=="
  # **∴ 构建失败 ⇒ 立刻退 ✗**（**∴ 不要**拿着旧二进制去量**✗ —— 那会**把两个修订量成同一个** ✓）★
  if ! cargo build --release -p yanshi-http --bin yanshi-serve --features gpu; then
    echo "✗ $rev 构建失败 ⇒ ∴ 不许用旧二进制继续（那会把两个修订量成同一个 ✗）" >&2
    exit 1
  fi
  # **∴ 每个 --spawn 作业开头都会 `pkill -9 -x yanshi-serve` ✗**
  #   ⇒ **∴ 这里先杀一次** ✗ ⇒ **∴ 于是**上一次的服务不会**被算进这一次的 CPU 账** ✓ ★★
  pkill -9 -x yanshi-serve 2>/dev/null
  sleep 1
  echo "== 测量 $rev（轮数 $rounds｜区域 $region）=="
  node scripts/tool-render-cost-accounts.mjs --spawn --rounds "$rounds" --region "$region" \
    --baseline target/release/yanshi-serve \
    --compare 'CPU=--gpu off' --compare 'GPU=--gpu on' 2>&1 | tee "$OUT_DIR/$rev.log"
  # **∴ 管道 ＋ `set -o pipefail` ✗** ⇒ **∴ 测量的失败**不会被 `tee` 吞掉 ✓ ★
  [ "${PIPESTATUS[0]}" = 0 ] || { echo "✗ $rev 测量失败" >&2; exit 1; }
done

echo ""
echo "== 并排报告（**墙钟第一列 ＋ 自动标出假收益 ✓）=="
node "$L4_DIR/scripts/report-accounts.mjs" \
  --label "A($A)=$OUT_DIR/$A.log" --label "B($B)=$OUT_DIR/$B.log"
