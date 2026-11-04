#!/usr/bin/env node
// **三方功能对齐的棘轮尺子**（2026-11-04）。
//
// **三方是哪三方** ✓：
//   ① 服务端工具注册表：`crates/yanshi-server/src/tools_registry.rs` 的 `ALL_TOOLS`
//      （HTTP `/api/tools/<name>` 的权威源头；`tools.rs` 的 `dispatch` 是同一个表）；
//   ② MCP 层：`crates/yanshi-mcp/src/lib.rs` 用 `ToolRegistry::with_profiles(...)` ——
//      **同一张注册表按 profile 过滤** ⇒ 结构上不可能另有一份工具名单 ✓；
//   ③ PWA 本地层：`crates/yanshi-http/assets/api-local.js` 的 `localDispatch`
//      （`web/api-local.js` 必须逐字节相同）。
//
// **它判什么** ✓：
//   · 注册表工具数（从源码现数，不写死）；
//   · 有本地路由 / 缺本地路由（按 `path === "/api/tools/<name>"` 分支现数）；
//   · 本地反向多出（本地有、注册表无）⇒ 必须为空，或在允许名单里且**写了原因**；
//   · 声明参数本地未读（生成块里的参数名在生成块之外 0 次出现 ⇒ 一定没读，单向判据）；
//   · 两个棘轮：缺本地路由 ≤ 59、参数未读 ≤ 66。**只许降**；要涨必须像
//     `check-index-html-byte-budget.mjs` 那样抬常量并在提交信息里写清「谁涨了多少、为什么」。
//   · 两份 `api-local.js` 逐字节相同（改一份必须镜像另一份）。
//
// **为什么「0 次出现 ⇒ 一定没读」是单向判据** ✗：
//   该文件没有按名字反射取参数（无 `body[k]` 循环、无 `...body` 转发）⇒
//   名字在生成块外一次都不出现，就不可能有代码读到它 ✓；
//   反过来「出现 > 0 次」**不代表**读了（可能只是注释/同名字段）⇒ 不据此判红 ✓。
//   **代价**：通用名（`width`/`size`）会被别处的出现掩盖 ⇒ 可能漏报（宁可漏，不可假红）。
//   这条与 `check-pwa-local-params.mjs` 同口径，两个判据对同一批参数给同一个数 ✓。
//
// **前提不成立 ⇒ exit 3**（不是 0，也不是 1）：
//   读不到注册表／本地层、找不到 `ALL_TOOLS`、找不到生成块、找不到 `localDispatch`、
//   读不到允许名单 —— 都是「判不了」，不是产品红 ✓。
//
// 用法：node scripts/check-three-way-alignment.mjs
// 退出码：0 = 绿｜1 = 有一项对齐缺口越过棘轮｜3 = 前提不成立（不可判）
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const REGISTRY = "crates/yanshi-server/src/tools_registry.rs";
const TOOLS_RS = "crates/yanshi-server/src/tools.rs";
const LOCAL = "crates/yanshi-http/assets/api-local.js";
const LOCAL_MIRROR = "web/api-local.js";
const MCP_LIB = "crates/yanshi-mcp/src/lib.rs";
const ALLOWLIST = "scripts/three-way-alignment-allowlist.txt";
const SPEC_BEGIN = "// <gen:tool-specs>";
const SPEC_END = "// </gen:tool-specs>";

