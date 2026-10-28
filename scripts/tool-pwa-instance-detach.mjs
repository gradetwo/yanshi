#!/usr/bin/env node
// **判据：PWA 本地的 `detach_instance` 与服务端**同一输入同一输出 ✓ ★**（台账 §659）。
//
// **∴ 它测什么 ✗**（**内核 ①：断言必须能证明自己能红 ✓**）：
//   `detach_instance` 在本地是**从原子日志重算**的 ✗ ⇒ **同一个请求分别打到真服务端与本地
//   `api-local` ✓** ⇒ **比解析后的 JSON 与比折叠后的对象状态 ✓**（**不比字节 ✓**）。
//   **∴ 三件事必须同时成立 ✗**：
//   ① **副本变换 = 实例自身 ∘ local_transform ∘ master 自身 ✓** ——
//      判据**自己**按这三个输入算一个期望矩阵 ✗（**∴ 不能只断「两边相等」**：
//      两边**同错**时相等断言照样绿 ✓ ⇒ **那是安慰剂** ✓）；
//   ② **副本数据 = master 的折叠 `data` ✓**；**`type` = `data.type`，缺省 `shape` ✓**
//      （**∴ 服务端就是这条口径 ✓ —— 实例是 `adjustment` 时副本也是 `shape` ✓，
//       **不是** master 顶层的 `type` ✗**）；
//   ③ **两步（建副本 ＋ 删实例）同属一个变更集 ✓** ⇒ `revert_changeset` **一把撤掉两步** ✓
//      （**∴ 断「撤完：副本从状态消失 ✓ ＋ 实例复活 ✓」** —— 只断回执 `reverted` 会漏掉半写 ✓）。
//   **∴ 报错口径逐字比 ✓**：不存在／不是实例／缺 `master_ref.object_id`／master 已删 ✓。
//
// **∴ 本判据对着的两种「状态」口径 ✗**（**∴ 与本地实现同一份权威 ✓**）：
//   · **「在状态里」= 创建未被 revert ✓**（**墓碑对象仍在 `state.objects` ✓**
//     ⇒ **∴ 已删实例的**前置检查**全过 ✓**，但**第二步 tombstone 被提交期校验拒绝** ⇒ 409 ✓）；
//   · **「master 已删」= 折叠加图层级联 ✓**（**`fold.rs:1988` 给层里每个对象写 `deleted_by` ✓**）。
//
// **∴ 前提 ✗**：需要**活着的服务端**（`run-criteria-local.sh` 起一个并把地址放进 argv[2] ✓）。
//   **拿不到 ⇒ `exit(3)`（判据前提不成立 ≠ 产品失败 ✓）**。
//
// **∴ 变异（四层齐第④层 ✓ 逐条见台账 §659）**：
//   ① `p2ObjectView` 不重放 `move`／`transform` ⇒ §A 的 master／实例变换与副本变换红 ✓；
//   ② 合成顺序改成 `master ∘ local ∘ 实例` ⇒ §A 的期望矩阵红（两边各自仍“自洽”）✓；
//   ③ tombstone 不带 `changeset_id` ⇒ §B 的「撤完实例复活」红（只撤回一条）✓；
//   ④ 去掉 master 已删检查 ⇒ §C③ 本地 200 而服务端 409 ⇒ 红 ✓；
//   ⑤ 副本 `type` 取 master 顶层 `type` ⇒ §A 的 `type` 红（服务端 `shape`）✓；
//   ⑥ `master_ref` 只读创建净荷、不看 `set_property` 覆盖 ⇒ §C⑤ 本地仍 200 ⇒ 红 ✓。
//   ⑦ 本地副本 data 改回裸净荷 `mv.data` ⇒ §E1 缺 override ⇒ 红 ✓（二期复核落地）。
//   ⑧ 去掉 `create_object` 的 p2RefGuard 撞 id 支路 ⇒ §E2 本地 200 而服务端 409 ⇒ 红 ✓。
process.on("uncaughtException", (error) => {
  console.error(String(error && error.stack || error).slice(0, 1200));
  process.exit(3);
});
import { pathToFileURL } from "node:url";

