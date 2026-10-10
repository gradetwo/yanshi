#!/usr/bin/env bash
# **★ 打印 `wasm-bindgen` 的**精确锁定版本**✗ ★**（**第 492 轮 ✓）
#
# **∴ 为什么需要它 ✗**（**2026-10-11 的一条真失败 ✓）：
#   **∴ `pwa.yml` 原来从 `crates/yanshi-wasm/Cargo.toml` 取版本**✗
#     ⇒ **∴ 而**那里写的是**范围** `wasm-bindgen = "0.2"`** ✓
#       ⇒ **∴ 于是**：**`cargo install wasm-bindgen-cli --version "0.2"`**✗
#         ⇒ **∴ cargo 拒绝**：`invalid value '0.2' for '--version': unexpected end of input`
#           ⇒ **∴ 那一步**必定失败**✗
#             ＋ **∴ 而**后面的「生成绑定（pkg/）」**从不执行** ✓
#               ⇒ **∴ 于是**：**PWA 的部署**根本没跑到** ✓ ★**** ✓✓
#
# **∴ 唯一权威来源是 `Cargo.lock` ✗**（**那里是**解析后的精确版本** ✓）
#   ＋ **∴ 而** `wasm-bindgen` CLI 的版本**必须与 crate 完全一致** ✗
#     （**∴ 否则**绑定不兼容 ⇒ 报 schema 版本不符 ✓）
#
# **∴ 用法 ✗**：
#   VER="$(scripts/wasm-bindgen-version.sh)"
#   cargo install wasm-bindgen-cli --version "$VER" --locked
#
# **∴ 失败语义 ✗**：**取不到 ⇒ **非零退出 ＋ 说清原因**✗
#   ⇒ **∴ 不许**回退到写死的版本** ✓（**∴ 否则**就是**下一次漂移** ✓）
#
# **∴ 两面（**AGENTS.md 第 3 条 ✓）★**：
#   **∴ 收益 ✗**：**版本**只有一个来源**✗ ⇒ **∴ 升级 crate 时**不用改工作流** ✓**** ✓✓
#     ＋ **∴ 且**：**所有调用点**不可能再漂移** ✓
#   **∴ 代价 ✗**：**多一个文件 ＋ 一次进程启动**✗（**∴ 可忽略 ✓）
#     ＋ **∴ 且**：**它**依赖 `Cargo.lock` 存在**✗ ⇒ **∴ 克隆缺 lock 时**会响亮失败** ✓**** ✓✓

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
lock="${repo_root}/Cargo.lock"

if [ ! -f "${lock}" ]; then
  echo "✗ 找不到 ${lock} ⇒ **无法确定 wasm-bindgen 的精确版本**" >&2
  echo "  （**不会**回退到写死的版本 ✗ —— 那正是下一次漂移 ✓）" >&2
  exit 1
fi

# **∴ 在 `Cargo.lock` 里找 `name = "wasm-bindgen"` 的下一行 `version` ✓**
#   ＋ **∴ 用 awk 而不是 sed**✗ ⇒ **∴ 逻辑**一眼可读 ＋ 不会贪婪** ✓
ver="$(awk '
  /^name = "wasm-bindgen"$/ { want = 1; next }
  want && /^version = "/ {
    line = $0
    sub(/^version = "/, "", line)
    sub(/"$/, "", line)
    print line
    exit
  }
' "${lock}")"

if [ -z "${ver}" ]; then
  echo "✗ ${lock} 里找不到 wasm-bindgen 的 version ⇒ **无法判断**" >&2
  exit 1
fi

# **∴ 只打印版本本身 ✗**（**∴ 便于 `$(…)` 直接用 ✓）
printf '%s\n' "${ver}"
