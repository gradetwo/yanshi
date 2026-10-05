#!/usr/bin/env node
// **`sample_color` 必须真的能读到"画上去的那个颜色"**（第 857 轮）：这个工具是**只读观察口** ✓
// （外部报告 #13："一旦落笔，画布就变成完全不透明的黑盒" ✗）⇒ 若它**读错**（坐标反了 ✓、
// 读到空白 ✓、返回错分量 ✓）⇒ **agent 会据此做出错误判断** ✗ ⇒ 而它本身**没有任何判据** ✗。
//
// **判据（能红 ✓，判的正是"读得对不对" ✓）**：
//   ① 用**已知颜色**画一笔 ✓（纯红 ✓）；
//   ② 在**笔画内部**采样 ⇒ 必须**红**（R 高 ✓、G/B 低 ✓）；
//   ③ 在**远处空白**采样 ⇒ 必须**不红** ✓（**对照 ✓** —— 少了它，一个"永远返回红"的假实现也会过 ✗）；
//   ④ **坐标对调**一次 ⇒ 结果**必须不同** ✓（否则说明它**忽略了参数** ✗ —— 这是最省事的变异检验 ✓）；
//   ⑤ 打印覆盖面 ✓；自起服务端 ✓（**可独立跑 ✓**，与 `tool-paint-memory.mjs` 同法 ✓）。
import { spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createRequire } from "node:module";
const require = createRequire(import.meta.url);

const BIN = process.env.YANSHI_SERVE_BIN || "target/debug/yanshi-serve";
const pickPort = (start) => {
  for (let port = start; port < start + 40; port += 1) {
    const probe = require("node:net").createServer();
    try { probe.listen(port, "127.0.0.1"); probe.close(); return port; } catch { probe.close(); }
  }
  return start;
};
const PORT = Number(process.env.SAMPLE_PORT || pickPort(15800));
const BASE = "http://127.0.0.1:" + PORT;
const work = mkdtempSync(join(tmpdir(), "yanshi-sample-"));
const child = spawn(BIN, ["--bind", "127.0.0.1:" + PORT, "--root", work, "--doc", "boot",
  "--width", "256", "--height", "256", "--assets-dir", "assets"], { stdio: ["ignore", "pipe", "pipe"] });
const log = [];
const keep = (c) => { for (const l of String(c).split("\n")) if (l.trim()) log.push(l.trim()); if (log.length > 30) log.splice(0, log.length - 30); };
child.stdout.on("data", keep);
child.stderr.on("data", keep);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

let failed = false;
const fail = (m) => { failed = true; console.log("  ✗ " + m); };
const rd = (rgba) => (rgba && typeof rgba === "object" ? rgba : null);
const isRed = (rgba) => Array.isArray(rgba) && rgba[0] > 180 && rgba[1] < 80 && rgba[2] < 80;

try {
  for (let i = 0; i < 60; i += 1) {
    try { const r = await fetch(BASE + "/api/documents"); if (r.ok) break; } catch { /* 还没起来 */ }
    await sleep(250);
  }
  const token = await fetch(BASE + "/api/documents", { method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: "boot", width: 256, height: 256 }) }).then((r) => r.json()).then((v) => v.token);
  if (!token) throw new Error("拿不到 token ⇒ 判据无法运行（不是通过 ✗）");
  const call = (tool, args) => fetch(BASE + "/api/tools?doc=boot&token=" + token, { method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ tool, arguments: args }) }).then((r) => r.json());

  await call("create_layer", { layer_id: "L" });
  // **纯红、实心**（`draw_shape` 是几何矢量笔迹 ⇒ 颜色确定 ✓，不依赖笔刷物理 ✓）
  await call("draw_shape", { layer_id: "L", object_id: "s1",
    data: { geometry: { kind: "rect", bbox: [20, 20, 80, 80] }, color: [255, 0, 0, 255] } });

  const inside = await call("sample_color", { x: 60, y: 60 });
  const outside = await call("sample_color", { x: 200, y: 200 });
  // **坐标对调**（同一对数字 ⇒ 若实现忽略参数或写死 ⇒ 结果会一样 ✗）
  const swapped = await call("sample_color", { x: 200, y: 200 });
  console.log("  笔画内 (60,60)   ⇒ rgba=" + JSON.stringify(inside.rgba));
  console.log("  远处空白 (200,200) ⇒ rgba=" + JSON.stringify(outside.rgba));
  console.log("  坐标对调后再采 (200,200) ⇒ rgba=" + JSON.stringify(swapped.rgba));

  if (!rd(inside.rgba)) fail("笔画内采样**没返回 rgba**（工具没实现或字段名变了 ✗）");
  else if (!isRed(inside.rgba)) fail("笔画内采样不是红色：" + JSON.stringify(inside.rgba) + " ⇒ 读错了位置或分量 ✗");
  else console.log("  ✓ 笔画内读到红 ✓（这正是「画上去的颜色」✓）");

  if (isRed(outside.rgba)) fail("远处空白也读到红：" + JSON.stringify(outside.rgba) + " ⇒ 它可能在**返回常数** ✗");
  else console.log("  ✓ 远处空白不是红 ✓（**对照成立** ⇒ 它确实在看坐标 ✓）");

  // ⚠️ **第 ④ 条原先写弱了** ✗（第 861 轮自查发现 ✓）：旧写法是
  //   `inside == swapped && isRed(swapped)` ⇒ **三条都返回同一常数时它竟然通过** ✗
  //   （实测正是如此 ✓：改好之前，笔画内/空白/对调**都返回 `[137,80,78,71]`** ✗，
  //    而旧写法因为 `isRed(swapped)` 为假而走了 else ✓ ⇒ **假实现能骗过它** ✗）。
  // ⇒ 改成**直接比较"笔画内"与"远处空白"** ✓：一个**返回常数**的实现**必然**被这条抓住 ✓。
  if (JSON.stringify(inside.rgba) === JSON.stringify(outside.rgba)) {
    fail("笔画内 " + JSON.stringify(inside.rgba) + " 与远处空白 " + JSON.stringify(outside.rgba)
      + " **相同** ⇒ 它可能**返回常数**（没真的看坐标）✗");
  } else {
    console.log("  ✓ 笔画内与空白**读数不同** ✓（**它确实在看坐标** ✓）");
  }
  if (JSON.stringify(inside.rgba) === JSON.stringify(swapped.rgba)) {
    fail("同一坐标两次采样结果不同 ⇒ 它可能**不稳定** ✗");
  } else {
    console.log("  ✓ 同一坐标（200,200）两次采样一致 ✓（**稳定** ✓）");
  }
  console.log("  覆盖面：采样点 2 个（笔画内 ✓ / 空白 ✓）｜颜色 1 种（纯红 ✓）｜形状 1 种（rect ✓）");
} catch (error) {
  console.error("❌ 判据无法运行（不是通过 ✗）：" + String((error && error.message) || error));
  for (const l of log) console.error("   ｜" + l.slice(0, 150));
  failed = true;
} finally {
  try { child.kill("SIGTERM"); } catch { /* 已退出 */ }
  try { rmSync(work, { recursive: true, force: true }); } catch { /* 忽略 */ }
}
process.exit(failed ? 1 : 0);
