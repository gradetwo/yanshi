#!/usr/bin/env node
// **判据：PWA 本地的 `vector_hit_test` 与服务端同答案** ✓（台账「第 六百六十八 节」）。
//
// **∴ 装配手法 ✗**：本地 `brush_stroke` 在 node 里跑不了（要浏览器的 `brush-local.js` ✓）
//   ⇒ 先在真服务端建矢量层／落笔／改属性 ✓ ⇒ 用 `GET /api/atoms` 取完整原子喂进本地 ✓
//   （与 `tool-pwa-atom-log`／`tool-pwa-revert-to-seq` 同一手法 ✓，是**判据装配**不是产品路径 ✓）。
//
// **∴ 它断什么 ✗**（每条都要能红 ✓）：
//   §A 基本几何：笔在／笔外／容差（clearance ≤ tolerance ✓）／两条命中按 clearance 升序 ✓；
//   §A2 压力剖面：半径 = size/2 × (min_ratio + (1−min_ratio)·p) ✓（中点压力 0.5 ⇒ 6.25／7.5 ✓）、
//       `smooth` 走**有效脊线**（Catmull-Rom 采样 ✓）、单点笔按点距 ✓；
//   §A3 只查 `Stroke`：图形／组**不数** ✓、隐藏对象**照查** ✓、几何坏掉照样算 `strokes_examined` ✓；
//   §B 七种入口错误逐字 ＋ `tolerance:null`（＝缺省 0 ✓）＋ 入口校验**先于**图层查找 ✓；
//   §C 已删图层仍解析得类型（读 0 ✓）；**建层被撤** ⇒ 404 ✓；
//   §D 字段集合（顶层 8 ＋ `ok`、命中 4 个键）与 `count === hits.length` ✓。
//
// **∴ 前提 ✗**：活服务端（`run-criteria-local.sh` ✓）＋ 不过期的真 wasm 内核（旧了 `exit(3)` ✓）。
process.on("uncaughtException", (error) => {
  console.error(String((error && error.stack) || error).slice(0, 1200));
  process.exit(3);
});
import { pathToFileURL } from "node:url";

const BASE = process.argv[2];
if (!BASE || !/^https?:\/\//.test(BASE)) {
  console.log("⊘ 不可判：argv[2] 缺服务端地址 ⇒ 请用 bash scripts/run-criteria-local.sh scripts/tool-pwa-vector-hit-test.mjs 跑"
    + "（判据前提不成立，不是产品问题）");
  process.exit(3);
}

const kernelLib = await import(pathToFileURL(process.cwd() + "/scripts/lib/pwa-node-kernel.mjs").href);
const staleness = kernelLib.kernelStaleness();
if (!staleness.fresh) {
  console.log(`⊘ 不可判：${staleness.reason}`);
  for (const line of staleness.stale) console.log("   · " + line);
  console.log("   ⇒ 判据测到的会是上一版内核 ⇒ 前提不成立，不是产品问题");
  process.exit(3);
}

const bad = [];
let okCount = 0;
const check = (cond, msg) => { if (cond) okCount += 1; else bad.push(msg); };
const UNIQ = "v" + Date.now().toString(36);
const { local: localApi } = await kernelLib.bootLocalWithRealKernel({ origin: BASE });
const nativeFetch = globalThis.fetch;
const storeLib = await import(pathToFileURL(process.cwd() + "/web/store.js").href);

