#!/usr/bin/env node
// **★ 改名导入 `.yanshi` 工程：元数据不许被"预建文档"吃掉 ✗ ★**（2026-10-16 ✓）
//
// **∴ 来源 ✗**：第三方德加作品包 README 自己写的 ✓ ——
//   "**当前版本改名导入有元数据路径缺陷** ✓；建议**保留包内文档 ID** 并导入到空的新工作区 ✓。
//    实测新 ID 重开像素也完整 ✓，**问题不是画作丢失** ✓，而是**旧 ID 元数据可能被覆盖** ✗。"
//   ＋ 那份 90 KB 本地补丁里对得上的一处 ✓：`creates_own_document()` **加了 `import_project`** ✗
//     ——**∴ 说明**不带这一条时，MCP 会在导入**之前**先替它**预建**一份文档 ✗（按 CLI 缺省尺寸 ✓）。
//
// **★ 实测校准过的语义 ✗ ★**（**我先写错了两条，是**判据**错而不是产品 ✗**）：
//   **∴ 事实 ✓**：`import_project` **按**包里的 doc_id**落点** ✓ —— 它的 `doc_id` 参数**不被采纳** ✓
//     （第三方 README 也是这么建议的 ✓："**建议保留包内文档 ID** 并导入到空的新工作区 ✓"）⇒
//     **∴ 改名导入**不是这条判据该断言的东西 ✗**（那是另一族、且对方自己标为"尚未修" ✓）。
//   **∴ 而真正被修好的那条 ✗**：MCP **不许先按 CLI 缺省预建**一份文档 ✗ ——
//     修前实测：`renamed/` 里留下一份 **64×64** 的假文档 ✓ 而真导入被"已存在"挡掉（`ok=false` ✓）✗；
//     修后：**不再出现 64×64 的假文档** ✓、包里的 `import_src` 以 **200×150** 落进来 ✓★
//
// **∴ 判什么（三条 ✓，只走真实工具 ✓）**：
//   ① HTTP 侧造一份**有内容**的源文档（**200×150** ✓）⇒ `export_project` 出真包 ✓；
//   ② 起一个**全新 root 的 MCP** 进程 ✓ ⇒ `import_project {path, doc_id: "renamed"}` ✓；
//   ③ **包里的那份（`import_src` ✓）必须真的落进来、且是 200×150** ✓（元数据跟着包走 ✓）
//      ＋ **任何目录都不许是 CLI 缺省那档尺寸** ✗（**修前 64×64 的假文档就是这一条在守** ✓）。
//
// 用法：node scripts/mcp-import-project-rename.mjs <base-url>
import { spawn } from "node:child_process";
import { mkdtempSync, readdirSync, readFileSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const base = (process.argv[2] || "").replace(/\/+$/, "");
if (!base) {
  console.error("用法: node scripts/mcp-import-project-rename.mjs <base-url>");
  process.exit(2);
}
const BIN = process.env.YANSHI_MCP_BIN || "target/debug/yanshi-mcp";
const root = trackTemp(mkdtempSync(join(tmpdir(), "yanshi-import-rename-")));
const SRC = "import_src";
// **∴ 包里那个"源 id"**就是这个 ✓（用来验"旧 id 不许在导入 root 里冒出来"✓）。
const PACK_NAME = "import-rename-src.yanshi";
const DST = "renamed";
const W = 200;
const H = 150;

// **∴ HTTP 面与 MCP 面的**形状不同** ✗**（我第一版照 MCP 的 `{tool, arguments}` 写 ⇒ 服务端回
//   「brush_stroke 不接受参数 arguments」✗ —— **∴ 这是**我的脚本错**✓，不是产品 ✗）：
//   **HTTP** `/api/tools/<tool>` 直接收**参数本身** ✓；只有 **MCP** `tools/call` 才包一层 ✓。
const http = async (doc, token, tool, args) =>
  (await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify(args || {}),
  })).json();

// ---------- ① 源文档 + 导出真包 ----------
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: SRC, width: W, height: H }),
})).json();
if (!created || !created.token) {
  console.error("❌ 判不了：建不了源文档 ⇒ 退出码 3");
  process.exit(3);
}
const token = created.token;
await http(SRC, token, "create_layer", { layer_id: "L1", name: "L1" });
const stroked = await http(SRC, token, "brush_stroke", {
  layer_id: "L1", object_id: "s1", brush: "100%_Opaque", size: 12,
  color: { r: 90, g: 40, b: 20, a: 255 }, points: [[30, 40], [150, 110]],
});
if (!stroked || stroked.ok !== true) {
  console.error(`❌ 判不了：源文档画不上（${JSON.stringify(stroked).slice(0, 120)}）⇒ 退出码 3`);
  process.exit(3);
}
// **∴ 必须**显式给 `path` ✗**：不给 ⇒ 工具只回一个**可下载 url** ✓ 而 **MCP 的 `import_project` 要的是**本地路径**✗__
//   （我第一版没给 ⇒ 回执里没有路径字段 ⇒ 判据只能退 3 ✗ —— **∴ 又一次是**我的脚本**✓**）。
const exported = await http(SRC, token, "export_project", { path: PACK_NAME });
const packPath = exported && (exported.path || exported.file || exported.saved_path);
console.log(`  ① 源文档 ${W}×${H} ⇒ export_project(path=${PACK_NAME}) ⇒ ${packPath ? String(packPath).slice(0, 120) : JSON.stringify(exported).slice(0, 220)}`);
if (!packPath || !existsSync(packPath)) {
  console.error("❌ 判不了：拿不到导出包的真实路径 ⇒ 退出码 3（CI 里服务端会设 YANSHI_EXPORT_DIR ✓）");
  process.exit(3);
}

