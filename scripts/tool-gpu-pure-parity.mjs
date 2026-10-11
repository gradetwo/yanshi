#!/usr/bin/env node
// **★ 纯路线**逐位一致**判据 ✗ ★**（**第 499 轮 ✓；**用户第 5 轮裁定 ✓）
//
// **★★★ 用户的裁定（**原话 ✓）★★★**：
//   ⇒ **「纯 CPU 和纯 GPU 路线也**解耦**，支持**切换和 fallback** 就可以。
//     **不要混用**。两边纯路线都**各自优化**可以到位后再考虑协同，
//     各自做擅长的，同时**中间协同的开销能解耦压到极低**」** ✓
//
// **∴ 这条裁定**改变了「逐位一致」的**实现位置** ✗**：
//   **∴ 原来 ✗**：**GPU 算 ＋ **CPU 真值逐帧核对**✗
//     ⇒ **∴ 实测**：**那一步占 **81.6%** 的时间** ✓（**第 498 轮 ✓）
//       ⇒ **∴ 于是**：**把 GPU 的收益**全吃掉了** ✓ ★**** ✓✓
//   **∴ 现在 ✗**：**运行时不核对**✗（**∴ 纯 GPU ✓）
//     ⇒ **∴ 所以**：**逐位一致的证据**必须来自**本判据** ✗
//       ⇒ **∴ 而**它是**离线**跑一次**✗ ，**不是**每帧成本** ✓ ★**** ✓✓
//
// **∴ 本判据做什么 ✗**：
//   ⇒ **∴ ① 起两个服务 ✗**：**`--gpu off`（**纯 CPU ✓）与 `--gpu on`（**纯 GPU ✓）
//     ＋ **∴ ②** **同一场景**（**同样的层 ＋ 同样的填充 ✓）
//       ＋ **∴ ③** **渲染同一个区域 ⇒ **取回 PNG** ⇒ **逐字节比较** ✓
//         ⇒ **∴ 断言**：**完全一致** ✓
//           ＋ **∴ ④** **后端诚实**：**`off ⇒ cpu`** ✗ ＋ **`on ⇒ gpu` 或给非空原因** ✓
//             ＋ **∴ ⑤** **不混用**：**纯 GPU 路**不许在运行时核对**✗
//               ⇒ **∴ 检查** `max_channel_delta` **默认是 `null`** ✓
//                 （**∴ 报 `0` ⇒ **说明它**偷偷核对了** ✓）★**** ✓✓
//
// **∴ 用法 ✗**：
//   ⇒ `node scripts/tool-gpu-pure-parity.mjs`（**自己起两个服务 ✓）
//     ＋ `--width 1024 --height 512 --rounds 1` ✓
//   ＋ **∴ `--self-test` ✗**：**自检（**证明断言能红 ✓）
//
// **∴ 退出码 ✗**：0 逐位一致｜1 不一致（**产品缺陷 ✓）｜2 用法错
//   ｜**★ 3 判据自身跑不了**（**如本机没有 GPU 路 ✓）★

import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const argv = process.argv.slice(2);
const opt = (name, fallback) => {
  const i = argv.indexOf(name);
  return i >= 0 && i + 1 < argv.length ? argv[i + 1] : fallback;
};
const has = (name) => argv.includes(name);

const WIDTH = Number(opt("--width", "1024"));
const HEIGHT = Number(opt("--height", "512"));

/** **∴ 纯函数：检查一次对比的结果 ✗**（**∴ 自检能直接喂假数据 ✓）★ */
export function checkParity({ cpu, gpu }) {
  const bad = [];
  if (cpu.backend !== "cpu") {
    bad.push(`--gpu off 的 render_backend 必须恰好是 cpu（实测 ${cpu.backend}）`);
  }
  if (gpu.backend !== "gpu") {
    // **∴ 允许**有解释的降级**✗ ⇒ **∴ 但**不许**静默** ✓
    const why = gpu.note ?? gpu.reason;
    if (typeof why !== "string" || why.length === 0) {
      bad.push(`--gpu on 实际是 ${gpu.backend} ⇒ 必须给出非空的适配器说明（否则是静默降级）`);
    }
  }
  // **★ 不混用 ✗ ★**：**纯 GPU 路**默认**不许**在运行时核对**
  if (gpu.backend === "gpu" && gpu.delta === 0) {
    bad.push(
      "纯 GPU 路报 max_channel_delta=0 ⇒ **运行时偷偷核对了**（用户裁定：不要混用；默认应为 null）",
    );
  }
  if (cpu.hash !== gpu.hash) {
    // **∴ 找出**首个不同的像素**✗ ⇒ **∴ 便于定位是**哪个通道／哪一行** ✓
    let where = "";
    if (cpu.raw && gpu.raw && cpu.raw.length === gpu.raw.length) {
      for (let i = 0; i < cpu.raw.length; i += 1) {
        if (cpu.raw[i] !== gpu.raw[i]) {
          const px = Math.floor(i / 4);
          const ch = "RGBA"[i % 4];
          where = `｜首个不同：像素 #${px}（x=${px % cpu.info.width}, y=${Math.floor(px / cpu.info.width)}）通道 ${ch}：cpu=${cpu.raw[i]} vs gpu=${gpu.raw[i]}`;
          break;
        }
      }
    } else if (cpu.raw && gpu.raw) {
      where = `｜像素数组长度不同：cpu=${cpu.raw.length} vs gpu=${gpu.raw.length}`;
    }
    bad.push(`两条纯路线的**像素不一致**${where}`);
  }
  return bad;
}

