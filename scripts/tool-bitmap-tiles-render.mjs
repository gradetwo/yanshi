#!/usr/bin/env node
// **★ 带 `tiles` 索引的位图**必须真的画到画布上** ★**（2026-10-15 ✓）
//
// **∴ 为什么写它 ✗**：追"《戴珍珠耳环的少女》虹膜画不出来"追了十几轮 ✓，最后定案在：
//   **渲染器先试分块索引路（`tiles`）✓，`stitched` 成功返回却是一张空/全零缓冲 ✗
//   ⇒ `blit; continue;` ⇒ 永远走不到下面那条"整幅回退" ✗** ⇒ 画面**一处都没变** ✓
//   （机理与证据：`docs/design/vermeer-acceptance.md` §18 ✓）。
// **∴ 而这正是"静默无效果"的样板 ✗** —— 本仓库最恨的一类 ✓：
//   调用方拿到 `ok=true` ✓、对象数据完整 ✓、`warnings` 为空 ✓，**而画布纹丝不动** ✗。
//
// **★ 判什么（两条，都能红 ✓）★**：
//   ① **正对照** ✓：普通 `draw_stroke` 必须让画布**非 0 像素改变** ✓
//      ⇒ 它绿说明**这套测量手法本身是好的** ✓（否则下面的红可能是判据自己坏了 ✗）；
//   ② **红的那一格** ✓：`medium_stroke` 落盘的对象**带 `tiles` 索引** ✓
//      ⇒ 它**同样必须**让画布**非 0 像素改变** ✓
//      ⇒ **修好之前这一条会红** ✗（实测：0 像素 ✓）★
//
// **∴ 变异（能红的根据 ✓）✗**：把渲染器里 `if let Some(rgba) = stitched { … continue; }`
//   那段**改成"拼出来全透明就落回整幅路"** ⇒ ② 变绿 ✓（这就是修法 ✓）。
//
// 用法：node scripts/tool-bitmap-tiles-render.mjs <base-url>
const base = (process.argv[2] || "").replace(/\/+$/, "");
if (!base) {
  console.error("用法：node scripts/tool-bitmap-tiles-render.mjs <base-url>");
  process.exit(2);
}

async function call(path, body, doc, token) {
  const url = `${base}${path}?doc=${doc}&token=${token}`;
  const res = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body ?? {}),
  });
  const text = await res.text();
  let json;
  try {
    json = JSON.parse(text);
  } catch {
    json = { ok: false, raw: text.slice(0, 200) };
  }
  return json;
}

async function rawRegion(doc, token, region) {
  const r = await call("/api/tools/render_region", { region, raw: true }, doc, token);
  const url = r.raw_url || r.rawUrl;
  if (!url) return null;
  const res = await fetch(url.startsWith("http") ? url : base + url);
  return Buffer.from(await res.arrayBuffer());
}

/** 量"撤销最近一笔"前后的像素差 ✓ —— 这是本轮一直在用的手法 ✓（比只看一个采样点可靠 ✓）。 */
async function changedPixels(doc, token, region) {
  const before = await rawRegion(doc, token, region);
  const undo = await call("/api/tools/undo_last", { count: 1 }, doc, token);
  if (undo.ok !== true) return { error: `undo 失败：${JSON.stringify(undo).slice(0, 160)}` };
  const after = await rawRegion(doc, token, region);
  if (!before || !after || before.length !== after.length) {
    return { error: `取不到像素（${before?.length} vs ${after?.length}）` };
  }
  let changed = 0;
  for (let i = 0; i < before.length; i += 4) {
    if (before[i] !== after[i] || before[i + 1] !== after[i + 1] || before[i + 2] !== after[i + 2]) {
      changed += 1;
    }
  }
  return { changed };
}

function ellipse(cx, cy, rx, ry, rings, per) {
  const pts = [];
  for (let ring = 0; ring < rings; ring += 1) {
    const r = 1 - ring / rings;
    const start = ring % 2 === 0 ? Math.PI * 0.5 : Math.PI * 1.5;
    const dir = ring % 2 === 0 ? 1 : -1;
    for (let s = 0; s <= per; s += 1) {
      const t = start + (dir * 2 * Math.PI * s) / per;
      pts.push([Number((cx + Math.cos(t) * rx * r).toFixed(2)), Number((cy + Math.sin(t) * ry * r).toFixed(2))]);
    }
  }
  return pts;
}

