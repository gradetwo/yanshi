#!/usr/bin/env node
// **位图缓存必须真的命中**判据（第 26 轮 ✓）
//
// **为什么需要它** ✗：`/health` 此前**只报 `misses`** ✗ ⇒ **命中率算不出来** ✓
// ⇒ 一次"**查表全变未命中**"的退化（缓存等于没做 ✗）会**无人察觉** ✓。
// 第 25 轮把底层**本来就有**的 `hits` 暴露出来 ✓（`render.rs:155` ✓、命中时 `hits += 1` ✓），
// 本判据就是给这个读数**立护栏** ✓。
//
// **判据（三条，各自能红 ✓；都打在"计数语义"上，不看耗时 ✗）**：
//   ① **冷启动那一次必须产生 `misses`** ✓ —— 证明"缓存确实被查过" ✓
//      （否则"根本没接缓存"✗ 也能让 ② 通过 ✓）。
//   ② **随后重复渲染同一份内容，`hits` 必须增长** ✓ —— 证明**命中了** ✓。
//   ③ **`tiles` 必须 > 0** ✓ —— 证明缓存里**真的有东西** ✓（防"计数涨了但没存"✗）。
//
// **变异**（打在**被判的那一处** ✗）：把缓存的查表去掉（每次都当未命中 ✓）
//   ⇒ ② 的 `hits` 不再增长 ⇒ **判据红** ✓。
//
// **不写死观测** ✓：只比"涨没涨"（`>` ✓）与"有没有"（`> 0` ✓），不看具体次数、不看耗时 ✓。
//
// 用法：node scripts/tool-cache-hit.mjs <base-url>

const base = process.argv[2];
if (!base) {
  console.error("用法: node scripts/tool-cache-hit.mjs <base-url>");
  process.exit(2);
}
const failures = [];
const check = (ok, label, detail) => {
  console.log("  " + (ok ? "✓" : "✗") + " " + label + (detail ? "（" + detail + "）" : ""));
  if (!ok) failures.push(label + (detail ? "：" + detail : ""));
};
const cache = async () => {
  const r = await (await fetch(`${base}/health`)).json();
  const c = r.cache || {};
  return { hits: Number(c.hits || 0), misses: Number(c.misses || 0), tiles: Number(c.tiles || 0) };
};

// 前置：`hits` 字段必须存在 ✓（否则本判据无意义 ✓）
const probe = await cache();
if (probe.hits === undefined || probe.hits === null || Number.isNaN(probe.hits)) {
  console.error("✗ 前置不成立：/health 的 cache 里没有 hits 字段 ⇒ 无法判命中率");
  process.exit(2);
}

const doc = "cachehit-" + Date.now().toString(36);
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 1024, height: 768 }),
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

const layer = await call("create_layer", { layer_id: "L1" });
if (layer.ok !== true) {
  console.error("✗ 前置不成立：建不出图层 ⇒ " + JSON.stringify(layer).slice(0, 160));
  process.exit(2);
}

// 用**带位图的笔刷**（`impressionism.myb` ✓）—— 实测它每次渲染都会走位图缓存 ✓。
const stroke = (id, y) => ({
  layer_id: "L1", object_id: id, brush: "impressionism.myb", size: 60,
  color: { r: 9, g: 40, b: 90, a: 255 }, points: [[150, y, 1], [600, y + 120, 1]],
});

const before = await cache();
const first = await call("brush_stroke", stroke("a", 150));
check(first.ok === true, "前置：第一笔必须成功", "ok=" + first.ok);
const afterFirst = await cache();
console.log(`  第一笔：hits ${before.hits}→${afterFirst.hits}｜misses ${before.misses}→${afterFirst.misses}｜tiles=${afterFirst.tiles}`);

// ① 冷启动那一次必须产生未命中（证明缓存确实被查过 ✓）
check(afterFirst.misses > before.misses,
  "第一次渲染必须产生 `misses`（证明缓存确实**被查过**）",
  `misses ${before.misses}→${afterFirst.misses}`);
check(afterFirst.tiles > 0, "缓存里必须**真的有东西**（tiles > 0）", "tiles=" + afterFirst.tiles);

// ② 重复渲染同样内容 ⇒ hits 必须增长
const second = await call("brush_stroke", stroke("b", 150));   // 同一 y ⇒ 同样的瓦片 ✓
check(second.ok === true, "前置：第二笔必须成功", "ok=" + second.ok);
const afterSecond = await cache();
console.log(`  第二笔：hits ${afterFirst.hits}→${afterSecond.hits}｜misses ${afterFirst.misses}→${afterSecond.misses}`);
check(afterSecond.hits > afterFirst.hits,
  "**重复渲染同一份内容必须命中缓存**（hits 必须增长）",
  `hits ${afterFirst.hits}→${afterSecond.hits}`);

console.log("");
if (failures.length) {
  console.error(`结论：位图缓存未命中/不可观测 ✗（${failures.length} 条）`);
  for (const f of failures) console.error("   - " + f);
  process.exit(1);
}
console.log("结论：✓ 缓存被查过、且重复渲染会命中（计数语义判据，不看耗时）");
process.exit(0);
