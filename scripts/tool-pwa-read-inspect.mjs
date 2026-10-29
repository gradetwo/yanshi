#!/usr/bin/env node
// **判据：PWA 本地的「读测三端点」与服务端**同一请求、同一答案** ✓**（台账「第 六百六十五 节」）。
//
// **∴ 它测什么 ✗**（**内核 ①：断言必须能证明自己能红 ✓**）：
//   同一个请求分别打到真服务端与本地 `api-local` ✓ ⇒ 比解析后的 JSON ✓（断结构不断字节 ✓）。
//   **∴ 每一面都有判据自己算的那一份 ✗**（**∴ 不能只断「两边相等」**：两边同错时相等照样绿 ✓）：
//   ① **`list_effects` 全字段与顺序**：`(layer_id, z_index, object_id)` 排序 ✓、
//      隐藏**对象**（`visible:false`）与禁用对象（`enabled:false`）**都要在** ✓、
//      `layer_id` 过滤（字符串／非字符串／不存在的层 ✓）✓；
//   ② **`sample_colors` 一次区域渲染 ＋ 画布外**：区域按**夹到画布内**的包围盒 ✓
//      （**§27 的 stride 坑 ✓**：同批里有一个画布外的远点 ⇒ 画布内的点仍要读出颜色 ✓）、
//      `rgba:null` ＋ 警告逐字 ✓、六个参数错误逐字 ✓、小数坐标取 `floor` ✓；
//   ③ **`get_histogram` 四个作用域**：整文档／选区／图层／图层×选区 ✓；
//      通道与亮度桶**逐桶相等** ✓、摘要按桶**独立重算** ✓（恒等式，不是快照 ✓）；
//   ④ **图层作用域的三处补偿**（**内核没有单层渲染通道 ⇒ 本地用合成原子 ✓**）：
//      ① 隐藏层也要画（`visible:true` ✓ —— H1）；
//      ② 组内层**不继承组不透明度**（`parent_id:null` ✓ —— L2 在 `G1{opacity:0.5}` 里，
//         **不补偿 ⇒ alpha 128 ⇒ 红 ✓**）；
//      ③ 纯调整层**不播种下方**（`clipping_mask:true` ✓ —— L3 的 invert 若播种 ⇒ 背景被反相）；
//      ④ 已删层 ⇒ 只剩背景 ✓；
//   ⑤ **合成原子不落盘** ✓：跑完图层作用域后比较两边日志条数与层级 `set_property` 计数 ✓
//      —— **∴ 若合成原子被持久化 ⇒ 「读」改了文档 ✗ ⇒ 红 ✓**（**这是「读不带副作用」的断言 ✓**）；
//   ⑥ **错误逐字**：不存在的层 404（`context.layer_id` ✓）／region 非法 400／
//      region 与画布不相交 400（**逐字 Bbox Debug ✓**）✓。
//
// **∴ 前提 ✗**：需要**活着的服务端**（`run-criteria-local.sh` 起一个并把地址放进 argv[2] ✓）
//   ＋ **不过期的真 wasm 内核**（旧了就 `exit(3)` ✓，**前提不成立 ≠ 产品失败 ✓**）。
//
// **∴ 本地多出的诚实字段**（`server:false`）**不比** ✗ —— 它是「本地做到了什么」的说明 ✓；
//   **共有的业务字段一律逐字比** ✓。
process.on("uncaughtException", (error) => {
  console.error(String(error && error.stack || error).slice(0, 1200));
  process.exit(3);
});
import { pathToFileURL } from "node:url";

const BASE = process.argv[2];
if (!BASE || !/^https?:\/\//.test(BASE)) {
  console.log("⊘ 不可判：argv[2] 缺服务端地址 ⇒ 请用 bash scripts/run-criteria-local.sh scripts/tool-pwa-read-inspect.mjs 跑"
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
const UNIQ = "r" + Date.now().toString(36);
const SERVER_DOC = `rsrv-${UNIQ}`;
const LOCAL_DOC = `rloc-${UNIQ}`;
const DOC_W = 96, DOC_H = 72;

const { local: localApi } = await kernelLib.bootLocalWithRealKernel({ origin: BASE });
const nativeFetch = globalThis.fetch;

const createdBody = await nativeFetch(`${BASE}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: SERVER_DOC, width: DOC_W, height: DOC_H }),
}).then((r) => r.json());
if (createdBody.ok !== true || !createdBody.token) {
  console.log(`⊘ 不可判：服务端建文档失败（${JSON.stringify(createdBody).slice(0, 160)}）⇒ 前提不成立，不是产品问题`);
  process.exit(3);
}
const TOKEN = createdBody.token;

const callServer = async (tool, args) => {
  const res = await nativeFetch(`${BASE}/api/tools/${tool}?token=${TOKEN}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: SERVER_DOC, actor: "human:judge", session: "session:judge", ...args }),
  });
  return { status: res.status, body: await res.json().catch(() => null) };
};
/** **`get_log` 的 `actor` 筛选**（`callServer` 会注入 `actor:"human:judge"` ⇒ 0 行 ✓）⇒ 读日志走裸调用 ✓。 */
const callServerBare = async (tool, args) => {
  const res = await nativeFetch(`${BASE}/api/tools/${tool}?token=${TOKEN}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: SERVER_DOC, ...args }),
  });
  return { status: res.status, body: await res.json().catch(() => null) };
};
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

const localCreated = await localApi(new Request(`https://x/api/documents?doc=${LOCAL_DOC}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: LOCAL_DOC, width: DOC_W, height: DOC_H }),
})).then((r) => r.json().catch(() => null));
if (!localCreated || localCreated.ok !== true) {
  console.log(`⊘ 不可判：本地建文档失败（${JSON.stringify(localCreated)?.slice(0, 160)}）⇒ 前提不成立，不是产品问题`);
  process.exit(3);
}

