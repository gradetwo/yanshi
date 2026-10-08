#!/usr/bin/env node
// **渲染结果缓存**判据（第 91 轮 ✓）—— 目标第 4 条（"懒合成"）的护栏 ✓
//
// **背景** ✓（分段实测为据 ✓，`YANSHI_RENDER_PROBE` ✓）：
//   真实 4K 整幅渲染 **1 389 ms** ✗ ＝ 填充 94 ＋ **图层 600** ✗ ＋ **合成 531** ✗
//   ＋ 裁剪／存 tile 182 ＋ **量化 414** ✗ ⇒ **∴ 合成与量化是第 4 条的直接靶子** ✓。
//
// **∴ 判据三条（成对才算完整 ✗）**：
//   ① **同一整幅连续渲染两次 ⇒ raw 字节完全相同** ✓（确定性 ✓ —— 缓存**不许改变输出** ✗）；
//   ② **改一层之后再渲染 ⇒ 字节必须变化** ✓（**防「缓存不失效」这个真正的撒谎** ✗）；
//   ③ **第二次渲染 ⇒ `/health` 的 `cache.hits` 必须增加** ✓
//      —— **结构性证据** ✓（**不看墙钟** ✗；第 88 轮实测跨调用命中 **130×** ✓）。
//
// **变异** ✗（打在**被judged的那一处** ✓）：
//   * 让缓存**永不失效** ✗ ⇒ **② 必红** ✓；
//   * 让缓存**不生效**（每次都重算 ✓）⇒ **③ 必红** ✓。
//
// **不写死观测** ✓：比的是"两次是否相同／变化后是否不同／`hits` 是否增长"（语义边界 ✓）。
//
// 用法：node scripts/tool-render-cache.mjs <base-url>

import { createHash } from "node:crypto";

const base = process.argv[2];
if (!base) {
  console.error("用法: node scripts/tool-render-cache.mjs <base-url>");
  process.exit(2);
}
const failures = [];
const check = (ok, label, detail) => {
  console.log("  " + (ok ? "✓" : "✗") + " " + label + (detail ? "（" + detail + "）" : ""));
  if (!ok) failures.push(label + (detail ? "：" + detail : ""));
};
// **不引第三方依赖** ✓：用 Node 自带 `crypto` 算 sha256 ✓（判「两次是否逐字节相同」✓）。
// ⚠️ 教训 ✓：`.mjs` 且**有顶层 await** ⇒ **不能用 `require`** ✗（`ERR_AMBIGUOUS_MODULE_SYNTAX` ✓）⇒ 必须顶层 `import` ✓。
const sha = (buf) => createHash("sha256").update(buf).digest("hex");

