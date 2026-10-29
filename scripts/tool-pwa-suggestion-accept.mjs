#!/usr/bin/env node
// **判据：PWA 本地的建议预览／接受与服务端**同一请求、同一答案** ✓**（台账「第 六百六十四 节」）。
//
// **∴ 它测什么 ✗**（**内核 ①：断言必须能证明自己能红 ✓**）：
//   同一个请求分别打到真服务端与本地 `api-local` ✓ ⇒ 比解析后的 JSON ✓（断结构不断字节 ✓）。
//   **∴ 每一面都有判据自己算的那一份 ✗**（**∴ 不能只断「两边相等」**：两边同错时相等照样绿 ✓）：
//   ① **`preview_suggestion` 逐步全字段**：`index/tool/class/affects/estimated_region/
//      layer_id/object_id/valid/error` 与聚合量 `estimated_dirty/estimated_region/notes` ✓
//      ⇒ **`error` 必须是 `Some("…")` 形态** ✓（`format!("{:?}", Option<String>)` ✓）；
//   ② **预检四类错误逐字**：未知工具／只读工具／参数缺失／效果名与范围／引用顺序 ✓；
//   ③ **预检**不按本地实现裁剪** ✓**：`revert_to_seq`（服务端 mutating；batch8 时本地 501、batch11 已落地）在预览里仍 `valid:true` ✓
//      —— **∴ 若按本地 get 裁剪 ⇒ 它会变 invalid ⇒ 红 ✓**（旧判据用 `try_stroke` ✗ ——
//      它在服务端是**只读** ⇒ 本来就该 invalid ✗ ⇒ 拿它测这一面等于测错对象 ✓）；
//   ④ **`suggestion_id` 路径 ✗ 与 `patch` 路径等价**（同一边内自比 ✓），
//      且 `suggestion_id` 的三种错误逐字（不存在／不是建议／两者都缺）✓；
//   ⑤ **接受成功 = 逐步重放 ＋ `accept_suggestion` 原子**：`applied_atom_ids` 的
//      **条数与对应日志 kind 序列**（`create_layer`→`draw_shape`）两边相同 ✓，
//      `resolved_annotations:[]`、`pending_annotations:0` ✓；
//   ⑥ **重复接受幂等**（`already_accepted:true`、`applied_atom_ids:[]`、note 逐字 ✓）；
//   ⑦ **已拒绝 ⇒ 409 逐字**（把各自的原子 id 代进去 ✓）；
//   ⑧ **预检拒收 ⇒ 404 顺序文案逐字 ＋ 两边零原子**（**不写半条** ✓）；
//   ⑨ **运行期失败的两条诚实口径**（**如实登记差异 ✗ 不是缺陷 ✓**）：
//      补丁 `[create_layer L4, create_layer L4]` 第二步在运行期 409 ⇒
//      服务端**留下第一步的部分应用**（L4 存在 ✓）；本地**回滚**（L4 不存在 ✓、
//      `rolled_back:true`、日志多一条 `revert` ✓）——**两边错误文案相同** ✓；
//   ⑩ **批量接受**：`accepted/failed/pending_annotations` ＋ 五项逐项
//      （成功／已接受／已拒绝／不存在／非字符串）✓ ＋ 空表与 65 条的闸门逐字 ✓；
//   ⑪ **`list_suggestions` 的冲突口径与服务端同源**（`suggestion_scope`＋`scopes_conflict`）：
//      「同对象」「整文档」两类冲突都被检出 ✓（**∴ 旧的简化模型会漏整文档级 ⇒ 红 ✓**）；
//   ⑫ **表漂移**：`gen-pwa-tool-specs.mjs --check` 必须退出 0（207 条工具的
//      `mutating`／参数表自证与服务端一致 ✓）。
//
// **∴ 前提 ✗**：需要**活着的服务端**（`run-criteria-local.sh` 起一个并把地址放进 argv[2] ✓）
//   ＋ **不过期的真 wasm 内核**（旧了就 `exit(3)` ✓，**前提不成立 ≠ 产品失败 ✓**）。
//
// **∴ 本地多出的诚实字段**（`server:false`／`local_replay_note`／`rolled_back*`）**不比** ✗
//   —— 它们是「本地做到了什么」的额外说明 ✓；**共有的字段一律逐字比** ✓。
process.on("uncaughtException", (error) => {
  console.error(String(error && error.stack || error).slice(0, 1200));
  process.exit(3);
});
import { pathToFileURL } from "node:url";
import { spawnSync } from "node:child_process";

