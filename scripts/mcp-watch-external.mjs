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
import os from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";
import { appendFileSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync } from "node:fs";
import { join } from "node:path";

const BIN = process.env.YANSHI_MCP_BIN || "./target/release/yanshi-mcp";
if (!existsSync(BIN)) {
  console.log("❌ 判不了：找不到 " + BIN + "（先 cargo build --release -p yanshi-mcp）⇒ 判据自身跑不了");
  process.exit(3);
}
// **不写死本机路径** ✗✓（2026-10-15 ✓；CI 里写死 ⇒ EACCES ✗）
// **★ 守卫：`argv[2]` 若**看起来像 URL**✗ ⇒ **绝不当作路径**✗ ★**（第 1000+ 轮 ✓；**实测抓到的 ✓**）：
//   **∴ 现象 ✗**：**运行器会给判据**追加一个 `http://…` 参数 ✗**（**`browser-*` 收查看器 URL ✓、
//     `tool-*` 收 base-url ✓**）⇒ **∴ 它落进了本判据的 `argv[2]` ✗** ⇒ **∴ `root` ＝ `http://127.0.0.1:PORT` ✗**
//     ⇒ **∴ 接下来的 `rmSync`／`mkdirSync` 就**就地在仓库顶层**造出 `http:/127.0.0.1:PORT/{docs,blobs,…}` ✗✓**
//       （**∴ 那是**一个 server 的 root 结构**✓ ⇒ **∴ 白名单判据抓到它 ✓；而那正是它存在的意义 ✓**）
//   ⇒ **∴ 修法（**判据侧 ✓，风险最小 ✓）**：**含 `://` 的参数一律**不当路径**✗ ⇒ **回落到临时目录 ✓，
//     并**打印一句**（**∴ 不静默 ✗ —— 静默会让下次更难查 ✓**）★
const looksLikeUrl = (v) => typeof v === "string" && /^[a-z][a-z0-9+.-]*:\/\//i.test(v);
if (looksLikeUrl(process.argv[2])) {
  console.log(
    `  ⚠️ argv[2] 看起来是 URL（${String(process.argv[2]).slice(0, 48)}）⇒ **∴ 不当路径用**✗ ⇒ 回落临时目录 ✓`
  );
}
const root =
  (looksLikeUrl(process.argv[2]) ? undefined : process.argv[2]) ||
  process.env.YANSHI_WATCH_DIR ||
  path.join(os.tmpdir(), "yanshi-watch-ext");
rmSync(root, { recursive: true, force: true });
mkdirSync(root, { recursive: true });

const docDirOf = (doc) => join(root, "docs", doc);
const logOf = (doc) => join(docDirOf(doc), "atoms.jsonl");

