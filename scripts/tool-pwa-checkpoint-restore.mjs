#!/usr/bin/env node
// **判据：PWA 本地的检查点／求值起点跳变与服务端**同一请求、同一答案** ✓**（台账「第 六百六十三 节」）。
//
// **∴ 它测什么 ✗**（**内核 ①：断言必须能证明自己能红 ✓**）：
//   同一个请求分别打到真服务端与本地 `api-local` ✓ ⇒ 比解析后的 JSON ✓
//   （整数严格、非整数 1e-12 相对误差 ✓；**不比字节** ✓）。
//   **∴ 每条期望都有判据自己算的那一份 ✗**（**∴ 不能只断「两边相等」**：两边同错时相等照样绿 ✓）：
//   ① **`checkpoint` 的锚点**：`anchor_seq` 必须**恰为原子 seq − 1** ✓（服务端 `write_checkpoint`
//      取提交前的 `head_seq` ✓；本地 `p2bHeadSeq` 同口径 ✓）；
//   ② **`get_checkpoints` 六个键一个不少 ✗**：`message`／`snapshot_id` **缺席时给 `null`** ✓
//      （**服务端那条路是手写 `json!` ✗ 不是结构体序列化 ✓** ⇒ `Option::None` 变 `null` ✓
//      ⇒ **`Checkpoint` 上的 `skip_serializing_if` 在这条路径**不生效 ✗**）；
//   ③ **`message` 取自原子封套 ✗ 不是净荷**：`checkpoint{message}` 只进 `payload.message` ✓
//      ⇒ 折叠出的 `Checkpoint.message = atom.message = None` ✓ ⇒ `get_checkpoints` 回 `null` ✓
//      （**∴ 这是**一个会让人以为「参数被吞了」的**真实口径 ✓**）；
//   ④ **恢复 = `declare_head{base:{type:"checkpoint"}}` 的求值起点跳变 ✓**：
//      恢复后 `state@seq` 只含锚点之前 ＋ 跳变之后 ✓ ⇒ S4（检查点之后画的）**消失** ✓、
//      `checkpoints` **变 0** ✓（检查点原子自己在窗口外 ✓）、像素少掉 64 个黑像素 ✓；
//   ⑤ **跨求值起点的撤销被拒 ✗**（`log.rs:893` ✓）：单条 `revert`／`reapply` 400、
//      批量 `revert_atoms` 逐项 `failed[0].error` 逐字 ✓（**∴ 这条旧本地不查 ⇒ 会「假成功」✓**）；
//   ⑥ **多层链 ✓**：`revert_to(S1 原子)` 之后再 `restore_checkpoint(CP1)` ⇒
//      窗口 = `(0, 锚点] ∪ (新跳变, n]` ✓ ⇒ 中间那次 `revert_to` 被跳过 ✓ ⇒ 状态回到 S1+S2+S3 ✓；
//   ⑦ **参数/引用错误逐字**：缺参、未知参数、类型错、未知检查点／未知原子 ✓
//      （**∴ 本地这四条此前不按 `validate_args` 校验 ✗** ⇒ 文案不同 ✓）。
//
// **∴ 前提 ✗**：需要**活着的服务端**（`run-criteria-local.sh` 起一个并把地址放进 argv[2] ✓）
//   ＋ **不过期的真 wasm 内核**（`web/wasm/` 是构建产物 ✓ ⇒ 旧了就 `exit(3)` ✓，
//   **前提不成立 ≠ 产品失败 ✓**）。
//
// **∴ 变异（四层齐第④层 ✓ 逐条实测见台账「六百六十三」）**：
//   ① 本地 `restore_checkpoint` 回到 501 ⇒ §C 全红 ✓；
//   ② `get_checkpoints` 不给 `message`／`snapshot_id` 两个 null 键 ⇒ §A 键集红 ✓；
//   ③ `get_checkpoints` 的 `message` 取 `payload.message` ⇒ §A 变「第一版说明」 ⇒ 红 ✓；
//   ④ `anchor_seq` 不回退到原子 seq ⇒ §A（若净荷缺 anchor_seq）红 ✓；
//   ⑤ 本地 `revert`／`reapply` 去掉跨起点检查 ⇒ §D 本地 200 而服务端 400 ⇒ 红 ✓；
//   ⑥ `revert_atoms` 不做跨起点检查 ⇒ §D 批量本地 `ok:true` ⇒ 红 ✓；
//   ⑦ `p2EvalOriginSeq` 恒回 0 ⇒ §D 全变成功 ⇒ 红 ✓；
//   ⑧ `p2StateWindow` 不按跳变裁窗口 ⇒ §C S4 还在／像素仍 2416 ⇒ 红 ✓；
//   ⑨ 本地 `restore_checkpoint` 不查检查点是否存在 ⇒ §F 404 变 200 ⇒ 红 ✓；
//   ⑩ 本地 `checkpoint` 不校验 `name` 类型 ⇒ §F 类型错 200 而服务端 400 ⇒ 红 ✓；
//   ⑪ **盲区写明 ✓**：本地**不生成检查点快照** ⇒ `checkpoint` 回执的 `snapshot_id`
//      本地恒 `null`、服务端可能是字符串 ⇒ **这一项不比**（如实差异，不是缺陷 ✓）；
//      `get_checkpoints` 里两边都是 `null`（快照 id 只在净荷里，提交时不写 ✓）⇒ 那一项**比**。
process.on("uncaughtException", (error) => {
  console.error(String(error && error.stack || error).slice(0, 1200));
  process.exit(3);
});
import { pathToFileURL } from "node:url";

