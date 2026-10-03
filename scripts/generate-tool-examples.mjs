#!/usr/bin/env node
// 扩大"可复制调用示例"的覆盖：候选**从清单里取**（不猜名字）、**排除破坏性前缀**、
// **逐个实调**、**只把跑得通的写进 TOOL_EXAMPLES**（判据见 tool-example-acceptance.mjs）。
//
// 用法：node scripts/generate-tool-examples.mjs <server-base> [--max N] [--dry]
//
// 这套逻辑此前只活在一次性的 /tmp 脚本里，跑完就没了（不可复现）。现在收进来。
//
// ⚠ **两次教训，用之前必读**：
// 1. 「已存在」的判断**必须在 TOOL_EXAMPLES 这一段里**做 ✗ —— 在整文件里找名字会命中
//    ALL_TOOLS 里的工具名（假阳性 ⇒ 一条都加不上）；
// 2. **它只负责"加"，不保证"加完还能跑"** ✗ —— 实测过：候选里 `create_layer` 给的必填参数是空的
//    ⇒ 它的示例变成 `{}` ⇒ 后面所有依赖 `L1` 的示例全挂。**因此流程是**：
//    `--dry` 看一遍 ⇒ 写回 ⇒ **重编** ⇒ 跑 `scripts/tool-example-acceptance.mjs`（必须全绿）⇒
//    `scripts/tool-examples-doc.mjs` 刷新文档 ⇒ 门禁。**验收红就别提交** ✗。
import { readFileSync, writeFileSync } from "node:fs";

const base = process.argv[2];
const maxArg = process.argv.indexOf("--max");
const max = maxArg > 0 ? Number(process.argv[maxArg + 1]) : 10;
const dry = process.argv.includes("--dry");
if (!base) { console.error("用法: node scripts/generate-tool-examples.mjs <server-base> [--max N] [--dry]"); process.exit(2); }
const doc = "genex";
const created = await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 900, height: 640 }),
}).then((r) => r.json());
const token = created.token;
if (!token) { console.error("建文档没拿到 token ⇒ 判据无法运行（不是成功）"); process.exit(1); }
const call = (tool, args) =>
  fetch(`${base}/api/tools?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ tool, arguments: args }),
  }).then(async (r) => (r.ok ? r.json() : r.json().catch(() => ({ ok: false }))));

const catalogue = await call("__nope__", {}).then(() => null).catch(() => null);
const all = await fetch(`${base}/api/tools?doc=${doc}&token=${token}`).then((r) => r.json());
const tools = new Map((all.tools || []).map((tool) => [tool.name, tool]));

const path = "crates/yanshi-server/src/tools.rs";
const source = readFileSync(path, "utf8");
const start = source.indexOf("pub const TOOL_EXAMPLES");
const end = source.indexOf("];", start);
let block = source.slice(start, end);
// **解析要覆盖两种排版** ✗ —— `cargo fmt` 会把短条目压成一行（`("name", r#"…"#),`），
// 只认多行形态会漏判 ⇒ 漏判会导致"重复收录/覆盖已有条目"（第 4 轮就是这么把表弄坏的 ✗）。
const have = new Set(
  [...block.matchAll(/\("([^"]+)",\s*r#"/g)].map((match) => match[1]),
);
console.log(`  已有示例 ${have.size} 条；清单 ${tools.size} 个工具`);

await call("create_layer", { layer_id: "L1", name: "Layer 1" });
await call("brush_stroke", { layer_id: "L1", object_id: "o1", brush: "100%_Opaque", size: 40,
  points: [[60, 60, 1], [120, 60, 1]] });

const DESTRUCTIVE = ["delete", "remove", "clear", "reset", "drop"];
// **互斥/一次性工具，预先排除** ✗ —— 别"加完再删"（第 6 轮实测：`begin_transaction` 与
// `begin_changeset` **互斥**，同一个文档里跑验收必然红一条，害得整批连同 5 条好的被回退）。
const EXCLUDE = new Map([
  ["begin_transaction", "与 begin_changeset 互斥：同一会话只能开一个变更集/事务"],
  ["commit_transaction", "同上：没有 begin_transaction 时它无从谈起"],
  ["abort_transaction", "同上"],
]);
const valueFor = (name, kind) => {
  const k = String(kind || "").toLowerCase();
  if (["layer_id", "layer"].includes(name)) return "L1";
  if (["doc_id", "document"].includes(name)) return "demo";
  if (name === "object_id") return "o1";
  if (["number", "integer"].includes(k)) return 1;
  if (k === "boolean") return true;
  if (k === "array") return [];
  if (k === "object") return {};
  return "x";
};
const candidates = [...tools.keys()].filter((n) => !have.has(n) && !have.has(n + ".myb")
  && !DESTRUCTIVE.some((d) => n.includes(d)) && !EXCLUDE.has(n)).sort();
for (const [name, why] of EXCLUDE) {
  if (!have.has(name) && tools.has(name)) console.log(`  （预先排除）${name}：${why}`);
}
let added = 0;
for (const name of candidates) {
  if (added >= max) break;
  const schema = tools.get(name).inputSchema || {};
  const props = schema.properties || {};
  const args = {};
  for (const key of schema.required || []) args[key] = valueFor(key, (props[key] || {}).type);
  const outcome = await call(name, args);
  if (outcome.ok) {
    block = block.replace("&[\n", '&[\n    (\n        "' + name + '",\n        r#"' + JSON.stringify(args) + '"#,\n    ),\n');
    added += 1;
    console.log(`  ✓ ${name} 跑通 ⇒ 收录 ${JSON.stringify(args).slice(0, 60)}`);
  } else {
    console.log(`  ✗ ${name} 没跑通（不收录）⇒ ${String((outcome.context || {}).detail || "").slice(0, 56)}`);
  }
}
if (!dry) writeFileSync(path, source.slice(0, start) + block + source.slice(end));
console.log(`  本轮新增 ${added} 条${dry ? "（--dry：没写文件）" : ""}`);
