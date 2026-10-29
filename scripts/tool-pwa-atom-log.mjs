#!/usr/bin/env node
// **判据：PWA 本地的 `get_stroke_log`／`list_atoms`／`get_atom` 与服务端**同一份原子日志、同一答案** ✓**
// （台账「第 六百六十六 节」）。
//
// **∴ 怎么做到「同一份日志」✗**：本地 `brush_stroke` 在 node 里要 `import("/brush-local.js")` ✗
//   （只存在于浏览器 ✓）⇒ **判据不能靠本地落笔** ✗。**∴ 做法 ✗**：先在**真服务端**建文档、落笔、
//   撤销 ✓ ⇒ 用 `GET /api/atoms` 取**完整原子**（含 payload／label／group_id ✓）⇒
//   按 seq 顺序 `store.putAtom` 喂进本地文档 ✓ ⇒ **∴ 两边读的是同一份日志 ✓**（id／时间戳都相同 ✓）。
//   **∴ 这是判据的装配手法 ✓，不是产品写入路径 ✓**（产品写入路径由别的判据守 ✓）。
//
// **∴ 它断什么 ✗**（内核 ①：断言必须能证明自己能红 ✓）：
//   ① `get_stroke_log`：只列笔触 ✓、顺序／`order:"newest"` 反转 ✓、`total` 与
//      `reverted_excluded` **恒等式** ✓（`total + reverted_excluded` = 全部匹配 ✓）、
//      `next_offset`／`truncated` 自洽 ✓、逐点压力 min/max/first/last **由判据自己算** ✓；
//   ② `list_atoms`：seq 倒序 ✓、label／layer／group／brush／bbox 五条筛选 ✓、
//      `hidden_non_stroke` **恒等式** ✓（`total(include) - total(default) = hidden` ✓）、
//      `offset`／`has_more`／`limit:0` **不自相矛盾** ✓、`origin` 是**来源**（笔触 = drawn ✓）、
//      `stored_origin` 与 `origin` 的关系 ✓（本地写入也盖 `origin` 戳 ✓）；
//   ③ `get_atom`：七个元数据字段齐全（`message`／`label`／`changeset_id`／`parents` ✓）；
//   ④ 错误逐字：负数／小数计数参数、`thumb_size ≥ 2^32`（不许回绕成 0 ✓）、未知键 ✓；
//   ⑤ 缩略图：只比**张数／跳过条数／PNG 头里的宽高** ✓ —— **不比字节** ✗
//      （两边的 PNG 压缩器不同 ✓；像素相同编出来也可能不同 ✓）。
//
// **∴ 前提 ✗**：活服务端（`run-criteria-local.sh` ✓）＋ 不过期的真 wasm 内核（旧了 `exit(3)` ✓）。
process.on("uncaughtException", (error) => {
  console.error(String((error && error.stack) || error).slice(0, 1200));
  process.exit(3);
});
import { pathToFileURL } from "node:url";

const BASE = process.argv[2];
if (!BASE || !/^https?:\/\//.test(BASE)) {
  console.log("⊘ 不可判：argv[2] 缺服务端地址 ⇒ 请用 bash scripts/run-criteria-local.sh scripts/tool-pwa-atom-log.mjs 跑"
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
const UNIQ = "a" + Date.now().toString(36);
const SERVER_DOC = `asrv-${UNIQ}`;
const LOCAL_DOC = `aloc-${UNIQ}`;
const SERVER_DOC2 = `asrv2-${UNIQ}`;
const LOCAL_DOC2 = `aloc2-${UNIQ}`;

const { local: localApi } = await kernelLib.bootLocalWithRealKernel({ origin: BASE });
const nativeFetch = globalThis.fetch;
const storeLib = await import(pathToFileURL(process.cwd() + "/web/store.js").href);

const createDoc = async (doc, w, h) => {
  const body = await nativeFetch(`${BASE}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: w, height: h }),
  }).then((r) => r.json());
  if (!body.ok || !body.token) {
    console.log(`⊘ 不可判：服务端建文档失败（${JSON.stringify(body).slice(0, 160)}）⇒ 前提不成立，不是产品问题`);
    process.exit(3);
  }
  return body.token;
};
const TOKEN = await createDoc(SERVER_DOC, 96, 72);
const TOKEN2 = await createDoc(SERVER_DOC2, 32, 32);

const callServer = async (tool, args, token = TOKEN, doc = SERVER_DOC) => {
  const res = await nativeFetch(`${BASE}/api/tools/${tool}?token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, actor: "human:judge", session: "session:judge", ...args }),
  });
  return { status: res.status, body: await res.json().catch(() => null) };
};
const localCall = async (tool, args, doc = LOCAL_DOC) => {
  try {
    const res = await localApi(new Request(`https://x/api/tools/${tool}?doc=${doc}`, {
      method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}),
    }));
    return { status: res.status, body: await res.json().catch(() => null) };
  } catch (e) {
    return { status: 0, body: { ok: false, error_code: "local_threw",
      context: { detail: String((e && e.message) || e).slice(0, 300) } } };
  }
};
const both = async (tool, args) => [await callServer(tool, args), await localCall(tool, args)];