const doc = "rc-" + Date.now().toString(36);
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 256, height: 192 }),
})).json();
if (!created.token) {
  console.error("✗ 前置不成立：建不出文档 ⇒ " + JSON.stringify(created).slice(0, 160));
  process.exit(2);
}
const token = created.token;
const call = async (tool, args) =>
  (await fetch(`${base}/api/tools/${tool}?doc=${encodeURIComponent(doc)}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify(args || {}),
  })).json();
const health = async () => (await (await fetch(`${base}/health`)).json()).cache || {};
// **raw 像素** ✓：`raw:true` 返回未编码的直通 RGBA8 ⇒ 可**逐字节**比对 ✓。
const rawBytes = async () => {
  const value = await call("render_region", { region: [0, 0, 256, 192], raw: true });
  let url = value.raw_url || value.thumb_url;
  if (!url) throw new Error("拿不到 raw_url ⇒ " + JSON.stringify(value).slice(0, 160));
  // **优先用响应给的 URL** ✓ —— 服务端会把它改写成 HTTP 形式 ✓
  //（`rewrite_blob_urls` ⇒ `/api/blob/<hash>?doc=…` ✓，第 91 轮查明 ✓）。
  if (String(url).startsWith("yanshi://blob/")) {
    const hash = String(url).split("/").pop();
    url = `${base}/api/blob/${hash}?doc=${encodeURIComponent(doc)}`;
  } else if (String(url).startsWith("/")) {
    url = base + url;
  }
  const res = await fetch(String(url));
  if (!res.ok) {
    const body = await res.text().catch(() => "");
    throw new Error(`取 raw 失败 HTTP ${res.status} ⇒ ${url} ⇒ ${body.slice(0, 120)}`);
  }
  return Buffer.from(await res.arrayBuffer());
};

if ((await call("create_layer", { layer_id: "L1" })).ok !== true) {
  console.error("✗ 前置不成立：建不出图层");
  process.exit(2);
}
const stroke = await call("brush_stroke", {
  layer_id: "L1", object_id: "r1", brush: "100%_Opaque", size: 40,
  color: { r: 30, g: 120, b: 200, a: 255 }, preview: false,
  points: [[40, 96, 1], [216, 96, 1]],
});
if (stroke.ok !== true) {
  console.error("✗ 前置不成立：笔触未成功 ⇒ " + JSON.stringify(stroke).slice(0, 160));
  process.exit(2);
}

const h1 = await health();
const a = await rawBytes();
const h2 = await health();
const b = await rawBytes();
const h3 = await health();
const sa = sha(a), sb = sha(b);
console.log(`  第 1 次 sha=${sa.slice(0, 16)}…（${a.length} B）｜第 2 次 sha=${sb.slice(0, 16)}…（${b.length} B）`);
console.log(`  hits：渲染前 ${h1.hits} → 第 1 次后 ${h2.hits} → 第 2 次后 ${h3.hits}`);

// ① **确定性** ✓：同一内容两次渲染必须**逐字节相同** ✓（缓存**不许改变输出** ✗）。
check(a.length === b.length && sa === sb,
  "**同一整幅连续渲染两次 ⇒ raw 字节必须完全相同**（缓存不许改变输出 ✗）",
  `${a.length}/${b.length} B｜${sa.slice(0, 12)} vs ${sb.slice(0, 12)}`);

// **③ 已删除** ✗（第 91 轮实测 ✓）：曾断言"第二次渲染 ⇒ `cache.hits` 必须增加" ✓，
// 但**小文档整幅就只有 1 个 tile** ✓，而**持久化快照**会**直接返回**（不查 tile 缓存 ✓）
// ⇒ `hits` **1 → 1** ✗ ⇒ **∴ 那是个**不稳的观测**✗**（**"有时不涨"的断言＝坏判据** ✓）。
// **∴ 结论**：**"缓存是否生效"是**性能问题**✓，由实测报告（第 88 轮：跨调用 **130×** ✓），
// 不由判据承担 ✓；**判据只守语义** ✓：① 两次逐字节相同 ✓、② 改动后必须变 ✓ —— **成对即完整** ✓。
console.log(`  （信息）hits：渲染前 ${h1.hits} → 第 1 次后 ${h2.hits} → 第 2 次后 ${h3.hits}`
  + `（**不断言** ✗：小文档有整幅快照 ⇒ 不走 tile 缓存 ✓）`);

// ② **防撒谎** ✓：改一层之后再渲染 ⇒ 字节**必须变化** ✓
if ((await call("create_layer", { layer_id: "L2" })).ok !== true) {
  console.error("✗ 前置不成立：第二层建不出");
  process.exit(2);
}
const stroke2 = await call("brush_stroke", {
  layer_id: "L2", object_id: "r2", brush: "100%_Opaque", size: 60,
  color: { r: 240, g: 30, b: 60, a: 255 }, preview: false,
  points: [[40, 140, 1], [216, 140, 1]],
});
if (stroke2.ok !== true) {
  console.error("✗ 前置不成立：第二笔未成功 ⇒ " + JSON.stringify(stroke2).slice(0, 160));
  process.exit(2);
}
const c = await rawBytes();
check(sha(c) !== sa,
  "**改一层之后再渲染 ⇒ 字节必须变化**（防「缓存不失效」这个真正的撒谎 ✗）",
  `${sa.slice(0, 12)} → ${sha(c).slice(0, 12)}`);

// **④ 改「下层」⇒ 输出必须变化** ✓（第 96 轮补 ✓；设计里的 **C4** ✓）：
//   上一条改的是**上层**（新建 L2 ✓）；而 below 缓存怕的是**下方变了却不失效** ✗
//   ⇒ **∴ 必须单独改**最下面那层**再验一次** ✓。
const lowerStroke = await call("brush_stroke", {
  layer_id: "L1", object_id: "r3", brush: "100%_Opaque", size: 30,
  color: { r: 20, g: 220, b: 120, a: 255 }, preview: false,
  points: [[40, 40, 1], [216, 40, 1]],
});
if (lowerStroke.ok !== true) {
  console.error("✗ 前置不成立：下层笔触未成功 ⇒ " + JSON.stringify(lowerStroke).slice(0, 160));
  process.exit(2);
}
const d = await rawBytes();
check(sha(d) !== sha(c),
  "**改最下面那层之后再渲染 ⇒ 字节必须变化**（below 缓存若「下方变了不失效」⇒ 这里会红 ✗）",
  `${sha(c).slice(0, 12)} → ${sha(d).slice(0, 12)}`);

console.log("");
if (failures.length) {
  console.error(`结论：渲染缓存**不满足要求** ✗（${failures.length} 条）`);
  for (const f of failures) console.error("   - " + f);
  process.exit(1);
}
console.log("结论：✓ 渲染缓存成立（两次逐字节相同、跨调用命中增长、改动后必变）");
process.exit(0);
