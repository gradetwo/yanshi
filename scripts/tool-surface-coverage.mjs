#!/usr/bin/env node
// **三个面的覆盖面对账**（第 803 轮）：服务端 `/api/tools`（权威全集）✓、MCP 各 profile ✓、
// 以及查看器实际调用的工具 ✓ —— 各自**实际暴露了什么**，而不是"应该一致"✓。
//
// **为什么需要它** ✗（第 802 轮的实测 ✓）：
//   ① `MCP ⊆ HTTP` 这个方向是干净的 ✓（MCP 独有 = 0 ✓），但**反过来差 68 个** ✓ ——
//      即 `profiles: ["core"]` 只暴露了 136 个里的 **一半** ✗，而 Web 查看器**真的在调**其中一批
//      （`clone_stamp` / `heal_stamp` / `smudge` / `liquify_pinch` / `create_mask` / `create_selection` /
//       `duplicate_layer` / `create_annotation` / `checkpoint` ✓）⇒ **同一份文档，人能做、Agent 看不到** ✓；
//   ② `Profile::Core` 的数量注释与文档头都写着「27 个」✗，而**实际是 68 个** ✓；
//   ③ **没有任何判据在对账"覆盖面"** ✗ ⇒ ①② 长期无人发现 ✓。
//
// **判据（每条都能红 ✓）**：
//   ① `MCP ⊆ HTTP`：任何 profile 里出现的工具，都必须在 `/api/tools` 里 ✓；
//   ② **每个 profile 单独启用时，必然包含 core 的全部工具** ✓（`with_profiles` 总是并入 Core ✓）；
//   ③ **声明 profile 集合必须与判据覆盖的集合一致** ✓（`/api/tools` 多出一个新 profile ⇒ 红 ✓，
//      否则新 profile 会**静默不被对账** ✗ —— 与"判据静默不运行"同一类病 ✓）；
//   ④ **源码里每一处「核心层 … N 个」都必须等于实际 core 数量** ✓（今天两处都写 27 ⇒ 修好后为 68 ✓）；
//   ⑤ **打印每个面的覆盖面** ✓ —— 让"绿"带上"覆盖了多少" ✓（第 800 轮的规矩 ✓）。
//
// 用法：node scripts/tool-surface-coverage.mjs <server-base>   （runner 会给 <base> <doc> <token> ✓）
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-surface-coverage.mjs <server-base>"); process.exit(2); }
const MCP_BIN = process.env.YANSHI_MCP_BIN || "target/debug/yanshi-mcp";
const TOOLS_RS = "crates/yanshi-server/src/tools.rs";

let bad = 0;
const fail = (message) => { bad += 1; console.log("  ✗ " + message); };

const toolsFor = (profiles) => {
  const out = execFileSync(MCP_BIN, ["--list-tools", "--profiles", profiles.join(",")],
    { encoding: "utf8", timeout: 120000 });
  return { set: new Set(JSON.parse(out).tools.map((t) => t.name)), raw: JSON.parse(out) };
};

// ① 权威全集：`/api/tools`（它同时声明了有哪些 profile ✓）
const catalog = await fetch(base.replace(/\/+$/, "") + "/api/tools").then((r) => r.json());
const http = new Set((catalog.tools || []).map((t) => t.name));
const declaredProfiles = (catalog.profiles || []).map(String);
if (http.size === 0) { console.error("  ✗ /api/tools 没返回任何工具 ⇒ 判据无法运行（不是通过）"); process.exit(1); }

const core = toolsFor(["core"]);
const perProfile = new Map(declaredProfiles.map((p) => [p, toolsFor([p]).set]));

// ② 每个 profile 单独启用时，必然包含 core 的全部工具 ✓
for (const profile of declaredProfiles) {
  const set = perProfile.get(profile);
  const missing = [...core.set].filter((name) => !set.has(name));
  if (missing.length > 0) {
    fail(`profile「${profile}」单独启用时**没有包含 core** 的 ${missing.length} 个工具（如 ${missing.slice(0, 5).join("、")}）`);
  }
}

// ③ MCP ⊆ HTTP ✓（任何 profile 都不许出现 HTTP 没有的工具 ✓）
for (const profile of declaredProfiles) {
  const extra = [...perProfile.get(profile)].filter((name) => !http.has(name));
  if (extra.length > 0) {
    fail(`profile「${profile}」里有 ${extra.length} 个工具是 HTTP 面没有的：${extra.slice(0, 6).join("、")}`);
  }
}