/** **比对器 ✓**（与 `tool-pwa-pixel-stats` 同一口径：断结构不断字节 ✓）。 */
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
const show = (value) => String(JSON.stringify(value)).slice(0, 360);
/** **首个差异路径 ✓**（`show` 截断到 360 字符 ⇒ 桶数组的差异会被吞掉 ⇒ 用它定位 ✓）。 */
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
const LOCAL_ONLY = ["server"];
const SERVER_ENVELOPE = ["created", "opened", "timings"];
const stripLocal = (body) => {
  const out = { ...(body || {}) };
  for (const key of LOCAL_ONLY) delete out[key];
  for (const key of SERVER_ENVELOPE) delete out[key];
  return out;
};
const detailOf = (res) => (res.body && res.body.context && res.body.context.detail) ?? null;

/** **公共前序：服务端拒绝 ⇒ 前提不成立（`exit(3)`）✓；本地拒绝 ⇒ 产品失败 ✓**。 */
async function setup(label, tool, args) {
  const [s, l] = await both(tool, args);
  if (s.status !== 200 || !s.body || s.body.ok === false) {
    console.log(`⊘ 不可判：${label} 服务端拒绝（${s.status} ${show(s.body)}）⇒ 前提不成立，不是产品问题`);
    process.exit(3);
  }
  check(l.status === 200 && l.body && l.body.ok !== false,
    `${label}：本地被拒 ✗ 状态 ${l.status}｜${show(l.body)}`);
  return [s, l];
}
/** **错误逐字比 ✓**（期望文本可分开给：两边应当相同 ✓）。 */
function compareError(label, srv, loc, status, code, expected) {
  if (!srv.body || !loc.body) { check(false, `${label}：有一边读不出 JSON ✗`); return; }
  check(srv.status === status, `${label}：服务端状态码应为 ${status} ✗ 实测 ${srv.status}｜${show(srv.body)}`);
  check(loc.status === status, `${label}：本地状态码应为 ${status} ✗ 实测 ${loc.status}｜${show(loc.body)}`);
  check(srv.body.error_code === code, `${label}：服务端错误码应为 ${code} ✗ 实测 ${srv.body.error_code} ⇒ 判据口径要跟着权威改`);
  check(loc.body.error_code === code, `${label}：本地错误码应为 ${code} ✗ 实测 ${loc.body.error_code}`);
  check(detailOf(srv) === detailOf(loc),
    `${label}：两边文案不同 ✗\n     服务端 ${detailOf(srv)}\n     本地   ${detailOf(loc)}`);
  if (expected !== undefined) {
    check(detailOf(loc) === expected, `${label}：本地文案与判据的期望不同 ✗\n     期望 ${expected}\n     实测 ${detailOf(loc)}`);
  }
}
/** **成功响应比对 ✓**：共有字段逐字相等 ＋ 本地如实标 `server:false`。 */
function compareBody(label, srv, loc) {
  check(srv.status === 200 && srv.body && srv.body.ok !== false,
    `${label}：服务端应 200 ✗ 状态 ${srv.status}｜${show(srv.body)}`);
  check(loc.status === 200 && loc.body && loc.body.ok !== false,
    `${label}：本地应 200 ✗ 状态 ${loc.status}｜${show(loc.body)}`);
  if (!srv.body || !loc.body) return;
  check(loc.body.server === false, `${label}：本地应标 server:false ✗ ${show(loc.body.server)}`);
  const vs = stripLocal(srv.body), vl = stripLocal(loc.body);
  check(same(vs, vl), `${label}：全字段不一致 ✗\n     首个差异 ${firstDiff(vs, vl) || "（结构相等但 same 判不等 ⇒ 数值口径）"}`);
}
const RECT = (x, y, w, h, color) => ({ geometry: { kind: "rect", bbox: { x, y, w, h } }, color });