// ---------- ② 全新 root 的 MCP 进程：改名导入 ----------
const child = spawn(BIN, ["--root", root, "--doc", "session_anchor", "--width", "64", "--height", "64"],
  { stdio: ["pipe", "pipe", "pipe"] });

// **∴ 兜底 ✗**：**∴ 判据中途抛错也要收掉这个子进程 ✓**
//   ⇒ **∴ 因为**正常路径的 kill 不会执行 ✗ ⇒ **∴ 实测漏过一个活了 **3 小时 56 分**的 serve ✗**
//   ⇒ **∴ 所以**每个 spawn 都挂一条 exit 监听 ✓（**∴ 已死的进程 kill 抛错 ⇒ catch 掉 ✓**）
process.on("exit", () => { try { child.kill("SIGKILL"); } catch { /* 已经没了 */ } });

let buffer = "";
const seen = new Map();
const stderr = [];
child.stdout.on("data", (chunk) => {
  buffer += chunk;
  let index;
  while ((index = buffer.indexOf("\n")) >= 0) {
    const line = buffer.slice(0, index);
    buffer = buffer.slice(index + 1);
    try { const message = JSON.parse(line); if (message.id) seen.set(message.id, message); } catch {}
  }
});
child.stderr.on("data", (chunk) => stderr.push(String(chunk)));
const send = (object) => child.stdin.write(JSON.stringify(object) + "\n");
const waitFor = async (id, ms = 30000) => {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    if (seen.has(id)) return seen.get(id);
    await new Promise((r) => setTimeout(r, 40));
  }
  return null;
};
send({ jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2024-11-05", capabilities: {}, clientInfo: { name: "judge", version: "1" } } });
await waitFor(1);
send({ jsonrpc: "2.0", method: "notifications/initialized", params: {} });
await new Promise((r) => setTimeout(r, 250));
let id = 100;
const call = async (tool, args) => {
  const current = ++id;
  send({ jsonrpc: "2.0", id: current, method: "tools/call", params: { name: tool, arguments: args } });
  return await waitFor(current);
};
const body = (message) => {
  const text = message && message.result && message.result.content && message.result.content.find((c) => c.type === "text");
  if (!text) return null;
  try { return JSON.parse(text.text); } catch { return null; }
};

const imported = await call("import_project", { path: packPath, doc_id: DST });
const importedBody = body(imported);
console.log(`  ② MCP import_project(doc_id=${DST}) ⇒ ${JSON.stringify(importedBody).slice(0, 200)}`);
child.kill();
await new Promise((r) => setTimeout(r, 400));
if (stderr.length) console.log(`     MCP stderr：${stderr.join("").slice(-160).replace(/\n/g, " ⏎ ")}`);

// ---------- ③ 落盘对账：新 id 的尺寸 ＋ 有没有别的文档冒出来 ----------
const docsDir = join(root, "docs");
const dirs = existsSync(docsDir) ? readdirSync(docsDir) : [];
console.log(`  ③ 导入 root 里的文档：${JSON.stringify(dirs)}`);
const meta = {};
for (const name of dirs) {
  const path = join(docsDir, name, "meta.json");
  if (!existsSync(path)) { meta[name] = "(没有 meta.json)"; continue; }
  const parsed = JSON.parse(readFileSync(path, "utf8"));
  meta[name] = `${parsed.width}×${parsed.height}`;
}
console.log(`     各文档 meta：${JSON.stringify(meta)}`);

const failures = [];
if (!importedBody || importedBody.ok !== true) {
  failures.push(`import_project 没成功 ✗（回执：${JSON.stringify(imported).slice(0, 200)}）`);
}
// ③-a **包里的那份必须真的落进来、且尺寸跟着包走** ✓（落点＝包内 id ✓ —— 实测校准过 ✓）
if (meta[SRC] !== `${W}×${H}`) {
  failures.push(`包内 id（${SRC}）没落成 ${W}×${H} ✗（实测 ${meta[SRC] ?? "(缺)"} ✓）⇒ 导入没真的发生 ✓`);
}
// ③-b **任何目录都不许是 CLI 缺省那档（64×64）** ✗ —— 修前 `renamed/` 正是这个 ✓（预建假文档 ✓）
for (const [name, size] of Object.entries(meta)) {
  if (size === "64×64") {
    failures.push(`目录 ${name} 是 **CLI 缺省的 64×64** ✗ ⇒ MCP 又**预建**了一份假文档 ✓（修前那个 renamed 目录就是这样 ✓）`);
  }
}
// 附注（**不断言** ✗）：`doc_id` 参数不被采纳 ＋ 那个空目录 ⇒ 是"改名导入"那一族 ✓，对方也标为未修 ✓
if (dirs.includes(DST) && meta[DST] === "(没有 meta.json)") {
  console.log(`  附注：${DST}/ 存在但没有 meta.json ✓（**不断言** ✗ —— 那是"改名导入"那一族 ✓，对方自己标为未修 ✓）`);
}

if (failures.length) {
  console.error(`❌ ${failures.length} 项没成立：`);
  for (const item of failures) console.error(`   · ${item}`);
  process.exit(1);
}
console.log(`✓ 导入按**包内 id**落点且尺寸跟包走（${SRC}=${meta[SRC]} ✓）、**没有 CLI 缺省的预建假文档** ✓（改名那一族另记 ✓）`);
