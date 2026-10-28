#!/usr/bin/env node
// **判据：PWA 本地的 `transform_object` 与服务端**同一输入同一输出 ✓ ★（台账 §660）。
//
// **∴ 它测什么 ✗**（**内核 ①：断言必须能证明自己能红 ✓**）：
//   同一个请求分别打到真服务端与本地 `api-local` ✓ ⇒ 比解析后的 JSON ✓
//   （整数严格、非整数 1e-12 相对误差 ✓；**不比字节** ✓）。
//   **∴ 三件事必须同时成立 ✗**：
//   ① **矩阵的算式**：`T(anchor) · op · T(−anchor)` ✓，`compose:true` 时再叠到
//      对象**当前**变换上 ✓ —— 判据**自己**按输入算一个期望矩阵 ✗
//      （**∴ 不能只断「两边相等」**：两边**同错**时相等断言照样绿 ✓ ⇒ 那是安慰剂 ✓）；
//   ② **缺省 anchor = 折叠后包围盒的中心** ✓ —— 形状（含描边半宽）／路径（控制点盒）／
//      文本（5×7 度量）／调整（零盒）／实例（按 master 解析）各自的期望中心**由判据独立算出** ✓；
//   ③ **读路径跟着走** ✓：`get_object.transform` 必须等于刚写进去的绝对矩阵 ✓
//      （**∴ 只修写路径会留下「回执对、读回旧」的形态③ ✗**）。
//   报错口径（404／400／409／403）**逐字比** ✓；`set_property{key:"transform"}` 的折叠
//   也在读路径上生效 ✓。
//
// **∴ 前提 ✗**：需要**活着的服务端**（`run-criteria-local.sh` 起一个并把地址放进 argv[2] ✓）。
//   拿不到 ⇒ `exit(3)`（判据前提不成立 ≠ 产品失败 ✓）。
//
// **∴ 变异（四层齐第④层 ✓ 逐条见台账「第 六百六十 节」与「第 六百六十一 节」）**：
//   ① 缺省 anchor 取 (0,0) ⇒ §B 全红 ✓；
//   ② 绕 anchor 的平移写错（漏 `−M·anchor`）⇒ §B1／§B2／§B4／§B6／§D3 红 ✓
//      （**∴ §A 的 anchor 是 (0,0) ✗ ⇒ 那一项不参与 ✓**）；
//   ③ `compose` 缺省成 false ⇒ §A／§D2／§E 红 ✓；
//   ④ `compose` 叠加顺序写反（`view.transform ∘ aboutAnchor`）⇒ §D2 红 ✓；
//   ⑤ 文本高度用 `lines * 行高`（多算一行）⇒ §B2 的 anchor 红 ✓
//      （**∴ 必须改 ASCII 分支 ✓** —— 改 atlas 分支对 `AB` 无效，实测绿 ✗）；
//   ⑥ 去掉「恰好一个」校验 ⇒ §C7 本地 200 而服务端 400 ⇒ 红 ✓；
//   ⑦ `set_property{key:"transform"}` 不重放 ⇒ §E 的 compose 结果红 ✓；
//   ⑧ 实例解析漏 `local_transform` ⇒ §B4 的 anchor 红 ✓；
//   ⑨ 实例包围盒取实例自己的 `data`（而非解析到的 master）⇒ §B4 红 ✓。
//
// **∴ 曾经的盲区已封 ✓**（第 六百六十 节实测出来的，不是推断 ✓）：
//   **`transform_object` 只回 `anchor`，而描边半宽／文本行高这类边距是中心对称的** ✓
//   ⇒ **∴ 变异「形状漏 `stroke_width/2`」在 §A–§E 上实测仍是绿 ✗**（中心不变 ✓）。
//   **∴ 宽高面唯一可观测面是 `get_resolved_state.bbox` ✓** ⇒ 本地那份曾是**另一种载荷** ✗
//   （文档级、无视 `object_id`、没有 `bbox`）⇒ **∴ 本批已改成逐对象契约 ✓**，§F 就此封住宽高面 ✓。
//
// **∴ §F 的变异（十次）**：
//   ⑩ 形状包围盒漏 `stroke_width/2` ⇒ §F 矩形含描边 ＋ §F 实例 红 ✓（其余 §B 仍绿 = 中心的盲区 ✓）；
//   ⑪ 实例包围盒的 `data` 取实例自己的（而非解析到的 master）⇒ §F 实例 红 ✓（实测 [5,7,0,0]）；
//   ⑫ 实例包围盒的变换取 `view.transform`（而非 `resolve_instance` 的合成）⇒ §F 实例 ＋ §B4 红 ✓；
//   ⑬ 去掉 master 已删检查 ⇒ §F master 已删 ＋ §C10 四条 红 ✓；
//   ⑭ `type` 恒 "shape" ⇒ §F 文本／实例／调整／master 已删 四条 红 ✓；
//   ⑮ 去掉类型校验（非字符串回「缺少必填」）⇒ §F 非字符串 ＋ null 红 ✓；
//   ⑯ 去掉未知参数校验 ⇒ §F 未知参数 四条 红 ✓；
//   ⑰ `master_id` 恒 null ⇒ §F 实例 ＋ master 已删 红 ✓；
//   ⑱ `local_transform` 恒 null ⇒ 同上两条红 ✓；
//   ⑲ 404 文案去掉对象名 ⇒ §F 对象不存在 红 ✓。
process.on("uncaughtException", (error) => {
  console.error(String(error && error.stack || error).slice(0, 1200));
  process.exit(3);
});
import { pathToFileURL } from "node:url";