// ④ 声明 profile 集合 vs 判据覆盖的集合 ✓（避免新 profile 静默不被对账 ✗）
const KNOWN_PROFILES = ["core", "history", "changeset", "retouch", "conflict", "annotation", "collab", "structure"];
const unaccounted = declaredProfiles.filter((p) => !KNOWN_PROFILES.includes(p));
if (unaccounted.length > 0) {
  fail(`/api/tools 声明了本判据未覆盖的 profile：${unaccounted.join("、")} ⇒ 请把它加进 KNOWN_PROFILES 并给出判据` +
       `（否则它会静默不被对账 ✗）`);
}

// ⑤ 源码里每一处「核心层 … N 个」都必须等于实际 core 数量 ✓
const source = readFileSync(TOOLS_RS, "utf8");
const mentions = [...source.matchAll(/核心层[^\n]*?(\d+)\s*个/g)];
if (mentions.length === 0) {
  fail(`在 ${TOOLS_RS} 里找不到任何「核心层 … N 个」的说明 ⇒ 说明被删了或格式变了 ⇒ 不能静默通过 ✗`);
} else {
  for (const m of mentions) {
    const declared = Number(m[1]);
    if (declared !== core.set.size) {
      fail(`${TOOLS_RS} 里写「核心层 … ${declared} 个」，而实际 core = ${core.set.size} 个｜原文：${m[0].trim()}`);
    }
  }
}

