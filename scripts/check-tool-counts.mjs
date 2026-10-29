#!/usr/bin/env node
// **工具计数文字的防滞后判据** ✓（2026-10-29 加）。
//
// **它补的是什么** ✓：rebackground 带入主干后，合入树注册表是 209 工具 / 128 core，
// **而计数文字只改了指南** ✗ ⇒ 漏了 docs/tools.md、tool-examples.md、tools.rs 两处注释 ✗
// ⇒ CI 四项（test 双版的 documented_tool_counts_match_the_registry、1/6、2/6）全因此红 ✗
// ⇒ 事后才补跟进提交 ✗。**教训**：带入合入两支各加一工具时，必须按合入树实测总数**全处扫一遍** ✗，
// 不能只改指南 ✗。**⇒ 所以**这条判据在提交前就把全处扫一遍 ✓，而不是等 CI 红 ✗。
//
// **实测方法** ✓：每次运行从注册表源码**现数**，绝不写死总数 ✓。
//   · 总数 = `crates/yanshi-server/src/tools_registry.rs` 里 `ALL_TOOLS` 数组体的 `ToolSpec {` 条目数 ✓；
//   · core 数 = 其中 `profile: Profile::Core` 的条目数 ✓。
//   **⇒ 依据**：`ToolRegistry::full()` = `with_profiles(ALL)` = 全量；`with_profiles([Core])`
//   就是按 `tool.profile` 过滤（tools.rs 的 `tools()`）⇒ 静态计数与编译后计数同口径 ✓。
//   **⇒ feature 无关**：文件里仅有的两个 `#[cfg]` 只换 import_image 的参数表，注释明言「工具总数不变」✓
//   ⇒ 静态计数不随 `--features raw` 变化 ✓。
//   **⇒ 与 CI 判据同源**：tools.md 主行复用 `tool_inventory.rs` 的锚点法（`core tools` / `tools in total`
//   首次出现处向前找最近数字）；tools.rs 注释复用 `tool-surface-coverage.mjs` 的
//   `/核心层[^\n]*?(\d+)\s*个/g` 全量匹配法 ⇒ 两边不会各说各话 ✓。
//
// **只检查、不自动改写** ✓：各处格式不同（英文 prose、中文 prose、代码注释）✗
// ⇒ 自动改写有 corrupt prose 的风险 ✗ ⇒ 脚本只报 mismatch（文件、锚点、实测值、现值），改数由人做 ✓。
//
// **检查表维护规则** ✓：新增计数点时，只需在下面的 `CHECKS` 里加一行（文件 + 锚点解析）✓，
// 不要另起一份实现 ✓。**锚点找不到时判据必须红**（格式变了 ⇒ 不能静默通过 ✓，
// 与 `tool-surface-coverage.mjs` 的「找不到说明就红」同理 ✓）。
//
// **不在表里的数** ✓（扫全树后有意排除，见包说明）：
//   · `tools_registry.rs` 的 64/128/256 档位、128–2048、128px 缩略图、128 底色 ⇒ 参数／像素值，不是工具数 ✗；
//   · `implementation-notes.md` 等历史轮次叙述里的旧数（27/49/52/113…）⇒  dated 叙述，不是现行声明 ✗；
//   · tools.md 注里「HTTP 面实测 206 个（2026-10-26 注册表实测）」⇒ 带日期的探针记录，验它要活服务，不属静态检查 ✗；
//   · `tool-examples-doc.mjs` 的模板行 ⇒ 它**生成**计数，不是声明计数；被它生成的文档行在表里 ✓。
import { readFileSync } from "node:fs";

const REGISTRY = "crates/yanshi-server/src/tools_registry.rs";

// ---- 实测：从注册表源码数条目 ----
function measure() {
  const src = readFileSync(REGISTRY, "utf8");
  // 锚点必须带 `= &[` 初始化器：文件头文档注释里也引用了 `pub const ALL_TOOLS: &[ToolSpec]`，
  // 不带初始化器会先命中注释（实测踩中：切到参数表就收尾，条目数为 0）。
  const start = src.indexOf("pub const ALL_TOOLS: &[ToolSpec] = &[");
  if (start < 0) throw new Error(`${REGISTRY} 里找不到 ALL_TOOLS 声明 ⇒ 判据无法运行`);
  const rest = src.slice(start);
  const end = rest.indexOf("\n];");
  if (end < 0) throw new Error(`${REGISTRY} 里 ALL_TOOLS 数组没有收尾 ]; ⇒ 判据无法运行`);
  const body = rest.slice(0, end);
  // 按 4 空格缩进的 `ToolSpec {` 分块（条目都长这样；结构体定义在 tools.rs，不在本文件）；
  // 每块取第一个 `profile: Profile::X`，缺失则报错（不能静默少算）。
  const blocks = body.split("    ToolSpec {").slice(1);
  if (blocks.length === 0) throw new Error(`${REGISTRY} 里 ALL_TOOLS 没有任何条目 ⇒ 判据无法运行`);
  const byProfile = new Map();
  for (const block of blocks) {
    const m = block.match(/profile:\s*Profile::([A-Za-z]+)/);
    if (!m) throw new Error(`${REGISTRY} 里有 ToolSpec 条目缺 profile ⇒ 判据无法运行`);
    byProfile.set(m[1], (byProfile.get(m[1]) || 0) + 1);
  }
  const total = blocks.length;
  const core = byProfile.get("Core") || 0;
  const sum = [...byProfile.values()].reduce((a, b) => a + b, 0);
  if (sum !== total) throw new Error(`profile 分组求和 ${sum} ≠ 条目数 ${total} ⇒ 判据无法运行`);
  return { total, core, byProfile };
}

