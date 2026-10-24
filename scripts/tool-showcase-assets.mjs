#!/usr/bin/env node
// **★ 作品展示资产不许烂掉 ✓ ★**（2026-10-16 ✓）
//
// **∴ 为什么要有它 ✗**：展示图被放进了 **README（中英两版）** 与 **应用首页** ✓ ——
//   三处都靠**同一个相对路径** `showcase/<name>.png` ✓ ⇒ 只要有人**改名/删图/挪目录** ✗
//   就会**静默**变成裂图 ✗（README 与应用都不会报错 ✓）⇒ **∴ 这里把它钉住** ✓。
//
// **∴ 第 745 轮补上第 ⑤ 条 ✗「反向」**（**原来只断**单向 ✓ 而**历史 bug 恰恰在反方向**✓**）：
//   **∴ 原来 ✗**：只断「**名单里每一张**都被三处引用」✓ ⇒ **「三处引用了名单外的图」**这一类**看不见**✓
//     ——**实测**：两版 README 各引用一张 `ballet-reference-study-4k.png`✓ 而它**不在名单里**✓
//     ⇒ **删掉它 ⇒ 本判据照样绿**✓ 而 README 上是**裂图**✓（**内核 1 ✗「有判据」≠「覆盖了」✓**）
//   **∴ 而**`.gitignore:68` 记的**正是**同一个 bug 的上一次：`web/showcase/` **曾被忽略**✗
//     ⇒ **本地文件都在 ✗ 远端 404**✓ ⇒ **所以第 ⑤ 条除了查存在 ✗ 还查「被 git 跟踪」**✓
//   **∴ 两面 ✓**：收益＝**引用侧也不再能自由漂移**✓（**改名／删图／漏跟踪三种都红 ✓**）；
//     **代价 ✗**：判据现在**依赖 `git ls-files`**（**拿不到 ⇒ `exit(3)` ✓ 不冒充绿 ✓**）
//
// **∴ 判什么（四条 ✓）**：
//   ① 名单里的图**都在**、都是**合法 PNG**（自己解析签名 ＋ IHDR ✓，不引依赖 ✗）、且**不是巨图** ✓
//      （放的是 900px 的展示副本 ⇒ 单张 ≤ 400 KB ✓，原图 6–25 MB ✗ 不进仓库 ✓）；
//      **∴ 这条上限只管名单里的**展示副本**✓**；仓库自带那张 6.0 M 原图（README 直接用 ✓）
//      **不归它管**✓ ⇒ 第 ⑤ 条**如实报大小但不判红**✓（**那是上游的决定 ✗ 不是我能替它定的**✓）
//   ② **两版 README** 都引用了**每一张** ✓；
//   ③ **应用首页**（`web/index.html` ✓）也引用了**每一张** ✓；
//   ④ **真的去服务端 fetch 名单里每一个 URL** ✓（`/showcase/<name>` ⇒ 200 ＋ 字节数与本地一致 ✓）。
//
// **∴ 能红 ✓**：删掉任一张图、或把它从任一版 README／首页里去掉 ⇒ 立刻红 ✓。
//
// 用法：node scripts/tool-showcase-assets.mjs [base-url]
// **前提失败必须 exit(3)**（第 1084 轮）：无服务时 fetch 抛异常 ⇒ uncaught ⇒ 退出码 1 ✗
// ⇒ 按 336 轮那条：条件不满足应 exit(3)（不可判），不是 exit(1)（产品失败）
process.on("uncaughtException", (error) => {
  console.error(`  ⊘ 判据自身出错（不是产品问题）：${String(error).slice(0, 200)}`);
  process.exit(3);
});
import { readFileSync, existsSync, statSync } from "node:fs";
import { spawnSync } from "node:child_process";