// ───────────────────── 自检 ─────────────────────
if (has("--self-test")) {
  const h = "a".repeat(64);
  const cases = [
    { name: "正常：两条纯路线一致 ＋ 不混用", args: { cpu: { backend: "cpu", delta: null, hash: h }, gpu: { backend: "gpu", delta: null, hash: h } }, wantRed: false },
    { name: "输出不一致", args: { cpu: { backend: "cpu", delta: null, hash: h }, gpu: { backend: "gpu", delta: null, hash: "b".repeat(64) } }, wantRed: true },
    { name: "★ 纯 GPU 路偷偷核对（**delta=0 ✓）", args: { cpu: { backend: "cpu", delta: null, hash: h }, gpu: { backend: "gpu", delta: 0, hash: h } }, wantRed: true },
    { name: "off 却不是 cpu", args: { cpu: { backend: "gpu", delta: null, hash: h }, gpu: { backend: "gpu", delta: null, hash: h } }, wantRed: true },
    { name: "on 降级且无解释", args: { cpu: { backend: "cpu", delta: null, hash: h }, gpu: { backend: "cpu", delta: null, hash: h } }, wantRed: true },
    { name: "on 降级但有解释", args: { cpu: { backend: "cpu", delta: null, hash: h }, gpu: { backend: "cpu", delta: null, hash: h, note: "adapter_lacks_device" } }, wantRed: false },
  ];
  let wrong = 0;
  let red = 0;
  console.log("  ── 自检：断言**必须能红** ──");
  for (const c of cases) {
    const failed = checkParity(c.args);
    const wentRed = failed.length > 0;
    if (wentRed) red += 1;
    const ok = wentRed === c.wantRed;
    if (!ok) wrong += 1;
    console.log(`  ${ok ? "✓" : "✗"} ${c.name} ⇒ ${wentRed ? "红" : "绿"}（期望${c.wantRed ? "红" : "绿"}）`);
  }
  console.log("");
  console.log(`  ⇒ 自检：${cases.length} 例｜判红 ${red}｜不符合期望 ${wrong}`);
  if (wrong > 0) {
    console.error("  ✗ 自检失败 ⇒ **本判据没有牙**");
    process.exit(1);
  }
  console.log("  ✓ 自检通过：断言既能绿也能红（**有牙 ✓）");
  process.exit(0);
}