// ---- 锚点解析（都不用行号；行号一移就失效，锚点才稳） ----
// tool_inventory.rs 的找法：锚点首次出现处，向前找最近的数字（中间可夹空格／括号等非数字字符）。
function numberBefore(text, needle) {
  const index = text.indexOf(needle);
  if (index < 0) return null;
  const digits = text
    .slice(0, index)
    .split("")
    .reverse()
    .join("")
    .match(/^\D*(\d+)/);
  return digits ? parseInt(digits[1].split("").reverse().join(""), 10) : null;
}

function firstMatch(text, re) {
  const m = text.match(re);
  return m ? m.slice(1).map((x) => parseInt(x, 10)) : null;
}

// ---- 检查表：每行一个计数点（新增计数点只加一行） ----
// parse 返回 { core, total }（缺失的那项填 null 表示该点不声明它）；
// 找不到锚点时 parse 返回 null（不是通过，是红）。
const CHECKS = [
  {
    file: "docs/tools.md",
    label: "core tools / tools in total 主行",
    parse: (text) => {
      const core = numberBefore(text, "core tools");
      const total = numberBefore(text, "tools in total");
      return core === null || total === null ? null : { core, total };
    },
  },
  {
    file: "docs/tools.md",
    label: "上面那两个数字（core / total）注",
    parse: (text) => {
      const m = firstMatch(text, /上面那两个数字（(\d+) \/ (\d+)）/);
      return m ? { core: m[0], total: m[1] } : null;
    },
  },
  {
    file: "docs/design/tool-examples.md",
    label: "当前共 N 个工具带示例",
    parse: (text) => {
      const m = firstMatch(text, /当前共 (\d+) 个工具带示例/);
      // 该行数的是「带示例的工具」；示例齐全由 every_registered_tool_has_a_documented_example
      // 钉死（示例数 = 工具数），故与总数对账；示例缺了先红那条测试，不是本判据的口径问题。
      return m ? { core: null, total: m[0] } : null;
    },
  },
  {
    file: "crates/yanshi-server/src/tools.rs",
    label: "核心层 … N 个注释（全部）",
    parse: (text) => {
      const found = [...text.matchAll(/核心层[^\n]*?(\d+)\s*个/g)].map((m) => parseInt(m[1], 10));
      if (found.length === 0) return null;
      return { core: found, total: null };
    },
  },
  {
    file: "crates/yanshi-server/src/tools_registry.rs",
    label: "核心层 N 个（数组头注释）",
    parse: (text) => {
      const m = firstMatch(text, /核心层 (\d+) 个/);
      return m ? { core: m[0], total: null } : null;
    },
  },
  {
    file: "docs/guide.md",
    label: "N tools against the M that core alone gives",
    parse: (text) => {
      const m = firstMatch(text, /(\d+) tools against the (\d+) that core alone gives/);
      return m ? { total: m[0], core: m[1] } : null;
    },
  },
  {
    file: "docs/guide.zh-CN.md",
    label: "N 个工具（只开 core 是 M 个）",
    parse: (text) => {
      const m = firstMatch(text, /(\d+)\s*个工具[^（]*（只开 core 是\s*(\d+)\s*个/);
      return m ? { total: m[0], core: m[1] } : null;
    },
  },
];

let measured;
try {
  measured = measure();
} catch (e) {
  console.log(`  ⊘ 不可判：${e.message} ⇒ 不是通过`);
  process.exit(1);
}
console.log(`  实测注册表：core ${measured.core} 个 / 全部 ${measured.total} 个`);

const failures = [];
for (const check of CHECKS) {
  let text;
  try {
    text = readFileSync(check.file, "utf8");
  } catch (e) {
    console.log(`  ✗ ${check.file}（${check.label}）：读不到文件 ⇒ 不能静默通过`);
    failures.push(check.file);
    continue;
  }
  const declared = check.parse(text);
  if (declared === null) {
    console.log(`  ✗ ${check.file}（${check.label}）：锚点找不到（格式变了？）⇒ 不能静默通过`);
    failures.push(check.file);
    continue;
  }
  const problems = [];
  if (declared.core !== null && declared.core !== undefined) {
    const list = Array.isArray(declared.core) ? declared.core : [declared.core];
    for (const value of list) {
      if (value !== measured.core) problems.push(`core 现值 ${value}，实测 ${measured.core}`);
    }
  }
  if (declared.total !== null && declared.total !== undefined && declared.total !== measured.total) {
    problems.push(`总数现值 ${declared.total}，实测 ${measured.total}`);
  }
  if (problems.length === 0) {
    console.log(`  ✓ ${check.file}（${check.label}）`);
  } else {
    console.log(`  ✗ ${check.file}（${check.label}）：${problems.join("；")}`);
    failures.push(check.file);
  }
}

if (failures.length > 0) {
  console.log(`  ✗ 工具计数文字与注册表不符（${failures.length} 处）：按实测 core ${measured.core} / 总数 ${measured.total} 改数，不要只改一处`);
  process.exit(1);
}
console.log(`  ✓ 工具计数文字与注册表一致（core ${measured.core} / 总数 ${measured.total}）`);
