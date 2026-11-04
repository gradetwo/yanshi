#!/usr/bin/env node
// **PWA 只读三方对齐路由的行为级判据**（line-parity）：
//   `stroke_heatmap`／`list_fonts`／`memory_status` 三条本地路由。
//
// **为什么是行为级**：静态判据只能证明文件里写了分支（`grep`），证明不了真的能用。
// 本判据在 node 里打桩 `indexedDB`／`window`／`document.fonts`／`navigator`，
// 真的执行 `makeLocalApi`，用真实原子＋blob 调这三条路由，断言回执的**形状与恒等式**。
//
// **不测什么**：不测服务端同名工具（那由 Rust 测试与 curl 覆盖）；不调内核（这三条都不需要）。
//
// **断言里的恒等式**（内核 ②：单点断言只对一次输入成立）：
//   ① `total_ink_mass === Σ cells[].ink_mass`
//   ② `Σ cell.strokes === 每一格被几笔覆盖`（≥ 覆盖最多那格，≤ total_strokes × 格数）
//   ③ `memory_status.total_bytes === Σ documents[].total_bytes`
//   ④ `doc.total_bytes === journal_bytes + retained_bytes`
//   ⑤ `headroom_bytes === max(0, hard_bytes − total_bytes)`
//
// 退出码：0 = 全绿｜1 = 断言红｜3 = 判据前提不成立（自身出错／环境缺失，不是产品问题）
process.on("uncaughtException", (error) => {
  console.error(`  ⊘ 判据自身出错（不是产品问题）：${String(error).slice(0, 200)}`);
  process.exit(3);
});
import { pathToFileURL } from "node:url";

const { makeIndexedDB } = await import(
  pathToFileURL(process.cwd() + "/scripts/lib/pwa-node-stub.mjs").href
);

const stub = (name, value) =>
  Object.defineProperty(globalThis, name, { value, writable: true, configurable: true });
stub("indexedDB", makeIndexedDB());
stub("location", {
  origin: "https://yanshi-online.wangda.today",
  href: "https://yanshi-online.wangda.today/",
});
// 字体表：`document.fonts` 用最小 forEach 形态（@font-face 已注册的预置字体）。
stub("document", {
  fonts: {
    forEach(fn) {
      fn({ family: '"Inter"' });
      fn({ family: "JetBrains Mono" });
    },
  },
});
// Local Font Access：两次同名 ⇒ 必须去重；Family 前后空白必须 trim。
stub("navigator", {
  gpu: undefined,
  queryLocalFonts: async () => [
    { family: "Foo Sans" },
    { family: "Inter" },
    { family: "  Bar Serif  " },
  ],
});
stub("window", {
  fetch: () => Promise.resolve(new Response("{}", { status: 200 })),
});

const base = pathToFileURL(`${process.cwd()}/web/`).href;
const { makeLocalApi } = await import(base + "api-local.js");
const { open, putAtom, putBlob } = await import(base + "store.js");

const db = await open();
const local = await makeLocalApi(db);
const failures = [];
const check = (ok, label, detail) => {
  console.log(`  ${ok ? "✓" : "✗"} ${label}${detail ? `（${detail}）` : ""}`);
  if (!ok) failures.push(label + (detail ? `：${detail}` : ""));
};
const call = (path, init = {}) =>
  local(new Request(`https://x${path}`, { method: init.method ?? "GET", ...init }));
const post = (path, body) =>
  call(path, { method: "POST", body: JSON.stringify(body) });

// ---- 建文档（走真实路由，不手写 meta）----
const DOC = "d-parity-readonly";
const created = await (await post(`/api/documents?doc=${DOC}`, {
  doc_id: DOC, width: 200, height: 200,
})).json();
check(created.ok === true, "前置：创建文档", JSON.stringify(created).slice(0, 120));

