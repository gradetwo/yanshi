#!/usr/bin/env node
// **★ `inputSchema` 里不许出现 `"type":"any"` ✗ ★**（2026-10-16 ✓；第三方实测 ＋ 本机复现 ✓）
//
// **∴ 症状 ✗**（**修前实测 ✓**）：核心 profile 82 个工具的 `inputSchema` 里
//   **12 处 `"type":"any"`** ✓ —— `brush_stroke.color`／`brush_stroke.color_to`／
//   `brush_preview.color`／`brush_preview.color_to`／`analyze_region.region`／`calibrate.points` … ✓
//
// **∴ 为什么是缺陷 ✗**：**JSON Schema 没有 `any` 这个类型** ✓
//   （合法的只有 string/number/integer/boolean/object/array/null ✓，或者**省略 `type`** ＝ 任意值 ✓）
//   ⇒ **严格客户端**（含部分 MCP 客户端 ✓）会因这一处**拒绝整份 schema／拒绝工具发现** ✗
//   ⇒ **∴ 代价不是"少一个类型标注"** ✗ 而**是"整批工具都用不了"** ✓ ★
//
// **∴ 判什么（两半都要 ✓ —— 只判 schema 形状会漏掉"改坏校验"✗）**：
//   ① **形状** ✓：`--list-tools`（core 与 all 两个 profile ✓）里**任何一层**都没有
//      `"type": "any"`（也含 `"type": ["any", …]` 这种数组写法 ✓）；
//   ② **没改坏别的** ✓：工具数与关键参数还在 ✓（不能把整个 properties 删光来"修好" ✗）；
//   ③ **行为两本账** ✓：真的让服务端收一次**曾经受影响的那个参数** ✓ ——
//      `brush_stroke.color` 分别用**字符串**（`#RRGGBB`）与**对象**（`{r,g,b,a}`）各画一笔 ✓
//      两笔都必须 `ok=true` ✓（**∴ 放宽的是"声明"** ✓，**不是"收哪些值"** ✗）。
//
// **∴ 能红 ✓**：把 `input_schema` 改回 `"type": param.kind.json_type()`（`Any => "any"` ✓）
//   ⇒ ① 立刻红 ✓（修前的 12 处就是实测的反例 ✓）。
//
// 用法：node scripts/mcp-schema-no-any.mjs <base-url>
import { execFileSync } from "node:child_process";

const base = (process.argv[2] || "").replace(/\/+$/, "");
if (!base) {
  console.error("用法: node scripts/mcp-schema-no-any.mjs <base-url>");
  process.exit(2);
}
const BIN = process.env.YANSHI_MCP_BIN || "target/debug/yanshi-mcp";

const listTools = (profile) => {
  let raw;
  try {
    raw = execFileSync(BIN, ["--list-tools", "--profiles", profile], { timeout: 600_000, encoding: "utf8", maxBuffer: 64 << 20 });
  } catch (error) {
    console.error(`❌ 判不了：${BIN} --list-tools --profiles ${profile} 跑不起来（${error.message}）⇒ 退出码 3`);
    process.exit(3);
  }
  try {
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed : parsed.tools;
  } catch (error) {
    console.error(`❌ 判不了：--list-tools 回的不是 JSON（${error.message}）⇒ 退出码 3`);
    process.exit(3);
  }
};

/// 递归找**任何一层**的 `type` 里出现 `any` ✓（含数组写法 ✓）。
const findAnyType = (node, tool, path, hits) => {
  if (Array.isArray(node)) {
    node.forEach((item, index) => findAnyType(item, tool, `${path}/${index}`, hits));
    return;
  }
  if (!node || typeof node !== "object") return;
  const type = node.type;
  const offenders = [];
  if (typeof type === "string" && type === "any") offenders.push(type);
  if (Array.isArray(type) && type.includes("any")) offenders.push(JSON.stringify(type));
  for (const offender of offenders) hits.push(`${tool}${path}.type = ${offender}`);
  for (const [key, value] of Object.entries(node)) {
    if (key === "description" || key === "title") continue; // 描述里出现"any"不算 ✗
    findAnyType(value, tool, `${path}/${key}`, hits);
  }
};