// ══ 前序：一层一对象，覆盖「组内层／隐藏层／已删层／纯调整层／纯滤镜层」════════════
// **∴ 层序（z 升序）✗**：L6(0) L1(1) G1(2) L2(3) L3(4) L4(5) H1(6) L5(7 已删)
//   ⇒ **∴ L3 在 L1 之上 ✓** ⇒ 它对下方合成做 invert ⇒ 红色矩形会变成青色 ✓（判据自己算 ✓）。
//   ⇒ **∴ L6 在最下 ✓** ⇒ 它的 noise 只作用在背景上 ✓（不污染红色矩形 ✓）。
const SETUP = [
  ["建底层的纯滤镜层 L6", "create_layer", { layer_id: "L6", name: "噪声层" }],
  ["L6 加禁止用滤镜 F2", "add_filter", { layer_id: "L6", object_id: "F2", name: "noise-F2",
    filter_name: "noise", params: { amount: 0.2, seed: 5 } }],
  ["建 L1", "create_layer", { layer_id: "L1", name: "红层" }],
  ["画红色矩形 S1", "draw_shape", { layer_id: "L1", object_id: "S1",
    data: RECT(4, 4, 16, 16, [1, 0, 0, 1]) }],
  ["L1 加**禁用**的亮度对比 A0", "add_adjustment", { layer_id: "L1", object_id: "A0", name: "bc-A0",
    adjustment_type: "brightness_contrast", params: { brightness: 0.1, contrast: 1.2 } }],
  ["禁用 A0", "set_property", { object_id: "A0", key: "enabled", value: false }],
  ["建组 G1", "create_layer", { layer_id: "G1", name: "组", type: "layer_group" }],
  ["建组内层 L2", "create_layer", { layer_id: "L2", name: "蓝层", parent_id: "G1" }],
  ["画蓝色矩形 S2", "draw_shape", { layer_id: "L2", object_id: "S2",
    data: RECT(20, 10, 12, 12, [0, 0, 1, 1]) }],
  ["组不透明度 0.5", "set_property", { layer_id: "G1", key: "opacity", value: 0.5 }],
  ["建纯调整层 L3", "create_layer", { layer_id: "L3", name: "反相层" }],
  ["L3 加反相 A1", "add_adjustment", { layer_id: "L3", object_id: "A1", name: "inv-A1",
    adjustment_type: "invert" }],
  ["L3 加海报化 A2", "add_adjustment", { layer_id: "L3", object_id: "A2", name: "post-A2",
    adjustment_type: "posterize", params: { levels: 4 } }],
  ["隐藏 A2（对象级）", "set_property", { object_id: "A2", key: "visible", value: false }],
  ["建 L4", "create_layer", { layer_id: "L4", name: "绿层" }],
  ["画绿色矩形 S4", "draw_shape", { layer_id: "L4", object_id: "S4",
    data: RECT(32, 32, 10, 10, [0, 1, 0, 1]) }],
  ["L4 加**禁用**的高斯模糊 F1", "add_filter", { layer_id: "L4", object_id: "F1", name: "blur-F1",
    filter_name: "gaussian_blur", params: { sigma: 2.0 } }],
  ["禁用 F1", "set_property", { object_id: "F1", key: "enabled", value: false }],
  ["建隐藏层 H1", "create_layer", { layer_id: "H1", name: "隐藏层" }],
  ["画黄色矩形 S5", "draw_shape", { layer_id: "H1", object_id: "S5",
    data: RECT(50, 40, 8, 8, [1, 1, 0, 1]) }],
  ["隐藏图层 H1", "set_property", { layer_id: "H1", key: "visible", value: false }],
  ["建将删层 L5", "create_layer", { layer_id: "L5", name: "临时层" }],
  ["画青色矩形 S6", "draw_shape", { layer_id: "L5", object_id: "S6",
    data: RECT(60, 50, 6, 6, [0, 1, 1, 1]) }],
  ["删层 L5", "delete_layer", { layer_id: "L5" }],
];
for (const [label, tool, args] of SETUP) await setup(label, tool, args);

