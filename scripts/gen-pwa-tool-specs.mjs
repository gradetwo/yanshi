#!/usr/bin/env node
// **PWA 本地工具表生成器**（`web/api-local.js` 里的 `P2_TOOL_SPECS`）。
//
// **为什么需要生成而不是手写**：本地 `preview_suggestion`／`accept_suggestion` 要在
// **不应用**的前提下做与服务端同口径的参数校验（`validate_args`），因此需要
// 「工具名 → {mutating, 参数名/类型/必填}」。227 条工具的这张表**手抄必漂移**。
//
// **两个数据源各管一半**（都有权威出处）：
//   ① 参数名/类型/必填 ⇒ `GET /api/tools`（与 MCP `tools/list` 逐字相同，`server.rs:701`）
//   ② `mutating` 位 ＋ **参数声明顺序** ⇒ `crates/yanshi-server/src/tools_registry.rs`
//      （**HTTP 清单不暴露这两样**：`serde_json::Map` 是 BTreeMap ⇒ `properties` 的键
//      序列化后**按字母序** ✗，而 `validate_args` 的「可用参数：…」按**声明顺序** join ✓
//      ⇒ **∴ 顺序必须从源码取，不能从清单取 ✓**）
//
// **漂移怎么被发现**：`scripts/tool-pwa-suggestion-accept.mjs` 会重新算这两个源并
// 与 `web/api-local.js` 里嵌的表逐项比对 —— 表过期 ⇒ 判据红（不是静默漂移）。
//
// 用法：
//   node scripts/gen-pwa-tool-specs.mjs <server-base> --check         # 只比对（判据用）
//   node scripts/gen-pwa-tool-specs.mjs <server-base> --print-block   # 打印生成块
//   node scripts/gen-pwa-tool-specs.mjs <server-base> --write         # 写回 api-local.js
//
// 退出码：0 一致/成功；1 表过期或数据源不全；3 不可判（缺 BASE / 服务连不上）。

import { readFileSync, writeFileSync } from "node:fs";

const BASE = process.argv[2];
if (!BASE || !/^https?:\/\//.test(BASE)) {
  console.log("⊘ 不可判：argv[2] 缺服务端地址 ⇒ 先生成一个临时服务（可参考 scripts/run-criteria-local.sh 的编排）");
  process.exit(3);
}
const MODE = process.argv.includes("--write") ? "write"
  : process.argv.includes("--print-block") ? "print"
  : "check";

const REGISTRY = "crates/yanshi-server/src/tools_registry.rs";
const TARGET = "web/api-local.js";
const BEGIN = "// <gen:tool-specs>";
const END = "// </gen:tool-specs>";

// **`a-z0-9_` 的紧凑写法**：20 KB → 原样 JSON 是 186 KB（还含长 summary/description）。
// 类型码：s=string i=integer n=number b=boolean o=object a=array x=any（缺省 type ⇒ any）。
const KIND_CODE = { string: "s", integer: "i", number: "n", boolean: "b", object: "o", array: "a" };

const json = await fetch(`${BASE}/api/tools`).then((r) => r.json()).catch((e) => {
  console.log(`⊘ 不可判：GET /api/tools 失败（${String((e && e.message) || e)}）`);
  process.exit(3);
});
const tools = (json && json.tools) || [];
if (tools.length === 0) {
  console.log("⊘ 不可判：GET /api/tools 返回空清单");
  process.exit(3);
}

/** 去掉注释 ✓（字符串里的 `//` 不算 ✓）—— 参数表里夹着 `//` 注释，注释里的逗号会被误当分隔 ✓。 */
function stripComments(src) {
  let out = "";
  let i = 0;
  while (i < src.length) {
    const c = src[i];
    if (c === '"') {
      let j = i + 1;
      while (j < src.length) {
        if (src[j] === "\\") { j += 2; continue; }
        if (src[j] === '"') { j += 1; break; }
        j += 1;
      }
      out += src.slice(i, j);
      i = j;
      continue;
    }
    if (c === "/" && src[i + 1] === "/") {
      const nl = src.indexOf("\n", i);
      i = nl < 0 ? src.length : nl;
      continue;
    }
    if (c === "/" && src[i + 1] === "*") {
      const close = src.indexOf("*/", i + 2);
      i = close < 0 ? src.length : close + 2;
      continue;
    }
    out += c;
    i += 1;
  }
  return out;
}

