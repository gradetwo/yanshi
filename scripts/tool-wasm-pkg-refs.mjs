#!/usr/bin/env node
// **★ 内核包的**引用完整性**判据 ✗ ★**（**第 502 轮 ✓）
//
// **★★★ 它守的是一个**已发生的真 bug**** ★★★**：
//   **∴ 症状 ✗**：**CI 的 PWA 真实浏览器判据报
//     `Failed to fetch dynamically imported module: /wasm/yanshi_wasm.js`** ✓
//     ＋ **∴ 而**文件**确实在**（**200｜text/javascript ✓）＋ **magic**也对** ✓
//       ⇒ **∴ 于是**：**排查了很久** ✓ ★
//   **∴ 真因 ✗**：**`wasm-bindgen --target web` 还会生成
//     **`snippets/<crate>-<hash>/inline0.js`**** ✓
//     ＋ **∴ 而** `yanshi_wasm.js` **内部**`import("./snippets/…")`** ✓
//       ＋ **∴ 而** `pwa-sync-wasm.mjs` 原来**硬编码 4 个文件名**✗
//         ⇒ **∴ 那个子文件**从不进 `web/wasm/`** ✓
//           ⇒ **∴ 浏览器**import 主文件 ⇒ **∴ 内部 import 子文件 ⇒ 404** ✓
//             ⇒ **★ 整个模块**求值失败**✗ ⇒ **∴ Chrome 报的却是**主文件** ✓ ★**** ✓✓
//
// **∴ 本判据做两件事 ✗**：
//   ⇒ **∴ ① 引用扫描 ✗**：**从 `crates/yanshi-wasm/pkg/yanshi_wasm.js` 里
//     抠出**相对引用**✗（`from "./x"`／`import("./x")`／`new URL("./x", …)` ✓）
//     ＋ **∴ 断言**每个目标**在 `web/wasm/` 里都存在** ✓ ★
//   ⇒ **∴ ② 同步脚本不许硬编码清单 ✗**：**它必须是**递归**的** ✓
//     ⇒ **∴ 否则**：**下次 wasm-bindgen 换个输出结构**再漏一次** ✓ ★
//
// **∴ 用法 ✗**：
//   ⇒ `node scripts/tool-wasm-pkg-refs.mjs` ✓
//     ＋ `--self-test`：**自检（**证明两条断言都能红 ✓）
//
// **∴ 退出码 ✗**：0 通过｜1 产品缺陷｜2 用法错
//   ｜**★ 3 判据自身跑不了**（**如本机没构建过 wasm ⇒ `pkg/` 不存在 ✓）★

import { existsSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

const PKG = "crates/yanshi-wasm/pkg";
const DST = "web/wasm";
const JS = join(PKG, "yanshi_wasm.js");

/** **∴ 从内核 JS 里抠出**相对引用**✗**（**纯函数 ⇒ 自检能直接喂文本 ✓）★ */
export function extractRelativeRefs(source) {
  const refs = new Set();
  const patterns = [
    /from\s*"(\.[^"]+)"/g,
    /from\s*'(\.[^']+)'/g,
    /import\s*\(\s*"(\.[^"]+)"\s*\)/g,
    /import\s*\(\s*'(\.[^']+)'\s*\)/g,
    /new\s+URL\s*\(\s*"(\.[^"]+)"/g,
    /new\s+URL\s*\(\s*'(\.[^']+)'/g,
  ];
  for (const re of patterns) {
    for (const m of source.matchAll(re)) refs.add(m[1]);
  }
  return [...refs];
}

