#!/usr/bin/env node
// **绘画过程中的 CPU 与内存**（第 822 轮）：设计 14.10 有内存预算 ✓，但覆盖表自己写着
// 「整机 RSS 实测 0.08GB 记录于本文档，**但 RSS 未纳入自动门禁**」✗ ⇒ **它会悄悄回归而无人知** ✓。
// 现有器具量的是**延迟**（首笔 <16ms ✓、帧 <8ms ✓）✗ ⇒ **"占用率/占用增量"不是同一件事** ✓。
//
// **为什么自足** ✓：要看**服务端进程的** RSS 与 CPU 时间 ✓ ⇒ 自己起一个服务端 ✓（Linux 下读 /proc ✓），
// 不依赖 runner 起的那一个 ✓ —— 与 `mcp-*.mjs` 同一做法 ✓。
//
// **判什么（能红 ✓，但预算必须**先测出来**再定 ✓）**：
//   ① **绝对 RSS** 必须小于设计 14.10 的预算 ✓（4K/10 图层 < 4GB ✓ —— 很宽 ✓，但它是**设计承诺** ✓）；
//   ② **绘画过程中的 RSS 增量** ≤ `PAINT_RSS_GROWTH_MB` ✓（缺省只**报告**不判 ✗ —— 待实测后定 ✓）；
//   ③ **每笔 CPU 时间** ≤ `PAINT_CPU_MS_PER_STROKE` ✓（同上 ✓）；
//   ④ **打印覆盖面** ✓（多少笔、多少像素、服务端 RSS 与 CPU 的起止值 ✓）。
//
// 用法：node scripts/tool-paint-memory.mjs [笔数]
//   PAINT_STROKES=200        画多少笔（缺省 200）
//   PAINT_RSS_GROWTH_MB=…    超过则红（不设则只报告 ✓）
//   PAINT_CPU_MS_PER_STROKE=… 超过则红（不设则只报告 ✓）
import { spawn } from "node:child_process";
import { readFileSync, writeFileSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const BIN = process.env.YANSHI_SERVE_BIN || "target/debug/yanshi-serve";
const STROKES = Number(process.argv[2] || process.env.PAINT_STROKES || 200);
const PORT = Number(process.env.PAINT_PORT || 15600);
const BASE = "http://127.0.0.1:" + PORT;
const RSS_GROWTH_BUDGET = process.env.PAINT_RSS_GROWTH_MB ? Number(process.env.PAINT_RSS_GROWTH_MB) : null;
const CPU_PER_STROKE_BUDGET = process.env.PAINT_CPU_MS_PER_STROKE ? Number(process.env.PAINT_CPU_MS_PER_STROKE) : null;
// 设计 14.10：内存（4K/10 图层）< 4GB —— **这是设计承诺 ✓**，所以它可以当门禁 ✓。
const ABSOLUTE_RSS_BUDGET_MB = 4 * 1024;

const work = mkdtempSync(join(tmpdir(), "yanshi-paint-"));
// **子进程输出落盘** ✓（第 823 轮 ✓）：`YANSHI_RENDER_PROBE=1` 时服务端会打印**各阶段耗时** ✓
// ⇒ 那正是"各个操作的占比" ✓ ⇒ 不能只在失败时才看它 ✓。
const CHILD_LOG = process.env.PAINT_CHILD_LOG || "/tmp/yanshi-paint-child.log";
const child = spawn(BIN, ["--bind", "127.0.0.1:" + PORT, "--root", work, "--doc", "boot",
  "--width", "512", "--height", "512", "--assets-dir", "assets"], { stdio: ["ignore", "pipe", "pipe"] });
// ⚠️ **必须留住子进程的输出** ✗（第 823 轮实测 ✓）：原先两行 `on("data", () => {})` 把它们**丢掉** ✓
// ⇒ 服务端启动失败时，判据只说"无法运行" ✗ ⇒ **真因一个字都没留下** ✓（`ENOENT: /proc/<pid>/status` ✓）。
const childLog = [];
const keep = (chunk) => { for (const line of String(chunk).split("\n")) if (line.trim()) childLog.push(line.trim()); if (childLog.length > 40) childLog.splice(0, childLog.length - 40); };
child.stdout.on("data", keep);
child.stderr.on("data", keep);

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const sample = () => {
  const status = readFileSync("/proc/" + child.pid + "/status", "utf8");
  const rssKb = Number((status.match(/^VmRSS:\s+(\d+) kB/m) || [])[1] || 0);
  const stat = readFileSync("/proc/" + child.pid + "/stat", "utf8");
  // 字段 14/15 = utime/stime（单位：时钟滴答 ✓，通常是 100/s ✓）
  const parts = stat.slice(stat.lastIndexOf(")") + 2).split(" ");
  const ticks = Number(parts[11]) + Number(parts[12]);
  return { rssMb: rssKb / 1024, cpuSeconds: ticks / 100 };
};

let failed = false;
const fail = (message) => { failed = true; console.log("  ✗ " + message); };

try {
  for (let i = 0; i < 60; i += 1) {
    try { const r = await fetch(BASE + "/api/documents"); if (r.ok) break; } catch { /* 还没起来 */ }
    await sleep(250);
  }
  const token = await fetch(BASE + "/api/documents", { method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: "boot", width: 512, height: 512 }) }).then((r) => r.json()).then((v) => v.token);
  if (!token) throw new Error("拿不到 token ⇒ 判据无法运行（不是通过 ✗）");

  await fetch(BASE + "/api/tools?doc=boot&token=" + token, { method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ tool: "create_layer", arguments: { layer_id: "L" } }) });

  const before = sample();
  const started = Date.now();
  let ok = 0;
  // **每笔单独计时** ✓（第 825 轮 ✓ —— 这是 `needs` 里那条"对象数增长曲线" ✓）：
  // 若成本随对象数累积 ✓，这条曲线会**单调上升** ✓；若只是首笔重建 ✓，它会**很快变平** ✓。
  const perStroke = [];
  for (let i = 0; i < STROKES; i += 1) {
    const strokeStarted = process.hrtime.bigint();
    // 每条笔触落在不同位置 ✓，避免"同一处重复覆盖"掩盖真实成本 ✓
    const x = 40 + (i % 16) * 26;
    const y = 40 + Math.floor(i / 16) * 26;
    const reply = await fetch(BASE + "/api/tools?doc=boot&token=" + token, { method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ tool: "brush_stroke", arguments: {
        layer_id: "L", object_id: "o_" + i, brush: "2B_pencil", size: 24,
        color: { r: 40, g: 90, b: 200, a: 255 },
        points: [[x, y, 0.4], [x + 40, y + 10, 0.8], [x + 80, y, 0.5]],
      } }) }).then((r) => r.json());
    if (reply && reply.ok) ok += 1;
    perStroke.push(Number(process.hrtime.bigint() - strokeStarted) / 1e6);
  }
  console.log("  每笔墙钟耗时（ms ✓）：" + perStroke.map((v) => v.toFixed(0)).join(" "));
  if (perStroke.length >= 4) {
    const head = perStroke.slice(0, Math.ceil(perStroke.length / 3)).reduce((a, b) => a + b, 0) / Math.ceil(perStroke.length / 3);
    const tail = perStroke.slice(-Math.ceil(perStroke.length / 3)).reduce((a, b) => a + b, 0) / Math.ceil(perStroke.length / 3);
    console.log("  前 1/3 平均 " + head.toFixed(0) + " ms ⇒ 后 1/3 平均 " + tail.toFixed(0) + " ms" +
      "｜比值 " + (tail / Math.max(1, head)).toFixed(2) + "×（**>1.5 ⇒ 随对象数累积** ✗）");
  }
  const wallSeconds = (Date.now() - started) / 1000;
  await sleep(500); // 让异步的缩略图/缓存落定 ✓
  const after = sample();

  const rssGrowth = after.rssMb - before.rssMb;
  const cpuSeconds = after.cpuSeconds - before.cpuSeconds;
  const cpuPerStroke = (cpuSeconds * 1000) / Math.max(1, ok);

  // **把「阶段外」的时间从外面切开** ✓（第 824 轮 ✓）：分别计时几种调用 ✓ ⇒ 差值定位成本 ✓。
  // 不动产品 ✓（最小改动 ✓）：get_document 与渲染无关 ⇒ HTTP+tool 的基线 ✓；
  // render_region 只渲染 ✓；brush_stroke 是全链 ✓。
  const timeIt = async (label, fn, rounds) => {
    const t0 = process.hrtime.bigint();
    for (let i = 0; i < rounds; i += 1) await fn(i);
    const ms = Number(process.hrtime.bigint() - t0) / 1e6 / rounds;
    console.log("    " + label.padEnd(32) + ms.toFixed(2) + " ms/次（" + rounds + " 次 ✓）");
    return ms;
  };
  console.log("  各调用的**单次耗时**（同机、同文档 ✓）：");
  await timeIt("get_document（无关渲染 ⇒ 基线）", () => fetch(BASE + "/api/tools?doc=boot&token=" + token, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ tool: "get_document", arguments: {} }) }).then((r) => r.json()), 10);
  await timeIt("render_region（只渲染 ✓）", () => fetch(BASE + "/api/tools?doc=boot&token=" + token, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ tool: "render_region", arguments: { x: 0, y: 0, w: 512, h: 512 } }) }).then((r) => r.json()), 3);
  await timeIt("brush_stroke（全链 ✓）", (i) => fetch(BASE + "/api/tools?doc=boot&token=" + token, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ tool: "brush_stroke", arguments: {
      layer_id: "L", object_id: "t_" + i, brush: "2B_pencil", size: 24,
      color: { r: 40, g: 90, b: 200, a: 255 },
      points: [[300 + i, 300, 0.4], [340 + i, 310, 0.8], [380 + i, 300, 0.5]] } }) }).then((r) => r.json()), 5);
  console.log("  画笔数：" + STROKES + "（成功 " + ok + "）｜文档 512×512｜每笔 3 个点");
  console.log("  服务端 RSS：" + before.rssMb.toFixed(1) + " MB ⇒ " + after.rssMb.toFixed(1) + " MB" +
    "（增量 " + rssGrowth.toFixed(1) + " MB）");
  console.log("  服务端 CPU：" + cpuSeconds.toFixed(2) + " s（每笔 " + cpuPerStroke.toFixed(1) + " ms）" +
    "｜墙钟 " + wallSeconds.toFixed(2) + " s");
  console.log("  ⇒ 覆盖率：服务端 RSS ✓、服务端 CPU 时间 ✓；**浏览器侧内存/CPU 不在本判据内** ✗（见说明 ✓）");

  if (after.rssMb > ABSOLUTE_RSS_BUDGET_MB) {
    fail("绝对 RSS " + after.rssMb.toFixed(0) + " MB 超过设计 14.10 的 " + ABSOLUTE_RSS_BUDGET_MB + " MB（4K/10 图层）");
  }
  if (RSS_GROWTH_BUDGET !== null && rssGrowth > RSS_GROWTH_BUDGET) {
    fail("绘画过程中 RSS 增量 " + rssGrowth.toFixed(1) + " MB 超过预算 " + RSS_GROWTH_BUDGET + " MB");
  }
  if (CPU_PER_STROKE_BUDGET !== null && cpuPerStroke > CPU_PER_STROKE_BUDGET) {
    fail("每笔 CPU " + cpuPerStroke.toFixed(1) + " ms 超过预算 " + CPU_PER_STROKE_BUDGET + " ms");
  }
  if (RSS_GROWTH_BUDGET === null || CPU_PER_STROKE_BUDGET === null) {
    console.log("  ⓘ 增量预算**尚未设定** ✓（`PAINT_RSS_GROWTH_MB` / `PAINT_CPU_MS_PER_STROKE`）——" +
      "**先测出来再定** ✓，而不是凭空写一个数 ✓（本轮就是这次测量 ✓）。");
  } else {
    console.log("  ✓ RSS 增量与每笔 CPU 都在预算内");
  }
} catch (error) {
  console.error("❌ 判据无法运行（不是通过 ✗）：" + String((error && error.message) || error));
  if (childLog.length) {
    console.error("   服务端最后 " + childLog.length + " 行输出：");
    for (const line of childLog) console.error("     ｜" + line.slice(0, 150));
  }
  failed = true;
} finally {
  try { writeFileSync(CHILD_LOG, childLog.join("\n") + "\n"); } catch { /* 忽略 */ }
  console.log("  服务端输出：" + CHILD_LOG + "（" + childLog.length + " 行 ✓）");
  try { child.kill("SIGTERM"); } catch { /* 已退出 */ }
  try { rmSync(work, { recursive: true, force: true }); } catch { /* 忽略 */ }
}
process.exit(failed ? 1 : 0);
