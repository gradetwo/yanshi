#!/usr/bin/env node
// **判据：PWA 本地的 `edit_path` 与服务端同答案** ✓（台账「第 六百七十 节」）。
//
// **装配手法 ✓**：前序**只在服务端**跑一遍 ✓ ⇒ 把服务端日志用 `GET /api/atoms` 原样播进本地文档 ✓
//   （`tool-pwa-get-path` 同一手法 ✓）⇒ 两边 `head`／原子 seq／笔画对象**逐位同源** ✓；
//   每个成功编辑都**回读 `get_path`** ✓ —— **写路径与读路径口径分裂**是一条独立的红 ✗。
//
// **它断什么 ✗**（每条都要能红 ✓）：
//   §A `move_node`：锚点移动 ＋ **相连控制柄刚性平移** ✓（`c2`／`c1` 各验一次）、
//      闭合首节点 ✓、越界／`to` 形态错误（少一元／含字符串／**多一元** ✓）；
//   §B `insert_node`：直段拆两段（含 t 缺省 0.5 ✓）、三次段 de Casteljau 切分 ✓、
//      **闭合回首虚拟段**（`segment = 段数` ✓）、越界／`t` 非开区间 ✓；
//   §C `delete_node`：首／末／中间节点 ✓、**混合段合并规则**（直段那侧取被删锚点 ✓）、
//     只剩一段拒绝 ✓；
//   §D `set_handle`：直段升格（`Out` 取 2/3、`In` 取 1/3 ✓）、既有三次段直改 ✓、
//     闭合首节点 `In`／末节点 `Out` 落成显式回首三次段 ✓、开放首/末的 `In`/`Out` 报错 ✓；
//   §E 入口与寻址：缺参／未知算子／**算子白名单先于对象查找** ✓／对象不存在／已删／
//     笔里塞 path 几何／矩形带 subpaths（两道几何守卫各自可观测 ✓）、`subpath` 负下标与越界 ✓；
//   §F 写的是**一条可撤的 `supersede`** ✓：`revert_atoms` 之后 `get_path` 回到编辑前 ✓。
//
// **前提 ✗**：活服务端（`run-criteria-local.sh` ✓）＋ 不过期的真 wasm 内核（旧了 `exit(3)` ✓）。
process.on("uncaughtException", (error) => {
  console.error(String((error && error.stack) || error).slice(0, 1200));
  process.exit(3);
});
import { pathToFileURL } from "node:url";

