#!/usr/bin/env bash
# wasm 运行时冒烟检查：把内核编译为 wasm32，用 node 真正**构造内核、渲染、并走一次写入路径**。
#
# 为什么需要它：`cargo test` 在宿主上跑，**原生全绿不等于浏览器可用**。
# 曾经有一个诊断探针在 wasm32 上调用了 `std::time::Instant::now()`（该目标不支持，直接 panic），
# 原生测试毫无反应，浏览器端却每次渲染都崩。本脚本把这类回归变成可自动执行的门禁。
#
# 覆盖三条路径：
#   1) **字节比对**：两端读**同一份场景文件**（`crates/yanshi-wasm/tests/data/wasm_smoke_scene.json`），
#      node 里的 wasm 内核与宿主上的原生内核渲染**同一块区域** ⇒ 各算 RGBA/PNG 的 sha256 ⇒
#      脚本比较。两端字节不同 ⇒ **非零退出并打印两个摘要** ✓。
#      **期望值不写死** ✓：它由这次运行的原生侧现算 ✓（写死一份观测值等于把"某次的样子"当判据 ✗）。
#      **取不到原生摘要也判红** ✓（"原生一侧缺失"不能静默通过 ✗）。
#   2) 渲染路径：加载原子 → 区域渲染（RGBA + PNG）的形状检查
#   3) 写入路径：乐观预览笔迹 → 提交（客户端由 JS 提供 id 与 timestamp，
#      因此 `yanshi_core::now_ms()` 不在 wasm 路径上；这条用例就是为盯住这件事）
#
# **这个比对覆盖什么、不覆盖什么**：它覆盖"同一份输入下，wasm 与原生**这个宿主**产出的
# RGBA/PNG 字节相同" ✓；它**不覆盖**真实浏览器（DOM/canvas/`ImageData`/传输）✗，
# 也不覆盖其它架构的原生（跨平台另有 `parity-arm64` 作业 ✓）。
#
# 依赖：rustup target `wasm32-unknown-unknown`、`wasm-bindgen-cli`、`node`。
# 用法：scripts/wasm-smoke.sh

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT

# **目标目录跟着 `CARGO_TARGET_DIR` 走** ✓：原来这里写死 `$ROOT/target` ✗ ⇒
# 独立 target 目录（本仓库的纪律，见实现笔记）下 `wasm-bindgen` 会找不到刚编出来的 .wasm ✗。
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"

# **两端共用的同一份输入** ✓：脚本把**这个路径**同时交给 node（`SMOKE_SCENE`）与
# 原生测试（`YANSHI_WASM_PARITY_SCENE`）✓ ⇒ 不存在"两份输入各自漂移" ✗。
SCENE="$ROOT/crates/yanshi-wasm/tests/data/wasm_smoke_scene.json"

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
  # **跳过 ≠ 通过** ✓：这里显式打出 `SKIP` ✓（读日志的人不该把"没跑"当"绿" ✗）。
  echo "   ⚠️ SKIP：未安装 wasm-bindgen ⇒ **跳过内核冒烟检查与字节比对**（都不算通过 ✗；不影响介质插件检查 ✓）"
  echo "      装上之后请重跑：cargo install wasm-bindgen-cli"
  echo "      注意：本脚本的**介质插件**部分由 scripts/medium-abi-check.mjs 独立覆盖 ✓"
  exit 0
fi
wasm-bindgen --target nodejs --out-dir "$OUT" --no-typescript \
  "$TARGET_DIR/wasm32-unknown-unknown/release/yanshi_wasm.wasm" >/dev/null

cat > "$OUT/smoke.mjs" <<'JS'
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { createHash } from "node:crypto";

const require = createRequire(process.env.SMOKE_OUT + "/");
const wasm = require(process.env.SMOKE_OUT + "/yanshi_wasm.js");

// **两端共用的同一份输入** ✓（脚本把同一个路径也交给原生测试 ✓）。
const scene = JSON.parse(readFileSync(process.env.SMOKE_SCENE, "utf8"));
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

const kernel = new wasm.WasmKernel(
  scene.doc_id, scene.tile_size, scene.width, scene.height, scene.memory_limit);
kernel.load_atoms_json(JSON.stringify(scene.atoms));