const show = (value) => String(JSON.stringify(value)).slice(0, 400);
function same(a, b) {
  if (a === b) return true;
  if (typeof a === "number" && typeof b === "number") {
    if (!Number.isFinite(a) || !Number.isFinite(b)) return false;
    if (Number.isInteger(a) && Number.isInteger(b)) return false;
    return Math.abs(a - b) <= 1e-12 * Math.max(1, Math.abs(a), Math.abs(b));
  }
  if (Array.isArray(a) || Array.isArray(b)) {
    if (!Array.isArray(a) || !Array.isArray(b) || a.length !== b.length) return false;
    return a.every((v, i) => same(v, b[i]));
  }
  if (a && b && typeof a === "object" && typeof b === "object") {
    const ka = Object.keys(a).sort(), kb = Object.keys(b).sort();
    if (ka.length !== kb.length || ka.some((k, i) => k !== kb[i])) return false;
    return ka.every((k) => same(a[k], b[k]));
  }
  return false;
}
function firstDiff(a, b, path = "") {
  if (same(a, b)) return null;
  if (a && b && typeof a === "object" && typeof b === "object"
      && !Array.isArray(a) && !Array.isArray(b)) {
    const keys = Array.from(new Set([...Object.keys(a), ...Object.keys(b)])).sort();
    for (const key of keys) {
      if (!(key in a)) return `${path}.${key}：服务端缺（本地 ${show(b[key])}）`;
      if (!(key in b)) return `${path}.${key}：本地缺（服务端 ${show(a[key])}）`;
      const hit = firstDiff(a[key], b[key], `${path}.${key}`);
      if (hit) return hit;
    }
    return null;
  }
  if (Array.isArray(a) && Array.isArray(b)) {
    if (a.length !== b.length) return `${path}.length：${a.length} vs ${b.length}`;
    for (let i = 0; i < a.length; i += 1) {
      const hit = firstDiff(a[i], b[i], `${path}[${i}]`);
      if (hit) return hit;
    }
    return null;
  }
  return `${path}：服务端 ${show(a)}｜本地 ${show(b)}`;
}
// **服务端 HTTP 信封 ✗ 不是工具契约** ✓（`created`／`opened`／`timings` 逐次不同 ✓）⇒ 比对前两边都剥掉 ✓。
const SERVER_ENVELOPE = ["created", "opened", "timings"];
const strip = (body) => {
  const out = { ...(body || {}) };
  for (const key of SERVER_ENVELOPE) delete out[key];
  delete out.server;
  return out;
};
const detailOf = (res) => (res.body && res.body.context && res.body.context.detail) ?? null;

function compareBody(label, srv, loc) {
  check(srv.status === 200 && srv.body && srv.body.ok !== false,
    `${label}：服务端应 200 ✗ 状态 ${srv.status}｜${show(srv.body)}`);
  check(loc.status === 200 && loc.body && loc.body.ok !== false,
    `${label}：本地应 200 ✗ 状态 ${loc.status}｜${show(loc.body)}`);
  if (!srv.body || !loc.body) return;
  check(loc.body.server === false, `${label}：本地应标 server:false ✗ ${show(loc.body.server)}`);
  const vs = strip(srv.body), vl = strip(loc.body);
  check(same(vs, vl), `${label}：全字段不一致 ✗\n     首个差异 ${firstDiff(vs, vl) || "（结构相等但 same 判不等 ⇒ 数值口径）"}`);
}
function compareError(label, srv, loc, code, status) {
  if (!srv.body || !loc.body) { check(false, `${label}：有一边读不出 JSON ✗`); return; }
  check(srv.status === status, `${label}：服务端状态码应为 ${status} ✗ 实测 ${srv.status}｜${show(srv.body)}`);
  check(loc.status === status, `${label}：本地状态码应为 ${status} ✗ 实测 ${loc.status}｜${show(loc.body)}`);
  check(srv.body.error_code === code,
    `${label}：服务端错误码应为 ${code} ✗ 实测 ${srv.body.error_code}`);
  check(loc.body.error_code === code,
    `${label}：本地错误码应为 ${code} ✗ 实测 ${loc.body.error_code}`);
  check(detailOf(srv) === detailOf(loc),
    `${label}：两边文案不同 ✗\n     服务端 ${detailOf(srv)}\n     本地   ${detailOf(loc)}`);
}