// ══ §A `list_effects`：全字段、顺序、隐藏/禁用、过滤 ═════════════════════════════
{
  const [s, l] = await both("list_effects", {});
  compareBody("§A list_effects 全表", s, l);
  const body = s.body || {};
  const effects = Array.isArray(body.effects) ? body.effects : [];
  check(body.count === effects.length,
    `§A count 应等于 effects 条数 ✗ ${show([body.count, effects.length])}`);
  const ids = effects.map((e) => e.object_id);
  check(same(ids, ["A0", "A1", "A2", "F1", "F2"]),
    `§A 顺序应为 (layer_id, z, id) ⇒ [A0,A1,A2,F1,F2] ✗ 实测 ${show(ids)}`);
  const types = effects.map((e) => e.type);
  check(same(types, ["adjustment", "adjustment", "adjustment", "filter", "filter"]),
    `§A type 序列 ✗ 实测 ${show(types)}`);
  const enabled = effects.map((e) => e.enabled);
  check(same(enabled, [false, true, true, false, true]),
    `§A enabled 序列应为 [false,true,true,false,true]（A0/F1 被禁用、A2 只是隐藏 ✓）✗ 实测 ${show(enabled)}`);
  check(ids.includes("A2"), `§A 隐藏对象 A2（visible:false）仍必须出现在 list_effects ✗`);
  check(ids.includes("F1"), `§A 禁用对象 F1 仍必须出现在 list_effects ✗`);
  const by = Object.fromEntries(effects.map((e) => [e.object_id, e]));
  check(by.A0 && by.A0.adjustment_type === "brightness_contrast" && by.A0.filter_name === null
      && same(by.A0.params, { brightness: 0.1, contrast: 1.2 }) && by.A0.name === "bc-A0"
      && by.A0.opacity === 1,
    `§A A0 字段不对 ✗ ${show(by.A0)}`);
  check(by.A1 && by.A1.adjustment_type === "invert" && same(by.A1.params, {}) && by.A1.layer_id === "L3",
    `§A A1 字段不对 ✗ ${show(by.A1)}`);
  check(by.A2 && by.A2.adjustment_type === "posterize" && same(by.A2.params, { levels: 4 }),
    `§A A2 字段不对 ✗ ${show(by.A2)}`);
  check(by.F1 && by.F1.filter_name === "gaussian_blur" && by.F1.adjustment_type === null
      && same(by.F1.params, { sigma: 2 }) && by.F1.layer_id === "L4",
    `§A F1 字段不对 ✗ ${show(by.F1)}`);
  check(by.F2 && by.F2.filter_name === "noise" && same(by.F2.params, { amount: 0.2, seed: 5 }),
    `§A F2 字段不对 ✗ ${show(by.F2)}`);
  const keys = effects.length ? Object.keys(effects[0]).sort() : [];
  check(same(keys, ["adjustment_type", "enabled", "filter_name", "layer_id", "name",
    "object_id", "opacity", "params", "type"]),
    `§A 每个 effect 的字段集变了（权威口径变了就改判据）✗ ${show(keys)}`);
  const sup = body.supported || {};
  check(Array.isArray(sup.adjustments) && sup.adjustments.includes("invert"),
    `§A supported.adjustments 应含 invert ✗ ${show(sup.adjustments)}`);
  check(Array.isArray(sup.filters) && sup.filters.includes("gaussian_blur"),
    `§A supported.filters 应含 gaussian_blur ✗ ${show(sup.filters)}`);
}
// ── §A2 层过滤的三种形态 ──────────────────────────────────────────────────────
{
  const [s, l] = await both("list_effects", { layer_id: "L3" });
  compareBody("§A2 list_effects layer_id=L3", s, l);
  check(s.body && same((s.body.effects || []).map((e) => e.object_id), ["A1", "A2"]),
    `§A2 layer_id=L3 只应给 A1/A2 ✗ ${show(s.body && s.body.effects)}`);
  // **∴ 类型校验在**入口**✗**（**`validate_args` ✓**）⇒ **∴ 非字符串**不是「当作没给」✗**，
  //   而是「参数 layer_id 类型应为 string，实际为 number」✓（**handler 的 `optional_str` 走不到 ✓**）。
  const [s2, l2] = await both("list_effects", { layer_id: 123 });
  compareError("§A3 list_effects layer_id=123", s2, l2, 400, "invalid_argument",
    "参数 layer_id 类型应为 string，实际为 number");
  // **∴ 未知参数也由入口拒 ✓**（**拼错的名字不许静默忽略 ✓**）。
  const [s5, l5] = await both("list_effects", { layers: "L3" });
  compareError("§A5 list_effects 未知参数 layers", s5, l5, 400, "invalid_argument",
    "list_effects 不接受参数 layers（拼写错误？）；可用参数：layer_id；框架级参数：doc_id, actor, session");
  const [s3, l3] = await both("list_effects", { layer_id: "NOPE" });
  compareBody("§A4 list_effects layer_id=NOPE", s3, l3);
  check(s3.body && s3.body.count === 0 && Array.isArray(s3.body.effects) && s3.body.effects.length === 0,
    `§A4 不存在的层 ⇒ 空表（不是 404 ✓）✗ ${show(s3.body)}`);
}