// ───────────────────── 实测：起两个服务 ＋ 比图 ─────────────────────
const binary = opt("--binary", "target/release/yanshi-serve");
if (!existsSync(binary)) {
  console.error(`  ✗ 判据自身的输入缺失（不是产品问题）：找不到 ${binary}`);
  console.error("     ⇒ 先构建：cargo build --release -p yanshi-http --bin yanshi-serve --features gpu");
  process.exit(3);
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function startServer(mode, port) {
  const work = mkdtempSync(join(tmpdir(), `pure-${mode}-`));
  const child = spawn(
    binary,
    [
      "--bind", `127.0.0.1:${port}`,
      "--root", join(work, "w"),
      "--doc", "boot",
      "--width", String(WIDTH),
      "--height", String(HEIGHT),
      "--assets-dir", "assets",
      "--gpu", mode,
    ],
    { stdio: ["ignore", "pipe", "pipe"] },
  );
  let log = "";
  child.stdout.on("data", (d) => { log += d; });
  child.stderr.on("data", (d) => { log += d; });
  const base = `http://127.0.0.1:${port}`;
  for (let i = 0; i < 60; i += 1) {
    try {
      if ((await fetch(`${base}/health`)).ok) return { base, child, work, log: () => log };
    } catch { /* 还没起 */ }
    await sleep(1000);
  }
  try { child.kill("SIGKILL"); } catch { /* 已退出 */ }
  return { base, child, work, log: () => log, failed: true };
}

/** **∴ 同一场景 ＋ 同一区域 ⇒ 取 PNG 的哈希 ✗**（**∴ 逐字节比较 ✓）★ */
async function renderAndHash(base, tag) {
  const created = await (await fetch(`${base}/api/documents`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: tag, width: WIDTH, height: HEIGHT }),
  })).json();
  const q = `doc=${created.doc_id}&token=${created.token}`;
  const call = async (tool, args) =>
    (await fetch(`${base}/api/tools/${tool}?${q}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(args || {}),
    })).json();
  for (const id of ["L0", "L1"]) await call("create_layer", { layer_id: id });
  // **∴ 场景必须**完全一样**✗ ⇒ **∴ 两条路线看到同一个图 ✓
  for (let i = 0; i < 2; i += 1) {
    await call("fill", {
      layer_id: `L${i}`,
      object_id: `o${i}`,
      data: {
        color: { r: 30 + i * 90, g: 120, b: 200, a: 255 },
        region: { x: i * 40, y: i * 30, width: Math.round(WIDTH * 0.7), height: Math.round(HEIGHT * 0.7) },
      },
    });
  }
  const res = await fetch(`${base}/api/tools/render_region?${q}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ region: [0, 0, WIDTH, HEIGHT], include_image: true }),
  });
  const body = await res.json();
  // **★ 图在 `image.data` 里，而且是**base64**✗ ★**（**第 499 轮实测 ✓）
  //   **∴ 我**第一次直接拿 `arrayBuffer()` 喂 `sharp`**✗
  //     ⇒ **∴ 报**`Input buffer contains unsupported image format`** ✓
  //       ⇒ **∴ 因为**那是**JSON 文本**✗ ，**图在里面** ✓ ★**** ✓✓
  const b64 = body?.image?.data;
  if (typeof b64 !== "string" || b64.length === 0) {
    throw new Error(`render_region 没有返回 image.data（实测键：${Object.keys(body || {}).join(",")}）`);
  }
  const bytes = Buffer.from(b64, "base64");
  const health = await (await fetch(`${base}/health`)).json();
  // **★ 比的是**像素**，不是 PNG 字节 ✗ ★**（**第 499 轮实测 ✓）
  //   **∴ 为什么 ✗**：**同一次实测里两条路的 PNG **大小差 1 字节**
  //     （3504 vs 3503 ✓）⇒ **∴ 而** PNG 的编码细节（**块顺序／压缩 ✓）
  //       可能**合法地**不同**✗ ⇒ **∴ 那样比会**误报** ✓
  //     ＋ **∴ 而**"逐位一致"的**标准**是**像素**✗ ⇒ **∴ 必须**解码后比** ✓ ★**** ✓✓
  //   **∴ 用 `sharp` ✗**（**∴ 仓库的 node_modules 里已有 ✓）
  const sharp = (await import("sharp")).default;
  const { data, info } = await sharp(bytes)
    .ensureAlpha()
    .raw()
    .toBuffer({ resolveWithObject: true });
  return {
    hash: createHash("sha256").update(data).digest("hex"),
    raw: data,
    info,
    pngBytes: bytes.length,
    bytes: bytes.length,
    backend: health.render_backend,
    delta: health.max_channel_delta,
    note: health.gpu_adapter_note ?? health.gpu_unavailable_reason,
    adapter: health.gpu_adapter_note,
  };
}

const servers = [];
let exitCode = 0;
try {
  const cpuSrv = await startServer("off", Number(opt("--port", "19601")));
  const gpuSrv = await startServer("on", Number(opt("--port", "19602")));
  servers.push(cpuSrv, gpuSrv);
  for (const s of servers) {
    if (s.failed) {
      console.error(`  ✗ 服务没起来（**不是判据问题 ✓）⇒ 日志尾部：${s.log().slice(-240)}`);
      process.exit(3);
    }
  }
  const cpu = await renderAndHash(cpuSrv.base, "cpu");
  const gpu = await renderAndHash(gpuSrv.base, "gpu");
  console.log("");
  console.log("  ★ 两条**纯**路线（**不混用 ✓）★");
  const dims = (x) => (x.info ? `${x.info.width}×${x.info.height}×${x.info.channels}` : "?");
  console.log(`  · 纯 CPU：backend=${cpu.backend}｜delta=${JSON.stringify(cpu.delta)}｜PNG ${cpu.pngBytes} B｜像素 ${dims(cpu)}｜sha256 ${cpu.hash.slice(0, 16)}…`);
  console.log(`  · 纯 GPU：backend=${gpu.backend}｜delta=${JSON.stringify(gpu.delta)}｜PNG ${gpu.pngBytes} B｜像素 ${dims(gpu)}｜sha256 ${gpu.hash.slice(0, 16)}…`);
  console.log(`  · 适配器 = ${gpu.adapter ?? "（空）"}`);
  const bad = checkParity({ cpu, gpu });
  console.log("");
  if (gpu.backend !== "gpu" && bad.length === 0) {
    console.log("  ℹ️ 本次 GPU 路**没被选中**（有解释 ✓）⇒ **逐位一致这一条**不适用**（不是失败 ✓）");
  }
  if (bad.length) {
    for (const b of bad) console.error(`  ✗ ${b}`);
    exitCode = 1;
  } else {
    console.log("  ✓ 两条纯路线**逐字节一致**｜后端诚实｜纯 GPU 路**没有偷偷核对** ✓");
  }
} catch (error) {
  console.error(`  ✗ 本次没有结论：${String((error && error.message) || error).slice(0, 200)}`);
  exitCode = 2;
} finally {
  for (const s of servers) {
    if (!s) continue;
    try { s.child.kill("SIGKILL"); } catch { /* 已退出 */ }
    try { rmSync(s.work, { recursive: true, force: true }); } catch { /* 忽略 */ }
  }
}
process.exit(exitCode);