/** 从 `open`（`[` 的下标）找**配对的** `]` ✓（跳过字符串里的括号 ✓）。
 *  **∴ 不能只找 `"];"`／`"],"` ✗**：常量收尾是 `];` ✓ 而**字段收尾是 `],`** ✓
 *  （`params: &[…]` 是结构体字段）⇒ 两种写法都要认 ✓。 */
function findCloseBracket(src, open) {
  let depth = 0;
  let i = open;
  while (i < src.length) {
    const c = src[i];
    if (c === '"') {
      let j = i + 1;
      while (j < src.length) {
        if (src[j] === "\\") { j += 2; continue; }
        if (src[j] === '"') { j += 1; break; }
        j += 1;
      }
      i = j;
      continue;
    }
    if (c === "[") depth += 1;
    else if (c === "]") {
      depth -= 1;
      if (depth === 0) return i;
    }
    i += 1;
  }
  return -1;
}

/** 按**顶层逗号**切数组字面量 ✓（`param!(…)` 里的逗号在括号里 ✓；字符串里的逗号要跳过 ✓）。 */
function splitTopLevel(body) {
  const items = [];
  let depth = 0;
  let cur = "";
  let i = 0;
  while (i < body.length) {
    const c = body[i];
    if (c === '"') {
      let j = i + 1;
      while (j < body.length) {
        if (body[j] === "\\") { j += 2; continue; }
        if (body[j] === '"') { j += 1; break; }
        j += 1;
      }
      cur += body.slice(i, j);
      i = j;
      continue;
    }
    if (c === "(" || c === "[" || c === "{") depth += 1;
    else if (c === ")" || c === "]" || c === "}") depth -= 1;
    if (c === "," && depth === 0) {
      if (cur.trim()) items.push(cur.trim());
      cur = "";
      i += 1;
      continue;
    }
    cur += c;
    i += 1;
  }
  if (cur.trim()) items.push(cur.trim());
  return items;
}

/** 单个 `ParamSpec` 常量 ✓（`const CLIP_TO_SELECTION_PARAM: ParamSpec = ParamSpec { name: "…" }`）
 *  ⇒ 它被**裸名**当作元素塞进别的参数表（`tools_registry.rs:2399`）⇒ 要能解析 ✓。 */