async function createDoc(doc, width, height) {
  const body = await nativeFetch(`${BASE}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width, height }),
  }).then((r) => r.json());
  if (!body.ok || !body.token) {
    console.log(`⊘ 不可判：服务端建文档失败（${show(body)}）⇒ 前提不成立，不是产品问题`);
    process.exit(3);
  }
  return body.token;
}
async function callServer(tool, args, token, doc) {
  const res = await nativeFetch(`${BASE}/api/tools/${tool}?token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, actor: "human:judge", session: "session:judge", ...args }),
  });
  return { status: res.status, body: await res.json().catch(() => null) };
}
async function localCall(tool, args, doc) {
  try {
    const res = await localApi(new Request(`https://x/api/tools/${tool}?doc=${doc}`, {
      method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}),
    }));
    return { status: res.status, body: await res.json().catch(() => null) };
  } catch (e) {
    return { status: 0, body: { ok: false, error_code: "local_threw",
      context: { detail: String((e && e.message) || e).slice(0, 300) } } };
  }
}
/** **把服务端的完整原子按 seq 顺序喂进本地文档 ✓**（判据装配，见文件头；**只能播一次** ✓）。 */
async function seedLocal(serverDoc, token, localDoc) {
  const res = await nativeFetch(`${BASE}/api/atoms?doc=${encodeURIComponent(serverDoc)}&since=0&token=${token}`);
  const body = await res.json().catch(() => null);
  if (!body || body.ok !== true || !Array.isArray(body.atoms)) {
    console.log(`⊘ 不可判：GET /api/atoms 读不出（${res.status} ${show(body)}）`
      + " ⇒ 前提不成立，不是产品问题");
    process.exit(3);
  }
  const db = await storeLib.open();
  for (const atom of body.atoms) await storeLib.putAtom(db, localDoc, structuredClone(atom));
  return body.atoms.length;
}

const RED = [1, 0, 0, 1];
const stroke = (objectId, data) => ["draw_stroke", { layer_id: "V1", object_id: objectId, data }];

/** **一个 case 开一份服务端文档 ＋ 一份本地文档** ✓（别共用文档：前一段的墨会污染后一段 ✓）。
 *  `prepare(ctx)` 在**所有服务端前序之后、播进本地之前**跑 ✓（要用服务端回执的 case 需要它 ✓）。
 */
async function makeCase(key, ops, prepare) {
  const serverDoc = `vhs-${key}-${UNIQ}`;
  const localDoc = `vhl-${key}-${UNIQ}`;
  const token = await createDoc(serverDoc, 96, 72);
  for (const [tool, args] of ops) {
    const res = await callServer(tool, args, token, serverDoc);
    if (res.status !== 200 || !res.body || res.body.ok === false) {
      console.log(`⊘ 不可判：前序 ${tool}（${key}）服务端拒绝（${res.status} ${show(res.body)}）`
        + " ⇒ 前提不成立，不是产品问题");
      process.exit(3);
    }
  }
  const ctx = {
    token, serverDoc, localDoc,
    server: (tool, args) => callServer(tool, args, token, serverDoc),
    local: (tool, args) => localCall(tool, args, localDoc),
    both: async (tool, args) => [await callServer(tool, args, token, serverDoc),
      await localCall(tool, args, localDoc)],
  };
  if (prepare) await prepare(ctx);
  await seedLocal(serverDoc, token, localDoc);
  return ctx;
}

