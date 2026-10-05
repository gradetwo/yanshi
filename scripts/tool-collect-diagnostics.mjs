#!/usr/bin/env node
// **诊断包：MCP 面与 Web 面的条目必须完全一致**（判据 5：parity ✓）。
//
// 根要求（产品负责人）：**一个实现、两个面** ✓ —— 只在一个面上有的能力算**没做完** ✗。
// 这条判据真的把两个面都跑一遍 ✓：
//   ① Web 面：`GET /api/diagnostics?doc=&token=` ⇒ **真的收下一个 zip 文件** ✓；
//   ② MCP 面：spawn `target/debug/yanshi-mcp` ⇒ `tools/call collect_diagnostics`（带 `path`）✓
//      ⇒ **真的收下一个 zip 文件** ✓（不是只看它自报的 `entries` 数组 ✓ —— 那只能证明它自己说了什么 ✓）；
//   ③ 两侧都用**真的 zip 读取器**（Python 的 `zipfile` ✓）列出条目 ✓，再逐字比对 ✓。
//
// **为什么断言"条目名"而不是"字节"** ✓：两个面本来就该带上各自的事实（MCP 的 transport=stdio ✓、
// Web 的 bind ✓），字节因此不可能相同 ✓；会漂移的是**条目清单** ✓，那才是"内容不能分叉"的落点 ✓。
//
// **能红**（都实测过 ✓）：
//   * 把 `ENTRY_NAMES` 里任一条目在某一面单独去掉/改名 ⇒ 两侧清单不等 ⇒ 红 ✓；
//   * 把 `GET /api/diagnostics` 路由删掉 ⇒ Web 面拿不到 zip ⇒ 红 ✓；
//   * 把 `collect_diagnostics` 从注册表/分发里去掉 ⇒ MCP 面调用报错 ⇒ 红 ✓；
//   * 把 zip 写成非 zip ⇒ `zipfile` 打不开 ⇒ 红 ✓。
//
// 用法（runner 会给 <base> <doc> <token>）：node scripts/tool-collect-diagnostics.mjs <base> <doc> <token>
import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const base = (process.argv[2] || "").replace(/\/+$/, "");
const doc = process.argv[3] || "diag_judge";
const token = process.argv[4] || "";
if (!base || !token) {
  console.error("用法: node scripts/tool-collect-diagnostics.mjs <server-base> <doc> <token>");
  process.exit(2);
}
const MCP_BIN = process.env.YANSHI_MCP_BIN || "target/debug/yanshi-mcp";

let bad = 0;
const check = (ok, label, detail) => {
  console.log(`  ${ok ? "✓" : "✗"} ${label}${detail ? ` ⇒ ${detail}` : ""}`);
  if (!ok) bad += 1;
};

// **必须与 `crates/yanshi-server/src/diagnostics.rs` 的 `ENTRY_NAMES` 逐字一致** ✓。
// 这里**故意抄一份** ✓：抄的一份与实现分叉 ⇒ 本判据红 ✓（而不是"跟着实现一起改"就永远绿 ✗）。
const REQUIRED = [
  "README.txt", "build.json", "config.json", "surface.json", "document.json",
  "atoms.jsonl", "atoms.meta.json", "warnings.json", "timings.json",
  "stderr.log", "stderr.meta.json", "thumbnail.json", "thumbnail.bin", "privacy.json",
];

const work = mkdtempSync(join(tmpdir(), "yanshi-diag-"));
const PY_LIST = [
  "import json,sys,zipfile",
  "z=zipfile.ZipFile(sys.argv[1])",
  "b=z.testzip()",
  "print(json.dumps({'names':z.namelist(),'bad':b}))",
].join("\n");
const zipNames = (path) => {
  const out = execFileSync("python3", ["-c", PY_LIST, path], { encoding: "utf8" });
  return JSON.parse(out);
};