const SHOWCASE = [
  "degas-backstage-oil.png",
  "degas-four-dancers-oil.png",
];
const MAX_BYTES = 400 * 1024; // 900px 展示副本的量级 ✓（原图 6–25 MB ✗）
// **∴ 图住在哪 ✗**：**单一源是 `assets/showcase/`** ✓（服务端也从它发 ✓ 见 `server.rs::showcase_asset` ✓）；
//   `web/showcase/` 由 `pwa-sync-viewer.mjs` 从源拷过去 ✓ **但它不是**纯导出产物 ✗**（第 745 轮核正 ✓）：
//   它**故意入库**✓（**`.gitignore` 里那条是被注释掉的**✓ 理由「README 的图在 GitHub 上 404」✓）
//   ⇒ **∴ 于是**它里面**可以**有源里没有的图 ✓（**README 直接引用的那一张 ✓**）⇒ 第 ⑤ 条据此**不判红**
const SOURCE_DIR = "assets/showcase";
const WEB_DIR = "web/showcase";
const readers = ["README.md", "README.zh-CN.md"];
// **★ 页面要查**两个地方** ✗ ★**（2026-10-16 ✓；**CI 教会我的** ✓）：
//   **∴ 症状 ✗**：我第一版只查 `web/index.html` ✓ ⇒ **本机绿、CI 红** ✗，而**不是**判据乱报 ✓ ——
//     CI 会**重新生成** `web/index.html`（`yanshi-serve --export-viewer-html` ＋ 注入 ✓，
//     见 `scripts/pwa-sync-viewer.mjs` ✓）⇒ **∴ 手改那一份会被覆盖** ✗ ⇒ 我的块没了 ✓ ★
//   **∴ 真相 ✗**：页面的**单一源**是 `crates/yanshi-http/src/viewer.rs` ✓（服务端渲的就是它 ✓）；
//     `web/index.html` 只是**导出产物** ✓（但它**故意入库** ✓ —— 缺了它 `wrangler deploy` 会 404 ✓，
//     见 `.gitignore:70` ✓）。
//   ⇒ **∴ 两处都查** ✓：源里必须有 ✓（否则下次导出就丢 ✗）；产物若在，也必须有 ✓。
const sources = [
  ["crates/yanshi-http/src/viewer.rs", "showcase/"],   // 单一源 ✓（服务端页面）
  ["web/index.html", "showcase/"],                      // 导出产物 ✓（故意入库的那一份）
];

/// 只读 PNG 头 ✓（签名 ＋ IHDR 的宽高 ✓）—— **不引依赖** ✓，与 `tool-tileable-textures` 同一手法 ✓。
const pngInfo = (path) => {
  const bytes = readFileSync(path);
  const signature = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
  for (let i = 0; i < signature.length; i += 1) if (bytes[i] !== signature[i]) return null;
  const width = bytes.readUInt32BE(16);
  const height = bytes.readUInt32BE(20);
  return { width, height, bytes: bytes.length };
};

const bad = [];
for (const name of SHOWCASE) {
  const path = `${SOURCE_DIR}/${name}`;
  if (!existsSync(path)) {
    bad.push(`${path} 不存在`);
    continue;
  }
  const size = statSync(path).size;
  const info = pngInfo(path);
  if (!info) {
    bad.push(`${path} 不是合法 PNG（签名不对）`);
    continue;
  }
  if (info.width < 600 || info.height < 600) {
    bad.push(`${path} 只有 ${info.width}×${info.height} ⇒ 展示副本不该这么小`);
  }
  if (size > MAX_BYTES) {
    bad.push(`${path} 有 ${(size / 1024).toFixed(0)} KB > ${MAX_BYTES / 1024} KB ⇒ 展示副本不该这么大`);
  }
  console.log(`  ${name}：${info.width}×${info.height}｜${(size / 1024).toFixed(0)} KB ✓`);
  // ② 两版 README 都要引用它 ✓
  for (const reader of readers) {
    if (!existsSync(reader)) {
      bad.push(`${reader} 不存在`);
      continue;
    }
    const text = readFileSync(reader, "utf8");
    if (!text.includes(`web/showcase/${name}`)) bad.push(`${reader} 没有引用 ${path}`);
  }
  // ③ **源**与**导出产物**都要引用它 ✓（只查产物 = 会被 CI 的重新导出打脸 ✗；只查源 = 产物可能没跟上 ✗）
  for (const [file, prefix] of sources) {
    if (!existsSync(file)) {
      bad.push(`${file} 不存在`);
      continue;
    }
    if (!readFileSync(file, "utf8").includes(`${prefix}${name}`)) {
      bad.push(`${file} 没有引用 ${prefix}${name}`);
    }
  }
}