function scrapeParamConsts(src) {
  const map = new Map();
  const re = /const\s+([A-Z_][A-Z0-9_]*)\s*:\s*ParamSpec\s*=\s*ParamSpec\s*\{/g;
  let m;
  while ((m = re.exec(src))) {
    const end = src.indexOf("\n};", re.lastIndex);
    const body = src.slice(re.lastIndex, end < 0 ? src.length : end);
    const name = body.match(/name:\s*"([a-z0-9_]+)"/);
    if (name) map.set(m[1], name[1]);
  }
  return map;
}

/** 解析参数表正文 ⇒ 参数名（声明顺序，含重复 ✓）。元素只有两种形态：
 *  `param!("name", …)` 与**裸常量名**（`CLIP_TO_SELECTION_PARAM`）；其余 ⇒ 响亮失败 ✓（不静默跳过 ✗）。 */
function paramNamesFromBody(body, paramConsts) {
  const out = [];
  for (const item of splitTopLevel(stripComments(body))) {
    const m = item.match(/^param!\s*\(\s*"([A-Za-z0-9_]+)"/);
    if (m) { out.push(m[1]); continue; }
    const id = item.match(/^([A-Z_][A-Z0-9_]*)$/);
    if (id && paramConsts.has(id[1])) { out.push(paramConsts.get(id[1])); continue; }
    return { error: `无法解析参数表里的元素：${item.slice(0, 80)}` };
  }
  return { params: out };
}

/** 参数表常量 ✓（`const LAYER_ID: &[ParamSpec] = &[param!(…)]` —— 多个 `params: IDENT` 工具共用 ✓）。
 *  **∴ 同名可能有多份 ✗**（`#[cfg(feature = "raw")]` 两个变体 ✓）⇒ 存成候选数组 ✓，
 *  按「去重集合 == 清单 properties」挑出**唯一**一份 ✓（挑不出就响亮失败 ✓）。 */
function scrapeConsts(src, paramConsts) {
  const map = new Map();
  const re = /const\s+([A-Z_][A-Z0-9_]*)\s*:\s*&\[ParamSpec\]\s*=\s*&\[/g;
  let m;
  while ((m = re.exec(src))) {
    const start = re.lastIndex;
    const end = findCloseBracket(src, start - 1);
    if (end < 0) continue;
    const parsed = paramNamesFromBody(src.slice(start, end), paramConsts);
    if (parsed.error) {
      console.log(`✗ 常量 ${m[1]}：${parsed.error}`);
      process.exit(1);
    }
    if (!map.has(m[1])) map.set(m[1], []);
    map.get(m[1]).push(parsed.params);
  }
  return map;
}

/** 从注册表源码取 `mutating` 位与**参数声明顺序** ✓
 *  （`ToolSpec { name: "x", … params: &[param!("a", …), param!("b", …)] }` ⇒ 数组序 ＝ 文案序 ✓；
 *    `params: IDENT` ⇒ 记下常量名，由 `scrapeConsts` 解析 ✓）。 */
function scrapeSpecs(path) {
  // **∴ 先剥注释 ✗ 再解析 ✓**：参数表里夹着 `//` 注释（`IMPORT_IMAGE_PARAMS` ✓），
  //   注释里的括号／逗号会打乱配对与切分 ✓（字符串保留 ⇒ 参数名／文案不受影响 ✓）。
  const src = stripComments(readFileSync(path, "utf8"));
  // **∴ 常量不一定与工具表同文件 ✗**（`NO_PARAMS` 与 `CLIP_TO_SELECTION_PARAM` 都在 `tools.rs` ✓，
  //   经 `use` 进来 ✓）⇒ 两份都扫；同名以注册表为准 ✓。
  const toolsRs = "crates/yanshi-server/src/tools.rs";
  const tsrc = stripComments(readFileSync(toolsRs, "utf8"));
  const paramConsts = new Map();
  for (const [k, v] of scrapeParamConsts(tsrc)) paramConsts.set(k, v);
  for (const [k, v] of scrapeParamConsts(src)) paramConsts.set(k, v);
  const consts = new Map();
  for (const [k, v] of scrapeConsts(tsrc, paramConsts)) consts.set(k, v);
  for (const [k, v] of scrapeConsts(src, paramConsts)) consts.set(k, v);
  const map = new Map();
  for (const block of src.split(/ToolSpec \{/).slice(1)) {
    const name = block.match(/name:\s*"([a-z0-9_]+)"/);
    const mutating = block.match(/mutating:\s*(true|false)/);
    if (!name || !mutating) continue;
    if (map.has(name[1])) {
      console.log(`✗ ${path} 里 ${name[1]} 出现两次（表的来源必须唯一）`);
      process.exit(1);
    }
    const inline = /params:\s*&\[/.test(block);
    const ref = inline ? null : (block.match(/params:\s*([A-Z_][A-Z0-9_]*)/) || [])[1] || null;
    let params = [];
    if (inline) {
      const open = block.indexOf("&[", block.search(/params:\s*&\[/));
      const close = findCloseBracket(block, open + 1);
      const parsed = paramNamesFromBody(block.slice(open + 2, close), paramConsts);
      if (parsed.error) {
        console.log(`✗ 工具 ${name[1]}：${parsed.error}`);
        process.exit(1);
      }
      params = parsed.params;
    }
    map.set(name[1], { mutating: mutating[1] === "true", params, ref, consts });
  }
  return map;
}

/** 生成排序后的紧凑表行（不带缩进之外的格式差异）。 */
export function buildRows(tools, specOf) {
  const rows = [];
  for (const tool of tools) {
    const spec = specOf.get(tool.name);
    if (!spec) {
      return { error: `工具 ${tool.name} 在注册表里找不到 mutating 位` };
    }
    const schema = tool.inputSchema || {};
    const required = new Set(schema.required || []);
    const props = schema.properties || {};
    const fromSchema = Object.keys(props);
    const setEq = (list) => {
      const dedup = [...new Set(list)];
      return dedup.length === fromSchema.length &&
        dedup.every((n) => Object.prototype.hasOwnProperty.call(props, n));
    };
    // **∴ 参数顺序 = 注册表声明顺序 ✗**（`validate_args` 的错误文案按它 join ✓）；
    //   **∴ 声明里允许重复名 ✓**（`brush_stroke` 有 `pressure_shape`／`stroke_taper` 各两条 ⇒
    //     服务端文案也各出现两次 ✓）⇒ 比对用**去重集合**，输出保留**原始顺序含重复** ✓。
    let declared = spec.params;
    if (spec.ref) {
      const variants = spec.consts.get(spec.ref) || [];
      const matching = variants.filter((v) => setEq(v));
      if (matching.length !== 1) {
        return { error: `工具 ${tool.name} 的 params 常量 ${spec.ref} 候选 ${variants.length} 份，`
          + `与清单匹配 ${matching.length} 份（必须唯一）` };
      }
      declared = matching[0];
    }
    if (!setEq(declared)) {
      return { error: `工具 ${tool.name} 的参数集合不一致：注册表 [${[...new Set(declared)].join(",")}]`
        + ` vs 清单 [${fromSchema.join(",")}]` };
    }
    const params = [];
    for (const name of declared) {
      const p = props[name];
      const type = p && p.type;
      // ⇒ `Any` 不写 type（`tools.rs:1137` 的注释：JSON Schema 没有 any）⇒ 缺省即 any ✓
      const code = type ? KIND_CODE[type] : "x";
      if (!code) return { error: `参数 ${tool.name}.${name} 的 type=${type} 不认识` };
      params.push(`["${name}","${code}",${required.has(name) ? 1 : 0}]`);
    }
    rows.push(`  ${tool.name}: [${spec.mutating ? 1 : 0}, [`
      + params.join(",") + "]],");
  }
  rows.sort((a, b) => (a.trim() < b.trim() ? -1 : a.trim() > b.trim() ? 1 : 0));
  return { block: rows.join("\n") };
}

const specOf = scrapeSpecs(REGISTRY);
const built = buildRows(tools, specOf);
if (built.error) {
  console.log(`✗ ${built.error}`);
  process.exit(1);
}

if (MODE === "print") {
  console.log(built.block);
  process.exit(0);
}

const src = readFileSync(TARGET, "utf8");
const begin = src.indexOf(BEGIN);
const end = src.indexOf(END);
if (begin < 0 || end < 0 || end < begin) {
  console.log(`✗ ${TARGET} 里找不到生成标记 ${BEGIN} / ${END}`);
  process.exit(1);
}
const current = src.slice(begin + BEGIN.length, end).replace(/^\n/, "").replace(/\n$/, "");

if (MODE === "check") {
  if (current === built.block) {
    console.log(`✓ 表与线上工具清单一致（${tools.length} 条工具）`);
    process.exit(0);
  }
  const cur = current.split("\n"), want = built.block.split("\n");
  const n = Math.max(cur.length, want.length);
  let shown = 0;
  for (let i = 0; i < n && shown < 6; i++) {
    if (cur[i] !== want[i]) {
      console.log(`✗ 第 ${i + 1} 行不一致：`);
      console.log(`   表里： ${cur[i] === undefined ? "(缺行)" : cur[i]}`);
      console.log(`   应为： ${want[i] === undefined ? "(多余行)" : want[i]}`);
      shown += 1;
    }
  }
  console.log("⇒ 用 --write 重新生成（前提：线上工具清单是权威的）");
  process.exit(1);
}

writeFileSync(TARGET, src.slice(0, begin + BEGIN.length) + "\n" + built.block + "\n" + src.slice(end));
console.log(`✓ 已写回 ${TARGET}（${tools.length} 条工具）`);
