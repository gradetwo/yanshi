#!/usr/bin/env node
// **★ 「一次请求不许把一个区域渲染多遍」判据 ✗ ★**（**第 527 轮 ✓）
//
// **★★★ 它守的是一个**已实测的真缺陷**** ★★★**：
//   **∴ 症状 ✗**：**一个 `include_image: true` 的 `render_region`**
//     ＋ **∴ 把 4K 区域**完整渲染了 3 次**** ✓（**PROBE 行计数 ✓）
//       ⇒ **∴ 而** `png_ms` 实测 **≈1062 ms**（**＝ 一次完整渲染 ＋ 缩放 16.6 ms ✓）
//         ⇒ **★ 所以：那 ≈1035 ms 是**重复工作**✗ ＝ 该请求的 **47%**** ✓ ★
//   **∴ 真因 ✗**：**缩略图那条路调了 `render_region_raw_active`** ✓
//     ⇒ **∴ 那**是**又一次完整渲染**✗（**只为拿像素去缩放 ✓）
//       ＋ **∴ 而**已经渲好的 `preview` **服务端类型不带像素** ✓
//         （**`RenderedPreview` 只有 url／blob_hash／宽高 ✓）
//         ⇒ **∴ 所以**：**修它需要**接口扩展**（**把渲染像素带出来 ✓）★
//
// **∴ 本判据做什么 ✗**：
//   ⇒ **∴ 用 `YANSHI_RENDER_PROBE=1` 起服务 ✗**（**那**是代码里现成的探针 ✓）
//     ＋ **∴ 发**一个**带图请求** ⇒ **∴ 数 PROBE 的**完成行**里该区域的次数** ✓
//       ⇒ **∴ 断言 ✗**：**次数 ≤ 上限**（**默认 1 ✓）★
//
// **∴ 为什么不用 `renders_done` ✗**：**实测它对带图与不带图**都报 2** ✓
//   ⇒ **∴ 它**区分不了重复渲染** ✓（**∴ 别拿它当证据 ✓）★
//
// **∴ 用法 ✗**：`node scripts/tool-render-count.mjs`
//   ｜`--side 3840 --height 2160`｜`--max 1`｜`--self-test`
// **∴ 退出码 ✗**：0 通过｜1 产品缺陷（**渲染了多遍 ✓）｜2 用法错
//   ｜**★ 3 判据自身跑不了**（**如二进制不存在 ✓）★

import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const argv = process.argv.slice(2);
const opt = (n, d) => {
  const i = argv.indexOf(n);
  return i >= 0 && i + 1 < argv.length ? argv[i + 1] : d;
};
const WIDTH = Number(opt("--side", "3840"));
const HEIGHT = Number(opt("--height", "2160"));
const MAX = Number(opt("--max", "1"));
const PORT = Number(opt("--port", "19611"));

/** **∴ 纯函数：数一个区域被渲染了几次 ✗**（**∴ 自检能直接喂假日志 ✓）★ */
export function countRenders(log, width, height) {
  const needle = `区域=${width}x${height}@`;
  // **∴ 只数**完成行**✗**（**进入行也有同样前缀 ⇒ **∴ 必须排除 ✓）★
  return log
    .split("\n")
    .filter((line) => line.includes("PROBE 区域=") && line.includes(needle)).length;
}

/** **∴ 纯函数：断言 ✗**（**∴ 自检能直接喂数 ✓）★ */
export function checkRenderCount({ count, max }) {
  const bad = [];
  if (count > max) {
    bad.push(`一个带图请求把 ${WIDTH}x${HEIGHT} 渲染了 ${count} 次 > 上限 ${max} ⇒ **重复渲染**`);
  }
  return bad;
}

if (argv.includes("--self-test")) {
  const cases = [
    { name: "正常：只渲一次", log: "PROBE 区域=3840x2160@0,0 对象=1", want: 1 },
    { name: "★ 旧缺陷：渲染三次", log: ["PROBE 区域=3840x2160@0,0", "PROBE 区域=3840x2160@0,0", "PROBE 区域=3840x2160@0,0"].join("\n"), want: 3 },
    { name: "只数完成行（进入行不算）", log: "PROBE 进入 区域=3840x2160@0,0\nPROBE 区域=3840x2160@0,0", want: 1 },
    { name: "不同尺寸不误数", log: "PROBE 区域=512x512@0,0\nPROBE 区域=3840x2160@0,0", want: 1 },
  ];
  let wrong = 0;
  let red = 0;
  console.log("  ── 自检：计数与断言都要能红 ──");
  for (const c of cases) {
    const got = countRenders(c.log, 3840, 2160);
    if (got !== c.want) {
      wrong += 1;
      console.log(`  ✗ ${c.name} ⇒ 数到 ${got}（期望 ${c.want}）`);
      continue;
    }
    const failed = checkRenderCount({ count: got, max: 1 });
    const wentRed = failed.length > 0;
    if (wentRed) red += 1;
    console.log(`  ✓ ${c.name} ⇒ 数到 ${got}｜${wentRed ? "红" : "绿"}（**符合预期 ✓）`);
  }
  // **∴ 断言本身的变异 ✗**：**上限 1 而次数 2 ⇒ 必须红** ✓
  const mustRed = checkRenderCount({ count: 2, max: 1 }).length > 0;
  if (!mustRed) {
    wrong += 1;
    console.log("  ✗ 变异：count=2／max=1 竟然不红 ⇒ **断言没有牙**");
  } else {
    red += 1;
    console.log("  ✓ 变异：count=2／max=1 ⇒ 红（**符合预期 ✓）");
  }
  console.log("");
  console.log(`  ⇒ 自检：${cases.length + 1} 例｜判红 ${red}｜不符合期望 ${wrong}`);
  if (wrong > 0) {
    console.error("  ✗ 自检失败 ⇒ **本判据没有牙**");
    process.exit(1);
  }
  console.log("  ✓ 自检通过：计数与断言都能红能绿（**有牙 ✓）");
  process.exit(0);
}

