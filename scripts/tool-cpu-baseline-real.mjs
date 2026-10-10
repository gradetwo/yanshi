#!/usr/bin/env node
// **★ 阶段二对照用的**CPU 四元组基线** ✗ ★**（第 191 轮 ✓；**用户裁决 ✓）。
//
// **∴ 为什么另写一个 ✗**：
//   **∴ ①** **场景必须**定死**✗** —— **文档 §14.1 那条是"**4K ＋ 4 层 ＋ 整幅**"** ✓
//     而 `tool-render-cost-accounts` 用的是 **`fill` 式 3840×2160** ✗ ⇒ **∴ 两套混着用会**对不上 ✓**
//   **∴ ②** **★ 夹具必须**真实素材** ✗ ★**（第 189 轮的教训 ✓）：
//     合成夹具（**空文档 ＋ 程序化笔触 ✓）把**占比整个颠倒** ✗
//     （**实测：量化 49.5%（**合成 ✓）vs 24.8%（**真实 ✓）** ✓✓
//   ⇒ **∴ 所以**：**本工具**用**仓库自带的真实参考图**✗
//     （`web/samples/sample-*.png` ✓）**铺在 4K 的 4 层上** ✓
//
// **★ 量四元组 ✗ ★**（目标第 5 条 ✓）：
//   **∴ ①** **墙钟**✗ **∴ ②** **进程 CPU（**utime+stime ✓）**✗
//   **∴ ③** **CPU÷墙钟（**并行度代理 ✓）**✗ **∴ ④** **峰值常驻（**VmHWM ✓）** ✓✓
//
// **∴ 口径 ✗**：**第一轮**是**热身**✗ ⇒ **∴ 已**从统计里剔除 ✓（**与既有工具一致 ✓）** ✓✓
//
// 用法：node scripts/tool-cpu-baseline-real.mjs [--rounds N]

import { spawn } from "node:child_process";
import { existsSync, statSync, mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const arg = (name, dflt) => {
  const i = process.argv.indexOf(name);
  return i >= 0 && process.argv[i + 1] ? Number(process.argv[i + 1]) : dflt;
};
const ROUNDS = arg("--rounds", 5);
const DOC_W = 4096;
const DOC_H = 4096;
const SAMPLES = ["sample-oil", "sample-watercolor", "sample-lake", "sample-yanshi"];

// **★ 选**最新的**二进制** ✗ ★**（第 173 轮的教训：**旧 release 会静默顶替 ✓）
const pickBinary = () => {
  const cands = [];
  for (const root of [process.env.CARGO_TARGET_DIR, "target"].filter(Boolean)) {
    for (const kind of ["release", "debug"]) {
      const p = join(root, kind, "yanshi-serve");
      if (existsSync(p)) cands.push(p);
    }
  }
  if (process.env.YANSHI_SERVE_BIN) cands.unshift(process.env.YANSHI_SERVE_BIN);
  cands.sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs);
  return cands[0];
};

/** **∴ 读进程四元组 ✗**（**utime+stime ＋ VmHWM ✓）** */
const readProc = (pid) => {
  try {
    const stat = readFileSync(`/proc/${pid}/stat`, "utf8");
    const tail = stat.slice(stat.lastIndexOf(")") + 2).split(" ");
    const utime = Number(tail[11]);
    const stime = Number(tail[12]);
    const status = readFileSync(`/proc/${pid}/status`, "utf8");
    const hwm = Number((status.match(/VmHWM:\s+(\d+)/) || [])[1] || 0);
    return { cpuTicks: utime + stime, hwmKb: hwm };
  } catch {
    return { cpuTicks: 0, hwmKb: 0 };
  }
};

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const median = (xs) => {
  const s = [...xs].sort((a, b) => a - b);
  return s.length === 0 ? 0 : s[Math.floor(s.length / 2)];
};

const BIN = pickBinary();
if (!BIN) { console.error("✗ 找不到 yanshi-serve ⇒ **∴ 本跑没有结论**"); process.exit(2); }
console.log(`  二进制：${BIN}（mtime ${statSync(BIN).mtime.toISOString()}）`);

const port = 14300 + Math.floor(Math.random() * 80);
const root = mkdtempSync(join(tmpdir(), "cpureal-"));
const proc = spawn(BIN, ["--root", root, "--bind", `127.0.0.1:${port}`], { stdio: "ignore" });
const base = `http://127.0.0.1:${port}`;

