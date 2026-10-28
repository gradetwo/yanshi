#!/usr/bin/env node
// **判据：PWA 本地的**写路径**（图层／对象／效果／级联）⇒ 与服务端**同一份口径 ✓ ★**（第 2 批 ✓／台账 §658）。
//
// **∴ 它测什么 ✗**（**内核 ①：断言必须能证明自己能红 ✓**）：
//   本地把服务端的**校验＋折叠**用 JS 重写了一遍 ✗ ⇒ **两份实现** ⇒
//   **唯一可信的说法是「同一输入同一输出」** ✓ ⇒ 这里**不读代码、只看行为**：
//   **同一批请求分别打到真服务端与本地 `api-local` ✗** ⇒ **比解析后的 JSON ✓**
//   （**∴ 不比字节 ✓：服务端 `serde` 写整数、本地 JS 写同一个数 ⇒ 浮点口径见 `same()` ✓**）。
//   **∴ 连报错文案也比 ✗**（`context.detail` 逐字 ✓）：**同一个拼错参数，PWA 与服务器必须给同一句话 ✓**
//   —— 否则调用方按服务器文案写的重试逻辑，在离线模式里永远命中不了 ✓（静默降级形态③）。
//
// **∴ 三处**已知分歧**也在这里断 ✓**（**∴ 不许当「两边应相等」✗ —— 那会逼本地复制缺陷 ✓**）：
//   ① **死父层建层**：服务端回 200 而图层**根本没进状态**（静默失败 ✗）；本地 404 当场拒绝 ✓
//      ⇒ 断「服务端 200 ＋ 两边 `list_layers` 都没有它」＋「本地 404 且说清父层」✓。
//   ② **changeset 半写**：服务端 `{visible:false, layer_id:"NOPE"}` 回 404 而 `visible` **已经落盘** ✗
//      （实测 `service.rs:2437 commit_changeset_timed` 逐条提交再 `attempt?` ✓）；本地要么全落要么全不落 ✓
//      ⇒ 断「服务端 false／本地 true」这个**分歧本身** ✓（**两条**：§C' 锁＋例外混写／§D② 坏引用 ✓）。
//   ③ **复制图层丢 `type`**：服务端 `create_object` 净荷**不带 `type`**（`tools.rs:3921` ✓）
//      ⇒ 副本对象被折成 `stroke` ✗（实测：调整对象复制后 `type:"stroke"` ✓）；本地保留原类型 ✓
//      ⇒ 断「服务端 `stroke`／本地 `adjustment`」＋其余字段（z／visible／locked／enabled）相等 ✓。
//   **∴ 这三条是**服务端缺陷**（补丁封面写给上游 ✓），不是本地的模仿对象 ✓。
//
// **∴ 前提 ✗**：需要**活着的服务端**（`run-criteria-local.sh` 起一个并把地址放进 argv[2] ✓）。
//   **拿不到 ⇒ `exit(3)`（判据前提不成立 ≠ 产品失败 ✓）**。
//
// **∴ 变异（四层齐第④层 ✓ 十次实测 ⇒ 全部退出码 1 ⇒ 恢复后 0 ✓，逐条见台账 §658）**：
//   ① `__p2Alive` 不看图层级联死 ⇒ §B 的 `list_objects` 条数／顺序红（服务端 2｜本地 3）✓；
//   ② `update_layer` 去掉 `p2RefGuard` ⇒ 「给已删层写属性」「层不存在」两条红（本地 200）✓；
//   ③ `create_layer` 去掉 `kind` 校验 ⇒ `kind=foo` 那条红（本地 200）✓；
//   ④ `p2cAddEffect` 的撞墓碑分支改成「已存在」⇒ 复活那一组红（本地没有 `revived`）✓；
//   ⑤ `p2DefaultZInLayer` 恒 0 ⇒ §B 的 z 序比对红（`list_objects` 第 1 条换人）✓；
//   ⑥ `update_object` 改成边校验边写 ⇒ §C' 本地 `visible` 也变 false ⇒ 红 ✓
//      （**∴ 它同时证明 `putAtom` 那道闸**在写路径上**真的生效** ✓）；
//   ⑦ `p1LayerOrder` 不再折叠 `set_property{z_index}` ⇒ §A 三条 `list_layers` 红 ✓
//      （**∴ 这条是**真缺口**：回执 ok 而面板不动 ⇒ 形态③ ✓ 已修 ✓**）；
//   ⑧ `list_layers` 的 `objects` 计数恒 0 ⇒ §A' 副本的 `objects` 红 ✓；
//   ⑨ `p2LockGuard` 恒放行 ⇒ §C 的锁闸三条红（403／200）✓ —— **锁闸本身也有变异 ✓**；
//   ⑩ 兜底 catch 去掉 `__p2Failure` 换装 ⇒ **本地抛异常** ⇒ §C 三条红（状态 0／`local_threw`）✓
//      （**∴ 这条第一次跑是**退出码 3**✗** ⇒ **∴ 暴露出判据自己的缺口 ✓**：`callLocal` 不接异常 ⇒
//        「产品抛错」被当成「判据坏了」✓ ⇒ **∴ 已改成**记成状态 0 的失败响应 ✓** ⇒ 现在退出码 1 ✓）。
process.on("uncaughtException", (error) => {
  console.error(String(error&&error.stack||error).slice(0,1200));
  process.exit(3);
});
import { pathToFileURL } from "node:url";

const BASE = process.argv[2];
if (!BASE || !/^https?:\/\//.test(BASE)) {
  console.log("⊘ 不可判：argv[2] 缺服务端地址 ⇒ 请用 bash scripts/run-criteria-local.sh scripts/tool-pwa-local-write-guards.mjs 跑"
    + "（判据前提不成立，不是产品问题）");
  process.exit(3);
}