// **★ ④ 真的去服务端取一次 ✗ ★**（2026-10-16 ✓；**这一条是补上我自己的漏** ✓）：
//   **∴ 我漏了什么 ✗**：第一版只查"三处引用了没有" ✓ ⇒ **本机全绿** ✓ 而**服务端首页里四张图全 404** ✗
//     —— 因为图只放进了**导出目录** `web/` ✗，而**服务端发的是它自己的资产树** ✓（`assets/` ✓）。
//   ⇒ **∴ 判据必须**两条腿走路**✗**：**文件在不在** ✓ ＋ **URL 取不取得到** ✓（两本账 ✓）。
const base = (process.argv[2] || "").replace(/\/+$/, "");
if (base) {
  for (const name of SHOWCASE) {
    let status = 0;
    let bytes = 0;
    try {
      const reply = await fetch(`${base}/showcase/${name}`);
      status = reply.status;
      bytes = (await reply.arrayBuffer()).byteLength;
    } catch (error) {
      bad.push(`取不到 ${base}/showcase/${name}（${error.message}）`);
      continue;
    }
    const local = statSync(`${SOURCE_DIR}/${name}`).size;
    console.log(`  GET /showcase/${name} ⇒ HTTP ${status}｜${bytes} 字节（本地 ${local} ✓）`);
    if (status !== 200) bad.push(`服务端发不出 ${name}（HTTP ${status} ✗）⇒ 页面会是裂图 ✗`);
    else if (bytes !== local) bad.push(`${name} 服务端发的字节数 ${bytes} ≠ 本地 ${local} ✗`);
  }
} else {
  console.log("  （没给 base-url ⇒ 跳过第 ④ 条 HTTP 自检 ✓；CI 里会给 ✓）");
}

// **★ ⑤ 反向：谁引用了就必须存在 ✗（README 那一份还要**被 git 跟踪**）✓ ★**（第 745 轮 ✓）
//   **∴ 断的是**方向**✗ 不是**数量**✓：①②③ 断「**名单 → 引用**」✓
//     ⇒ **∴ 组合盲区**正是「**引用 → 名单**」✓（**加了张图不写进名单 ✗ 或把图删了 ⇒ 全绿 ✓**）
console.log("");
console.log("⑤ 反向：三处引用的每一张都必须**在**（README 那一份还要**被 git 跟踪**）：");
const tracked = (() => {
  const r = spawnSync("git", ["ls-files", "web/showcase"], { encoding: "utf8" });
  if (r.error || r.status !== 0) return null;
  return new Set(r.stdout.split("\n").filter(Boolean));
})();
if (tracked === null) {
  console.error("  ⊘ 拿不到 `git ls-files web/showcase` ⇒ 第 ⑤ 条无法判定（判据前提不成立，不是产品失败）");
  process.exit(3);
}
const namesIn = (file) => (existsSync(file)
  ? [...new Set([...readFileSync(file, "utf8").matchAll(/showcase\/([A-Za-z0-9._-]+\.png)/g)].map((m) => m[1]))]
  : []);
let reverseChecked = 0;
for (const reader of readers) {
  if (!existsSync(reader)) { bad.push(`${reader} 不存在`); continue; }
  for (const name of namesIn(reader)) {
    reverseChecked += 1;
    const at = `web/showcase/${name}`;
    if (!existsSync(at)) bad.push(`${reader} 引用了 ${at} ⇒ 文件不在 ⇒ GitHub 上就是裂图 ✗`);
    else if (!tracked.has(at)) bad.push(`${reader} 引用了 ${at} ⇒ 文件在但**没被 git 跟踪** ✗ 远端照样 404 ✓（见 .gitignore 第 68 行的理由 ✓）`);
    else console.log(`  ${reader} → ${at} 在且已跟踪 ✓（${(statSync(at).size / 1048576).toFixed(1)} MB ⇒ 只报不判红 ✓）`);
  }
}
for (const [file] of sources) {
  for (const name of namesIn(file)) {
    reverseChecked += 1;
    for (const dir of [SOURCE_DIR, WEB_DIR]) {
      if (!existsSync(`${dir}/${name}`)) {
        bad.push(`${file} 的页面引用了 ${name} ⇒ 但 ${dir}/${name} 不在（服务端读 assets/、静态站读 web/ ✗ 缺一个就有一侧裂图 ✓）`);
      }
    }
  }
}
if (reverseChecked === 0) {
  bad.push("第 ⑤ 条一条都没查到 ⇒ 三处里**一张图都没引用**✓ 那本判据已空转（这不是产品通过 ✗）");
}

if (bad.length) {
  console.error(`❌ 作品展示资产有 ${bad.length} 处问题：`);
  for (const item of bad) console.error(`   · ${item}`);
  console.error("   ⇒ 三处（README×2 ＋ 首页）用的是**同一个相对路径** ⇒ 改名／删图／漏跟踪都会让它静默变裂图 ✗");
  process.exit(1);
}
console.log(`✓ 展示图资产：名单 ${SHOWCASE.length} 张都在且 ≤400 KB｜反向查到 ${reverseChecked} 处引用，全部存在（web/showcase 已跟踪 ${tracked.size} 个 ✓）✓`);