// ══ §A 基本几何：笔在／笔外／容差／排序 ═════════════════════════════════════════
{
  const ctx = await makeCase("a", [
    ["create_layer", { layer_id: "V1", kind: "vector", name: "矢量层" }],
    stroke("S1", { points: [[10, 10], [90, 10]], size: 20, color: RED }),
    stroke("S2", { points: [[10, 50], [90, 50]], size: 10, color: RED }),
    stroke("S3", { points: [[50, 0], [50, 30]], size: 4, color: RED }),
  ]);
  // A1 笔心：S1 距离 0、半径 10、余量 −10 ⇒ 命中；S3 也穿过 (50,10)（余量 −2）⇒ 两笔都命中，
  // S1 余量更小排前；S2 余量 35 ⇒ 容差 0 不命中。
  const [s1, l1] = await ctx.both("vector_hit_test", { layer_id: "V1", x: 50, y: 10 });
  compareBody("§A1 笔心", s1, l1);
  check(s1.body && s1.body.count === 2 && s1.body.hits[0].object_id === "S1"
    && s1.body.hits[1].object_id === "S3",
  `§A1 应命中 S1 与 S3（S1 排前）✗ ${show(s1.body && s1.body.hits)}`);
  check(s1.body && s1.body.hits[0].distance === 0 && s1.body.hits[0].radius === 10
    && s1.body.hits[0].clearance === -10,
  `§A1 距离／半径／余量应 0／10／−10 ✗ ${show(s1.body && s1.body.hits[0])}`);
  // A2 笔外（S2 余量 1）：容差 0 ⇒ 无命中。
  const [s2, l2] = await ctx.both("vector_hit_test", { layer_id: "V1", x: 50, y: 56 });
  compareBody("§A2 笔外 tol=0", s2, l2);
  check(s2.body && s2.body.count === 0, `§A2 容差 0 应无命中 ✗ ${show(s2.body)}`);
  // A3 同一点容差 2 ⇒ 命中（余量 1 ≤ 2 ✓）—— 容差**真的被读**。
  const [s3, l3] = await ctx.both("vector_hit_test", { layer_id: "V1", x: 50, y: 56, tolerance: 2 });
  compareBody("§A3 笔外 tol=2", s3, l3);
  check(s3.body && s3.body.count === 1 && s3.body.hits[0].object_id === "S2"
    && s3.body.tolerance === 2,
  `§A3 容差 2 应命中 S2 且回显 tolerance ✗ ${show(s3.body)}`);
  // A4 两笔同时命中（S1 余量 −10、S3 余量 −2）⇒ 按余量升序。
  const [s4, l4] = await ctx.both("vector_hit_test", { layer_id: "V1", x: 50, y: 12 });
  compareBody("§A4 两条命中排序", s4, l4);
  check(s4.body && same(s4.body.hits.map((h) => h.object_id), ["S1", "S3"]),
    `§A4 命中应按余量升序 [S1,S3] ✗ ${show(s4.body && s4.body.hits.map((h) => h.object_id))}`);
  check(s4.body && s4.body.hits[0].clearance <= s4.body.hits[1].clearance,
    `§A4 余量应非降 ✗ ${show(s4.body && s4.body.hits)}`);
  // A5 缺省容差与 `tolerance:null` 同答（服务端把 null 当「未提供」✓）。
  const [s5, l5] = await ctx.both("vector_hit_test", { layer_id: "V1", x: 50, y: 10, tolerance: null });
  compareBody("§A5 tolerance=null", s5, l5);
  check(s5.body && s5.body.tolerance === 0, `§A5 null 容差应按 0 ✗ ${show(s5.body && s5.body.tolerance)}`);
  check(s5.body && s5.body.strokes_examined === 3,
    `§A5 三笔都应被查 ✗ ${show(s5.body && s5.body.strokes_examined)}`);
  // A6 小数坐标（回显照原值 ✓）。
  const [s6, l6] = await ctx.both("vector_hit_test", { layer_id: "V1", x: 50.5, y: 10.25 });
  compareBody("§A6 小数坐标", s6, l6);
}