/** **把服务端的完整原子按 seq 顺序喂进本地文档 ✓**（判据装配 ✓，见文件头）。 */
async function seedLocal(serverDoc, token, localDoc) {
  const res = await nativeFetch(`${BASE}/api/atoms?doc=${encodeURIComponent(serverDoc)}&since=0&token=${token}`);
  const body = await res.json().catch(() => null);
  if (!body || body.ok !== true || !Array.isArray(body.atoms)) {
    console.log(`⊘ 不可判：GET /api/atoms 读不出（${res.status} ${JSON.stringify(body)?.slice(0, 160)}）`
      + " ⇒ 前提不成立，不是产品问题");
    process.exit(3);
  }
  const db = await storeLib.open();
  for (const atom of body.atoms) await storeLib.putAtom(db, localDoc, structuredClone(atom));
  return body.atoms.length;
}

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
const show = (value) => String(JSON.stringify(value)).slice(0, 400);
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
// **服务端 HTTP 信封 ✗ 不是工具契约** ✓（`server.rs:2866` 的 `pre_create` 给**每个**工具响应
//   无条件写 `created`／`opened`；`timings` 是逐次不同 ✓）⇒ **∴ 比对前两边都剥掉** ✓。
//   **∴ 本地没有那一层**（直调 `api-local.js` ✓）⇒ **∴ 剥掉后仍逐字段比 ✗** ⇒ 多一个字段就红 ✓。
const SERVER_ENVELOPE = ["created", "opened", "timings"];
const stripLocal = (body) => {
  const out = { ...(body || {}) };
  for (const key of SERVER_ENVELOPE) delete out[key];
  delete out.server;
  return out;
};
const detailOf = (res) => (res.body && res.body.context && res.body.context.detail) ?? null;

function compareBody(label, srv, loc, normalize = (x) => x) {
  check(srv.status === 200 && srv.body && srv.body.ok !== false,
    `${label}：服务端应 200 ✗ 状态 ${srv.status}｜${show(srv.body)}`);
  check(loc.status === 200 && loc.body && loc.body.ok !== false,
    `${label}：本地应 200 ✗ 状态 ${loc.status}｜${show(loc.body)}`);
  if (!srv.body || !loc.body) return;
  check(loc.body.server === false, `${label}：本地应标 server:false ✗ ${show(loc.body.server)}`);
  const vs = normalize(stripLocal(srv.body)), vl = normalize(stripLocal(loc.body));
  check(same(vs, vl), `${label}：全字段不一致 ✗\n     首个差异 ${firstDiff(vs, vl) || "（结构相等但 same 判不等 ⇒ 数值口径）"}`);
}
function compareError(label, srv, loc, status, code, expected) {
  if (!srv.body || !loc.body) { check(false, `${label}：有一边读不出 JSON ✗`); return; }
  check(srv.status === status, `${label}：服务端状态码应为 ${status} ✗ 实测 ${srv.status}｜${show(srv.body)}`);
  check(loc.status === status, `${label}：本地状态码应为 ${status} ✗ 实测 ${loc.status}｜${show(loc.body)}`);
  check(srv.body.error_code === code, `${label}：服务端错误码应为 ${code} ✗ 实测 ${srv.body.error_code}`);
  check(loc.body.error_code === code, `${label}：本地错误码应为 ${code} ✗ 实测 ${loc.body.error_code}`);
  check(detailOf(srv) === detailOf(loc),
    `${label}：两边文案不同 ✗\n     服务端 ${detailOf(srv)}\n     本地   ${detailOf(loc)}`);
  if (expected !== undefined) {
    check(detailOf(loc) === expected, `${label}：本地文案与判据的期望不同 ✗\n     期望 ${expected}\n     实测 ${detailOf(loc)}`);
  }
}

