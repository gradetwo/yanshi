#!/usr/bin/env node
// **低沉积涂抹判据** ✓：`smudge` 的 `deposit`（缺省 0 ⇒ 与今天逐字节相同）。
//
// **为什么要有它** ✗（外部实测报告 #7 ✓）：`smudge` 只能"糊"（色不变 ✗），`rub` 能"融"但落新色 ⇒
// 造新硬边 ✗ ⇒ 画家要的"上下笔触柔和衔接"两样都做不到 ✓。
// **∴ 语义（B 方案 ✓）**：`deposit` 补的**不是**用户给的色 ✗（那等于 `rub` ✓），而是**该 stamp 采到的
// 后方色**✓ ⇒ **∴ 不引入任何新色 ⇒ **∴ 只把已有的色压实一点 ✓ ⇒ "既带色、又不造新边" ✓✓**
//
// 三条断言：
//   ① 不给 `deposit` 与给 `deposit: 0` ⇒ 导出**逐字节相同** ✓（**∴ 向后兼容的硬证据 ✓**）
//   ② `deposit: 0.6` ⇒ 导出**必须不同** ✓（**∴ 功能真的生效 ✓**）
//   ③ `deposit: 0.6` 的方向必须"更实/更暗" ✓（`analyze_region` 的平均亮度下降 ✓）
//
// 用法：node scripts/tool-smudge-deposit.mjs <server-base> <doc-id> <token>
import { readFileSync } from "node:fs";
import { createHash } from "node:crypto";

const [base, doc, token, root] = process.argv.slice(2);
if (!base || !doc || !token || !root) {
  console.error("用法: node scripts/tool-smudge-deposit.mjs <base> <doc> <token> <server-root>");
  process.exit(3);
}
const call = async (name, args) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
  const reply = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(args),
  });
  return { status: reply.status, body: await reply.json() };
};
const BAND = [[40, 60], [200, 60], [200, 120], [40, 120]];
const REGION = { x: 20, y: 40, w: 220, h: 100 };
const SMUDGE_PTS = [[60, 90], [100, 90], [140, 90], [180, 90]];

// 一个变体：一层 → 铺斑块 → 涂抹 → 导出该层那片区域 → 求哈希
const variant = async (layer, deposit) => {
  await call("create_layer", { layer_id: layer, name: layer });
  const painted = await call("brush_stroke", {
    layer_id: layer, brush: "100%_Opaque", color: { r: 30, g: 30, b: 40, a: 255 },
    points: BAND.map(([x, y]) => [x, y, 1]), preview: false,
  });
  if (painted.body?.ok !== true) {
    console.log(`  ✗ ${layer}: brush_stroke 失败 ${JSON.stringify(painted.body).slice(0, 150)}`);
    process.exit(1);
  }
  const args = { layer_id: layer, points: SMUDGE_PTS, size: 40, smudge_length: 14, hardness: 0.4, opacity: 1 };
  if (deposit !== null) args.deposit = deposit;
  const smudged = await call("smudge", args);
  if (smudged.body?.ok !== true) {
    console.log(`  ✗ ${layer}: smudge 失败 ${JSON.stringify(smudged.body).slice(0, 200)}`);
    process.exit(1);
  }
  // **`export_png` 必须给落盘路径** ✗，且路径在服务端 root 内 ✓ ⇒ 用相对路径 + 本地拼 root 读回来 ✓
  const rel = `${layer}.png`;
  const png = await call("export_png", { layer_id: layer, region: REGION, path: rel });
  const path = png.body?.path ?? png.body?.file ?? `${root}/${rel}`;
  if (!path) {
    console.log(`  ✗ ${layer}: export_png 没给路径 ${JSON.stringify(png.body).slice(0, 150)}`);
    process.exit(3);
  }
  const bytes = readFileSync(path);
  const bright = await call("analyze_region", { region: REGION });
  return { hash: createHash("sha256").update(bytes).digest("hex"), size: bytes.length, brightness: bright.body?.avg_brightness };
};

const omitted = await variant("dep_omitted", null);
const zero = await variant("dep_zero", 0);
const some = await variant("dep_some", 0.6);
let bad = 0;
const check = (ok, why) => { console.log(`  ${ok ? "✓" : "✗"} ${why}`); if (!ok) bad += 1; };

check(omitted.hash === zero.hash, `不给 deposit 与给 0 逐字节相同（${omitted.hash.slice(0, 12)} vs ${zero.hash.slice(0, 12)}）`);
check(omitted.hash !== some.hash, `deposit=0.6 改变了输出（${omitted.hash.slice(0, 12)} vs ${some.hash.slice(0, 12)}）`);
const darker = (omitted.brightness ?? 1) - (some.brightness ?? 1);
check(darker > 0.002, `deposit=0.6 让那片更实（亮度 ${omitted.brightness} → ${some.brightness}，差 ${darker.toFixed(4)}）`);
if (bad) { console.log(`  ✗ 低沉积涂抹有 ${bad} 处不满足`); process.exit(1); }
console.log("  ✓ 低沉积涂抹：缺省路径逐字节不变、deposit>0 生效且方向正确");