// ══ §A2 压力剖面／smooth 有效脊线／单点笔 ═════════════════════════════════════════
{
  const ctx = await makeCase("a2", [
    ["create_layer", { layer_id: "V1", kind: "vector", name: "矢量层" }],
    // P1 中点压力 0.5 ⇒ 半径 = 10 × (0.25 + 0.75×0.5) = 6.25 ✓（缺省 min_size_ratio 0.25）。
    stroke("P1", { points: [[10, 10, 0], [90, 10, 1]], size: 20, color: RED }),
    // P2 同压力、min_size_ratio 0.5 ⇒ 半径 = 10 × (0.5 + 0.5×0.5) = 7.5 ✓。
    stroke("P2", { points: [[10, 30, 0], [90, 30, 1]], size: 20, color: RED, min_size_ratio: 0.5 }),
    // P3 平滑折线：有效脊线是 Catmull-Rom 采样（不是存储点列 ✓）。
    stroke("P3", { points: [[10, 60], [40, 80], [70, 60]], size: 8, smooth: true, color: RED }),
    // P4 单点笔：按点距 ✓。
    stroke("P4", { points: [[120, 20]], size: 10, color: RED }),
  ]);
  const [s1, l1] = await ctx.both("vector_hit_test", { layer_id: "V1", x: 50, y: 10 });
  compareBody("§A2 压力中点 P1", s1, l1);
  check(s1.body && s1.body.strokes_examined === 4,
    `§A2 四笔都应被查 ✗ ${show(s1.body && s1.body.strokes_examined)}`);
  const p1 = (s1.body && s1.body.hits || []).find((h) => h.object_id === "P1");
  check(p1 && Math.abs(p1.radius - 6.25) < 1e-9,
    `§A2 P1 中点半径应为 6.25 ✗ ${show(p1)}`);
  const [s2, l2] = await ctx.both("vector_hit_test", { layer_id: "V1", x: 50, y: 30 });
  compareBody("§A2 压力中点 P2", s2, l2);
  const p2 = (s2.body && s2.body.hits || []).find((h) => h.object_id === "P2");
  check(p2 && Math.abs(p2.radius - 7.5) < 1e-9,
    `§A2 P2 中点半径应为 7.5（min_size_ratio 0.5）✗ ${show(p2)}`);
  // 查询点取平滑脊线的 t=0.5 采样（(25, 72.5) ⇒ 距离 0、半径 4、余量 −4 ✓）；
  // 同一坐标到**原始折线**的距离约 2.08 ⇒ 若 smooth 被忽略，距离／余量两条就红 ✓
  // （折线弯度 2.44 < 半径 4 ⇒ 命中集合分不出来，只能断距离 ✓）。
  const [s3, l3] = await ctx.both("vector_hit_test", { layer_id: "V1", x: 25, y: 72.5 });
  compareBody("§A2 smooth 有效脊线", s3, l3);
  const p3 = (s3.body && s3.body.hits || []).find((h) => h.object_id === "P3");
  check(p3 && Math.abs(p3.distance) < 1e-9 && Math.abs(p3.radius - 4) < 1e-9
    && Math.abs(p3.clearance + 4) < 1e-9,
  `§A2 smooth 点在平滑脊线上应距离 0／半径 4／余量 −4 ✗ ${show(p3)}`);
  const [s4, l4] = await ctx.both("vector_hit_test", { layer_id: "V1", x: 120, y: 33 });
  compareBody("§A2 单点笔 tol=0", s4, l4);
  check(s4.body && !s4.body.hits.some((h) => h.object_id === "P4"),
    `§A2 单点笔余量 8 ⇒ 容差 0 不该命中 ✗ ${show(s4.body && s4.body.hits)}`);
  const [s5, l5] = await ctx.both("vector_hit_test", { layer_id: "V1", x: 120, y: 33, tolerance: 10 });
  compareBody("§A2 单点笔 tol=10", s5, l5);
  const p4 = (s5.body && s5.body.hits || []).find((h) => h.object_id === "P4");
  check(p4 && Math.abs(p4.distance - 13) < 1e-9 && Math.abs(p4.radius - 5) < 1e-9,
    `§A2 单点笔距离应 13、半径应 5 ✗ ${show(p4)}`);
}