// ⑥ **文档里写死的工具数也必须等于实测** ✓（第 807 轮 ✓）：这类数字会**静默过时** ✗ ——
// 实测过一次：网站的落地页与两份指南都写着「114 个工具（core 46 个）」✗，而真实是 136 / 68 ✓
//（那是我把过时数字**从 README 一起搬进指南**时发现的 ✗ ⇒ 搬运也会搬运错误 ✓）。
// ⇒ 找不到那句话就**报错**（不许静默通过 ✗ —— 改写了措辞等于**检查消失了** ✓）。
const docClaims = [
  { file: "docs/guide.md", re: /(\d+) tools against the (\d+) that core alone gives/ },
  // ⚠️ 中文那句里「个工具」与「（」之间夹着 `**`（粗体标记 ✓）⇒ 正则必须容忍它 ✓。
  { file: "docs/guide.zh-CN.md", re: /(\d+) 个工具\**（只开 core 是 (\d+) 个/ },
];
let docChecked = 0;
for (const { file, re } of docClaims) {
  const text = readFileSync(file, "utf8");
  const m = text.match(re);
  if (!m) {
    fail(`${file} 里找不到「工具数」那句话 ⇒ 要么改写了措辞、要么删了 ⇒ 检查随之消失 ✗（请更新本判据的正则）`);
    continue;
  }
  docChecked += 1;
  // ⚠️ **局部名不要用 `core`/`all`** ✗：`core` 是外层的 `{ set, raw }` ✓、`all` 是全局 ✓
  // ⇒ 遮蔽过一次，`core.set` 当场变成读一个数字 ⇒ `TypeError` ✓（**跑一次才发现的 ✓**）。
  const [, claimedTotal, claimedCore, sentence] = [m[0], Number(m[1]), Number(m[2]), m[0]];
  if (claimedTotal !== http.size || claimedCore !== core.set.size) {
    fail(`${file} 写「${sentence.trim()}」，而实测是 ${http.size} / core ${core.set.size}`);
  }
}

  // **Web 面**（目标 (C)②）：查看器实际调用的工具必须在 HTTP 面里。
  // **为什么能红**：viewer-app.js 里每出现一个 callTool(name)，而 name 不在 /api/tools 里，
  // 就是「界面能调、服务端没有」⇒ 用户点下去必失败。（2026-10-07 首次对账：这个方向当时是空的。）
  const WEB_JS = "crates/yanshi-http/assets/viewer-app.js";
  const web = new Set(
    (readFileSync(WEB_JS, "utf8").match(/callTool\("([a-z_0-9]+)"/g) || [])
      .map((t) => t.replace(/^callTool\("/, "").replace(/"$/, ""))
  );
  const webNotHttp = [...web].filter((name) => !http.has(name));
  if (webNotHttp.length > 0) {
    fail(`viewer 调用了 HTTP 面没有的工具（点下去必失败）：${webNotHttp.join("、")}`);
  }
  // **缺口：Web 用、而任何 profile 都拿不到** = 「人能做、Agent 看不到」。
  // **只记录、不判**：「Web 面独有的操作是否该让 Agent 也能做」是产品决策，
  // 既定设计只要求 MCP ⊆ HTTP 与「每 profile 必含 core」，没有说这一条。
  const anyProfile = new Set([...core.set, ...[...perProfile.values()].flatMap((v) => [...v])]);
  const webNoProfile = [...web].filter((name) => !anyProfile.has(name));
  // **参数名也要对账**（目标 (C)③）：viewer 是**唯一**手写参数对象的面，
  // 所以只有它可能把参数名写错（另两面共用同一份 inputSchema，结构上不会漂移）。
  // 做法：从 callTool("name", { 起，按**字符串／注释／括号深度**取深度 1 的 `ident:` 键名，
  // 再与该工具 inputSchema.properties 的名字比对。只看**字面量**参数；变量或函数调用无法静态看，单独计数。
  const schemaProps = new Map((catalog.tools || []).map((t) =>
    [t.name, new Set(Object.keys((t.inputSchema || {}).properties || {}))]));
  const topLevelKeys = (text, at) => {
    const keys = [];
    let depth = 0, j = at;
    const n = text.length;
    while (j < n) {
      const ch = text[j];
      if (ch === '"' || ch === "'" || ch === '`') {
        const q = ch; j += 1;
        while (j < n && text[j] !== q) j += text[j] === '\\' ? 2 : 1;
        j += 1; continue;
      }
      if (ch === '/' && text[j + 1] === '/') { const nl = text.indexOf('\n', j); j = nl < 0 ? n : nl; continue; }
      if (ch === '/' && text[j + 1] === '*') { const e = text.indexOf('*/', j); j = e < 0 ? n : e + 2; continue; }
      if ('{([ '.includes(ch) && ch !== ' ') depth += 1;
      else if ('})]'.includes(ch)) { depth -= 1; j += 1; }
      if (depth === 0) break;
      if (depth === 1) {
        const m = /^\s*([A-Za-z_$][\w$]*)\s*:/.exec(text.slice(j));
        if (m) { keys.push(m[1]); j += m[0].length; continue; }
      }
      j += 1;
    }
    return keys;
  };
  let argChecked = 0, argSkipped = 0;
  const badArgs = [];
  for (const m of readFileSync(WEB_JS, "utf8").matchAll(/callTool\(\s*"([a-z_0-9]+)"\s*,\s*/g)) {
    const name = m[1];
    let at = m.index + m[0].length;
    const src = readFileSync(WEB_JS, "utf8");
    while (at < src.length && ' \t\r\n'.includes(src[at])) at += 1;
    if (src[at] !== '{') { argSkipped += 1; continue; }
    const props = schemaProps.get(name);
    if (!props) continue;
    for (const key of topLevelKeys(src, at)) {
      if (!props.has(key)) badArgs.push(`${name}.${key}`);
    }
    argChecked += 1;
  }
  if (badArgs.length > 0) {
    fail(`viewer 传了 schema 里没有的参数名（写错 ⇒ 服务端会忽略或报错）：${badArgs.join("、")}`);
  }
  console.log(`  参数名对账：检查了 ${argChecked} 处字面量参数对象（另有 ${argSkipped} 处无法静态看，跳过）⇒ 越界 ${badArgs.length} 处`);
  console.log(`  Web 面：${web.size} 个工具（viewer 实际调用）｜任何 profile 都拿不到的 ${webNoProfile.length} 个（记录，不判）` +
    (webNoProfile.length > 0 ? `：${webNoProfile.join("、")}` : ""));

// ⑥ 打印覆盖面 ✓（让"绿"带上"覆盖了多少" ✓）
console.log(`  HTTP 面：${http.size} 个工具｜声明 profile：${declaredProfiles.join("、")}`);
console.log(`  MCP 默认（core）：${core.set.size} 个 ⇒ 覆盖 HTTP 的 ${(100 * core.set.size / http.size).toFixed(1)}%`);
for (const profile of declaredProfiles) {
  if (profile === "core") continue;
  const set = perProfile.get(profile);
  const added = [...set].filter((name) => !core.set.has(name));
  console.log(`    + ${profile.padEnd(10)} 启用后共 ${String(set.size).padStart(3)} 个（比 core 多 ${added.length}：` +
    `${added.slice(0, 5).join("、")}${added.length > 5 ? " …" : ""}）`);
}
console.log(`  源码里「核心层 N 个」的说明：${mentions.length} 处，均已与实际一致 ✓`);
console.log(`  文档里写死的工具数：${docChecked}/${docClaims.length} 处已核对 ✓`);

if (bad > 0) {
  console.log(`  结论：三个面之间存在 ${bad} 处不一致 ✗（覆盖面对账不通过）`);
  process.exit(1);
}
console.log("  ✓ 三个面的覆盖面对账通过（MCP ⊆ HTTP、profile 必含 core、说明与实现一致）");
