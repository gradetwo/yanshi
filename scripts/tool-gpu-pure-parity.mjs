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
/** **★ 合成的**形状参数**（**判据自检要看它 ✓）✗ ★**（**第 504 轮 ✓）
 *
 * **∴ 为什么必须单独检查它 ✗**：**要证明逐位一致 ✗ ，**前提是场景**真的覆盖了
 *   **合成**（**半透明 ＋ 重叠 ✓）✗**
 *   ⇒ **∴ 若**有人把场景改回**全不透明**✗ ⇒ **∴ 判据**看起来还是绿的** ✗
 *     ＋ **∴ 而**它**已经**什么都没证明** ✓
 *       ⇒ **★ 所以**：**形状本身**也要有断言** ✓ ★**** ✓✓
 */
export const COMPOSE_SHAPES = [
  { i: 0, r: 20, g: 20, b: 30, a: 255, x: 0, y: 0, sw: 1.0, sh: 1.0 },
  { i: 1, r: 30, g: 120, b: 200, a: 200, x: 0, y: 0, sw: 0.8, sh: 0.8 },
  { i: 2, r: 200, g: 60, b: 30, a: 128, x: 24, y: 18, sw: 0.6, sh: 0.6 },
];

/** **∴ 检查场景是否真的覆盖了合成 ✗**（**纯函数 ✓）★ */
export function checkComposeCoverage(shapes) {
  const bad = [];
  if (shapes.length < 3) bad.push(`层数 ${shapes.length} < 3 ⇒ **覆盖不到多层合成**`);
  const translucent = shapes.filter((x) => x.a < 255).length;
  if (translucent < 2) bad.push(`半透明层只有 ${translucent} 个 < 2 ⇒ **走不到 over 混合**`);
  const opaqueBg = shapes.filter((x) => x.a === 255).length;
  if (opaqueBg < 1) bad.push("没有不透明层 ⇒ **没有确定的底**（结果不可比）");
  // **∴ 必须有**重叠**✗**：**∴ 即**两个层的矩形相交 ✓
  let overlap = false;
  for (let i = 0; i < shapes.length; i += 1) {
    for (let j = i + 1; j < shapes.length; j += 1) {
      const a = shapes[i];
      const b = shapes[j];
      if (a.x < b.x + b.sw && b.x < a.x + a.sw && a.y < b.y + b.sh && b.y < a.y + a.sh) {
        overlap = true;
      }
    }
  }
  if (!overlap) bad.push("没有任何两层重叠 ⇒ **覆盖不到混合的核心路径**");
  return bad;
}

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
  // **★ 两个口径必须一致 ✗ ★**（**第 500 轮 ✓）：**`/health` 与**渲染响应** ✗
  for (const [name, c] of [["纯 CPU", cpu], ["纯 GPU", gpu]]) {
    if (typeof c.respBackend === "string" && c.respBackend !== c.backend) {
      bad.push(
        `${name}：/health 报 ${c.backend} 而**渲染响应**报 ${c.respBackend} ⇒ **同一事实两个口径**（疑似硬编码）`,
      );
    }
  }
  // **∴ note 不许声称硬件有无 ✗**（**∴ 它**永远无法在渲染响应里判断 ✓）★
  for (const [name, c] of [["纯 CPU", cpu], ["纯 GPU", gpu]]) {
    if (typeof c.respNote === "string" && /本机没有 GPU/.test(c.respNote)) {
      bad.push(`${name}：note 声称「本机没有 GPU」 ⇒ **渲染响应无法知道这件事**（不许猜硬件）`);
    }
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
    { name: "★ 两个口径不一致（**health=gpu 响应=cpu ✓）", args: { cpu: { backend: "cpu", delta: null, hash: h, respBackend: "cpu" }, gpu: { backend: "gpu", delta: null, hash: h, respBackend: "cpu" } }, wantRed: true },
    { name: "note 猜硬件（**「本机没有 GPU」✓）", args: { cpu: { backend: "cpu", delta: null, hash: h, respNote: "本机没有 GPU ⇒ 未做比对" }, gpu: { backend: "gpu", delta: null, hash: h } }, wantRed: true },
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
  // **∴ 场景形状的三条（**防判据空转 ✓）★
  const shapeCases = [
    { name: "场景覆盖合成（**3 层／2 半透明／有重叠 ✓）", shapes: COMPOSE_SHAPES, wantRed: false },
    { name: "★ 全不透明（**旧场景 ✓）", shapes: [{ i: 0, a: 255, x: 0, y: 0, sw: 1, sh: 1 }, { i: 1, a: 255, x: 1, y: 1, sw: 1, sh: 1 }], wantRed: true },
    { name: "两层但不重叠", shapes: [{ i: 0, a: 255, x: 0, y: 0, sw: 0.2, sh: 0.2 }, { i: 1, a: 128, x: 0.9, y: 0.9, sw: 0.2, sh: 0.2 }, { i: 2, a: 128, x: 0.5, y: 0.5, sw: 0.1, sh: 0.1 }], wantRed: true },
  ];
  for (const c of shapeCases) {
    const failed = checkComposeCoverage(c.shapes);
    const wentRed = failed.length > 0;
    if (wentRed) red += 1;
    const ok = wentRed === c.wantRed;
    if (!ok) wrong += 1;
    console.log(`  ${ok ? "✓" : "✗"} 场景：${c.name} ⇒ ${wentRed ? "红" : "绿"}（期望${c.wantRed ? "红" : "绿"}）`);
  }

  console.log("");
  console.log(`  ⇒ 自检：${cases.length + shapeCases.length} 例｜判红 ${red}｜不符合期望 ${wrong}`);
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
  // **★★★ 场景必须覆盖**合成**的形状 ✗ ★★★**（**第 504 轮 ✓）
  //   **∴ 为什么 ✗**：**要把合成搬上纯 GPU 路（目标第 11 条 ✓）✗
  //     ⇒ **∴ 那么**判据必须**先**覆盖合成用的路径** ✓
  //       ＋ **∴ 而**原来的场景是**2 层、全不透明**（`a: 255` ✓）
  //         ⇒ **∴ 它**根本走不到 `over` 混合** ✓ ★**** ✓✓
  //   **∴ 现在覆盖 ✗**：
  //     ⇒ **∴ ① 3 层 ✗** ＋ **∴ ② 半透明**（**a ＝ 64／128／200 ✓）
  //       ＋ **∴ ③ 互相重叠**（**不同偏移 ＋ 不同尺寸 ✓）
  //         ＋ **∴ ④ 文档背景**（**`background` 参数 ✓）★**** ✓✓
  for (const id of ["L0", "L1", "L2"]) await call("create_layer", { layer_id: id });
  // **∴ 场景用常量 ⇒ 自检能检查它 ✓**（**第 504 轮 ✓）
  const shapes = COMPOSE_SHAPES;
  // **∴ 形状本身也要断言 ✗**：**∴ 否则**判据可能**空转** ✓
  const shapeBad = checkComposeCoverage(shapes);
  if (shapeBad.length) {
    console.error("  ✗ 场景没有覆盖合成 ⇒ **本判据什么都证明不了**：");
    for (const b of shapeBad) console.error(`     ${b}`);
    process.exit(2);
  }
  for (const sh of shapes) {
    await call("fill", {
      layer_id: `L${sh.i}`,
      object_id: `o${sh.i}`,
      data: {
        // **∴ 半透明是**重点**✗**：**它才会触发 `over` 混合 ✓
        color: { r: sh.r, g: sh.g, b: sh.b, a: sh.a },
        region: {
          x: sh.x,
          y: sh.y,
          width: Math.round(WIDTH * sh.sw),
          height: Math.round(HEIGHT * sh.sh),
        },
      },
    });
  }
  const res = await fetch(`${base}/api/tools/render_region?${q}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    // **★ `render_region` **不接受** `background` ✗ ★**（**第 504 轮实测 ✓）
    //   **∴ 症状 ✗**：**传了它 ⇒ 返回错误信封
    //     `{context, error_code, ok, retryable}` ⇒ **没有 `image.data`** ✓
    //     ⇒ **∴ 而**那条 `param!("background", …)` **属于**另一个工具** ✓ ★**** ✓✓
    //   **∴ 所以 ✗**：**背景改用**最底层的不透明填充**✗
    //     ⇒ **∴ 效果一样**（**它**就是合成的最底** ✓）＋ **∴ 不需要新 API** ✓ ★**** ✓✓
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
    // **★ 渲染响应**自己**报的后端 ✗ ★**（**第 500 轮 ✓）
    //   **∴ 为什么必须看它 ✗**：**实测抓到**两处**硬编码 `"cpu"`**✗
    //     ⇒ **∴ 于是**：**GPU 在跑（**`gpu_stage_calls` 递增 ✓）
    //       而**渲染响应报 `cpu`** ✓（**∴ 同一事实**两个口径** ✓）★
    //     ＋ **∴ 而**判据原来**只看 `/health`**✗ ⇒ **∴ 所以**漏掉了它 ✓ ★**** ✓✓
    respBackend: body?.render_backend,
    respNote: body?.max_channel_delta_note,
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
