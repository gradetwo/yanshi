#!/usr/bin/env bash
# **★ 在 L4 上验证**「有 GPU 就打开有收益的功能」这一轮的改动 ✗ ★**（**第 246 轮 ✓）
#
# **∴ 为什么要有这个脚本 ✗**（**用户指令「反复做的事要造工具」✓）★**：
#   **∴ 本轮要证明三件事 ✗**，而**它们**都要真 GPU ＋ **串行**跑** ✓：
#     **∴ ①** 新判据（**第一帧的大滤镜**必须走 GPU ✓）**先绿** ✓
#     **∴ ②** **变异**（**把创建那一步拿掉 ⇒ **同一判据**必须红 ✓）⇒ **∴ 证明确实**有牙** ✓
#     **∴ ③** 高斯模糊的**提速基准**（**`--ignored` ＋ **release** ✗ 见那条测试的注释 ✓）✓
#   ＋ **∴ 而**手工做这三步**要在 ssh 里**拼一串带引号的命令**✗
#     ⇒ **∴ 那**正是我本会话踩过两次的坑**（**引号被 `printf %q` 拆坏 ✓）★
#       ⇒ **∴ 所以**：**脚本进仓库**✗ ⇒ **∴ L4 侧 `git reset --hard` 拿到它**✗
#         ⇒ **∴ 本地只发**一条不含引号的命令** ✓ ★★
#
# **∴ 用法 ✗**（**本地 ✓）**：
#   `bash scripts/l4.sh bg gpuenable "cd /content/yanshi && bash scripts/l4-check-gpu-enable.sh"`
#   ⇒ **∴ 然后** `scripts/l4.sh tail gpuenable` ／ `rc gpuenable` ✓
#
# **∴ 退出码 ✗**：0 三步都按预期｜**1 有一步不符预期**｜**3 前提不满足**（**没有仓库 ✓）★
set -uo pipefail

L4_DIR="${L4_DIR:-/content/yanshi}"
export PATH="${HOME}/.cargo/bin:${PATH}"
cd "${L4_DIR}" || { echo "✗ 没有 ${L4_DIR}" >&2; exit 3; }

git fetch -q origin && git checkout -q l4-sync && git reset -q --hard origin/l4-sync || {
  echo "✗ 同步失败" >&2
  exit 3
}
echo "== 版本 =="
git log --oneline -1
echo "== 适配器 =="
vulkaninfo 2>/dev/null | grep -E "deviceName|driverInfo" | head -3

echo ""
echo "== ① 新判据：第一帧的大滤镜必须走 GPU（**应当绿**）=="
if cargo test -p yanshi-render --features gpu --test gpu_blur_first_use -- --nocapture 2>&1 | tail -8; then
  echo "  ✓ ① 绿"
else
  echo "  ✗ ① 红 ⇒ 修复没生效（或本机没有适配器）" >&2
  exit 1
fi

echo ""
echo "== ② 变异：拿掉创建那一步 ⇒ **同一判据应当红**（**∴ 证明有牙**）=="
# **∴ 用 sed 做一处**最小**变异 ✗** ⇒ **∴ 并**打印它到底改了没有** ✓
#   （**∴ 若**模式没匹配上 ✗ ⇒ **∴ 变异**没发生**✗ ⇒ **∴ 那**一次的"绿"**毫无意义** ✓）★★
# **不用 `sed -i`**（第 1018 轮）：macOS 的 BSD sed 要求 `sed -i ''`，
#   而 GNU sed 要求 `sed -i`（无后缀）⇒ **两边不兼容** ✗
# ⇒ 所以改用「写临时文件再 mv」（两边都行）✓
_tmp="$(mktemp)"
if ! sed 's/if !crate::buffer::ensure_quantizer(crate::color::srgb_encode_table()) {/if false {/' \
  crates/yanshi-render/src/filter.rs > "$_tmp"; then
  echo "  ✗ sed 失败" >&2
  rm -f "$_tmp"
  exit 1
fi
mv "$_tmp" crates/yanshi-render/src/filter.rs
if grep -q "if false {" crates/yanshi-render/src/filter.rs; then
  echo "  ✓ 变异已落地（源码里现在有 'if false {'）"
else
  echo "  ✗ 变异没落地 ⇒ **∴ 模式与源码不匹配** ✗ ⇒ **∴ 本步**没有结论**" >&2
  git checkout -- crates/yanshi-render/src/filter.rs
  exit 1
fi
if cargo test -p yanshi-render --features gpu --test gpu_blur_first_use 2>&1 | tail -6; then
  echo "  ✗ ② 变异之后**仍然绿** ⇒ **∴ 本判据**没有牙** ✗" >&2
  git checkout -- crates/yanshi-render/src/filter.rs
  exit 1
fi
echo "  ✓ ② 变异之后变红（**∴ 有牙 ✓）"
git checkout -- crates/yanshi-render/src/filter.rs

echo ""
echo "== ③ 高斯模糊提速基准（**release ＋ --ignored**）=="
echo "   （**∴ 它**是 \`#[ignore]\` 的 ✗ ⇒ **∴ 手动跑**是唯一入口 ✓ —— 见那条测试的注释 ✓）"
# **∴ 窗口要**够宽**✗ ★**（**∴ 实测教训 ✓）：**那条基准**每个尺寸 × 每个 σ 各一行**✗
#   ⇒ **∴ 4 行 ＋ 汇总** ✓ ⇒ **∴ 我第一版写 `tail -16`**✗
#     ⇒ **∴ 于是**：**1080p 那两行**被我自己丢掉**✗ 而**日志里**再也找不到** ✓ ★★
#       ＋ **∴ 所以**：**取 `tail -40`**✗ ⇒ **∴ 两档**都在** ✓
cargo test -p yanshi-render --release --features gpu --test gpu_blur_parity -- --ignored --nocapture 2>&1 |
  tail -40

echo ""
echo "★ 三步完成：① 绿｜② 变异红｜③ 基准已打印 ★"
