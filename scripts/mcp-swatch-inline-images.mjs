#!/usr/bin/env node
// **`swatch_brush_strokes` 必须在 stdio 下直接交出 image 块** ✓（外部复测 `QX-G01` ✓）
//   ⇒ **∴ 他们实测**✗**：`single`／`area` 各 3 刷 ⇒ **3 个 URL、0 个 image 块**✗**
//      ⇒ **∴ 而**单刷 `brush_preview(include_image=true)` ⇒ **1 个 image 块**✓**
//   ⇒ **∴ 根因**✗**：**∴ `tool_result` 只从**顶层**✗ 取 `image`**✗** ⇒ **∴ 而** `swatch` 的图在**每项**✗ ✓
//   ⇒ **∴ 判据**✗**：**∴ 无 HTTP 服务**✗**（**∴ 纯 stdio ✓**）⇒ **∴ 数 image 块**✗** ✓
import { spawn } from "node:child_process";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const BIN = process.env.YANSHI_MCP_BIN || "target/release/yanshi-mcp";
const root = trackTemp(mkdtempSync(join(tmpdir(), "mcp-swatch-img-")));
const child = spawn(BIN, ["--root", root, "--doc", "s", "--width", "160", "--height", "120"], { stdio: ["pipe", "pipe", "pipe"] });
process.on("exit", () => { try { child.kill("SIGKILL"); } catch { /* 已经没了 */ } });

let buffer = ""; const seen = new Map();
child.stdout.on("data", (chunk) => {
  buffer += chunk; let i;
  while ((i = buffer.indexOf("\n")) >= 0) {
    const line = buffer.slice(0, i); buffer = buffer.slice(i + 1);
    try { const m = JSON.parse(line); if (m.id) seen.set(m.id, m); } catch { /* 忽略非 JSON */ }
  }
});
const send = (o) => child.stdin.write(JSON.stringify(o) + "\n");
const waitFor = async (id, ms = 30000) => {
  const end = Date.now() + ms;
  while (Date.now() < end) { if (seen.has(id)) return seen.get(id); await new Promise((r) => setTimeout(r, 80)); }
  return null;
};
send({ jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2024-11-05", capabilities: {}, clientInfo: { name: "judge", version: "1" } } });
await waitFor(1);
send({ jsonrpc: "2.0", method: "notifications/initialized", params: {} });
await new Promise((r) => setTimeout(r, 300));
let id = 10;
const call = async (tool, args) => {
  const cur = ++id;
  send({ jsonrpc: "2.0", id: cur, method: "tools/call", params: { name: tool, arguments: args } });
  return await waitFor(cur);
};
const failures = [];
const countImages = (reply) => (reply?.result?.content ?? []).filter((c) => c.type === "image" && c.data).length;

// **① 批量试笔 ⇒ 必须有多条 image 块** ✓
const batch = await call("swatch_brush_strokes", {
  brushes: ["ink-slowline_s", "calligraphy", "2B_pencil"],
  colors: ["#272720"], size: 12, include_image: true,
});
const n = countImages(batch);
const text = JSON.stringify(batch?.result ?? {}).slice(0, 200);
console.log(`  批量试笔 ⇒ image 块 ${n} 个｜${text}`);
if (batch?.result?.isError) failures.push("工具返回 isError ✗");
if (n < 3) failures.push(`批量试笔只交出 ${n} 个 image 块（期望 ≥3）⇒ stdio Agent 看不到对照 ✗（QX-G01）`);

// **② 移走的项要在 text 里说清** ✓（**∴ 不能静默丢 ✓**）
const body = batch?.result?.content?.find((c) => c.type === "text")?.text ?? "";
let parsed = {};
try { parsed = JSON.parse(body); } catch { /* 非 JSON */ }
console.log(`  inline_images=${JSON.stringify(parsed.inline_images)}`);
if (!parsed.inline_images || parsed.inline_images.count !== n) {
  failures.push("text 里没有 inline_images.count 或不等于实际交出的张数 ⇒ 静默丢图 ✗");
}

// **③ 单刷阳性对照 ⇒ 1 个 image 块** ✓（**∴ 否则② 的 0 可能只是"全都不行" ✓**）
const one = await call("brush_preview", { brush: "ink-slowline_s", size: 12, color: "#272720", include_image: true });
const m = countImages(one);
console.log(`  单刷对照 ⇒ image 块 ${m} 个`);
if (m < 1) failures.push("单刷 brush_preview 也没有 image 块 ⇒ 判据无效（② 的 0 无法归因）✗");

// **∴ 收尾要显式 kill**✗**（**∴ 子进程**挂着 stdin ⇒ **∴ 事件循环**不退出**✗ ⇒ **∴ 超时 124 ✓**）**
//   ⇒ **∴ 第 395 轮实测**✗**：判据**全绿**✗ 而**退出码 124**✗** ⇒ **∴ 那条红**是**判据自己**造的**✗** ✓✓
const finish = (code) => { try { child.kill("SIGKILL"); } catch { /* 已经没了 */ } process.exit(code); };
if (failures.length) { console.log("  ✗ " + failures.join("；")); finish(1); }
console.log(`  ✓ 批量试笔直接交出 ${n} 个 image 块，且 text 里 inline_images.count=${n}`);
finish(0);
