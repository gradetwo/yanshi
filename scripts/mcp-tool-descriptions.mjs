#!/usr/bin/env node
// **工具描述必须与实现一致、且说清"结果长什么样"**判据 ✓ —— 用**MCP 客户端真正的视角** ✓：
// 直接驱动 `yanshi-mcp` 的 `tools/list` ✓（不是看源码 ✗、也不是看 HTTP 目录 ✗ —— 两者字段并不相同 ✓）。
//
// 为什么需要它 ✓（三次报告的主因 ✓）：P0-2 的根因是 `new_document` 的描述**写着"会被拒绝"** ✗，
// 而实现早已是"同 id ⇒ 打开且不清空" ✓ ⇒ 报告作者据此判"无法续画" ✓；
// P0-1 的根因是 `include_image` **没说图在 MCP 的 `image` 内容块里** ✗ ⇒ 只读 text 的客户端判"没图" ✓。
// ⇒ 这条判据把"**描述与实现相反**"✗ 与"**没说清图在哪**"✗ 都钉死 ✓。
import { spawn } from "node:child_process";
const BIN = process.env.YANSHI_MCP_BIN || "target/debug/yanshi-mcp";
const child = spawn(BIN, ["--root", "/tmp/mcp-desc-judge", "--doc", "desc", "--width", "200", "--height", "150"],
  { stdio: ["pipe", "pipe", "pipe"] });
let buffer = "";
const seen = new Map();
child.stdout.on("data", (chunk) => {
  buffer += chunk;
  let index;
  while ((index = buffer.indexOf("\n")) >= 0) {
    const line = buffer.slice(0, index); buffer = buffer.slice(index + 1);
    try { const message = JSON.parse(line); if (message.id) seen.set(message.id, message); } catch { /* 非 JSON 行忽略 ✓ */ }
  }
});
const send = (object) => child.stdin.write(JSON.stringify(object) + "\n");
const waitFor = async (id, ms = 15000) => {
  const end = Date.now() + ms;
  while (Date.now() < end) { if (seen.has(id)) return seen.get(id); await new Promise((r) => setTimeout(r, 100)); }
  return null;
};
send({ jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2024-11-05", capabilities: {}, clientInfo: { name: "judge", version: "1" } } });
await waitFor(1);
send({ jsonrpc: "2.0", method: "notifications/initialized", params: {} });
await new Promise((r) => setTimeout(r, 300));
send({ jsonrpc: "2.0", id: 2, method: "tools/list", params: {} });
const reply = await waitFor(2, 20000);
child.kill();
const tools = ((reply || {}).result || {}).tools || [];
const by = new Map(tools.map((tool) => [tool.name, tool]));
const failures = [];
const check = (ok, label, detail) => { console.log(`  ${ok ? "✓" : "✗"} ${label}${detail ? ` ⇒ ${detail}` : ""}`); if (!ok) failures.push(label); };
check(tools.length > 0, "MCP tools/list 有工具", `${tools.length} 个`);
const doc = by.get("new_document") || {};
const docText = doc.description || "";
const paramsOf = (tool) => ((tool.inputSchema || {}).properties || {});
const includeText = (name) => (paramsOf(by.get(name) || {}).include_image || {}).description || "";
// ① 描述**不许**与实现相反 ✗（这是 P0-2 的根因 ✓）
check(!/会被拒绝/.test(docText), "new_document 描述不再声称「已存在的会被拒绝」（**实现是打开** ✓）", docText.slice(0, 60));
check(/已存在/.test(docText) && /打开/.test(docText), "new_document 描述写明了「已存在 ⇒ 打开」", docText.slice(0, 80));
check(/opened/.test(docText), "new_document 描述提到 opened 返回值", "");
// ② 图在哪必须说清 ✓（这是 P0-1 的根因 ✓）
for (const name of ["brush_preview", "render_region"]) {
  const text = includeText(name);
  check(/image/.test(text) && /内容块/.test(text),
    `${name} 的 include_image 说明了「图在 MCP 的 image 内容块里」`, text.replace(/\n/g, " ").slice(0, 90));
  check(/text/.test(text) && /yanshi:\/\/blob/.test(text),
    `${name} 的 include_image 说明了「text 里只有地址」`, "");
}
if (failures.length) { console.log(`  ✗ 描述未达标 ${failures.length} 条：${failures.join("；")}`); process.exit(1); }
console.log("  ✓ 工具描述与实现一致，且说清了「图在哪里」");