// ══ 前序：真服务端落笔（label／group／revert 都有）══════════════════════════════
const RED = { r: 255, g: 0, b: 0, a: 255 };
const STROKES = [
  { key: "B1", args: { points: [[10, 10, 0.2], [40, 30, 0.8], [70, 50, 0.5]],
                       label: "eye", group_id: "G1" } },
  { key: "B2", args: { points: [[15, 40, 1], [45, 60, 0.4]], label: "eye", group_id: "G1" } },
  { key: "B3", args: { points: [[60, 20, 1], [80, 25, 0.6]], label: "mouth" } },
  { key: "B4", args: { points: [[5, 60, 1], [20, 65, 0.5]], label: "ear", group_id: "GX" } },
];
const setupOps = [
  ["create_layer", { layer_id: "L1", name: "笔迹层" }],
  ...STROKES.map((s) => ["brush_stroke", {
    layer_id: "L1", brush: "100%_Opaque", size: 8, color: RED, ...s.args,
  }]),
  ["revert_group", { group_id: "GX" }],
  // **矢量笔触 ✓**：点列在 `payload.data` 里（不是 `source`）⇒ **∴ 走回退分支 ✓**
  //   （服务端历史上整条被 `continue` 掉 ✓）＋ `data.color` 是**数组** ⇒ 顺带验 `normalize_color` ✓。
  ["draw_stroke", { layer_id: "L1", label: "vec",
                    data: { points: [[12, 12], [30, 22]], size: 3, color: [0, 0, 1, 1] } }],
];
for (const [tool, args] of setupOps) {
  const res = await callServer(tool, args);
  if (res.status !== 200 || !res.body || res.body.ok === false) {
    console.log(`⊘ 不可判：前序 ${tool} 服务端拒绝（${res.status} ${show(res.body)}）⇒ 前提不成立，不是产品问题`);
    process.exit(3);
  }
}
const localSeeded = await seedLocal(SERVER_DOC, TOKEN, LOCAL_DOC);
check(localSeeded >= setupOps.length, `本地装配只喂了 ${localSeeded} 条原子 ⇒ 少于前序操作数 ✗`);

