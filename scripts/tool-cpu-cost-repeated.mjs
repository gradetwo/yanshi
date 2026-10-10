#!/usr/bin/env node
// **★ 重复测量的 CPU 成本** ✗ ★**（第 212 轮 ✓；**∴ 先立测量方法，再判优化 ✓）。
//
// **∴ 为什么要它 ✗**：**实测**（**第 211 轮 ✓）证明**✗**：
//   **∴ 同一份代码**两次测得 **14721.4 ms** 与 **13309.4 ms**
//     ⇒ **∴ 差 **9.6%**** ✓
//   **∴ 而**我**当时**用一个**单次测量**（**15819.6 vs 14721.4 ＝ +7.5%**）
//     ⇒ **∴ 判定**一个改动**更慢**✗ ⇒ **∴ 于是**回退** ✓
//       ⇒ **★ 而**那个判定**不成立** ✓ ★**** ✓✓
//   **⇒ ∴ 所以**：**任何小于噪声的差异**✗ ⇒ **∴ 必须**重复测量** ✓**** ✓✓
//
// **★ 口径 ✗ ★**：**每轮都是**冷帧**✗
//   ⇒ **∴ 做法**：**每轮**删掉 `<root>/docs/<doc>/render.{png,seq}`**✗
//     ⇒ **∴ 并**重启服务** ✓（**∴ 因为**服务会**把渲染结果写回 root ✓）** ✓✓
//   **∴ 为什么不用暖帧 ✗**：**∴ 冷首帧**才是**用户可感知的**那个数** ✓**** ✓✓
//
// **∴ 输出 ✗**：**每项**给**中位数 ＋ 最小／最大**✗
//   ⇒ **∴ 于是**：**读者**能看出**噪声区间** ✓
//     ⇒ **∴ 而**「**差异 < 区间**」时**不许**宣称收益** ✓**** ✓✓
//
// 用法：node scripts/tool-cpu-cost-repeated.mjs [--rounds N] [--doc <id>]

import { spawn, spawnSync } from "node:child_process";
import { existsSync, statSync, readFileSync, rmSync } from "node:fs";
import { join } from "node:path";

const arg = (name, dflt) => {
  const i = process.argv.indexOf(name);
  return i >= 0 && process.argv[i + 1] ? process.argv[i + 1] : dflt;
};
const ROUNDS = Number(arg("--rounds", 3));
const DOC = arg("--doc", "art_60301188_4k_monet_sunrise");
const ROOT = arg("--root", "/home/crow/yanshi-tmp/real-cold");

const pickBinary = () => {
  const cands = [];
  for (const root of [process.env.CARGO_TARGET_DIR, "target"].filter(Boolean)) {
    for (const kind of ["release", "debug"]) {
      const p = join(root, kind, "yanshi-serve");
      if (existsSync(p)) cands.push(p);
    }
  }
  cands.sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs);
  return cands[0];
};
const BIN = pickBinary();
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const median = (xs) => {
  const s = [...xs].sort((a, b) => a - b);
  return s.length === 0 ? 0 : s[Math.floor(s.length / 2)];
};

/** **∴ 解析一行 PROBE ✗**（**阶段毫秒 ＋ 文件大小 ✓）。 */
const parseProbe = (line) => {
  const grab = (label) => {
    const m = line.match(new RegExp(`${label}=([0-9.]+)(ms|µs|s)`));
    if (!m) return null;
    const v = Number(m[1]);
    return m[2] === "s" ? v * 1000 : m[2] === "µs" ? v / 1000 : v;
  };
  return {
    fill: grab("填充"), layers: grab("图层"), composite: grab("合成"),
    crop: grab("裁剪\\+存tile"), quantize: grab("量化"),
  };
};