const BASE = process.argv[2];
if (!BASE || !/^https?:\/\//.test(BASE)) {
  console.log("⊘ 不可判：argv[2] 缺服务端地址 ⇒ 请用 bash scripts/run-criteria-local.sh scripts/tool-pwa-instance-detach.mjs 跑"
    + "（判据前提不成立，不是产品问题）");
  process.exit(3);
}

const bad = [];
const check = (cond, msg) => { if (!cond) bad.push(msg); };
const UNIQ = "d" + Date.now().toString(36);
const SERVER_DOC = `dsrv-${UNIQ}`;
const LOCAL_DOC = `dloc-${UNIQ}`;

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
/** **本地抛异常记成产品失败 ✗ 不是判据坏了 ✓**（**与 `tool-pwa-local-write-guards` 同一条纪律 ✓**）。 */
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

/** **比对器 ✓**（与 `tool-pwa-local-write-guards` 同一口径：断结构不断字节 ✓）。 */
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
/** **错误逐字比 ✓**（**∴ 本地错误是扁平的 `error_code` ＋ `context.detail` ✓**）。 */
function compareError(label, srv, loc, status, code) {
  if (!srv.body || !loc.body) { check(false, `${label}：有一边读不出 JSON ✗`); return; }
  check(srv.status === status, `${label}：服务端状态码应为 ${status} ✗ 实测 ${srv.status}｜${String(JSON.stringify(srv.body)).slice(0, 200)}`);
  check(loc.status === status, `${label}：本地状态码应为 ${status} ✗ 实测 ${loc.status}｜${String(JSON.stringify(loc.body)).slice(0, 200)}`);
  check(srv.body.error_code === code, `${label}：服务端错误码应为 ${code} ✗ 实测 ${srv.body.error_code} ⇒ 判据口径要跟着权威改`);
  check(loc.body.error_code === code, `${label}：本地错误码应为 ${code} ✗ 实测 ${loc.body.error_code}`);
  const sd = srv.body.context && srv.body.context.detail;
  const ld = loc.body.context && loc.body.context.detail;
  check(typeof ld === "string" && ld.length > 0, `${label}：本地没给 context.detail ⇒ 调用方读不到原因 ✗`);
  check(sd === ld, `${label}：错误文案不同 ✗\n     服务端 ${sd}\n     本地     ${ld}`);
}
/** **取对象并比指定字段 ✓**（**∴ 两边各自的原子 id 不同 ⇒ `created_by`／`deleted_by` 不在此列 ✓**）。 */
async function compareObject(label, objectId, keys) {
  const [srv, loc] = await both("get_object", { object_id: objectId });
  check(srv.status === loc.status, `${label}：状态不同 ✗ 服务端 ${srv.status}｜本地 ${loc.status}`);
  if (!srv.body || !loc.body) { check(false, `${label}：有一边读不出 JSON ✗`); return [srv, loc]; }
  for (const k of keys) {
    check(k in srv.body, `${label}：服务端响应里没有「${k}」✗ ⇒ 权威口径变了，要改判据`);
    check(same(srv.body[k], loc.body[k]),
      `${label}：字段「${k}」不同 ✗\n     服务端 ${String(JSON.stringify(srv.body[k])).slice(0, 260)}\n     本地     ${String(JSON.stringify(loc.body[k])).slice(0, 260)}`);
  }
  return [srv, loc];
}
/** **两边活着对象的 id 集合必须一样 ✓**（**∴ 删除／复活只看「在不在列表里」✓**）。 */
async function liveIds(label) {
  const [srv, loc] = await both("list_objects", {});
  if (!srv.body || !loc.body) { check(false, `${label}：list_objects 有一边读不出 ✗`); return { s: [], l: [] }; }
  const s = (srv.body.objects || []).map((o) => o.object_id).sort();
  const l = (loc.body.objects || []).map((o) => o.object_id).sort();
  check(same(s, l), `${label}：活着对象集合不同 ✗ 服务端 ${JSON.stringify(s)}｜本地 ${JSON.stringify(l)}`);
  return { s, l };
}