// 1) **字节比对**：与原生侧**完全同序**的两次渲染 ⇒ 两个可解析的摘要。
//    摘要在这里现算 ⇒ 不需要（也不许）写死期望值 ✓。
const region = scene.render;
const rgba = kernel.render_region_rgba(region.x, region.y, region.w, region.h);
if (!rgba || rgba.length !== region.w * region.h * 4) {
  throw new Error("区域渲染返回异常长度：" + (rgba ? rgba.length : "null"));
}
const png = kernel.render_region_png(region.x, region.y, region.w, region.h);
if (!png || png.length < 100) {
  throw new Error("PNG 输出异常：" + (png ? png.length : "null"));
}
console.log("YANSHI_PARITY_WASM_RGBA=" + sha256(rgba));
console.log("YANSHI_PARITY_WASM_PNG=" + sha256(png));

// 2) 渲染路径（形状检查，不参与比对）：更大区域必须返回正确长度。
const wide = kernel.render_region_rgba(0, 0, 256, 256);
if (!wide || wide.length !== 256 * 256 * 4) {
  throw new Error("256×256 区域渲染返回异常长度：" + (wide ? wide.length : "null"));
}
console.log("  ✅ 渲染路径：RGBA " + rgba.length + " 字节，PNG " + png.length + " 字节（256² 亦正常）");

// 3) 写入路径：预览 + 提交（id/timestamp 由调用方给出）
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

echo "== node 里跑 wasm 内核"
if ! node_out="$(SMOKE_OUT="$OUT" SMOKE_SCENE="$SCENE" node "$OUT/smoke.mjs" 2>&1)"; then
  echo "❌ wasm 侧运行失败 ⇒ 判据不成立（**不是通过**）"
  printf '%s\n' "$node_out"
  exit 1
fi
printf '%s\n' "$node_out"

wasm_rgba="$(printf '%s\n' "$node_out" | sed -n 's/^YANSHI_PARITY_WASM_RGBA=//p' | tail -n 1)"
wasm_png="$(printf '%s\n' "$node_out" | sed -n 's/^YANSHI_PARITY_WASM_PNG=//p' | tail -n 1)"
if [ -z "$wasm_rgba" ] || [ -z "$wasm_png" ]; then
  echo "❌ wasm 侧没有打印摘要 ⇒ 判据不成立（**不是通过**）"
  exit 1
fi

echo "== 原生侧对同一份输入算摘要"
# **真的去跑原生代码** ✓（不是抄一个数 ✓）：同一个场景文件 `SCENE` ✓、
# 同一串原子 ✓、同一块区域 ✓，由 `crates/yanshi-wasm/tests/wasm_native_parity.rs` 现算并打印 ✓。
if ! native_out="$("$CARGO_BIN" test --manifest-path "$ROOT/Cargo.toml" -p yanshi-wasm \
      --test wasm_native_parity -- --nocapture 2>&1)"; then
  echo "❌ 原生侧摘要无法取得：cargo test 失败 ⇒ 判据不成立（**不是通过**）"
  printf '%s\n' "$native_out"
  exit 1
fi

native_rgba="$(printf '%s\n' "$native_out" | sed -n 's/^YANSHI_PARITY_NATIVE_RGBA=//p' | tail -n 1)"
native_png="$(printf '%s\n' "$native_out" | sed -n 's/^YANSHI_PARITY_NATIVE_PNG=//p' | tail -n 1)"
if [ -z "$native_rgba" ] || [ -z "$native_png" ]; then
  # **"缺一侧"必须判红** ✓：取不到原生摘要时**不能**当成通过 ✗。
  echo "❌ 原生侧没有给出摘要（取不到原生一侧）⇒ 判据不成立（**不是通过**）"
  printf '%s\n' "$native_out"
  exit 1
fi

echo "== 逐字节比对（同一份输入：${SCENE}）"
echo "   RGBA  原生（host）=$native_rgba"
echo "   RGBA  wasm        =$wasm_rgba"
echo "   PNG   原生（host）=$native_png"
echo "   PNG   wasm        =$wasm_png"
if [ "$native_rgba" != "$wasm_rgba" ] || [ "$native_png" != "$wasm_png" ]; then
  echo "❌ wasm 与原生输出字节不一致 ⇒ 判据红"
  exit 1
fi

echo "✅ wasm 冒烟检查通过（渲染路径 + 写入路径 + 原生/wasm 字节一致）"
