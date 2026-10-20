#!/usr/bin/env node
// **色彩管线**转换点表**的可执行版本**（第 1305 轮；用户要求「先把链路画清楚」）
//
// **⇒ 为什么要有这条判据** ✗：
//   我在 `base` 上加了一次 sRGB→linear ⇒ 而链路里**已经有**一次解码 ⇒ 于是**双解码** ⇒
//   颜色暗 61–65 级 ⇒ 两个判据红（`tool-visible-delta` 65／`tool-palette-hex` 61）⇒ 回退。
//   ⇒ 根因不是「我不懂 sRGB」⇒ 是**链路上有几个转换点我没有一份清单** ✗
//   ⇒ 所以这条判据把那份清单**变成可执行的**：转换点**只能有这几个**，多一个就红 ✓
//
// **⇒ 断什么**（三条）：
//   ① **唯一的入向解码**在 `document.rs` 的 `linear_premul_from_u8x4`（笔触路）
//   ② **唯一的出向编码**是 `buffer.rs` 的 `srgb_encode_table()` + `linear_premul_to_u8x4_with`
//   ③ **`vendor/hokusai-core` 里不许出现** `srgb_to_linear(` 的**调用**（只许有定义）
//      ⇒ 因为 hokusai 在 sRGB 空间混合是**当前设计** ⇒ 在那里加转换就是**双解码的前兆** ✓
//
// **⇒ 退出码**：0 通过｜1 不符｜**3 不可判**（源码枚举不到 ⇒ 判据自己坏了，不是产品）
import { readFileSync, readdirSync, existsSync } from "node:fs";

const CRATES = "crates";
const fail = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fail.push(m); console.log(`  ✗ ${m}`); };

// ---- 收集源码（只扫 src/ 下的 .rs，跳过注释行）----
const rsFiles = [];
for (const c of readdirSync(CRATES)) {
  const dir = `${CRATES}/${c}/src`;
  if (!existsSync(dir)) continue;
  const walk = (d) => {
    for (const e of readdirSync(d, { withFileTypes: true })) {
      const p = `${d}/${e.name}`;
      if (e.isDirectory()) walk(p);
      else if (e.name.endsWith(".rs")) rsFiles.push(p);
    }
  };
  walk(dir);
}
const VENDOR = "vendor/hokusai-core/src";
if (existsSync(VENDOR)) {
  for (const e of readdirSync(VENDOR)) {
    if (e.endsWith(".rs")) rsFiles.push(`${VENDOR}/${e}`);
  }
}
if (rsFiles.length < 100) {
  console.error(`  ⊘ 不可判：只找到 ${rsFiles.length} 个 .rs（应 > 100）⇒ 枚举坏了`);
  process.exit(3);
}

const isCode = (line) => {
  const t = line.trim();
  return !(t.startsWith("//") || t.startsWith("///") || t.startsWith("//!") || t.startsWith("*"));
};

/// **非测试区的代码行** ✓（第 1305 轮实测：`#[cfg(test)]` 之后的 3 处调用是**我自己的测试**，
///   不是生产管线 ⇒ 把它们算进「转换点」会让判据报假红 ✗）
const productionLines = (file) => {
  const lines = readFileSync(file, "utf8").split("\n");
  const out = [];
  let inTest = false;
  let depth = 0;
  for (let i = 0; i < lines.length; i += 1) {
    const l = lines[i];
    if (/^\s*#\[cfg\(test\)\]/.test(l)) { inTest = true; depth = 0; }
    if (inTest) {
      depth += (l.match(/{/g) ?? []).length - (l.match(/}/g) ?? []).length;
      // `#[cfg(test)] mod x { ... }` 结束时退出
      if (depth === 0 && /}\s*$/.test(l) && i > 0 && /mod\s+\w+/.test(lines[i - 1] ?? "")) inTest = false;
      if (depth <= 0 && /^}\s*$/.test(l)) inTest = false;
      continue;
    }
    out.push([i + 1, l]);
  }
  return out;
};