// ── 公共前序：两层 ＋ master（adjustment）＋ 实例 ──
await both("create_layer", { layer_id: "L1" });
await both("create_layer", { layer_id: "L2" });
const mkMaster = (id) => both("add_adjustment", { layer_id: "L1", object_id: id, adjustment_type: "levels" });
const mkInstance = (id, master, extra = {}) => both("create_instance", {
  instance_id: id, layer_id: "L2", master_id: master,
  local_transform: { matrix: [1, 0, 0, 1, 5, 7], pivot: [3, 4] }, ...extra,
});

// ══ §A 成功路径：变换折叠 ＋ 类型口径 ＋ 变更集 ══
{
  await mkMaster("M1");
  // **实例的 `local_transform` 带**线性部分**（2 倍缩放）＋ 非零 pivot ✓**
  //   （**∴ `create_instance` 就收完整矩阵 ✓ —— **不必借 `transform_object` ✓**；
  //   **∴ 且 pivot 折平后会**改变平移** ✓ ⇒ 漏折 pivot 的变异能红 ✓**）。
  await mkInstance("I1", "M1", { local_transform: { matrix: [2, 0, 0, 2, 0, 0], pivot: [3, 4] } });
  // **master 与实例都被 `move` 过 ✗**（**∴ 折叠层把 `delta` 叠进矩阵的 `e,f` ✓**）：
  //   判据顺便断**读路径**也折叠了它 ✓ —— 只修写路径会留下「get_object 报旧变换」的形态③ ✗。
  await both("move_object", { object_id: "M1", delta: { dx: 10, dy: 20 } });
  await both("move_object", { object_id: "I1", delta: { dx: 7, dy: 9 } });
  const [mS, mL] = await compareObject("§A master 的 move 折叠", "M1", ["type", "layer_id", "transform", "data"]);
  check(mS.body && same(mS.body.transform && mS.body.transform.matrix, [1, 0, 0, 1, 10, 20]),
    `§A 服务端 master 的变换应为 [1,0,0,1,10,20] ✗ 实测 ${JSON.stringify(mS.body && mS.body.transform)}`);
  const [i0S, i0L] = await compareObject("§A 实例的 move 折叠", "I1", ["type", "layer_id", "transform", "data"]);
  check(i0S.body && same(i0S.body.transform && i0S.body.transform.matrix, [1, 0, 0, 1, 7, 9]),
    `§A 服务端实例的变换应为 [1,0,0,1,7,9] ✗ 实测 ${JSON.stringify(i0S.body && i0S.body.transform)}`);

  const [sD, lD] = await both("detach_instance", { instance_id: "I1", object_id: "D1" });
  check(sD.status === 200 && lD.status === 200,
    `§A 两边都应 200 ✗ 服务端 ${sD.status}｜本地 ${lD.status}｜${String(JSON.stringify(lD.body)).slice(0, 220)}`);
  if (sD.body && lD.body) {
    check(sD.body.detached_id === "D1" && lD.body.detached_id === "D1",
      `§A detached_id 应为显式给的 D1 ✗ 服务端 ${sD.body.detached_id}｜本地 ${lD.body.detached_id}`);
    check(sD.body.removed_instance === "I1" && lD.body.removed_instance === "I1",
      `§A removed_instance 应为 I1 ✗ 服务端 ${sD.body.removed_instance}｜本地 ${lD.body.removed_instance}`);
    for (const [who, r] of [["服务端", sD], ["本地", lD]]) {
      check(typeof r.body.changeset_id === "string" && r.body.changeset_id.length > 0,
        `§A（${who}）回执缺 changeset_id ✗ ⇒ 调用方没法整组撤销`);
      check(Number.isInteger(r.body.head) && Number.isInteger(r.body.created_head)
        && r.body.head > r.body.created_head,
        `§A（${who}）head/created_head 应为「删实例 > 建副本」的整数 ✗ 实测 ${r.body.head}/${r.body.created_head}`);
    }
  }

  const [dS, dL] = await compareObject("§A 副本 D1", "D1", ["object_id", "layer_id", "type", "data", "transform"]);
  // **期望矩阵 ✗**（**判据自己算 ✓**）：实例自身 [1,0,0,1,7,9] ∘ local([2,0,0,2,0,0] pivot[3,4]) ∘ master([1,0,0,1,10,20])
  //   ⇒ local 的 pivot 折平后是 [2,0,0,2,-3,-4] ✓ ⇒ 内层 = [2,0,0,2, 2·10−3, 2·20−4] = [2,0,0,2,17,36]
  //   ⇒ 外层平移 (7,9) ⇒ [2,0,0,2,24,45] ✓（**∴ 顺序写反或漏折 pivot 都会得到别的数 ✓**）。
  const expected = [2, 0, 0, 2, 24, 45];
  for (const [who, r] of [["服务端", dS], ["本地", dL]]) {
    check(r.body && r.body.transform && same(r.body.transform.matrix, expected),
      `§A（${who}）副本变换应为 [2,0,0,2,24,45] ✗ 实测 ${JSON.stringify(r.body && r.body.transform)}`
      + " ⇒ 合成顺序/操作数不对（不是「两边一致」就够 ✓：两边同错也绿）");
  }
  // **类型口径 ✗**：`type` = master 的 `data.type`，缺省 `shape` ✓（**不是顶层 `type` ✗**）。
  const masterData = (mS.body && mS.body.data) || {};
  const expectedType = masterData.type !== undefined ? masterData.type : "shape";
  for (const [who, r] of [["服务端", dS], ["本地", dL]]) {
    check(r.body && r.body.type === expectedType,
      `§A（${who}）副本 type 应为 ${JSON.stringify(expectedType)}（= master.data.type ?? "shape"）✗ 实测 ${JSON.stringify(r.body && r.body.type)}`);
  }
  check(dS.body && dL.body && same(dS.body.data, dL.body.data),
    "§A 副本 data 应与 master 的折叠 data 相同 ✗");

  // **实例已删 ✓、副本活着 ✓**（**默认 `list_objects` 不含已删 ✓**）。
  const ids1 = await liveIds("§A detach 之后");
  check(!ids1.s.includes("I1") && !ids1.l.includes("I1"), "§A 实例 I1 应已从状态里删掉 ✗");
  check(ids1.s.includes("D1") && ids1.l.includes("D1"), "§A 副本 D1 应活着 ✗");

  // ══ §B 变更集：一把撤掉两步 ══
  const sR = await callServer("revert_changeset", { changeset_id: sD.body && sD.body.changeset_id });
  const lR = await localCall("revert_changeset", { changeset_id: lD.body && lD.body.changeset_id });
  check(sR.status === 200 && lR.status === 200,
    `§B revert_changeset 两边都应 200 ✗ 服务端 ${sR.status}｜本地 ${lR.status}｜${String(JSON.stringify(lR.body)).slice(0, 200)}`);
  check(sR.body && sR.body.reverted === 2, `§B 服务端这次撤销的原子数应为 2 ✗ 实测 ${sR.body && sR.body.reverted}`);
  check(lR.body && lR.body.reverted === 2, `§B 本地这次撤销的原子数应为 2 ✗ 实测 ${lR.body && lR.body.reverted}`);
  const ids2 = await liveIds("§B 撤销变更集之后");
  check(ids2.s.includes("I1") && ids2.l.includes("I1"), "§B 撤销后实例 I1 应复活 ✗ ⇒ tombstone 没进变更集（只撤了一半）");
  check(!ids2.s.includes("D1") && !ids2.l.includes("D1"), "§B 撤销后副本 D1 应从状态消失 ✗ ⇒ create 没进变更集");
  const [iS, iL] = await compareObject("§B 复活的实例", "I1", ["object_id", "type", "layer_id", "transform", "data"]);
  check(iS.status === 200 && iL.status === 200, "§B 撤销后两边都应能读到 I1 ✗");
}