const BASE = process.argv[2];
if (!BASE || !/^https?:\/\//.test(BASE)) {
  console.log("⊘ 不可判：argv[2] 缺服务端地址 ⇒ 请用 bash scripts/run-criteria-local.sh scripts/tool-pwa-transform-object.mjs 跑"
    + "（判据前提不成立，不是产品问题）");
  process.exit(3);
}

const bad = [];
const check = (cond, msg) => { if (!cond) bad.push(msg); };
const UNIQ = "t" + Date.now().toString(36);
const SERVER_DOC = `tsrv-${UNIQ}`;
const LOCAL_DOC = `tloc-${UNIQ}`;

const localApi = await import(pathToFileURL(process.cwd() + "/scripts/lib/pwa-node-stub.mjs").href)
  .then((m) => m.bootPwaLocalApi({ origin: BASE, localOnly: true }));
const nativeFetch = globalThis.fetch;

const createdBody = await nativeFetch(`${BASE}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: SERVER_DOC, width: 400, height: 300 }),
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
/** **本地抛异常记成产品失败 ✗ 不是判据坏了 ✓**（与 `tool-pwa-instance-detach` 同一纪律 ✓）。 */
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
  body: JSON.stringify({ doc_id: LOCAL_DOC, width: 400, height: 300 }),
})).then((r) => r.json().catch(() => null));
if (!localCreated || localCreated.ok !== true) {
  console.log(`⊘ 不可判：本地建文档失败（${JSON.stringify(localCreated)?.slice(0, 160)}）⇒ 前提不成立，不是产品问题`);
  process.exit(3);
}

/** **比对器 ✓**（与 `tool-pwa-instance-detach` 同一口径：断结构不断字节 ✓）。 */
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
const show = (value) => String(JSON.stringify(value)).slice(0, 220);
/** **公共前序：服务端拒绝 ⇒ 前提不成立（`exit(3)`）✓；本地拒绝 ⇒ 产品失败 ✓**。 */
async function setup(label, tool, args) {
  const [s, l] = await both(tool, args);
  if (s.status !== 200 || !s.body || s.body.ok !== true) {
    console.log(`⊘ 不可判：${label} 服务端拒绝（${s.status} ${show(s.body)}）⇒ 前提不成立，不是产品问题`);
    process.exit(3);
  }
  check(l.status === 200 && l.body && l.body.ok === true,
    `${label}：本地被拒 ✗ 状态 ${l.status}｜${show(l.body)}`);
  return [s, l];
}
/** **错误逐字比 ✓**（本地错误是扁平的 `error_code` ＋ `context.detail` ✓）。 */
function compareError(label, srv, loc, status, code) {
  if (!srv.body || !loc.body) { check(false, `${label}：有一边读不出 JSON ✗`); return; }
  check(srv.status === status, `${label}：服务端状态码应为 ${status} ✗ 实测 ${srv.status}｜${show(srv.body)}`);
  check(loc.status === status, `${label}：本地状态码应为 ${status} ✗ 实测 ${loc.status}｜${show(loc.body)}`);
  check(srv.body.error_code === code, `${label}：服务端错误码应为 ${code} ✗ 实测 ${srv.body.error_code} ⇒ 判据口径要跟着权威改`);
  check(loc.body.error_code === code, `${label}：本地错误码应为 ${code} ✗ 实测 ${loc.body.error_code}`);
  const sd = srv.body.context && srv.body.context.detail;
  const ld = loc.body.context && loc.body.context.detail;
  check(typeof ld === "string" && ld.length > 0, `${label}：本地没给 context.detail ⇒ 调用方读不到原因 ✗`);
  check(sd === ld, `${label}：错误文案不同 ✗\n     服务端 ${sd}\n     本地     ${ld}`);
}
/** **一次成功变换 ✓**：两边都 200 ✓、anchor／matrix／composed 各自等于**判据算的期望** ✓，
 *  再回读 `get_object.transform` ✓（**∴ 只比「两边相等」不够 ✓：两边同错也绿 ✗**）。 */
async function transformCase(label, args, expectedAnchor, expectedMatrix, composed) {
  const [s, l] = await both("transform_object", args);
  check(s.status === 200 && s.body && s.body.ok === true,
    `${label}：服务端应 200 ✗ 状态 ${s.status}｜${show(s.body)}`);
  check(l.status === 200 && l.body && l.body.ok === true,
    `${label}：本地应 200 ✗ 状态 ${l.status}｜${show(l.body)}`);
  if (!s.body || !l.body) { check(false, `${label}：有一边读不出 JSON ✗`); return; }
  for (const [who, r] of [["服务端", s], ["本地", l]]) {
    check(same(r.body.object_id, args.object_id),
      `${label}（${who}）回执 object_id 应为 ${args.object_id} ✗ 实测 ${show(r.body.object_id)}`);
    check(same(r.body.anchor, expectedAnchor),
      `${label}（${who}）anchor 应为 ${show(expectedAnchor)} ✗ 实测 ${show(r.body.anchor)}`
      + " ⇒ 缺省 anchor = 折叠后包围盒中心这条口径不对");
    check(same(r.body.matrix, expectedMatrix),
      `${label}（${who}）matrix 应为 ${show(expectedMatrix)} ✗ 实测 ${show(r.body.matrix)}`
      + " ⇒ 绕 anchor 的算式或 compose 叠加不对（不是「两边一致」就够 ✓）");
    check(r.body.composed === composed,
      `${label}（${who}）composed 应为 ${composed} ✗ 实测 ${show(r.body.composed)}`);
    check(Number.isInteger(r.body.head), `${label}（${who}）head 应为整数 ✗ 实测 ${show(r.body.head)}`);
  }
  check(same(s.body.matrix, l.body.matrix), `${label}：两边的 matrix 不同 ✗ 服务端 ${show(s.body.matrix)}｜本地 ${show(l.body.matrix)}`);
  check(same(s.body.anchor, l.body.anchor), `${label}：两边的 anchor 不同 ✗ 服务端 ${show(s.body.anchor)}｜本地 ${show(l.body.anchor)}`);
  // **读路径 ✓**：`get_object.transform` 必须反映刚写进去的绝对矩阵 ✓。
  const [gs, gl] = await both("get_object", { object_id: args.object_id });
  check(gs.status === 200 && gl.status === 200, `${label} 读回：两边都应 200 ✗ 服务端 ${gs.status}｜本地 ${gl.status}`);
  for (const [who, r] of [["服务端", gs], ["本地", gl]]) {
    check(r.body && r.body.transform && same(r.body.transform.matrix, expectedMatrix),
      `${label}（${who}）get_object.transform.matrix 应为 ${show(expectedMatrix)} ✗ 实测 ${show(r.body && r.body.transform)}`
      + " ⇒ 写路径与读路径口径分裂");
  }
}

// ── 公共前序：图层 ＋ 各种 master ──
await setup("建图层", "create_layer", { layer_id: "L1" });
const rectData = (x, y, w, h, strokeWidth) => ({
  geometry: { kind: "rect", bbox: { x, y, w, h } },
  color: [1, 0, 0, 1], stroke_width: strokeWidth,
});
await setup("S1 矩形（描边 4）", "draw_shape", {
  layer_id: "L1", object_id: "S1", data: rectData(10, 20, 40, 20, 4),
});
await setup("S2 矩形（描边 4）", "draw_shape", {
  layer_id: "L1", object_id: "S2", data: rectData(10, 20, 40, 20, 4),
});
await setup("T1 文本", "draw_text", {
  layer_id: "L1", object_id: "T1",
  data: { text: "AB", font: "builtin", size: 14, color: [1, 1, 1, 1], position: [10, 20] },
});
await setup("A1 调整", "add_adjustment", {
  layer_id: "L1", object_id: "A1", adjustment_type: "levels",
});
await setup("S3 实例的 master（矩形 20×10，无描边）", "draw_shape", {
  layer_id: "L1", object_id: "S3", data: rectData(100, 50, 20, 10, 0),
});
await setup("I1 实例（local_transform 平移 5,7）", "create_instance", {
  instance_id: "I1", layer_id: "L1", master_id: "S3",
  local_transform: { matrix: [1, 0, 0, 1, 5, 7], pivot: [0, 0] },
});
await setup("A2 调整（实例的 master）", "add_adjustment", {
  layer_id: "L1", object_id: "A2", adjustment_type: "levels",
});
await setup("I2 实例（master 是调整）", "create_instance", {
  instance_id: "I2", layer_id: "L1", master_id: "A2",
  local_transform: { matrix: [1, 0, 0, 1, 0, 0], pivot: [0, 0] },
});
await setup("P1 路径（无描边）", "draw_shape", {
  layer_id: "L1", object_id: "P1",
  data: {
    geometry: {
      kind: "path",
      subpaths: [{ start: [0, 0], closed: true, segments: [
        { op: "L", to: [20, 0] }, { op: "L", to: [20, 10] },
      ] }],
    },
    stroke_width: 0,
  },
});
await setup("S6 矩形（无描边，原点）", "draw_shape", {
  layer_id: "L1", object_id: "S6", data: rectData(0, 0, 10, 10, 0),
});
await setup("S7 矩形（无描边，原点）", "draw_shape", {
  layer_id: "L1", object_id: "S7", data: rectData(0, 0, 10, 10, 0),
});
await setup("S4 矩形（待删）", "draw_shape", {
  layer_id: "L1", object_id: "S4", data: rectData(0, 0, 10, 10, 0),
});
await setup("S5 矩形（待锁）", "draw_shape", {
  layer_id: "L1", object_id: "S5", data: rectData(0, 0, 10, 10, 0),
});
await setup("M3 矩形（实例的 master，待删）", "draw_shape", {
  layer_id: "L1", object_id: "M3", data: rectData(0, 0, 10, 10, 0),
});
await setup("I3 实例（master 待删）", "create_instance", {
  instance_id: "I3", layer_id: "L1", master_id: "M3",
  local_transform: { matrix: [1, 0, 0, 1, 0, 0], pivot: [0, 0] },
});

// ══ §A 显式 anchor ＋ rotate（compose 缺省 true，当前变换是单位阵） ══
// 绕 (0,0) 转 90°：矩阵 [0,1,-1,0,0,0] ✓（**∴ 判据自己算 ✓**）。
await transformCase("§A 绕原点转 90°", {
  object_id: "S1", rotate: { degrees: 90 }, anchor: { x: 0, y: 0 },
}, [0, 0], [0, 1, -1, 0, 0, 0], true);

// ══ §B 缺省 anchor = 折叠后包围盒中心 ══
// B1 矩形 {10,20,40,20} ＋ 描边 4 ⇒ 盒 {8,18,44,24} ⇒ 中心 (30,30) ✓；
//     绕它转 90°：e = 30 − (−1×30) = 60，f = 30 − (1×30) = 0 ✓。
await transformCase("§B1 矩形缺省 anchor", {
  object_id: "S2", rotate: { degrees: 90 },
}, [30, 30], [0, 1, -1, 0, 60, 0], true);

// B2 文本 "AB" size 14 ⇒ scale=round(14/7)=2 ⇒ 20×14 ⇒ 盒 {10,20,20,14} ⇒ 中心 (20,27) ✓；
//     绕它转 180°：e = 20 − (−1×20) = 40，f = 27 − (−1×27) = 54 ✓。
await transformCase("§B2 文本缺省 anchor（5×7 度量）", {
  object_id: "T1", rotate: { degrees: 180 },
}, [20, 27], [-1, 0, 0, -1, 40, 54], true);

// B3 调整对象没有自身几何 ⇒ 盒 (0,0,0,0) ⇒ 中心 (0,0) ✓；平移 (5,−3) ⇒ [1,0,0,1,5,−3] ✓。
await transformCase("§B3 调整对象缺省 anchor（零盒）", {
  object_id: "A1", translate: { dx: 5, dy: -3 },
}, [0, 0], [1, 0, 0, 1, 5, -3], true);

// B4 实例：master 矩形 {100,50,20,10} ＋ local_transform 平移 (5,7) ⇒ 盒 {105,57,20,10}
//     ⇒ 中心 (115,62) ✓；绕它放大 2×：e = 115 − 2×115 = −115，f = 62 − 2×62 = −62 ✓。
await transformCase("§B4 实例缺省 anchor（解析 master）", {
  object_id: "I1", scale: { x: 2, y: 2 },
}, [115, 62], [2, 0, 0, 2, -115, -62], true);

// B5 实例的 master 是调整对象 ⇒ 合成对象按**形状**解析调整的 data ⇒ 零盒 ⇒ 中心 (0,0) ✓
//     （**∴ 服务端口径如此 ✓ —— 把 master 的 `data` 当 `Shape` 解析 ✓；不是按 master 顶层类型 ✗**）。
await transformCase("§B5 实例（master 是调整）缺省 anchor", {
  object_id: "I2", translate: { dx: 1, dy: 2 },
}, [0, 0], [1, 0, 0, 1, 1, 2], true);

// B6 路径：控制点盒 {0,0,20,10} ⇒ 中心 (10,5) ✓；绕它转 90°：e = 10 − (−1×5) = 15，
//     f = 5 − (1×10) = −5 ✓。
await transformCase("§B6 路径缺省 anchor（控制点盒）", {
  object_id: "P1", rotate: { degrees: 90 },
}, [10, 5], [0, 1, -1, 0, 15, -5], true);

// ══ §C 报错口径（逐字比 ✓） ══
{
  const c1 = await both("transform_object", { object_id: "NOPE", rotate: { degrees: 10 } });
  compareError("§C1 对象不存在", c1[0], c1[1], 404, "reference_not_found");

  const c2 = await both("transform_object", { rotate: { degrees: 10 } });
  compareError("§C2 缺 object_id", c2[0], c2[1], 400, "invalid_argument");

  const c3 = await both("transform_object", { object_id: "S1", matrix: [1, 0, 0, 1, 0, 0] });
  compareError("§C3 未知参数（旧的本地契约 `matrix`）", c3[0], c3[1], 400, "invalid_argument");

  const c4 = await both("transform_object", { object_id: "S1", rotate: { degrees: 10 }, anchor: { x: 1 } });
  compareError("§C4 anchor 缺 y", c4[0], c4[1], 400, "invalid_argument");

  const c5 = await both("transform_object", { object_id: "S1", rotate: {} });
  compareError("§C5 rotate 缺 degrees", c5[0], c5[1], 400, "invalid_argument");

  const c6 = await both("transform_object", { object_id: "S1", scale: { x: 0, y: 0 } });
  compareError("§C6 scale 两向为 0", c6[0], c6[1], 400, "invalid_argument");

  const c7 = await both("transform_object", {
    object_id: "S1", rotate: { degrees: 10 }, translate: { dx: 1, dy: 1 }, anchor: { x: 0, y: 0 },
  });
  compareError("§C7 同时给 rotate 与 translate", c7[0], c7[1], 400, "invalid_argument");

  const c8 = await both("transform_object", { object_id: 5, rotate: { degrees: 10 } });
  compareError("§C8 object_id 类型错", c8[0], c8[1], 400, "invalid_argument");

  const c9 = await both("transform_object", { object_id: "S1", anchor: [0, 0], rotate: { degrees: 10 } });
  compareError("§C9 anchor 类型错", c9[0], c9[1], 400, "invalid_argument");

  // §C10 master 已删 ⇒ 实例没有可用包围盒 ⇒ 409（**∴ 缺省 anchor 解析不出时**明确报错 ✓，不悄悄用 (0,0) ✗**）。
  await setup("删掉 I3 的 master", "delete_object", { object_id: "M3" });
  const c10 = await both("transform_object", { object_id: "I3", rotate: { degrees: 10 } });
  compareError("§C10 无包围盒（master 已删）", c10[0], c10[1], 409, "precondition_failed");

  // §C11 已删对象 ⇒ 409「已被 tombstone，move 被拒绝」✓（**∴ 墓碑对象仍在状态里 ✓ ⇒ 不是 404 ✓**）。
  await setup("删掉 S4", "delete_object", { object_id: "S4" });
  const c11 = await both("transform_object", {
    object_id: "S4", translate: { dx: 1, dy: 1 }, anchor: { x: 0, y: 0 },
  });
  compareError("§C11 对象已被 tombstone", c11[0], c11[1], 409, "precondition_failed");

  // §C12 锁定的对象 ⇒ 403（**∴ 闸在提交期 ✓，与 anchor/参数校验的次序一致 ✓**）。
  await setup("锁住 S5", "set_property", { object_id: "S5", key: "locked", value: true });
  const c12 = await both("transform_object", {
    object_id: "S5", translate: { dx: 1, dy: 1 }, anchor: { x: 0, y: 0 },
  });
  compareError("§C12 对象已锁定", c12[0], c12[1], 403, "permission_denied");
}

// ══ §D compose 的两种语义 ══
// D1 compose:false ⇒ 用本次算出的矩阵**替换**当前变换 ✓（不是叠加 ✗）：[1,0,0,1,5,7] ✓。
await transformCase("§D1 compose=false 替换", {
  object_id: "S6", translate: { dx: 5, dy: 7 }, anchor: { x: 0, y: 0 }, compose: false,
}, [0, 0], [1, 0, 0, 1, 5, 7], false);
// D2 compose:true ⇒ 绕原点转 90° 叠到 [1,0,0,1,5,7] 上 ⇒ R×T ⇒ 平移 = R·(5,7) = (−7,5) ✓。
await transformCase("§D2 compose=true 叠加", {
  object_id: "S6", rotate: { degrees: 90 }, anchor: { x: 0, y: 0 },
}, [0, 0], [0, 1, -1, 0, -7, 5], true);
// D3 compose:false ＋ 非零 anchor：绕 (5,5) 放大 2×3 ⇒ [2,0,0,3,−5,−10] ✓。
await transformCase("§D3 compose=false 绕非零 anchor", {
  object_id: "S6", scale: { x: 2, y: 3 }, anchor: { x: 5, y: 5 }, compose: false,
}, [5, 5], [2, 0, 0, 3, -5, -10], false);

// ══ §E 读路径：`set_property{key:"transform"}` 的折叠 ══
// 先设绝对变换 [1,0,0,1,3,4] ✓，再 compose 一次绕原点转 90° ⇒ R×T ⇒ 平移 = R·(3,4) = (−4,3) ✓。
// **∴ 这条专门守「本地视图不重放 set_property 的 transform」✗**（§E 曾红过 ✓）。
const setT = await both("set_property", {
  object_id: "S7", key: "transform", value: { matrix: [1, 0, 0, 1, 3, 4], pivot: [0, 0] },
});
check(setT[0].status === 200 && setT[1].status === 200,
  `§E 写 set_property transform：两边都应 200 ✗ 服务端 ${setT[0].status}｜本地 ${setT[1].status}｜${show(setT[1].body)}`);
await transformCase("§E compose 在 set_property 变换之上", {
  object_id: "S7", rotate: { degrees: 90 }, anchor: { x: 0, y: 0 },
}, [0, 0], [0, 1, -1, 0, -4, 3], true);

// ══ §F 包围盒宽高面：`get_resolved_state.bbox` ══
// **∴ 为什么必须有这一段 ✗**：`transform_object` 只回 `anchor` ✓ ⇒ **盒的宽高看不见** ✗
//   ⇒ 「形状漏 `stroke_width/2`」这类变异**中心不变** ⇒ 只看 anchor 的判据**永远绿** ✗
//   （**第 六百六十 节实测过一次 ✓**）。**∴ `get_resolved_state` 是服务端给的宽高面 ✓**。
//   **∴ 期望盒由判据自己按输入算 ✓**（不是只断「两边相等」✓）。
async function stateCase(label, objectId, expect) {
  const [s, l] = await both("get_resolved_state", { object_id: objectId });
  check(s.status === 200 && l.status === 200,
    `${label}：两边都应 200 ✗ 服务端 ${s.status}｜本地 ${l.status}｜${show(l.body)}`);
  if (!s.body || !l.body) { check(false, `${label}：有一边读不出 JSON ✗`); return; }
  for (const key of ["type", "is_instance", "master_id", "local_transform", "bbox"]) {
    check(key in s.body, `${label}：服务端响应里没有「${key}」✗ ⇒ 权威口径变了，要改判据`);
    check(same(s.body[key], l.body[key]),
      `${label}：字段「${key}」不同 ✗ 服务端 ${show(s.body[key])}｜本地 ${show(l.body[key])}`);
  }
  if (expect !== undefined) {
    for (const [who, r] of [["服务端", s], ["本地", l]]) {
      check(same(r.body.bbox, expect),
        `${label}（${who}）bbox 应为 ${show(expect)} ✗ 实测 ${show(r.body.bbox)}`
        + " ⇒ 包围盒算式/取值不对（宽高面只有这里看得见 ✓）");
    }
  }
}
await setup("R1 矩形（描边 4，算宽高）", "draw_shape", {
  layer_id: "L1", object_id: "R1", data: rectData(10, 20, 40, 20, 4),
});
await setup("R2 文本（算宽高）", "draw_text", {
  layer_id: "L1", object_id: "R2",
  data: { text: "AB", font: "builtin", size: 14, color: [1, 1, 1, 1], position: [10, 20] },
});
await setup("R3 路径形状（描边 0，算宽高）", "draw_shape", {
  layer_id: "L1", object_id: "R3",
  data: {
    geometry: { kind: "path", subpaths: [{ start: [0, 0], closed: true, segments: [
      { op: "L", to: [20, 0] }, { op: "L", to: [20, 10] },
    ] }] },
    stroke_width: 0,
  },
});
await setup("R4 实例（master=R1，平移 5,7）", "create_instance", {
  instance_id: "R4", layer_id: "L1", master_id: "R1",
  local_transform: { matrix: [1, 0, 0, 1, 5, 7], pivot: [0, 0] },
});
await setup("A3 调整（零盒）", "add_adjustment", {
  layer_id: "L1", object_id: "A3", adjustment_type: "levels",
});
// 矩形 (10,20,40,20) ＋ 描边 4 ⇒ 四边各外扩 2 ⇒ [8,18,44,24] ✓（**漏 `stroke_width/2` ⇒ 宽高各少 4** ✓）。
await stateCase("§F 矩形含描边", "R1", [8, 18, 44, 24]);
// 文本 5×7：size 14 ⇒ scale 2 ⇒ 宽 2·5·2=20、高 7·2=14 ✓。
await stateCase("§F 文本度量", "R2", [10, 20, 20, 14]);
// 路径形状：控制点盒 (0,0)–(20,10)，描边 0 ⇒ 盒不外扩 ✓。
await stateCase("§F 路径控制点盒", "R3", [0, 0, 20, 10]);
// 实例：master 的盒按解析后的变换平移 (5,7) ⇒ 宽高不变 ✓。
await stateCase("§F 实例解析后的盒", "R4", [13, 25, 44, 24]);
await stateCase("§F 调整（零盒）", "A3", [0, 0, 0, 0]);
// **master 已删 ⇒ 实例的盒解析不出来 ⇒ `null`** ✓（§C③ 已把 M3 删了 ✓）。
await stateCase("§F master 已删的实例", "I3", null);
// **报错逐字 ✓**：缺 `object_id`／给了非字符串 ⇒ 400「缺少必填参数 object_id」；
//   对象不存在 ⇒ 404（**∴ 与 `transform_object` 的 404 文案同形 ✓**）。
const g1 = await both("get_resolved_state", {});
compareError("§F 缺 object_id", g1[0], g1[1], 400, "invalid_argument");
const g2 = await both("get_resolved_state", { object_id: 123 });
compareError("§F object_id 非字符串", g2[0], g2[1], 400, "invalid_argument");
const g2n = await both("get_resolved_state", { object_id: null });
compareError("§F object_id 为 null", g2n[0], g2n[1], 400, "invalid_argument");
const g2u = await both("get_resolved_state", { object_id: "R1", bogus: 1 });
compareError("§F 未知参数", g2u[0], g2u[1], 400, "invalid_argument");
const g3 = await both("get_resolved_state", { object_id: "NOPE2" });
compareError("§F 对象不存在", g3[0], g3[1], 404, "reference_not_found");

if (bad.length) {
  console.log(`✗ PWA 本地 transform_object 与服务端同口径：${bad.length} 条不符`);
  for (const m of bad) console.log("   · " + m);
  process.exit(1);
}
console.log("✓ PWA 本地 transform_object 与服务端同口径（绕 anchor 算式／compose 两态／"
  + "六种缺省 anchor（形状描边／文本度量／调整零盒／实例解析／实例的 master 是调整／路径控制点）／"
  + "十二条报错逐字／set_property transform 的读路径折叠／"
  + "get_resolved_state 的包围盒宽高面与五条报错）");