const bad = [];
const check = (cond, msg) => { if (!cond) bad.push(msg); };
const UNIQ = "w" + Date.now().toString(36);
const SERVER_DOC = `wsrv-${UNIQ}`;
const LOCAL_DOC = `wloc-${UNIQ}`;

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
/** **本地调用 ✗ 一处收口 ✓**（`callLocal` 与 `openPair` 的 `L` 共用 ⇒ 形状只写一遍 ✓）。
 *   **★ 本地抛异常必须记成「产品失败」✗ 不是「判据自己坏了」✓ ★**（变异实测 ✓）：
 *   **∴ 原来**不接 ⇒ 异常穿到 `uncaughtException` ⇒ **`exit(3)`（不可判）** ✗
 *   ⇒ **∴ 而**「锁闸放行了／错误没换成 403」**是产品行为** ✓ ⇒ 必须走 `exit(1)` ✓
 *   （**∴ 那条变异「兜底 catch 去掉 `__p2Failure` 换装」就这样把自己的退出码暴露出来 ✓**）。 */
const localCall = async (doc, tool, args) => {
  try {
    const res = await localApi(new Request(`https://x/api/tools/${tool}?doc=${doc}`, {
      method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}),
    }));
    return { status: res.status, body: await res.json().catch(() => null) };
  } catch (e) {
    // **∴ 形状造得像一条本地错误响应 ✗**：状态 0 ＋ `local_threw` ⇒ 后面的逐字段比对会红 ✓。
    return { status: 0, body: { ok: false, error_code: "local_threw",
      context: { detail: String((e && e.message) || e).slice(0, 300) } } };
  }
};
const callLocal = (tool, args) => localCall(LOCAL_DOC, tool, args);
const both = async (tool, args) => [await callServer(tool, args), await callLocal(tool, args)];
const localCreated = await localApi(new Request(`https://x/api/documents?doc=${LOCAL_DOC}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: LOCAL_DOC, width: 400, height: 300 }),
})).then((r) => r.json().catch(() => null));
if (!localCreated || localCreated.ok !== true) {
  console.log(`⊘ 不可判：本地建文档失败（${JSON.stringify(localCreated)?.slice(0, 160)}）⇒ 前提不成立，不是产品问题`);
  process.exit(3);
}

// **② 比对器 ✓**（与 `tool-pwa-preset-server-parity.mjs` 同一口径：**断结构 ✗ 不断字节 ✓**）。
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
/** **服务端每个写工具都会多给的字段**（渲染／journal／HTTP 层 ✓）⇒ 一律进 `serverOnly` ✓。
 *   **∴ 这里放**全集**而不是逐条挑 ✗**：服务端哪天在某个工具上多给一个字段，
 *   归入这里**不会假红** ✓；而**`shared` 少给**会由下面的「键集 ⊆ 允许集」抓住 ✓。 */
const NOISE = ["atom_id", "atoms", "changeset_id", "created", "dirty_bbox", "dirty_kind", "dirty_tiles",
               "duplicate", "head", "job_id", "job_status", "opened", "preview", "seq",
               "snapshotted", "suggestions", "timings", "warnings", "region", "saved"];
function compare(label, srv, loc, shared, opts = {}) {
  const serverOnly = opts.serverOnly || [];
  const localOnly = opts.localOnly || [];
  const divergent = opts.divergent || [];
  if (!srv.body || !loc.body) {
    check(false, `${label}：有一边读不出 JSON ✗（服务端 ${JSON.stringify(srv.body)?.slice(0, 80)}｜本地 ${JSON.stringify(loc.body)?.slice(0, 80)}）`);
    return;
  }
  check(srv.status === loc.status, `${label}：HTTP 状态不同 ✗ 服务端 ${srv.status}｜本地 ${loc.status}`);
  const sKeys = Object.keys(srv.body);
  const lKeys = Object.keys(loc.body);
  for (const k of shared) {
    check(sKeys.includes(k), `${label}：服务端响应里没有字段「${k}」✗ ⇒ 权威口径变了，要改的是这条判据＋台账，不是偷偷放宽本地`);
    check(lKeys.includes(k), `${label}：本地响应里没有字段「${k}」✗（虚报少给）`);
    // **`JSON.stringify(undefined)` 是 `undefined` ✗**（不是字符串 ✓）⇒ 直接 `.slice` 会把判据自己搞崩 ✗
    //   ⇒ 统一过 `String(...)` ✓（这也是「判据自己坏了要能说出来」的一小步 ✓）。
    const show = (v) => String(JSON.stringify(v)).slice(0, 300);
    check(same(srv.body[k], loc.body[k]), `${label}：字段「${k}」两边不一致 ✗\n     服务端 ${show(srv.body[k])}\n     本地     ${show(loc.body[k])}`);
  }
  // **两边都有、但**值空间不同**（形如 id）的字段 ⇒ 只断「都在」✗ 比值 ✓**（`divergent` 注释见调用处）。
  for (const k of divergent) {
    check(sKeys.includes(k) && lKeys.includes(k),
      `${label}：约定「两边都有但值空间不同」的字段「${k}」有一边没给 ✗（服务端 ${sKeys.includes(k)}｜本地 ${lKeys.includes(k)}）`);
  }
  for (const k of sKeys) {
    check(shared.includes(k) || serverOnly.includes(k) || divergent.includes(k),
      `${label}：服务端有、本地没实现、且不在允许清单里的字段「${k}」✗ ⇒ 要么补进本地，要么写进 serverOnly 并说明理由`);
  }
  for (const k of lKeys) {
    check(shared.includes(k) || localOnly.includes(k) || divergent.includes(k),
      `${label}：本地有、服务端没有（或含义不同）且不在允许清单里的字段「${k}」✗ ⇒ 双份口径：要就删，要么承认并说明`);
  }
}
function compareError(label, srv, loc, code) {
  if (!srv.body || !loc.body) { check(false, `${label}：有一边读不出 JSON ✗`); return; }
  check(srv.status === loc.status, `${label}：错误状态码不同 ✗ 服务端 ${srv.status}｜本地 ${loc.status}`);
  check(srv.body.error_code === code, `${label}：服务端这次没按 ${code} 报（实测 ${srv.body.error_code}）✗ ⇒ 判据口径要跟着权威改`);
  check(loc.body.error_code === code, `${label}：本地错误码应为 ${code} ✗ 实测 ${loc.body.error_code}｜${JSON.stringify(loc.body).slice(0, 200)}`);
  const sDetail = srv.body.context && srv.body.context.detail;
  const lDetail = loc.body.context && loc.body.context.detail;
  check(typeof lDetail === "string" && lDetail.length > 0,
    `${label}：本地没给 context.detail ⇒ 调用方读不到原因 ✗（内核：看到 ok=false 先读 error ✓）`);
  check(sDetail === lDetail, `${label}：错误文案不同 ✗\n     服务端 ${sDetail}\n     本地     ${lDetail}`);
}
const compareErrBoth = async (label, tool, args, code) => {
  const [srv, loc] = await both(tool, args);
  compareError(label, srv, loc, code);
  return [srv, loc];
};
/** **`list_layers` 整条比 ✗**（13 个字段逐层相等 ⇒ `p1LayerOrder` 的折叠缺口无处可藏 ✓）。 */
async function compareLayers(label) {
  const [srv, loc] = await both("list_layers", {});
  compare(label, srv, loc, ["ok", "layers", "count"],
    { serverOnly: ["created", "opened", "timings"], localOnly: ["server", "derived_from"] });
  return { srv, loc };
}
/** **`list_objects` 比投影 ✗**：服务端 `summary_object`（`tools.rs:2823`）给 11 个字段，
 *   本地如实给 8 个 ✓ ⇒ `bbox`／`current_version`／`versions` **列入 serverOnly** ✓
 *   （**∴ 前者要渲染层、后两者要版本链 ✗ —— 本地折叠没有这两样 ⇒ 不给 ≠ 忘了给 ✓**）。
 *   **∴ 仍然断「服务端确实还在给那三个」✗**：哪天它少了 ⇒ 这条判据要跟着改 ✓（不是本地偷偷补）。 */
const OBJ_SHARED = ["object_id", "layer_id", "type", "visible", "enabled", "locked", "z_index", "medium"];
async function compareObjects(label, args = {}, includeHiddenCheck = false) {
  const [srv, loc] = await both("list_objects", args);
  // **`objects` 数组单独逐项比 ✓**（下面的 `OBJ_SHARED` 比法更严 ✓）⇒ 外层只断**键集**：
  //   **∴ 两边都有它 ⇒ 两个允许清单都要列 ✗**（否则「键集 ⊆ 允许集」会两头报 ✓）。
  compare(label + "（外层）", srv, loc, ["ok", "count"],
    { serverOnly: ["created", "opened", "timings", "objects"],
      localOnly: ["server", "derived_from", "objects"] });
  if (!srv.body || !loc.body || !Array.isArray(srv.body.objects) || !Array.isArray(loc.body.objects)) {
    check(false, `${label}：有一边没有 objects 数组 ✗`);
    return;
  }
  const sObs = srv.body.objects, lObs = loc.body.objects;
  check(sObs.length === lObs.length, `${label}：对象条数不同 ✗ 服务端 ${sObs.length}｜本地 ${lObs.length}`);
  const n = Math.min(sObs.length, lObs.length);
  for (let i = 0; i < n; i++) {
    const s = sObs[i], l = lObs[i];
    for (const k of OBJ_SHARED) {
      check(same(s[k], l[k]), `${label}：第 ${i} 个对象的「${k}」不同 ✗ 服务端 ${JSON.stringify(s[k])}｜本地 ${JSON.stringify(l[k])}`);
    }
  }
  for (const s of sObs) {
    for (const k of ["bbox", "current_version", "versions"]) {
      check(k in s, `${label}：服务端的对象少了「${k}」✗ ⇒ summary_object 变了，要改判据＋台账`);
    }
  }
  // **恒等式 ✓**：每个对象的 `layer_id` 必须在 `list_layers` 里出现（**两边各自成立 ✓**）。
  for (const [who, caller] of [["服务端", callServer], ["本地", callLocal]]) {
    const layers = await caller("list_layers", {});
    const ids = new Set(((layers.body && layers.body.layers) || []).map((x) => x.layer_id));
    for (const o of (who === "服务端" ? sObs : lObs)) {
      check(ids.has(o.layer_id), `${label}（${who}）：对象 ${o.object_id} 的图层 ${o.layer_id} 不在 list_layers 里 ✗ ⇒ 两条读接口互相矛盾`);
    }
  }
  if (includeHiddenCheck) {
    check(srv.body.count === sObs.length, `${label}：服务端 count 与数组长度不一致 ✗`);
  }
  return { srv, loc };
}

/** **另起一对文档 ✗**（**∴ 只给「会造随机 id」的用例用 ✓**）：
 *   **∴ 为什么必须隔开 ✗**：`duplicate_layer` 两边各自新生成副本 `layer_id` ✓（值空间不同 ⇒ 本该如此 ✓）
 *   ⇒ **∴ 而后续每一条「引用不存在」的报错都**列出候选名单** ✗**（`现有图层：…` ✓）
 *   ⇒ 随机 id 一进名单，**之后所有文案比对全部假红** ✓（**第 901／916 轮同一条教训的第二次 ✓**：
 *   **∴ 一个用例造的状态，别让它污染别的用例的观测 ✓**）。
 *   @returns `{S, L, both}` 三个**绑定到这一对文档**的调用器 ✓
 */
async function openPair(prefix) {
  const tag = `${prefix}-${UNIQ}`;
  const sDoc = `srv-${tag}`, lDoc = `loc-${tag}`;
  const s = await nativeFetch(`${BASE}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: sDoc, width: 400, height: 300 }),
  }).then((r) => r.json());
  if (!s || s.ok !== true || !s.token) {
    console.log(`⊘ 不可判：服务端建文档 ${sDoc} 失败（${JSON.stringify(s)?.slice(0, 160)}）⇒ 前提不成立，不是产品问题`);
    process.exit(3);
  }
  const l = await localApi(new Request(`https://x/api/documents?doc=${lDoc}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: lDoc, width: 400, height: 300 }),
  })).then((r) => r.json().catch(() => null));
  if (!l || l.ok !== true) {
    console.log(`⊘ 不可判：本地建文档 ${lDoc} 失败（${JSON.stringify(l)?.slice(0, 160)}）⇒ 前提不成立，不是产品问题`);
    process.exit(3);
  }
  const S = async (tool, args) => {
    const res = await nativeFetch(`${BASE}/api/tools/${tool}?token=${s.token}`, {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ doc_id: sDoc, actor: "human:judge", session: "session:judge", ...args }),
    });
    return { status: res.status, body: await res.json().catch(() => null) };
  };
  const L = (tool, args) => localCall(lDoc, tool, args);
  return { sDoc, lDoc, S, L, both: async (tool, args) => [await S(tool, args), await L(tool, args)] };
}

// **════ §A 图层：建／复用／改／删 ＋ 13 个字段的折叠 ════**
{
  const [srv, loc] = await both("create_layer", { layer_id: "L1" });
  // **`name` 只在本地 ✓**：服务端 `write_create_layer` 的回执**没有**它（实测 ✓，`tools.rs` 那个分支只回
  //   `ok／seq／layer_id／layer_type／reused／detail` ✓）⇒ **∴ 归 `localOnly`** ✓，但本地那条缺省名
  //   **仍然要断**（`图层 L1` ✓ —— 面板显示的就是它 ✓）。
  compare("§A 建 L1（缺省名照 id 末四位）", srv, loc, ["ok", "seq", "layer_id", "layer_type"],
    { serverOnly: NOISE, localOnly: ["server", "note", "name"] });
  check(loc.body && loc.body.name === "图层 L1", `§A 本地缺省名应为「图层 L1」✗ 实测 ${JSON.stringify(loc.body && loc.body.name)}`);
}
{
  const [srv, loc] = await both("create_layer", { layer_id: "G1", type: "layer_group" });
  compare("§A 建图层组 G1", srv, loc, ["ok", "seq", "layer_id", "layer_type"],
    { serverOnly: NOISE, localOnly: ["server", "note", "name"] });
  check(loc.body && loc.body.layer_type === "layer_group", `§A 本地 G1 的 layer_type 应为 layer_group ✗ 实测 ${JSON.stringify(loc.body && loc.body.layer_type)}`);
}
{
  const [srv, loc] = await both("create_layer", { layer_id: "L1", if_exists: "reuse" });
  compare("§A L1 再建 if_exists=reuse", srv, loc, ["ok", "reused", "layer_id", "name", "detail"],
    { serverOnly: NOISE, localOnly: ["server"] });
}
// **改名后复用：名字必须取「折叠后的视图」✗ 不是这次入参 ✓**（服务端 `state.layers[id].name` ✓）。
{
  const [srv, loc] = await both("update_layer", { layer_id: "L1", patch: { name: "主层" } });
  compare("§A 改 L1 名", srv, loc, ["ok", "count", "head"],
    { serverOnly: NOISE, localOnly: ["server", "layer_id", "updated", "note"], divergent: ["atoms", "changeset_id"] });
  const [srv2, loc2] = await both("create_layer", { layer_id: "L1", if_exists: "reuse" });
  compare("§A 改名后复用（名字取折叠视图）", srv2, loc2, ["ok", "reused", "layer_id", "name", "detail"],
    { serverOnly: NOISE, localOnly: ["server"] });
  check(loc2.body && loc2.body.name === "主层", `§A 复用应回折叠后的名字「主层」✗ 实测 ${JSON.stringify(loc2.body && loc2.body.name)}`);
}
await compareErrBoth("§A 重复建层", "create_layer", { layer_id: "L1" }, "precondition_failed");
await compareErrBoth("§A kind=foo", "create_layer", { layer_id: "L2", kind: "foo" }, "invalid_argument");
await compareErrBoth("§A kind/type 不一致", "create_layer", { layer_id: "L2", kind: "vector", type: "raster" }, "invalid_argument");
await compareErrBoth("§A type=nope", "create_layer", { layer_id: "L2", type: "nope" }, "invalid_argument");
{
  const [srv, loc] = await both("create_layer", { layer_id: "L2", type: "layer_group" });
  compare("§A 建第二个组 L2", srv, loc, ["ok", "seq", "layer_id", "layer_type"],
    { serverOnly: NOISE, localOnly: ["server", "note", "name"] });
}
// **`z_index`／`parent_id` 折进 `list_layers` ✗**（旧本地写进原子但推导不认 ⇒ 面板不动 ✓）。
{
  const [srv, loc] = await both("update_layer", { layer_id: "L1", patch: { visible: false, z_index: 2 } });
  compare("§A L1 隐藏＋z=2", srv, loc, ["ok", "count", "head"],
    { serverOnly: NOISE, localOnly: ["server", "layer_id", "updated", "note"], divergent: ["atoms", "changeset_id"] });
  await compareLayers("§A list_layers（z 排序：default -1／G1 1／L1 2／L2 3）");
}
{
  const [srv, loc] = await both("update_layer", { layer_id: "L1", patch: { blend_mode: "multiply", opacity: 0.5, locked: true } });
  compare("§A L1 混合模式／不透明度／锁定", srv, loc, ["ok", "count", "head"],
    { serverOnly: NOISE, localOnly: ["server", "layer_id", "updated", "note"], divergent: ["atoms", "changeset_id"] });
  await compareLayers("§A list_layers（属性折叠）");
}
{
  const [srv, loc] = await both("update_layer", { layer_id: "L1", patch: { parent_id: "G1" } });
  compare("§A L1 移进组 G1", srv, loc, ["ok", "count", "head"],
    { serverOnly: NOISE, localOnly: ["server", "layer_id", "updated", "note"], divergent: ["atoms", "changeset_id"] });
  const { loc: layersLoc } = await compareLayers("§A list_layers（parent_id 折叠）");
  const l1 = ((layersLoc.body && layersLoc.body.layers) || []).find((x) => x.layer_id === "L1");
  check(!!l1 && l1.parent_id === "G1", `§A 本地 list_layers 里 L1 的 parent_id 应为 G1 ✗ 实测 ${JSON.stringify(l1 && l1.parent_id)}`);
}
// **负 z**：服务端允许 `z_index=-5` ⇒ 排序必须把它挪到最底 ✓（结构量：与视口／机器无关 ✓）。
{
  await both("update_layer", { layer_id: "L1", patch: { z_index: -5 } });
  await compareLayers("§A list_layers（负 z 排到最底）");
}
await compareErrBoth("§A update_layer 拼错参数", "update_layer", { layer_id: "L1", name: "x" }, "invalid_argument");
await compareErrBoth("§A update_layer 空 patch", "update_layer", { layer_id: "L1", patch: {} }, "invalid_argument");
await compareErrBoth("§A update_layer 层不存在", "update_layer", { layer_id: "NOPE", patch: { visible: false } }, "reference_not_found");
await compareErrBoth("§A blend_mode 非法", "update_layer", { layer_id: "L1", patch: { blend_mode: "bogus" } }, "invalid_argument");
await compareErrBoth("§A 父=自己", "update_layer", { layer_id: "L1", patch: { parent_id: "L1" } }, "invalid_argument");
await compareErrBoth("§A 成环", "update_layer", { layer_id: "G1", patch: { parent_id: "L1" } }, "invalid_argument");
await compareErrBoth("§A 父不存在", "update_layer", { layer_id: "L1", patch: { parent_id: "NOPE" } }, "reference_not_found");
await compareErrBoth("§A 父不是组", "update_layer", { layer_id: "L1", patch: { parent_id: "layer_default" } }, "invalid_argument");
{
  const [srv, loc] = await both("update_layer", { layer_id: "L1", patch: { parent_id: null } });
  compare("§A L1 移出组", srv, loc, ["ok", "count", "head"],
    { serverOnly: NOISE, localOnly: ["server", "layer_id", "updated", "note"], divergent: ["atoms", "changeset_id"] });
  await compareLayers("§A list_layers（移出组 ⇒ parent_id 为 null）");
}
{
  const [srv, loc] = await both("delete_layer", { layer_id: "L1" });
  compare("§A 删 L1", srv, loc, ["ok", "seq", "head"],
    { serverOnly: NOISE, localOnly: ["server", "layer_id", "note"], divergent: ["atom_id"] });
  await compareLayers("§A list_layers（L1 消失）");
}
await compareErrBoth("§A 再删 L1（墓碑）", "delete_layer", { layer_id: "L1" }, "precondition_failed");
await compareErrBoth("§A 给已删层写属性", "update_layer", { layer_id: "L1", patch: { visible: false } }, "precondition_failed");
await compareErrBoth("§A 删从未有过的层", "delete_layer", { layer_id: "NOPE" }, "reference_not_found");
await compareErrBoth("§A 复制已删层", "duplicate_layer", { layer_id: "L1" }, "reference_not_found");
await compareErrBoth("§A 复制从未有过的层", "duplicate_layer", { layer_id: "NOPE" }, "reference_not_found");
// **`duplicate_layer` ✗**：副本 id 两边都是新生成的（值空间不同 ✓）⇒ 用**独立文档对** ✓
//   （否则随机 id 会进后续每一条「现有图层：…」的候选名单 ⇒ 假红 ✓）。
{
  const P = await openPair("dup");
  await P.both("create_layer", { layer_id: "DG" });
  await P.both("add_adjustment", { layer_id: "DG", object_id: "DA1", adjustment_type: "levels" });
  const [srv, loc] = await P.both("duplicate_layer", { layer_id: "DG" });
  // **`changeset_id` 只在服务端 ✓**（本地没有 changeset 机制 ⇒ 如实不给 ✓）。
  compare("§A' 复制层 DG（独立文档对）", srv, loc, ["ok", "from", "name", "objects", "z_index", "head"],
    { serverOnly: NOISE.concat(["skipped_objects", "skipped_reason"]), localOnly: ["server", "note", "seq"],
      divergent: ["layer_id"] });
  for (const [who, r] of [["服务端", srv], ["本地", loc]]) {
    const id = r.body && r.body.layer_id;
    check(typeof id === "string" && id.length > 0, `§A'（${who}）复制没有回 layer_id ✗ ⇒ 调用方还得再查一次`);
  }
  const sId = srv.body && srv.body.layer_id, lId = loc.body && loc.body.layer_id;
  const sLayers = ((await P.S("list_layers", {})).body || {}).layers || [];
  const lLayers = ((await P.L("list_layers", {})).body || {}).layers || [];
  check(sLayers.length === lLayers.length,
    `§A' 复制后层数不同 ✗ 服务端 ${sLayers.length}｜本地 ${lLayers.length}`);
  const sCopy = sLayers.find((x) => x.layer_id === sId) || {};
  const lCopy = lLayers.find((x) => x.layer_id === lId) || {};
  check(!!sCopy.layer_id && !!lCopy.layer_id, "§A' 副本没有进 list_layers ✗ ⇒ 回执给了 id 而状态里没有（形态③）");
  for (const k of ["name", "type", "kind", "z_index", "objects", "visible", "opacity",
                   "blend_mode", "parent_id", "locked", "alpha_lock", "clipping_mask", "mask_id"]) {
    check(same(sCopy[k], lCopy[k]),
      `§A' 副本的「${k}」不同 ✗ 服务端 ${JSON.stringify(sCopy[k])}｜本地 ${JSON.stringify(lCopy[k])}`);
  }
  // **恒等式 ✓**：副本层里的对象 ⇒ 两边各自 `layer_id` 指向**自己的**副本 id ✓（不是照抄别人的 id ✓）。
  //   **∴ 必须按副本层**过滤 ✗**：`list_objects` 返回**整篇文档** ✓ —— 原对象仍在源层 ✓
  //   （实测：对**全集**做 `every()` ⇒ 恒假 ✗ ⇒ 那是判据自己错 ✓，不是产品 ✓）。
  const sObjs = ((await P.S("list_objects", {})).body || {}).objects || [];
  const lObjs = ((await P.L("list_objects", {})).body || {}).objects || [];
  check(sObjs.length === lObjs.length, `§A' 复制后对象数不同 ✗ 服务端 ${sObjs.length}｜本地 ${lObjs.length}`);
  const sCopyObjs = sObjs.filter((o) => o.layer_id === sId);
  const lCopyObjs = lObjs.filter((o) => o.layer_id === lId);
  check(sCopyObjs.length === (srv.body && srv.body.objects) && lCopyObjs.length === (loc.body && loc.body.objects),
    `§A' 副本层里的对象数与回执 objects 不一致 ✗ 服务端 ${sCopyObjs.length}/${srv.body && srv.body.objects}｜本地 ${lCopyObjs.length}/${loc.body && loc.body.objects}`);
  check(sCopyObjs.length === 1 && lCopyObjs.length === 1,
    `§A' 副本层里应恰好 1 个对象 ✗ 服务端 ${sCopyObjs.length}｜本地 ${lCopyObjs.length}`);
  // **上游缺陷 ✓：服务端 `create_object` 净荷**不带 `type` ✗**（`tools.rs:3921` ✓）
  //   ⇒ **∴ 副本被折成 `stroke` ✓**（实测 ✓）⇒ **∴ 本地保留原类型 ✓ 不复刻 ✓**（台账 §658 记这条分歧）。
  check(sCopyObjs[0] && sCopyObjs[0].type === "stroke",
    `§A' 服务端的副本对象类型不再是 stroke（实测 ${JSON.stringify(sCopyObjs[0] && sCopyObjs[0].type)}）✗ ⇒ 上游修好「复制丢 type」后这条要跟着改（不是改本地）`);
  check(lCopyObjs[0] && lCopyObjs[0].type === "adjustment",
    `§A' 本地副本必须保留原类型 adjustment ✗ 实测 ${JSON.stringify(lCopyObjs[0] && lCopyObjs[0].type)}`);
  for (const k of ["z_index", "visible", "locked", "enabled"]) {
    check(same(sCopyObjs[0] && sCopyObjs[0][k], lCopyObjs[0] && lCopyObjs[0][k]),
      `§A' 副本对象的「${k}」不同 ✗ 服务端 ${JSON.stringify(sCopyObjs[0] && sCopyObjs[0][k])}｜本地 ${JSON.stringify(lCopyObjs[0] && lCopyObjs[0][k])}`);
  }
}