const call = async (doc, token, tool, args) => {
  const r = await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}),
  });
  return await r.json();
};

try {
  let up = false;
  for (let i = 0; i < 80; i += 1) {
    try { if ((await fetch(`${base}/health`)).ok) { up = true; break; } } catch { /* 还没起来 */ }
    await sleep(300);
  }
  if (!up) throw new Error(`服务没起来（端口 ${port}）`);

  const doc = "cpureal";
  const token = (await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: DOC_W, height: DOC_H }),
  })).json()).token;

  // **★ 真实素材铺 4 层 ✗ ★**（**∴ 每张**按其**原生尺寸**放在**四个象限** ✓）
  let i = 0;
  for (const name of SAMPLES) {
    i += 1;
    const layer = `R${i}`;
    const png = readFileSync(join("web", "samples", `${name}.png`));
    await call(doc, token, "create_layer", { layer_id: layer, name: layer });
    // **★ 上传后服务端**已解码成原始像素** ✗ ★**（第 189 轮的坑 ✓）：
    //   **∴ `mime_type` 变成 `image/x-yanshi-raw`**✗
    //     ＋ **∴ `size` 是**解码后**的字节数 ✓**** ✓✓
    //   ⇒ **∴ 所以**：**必须**用响应里那两个值**✗
    //     ⇒ **∴ 而**不是**上传 PNG 的字节数 ✓（**∴ 否则**报"**PNG 解码失败 ✓" ✓）** ✓✓
    const up2 = await (await fetch(`${base}/api/blob?doc=${doc}&token=${token}`, {
      method: "POST", headers: { "content-type": "image/png" }, body: png,
    })).json();
    const w = up2.decoded_from ? up2.decoded_from.width : 900;
    const h = up2.decoded_from ? up2.decoded_from.height : 640;
    const x = ((i - 1) % 2) * 2000;
    const y = Math.floor((i - 1) / 2) * 2000;
    // **∴ `region` **必须匹配位图尺寸** ✗**（第 189 轮的坑 ✓）。
    const imported = await call(doc, token, "import_image", {
      layer_id: layer,
      bitmap: { blob_hash: up2.blob_hash, size: up2.size, mime_type: up2.mime_type },
      region: { x, y, w, h },
    });
    if (imported.ok !== true) throw new Error(`导入 ${name} 失败：${JSON.stringify(imported).slice(0, 200)}`);
  }
  console.log(`  场景：${DOC_W}² ＋ ${SAMPLES.length} 层**真实素材** ✓`);

  const region = { x: 0, y: 0, w: DOC_W, h: DOC_H };
  const samples = [];
  for (let round = 1; round <= ROUNDS; round += 1) {
    const warm = round === 1;
    const before = readProc(proc.pid);
    const t0 = process.hrtime.bigint();
    const out = await call(doc, token, "render_region", { region });
    const wallMs = Number(process.hrtime.bigint() - t0) / 1e6;
    const after = readProc(proc.pid);
    samples.push({
      wallMs,
      cpuMs: (after.cpuTicks - before.cpuTicks) * (1000 / 100),
      hwmKb: after.hwmKb,
      ok: out.ok === true,
      warm,
    });
    console.log(`    第 ${round} 轮${warm ? "（热身，剔除）" : ""}：墙钟 ${wallMs.toFixed(1)} ms｜ok=${out.ok === true}`);
  }
  const usable = samples.length > 1 ? samples.slice(1) : samples;
  const wall = median(usable.map((s) => s.wallMs));
  const cpu = median(usable.map((s) => s.cpuMs));
  console.log("");
  console.log("  ★ **真实素材**的四元组（4K ＋ 4 层 ＋ 整幅；中位数；热身已剔除）★");
  console.log(`    ① 墙钟 ${wall.toFixed(1)} ms｜② 进程 CPU ${cpu.toFixed(1)} ms｜③ CPU÷墙钟 ${(cpu / wall).toFixed(2)}×｜④ 峰值 RSS ${(Math.max(...samples.map((s) => s.hwmKb)) / 1024).toFixed(0)} MB`);
  console.log(`    （预热墙钟 ${samples[0].wallMs.toFixed(1)} ms｜全部 ok=${samples.every((s) => s.ok)}）`);
} catch (error) {
  console.error(`  ✗ 本跑没有结论：${String(error && error.message || error).slice(0, 200)}`);
  proc.kill("SIGKILL");
  process.exit(2);
}
proc.kill("SIGKILL");
process.exit(0);
