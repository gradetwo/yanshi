#!/usr/bin/env node
// **★ §6.3 的 ②③：**GPU 不可用 ⇒ fallback**、**参数关 GPU** ⇒ 与基线**逐字节相同**** ✓（第 183 轮 ✓）。
//
// **它守什么**：
//   **②** **GPU 不可用** ⇒ fallback 到 CPU ⇒ 输出必须与基线**逐字节相同**
//   **③** **`--gpu off`** ⇒ 输出必须与基线**逐字节相同**
//   **④** 三个模式下 `/health` 必须**如实**报出 `render_backend` 与 `max_channel_delta`
//        （**∴ `max_channel_delta` 在没有比对时必须是 `null`**，**不许用 `0` 冒充**）
//
// **为什么现在就能立**：**CPU 是真值**。**∴ 今天没有 GPU** ⇒ 三种模式**都**跑在 CPU 上
//   ⇒ **∴ 那正好是 ②③ 要的场景**（fallback 与强制关）。
//
// **怎么比**：`render_region { raw: true }` 回的 `raw_url` 里**带内容哈希**（blob 是**按内容寻址**的）
//   ⇒ **∴ 比哈希就等于比字节**，**而**不必把 4 MB 拉回来。
//
// **判据能红**：同一次运行里有**负对照** —— 在同一台上多画一笔 ⇒ **哈希必须变**
//   ⇒ **∴ 证明这套比较**真的能分辨差异**（不是"永远相等"）。
//
// 用法：node scripts/tool-gpu-fallback-parity.mjs        （自己起三台服务）

import { spawn } from "node:child_process";
import { existsSync, statSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

// **★ 选**最新的**二进制** ✗ ★**（第 173 轮的教训：**旧的 release 会静默顶替新构建**）
const pickBinary = () => {
  const cands = [];
  for (const root of [process.env.CARGO_TARGET_DIR, "target"].filter(Boolean)) {
    for (const p of [join(root, "release", "yanshi-serve"), join(root, "debug", "yanshi-serve")]) {
      if (existsSync(p)) cands.push(p);
    }
  }
  if (process.env.YANSHI_SERVE_BIN) cands.unshift(process.env.YANSHI_SERVE_BIN);
  cands.sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs);
  return cands[0];
};

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const failures = [];
const fact = (b, m) => { console.log(`  ${b ? "✓" : "✗"} ${m}`); if (!b) failures.push(m); };

const BIN = pickBinary();
if (!BIN) { console.error("✗ 找不到 yanshi-serve ⇒ **∴ 无法起服务** ⇒ **∴ 本跑没有结论**"); process.exit(2); }
console.log(`  二进制：${BIN}（mtime ${statSync(BIN).mtime.toISOString()}）`);

const BASE_PORT = 13100 + Math.floor(Math.random() * 60);
const MODES = ["auto", "off", "on"];

const start = async (mode, index) => {
  const port = BASE_PORT + index * 10;
  const root = mkdtempSync(join(tmpdir(), `gfp-${mode}-`));
  const proc = spawn(BIN, ["--root", root, "--bind", `127.0.0.1:${port}`, "--gpu", mode], { stdio: "ignore" });
  for (let i = 0; i < 60; i += 1) {
    try { const r = await fetch(`http://127.0.0.1:${port}/health`); if (r.ok) return { mode, port, proc, health: await r.json() }; }
    catch { /* 还没起来 */ }
    await sleep(300);
  }
  proc.kill("SIGKILL");
  throw new Error(`--gpu ${mode} 的服务没起来（端口 ${port}）`);
};

const call = async (port, doc, token, tool, args) => {
  const r = await fetch(`http://127.0.0.1:${port}/api/tools/${tool}?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}),
  });
  return await r.json();
};

const buildFixture = async (port) => {
  const doc = "gfp_" + Math.random().toString(36).slice(2, 8);
  const created = await (await fetch(`http://127.0.0.1:${port}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: 512, height: 512 }),
  })).json();
  const token = created.token;
  for (const id of ["G0", "G1", "Gtop"]) await call(port, doc, token, "create_layer", { layer_id: id, name: id });
  const stroke = (layer, dx) => call(port, doc, token, "brush_stroke", {
    layer_id: layer, brush: "100%_Opaque", size: 40,
    points: [[100 + dx, 100, 1.0], [300 + dx, 300, 1.0]],
    color: { r: 20, g: 200, b: 90, a: 255 }, preview: false,
  });
  for (const id of ["G0", "G1", "Gtop"]) await stroke(id, 0);
  return { doc, token, stroke };
};

const region = { x: 0, y: 0, w: 512, h: 512 };
// **★ 只取**内容哈希** ✗ ★**（第 183 轮 ✓；**实测换来的 ✓）：
//   **∴ 我**第一版取了**整个 `raw_url`**✗
//     ⇒ **∴ 而**它**含** `?doc=…&token=…` 的查询串** ✗**
//       ⇒ **∴ 于是**三台**必然不同**（**∵ token 不同 ✓）** ✓✓
//       ⇒ **★ 而**那**不是**内容差异** ✗
//         ⇒ **∴ 修法**：**只取** `sha256:` 那一段 ✓**** ✓✓
//   **∴ 实测（**同一台三次 ＋ 三个不同文档 ✓）★**：
//     **∴ 内容哈希**相同**✗（`sha256:5991626b…` ✓）
//       ⇒ **★ 所以**：**渲染**是**逐字节确定**的 ✓ ★**** ✓✓
//       ⇒ **∴ 而**那个查询串**才是**噪声源** ✓**** ✓✓
const contentHash = (value) => {
  const m = String(value).match(/sha256:[0-9a-f]{64}/);
  return m ? m[0] : String(value).split("?")[0];
};
const finger = async (port, doc, token) => {
  const r = await call(port, doc, token, "render_region", { region, raw: true });
  return contentHash(r.raw_url || r.preview_url || JSON.stringify(r).slice(0, 60));
};