const BASE = process.argv[2];
if (!BASE || !/^https?:\/\//.test(BASE)) {
  console.log("⊘ 不可判：argv[2] 缺服务端地址 ⇒ 请用 bash scripts/run-criteria-local.sh scripts/tool-pwa-checkpoint-restore.mjs 跑"
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
const UNIQ = "c" + Date.now().toString(36);
const SERVER_DOC = `csrv-${UNIQ}`;
const LOCAL_DOC = `cloc-${UNIQ}`;
const DOC_W = 64, DOC_H = 48;

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
/** **`actor` 既是框架参数 ✗ 又是 `get_log` 的筛选参数 ✓**（`tools_read.rs:698` 直接从 args 读 ✓）：
 *   **∴ 走 `callServer` 会把 `actor:"human:judge"` 当成筛选条件 ✗** ⇒ 而服务端原子盖的章是
 *   token 持有者（实测 `human:web` ✓）⇒ **0 行** ✓（静默：状态 200、`count: 0` ✓）。
 *   **∴ 所以**读日志用**不带框架参数**的这条 ✓ —— 否则判据在**自己造的空列表**上判「找不到原子」✗
 *   （这正是「测量工具要先被测量」✓：产品端 `get_log` 的 actor 筛选本身是对的 ✓）。
 */
const callServerBare = async (tool, args) => {
  const res = await nativeFetch(`${BASE}/api/tools/${tool}?token=${TOKEN}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: SERVER_DOC, ...args }),
  });
  return { status: res.status, body: await res.json().catch(() => null) };
};
/** **本地抛异常记成产品失败 ✗ 不是判据坏了 ✓**（与 `tool-pwa-pixel-stats` 同一纪律 ✓）。 */
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
const show = (value) => String(JSON.stringify(value)).slice(0, 280);
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
/** **错误逐字比 ✓**（期望文本可分开给：原子 id 两边不同 ✓）。 */
function compareError(label, srv, loc, status, code, expectedS, expectedL) {
  if (!srv.body || !loc.body) { check(false, `${label}：有一边读不出 JSON ✗`); return; }
  check(srv.status === status, `${label}：服务端状态码应为 ${status} ✗ 实测 ${srv.status}｜${show(srv.body)}`);
  check(loc.status === status, `${label}：本地状态码应为 ${status} ✗ 实测 ${loc.status}｜${show(loc.body)}`);
  check(srv.body.error_code === code, `${label}：服务端错误码应为 ${code} ✗ 实测 ${srv.body.error_code} ⇒ 判据口径要跟着权威改`);
  check(loc.body.error_code === code, `${label}：本地错误码应为 ${code} ✗ 实测 ${loc.body.error_code}`);
  const sd = srv.body.context && srv.body.context.detail;
  const ld = loc.body.context && loc.body.context.detail;
  check(typeof ld === "string" && ld.length > 0, `${label}：本地没给 context.detail ⇒ 调用方读不到原因 ✗`);
  const wantS = expectedS === undefined ? expectedL : expectedS;
  const wantL = expectedL === undefined ? expectedS : expectedL;
  if (wantS !== undefined) {
    check(sd === wantS, `${label}：服务端文案与判据的期望不同 ✗（权威口径变了就改判据）\n     期望 ${wantS}\n     实测 ${sd}`);
    check(ld === wantL, `${label}：本地文案与判据的期望不同 ✗\n     期望 ${wantL}\n     实测 ${ld}`);
  }
}

// ── 原子日志的读取（两边字段名不同：服务端 `atom_id`／本地 `id` ✓）──────────────
const getLog = async (side) => {
  const res = side === "server" ? await callServerBare("get_log", { limit: 2000 })
    : await localCall("get_log", { limit: 2000 });
  if (res.status !== 200 || !res.body || !Array.isArray(res.body.atoms)) {
    check(false, `get_log（${side}）读不出原子列表 ✗ ${res.status}｜${show(res.body)}`);
    return null;
  }
  return res.body;
};
const rowId = (row) => (row && (row.atom_id ?? row.id)) || null;
const rowOf = (log, id) => (log ? log.atoms.find((row) => rowId(row) === id) || null : null);
/** **对象创建原子 = `get_object.created_by` ✓**，再从日志取 seq ✓（两边 seq 必须相同 ✓）。 */
const objectAtom = async (side, objectId) => {
  const res = side === "server" ? await callServer("get_object", { object_id: objectId })
    : await localCall("get_object", { object_id: objectId });
  if (res.status !== 200 || !res.body || typeof res.body.created_by !== "string") {
    check(false, `get_object(${objectId})（${side}）没给 created_by ✗ ${res.status}｜${show(res.body)}`);
    return null;
  }
  return res.body.created_by;
};

// ══ 前序：三层 ＋ 三个矩形（互不重叠）═════════════════════════════════════════
const RECTS = {
  S1: { layer: "L1", x: 4, y: 4, w: 16, h: 16, color: [1, 0, 0, 1] },     // 红 256
  S2: { layer: "L2", x: 32, y: 32, w: 16, h: 12, color: [0, 0, 1, 1] },   // 蓝 192
  S3: { layer: "L3", x: 40, y: 4, w: 12, h: 12, color: [0, 1, 0, 1] },    // 绿 144
  S4: { layer: "L4", x: 52, y: 36, w: 8, h: 8, color: [0, 0, 0, 1] },     // 黑 64
};
const drawShape = (objectId) => {
  const r = RECTS[objectId];
  return { layer_id: r.layer, object_id: objectId,
    data: { geometry: { kind: "rect", bbox: { x: r.x, y: r.y, w: r.w, h: r.h } }, color: r.color } };
};
for (const layerId of ["L1", "L2", "L3"]) {
  await setup(`建图层 ${layerId}`, "create_layer", { layer_id: layerId, name: "层 " + layerId });
}
for (const objectId of ["S1", "S2", "S3"]) {
  await setup(`画 ${objectId}`, "draw_shape", drawShape(objectId));
}

const sAtomS1 = await objectAtom("server", "S1");
const lAtomS1 = await objectAtom("local", "S1");
const sAtomS2 = await objectAtom("server", "S2");
const lAtomS2 = await objectAtom("local", "S2");
const logBefore = { server: await getLog("server"), local: await getLog("local") };
const seqOfAtom = (side, id, wantObjectId) => {
  const row = rowOf(logBefore[side], id);
  if (!row) { check(false, `日志（${side}）里找不到原子 ${id} ✗`); return null; }
  check(row.payload && row.payload.object_id === wantObjectId,
    `日志（${side}）里 ${id} 的净荷 object_id 应为 ${wantObjectId} ✗ ${show(row.payload)}`);
  return row.seq;
};
const sSeqS1 = seqOfAtom("server", sAtomS1, "S1");
const lSeqS1 = seqOfAtom("local", lAtomS1, "S1");
const sSeqS2 = seqOfAtom("server", sAtomS2, "S2");
const lSeqS2 = seqOfAtom("local", lAtomS2, "S2");
// **∴ 同一串操作 ⇒ 两边 seq 必须相同 ✓**（这也是下面错误文案里 seq 的独立来源 ✓）。
check(sSeqS1 === lSeqS1, `S1 创建原子的 seq 两边不同 ✗ 服务端 ${sSeqS1}｜本地 ${lSeqS1}`);
check(sSeqS2 === lSeqS2, `S2 创建原子的 seq 两边不同 ✗ 服务端 ${sSeqS2}｜本地 ${lSeqS2}`);
check(Number.isInteger(sSeqS1) && Number.isInteger(sSeqS2) && sSeqS2 > sSeqS1,
  `两个创建原子的 seq 关系不对 ✗ ${show([sSeqS1, sSeqS2])}`);

// ══ §A checkpoint ＋ get_checkpoints 的六个键 ══════════════════════════════════
const CP_NAME = "CP1";
const CP_MESSAGE = "第一版说明";
const [sCp, lCp] = await both("checkpoint", { name: CP_NAME, message: CP_MESSAGE });
check(sCp.status === 200 && sCp.body && sCp.body.ok === true,
  `§A 服务端 checkpoint 失败 ✗ ${sCp.status}｜${show(sCp.body)}`);
check(lCp.status === 200 && lCp.body && lCp.body.ok === true,
  `§A 本地 checkpoint 失败 ✗ ${lCp.status}｜${show(lCp.body)}`);
const sCpSeq = sCp.body && sCp.body.seq;
const lCpSeq = lCp.body && lCp.body.seq;
check(sCpSeq === lCpSeq, `§A checkpoint 的 seq 两边不同 ✗ 服务端 ${sCpSeq}｜本地 ${lCpSeq}`);
check(typeof sCp.body.checkpoint_id === "string" && sCp.body.checkpoint_id.startsWith("ckpt_"),
  `§A 服务端 checkpoint_id 应以 ckpt_ 开头 ✗ ${show(sCp.body.checkpoint_id)}`);
check(typeof lCp.body.checkpoint_id === "string" && lCp.body.checkpoint_id.startsWith("ckpt_"),
  `§A 本地 checkpoint_id 应以 ckpt_ 开头 ✗ ${show(lCp.body.checkpoint_id)}`);

const logAfterCp = { server: await getLog("server"), local: await getLog("local") };
const sCpRow = rowOf(logAfterCp.server, sCp.body && sCp.body.atom_id);
const lCpRow = rowOf(logAfterCp.local, lCp.body && lCp.body.atom_id);
check(!!sCpRow && sCpRow.kind === "create_checkpoint",
  `§A 服务端检查点原子 kind 应为 create_checkpoint ✗ ${show(sCpRow && sCpRow.kind)}`);
check(!!lCpRow && lCpRow.kind === "checkpoint",
  `§A 本地检查点原子 kind 应为 checkpoint ✗ ${show(lCpRow && lCpRow.kind)}`);
for (const [who, row] of [["服务端", sCpRow], ["本地", lCpRow]]) {
  if (!row) { check(false, `§A（${who}）日志里找不到检查点原子 ✗`); continue; }
  check(row.payload && row.payload.name === CP_NAME,
    `§A（${who}）检查点净荷 name 应为 ${CP_NAME} ✗ ${show(row.payload)}`);
  check(row.payload && row.payload.message === CP_MESSAGE,
    `§A（${who}）检查点净荷 message 应为「${CP_MESSAGE}」✗ ${show(row.payload)}`);
  check(row.payload && row.payload.anchor_seq === row.seq - 1,
    `§A（${who}）anchor_seq 应恰为原子 seq − 1 ✗ seq=${row.seq}｜anchor=${show(row.payload && row.payload.anchor_seq)}`);
  // **∴ `message` 在封套上是空的 ✗**（净荷那份不被折叠读 ✓）⇒ 只有净荷有 ✓。
  check(row.message === undefined,
    `§A（${who}）原子封套上不该出现 message ✗ ${show(row.message)}`);
}

const [sGc, lGc] = await both("get_checkpoints", {});
check(sGc.status === 200 && sGc.body && Array.isArray(sGc.body.checkpoints),
  `§A 服务端 get_checkpoints 失败 ✗ ${sGc.status}｜${show(sGc.body)}`);
check(lGc.status === 200 && lGc.body && Array.isArray(lGc.body.checkpoints),
  `§A 本地 get_checkpoints 失败 ✗ ${lGc.status}｜${show(lGc.body)}`);
if (sGc.body && lGc.body && Array.isArray(sGc.body.checkpoints) && Array.isArray(lGc.body.checkpoints)) {
  check(sGc.body.count === 1 && lGc.body.count === 1,
    `§A get_checkpoints 应恰有 1 个检查点 ✗ 服务端 ${show(sGc.body.count)}｜本地 ${show(lGc.body.count)}`);
  const sItem = sGc.body.checkpoints[0];
  const lItem = lGc.body.checkpoints[0];
  const WANT_KEYS = ["anchor_seq", "checkpoint_id", "created_by", "message", "name", "snapshot_id"];
  for (const [who, item] of [["服务端", sItem], ["本地", lItem]]) {
    if (!item) { check(false, `§A（${who}）get_checkpoints 没有第 1 项 ✗`); continue; }
    check(same(Object.keys(item).sort(), WANT_KEYS),
      `§A（${who}）检查点应恰有六个键（缺值给 null）✗ 实测 ${show(Object.keys(item).sort())}`);
    check(typeof item.checkpoint_id === "string" && item.checkpoint_id.startsWith("ckpt_"),
      `§A（${who}）checkpoint_id 应以 ckpt_ 开头 ✗ ${show(item.checkpoint_id)}`);
    check(item.name === CP_NAME, `§A（${who}）name 应为 ${CP_NAME} ✗ ${show(item.name)}`);
    check(typeof item.created_by === "string" && item.created_by.length > 0,
      `§A（${who}）created_by 应是非空字符串 ✗ ${show(item.created_by)}`);
    // **∴ 这两条是本判据最重要的一对 ✓**：`checkpoint` 的 message 参数**不**进
    //   `Checkpoint.message`（那是原子封套 ✓），而 `snapshot_id` 提交时**不写净荷** ✓。
    check(item.message === null,
      `§A（${who}）message 应为 null（取自原子封套，不是 netload）✗ ${show(item.message)}`
      + " ⇒ 很像把 payload.message 当成了检查点说明");
    check(item.snapshot_id === null,
      `§A（${who}）snapshot_id 应为 null（提交时不写净荷）✗ ${show(item.snapshot_id)}`);
    check(item.anchor_seq === sCpSeq - 1,
      `§A（${who}）anchor_seq 应为检查点原子 seq − 1 = ${sCpSeq - 1} ✗ ${show(item.anchor_seq)}`);
  }
}

// ══ §B 检查点之后画 S4 ⇒ 状态 5 层 4 对象，像素含 64 黑 ═══════════════════════
await setup("建图层 L4", "create_layer", { layer_id: "L4", name: "层 L4" });
await setup("画 S4", "draw_shape", drawShape("S4"));

const LAYER_IDS_WITH_S4 = ["layer_default", "L1", "L2", "L3", "L4"];
const LAYER_IDS_AFTER_RESTORE = ["layer_default", "L1", "L2", "L3"];
/** **`revert_to(S1 的创建原子)` 之后仍有三层 ✓**：三个图层原子都**早于** S1（先建层、后画 ✓）
 *   ⇒ 跳到 S1 的 seq ⇒ 图层一个不少 ✓。**∴ 本判据原来写「只留 L1」✗ 是自己算错了**
 *   （那样要求 S1 落在 L1 与 L2 之间 ✗）—— 实测服务端与本地**都**给四层 ✓ ⇒ **不是产品缺陷** ✓。
 */
const LAYER_IDS_AT_S1 = ["layer_default", "L1", "L2", "L3"];
const LAYER_PROJ = ["layer_id", "name", "type", "z_index", "visible", "opacity",
  "blend_mode", "locked", "mask_id"];
const OBJECT_PROJ = ["object_id", "layer_id", "type", "z_index", "visible", "enabled",
  "locked", "medium"];
const projLayer = (l) => Object.fromEntries(LAYER_PROJ.map((k) => [k, l[k]]));
const projObject = (o) => Object.fromEntries(OBJECT_PROJ.map((k) => [k, o[k]]));

async function stateCase(label, wantLayerIds, wantObjectIds, wantCheckpoints) {
  const [s, l] = await both("get_state", { include_objects: true });
  check(s.status === 200 && s.body, `${label}：服务端 get_state 失败 ✗ ${s.status}｜${show(s.body)}`);
  check(l.status === 200 && l.body, `${label}：本地 get_state 失败 ✗ ${l.status}｜${show(l.body)}`);
  if (!s.body || !l.body) return;
  check(same(projLayerList(s.body), projLayerList(l.body)),
    `${label}：图层逐字段不同 ✗\n     服务端 ${show(projLayerList(s.body))}\n     本地     ${show(projLayerList(l.body))}`);
  check(same(projObjectList(s.body), projObjectList(l.body)),
    `${label}：对象摘要逐字段不同 ✗\n     服务端 ${show(projObjectList(s.body))}\n     本地     ${show(projObjectList(l.body))}`);
  for (const [who, body] of [["服务端", s.body], ["本地", l.body]]) {
    const ids = (body.layers || []).map((x) => x.layer_id);
    check(same(ids, wantLayerIds),
      `${label}（${who}）图层应为 ${show(wantLayerIds)} ✗ 实测 ${show(ids)}`);
    const objectIds = (body.objects || []).map((x) => x.object_id).sort();
    check(same(objectIds, wantObjectIds.slice().sort()),
      `${label}（${who}）对象应为 ${show(wantObjectIds)} ✗ 实测 ${show(objectIds)}`);
    check(body.checkpoints === wantCheckpoints,
      `${label}（${who}）checkpoints 应为 ${wantCheckpoints} ✗ 实测 ${show(body.checkpoints)}`);
  }
  // **∴ 两份摘要的来源是**同一份** ✓（`summary_object`／`p2ObjectSummaries` ✓）⇒ 计数必须一致 ✓。
  const [sList, lList] = await both("list_objects", {});
  for (const [who, body] of [["服务端", sList.body], ["本地", lList.body]]) {
    check(body && body.count === wantObjectIds.length,
      `${label}（${who}）list_objects.count 应为 ${wantObjectIds.length} ✗ ${show(body && body.count)}`);
  }
  const [sLayers, lLayers] = await both("list_layers", {});
  for (const [who, body] of [["服务端", sLayers.body], ["本地", lLayers.body]]) {
    check(body && body.count === wantLayerIds.length,
      `${label}（${who}）list_layers.count 应为 ${wantLayerIds.length} ✗ ${show(body && body.count)}`);
  }
}
const projLayerList = (body) => (body.layers || []).map(projLayer);
const projObjectList = (body) => (body.objects || []).map(projObject);

const STAT_KEYS = ["width", "height", "pixels", "dominant_colors",
  "warm_pixels", "cool_pixels", "avg_brightness"];
async function statsCase(label) {
  const region = { x: 0, y: 0, w: DOC_W, h: DOC_H };
  const [s, l] = await both("analyze_region", { region });
  check(s.status === 200 && s.body && s.body.ok !== false,
    `${label}：服务端 analyze_region 失败 ✗ ${s.status}｜${show(s.body)}`);
  check(l.status === 200 && l.body && l.body.ok === true,
    `${label}：本地 analyze_region 失败 ✗ ${l.status}｜${show(l.body)}`);
  if (!s.body || !l.body) return null;
  for (const key of STAT_KEYS) {
    check(same(s.body[key], l.body[key]),
      `${label}：字段「${key}」不同 ✗ 服务端 ${show(s.body[key])}｜本地 ${show(l.body[key])}`);
  }
  return [s.body, l.body];
}
/** **桶的独立期望 ✓**：判据自己算每个颜色的像素数 ✓（`hex ⇒ count` ✓）。 */
function expectBuckets(label, who, body, expected, warm, cool) {
  const got = {};
  for (const bucket of body.dominant_colors || []) got[bucket.hex] = bucket.count;
  const want = Object.keys(expected).sort();
  check(same(Object.keys(got).sort(), want),
    `${label}（${who}）桶集合应为 ${show(want)} ✗ 实测 ${show(Object.keys(got).sort())}`);
  for (const hex of want) {
    check(got[hex] === expected[hex],
      `${label}（${who}）桶 ${hex} 计数应为 ${expected[hex]} ✗ 实测 ${show(got[hex])}`);
  }
  const total = Object.values(got).reduce((a, b) => a + b, 0);
  check(total === body.pixels && body.pixels === DOC_W * DOC_H,
    `${label}（${who}）桶计数之和应等于像素数 ${DOC_W * DOC_H} ✗ 和=${total}｜pixels=${show(body.pixels)}`);
  check(body.warm_pixels === warm && body.cool_pixels === cool,
    `${label}（${who}）暖/冷应为 ${warm}/${cool} ✗ 实测 ${show([body.warm_pixels, body.cool_pixels])}`);
}

// §B 恢复前：白 2416 ＋ 红 256 ＋ 蓝 192 ＋ 绿 144 ＋ 黑 64 = 3072
await stateCase("§B 恢复前", LAYER_IDS_WITH_S4, ["S1", "S2", "S3", "S4"], 1);
{
  const got = await statsCase("§B 恢复前像素");
  if (got) {
    const want = { "#ffffff": 2416, "#ff0000": 256, "#0000ff": 192, "#00ff00": 144, "#000000": 64 };
    for (const [who, body] of [["服务端", got[0]], ["本地", got[1]]]) expectBuckets("§B", who, body, want, 256, 192);
  }
}

// ══ §C restore_checkpoint ⇒ 求值起点跳到 CP1 ══════════════════════════════════
const sRestore = await callServer("restore_checkpoint", { checkpoint_id: sCp.body.checkpoint_id });
const lRestore = await localCall("restore_checkpoint", { checkpoint_id: lCp.body.checkpoint_id });
check(sRestore.status === 200 && sRestore.body && sRestore.body.ok === true,
  `§C 服务端 restore_checkpoint 失败 ✗ ${sRestore.status}｜${show(sRestore.body)}`);
check(lRestore.status === 200 && lRestore.body && lRestore.body.ok === true,
  `§C 本地 restore_checkpoint 失败 ✗ ${lRestore.status}｜${show(lRestore.body)}`);
const sOrigin = sRestore.body && sRestore.body.seq;
const lOrigin = lRestore.body && lRestore.body.seq;
check(sOrigin === lOrigin, `§C restore 的 seq 两边不同 ✗ 服务端 ${sOrigin}｜本地 ${lOrigin}`);
check(typeof sRestore.body.atom_id === "string" && typeof lRestore.body.atom_id === "string",
  `§C restore 两边都应回原子 id ✗ 服务端 ${show(sRestore.body.atom_id)}｜本地 ${show(lRestore.body.atom_id)}`);
for (const [who, body] of [["服务端", sRestore.body], ["本地", lRestore.body]]) {
  check(body.head === body.seq,
    `§C（${who}）head 应等于 seq ✗ ${show([body.head, body.seq])}`);
  check(body.dirty_kind === "full",
    `§C（${who}）dirty_kind 应为 full ✗ ${show(body.dirty_kind)}`);
}
const logAfterRestore = { server: await getLog("server"), local: await getLog("local") };
for (const [who, log] of [["服务端", logAfterRestore.server], ["本地", logAfterRestore.local]]) {
  check(log && log.head_seq === sOrigin,
    `§C（${who}）get_log.head_seq 应为 ${sOrigin} ✗ ${show(log && log.head_seq)}`);
}

await stateCase("§C 恢复后", LAYER_IDS_AFTER_RESTORE, ["S1", "S2", "S3"], 0);
{
  // **∴ `get_checkpoints` 变 0 ✗**：检查点原子在窗口外（`(0, 锚点] ∪ (跳变, n]` ✓）。
  const [s, l] = await both("get_checkpoints", {});
  check(s.body && s.body.count === 0,
    `§C（服务端）恢复后 get_checkpoints 应为空（检查点原子在窗口外）✗ ${show(s.body)}`);
  check(l.body && l.body.count === 0,
    `§C（本地）恢复后 get_checkpoints 应为空（检查点原子在窗口外）✗ ${show(l.body)}`);
  const s4 = await callServer("get_object", { object_id: "S4" });
  const l4 = await localCall("get_object", { object_id: "S4" });
  compareError("§C 恢复后 S4 不存在", s4, l4, 404, "reference_not_found", "对象 S4 不存在");
  const s1 = await callServer("get_object", { object_id: "S1" });
  const l1 = await localCall("get_object", { object_id: "S1" });
  check(s1.status === 200, `§C（服务端）S1 应仍在状态里 ✗ ${s1.status}｜${show(s1.body)}`);
  check(l1.status === 200, `§C（本地）S1 应仍在状态里 ✗ ${l1.status}｜${show(l1.body)}`);
  const got = await statsCase("§C 恢复后像素");
  if (got) {
    const want = { "#ffffff": 2480, "#ff0000": 256, "#0000ff": 192, "#00ff00": 144 };
    for (const [who, body] of [["服务端", got[0]], ["本地", got[1]]]) expectBuckets("§C", who, body, want, 256, 192);
  }
}

// ══ §D 跨求值起点的撤销被拒（单条／批量／重做）════════════════════════════════
const crossOriginText = (id, seq, origin) => "目标原子 " + id + " (seq " + seq + ") 位于当前求值起点 seq "
  + origin + " 之前，需先回到该起点或重新提交等价操作";
{
  const sRv = await callServer("revert", { atom_id: sAtomS2 });
  const lRv = await localCall("revert", { atom_id: lAtomS2 });
  compareError("§D1 revert 跨起点", sRv, lRv, 400, "invalid_argument",
    crossOriginText(sAtomS2, sSeqS2, sOrigin), crossOriginText(lAtomS2, lSeqS2, lOrigin));
}
{
  const sRa = await callServer("reapply", { atom_id: sAtomS2 });
  const lRa = await localCall("reapply", { atom_id: lAtomS2 });
  compareError("§D2 reapply 跨起点", sRa, lRa, 400, "invalid_argument",
    crossOriginText(sAtomS2, sSeqS2, sOrigin), crossOriginText(lAtomS2, lSeqS2, lOrigin));
}
{
  const [s, l] = [await callServer("revert_atoms", { atom_ids: [sAtomS2] }),
    await localCall("revert_atoms", { atom_ids: [lAtomS2] })];
  for (const [who, res, id] of [["服务端", s, sAtomS2], ["本地", l, lAtomS2]]) {
    check(res.body && res.body.ok === false,
      `§D3 批量（${who}）ok 应为 false ✗ ${show(res.body && res.body.ok)}`);
    check(res.body && res.body.reverted_count === 0 && res.body.failed_count === 1,
      `§D3 批量（${who}）应为 0 成功 1 失败 ✗ ${show([res.body && res.body.reverted_count, res.body && res.body.failed_count])}`);
    const failed = res.body && Array.isArray(res.body.failed) ? res.body.failed[0] : null;
    check(failed && failed.atom_id === id,
      `§D3 批量（${who}）failed[0].atom_id 应为 ${id} ✗ ${show(failed)}`);
    check(failed && failed.error === "invalid_argument: " + crossOriginText(id,
      who === "服务端" ? sSeqS2 : lSeqS2, who === "服务端" ? sOrigin : lOrigin),
    `§D3 批量（${who}）failed[0].error 文案不同 ✗ ${show(failed && failed.error)}`);
  }
}

// ══ §E revert_to（原子跳变）＋ 再恢复（多层链）═════════════════════════════════
const sRT = await callServer("revert_to", { atom_id: sAtomS1 });
const lRT = await localCall("revert_to", { atom_id: lAtomS1 });
check(sRT.status === 200 && sRT.body && sRT.body.ok === true,
  `§E 服务端 revert_to 失败 ✗ ${sRT.status}｜${show(sRT.body)}`);
check(lRT.status === 200 && lRT.body && lRT.body.ok === true,
  `§E 本地 revert_to 失败 ✗ ${lRT.status}｜${show(lRT.body)}`);
check(sRT.body && sRT.body.head === sRT.body.seq && lRT.body && lRT.body.head === lRT.body.seq,
  `§E revert_to 两边 head 都应等于 seq ✗ 服务端 ${show(sRT.body && [sRT.body.head, sRT.body.seq])}`
  + `｜本地 ${show(lRT.body && [lRT.body.head, lRT.body.seq])}`);
check(sRT.body && lRT.body && sRT.body.seq === lRT.body.seq,
  `§E revert_to 的 seq 两边不同 ✗ ${show([sRT.body && sRT.body.seq, lRT.body && lRT.body.seq])}`);

await stateCase("§E revert_to 之后", LAYER_IDS_AT_S1, ["S1"], 0);
{
  const got = await statsCase("§E revert_to 之后像素");
  if (got) {
    const want = { "#ffffff": 2816, "#ff0000": 256 };
    for (const [who, body] of [["服务端", got[0]], ["本地", got[1]]]) expectBuckets("§E", who, body, want, 256, 0);
  }
}
// **∴ 多层链 ✓**：`revert_to` 之后再 `restore_checkpoint(CP1)` ⇒
//   窗口 = `(0, CP1 锚点] ∪ (新跳变, n]` ⇒ 中间那次 `revert_to` 被跳过 ✓。
const sRestore2 = await callServer("restore_checkpoint", { checkpoint_id: sCp.body.checkpoint_id });
const lRestore2 = await localCall("restore_checkpoint", { checkpoint_id: lCp.body.checkpoint_id });
check(sRestore2.status === 200 && sRestore2.body && sRestore2.body.ok === true,
  `§E 第二次 restore（服务端）失败 ✗ ${sRestore2.status}｜${show(sRestore2.body)}`);
check(lRestore2.status === 200 && lRestore2.body && lRestore2.body.ok === true,
  `§E 第二次 restore（本地）失败 ✗ ${lRestore2.status}｜${show(lRestore2.body)}`);
check(sRestore2.body && lRestore2.body && sRestore2.body.seq === lRestore2.body.seq,
  `§E 第二次 restore 的 seq 两边不同 ✗ ${show([sRestore2.body && sRestore2.body.seq, lRestore2.body && lRestore2.body.seq])}`);
await stateCase("§E 第二次恢复后", LAYER_IDS_AFTER_RESTORE, ["S1", "S2", "S3"], 0);
{
  const [s, l] = await both("get_checkpoints", {});
  check(s.body && s.body.count === 0 && l.body && l.body.count === 0,
    `§E 第二次恢复后 get_checkpoints 应为空 ✗ 服务端 ${show(s.body && s.body.count)}｜本地 ${show(l.body && l.body.count)}`);
  const got = await statsCase("§E 第二次恢复后像素");
  if (got) {
    const want = { "#ffffff": 2480, "#ff0000": 256, "#0000ff": 192, "#00ff00": 144 };
    for (const [who, body] of [["服务端", got[0]], ["本地", got[1]]]) expectBuckets("§E2", who, body, want, 256, 192);
  }
}

// ══ §E3 真正的多层链（窗口必须两段拼接）═══════════════════════════════════════
// **∴ 上面那次「第二次恢复」并不构成多层链 ✗**：它的 base 是检查点锚点 7，**早于**前一个
//   `declare_head`（seq 11）⇒ `headUpto(7)` 为空 ⇒ 窗口仍只有一段 ✓
//   （**∴ 那段验的是「恢复盖住恢复」✓，不是链 ✓**）。
// **∴ 链要成立 ✗**：后一个 head 的 base 必须**落在前一个 head 之后** ✓
//   ⇒ 才需要 `(0,7] ∪ (11,14]` 两段拼接 ✓。**∴ 而单段近似**会把 8,9,10 三条原子
//   （检查点／L4／S4）也含进来 ⇒ **检查点数、图层、黑像素三处同时变** ✓ —— 独立期望如下 ✓。
await setup("§E3 建图层 L5", "create_layer", { layer_id: "L5", name: "层 L5" });
await setup("§E3 建图层 L6", "create_layer", { layer_id: "L6", name: "层 L6" });
// **∴ 图层原子的 id 从日志取 ✗**：本地 `create_layer` 的回执是**精简形状**（无 `atom_id` ✓），
//   而服务端是 `commit_response`（有 ✓）⇒ **∴ 判据不能拿本地回执去要 atom_id** ✓
//   （**∴ 两边都从日志按 `payload.layer_id` 找 ✓** —— 同一口径、不依赖回执形状 ✓）。
const logForE3 = { server: await getLog("server"), local: await getLog("local") };
const layerAtomId = (side, layerId) => {
  const rows = (logForE3[side] && logForE3[side].atoms) || [];
  const row = rows.find((r) => r.payload && r.payload.layer_id === layerId
    && r.kind === "create_layer");
  check(!!row, `§E3（${side}）日志里找不到图层 ${layerId} 的创建原子 ✗`);
  return row ? rowId(row) : null;
};
const sL5Atom = layerAtomId("server", "L5");
const lL5Atom = layerAtomId("local", "L5");
{
  const sRT2 = await callServer("revert_to", { atom_id: sL5Atom });
  const lRT2 = await localCall("revert_to", { atom_id: lL5Atom });
  check(sRT2.status === 200 && sRT2.body && sRT2.body.ok === true,
    `§E3 服务端链式 revert_to 失败 ✗ ${sRT2.status}｜${show(sRT2.body)}`);
  check(lRT2.status === 200 && lRT2.body && lRT2.body.ok === true,
    `§E3 本地链式 revert_to 失败 ✗ ${lRT2.status}｜${show(lRT2.body)}`);
  check(sRT2.body && lRT2.body && sRT2.body.seq === lRT2.body.seq,
    `§E3 链式 revert_to 的 seq 两边不同 ✗ ${show([sRT2.body && sRT2.body.seq, lRT2.body && lRT2.body.seq])}`);
}
// **∴ 期望 = `state@(L5 的 seq)`**：`(0,7]`（三层＋三个矩形）＋ `(11,该 seq]`（L5）
//   ⇒ L6（在 base 之后）**不在**其中 ✓；检查点／L4／S4 也不在 ✓。
await stateCase("§E3 链式跳变之后", ["layer_default", "L1", "L2", "L3", "L5"], ["S1", "S2", "S3"], 0);
{
  const got = await statsCase("§E3 链式跳变之后像素");
  if (got) {
    const want = { "#ffffff": 2480, "#ff0000": 256, "#0000ff": 192, "#00ff00": 144 };
    for (const [who, body] of [["服务端", got[0]], ["本地", got[1]]]) expectBuckets("§E3", who, body, want, 256, 192);
  }
}

// ══ §F 参数／引用错误逐字 ══════════════════════════════════════════════════════
{
  const f = await both("restore_checkpoint", {});
  compareError("§F1 restore 缺 checkpoint_id", f[0], f[1], 400, "invalid_argument",
    "缺少必填参数 checkpoint_id");
}
{
  const f = await both("restore_checkpoint", { checkpoint_id: "nope" });
  compareError("§F2 restore 未知检查点", f[0], f[1], 404, "reference_not_found",
    "检查点 nope 不存在");
}
{
  const f = await both("restore_checkpoint", { checkpoint_id: 5 });
  compareError("§F3 restore 类型错", f[0], f[1], 400, "invalid_argument",
    "参数 checkpoint_id 类型应为 string，实际为 number");
}
{
  const f = await both("restore_checkpoint", { checkpoint_id: "x", bogus: 1 });
  const want = "restore_checkpoint 不接受参数 bogus（拼写错误？）；可用参数：checkpoint_id；框架级参数：doc_id, actor, session";
  compareError("§F4 restore 未知参数", f[0], f[1], 400, "invalid_argument", want);
}
{
  const f = await both("revert", {});
  compareError("§F5 revert 缺 atom_id", f[0], f[1], 400, "invalid_argument",
    "缺少必填参数 atom_id");
}
{
  const f = await both("reapply", { atom_id: "nope" });
  compareError("§F6 reapply 未知原子", f[0], f[1], 404, "reference_not_found",
    "目标原子 nope 不在日志中");
}
{
  const f = await both("revert_to", { atom_id: "nope" });
  compareError("§F7 revert_to 未知原子", f[0], f[1], 404, "reference_not_found",
    "declare_head 的 base 原子 nope 不在日志中");
}
{
  const f = await both("declare_head", { base_type: "nope", base_id: "x" });
  compareError("§F8 declare_head 坏 base_type", f[0], f[1], 400, "invalid_argument",
    "base_type 必须是 atom 或 checkpoint，得到 nope");
}
{
  const f = await both("declare_head", { base_type: "atom", base_id: "nope" });
  compareError("§F9 declare_head 未知原子", f[0], f[1], 404, "reference_not_found",
    "declare_head 的 base 原子 nope 不在日志中");
}
{
  const f = await both("declare_head", { base_type: "checkpoint", base_id: "nope" });
  compareError("§F10 declare_head 未知检查点", f[0], f[1], 404, "reference_not_found",
    "检查点 nope 不存在");
}

if (bad.length) {
  console.log(`✗ PWA 本地检查点／求值起点跳变与服务端同口径：${bad.length} 条不符（另有 ${okCount} 条通过）`);
  for (const m of bad) console.log("   · " + m);
  process.exit(1);
}
console.log(`✓ PWA 本地检查点／求值起点跳变与服务端同口径（${okCount} 条断言：`
  + "checkpoint 锚点＝seq−1／get_checkpoints 六个键含两个 null／message 取封套不取净荷／"
  + "restore 后 state@seq 只含锚点前＋跳变后（S4 消失、checkpoints 归零、黑像素消失）／"
  + "跨起点 revert·reapply·revert_atoms 逐字拒绝／revert_to 后二次恢复的多层链／"
  + "十条参数与引用的逐字文案）");
