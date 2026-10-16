#!/usr/bin/env node
// **★ 大文档冷开之后必须还能继续画（快照那条路不许把文档开坏）✗ ★**（2026-10-16 ✓）
//
// **∴ 症状 ✗**（**本机独立复现 ✓，第三方同报 ✓**）：文档原子数 ≥ `SNAPSHOT_LAG_ATOMS`（**1000** ✓）
//   时磁盘上会出现 `snapshot-<seq>.json` ✓ ⇒ **换一个进程重开**就走「快照 ＋ 尾巴」✗ ⇒
//   `head_seq` 变 **0** ✗、**再提交一笔直接失败** ✗（`渲染区域 … 与文档 0×0 不相交` ✓）、
//   此后 `get_document` 取不到 ✗ ⇒ **∴ 那不是"慢"** ✗ 而**是"可编辑文档被开坏"** ✓ ★
//
// **∴ 判什么 ✗**（**五条 ✓，全部只走真实 API ✓，而且**必须真的重启进程**✗ —— 不重启就测不到这条路 ✓**）：
//   ① 造到磁盘上**真的有快照** ✓（没造出来就**判不了 ⇒ 退 3** ✗，不许当"通过" ✓）；
//   ② **冷开**之后 `head_seq` 必须**不是 0** ✓ 且层/对象数**与重启前一致** ✓；
//   ③ 冷开之后**再提交一笔必须成功** ✓（修前这里是 `ok=false` ✗），且只多出一个对象 ✓；
//   ④ **锚点**也要在 ✓（`get_anchors` ⇒ 名字与坐标一致 ✓）；
//   ⑤ **两次冷开出图逐字节一致** ✓（第二次冷开前的画面 vs 第二次冷开后的画面 ✓ —— 两本账 ✓）。
//
// **∴ 能红 ✓**：把 `service.rs` 的打开路径改回「快照 ＋ 尾巴」⇒ ②③ 立刻红 ✓（本机修前实测就是这个形态 ✓）。
//
// 用法：node scripts/tool-snapshot-reopen.mjs [--atoms 1100] [--keep]
import { spawn } from "node:child_process";
import { mkdtempSync, readdirSync, existsSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
// **∴ 临时目录必须用**共享清理模块**✗**（本仓约定 ✓，判据 `scripts_clean_their_temp_dirs.rs` ✓）：
//   **∴ 教训 ✗**：我第一版自己写 `rmSync` ✓ ⇒ 那条约定测试**当场红** ✗（`__cleanupTemp`／`temp-dirs.mjs` 二者必居其一 ✓）
//   ⇒ **∴ 用推荐路径** ✓：`trackTemp` **顺带注册**清理 ＋ 信号处理 ✓（调用方不用再写 ✓）。
import { trackTemp } from "./lib/temp-dirs.mjs";

const argv = process.argv.slice(2);
const flag = (name, fallback) => {
  const at = argv.indexOf(name);
  return at >= 0 && argv[at + 1] !== undefined ? Number(argv[at + 1]) : fallback;
};
const ATOMS = flag("--atoms", 1100);
const BIN = process.env.YANSHI_SERVE_BIN || "target/debug/yanshi-serve";
const PORT = 8600 + (process.pid % 300);
const root = trackTemp(mkdtempSync(join(tmpdir(), "yanshi-snapshot-reopen-")));
const DOC = "coldopen";
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const startServer = async () => {
  const child = spawn(BIN, ["--bind", `127.0.0.1:${PORT}`, "--root", root, "--doc", "boot",
    "--width", "32", "--height", "32"], { stdio: ["ignore", "pipe", "pipe"] });
  let stderr = "";
  child.stderr.on("data", (chunk) => { stderr += String(chunk); });
  for (let i = 0; i < 100; i += 1) {
    try { const reply = await fetch(`http://127.0.0.1:${PORT}/health`); if (reply.ok) return child; } catch {}
    await sleep(250);
  }
  console.error(`❌ 判不了：${BIN} 起不来（stderr 尾部：${stderr.slice(-200)}）⇒ 退出码 3`);
  console.error("   ⇒ 本机若只有 release，可用 YANSHI_SERVE_BIN=target/release/yanshi-serve ✓");
  process.exit(3);
};
const base = () => `http://127.0.0.1:${PORT}`;
const call = async (token, tool, args) =>
  (await fetch(`${base()}/api/tools?doc=${DOC}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ tool, arguments: args || {} }),
  })).json();
const docDir = () => join(root, "docs", DOC);
const snapshots = () => (existsSync(docDir()) ? readdirSync(docDir()).filter((f) => f.startsWith("snapshot-")) : []);
const atomCount = () => (existsSync(join(docDir(), "atoms.jsonl"))
  ? readFileSync(join(docDir(), "atoms.jsonl"), "utf8").split("\n").filter((l) => l.trim()).length : 0);
/// 整幅原始像素 ✓（第二本账用 ✓）。
const pixels = async (token) => {
  const reply = await call(token, "render_region", { region: { x: 0, y: 0, w: 96, h: 96 }, raw: true });
  const url = reply && (reply.raw_url || reply.rawUrl);
  if (!url) return null;
  const absolute = String(url).startsWith("http") ? String(url) : `${base()}${url}`;
  return Buffer.from(await (await fetch(absolute)).arrayBuffer());
};

let server = await startServer();
const created = await (await fetch(`${base()}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: DOC, width: 96, height: 96 }),
})).json();
if (!created || !created.token) { console.error("❌ 判不了：建不了文档 ⇒ 退出码 3"); process.exit(3); }
const token = created.token;
await call(token, "create_layer", { layer_id: "L1", name: "L1" });
await call(token, "brush_stroke", { layer_id: "L1", object_id: "keep", brush: "100%_Opaque", size: 10,
  color: { r: 120, g: 60, b: 30, a: 255 }, points: [[20, 20], [70, 70]] });