// ══ §B `sample_colors`：区域渲染、夹包围盒、画布外 ═══════════════════════════════
{
  const [s, l] = await both("sample_colors", { points: [[4, 4], [19, 19], [95, 71]] });
  compareBody("§B 三个画布内点", s, l);
  const body = s.body || {};
  check(same(body.region, { x: 4, y: 4, w: 92, h: 68 }),
    `§B 包围盒应为 (4,4,92,68) ✗ ${show(body.region)}`);
  check(body.outside_count === 0 && same(body.warnings, []),
    `§B 全在画布内 ⇒ outside_count 0、warnings 空 ✗ ${show([body.outside_count, body.warnings])}`);
  check(body.count === 3 && Array.isArray(body.samples) && body.samples.length === 3,
    `§B count 应为 3 ✗ ${show(body.count)}`);
  const sm = body.samples || [];
  // **∴ 判据自己算的那一份 ✗**：L3 的反相对 L1 的红 (255,0,0) 是逐通道 255−x ✓ ⇒ 青 ✓。
  check(sm[0] && same(sm[0].rgba, [0, 255, 255, 255]),
    `§B (4,4) 应为反相后的青 [0,255,255,255] ✗ ${show(sm[0] && sm[0].rgba)}`);
  check(sm[1] && same(sm[1].rgba, [0, 255, 255, 255]),
    `§B (19,19) 应为青（红矩形右下角内侧 ✓）✗ ${show(sm[1] && sm[1].rgba)}`);
  check(sm.every((x) => x.outside === false && Array.isArray(x.rgba)),
    `§B 每个画布内点都应给 rgba ✗ ${show(sm)}`);
}
{
  const [s, l] = await both("sample_colors", { points: [[4.5, 4.5]] });
  compareBody("§B2 小数坐标", s, l);
  const sm = (s.body && s.body.samples) || [];
  check(sm[0] && sm[0].x === 4.5 && sm[0].y === 4.5,
    `§B2 回显应保留小数坐标 ✗ ${show(sm[0])}`);
  check(sm[0] && same(sm[0].rgba, [0, 255, 255, 255]),
    `§B2 (4.5,4.5) 应向下取整成 (4,4) ⇒ 青 ✗ ${show(sm[0] && sm[0].rgba)}`);
}
{
  const [s, l] = await both("sample_colors", { points: [[-1, 5], [96, 0], [10, 72]] });
  compareBody("§B3 三个画布外的点", s, l);
  const body = s.body || {};
  check(body.outside_count === 3, `§B3 outside_count 应为 3 ✗ ${show(body.outside_count)}`);
  check((body.samples || []).every((x) => x.outside === true && x.rgba === null),
    `§B3 画布外的点必须 rgba:null（不许夹到边缘冒充成功 ✗）✗ ${show(body.samples)}`);
  check(Array.isArray(body.warnings) && body.warnings.length === 1
      && body.warnings[0].includes("3 个点在画布**外面**") && body.warnings[0].includes("96×72"),
    `§B3 警告应点名 3 个点与画布 96×72 ✗ ${show(body.warnings)}`);
}
{
  // **§27 的 stride 坑 ✓**：同批里有一个远在画布外的点 ⇒ 夹过包围盒后，
  //   画布内的右下点仍要读得出（未夹 ⇒ offset 越界 ⇒ null ✗）。
  const [s, l] = await both("sample_colors", { points: [[90, 66], [95, 71], [150, 150]] });
  compareBody("§B4 夹包围盒 + 远点", s, l);
  const body = s.body || {};
  check(same(body.region, { x: 90, y: 66, w: 6, h: 6 }),
    `§B4 包围盒应夹到 (90,66,6,6) ✗ ${show(body.region)}`);
  const sm = body.samples || [];
  check(sm[1] && sm[1].outside === false && Array.isArray(sm[1].rgba),
    `§B4 (95,71) 在画布内 ⇒ 必须读到颜色（§27：stride 用夹过的 w ✓）✗ ${show(sm[1])}`);
  check(sm[2] && sm[2].outside === true && sm[2].rgba === null,
    `§B4 (150,150) 应在画布外 ⇒ null ✗ ${show(sm[2])}`);
}
// ── §B5 参数错误逐字 ─────────────────────────────────────────────────────────
// **∴ 两层口径 ✗**：① 入口校验（缺参／类型／未知键 ✓）② handler 逐项校验（元素形状 ✓）。
{
  const [s, l] = await both("sample_colors", {});
  compareError("§B5 缺 points（入口）", s, l, 400, "invalid_argument", "缺少必填参数 points");
  const [s2, l2] = await both("sample_colors", { points: "x" });
  compareError("§B5 points 不是数组（入口）", s2, l2, 400, "invalid_argument",
    "参数 points 类型应为 array，实际为 string");
}
for (const [label, args, expected] of [
  ["points 元素不是数组（字符串）", { points: ["abc"] }, "points[0] 应为 [x,y]，实际 \"abc\""],
  ["points 元素不是数组（对象）", { points: [{ x: 1 }] }, "points[0] 应为 [x,y]，实际 {\"x\":1}"],
  ["x 不是数字", { points: [["a", 0]] }, "points[0][0] 不是数字"],
  ["y 不是数字", { points: [[0, "b"]] }, "points[0][1] 不是数字"],
  ["y 缺失", { points: [[1]] }, "points[0][1] 不是数字"],
  ["空数组", { points: [] }, "points 不能为空"],
]) {
  const [s, l] = await both("sample_colors", args);
  compareError(`§B5 sample_colors ${label}`, s, l, 400, "invalid_argument", expected);
}

