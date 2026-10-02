#!/usr/bin/env bash
# wasm 运行时冒烟检查：把内核编译为 wasm32，用 node 真正**构造内核、渲染、并走一次写入路径**。
#
# 为什么需要它：`cargo test` 在宿主上跑，**原生全绿不等于浏览器可用**。
# 曾经有一个诊断探针在 wasm32 上调用了 `std::time::Instant::now()`（该目标不支持，直接 panic），
# 原生测试毫无反应，浏览器端却每次渲染都崩。本脚本把这类回归变成可自动执行的门禁。
#
# 覆盖两条路径：
#   1) 渲染路径：加载原子 → 区域渲染（RGBA + PNG）
#   2) 写入路径：乐观预览笔迹 → 提交（客户端由 JS 提供 id 与 timestamp，
#      因此 `yanshi_core::now_ms()` 不在 wasm 路径上；这条用例就是为盯住这件事）
#
# 依赖：rustup target `wasm32-unknown-unknown`、`wasm-bindgen-cli`、`node`。
# 用法：scripts/wasm-smoke.sh

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT

echo "== 构建 wasm32（release）"
# **挑一个带 wasm32 目标的 toolchain** ✓（本机实测：`rustup` 命令不在 PATH 上 ✓，
# 但 `~/.rustup/toolchains/*/` 里**装着** wasm32 目标 ✓ ⇒ 直接用那条工具链的 cargo ✓）。
#
# **为什么值得修** ✓：这台机器上原来这个脚本**根本跑不起来** ✗
#（系统 rust 只有 x86 标准库 ✓ ⇒ 报 `can't find crate for core` ✓）⇒
# **一个本机跑不动的守卫等于没有** ✗ ⇒ 它此前一直被跳过 ✓，而"被跳过的检查"最危险 ✓。
CARGO_BIN="$(command -v cargo)"
for toolchain in "$HOME"/.rustup/toolchains/*/; do
  if [ -d "${toolchain}lib/rustlib/wasm32-unknown-unknown" ] && [ -x "${toolchain}bin/cargo" ]; then
    # **`PATH` 也要跟着换** ✓ —— 只换 cargo 不够 ✗：它会去 PATH 里找 **rustc** ✓，
    # 而系统 rustc 没有 wasm 标准库 ✓ ⇒ 报 `can't find crate for core` ✗。
    #（我第一次就只换了 cargo ✓，症状一模一样 ✓。）
    PATH="${toolchain}bin:$PATH"
    export PATH
    CARGO_BIN="${toolchain}bin/cargo"
    break
  fi
done
"$CARGO_BIN" build --manifest-path "$ROOT/Cargo.toml" -p yanshi-wasm \
  --target wasm32-unknown-unknown --release >/dev/null

echo "== 生成 node 目标绑定"
# **预检：`wasm-bindgen` 是外部 CLI** ✓（不是 Rust 依赖 ✓，装不了就只能跳过 ✓）。
# **为什么要明说** ✓：原来的失败长这样 —— `wasm-bindgen: 未找到命令` ✗
# —— 那看起来像"脚本坏了" ✗，而真相是"**这台机器缺一个可选工具** ✓，
# 于是**内核这一段的冒烟检查没跑** ✓" ⇒ **被跳过的检查必须自己说清楚** ✗，
# 否则读日志的人会以为它通过了 ✓（这正是最危险的一种绿 ✓）。
# **`wasm-bindgen` 常装在 `~/.cargo/bin` 而不在 PATH 上** ✗ ——
# 实测：`command -v wasm-bindgen` 说"没有" ✓，其实**装着** ✓（同一个坑我在打包脚本里也踩过 ✓）。
bindgen="$(command -v wasm-bindgen 2>/dev/null || true)"
if [ -z "$bindgen" ] && [ -x "$HOME/.cargo/bin/wasm-bindgen" ]; then
  bindgen="$HOME/.cargo/bin/wasm-bindgen"
  PATH="$HOME/.cargo/bin:$PATH"
  export PATH
fi
if [ -z "$bindgen" ]; then
  echo "   ⚠️ 未安装 wasm-bindgen ⇒ **跳过内核冒烟检查**（不影响介质插件检查 ✓）"
  echo "      装上之后请重跑：cargo install wasm-bindgen-cli"
  echo "      注意：本脚本的**介质插件**部分由 scripts/medium-abi-check.mjs 独立覆盖 ✓"
  exit 0
fi
wasm-bindgen --target nodejs --out-dir "$OUT" --no-typescript \
  "$ROOT/target/wasm32-unknown-unknown/release/yanshi_wasm.wasm" >/dev/null

cat > "$OUT/smoke.mjs" <<'JS'
import { createRequire } from "node:module";
const require = createRequire(process.env.SMOKE_OUT + "/");
const wasm = require(process.env.SMOKE_OUT + "/yanshi_wasm.js");

const kernel = new wasm.WasmKernel("smoke", 256, 512, 512, 64 * 1024 * 1024);
kernel.load_atoms_json(JSON.stringify([
  { seq: 1, id: "01AAAAAAAAAAAAAAAAAAAAAAAA", kind: "create_document",
    actor: "human:1", session: "session:a", timestamp: 1,
    payload: { doc_id: "smoke", width: 512, height: 512,
               background: { r: 255, g: 255, b: 255, a: 255 } } },
  { seq: 2, id: "01BBBBBBBBBBBBBBBBBBBBBBBB", kind: "create_layer",
    actor: "human:1", session: "session:a", timestamp: 2,
    payload: { layer_id: "layer_1", name: "base" } },
  { seq: 3, id: "01CCCCCCCCCCCCCCCCCCCCCCCC", kind: "create_object",
    actor: "human:1", session: "session:a", timestamp: 3,
    payload: { object_id: "shape", layer_id: "layer_1", kind: "shape",
               data: { geometry: { kind: "rect", bbox: { x: 32, y: 32, w: 128, h: 128 } },
                       color: { r: 200, g: 80, b: 40, a: 255 } } } },
  { seq: 4, id: "01DDDDDDDDDDDDDDDDDDDDDDDD", kind: "create_object",
    actor: "human:1", session: "session:a", timestamp: 4,
    payload: { object_id: "grain", layer_id: "layer_1", kind: "filter",
               data: { filter_name: "film_grain", params: { amount: 0.4, size: 3, seed: 7 } } } }
]));

// 1) 渲染路径
const rgba = kernel.render_region_rgba(0, 0, 256, 256);
if (!rgba || rgba.length !== 256 * 256 * 4) {
  throw new Error("区域渲染返回异常长度：" + (rgba ? rgba.length : "null"));
}
const png = kernel.render_region_png(0, 0, 64, 64);
if (!png || png.length < 100) {
  throw new Error("PNG 输出异常：" + (png ? png.length : "null"));
}
console.log("  ✅ 渲染路径：RGBA " + rgba.length + " 字节，PNG " + png.length + " 字节");

// 2) 写入路径：预览 + 提交（id/timestamp 由调用方给出）
const dirty = kernel.extend_preview_stroke(JSON.stringify({
  geometry: { kind: "stroke", points: [[10, 10], [40, 30], [70, 20]] },
  brush: { size: 6, hardness: 0.8, color: { r: 10, g: 200, b: 90, a: 255 } }
}));
if (!dirty) {
  throw new Error("预览笔迹未返回脏区域");
}
const committed = JSON.parse(kernel.commit_preview(JSON.stringify({
  id: "01EEEEEEEEEEEEEEEEEEEEEEEE", seq: 5, timestamp: 5,
  kind: "draw_stroke", actor: "human:1", session: "session:web",
  payload: { layer_id: "layer_1", object_id: "stroke_1", size: 6,
             color: { r: 10, g: 200, b: 90, a: 255 },
             data: { points: [[10, 10], [40, 30], [70, 20]] } }
})));
if (!committed || committed.seq !== 5) {
  throw new Error("提交预览未返回预期 seq：" + JSON.stringify(committed));
}
const after = kernel.render_region_rgba(0, 0, 256, 256);
if (!after || after.length !== 256 * 256 * 4) {
  throw new Error("提交后渲染异常");
}
console.log("  ✅ 写入路径：预览 + 提交成功（seq " + committed.seq + "）");
JS

SMOKE_OUT="$OUT" node "$OUT/smoke.mjs"
echo "✅ wasm 冒烟检查通过（渲染路径 + 写入路径）"