await call(token, "create_layer", { layer_id: "L2", name: "L2" });
const anchor = await call(token, "set_anchor", { name: "eye", x: 48, y: 48 });
console.log(`  ① 造数据：L1/L2 各就位｜set_anchor(eye) ⇒ ok=${anchor && anchor.ok}`);

// 刷原子直到快照落盘 ✓（SNAPSHOT_LAG_ATOMS = 1000 ✓）
let strokes = 0;
while (snapshots().length === 0 && strokes < ATOMS + 200) {
  strokes += 1;
  await call(token, "brush_stroke", { layer_id: "L2", object_id: `f${strokes}`, brush: "100%_Opaque", size: 3,
    color: { r: 30, g: 30, b: 30, a: 255 }, points: [[10 + (strokes % 70), 30], [18 + (strokes % 70), 44]] });
}
const snaps = snapshots();
console.log(`  ① 刷了 ${strokes} 笔｜atoms.jsonl ${atomCount()} 行｜快照 ${JSON.stringify(snaps)}`);
if (snaps.length === 0) {
  console.error(`❌ 判不了：刷了 ${strokes} 笔仍没落快照 ⇒ 这一轮测不到那条路 ⇒ 退出码 3`);
  server.kill(); process.exit(3);
}
const beforeState = await call(token, "get_document", {});
const beforePixels = await pixels(token);
console.log(`     重启前：layers=${beforeState.layers} objects=${beforeState.objects} head_seq=${beforeState.head_seq}`);
server.kill();
await sleep(700);

// ② 冷开
server = await startServer();
const reState = await call(token, "get_document", {});
console.log(`  ② 冷开后：layers=${reState && reState.layers} objects=${reState && reState.objects} head_seq=${reState && reState.head_seq}`);
const failures = [];
if (!reState || typeof reState.objects !== "number") {
  failures.push(`冷开之后 get_document **取不到** ✗（回执：${JSON.stringify(reState).slice(0, 160)}）`);
} else {
  if (reState.head_seq === 0) failures.push(`冷开后 head_seq=0 ✗ ⇒ 日志没被真正加载（这正是修前那个形态 ✓）`);
  if (reState.head_seq !== beforeState.head_seq) {
    failures.push(`冷开后 head_seq=${reState.head_seq} ≠ 重启前 ${beforeState.head_seq} ✗`);
  }
  if (reState.layers !== beforeState.layers || reState.objects !== beforeState.objects) {
    failures.push(`冷开后层/对象数变了：${beforeState.layers}/${beforeState.objects} ⇒ ${reState.layers}/${reState.objects} ✗`);
  }
}