// ══ §A3 只查 Stroke：图形／组不数、隐藏照查、坏几何照数 ═══════════════════════════
{
  const ctx = await makeCase("a3", [
    ["create_layer", { layer_id: "V1", kind: "vector", name: "矢量层" }],
    // S1 被盖住查询点 ⇒ 若它被当成笔迹，`strokes_examined` 会多 1。
    stroke("S1", { points: [[10, 10], [90, 10]], size: 20, color: RED }),
    // SH1 是**图形**（DrawShape ⇒ Shape ✓）—— 只写 bbox、没有 points。
    ["draw_shape", { layer_id: "V1", object_id: "SH1",
                     data: { geometry: { kind: "rect", bbox: { x: 40, y: 0, w: 20, h: 30 } },
                             color: [0, 0, 1, 1] } }],
    // G1 是**组**（CreateObject + type:group ✓）。
    ["create_group", { group_id: "G1", layer_id: "V1" }],
    // S2 的几何随后被替换成**没有 points** 的数据 ⇒ 仍算「查过」但不出命中 ✓。
    stroke("S2", { points: [[10, 40], [90, 40]], size: 10, color: RED }),
    ["replace_object_data", { object_id: "S2", data: { size: 10 } }],
    // S1 隐藏 —— 服务端 `objects_in_layer` **不过滤可见性** ⇒ 照查 ✓。
    ["set_property", { object_id: "S1", key: "visible", value: false }],
  ]);
  const [s, l] = await ctx.both("vector_hit_test", { layer_id: "V1", x: 50, y: 10 });
  compareBody("§A3 只查笔迹", s, l);
  check(s.body && s.body.strokes_examined === 2,
    `§A3 只该数 S1／S2 两笔（图形与组不算）✗ ${show(s.body && s.body.strokes_examined)}`);
  check(s.body && same((s.body.hits || []).map((h) => h.object_id), ["S1"]),
    `§A3 隐藏的 S1 照样命中 ✗ ${show(s.body && s.body.hits)}`);
  const [ss, ll] = await ctx.both("vector_hit_test", { layer_id: "V1", x: 50, y: 40 });
  compareBody("§A3 坏几何不命中", ss, ll);
  check(ss.body && ss.body.count === 0,
    `§A3 没有 points 的 S2 不该出命中 ✗ ${show(ss.body)}`);
}

// ══ §B 入口与处理器错误逐字 ══════════════════════════════════════════════════════
{
  const ctx = await makeCase("b", [
    ["create_layer", { layer_id: "V1", kind: "vector", name: "矢量层" }],
    ["create_layer", { layer_id: "R1", type: "raster", name: "光栅层" }],
    ["create_layer", { layer_id: "G1", type: "layer_group", name: "组" }],
    ["create_layer", { layer_id: "M1", type: "mask", name: "蒙版" }],
    stroke("S1", { points: [[10, 10], [90, 10]], size: 20, color: RED }),
  ]);
  const errors = [
    ["§B1 缺 layer_id", {}, "invalid_argument", 400],
    ["§B2 缺 x", { layer_id: "V1" }, "invalid_argument", 400],
    ["§B3 x 是字符串", { layer_id: "V1", x: "5", y: 1 }, "invalid_argument", 400],
    ["§B4 未知键", { layer_id: "V1", x: 1, y: 1, foo: 2 }, "invalid_argument", 400],
    ["§B5 图层不存在", { layer_id: "NOPE", x: 1, y: 1 }, "reference_not_found", 404],
    ["§B6 光栅层", { layer_id: "R1", x: 1, y: 1 }, "invalid_argument", 400],
    ["§B7 图层组", { layer_id: "G1", x: 1, y: 1 }, "invalid_argument", 400],
    ["§B8 蒙版层", { layer_id: "M1", x: 1, y: 1 }, "invalid_argument", 400],
    ["§B9 容差为负", { layer_id: "V1", x: 1, y: 1, tolerance: -1 }, "invalid_argument", 400],
    // **入口校验先于图层查找** ✓：层不存在**也**先报缺 x ✓（服务端 `validate_args` 在 dispatch 之前 ✓）。
    ["§B10 入口校验优先", { layer_id: "NOPE" }, "invalid_argument", 400],
  ];
  for (const [label, args, code, status] of errors) {
    const [s, l] = await ctx.both("vector_hit_test", args);
    compareError(label, s, l, code, status);
  }
  // 服务端的三种图层类型名照 Rust `Debug`（Raster／LayerGroup／Mask ✓）—— 判据自己点名 ✓。
  const [sr] = await ctx.both("vector_hit_test", { layer_id: "R1", x: 1, y: 1 });
  check(detailOf(sr) === "图层 R1 是 Raster 图层、不是矢量图层 => 矢量工具不作用于它（建矢量层请用 create_layer 的 kind 参数传 vector）",
    `§B6 文案不对 ✗ ${show(detailOf(sr))}`);
}