// ---- 让这份文档真的有内容（原子 / 渲染 / 缩略图）--------------------------------
const call = async (tool, args) => {
  const response = await fetch(`${base}/api/tools/${tool}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(args || {}),
  });
  return await response.json();
};
const filled = await call("fill_region", {
  layer_id: "layer_default",
  shape: { type: "rect", x: 4, y: 4, w: 120, h: 90 },
  color: { r: 30, g: 90, b: 160, a: 255 },
});
check(filled && filled.ok !== false, "前置：往被测文档里画一笔（fill_region）",
  filled && filled.ok === false ? JSON.stringify(filled).slice(0, 160) : `head=${filled && filled.head_seq}`);

// ---- ① Web 面：真的下载一个 zip -------------------------------------------------
const webUrl = `${base}/api/diagnostics?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
const webResponse = await fetch(webUrl);
check(webResponse.ok, "Web GET /api/diagnostics 返回 2xx", `status=${webResponse.status}`);
const webType = webResponse.headers.get("content-type") || "";
check(/application\/zip/.test(webType), "Web 响应的 Content-Type 是 application/zip", webType);
const webBytes = Buffer.from(await webResponse.arrayBuffer());
check(webBytes.length > 0, "Web 面真的交出了字节", `${webBytes.length} B`);
const webPath = join(work, "web.zip");
writeFileSync(webPath, webBytes);
const headerEntries = (webResponse.headers.get("x-yanshi-diagnostics-entries") || "")
  .split(",").map((s) => s.trim()).filter(Boolean);

// ---- ② MCP 面：spawn 真进程、调工具、拿 path 指向的 zip --------------------------
const root = join(work, "mcp-root");
const mcpZip = join(work, "mcp.zip");
const child = spawn(MCP_BIN, ["--root", root, "--doc", doc, "--width", "200", "--height", "150"],
  { stdio: ["pipe", "pipe", "pipe"] });
let stdout = "";
let stderr = "";
const seen = new Map();
child.stdout.on("data", (chunk) => {
  stdout += chunk;
  let index;
  while ((index = stdout.indexOf("\n")) >= 0) {
    const line = stdout.slice(0, index);
    stdout = stdout.slice(index + 1);
    try {
      const message = JSON.parse(line);
      if (message.id) seen.set(message.id, message);
    } catch { /* 非 JSON 行忽略 */ }
  }
});
child.stderr.on("data", (chunk) => { stderr += chunk; });
const send = (object) => child.stdin.write(JSON.stringify(object) + "\n");
const waitFor = async (id, ms = 30000) => {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    if (seen.has(id)) return seen.get(id);
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  return null;
};
send({ jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2024-11-05", capabilities: {}, clientInfo: { name: "diag-judge", version: "1" } } });
await waitFor(1);
send({ jsonrpc: "2.0", method: "notifications/initialized", params: {} });
send({ jsonrpc: "2.0", id: 2, method: "tools/call", params: { name: "collect_diagnostics", arguments: { path: mcpZip } } });
const reply = await waitFor(2);
child.kill();
check(reply !== null, "MCP collect_diagnostics 有响应", reply === null ? stderr.slice(-200) : "");
const text = ((((reply || {}).result || {}).content || [])[0] || {}).text || "{}";
let mcpValue = {};
try { mcpValue = JSON.parse(text); } catch { /* 下面会报 */ }
check(mcpValue.ok !== false, "MCP collect_diagnostics 返回 ok", mcpValue.ok === false ? text.slice(0, 200) : "");
check(typeof mcpValue.archive_path === "string", "MCP 把 zip 落到了 archive_path", mcpValue.archive_path || "");
const mcpReported = Array.isArray(mcpValue.entries) ? mcpValue.entries : [];
check(mcpReported.join(",") === REQUIRED.join(","),
  "MCP 自报的 entries 等于固定清单", `自报 ${mcpReported.length} 条`);

// ---- ③ 两侧都用真 zip 读取器列条目，逐字比对 -------------------------------------
let web = { names: [], bad: "unreadable" };
let mcp = { names: [], bad: "unreadable" };
try { web = zipNames(webPath); } catch (error) { check(false, "Web zip 能被 Python zipfile 打开", String(error).slice(0, 160)); }
try { mcp = zipNames(mcpZip); } catch (error) { check(false, "MCP zip 能被 Python zipfile 打开", String(error).slice(0, 160)); }
check(web.bad === null || web.bad === undefined, "Web zip 的 CRC 自检通过（testzip）", String(web.bad));
check(mcp.bad === null || mcp.bad === undefined, "MCP zip 的 CRC 自检通过（testzip）", String(mcp.bad));
check(web.names.join(",") === REQUIRED.join(","), "Web 面的条目清单等于固定清单",
  `实际 ${web.names.length} 条：${web.names.join(",")}`);
check(mcp.names.join(",") === REQUIRED.join(","), "MCP 面的条目清单等于固定清单",
  `实际 ${mcp.names.length} 条：${mcp.names.join(",")}`);
check(web.names.join(",") === mcp.names.join(","), "**两面条目逐字相同**（这是本判据的核心）",
  `Web=${web.names.length}｜MCP=${mcp.names.length}`);
check(headerEntries.join(",") === web.names.join(","), "Web 响应头里的条目名与包内实际条目一致",
  headerEntries.join(","));

// ---- ④ 包体里不许出现当前令牌（privacy 硬要求；真实字节上扫）----------------------
const key = process.env.YANSHI_API_KEY || "";
const secrets = [token, key].filter((value) => value && value.length >= 8);
for (const secret of secrets) {
  const label = secret === token ? "当前 capability token" : "YANSHI_API_KEY";
  const inWeb = webBytes.includes(Buffer.from(secret));
  const inMcp = readFileSync(mcpZip).includes(Buffer.from(secret));
  check(!inWeb, `Web zip 的真实字节里没有 ${label}`);
  check(!inMcp, `MCP zip 的真实字节里没有 ${label}`);
}

rmSync(work, { recursive: true, force: true });
if (bad > 0) {
  console.log(`  ✗ 诊断包判据未通过 ${bad} 条`);
  process.exit(1);
}
console.log(`  ✓ 诊断包：两面条目一致（${REQUIRED.length} 条），zip 可读，且未泄漏令牌`);
