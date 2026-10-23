#!/usr/bin/env node
// **★ 把内核产物同步到 PWA 的静态目录 ✓ ★**（第 608 轮 ✓；**部署矩阵 §9 ① ✓**）
//
// **∴ 为什么需要它 ✗**：**`crates/yanshi-wasm/pkg/` 是构建产物 ✗**（**不在 `web/` 里 ✓**），
//   而 **PWA 要求**静态托管**✗** ⇒ **∴ 必须把 `pkg/` 拷到 `web/wasm/` ✓**。
//
// **∴ 用法** ✓：`node scripts/pwa-sync-wasm.mjs`（**先 `cargo build -p yanshi-wasm --target
//   wasm32-unknown-unknown` ＋ `wasm-bindgen` 生成 `pkg/` ✓，**或复用已有的 `pkg/` ✓**）
//
// **∴ 判据** ✓：**拷完后 `web/wasm/yanshi_wasm_bg.wasm` 必须存在且非空 ✗**
//   ⇒ **∴ 否则 `wrangler deploy` 会传一个没有内核的 PWA ✗**（**∴ 那就成了"假装能画"✗**）。
import { copyFileSync, existsSync, mkdirSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const src = "crates/yanshi-wasm/pkg";
const dst = "web/wasm";

// **★★★ 递归同步整个 `pkg/` ⇒ 不再硬编码文件名 ✗ ★★★**（**第 502 轮实测 ✓）
//
// **∴ 原来的缺陷（**真 bug ✓）✗**：**这里**硬编码了 4 个文件名**✗
//   ⇒ **∴ 而**实测：**`wasm-bindgen --target web` 还会生成 **`snippets/`** 目录** ✓
//     （**∴ 本机 ✗**：`crates/yanshi-wasm/pkg/snippets/` 存在 ✓）
//     ⇒ **∴ 于是**：**`yanshi_wasm.js` 内部的 `import("./snippets/…")`**✗
//       ⇒ **∴ 在 PWA 里**404** ✓
//         ⇒ **★ 症状 ✗ ★**：**浏览器报
//           `Failed to fetch dynamically imported module: /wasm/yanshi_wasm.js`** ✓
//           （**∴ 看起来像**主文件坏了**✗ ，**其实**是它引用的**子文件缺了** ✓）★**** ✓✓
//
// **∴ 为什么改成递归 ✗**：**硬编码清单一改 wasm-bindgen 的输出结构就**再漏** ✗
//   ⇒ **∴ 递归 ⇒ **生成什么就同步什么** ✓
//     ＋ **∴ 且**：**它**不再需要维护** ✓ ★**** ✓✓
function walk(dir, base = dir) {
  const out = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) out.push(...walk(full, base));
    else out.push(relative(base, full));
  }
  return out;
}

if (!existsSync(src)) {
  console.error(`✗ 找不到内核产物 ${src} ⇒ 请先构建 yanshi-wasm（见本脚本头部说明）`);
  process.exit(2);
}

