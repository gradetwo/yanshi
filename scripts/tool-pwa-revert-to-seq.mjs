#!/usr/bin/env node
// **判据：PWA 本地的 `revert_to_seq` 与服务端同答案** ✓（台账「第 六百六十七 节」）。
//
// **∴ 装配手法 ✗**：本地 `brush_stroke` 在 node 里跑不了（要浏览器的 `brush-local.js` ✓）
//   ⇒ 先在真服务端建文档／落笔／`revert_atoms`／`declare_head` ✓ ⇒ 用 `GET /api/atoms`
//   取完整原子按 seq 喂进本地 ✓（与 `tool-pwa-atom-log` 同一手法 ✓，是**判据装配**不是产品路径 ✓）。
//
// **∴ 它断什么 ✗**（每条都要能红 ✓）：
//   §A 基本回退：`reverted_count`／目标集合（判据自己从 setup 的笔算 ✓）／回退后 `get_stroke_log`
//      与 `list_atoms` 两边全字段相同 ✓；
//   §B 空集支：**没有 `reverted`／`failed` 两个键** ✓（形状照实测，不补空数组 ✓）、日志不变 ✓；
//   §C 朴素集合口径：已撤的笔跳过 ✓；**撤过又 `reapply` 回来的照样跳过** ✓（服务端现状，照抄 ✓）；
//   §D 求值起点之后的笔才能撤：`declare_head` 之前的笔**逐条失败** ✓（错误串逐字比 ✓，`ok:false` ✓）；
//   §E 入口与处理器的六种错误逐字 ✓（缺／null／字符串／负数／小数／未知键 ✓）；
//   §F **只碰有 `source` 的原子** ✓：`draw_shape`（无 `source`）在目标之上也不被撤 ✓。
//
// **∴ 前提 ✗**：活服务端（`run-criteria-local.sh` ✓）＋ 不过期的真 wasm 内核（旧了 `exit(3)` ✓）。
process.on("uncaughtException", (error) => {
  console.error(String((error && error.stack) || error).slice(0, 1200));
  process.exit(3);
});
import { pathToFileURL } from "node:url";