// ══ §C 已删图层仍可查（读 0）；建层被撤 ⇒ 404 ═════════════════════════════════════
{
  // C1：删层后服务端仍从 `state.layers.get()` 解析得类型 ⇒ 200、0 笔（对象随层死 ✓）。
  const ctx = await makeCase("c1", [
    ["create_layer", { layer_id: "V1", kind: "vector", name: "矢量层" }],
    stroke("S1", { points: [[10, 10], [90, 10]], size: 20, color: RED }),
    ["delete_layer", { layer_id: "V1" }],
  ]);
  const [s, l] = await ctx.both("vector_hit_test", { layer_id: "V1", x: 50, y: 10 });
  compareBody("§C1 已删图层", s, l);
  check(s.body && s.body.strokes_examined === 0 && s.body.count === 0,
    `§C1 已删图层应读到 0 笔 0 命中（不是 404）✗ ${show(s.body)}`);
}
{
  // C2：建层被 revert ⇒ 层不在状态里 ⇒ 404（`p2LayerTypeFold` 跳过被中和的 create ✓）。
  const ctx = await makeCase("c2", [
    ["create_layer", { layer_id: "V1", kind: "vector", name: "矢量层" }],
    stroke("S1", { points: [[10, 10], [90, 10]], size: 20, color: RED }),
  ], async (c) => {
    const atoms = await c.server("list_atoms", { layer_id: "V1", include_non_stroke: true });
    const createAtom = (atoms.body && atoms.body.atoms || [])
      .find((a) => a.kind === "create_layer" && a.layer_id === "V1");
    if (!createAtom) {
      console.log("⊘ 不可判：list_atoms 里找不到建层原子 ⇒ 前提不成立，不是产品问题");
      process.exit(3);
    }
    const reverted = await c.server("revert_atoms", { atom_ids: [createAtom.atom_id] });
    if (reverted.status !== 200 || !reverted.body || reverted.body.ok === false) {
      console.log(`⊘ 不可判：revert_atoms 被拒（${show(reverted.body)}）⇒ 前提不成立，不是产品问题`);
      process.exit(3);
    }
  });
  const [s, l] = await ctx.both("vector_hit_test", { layer_id: "V1", x: 50, y: 10 });
  compareError("§C2 建层被撤", s, l, "reference_not_found", 404);
}

// ══ §D 字段集合 ＋ 恒等式 ═════════════════════════════════════════════════════════
{
  const ctx = await makeCase("d", [
    ["create_layer", { layer_id: "V1", kind: "vector", name: "矢量层" }],
    stroke("S1", { points: [[10, 10], [90, 10]], size: 20, color: RED }),
  ]);
  const [s, l] = await ctx.both("vector_hit_test", { layer_id: "V1", x: 50, y: 10, tolerance: 3 });
  compareBody("§D 字段集合", s, l);
  const keys = Object.keys(strip(s.body || {})).sort();
  check(same(keys, ["count", "hits", "layer_id", "ok", "strokes_examined", "tolerance", "x", "y"]),
    `§D 顶层字段集合不对 ✗ ${show(keys)}`);
  check(s.body && s.body.count === s.body.hits.length,
    `§D count 应等于 hits.length ✗ ${show(s.body && { count: s.body.count, len: s.body.hits.length })}`);
  check(s.body && s.body.hits.every((h) => same(Object.keys(h).sort(),
    ["clearance", "distance", "object_id", "radius"])),
  `§D 命中的字段集合不对 ✗ ${show(s.body && s.body.hits)}`);
}

if (bad.length) {
  console.log(`✗ 失败 ${bad.length} 条（通过 ${okCount}）：`);
  for (const line of bad) console.log("   " + line);
  process.exit(1);
}
console.log(`✓ tool-pwa-vector-hit-test：${okCount} 条断言全过（vector_hit_test 与服务端同答案 ✓）`);