// **棘轮上界**（只许降；要涨必须抬值并写理由 —— 同 `check-index-html-byte-budget`）。
// 基线 2026-11-04：缺本地路由 60 → 补 `put_blob` 后 59；参数未读 66（与
// `check-pwa-local-params.mjs` 的白名单同数）。两个数都由本判据每次现算，
// 常量只用来**拦住增长**，不是快照。
// line-parity（同一天）：补 `stroke_heatmap`／`list_fonts`／`memory_status` 三条只读本地路由
// ⇒ 缺本地路由 59 → 56 ⇒ 上界一并降到 56（缺口只许降）。
// 同日第二批：本地补读 7 个参数 ⇒ 66 → 54（**∴ 其中 5 个是「同名掩盖」✗，不是真读了**）：
//   · 真读（行为判据 `check-pwa-local-param-behaviour.mjs` ✓）：`batch.slim_receipt`／
//     `smudge.deposit`／`brush_preview.include_image`·`return_image`·`max_px`／
//     `create_mask.from_selection`／`create_selection.from_mask`；
//   · 顺带掉出死名单（**假的** ✗）：`export_png.return_image`·`max_px` 与
//     `render_region.include_image`·`max_px`·`return_image` —— 这三个名字现在出现在
//     `brush_preview` 的分支里，而本判据按**全文件出现次数**数（单向判据的已知漏报 ✓）。
//   ⇒ **∴ 所以 54 里有 5 个不是覆盖** ✓：要真补它们，得等 `export_png`／`render_region`
//     自己读（前者会改「总是内嵌 base64」的既有行为，后者本地回的是二进制 PNG）。
const MISSING_CEILING = 56;
const DEAD_PARAMS_CEILING = 52;

const failures = [];
const check = (ok, label, detail) => {
  console.log(`  ${ok ? "✓" : "✗"} ${label}${detail ? `（${detail}）` : ""}`);
  if (!ok) failures.push(label + (detail ? `：${detail}` : ""));
};
const premise = (message) => {
  console.error(`  ⊘ 判据前提不成立：${message}（不是产品问题）`);
  process.exit(3);
};
const read = (path) => {
  try {
    return readFileSync(join(ROOT, path), "utf8");
  } catch (error) {
    premise(`读不到 ${path}：${error.message}`);
  }
};

// ---- ① 注册表：名字 + profile（从 ALL_TOOLS 源码现数，绝不用文档里的数字） ----
function parseRegistry(source) {
  const anchor = "pub const ALL_TOOLS: &[ToolSpec] = &[";
  const start = source.indexOf(anchor);
  if (start < 0) premise(`${REGISTRY} 里找不到 ALL_TOOLS 声明`);
  const rest = source.slice(start);
  const end = rest.indexOf("\n];");
  if (end < 0) premise(`${REGISTRY} 里 ALL_TOOLS 数组没有收尾 ];`);
  const blocks = rest.slice(0, end).split("    ToolSpec {").slice(1);
  if (blocks.length === 0) premise(`${REGISTRY} 里 ALL_TOOLS 没有任何条目`);
  const tools = [];
  const seen = new Set();
  for (const block of blocks) {
    const name = block.match(/name:\s*"([a-z0-9_]+)"/)?.[1];
    const profile = block.match(/profile:\s*Profile::([A-Za-z]+)/)?.[1];
    if (!name || !profile) premise(`${REGISTRY} 里有 ToolSpec 条目缺 name/profile`);
    if (seen.has(name)) premise(`${REGISTRY} 里工具名重复：${name}`);
    seen.add(name);
    tools.push({ name, profile });
  }
  return tools;
}