// ══ §C 报错口径（逐字比 ✓） ══
{
  // ① 不是实例
  const [s1, l1] = await both("detach_instance", { instance_id: "M1" });
  compareError("§C① 不是实例", s1, l1, 400, "invalid_argument");
  // ② 实例不存在
  const [s2, l2] = await both("detach_instance", { instance_id: "NOPE" });
  compareError("§C② 实例不存在", s2, l2, 404, "reference_not_found");
  // ③ master 已删 ⇒ 409（**∴ 自己墓碑 ＋ 图层级联死都算 ✓**）
  await mkMaster("M3");
  await mkInstance("I3", "M3");
  await both("delete_object", { object_id: "M3" });
  const [s3, l3] = await both("detach_instance", { instance_id: "I3" });
  compareError("§C③ master 已删", s3, l3, 409, "precondition_failed");
  // ④ master 的创建被 revert ⇒ master 不在状态里 ⇒ 404（**∴ 与「已删」是两件事 ✓**）
  await mkMaster("M2");
  await mkInstance("I2", "M2");
  const sM2 = await callServer("get_object", { object_id: "M2" });
  const lM2 = await localCall("get_object", { object_id: "M2" });
  const sCreate = sM2.body && sM2.body.created_by;
  const lCreate = lM2.body && lM2.body.created_by;
  check(typeof sCreate === "string" && typeof lCreate === "string",
    "§C④ 两边都应能用 created_by 找到 master 的创建原子 ✗");
  if (typeof sCreate === "string") await callServer("revert", { atom_id: sCreate });
  if (typeof lCreate === "string") await localCall("revert", { atom_id: lCreate });
  const [s4, l4] = await both("detach_instance", { instance_id: "I2" });
  compareError("§C④ master 创建被撤", s4, l4, 404, "reference_not_found");
  // ⑤ 缺 master_ref.object_id ⇒ 400（**∴ 用 `set_property` 把 master_ref 换成没有 object_id 的 ✓**）
  await mkMaster("M5");
  await mkInstance("I5", "M5");
  await both("set_property", { object_id: "I5", key: "master_ref", value: { local_transform: { matrix: [1, 0, 0, 1, 0, 0], pivot: [0, 0] } } });
  const [s5, l5] = await both("detach_instance", { instance_id: "I5" });
  compareError("§C⑤ 缺 master_ref.object_id", s5, l5, 400, "invalid_argument");
}