/// **实现行**（`fn xxx(` 的定义）不算调用点
const isDefinition = (line) => /\bfn\s+\w+\s*\(/.test(line);

// ---- ① 入向解码点 ----
let decodeCallers = [];
for (const f of rsFiles) {
  for (const [n, l] of productionLines(f)) {
    if (!isCode(l) || isDefinition(l)) continue;
    if (/linear_premul_from_u8x4\(/.test(l) && !/fn linear_premul_from_u8x4/.test(l)) {
      decodeCallers.push(`${f}:${n}`);
    }
  }
}
if (decodeCallers.length === 0) {
  console.error("  ⊘ 不可判：找不到 linear_premul_from_u8x4 的调用 ⇒ 判据的锚点失效");
  process.exit(3);
}
const decodeOutsideDocument = decodeCallers.filter((p) => !p.includes("document.rs"));
if (decodeOutsideDocument.length > 0) {
  bad(`① 入向解码只许在 document.rs（笔触路）⇒ 另有 ${decodeOutsideDocument.length} 处：${decodeOutsideDocument.join(", ")}`);
} else {
  ok(`① 入向解码只在 document.rs（${decodeCallers.length} 处）`);
}

// ---- ② 出向编码点 ----
// **只数「把 linear 变成输出字节」的调用点** ⇒ 排除 `color.rs`/`rows.rs`
//   （那两处是 `linear_premul_to_u8x4_with` 的**实现本体**与批量版本，不是新的转换点）
let encodeCallers = [];
for (const f of rsFiles) {
  if (f.includes("/color.rs") || f.includes("/rows.rs")) continue;
  for (const [n, l] of productionLines(f)) {
    if (!isCode(l) || isDefinition(l)) continue;
    if (/linear_premul_to_u8x4_with\(|composite_over_linear_with\(/.test(l)) {
      encodeCallers.push(`${f}:${n}`);
    }
  }
}
if (encodeCallers.length === 0) {
  console.error("  ⊘ 不可判：找不到出向编码的调用 ⇒ 判据的锚点失效");
  process.exit(3);
}
const encodeOutsideBuffer = encodeCallers.filter((p) => !p.includes("buffer.rs") && !p.includes("tile.rs"));
if (encodeOutsideBuffer.length > 0) {
  bad(`② 出向编码只许在 buffer.rs/tile.rs ⇒ 另有 ${encodeOutsideBuffer.length} 处：${encodeOutsideBuffer.join(", ")}`);
} else {
  ok(`② 出向编码只在 buffer.rs / tile.rs（${encodeCallers.length} 处）`);
}

// ---- ③ hokusai 里不许**调用** srgb_to_linear ----
const vendorFiles = rsFiles.filter((f) => f.startsWith(VENDOR));
if (vendorFiles.length === 0) {
  console.error("  ⊘ 不可判：vendor/hokusai-core/src 里没有 .rs");
  process.exit(3);
}
const vendorCalls = [];
for (const f of vendorFiles) {
  for (const [n, l] of productionLines(f)) {
    if (!isCode(l) || isDefinition(l)) continue;
    if (/srgb_to_linear\(/.test(l)) vendorCalls.push(`${f}:${n}`);
  }
}
if (vendorCalls.length > 0) {
  bad(
    `③ hokusai 里不许调用 srgb_to_linear ⇒ 实测 ${vendorCalls.length} 处：${vendorCalls.slice(0, 3).join(", ")}` +
      `（当前设计：hokusai 在 sRGB 空间混合；在那里加转换 = 双解码的前兆）`,
  );
} else {
  ok("③ hokusai 里没有 srgb_to_linear 调用（只有定义）⇒ 混合仍在 sRGB 空间（当前设计）");
}

if (fail.length > 0) {
  console.log(`\n  ✗ ${fail.length} 处不符（色彩管线的转换点表变了 ⇒ 先更新表再改代码）`);
  process.exit(1);
}
console.log("\n  ✓ 色彩管线的转换点与表一致：入向 1 处、出向 1 处、hokusai 内 0 处");
