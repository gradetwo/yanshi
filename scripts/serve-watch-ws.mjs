#!/usr/bin/env node
// **判据（serve 侧 ✓）：`yanshi-serve --watch` ⇒ 外部直接追加的原子，必须**推**给已连接的 WS 客户端** ✓
//
// **用户裁定（2026-10-15 ✓）**：`yanshi-serve` 与 `yanshi-mcp` 都加 `--watch`（**默认关** ✗）；
// 监视 `--root/docs/*/atoms.jsonl` ✓；**增量解析** ✓；冲突**最后写赢** ✓；不监视 blob ✗。
// **MCP 侧** 已由 `scripts/mcp-watch-external.mjs` 守住 ✓；**本条守 serve／WS 那一半** ✓。
//
// **为什么不用浏览器** ✓（2026-10-15 实测 ✓）：Node **v26 原生带 `WebSocket`** ✓
// ⇒ 不必 CDP ✗、不必 chromium ✗ ⇒ 也避开了本仓浏览器判据偶发的"内核就绪超时" ✗
//   （见已知红台账里 `browser-export-full-document.mjs` 那条 ✓）。
//
// **怎么跑** ✓：`node scripts/serve-watch-ws.mjs [<base-url>（本判据自起服务 ⇒ 该参数忽略 ✓）]`
//   **退出码**：0 通过 ✓｜1 **产品不达标**（含"`--watch` 尚不存在" ✓）｜3 **判据自身跑不了** ✓
//
// **两头都看** ✓：① **带 `--watch`** ⇒ 外部追加 ⇒ **1.5 s 内**必须收到一帧、其中出现该原子 id ✓；
//               ② **负对照**（不加 `--watch` ✓）⇒ **不许**收到它 ✓。
import os from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";
import { appendFileSync, existsSync, mkdirSync, readFileSync, rmSync } from "node:fs";
import { join } from "node:path";

const BIN = process.env.YANSHI_SERVE_BIN || "./target/release/yanshi-serve";
if (!existsSync(BIN)) {
  console.log("❌ 判不了：找不到 " + BIN + "（先 cargo build --release -p yanshi-http）⇒ 判据自身跑不了");
  process.exit(3);
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const failures = [];
const note = (ok, msg) => { console.log((ok ? "  ✓ " : "  ✗ ") + msg); if (!ok) failures.push(msg); };

async function health(base) {
  try {
    const r = await fetch(base + "/health", { signal: AbortSignal.timeout(1500) });
    return r.ok;
  } catch { return false; }
}

/** 起一个服务；`watch` 为真时加 `--watch` ✓；返回 {child, base, root, logOf, token, doc, ws, frames} */
async function startSide(port, watch, tag) {
  // **不写死本机路径** ✗✓（2026-10-15 ✓）：CI 的 runner 不是 crow ✓ ⇒
  //   写死会让 `mkdir` 直接 EACCES ✗（实测：criteria shard 3/6 就红在这 ✓）。
  const root =
    process.env.YANSHI_WATCH_DIR || path.join(os.tmpdir(), "yanshi-watch-" + tag);
  rmSync(root, { recursive: true, force: true });
  mkdirSync(root, { recursive: true });
  const argv = ["--bind", "127.0.0.1:" + port, "--root", root, "--doc", "seed",
    "--width", "160", "--height", "120", "--assets-dir", "assets"];
  if (watch) argv.push("--watch");
  const child = spawn(BIN, argv, { stdio: ["ignore", "pipe", "pipe"] });

// **∴ 兜底 ✗**：**∴ 判据中途抛错也要收掉这个子进程 ✓**
//   ⇒ **∴ 因为**正常路径的 kill 不会执行 ✗ ⇒ **∴ 实测漏过一个活了 **3 小时 56 分**的 serve ✗**
//   ⇒ **∴ 所以**每个 spawn 都挂一条 exit 监听 ✓（**∴ 已死的进程 kill 抛错 ⇒ catch 掉 ✓**）
process.on("exit", () => { try { child.kill("SIGKILL"); } catch { /* 已经没了 */ } });

  let stderr = "";
  child.stderr.on("data", (b) => { stderr += String(b); });
  const base = "http://127.0.0.1:" + port;
  for (let i = 0; i < 40; i++) { if (await health(base)) break; await sleep(250); }
  if (!(await health(base))) {
    // **区分两头** ✓：认不出 `--watch` ⇒ **产品红** ✓；否则判据自身坏了 ✓
    const why = stderr.replace(/\s+/g, " ").slice(0, 240);
    child.kill();
    if (/unknown|unexpected|invalid|usage|用法|--watch/i.test(why)) {
      console.log("  ✗ 服务器不接受 `--watch` ⇒ 该特性尚未实现 ⇒ **产品红** ✓（不是判据问题 ✓）");
      console.log("    实测 stderr：" + why);
      process.exit(1);
    }
    console.log("❌ 判不了：服务起不来 ⇒ " + why);
    process.exit(3);
  }
  return { child, base, root, tag };
}

/** 建文档拿 token ✓（照 `run-criteria.sh:78` 的 token_for() ✓）⇒ 拿不到就是 **3** ✓，不冒充产品红 ✓ */
async function tokenFor(side) {
  const res = await fetch(side.base + "/api/documents", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: "w", width: 160, height: 120 }),
  });
  const body = await res.text();
  let token = null;
  try { token = JSON.parse(body).token || null; } catch { /* 下面统一报 ✓ */ }
  if (!token) {
    console.log("❌ 判不了：拿不到 token（POST /api/documents ⇒ HTTP " + res.status + "）");
    console.log("    正文前 200 字节：" + body.slice(0, 200));
    console.log("    ⇒ 这是**判据自己**的失败 ✓ ⇒ 退出 3 ✓（绝不冒充产品红 ✗ —— 照 token_for() 那条先例 ✓）");
    process.exit(3);
  }
  return token;
}

