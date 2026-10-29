#!/usr/bin/env node
// **判据：PWA 本地的 `get_path` 与服务端同答案** ✓（台账「第 六百六十九 节」）。
//
// **∴ 装配手法 ✗**：本地 `draw_shape` 不碰像素、但**要原子日志** ✓ ⇒ 与服务端判据同一手法：
//   先在真服务端建层／建 path／改数据／变换 ✓ ⇒ 用 `GET /api/atoms` 把完整原子喂进本地 ✓
//   （判据装配，不是产品路径 ✓）。
//
// **∴ 它断什么 ✗**（每条都要能红 ✓）：
//   §A 几何读回：`d` 字符串逐字（Rust `Display` 形 ⇒ 小数不外露指数 ✓、大数补零 ✓）、
//       直段总长精确 80／闭合回首再 +10 ⇒ 40、三次段按容差 0.10 展平的总长在 190–210 ✓、
//       `bbox` 是**控制点**盒（三次段给 y 0–100 ⇒ h=100 ✓ 不是展平曲线盒 ✗）、
//       `node_counts` = 段数 + 1（**不是展平采样数** ✓）、多子路径、`fill_rule` 缺省 evenodd ✓ 回显 nonzero ✓；
//   §A2 **本地坐标**：对象被 `transform_object` 平移 1000 后读回不变 ✓（不应用 `object.transform` ✓）、
//       隐藏对象照读 ✓（服务端不查可见性 ✓）；
//   §B 两个 404（不存在／已删／随层死）＋ 六种 400（笔／矩形／笔里塞 path 几何／
//       空 subpaths／`fill_rule:null`／未知段算子）＋ 入口校验**先于**对象查找 ✓；
//   §C 建对象被撤 ⇒ 404「不存在」✓（不是「已被删除」✗）；
//   §D 字段集合（`ok` ＋ 10）与 `node_counts`／`subpaths`／`geometry` 的恒等式 ✓。
//
// **∴ 前提 ✗**：活服务端（`run-criteria-local.sh` ✓）＋ 不过期的真 wasm 内核（旧了 `exit(3)` ✓）。
process.on("uncaughtException", (error) => {
  console.error(String((error && error.stack) || error).slice(0, 1200));
  process.exit(3);
});
import { pathToFileURL } from "node:url";

