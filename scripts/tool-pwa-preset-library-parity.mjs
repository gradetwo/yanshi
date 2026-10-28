#!/usr/bin/env node
// **判据：PWA 内联的预设库必须与服务端那份**逐字节同一份 ✓ ★**（第 2 批 ✓／台账「第 六百五十七 节」）。
//
// **∴ 为什么要有这一条 ✗**（**内核 ①「别相信看起来对」＋ 内核 ②「同一事实只写一处」✓**）：
//   本地模式把 `style-presets.json` **整份内联**进 `api-local.js`（**离线必定可用 ✓**）。
//   **∴ 于是**这一份事实**在两处各存一遍 ✗** ⇒ **∴ 一定漂移**（**上游改了库、PWA 没跟上 ✓**）
//   ⇒ **∴ 而漂移的 symptoms 是**「PWA 少一套预设／参数不一样」**而页面 `ok:true` ✓**
//   ＝ **静默降级形态③ ✗**（**被覆盖而不说 ✓**）。**⇒ 唯一的解法是把「两处相同」变成一条红着的判据 ✓**。
//
// **∴ 断的是结构量 ✗**（**内核：判据优先断结构量 ✓**）：**字节相等 ✓ 与 分支数 ✓**，不断任何像素／耗时 ✓。
//
// **∴ 四层齐 ✓**：实现（内联块 ✓）／输入触发（**上游文件就是那个输入 ✓**）／断言（本文件 ✓）
//   ／**变异**（**改内联块里一个数字 ⇒ 本判据必须红 ✓ 已实测 ⇒ 见台账 ✓**）。
// **∴ 改内联库的正确做法 ✗**：`node scripts/gen-pwa-preset-library.mjs --write`（**再生成，别手改 ✓**）。
import { readFileSync } from "node:fs";
import path from "node:path";

const ROOT = process.cwd();
const SOURCE = path.join(ROOT, "crates/yanshi-server/data/style-presets.json");
const WEB = path.join(ROOT, "web/api-local.js");
const ASSETS = path.join(ROOT, "crates/yanshi-http/assets/api-local.js");
const BEGIN = "// <<P2_PRESET_RAW:begin>>";
const END = "// <<P2_PRESET_RAW:end>>";

const bad = [];
const check = (cond, msg) => { if (!cond) bad.push(msg); };
const skip = (why) => { console.log(`⊘ 不可判：${why}（判据前提不成立，不是产品问题）`); process.exit(3); };

const read = (f) => { try { return readFileSync(f, "utf8"); } catch { return null; } }

const source = read(SOURCE);
const web = read(WEB);
const assets = read(ASSETS);
if (source === null) skip(`读不到服务端库 ${SOURCE}`);
if (web === null) skip(`读不到 ${WEB}`);
if (assets === null) skip(`读不到 ${ASSETS}`);

/** **取内联块 ✓**（**两个标记之间、去掉注释行与 `String.raw` 包装 ✗ ⇒ 剩下的应当**就是**那份 JSON ✓**）。 */
function extractInline(text, label) {
  const b = text.indexOf(BEGIN);
  const e = text.indexOf(END);
  if (b < 0 || e < 0 || e < b) { check(false, `${label} 里没有内联库标记（begin/end 缺一或顺序反了）✗`); return null; }
  const seg = text.slice(b + BEGIN.length, e);
  const m = seg.match(/String\.raw`([\s\S]*)`\s*;?\s*$/);
  if (!m) { check(false, `${label} 的标记之间不是 String.raw 模板串形式 ✗（内联库里若出现反引号或美元花括号会截断模板串，见生成器的前置检查）`); return null; }
  return m[1];
}

const inline = extractInline(web, "web/api-local.js");
const inlineAssets = extractInline(assets, "assets/api-local.js");
if (inline !== null) {
  // **① 与源文件逐字节相同 ✓**（**这是本判据的主断言 ✗ 其余都是它的护栏 ✓**）。
  check(inline === source,
    `内联的预设库与 crates/yanshi-server/data/style-presets.json 不再逐字节相同 ✗`
    + `（内联 ${Buffer.byteLength(inline, "utf8")} B｜源 ${Buffer.byteLength(source, "utf8")} B）`
    + ` ⇒ 运行 node scripts/gen-pwa-preset-library.mjs --write 重新生成 ✓`);
  // **② 两份副本之间也相同 ✓**（**∴ `web/` 与 `crates/yanshi-http/assets/` 必须一起再生成 ✗**）。
  check(inlineAssets !== null && inlineAssets === source,
    "assets/api-local.js 里的内联库与源文件不同 ✗ ⇒ 两条部署的 PWA 会给不同预设（分叉）");
  // **③ 解析后确实是一库预设 ✓**（**∴ 防「两边都是垃圾但相同」✗ 这种相同没有意义 ✓**）。
  let parsed = null;
  try { parsed = JSON.parse(inline); } catch (error) { check(false, `内联库解析失败 ✗：${error.message}`); }
  if (parsed) {
    const n = Array.isArray(parsed.presets) ? parsed.presets.length : 0;
    check(n >= 20, `内联库里的预设只有 ${n} 套 ✗（少于 20 ⇒ 多半是被截断的片段）`);
    check(typeof parsed.version === "number", "内联库缺 version ✗");
    check(parsed.presets.every((p) => Array.isArray(p.style_ops) && p.style_ops.length > 0),
      "内联库里有预设的 style_ops 为空 ✗（展开会是空清单 ⇒ apply 会报「库数据非法」✓）");
    console.log(`  内联库：${n} 套｜version=${parsed.version}｜${Buffer.byteLength(inline, "utf8")} 字节 ✓`);
  }
}

// **④ 恒等式（优于单点断言 ✓ 内核 ②）✗**：**模块层声明的「本地已实现端点数」**
//   **必须**等于**机械重数的分支数 ✗** ⇒ 否则 `LOCAL_IMPLEMENTED` 是在**虚报能力 ✓**。
const declared = Number((web.match(/LOCAL_IMPLEMENTED\s*=\s*(\d+)/) || [])[1] ?? -1);
const actual = ((web.match(/(?:url\.pathname|path) === "/g) || []).length);
check(declared === actual,
  `LOCAL_IMPLEMENTED=${declared} 与机械重数的实现分支 ${actual} 不一致 ⇒ 虚报能力 ✗`
  + `（数它：node -e 'const a=require("fs").readFileSync("web/api-local.js","utf8");`
  + `console.log(((a.match(/(?:url\\.pathname|path) === "/g)||[]).length))'）`);
check(declared >= 0, `读不到 LOCAL_IMPLEMENTED 的数值 ✗（声明行被改写了？）`);

// **⑤ 生成器的自检也要跑一遍 ✓**（**∴ 同一事实只有一处判法 ✗ ⇒ 让生成器自己说「一致」✓**）。
const { spawnSync } = await import("node:child_process");
const gen = spawnSync("node", ["scripts/gen-pwa-preset-library.mjs", "--check"], { encoding: "utf8" });
check(gen.status === 0, `gen-pwa-preset-library.mjs --check 退出码 ${gen.status} ✗：`
  + String(gen.stdout || "").trim().slice(0, 200));

if (bad.length) {
  console.log("✗ PWA 预设库逐字相同 判据失败：");
  for (const m of bad) console.log("   · " + m);
  process.exit(1);
}
console.log("✓ PWA 内联预设库与服务端那份逐字节相同（含两份部署副本、端点计数自洽）");