// **════ §B 效果：默认 z／撞 id／层闸／更新／删除／复活 ════**
{
  const [srv, loc] = await both("create_layer", { layer_id: "L3" });
  check(!!srv.body && srv.body.ok === true && !!loc.body && loc.body.ok === true, "§B 建 L3 失败 ✗");
}
{
  const [srv, loc] = await both("add_adjustment", { layer_id: "L3", object_id: "A1", adjustment_type: "levels", params: { black: 0.1 } });
  compare("§B 建调整 A1", srv, loc, ["ok", "seq"],
    { serverOnly: NOISE, localOnly: ["server", "object_id", "adjustment_type", "note"] });
}
{
  const [srv, loc] = await both("add_filter", { layer_id: "L3", object_id: "F1", filter_name: "clarity", params: { amount: 0.2 } });
  compare("§B 建滤镜 F1", srv, loc, ["ok", "seq"],
    { serverOnly: NOISE, localOnly: ["server", "object_id", "filter_name", "note"] });
}
{
  const [srv, loc] = await both("add_adjustment", { layer_id: "L3", object_id: "A2", adjustment_type: "exposure" });
  compare("§B 建调整 A2（缺省 z）", srv, loc, ["ok", "seq"],
    { serverOnly: NOISE, localOnly: ["server", "object_id", "adjustment_type", "note"] });
}
await compareObjects("§B 三个效果的 z＝0/1/2");
await compareErrBoth("§B 同 id 再建", "add_adjustment", { layer_id: "L3", object_id: "A1", adjustment_type: "levels" }, "precondition_failed");
await compareErrBoth("§B 建到不存在的层", "add_adjustment", { layer_id: "NOPE", object_id: "A9", adjustment_type: "levels" }, "reference_not_found");
{
  await both("create_layer", { layer_id: "Ldead" });
  await both("delete_layer", { layer_id: "Ldead" });
  await compareErrBoth("§B 建到已删的层", "add_adjustment", { layer_id: "Ldead", object_id: "A9", adjustment_type: "levels" }, "precondition_failed");
}
{
  const [srv, loc] = await both("update_adjustment", { object_id: "A2", params: { ev: 0.3 } });
  compare("§B 更新 A2", srv, loc, ["ok", "seq"],
    { serverOnly: NOISE, localOnly: ["server", "object_id", "note"] });
}
{
  const [srv, loc] = await both("update_filter", { object_id: "F1", params: { amount: 0.4 } });
  compare("§B 更新 F1", srv, loc, ["ok", "seq"],
    { serverOnly: NOISE, localOnly: ["server", "object_id", "note"] });
}
await compareErrBoth("§B 用 update_adjustment 改滤镜", "update_adjustment", { object_id: "F1", params: { amount: 0.1 } }, "invalid_argument");
await compareErrBoth("§B 更新不存在的对象", "update_adjustment", { object_id: "NOPE", params: { ev: 0.1 } }, "reference_not_found");
await compareErrBoth("§B 删不存在的对象", "delete_object", { object_id: "NOPE" }, "reference_not_found");
{
  const [srv, loc] = await both("delete_object", { object_id: "A2" });
  compare("§B 删 A2", srv, loc, ["ok", "seq"],
    { serverOnly: NOISE, localOnly: ["server", "object_id", "note"] });
}
await compareErrBoth("§B 再删 A2", "delete_object", { object_id: "A2" }, "precondition_failed");
await compareErrBoth("§B 更新已删对象", "update_adjustment", { object_id: "A2", params: { ev: 0.5 } }, "precondition_failed");
{
  const [srv, loc] = await both("add_adjustment", { layer_id: "L3", object_id: "A2", adjustment_type: "exposure" });
  compare("§B 复活 A2（点名 revived）", srv, loc, ["ok", "seq", "revived"],
    { serverOnly: NOISE, localOnly: ["server", "object_id", "adjustment_type", "note"] });
  check(loc.body && loc.body.revived === true, "§B 本地复活没有点名 revived:true ✗ ⇒ 调用方分不出「新建」与「复活」");
}
// **复活的两道 400 闸 ✓**（类型不符／改名 ✓）：要先造出一块**墓碑**✓。
{
  await both("delete_object", { object_id: "F1" });
  await compareErrBoth("§B 复活改型", "add_adjustment", { layer_id: "L3", object_id: "F1", adjustment_type: "levels" }, "invalid_argument");
  await compareErrBoth("§B 复活改名", "add_filter", { layer_id: "L3", object_id: "F1", filter_name: "sharpen" }, "invalid_argument");
}
await compareObjects("§B list_objects（A2 回来了／F1 死着）");

