#!/usr/bin/env node
// **判据（新 ✓）：`--watch` ⇒ 别的进程直接往 `atoms.jsonl` 追加的原子，MCP 必须看得见** ✓
//
// **用户裁定（2026-10-15 ✓）**：`yanshi-serve` 与 `yanshi-mcp` 都加 `--watch`（**默认关** ✗）；
// 监视 `--root/docs/*/atoms.jsonl`；**增量解析**；冲突用**最后写赢**（不做合并 ✓）；不监视 blob ✓。
// **本判据只管 MCP 侧**（serve 侧走浏览器判据 ✓）⇒ 它与 `--watch` 的实现**同生共死** ✓。
//
// **怎么跑** ✓（照 §43：让 harness 起环境 ✓；这里不需要浏览器 ✓）：
//   node scripts/mcp-watch-external.mjs [根目录]
//   退出码：0 通过 ✓｜1 **产品不达标**（含"`--watch` 还不存在" ✓）｜3 **判据自身跑不了**（二进制缺失等 ✓）
//
// **两头都要看** ✓（纪律：判据没红过不算有牙 ✓）：
//   * 正向 ✓：**带 `--watch`** ⇒ 外部追加的原子**必须**在 ~1.5 s 内出现在 `get_document` 里 ✓；
//   * **负对照** ✓：**不带 `--watch`**（默认关 ✗）⇒ 外部追加的原子**必须看不见** ✓
//     —— 这一半挡住"其实一直是全量重读、根本没接 watcher"的假绿 ✓。
import { spawn } from "node:child_process";
import { appendFileSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync } from "node:fs";
import { join } from "node:path";

const BIN = process.env.YANSHI_MCP_BIN || "./target/release/yanshi-mcp";
if (!existsSync(BIN)) {
  console.log("❌ 判不了：找不到 " + BIN + "（先 cargo build --release -p yanshi-mcp）⇒ 判据自身跑不了");
  process.exit(3);
}
const root = process.argv[2] || "/home/crow/yanshi-tmp/watch-ext";
rmSync(root, { recursive: true, force: true });
mkdirSync(root, { recursive: true });

const docDirOf = (doc) => join(root, "docs", doc);
const logOf = (doc) => join(docDirOf(doc), "atoms.jsonl");

// 起一个 MCP 进程；`watch` 为真时加 `--watch` ✓
function startServer(doc, watch) {
  const argv = ["--root", root, "--doc", doc, "--width", "160", "--height", "120"];
  if (watch) argv.push("--watch");
  const child = spawn(BIN, argv, { stdio: ["pipe", "pipe", "pipe"] });
  const state = { child, nextId: 1, pending: new Map(), stderr: "", dead: false, exitCode: null };
  child.stderr.on("data", (b) => { state.stderr += String(b); });
  child.on("exit", (code) => { state.dead = true; state.exitCode = code; });
  let buf = "";
  child.stdout.on("data", (b) => {
    buf += String(b);
    let i;
    while ((i = buf.indexOf("\n")) >= 0) {
      const line = buf.slice(0, i).trim(); buf = buf.slice(i + 1);
      if (!line) continue;
      let msg; try { msg = JSON.parse(line); } catch { continue; }
      if (msg.id !== undefined && msg.id !== null && state.pending.has(msg.id)) {
        state.pending.get(msg.id)(msg); state.pending.delete(msg.id);
      }
    }
  });
  const send = (method, params) => new Promise((resolve, reject) => {
    const id = state.nextId++;
    state.pending.set(id, resolve);
    child.stdin.write(JSON.stringify({ jsonrpc: "2.0", id, method, params: params || {} }) + "\n");
    setTimeout(() => {
      if (state.pending.has(id)) {
        state.pending.delete(id);
        reject(new Error(state.dead
          ? `进程已退出（码 ${state.exitCode}）：${state.stderr.trim().slice(0, 300)}`
          : `等 ${method} 超时`));
      }
    }, 8000);
  });
  return { state, send };
}
async function open(doc, watch) {
  const s = startServer(doc, watch);
  await s.send("initialize", { protocolVersion: "2024-11-05", capabilities: {}, clientInfo: { name: "watch-ext", version: "1" } });
  await s.send("notifications/initialized", {});
  const call = async (tool, args) => {
    const r = await s.send("tools/call", { name: tool, arguments: args || {} });
    return r;
  };
  return { ...s, call };
}
const asText = (r) => {
  const c = (r && r.result && r.result.content) || [];
  return c.map((x) => x.text || "").join("\n");
};