// ══ §A `get_stroke_log`：只列笔触、顺序、撤销排除、压力 ══════════════════════════
{
  const [s, l] = await both("get_stroke_log", {});
  compareBody("§A get_stroke_log 全表", s, l);
  const body = s.body || {};
  const entries = Array.isArray(body.entries) ? body.entries : [];
  check(body.ok === true && body.returned === entries.length && body.total === entries.length,
    `§A returned/total 应等于 entries 条数 ✗ ${show([body.returned, body.total, entries.length])}`);
  check(body.total === 4, `§A total 应为 4 条 live 笔（B4 已被 revert ✓；含矢量笔 ✓）✗ 实测 ${show(body.total)}`);
  check(body.reverted_excluded === 1,
    `§A reverted_excluded 应为 1（B4 ✓）✗ 实测 ${show(body.reverted_excluded)}`);
  check(same(entries.map((e) => e.kind), ["brush", "brush", "brush", "draw"]),
    `§A kind 应三支 brush ＋ 一条 draw（矢量笔走 data 回退 ✓）✗ ${show(entries.map((e) => e.kind))}`);
  check(entries.every((e) => e.layer_id === "L1"),
    `§A layer_id 应全是 L1 ✗ ${show(entries.map((e) => e.layer_id))}`);
  check(same(entries.map((e) => e.points_count), [3, 2, 2, 2]),
    `§A points_count 应 [3,2,2,2] ✗ ${show(entries.map((e) => e.points_count))}`);
  // **判据自己算压力** ✓（不是抄服务端的输出 ✓）。
  check(same(entries[0] && entries[0].pressure, { min: 0.2, max: 0.8, first: 0.2, last: 0.5 }),
    `§A B1 压力应为 {0.2,0.8,0.2,0.5} ✗ ${show(entries[0] && entries[0].pressure)}`);
  check(same(entries[0] && entries[0].first_point, { x: 10, y: 10 })
      && same(entries[0] && entries[0].last_point, { x: 70, y: 50 }),
    `§A B1 首尾点不对 ✗ ${show(entries[0] && [entries[0].first_point, entries[0].last_point])}`);
  check(entries[1] && entries[1].first_point && entries[1].first_point.x === 15
      && entries[1].first_point.y === 40,
    `§A B2 首点应为 (15,40) ✗ ${show(entries[1] && entries[1].first_point)}`);
  const seqs = entries.map((e) => e.seq);
  check(seqs.every((v, i) => i === 0 || v > seqs[i - 1]),
    `§A 缺省顺序应按 seq 升序 ✗ ${show(seqs)}`);
  // **矢量笔的 `data.color` 是数组 ⇒ `normalize_color` 要把它变成 `{r,g,b,a}` 整数** ✓
  //   （服务端 `normalize_color` ✓：缺 alpha 补 255 ✓；**不是**原样回数组 ✓）。
  check(same(entries[3] && entries[3].color, { r: 0, g: 0, b: 1, a: 1 }),
    `§A 矢量笔颜色应归一成 {0,0,1,1} ✗ ${show(entries[3] && entries[3].color)}`);
  check(entries[3] && entries[3].pressure === null,
    `§A 矢量笔没给压力 ⇒ pressure 应是 null ✗ ${show(entries[3] && entries[3].pressure)}`);
  check(body.next_offset === null && body.truncated === false,
    `§A 缺省一页装得下 ⇒ next_offset=null、truncated=false ✗ ${show([body.next_offset, body.truncated])}`);
  check(body.total + body.reverted_excluded === 5,
    `§A 恒等式 total + reverted_excluded = 全部匹配（5）✗ ${show([body.total, body.reverted_excluded])}`);
}
{
  const [s, l] = await both("get_stroke_log", { limit: 2 });
  compareBody("§A2 limit=2", s, l);
  check(s.body && s.body.returned === 2 && s.body.total === 4 && s.body.truncated === true
      && s.body.next_offset === 2,
    `§A2 limit=2 ⇒ returned 2／total 4／truncated／next_offset 2 ✗ ${show(s.body)}`);
  check(s.body && s.body.entries.length === 2 && s.body.entries[0].points_count === 3,
    `§A2 第一页应是 B1/B2 ✗ ${show(s.body && s.body.entries.map((e) => e.points_count))}`);
}
{
  const [s, l] = await both("get_stroke_log", { limit: 2, offset: 2 });
  compareBody("§A3 offset=2", s, l);
  check(s.body && s.body.returned === 2 && s.body.total === 4 && s.body.truncated === false
      && s.body.next_offset === null,
    `§A3 offset=2 ⇒ 剩 2 条且没有下一页 ✗ ${show(s.body)}`);
  check(s.body && same(s.body.entries.map((e) => e.points_count), [2, 2]),
    `§A3 第二页应是 B3＋矢量笔 ✗ ${show(s.body && s.body.entries.map((e) => e.points_count))}`);
}
{
  const [s, l] = await both("get_stroke_log", { order: "newest" });
  compareBody("§A4 order=newest", s, l);
  const seqs = (s.body && s.body.entries || []).map((e) => e.seq);
  check(seqs.length === 4 && seqs[0] > seqs[1] && seqs[1] > seqs[2] && seqs[2] > seqs[3],
    `§A4 order=newest 应按 seq 降序 ✗ ${show(seqs)}`);
  check(s.body && s.body.next_offset === null,
    `§A4 全部装得下 ⇒ next_offset=null ✗ ${show(s.body && s.body.next_offset)}`);
}
{
  const [s, l] = await both("get_stroke_log", { layer_id: "L1" });
  compareBody("§A5 layer_id=L1", s, l);
  check(s.body && s.body.total === 4, `§A5 layer_id=L1 应 4 条 ✗ ${show(s.body && s.body.total)}`);
  const [s2, l2] = await both("get_stroke_log", { layer_id: "NOPE" });
  compareBody("§A6 layer_id=NOPE", s2, l2);
  check(s2.body && s2.body.total === 0 && s2.body.entries.length === 0,
    `§A6 不存在的层 ⇒ 0 条（不是 404 ✓）✗ ${show(s2.body)}`);
}
{
  const [s, l] = await both("get_stroke_log", { group_id: "G1" });
  compareBody("§A7 group_id=G1", s, l);
  // **`get_stroke_log` 的条目**没有 `label` ✓（服务端也不给 ✓）⇒ 用总条数与首点认 B1/B2 ✓。
  check(s.body && s.body.total === 2
      && same((s.body.entries || []).map((e) => e.first_point), [{ x: 10, y: 10 }, { x: 15, y: 40 }]),
    `§A7 group_id=G1 应 2 条（B1/B2）✗ ${show(s.body)}`);
}
{
  // **`since_seq` 跳过已读** ✓：取第二条的 seq ⇒ 只剩后两条 ✓（判据自己从响应里取号 ✓）。
  const sAll = (await both("get_stroke_log", {}))[0];
  const since = sAll.body && sAll.body.entries && sAll.body.entries[1] && sAll.body.entries[1].seq;
  check(typeof since === "number", `§A8 取不到第二条 seq ✗ ${show(sAll.body)}`);
  const [s, l] = await both("get_stroke_log", { since_seq: since });
  compareBody("§A8 since_seq 跳过已读", s, l);
  check(s.body && s.body.total === 2 && s.body.entries[0].points_count === 2,
    `§A8 since_seq 之后应只剩 B3＋矢量笔 ✗ ${show(s.body)}`);
}