// **════ §C update_object ＋ 图层级联 ════**
await compareErrBoth("§C 更新不存在的对象", "update_object", { object_id: "NOPE", patch: { visible: true } }, "reference_not_found");
await compareErrBoth("§C 空 patch", "update_object", { object_id: "A1", patch: {} }, "invalid_argument");
await compareErrBoth("§C patch 指向不存在的层", "update_object", { object_id: "A1", patch: { layer_id: "NOPE" } }, "reference_not_found");
{
  const [srv, loc] = await both("update_object", { object_id: "A1", patch: { enabled: false } });
  // **`atoms`／`changeset_id` 只在服务端 ✓**（本地没有 changeset ⇒ 只回 `seqs` ✓）⇒ 都在 `NOISE` 里 ✓。
  compare("§C A1 enabled=false", srv, loc, ["ok", "count", "head"],
    { serverOnly: NOISE, localOnly: ["server", "seqs", "keys", "object_id", "note"] });
  await compareObjects("§C list_objects（enabled 折叠）");
}
{
  await both("update_object", { object_id: "A1", patch: { visible: false, locked: true } });
  const { srv, loc } = await compareObjects("§C list_objects include_hidden（visible/locked 折叠）", { include_hidden: true }, true);
  const pick = (r) => ((r.body && r.body.objects) || []).find((o) => o.object_id === "A1") || {};
  check(pick(srv).visible === false && pick(loc).visible === false, "§C 隐藏对象在 include_hidden:true 里应可见地标着 visible:false ✗");
  check(pick(srv).locked === true && pick(loc).locked === true, "§C locked:true 没有折进 list_objects ✗");
}
// **★ 锁必须被强制 ✗ ★**（第 2 批新闸 ✓：服务端 `log.rs:786 validate_lock` ⇒ **本地此前一条都没有** ✓
//   ⇒ 锁住的层照样能画、能删 ✓ —— 本判据 §C 的「A1 移到 L2」（服务端 403／本地 200）把它抓了出来 ✓）。
await compareErrBoth("§C 锁住的 A1 改 z_index（对象锁）", "update_object",
  { object_id: "A1", patch: { z_index: 5 } }, "permission_denied");