// ══ §C `get_histogram`：四个作用域 ＋ 摘要独立重算 ═══════════════════════════════
/** **摘要按返回的桶独立重算 ✓**（**恒等式**：字段之间不许互相矛盾 ✓）。 */
function checkHist(label, body) {
  if (!body) { check(false, `${label}：响应为空 ✗`); return; }
  const pixels = body.pixels;
  check(Number.isInteger(pixels) && pixels > 0, `${label}：pixels 应为正整数 ✗ ${show(pixels)}`);
  const ch = body.channels || {};
  const luma = body.luma;
  for (const key of ["r", "g", "b", "a"]) {
    const buckets = ch[key];
    check(Array.isArray(buckets) && buckets.length === 256,
      `${label}：channels.${key} 应为 256 桶 ✗ ${show(Array.isArray(buckets) ? buckets.length : buckets)}`);
    if (Array.isArray(buckets)) {
      const sum = buckets.reduce((acc, n) => acc + n, 0);
      check(sum === pixels, `${label}：channels.${key} 桶和应为 pixels=${pixels} ✗ 实测 ${sum}`);
    }
  }
  check(Array.isArray(luma) && luma.length === 256, `${label}：luma 应为 256 桶 ✗`);
  if (Array.isArray(luma)) {
    const sum = luma.reduce((acc, n) => acc + n, 0);
    check(sum === pixels, `${label}：luma 桶和应为 pixels=${pixels} ✗ 实测 ${sum}`);
  }
  // **独立重算**：mean/min/max/p5/median/p95 全部从桶推出 ✓（不信任产品自报 ✓）。
  for (const key of ["r", "g", "b", "a", "luma"]) {
    const buckets = key === "luma" ? luma : ch[key];
    if (!Array.isArray(buckets) || buckets.length !== 256) continue;
    const summary = (body.summary || {})[key] || {};
    let weighted = 0, min = null, max = null;
    for (let v = 0; v < 256; v++) {
      const n = buckets[v];
      if (!n) continue;
      if (min === null) min = v;
      max = v;
      weighted += v * n;
    }
    const pct = (percent) => {
      const rank = Math.ceil((percent * pixels) / 100);
      let cumulative = 0;
      for (let v = 0; v < 256; v++) { cumulative += buckets[v]; if (cumulative >= rank) return v; }
      return 255;
    };
    check(summary.mean === weighted / pixels,
      `${label}：summary.${key}.mean 应与桶加权均值一致 ✗ 自报 ${show(summary.mean)}｜重算 ${weighted / pixels}`);
    check(summary.min === min && summary.max === max,
      `${label}：summary.${key} 的 min/max 应为首个/最后一个非空桶 ✗ 自报 ${show([summary.min, summary.max])}｜重算 ${show([min, max])}`);
    check(summary.p5 === pct(5) && summary.median === pct(50) && summary.p95 === pct(95),
      `${label}：summary.${key} 的 p5/median/p95 与桶不符 ✗ 自报 ${show([summary.p5, summary.median, summary.p95])}｜重算 ${show([pct(5), pct(50), pct(95)])}`);
  }
}
{
  const [s, l] = await both("get_histogram", {});
  compareBody("§C1 get_histogram 整文档", s, l);
  const body = s.body || {};
  check(body.scope === "document" && body.layer_id === null,
    `§C1 scope/layer_id 应为 document/null ✗ ${show([body.scope, body.layer_id])}`);
  check(same(body.region, { x: 0, y: 0, w: DOC_W, h: DOC_H }),
    `§C1 region 应为整幅 ✗ ${show(body.region)}`);
  check(body.width === DOC_W && body.height === DOC_H && body.pixels === DOC_W * DOC_H,
    `§C1 宽高/像素数应为 ${DOC_W}/${DOC_H}/${DOC_W * DOC_H} ✗ ${show([body.width, body.height, body.pixels])}`);
  checkHist("§C1 整文档", body);
}
{
  // **红色矩形区域 ✓**：S1 是 (255,0,0,255) ⇒ L3 反相 ⇒ (0,255,255,255) ⇒
  //   16×16=256 像素整块同色 ⇒ 判据自己算得出每个桶 ✓。
  const [s, l] = await both("get_histogram", { region: { x: 4, y: 4, w: 16, h: 16 } });
  compareBody("§C2 get_histogram 选区（红矩形）", s, l);
  const body = s.body || {};
  check(body.scope === "region" && body.layer_id === null,
    `§C2 scope 应为 region ✗ ${show([body.scope, body.layer_id])}`);
  check(same(body.region, { x: 4, y: 4, w: 16, h: 16 }),
    `§C2 region 应回显未夹的入参 ✗ ${show(body.region)}`);
  check(body.pixels === 256 && body.width === 16 && body.height === 16,
    `§C2 应为 16×16=256 像素 ✗ ${show([body.width, body.height, body.pixels])}`);
  const ch = body.channels || {};
  check(same((ch.r || []).map((n) => n), (ch.r || []).map((n, i) => (i === 0 ? 256 : 0))),
    `§C2 反相后的红通道应全落 0 桶（256 个）✗ ${show((ch.r || []).slice(0, 3))}`);
  check(ch.a && ch.a[255] === 256, `§C2 alpha 应全不透明（255 桶 256 个）✗ ${show(ch.a && ch.a[255])}`);
  check(body.summary && body.summary.r && body.summary.r.mean === 0 && body.summary.r.max === 0,
    `§C2 r 的均值/最大都应 0 ✗ ${show(body.summary && body.summary.r)}`);
  checkHist("§C2 选区", body);
}
{
  // **∴ 数组形式过不了入口 ✗**：`region` 声明为 `object` ✓（**`parse_bbox` 的数组分支经工具入口走不到 ✓**）。
  const [sArr, lArr] = await both("get_histogram", { region: [0, 0, 8, 6] });
  compareError("§C3 get_histogram region 数组", sArr, lArr, 400, "invalid_argument",
    "参数 region 类型应为 object，实际为 array");
  // **width/height 别名 ✓**（仍是对象 ✓）。
  const [sWh, lWh] = await both("get_histogram", { region: { x: 0, y: 0, width: 8, height: 6 } });
  compareBody("§C4 get_histogram region={width,height}", sWh, lWh);
  check(sWh.body && sWh.body.pixels === 48,
    `§C4 {width,height} 形式应为 48 像素 ✗ ${show(sWh.body && sWh.body.pixels)}`);
  checkHist("§C4 {width,height}", sWh.body);
}
{
  // **图层作用域 ✓**：L3 是纯调整层（A1 可见+启用、A2 隐藏 ✓）
  //   ⇒ 反相不播种 ⇒ 只剩背景 ✓（**clipping_mask 补偿 ✓**）。
  const [s, l] = await both("get_histogram", { layer_id: "L3" });
  compareBody("§C5 get_histogram 纯调整层 L3", s, l);
  const body = s.body || {};
  check(body.scope === "layer" && body.layer_id === "L3",
    `§C5 scope/layer_id ✗ ${show([body.scope, body.layer_id])}`);
  check(body.pixels === DOC_W * DOC_H, `§C5 像素数应整幅 ✗ ${show(body.pixels)}`);
  checkHist("§C5 L3", body);
  // **∴ 判据自己算的那一份 ✗**：不播种 ⇒ invert 作用在空缓冲上 ⇒ 结果就是背景 ✓。
  check(body.channels && body.channels.a && body.channels.a[255] === DOC_W * DOC_H,
    `§C5 纯调整层作用域应完全不透明（背景 ✓）✗ alpha255=${show(body.channels && body.channels.a && body.channels.a[255])}`);
}
{
  // **组内层 ＋ 组不透明度 0.5 ✓**：`set_only_layer` **不继承**组不透明度 ⇒
  //   蓝色矩形应满 alpha 255（**不补偿 `parent_id:null` ⇒ alpha≈128 ⇒ 红 ✓**）。
  const [s, l] = await both("get_histogram", { layer_id: "L2", region: { x: 20, y: 10, w: 12, h: 12 } });
  compareBody("§C6 get_histogram 组内层 L2 × 选区", s, l);
  const body = s.body || {};
  check(body.scope === "layer_region" && body.layer_id === "L2",
    `§C6 scope/layer_id ✗ ${show([body.scope, body.layer_id])}`);
  check(body.pixels === 144, `§C6 像素数应为 144 ✗ ${show(body.pixels)}`);
  const ch = body.channels || {};
  check(ch.a && ch.a[255] === 144,
    `§C6 组内层应满不透明（不继承组 0.5 ✓）⇒ alpha255 应 144 ✗ 实测 ${show(ch.a && ch.a[255])}`);
  check(ch.r && ch.r[0] === 144 && ch.b && ch.b[255] === 144,
    `§C6 应为纯蓝（r=0、b=255 各 144）✗ ${show([ch.r && ch.r[0], ch.b && ch.b[255]])}`);
  checkHist("§C6 L2", body);
}
{
  // **隐藏图层 ✓**：H1 的 `visible:false` 必须被补偿成可见 ✓（否则只剩背景 ⇒ 红 ✓）。
  const [s, l] = await both("get_histogram", { layer_id: "H1", region: { x: 50, y: 40, w: 8, h: 8 } });
  compareBody("§C7 get_histogram 隐藏层 H1", s, l);
  const body = s.body || {};
  check(body.pixels === 64, `§C7 像素数应为 64 ✗ ${show(body.pixels)}`);
  const ch = body.channels || {};
  check(ch.r && ch.r[255] === 64 && ch.g && ch.g[255] === 64 && ch.b && ch.b[0] === 64,
    `§C7 应为纯黄（r=g=255、b=0 各 64）✗ ${show([ch.r && ch.r[255], ch.g && ch.g[255], ch.b && ch.b[0]])}`);
  checkHist("§C7 H1", body);
}
{
  // **已删层 ✓**：仍算「存在」（墓碑留档 ✓）⇒ 不是 404；只剩背景 ✓。
  const [s, l] = await both("get_histogram", { layer_id: "L5" });
  compareBody("§C8 get_histogram 已删层 L5", s, l);
  const body = s.body || {};
  check(body.scope === "layer" && body.layer_id === "L5",
    `§C8 已删层仍应算存在（墓碑留档 ✓）✗ ${show([body.scope, body.layer_id, s.status])}`);
  check(body.pixels === DOC_W * DOC_H, `§C8 像素数应整幅 ✗ ${show(body.pixels)}`);
  checkHist("§C8 L5", body);
}
{
  // **∴ 非字符串 layer_id 由**入口**拒 ✗**（不是「等同没给」✗）。
  const [s, l] = await both("get_histogram", { layer_id: 123 });
  compareError("§C9 get_histogram layer_id=123", s, l, 400, "invalid_argument",
    "参数 layer_id 类型应为 string，实际为 number");
  // **显式 null ⇒ 视为未提供 ✓**（可选参数口径 ✓）。
  const [sn, ln] = await both("get_histogram", { layer_id: null });
  compareBody("§C9b get_histogram layer_id=null", sn, ln);
  check(sn.body && sn.body.scope === "document" && sn.body.layer_id === null,
    `§C9b 显式 null 应等同未提供 ✗ ${show(sn.body && [sn.body.scope, sn.body.layer_id])}`);
}
// ── §C10 错误逐字 ───────────────────────────────────────────────────────────
{
  const [s, l] = await both("get_histogram", { layer_id: "NOPE" });
  compareError("§C10 不存在的层", s, l, 404, "reference_not_found", "图层 NOPE 不存在");
  check(l.body && l.body.context && l.body.context.layer_id === "NOPE",
    `§C10 本地 context.layer_id 应为 NOPE ✗ ${show(l.body && l.body.context)}`);
  check(s.body && s.body.context && s.body.context.layer_id === "NOPE",
    `§C10 服务端 context.layer_id 应为 NOPE ✗ ${show(s.body && s.body.context)}`);
}
{
  const [s, l] = await both("get_histogram", { region: { x: 1, y: 2, h: 3 } });
  compareError("§C11 region 缺 w（handler）", s, l, 400, "invalid_argument",
    "region 必须是 {x,y,w,h} 或 [x,y,w,h]");
  const [s2, l2] = await both("get_histogram", { region: "x" });
  compareError("§C11 region 是字符串（入口）", s2, l2, 400, "invalid_argument",
    "参数 region 类型应为 object，实际为 string");
  const [s3, l3] = await both("get_histogram", { region: [0, 0, 4] });
  compareError("§C11 region 是数组（入口）", s3, l3, 400, "invalid_argument",
    "参数 region 类型应为 object，实际为 array");
}
{
  const [s, l] = await both("get_histogram", { region: { x: 500, y: 500, w: 10, h: 10 } });
  compareError("§C12 region 与画布不相交", s, l, 400, "invalid_argument",
    "渲染区域 Bbox { x: 500.0, y: 500.0, w: 10.0, h: 10.0 } 与文档 96×72 不相交");
}

