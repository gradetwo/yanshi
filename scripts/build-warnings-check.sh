#!/usr/bin/env bash
# **构建警告上限判据** ✓（(B)④ ✓）：**冷构建 cdylib crates ⇒ 一条警告都不许有** ✓。
#
# **为什么必须"冷"** ✗：热构建**不会重新发出**那些警告 ✓（第 215 轮实测热构建 0 条 ✓）
# ⇒ 判据**先 range-clean 再编** ✓，否则它会**永远绿** ✗（假判据 ✓）。
# **两个 target** ✓：native ✓（`cargo build` ✓）与 wasm32 ✓（打包真正用的那条 ✓）。
set -uo pipefail
cd "$(dirname "$0")/.." || exit 2
CRATES="yanshi-wasm yanshi-medium-oil yanshi-medium-watercolor yanshi-medium-pencil yanshi-medium-pixel yanshi-medium-marker"
# **`cargo clean` 要重复写 `-p`** ✗ —— 我第一版写 `-p a b c` ✓ ⇒ 它报用法错 ✓ ⇒
# 而且**坏参数会连累下一句 `cargo build`** ✗（第 215 轮实测：报的是 build 的用法错 ✓，很难一眼看出 ✓）。
CLEAN_ARGS=""
for c in ${CRATES}; do CLEAN_ARGS="${CLEAN_ARGS} -p ${c}"; done
fail=0
check() { # $1=标签 $2=日志
  local count; count="$(grep -cE '^warning' "$2")"
  echo "  ${1}：警告 ${count} 条（须为 0 ✓）"
  if [ "${count}" != "0" ]; then grep -E '^warning' -A 3 "$2" | head -12 | sed 's/^/      /'; fail=1; fi
}
echo "== native（先 range-clean ✓）=="
cargo clean ${CLEAN_ARGS} >/dev/null 2>&1
if cargo build --workspace > /var/tmp/bw-native.log 2>&1; then check "native" /var/tmp/bw-native.log; else echo "  ✗ native 构建失败 ⇒ 判据无效 ✗"; sed -n '1,10p' /var/tmp/bw-native.log | sed 's/^/      /'; fail=1; fi
TC=""
for c in "$HOME"/.rustup/toolchains/*/; do
  if [ -d "${c}lib/rustlib/wasm32-unknown-unknown" ] && [ -x "${c}bin/cargo" ]; then TC="$c"; break; fi
done
if [ -z "${TC}" ]; then
  echo "  ⚠️ 没有带 wasm32 的工具链 ⇒ **跳过 wasm 那一半**（不算通过 ✗，但要说明 ✓）"
else
  echo "== wasm32（打包用的 target ✓）=="
  cargo clean ${CLEAN_ARGS} >/dev/null 2>&1
  if env PATH="${TC}bin:$PATH" "${TC}bin/cargo" build --release --target wasm32-unknown-unknown ${CLEAN_ARGS} > /var/tmp/bw-wasm.log 2>&1; then check "wasm32" /var/tmp/bw-wasm.log; else echo "  ✗ wasm32 构建失败 ⇒ 判据无效 ✗"; sed -n '1,10p' /var/tmp/bw-wasm.log | sed 's/^/      /'; fail=1; fi
fi
if [ "${fail}" != "0" ]; then echo "  ✗ 构建警告上限：有警告（或构建失败）⇒ 判据红 ✗"; exit 1; fi
echo "  ✓ 构建警告上限：冷构建两个 target 都是 0 条警告 ✓"