await compareErrBoth("§C 锁住的 A1 删除（对象锁）", "delete_object", { object_id: "A1" }, "permission_denied");
await compareErrBoth("§C 锁住的 A1 更新调整（对象锁）", "update_adjustment",
  { object_id: "A1", params: { black: 0.2 } }, "permission_denied");
{
  // **例外：`locked`／`visible` 永远放行 ✓**（否则**解锁永远做不到** ✓ —— 服务端注释点名的唯一出口 ✓）。
  const [srv, loc] = await both("update_object", { object_id: "A1", patch: { visible: true } });
  compare("§C 锁住的 A1 改 visible（例外放行）", srv, loc, ["ok", "count", "head"],
    { serverOnly: NOISE, localOnly: ["server", "seqs", "keys", "object_id", "note"] });
  check(loc.body && loc.body.ok === true, "§C 本地必须放行 locked/visible 例外 ✗（否则锁了就永远解不开）");
}
// **混合 patch ✗**：`visible`（例外、可写）＋ `z_index`（被锁拒）⇒ 服务端**半写** ✓（逐条提交 ✓）；
//   本地**整批拒绝** ✓ ⇒ 与 §D② 同一条分歧（`service.rs:2437` 非原子 changeset ✓）⇒ **断分歧本身** ✓。
//   **★ 必须用**独立文档对 ✓ ★**（第 916 轮教训 ✓）：服务端这里会**真写进 1 条原子** ✓
//   ⇒ **∴ 若在**主文档对**里做 ⇒ 之后每条 `head`／`seq` 都差 1 ✗**（实测：接着红了 5 条 ✓）。
{
  const P = await openPair("mix");
  await P.both("create_layer", { layer_id: "LM" });
  await P.both("add_adjustment", { layer_id: "LM", object_id: "M1", adjustment_type: "levels" });
  await P.both("update_object", { object_id: "M1", patch: { locked: true } });
  const [srv, loc] = await P.both("update_object", { object_id: "M1", patch: { visible: false, z_index: 7 } });
  compareError("§C' 混合 patch（锁 + 例外，文案仍逐字）", srv, loc, "permission_denied");
  const vis = async (caller) => {
    const r = await caller("list_objects", { include_hidden: true });
    const o = ((r.body && r.body.objects) || []).find((x) => x.object_id === "M1") || {};
    return o.visible;
  };
  const sVis = await vis(P.S), lVis = await vis(P.L);
  check(sVis === false,
    `§C' 服务端「403 却已落 visible=false」的行为变了（实测 ${JSON.stringify(sVis)}）✗ ⇒ 上游修好非原子提交后这条要跟着改（不是改本地）`);
  check(lVis === true,
    `§C' 本地必须整批拒绝（visible 仍为 true）✗ 实测 ${JSON.stringify(lVis)} —— 不许复刻「半写＋报错」✓`);
}
{
  // 解锁 ⇒ 再移动 ✓（**∴ 例外是解锁的唯一出路 ⇒ 这条同时证明它真的解开了 ✓**）。
  const [srv, loc] = await both("update_object", { object_id: "A1", patch: { locked: false, visible: true } });
  compare("§C 解锁 A1", srv, loc, ["ok", "count", "head"],
    { serverOnly: NOISE, localOnly: ["server", "seqs", "keys", "object_id", "note"] });
}
{
  const [srv, loc] = await both("update_object", { object_id: "A1", patch: { layer_id: "L2" } });
  compare("§C A1 移到 L2", srv, loc, ["ok", "count", "head"],
    { serverOnly: NOISE, localOnly: ["server", "seqs", "keys", "object_id", "note"] });
  await compareObjects("§C list_objects（layer_id 折叠，排序按 (layer, z, id)）", { include_hidden: true });
}
// **图层锁 ✓**：锁的是**内容** ✗ 不是这个图层对象的管理属性 ✓（服务端注释逐字 ✓）。
{
  await both("create_layer", { layer_id: "Lk" });
  await both("add_adjustment", { layer_id: "Lk", object_id: "K1", adjustment_type: "levels" });
  await both("update_layer", { layer_id: "Lk", patch: { locked: true } });
  await compareErrBoth("§C 建对象到锁定层", "add_adjustment",
    { layer_id: "Lk", object_id: "K2", adjustment_type: "levels" }, "permission_denied");
  await compareErrBoth("§C 锁定层里的对象改 z（图层锁）", "update_object",
    { object_id: "K1", patch: { z_index: 1 } }, "permission_denied");
  await compareErrBoth("§C 锁定层里的对象删除（图层锁）", "delete_object", { object_id: "K1" }, "permission_denied");
  {
    const [srv, loc] = await both("update_layer", { layer_id: "Lk", patch: { visible: false } });
    compare("§C 锁定层仍可改自身属性", srv, loc, ["ok", "count", "head"],
      { serverOnly: NOISE, localOnly: ["server", "layer_id", "updated", "note"], divergent: ["atoms", "changeset_id"] });
  }
  {
    const [srv, loc] = await both("delete_layer", { layer_id: "Lk" });
    compare("§C 锁定层仍可删除（锁的是内容，不是层管理）", srv, loc, ["ok", "seq", "head"],
      { serverOnly: NOISE, localOnly: ["server", "layer_id", "note"], divergent: ["atom_id"] });
  }
}
// **级联墓碑**：删层 ⇒ 层里对象一起从 `list_objects` 消失 ✓（两边各自成立 ✓）。
{
  await both("create_layer", { layer_id: "Lc" });
  await both("add_adjustment", { layer_id: "Lc", object_id: "C1", adjustment_type: "levels" });
  await both("delete_layer", { layer_id: "Lc" });
  await compareObjects("§C 级联删后 list_objects（C1 消失）", { include_hidden: true });
  const { loc } = await compareObjects("§C 级联后再验一次", { include_hidden: true });
  check(!((loc.body && loc.body.objects) || []).some((o) => o.object_id === "C1"),
    "§C 图层删了而对象还在本地 list_objects 里 ✗ ⇒ `__p2Alive` 没看图层级联死（形态③：删了还在）");
  await compareErrBoth("§C 给已删层的对象重排", "reorder_objects", { layer_id: "Lc", order: ["C1"] }, "reference_not_found");
}