const BASE = process.argv[2];
if (!BASE || !/^https?:\/\//.test(BASE)) {
  console.log("⊘ 不可判：argv[2] 缺服务端地址 ⇒ 请用 bash scripts/run-criteria-local.sh scripts/tool-pwa-revert-to-seq.mjs 跑"
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
// **服务端 HTTP 信封 ✗ 不是工具契约** ✓（`server.rs` 的 `pre_create` 给每个响应写
//   `created`／`opened`；`timings` 逐次不同 ✓）⇒ **∴ 比对前两边都剥掉** ✓。
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
/** **逐条失败时 `ok:false` 是正确答案** ✓（`revert_to_seq` 的 `ok` 由 `failed` 决定 ✓）⇒
 *  这一支**只比状态码与全字段**，不断言 `ok !== false` ✓（否则会把「照实报失败」判成缺陷 ✗）。
 *  **∴ 状态码要两边相同 ✗**（服务端把工具级 `ok:false` 映射成 400 ✓ —— `server.rs:2872`；
 *   本地**同映射** ✓ ⇒ 这一条就是在钉「别一边 400 一边 200」✗）。
 */
function compareBodyAllowFail(label, srv, loc) {
  check(srv.status === loc.status,
    `${label}：状态码应两边相同 ✗ 服务端 ${srv.status}｜本地 ${loc.status}（${show(loc.body)}）`);
  if (!srv.body || !loc.body) return;
  check(loc.body.server === false, `${label}：本地应标 server:false ✗ ${show(loc.body.server)}`);
  const vs = strip(srv.body), vl = strip(loc.body);
  check(same(vs, vl), `${label}：全字段不一致 ✗\n     首个差异 ${firstDiff(vs, vl) || "（结构相等但 same 判不等 ⇒ 数值口径）"}`);
}
function compareError(label, srv, loc) {
  if (!srv.body || !loc.body) { check(false, `${label}：有一边读不出 JSON ✗`); return; }
  check(srv.status === 400, `${label}：服务端状态码应为 400 ✗ 实测 ${srv.status}｜${show(srv.body)}`);
  check(loc.status === 400, `${label}：本地状态码应为 400 ✗ 实测 ${loc.status}｜${show(loc.body)}`);
  check(srv.body.error_code === "invalid_argument",
    `${label}：服务端错误码应为 invalid_argument ✗ 实测 ${srv.body.error_code}`);
  check(loc.body.error_code === "invalid_argument",
    `${label}：本地错误码应为 invalid_argument ✗ 实测 ${loc.body.error_code}`);
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
/** **把服务端的完整原子按 seq 顺序喂进本地文档 ✓**（判据装配，见文件头）。 */
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

const RED = { r: 255, g: 0, b: 0, a: 255 };
const stroke = (points, label) => ["brush_stroke", {
  layer_id: "L1", brush: "100%_Opaque", size: 8, color: RED, points, label,
}];

/** **一个 case 开一份服务端文档 ＋ 一份本地文档** ✓（内核 ①：别共用文档 —— 前一段的墨会污染后一段 ✓）。
 *  **∴ `seed:false` 时由调用方在「补完服务端前序」之后再 `ctx.seed()`** ×（`store.putAtom` 每次都会
 *  分配新 seq ✓ ⇒ **不能重播**，重播 = 同一批原子在本地出现两次 ✗）。
 */
async function makeCase(key, ops, { seed = true } = {}) {
  const serverDoc = `rsrv-${key}-${UNIQ}`;
  const localDoc = `rloc-${key}-${UNIQ}`;
  const token = await createDoc(serverDoc, 96, 72);
  for (const [tool, args] of ops) {
    const res = await callServer(tool, args, token, serverDoc);
    if (res.status !== 200 || !res.body || res.body.ok === false) {
      console.log(`⊘ 不可判：前序 ${tool}（${key}）服务端拒绝（${res.status} ${show(res.body)}）`
        + " ⇒ 前提不成立，不是产品问题");
      process.exit(3);
    }
  }
  const seedOnce = async () => {
    const seeded = await seedLocal(serverDoc, token, localDoc);
    check(seeded >= ops.length, `${key}：本地装配只喂了 ${seeded} 条原子 ⇒ 少于前序操作数 ✗`);
  };
  if (seed) await seedOnce();
  return {
    token, serverDoc, localDoc, seed: seedOnce,
    server: (tool, args) => callServer(tool, args, token, serverDoc),
    local: (tool, args) => localCall(tool, args, localDoc),
    both: async (tool, args) => [await callServer(tool, args, token, serverDoc),
      await localCall(tool, args, localDoc)],
  };
}
/** **缩略图 PNG 字节不是契约 ✗**：服务端与本地各用自己的编码器 ✓ ⇒ 同一张图也给出不同字节 ✓
 *  ⇒ **比表时把 `thumbnail` 统一换成占位符** ✓（**存在性／位置仍被比到** ✓ ——
 *  一边有、一边没有照样红 ✓）。判据只关心「撤销之后日志里还剩哪些原子」✗ 不关心 PNG 字节 ✓。
 */
function maskThumbnails(body) {
  const out = structuredClone(body || {});
  for (const a of (out && out.atoms) || []) {
    if (a && typeof a === "object" && "thumbnail" in a) a.thumbnail = "<png>";
  }
  return out;
}
/** **回退之后两边读出来还得一样** ✓（只看 `get_stroke_log`／缺省 `list_atoms` —— 两边生成的
 *  撤销原子 id 不同 ✗ ⇒ 带非笔迹的全表不能比 ✓）。 */
async function compareAfter(label, ctx) {
  const [sg, lg] = await ctx.both("get_stroke_log", {});
  compareBody(`${label} 之后 get_stroke_log`, sg, lg);
  const [sa, la] = await ctx.both("list_atoms", {});
  compareBody(`${label} 之后 list_atoms`,
    { ...sa, body: maskThumbnails(sa.body) }, { ...la, body: maskThumbnails(la.body) });
  return { strokes: sg.body && sg.body.entries || [], atoms: sa.body && sa.body.atoms || [] };
}

// ══ §A 基本回退：目标之上一笔不留 ═════════════════════════════════════════════
{
  const ctx = await makeCase("a", [
    ["create_layer", { layer_id: "L1", name: "笔迹层" }],
    stroke([[10, 10], [40, 30]], "b1"),
    stroke([[15, 40], [45, 60]], "b2"),
    stroke([[60, 20], [80, 25]], "b3"),
  ]);
  const before = await ctx.server("get_stroke_log", {});
  const entries = (before.body && before.body.entries) || [];
  check(entries.length === 3, `§A 前序应有 3 笔 ✗ ${show(entries.map((e) => e.seq))}`);
  const ids = entries.map((e) => e.atom_id);
  const [s, l] = await ctx.both("revert_to_seq", { seq: entries[0].seq });
  compareBody("§A revert_to_seq", s, l);
  // **判据自己算目标集合** ✓：不是抄服务端的 `reverted` ✓。
  check(same(s.body && s.body.reverted, [ids[1], ids[2]]),
    `§A 应撤销第 2／3 笔（顺序照日志）✗ ${show(s.body && s.body.reverted)}`);
  check(s.body && s.body.reverted_count === 2 && s.body.failed_count === 0
      && s.body.ok === true && s.body.seq === entries[0].seq,
    `§A 计数／ok／seq 不对 ✗ ${show(s.body)}`);
  const after = await compareAfter("§A", ctx);
  check(after.strokes.length === 1 && after.strokes[0].atom_id === ids[0],
    `§A 回退后应只剩第 1 笔 ✗ ${show(after.strokes.map((e) => e.atom_id))}`);
}

// ══ §B 空集支：形状里没有 `reverted`／`failed`，日志不动 ═══════════════════════
{
  const ctx = await makeCase("b", [
    ["create_layer", { layer_id: "L1", name: "笔迹层" }],
    stroke([[10, 10], [20, 20]], "b1"),
    stroke([[30, 30], [40, 40]], "b2"),
  ]);
  const before = await ctx.server("get_stroke_log", {});
  const last = ((before.body && before.body.entries) || []).slice(-1)[0];
  const [s, l] = await ctx.both("revert_to_seq", { seq: (last && last.seq || 0) + 1000 });
  compareBody("§B 空集", s, l);
  check(s.body && s.body.ok === true && s.body.reverted_count === 0 && s.body.failed_count === 0,
    `§B 空集计数不对 ✗ ${show(s.body)}`);
  check(s.body && s.body.note === "没有需要撤销的笔迹（seq 大于它的一笔都没有）",
    `§B 空集原因串不对 ✗ ${show(s.body && s.body.note)}`);
  check(s.body && !("reverted" in s.body) && !("failed" in s.body),
    `§B 空集**不该**有 reverted／failed 键（形状照实测）✗ ${show(Object.keys(s.body || {}))}`);
  const after = await compareAfter("§B", ctx);
  check(after.strokes.length === 2, `§B 空集不该改动日志 ✗ 剩 ${after.strokes.length} 笔`);
}

// ══ §C 朴素集合：已撤跳过；撤了又 `reapply` 回来的**照样**跳过 ═════════════════
{
  const ctx = await makeCase("c", [
    ["create_layer", { layer_id: "L1", name: "笔迹层" }],
    stroke([[10, 10], [20, 20]], "b1"),
    stroke([[30, 30], [40, 40]], "b2"),
    stroke([[50, 50], [60, 60]], "b3"),
  ], { seed: false });
  const before = await ctx.server("get_stroke_log", {});
  const entries = (before.body && before.body.entries) || [];
  const reverted = await ctx.server("revert_atoms", { atom_ids: [entries[1].atom_id] });
  check(reverted.status === 200 && reverted.body && reverted.body.ok === true,
    `§C 前序 revert_atoms 失败 ✗ ${show(reverted.body)}`);
  // **补完服务端前序之后再装配一次**（本地要看到那条 revert 原子 ✓；`putAtom` 每次分配新 seq ⇒ 只播一次 ✓）。
  await ctx.seed();
  const [s, l] = await ctx.both("revert_to_seq", { seq: entries[0].seq });
  compareBody("§C 已撤跳过", s, l);
  check(same(s.body && s.body.reverted, [entries[2].atom_id]),
    `§C 只该撤第 3 笔（第 2 笔已撤过）✗ ${show(s.body && s.body.reverted)}`);
  const after = await compareAfter("§C", ctx);
  check(after.strokes.length === 1 && after.strokes[0].atom_id === entries[0].atom_id,
    `§C 回退后应只剩第 1 笔 ✗ ${show(after.strokes.map((e) => e.atom_id))}`);
}

// ══ §C2 撤过又 reapply：服务端朴素集合扫**全部** revert ⇒ 照样跳过 ═════════════
{
  const ctx = await makeCase("c2", [
    ["create_layer", { layer_id: "L1", name: "笔迹层" }],
    stroke([[10, 10], [20, 20]], "b1"),
    stroke([[30, 30], [40, 40]], "b2"),
    stroke([[50, 50], [60, 60]], "b3"),
  ], { seed: false });
  const before = await ctx.server("get_stroke_log", {});
  const entries = (before.body && before.body.entries) || [];
  await ctx.server("revert_atoms", { atom_ids: [entries[1].atom_id] });
  const reapply = await ctx.server("reapply", { atom_id: entries[1].atom_id });
  check(reapply.status === 200 && reapply.body && reapply.body.ok === true,
    `§C2 前序 reapply 失败 ✗ ${show(reapply.body)}`);
  await ctx.seed();
  const [s, l] = await ctx.both("revert_to_seq", { seq: entries[0].seq });
  compareBody("§C2 reapply 后仍跳过", s, l);
  check(same(s.body && s.body.reverted, [entries[2].atom_id]),
    `§C2 只该撤第 3 笔（第 2 笔撤过又恢复 —— 服务端朴素集合照样跳过）✗ ${show(s.body && s.body.reverted)}`);
  const after = await compareAfter("§C2", ctx);
  check(after.strokes.length === 2 && same(after.strokes.map((e) => e.atom_id),
    [entries[0].atom_id, entries[1].atom_id]),
  `§C2 回退后应剩第 1／2 笔（第 2 笔是 reapply 回来的）✗ ${show(after.strokes.map((e) => e.atom_id))}`);
}

// ══ §D 求值起点之前的笔逐条失败（`declare_head` 跳变之后）═════════════════════
{
  const ctx = await makeCase("d", [
    ["create_layer", { layer_id: "L1", name: "笔迹层" }],
    stroke([[10, 10], [20, 20]], "b1"),
    stroke([[30, 30], [40, 40]], "b2"),
  ], { seed: false });
  const before = await ctx.server("get_stroke_log", {});
  const entries = (before.body && before.body.entries) || [];
  const head = await ctx.server("declare_head", {
    base_type: "atom", base_id: entries[1].atom_id, reason: "判据：把求值起点推到第 2 笔",
  });
  check(head.status === 200 && head.body && head.body.ok !== false,
    `§D 前序 declare_head 失败 ✗ ${show(head.body)}`);
  const b3 = await ctx.server("brush_stroke", {
    layer_id: "L1", brush: "100%_Opaque", size: 8, color: RED,
    points: [[50, 50], [60, 60]], label: "b3",
  });
  check(b3.status === 200 && b3.body && b3.body.ok !== false,
    `§D 前序第 3 笔失败 ✗ ${show(b3.body)}`);
  await ctx.seed();
  const [s, l] = await ctx.both("revert_to_seq", { seq: 0 });
  compareBodyAllowFail("§D 起点之前逐条失败", s, l);
  check(s.body && s.body.ok === false && s.body.failed_count === 2 && s.body.reverted_count === 1,
    `§D 应 2 失败 ＋ 1 成功（前两笔在求值起点之前）✗ ${show(s.body)}`);
  const errors = (s.body && s.body.failed || []).map((f) => f.error);
  check(errors.every((e) => typeof e === "string" && e.includes("位于当前求值起点")),
    `§D 失败原因应逐条说「位于当前求值起点」✗ ${show(errors)}`);
  const after = await compareAfter("§D", ctx);
  check(after.strokes.length === 2, `§D 回退后应剩前两笔 ✗ ${show(after.strokes.map((e) => e.label))}`);
}

// ══ §E 六种入口／处理器错误逐字 ═══════════════════════════════════════════════
{
  const ctx = await makeCase("e", [
    ["create_layer", { layer_id: "L1", name: "笔迹层" }],
    ["brush_stroke", { layer_id: "L1", brush: "100%_Opaque", size: 8, color: RED,
                       points: [[10, 10], [20, 20]], label: "b1" }],
  ]);
  const cases = [
    ["§E1 缺 seq", {}],
    ["§E2 seq=null", { seq: null }],
    ["§E3 seq 是字符串", { seq: "1" }],
    ["§E4 seq 是负数", { seq: -1 }],
    ["§E5 seq 是小数", { seq: 1.5 }],
    ["§E6 未知键", { seq: 1, foo: 2 }],
  ];
  for (const [label, args] of cases) {
    const [s, l] = await ctx.both("revert_to_seq", args);
    compareError(label, s, l);
  }
}

// ══ §F 只碰有 `source` 的原子：`draw_shape` 在目标之上也不被撤 ══════════════════
{
  const ctx = await makeCase("f", [
    ["create_layer", { layer_id: "L1", name: "图形成员" }],
    ["draw_shape", { layer_id: "L1", object_id: "T1", label: "shape",
                     data: { geometry: { kind: "rect", bbox: { x: 4, y: 4, w: 12, h: 8 } },
                             color: [0, 0, 1, 1] } }],
    stroke([[10, 20], [30, 40]], "b1"),
  ]);
  const [s, l] = await ctx.both("revert_to_seq", { seq: 1 });
  compareBody("§F 只撤笔迹", s, l);
  check(s.body && s.body.reverted_count === 1 && s.body.failed_count === 0,
    `§F 应只撤 1 笔（图形没有 source）✗ ${show(s.body)}`);
  // **图形还在**：按 label 过滤（撤销原子没有 label ⇒ 不进这张表 ✓，id 不同也不影响 ✓）。
  const [ss, ll] = await ctx.both("list_atoms", { label: "shape", include_non_stroke: true });
  compareBody("§F 图形仍在", ss, ll);
  check(ss.body && Array.isArray(ss.body.atoms) && ss.body.atoms.length === 1,
    `§F 图形应仍在日志里 ✗ ${show(ss.body && ss.body.atoms)}`);
  const after = await compareAfter("§F", ctx);
  check(after.strokes.length === 0, `§F 笔迹应被撤光 ✗ 剩 ${after.strokes.length} 笔`);
}

if (bad.length) {
  console.log(`✗ 失败 ${bad.length} 条（通过 ${okCount}）：`);
  for (const line of bad) console.log("   " + line);
  process.exit(1);
}
console.log(`✓ tool-pwa-revert-to-seq：${okCount} 条断言全过（revert_to_seq 与服务端同答案 ✓）`);