/** **∴ 检查：引用是否都能在 dst 找到 ✗**（**纯函数 ✓）★ */
export function checkRefs({ refs, exists }) {
  const bad = [];
  for (const ref of refs) {
    // **∴ `./x` ⇒ `x`（**相对 dst 根 ✓）
    const rel = ref.replace(/^\.\//, "");
    if (!exists(rel)) bad.push(`内核 JS 引用了 ${ref}，但它不在 ${DST}/ ⇒ **PWA 里会 404**`);
  }
  return bad;
}

/** **∴ 检查同步脚本必须递归 ✗**（**纯函数 ✓）★ */
export function checkSyncIsRecursive(source) {
  const bad = [];
  // **∴ 不许再出现**硬编码的文件数组**✗**（**∴ 那**正是漏 snippets 的原因 ✓）★
  if (/const\s+files\s*=\s*\[/.test(source)) {
    bad.push("pwa-sync-wasm.mjs 里又出现了硬编码的 `const files = [...]` ⇒ **子目录会被漏**（正是本 bug 的成因）");
  }
  if (!/readdirSync/.test(source)) {
    bad.push("pwa-sync-wasm.mjs 没有用 readdirSync ⇒ **不是递归同步** ⇒ 子目录会被漏");
  }
  return bad;
}

// ───────────────────── 自检 ─────────────────────
if (process.argv.includes("--self-test")) {
  const jsText = `
    import { x } from "./snippets/yanshi-wasm-abc/inline0.js";
    const u = new URL('./yanshi_wasm_bg.wasm', import.meta.url);
    import("./lazy.js");
  `;
  const refs = extractRelativeRefs(jsText);
  const cases = [
    {
      name: "正常：三个引用都在",
      args: { refs, exists: () => true },
      wantRed: false,
    },
    {
      name: "★ 缺 snippets 子文件（**本 bug 的形状 ✓）",
      args: { refs, exists: (rel) => !rel.startsWith("snippets/") },
      wantRed: true,
    },
    {
      name: "缺 wasm",
      args: { refs, exists: (rel) => rel !== "yanshi_wasm_bg.wasm" },
      wantRed: true,
    },
    {
      name: "提取器抓到三个引用",
      args: { refs, exists: () => true, minRefs: 3 },
      wantRed: false,
    },
  ];
  let wrong = 0;
  let red = 0;
  console.log("  ── 自检：断言**必须能红** ──");
  for (const c of cases) {
    const failed = checkRefs(c.args);
    if (c.args.minRefs && refs.length < c.args.minRefs) {
      failed.push(`只抠出 ${refs.length} 个引用 ⇒ 提取器坏了`);
    }
    const wentRed = failed.length > 0;
    if (wentRed) red += 1;
    const ok = wentRed === c.wantRed;
    if (!ok) wrong += 1;
    console.log(`  ${ok ? "✓" : "✗"} ${c.name} ⇒ ${wentRed ? "红" : "绿"}（期望${c.wantRed ? "红" : "绿"}）`);
  }
  // **∴ 同步脚本的两条 ✗**
  const syncCases = [
    { name: "递归（**正常 ✓）", src: 'import { readdirSync } from "node:fs"; const x = walk(src);', wantRed: false },
    { name: "★ 硬编码清单（**旧 bug ✓）", src: 'const files = ["a.js","b.wasm"]; for (const f of files) {}', wantRed: true },
    { name: "没有 readdirSync", src: 'copyFileSync(a, b);', wantRed: true },
  ];
  for (const c of syncCases) {
    const failed = checkSyncIsRecursive(c.src);
    const wentRed = failed.length > 0;
    if (wentRed) red += 1;
    const ok = wentRed === c.wantRed;
    if (!ok) wrong += 1;
    console.log(`  ${ok ? "✓" : "✗"} 同步脚本：${c.name} ⇒ ${wentRed ? "红" : "绿"}（期望${c.wantRed ? "红" : "绿"}）`);
  }
  const total = cases.length + syncCases.length;
  console.log("");
  console.log(`  ⇒ 自检：${total} 例｜判红 ${red}｜不符合期望 ${wrong}`);
  if (wrong > 0) {
    console.error("  ✗ 自检失败 ⇒ **本判据没有牙**");
    process.exit(1);
  }
  console.log("  ✓ 自检通过：断言既能绿也能红（**有牙 ✓）");
  process.exit(0);
}

// ───────────────────── 实测 ─────────────────────
if (!existsSync(JS)) {
  console.error(`  ✗ 判据自身的输入缺失（不是产品问题）：找不到 ${JS}`);
  console.error("     ⇒ 本机没有 wasm 目标时无法跑 ⇒ 由 CI 的 wasm 步骤覆盖");
  process.exit(3);
}
if (!existsSync(DST)) {
  console.error(`  ✗ 判据自身的输入缺失（不是产品问题）：找不到 ${DST} ⇒ 先跑 pwa-sync-wasm.mjs`);
  process.exit(3);
}

const source = readFileSync(JS, "utf8");
const refs = extractRelativeRefs(source);
const exists = (rel) => {
  const p = join(DST, rel);
  return existsSync(p) && statSync(p).size > 0;
};

console.log("");
console.log(`  ★ 内核包的引用完整性（${refs.length} 个相对引用）★`);
for (const ref of refs) {
  const rel = ref.replace(/^\.\//, "");
  const ok = exists(rel);
  console.log(`  ${ok ? "✓" : "✗"} ${ref} ⇒ ${ok ? "已在 " + DST + " ✓" : "**缺失**"}`);
}
const bad = checkRefs({ refs, exists });
const syncSource = readFileSync("scripts/pwa-sync-wasm.mjs", "utf8");
bad.push(...checkSyncIsRecursive(syncSource));
if (refs.length === 0) {
  console.log("  ℹ️ 没有相对引用 ⇒ **本判据的 ① 在空转**（不是失败，但要如实说）");
}
console.log("");
if (bad.length) {
  for (const b of bad) console.error(`  ✗ ${b}`);
  process.exit(1);
}
console.log("  ✓ 内核 JS 的全部相对引用都在 web/wasm 里｜同步脚本是递归的（**不会漏子目录** ✓）");
process.exit(0);