const binary = opt("--binary", "target/release/yanshi-serve");
if (!existsSync(binary)) {
  console.error(`  ✗ 判据自身的输入缺失（不是产品问题）：找不到 ${binary}`);
  process.exit(3);
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const work = trackTemp(mkdtempSync(join(tmpdir(), "rendercount-")));
const logPath = join(work, "server.log");
let child;
let code = 0;
try {
  child = spawn(
    binary,
    ["--bind", `127.0.0.1:${PORT}`, "--root", join(work, "w"), "--doc", "boot",
     "--width", String(WIDTH), "--height", String(HEIGHT), "--assets-dir", "assets", "--gpu", "off"],
    { stdio: ["ignore", "pipe", "pipe"], env: { ...process.env, YANSHI_RENDER_PROBE: "1" } },
  );
  // **★ 必须收尾 ✗ ★**（2026-10-16 ✓）：它起的是**长期服务** ⇒ 判据退出后不杀它 ⇒
  //   进程一直活着 ⇒ **CI 的 step 可能一直等 ⇒ 分片永不结束** ✗
  //   —— **∴ 运行器实测会打出 `Terminate orphan process (yanshi-serve)`** ✓（§309 ✓）。
  process.on("exit", () => { try { child.kill(); } catch { /* 已退出 */ } });
  const chunks = [];
  child.stdout.on("data", (d) => chunks.push(String(d)));
  child.stderr.on("data", (d) => chunks.push(String(d)));
  const base = `http://127.0.0.1:${PORT}`;
  let up = false;
  for (let i = 0; i < 60; i += 1) {
    try {
      if ((await fetch(`${base}/health`)).ok) { up = true; break; }
    } catch { /* 还没起 */ }
    await sleep(500);
  }
  if (!up) {
    console.error("  ✗ 服务没起来（**不是判据问题 ✓）");
    process.exit(3);
  }
  const created = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: "rc", width: WIDTH, height: HEIGHT }),
  })).json();
  const q = `doc=rc&token=${created.token}`;
  const call = async (tool, args) =>
    (await fetch(`${base}/api/tools/${tool}?${q}`, {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify(args || {}),
    })).json();
  await call("create_layer", { layer_id: "L0" });
  await call("fill", {
    layer_id: "L0", object_id: "o0",
    data: { color: { r: 200, g: 30, b: 60, a: 255 },
            region: { x: 5, y: 7, width: Math.round(WIDTH * 0.3), height: Math.round(HEIGHT * 0.4) } },
  });
  // **∴ 一次带图请求 ⇒ 数它渲染了几遍 ✓** ★
  const res = await call("render_region", {
    region: [0, 0, WIDTH, HEIGHT], include_image: true,
  });
  await sleep(300);
  const log = chunks.join("");
  const count = countRenders(log, WIDTH, HEIGHT);
  console.log("");
  console.log(`  ★ 一次带图请求渲染 ${WIDTH}x${HEIGHT} 的次数 ★`);
  console.log(`  · 实测次数 = ${count}（上限 ${MAX}）`);
  console.log(`  · png_ms = ${(res.timings || {}).png_ms}`);
  const bad = checkRenderCount({ count, max: MAX });
  console.log("");
  if (bad.length) {
    for (const b of bad) console.error(`  ✗ ${b}`);
    code = 1;
  } else {
    console.log("  ✓ 一次请求只渲染该区域一次（**没有重复渲染 ✓）");
  }
} catch (error) {
  console.error(`  ✗ 本次没有结论：${String((error && error.message) || error).slice(0, 200)}`);
  code = 2;
} finally {
  try { child?.kill("SIGKILL"); } catch { /* 已退出 */ }
  try {
    if (existsSync(logPath)) readFileSync(logPath);
  } catch { /* 忽略 */ }
  try { rmSync(work, { recursive: true, force: true }); } catch { /* 忽略 */ }
}
process.exit(code);
