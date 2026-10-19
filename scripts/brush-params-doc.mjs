#!/usr/bin/env node
// **笔刷参数清单成文** ✓：`docs/design/brush-params.md`。
//
//   node scripts/brush-params-doc.mjs write <server-base> <doc> <token>   # 生成
//   node scripts/brush-params-doc.mjs check <server-base> <doc> <token>   # 校验（CI 用）
//
// **为什么要有它** ✗（外部报告 #10 ✓）：63 个规范名此前只能从 `list_brushes.canonical_param_names`
// 或 `list_engine_capabilities.brush_settings` 拿 ✓，**没有成文清单** ✗ ⇒ agent 要猜、人要翻代码 ✓。
//
// **数据从哪来（不硬编码 ✓）**：
//   · 63 个规范名 ⇒ `list_engine_capabilities.brush_settings` ✓（工具侧遍历 `BrushSetting::ALL` ✓）；
//   · 四组别名与展开 ⇒ **读 `crates/yanshi-server/src/tools.rs` 的 `semantic_brush_alias`** ✓
//     （**∴ 与 `tool-brush-aliases.mjs` 同一手法 ✓：读源码，不抄一份 ✓**）；
//   · `medium_stroke` 那条路的参数 ⇒ `list_engine_capabilities.medium_params` ✓。
import { readFileSync, writeFileSync } from "node:fs";

const [mode, base, doc, token] = process.argv.slice(2);
if (!mode || !base || !doc || !token) {
  console.error("用法: node scripts/brush-params-doc.mjs <write|check> <base> <doc> <token>");
  process.exit(3);
}
const OUT = new URL("../docs/design/brush-params.md", import.meta.url);
const call = async (name, args) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
  const reply = await fetch(url, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args) });
  return await reply.json();
};
const caps = await call("list_engine_capabilities", {});
const names = caps?.brush_settings ?? [];
// **别把数写死**（第 798 轮）：这里原来是 `!== 63`，而 mask-texture 补丁给
// `BrushSetting::ALL` 加了 3 个 ⇒ 实际 66 ⇒ 判据一直 `exit(3)`「无法判定」。
// ⇒ 改成**下界检查**：只要 ≥ 63 就继续（真正的比对在下面与 `docs/brush-params.md` 逐名对，
//   名字对不上才是产品问题）。这样以后再加设置不会再让这条判据整体失效。
if (names.length < 63) { console.log(`  ✗ brush_settings 只有 ${names.length} 个（至少应有 63）⇒ 判据无法判定`); process.exit(3); }
const medium = caps?.medium_params?.names ?? [];
// 别名与展开：读源码
const src = readFileSync(new URL("../crates/yanshi-server/src/tools.rs", import.meta.url), "utf8");
const aliases = [...src.matchAll(/"([a-z_]+)"\s*=>\s*Some\(&\[([^\]]*)\]\)/g)]
  .map((m) => ({ alias: m[1], expands: m[2].split(",").map((x) => x.trim().replace(/^"|"$/g, "")).filter(Boolean) }));
if (aliases.length !== 4) { console.log(`  ✗ 源码里别名组 ${aliases.length} 个（应为 4）⇒ 判据无法判定`); process.exit(3); }

const lines = [];
lines.push("# 笔刷参数清单");
lines.push("");
lines.push("**这份文件是生成的** ✗ —— `node scripts/brush-params-doc.mjs write <base> <doc> <token>` ✓；");
lines.push("**别手改** ✗，改了会和 `BrushSetting::ALL` 对不上，CI 的 `brush-params-doc` 判据会红 ✓。");
lines.push("");
lines.push("## 两条路，两套参数");
lines.push("");
lines.push("| 路径 | 参数来自 | 数量 |");
lines.push("|---|---|---|");
lines.push(`| \`brush_stroke\` 的 \`brush_params\` | \`.myb\`（MyPaint 动力学） | **${names.length}** |`);
lines.push(`| \`medium_stroke\` | 介质插件（油画／水彩／…） | **${medium.length}**（含 \`load\`／\`wetness\`／\`color_mixing\`） |`);
lines.push("");
lines.push("**∴ 要载墨量／湿度／光谱混色 ⇒ 走 `medium_stroke`** ✓（**∵ `.myb` 的清单里没有它们** ✗）。");
lines.push("");
lines.push("## 四组语义别名（`brush_params` 里可以直接写）");
lines.push("");
lines.push("| 别名 | 展开成 |");
lines.push("|---|---|");
for (const { alias, expands } of aliases) lines.push(`| \`${alias}\` | ${expands.map((x) => `\`${x}\``).join(" ＋ ")} |`);
lines.push("");
lines.push(`## ${names.length} 个规范名（\`BrushSetting::ALL\`，按字母序）`);
lines.push("");
for (const n of names) lines.push(`- \`${n}\``);
lines.push("");
const text = lines.join("\n");

if (mode === "write") {
  writeFileSync(OUT, text);
  console.log(`  ✓ 已写出 docs/design/brush-params.md：${names.length} 个规范名、${aliases.length} 组别名、medium ${medium.length} 项`);
  process.exit(0);
}
if (mode !== "check") { console.error(`未知模式 ${mode}`); process.exit(3); }
// check：与"现算的"逐行比
let have = "";
try { have = readFileSync(OUT, "utf8"); } catch { console.log("  ✗ 文档不存在 ⇒ 先跑 write ✓"); process.exit(1); }
if (have === text) { console.log(`  ✓ 文档与引擎/源码一致（${names.length} 个规范名、${aliases.length} 组别名、medium ${medium.length} 项）`); process.exit(0); }
const hl = have.split("\n"), tl = text.split("\n");
let first = 0;
while (first < hl.length && first < tl.length && hl[first] === tl[first]) first += 1;
console.log(`  ✗ docs/design/brush-params.md 与现算结果不一致（第 ${first + 1} 行起）`);
console.log(`     文档：${(hl[first] ?? "<无>").slice(0, 100)}`);
console.log(`     应为：${(tl[first] ?? "<无>").slice(0, 100)}`);
process.exit(1);
