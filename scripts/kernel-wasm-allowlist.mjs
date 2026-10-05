#!/usr/bin/env node
// **内核里的 wasm 条件编译必须"白名单 + 理由"**判据（用户要求：两端尽量同一份代码 ✓，别让核心按目标分叉 ✗）。
//
// 背景（实测 ✓）：`yanshi-core` / `yanshi-render` 是**服务端与浏览器共用**的内核 ✓
// （`crates/yanshi-wasm` 就是依赖它们 + `Renderer` + `encode_png` ✓）。
// 一旦核心里出现 `cfg(target_arch = "wasm32")`，就存在"同一份代码在两个目标上行为不同"的风险 ✗ ——
// 而**像素必须一致** ✓（用户裁定：默认客户端渲染、服务端兜底 ✓ ⇒ 不一致就是画错 ✗）。
//
// 判据 ✓：内核 crate 里每一处 `cfg(target_arch = "wasm32")` 都必须**在白名单里** ✓，
// 且必须写明**为什么它不影响像素** ✓；出现新的（或白名单里那条已不存在）⇒ **判红** ✗。
//
// ⚠️ **白名单按"锚点"匹配，不按行号**（第 789 轮改 ✓）：
// 原先它记的是 `{file, line}` ✗ —— 而**代码一动行号就漂** ✗ ⇒ 于是同一处会被同时报成
// 「未登记」（新行号 ✓）与「白名单过期」（旧行号 ✓），**判据自己腐化** ✗。
// 现在每条登记的是**这个 cfg 之后的第一个非空、非注释行**里的一个片段 ✓
//（如 `mod stage_probe` / `fn object_probe_label` ✓）⇒ **行号漂移不再影响它** ✓。
//
// 已登记的 2 处（读代码确认 ✓，都是**诊断**、不参与像素 ✓）。
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

const ALLOWED = [
  { file: "crates/yanshi-render/src/render.rs", anchor: "mod stage_probe",
    why: "计时探针模块 stage_probe：wasm 上为空实现（只影响性能观测，不参与像素）" },
  { file: "crates/yanshi-render/src/render.rs", anchor: "fn object_probe_label",
    why: "调试标签 object_probe_label：wasm 上返回空串（只是日志/诊断字符串，不参与像素）" },
  { file: "crates/yanshi-render/src/render.rs", anchor: "mod parallel_impl",
    why: "并行分块渲染的执行器：wasm32 没有共享内存线程 ⇒ 该目标下 workers() 恒为 1、整条 std::thread 路径被 cfg 掉（不参与编译），回退是**纯串行**；并行只是把同一段 render_accumulation 分块，块的像素由该像素自己的输入决定 ⇒ 与串行逐字节一致（仅滑动窗口方框模糊有项目已批准的 ±1 LSB D1 例外）" },
];

const roots = ["crates/yanshi-core/src", "crates/yanshi-render/src"];
const hits = [];
for (const root of roots) {
  for (const entry of readdirSync(root, { withFileTypes: true })) {
    if (!entry.isFile() || !entry.name.endsWith(".rs")) continue;
    const path = join(root, entry.name);
    const lines = readFileSync(path, "utf8").split("\n");
    lines.forEach((text, index) => {
      if (!text.includes('cfg(target_arch = "wasm32")')) return;
      // **锚点 = 这个 cfg 之后的第一个"非空且非注释"行** ✓ ⇒ 与行号无关 ✓。
      let anchorLine = "";
      for (let k = index + 1; k < lines.length; k += 1) {
        const trimmed = lines[k].trim();
        if (trimmed && !trimmed.startsWith("//")) { anchorLine = trimmed; break; }
      }
      hits.push({ path, line: index + 1, text: text.trim(), anchorLine });
    });
  }
}

const matches = (entry, hit) => entry.file === hit.path && hit.anchorLine.includes(entry.anchor);
const unlisted = hits.filter((hit) => !ALLOWED.some((entry) => matches(entry, hit)));
const stale = ALLOWED.filter((entry) => !hits.some((hit) => matches(entry, hit)));

console.log(`  内核里 cfg(target_arch = "wasm32")：${hits.length} 处｜白名单登记：${ALLOWED.length} 处`);
for (const entry of ALLOWED) console.log(`     · ${entry.file} ⇒ ${entry.anchor}｜${entry.why}`);
for (const hit of unlisted) console.log(`     ✗ 未登记：${hit.path}:${hit.line} ⇒ ${hit.text}（其后是 ${JSON.stringify(hit.anchorLine)}）`);
for (const entry of stale) console.log(`     ✗ 白名单过期（已找不到锚点）：${entry.file} ⇒ ${entry.anchor}`);

if (unlisted.length || stale.length) {
  console.log("  结论：内核里的按目标分叉与白名单不一致 ✗ ——");
  console.log("        新出现的 ⇒ 要么消除它，要么登记并写明为什么不影响像素 ✓；");
  console.log("        白名单过期的 ⇒ 删掉那一条（否则白名单会变成谎话 ✗）。");
  process.exit(1);
}
console.log("  ✓ 内核里的按目标分叉全部已登记，且都写明不影响像素");
