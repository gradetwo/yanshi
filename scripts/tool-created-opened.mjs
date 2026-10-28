#!/usr/bin/env node
// **写路径建文档要回显**（B6）：回执加 `created: bool` + `opened: bool`
//   （复用 `new_document` 回执已有的同一对名字，不另造），由入口层填入 ——
//   调用前快照存在性（内存或磁盘任一存在即真），`registry.call` 之后插字段；
//   `new_document` / `import_project`（自建文档）跳过，自带这两个字段，不覆盖。
//
// **判据**（HTTP 臂 + MCP 臂 —— 写路径自动建文档是 MCP 入口的真实行为；
//   HTTP 的 `authorize` 会先拦住不存在的文档，所以 `created:true` 那支
//   只能在 MCP stdio 会话里验，HTTP 臂验已存在 / 只读 / `new_document` 自身）：
//   ① HTTP：`brush_stroke` 打到已存在的文档 ⇒ `created:false` + `opened:true`；
//   ② HTTP：只读工具打到不存在的 `doc_id` ⇒ `permission_denied`（05 复审 NEW-C1 ✓：授权浅探
//      先于开文档 ⇒ **对无权限调用方不报"不存在"**，存在性不外泄 ✓），
//      且文档列表里不出现该 id（无副作用）；
//   ③ HTTP：`new_document` 自身的 `created`/`opened` 不被入口覆盖 ——
//      新建 ⇒ `created:true`；同名再调 ⇒ `opened:true` + `created:false`；
//   ④ MCP：`brush_stroke` 打到不存在的 `doc_id` ⇒ `created:true` + `opened:false`，
//      事后 `list_documents` 可见该文档；
//   ⑤ MCP：同一文档再画一笔 ⇒ `created:false` + `opened:true`。
//
// **变异**：入口恒写 `created:false` ⇒ ④ 红；入口覆盖 `new_document` 自身的
//   字段（新建也报 `opened:true`）⇒ ③ 红；只读工具的守卫被去掉 ⇒ ② 红。
//   （MCP 臂 spawn `yanshi-mcp`：见 `mcp-document-switch.mjs` 同款做法。）
import { spawn } from "node:child_process";
import { existsSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";
const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-created-opened.mjs <base> <doc> <token>");
  process.exit(2);
}
const call = async (name, args, docId = doc, tok = token) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(docId)}&token=${encodeURIComponent(tok)}`;
  const response = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(args ?? {}),
  });
  return response.json();
};
const fail = (message) => {
  console.error(`  ✗ ${message}`);
  process.exit(1);
};
const fails = [];
const check = (ok, msg) => {
  if (!ok) fails.push(msg);
  console.log(`  ${ok ? "✓" : "✗"} ${msg}`);
};
const ids = async () => {
  const listing = await (
    await fetch(`${base}/api/documents?token=${encodeURIComponent(token)}`)
  ).json();
  return (listing.documents ?? listing ?? []).map((entry) =>
    typeof entry === "string" ? entry : entry.doc_id,
  );
};

const strokePoints = [
  [40, 60, 0.8],
  [280, 180, 0.8],
];

// ① 已存在的文档上落笔 ⇒ created:false + opened:true
const layer = await call("create_layer", { layer_id: "L_created", name: "created" });
if (layer.ok !== true && !String(layer.context?.detail ?? "").includes("已存在")) {
  fail(`create_layer ⇒ ${JSON.stringify(layer).slice(0, 120)}`);
}
const existing = await call("brush_stroke", {
  layer_id: "L_created",
  brush: "calligraphy",
  points: strokePoints,
  size: 30,
  color: [0.2, 0.4, 0.9, 1.0],
});
if (existing.ok !== true) fail(`① brush_stroke（已存在文档）⇒ ${JSON.stringify(existing).slice(0, 160)}`);
check(existing.created === false, `① 已存在文档 ⇒ created=${existing.created}（应为 false）`);
check(existing.opened === true, `① 已存在文档 ⇒ opened=${existing.opened}（应为 true）`);

// ② 只读工具打到不存在的 doc ⇒ permission_denied（NEW-C1：无权限者拿不到"不存在"这个码 ✓），且不建文档
const fake = `no_such_doc_${Date.now().toString(36)}`;
const before = await ids();
if (before.includes(fake)) {
  console.log(`  ⊘ 不可判：伪造 id ${fake} 居然已经存在`);
  process.exit(3);
}
const readMissing = await call("get_document", {}, fake, token);
check(readMissing.ok === false, `② 只读工具打不存在的 doc ⇒ ok=${readMissing.ok}（应为 false）`);
check(
  readMissing.error_code === "permission_denied",
  `② 错误码=${readMissing.error_code}（应为 permission_denied ⇒ 存在性不外泄 ✓）`,
);
const after = await ids();
check(!after.includes(fake), `② 调用之后文档列表里没有 ${fake}（只读工具不建文档）`);

// ③ new_document 自身的 created/opened 不被入口覆盖
const freshDoc = `b6_new_${Date.now().toString(36)}`;
const created = await call("new_document", { doc_id: freshDoc, width: 320, height: 240 });
if (created.ok !== true) fail(`③ new_document（新建）⇒ ${JSON.stringify(created).slice(0, 160)}`);
check(created.created === true, `③ 新建 ⇒ created=${created.created}（应为 true）`);
check(created.opened === false, `③ 新建 ⇒ opened=${created.opened}（应为 false）`);
const reopened = await call("new_document", { doc_id: freshDoc, width: 320, height: 240 });
if (reopened.ok !== true) fail(`③ new_document（同名再调）⇒ ${JSON.stringify(reopened).slice(0, 160)}`);
check(reopened.opened === true, `③ 同名再调 ⇒ opened=${reopened.opened}（应为 true）`);
check(reopened.created === false, `③ 同名再调 ⇒ created=${reopened.created}（应为 false）`);

// ④⑤ MCP：写路径自动建文档（HTTP 的 authorize 会先拦，故这支只能走 MCP）
const BIN = process.env.YANSHI_MCP_BIN || "target/release/yanshi-mcp";
if (!existsSync(BIN)) {
  console.log(`  ⊘ 不可判：MCP 二进制不存在（${BIN}）⇒ ④⑤ 臂跑不了`);
  process.exit(3);
}
const root = trackTemp(mkdtempSync(join(tmpdir(), "mcp-created-opened-")));
const freshMcp = `b6_mcp_${Date.now().toString(36)}`;
const child = spawn(BIN, ["--root", root, "--doc", "_bootstrap", "--width", "320", "--height", "240"], {
  stdio: ["pipe", "pipe", "pipe"],
});
process.on("exit", () => {
  try {
    child.kill("SIGKILL");
  } catch {
    /* 已经没了 */
  }
  // 临时目录的清理由 `trackTemp`（`scripts/lib/temp-dirs.mjs`）接管 ✓
});
let buffer = "";
const seen = new Map();
child.stdout.on("data", (chunk) => {
  buffer += chunk;
  let index;
  while ((index = buffer.indexOf("\n")) >= 0) {
    const line = buffer.slice(0, index);
    buffer = buffer.slice(index + 1);
    try {
      const message = JSON.parse(line);
      if (message.id) seen.set(message.id, message);
    } catch {
      /* 忽略非 JSON */
    }
  }
});
const send = (object) => child.stdin.write(`${JSON.stringify(object)}\n`);
const waitFor = async (id, ms = 20000) => {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    if (seen.has(id)) return seen.get(id);
    await new Promise((r) => setTimeout(r, 100));
  }
  return null;
};
send({
  jsonrpc: "2.0",
  id: 1,
  method: "initialize",
  params: { protocolVersion: "2024-11-05", capabilities: {}, clientInfo: { name: "judge", version: "1" } },
});
await waitFor(1);
send({ jsonrpc: "2.0", method: "notifications/initialized", params: {} });
await new Promise((r) => setTimeout(r, 300));
let nextId = 10;
const mcpCall = async (tool, args) => {
  const current = ++nextId;
  send({ jsonrpc: "2.0", id: current, method: "tools/call", params: { name: tool, arguments: args } });
  const reply = await waitFor(current);
  if (!reply) return null;
  const text = reply.result?.content?.[0]?.text ?? "";
  try {
    return JSON.parse(text);
  } catch {
    return { _unparsed: text.slice(0, 120), _isError: reply.result?.isError };
  }
};
// 新文档自带默认图层 `layer_default` ⇒ 第一笔可直接落，不必先建图层
// （先调别的工具会先把文档建出来，④ 臂就测不到了）。
const firstStroke = await mcpCall("brush_stroke", {
  doc_id: freshMcp,
  layer_id: "layer_default",
  brush: "calligraphy",
  points: strokePoints,
  size: 30,
  color: [0.2, 0.4, 0.9, 1.0],
});
if (!firstStroke || firstStroke.ok !== true) {
  fail(`④ MCP brush_stroke（不存在的 doc）⇒ ${JSON.stringify(firstStroke ?? null).slice(0, 160)}`);
}
check(firstStroke.created === true, `④ 不存在的 doc 上落笔 ⇒ created=${firstStroke.created}（应为 true）`);
check(firstStroke.opened === false, `④ 不存在的 doc 上落笔 ⇒ opened=${firstStroke.opened}（应为 false）`);
const listing = await mcpCall("list_documents", {});
const listedIds = (listing?.documents ?? []).map((entry) =>
  typeof entry === "string" ? entry : entry.doc_id,
);
check(listedIds.includes(freshMcp), `④ 事后 list_documents 可见 ${freshMcp}（列表 ${listedIds.length} 份）`);
const secondStroke = await mcpCall("brush_stroke", {
  doc_id: freshMcp,
  layer_id: "layer_default",
  brush: "calligraphy",
  points: strokePoints,
  size: 30,
  color: [0.2, 0.4, 0.9, 1.0],
});
if (!secondStroke || secondStroke.ok !== true) {
  fail(`⑤ MCP brush_stroke（已存在）⇒ ${JSON.stringify(secondStroke ?? null).slice(0, 160)}`);
}
check(secondStroke.created === false, `⑤ 已存在文档 ⇒ created=${secondStroke.created}（应为 false）`);
check(secondStroke.opened === true, `⑤ 已存在文档 ⇒ opened=${secondStroke.opened}（应为 true）`);
child.kill();

if (fails.length) {
  console.log(`\n  ✗ ${fails.length} 处不符（写路径建文档回显，B6）`);
  process.exit(1);
}
console.log("\n  ✓ 写路径建文档回显：HTTP 已存在/只读/new_document 自身 + MCP 新建/再画全对");