// ══ §B `list_atoms`：倒序、五条筛选、hidden_non_stroke、分页 ═══════════════════
{
  const [s, l] = await both("list_atoms", { limit: 500, want_thumb: false });
  compareBody("§B list_atoms 全表（含非笔迹）", s, l);
  const body = s.body || {};
  const atoms = Array.isArray(body.atoms) ? body.atoms : [];
  check(body.ok === true && body.count === atoms.length && body.limit === 500,
    `§B count/limit 不对 ✗ ${show([body.count, body.limit, atoms.length])}`);
  check(body.truncated === false && body.has_more === false,
    `§B 未满页 ⇒ truncated/has_more 都 false ✗ ${show([body.truncated, body.has_more])}`);
  const seqs = atoms.map((a) => a.seq);
  check(seqs.every((v, i) => i === 0 || v < seqs[i - 1]),
    `§B 应按 seq 倒序 ✗ ${show(seqs)}`);
  check(atoms.filter((a) => a.kind === "import_image").length === 3,
    `§B 三条 live 笔迹应是 import_image ✗ ${show(atoms.map((a) => a.kind))}`);
  check(atoms.filter((a) => a.kind === "import_image").every((a) => a.origin === "drawn"),
    `§B 笔迹的 origin 必须是 drawn（来源，不是存储形态 ✓）✗ ${show(atoms.map((a) => a.origin))}`);
  check(atoms.every((a) => a.stored_origin === null || typeof a.stored_origin === "string"),
    `§B stored_origin 应是字符串或 null ✗ ${show(atoms.map((a) => a.stored_origin))}`);
  const labels = Object.fromEntries(atoms.filter((a) => a.kind === "import_image")
    .map((a) => [a.label, (atoms.filter((x) => x.kind === "import_image" && x.label === a.label).length)]));
  check(labels.eye === 2 && labels.mouth === 1,
    `§B 标签应是 eye×2／mouth×1（B4 已撤 ✓）✗ ${show(labels)}`);
}
{
  const [s, l] = await both("list_atoms", { limit: 500, include_non_stroke: true, want_thumb: false });
  compareBody("§B2 include_non_stroke=true", s, l);
  const [s0] = await both("list_atoms", { limit: 500, want_thumb: false });
  check(s.body && s0.body
      && s.body.total === s0.body.total + s0.body.hidden_non_stroke,
    `§B2 恒等式 total(include) = total(default) + hidden_non_stroke ✗ `
    + `${show([s.body?.total, s0.body?.total, s0.body?.hidden_non_stroke])}`);
  check(s.body && s.body.hidden_non_stroke === 0,
    `§B2 include_non_stroke=true ⇒ hidden 恒 0 ✗ ${show(s.body && s.body.hidden_non_stroke)}`);
  // **`origin` 是来源 ✓**：建文档／建层的原子是 system ✓（缺省列表把非笔迹藏了 ⇒ 必须在这份里查 ✓）。
  const systemAtoms = (s.body && s.body.atoms || [])
    .filter((a) => a.kind === "create_layer" || a.kind === "create_document");
  check(systemAtoms.length >= 2 && systemAtoms.every((a) => a.origin === "system"),
    `§B2 建文档／建层的 origin 应为 system（≥2 条）✗ ${show(systemAtoms)}`);
}
{
  const [s, l] = await both("list_atoms", { label: "eye", want_thumb: false });
  compareBody("§B3 label=eye", s, l);
  check(s.body && s.body.total === 2 && s.body.atoms.every((a) => a.label === "eye"),
    `§B3 label=eye 应 2 条且都带该标签 ✗ ${show(s.body)}`);
  const [s2, l2] = await both("list_atoms", { label: "nope", want_thumb: false });
  compareBody("§B4 label=nope", s2, l2);
  check(s2.body && s2.body.total === 0, `§B4 没有该标签 ⇒ 0 ✗ ${show(s2.body && s2.body.total)}`);
}
{
  const [s, l] = await both("list_atoms", { group_id: "G1", want_thumb: false });
  compareBody("§B5 group_id=G1", s, l);
  check(s.body && s.body.total === 2 && s.body.atoms.every((a) => a.group_id === "G1"),
    `§B5 group_id=G1 应 2 条且都同组 ✗ ${show(s.body)}`);
  const [s2, l2] = await both("list_atoms", { group_id: "GX", want_thumb: false });
  compareBody("§B6 group_id=GX（已撤销）", s2, l2);
  check(s2.body && s2.body.total === 0,
    `§B6 GX 的笔已撤销 ⇒ 0 条 ✗ ${show(s2.body && s2.body.total)}`);
}
{
  const [s, l] = await both("list_atoms", { brush: "OPAQUE", want_thumb: false });
  compareBody("§B7 brush=OPAQUE（大小写不敏感）", s, l);
  check(s.body && s.body.total === 3,
    `§B7 三支笔都用 100%_Opaque ⇒ 3 条 ✗ ${show(s.body && s.body.total)}`);
  const [s2, l2] = await both("list_atoms", { brush: "zzz", want_thumb: false });
  compareBody("§B8 brush=zzz", s2, l2);
  check(s2.body && s2.body.total === 0, `§B8 没有该笔刷 ⇒ 0 ✗ ${show(s2.body && s2.body.total)}`);
}
{
  const [s, l] = await both("list_atoms", { bbox: [200, 200, 10, 10], want_thumb: false });
  compareBody("§B9 bbox 画布外", s, l);
  check(s.body && s.body.total === 0, `§B9 画布外框 ⇒ 0 条 ✗ ${show(s.body && s.body.total)}`);
  const [s2, l2] = await both("list_atoms", { bbox: [1, 2, 3], want_thumb: false });
  compareBody("§B10 bbox 形状非法 ⇒ 忽略（不报错）", s2, l2);
  check(s2.body && s2.body.total === 3,
    `§B10 bbox 非 4 元组应被忽略 ⇒ 3 条 ✗ ${show(s2.body && s2.body.total)}`);
}
{
  const [s, l] = await both("list_atoms", { limit: 2, want_thumb: false });
  compareBody("§B11 limit=2", s, l);
  check(s.body && s.body.count === 2 && s.body.total === 3 && s.body.has_more === true
      && s.body.truncated === true && s.body.offset === 0,
    `§B11 limit=2 ⇒ count 2／total 3／has_more／offset 0 ✗ ${show(s.body)}`);
  const [s2, l2] = await both("list_atoms", { limit: 2, offset: 2, want_thumb: false });
  compareBody("§B12 offset=2", s2, l2);
  check(s2.body && s2.body.count === 1 && s2.body.has_more === false,
    `§B12 offset=2 ⇒ 剩 1 条、没有更多 ✗ ${show(s2.body)}`);
  const [s3, l3] = await both("list_atoms", { limit: 0, want_thumb: false });
  compareBody("§B13 limit=0", s3, l3);
  check(s3.body && s3.body.count === 0 && s3.body.total === 3 && s3.body.truncated === false
      && s3.body.has_more === false,
    `§B13 limit=0 ⇒ 0 条、total 照数、不许报截断（自相矛盾 ✓）✗ ${show(s3.body)}`);
}