const failures = [];
for (const profile of ["core", "all"]) {
  const tools = listTools(profile);
  if (!Array.isArray(tools) || tools.length === 0) {
    console.error(`❌ 判不了：profile ${profile} 没列出任何工具 ⇒ 退出码 3`);
    process.exit(3);
  }
  const hits = [];
  for (const tool of tools) findAnyType(tool.inputSchema, tool.name, "", hits);
  console.log(`  profile ${profile}：${tools.length} 个工具｜含 \`type:"any"\` 的 ${hits.length} 处`);
  for (const hit of hits.slice(0, 6)) console.log(`     · ${hit}`);
  if (hits.length > 0) {
    failures.push(`${profile} 里有 ${hits.length} 处非法的 \`type:"any"\`（JSON Schema 无此类型 ⇒ 严格客户端会拒绝整份 schema ✗）`);
  }
  // ② 没改坏别的：工具数与"曾经受影响的参数"都还在 ✓
  if (profile === "core") {
    const byName = new Map(tools.map((tool) => [tool.name, tool]));
    for (const [tool, param] of [["brush_stroke", "color"], ["brush_stroke", "color_to"], ["analyze_region", "region"]]) {
      const spec = byName.get(tool);
      if (!spec) {
        failures.push(`核心 profile 里少了工具 ${tool} ✗`);
        continue;
      }
      const property = spec.inputSchema && spec.inputSchema.properties && spec.inputSchema.properties[param];
      if (!property) {
        failures.push(`${tool} 少了参数 ${param} ✗（不能靠删参数来"修好" schema ✗）`);
        continue;
      }
      if (property.type !== undefined) {
        failures.push(`${tool}.${param} 应当**省略** type（任意值 ✓），实际是 ${JSON.stringify(property.type)} ✗`);
      }
      if (!property.description) failures.push(`${tool}.${param} 丢了 description ✗`);
    }
    console.log(`  关键参数在、且都是"省略 type"（＝任意值 ✓）`);
  }
}

// ③ 行为两本账：字符串与对象**都还得能画** ✓（放宽的是声明，不是收哪些值 ✓）
const call = async (doc, token, tool, args) =>
  (await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify(args || {}),
  })).json();
const doc = `any_${Date.now().toString(36)}`;
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 160, height: 120 }),
})).json();
if (!created || !created.token) {
  console.error("❌ 判不了：建不了文档 ⇒ 退出码 3");
  process.exit(3);
}
const token = created.token;
await call(doc, token, "create_layer", { layer_id: "L1", name: "L1" });
const shapes = [
  ["字符串 #RRGGBB", "#3366aa"],
  ["对象 {r,g,b,a}", { r: 40, g: 90, b: 170, a: 255 }],
];
for (const [label, color] of shapes) {
  const replied = await call(doc, token, "brush_stroke", {
    layer_id: "L1", object_id: `o${shapes.findIndex((s) => s[1] === color)}`,
    brush: "100%_Opaque", size: 10, color, points: [[30, 40], [120, 60]],
  });
  const ok = replied && replied.ok === true;
  console.log(`  两本账：color 用${label} ⇒ ok=${ok}`);
  if (!ok) {
    failures.push(`color 用${label}时被拒了 ✗（回执：${JSON.stringify(replied).slice(0, 140)}）⇒ 放宽声明时把校验也改坏了 ✗`);
  }
}

if (failures.length) {
  console.error(`❌ ${failures.length} 项没成立：`);
  for (const item of failures) console.error(`   · ${item}`);
  process.exit(1);
}
console.log(`✓ inputSchema 里没有非法的 \`type:"any"\`（core 与 all 两个 profile ✓），关键参数仍在，且"任意值参数"字符串/对象都还收得下 ✓`);