const BASE = process.argv[2];
if (!BASE || !/^https?:\/\//.test(BASE)) {
  console.log("⊘ 不可判：argv[2] 缺服务端地址 ⇒ 请用 bash scripts/run-criteria-local.sh scripts/tool-pwa-suggestion-accept.mjs 跑"
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
const SERVER_DOC = `gsrv-${UNIQ}`;
const LOCAL_DOC = `gloc-${UNIQ}`;
const DOC_W = 96, DOC_H = 72;

// ══ §表 工具表漂移（207 条：mutating 位 ＋ 参数名/类型/必填）════════════════════
{
  const gen = spawnSync("node", ["scripts/gen-pwa-tool-specs.mjs", BASE, "--check"], { encoding: "utf8" });
  check(gen.status === 0,
    `§表 内联工具表与服务端不一致 ✗（gen-pwa-tool-specs.mjs --check 退出码 ${gen.status}）`
    + ` ⇒ 运行 node scripts/gen-pwa-tool-specs.mjs ${BASE} --write\n     `
    + String(gen.stdout || "").trim().split("\n").slice(-3).join("\n     "));
}

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
const show = (value) => String(JSON.stringify(value)).slice(0, 320);
/** **去掉两边各自的「传输层噪声」✓**：
 *   · **服务端**：HTTP 信封的 `created/opened/timings` ✓（`timings` 每次都不一样 ✗ ⇒ 不能逐字比）；
 *   · **本地**：多出的诚实字段（`server`／说明串／回滚字段 ✓）。
 *   **共有的业务字段一律逐字比 ✓**（`ok` 保留 ✓ —— 本地也必须给 ✓）。 */
const LOCAL_ONLY = ["server", "local_replay_note", "local_rollback_note",
  "rolled_back", "rolled_back_atom_ids", "rollback_skipped", "rollback_failed",
  "rollback_pending_atom_ids"];
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
/** **错误逐字比 ✓**（期望文本可分开给：原子 id 两边不同 ✓）。 */
function compareError(label, srv, loc, status, code, expectedS, expectedL) {
  if (!srv.body || !loc.body) { check(false, `${label}：有一边读不出 JSON ✗`); return; }
  check(srv.status === status, `${label}：服务端状态码应为 ${status} ✗ 实测 ${srv.status}｜${show(srv.body)}`);
  check(loc.status === status, `${label}：本地状态码应为 ${status} ✗ 实测 ${loc.status}｜${show(loc.body)}`);
  check(srv.body.error_code === code, `${label}：服务端错误码应为 ${code} ✗ 实测 ${srv.body.error_code} ⇒ 判据口径要跟着权威改`);
  check(loc.body.error_code === code, `${label}：本地错误码应为 ${code} ✗ 实测 ${loc.body.error_code}`);
  const inBand = (res) => {
    const e = res.body.error;
    return typeof e === "string" && e !== "invalid_argument"
      && e !== "reference_not_found" && e !== "precondition_failed";
  };
  if (!inBand(loc)) {
    check(typeof detailOf(loc) === "string" && detailOf(loc).length > 0,
      `${label}：本地没给 context.detail ⇒ 调用方读不到原因 ✗`);
  }
  const wantS = expectedS === undefined ? expectedL : expectedS;
  const wantL = expectedL === undefined ? expectedS : expectedL;
  if (wantS !== undefined && !inBand(srv)) {
    check(detailOf(srv) === wantS, `${label}：服务端文案与判据的期望不同 ✗（权威口径变了就改判据）\n     期望 ${wantS}\n     实测 ${detailOf(srv)}`);
  }
  if (wantL !== undefined && !inBand(loc)) {
    check(detailOf(loc) === wantL, `${label}：本地文案与判据的期望不同 ✗\n     期望 ${wantL}\n     实测 ${detailOf(loc)}`);
  }
}

const getLog = async (side) => {
  const res = side === "server" ? await callServerBare("get_log", { limit: 2000 })
    : await localCall("get_log", { limit: 2000 });
  if (res.status !== 200 || !res.body || !Array.isArray(res.body.atoms)) {
    check(false, `get_log（${side}）读不出原子列表 ✗ ${res.status}｜${show(res.body)}`);
    return null;
  }
  return res.body;
};
const atomRows = (log) => (log ? log.atoms : []);
const rowId = (row) => (row && (row.atom_id ?? row.id)) || null;
const rowKind = (row) => (row && row.kind) || null;
const findAtomId = (log, kind, predicate) => {
  const row = atomRows(log).find((r) => rowKind(r) === kind && (!predicate || predicate(r)));
  return row ? rowId(row) : null;
};
const layerIds = async (side) => {
  const res = side === "server" ? await callServer("list_layers", {}) : await localCall("list_layers", {});
  if (res.status !== 200 || !res.body || !Array.isArray(res.body.layers)) return null;
  return res.body.layers.map((l) => l.layer_id || l.id);
};

// ══ 前序：L1 ＋ S1（红色矩形）════════════════════════════════════════════════
const RECT = (x, y, w, h, color) => ({ geometry: { kind: "rect", bbox: { x, y, w, h } }, color });
await setup("建图层 L1", "create_layer", { layer_id: "L1", name: "层 L1" });
await setup("画 S1", "draw_shape", { layer_id: "L1", object_id: "S1", data: RECT(4, 4, 16, 16, [1, 0, 0, 1]) });

// ══ §A preview(patch)：逐条全字段 ＋ 聚合量 ════════════════════════════════════
const MIXED_PATCH = [
  { tool: "create_layer", arguments: { layer_id: "L2", name: "层 L2" } },
  { tool: "add_adjustment", arguments: { layer_id: "L2", adjustment_type: "brightness_contrast",
    params: { brightness: 0.2, contrast: 1.0 } } },
  { tool: "draw_shape", arguments: { layer_id: "L2", object_id: "S2", data: RECT(20, 20, 10, 10, [0, 1, 0, 1]) } },
  { tool: "no_such_tool", arguments: {} },
  { tool: "get_properties", arguments: {} },
  { tool: "draw_shape", arguments: { layer_id: "ghost", object_id: "S9", data: RECT(40, 40, 5, 5, [0, 0, 1, 1]) } },
  { tool: "add_adjustment", arguments: { layer_id: "L2", adjustment_type: "no_such_adj" } },
  { tool: "add_adjustment", arguments: { layer_id: "L2", adjustment_type: "brightness_contrast",
    params: { brightness: 5 } } },
  { tool: "draw_shape", arguments: { layer_id: "L2", object_id: "S3", bogus: 1,
    data: RECT(50, 10, 6, 6, [1, 1, 0, 1]) } },
  { tool: "draw_shape", arguments: { object_id: "S4", data: RECT(10, 50, 4, 4, [0, 1, 1, 1]) } },
  // **预检不按本地实现裁剪 ✓**（第 ③ 面 ✓）。样本演变如实记 ✗→✓：
  // **∴ 必须选**服务端 mutating 的工具 ✗：`try_stroke` 在服务端是**只读** ✓
  //   ⇒ 预检本来就该判它 invalid ✗（旧判据拿它当「本地 501」样本 ⇒ 测错了对象 ✓）；
  //   `revert_to_seq` 服务端 `mutating: true` ✓——**batch8 写本判据时本地还没有它的端点（501）**，
  //   **batch11 已在本地落地**（`api-local.js` P2c-57）⇒ 这一条现在断的是**两边预检同口径不裁剪** ✓。
  { tool: "revert_to_seq", arguments: { seq: 1 } },
];
const previewByPatch = async (side, patch) => side === "server"
  ? callServer("preview_suggestion", { patch }) : localCall("preview_suggestion", { patch });
{
  const [s, l] = [await previewByPatch("server", MIXED_PATCH), await previewByPatch("local", MIXED_PATCH)];
  check(s.status === 200 && s.body && Array.isArray(s.body.steps), `§A 服务端 preview(patch) 失败 ✗ ${s.status}｜${show(s.body)}`);
  check(l.status === 200 && l.body && Array.isArray(l.body.steps), `§A 本地 preview(patch) 失败 ✗ ${l.status}｜${show(l.body)}`);
  const vs = stripLocal(s.body), vl = stripLocal(l.body);
  check(same(vs, vl), `§A preview(patch) 全字段不一致 ✗\n     服务端 ${show(vs)}\n     本地   ${show(vl)}`);
  check(vs && vs.total_steps === MIXED_PATCH.length, `§A total_steps 应为 ${MIXED_PATCH.length} ✗ ${show(vs && vs.total_steps)}`);
  check(vs && vs.invalid_steps === 7, `§A invalid_steps 应为 7（未知工具/只读/引用顺序/效果名/范围/未知键/缺参）✗ 服务端 ${vs && vs.invalid_steps}`);
  check(vs && vs.applicable === false, `§A applicable 应为 false ✗ ${show(vs && vs.applicable)}`);
  const step = (body, i) => body && Array.isArray(body.steps) ? body.steps[i] : null;
  for (const i of [3, 4, 5, 6, 7, 8, 9]) {
    const entry = step(vs, i);
    check(entry && entry.valid === false, `§A 第 ${i} 步应为 invalid ✗ ${show(entry)}`);
    check(entry && typeof entry.error === "string" && /^Some\(".*"\)$/s.test(entry.error),
      `§A 第 ${i} 步的 error 应是 Some("…") 形态 ✗ ${show(entry && entry.error)}`);
  }
  check(step(vs, 10) && step(vs, 10).valid === true,
    `§A 第 10 步 revert_to_seq（服务端 mutating、本地 batch11 已落地）应仍 valid ✓ 实测 ${show(step(vs, 10))}`
    + " ⇒ 预检被本地实现裁剪了 ✗");
  check(step(vs, 1) && step(vs, 1).affects === "layer", `§A add_adjustment 应 affects=layer ✗ ${show(step(vs, 1))}`);
  check(step(vs, 0) && step(vs, 0).affects === "document", `§A create_layer 应 affects=document ✗ ${show(step(vs, 0))}`);
  check(step(vs, 2) && same(step(vs, 2).estimated_region, [20, 20, 10, 10]),
    `§A draw_shape 的 estimated_region 应为 [20,20,10,10] ✗ ${show(step(vs, 2))}`);
  check(typeof vs.notes === "string" && vs.notes.includes("静态检查"), `§A notes 缺失或形状变了 ✗ ${show(vs && vs.notes)}`);
}

// ══ §B preview(suggestion_id) 与错误文案 ═══════════════════════════════════════
const P_ACCEPT = [
  { tool: "create_layer", arguments: { layer_id: "L2", name: "层 L2" } },
  { tool: "draw_shape", arguments: { layer_id: "L2", object_id: "S2", data: RECT(20, 20, 10, 10, [0, 1, 0, 1]) } },
];
const [sSugA, lSugA] = await both("suggest", { patch: P_ACCEPT, summary: "T-ACCEPT", priority: 7 });
const sSugId = sSugA.body && sSugA.body.suggestion_id;
const lSugId = lSugA.body && lSugA.body.suggestion_id;
check(typeof sSugId === "string" && sSugId.length > 0, `§B 服务端 suggest 没给 suggestion_id ✗ ${show(sSugA.body)}`);
check(typeof lSugId === "string" && lSugId.length > 0, `§B 本地 suggest 没给 suggestion_id ✗ ${show(lSugA.body)}`);
{
  const [sById, lById] = [await callServer("preview_suggestion", { suggestion_id: sSugId }),
    await localCall("preview_suggestion", { suggestion_id: lSugId })];
  const [sByPatch, lByPatch] = [await previewByPatch("server", P_ACCEPT), await previewByPatch("local", P_ACCEPT)];
  // **同一边内自比：按 id 取到的 patch 必须与直接给 patch 同形**（`suggestion_id` 除外）✓
  const norm = (body) => { const v = stripLocal(body); if (v) v.suggestion_id = "<suggestion>"; return v; };
  check(same(norm(sById.body), norm(sByPatch.body)),
    `§B 服务端：preview(id) 与 preview(patch) 不同 ✗\n     id    ${show(norm(sById.body))}\n     patch ${show(norm(sByPatch.body))}`);
  check(same(norm(lById.body), norm(lByPatch.body)),
    `§B 本地：preview(id) 与 preview(patch) 不同 ✗\n     id    ${show(norm(lById.body))}\n     patch ${show(norm(lByPatch.body))}`);
  check(same(norm(sById.body), norm(lById.body)),
    `§B preview(id) 两边不同 ✗\n     服务端 ${show(norm(sById.body))}\n     本地   ${show(norm(lById.body))}`);
  check(sById.body && sById.body.suggestion_id === sSugId, `§B 服务端没回显 suggestion_id ✗ ${show(sById.body && sById.body.suggestion_id)}`);
  check(lById.body && lById.body.suggestion_id === lSugId, `§B 本地没回显 suggestion_id ✗ ${show(lById.body && lById.body.suggestion_id)}`);

  // 三种错误 + patch 形状错 + 空 patch
  const sLog = await getLog("server"), lLog = await getLog("local");
  const sAtom = findAtomId(sLog, "create_layer", (r) => r.payload && r.payload.layer_id === "L1");
  const lAtom = findAtomId(lLog, "create_layer", (r) => r.payload && r.payload.layer_id === "L1");
  check(sAtom !== null && lAtom !== null, `§B 取不到 L1 的创建原子 ✗ 服务端 ${sAtom}｜本地 ${lAtom}`);

  const [sNo, lNo] = await both("preview_suggestion", { suggestion_id: "deadbeef" });
  compareError("§B 不存在的建议", sNo, lNo, 404, "reference_not_found", "建议 deadbeef 不在日志中");

  const [sKind, lKind] = [await callServer("preview_suggestion", { suggestion_id: sAtom }),
    await localCall("preview_suggestion", { suggestion_id: lAtom })];
  compareError("§B 非建议原子", sKind, lKind, 400, "invalid_argument",
    `${sAtom} 不是建议（suggest）原子`, `${lAtom} 不是建议（suggest）原子`);

  const [sNeither, lNeither] = await both("preview_suggestion", {});
  compareError("§B 两者都缺", sNeither, lNeither, 400, "invalid_argument", "需要 suggestion_id 或 patch");

  const [sBadPatch, lBadPatch] = await both("preview_suggestion", { patch: "not-an-array" });
  if (sBadPatch.status === 400 && lBadPatch.status === 400) {
    check(sBadPatch.body.error_code === lBadPatch.body.error_code,
      `§B patch 非数组：错误码不同 ✗ 服务端 ${sBadPatch.body.error_code}｜本地 ${lBadPatch.body.error_code}`);
    check(detailOf(sBadPatch) === detailOf(lBadPatch),
      `§B patch 非数组：文案不同 ✗\n     服务端 ${detailOf(sBadPatch)}\n     本地   ${detailOf(lBadPatch)}`);
  } else {
    check(false, `§B patch 非数组应 400 ✗ 服务端 ${sBadPatch.status}｜本地 ${lBadPatch.status}`);
  }

  const [sEmpty, lEmpty] = await both("preview_suggestion", { patch: [] });
  check(same(stripLocal(sEmpty.body), stripLocal(lEmpty.body)),
    `§B 空 patch 两边不同 ✗\n     服务端 ${show(stripLocal(sEmpty.body))}\n     本地   ${show(stripLocal(lEmpty.body))}`);
  check(sEmpty.body && sEmpty.body.applicable === false && sEmpty.body.total_steps === 0,
    `§B 空 patch 的 applicable/total_steps 不对 ✗ ${show(sEmpty.body)}`);
}

// ══ §C accept 成功：重放 ＋ 接受原子 ═══════════════════════════════════════════
const logBeforeAccept = { server: await getLog("server"), local: await getLog("local") };
const seqBeforeAccept = { server: atomRows(logBeforeAccept.server).length, local: atomRows(logBeforeAccept.local).length };
const [sAcc, lAcc] = [await callServer("accept_suggestion", { suggestion_id: sSugId }),
  await localCall("accept_suggestion", { suggestion_id: lSugId })];
check(sAcc.status === 200, `§C 服务端 accept 应为 200 ✗ ${sAcc.status}｜${show(sAcc.body)}`);
check(lAcc.status === 200, `§C 本地 accept 应为 200 ✗ ${lAcc.status}｜${show(lAcc.body)}`);
check(sAcc.body && sAcc.body.suggestion_id === sSugId, `§C 服务端没回显 suggestion_id ✗ ${show(sAcc.body)}`);
check(lAcc.body && lAcc.body.suggestion_id === lSugId, `§C 本地没回显 suggestion_id ✗ ${show(lAcc.body)}`);
const sApplied = (sAcc.body && sAcc.body.applied_atom_ids) || null;
const lApplied = (lAcc.body && lAcc.body.applied_atom_ids) || null;
check(Array.isArray(sApplied) && sApplied.length === 2, `§C 服务端 applied_atom_ids 应有 2 条 ✗ ${show(sApplied)}`);
check(Array.isArray(lApplied) && lApplied.length === 2, `§C 本地 applied_atom_ids 应有 2 条 ✗ ${show(lApplied)}`);
check(sAcc.body && same(sAcc.body.resolved_annotations, []), `§C 服务端 resolved_annotations 应为 [] ✗ ${show(sAcc.body && sAcc.body.resolved_annotations)}`);
check(lAcc.body && same(lAcc.body.resolved_annotations, []), `§C 本地 resolved_annotations 应为 [] ✗ ${show(lAcc.body && lAcc.body.resolved_annotations)}`);
check(sAcc.body && sAcc.body.pending_annotations === 0, `§C 服务端 pending_annotations 应为 0 ✗ ${show(sAcc.body && sAcc.body.pending_annotations)}`);
check(lAcc.body && lAcc.body.pending_annotations === 0, `§C 本地 pending_annotations 应为 0 ✗ ${show(lAcc.body && lAcc.body.pending_annotations)}`);
check(sAcc.body && !sAcc.body.already_accepted, `§C 首次接受不该带 already_accepted ✗ ${show(sAcc.body)}`);
check(lAcc.body && !lAcc.body.already_accepted, `§C 首次接受不该带 already_accepted ✗ ${show(lAcc.body)}`);
const sAcceptAtom = sAcc.body && sAcc.body.accept_atom_id;
const lAcceptAtom = lAcc.body && lAcc.body.accept_atom_id;
check(typeof sAcceptAtom === "string" && sAcceptAtom.length > 0, `§C 服务端没给 accept_atom_id ✗ ${show(sAcceptAtom)}`);
check(typeof lAcceptAtom === "string" && lAcceptAtom.length > 0, `§C 本地没给 accept_atom_id ✗ ${show(lAcceptAtom)}`);
{
  const logAfter = { server: await getLog("server"), local: await getLog("local") };
  const kindsOf = (log, ids) => {
    if (!Array.isArray(ids)) return null;
    const map = new Map(atomRows(log).map((r) => [rowId(r), rowKind(r)]));
    return ids.map((id) => map.get(id) || null);
  };
  const sKinds = kindsOf(logAfter.server, sApplied);
  const lKinds = kindsOf(logAfter.local, lApplied);
  check(same(sKinds, ["create_layer", "draw_shape"]),
    `§C 服务端 applied_atom_ids 对应的 kind 应为 [create_layer, draw_shape] ✗ ${show(sKinds)}`);
  check(same(lKinds, ["create_layer", "draw_shape"]),
    `§C 本地 applied_atom_ids 对应的 kind 应为 [create_layer, draw_shape] ✗ ${show(lKinds)}`);
  check(atomRows(logAfter.server).length === seqBeforeAccept.server + 3,
    `§C 服务端日志应多 3 条（两步 ＋ 接受原子）✗ 实测多 ${atomRows(logAfter.server).length - seqBeforeAccept.server}`);
  check(atomRows(logAfter.local).length === seqBeforeAccept.local + 3,
    `§C 本地日志应多 3 条（两步 ＋ 接受原子）✗ 实测多 ${atomRows(logAfter.local).length - seqBeforeAccept.local}`);
}
{
  const [sObj, lObj] = await both("get_object", { object_id: "S2" });
  for (const [label, res] of [["服务端", sObj], ["本地", lObj]]) {
    check(res.status === 200 && res.body && res.body.object_id === "S2",
      `§C ${label} 接受后应有对象 S2 ✗ ${res.status}｜${show(res.body)}`);
    check(res.body && res.body.layer_id === "L2", `§C ${label} 的 S2 应在 L2 ✗ ${show(res.body && res.body.layer_id)}`);
    check(res.body && res.body.type === "shape", `§C ${label} 的 S2 应为 shape ✗ ${show(res.body && res.body.type)}`);
  }
  const [sLayers, lLayers] = [await layerIds("server"), await layerIds("local")];
  check(Array.isArray(sLayers) && sLayers.includes("L2"), `§C 服务端应有图层 L2 ✗ ${show(sLayers)}`);
  check(Array.isArray(lLayers) && lLayers.includes("L2"), `§C 本地应有图层 L2 ✗ ${show(lLayers)}`);
}

// ══ §D 重复接受：幂等 ═════════════════════════════════════════════════════════
{
  const [s, l] = [await callServer("accept_suggestion", { suggestion_id: sSugId }),
    await localCall("accept_suggestion", { suggestion_id: lSugId })];
  check(s.status === 200 && l.status === 200, `§D 重复接受应为 200 ✗ 服务端 ${s.status}｜本地 ${l.status}`);
  check(s.body && s.body.already_accepted === true, `§D 服务端应 already_accepted=true ✗ ${show(s.body)}`);
  check(l.body && l.body.already_accepted === true, `§D 本地应 already_accepted=true ✗ ${show(l.body)}`);
  check(s.body && same(s.body.applied_atom_ids, []), `§D 服务端 applied_atom_ids 应为 [] ✗ ${show(s.body && s.body.applied_atom_ids)}`);
  check(l.body && same(l.body.applied_atom_ids, []), `§D 本地 applied_atom_ids 应为 [] ✗ ${show(l.body && l.body.applied_atom_ids)}`);
  check(s.body && s.body.accept_atom_id === sAcceptAtom, `§D 服务端 accept_atom_id 应回指首次那条 ✗ ${show(s.body && s.body.accept_atom_id)}`);
  check(l.body && l.body.accept_atom_id === lAcceptAtom, `§D 本地 accept_atom_id 应回指首次那条 ✗ ${show(l.body && l.body.accept_atom_id)}`);
  check(s.body && s.body.note === "该建议此前已被接受，本次不重复施加效果",
    `§D 服务端 note 文案变了 ✗ ${show(s.body && s.body.note)}`);
  check(l.body && l.body.note === "该建议此前已被接受，本次不重复施加效果",
    `§D 本地 note 文案变了 ✗ ${show(l.body && l.body.note)}`);
  check(s.body && l.body && s.body.note === l.body.note, `§D note 两边不同 ✗ 服务端 ${show(s.body && s.body.note)}｜本地 ${show(l.body && l.body.note)}`);
}

// ══ §E 已拒绝的建议 ⇒ 409（文案代入各自的原子 id）══════════════════════════════
const [sSugR, lSugR] = await both("suggest", { patch: [{ tool: "create_layer", arguments: { layer_id: "L5", name: "层 L5" } }],
  summary: "T-REJECT", priority: 5 });
const sSugRId = sSugR.body && sSugR.body.suggestion_id;
const lSugRId = lSugR.body && lSugR.body.suggestion_id;
{
  const [sRej, lRej] = [await callServer("reject_suggestion", { suggestion_id: sSugRId, reason: "不需要" }),
    await localCall("reject_suggestion", { suggestion_id: lSugRId, reason: "不需要" })];
  check(sRej.status === 200 && lRej.status === 200, `§E 拒绝应 200 ✗ 服务端 ${sRej.status}｜本地 ${lRej.status}`);
  const sRejAtom = (sRej.body && sRej.body.reject_atom_id)
    || findAtomId(await getLog("server"), "reject_suggestion", (r) => r.payload && r.payload.target_atom_id === sSugRId);
  const lRejAtom = (lRej.body && lRej.body.reject_atom_id)
    || findAtomId(await getLog("local"), "reject_suggestion", (r) => r.payload && r.payload.target_atom_id === lSugRId);
  check(typeof sRejAtom === "string" && typeof lRejAtom === "string", `§E 取不到 reject 原子 id ✗ 服务端 ${sRejAtom}｜本地 ${lRejAtom}`);
  const [s, l] = [await callServer("accept_suggestion", { suggestion_id: sSugRId }),
    await localCall("accept_suggestion", { suggestion_id: lSugRId })];
  compareError("§E 接受已拒绝的建议", s, l, 409, "precondition_failed",
    `建议 ${sSugRId} 已被拒绝（原子 ${sRejAtom}），不能接受已拒绝的建议`,
    `建议 ${lSugRId} 已被拒绝（原子 ${lRejAtom}），不能接受已拒绝的建议`);
}

// ══ §F 预检拒收 ⇒ 404 顺序文案 ＋ 两边零原子 ═══════════════════════════════════
{
  const [sSugF, lSugF] = await both("suggest", {
    patch: [{ tool: "draw_shape", arguments: { layer_id: "ghost2", object_id: "S7", data: RECT(2, 2, 3, 3, [1, 1, 1, 1]) } }],
    summary: "T-PREFLIGHT", priority: 5 });
  const sId = sSugF.body && sSugF.body.suggestion_id;
  const lId = lSugF.body && lSugF.body.suggestion_id;
  const [sPv, lPv] = [await callServer("preview_suggestion", { suggestion_id: sId }),
    await localCall("preview_suggestion", { suggestion_id: lId })];
  check(sPv.body && sPv.body.applicable === false && sPv.body.invalid_steps === 1,
    `§F 预检应报 1 步无效 ✗ ${show(sPv.body)}`);
  // **∴ 两边的 `suggestion_id` 必然不同 ✗**（各自日志里的原子 id ✓）⇒ 投影掉再比其余字段 ✓
  const normF = (body) => { const v = stripLocal(body); if (v) v.suggestion_id = "<suggestion>"; return v; };
  check(same(normF(sPv.body), normF(lPv.body)),
    `§F 预检两边不同（投影 suggestion_id 后）✗\n     服务端 ${show(normF(sPv.body))}\n     本地   ${show(normF(lPv.body))}`);
  const before = { server: atomRows(await getLog("server")).length, local: atomRows(await getLog("local")).length };
  const [s, l] = [await callServer("accept_suggestion", { suggestion_id: sId }),
    await localCall("accept_suggestion", { suggestion_id: lId })];
  compareError("§F 接受引用不存在的补丁", s, l, 404, "reference_not_found",
    "该步骤引用的图层 ghost2 在此时尚未创建（补丁顺序问题）");
  const after = { server: atomRows(await getLog("server")).length, local: atomRows(await getLog("local")).length };
  check(after.server === before.server, `§F 服务端预检拒收却写了原子 ✗ ${before.server} → ${after.server}`);
  check(after.local === before.local, `§F 本地预检拒收却写了原子 ✗ ${before.local} → ${after.local}`);
}

// ══ §G 运行期失败：服务端部分应用／本地回滚（诚实差异）══════════════════════════
{
  const [sSugG, lSugG] = await both("suggest", {
    patch: [
      { tool: "create_layer", arguments: { layer_id: "L4", name: "层 L4" } },
      { tool: "create_layer", arguments: { layer_id: "L4", name: "层 L4 再来一次" } },
    ],
    summary: "T-RUNTIME", priority: 5 });
  const sId = sSugG.body && sSugG.body.suggestion_id;
  const lId = lSugG.body && lSugG.body.suggestion_id;
  // 静态预检看不到「运行期撞已有图层」⇒ 两边都必须说 applicable ✓
  const [sPv, lPv] = [await callServer("preview_suggestion", { suggestion_id: sId }),
    await localCall("preview_suggestion", { suggestion_id: lId })];
  check(sPv.body && sPv.body.applicable === true && sPv.body.invalid_steps === 0,
    `§G 静态预检应 applicable=true ✗ ${show(sPv.body)}`);
  check(lPv.body && lPv.body.applicable === true && lPv.body.invalid_steps === 0,
    `§G 本地静态预检应 applicable=true ✗ ${show(lPv.body)}`);
  const before = { server: atomRows(await getLog("server")).length, local: atomRows(await getLog("local")).length };
  const [s, l] = [await callServer("accept_suggestion", { suggestion_id: sId }),
    await localCall("accept_suggestion", { suggestion_id: lId })];
  compareError("§G 运行期撞已有图层", s, l, 409, "precondition_failed", "图层 L4 已存在");
  const sLayers = await layerIds("server");
  const lLayers = await layerIds("local");
  check(Array.isArray(sLayers) && sLayers.includes("L4"),
    `§G 服务端应留下第一步的部分应用（L4 存在）✗ ${show(sLayers)} ⇒ 判据口径变了就改这条`);
  check(Array.isArray(lLayers) && !lLayers.includes("L4"),
    `§G 本地应回滚掉 L4 ✗ ${show(lLayers)}`);
  check(l.body && l.body.rolled_back === true, `§G 本地应报 rolled_back=true ✗ ${show(l.body)}`);
  check(l.body && Array.isArray(l.body.rolled_back_atom_ids) && l.body.rolled_back_atom_ids.length === 1,
    `§G 本地应列出 1 条被回滚的原子 ✗ ${show(l.body && l.body.rolled_back_atom_ids)}`);
  check(l.body && l.body.context && l.body.context.step_index === 1,
    `§G 本地应在 context 里指出失败步号 1 ✗ ${show(l.body && l.body.context)}`);
  const after = { server: atomRows(await getLog("server")).length, local: atomRows(await getLog("local")).length };
  check(after.server === before.server + 1,
    `§G 服务端应只多 1 条（成功的第一步）✗ 多 ${after.server - before.server}`);
  check(after.local === before.local + 2,
    `§G 本地应多 2 条（第一步 ＋ revert）✗ 多 ${after.local - before.local}`);
  const logLocal = await getLog("local");
  check(atomRows(logLocal).slice(-1).some((r) => rowKind(r) === "revert"),
    `§G 本地日志末条应为 revert ✗ ${show(atomRows(logLocal).slice(-2))}`);
}

// ══ §H 批量接受：五项混合 ＋ 闸门 ══════════════════════════════════════════════
{
  const [sSugB, lSugB] = await both("suggest", { patch: [{ tool: "create_layer", arguments: { layer_id: "L6", name: "层 L6" } }],
    summary: "T-BATCH", priority: 4 });
  const sBId = sSugB.body && sSugB.body.suggestion_id;
  const lBId = lSugB.body && lSugB.body.suggestion_id;
  const sIds = [sBId, sSugId, sSugRId, "missing-batch-id", 42];
  const lIds = [lBId, lSugId, lSugRId, "missing-batch-id", 42];
  const [s, l] = [await callServer("accept_suggestions", { suggestion_ids: sIds }),
    await localCall("accept_suggestions", { suggestion_ids: lIds })];
  check(s.status === 200 && l.status === 200, `§H 批量应为 200 ✗ 服务端 ${s.status}｜本地 ${l.status}`);
  check(s.body && s.body.accepted === 2 && s.body.failed === 3,
    `§H 服务端 accepted/failed 应为 2/3（已接受的那条幂等成功 ⇒ 也算 accepted ✓）✗ ${show(s.body && [s.body.accepted, s.body.failed])}`);
  check(l.body && l.body.accepted === 2 && l.body.failed === 3,
    `§H 本地 accepted/failed 应为 2/3（已接受的那条幂等成功 ⇒ 也算 accepted ✓）✗ ${show(l.body && [l.body.accepted, l.body.failed])}`);
  check(s.body && s.body.pending_annotations === 0, `§H 服务端 pending_annotations 应为 0 ✗ ${show(s.body && s.body.pending_annotations)}`);
  check(l.body && l.body.pending_annotations === 0, `§H 本地 pending_annotations 应为 0 ✗ ${show(l.body && l.body.pending_annotations)}`);
  const results = (body) => (body && Array.isArray(body.results) ? body.results : []);
  const sr = results(s.body), lr = results(l.body);
  check(sr.length === 5 && lr.length === 5, `§H results 长度应为 5 ✗ 服务端 ${sr.length}｜本地 ${lr.length}`);
  const proj = (item) => {
    if (!item) return null;
    if (item.ok === true) {
      return { ok: true, suggestion_id: item.suggestion_id,
        applied: Array.isArray(item.applied_atom_ids) ? item.applied_atom_ids.length : -1,
        resolved: Array.isArray(item.resolved_annotations) ? item.resolved_annotations.length : -1 };
    }
    return { ok: false, suggestion_id: item.suggestion_id,
      error_code: item.error_code ?? null, detail: item.detail ?? null, error: item.error ?? null };
  };
  // **∴ 投影要带上 `error:null`／`detail:null` ✓**（`proj` 两个键都出 ✓ ⇒ 期望也得有 ✓）
  const want = [
    { ok: true, suggestion_id: sBId, applied: 1, resolved: 0 },
    { ok: true, suggestion_id: sSugId, applied: 0, resolved: 0 },
    { ok: false, suggestion_id: sSugRId, error_code: "precondition_failed" },
    { ok: false, suggestion_id: "missing-batch-id", error_code: "reference_not_found",
      detail: "建议 missing-batch-id 不在日志中", error: null },
    { ok: false, suggestion_id: 42, error_code: null, detail: null, error: "id 必须是字符串" },
  ];
  check(same(proj(sr[0]), want[0]), `§H 服务端第 1 项应对 ✗ ${show(proj(sr[0]))}`);
  check(same(proj(sr[1]), want[1]), `§H 服务端第 2 项（已接受）应对 ✗ ${show(proj(sr[1]))}`);
  check(proj(sr[2]) && proj(sr[2]).error_code === "precondition_failed",
    `§H 服务端第 3 项（已拒绝）应为 precondition_failed ✗ ${show(proj(sr[2]))}`);
  check(same(proj(sr[3]), want[3]), `§H 服务端第 4 项（不存在）应对 ✗ ${show(proj(sr[3]))}`);
  check(same(proj(sr[4]), want[4]), `§H 服务端第 5 项（非字符串）应对 ✗ ${show(proj(sr[4]))}`);
  check(same(proj(lr[0]), { ok: true, suggestion_id: lBId, applied: 1, resolved: 0 }),
    `§H 本地第 1 项应对 ✗ ${show(proj(lr[0]))}`);
  check(same(proj(lr[1]), { ok: true, suggestion_id: lSugId, applied: 0, resolved: 0 }),
    `§H 本地第 2 项（已接受）应对 ✗ ${show(proj(lr[1]))}`);
  check(proj(lr[2]) && proj(lr[2]).error_code === "precondition_failed",
    `§H 本地第 3 项（已拒绝）应为 precondition_failed ✗ ${show(proj(lr[2]))}`);
  check(same(proj(lr[3]), want[3]), `§H 本地第 4 项（不存在）应对 ✗ ${show(proj(lr[3]))}`);
  check(same(proj(lr[4]), want[4]), `§H 本地第 5 项（非字符串）应对 ✗ ${show(proj(lr[4]))}`);

  const [sEmpty, lEmpty] = await both("accept_suggestions", { suggestion_ids: [] });
  compareError("§H 空列表", sEmpty, lEmpty, 400, "invalid_argument", "suggestion_ids 不能为空");
  const many = Array.from({ length: 65 }, (_, i) => "x" + i);
  const [sMany, lMany] = await both("accept_suggestions", { suggestion_ids: many });
  compareError("§H 超过 64 条", sMany, lMany, 400, "invalid_argument", "一次最多接受 64 条建议（收到 65）");
}

// ══ §I list_suggestions 的冲突口径（整文档／同对象）════════════════════════════
{
  const J = [
    { summary: "J-OBJ-A", priority: 3, patch: [{ tool: "draw_shape",
      arguments: { layer_id: "L1", object_id: "S5", data: RECT(30, 30, 8, 8, [1, 0, 1, 1]) } }] },
    { summary: "J-OBJ-B", priority: 6, patch: [{ tool: "draw_shape",
      arguments: { layer_id: "L1", object_id: "S5", data: RECT(34, 34, 8, 8, [1, 1, 0, 1]) } }] },
    { summary: "J-DOC-A", priority: 5, patch: [{ tool: "create_layer", arguments: { layer_id: "L7", name: "层 L7" } }] },
    { summary: "J-DOC-B", priority: 5, patch: [{ tool: "create_layer", arguments: { layer_id: "L8", name: "层 L8" } }] },
  ];
  for (const item of J) await both("suggest", item);
  const [s, l] = [await callServerBare("list_suggestions", { limit: 500 }),
    await localCall("list_suggestions", { limit: 500 })];
  check(s.status === 200 && l.status === 200, `§I list_suggestions 应为 200 ✗ 服务端 ${s.status}｜本地 ${l.status}`);
  // **∴ 先取**全表** ✗**（冲突另一方可能是 T- 开头的非 J- 建议 ✓ —— 旧判据只映射 J- ⇒
  //   另一端点落到**各自的原子 id** 上 ⇒ 两边永不相同 ✗ ⇒ 假红 ✓）。
  const allOf = (body) => (body && Array.isArray(body.suggestions) ? body.suggestions : []);
  const smAll = allOf(s.body), lmAll = allOf(l.body);
  const mine = (items) => items
    .filter((item) => typeof item.summary === "string" && item.summary.startsWith("J-"));
  const sm = mine(smAll), lm = mine(lmAll);
  check(sm.length === 4, `§I 服务端应列出 4 条 J- 建议 ✗ ${sm.length}`);
  check(lm.length === 4, `§I 本地应列出 4 条 J- 建议 ✗ ${lm.length}`);
  const view = (items) => items
    .map((item) => ({ summary: item.summary, priority: item.priority, patch: item.patch,
      status: item.status, reason: item.reason, annotation_id: item.annotation_id }))
    .sort((x, y) => (x.summary < y.summary ? -1 : 1));
  check(same(view(sm), view(lm)),
    `§I 建议列表投影不同 ✗\n     服务端 ${show(view(sm))}\n     本地   ${show(view(lm))}`);
  const conflictsOf = (body, allItems) => {
    const nameOf = new Map(allItems.map((item) => [item.suggestion_id, item.summary]));
    return (body && Array.isArray(body.conflicts) ? body.conflicts : [])
      .map((c) => {
        // **∴ 一对冲突只看**无序集合** ✗**（谁先谁后由 id 排序决定 ✓ ⇒ 旧判据要求固定顺序 ⇒ 假红 ✓）
        const pair = [nameOf.get(c.suggestion_id) || c.suggestion_id,
          nameOf.get(c.conflicts_with) || c.conflicts_with].sort();
        return `${pair[0]}|${pair[1]}|${c.reason}`;
      })
      .filter((line) => line.startsWith("J-"))
      .sort();
  };
  const sConf = conflictsOf(s.body, smAll), lConf = conflictsOf(l.body, lmAll);
  check(same(sConf, lConf), `§I 冲突集合不同 ✗\n     服务端 ${show(sConf)}\n     本地   ${show(lConf)}`);
  check(sConf.includes("J-OBJ-A|J-OBJ-B|两者修改同一个对象"),
    `§I 应检出「同对象」冲突（J-OBJ-A/J-OBJ-B）✗ ${show(sConf)}`);
  check(sConf.some((line) => line.includes("两者之一会改变图层结构（整文档失效）")
      && line.includes("J-DOC-A") && line.includes("J-DOC-B")),
    `§I 应检出「整文档」冲突（J-DOC-A/J-DOC-B）✗ ${show(sConf)}`);
  check(s.body && l.body && s.body.pending === l.body.pending && s.body.total === l.body.total,
    `§I pending/total 不同 ✗ 服务端 ${show(s.body && [s.body.pending, s.body.total])}｜本地 ${show(l.body && [l.body.pending, l.body.total])}`);
}

// ══ §J 本地诚实字段（多出来的说明必须存在且不冒充服务端）══════════════════════
{
  const lPv = await previewByPatch("local", MIXED_PATCH);
  check(lPv.body && lPv.body.server === false, `§J 本地 preview 应标 server:false ✗ ${show(lPv.body && lPv.body.server)}`);
  check(lPv.body && typeof lPv.body.local_replay_note === "string" && lPv.body.local_replay_note.length > 0,
    `§J 本地 preview 应说明接受时的重放/回滚口径 ✗ ${show(lPv.body && lPv.body.local_replay_note)}`);
  const lAcc = await localCall("accept_suggestion", { suggestion_id: lSugId });
  check(lAcc.body && lAcc.body.server === false, `§J 本地 accept 应标 server:false ✗ ${show(lAcc.body && lAcc.body.server)}`);
}

if (bad.length) {
  console.log("✗ PWA 建议预览／接受 判据失败：");
  for (const m of bad) console.log("   · " + m);
  console.log(`   ⇒ 通过 ${okCount} 条｜失败 ${bad.length} 条`);
  process.exit(1);
}
console.log(`✓ PWA 本地建议预览／接受与服务端同口径（${okCount} 条断言全过：`
  + "表漂移、逐步校验文案、id／patch 两路径、幂等、拒绝、预检零写入、运行期回滚、批量与冲突模型）");