// ---- ② 本地镜像的规格块（参数名的权威本地副本，由生成器从 /api/tools 产出） ----
function parseSpecs(source) {
  const begin = source.indexOf(SPEC_BEGIN);
  const end = source.indexOf(SPEC_END);
  if (begin < 0 || end < 0 || end <= begin) {
    premise(`${LOCAL} 里找不到生成块 ${SPEC_BEGIN}…${SPEC_END}`);
  }
  const block = source.slice(begin, end);
  const specs = new Map();
  for (const line of block.split("\n")) {
    const m = line.match(/^ {2}([a-z0-9_]+): \[([01]), \[(.*)\]\]/);
    if (!m) continue;
    const params = [...m[3].matchAll(/\["([a-z0-9_]+)","/g)].map((x) => x[1]);
    specs.set(m[1], params);
  }
  if (specs.size === 0) premise(`${LOCAL} 的生成块里没有任何工具行`);
  return { specs, outside: source.slice(0, begin) + source.slice(end) };
}

// ---- ③ 本地路由：只认 `path === "/api/tools/<name>"` 分支 ----
//   **∴ 不能全局搜 `path === "`** ✗：`body.path === "string"` 这种参数类型判断也会命中
//   （本判据第一版实测踩中 ⇒ 多算 1 个分支）⇒ 只扫「以 if ( 或 const locked = 开头的行」✓。
function parseRoutes(source) {
  if (!source.includes("async function localDispatch(req)")) {
    premise(`${LOCAL} 里找不到 localDispatch`);
  }
  const direct = new Set();
  for (const line of source.split("\n")) {
    const trimmed = line.trim();
    if (!trimmed.startsWith("if (") && !trimmed.startsWith("const locked = ")) continue;
    for (const m of line.matchAll(/(?:url\.pathname|path) === "\/api\/tools\/([a-z0-9_]+)"/g)) {
      direct.add(m[1]);
    }
  }
  // **别名**（`undo`／`redo`／`list_brushes`）不是新工具：它们只是同一实现的另一个请求名。
  //   目标若被本地处理（工具分支或非工具路径），请求名就**不算缺路由**；
  //   但**不算**「本地反向多出」（那是给注册表里没有的**功能**记的账）。
  const aliasRoutes = new Set();
  for (const m of source.matchAll(/p2Norm === "\/api\/tools\/([a-z0-9_]+)"\s*\?\s*"([^"]+)"/g)) {
    const [, from, to] = m;
    if (to.startsWith("/api/tools/")) {
      if (direct.has(to.slice("/api/tools/".length))) aliasRoutes.add(from);
    } else if (source.includes(`path === "${to}"`)) {
      aliasRoutes.add(from);
    }
  }
  return { direct, aliasRoutes };
}

// ---- ④ 允许名单：每行 `工具名 原因`；注释以 # 开头 ----
function parseAllowlist(text) {
  const entries = [];
  for (const raw of text.split("\n")) {
    const line = raw.trim();
    if (!line || line.startsWith("#")) continue;
    const name = line.split(/\s+/)[0];
    const reason = line.slice(name.length).trim().replace(/^#\s*/, "");
    entries.push({ name, reason });
  }
  return entries;
}

// ---- ⑤ MCP：同一注册表按 `Profile::DEFAULT` 过滤（DEFAULT 的展开规则照 tools.rs） ----
const PROFILE_EXPANSIONS = {
  Oil: ["History", "Retouch"],
  Pastel: ["History", "Retouch"],
  Watercolor: ["History"],
  Sketch: ["History"],
  Ink: ["History"],
  Photo: ["History", "Changeset", "Retouch", "Structure"],
  Svg: ["History", "Changeset", "Structure"],
};
function parseMcpDefaultProfiles(toolsSource) {
  const m = toolsSource.match(/pub const DEFAULT: \[Profile; \d+\] = \[([\s\S]*?)\];/);
  if (!m) premise(`${TOOLS_RS} 里找不到 Profile::DEFAULT`);
  const enabled = new Set(["Core"]); // expand_profiles 恒加 Core
  for (const hit of m[1].matchAll(/Profile::([A-Za-z]+)/g)) {
    enabled.add(hit[1]);
    for (const implied of PROFILE_EXPANSIONS[hit[1]] || []) enabled.add(implied);
  }
  return enabled;
}

// ============================ 主流程 ============================
const registrySource = read(REGISTRY);
const toolsSource = read(TOOLS_RS);
const localSource = read(LOCAL);
const mirrorSource = read(LOCAL_MIRROR);
const mcpSource = read(MCP_LIB);
const allowlistText = read(ALLOWLIST);

const tools = parseRegistry(registrySource);
const registryNames = new Set(tools.map((tool) => tool.name));
const { specs, outside } = parseSpecs(localSource);
const { direct, aliasRoutes } = parseRoutes(localSource);
const allowlist = parseAllowlist(allowlistText);

// 前提：MCP 必须真的复用同一张注册表（否则「MCP 有」这一列没有依据）。
if (!mcpSource.includes("ToolRegistry::with_profiles") || !mcpSource.includes("tools_list_json")) {
  premise(`${MCP_LIB} 不再通过 ToolRegistry::with_profiles(...).tools_list_json() 列工具`);
}
const mcpProfiles = parseMcpDefaultProfiles(toolsSource);

// ---- 逐项计算 ----
const routed = tools.filter((tool) => direct.has(tool.name) || aliasRoutes.has(tool.name));
const missing = tools.filter((tool) => !direct.has(tool.name) && !aliasRoutes.has(tool.name));
const extras = [...direct].filter((name) => !registryNames.has(name)).sort();
const mcpDefault = tools.filter((tool) => mcpProfiles.has(tool.profile));

// 参数未读：只对「有本地路由」的工具有意义（没有分支就无从读参数）。
const deadByTool = new Map();
for (const tool of tools) {
  if (!direct.has(tool.name) && !aliasRoutes.has(tool.name)) continue;
  const params = specs.get(tool.name);
  if (!params) continue; // study 工具没有本地镜像行，不参与
  const dead = params.filter((param) => {
    const hits = (outside.match(new RegExp(`\\b${param}\\b`, "g")) || []).length;
    return hits === 0;
  });
  if (dead.length > 0) deadByTool.set(tool.name, dead);
}
const deadParams = [...deadByTool.values()].reduce((sum, list) => sum + list.length, 0);

// ---- 输出实测数（报告里的四个数就用这几行） ----
console.log("三方对齐（静态解析）：");
console.log(`  注册表工具 ${tools.length}｜MCP 缺省可见 ${mcpDefault.length}（Study 专属 ${tools.length - mcpDefault.length} 只需 --profile study）`);
console.log(`  本地镜像规格 ${specs.size} 行｜有本地路由 ${routed.length}｜缺本地路由 ${missing.length}`);
console.log(`  声明参数本地未读 ${deadParams}（${deadByTool.size} 个工具）｜本地反向多出 ${extras.length}`);
console.log("");

// **`--list` ⇒ 把缺口逐条打出来**（2026-11-05 加 ✓）：只打印，不改判定 ✓
//   **∴ 为什么需要** ✗：四个数只能说明「有 56 个」，**看不到是哪些** ⇒ 补哪个全靠猜 ✓。
if (process.argv.includes("--list")) {
  console.log("");
  console.log(`缺本地路由（${missing.length}）：`);
  for (const tool of missing) console.log(`  - ${tool.name}  [${tool.profile}]`);
  console.log("");
  console.log(`声明了但本地从不读的参数（${deadParams}）：`);
  for (const [tool, params] of [...deadByTool].sort((a, b) => a[0].localeCompare(b[0]))) {
    console.log(`  - ${tool}: ${params.join(", ")}`);
  }
  console.log("");
}

// ---- 断言 ----
check(
  registryNames.size === tools.length,
  `注册表工具名唯一`,
  `${tools.length} 条`,
);

// 本地镜像规格：每一个**非 Study** 的注册表工具都该有参数行；反向不许有幽灵行。
//   Study 是为「显式 --profile study」预留的参考图分析组，PWA 本地不接 ⇒ 允许缺。
//   **这不是写死 223**：新增 Study 工具时该恒等式照样成立（内核 ②：恒等式优于单点断言）。
const specMissing = tools
  .filter((tool) => tool.profile !== "Study" && !specs.has(tool.name))
  .map((tool) => tool.name);
const specGhost = [...specs.keys()].filter((name) => !registryNames.has(name));
check(
  specMissing.length === 0 && specGhost.length === 0,
  `本地规格块与注册表一一对应（Study 除外）`,
  specMissing.length || specGhost.length
    ? `缺 ${specMissing.join("、") || "无"}；多 ${specGhost.join("、") || "无"}`
    : `${specs.size} 行 = ${tools.length} − ${tools.length - specs.size} 个 Study 工具`,
);

// MCP 与注册表的结构关系：缺省会话应覆盖所有**本该可见**的工具。
//   排除两类「缺省不可见是设计」的 profile：
//     · Study —— 参考图分析组，必须显式 `--profile study` 点名；
//     · Semantic —— 预留不开发组（tools.rs 的注释），没有工具，将来有也不进缺省。
//   其余 profile 若被缺省漏掉 ⇒ 面板会报「未知工具」⇒ 红。
const MCP_DEFAULT_EXEMPT = new Set(["Study", "Semantic"]);
const mcpGaps = tools
  .filter((tool) => !MCP_DEFAULT_EXEMPT.has(tool.profile) && !mcpProfiles.has(tool.profile))
  .map((tool) => tool.name);
check(
  mcpGaps.length === 0,
  `MCP 缺省 profile 覆盖全部非 Study/Semantic 工具`,
  mcpGaps.length ? `漏 ${mcpGaps.join("、")}` : `${mcpDefault.length} 个`,
);

// 两份本地层必须逐字节相同（改一份必须镜像另一份）。
check(
  localSource === mirrorSource,
  `${LOCAL} 与 ${LOCAL_MIRROR} 逐字节相同`,
  // **∴ `.length` 是 UTF-16 码元数（774745）✗，不是字节 ✗** ⇒ 标成「字节」会误导 ✓
  //   **∴ 真实 UTF-8 字节用 Buffer.byteLength＝915872 ✓**（与 tool-web-assets-parity 报的 B 一致 ✓）
  localSource === mirrorSource
    ? `${Buffer.byteLength(localSource, "utf8")} 字节`
    : "两份已分叉 ⇒ 先镜像再提交",
);

// 反向多出：必须为空，或在允许名单里且写了原因。
const allowedNames = new Set(allowlist.map((entry) => entry.name));
const unexplained = extras.filter((name) => !allowedNames.has(name));
const noReason = allowlist.filter((entry) => !entry.reason).map((entry) => entry.name);
const stale = allowlist.filter((entry) => !extras.includes(entry.name)).map((entry) => entry.name);
check(
  unexplained.length === 0,
  `本地反向多出全在允许名单里`,
  unexplained.length ? `未登记：${unexplained.join("、")}` : `${extras.length} 个`,
);
check(
  noReason.length === 0,
  `允许名单每条都写了原因`,
  noReason.length ? `没写原因：${noReason.join("、")}` : `${allowlist.length} 条`,
);
check(
  stale.length === 0,
  `允许名单没有过期条目`,
  stale.length ? `已不再多出：${stale.join("、")}` : "无过期",
);

// 两个棘轮：只许降。
check(
  missing.length <= MISSING_CEILING,
  `缺本地路由 ${missing.length} ≤ 棘轮 ${MISSING_CEILING}`,
  missing.length > MISSING_CEILING
    ? `涨了 ${missing.length - MISSING_CEILING} 个 ⇒ 要么补路由，要么抬常量并写理由`
    : `余量 ${MISSING_CEILING - missing.length}`,
);
check(
  deadParams <= DEAD_PARAMS_CEILING,
  `参数未读 ${deadParams} ≤ 棘轮 ${DEAD_PARAMS_CEILING}`,
  deadParams > DEAD_PARAMS_CEILING
    ? `涨了 ${deadParams - DEAD_PARAMS_CEILING} 个 ⇒ 要么本地读它，要么在白名单登记并写理由`
    : `余量 ${DEAD_PARAMS_CEILING - deadParams}`,
);

if (failures.length > 0) {
  console.error(`\n✗ 三方对齐判据红 ${failures.length} 条`);
  process.exit(1);
}
console.log("\n✓ 三方对齐在棘轮内（缺口只许降）");
