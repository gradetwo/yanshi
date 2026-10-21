#!/usr/bin/env node
// **D08 的完整判据：冷打开后导出必须与热导出逐字节一致**（第 768 轮）
//  现有的 `tool-export-reopen-identical.mjs` 只覆盖「同进程内两次导出」，
//  真正的 D08（「不连续瓦片缓存被重复混合，热导出与冷打开灰度不同」）要**重启进程**才验得到。
//  做法：起服务 → 画 4 笔不连续笔画 → 导出 A → **杀进程** → 用同一 --root 重启 →
//        打开同一 doc → 导出 B ⇒ A 与 B 的 sha256 必须相同。
import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createHash } from "node:crypto";
import { existsSync } from "node:fs";
import { trackTemp } from "./lib/temp-dirs.mjs";

// **⇒ 两种入口都收** ✓（第 1430 轮）：
//   · 给**二进制路径** ⇒ 用它（本判据要自己 spawn、还要中途重启 ⇒ 必须是路径）
//   · 给 **URL**（`run-criteria-local.sh` 统一传的是 URL）⇒ **回退到 `target/release/yanshi-serve`**
//     本来这里直接 `spawn(url)` ⇒ `ENOENT` ⇒ 长期 `exit 3`「本机不可判」✗（判据自己从未真正跑过 ✗）
let bin = process.argv[2];
if (!bin || /^https?:\/\//.test(bin)) {
  bin = join(process.cwd(), "target", "release", "yanshi-serve");
}
if (!existsSync(bin)) {
  console.error(`判据自身的前提不成立（不是产品问题）：找不到 yanshi-serve（${bin}）⇒ 先 cargo build --release -p yanshi-http --bin yanshi-serve`);
  process.exit(3);
}
const root = mkdtempSync(join(tmpdir(), "cold-export-"));
trackTemp(root);
const PORT = 22541;
const base = `http://127.0.0.1:${PORT}`;
const DOC = "cold";

const sha = (p) => createHash("sha256").update(readFileSync(p)).digest("hex").slice(0, 16);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function boot() {
  const proc = spawn(bin, ["--root", root, "--doc", DOC, "--width", "300", "--height", "300",
    "--bind", `127.0.0.1:${PORT}`], { stdio: "ignore", detached: false });
  // **必须接 `error` 事件**（第 801 轮，CI 实测）：`spawn` 失败（路径不存在／不可执行）时
  // 不接就会冒泡成未捕获异常 —— CI 日志里是 `node:events:497`，退出码 1，
  // **看起来像产品失败**。而它是「判据自身的前提不成立」⇒ 必须 exit(3) 并说清。
  let spawnError = null;
  proc.on("error", (e) => { spawnError = e; });
  for (let i = 0; i < 60; i += 1) {
    if (spawnError) {
      console.error(`判据自身的前提不成立（不是产品问题）：起不来服务 ${bin} ⇒ ${spawnError.message}`);
      process.exit(3);
    }
    try { if ((await fetch(`${base}/health`)).ok) return proc; } catch { /* 还没起来 */ }
    await sleep(200);
  }
  console.error(`判据自身的前提不成立（不是产品问题）：服务 60 次探测都没起来（${bin}）`);
  process.exit(3);
}
const stop = async (proc) => { proc.kill("SIGKILL"); await sleep(600); };

// —— 第一次：建文档、画笔画、热导出 ——
let proc = await boot();
let token;
{
  const c = await (await fetch(`${base}/api/documents`, { method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: DOC, width: 300, height: 300 }) })).json();
  token = c.token;
  if (!token) { console.error("判据自身前提不成立：拿不到 token（不是产品问题）"); await stop(proc); process.exit(3); }
  const call = (tool, args) => fetch(`${base}/api/tools?doc=${DOC}&token=${token}`, { method: "POST",
    headers: { "content-type": "application/json" }, body: JSON.stringify({ tool, arguments: args }) }).then((r) => r.json());
  await call("create_layer", { layer_id: "p", name: "p" });
  for (let i = 0; i < 4; i += 1) {
    await call("brush_stroke", { layer_id: "p", brush: "100%_Opaque", size: 12,
      color: [0.1 + i * 0.2, 0.2, 0.3, 1], points: [[30 + i * 60, 40, 1], [60 + i * 60, 90, 1]] });
  }
  const e = await call("export_png", { path: join(root, "hot.png") });
  if (!e.ok) { console.error(`判据自身前提不成立：热导出失败 ${JSON.stringify(e.context ?? {})}`); await stop(proc); process.exit(3); }
}
await stop(proc);

// —— 第二次：**冷打开**同一 root，导出 ——
proc = await boot();
let coldOk;
{
  const c = await (await fetch(`${base}/api/documents`, { method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: DOC, width: 300, height: 300 }) })).json();
  const call = (tool, args) => fetch(`${base}/api/tools?doc=${DOC}&token=${c.token}`, { method: "POST",
    headers: { "content-type": "application/json" }, body: JSON.stringify({ tool, arguments: args }) }).then((r) => r.json());
  const atoms = await call("list_atoms", { limit: 50 });
  if (Number(atoms.count) < 4) {
    console.error(`判据自身前提不成立：冷打开后只看到 ${atoms.count} 条笔迹（应 ≥4）⇒ 文档没落盘`);
    await stop(proc); process.exit(3);
  }
  const e = await call("export_png", { path: join(root, "cold.png") });
  coldOk = e.ok;
  if (!coldOk) console.error(`冷导出失败：${JSON.stringify(e.context ?? {})}`);
}
await stop(proc);

const hot = sha(join(root, "hot.png"));
const cold = sha(join(root, "cold.png"));
console.log(`  热导出=${hot}｜冷打开后导出=${cold}｜一致=${hot === cold}`);
if (!coldOk) { console.log("  ✗ 冷打开后导出失败"); process.exit(1); }
if (hot !== cold) {
  console.log("  ✗ 冷打开后的导出于热导出**字节不同** ⇒ 缓存混合或持久化丢信息（D08 那条缺陷）");
  process.exit(1);
}
console.log("  ✓ 冷打开后导出与热导出逐字节一致");