// ══ §D 合成原子不许落盘（「读」不能改文档 ✓）══════════════════════════════════
{
  const srvLog = await callServerBare("get_log", { limit: 2000 });
  const locLog = await localCall("get_log", { limit: 2000 });
  const sAtoms = (srvLog.body && srvLog.body.atoms) || [];
  const lAtoms = (locLog.body && locLog.body.atoms) || [];
  check(srvLog.status === 200 && locLog.status === 200,
    `§D 读日志失败 ✗ ${show([srvLog.status, locLog.status])}`);
  check(sAtoms.length === lAtoms.length,
    `§D 原子条数应相同（合成原子不许落盘 ✓）✗ 服务端 ${sAtoms.length}｜本地 ${lAtoms.length}`);
  // **∴ `get_log` 的每行就是**原子本身**（扁平 ✓ —— 与 `tool-pwa-suggestion-accept` 同一读法 ✓）。**
  const kinds = (rows) => rows.map((r) => (r && r.kind) || null);
  check(same(kinds(sAtoms), kinds(lAtoms)), `§D 原子 kind 序列不同 ✗`);
  const layerProps = (rows) => rows.filter((r) => {
    const pl = (r && r.payload) || {};
    return r && r.kind === "set_property" && !pl.object_id && typeof pl.layer_id === "string";
  }).map((r) => ({ layer_id: r.payload.layer_id, key: r.payload.key }));
  const sLP = layerProps(sAtoms), lLP = layerProps(lAtoms);
  check(same(sLP, lLP), `§D 层级 set_property 应只含判据自己写的那条（H1 visible、G1 opacity）✗\n     服务端 ${show(sLP)}\n     本地   ${show(lLP)}`);
  check(lLP.filter((p) => p.key === "visible").length === 1 && lLP.some((p) => p.layer_id === "H1"),
    `§D 层级 visible 只该有 H1 那一条 ✗ ${show(lLP)}`);
  check(!lLP.some((p) => p.key === "parent_id") && !lLP.some((p) => p.key === "clipping_mask"),
    `§D 图层作用域的补偿原子（parent_id/clipping_mask）落盘了 ✗ ${show(lLP)}`);
}

if (bad.length) {
  console.log("✗ PWA 读测三端点 判据失败：");
  for (const m of bad) console.log("   · " + m);
  console.log(`   ⇒ 通过 ${okCount} 条｜失败 ${bad.length} 条`);
  process.exit(1);
}
console.log(`✓ PWA 本地读测三端点与服务端同口径（${okCount} 条断言全过：`
  + "list_effects 顺序/隐藏/禁用、sample_colors 夹包围盒/画布外/参数错误、"
  + "get_histogram 四作用域与摘要恒等式、图层作用域三处补偿、合成原子不落盘、错误逐字）");
