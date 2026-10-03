#!/usr/bin/env node
// **「新建文档后，后续工具必须作用在新文档上」判据** ✓（用户报告 2.2 ✓ MCP 会话级上下文锁定 ✗）。
// 现象 ✓：`new_document` 回成功 ✓，但之后 `create_layer` 等**仍然画在启动参数 `--doc` 那个旧文档** ✗
// ⇒ 客户端画第二幅画只能**重启 MCP 进程** ✗。
// 判据 ✓：**不猜** ✓ —— 直接看磁盘：这个新图层到底落进了哪个文档的原子文件里 ✓。
import { spawn } from "node:child_process";
import { readFileSync, readdirSync, rmSync, mkdirSync } from "node:fs";
const BIN = process.env.YANSHI_MCP_BIN || "target/debug/yanshi-mcp";
const root = "/tmp/mcp-switch-judge";
rmSync(root, { recursive: true, force: true });
mkdirSync(root, { recursive: true });
const child = spawn(BIN, ["--root", root, "--doc", "first", "--width", "120", "--height", "90"], { stdio: ["pipe", "pipe", "pipe"] });
let buffer = ""; const seen = new Map();
child.stdout.on("data", (chunk) => {
  buffer += chunk; let index;
  while ((index = buffer.indexOf("\n")) >= 0) {
    const line = buffer.slice(0, index); buffer = buffer.slice(index + 1);
    try { const message = JSON.parse(line); if (message.id) seen.set(message.id, message); } catch { /* 忽略非 JSON ✓ */ }
  }
});
const send = (object) => child.stdin.write(JSON.stringify(object) + "\n");
const waitFor = async (id, ms = 20000) => {
  const end = Date.now() + ms;
  while (Date.now() < end) { if (seen.has(id)) return seen.get(id); await new Promise((r) => setTimeout(r, 100)); }
  return null;
};
send({ jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2024-11-05", capabilities: {}, clientInfo: { name: "judge", version: "1" } } });
await waitFor(1);
send({ jsonrpc: "2.0", method: "notifications/initialized", params: {} });
await new Promise((r) => setTimeout(r, 300));
let id = 10;
const call = async (tool, args) => {
  const current = ++id;
  send({ jsonrpc: "2.0", id: current, method: "tools/call", params: { name: tool, arguments: args } });
  return await waitFor(current);
};
const make = await call("new_document", { doc_id: "second", width: 120, height: 90 });
console.log("  new_document(second) ⇒ " + JSON.stringify((make || {}).result || (make || {}).error || {}).slice(0, 160));
const layer = await call("create_layer", { layer_id: "probe_layer", name: "探针层" });
console.log("  create_layer(probe_layer) ⇒ " + JSON.stringify((layer || {}).result || (layer || {}).error || {}).slice(0, 160));
await new Promise((r) => setTimeout(r, 600));
child.kill();
// **看磁盘** ✓：哪个文档的原子文件里出现了 `probe_layer` ✓
const hits = [];
// 实测：文档在 `root/docs/<doc_id>/…` ✓（我先前只走了一层 ⇒ 当然找不到 ✓）
const docsDir = root + "/docs";
for (const name of readdirSync(docsDir)) {
  const path = docsDir + "/" + name;
  try {
    const stat = readdirSync(path);
    for (const f of stat) {
      const full = path + "/" + f;
      let text = "";
      try { text = readFileSync(full, "utf8"); } catch { continue; }
      if (text.includes("probe_layer")) hits.push(name + "/" + f);
    }
  } catch { /* 不是目录 ✓ */ }
}
console.log("  含 probe_layer 的文档目录：" + JSON.stringify(hits));
const failures = [];
if (!hits.length) failures.push("磁盘上找不到 probe_layer ⇒ 判据无效（也许它只在内存里）");
else {
  const where = new Set(hits.map((h) => h.split("/")[0]));
  if (where.has("second") && !where.has("first")) console.log("  ✓ 新图层落在新文档 second 上（会话上下文已切换 ✓）");
  else failures.push(`新图层落在 ${[...where].join(",")}（应当只在 second ✓）⇒ 会话仍钉在旧文档 ✗`);
}
if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
process.exit(0);