// **════ §D 两个**故意分歧**（服务端缺陷 ⇒ 本地不照抄 ✗ ⇒ 断分歧本身 ✓）════**
// ① **死父层**：服务端 200 而图层不在状态里（静默失败 ✗）／本地 404 当场拒绝 ✓。
{
  await both("create_layer", { layer_id: "Lp" });
  await both("delete_layer", { layer_id: "Lp" });
  const [srv, loc] = await both("create_layer", { layer_id: "L6", name: "孤", parent_id: "Lp" });
  check(srv.status === 200 && srv.body && srv.body.ok === true,
    `§D① 服务端对「死父层建层」的状态变了（实测 ${srv.status} / ${srv.body && srv.body.ok}）✗ ⇒ 这个已知缺陷若被上游修好，这条断言要跟着改（不是改本地）`);
  check(loc.status === 404 && loc.body && loc.body.error_code === "reference_not_found",
    `§D① 本地应对死父层**响亮拒绝**（404 reference_not_found）✗ 实测 ${loc.status} / ${loc.body && loc.body.error_code}`);
  check(!!(loc.body && loc.body.context && /父图层 Lp/.test(loc.body.context.detail)),
    "§D① 本地的拒绝必须点名那个死的父层 ✗（否则调用方不知道是哪个 id 写错了）");
  for (const [who, caller] of [["服务端", callServer], ["本地", callLocal]]) {
    const r = await caller("list_layers", {});
    const has = ((r.body && r.body.layers) || []).some((x) => x.layer_id === "L6");
    check(!has, `§D①（${who}）L6 不该出现在 list_layers 里 ✗ ⇒ 「200 但层不存在」已变成「200 且层存在」`);
  }
}
// ② **changeset 半写**：`{visible:false, layer_id:"NOPE"}` ⇒ 两边都 404，但**服务端已经改了 visible** ✗。
{
  await both("create_layer", { layer_id: "Lq" });
  await both("add_adjustment", { layer_id: "Lq", object_id: "P1", adjustment_type: "levels" });
  const [srv, loc] = await both("update_object", { object_id: "P1", patch: { visible: false, layer_id: "NOPE" } });
  compareError("§D② 双键其一层无效（文案仍要逐字）", srv, loc, "reference_not_found");
  const sList = await callServer("list_objects", { include_hidden: true });
  const lList = await callLocal("list_objects", { include_hidden: true });
  const get1 = (r) => (((r.body && r.body.objects) || []).find((o) => o.object_id === "P1") || {});
  const sVis = get1(sList).visible, lVis = get1(lList).visible;
  check(sVis === false,
    `§D② 服务端「报 404 却已落 visible=false」的行为变了（实测 visible=${JSON.stringify(sVis)}）✗ ⇒ 上游若把它改成原子提交，这条断言要跟着改`);
  check(lVis === true,
    `§D② 本地必须**整批拒绝**（visible 仍为 true）✗ 实测 ${JSON.stringify(lVis)} —— 本地不许复刻「半写＋报错」✓`);
}

if (bad.length) {
  console.log(`✗ PWA 本地写路径与服务端同口径：${bad.length} 条不符`);
  for (const m of bad) console.log("   · " + m);
  process.exit(1);
}
console.log("✓ PWA 本地写路径与服务端逐字段同口径（建/复用/改/删图层 13 字段折叠／效果与复活／update_object／级联／"
  + "三处已知分歧断在分歧本身）");