const BASE = process.argv[2];
if (!BASE || !/^https?:\/\//.test(BASE)) {
  console.log("⊘ 不可判：argv[2] 缺服务端地址 ⇒ 请用 bash scripts/run-criteria-local.sh scripts/tool-pwa-edit-path.mjs 跑"
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
const UNIQ = "e" + Date.now().toString(36);
const SERVER_DOC = `eps-${UNIQ}`;
const LOCAL_DOC = `epl-${UNIQ}`;

const { local: localApi } = await kernelLib.bootLocalWithRealKernel({ origin: BASE });
const nativeFetch = globalThis.fetch;
const storeLib = await import(pathToFileURL(process.cwd() + "/web/store.js").href);

const created = await nativeFetch(`${BASE}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: SERVER_DOC, width: 400, height: 300 }),
}).then((r) => r.json()).catch(() => null);
if (!created || created.ok !== true || !created.token) {
  console.log(`⊘ 不可判：服务端建文档失败（${String(JSON.stringify(created)).slice(0, 200)}）`
    + " ⇒ 前提不成立，不是产品问题");
  process.exit(3);
}
const TOKEN = created.token;

/** **把服务端日志原样播进本地文档 ✓**（`tool-pwa-get-path` 同一手法 ✓）：
 *  **∴ 前序只在服务端跑一次 ✗** ⇒ **∴ 两边从同一段日志出发 ✓** ⇒ `head` 同步 ✓
 *  （**∴ 本地不重跑前序 ⇒ 不会因本地缺内核而少写原子 ✓**）。 */
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

const callServer = async (tool, args) => {
  const res = await nativeFetch(`${BASE}/api/tools/${tool}?token=${TOKEN}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: SERVER_DOC, actor: "human:judge", session: "session:judge", ...args }),
  });
  return { status: res.status, body: await res.json().catch(() => null) };
};
/** **本地抛异常记成产品失败 ✗ 不是判据坏了 ✓**。 */
const localCall = async (tool, args) => {
  try {
    const res = await localApi(new Request(`https://x/api/tools/${tool}?doc=${LOCAL_DOC}`, {
      method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}),
    }));
    return { status: res.status, body: await res.json().catch(() => null) };
  } catch (e) {
    return { status: 0, body: { ok: false, error_code: "local_threw",
      context: { detail: String((e && e.message) || e).slice(0, 300) } } };
  }
};
const both = async (tool, args) => [await callServer(tool, args), await localCall(tool, args)];

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
// **服务端 HTTP 信封 ✗ 不是工具契约** ✓ ⇒ 比对前两边都剥掉 ✓。
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
function compareError(label, srv, loc, status, code) {
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

/** **前序 ✓**：只在**服务端**建（随后整段日志播进本地 ⇒ 两边从同一段出发 ✓）；
 *  服务端拒绝 ⇒ 前提不成立（`exit(3)`）✓。 */
async function setup(label, tool, args) {
  const s = await callServer(tool, args);
  if (s.status !== 200 || !s.body || s.body.ok === false) {
    console.log(`⊘ 不可判：${label} 服务端拒绝（${s.status} ${show(s.body)}）⇒ 前提不成立，不是产品问题`);
    process.exit(3);
  }
  return s;
}

const BLUE = [0, 0, 1, 1];
const line = (to) => ({ op: "L", to });
const cubic = (c1, c2, to) => ({ op: "C", c1, c2, to });
/** **期望值按规范算 ✓**（`lerp_point(a,b,t) = a + (b-a)t` ✓）—— 不从实现抄 ✓。 */
const lerp = (a, b, t) => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
const pathData = (start, segments, closed = false) => ({
  geometry: { kind: "path", subpaths: [{ start, closed, segments }] },
  color: BLUE,
});
const shape = (objectId, data, layerId = "L1") =>
  ({ layer_id: layerId, object_id: objectId, data });

/** **一次成功编辑 ✓**：两边 200 ✓、全字段相等 ✓、恒等式 ✓、`get_path` 回读也相等 ✓；
 *   `expected` 给**判据自己算的期望值** ✓（只比「两边相等」不够 ⇒ 两边同错也绿 ✗）。 */
async function editCase(label, args, expected) {
  const [s, l] = await both("edit_path", args);
  check(s.status === 200 && s.body && s.body.ok === true,
    `${label}：服务端应 200 ✗ 状态 ${s.status}｜${show(s.body)}`);
  check(l.status === 200 && l.body && l.body.ok === true,
    `${label}：本地应 200 ✗ 状态 ${l.status}｜${show(l.body)}`);
  if (!s.body || !l.body) { check(false, `${label}：有一边读不出 JSON ✗`); return null; }
  check(l.body.server === false, `${label}：本地应标 server:false ✗ ${show(l.body.server)}`);
  const vs = strip(s.body), vl = strip(l.body);
  check(same(vs, vl), `${label}：全字段不一致 ✗\n     首个差异 ${firstDiff(vs, vl) || "（结构相等但 same 判不等 ⇒ 数值口径）"}`);
  check(Number.isInteger(l.body.head), `${label}：head 应为整数 ✗ ${show(l.body.head)}`);
  check(Array.isArray(l.body.node_counts) && l.body.node_counts.length === l.body.subpaths.length
    && l.body.node_counts.every((n, i) => n === l.body.subpaths[i].segments.length + 1),
  `${label}：node_counts 应与每条子路径段数 + 1 一致 ✗ ${show(l.body.node_counts)}`);
  check(l.body.bbox && l.body.bbox.w >= 0 && l.body.bbox.h >= 0,
    `${label}：bbox 宽高应非负 ✗ ${show(l.body.bbox)}`);
  for (const [key, want] of Object.entries(expected || {})) {
    check(same(vl[key], want), `${label}：${key} 应为 ${show(want)} ✗ 实测 ${show(vl[key])}`);
  }
  const [gs, gl] = await both("get_path", { object_id: args.object_id });
  compareBody(`${label} 回读`, gs, gl);
  if (gs.body && gl.body) {
    check(same(gl.body.subpaths, l.body.subpaths),
      `${label}：回读 subpaths 与编辑回执不同 ⇒ 写读口径分裂 ✗`);
    check(same(gl.body.d, l.body.d), `${label}：回读 d 与编辑回执不同 ✗`);
    check(same(gl.body.bbox, l.body.bbox), `${label}：回读 bbox 与编辑回执不同 ✗`);
    check(same(gl.body.length, l.body.length), `${label}：回读 length 与编辑回执不同 ✗`);
  }
  return l.body;
}

// ══ 公共前序：一个矢量层 ＋ 各种 path 对象 ═══════════════════════════════════════
await setup("建矢量层", "create_layer", { layer_id: "L1", kind: "vector", name: "矢量层" });
await setup("P1 折线（两段三点）", "draw_shape",
  shape("P1", pathData([10, 10], [line([30, 10]), line([50, 10])])));
await setup("P2 三次段（开放）", "draw_shape",
  shape("P2", pathData([10, 20], [cubic([10, 60], [50, 60], [50, 20])])));
await setup("P3 闭合方形", "draw_shape",
  shape("P3", pathData([0, 0], [line([20, 0]), line([20, 20]), line([0, 20])], true)));
await setup("P4 单段（B3 插点用）", "draw_shape",
  shape("P4", pathData([100, 100], [line([120, 100])])));
await setup("P9 单段（删节点会零段）", "draw_shape",
  shape("P9", pathData([140, 140], [line([160, 140])])));
// **末锚点 ≠ start 的闭合子路径** ✓：P3 在 §D5 之后末锚点已回到 start ⇒ 回首线的 lerp
//   **退化**（两端相等）⇒ 2/3 与 1/3 的差别**测不出来** ✗（首跑变异没红就是这么来的 ✓）。
await setup("P10 闭合（末锚点 ≠ start）", "draw_shape",
  shape("P10", pathData([0, 0], [line([10, 0]), line([10, 10])], true)));
// **类型守卫要能单独观测** ✓：笔的对象类型是 Stroke，但 `data` 里塞的是**合法 path 几何** ⇒
//   只按几何判断就会误读成 200 ✗（与 get_path 判据同一手法 ✓）。
await setup("P11 笔（塞 path 几何）", "draw_stroke",
  { layer_id: "L1", object_id: "P11",
    data: { points: [[0, 0], [10, 10]], size: 4, color: BLUE } });
await setup("P11 塞 path 几何", "replace_object_data", {
  object_id: "P11",
  data: { geometry: { kind: "path",
    subpaths: [{ start: [0, 0], segments: [line([10, 10])] }] } } });
// **`geometry.kind` 守卫同理** ✓：对象是 Shape，但 kind 是 rect ⇒ 只有那道守卫能拒 ✓。
await setup("P12 矩形（塞 subpaths）", "draw_shape",
  { layer_id: "L1", object_id: "P12",
    data: { geometry: { kind: "rect", bbox: { x: 0, y: 0, w: 5, h: 5 } }, color: BLUE } });
await setup("P12 塞 subpaths", "replace_object_data", {
  object_id: "P12",
  data: { geometry: { kind: "rect", bbox: { x: 0, y: 0, w: 5, h: 5 },
    subpaths: [{ start: [0, 0], segments: [line([4, 4])] }] } } });
await setup("P5 矩形（不是 path）", "draw_shape",
  { layer_id: "L1", object_id: "P5",
    data: { geometry: { kind: "rect", bbox: { x: 0, y: 0, w: 5, h: 5 } }, color: BLUE } });
await setup("P6 笔（不是 path）", "draw_stroke",
  { layer_id: "L1", object_id: "P6",
    data: { points: [[0, 0], [10, 10]], size: 4, color: BLUE } });
await setup("P7 待删", "draw_shape", shape("P7", pathData([0, 40], [line([20, 40])])));
await setup("删 P7", "delete_object", { object_id: "P7" });
await setup("P8 待编辑", "draw_shape", shape("P8", pathData([0, 70], [line([20, 70])])));

// **前序播进本地 ✓**：此后两边日志同源 ✓（`head` 与原子顺序都对得上 ✓）。
const seeded = await seedLocal(SERVER_DOC, TOKEN, LOCAL_DOC);
check(seeded > 0, `前序日志应已播进本地 ✗ ${show(seeded)} 条`);

// ══ §A move_node ════════════════════════════════════════════════════════════════
{
  // A1 中间节点：P1 节点 1 移到 (35, 25) ⇒ 相邻两段都是直段 ⇒ 只改终点。
  const body = await editCase("§A1 中间节点", {
    object_id: "P1", op: "move_node", subpath: 0, node: 1, to: [35, 25],
  }, {
    subpaths: [{ start: [10, 10], closed: false,
                 segments: [line([35, 25]), line([50, 10])] }],
    d: "M 10 10 L 35 25 L 50 10",
    node_counts: [3],
  });
  check(body && same(body.bbox, { x: 10, y: 10, w: 40, h: 15 }),
    `§A1 bbox 应为控制点盒 10/10/40/15 ✗ ${show(body && body.bbox)}`);
  // A2 三次段末端节点：P2 节点 1 移到 (60, 30) ⇒ 前一段的 c2 按同一位移平移。
  await editCase("§A2 三次段末端", {
    object_id: "P2", op: "move_node", subpath: 0, node: 1, to: [60, 30],
  }, {
    subpaths: [{ start: [10, 20], closed: false,
                 segments: [cubic([10, 60], [60, 70], [60, 30])] }],
  });
  // A3 三次段首节点：P2 节点 0 移到 (20, 20) ⇒ 后一段的 c1 平移 (+10, 0)。
  await editCase("§A3 三次段首节点", {
    object_id: "P2", op: "move_node", subpath: 0, node: 0, to: [20, 20],
  }, {
    subpaths: [{ start: [20, 20], closed: false,
                 segments: [cubic([20, 60], [60, 70], [60, 30])] }],
  });
  // A4 闭合子路径首节点：P3 节点 0 移到 (-5, -5)；闭合标志与段数不变。
  await editCase("§A4 闭合首节点", {
    object_id: "P3", op: "move_node", subpath: 0, node: 0, to: [-5, -5],
  }, {
    subpaths: [{ start: [-5, -5], closed: true,
                 segments: [line([20, 0]), line([20, 20]), line([0, 20])] }],
  });
  const [s5, l5] = await both("edit_path", {
    object_id: "P1", op: "move_node", subpath: 0, node: 9, to: [0, 0],
  });
  compareError("§A5 节点越界", s5, l5, 400, "invalid_argument");
  const [s6, l6] = await both("edit_path", {
    object_id: "P1", op: "move_node", subpath: 0, node: 1, to: [1],
  });
  compareError("§A6 to 不是二元", s6, l6, 400, "invalid_argument");
  const [s7, l7] = await both("edit_path", {
    object_id: "P1", op: "move_node", subpath: 0, node: 1, to: ["1", 2],
  });
  compareError("§A7 to 含字符串", s7, l7, 400, "invalid_argument");
  const [s8, l8] = await both("edit_path", {
    object_id: "P1", op: "move_node", subpath: 0, to: [1, 2],
  });
  compareError("§A8 缺 node", s8, l8, 400, "invalid_argument");
  // **三元 `to`** ✓：`[1]` 少一个时两边都靠「第二个不是有限数」拒掉 ⇒ **测不出两元检查** ✗；
  //   多一个才只有 `length === 2` 能拒 ✓（首跑变异没红就是这么来的 ✓）。
  const [s9, l9] = await both("edit_path", {
    object_id: "P1", op: "move_node", subpath: 0, node: 1, to: [1, 2, 3],
  });
  compareError("§A9 to 三元", s9, l9, 400, "invalid_argument");
}

// ══ §B insert_node ══════════════════════════════════════════════════════════════
{
  // B1 直段 t=0.25：P1 段 0（[10,10] → [35,25]，A1 已挪过）在 (16.25,13.75) 处拆两段。
  await editCase("§B1 直段插点", {
    object_id: "P1", op: "insert_node", subpath: 0, segment: 0, t: 0.25,
  }, {
    new_node: 1,
    subpaths: [{ start: [10, 10], closed: false,
                 segments: [line(lerp([10, 10], [35, 25], 0.25)),
                            line([35, 25]), line([50, 10])] }],
    node_counts: [4],
  });
  // B2 三次段 t=0.3：de Casteljau 切分（几何不变 ⇒ 总长不变）。
  const before = (await both("get_path", { object_id: "P2" }))[0].body;
  await editCase("§B2 三次段插点", {
    object_id: "P2", op: "insert_node", subpath: 0, segment: 0, t: 0.3,
  }, { new_node: 1, node_counts: [3] });
  const after = (await both("get_path", { object_id: "P2" }))[0].body;
  // **∴ 容差 ✗**：切分后**两段各自展平**的采样点不同 ⇒ 折线总长会差 ~1e-5 相对量 ✓
  //   （**∴ 几何同一 ✓ 不是逐字节同一 ✓**）⇒ 断「几乎不变」✓ 不断「逐位相等」✗。
  check(before && after && Math.abs(before.length - after.length) <= 1e-4 * Math.max(1, before.length),
    `§B2 切分是同一曲线 ⇒ 总长几乎不变 ✗ ${show(before && before.length)} → ${show(after && after.length)}`);
  // B3 t 缺省 0.5：P4 直段中点 (110,100)。
  await editCase("§B3 缺省 t=0.5", {
    object_id: "P4", op: "insert_node", subpath: 0, segment: 0,
  }, {
    new_node: 1,
    subpaths: [{ start: [100, 100], closed: false,
                 segments: [line([110, 100]), line([120, 100])] }],
  });
  // B4 闭合回首虚拟段（segment = 段数）：P3 从末节点 [0,20] 回首 [start]=[-5,-5]（A4 挪过）
  //   ⇒ 追加显式直段到中点 [-2.5,7.5]。
  await editCase("§B4 闭合回首段", {
    object_id: "P3", op: "insert_node", subpath: 0, segment: 3, t: 0.5,
  }, {
    new_node: 4,
    subpaths: [{ start: [-5, -5], closed: true,
                 segments: [line([20, 0]), line([20, 20]), line([0, 20]),
                            line(lerp([0, 20], [-5, -5], 0.5))] }],
    node_counts: [5],
  });
  const [s5, l5] = await both("edit_path", {
    object_id: "P4", op: "insert_node", subpath: 0, segment: 2, t: 0.5,
  });
  compareError("§B5 段越界", s5, l5, 400, "invalid_argument");
  const [s6, l6] = await both("edit_path", {
    object_id: "P4", op: "insert_node", subpath: 0, segment: 0, t: 1,
  });
  compareError("§B6 t=1", s6, l6, 400, "invalid_argument");
  const [s7, l7] = await both("edit_path", {
    object_id: "P4", op: "insert_node", subpath: 0, segment: 0, t: 0,
  });
  compareError("§B7 t=0", s7, l7, 400, "invalid_argument");
  const [s8, l8] = await both("edit_path", {
    object_id: "P4", op: "insert_node", subpath: 0, t: 0.5,
  });
  compareError("§B8 缺 segment", s8, l8, 400, "invalid_argument");
}

// ══ §C delete_node ══════════════════════════════════════════════════════════════
{
  // C1 中间节点（直＋直 ⇒ 直）：P1 的节点 1（B1 插的 (16.25,13.75)）删掉 ⇒ 与节点 2 合并成 (35,25)。
  await editCase("§C1 直直合并", {
    object_id: "P1", op: "delete_node", subpath: 0, node: 1,
  }, {
    subpaths: [{ start: [10, 10], closed: false,
                 segments: [line([35, 25]), line([50, 10])] }],
    node_counts: [3],
  });
  // C2 首节点：P3 节点 0 删掉 ⇒ 新 start = (20,0)，段数 4 → 3。
  await editCase("§C2 删首节点", {
    object_id: "P3", op: "delete_node", subpath: 0, node: 0,
  }, {
    subpaths: [{ start: [20, 0], closed: true,
                 segments: [line([20, 20]), line([0, 20]),
                            line(lerp([0, 20], [-5, -5], 0.5))] }],
    node_counts: [4],
  });
  // C3 末节点：P3 节点 3 删掉 ⇒ 丢掉最后一段。
  await editCase("§C3 删末节点", {
    object_id: "P3", op: "delete_node", subpath: 0, node: 3,
  }, {
    subpaths: [{ start: [20, 0], closed: true,
                 segments: [line([20, 20]), line([0, 20])] }],
    node_counts: [3],
  });
  // C4 混合合并（入三出三，P2 在 §D 之前只有 B2 切出的两段）：
  //   节点 1 ([28.64,47.36]) 删掉 ⇒ c1 取入段 c1 [20,32]，c2 取出段 c2 [60,58]，to [60,30]。
  await editCase("§C4 三三合并", {
    object_id: "P2", op: "delete_node", subpath: 0, node: 1,
  }, {
    subpaths: [{ start: [20, 20], closed: false,
                 segments: [cubic([20, 32], [60, 58], [60, 30])] }],
    node_counts: [2],
  });
  const [s5, l5] = await both("edit_path", {
    object_id: "P9", op: "delete_node", subpath: 0, node: 0,
  });
  compareError("§C5 只剩一段拒绝", s5, l5, 400, "invalid_argument");
  const [s6, l6] = await both("edit_path", {
    object_id: "P1", op: "delete_node", subpath: 0, node: 5,
  });
  compareError("§C6 节点越界", s6, l6, 400, "invalid_argument");
}

// ══ §D set_handle ═══════════════════════════════════════════════════════════════
{
  // D1 直段 Out 升格：P4 节点 0 out → (110,90)（B3 之后段 0 = [100,100] → [110,100]）
  //   ⇒ c2 取段上 2/3 点。
  await editCase("§D1 直段 Out 升格", {
    object_id: "P4", op: "set_handle", subpath: 0, node: 0, side: "out", to: [110, 90],
  }, {
    subpaths: [{ start: [100, 100], closed: false,
                 segments: [cubic([110, 90], lerp([100, 100], [110, 100], 2 / 3), [110, 100]),
                            line([120, 100])] }],
  });
  // D2 直段 In 升格：P4 节点 2 in → (100,110)（段 1 = [110,100] → [120,100]）
  //   ⇒ c1 取段上 1/3 点，c2 = to。
  await editCase("§D2 直段 In 升格", {
    object_id: "P4", op: "set_handle", subpath: 0, node: 2, side: "in", to: [100, 110],
  }, {
    subpaths: [{ start: [100, 100], closed: false,
                 segments: [cubic([110, 90], lerp([100, 100], [110, 100], 2 / 3), [110, 100]),
                            cubic(lerp([110, 100], [120, 100], 1 / 3), [100, 110], [120, 100])] }],
  });
  // D3 既有三次段直改：C4 之后 P2 只剩一段（节点 0 / 1）⇒ 节点 0 out 改段 0 的 c1。
  await editCase("§D3 三次段 Out 直改", {
    object_id: "P2", op: "set_handle", subpath: 0, node: 0, side: "out", to: [20, 50],
  }, {
    subpaths: [{ start: [20, 20], closed: false,
                 segments: [cubic([20, 50], [60, 58], [60, 30])] }],
  });
  // D4 末节点 in ⇒ 改末段（段 0）的 c2。
  await editCase("§D4 三次段 In 直改", {
    object_id: "P2", op: "set_handle", subpath: 0, node: 1, side: "in", to: [55, 65],
  }, {
    subpaths: [{ start: [20, 20], closed: false,
                 segments: [cubic([20, 50], [55, 65], [60, 30])] }],
  });
  // D5 闭合首节点 In：P3 落成显式回首三次段（终点 = start，c1 取回首线 1/3 点）。
  await editCase("§D5 闭合首节点 In", {
    object_id: "P3", op: "set_handle", subpath: 0, node: 0, side: "in", to: [10, -5],
  }, {
    subpaths: [{ start: [20, 0], closed: true,
                 segments: [line([20, 20]), line([0, 20]),
                            cubic(lerp([0, 20], [20, 0], 1 / 3), [10, -5], [20, 0])] }],
    node_counts: [4],
  });
  // D6 闭合末节点 Out：P10 是**末锚点 ≠ start** 的闭合子路径 ⇒ 回首线不退化，2/3 点可观测。
  await editCase("§D6 闭合末节点 Out", {
    object_id: "P10", op: "set_handle", subpath: 0, node: 2, side: "out", to: [5, 5],
  }, {
    subpaths: [{ start: [0, 0], closed: true,
                 segments: [line([10, 0]), line([10, 10]),
                            cubic([5, 5], lerp([10, 10], [0, 0], 2 / 3), [0, 0])] }],
    node_counts: [4],
  });
  const [s7, l7] = await both("edit_path", {
    object_id: "P2", op: "set_handle", subpath: 0, node: 0, side: "in", to: [0, 0],
  });
  compareError("§D7 开放首节点无 in", s7, l7, 400, "invalid_argument");
  const [s8, l8] = await both("edit_path", {
    object_id: "P2", op: "set_handle", subpath: 0, node: 1, side: "out", to: [0, 0],
  });
  compareError("§D8 开放末节点无 out", s8, l8, 400, "invalid_argument");
  const [s9, l9] = await both("edit_path", {
    object_id: "P2", op: "set_handle", subpath: 0, node: 0, side: "left", to: [0, 0],
  });
  compareError("§D9 side 非法", s9, l9, 400, "invalid_argument");
  const [s10, l10] = await both("edit_path", {
    object_id: "P2", op: "set_handle", subpath: 0, node: 0, to: [0, 0],
  });
  compareError("§D10 缺 side", s10, l10, 400, "invalid_argument");
}

// ══ §E 入口与寻址 ═══════════════════════════════════════════════════════════════
{
  const cases = [
    ["§E1 缺 object_id", { op: "move_node", subpath: 0, node: 0, to: [0, 0] }, 400, "invalid_argument"],
    ["§E2 缺 op", { object_id: "P1", subpath: 0, node: 0, to: [0, 0] }, 400, "invalid_argument"],
    ["§E3 未知算子", { object_id: "P1", op: "explode", subpath: 0, node: 0, to: [0, 0] }, 400, "invalid_argument"],
    ["§E4 未知键", { object_id: "P1", op: "move_node", subpath: 0, node: 0, to: [0, 0], foo: 1 }, 400, "invalid_argument"],
    ["§E5 subpath 越界", { object_id: "P1", op: "move_node", subpath: 3, node: 0, to: [0, 0] }, 400, "invalid_argument"],
    ["§E6 subpath 负下标", { object_id: "P1", op: "move_node", subpath: -1, node: 0, to: [0, 0] }, 400, "invalid_argument"],
    ["§E7 对象不存在", { object_id: "NOPE", op: "move_node", subpath: 0, node: 0, to: [0, 0] }, 404, "reference_not_found"],
    ["§E8 不是 path（矩形）", { object_id: "P5", op: "move_node", subpath: 0, node: 0, to: [0, 0] }, 400, "invalid_argument"],
    ["§E9 不是 path（笔）", { object_id: "P6", op: "move_node", subpath: 0, node: 0, to: [0, 0] }, 400, "invalid_argument"],
    // **入口校验先于一切** ✓：缺 subpath + 对象不存在 ⇒ 仍报缺参（不是 404）。
    ["§E10 入口校验优先", { object_id: "NOPE", op: "move_node", node: 0, to: [0, 0] }, 400, "invalid_argument"],
    // **算子白名单先于对象查找** ✓：对象不存在 + 未知算子 ⇒ 报未知算子。
    ["§E11 算子白名单优先", { object_id: "NOPE", op: "explode", subpath: 0, node: 0, to: [0, 0] }, 400, "invalid_argument"],
  ];
  for (const [label, args, status, code] of cases) {
    const [s, l] = await both("edit_path", args);
    compareError(label, s, l, status, code);
  }
  const [sr, lr] = await both("edit_path", {
    object_id: "NOPE", op: "explode", subpath: 0, node: 0, to: [0, 0],
  });
  check(detailOf(sr) === detailOf(lr) && /未知的 edit_path 算子 explode/.test(String(detailOf(sr))),
    `§E11 文案应是未知算子 ✗ ${show(detailOf(sr))}`);
  // **已删对象 ⇒ 404「已被删除」** ✓（P7 在前序里建好又删掉 ✓）。
  const [sd, ld] = await both("edit_path", {
    object_id: "P7", op: "move_node", subpath: 0, node: 0, to: [0, 0],
  });
  compareError("§E12 对象已删", sd, ld, 404, "reference_not_found");
  // **两道几何守卫各自要能单独观测** ✓：P6／P5 的 `data` 里没有 path 几何 ⇒ 下游解析也会拒 ✗
  //   ⇒ 关掉守卫**看不出差别** ✗（首跑变异没红就是这么来的 ✓）⇒ 用注入几何的 P11／P12 ✓。
  const [st, lt] = await both("edit_path", {
    object_id: "P11", op: "move_node", subpath: 0, node: 0, to: [0, 0],
  });
  compareError("§E13 笔里塞 path 几何", st, lt, 400, "invalid_argument");
  const [sk, lk] = await both("edit_path", {
    object_id: "P12", op: "move_node", subpath: 0, node: 0, to: [0, 0],
  });
  compareError("§E14 矩形带 subpaths", sk, lk, 400, "invalid_argument");
}

// ══ §F 写的是「一条可撤的 supersede」 ═══════════════════════════════════════════
{
  const [beforeS, beforeL] = await both("get_path", { object_id: "P8" });
  compareBody("§F 编辑前", beforeS, beforeL);
  await editCase("§F 一次编辑", {
    object_id: "P8", op: "move_node", subpath: 0, node: 1, to: [30, 70],
  }, {});
  // **`list_atoms` 是倒序（服务端 `log().atoms().iter().rev()` ✓）** ⇒ 从**头**找最新 ✓。
  const lastSupersede = async (side) => {
    const args = { include_non_stroke: true, limit: 5000 };
    const atoms = side === "server" ? await callServer("list_atoms", args)
      : await localCall("list_atoms", args);
    const list = (atoms.body && atoms.body.atoms) || [];
    for (const atom of list) if (atom.kind === "supersede") return atom.atom_id;
    return null;
  };
  const atomIdS = await lastSupersede("server");
  const atomIdL = await lastSupersede("local");
  check(typeof atomIdS === "string" && typeof atomIdL === "string",
    `§F 应能找到 supersede 原子 ✗ 服务端 ${show(atomIdS)}｜本地 ${show(atomIdL)}`);
  if (typeof atomIdS === "string" && typeof atomIdL === "string") {
    const [rs, rl] = await Promise.all([
      callServer("revert_atoms", { atom_ids: [atomIdS] }),
      localCall("revert_atoms", { atom_ids: [atomIdL] }),
    ]);
    check(rs.status === 200 && rs.body && rs.body.ok === true
      && rs.body.reverted_count === 1 && rs.body.failed_count === 0,
    `§F 服务端 revert 应成功 ✗ ${rs.status}｜${show(rs.body)}`);
    check(rl.status === 200 && rl.body && rl.body.ok === true
      && rl.body.reverted_count === 1 && rl.body.failed_count === 0,
    `§F 本地 revert 应成功 ✗ ${rl.status}｜${show(rl.body)}`);
    const [gs, gl] = await both("get_path", { object_id: "P8" });
    compareBody("§F 撤销后读回", gs, gl);
    check(gs.body && beforeS.body && same(strip(gs.body), strip(beforeS.body)),
      `§F 撤销后应回到编辑前 ✗ ${show(gs.body)}`);
  }
}

// ══ §G 字段集合 ═════════════════════════════════════════════════════════════════
{
  const [s, l] = await both("get_path", { object_id: "P1" });
  compareBody("§G 字段集合", s, l);
  const keys = Object.keys(strip(s.body || {})).sort();
  check(same(keys, ["bbox", "d", "fill_rule", "geometry", "kind", "layer_id", "length",
    "node_counts", "object_id", "ok", "subpaths"]),
  `§G get_path 顶层字段集合不对 ✗ ${show(keys)}`);
}

if (bad.length) {
  console.log(`✗ 失败 ${bad.length} 条（通过 ${okCount}）：`);
  for (const line of bad) console.log("   " + line);
  process.exit(1);
}
console.log(`✓ tool-pwa-edit-path：${okCount} 条断言全过（edit_path 与服务端同答案 ✓）`);