// ══ §D 已删实例的脱离 ⇒ 409，**但服务端会留下半成品副本** ✓ ══
//   **∴ 不是「已删照样能脱离」✗**：`state.objects` 里**有**墓碑 ✓ ⇒ 前置检查全过 ✓，
//   但**第二步 `Tombstone` 撞上提交期校验** ✓ ⇒ 409「对象 X 已被 tombstone，tombstone 被拒绝」✓。
//   **∴ 次序决定了副作用 ✗**：**先建副本 ✓、再被拒 ✗** ⇒ **副本留下 ✓**（**实测 `get_object D4` 仍 200 ✓**）。
//   **∴ 本地照抄这个半成品 ✓** —— 本地若「先判死再拒」✗ ⇒ 同一个请求**两边状态不同** ✗
//   （**∴ 那是另一种口径分裂 ✓，不是更安全 ✓**）。**这一个缺陷归属服务端 ✓**（已记台账 §659）。
{
  await mkMaster("M4");
  await mkInstance("I4", "M4");
  await both("delete_object", { object_id: "I4" });
  const [sD, lD] = await both("detach_instance", { instance_id: "I4", object_id: "D4" });
  compareError("§D 已删实例的脱离", sD, lD, 409, "precondition_failed");
  const [dS, dL] = await compareObject("§D 半成品副本 D4", "D4", ["object_id", "layer_id", "type", "data", "transform"]);
  check(dS.status === 200 && dL.status === 200,
    `§D 两边都应留下半成品副本 D4（服务端行为如此）✗ 服务端 ${dS.status}｜本地 ${dL.status}`);
  await liveIds("§D 之后");
}

