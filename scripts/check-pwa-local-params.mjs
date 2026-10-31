#!/usr/bin/env node
// **PWA 本地层「声明了却从未被读」的参数：棘轮守卫**
//
// **⇒ 为什么这条能用** ✗：`P2_TOOL_SPECS` 是从服务端 `/api/tools` 生成的 ✓ ⇒
//   任何参数名都会出现在 `web/api-local.js` 里 ✓。若某个参数名在**生成块之外全文件 0 次出现** ✓
//   ⇒ 本地层**没有用任何方式**读它（该文件没有按名字反射取参数：无 `body[k]` 循环、
//     无 `...body` 转发 ✓）⇒ **一定被忽略** ✓。这是**单向**判据：
//     · 0 次出现 ⇒ 必然没读 ✓（不会假阳性）
//     · 出现 > 0 次 ⇒ **不代表**读了 ✓（可能只是注释/同名字段）⇒ 不据此判红 ✓
//   ⇒ **∴ 代价**：通用名（`width`/`size`）会被别处的出现掩盖 ⇒ 可能漏报 ✓（宁可漏，不可假红）。
//
// **⇒ 两个方向都判**（棘轮 ✓）：
//   ① 新出现的「0 次」参数不在白名单 ⇒ 红（要么本地处理，要么在白名单登记理由）；
//   ② 白名单里的条目**现在不再是 0 次**（或工具不再被路由）⇒ 红（白名单过期，删掉那一行）。
//
// **⇒ 白名单**：`scripts/pwa-local-ignored-params.txt`（每行 `工具.参数`，`#` 注释）。
//
// **⇒ 变异**（`--self-test` 用合成源码验 ✓）：
//   ① 新参数没读且未登记 ⇒ 红；② 登记后 ⇒ 绿；③ 登记过但后来被读了 ⇒ 红（过期）；④ 登记了不存在的 ⇒ 红。
//
// 用法：node scripts/check-pwa-local-params.mjs [--self-test]
// 退出码：0 = 无新增静默忽略且白名单不过期｜1 = 有新增/过期｜3 = 读不到源码或生成块（不可判）
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const TARGET = "web/api-local.js";
const ALLOWLIST = "scripts/pwa-local-ignored-params.txt";
const BEGIN = "// <gen:tool-specs>";
const END = "// </gen:tool-specs>";

/// 判据本身（**可对合成源码/白名单跑** ✓ —— `--self-test` 就靠这个 ✓）。
export function judge(source, allowlistText) {
  const problems = [];
  const begin = source.indexOf(BEGIN);
  const end = source.indexOf(END);
  if (begin < 0 || end < 0 || end <= begin) {
    return { problems: ["找不到生成块 `// <gen:tool-specs>`…`// </gen:tool-specs>`"], dead: [] };
  }
  const spec = source.slice(begin, end);
  const outside = source.slice(0, begin) + source.slice(end);
  const entries = [];
  for (const line of spec.split("\n")) {
    const m = line.match(/^ {2}([a-z0-9_]+): \[/);
    if (!m) continue;
    const params = [...line.matchAll(/\["([a-z0-9_]+)","[a-z]",[01]\]/g)].map((x) => x[1]);
    entries.push({ tool: m[1], params });
  }
  if (entries.length === 0) return { problems: ["生成块里没有任何工具行"], dead: [] };
  const allow = new Set(
    allowlistText
      .split("\n")
      .map((line) => line.trim())
      .filter((line) => line && !line.startsWith("#"))
      .map((line) => line.split(/\s+/)[0]),
  );
  const dead = [];
  for (const entry of entries) {
    // 没被本地路由的工具不进判断（它连分支都没有 ⇒ 参数无从读起，也就无从「静默忽略」）。
    if (!outside.includes(`/api/tools/${entry.tool}`)) continue;
    for (const param of entry.params) {
      const hits = (outside.match(new RegExp(`\\b${param}\\b`, "g")) || []).length;
      if (hits === 0) dead.push(`${entry.tool}.${param}`);
    }
  }
  const deadSet = new Set(dead);
  const unknown = dead.filter((key) => !allow.has(key));
  const stale = [...allow].filter((key) => !deadSet.has(key));
  if (unknown.length > 0) {
    problems.push(
      "新声明却从未被本地读（处理它，或加进白名单并写理由）：" + unknown.join("、"),
    );
  }
  if (stale.length > 0) {
    problems.push(
      "白名单已过期（参数现在被读了／不在路由里了／不在生成块里）：删掉这些行：" + stale.join("、"),
    );
  }
  return { problems, dead };
}

const selfTest = process.argv.includes("--self-test");
if (selfTest) {
  const makeSource = (specLine, body) => `${BEGIN}\n${specLine}\n${END}\n${body}`;
  const spec = '  demo_tool: [0, [["alpha","s",1],["beta","n",0]]],';
  const readsAlpha = 'if (path === "/api/tools/demo_tool") { const a = body.alpha; }';
  const cases = [
    ["正例（唯一参数被读）", makeSource(spec, readsAlpha), "demo_tool.beta", []],
    ["变异①新参数没读且未登记", makeSource(spec, readsAlpha), "", ["从未被本地读"]],
    ["正例（登记后不报）", makeSource(spec, readsAlpha), "demo_tool.beta", []],
    ["变异②白名单过期（alpha 已被读）", makeSource(spec, readsAlpha), "demo_tool.alpha", ["已过期"]],
    ["变异③登记了不存在的参数", makeSource(spec, readsAlpha), "demo_tool.ghost", ["已过期"]],
    ["正例（没有生成块 ⇒ 不可判语句）", "no spec", "", ["找不到生成块"]],
  ];
  let bad = 0;
  for (const [name, src, allowText, want] of cases) {
    const got = judge(src, allowText).problems;
    const ok = want.length === 0 ? got.length === 0 : want.every((w) => got.some((g) => g.includes(w)));
    if (!ok) {
      bad += 1;
      console.log(`  ✗ 自测：${name}｜期望 ${JSON.stringify(want)}｜实测 ${JSON.stringify(got)}`);
    } else {
      console.log(`  ✓ 自测：${name}`);
    }
  }
  process.exit(bad === 0 ? 0 : 1);
}

let source;
let allowlistText;
try {
  source = readFileSync(join(ROOT, TARGET), "utf8");
  allowlistText = readFileSync(join(ROOT, ALLOWLIST), "utf8");
} catch (error) {
  console.error(`  ⊘ 不可判：读不到源码或白名单（${error.message}）（不是产品问题）`);
  process.exit(3);
}
const { problems, dead } = judge(source, allowlistText);
if (problems.length > 0) {
  console.error(`✗ PWA 本地参数守卫不成立（${problems.length} 条）：`);
  for (const problem of problems) console.error(`   - ${problem}`);
  process.exit(1);
}
console.log(
  `✓ PWA 本地参数：${dead.length} 个「声明了但从未被读」全在白名单里，且无过期条目`,
);