const runs = [];
for (let round = 1; round <= ROUNDS; round += 1) {
  // **∴ 清缓存 ⇒ 保证是冷帧 ✓**
  for (const name of ["render.png", "render.seq"]) {
    const p = join(ROOT, "docs", DOC, name);
    try { rmSync(p, { force: true }); } catch { /* 本来就不在 */ }
  }
  const port = 17300 + Math.floor(Math.random() * 200);
  const proc = spawn(BIN, ["--root", ROOT, "--bind", `127.0.0.1:${port}`], {
    stdio: ["ignore", "ignore", "pipe"],
    env: { ...process.env, YANSHI_RENDER_PROBE: "1" },
  });
  let stderr = "";
  proc.stderr.on("data", (b) => { stderr += b.toString(); });
  const base = `http://127.0.0.1:${port}`;
  try {
    let up = false;
    for (let i = 0; i < 80; i += 1) {
      try { if ((await fetch(`${base}/health`)).ok) { up = true; break; } } catch { /* 未起 */ }
      await sleep(300);
    }
    if (!up) throw new Error(`服务没起来（${port}）`);
    const token = (await (await fetch(`${base}/api/documents`, {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ doc_id: DOC }),
    })).json()).token;
    const meta = JSON.parse(readFileSync(join(ROOT, "docs", DOC, "meta.json"), "utf8"));
    const t0 = process.hrtime.bigint();
    const out = await (await fetch(`${base}/api/tools/render_region?doc=${DOC}&token=${token}`, {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ region: { x: 0, y: 0, w: meta.width, h: meta.height } }),
    })).json();
    const wall = Number(process.hrtime.bigint() - t0) / 1e6;
    // **∴ 聚合**全部 chunk** 的 `primitive:Shape` 与其它标签 ✓（**∴ 不能只取一行 ✓）**
    const shapeMs = [...stderr.matchAll(/primitive:Shape=([0-9.]+)(ms|µs|s)×([0-9]+)/g)]
      .reduce((acc, m) => {
        const v = Number(m[1]);
        const conv = m[2] === "s" ? v * 1000 : m[2] === "µs" ? v / 1000 : v;
        return acc + conv * Number(m[3]);
      }, 0);
    const probeLine = [...stderr.split("\n")].reverse().find((l) => l.includes("PROBE 区域")) || "";
    runs.push({ wall, shapeMs, ok: out.ok === true, ...parseProbe(probeLine) });
    console.log(`  第 ${round} 轮：墙钟 ${wall.toFixed(0)} ms｜图层 ${(parseProbe(probeLine).layers || 0).toFixed(0)} ms｜Shape ${shapeMs.toFixed(0)} ms｜ok=${out.ok === true}`);
  } catch (error) {
    console.error(`  第 ${round} 轮 ✗ ${String(error && error.message || error).slice(0, 120)}`);
    runs.push({ wall: null, shapeMs: null, ok: false });
  } finally {
    proc.kill("SIGKILL");
    await sleep(300);
  }
}

const good = runs.filter((r) => r.ok && r.wall !== null);
if (good.length === 0) { console.error("  ✗ 没有可用的一轮 ⇒ **∴ 本跑没有结论**"); process.exit(2); }
const col = (key) => good.map((r) => r[key]).filter((v) => typeof v === "number");
const report = (label, key) => {
  const xs = col(key);
  if (xs.length === 0) return;
  const lo = Math.min(...xs);
  const hi = Math.max(...xs);
  const med = median(xs);
  const spread = med > 0 ? ((hi - lo) / med) * 100 : 0;
  console.log(`    ${label.padEnd(22)} 中位 ${med.toFixed(0).padStart(6)} ms｜${lo.toFixed(0)}–${hi.toFixed(0)}｜极差 ${spread.toFixed(1)}%`);
};
console.log("");
console.log(`  ★ 重复测量（${good.length}/${ROUNDS} 轮冷帧）★`);
report("墙钟（cold first frame）", "wall");
report("图层", "layers");
report("Shape 合计（CPU）", "shapeMs");
report("量化", "quantize");
report("合成", "composite");
report("裁剪+存tile", "crop");
report("填充", "fill");
console.log("");
console.log("  ★ **∴ 判定规则 ✗**：**差异 < 极差** ⇒ **∴ 不许**宣称收益 ✓ ★");