const failures = [];
const note = (ok, msg) => { console.log((ok ? "  ✓ " : "  ✗ ") + msg); if (!ok) failures.push(msg); };

// ── 正向 ✓：带 `--watch` ──
console.log("① 带 `--watch`：外部追加必须看得见 ✓");
let fwd;
try {
  fwd = await open("watched", true);
} catch (error) {
  // **区分两头** ✓：`--watch` 还不被认识 ⇒ 这是**产品不达标**（红 ✓），不是判据坏了 ✓
  const text = String(error && error.message || error);
  if (/unknown|unexpected|invalid|--watch|usage/i.test(text)) {
    console.log("  ✗ 服务器不接受 `--watch` ⇒ 该特性尚未实现（这是产品红 ✓，不是判据问题 ✓）");
    console.log("    实测：" + text.slice(0, 200));
    process.exit(1);
  }
  console.log("❌ 判不了：起不了服务器 ⇒ " + text.slice(0, 300));
  process.exit(3);
}
// 先画一笔 ⇒ 让日志里至少有一行，好照着它**克隆**出一个合法的外部原子 ✓（保证 schema 正确 ✓）
await fwd.call("brush_stroke", { points: [[10, 10], [40, 40]], brush: "round", color: "#204060" });
const first = await fwd.call("get_document", {});
const firstText = asText(first);
console.log("  get_document ⇒ " + firstText.replace(/\s+/g, " ").slice(0, 160));
const log = logOf("watched");
if (!existsSync(log)) { console.log("❌ 判不了：没有 atoms.jsonl ⇒ " + log); process.exit(3); }
const lines = readFileSync(log, "utf8").split("\n").filter((l) => l.trim());
const template = JSON.parse(lines[lines.length - 1]);
// **克隆 + 改 id/seq** ✓ ⇒ 结构必然合法 ✓（不去猜 schema ✓）
const foreign = JSON.parse(JSON.stringify(template));
foreign.id = "foreign-1";
if (typeof foreign.seq === "number") foreign.seq = foreign.seq + 1000;
appendFileSync(log, JSON.stringify(foreign) + "\n");
console.log("  已外部追加一个原子（id=foreign-1 ✓，seq+" + (typeof template.seq === "number" ? "1000" : "?") + "）");
const deadline = Date.now() + 1500;
let seen = false;
while (Date.now() < deadline) {
  const now = await fwd.call("get_document", {});
  const t = asText(now);
  if (t.includes("foreign-1")) { seen = true; break; }
  await new Promise((r) => setTimeout(r, 120));
}
note(seen, "带 `--watch` 时，外部追加的原子在 1.5 s 内出现在 `get_document` 里 ✓");
fwd.state.child.kill();

// ── 负对照 ✓：不带 `--watch`（默认关 ✗）⇒ 必须看不见 ✓ ──
console.log("② 负对照（默认关 ✗）：外部追加**必须看不见** ✓");
const neg = await open("plain", false);
await neg.call("brush_stroke", { points: [[10, 10], [40, 40]], brush: "round", color: "#204060" });
const nlog = logOf("plain");
if (!existsSync(nlog)) { console.log("❌ 判不了：负对照的 atoms.jsonl 不在 ⇒ " + nlog); process.exit(3); }
const nlines = readFileSync(nlog, "utf8").split("\n").filter((l) => l.trim());
const nforeign = JSON.parse(nlines[nlines.length - 1]);
nforeign.id = "foreign-2";
if (typeof nforeign.seq === "number") nforeign.seq = nforeign.seq + 1000;
appendFileSync(nlog, JSON.stringify(nforeign) + "\n");
let negSeen = false;
for (let i = 0; i < 6; i++) {
  const now = await neg.call("get_document", {});
  if (asText(now).includes("foreign-2")) { negSeen = true; break; }
  await new Promise((r) => setTimeout(r, 150));
}
note(!negSeen, "默认（不加 `--watch`）时，外部追加**看不见** ✓（挡住「全量重读」的假绿 ✓）");
neg.state.child.kill();

// 收尾：把两边的文档目录列一下，便于人工复核 ✓
try { console.log("  文档目录：" + JSON.stringify(readdirSync(join(root, "docs")))); } catch { /* 无妨 ✓ */ }
if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ `--watch` 在 MCP 侧成立：带 ⇒ 立刻看得见 ✓；不带 ⇒ 看不见 ✓");
process.exit(0);
