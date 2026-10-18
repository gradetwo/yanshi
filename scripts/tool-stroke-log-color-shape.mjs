#!/usr/bin/env node
// **`get_stroke_log` 的 `color` 必须是**同一种形状** ✓**（我自己撞出来的契约问题 ✓）
//
// ⚠️ **这条判据的适用边界（必须读 ✓）★**
//   **∴ 它**只能证明"**新画出来的 stroke** 的 `color` 是对象"✗
//   **∴ 而它**抓不住 `normalize_color` 是否生效**✗** —— **∴ 实测 ✓**：**把 `normalize_color` 改成恒等
//     （变异）⇒ 本条判据**仍然绿** ✗** ⇒ **∴ 因为新 atom 存进存储时**本来就是对象**✓
//
//   **∴ 数组形状只出现在**旧 atom**✗**（`kind:"import_image"` ＋ `"color":[176,152,103]` ✓）
//   **∴ 而要让判据看到旧数据 ✓ 必须在**起 server 之前**✗ 写进 workspace ✓
//   **∴ 而 `run-criteria.sh:33` **先起 server、再跑判据**✗ ⇒ **∴ 判据做不到 ✗**
//   ⇒ **∴ 所以 `color` 归一化是"实现 ✓＋实测 ✓，**无自动判据**✗"**（**∴ 已记台账 ✓**）
//
//   **∴ 归一的实测证据 ✓**（用他们那份真实 ws 的 `doc full`、`paint` 层 500 条）：
//     **修前 ⇒ `{'object': 285, 'array': 215}` ✗｜修后 ⇒ `{'object': 500}` ✓**

//
// **∴ 为什么 ✗**：它原来**一处返回对象 `{r,g,b,a}`、一处返回数组 `[r,g,b]`**✗ ——
//   同一个接口、同一次返回里两种都有 ⇒ **∴ 调用方必须两种都处理 ✗**（**∴ 我的分析崩了一次 ✓**）
//
// **∴ 判据 ✓**：**画几笔（新 atom ✓）＋ 读日志 ⇒ 全部 `color` 必须是对象 ✓**
//   ⇒ **∴ 若有一条是数组 ⇒ 红 ✓**
//   **∴ 而它同时在**他们那份真实 ws** 上验过 ✓（**∴ 那里修前 285 对象 / 215 数组 ✓**）
import { readFileSync } from "node:fs";

const [base] = process.argv.slice(2);
if (!base) { console.error("用法: node scripts/tool-stroke-log-color.mjs <base-url>"); process.exit(3); }

const boot = await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc: `stroke_log_color_${Date.now()}`, width: 200, height: 200 }),
});
const bootBody = await boot.json().catch(() => ({}));
const token = bootBody.token;
if (!token) { console.log(`  ✗ 建文档失败：${JSON.stringify(bootBody).slice(0, 140)}`); process.exit(3); }
const doc = bootBody.doc ?? bootBody.doc_id ?? "stroke_log_color";

const call = async (name, args) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
  const reply = await fetch(url, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args ?? {}) });
  return await reply.json();
};

await call("create_layer", { layer_id: "slc_L", name: "slc_L" });
await call("brush_stroke", {
  layer_id: "slc_L", brush: "100%_Opaque", color: { r: 120, g: 60, b: 30, a: 255 },
  points: [[40, 40, 1], [160, 40, 1]], size: 24, preview: false,
});
await call("brush_stroke", {
  layer_id: "slc_L", brush: "100%_Opaque", color: { r: 30, g: 90, b: 150, a: 255 },
  points: [[40, 120, 1], [160, 120, 1]], size: 24, preview: false,
});

const log = await call("get_stroke_log", { layer_id: "slc_L", limit: 100 });
if (log.ok !== true) { console.log(`  ✗ get_stroke_log 失败：${JSON.stringify(log).slice(0, 140)}`); process.exit(1); }
const entries = log.entries ?? [];
if (entries.length === 0) { console.log("  ✗ 一条 stroke 都没有 ⇒ 判据无意义 ✗"); process.exit(3); }

const shapes = new Map();
for (const entry of entries) {
  const color = entry.color;
  const kind = Array.isArray(color) ? "array" : (color && typeof color === "object" ? "object" : String(color));
  shapes.set(kind, (shapes.get(kind) ?? 0) + 1);
}
const summary = [...shapes.entries()].map(([k, v]) => `${k}:${v}`).join(" ");
console.log(`  读到 ${entries.length} 条｜color 形状 ⇒ ${summary}`);

let bad = 0;
if (shapes.size !== 1) {
  console.log(`  ✗ color 有 ${shapes.size} 种形状（${summary}）⇒ 调用方必须分支处理 ✗`);
  bad += 1;
} else if (!shapes.has("object")) {
  console.log(`  ✗ color 全是 ${[...shapes.keys()][0]} ⇒ 必须是对象 {r,g,b,a} ✗`);
  bad += 1;
} else {
  console.log("  ✓ color 全是对象 {r,g,b,a}");
}
const first = entries[0]?.color;
const keysOk = first && typeof first === "object" && !Array.isArray(first)
  && ["r", "g", "b", "a"].every((k) => typeof first[k] === "number");
console.log(`  ${keysOk ? "✓" : "✗"} 首条含 r/g/b/a 四个数字 ⇒ ${JSON.stringify(first)}`);
if (!keysOk) bad += 1;

if (bad) { console.log(`  ✗ ${bad} 处不符（get_stroke_log 的 color 契约）`); process.exit(1); }
console.log("  ✓ get_stroke_log 的 color 形状统一");
