#!/usr/bin/env node
// **`smart_select` 的 `mode="add"` 必须**真的合并**（第 1292 轮；外部 `E1041` 的代价换来的判据）
//
// **⇒ 为什么有这条判据** ✗：
//   外部（apple2011）曾用「七次 `smart_select` 叠选区」（`mode="add"`）⇒
//   **每次都以为在累加** ⇒ **而实际只保留了最后一次** ⇒ 他因此放弃了多边形选区路线。
//   根因（第三轮复审 N3D-04）：`fold.rs:683` 只做「允许覆盖」，而真正的布尔合并
//   （`merge_selection_shapes`）**从未被调用** ⇒ 于是 `add` 静默覆盖。
//
// **⇒ 判据（外部建议的形状）**：两次 `add`，两个**不相交**的矩形
//   ⇒ 最终选区的 `canvas_coverage_ratio` 应 ≈ **两矩形面积之和**
//   ⇒ 若 ≈ 其中一个 ⇒ **覆盖 bug 复发** ⇒ 红。
//
// **⇒ 为什么用 `canvas_coverage_ratio` 而不是像素数**：它是**比值** ⇒ 跨机器稳定
//   （判据优先断结构量/相对量，不断绝对像素数）。
import { existsSync } from "node:fs";

const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-smart-select-add-merges.mjs <base> <doc> <token>");
  process.exit(3);
}

const call = async (name, args) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
  const reply = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(args),
    signal: AbortSignal.timeout(60_000),
  });
  return await reply.json();
};

const fails = [];
const check = (ok, msg) => { if (!ok) fails.push(msg); console.log(`  ${ok ? "✓" : "✗"} ${msg}`); };

// 先铺两块**离得很远**的纯色 ⇒ 用 `smart_select` 的魔棒各选一块
const docInfo = await call("get_document", {});
const W = Number(docInfo.width ?? 200), H = Number(docInfo.height ?? 150);
const layer = await call("create_layer", { layer_id: "L_add", name: "ADD", if_exists: "reuse" });
if (layer.ok !== true) {
  console.error("  ⊘ 不可判：建层失败");
  process.exit(3);
}
// 左上红块 40×40，右下蓝块 40×40（互不相交、颜色不同 ⇒ 魔棒各选一块）
const red = await call("fill_region", {
  layer_id: "L_add", shape: { type: "rect", x: 10, y: 10, w: 40, h: 40 },
  color: "#c81818", opacity: 1.0,
});
check(red.ok === true, `① 铺红块 ⇒ ok=${red.ok}`);
const blue = await call("fill_region", {
  layer_id: "L_add", shape: { type: "rect", x: W - 50, y: H - 50, w: 40, h: 40 },
  color: "#1818c8", opacity: 1.0,
});
check(blue.ok === true, `② 铺蓝块 ⇒ ok=${blue.ok}`);

// 第一次 add：选红块
const first = await call("smart_select", {
  selection_id: "S_add", mode: "add", seed: [30, 30],
  tolerance: 0.15, feather: 0,
});
check(first.ok === true, `③ 第一次 smart_select(mode=add, 红) ⇒ ok=${first.ok}｜${JSON.stringify(first.context?.detail ?? "").slice(0, 80)}`);
// **★ 必须读 `list_selections` 的 `area`，不能读 `canvas_coverage_ratio`** ✓（第 1301 轮实测）
//   **为什么**：`canvas_coverage_ratio` 反映的是**本次** mask 的覆盖比
//     ⇒ 而**合并后的选区**在**已存储的状态**里 ⇒ 所以那个比值第二次仍等于第一次
//     ⇒ 于是判据**永远红**（而产品其实是对的：`area 1600 → 3200`）✗
//   ⇒ **测量比被测对象更容易错**（AGENTS.md 那条）
const areaOf = async () => {
  const r = await call("list_selections", {});
  const hit = (r.selections ?? []).find((s) => s.selection_id === "S_add");
  return Number(hit?.area ?? 0);
};
const r1 = await areaOf();

// 第二次 add：选蓝块（**不相交** ⇒ 正确时应累加）
const second = await call("smart_select", {
  selection_id: "S_add", mode: "add", seed: [W - 30, H - 30],
  tolerance: 0.15, feather: 0,
});
check(second.ok === true, `④ 第二次 smart_select(mode=add, 蓝) ⇒ ok=${second.ok}`);
const r2 = await areaOf();

// **核心断言**：第二次之后应 ≈ 两次之和（≈ 2×r1，因为两块面积相同）
//   若 bug 复发（覆盖）⇒ r2 ≈ r1 ⇒ 比值 ≈ 1.0 ⇒ 红
const ratio = r1 > 0 ? r2 / r1 : 0;
check(
  ratio > 1.5,
  `⑤ mode=add 必须**累加**（第二次/第一次 = ${ratio.toFixed(2)}，应 > 1.5；` +
  `若 ≈ 1.0 ⇒ 覆盖 bug 复发）｜area1=${r1} area2=${r2}`,
);

if (fails.length) {
  console.log(`\n  ✗ ${fails.length} 处不符（smart_select 的 mode=add 必须合并）`);
  process.exit(1);
}
console.log("\n  ✓ smart_select 的 mode=add 真的累加（不是覆盖）");