// ══ §E 二期复核补的两条**输入触发**（四层齐第②层 ✗ 此前判据的输入造不出这两个情况 ✓）══
//   **∴ E1 折叠 data ✗**：服务端 `tools.rs:6325` 复制的是 `master.data.clone()` ✓ —— 状态里**已含**
//     `set_property` 写进 `data` 的键 ✓。此前判据**从没 set_property 过 master** ⇒
//     本地读裸净荷 `mv.data` 的分裂**测不到** ✓（断言恒绿＝安慰剂 ✓）。
//     **∴ 变异 ✗**：把本地副本 data 改回 `mv.data` ⇒ 本地缺 override ⇒ 红 ✓。
//   **∴ E2 撞 id ✗**：`log.rs:100` 对 `create_object` 是 `MustNotExist` ✓ ⇒ 服务端 409「对象已存在」✓；
//     此前本地**只有锁守卫** ⇒ **照写不误 ✓ 被折叠层丢弃（`fold.rs:551`）⇒ 回执 200、状态里没副本**✗
//     （**静默降级形态③ ✓**）。**∴ 变异 ✗**：去掉 `create_object` 的 p2RefGuard ⇒ 本地 200 而服务端 409 ⇒ 红 ✓。
{
  await mkMaster("M6");
  await mkInstance("I6", "M6");
  const MARK = { tint: "override-from-set-property" };
  await both("set_property", { object_id: "M6", key: "override", value: MARK });
  // **服务端自己折叠出什么就是什么 ✓** —— 判据拿服务端当权威，不做「两边相等」的安慰剂 ✓
  const sM6 = await callServer("get_object", { object_id: "M6" });
  const sMark = sM6.body && sM6.body.data && sM6.body.data.override;
  check(same(sMark, MARK),
    `§E1 权威前提：服务端 master 的 data 应含 override ✗ 实测 ${String(JSON.stringify(sMark)).slice(0, 160)} ⇒ 服务端口径变了要回来改这条`);
  const eS = await callServer("detach_instance", { instance_id: "I6", object_id: "E1S" });
  const eL = await localCall("detach_instance", { instance_id: "I6", object_id: "E1L" });
  check(eS.status === 200 && eL.status === 200,
    `§E1 脱离应成功 ✗ 服务端 ${eS.status}｜本地 ${eL.status}｜${String(JSON.stringify(eL.body)).slice(0, 200)}`);
  const eLS = await callServer("get_object", { object_id: "E1S" });
  const eLL = await localCall("get_object", { object_id: "E1L" });
  const copyMarkS = eLS.body && eLS.body.data && eLS.body.data.override;
  const copyMarkL = eLL.body && eLL.body.data && eLL.body.data.override;
  check(same(copyMarkS, MARK), `§E1 服务端副本 data 应带 override ✗ 实测 ${String(JSON.stringify(copyMarkS)).slice(0, 160)}`);
  check(same(copyMarkL, MARK),
    `§E1 本地副本 data 缺 override ⇒ 用了裸净荷而非折叠 data ✗ 实测 ${String(JSON.stringify(copyMarkL)).slice(0, 160)}`);

  // **E2 撞 id**：副本 id 撞一个**活着**的对象 ⇒ 两边 409、文案逐字、实例不许被删 ✓
  await mkMaster("M7");
  await mkInstance("I7", "M7");
  const [sC, lC] = await both("detach_instance", { instance_id: "I7", object_id: "M7" });
  compareError("§E2 副本撞 id", sC, lC, 409, "precondition_failed");
  // **∴ 不 whole-list 比对 ✗**：§E1 两边的副本 id 是**各自**的（E1S／E1L ⇒ 故意不同 ✓）——
  //   **∴ 只断「两边各自的 I7 都还活着 ✓＋两边都不许多出 M7 的副本 ✗」**（同源核对 ✓）
  const sIds = (await (await nativeFetch(`${BASE}/api/tools/list_objects?token=${TOKEN}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: SERVER_DOC, actor: "human:judge", session: "session:judge" }),
  })).json()).objects.map((o) => o.object_id);
  const lIds = (await localCall("list_objects", {})).body.objects.map((o) => o.object_id);
  check(sIds.includes("I7") && lIds.includes("I7"),
    `§E2 撞 id 被拒后实例 I7 两边都应还在（第一步都没成 ⇒ 不许 tombstone）✗ 服务端 ${sIds}｜本地 ${lIds}`);
}

if (bad.length) {
  console.log(`✗ PWA 本地 detach_instance 与服务端同口径：${bad.length} 条不符`);
  for (const m of bad) console.log("   · " + m);
  process.exit(1);
}
console.log("✓ PWA 本地 detach_instance 与服务端同口径（变换合成逐位相同／副本数据与类型／"
  + "一个变更集可整体撤销／五条报错逐字／已删实例的 409 与半成品副本同形）");