/** 连 WS ✓；返回 {frames:[], waitForId(id, ms)} */
function openWs(base, doc, token) {
  const wsUrl = base.replace("http://", "ws://") + "/ws?doc=" + doc + "&token=" + token;
  const ws = new WebSocket(wsUrl);
  const frames = [];
  const waiters = [];
  ws.addEventListener("message", (ev) => {
    const s = typeof ev.data === "string" ? ev.data : "";
    frames.push(s);
    for (const w of waiters.splice(0)) w();
  });
  const opened = new Promise((resolve) => {
    ws.addEventListener("open", () => resolve(true));
    ws.addEventListener("error", () => resolve(false));
    setTimeout(() => resolve(false), 5000);
  });
  const waitForId = async (id, ms) => {
    const end = Date.now() + ms;
    while (Date.now() < end) {
      if (frames.some((f) => f.includes(id))) return true;
      await new Promise((r) => setTimeout(r, 100));
    }
    return frames.some((f) => f.includes(id));
  };
  return { ws, frames, opened, waitForId };
}

/** 外部追加一个合法原子 ✓：克隆该文档自己的 `atoms.jsonl` 最后一行、改 id／seq ✓（不猜 schema ✓） */
function appendForeign(side, doc, id) {
  const log = join(side.root, "docs", doc, "atoms.jsonl");
  if (!existsSync(log)) {
    console.log("❌ 判不了：没有 atoms.jsonl ⇒ " + log);
    process.exit(3);
  }
  const lines = readFileSync(log, "utf8").split("\n").filter((l) => l.trim());
  if (lines.length === 0) { console.log("❌ 判不了：日志是空的（服务没落任何东西？）"); process.exit(3); }
  const atom = JSON.parse(lines[lines.length - 1]);
  atom.id = id;
  if (typeof atom.seq === "number") atom.seq = atom.seq + 1000;
  appendFileSync(log, JSON.stringify(atom) + "\n");
  return log;
}

// ── ① 带 `--watch` ✓ ──
console.log("① 带 `--watch`：外部追加必须**推**出来 ✓");
const a = await startSide(7871, true, "on");
const tokenA = await tokenFor(a);
const wsA = openWs(a.base, "w", tokenA);
if (!(await wsA.opened)) { console.log("❌ 判不了：WS 连不上（带 token 仍被拒 ⇒ 判据自身/契约问题）"); process.exit(3); }
console.log("  WS 已连上 ✓（带 token ✓）");
appendForeign(a, "w", "foreign-serve-1");
console.log("  已外部追加 foreign-serve-1 ✓");
const seenA = await wsA.waitForId("foreign-serve-1", 1500);
if (!seenA) {
  console.log("  收到的帧（最多 4 条，供取证 ✓）：");
  for (const f of wsA.frames.slice(0, 4)) console.log("    " + f.slice(0, 200));
}
note(seenA, "带 `--watch` 时，外部追加 1.5 s 内被推给 WS 客户端 ✓");
wsA.ws.close();
a.child.kill();

// ── ② 负对照 ✓：不加 `--watch` ──
console.log("② 负对照（默认关 ✗）：外部追加**不许**被推出来 ✓");
const b = await startSide(7872, false, "off");
const tokenB = await tokenFor(b);
const wsB = openWs(b.base, "w", tokenB);
if (!(await wsB.opened)) { console.log("❌ 判不了：负对照侧的 WS 也连不上"); process.exit(3); }
appendForeign(b, "w", "foreign-serve-2");
console.log("  已外部追加 foreign-serve-2 ✓");
const seenB = await wsB.waitForId("foreign-serve-2", 1800);
note(!seenB, "默认（不加 `--watch`）时，外部追加**看不见** ✓（挡住「其实全量重读」的假绿 ✓）");
wsB.ws.close();
b.child.kill();

if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ serve 侧 `--watch` 成立：带 ⇒ 1.5 s 内推出来 ✓；不带 ⇒ 不推 ✓");
process.exit(0);