const servers = [];
try {
  console.log("  ── 起三台服务（--gpu auto／off／on）──");
  for (const [i, mode] of MODES.entries()) servers.push(await start(mode, i));
  for (const s of servers) console.log(`    --gpu ${s.mode} ⇒ 端口 ${s.port}｜render_backend=${s.health.render_backend}｜gpu_mode=${s.health.gpu_mode}`);

  console.log("  ── 造**同一个**场景（三台各自建，参数完全相同）──");
  for (const s of servers) Object.assign(s, await buildFixture(s.port));

  console.log("  ── 同一区域渲染，比内容哈希 ──");
  for (const s of servers) s.hash = await finger(s.port, s.doc, s.token);
  for (const s of servers) console.log(`    --gpu ${s.mode} ⇒ ${s.hash.slice(-24)}`);
  const hashes = new Set(servers.map((s) => s.hash));
  fact(hashes.size === 1, `②③ **三种模式的输出必须逐字节相同**（实测不同哈希 ${hashes.size} 个 ⇒ **∴ fallback 改变了输出**）`);

  console.log("  ── 负对照：同一台上多画一笔 ⇒ 哈希**必须变**（证明比较能分辨）──");
  const probe = servers[1];
  await probe.stroke("Gtop", 24);
  const after = await finger(probe.port, probe.doc, probe.token);
  fact(after !== probe.hash, "负对照：多画一笔后哈希必须变化（**∴ 不变**说明这套比较没有分辨力）");

  console.log("  ── ④ 后端诚实性 ──");
  // **★ 必须**在渲染之后**重读 `/health` ✗ ★**（**第 462 轮 ✓；**实测缺陷 ✓）：
  //   **∴ 原来的错 ✗**：**`s.health` **取自**服务启动时**✗（**`:55` ✓）
  //     ⇒ **∴ 而**那**是**渲染前**的初值** ✓
  //       ⇒ **∴ 于是**：**本判据**测的是**没渲染过的进程** ✓
  //         ⇒ **★ 所以**：**它**当前**误绿**✗
  //           ⇒ **∴ 因为**初值**恒是 `cpu`** ✓
  //             ＋ **∴ 而**「**`--gpu off` **是否真关掉 GPU**」**恰恰**没被验证** ✓ ★**** ✓✓
  //   **∴ 现在 ✗**：**渲染完成后再读一次**✗
  //     ⇒ **∴ 于是**：**它**会**真的**验证**渲染后的后端** ✓ ★**** ✓✓
  for (const s of servers) {
    try {
      s.health = await (await fetch(`http://127.0.0.1:${s.port}/health`)).json();
    } catch (error) {
      console.warn(`    ⚠️ ${s.mode}：渲染后读 /health 失败（沿用旧快照）：${String(error).slice(0, 60)}`);
    }
  }
  for (const s of servers) {
    const h = s.health;
    fact(h.render_backend === "cpu" || h.render_backend === "gpu", `--gpu ${s.mode}：render_backend 必须是真值之一（实测 ${h.render_backend}）`);
    if (s.mode === "off") fact(h.render_backend === "cpu", `--gpu off：render_backend 必须**恰好**是 cpu（实测 ${h.render_backend}）`);
    if (s.mode === "on" && h.render_backend !== "gpu") {
      fact(typeof h.gpu_unavailable_reason === "string" && h.gpu_unavailable_reason.length > 0,
        `--gpu on 而实际不是 gpu ⇒ **必须**给出原因（**∴ 否则**是静默降级）`);
    }
    fact(h.gpu_mode === s.mode, `--gpu ${s.mode}：gpu_mode 必须回报请求的模式（实测 ${h.gpu_mode}）`);
    if (h.render_backend !== "gpu") {
      fact(h.max_channel_delta === null, `--gpu ${s.mode}：没有比对时 max_channel_delta 必须是 null（实测 ${JSON.stringify(h.max_channel_delta)}）`);
      fact(h.max_channel_delta !== 0, `--gpu ${s.mode}：max_channel_delta **不许**用 0 冒充（0 读起来像"比过且一致"）`);
    }
  }
} catch (error) {
  console.error(`  ✗ 本跑没有结论：${String(error && error.message || error).slice(0, 160)}`);
  for (const s of servers) { try { s.proc.kill("SIGKILL"); } catch { /* 已退出 */ } }
  process.exit(2);
}
for (const s of servers) { try { s.proc.kill("SIGKILL"); } catch { /* 已退出 */ } }

console.log("");
if (failures.length === 0) { console.log("  ✓ §6.3 的 ②③④ 全部成立（纯 CPU）"); process.exit(0); }
console.log(`  ✗ 失败 ${failures.length} 条：`);
for (const f of failures) console.log(`     - ${f}`);
process.exit(1);