// ══ §C `get_atom`：七个元数据字段 ══════════════════════════════════════════════
{
  const listed = await callServer("list_atoms", { limit: 500, want_thumb: false });
  const first = (listed.body && listed.body.atoms || []).find((a) => a.label === "eye");
  check(Boolean(first), `§C 服务端 list_atoms 里找不到 eye 笔 ✗ ${show(listed.body && listed.body.atoms)}`);
  if (first) {
    const [s, l] = await both("get_atom", { atom_id: first.atom_id });
    compareBody("§C get_atom", s, l);
    const keys = ["atom_id", "seq", "kind", "actor", "session", "timestamp",
      "message", "label", "changeset_id", "parents", "payload"];
    check(s.body && keys.every((k) => k in s.body),
      `§C get_atom 少字段 ✗ 缺 ${show(keys.filter((k) => !(s.body && k in s.body)))}`);
    check(s.body && s.body.label === "eye",
      `§C get_atom 应读回 label:eye ✗ ${show(s.body && s.body.label)}`);
    check(s.body && Array.isArray(s.body.parents),
      `§C parents 应是数组 ✗ ${show(s.body && s.body.parents)}`);
  }
}

// ══ §D 错误逐字 ═════════════════════════════════════════════════════════════════
{
  const [s, l] = await both("get_stroke_log", { limit: -1 });
  compareError("§D1 get_stroke_log limit=-1", s, l, 400, "invalid_argument",
    "参数 limit 必须是不小于 0 的整数（它是计数／序号，不能是负数或小数），得到 -1；"
    + "要「从头开始」就**别传这个参数**（缺省即 0），传 0 也可以");
  const [s2, l2] = await both("get_stroke_log", { since_seq: 1.5 });
  compareError("§D2 get_stroke_log since_seq=1.5", s2, l2, 400, "invalid_argument",
    "参数 since_seq 必须是不小于 0 的整数（它是计数／序号，不能是负数或小数），得到 1.5；"
    + "要「从头开始」就**别传这个参数**（缺省即 0），传 0 也可以");
  const [s3, l3] = await both("list_atoms", { limit: -2 });
  compareError("§D3 list_atoms limit=-2", s3, l3, 400, "invalid_argument");
  const [s4, l4] = await both("list_atoms", { thumb_size: 4294967296 });
  compareError("§D4 list_atoms thumb_size=2^32（不许回绕成 0）", s4, l4, 400, "invalid_argument",
    "thumb_size 太大（4294967296）⇒ 缩略图边长上限 4294967295（不许回绕成 0）");
  const [s5, l5] = await both("list_atoms", { thumbs: true });
  compareError("§D5 list_atoms 未知键 thumbs", s5, l5, 400, "invalid_argument");
  const [s6, l6] = await both("list_atoms", { limit: "5" });
  compareError("§D6 list_atoms limit 类型错", s6, l6, 400, "invalid_argument");
  const [s7, l7] = await both("get_atom", {});
  compareError("§D7 get_atom 缺 atom_id", s7, l7, 400, "invalid_argument");
  const [s8, l8] = await both("get_atom", { atom_id: "NOPE" });
  compareError("§D8 get_atom 不存在", s8, l8, 404, "reference_not_found");
}