// ③ 冷开之后还能不能继续画 ✓
const more = await call(token, "brush_stroke", { layer_id: "L2", object_id: "after_coldopen", brush: "100%_Opaque",
  size: 8, color: { r: 200, g: 40, b: 40, a: 255 }, points: [[30, 60], [80, 70]] });
const afterState = await call(token, "get_document", {});
console.log(`  ③ 冷开后提交一笔：ok=${more && more.ok}｜layers=${afterState && afterState.layers} objects=${afterState && afterState.objects}`);
if (!more || more.ok !== true) {
  failures.push(`冷开后**再提交一笔就失败** ✗（回执：${JSON.stringify(more).slice(0, 200)}）`);
} else if (afterState && typeof afterState.objects === "number" && reState && typeof reState.objects === "number"
           && afterState.objects !== reState.objects + 1) {
  failures.push(`那一笔应当只多出一个对象：${reState.objects} ⇒ ${afterState.objects} ✗`);
}

// ④ 锚点还在不在 ✓
// **∴ 实测返回是**名字 → {x,y} 的映射**✗**（我先按数组解析 ⇒ 自己误报了一次 ✗ ——
//   这与第三方 README 里那句"没有把所有脚本错误算成产品缺陷"是同一条纪律 ✓）⇒ 两种写法都收 ✓。
const anchorsReply = await call(token, "get_anchors", {});
const raw = (anchorsReply && (anchorsReply.anchors || anchorsReply.items)) || {};
const eye = Array.isArray(raw)
  ? raw.find((item) => item && item.name === "eye")
  : (raw.eye ? { name: "eye", ...raw.eye } : null);
console.log(`  ④ 锚点：${JSON.stringify(raw).slice(0, 120)}`);
if (!eye) failures.push("冷开之后**锚点 eye 不见了** ✗");
else if (Math.round(eye.x) !== 48 || Math.round(eye.y) !== 48) failures.push(`锚点坐标变了：${eye.x},${eye.y} ≠ 48,48 ✗`);

// ⑤ 两次冷开出图逐字节一致 ✓（再冷开一次 ✓）
//   **∴ 关键 ✗**：这一对必须**在同一次提交之后各取一次** ✓ ——
//   我第一版拿"第一次重启**前**"的去比"③ 提交**后**"的 ⇒ 画面本来就该变 ✗ ⇒ **自己误报一次** ✓。
const beforeSecondOpen = await pixels(token);
server.kill();
await sleep(700);
server = await startServer();
const afterPixels = await pixels(token);
server.kill();
const beforePixelsRef = beforeSecondOpen || beforePixels;
if (!beforePixelsRef || !afterPixels) {
  failures.push("有一侧拿不到原始像素 ⇒ 第二本账没量成 ✗");
} else if (!beforePixelsRef.equals(afterPixels)) {
  failures.push(`冷开前后的出图**不一致** ✗（${beforePixelsRef.length} vs ${afterPixels.length} 字节）`);
} else {
  console.log(`  ⑤ 冷开前后出图逐字节一致 ✓（${afterPixels.length} 字节 ✓）`);
}

// **∴ 先记下原子数 ✗**（我第一版删完才读 ⇒ 打印成「0 原子」✗ —— 自己的输出也不许撒谎 ✓）。
//   **∴ 清理交给模块** ✓（它在 exit／信号时删三遍 ✓）—— **∴ 所以我这里不再自己 rmSync** ✗。
const finalAtoms = atomCount();
if (failures.length) {
  console.error(`❌ ${failures.length} 项没成立：`);
  for (const item of failures) console.error(`   · ${item}`);
  process.exit(1);
}
console.log(`✓ 大文档（${finalAtoms} 原子 ＋ 快照）冷开后：head/层/对象/锚点都在、还能继续画、两次冷开出图逐字节一致 ✓`);
