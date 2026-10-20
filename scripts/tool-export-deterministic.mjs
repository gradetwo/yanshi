#!/usr/bin/env node
// **`export_png(deterministic=true)` 必须让「同一文档两次导出」逐字节相同** ✓
// （外部 §164 请求；第三轮复审的「确定性破口」同族）
//
// **∴ 问题**：外部实测「同一文档连续两次 `export` ⇒ 43,194 像素不同、最大差 25」
//   而他们同时实测 `render_region(raw=true)` 两次的 CAS 哈希**完全相同**
//   ⇒ 所以非确定源**不在渲染**，在**导出路径的后半段**（缩放 or 编码）✓
//
// **∴ 这条判据守什么**（两条断言走**不同**路径）：
//   ① **正向**：`deterministic: true` + 源尺寸 ⇒ 两次导出**逐字节相同**
//   ② **反向**：`deterministic: true` + **缩放** ⇒ 必须**明确报错**
//      （缩放是逐行浮点重采样 ⇒ 最可能的累加顺序敏感点；本模式明确拒绝它）
//
// **∴ 变异**：把 `deterministic` 的强制 `Nearest` 去掉（或允许缩放）⇒ ② 必红 ✓
import { readFileSync, existsSync } from "node:fs";
import { join } from "node:path";

const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-export-deterministic.mjs <base> <doc> <token>");
  process.exit(2);
}
const dir = process.env.YANSHI_EXPORT_DIR;
if (!dir) {
  console.error("  ⊘ 不可判：没设 YANSHI_EXPORT_DIR（判据要靠它找到导出文件）");
  process.exit(3);
}

const call = async (name, args) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
  const reply = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(args),
  });
  return await reply.json();
};

const fails = [];
const check = (ok, msg) => { if (!ok) fails.push(msg); console.log(`  ${ok ? "✓" : "✗"} ${msg}`); };

// ① 正向：同尺寸 + deterministic ⇒ 两次逐字节相同
const a = await call("export_png", { path: "det_a.png", deterministic: true });
const b = await call("export_png", { path: "det_b.png", deterministic: true });
check(a.ok === true, `①a 第一次导出 ⇒ ok=${a.ok}｜${JSON.stringify(a.context?.detail ?? "").slice(0, 70)}`);
check(b.ok === true, `①b 第二次导出 ⇒ ok=${b.ok}｜${JSON.stringify(b.context?.detail ?? "").slice(0, 70)}`);
const pa = join(dir, "det_a.png"), pb = join(dir, "det_b.png");
if (existsSync(pa) && existsSync(pb)) {
  const x = readFileSync(pa), y = readFileSync(pb);
  check(x.equals(y), `①c 两次导出**逐字节相同**（${x.length} B vs ${y.length} B）`);
} else {
  check(false, `①c 找不到导出文件（${pa} / ${pb}）`);
}

// ② 反向：deterministic + 缩放 ⇒ 必须明确报错
const scaled = await call("export_png", { path: "det_scaled.png", deterministic: true, width: 100, height: 100 });
const detail = String(scaled.context?.detail ?? "");
check(scaled.ok === false, `②a deterministic + 缩放必须**拒绝** ⇒ ok=${scaled.ok}`);
check(/deterministic 模式不做缩放/.test(detail), `②b 错误信息必须说「不做缩放」（可诊断）⇒ ${JSON.stringify(detail).slice(0, 90)}`);

if (fails.length) {
  console.log(`\n  ✗ ${fails.length} 处不符（export_png 的确定性模式）`);
  process.exit(1);
}
console.log("\n  ✓ deterministic 模式：两次导出逐字节相同，且明确拒绝缩放");