// ---- 造一笔带 tile 的 brush 笔（与本地 brush_stroke 写出的原子同形）----
function makeTile(w, h, alphas) {
  const bytes = new Array(w * h * 4).fill(0);
  for (const [x, y, a] of alphas) {
    const i = (y * w + x) * 4;
    bytes[i] = 255; bytes[i + 1] = 0; bytes[i + 2] = 0; bytes[i + 3] = a;
  }
  return bytes;
}
const tile = makeTile(20, 20, [[0, 0, 7], [5, 5, 13]]); // α 和 = 20
const hash = "sha256:parity-heatmap";
await putBlob(db, hash, tile);
const stroke = (id, points, region, extra = {}) => ({
  actor: "human:web",
  id,
  kind: "import_image",
  payload: {
    type: "raster_patch",
    bitmap: { blob_hash: hash, mime_type: "image/x-yanshi-raw", size: tile.length },
    width: region.w, height: region.h, layer_id: "L0", object_id: "obj_" + id,
    region,
    source: { kind: "brush", brush: "b.myb", points, size: 10 },
    ...extra,
  },
  schema_version: 1,
  session: "session:web",
  timestamp: 1700000000000,
});
// A：区域内的活笔
await putAtom(db, DOC, stroke("a-live", [[12, 12], [15, 15], [25, 25]],
  { x: 10, y: 10, w: 20, h: 20 }));
// B：区域外（不应计入）
await putAtom(db, DOC, stroke("b-outside", [[180, 180]],
  { x: 170, y: 170, w: 10, h: 10 }));
// C：区域内但已撤销（不应计入）
await putAtom(db, DOC, stroke("c-reverted", [[80, 80]],
  { x: 70, y: 70, w: 20, h: 20 }));
await putAtom(db, DOC, {
  actor: "human:web", id: "revert-c", kind: "revert",
  payload: { target: "c-reverted" }, schema_version: 1,
  session: "session:web", timestamp: 1700000000001,
});

// ---- ① stroke_heatmap：形状 + 恒等式 ----
const heatRes = await post(`/api/tools/stroke_heatmap?doc=${DOC}`, {
  region: { x: 0, y: 0, w: 100, h: 100 }, cell_size: 10,
});
const heat = await heatRes.json();
check(heatRes.status === 200 && heat.ok === true, "stroke_heatmap 成功",
  `${heatRes.status} ${JSON.stringify(heat).slice(0, 160)}`);
check(heat.cols === 10 && heat.rows === 10 && heat.cells.length === 100,
  "stroke_heatmap 网格 10×10=100 格", `cols=${heat.cols} rows=${heat.rows} cells=${heat.cells?.length}`);
check(heat.total_strokes === 1, "stroke_heatmap 只数区域内的活笔（撤销与区域外排除）",
  `total_strokes=${heat.total_strokes}`);
check(heat.total_points === 3, "stroke_heatmap 点数 = 区域内点", `total_points=${heat.total_points}`);
check(heat.total_ink_mass === 20, "stroke_heatmap 墨量 = tile 逐像素 alpha 和",
  `total_ink_mass=${heat.total_ink_mass}`);
const inkSum = (heat.cells || []).reduce((s, c) => s + c.ink_mass, 0);
check(inkSum === heat.total_ink_mass, "恒等式：total_ink_mass = Σ cells[].ink_mass",
  `Σ=${inkSum} total=${heat.total_ink_mass}`);
const strokeSum = (heat.cells || []).reduce((s, c) => s + c.strokes, 0);
check(strokeSum >= 1 && strokeSum <= heat.total_strokes * heat.cells.length,
  "恒等式：Σ cell.strokes 在 [1, total_strokes × 格数]", `Σ=${strokeSum}`);
check(heat.strokes_without_ink === 0, "有 tile 的笔不计入 strokes_without_ink",
  `strokes_without_ink=${heat.strokes_without_ink}`);
check(heat.server === false, "stroke_heatmap 标 server:false");

// 数组形态的 region 也必须认（服务端 parse_bbox 的第二种形态）。
const heatArray = await (await post(`/api/tools/stroke_heatmap?doc=${DOC}`, {
  region: [0, 0, 100, 100], cell_size: 10,
})).json();
check(heatArray.ok === true && heatArray.total_ink_mass === 20,
  "stroke_heatmap 接受 [x,y,w,h] 形态", JSON.stringify(heatArray).slice(0, 100));

// ---- ② 参数错误必须 400 invalid_argument，不是静默 ----
const badRegion = await post(`/api/tools/stroke_heatmap?doc=${DOC}`, { cell_size: 10 });
check(badRegion.status === 400 && (await badRegion.json()).error === "invalid_argument",
  "缺 region ⇒ 400 invalid_argument");
const emptyRegion = await post(`/api/tools/stroke_heatmap?doc=${DOC}`, {
  region: { x: 0, y: 0, w: 0, h: 10 },
});
check(emptyRegion.status === 400 && (await emptyRegion.json()).error === "invalid_argument",
  "空 region ⇒ 400 invalid_argument");
