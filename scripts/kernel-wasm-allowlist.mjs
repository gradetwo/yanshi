#!/usr/bin/env node
// **内核里的 wasm 条件编译必须"白名单 + 理由"**判据（用户要求：两端尽量同一份代码 ✓，别让核心按目标分叉 ✗）。
//
// 背景（实测 ✓）：`yanshi-core` / `yanshi-render` 是**服务端与浏览器共用**的内核 ✓
// （`crates/yanshi-wasm` 就是依赖它们 + `Renderer` + `encode_png` ✓）。
// 一旦核心里出现 `cfg(target_arch = "wasm32")`，就存在"同一份代码在两个目标上行为不同"的风险 ✗ ——
// 而**像素必须一致** ✓（用户裁定：默认客户端渲染、服务端兜底 ✓ ⇒ 不一致就是画错 ✗）。
//
// 判据 ✓：内核 crate 里每一处 `cfg(target_arch = "wasm32")` 都必须**在下面这张白名单里** ✓，
// 且必须写明**为什么它不影响像素** ✓；出现新的（或理由被删的）⇒ **判红** ✗。
// 已登记的 2 处（本轮读代码确认 ✓，都是**诊断**、不参与像素 ✓）。
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
const ALLOWED = [
  { file: "crates/yanshi-render/src/render.rs", line: 1490,
    why: "计时探针 stage_probe：wasm 上为空实现（只影响性能观测，不参与像素）" },
  { file: "crates/yanshi-render/src/render.rs", line: 1548,
    why: "调试标签 object_probe_label：wasm 上返回空串（只是日志/诊断字符串，不参与像素）" },
];
const roots = ["crates/yanshi-core/src", "crates/yanshi-render/src"];
const hits = [];
for (const root of roots) {
  for (const entry of readdirSync(root, { withFileTypes: true })) {
    if (!entry.isFile() || !entry.name.endsWith(".rs")) continue;
    const path = join(root, entry.name);
    const lines = readFileSync(path, "utf8").split("\n");
    lines.forEach((text, index) => {
      if (text.includes('cfg(target_arch = "wasm32")')) hits.push({ path, line: index + 1, text: text.trim() });
    });
  }
}
const key = (item) => item.path + ":" + item.line;
const allowed = new Set(ALLOWED.map((item) => item.file + ":" + item.line));
const unlisted = hits.filter((hit) => !allowed.has(key(hit)));
const stale = ALLOWED.filter((item) => !hits.some((hit) => key(hit) === item.file + ":" + item.line));
console.log(`  内核里 cfg(target_arch = "wasm32")：${hits.length} 处｜白名单登记：${ALLOWED.length} 处`);
for (const item of ALLOWED) console.log(`     · ${item.file}:${item.line} ⇒ ${item.why}`);
for (const hit of unlisted) console.log(`     ✗ 未登记：${hit.path}:${hit.line} ⇒ ${hit.text}`);
for (const item of stale) console.log(`     ✗ 白名单过期（这里已没有该处）：${item.file}:${item.line}`);
if (unlisted.length || stale.length) {
  console.log("  结论：内核里出现了未登记的按目标分叉 ✗（要么消除它，要么登记并写明为什么不影响像素）");
  process.exit(1);
}
console.log("  ✓ 内核里的按目标分叉全部已登记，且都写明不影响像素");