const BASE = process.argv[2];
if (!BASE || !/^https?:\/\//.test(BASE)) {
  console.log("⊘ 不可判：argv[2] 缺服务端地址 ⇒ 请用 bash scripts/run-criteria-local.sh scripts/tool-pwa-get-path.mjs 跑"
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
const UNIQ = "g" + Date.now().toString(36);
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

const BLUE = [0, 0, 1, 1];
const pathGeometry = (subpaths, extra = {}) => ({ kind: "path", subpaths, ...extra });
const line = (from, to) => ({ start: from, segments: [{ op: "L", to }] });
const shape = (objectId, geometry, extraData = {}, layerId = "V1") =>
  ["draw_shape", { layer_id: layerId, object_id: objectId,
                   data: { geometry, color: BLUE, ...extraData } }];

/** **一个 case 开一份服务端文档 ＋ 一份本地文档** ✓（每条子路径的期望值都独立算 ✓）。
 *  `prepare(ctx)` 在**所有服务端前序之后、播进本地之前**跑 ✓（要用服务端回执的 case 需要它 ✓）。
 */
async function makeCase(key, ops, prepare) {
  const serverDoc = `gps-${key}-${UNIQ}`;
  const localDoc = `gpl-${key}-${UNIQ}`;
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

// ══ §A 几何读回：d／length／bbox／node_counts／fill_rule ═════════════════════════
{
  const ctx = await makeCase("a", [
    ["create_layer", { layer_id: "V1", kind: "vector", name: "矢量层" }],
    // P1 直段：总长精确 80。
    shape("P1", pathGeometry([line([10, 10], [90, 10])])),
    // P2 三次段：控制点盒 x 10–110／y 20–120 ⇒ w/h 都是 100（展平盒会略小于它 ⇒
    // 区分「控制点盒」✓）；起点非零 ⇒ 霍纳的常数项真的被读 ✓。
    shape("P2", pathGeometry([{ start: [10, 20],
      segments: [{ op: "C", c1: [10, 120], c2: [110, 120], to: [110, 20] }] }])),
    // P3 闭合方形：三边 30 ＋ 回首 10 ⇒ 40；`d` 以 Z 收尾。
    shape("P3", pathGeometry([{ start: [0, 0], closed: true,
      segments: [{ op: "L", to: [10, 0] }, { op: "L", to: [10, 10] }, { op: "L", to: [0, 10] }] }])),
    // P4 两条子路径：node_counts [2,3]、总长 5 + 10 = 15。
    shape("P4", pathGeometry([
      line([0, 0], [5, 0]),
      { start: [20, 20], segments: [{ op: "L", to: [20, 25] }, { op: "L", to: [25, 25] }] },
    ])),
    // P5 显式 nonzero。
    shape("P5", pathGeometry([line([0, 0], [3, 4])], { fill_rule: "nonzero" })),
    // P6 随后隐藏（服务端不查可见性 ⇒ 照读 ✓）。
    shape("P6", pathGeometry([line([0, 0], [2, 2])])),
    // P7 随后整体平移 1000（读回仍是本地坐标 ✓）。
    shape("P7", pathGeometry([line([10, 10], [20, 10])])),
    // P8 小数坐标：`d` 里 1e-7 要写成 0.0000001（Rust Display 无科学计数法 ✓）。
    shape("P8", pathGeometry([{ start: [1e-7, 2e-7],
      segments: [{ op: "L", to: [1 / 3, 0] }] }])),
    // P9 大数坐标：1e21 要写成 22 位十进制（JS 的 1e+21 形不行 ✗）。
    shape("P9", pathGeometry([line([1e21, 0], [-1e21, 5])])),
    ["set_property", { object_id: "P6", key: "visible", value: false }],
    ["transform_object", { object_id: "P7", translate: { dx: 1000, dy: 1000 } }],
  ]);
  const getPath = (objectId) => ctx.both("get_path", { object_id: objectId });

  // A1 直段：全部期望值由判据自己算（不抄服务端 ✓）。
  const [s1, l1] = await getPath("P1");
  compareBody("§A1 直段", s1, l1);
  check(s1.body && s1.body.d === "M 10 10 L 90 10",
    `§A1 d 应为「M 10 10 L 90 10」✗ ${show(s1.body && s1.body.d)}`);
  check(s1.body && s1.body.length === 80,
    `§A1 总长应为 80 ✗ ${show(s1.body && s1.body.length)}`);
  check(s1.body && same(s1.body.node_counts, [2]),
    `§A1 node_counts 应为 [2] ✗ ${show(s1.body && s1.body.node_counts)}`);
  check(s1.body && same(s1.body.bbox, { x: 10, y: 10, w: 80, h: 0 }),
    `§A1 bbox 应为 10/10/80/0 ✗ ${show(s1.body && s1.body.bbox)}`);
  check(s1.body && s1.body.fill_rule === "evenodd" && s1.body.kind === "path"
    && s1.body.object_id === "P1" && s1.body.layer_id === "V1",
  `§A1 fill_rule／kind／object_id／layer_id 不对 ✗ ${show(s1.body)}`);
  check(s1.body && same(s1.body.subpaths,
    [{ start: [10, 10], closed: false, segments: [{ op: "L", to: [90, 10] }] }]),
  `§A1 subpaths 应带 closed:false ✗ ${show(s1.body && s1.body.subpaths)}`);

  // A2 三次段：bbox 是控制点盒（h=100）；总长在 190–210（展平值，非控制多边形周长 ✓）。
  const [s2, l2] = await getPath("P2");
  compareBody("§A2 三次段", s2, l2);
  check(s2.body && same(s2.body.bbox, { x: 10, y: 20, w: 100, h: 100 }),
    `§A2 bbox 应是控制点盒 10/20/100/100 ✗ ${show(s2.body && s2.body.bbox)}`);
  check(s2.body && same(s2.body.node_counts, [2]),
    `§A2 node_counts 应是 [2]（段数 + 1，不是展平采样数）✗ ${show(s2.body && s2.body.node_counts)}`);
  check(s2.body && s2.body.length > 190 && s2.body.length < 210,
    `§A2 三次段总长应在 190–210 ✗ ${show(s2.body && s2.body.length)}`);
  check(s2.body && s2.body.d === "M 10 20 C 10 120 110 120 110 20",
    `§A2 d 应为「M 10 20 C 10 120 110 120 110 20」✗ ${show(s2.body && s2.body.d)}`);

  // A3 闭合：总长含回首段 ⇒ 40；d 以 Z 收尾。
  const [s3, l3] = await getPath("P3");
  compareBody("§A3 闭合方形", s3, l3);
  check(s3.body && s3.body.length === 40,
    `§A3 闭合方形总长应 40（三边 30 ＋ 回首 10）✗ ${show(s3.body && s3.body.length)}`);
  check(s3.body && s3.body.d === "M 0 0 L 10 0 L 10 10 L 0 10 Z",
    `§A3 d 应以 Z 收尾 ✗ ${show(s3.body && s3.body.d)}`);
  check(s3.body && same(s3.body.node_counts, [4]),
    `§A3 node_counts 应为 [4] ✗ ${show(s3.body && s3.body.node_counts)}`);

  // A4 多子路径：d 里子路径之间一个空格；node_counts 逐条。
  const [s4, l4] = await getPath("P4");
  compareBody("§A4 多子路径", s4, l4);
  check(s4.body && same(s4.body.node_counts, [2, 3]),
    `§A4 node_counts 应为 [2,3] ✗ ${show(s4.body && s4.body.node_counts)}`);
  check(s4.body && s4.body.length === 15,
    `§A4 总长应 15 ✗ ${show(s4.body && s4.body.length)}`);
  check(s4.body && same(s4.body.bbox, { x: 0, y: 0, w: 25, h: 25 }),
    `§A4 bbox 应为 0/0/25/25 ✗ ${show(s4.body && s4.body.bbox)}`);
  check(s4.body && s4.body.d === "M 0 0 L 5 0 M 20 20 L 20 25 L 25 25",
    `§A4 d 子路径之间应一个空格 ✗ ${show(s4.body && s4.body.d)}`);

  // A5 fill_rule 回显：顶层与 geometry 两处都是 nonzero。
  const [s5, l5] = await getPath("P5");
  compareBody("§A5 nonzero", s5, l5);
  check(s5.body && s5.body.fill_rule === "nonzero"
    && s5.body.geometry && s5.body.geometry.fill_rule === "nonzero",
  `§A5 fill_rule 两处都应 nonzero ✗ ${show(s5.body)}`);

  // A6 隐藏仍可读。
  const [s6, l6] = await getPath("P6");
  compareBody("§A6 隐藏对象", s6, l6);
  check(s6.body && s6.body.d === "M 0 0 L 2 2",
    `§A6 隐藏对象应照读 ✗ ${show(s6.body)}`);

  // A7 变换后读回本地坐标（不应用 object.transform）。
  const [s7, l7] = await getPath("P7");
  compareBody("§A7 变换后", s7, l7);
  check(s7.body && s7.body.d === "M 10 10 L 20 10"
    && same(s7.body.bbox, { x: 10, y: 10, w: 10, h: 0 }),
  `§A7 平移 1000 后读回应不变（本地坐标）✗ ${show(s7.body && { d: s7.body.d, bbox: s7.body.bbox })}`);

  // A8 小数：Rust Display 形（无指数）。
  const [s8, l8] = await getPath("P8");
  compareBody("§A8 小数", s8, l8);
  check(s8.body && s8.body.d === "M 0.0000001 0.0000002 L 0.3333333333333333 0",
    `§A8 小数应写成普通十进制 ✗ ${show(s8.body && s8.body.d)}`);

  // A9 大数：1e21 展开成 22 位十进制。
  const [s9, l9] = await getPath("P9");
  compareBody("§A9 大数", s9, l9);
  check(s9.body && s9.body.d === "M 1000000000000000000000 0 L -1000000000000000000000 5",
    `§A9 1e21 应展开成 22 位十进制 ✗ ${show(s9.body && s9.body.d)}`);
}

// ══ §B 两个 404 ＋ 六种 400 ＋ 入口校验优先 ═══════════════════════════════════════
{
  const valid = pathGeometry([line([0, 0], [1, 1])]);
  const ctx = await makeCase("b", [
    ["create_layer", { layer_id: "V1", kind: "vector", name: "矢量层" }],
    ["draw_stroke", { layer_id: "V1", object_id: "S1",
                      data: { points: [[10, 10], [20, 10]], size: 5, color: [1, 0, 0, 1] } }],
    shape("R1", { kind: "rect", bbox: { x: 0, y: 0, w: 10, h: 10 } }),
    // **笔里塞 path 几何** ✓（对象类型仍是 Stroke ⇒ 服务端不看几何直接 400 ✓；
    //   若只按几何判断就会误读成 200 ✗）。
    ["draw_stroke", { layer_id: "V1", object_id: "G1",
                      data: { points: [[30, 30], [40, 30]], size: 3, color: [1, 0, 0, 1] } }],
    ["replace_object_data", { object_id: "G1",
                              data: { geometry: pathGeometry([line([0, 0], [1, 1])]) } }],
    // **矩形带 subpaths** ✓：若省掉 `geometry.kind` 判断就会误读成 200 ✗。
    shape("R2", valid),
    ["replace_object_data", { object_id: "R2",
      data: { geometry: { kind: "rect", bbox: { x: 0, y: 0, w: 5, h: 5 },
                          subpaths: [line([0, 0], [4, 4])] } } }],
    // **`closed` 非布尔** ✓（服务端拒绝非布尔 ⇒ 400 ✓）。
    shape("M1", valid),
    ["replace_object_data", { object_id: "M1",
      data: { geometry: pathGeometry([{ start: [0, 0], closed: 1,
        segments: [{ op: "L", to: [1, 1] }] }]) } }],
    shape("D1", valid),
    ["delete_object", { object_id: "D1" }],
    shape("Q1", valid),
    ["replace_object_data", { object_id: "Q1", data: { geometry: pathGeometry([]) } }],
    shape("W1", valid),
    ["replace_object_data", { object_id: "W1",
      data: { geometry: pathGeometry([line([0, 0], [1, 1])], { fill_rule: null }) } }],
    shape("X1", valid),
    ["replace_object_data", { object_id: "X1",
      data: { geometry: pathGeometry([{ start: [0, 0],
        segments: [{ op: "X", to: [1, 1] }] }]) } }],
  ]);
  const errors = [
    ["§B1 缺 object_id", {}, "invalid_argument", 400],
    ["§B2 object_id 非字符串", { object_id: 5 }, "invalid_argument", 400],
    ["§B3 未知键", { object_id: "S1", foo: 1 }, "invalid_argument", 400],
    ["§B4 对象不存在", { object_id: "NOPE" }, "reference_not_found", 404],
    ["§B5 对象已删", { object_id: "D1" }, "reference_not_found", 404],
    ["§B6 笔迹", { object_id: "S1" }, "invalid_argument", 400],
    ["§B7 矩形", { object_id: "R1" }, "invalid_argument", 400],
    ["§B8 笔里塞 path 几何", { object_id: "G1" }, "invalid_argument", 400],
    ["§B9 空 subpaths", { object_id: "Q1" }, "invalid_argument", 400],
    ["§B10 fill_rule 为 null", { object_id: "W1" }, "invalid_argument", 400],
    ["§B11 未知段算子", { object_id: "X1" }, "invalid_argument", 400],
    ["§B14 矩形带 subpaths", { object_id: "R2" }, "invalid_argument", 400],
    ["§B15 closed 非布尔", { object_id: "M1" }, "invalid_argument", 400],
    // **入口校验先于对象查找** ✓：对象不存在**也**先报未知键 ✓。
    ["§B12 入口校验优先", { object_id: "NOPE", foo: 1 }, "invalid_argument", 400],
  ];
  for (const [label, args, code, status] of errors) {
    const [s, l] = await ctx.both("get_path", args);
    compareError(label, s, l, code, status);
  }
  const [sMissing] = await ctx.both("get_path", { object_id: "NOPE" });
  check(detailOf(sMissing) === "对象 NOPE 不存在",
    `§B4 文案不对 ✗ ${show(detailOf(sMissing))}`);
  const [sDeleted] = await ctx.both("get_path", { object_id: "D1" });
  check(detailOf(sDeleted) === "对象 D1 已被删除",
    `§B5 文案不对 ✗ ${show(detailOf(sDeleted))}`);
}
{
  // B13 随层死：删图层 ⇒ 对象墓碑 ⇒ 404「已被删除」（不是「不存在」✗）。
  const ctx = await makeCase("b2", [
    ["create_layer", { layer_id: "V2", kind: "vector", name: "矢量层" }],
    shape("C1", pathGeometry([line([0, 0], [4, 4])]), {}, "V2"),
    ["delete_layer", { layer_id: "V2" }],
  ]);
  const [s, l] = await ctx.both("get_path", { object_id: "C1" });
  compareError("§B13 随层死", s, l, "reference_not_found", 404);
  check(detailOf(s) === "对象 C1 已被删除",
    `§B13 文案应是「已被删除」✗ ${show(detailOf(s))}`);
}

// ══ §C 建对象被撤 ⇒ 404「不存在」 ═══════════════════════════════════════════════
{
  const ctx = await makeCase("c", [
    ["create_layer", { layer_id: "V1", kind: "vector", name: "矢量层" }],
    shape("K1", pathGeometry([line([0, 0], [6, 6])])),
  ], async (c) => {
    const atoms = await c.server("list_atoms", { layer_id: "V1", include_non_stroke: true });
    // **`list_atoms` 不回 `payload`** ✓（只有 `kind`／`layer_id`／`atom_id` ✓）⇒ 按 kind ＋ 图层找 ✓
    //（本 case 该层只有一个 draw_shape ⇒ 唯一 ✓）。
    const createAtom = (atoms.body && atoms.body.atoms || [])
      .find((a) => a.kind === "draw_shape" && a.layer_id === "V1");
    if (!createAtom) {
      console.log("⊘ 不可判：list_atoms 里找不到 draw_shape 原子 ⇒ 前提不成立，不是产品问题");
      process.exit(3);
    }
    const reverted = await c.server("revert_atoms", { atom_ids: [createAtom.atom_id] });
    if (reverted.status !== 200 || !reverted.body || reverted.body.ok === false) {
      console.log(`⊘ 不可判：revert_atoms 被拒（${show(reverted.body)}）⇒ 前提不成立，不是产品问题`);
      process.exit(3);
    }
  });
  const [s, l] = await ctx.both("get_path", { object_id: "K1" });
  compareError("§C 建对象被撤", s, l, "reference_not_found", 404);
  check(detailOf(s) === "对象 K1 不存在",
    `§C 文案应是「不存在」（被中和的建对象）✗ ${show(detailOf(s))}`);
}

// ══ §D 字段集合 ＋ 恒等式 ═════════════════════════════════════════════════════════
{
  const ctx = await makeCase("d", [
    ["create_layer", { layer_id: "V1", kind: "vector", name: "矢量层" }],
    shape("P1", pathGeometry([
      line([10, 10], [90, 10]),
      { start: [20, 20], closed: true,
        segments: [{ op: "C", c1: [30, 40], c2: [50, 40], to: [60, 20] }] },
    ])),
  ]);
  const [s, l] = await ctx.both("get_path", { object_id: "P1" });
  compareBody("§D 字段集合", s, l);
  const keys = Object.keys(strip(s.body || {})).sort();
  check(same(keys, ["bbox", "d", "fill_rule", "geometry", "kind", "layer_id", "length",
    "node_counts", "object_id", "ok", "subpaths"]),
  `§D 顶层字段集合不对 ✗ ${show(keys)}`);
  check(s.body && Array.isArray(s.body.node_counts)
    && s.body.node_counts.length === s.body.subpaths.length,
  `§D node_counts 条数应等于子路径条数 ✗ ${show(s.body && { nc: s.body.node_counts, sp: s.body.subpaths.length })}`);
  check(s.body && same(s.body.geometry && s.body.geometry.subpaths, s.body.subpaths)
    && s.body.geometry && s.body.geometry.kind === "path"
    && s.body.geometry.fill_rule === s.body.fill_rule,
  `§D geometry 应与顶层 subpaths／fill_rule 一致 ✗ ${show(s.body && s.body.geometry)}`);
  check(s.body && s.body.bbox.w >= 0 && s.body.bbox.h >= 0 && s.body.length >= 0,
    `§D bbox 宽高与 length 不应为负 ✗ ${show(s.body && { b: s.body.bbox, l: s.body.length })}`);
  check(s.body && s.body.node_counts.every((n, i) =>
    n === s.body.subpaths[i].segments.length + 1),
  `§D 每条 node_counts 应等于段数 + 1 ✗ ${show(s.body && s.body.node_counts)}`);
}

if (bad.length) {
  console.log(`✗ 失败 ${bad.length} 条（通过 ${okCount}）：`);
  for (const line of bad) console.log("   " + line);
  process.exit(1);
}
console.log(`✓ tool-pwa-get-path：${okCount} 条断言全过（get_path 与服务端同答案 ✓）`);