const badCell = await post(`/api/tools/stroke_heatmap?doc=${DOC}`, {
  region: { x: 0, y: 0, w: 10, h: 10 }, cell_size: 0,
});
check(badCell.status === 400 && (await badCell.json()).error === "invalid_argument",
  "cell_size=0 ⇒ 400 invalid_argument");
const hugeGrid = await post(`/api/tools/stroke_heatmap?doc=${DOC}`, {
  region: { x: 0, y: 0, w: 10000, h: 10000 }, cell_size: 1,
});
check(hugeGrid.status === 400 && (await hugeGrid.json()).error === "invalid_argument",
  "网格 > 10000 格 ⇒ 400 invalid_argument");

// ---- ③ list_fonts：去重／trim／排序／形状 ----
const fontsRes = await call(`/api/tools/list_fonts?doc=${DOC}`);
const fonts = await fontsRes.json();
const names = (fonts.fonts || []).map((f) => f.family);
check(fontsRes.status === 200 && fonts.ok === true && fonts.count === names.length,
  "list_fonts 成功且 count = fonts.length", `${fontsRes.status} count=${fonts.count}`);
check(JSON.stringify(names) === JSON.stringify(["Bar Serif", "Foo Sans", "Inter", "JetBrains Mono"]),
  "list_fonts 去重 + trim + 族名升序", JSON.stringify(names));
check(names.indexOf("Inter") >= 0 && names.filter((n) => n === "Inter").length === 1,
  "list_fonts document.fonts 与 queryLocalFonts 同名只留一条");
check((fonts.fonts || []).every((f) => Object.keys(f).join(",") === "family"),
  "list_fonts 每项形状只有 family（与服务端一致）");

// queryLocalFonts 被拒时：仍列 document.fonts，并如实留错误
const savedQuery = globalThis.navigator.queryLocalFonts;
globalThis.navigator.queryLocalFonts = async () => { throw new Error("NotAllowedError"); };
const denied = await (await call(`/api/tools/list_fonts?doc=${DOC}`)).json();
check(denied.ok === true && denied.local_fonts_error !== null
  && denied.fonts.some((f) => f.family === "Inter"),
  "queryLocalFonts 被拒 ⇒ 如实记 local_fonts_error，不编造系统字体",
  JSON.stringify(denied).slice(0, 160));
globalThis.navigator.queryLocalFonts = savedQuery;

// ---- ④ memory_status：形状 + 恒等式 ----
const memRes = await call(`/api/tools/memory_status?doc=${DOC}`);
const mem = await memRes.json();
const status = mem.memory_status || {};
check(memRes.status === 200 && mem.ok === true && status.level === "ok",
  "memory_status 成功且 level=ok", `${memRes.status} ${JSON.stringify(status).slice(0, 120)}`);
check(status.process_rss_bytes === null, "memory_status 浏览器 process_rss_bytes 如实为 null");
check(Array.isArray(status.documents) && status.documents.some((d) => d.doc_id === DOC),
  "memory_status 列出当前文档", `documents=${(status.documents || []).map((d) => d.doc_id)}`);
const memSum = (status.documents || []).reduce((s, d) => s + d.total_bytes, 0);
check(status.total_bytes === memSum,
  "恒等式：total_bytes = Σ documents[].total_bytes", `Σ=${memSum} total=${status.total_bytes}`);
check((status.documents || []).every((d) => d.total_bytes === d.journal_bytes + d.retained_bytes),
  "恒等式：每档 total_bytes = journal_bytes + retained_bytes");
check(status.headroom_bytes === Math.max(0, status.hard_bytes - status.total_bytes),
  "恒等式：headroom_bytes = max(0, hard − total)");
check((status.documents || []).every((d) => d.journal_bytes > 0),
  "memory_status 的 journal_bytes 真的量到原子字节（> 0）");

const mem2 = await (await call(`/api/tools/memory_status?doc=${DOC}`)).json();
check(JSON.stringify(mem2.memory_status) === JSON.stringify(status),
  "memory_status 只读：连调两次结果一致");

if (failures.length > 0) {
  console.error(`\n✗ PWA 只读路由判据红 ${failures.length} 条`);
  process.exit(1);
}
console.log("\n✓ PWA 只读三方对齐路由（stroke_heatmap／list_fonts／memory_status）行为与恒等式均成立");