// 起一个 MCP 进程；`watch` 为真时加 `--watch` ✓
//
// **★ 客户端照 `scripts/mcp-document-switch.mjs` 的**已验证**写法（2026-10-15 修 ✓）★**
//   我之前用的是"promise ＋ 即时超时" ✗ ⇒ 实测**连 `initialize` 都走不过** ✗
//   （探针空日志、退出码 124 ✓）⇒ 换成模板的 **`seen` 映射 ＋ `waitFor` 轮询** ✓。
function startServer(doc, watch) {
  const argv = ["--root", root, "--doc", doc, "--width", "160", "--height", "120"];
  if (watch) argv.push("--watch");
  const child = spawn(BIN, argv, { stdio: ["pipe", "pipe", "pipe"] });

  // **∴ 兜底 ✗**：**∴ 判据中途抛错也要收掉这个子进程 ✓**
  //   ⇒ **∴ 因为**正常路径的 kill 不会执行 ✗ ⇒ **∴ 实测漏过一个活了 **3 小时 56 分**的 serve ✗**
  //   ⇒ **∴ 所以**每个 spawn 都挂一条 exit 监听 ✓（**∴ 已死的进程 kill 抛错 ⇒ catch 掉 ✓**）
  process.on("exit", () => { try { child.kill("SIGKILL"); } catch { /* 已经没了 */ } });

  const state = { child, seen: new Map(), stderr: "", dead: false, exitCode: null, nextId: 10 };
  child.stderr.on("data", (b) => { state.stderr += String(b); });
  child.on("exit", (code) => { state.dead = true; state.exitCode = code; });
  // **写一个已退出的子进程会抛 EPIPE** ✗ ⇒ 吞掉它 ✓（否则 node 会崩 ✗、退出码就变成"崩溃" ✓）
  child.stdin.on("error", () => { /* 忽略 ✓ */ });
  let buffer = "";
  child.stdout.on("data", (chunk) => {
    buffer += String(chunk);
    let index;
    while ((index = buffer.indexOf("\n")) >= 0) {
      const line = buffer.slice(0, index); buffer = buffer.slice(index + 1);
      try {
        const message = JSON.parse(line);
        if (message.id) state.seen.set(message.id, message);
      } catch { /* 忽略非 JSON ✓ */ }
    }
  });
  const send = (object) => child.stdin.write(JSON.stringify(object) + "\n");
  const waitFor = async (id, ms = 8000) => {
    const end = Date.now() + ms;
    while (Date.now() < end) {
      if (state.seen.has(id)) return state.seen.get(id);
      await new Promise((r) => setTimeout(r, 60));
    }
    return null;
  };
  const call = async (tool, args) => {
    const id = ++state.nextId;
    send({ jsonrpc: "2.0", id, method: "tools/call", params: { name: tool, arguments: args || {} } });
    return await waitFor(id);
  };
  return { state, send, waitFor, call };
}
async function open(doc, watch) {
  const s = startServer(doc, watch);
  // **★ 先等一小会儿，看它是不是当场就退了（2026-10-15 ✓）★**
  //   服务器不认识 `--watch` 时会**立刻打印用法并退出** ✓ ⇒ 这时**不能**再往 stdin 写 ✗
  //   （写了就是 EPIPE ✗ ⇒ node 崩 ✗ ⇒ 退出码变成"崩溃"而不是"产品红" ✗）。
  await new Promise((r) => setTimeout(r, 400));
  const early = (s.state.stderr || "").trim();
  if (s.state.dead && /usage|用法|unknown|unexpected|invalid|--watch/i.test(early)) {
    console.log("  ✗ 服务器不接受 `--watch` ⇒ 该特性尚未实现 ⇒ **产品红** ✓（不是判据问题 ✓）");
    console.log("    实测 stderr：" + early.replace(/\s+/g, " ").slice(0, 300));
    process.exit(1);
  }
  s.send({ jsonrpc: "2.0", id: 1, method: "initialize",
    params: { protocolVersion: "2024-11-05", capabilities: {}, clientInfo: { name: "watch-ext", version: "1" } } });
  const hello = await s.waitFor(1);
  if (!hello) {
    // **区分两头** ✓：启动就被拒（不认识 `--watch` 等 ✓）⇒ **产品不达标**（红 ✓）；否则判据自身坏了 ✓
    const why = (s.state.stderr || "").trim();
    if (/usage|用法|unknown|unexpected|invalid|--watch/i.test(why) || s.state.dead) {
      console.log("  ✗ 服务器不接受 `--watch`（或直接退出）⇒ 该特性尚未实现 ⇒ **产品红** ✓");
      console.log("    实测 stderr：" + why.replace(/\s+/g, " ").slice(0, 260));
      process.exit(1);
    }
    console.log("❌ 判不了：`initialize` 没有回复（客户端自身的问题 ✓）");
    process.exit(3);
  }
  s.send({ jsonrpc: "2.0", method: "notifications/initialized", params: {} });
  await new Promise((r) => setTimeout(r, 300));
  return s;
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
