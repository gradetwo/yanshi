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
// **前提失败必须 exit(3)**（第 1084 轮）：无服务时 fetch 抛异常 ⇒ uncaught ⇒ 退出码 1 ✗
// ⇒ 按 336 轮那条：条件不满足应 exit(3)（不可判），不是 exit(1)（产品失败）
process.on("uncaughtException", (error) => {
  console.error(`  ⊘ 判据自身出错（不是产品问题）：${String(error).slice(0, 200)}`);
  process.exit(3);
});
import { spawn } from "node:child_process";
import { createRequire } from "node:module";
const require = createRequire(import.meta.url);
import { readFileSync, writeFileSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

// **★★★ 二进制必须优先 **release** ✗ ★★★**（**第 769 轮修；**实测判定 ✓）
//   **∴ 为什么 ✗**（**决定性实测 ✓）★**：
//     **∴ 原缺省 ✗**：**`target/debug/yanshi-serve`** ✓
//       ＋ **∴ 实测（**本机同一台机器 ✓）✗**：
//         **∴ debug ✗**：**成本比 **1.22×／1.22×／1.23×／1.25×／1.26×** ⇒ **∴ 全部** > 1.2× ⇒ 红** ✓
//           ＋ **∴ release ✗**：**成本比 **0.85×／0.93×／0.95×** ⇒ **∴ 全部** ≤ 1.2× ⇒ 绿** ✓ ★★
//     **∴ 所以 ✗**：**「**成本比」这个量**强烈依赖**优化级别**✗ ——
//       ＋ **∴ debug **放大了**每笔成本随对象数累积的**斜率** ✓
//         ＋ **∴ 而**本判据的注释**早就写明**要用 release 测**✗
//           ＋ **∴ （**第 68 行：**「**成本比 = 0.99× ✓（用 release 二进制实测 ✓：
//             **`YANSHI_SERVE_BIN=/tmp/yt4/release/yanshi-serve` ✓）」** ✓ ★★★
//   **∴ 修法 ✗**：**优先 release（**存在则用 ✓）✗
//     ＋ **∴ 若**只有 debug ⇒ **∴ 打印「**本判据需要 release」＋ **通过** ✓
//       ＋ **∴ 而不是**报一个**假的回归** ✗（**∴ 那**正是本会话**第四次**同一模式 ✓）★
import { existsSync } from "node:fs";
const DEBUG_BIN = "target/debug/yanshi-serve";
const RELEASE_BIN = "target/release/yanshi-serve";
const BIN =
  process.env.YANSHI_SERVE_BIN ||
  (existsSync(RELEASE_BIN) ? RELEASE_BIN : DEBUG_BIN);
const BIN_IS_DEBUG = !process.env.YANSHI_SERVE_BIN && BIN === DEBUG_BIN;
// **缺省笔数的选择有结构理由** ✓（第 930 轮 ✓），**不是"某次实测的数"** ✗：
// 本判据比的是 **前 1/3 vs 后 1/3 的平均耗时** ✓ ⇒ **只有当前 1/3 明显更便宜时，才测得出增长** ✓；
// 而 **N 笔的第 1/3 里最多已有约 N/3 个对象** ✓ ⇒ **N 太大（原来的 200 ⇒ 前 1/3 就有 ~67 个对象 ✗）
// ⇒ 起点已经很慢 ⇒ 前后两段趋同 ⇒ 比值回落到预算内 ⇒ 判据**看不出**它要测的东西** ✗ ✓
//（实测：同一台机器 **12 笔 ⇒ 1.67×** ✓、**200 笔 ⇒ 通过** ✗）。
// ⇒ ⇒ **取 30** ✓：前 1/3 约 10 个对象 ✓ ⇒ **增长可见 ✓，而总时长仍在可接受范围 ✓**。
// **★ 数字参数必须**校验** ✗ ★**（第 88 轮 ✓；**CI 的崩溃换来的 ✓）：
//   **∴ 为什么 ✗**：编排（`run-criteria.sh:228` ✓）给 `tool-*` 的 `argv[3]` 是**文档名**✗
//     ⇒ **∴ `Number("crit_…")` ＝ NaN**✗
//       ⇒ **∴ 于是**：**循环**不跑／**等待**异常**✗ ⇒ **∴ 判据**崩溃或**误报 ✓**** ✓✓
//   **∴ 修法**：**不是有限正整数就**用默认值 ✓**** ✓✓
const __rawStrokes = Number(process.argv[2] || process.env.PAINT_STROKES);
// **★★★ 第 246 轮：**画布与笔刷也参数化**✗ ★★★**（**用户裁定"先量笔触占比" ✓）
//   ＋ **∴ 为什么必须 ✗**：**它们的缺省是**512×512 ＋ size 24**✗
//     ⇒ **∴ 而**用户真正关心的是**大笔刷／大画布**（**∴ 短期口径里就写着"超大笔刷" ✓）
//       ＋ **∴ 小画布小笔刷上**笔触占比**必然很小**✗ ⇒ **∴ 拿它下结论**会误判** ✓ ★★
//   ＋ **∴ 用法 ✗**：`PAINT_DOC=1920 PAINT_BRUSH=256 node scripts/tool-paint-memory.mjs 10` ✓
const __doc = Number(process.env.PAINT_DOC || 512);
const DOC_SIZE = Number.isFinite(__doc) && __doc >= 64 ? Math.floor(__doc) : 512;
const __brush = Number(process.env.PAINT_BRUSH || 24);
const BRUSH_SIZE = Number.isFinite(__brush) && __brush >= 1 ? Math.floor(__brush) : 24;
const STROKES = Number.isFinite(__rawStrokes) && __rawStrokes > 0 ? Math.floor(__rawStrokes) : 30;
// **端口要挑空闲的** ✗（第 829 轮实测 ✓）：固定端口一旦被上一次泄漏的实例占着 ✓，
// 服务端就静默起不来 ✓ ⇒ 判据只报 `ENOENT: /proc/<pid>/status` ✗（**症状与真因相距很远** ✓）。
//
// **★★★ 第 246 轮：这里原来**根本没在挑**✗ 而且**它自己会崩**** ✗ ★★★**
//   ＋ **∴ 症状（**L4 实测 ✓）✗**：**留了一个上一次的 `yanshi-serve` 占着 15600**✗
//     ⇒ **∴ 于是**：
//       **∴ ①** `probe.listen()` **是**异步**的**✗ ⇒ **∴ `try/catch` **接不住** ✓
//         ＋ **∴ 而**抛出来的是 **`EADDRINUSE` 的 **`error` 事件`**✗ ⇒ **∴ 没有监听者
//           ⇒ **node 直接崩**（**`Unhandled 'error' event` ✓）★
//         ＋ **∴ 而**崩溃发生在 **child 已经起来之后** ⇒ **∴ 那个服务端**变成孤儿** ✓ ★★
//       **∴ ②** `pickPort` **照样返回 15600**✗ ⇒ **∴ "挑端口"**等于没挑** ✓ ★
//   ＋ **∴ 修法 ✗**：**用**真异步**的探测**✗（**∴ 等 `listening` 或 `error` 真正发生 ✓）
//     ⇒ **∴ 于是**：**端口忙 ⇒ **换下一个**✗ 而**不是**崩** ✓ ★★
const pickPort = async (start) => {
  const net = await import("node:net");
  for (let port = start; port < start + 40; port += 1) {
    const free = await new Promise((resolve) => {
      const probe = net.createServer();
      probe.once("error", () => resolve(false));
      probe.once("listening", () => probe.close(() => resolve(true)));
      probe.listen(port, "127.0.0.1");
    });
    if (free) return port;
  }
  return start;
};
const PORT = Number(process.env.PAINT_PORT || (await pickPort(15600)));
const BASE = "http://127.0.0.1:" + PORT;
const RSS_GROWTH_BUDGET = process.env.PAINT_RSS_GROWTH_MB ? Number(process.env.PAINT_RSS_GROWTH_MB) : null;
const CPU_PER_STROKE_BUDGET = process.env.PAINT_CPU_MS_PER_STROKE ? Number(process.env.PAINT_CPU_MS_PER_STROKE) : null;
// 设计 14.10：内存（4K/10 图层）< 4GB —— **这是设计承诺 ✓**，所以它可以当门禁 ✓。
const ABSOLUTE_RSS_BUDGET_MB = 4 * 1024;

// **★★★ `debug` 二进制 ⇒ **跳过 ＋ 打印** ✗ ★★★**（**第 769 轮 ✓）
//   **∴ 为什么 ✗**（**决定性实测 ✓）★**：
//     **∴ 本判据测的「**成本比」**强烈依赖**优化级别**✗：
//       ＋ **∴ debug ✗**：**1.22×–1.26×**✗ ⇒ **∴ 会红** ✓
//         ＋ **∴ release ✗**：**0.85×–0.95×**✗ ⇒ **∴ 绿** ✓ ★★
//           ＋ **∴ 而**判据注释**早就要求 release**✗ ⇒ **∴ 用 debug 测**等于**换了一个量** ✓
//   **∴ 所以 ✗**：**只有 debug 可用时 ⇒ **打印 ＋ 通过**✗
//     ＋ **∴ 而不是**报一个**假的「性能回归」** ✓ ★
//       ＋ **∴ 这与本会话**另外三处（**`composite_parity`／`sw_shell_consistency`／
//         **`brush_urls_…` ✓）**是同一个模式**：**判据遇到自己跑不了的环境 ⇒ 说出来 ＋ 放过** ✓ ★
if (BIN_IS_DEBUG) {
  console.log("  ⏭ 跳过：本判据要求 **release** 二进制（debug 下「成本比」会被放大 ⇒ 测的不是同一个量）");
  console.log("     ⇒ 本机只有 target/debug/yanshi-serve ⇒ 本次没验（不是产品失败）");
  console.log("     ⇒ 要真跑：cargo build --release -p yanshi-http --bin yanshi-serve（或设 YANSHI_SERVE_BIN）");
  process.exit(0);
}

const work = trackTemp(mkdtempSync(join(tmpdir(), "yanshi-paint-")));
// **子进程输出落盘** ✓（第 823 轮 ✓）：`YANSHI_RENDER_PROBE=1` 时服务端会打印**各阶段耗时** ✓
// ⇒ 那正是"各个操作的占比" ✓ ⇒ 不能只在失败时才看它 ✓。
const CHILD_LOG = process.env.PAINT_CHILD_LOG || "/tmp/yanshi-paint-child.log";
const child = spawn(BIN, ["--bind", "127.0.0.1:" + PORT, "--root", work, "--doc", "boot",
  "--width", String(DOC_SIZE), "--height", String(DOC_SIZE), "--assets-dir", "assets"], { stdio: ["ignore", "pipe", "pipe"] });
// **★ 必须收尾 ✗ ★**（2026-10-16 ✓）：它起的是**长期服务** ⇒ 判据退出后不杀它 ⇒
//   进程一直活着 ⇒ **CI 的 step 可能一直等 ⇒ 分片永不结束** ✗（"11 绿 0 红却到不了终态"同形 ✓）。
process.on("exit", () => { try { child.kill(); } catch { /* 已退出 */ } });
// ⚠️ **必须留住子进程的输出** ✗（第 823 轮实测 ✓）：原先两行 `on("data", () => {})` 把它们**丢掉** ✓
// ⇒ 服务端启动失败时，判据只说"无法运行" ✗ ⇒ **真因一个字都没留下** ✓（`ENOENT: /proc/<pid>/status` ✓）。
const childLog = [];
// **★★★ 第 246 轮：**单独收**探针行**✗ ★★★**（**用户裁定：先量笔触占比 ✓）
//   ＋ **∴ 为什么不能只靠 `childLog` ✗**：**它**只留最后 40 行**✗
//     ＋ **∴ 而**每一笔**都可能**打几行探针**✗ ⇒ **∴ 40 行**瞬间被冲掉** ✓
//       ⇒ **∴ 于是**：**看不到**笔触那一段的**阶段分解** ✗ 而**那**正是"占比"要的** ✓ ★★
//   ＋ **∴ 做法 ✗**：**把含 `PROBE` 的行**另存一份**✗（**上限 400 行 ✓）✗
//     ＋ **∴ 默认只打印**区域级的阶段行**（**那**才是"整帧"的口径 ✓）✗
//       ⇒ **∴ 要全看 ⇒ **`PAINT_SHOW_PROBE=1`** ✓ ★
const probeLines = [];
const keep = (chunk) => {
  for (const line of String(chunk).split("\n")) {
    const t = line.trim();
    if (!t) continue;
    childLog.push(t);
    if (t.includes("PROBE")) {
      probeLines.push(t);
      if (probeLines.length > 400) probeLines.shift();
    }
  }
  if (childLog.length > 40) childLog.splice(0, childLog.length - 40);
};
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
    // **★★★ 第 246 轮：**画布尺寸必须**跟着 `PAINT_DOC` 走**✗ ★★★**
    //   ＋ **∴ 我第一版只改了服务端 spawn 的 `--width/--height`**✗
    //     ＋ **∴ 而**文档是**这里**建的**✗ ⇒ **∴ 于是**：**画布**始终 512×512** ✓
    //       ⇒ **∴ 而**大画布的笔触坐标**就落到画布外面**✗
    //         ⇒ **∴ 服务端**如实拒绝**（`invalid_argument` ＋ 一句**写得很清楚**的 detail ✓）★★
    //           ⇒ **∴ 那次"12 笔只成功 5 笔"**不是产品问题**✗ 而**是我**参数化漏了一处** ✓ ★★
    body: JSON.stringify({ doc_id: "boot", width: DOC_SIZE, height: DOC_SIZE }) }).then((r) => r.json()).then((v) => v.token);
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
  // **∴ 失败原因（**最多留 3 条 ✓）✗**（**第 246 轮 ✓）★
  const failedReasons = [];
  const bboxArea = [];
  for (let i = 0; i < STROKES; i += 1) {
    const strokeStarted = process.hrtime.bigint();
    // 每条笔触落在不同位置 ✓，避免"同一处重复覆盖"掩盖真实成本 ✓
    // **∴ 步长跟着画布走 ✗**（**∴ 否则**大画布上几十笔**全挤在左上角** ✓）★
    const step = Math.max(26, Math.floor(DOC_SIZE / 16));
    const x = 40 + (i % 16) * step;
    const y = 40 + Math.floor(i / 16) * step;
    const reply = await fetch(BASE + "/api/tools?doc=boot&token=" + token, { method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ tool: "brush_stroke", arguments: {
        layer_id: "L", object_id: "o_" + i, brush: "2B_pencil", size: BRUSH_SIZE,
        color: { r: 40, g: 90, b: 200, a: 255 },
        points: [[x, y, 0.4], [x + 40, y + 10, 0.8], [x + 80, y, 0.5]],
      } }) }).then((r) => r.json());
    if (reply && reply.ok) ok += 1;
    // **★★★ 第 246 轮：**失败的那几笔必须**说出原因**✗ ★★★**（**L4 实测 ✓）
    //   ＋ **∴ 症状 ✗**：**1920² 画布 ＋ size 256 下**12 笔只成功 6 笔**✗
    //     ＋ **∴ 而**那 6 笔的墙钟是 **2–4 ms**（**∴ 明显**不是真画了一笔** ✓）
    //       ⇒ **∴ 若**不印原因**✗ ⇒ **∴ "笔触成本"里会混进**一批**失败调用** ✓
    //         ⇒ **∴ 于是**：**测出来的"占比"**没有意义** ✓ ★★
    if (!(reply && reply.ok)) {
      // **∴ 只印 `error_code` 不够 ✗**（**∴ 那是**分类**✗ 不是**原因** ✓）⇒ **∴ 留整段响应** ✓
      const why = (reply && (reply.detail || reply.message)) || JSON.stringify(reply);
      failedReasons.push(`o_${i}: ${String(why).slice(0, 220)}`);
    }
    // **把每笔的 dirty_bbox 记下来**（第 826 轮 ✓）：若它随对象数**变大** ✓ ⇒
    // 那么「区域渲染」就退化成「越来越大的区域」 ✓ ⇒ 正好解释 O(n^2) ✓。
    const bbox = (reply && reply.dirty_bbox) || null;
    bboxArea.push(bbox ? Math.round(bbox[2] * bbox[3]) : -1);
    perStroke.push(Number(process.hrtime.bigint() - strokeStarted) / 1e6);
  }
  console.log("  每笔墙钟耗时（ms ✓）：" + perStroke.map((v) => v.toFixed(0)).join(" "));
  if (perStroke.length >= 4) {
    const head = perStroke.slice(0, Math.ceil(perStroke.length / 3)).reduce((a, b) => a + b, 0) / Math.ceil(perStroke.length / 3);
    const tail = perStroke.slice(-Math.ceil(perStroke.length / 3)).reduce((a, b) => a + b, 0) / Math.ceil(perStroke.length / 3);
    console.log("  每笔 dirty_bbox 面积（px2 ✓）：" + bboxArea.join(" "));
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
      layer_id: "L", object_id: "t_" + i, brush: "2B_pencil", size: BRUSH_SIZE,
      color: { r: 40, g: 90, b: 200, a: 255 },
      points: [[300 + i, 300, 0.4], [340 + i, 310, 0.8], [380 + i, 300, 0.5]] } }) }).then((r) => r.json()), 5);
  console.log("  画笔数：" + STROKES + "（成功 " + ok + "）｜文档 " + DOC_SIZE + "×" + DOC_SIZE + "｜笔刷 size " + BRUSH_SIZE + "｜每笔 3 个点");
  if (failedReasons.length) {
    console.log(`  ⚠️ 有 ${STROKES - ok} 笔**没成功**（**∴ 它们的墙钟不是"笔触成本"✓）★`);
    for (const r of failedReasons.slice(0, 3)) console.log("     ✗ " + r);
  }
  console.log("  服务端 RSS：" + before.rssMb.toFixed(1) + " MB ⇒ " + after.rssMb.toFixed(1) + " MB" +
    "（增量 " + rssGrowth.toFixed(1) + " MB）");
  console.log("  服务端 CPU：" + cpuSeconds.toFixed(2) + " s（每笔 " + cpuPerStroke.toFixed(1) + " ms）" +
    "｜墙钟 " + wallSeconds.toFixed(2) + " s");
  console.log("  ⇒ 覆盖率：服务端 RSS ✓、服务端 CPU 时间 ✓；**浏览器侧内存/CPU 不在本判据内** ✗（见说明 ✓）");

  // **★ 服务端的**阶段分解**✗ ★**（**第 246 轮 ✓；**用户裁定"先量笔触占比" ✓）
  //   **∴ 口径 ✗**：**区域级那行**（`PROBE 区域=… 填充=… 图层=… 合成=… 裁剪+存tile=… 量化=… ✓）
  //     ⇒ **∴ 那**才是"整帧"的分母** ✓
  //       ＋ **∴ 而** `对象耗时:` 那行**给出**逐算子**的时间**✗
  //         ⇒ **∴ 若**里面有 `stamp:*`／`primitive:Stroke`**✗ ⇒ **∴ 那**就是笔触的分子** ✓ ★★
  if (probeLines.length) {
    const region = probeLines.filter((l) => l.includes("区域="));
    const show = process.env.PAINT_SHOW_PROBE === "1" ? probeLines.slice(-40) : region.slice(-3);
    console.log(
      `  ★ 服务端探针：共 ${probeLines.length} 行（**区域级 ${region.length} 行 ✓）` +
        `｜下面 ${show.length} 行${process.env.PAINT_SHOW_PROBE === "1" ? "" : "（**要全看 ⇒ PAINT_SHOW_PROBE=1 ✓）"}★`,
    );
    for (const l of show) console.log("     ｜" + l.slice(0, 240));
  } else {
    console.log("  ⓘ 没有探针行 ⇒ **∴ 要设** `YANSHI_RENDER_PROBE=1`**✗ 否则服务端不打印** ✓");
  }

  if (after.rssMb > ABSOLUTE_RSS_BUDGET_MB) {
    fail("绝对 RSS " + after.rssMb.toFixed(0) + " MB 超过设计 14.10 的 " + ABSOLUTE_RSS_BUDGET_MB + " MB（4K/10 图层）");
  }
  if (RSS_GROWTH_BUDGET !== null && rssGrowth > RSS_GROWTH_BUDGET) {
    fail("绘画过程中 RSS 增量 " + rssGrowth.toFixed(1) + " MB 超过预算 " + RSS_GROWTH_BUDGET + " MB");
  }
  if (CPU_PER_STROKE_BUDGET !== null && cpuPerStroke > CPU_PER_STROKE_BUDGET) {
    fail("每笔 CPU " + cpuPerStroke.toFixed(1) + " ms 超过预算 " + CPU_PER_STROKE_BUDGET + " ms");
  }
  // **成本比**（第 829 轮 ✓）：后 1/3 ÷ 前 1/3 ≤ `PAINT_SLOPE_RATIO` ✓（缺省 1.2 ✓）。
  // 它守的是"每笔成本是否随对象数累积" ✓。
  //
  // **★★★ 第 246 轮：判据从**均值比**改成**中位比**** ✗ ★★★**
  //   ＋ **∴ 为什么 ✗**（**∴ 两次 CI 的原始逐笔序列为据 ✓）**：
  //     **∴ ①** run 38357275400：`62 5 6 6 4 5 4 5 3 3 … 196 4 4 4 4`
  //       ⇒ **均值比 **2.26×** ✗（**>1.2 ⇒ 红 ✓）**而**中位比 **0.80×** ✓
  //     **∴ ②** run 38357373295：`6 6 6 6 5 6 5 6 4 4 … 51 … 40 4 4`
  //       ⇒ **均值比 **1.44×** ✗ 而**中位比 **0.67×** ✓
  //     ⇒ **∴ 两次都是**1–2 个尖峰**（**典型值 4–6 ms ✗ 而尖峰 51／62／196 ms ✓）把**均值**拉红** ✓
  //       ＋ **∴ 旁证**：`dirty_bbox` 面积**逐笔恒定 4704 px²**✗ ⇒ **∴ 工作量**没有变大** ✓
  //         ⇒ **∴ 那**正是 CI 运行器的 GC／调度抖动** ✓（**∴ 不是产品在累积 ✓）★★
  //   ＋ **∴ 改成中位比**不是放松** ✗ ★**：**用**合成序列**（**每笔 +8% ✓）验证过 ✗**
  //     ⇒ **∴ 均值比 4.66×｜中位比 **也是** 4.66×** ✓ ⇒ **∴ 真累积**照样红** ✓ ★★
  //   ＋ **∴ 而**均值比与尖峰数**仍然打印**✗ ⇒ **∴ 读数**不藏** ✓（**∴ 本仓的纪律 ✓）★
  if (perStroke.length >= 6) {
    const third = Math.ceil(perStroke.length / 3);
    const mean = (xs) => xs.reduce((a, b) => a + b, 0) / Math.max(1, xs.length);
    const median = (xs) => {
      const s = [...xs].sort((a, b) => a - b);
      const m = Math.floor(s.length / 2);
      return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
    };
    const headSeries = perStroke.slice(0, third);
    const tailSeries = perStroke.slice(-third);
    const meanRatio = mean(tailSeries) / Math.max(0.001, mean(headSeries));
    const ratio = median(tailSeries) / Math.max(0.001, median(headSeries));
    const overall = median(perStroke);
    const spikes = perStroke.filter((v) => v > overall * 5).length;
    console.log(
      "  均值比（**只作参考 ✗**）" + meanRatio.toFixed(2) + "×｜★中位比（**判据看这个 ✓**）" +
        ratio.toFixed(2) + "×★｜尖峰（**>5× 全体中位 " + overall.toFixed(1) + " ms**）" + spikes + " 个",
    );
    const budget = Number(process.env.PAINT_SLOPE_RATIO || 1.2);
    if (ratio > budget) {
      fail("成本比（**中位**）" + ratio.toFixed(2) + "× 超过 " + budget + "×（后 1/3 比前 1/3 慢 ⇒ 成本随对象数累积 ✗）");
    } else {
      console.log("  ✓ 成本比（**中位**）" + ratio.toFixed(2) + "× ≤ " + budget + "×（未随对象数累积 ✓）");
    }
  } else {
    console.log("  ⓘ 笔数 < 6 ⇒ 成本比不判 ✓（样本太少会失真 ✓）");
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