// ══ §E 缩略图：只比张数／跳过数／PNG 头宽高（不比字节 ✓）═══════════════════════
// **∴ 为什么用 `import_image` 而不是 `draw_shape` ✗**：缩略图只认**顶层 `region`** ✓，
//   而 `draw_shape` 的净荷**没有** `region`（只有 `bbox` ✓）⇒ **∴ 服务端根本不出缩略图 ✗**
//   ⇒ **∴ 那样这条判据会静默空转 ✓**（实测：`region:null`、缩略图 0 张 ✓）。
//   **∴ 做法 ✗**：判据自己造一块 **12×8 纯蓝裸像素** ✓ ⇒ 两边各传一次 `/api/blob`
//   ＋ 各 `import_image` 一次 ✓（区域 `4,4,12,8` 是**判据自己给的** ⇒ 尺寸期望可写死 ✓）。
{
  const RW = 12, RH = 8, REGION = { x: 4, y: 4, w: RW, h: RH };
  const raw = new Uint8Array(RW * RH * 4);
  for (let i = 0; i < RW * RH; i += 1) {
    raw[i * 4] = 0; raw[i * 4 + 1] = 0; raw[i * 4 + 2] = 255; raw[i * 4 + 3] = 255;
  }
  const layerRes = await callServer("create_layer", { layer_id: "TL", name: "缩略图层" }, TOKEN2, SERVER_DOC2);
  check(layerRes.status === 200 && layerRes.body && layerRes.body.ok !== false,
    `§E 前序建层失败 ✗ ${layerRes.status} ${show(layerRes.body)}`);
  // **装配必须在两边各自的 `import_image` 之前** ✓：否则 `seedLocal` 会把**服务端那条**
  //   import_image 也喂进本地 ⇒ 本地多出一张缩略图（实测 2 vs 1 ✗）。
  await seedLocal(SERVER_DOC2, TOKEN2, LOCAL_DOC2);
  // **服务端**：先传裸像素 ⇒ 拿服务端 hash ⇒ import_image。
  const upS = await nativeFetch(`${BASE}/api/blob?doc=${encodeURIComponent(SERVER_DOC2)}&token=${TOKEN2}`, {
    method: "POST", headers: { "content-type": "image/x-yanshi-raw" }, body: raw,
  }).then((r) => r.json()).catch(() => null);
  check(Boolean(upS && upS.ok === true && typeof upS.blob_hash === "string"),
    `§E 服务端 blob 上传失败 ✗ ${show(upS)}`);
  const impS = await callServer("import_image", {
    layer_id: "TL", object_id: "T1", region: REGION,
    bitmap: { blob_hash: upS && upS.blob_hash, mime_type: "image/x-yanshi-raw", size: upS && upS.size },
  }, TOKEN2, SERVER_DOC2);
  check(impS.status === 200 && impS.body && impS.body.ok !== false,
    `§E 服务端 import_image 失败 ✗ ${impS.status} ${show(impS.body)}`);
  // **本地**：装配已完成（上面 ✓），走本地的 blob 上传与 import_image ✓。
  const upLRes = await localApi(new Request(`https://x/api/blob?doc=${LOCAL_DOC2}`, {
    method: "POST", headers: { "content-type": "image/x-yanshi-raw" }, body: raw,
  }));
  const upL = await upLRes.json().catch(() => null);
  check(Boolean(upL && upL.ok === true && typeof upL.blob_hash === "string"),
    `§E 本地 blob 上传失败 ✗ ${show(upL)}`);
  const impL = await localCall("import_image", {
    layer_id: "TL", object_id: "T1", region: REGION,
    bitmap: { blob_hash: upL && upL.blob_hash, mime_type: "image/x-yanshi-raw", size: upL && upL.size },
  }, LOCAL_DOC2);
  check(impL.status === 200 && impL.body && impL.body.ok !== false,
    `§E 本地 import_image 失败 ✗ ${impL.status} ${show(impL.body)}`);
  // **本地写入路径也要盖 `origin` 戳** ✓（服务端 `append_validated` 在落账时补 ✓）：
  //   **∴ 断言断的是**本地自己写的那条 ✗（不是装配进来的 ✓）⇒ `stored_origin` 必须非 null ✓。
  const lOwn = await localCall("list_atoms", { include_non_stroke: true, want_thumb: false }, LOCAL_DOC2);
  const lOwnAtom = (lOwn.body && lOwn.body.atoms || []).find((x) => x.kind === "import_image");
  check(Boolean(lOwnAtom) && lOwnAtom.origin === "imported" && lOwnAtom.stored_origin === "imported",
    `§E 本地写入的 import_image 应盖 origin 戳（origin/stored_origin 都 = imported）✗ ${show(lOwnAtom)}`);
  const pngSize = (b64) => {
    const bytes = Buffer.from(String(b64), "base64");
    if (bytes.length < 24) return null;
    if (bytes[0] !== 0x89 || bytes[1] !== 0x50 || bytes[2] !== 0x4e || bytes[3] !== 0x47) return null;
    const be = (offset) => bytes.readUInt32BE(offset);
    return { w: be(16), h: be(20) };
  };
  for (const [label, args, want] of [
    ["§E1 缺省尺寸", { include_non_stroke: true }, { w: RW, h: RH }],
    ["§E2 thumb_size=4", { include_non_stroke: true, thumb_size: 4 }, { w: 4, h: 2 }],
  ]) {
    const ts = await callServer("list_atoms", args, TOKEN2, SERVER_DOC2);
    const tl = await localCall("list_atoms", args, LOCAL_DOC2);
    check(ts.status === 200 && ts.body && ts.body.ok !== false,
      `${label}：服务端应 200 ✗ ${show(ts.body)}`);
    check(tl.status === 200 && tl.body && tl.body.ok !== false,
      `${label}：本地应 200 ✗ ${tl.status} ${show(tl.body)}`);
    const a = (ts.body && ts.body.atoms || []).find((x) => x.kind === "import_image");
    const b = (tl.body && tl.body.atoms || []).find((x) => x.kind === "import_image");
    check(Boolean(a && b), `${label}：两边都应有 import_image 条目 ✗ ${show([a, b])}`);
    // **不许空转** ✓：这一份文档只有一个带 region 的原子 ⇒ 缩略图**恰好 1 张** ✓。
    check(ts.body && ts.body.thumbnail_count === 1,
      `${label}：服务端缩略图应 1 张 ✗ ${show(ts.body && ts.body.thumbnail_count)}`);
    check(tl.body && tl.body.thumbnail_count === 1,
      `${label}：本地缩略图应 1 张 ✗ ${show(tl.body && tl.body.thumbnail_count)}`);
    check((ts.body && ts.body.thumbnail_count) === (tl.body && tl.body.thumbnail_count),
      `${label}：缩略图张数不同 ✗ 服务端 ${show(ts.body && ts.body.thumbnail_count)}`
      + `｜本地 ${show(tl.body && tl.body.thumbnail_count)}`);
    check(((ts.body && ts.body.thumbnail_skipped) || []).length
          === ((tl.body && tl.body.thumbnail_skipped) || []).length,
      `${label}：跳过条数不同 ✗ ${show([ts.body && ts.body.thumbnail_skipped, tl.body && tl.body.thumbnail_skipped])}`);
    if (a && b && a.thumbnail && b.thumbnail) {
      const sa = pngSize(a.thumbnail), sb = pngSize(b.thumbnail);
      check(Boolean(sa && sb), `${label}：缩略图不是 PNG ✗ ${show([sa, sb])}`);
      if (sa && sb) {
        check(sa.w === sb.w && sa.h === sb.h,
          `${label}：PNG 宽高不同 ✗ 服务端 ${show(sa)}｜本地 ${show(sb)}`);
        // **期望尺寸是判据自己造的** ✓（区域 12×8 ⇒ 缺省 12×8；`thumb_size:4` 按长边缩成 4×2 ✓）。
        check(sa.w === want.w && sa.h === want.h,
          `${label}：PNG 尺寸应为 ${want.w}×${want.h} ✗ 服务端 ${show(sa)}`);
        check(sb.w === want.w && sb.h === want.h,
          `${label}：PNG 尺寸应为 ${want.w}×${want.h} ✗ 本地 ${show(sb)}`);
      }
    }
  }
}

if (bad.length) {
  console.log(`✗ 失败 ${bad.length} 条（通过 ${okCount}）：`);
  for (const line of bad) console.log("   " + line);
  process.exit(1);
}
console.log(`✓ tool-pwa-atom-log：${okCount} 条断言全过（get_stroke_log／list_atoms／get_atom 与服务端同答案 ✓）`);