const S = 760;
const doc = "tiles_render";
const created = await call("/api/documents", { doc_id: doc, width: S, height: S });
const token = created.token;
if (!token) {
  console.error(`❌ 建文档失败：${JSON.stringify(created).slice(0, 200)}`);
  process.exit(3);
}
await call("/api/tools/create_layer", { layer_id: "L1", name: "L1" }, doc, token);

const whole = { x: 0, y: 0, w: S, h: S };
const eye = { cx: S * 0.38, cy: S * 0.42, rx: S * 0.15, ry: S * 0.075 };
const iris = { cx: S * 0.365, cy: S * 0.428, r: S * 0.055 };
const fails = [];

// ── ① 正对照：draw_stroke 必须改动画布 ✓
const sclera = await call(
  "/api/tools/draw_stroke",
  {
    layer_id: "L1",
    object_id: "sclera",
    data: {
      points: ellipse(eye.cx, eye.cy, eye.rx * 0.94, eye.ry * 0.98, 3, 24),
      size: 38,
      hardness: 1.0,
      opacity: 1.0,
      color: { r: 216, g: 208, b: 196, a: 255 },
    },
  },
  doc,
  token,
);
if (sclera.ok !== true) {
  fails.push(`① 眼白那一笔没成功：${JSON.stringify(sclera).slice(0, 160)}`);
} else {
  const m = await changedPixels(doc, token, whole);
  if (m.error) fails.push(`① 量不出来：${m.error}`);
  else if (m.changed <= 0) fails.push("① 正对照失败：`draw_stroke` 竟然没改动画布 ⇒ 判据自身可疑 ✗");
  else console.log(`  ✓ ① 正对照：draw_stroke 改了 ${m.changed} 个像素`);
  // 撤销那次 undo 已经把这笔撤掉了 ⇒ 重新画回来 ✓
  await call(
    "/api/tools/draw_stroke",
    {
      layer_id: "L1",
      object_id: "sclera",
      data: {
        points: ellipse(eye.cx, eye.cy, eye.rx * 0.94, eye.ry * 0.98, 3, 24),
        size: 38,
        hardness: 1.0,
        opacity: 1.0,
        color: { r: 216, g: 208, b: 196, a: 255 },
      },
    },
    doc,
    token,
  );
}

// ── ② 红的那一格：medium_stroke（落盘带 tiles 索引 ✓）也必须改动画布 ✓
const stri = await call(
  "/api/tools/medium_stroke",
  {
    layer_id: "L1",
    object_id: "iris",
    medium: "oil",
    size: 26,
    load: 0.7,
    wetness: 0.25,
    texture: 0.45,
    color: { r: 58, g: 44, b: 30, a: 255 },
    color_to: { r: 150, g: 116, b: 68, a: 255 },
    color_mixing: "pigment",
    points: ellipse(iris.cx, iris.cy, iris.r, iris.r * 1.12, 6, 26),
  },
  doc,
  token,
);
if (stri.ok !== true) {
  fails.push(`② 虹膜那一笔没成功：${JSON.stringify(stri).slice(0, 200)}`);
} else {
  const obj = await call("/api/tools/get_object", { object_id: "iris" }, doc, token);
  const hasTiles = Boolean(obj?.data?.tiles);
  const m = await changedPixels(doc, token, whole);
  if (m.error) fails.push(`② 量不出来：${m.error}`);
  else if (m.changed <= 0) {
    fails.push(
      `② **带 tiles 索引的位图没画到画布上**（对象 iris 有 tiles=${hasTiles}，` +
        `但撤销前后 0 像素改变 ✗）⇒ 分块索引路拼出空缓冲却没落回整幅路 ✗`,
    );
  } else {
    console.log(`  ✓ ② 带索引的 medium_stroke 改了 ${m.changed} 个像素`);
  }
}

if (fails.length) {
  console.error("❌ 没通过：");
  for (const f of fails) console.error(`   ${f}`);
  process.exit(1);
}
console.log("✓ 带 tiles 索引的位图确实画到了画布上（两条都过 ✓）");