// **★★★ 陈旧守卫 ✗：宁可拒绝发布，也不把过期内核拷进 PWA ✓ ★**（**第 742 轮实测 ✓**）
//
// **∴ 原来缺什么 ✗**：本脚本**只验**「拷完之后文件在不在、空不空」✓（**见下方 `missing` ✓**）
//   ⇒ **∴ 而**`pkg/` 本身**是构建产物**✗ ⇒ **∴ 它可以比源码旧** ✓
//     ⇒ **∴ 于是**`web/wasm/` 拿到**上一版内核**✗ 而**没有任何信号** ✓
//       ⇒ **★ 这就是「静默降级」的第 5 种形态 ✗**（**「做了 0 次」冒充「不需要做」✓**）：
//         **∴ 「产物存在」被当成了「产物是新的**」✗ **✓
//
// **∴ 实测代价 ✗**（**第 740–742 轮 ✓**）：两条浏览器判据报「**文档 0×0**」✗
//   ⇒ **∴ 我**据此去追「**客户端落笔一族**」的根因 ✗
//     ⇒ **∴ 而**追的对象**是陈旧内核** ✓（**∴ 与第 956 轮同一个坑 ✗：陈旧二进制测出假象 ✓**）
//
// **∴ 守卫的判据 ✗**：内核由**这三个 crate 的 `.rs`** 决定 ✓
//   ⇒ **∴ 其中任何一个**比 `pkg/yanshi_wasm_bg.wasm` 新**✗ ⇒ **∴ 产物就是旧的** ✗
//   **∴ 为什么**响亮失败**✗ 而不是警告 ✓**：警告**会被 CI 日志淹没** ✗
//     ⇒ **∴ 而**「**发布了假装是新的内核**」✗ 会让**之后每一条**浏览器判据**测到错的东西** ✓
//
// **∴ 什么时候不该报警 ✗**：**CI 的两个 job 都是**先 build ＋ `wasm-bindgen` ⇒ 再跑本脚本** ✓
//   （**`.github/workflows/ci.yml` ✓／`pwa.yml` ✓**）⇒ **∴ 所以**守卫在 CI 上**恒通过** ✓
//     **∴ 只有**本机**复用旧 `pkg/`** 时**才会拦 ✗（**∴ 那**正是**本守卫要拦的 ✓**）
const KERNEL_SOURCE_DIRS = [
  "crates/yanshi-wasm/src",
  "crates/yanshi-render/src",
  "crates/yanshi-core/src",
];
const pkgWasm = join(src, "yanshi_wasm_bg.wasm");
if (existsSync(pkgWasm)) {
  const built = statSync(pkgWasm).mtimeMs;
  const stale = [];
  for (const dir of KERNEL_SOURCE_DIRS) {
    if (!existsSync(dir)) continue;
    for (const rel of walk(dir)) {
      if (!rel.endsWith(".rs")) continue;
      const p = join(dir, rel);
      if (statSync(p).mtimeMs > built) stale.push(p);
    }
  }
  if (stale.length > 0) {
    console.error(
      `✗ 内核产物比源码陈旧 ⇒ 拒绝把 ${pkgWasm} 拷进 ${dst}（第 742 轮加的守卫 ✓）\n` +
        `  比产物新的源码（最多列 5 个）：\n    ${stale.slice(0, 5).join("\n    ")}\n` +
        `  修法（两条都要跑 ✗ ⇒ 只 build 不 wasm-bindgen 的话 pkg/ 不会更新 ✓）：\n` +
        `    cargo build -p yanshi-wasm --release --target wasm32-unknown-unknown\n` +
        `    wasm-bindgen --target web --out-dir ${src} target/wasm32-unknown-unknown/release/yanshi_wasm.wasm\n` +
        `    ＋ wasm-bindgen 的版本要取自主线锁：scripts/wasm-bindgen-version.sh（第 492 轮的教训 ✗：别写死）`
    );
    process.exit(2);
  }
}

mkdirSync(dst, { recursive: true });
let copied = 0;
const rels = walk(src);
for (const rel of rels) {
  const from = join(src, rel);
  const to = join(dst, rel);
  // **∴ 子目录要先建 ✗**（**∴ snippets 就在子目录里 ✓）★
  const parent = to.slice(0, to.lastIndexOf("/"));
  if (parent) mkdirSync(parent, { recursive: true });
  copyFileSync(from, to);
  copied += 1;
}
const wasm = join(dst, "yanshi_wasm_bg.wasm");
const js = join(dst, "yanshi_wasm.js");
// **★ 两个都要验 ✗ ★**：之前只验 wasm，js 空了也不报错 ⇒
//   浏览器 `import()` 报 "Failed to fetch dynamically imported module" ✗。
const missing = [];
for (const [label, p] of [["wasm", wasm], ["js", js]]) {
  if (!existsSync(p) || statSync(p).size === 0) missing.push(label + ":" + p);
}
if (missing.length) {
  console.error("✗ 同步后以下文件缺失或为空 ⇒ 该 PWA 会没有内核: " + missing.join(", "));
  process.exit(1);
}
const nested = rels.filter((r) => r.includes("/")).length;
console.log(
  `✓ 已同步 ${copied} 个文件 ⇒ ${dst}` +
    `（wasm ${(statSync(wasm).size / 1048576).toFixed(2)} MiB ✓` +
    `｜含子目录项 ${nested} 个${nested > 0 ? " ⇒ **snippets 等子目录不会漏** ✓" : ""}）`,
);
