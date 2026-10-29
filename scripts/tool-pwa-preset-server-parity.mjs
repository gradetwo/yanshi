#!/usr/bin/env node
// **判据：PWA 本地的风格预设四端点 ⇒ 必须与服务端**同一份口径 ✓ ★**（第 2 批 ✓／台账「第 六百五十七 节」）。
//
// **∴ 它测什么 ✗**（**内核 ①：断言必须能证明自己能红 ✓**）：
//   本地把 `presets.rs` 的展开逻辑**重写了一遍 JS**✗ ⇒ **两份实现** ⇒ **唯一可信的说法是「同一输入同一输出」**✓
//   ⇒ 所以这里**不读代码、只看行为**：**同一批请求分别打到真服务端与本地 `api-local` ✗**
//   ⇒ **比对解析后的 JSON ✓**（**∴ 不比字节 ✓：serde 写 `2.0`／JS 写 `2` ⇒ 字节必不同 ✓**
//     **∴ 断的是结构量 ✓**）。
//   **∴ 连报错文案也比 ✓**（`context.detail` 逐字 ✓）——**参数写错时两边必须给同一句话**✓，
//   否则调用方在 PWA 里看到的错误码／建议会和服务器上不同（**静默降级形态③ ✓**）。
//
// **∴ 前提 ✗**：需要**活着的服务端**（`run-criteria-local.sh` 会起一个并把地址放进 argv[2] ✓）。
//   **拿不到 ⇒ `exit(3)`（判据前提不成立 ≠ 产品失败 ✓）**。
//
// **∴ 变异（四层齐第④层 ✓ 下面每一条都实测过「改坏 ⇒ 退出码 1」✓）**：
//   ① `p2ExpandPreset` 的中性值插值把 `strength` 换成 `1.0` ⇒ preview @0／@0.5 那一批红 ✓；
//   ② 删掉 `list_presets` 分支的 `!__csClientMode()` 闸 ⇒ ⑩「没走代理」红 ✓；
//   ③ 删掉 `preview_preset` 分支的同一道闸 ⇒ ⑩ 新增的那条红 ✓
//      （**⇒ 正是它**逼出了这条断言**：第一版只测 `list_presets`，②③里的③删掉了判据照样全绿 ✗**）；
//   ④ 关掉 `p2PresetSchemaCheck` 的「未知名」检查 ⇒ 拼错参数那几条红 ✓；
//   ⑤ `apply_preset` 的确定性 `object_id` 改成随机 ⇒「第二次应全部走更新」红 ✓；
//   ⑥ 把 `p2PresetLand` 的失败分支还原成「直接交回已读过的 Response」⇒ ⑦「读不出 JSON」红 ✓
//      （**∴ 这条是**真 bug**：本地 409 的 body 被自己吃掉 ⇒ 调用方看不到原因 ✓ 已在本地修掉 ✓**）。
//   ⑦ 把带 `render` 的本地分支改回「无条件 501 ＋ 理由写『本地渲染不出来』」⇒ ⑨ 那条理由正则红 ✓
//      （**∴ 而 ⑨ 原来断的就是这个 ✗ 所以它是**把错的口径钉住**的那三条断言 ✗ 已跟权威改 ✓**）。
process.on("uncaughtException", (error) => {
  console.error(String(error&&error.stack||error).slice(0,1200));
  process.exit(3);
});
import { pathToFileURL } from "node:url";

const BASE = process.argv[2];
if (!BASE || !/^https?:\/\//.test(BASE)) {
  console.log("⊘ 不可判：argv[2] 缺服务端地址 ⇒ 请用 bash scripts/run-criteria-local.sh scripts/tool-pwa-preset-server-parity.mjs 跑"
    + "（判据前提不成立，不是产品问题）");
  process.exit(3);
}

const bad = [];
const check = (cond, msg) => { if (!cond) bad.push(msg); };
const UNIQ = "p" + Date.now().toString(36);
const SERVER_DOC = `psrv-${UNIQ}`;
const LOCAL_DOC = `ploc-${UNIQ}`;

const stub = (name, value) =>
  Object.defineProperty(globalThis, name, { value, writable: true, configurable: true });
const localApi = await import(pathToFileURL(process.cwd() + "/scripts/lib/pwa-node-stub.mjs").href)
  .then((m) => m.bootPwaLocalApi({ origin: BASE, localOnly: true }));
const nativeFetch = globalThis.fetch;

// **① 服务端文档 ✗**：先建文档拿 token（**写工具也要 token ⇒ 与其它 `tool-*` 判据同一姿势 ✓**）。
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
const callLocal = async (tool, args) => {
  const res = await localApi(new Request(`https://x/api/tools/${tool}?doc=${LOCAL_DOC}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}),
  }));
  return { status: res.status, body: await res.json().catch(() => null) };
};
const localCreated = await localApi(new Request(`https://x/api/documents?doc=${LOCAL_DOC}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: LOCAL_DOC, width: 400, height: 300 }),
})).then((r) => r.json().catch(() => null));
if (!localCreated || localCreated.ok !== true) {
  console.log(`⊘ 不可判：本地建文档失败（${JSON.stringify(localCreated)?.slice(0, 160)}）⇒ 前提不成立，不是产品问题`);
  process.exit(3);
}

// **② 比对器 ✓**（**∴ 断「解析后的结构」✗ 不断字节 ✓**）。
const sortKeys = (v) => {
  if (Array.isArray(v)) return v.map(sortKeys);
  if (v && typeof v === "object") {
    const out = {};
    // **键序不算差异 ✓**（服务端 Map 按排序输出、本地按声明序输出 ⇒ 只有**值**要相等 ✓）。
    for (const k of Object.keys(v).sort()) out[k] = sortKeys(v[k]);
    return out;
  }
  return v;
};
const deep = (a, b) => same(a, b);
/** **数值按相对误差比 ✗**（**∴ 为什么不比字节／不严格相等 ✓**）：
 *   · **`serde_json` 解析浮点**用累乘 ⇒ 对长尾小数可能差 **1 ULP**（实测：源文件写 `0.010249999999999999`，
 *     服务端 `to_value` 后是**相邻的另一个 double** ⇒ 打印成 `0.01025` ✓），而 JS 的 `JSON.parse` 是正确舍入 ✓；
 *   · 展开里的插值两边**运算顺序不同** ⇒ 末位可能再差一两位。
 *   **∴ 所以**：整数仍严格相等（差 1 就是 bug ✓），小数按 `1e-12` 相对误差 ✓ ——
 *   **这断的是「同一口径」✗ 不是「同一浮点实现」✓**（像素与参数语义上等价的判据 ✓）。 */
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
/** **成功响应比对 ✗**：`shared`＝两边都该有**且要相等**的字段｜`serverOnly`／`localOnly`＝**允许**只在一边有的字段。
 *   **∴ 为什么要断「键集 ⊆ 允许集」✗**：只比 shared 的话 ⇒ **服务端哪天新增一个字段 ⇒ 本地静默少给 ✓**
 *   ⇒ 而那正是「看起来对」✗。多断这一条 ⇒ **新字段会让判据红 ✗ 逼两边对齐 ✓**。 */
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
    check(deep(srv.body[k], loc.body[k]), `${label}：字段「${k}」两边不一致 ✗\n     服务端 ${JSON.stringify(srv.body[k]).slice(0, 260)}\n     本地     ${JSON.stringify(loc.body[k]).slice(0, 260)}`);
  }
  // **`divergent`＝两边都有、但**含义**不同（已知且写进台账的差异 ✓）**：只断「都在」✗ 比值 ✗。
  //   **∴ 现在只有一项 ✗**：`created` ——服务端的 HTTP 层给**每个**工具响应无条件覆写 `created`／`opened`
  //   （`server.rs` 的 `pre_create`）⇒ `apply_preset` 自己那个「建了几笔」的数字**被换成布尔**✓
  //   ⇒ 本地没有那一层 ⇒ 给的是真数字 ✓。**∴ 所以调用方要读 `*_object_ids.length` ✗ 不许读 `created` ✓。**
  for (const k of divergent) {
    check(sKeys.includes(k) && lKeys.includes(k),
      `${label}：约定「两边都有但含义不同」的字段「${k}」有一边没给 ✗（服务端 ${sKeys.includes(k)}｜本地 ${lKeys.includes(k)}）`);
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
/** **错误响应比对 ✗**：**状态码 ＋ `error_code` ＋ `context.detail` 逐字 ✓**（**文案就是契约 ✓**）。 */
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

// **③ 输入矩阵 ✓**（**∴ 预设 id／旋钮 id／取值全部从服务端响应里取 ✗ 不写死数字 ✓**（内核 ②：别写快照 ✓））。
const libSrv = await callServer("list_presets", {});
const libLoc = await callLocal("list_presets", {});
compare("list_presets（全量）", libSrv, libLoc,
  ["ok", "presets", "count", "library_version", "library_source"],
  { serverOnly: ["created", "opened", "timings"], localOnly: ["server"] });
check(libLoc.body && libLoc.body.count > 0, "本地 list_presets 返回 0 套 ✗（内联库没进包？）");

const presetsSrv = (libSrv.body && libSrv.body.presets) || [];
const presetIds = presetsSrv.map((p) => p.preset_id);
const withKnobs = presetsSrv.filter((p) => (p.knobs || []).length > 0);
/** **数组与单项分两个名字 ✓**：此前同一个 `withKnobs` 既当数组（`[0]`）又当对象（`.knobs`）
 *   ⇒ 后者恒 `undefined` ⇒ 要么静默跳过、要么 `TypeError`（第一版就死在 `bwId` 未定义上 ✓）。 */
const knobPreset = withKnobs[0] || null;
const first = presetsSrv[0] || {};

// **③b `get_preset` 正向比对 ＋ 从**权威响应**里找「用 `black_and_white` 的那一套」✓**
//   **∴ 不写死预设 id ✓**（内核 ②：库会变 ⇒ 判据自己找 ✓）。
//   **∴ 找不到就**明说跳过**✗**（**「默认通过」是最坏的第四种诚实 ✗ ⇒ 跳过要说出来 ✓**）：
//     实测 22 套里**零套**含 `black_and_white` 操作（`black_white` 那套走的是 `saturation:0` ✓）
//     ⇒ 本地／服务端的那条 `amount` 中性规则**今天没有输入能触发** ⇒ 不虚报覆盖 ✓。
let bwId = null;
for (const id of presetIds) {
  const srv = await callServer("get_preset", { preset_id: id });
  const loc = await callLocal("get_preset", { preset_id: id });
  compare(`get_preset ${id}`, srv, loc, ["ok", "preset", "expanded_default"],
    { serverOnly: ["created", "opened", "timings"], localOnly: ["server"] });
  const ops = (srv.body && srv.body.expanded_default && srv.body.expanded_default.ops) || [];
  if (!bwId && ops.some((o) => o && o.type === "black_and_white")) bwId = id;
}
if (!bwId) {
  console.log("⊘ 跳过「黑白 amount 恒按中性 0 插值」：库内没有任何预设的展开含 black_and_white 操作");
}

const listCases = [
  ["list_presets tag 筛选", { tag: (first.tags || [])[0] }],
  ["list_presets subject 子串", { subject: ((first.suitable || [])[0] || "x").slice(0, 2) }],
  ["list_presets status 筛选", { status: first.status }],
  ["list_presets 落空", { status: "不存在的状态值" }],
];
for (const [label, args] of listCases) {
  compare(label, await callServer("list_presets", args), await callLocal("list_presets", args),
    ["ok", "presets", "count", "library_version", "library_source"],
    { serverOnly: ["created", "opened", "timings"], localOnly: ["server"] });
}

compareError("list_presets 拼错参数", await callServer("list_presets", { tet: 1 }),
  await callLocal("list_presets", { tet: 1 }), "invalid_argument");
compareError("get_preset 缺必填", await callServer("get_preset", {}),
  await callLocal("get_preset", {}), "invalid_argument");
compareError("get_preset 未知 id", await callServer("get_preset", { preset_id: "不存在的预设" }),
  await callLocal("get_preset", { preset_id: "不存在的预设" }), "invalid_argument");
compareError("get_preset 参数类型错", await callServer("get_preset", { preset_id: 7 }),
  await callLocal("get_preset", { preset_id: 7 }), "invalid_argument");
compareError("preview strength 越界", await callServer("preview_preset", { preset_id: presetIds[0], strength: 2 }),
  await callLocal("preview_preset", { preset_id: presetIds[0], strength: 2 }), "invalid_argument");
compareError("preview strength 非数字", await callServer("preview_preset", { preset_id: presetIds[0], strength: "强" }),
  await callLocal("preview_preset", { preset_id: presetIds[0], strength: "强" }), "invalid_argument");
compareError("preview 未知旋钮", await callServer("preview_preset", { preset_id: presetIds[0], knobs: { 不存在的旋钮: 1 } }),
  await callLocal("preview_preset", { preset_id: presetIds[0], knobs: { 不存在的旋钮: 1 } }), "invalid_argument");
if (withKnobs.length) {
  const p = withKnobs[0];
  const k = p.knobs[0];
  compareError("preview 旋钮越界", await callServer("preview_preset", { preset_id: p.preset_id, knobs: { [k.id]: k.max + 1 } }),
    await callLocal("preview_preset", { preset_id: p.preset_id, knobs: { [k.id]: k.max + 1 } }), "invalid_argument");
  compareError("preview 旋钮不是数字", await callServer("preview_preset", { preset_id: p.preset_id, knobs: { [k.id]: "热" } }),
    await callLocal("preview_preset", { preset_id: p.preset_id, knobs: { [k.id]: "热" } }), "invalid_argument");
}
compareError("preview knobs 不是对象", await callServer("preview_preset", { preset_id: presetIds[0], knobs: [1] }),
  await callLocal("preview_preset", { preset_id: presetIds[0], knobs: [1] }), "invalid_argument");
compareError("preview render.max_edge 越界", await callServer("preview_preset", { preset_id: presetIds[0], render: { max_edge: 9999 } }),
  await callLocal("preview_preset", { preset_id: presetIds[0], render: { max_edge: 9999 } }), "invalid_argument");
compareError("preview render.max_edge 不是整数", await callServer("preview_preset", { preset_id: presetIds[0], render: { max_edge: 512.5 } }),
  await callLocal("preview_preset", { preset_id: presetIds[0], render: { max_edge: 512.5 } }), "invalid_argument");
compareError("preview render 不认键", await callServer("preview_preset", { preset_id: presetIds[0], render: { size: 512 } }),
  await callLocal("preview_preset", { preset_id: presetIds[0], render: { size: 512 } }), "invalid_argument");
compareError("preview render.format 非法", await callServer("preview_preset", { preset_id: presetIds[0], render: { max_edge: 512, format: "webp" } }),
  await callLocal("preview_preset", { preset_id: presetIds[0], render: { max_edge: 512, format: "webp" } }), "invalid_argument");
compareError("preview render 不给 layer_id", await callServer("preview_preset", { preset_id: presetIds[0], render: { max_edge: 512 } }),
  await callLocal("preview_preset", { preset_id: presetIds[0], render: { max_edge: 512 } }), "invalid_argument");
compareError("apply 缺 layer_id", await callServer("apply_preset", { preset_id: presetIds[0] }),
  await callLocal("apply_preset", { preset_id: presetIds[0] }), "invalid_argument");
compareError("apply 未知预设", await callServer("apply_preset", { preset_id: "不存在的预设", layer_id: "layer_default" }),
  await callLocal("apply_preset", { preset_id: "不存在的预设", layer_id: "layer_default" }), "invalid_argument");

// **④ 展开比对（三档 strength ＋ 只修正段 ＋ 旋钮两端）✓**
const PREVIEW_SHARED = ["ok", "preset_id", "name_zh", "status", "strength", "include_correction",
                        "ops", "op_count", "notes", "preset_notes"];
const previewCases = [];
for (const id of [presetIds[0], bwId, knobPreset && knobPreset.preset_id].filter(Boolean)) {
  for (const strength of [0, 0.5, 1]) previewCases.push([`preview ${id} @${strength}`, { preset_id: id, strength }]);
  previewCases.push([`preview ${id} 不含修正段`, { preset_id: id, include_correction: false }]);
}
if (knobPreset) {
  const k = knobPreset.knobs[0];
  previewCases.push([`preview ${knobPreset.preset_id} 旋钮=${k.id}→min`, { preset_id: knobPreset.preset_id, knobs: { [k.id]: k.min } }]);
  previewCases.push([`preview ${knobPreset.preset_id} 旋钮=${k.id}→max`, { preset_id: knobPreset.preset_id, knobs: { [k.id]: k.max } }]);
}
for (const [label, args] of previewCases) {
  compare(label, await callServer("preview_preset", args), await callLocal("preview_preset", args),
    PREVIEW_SHARED, { serverOnly: ["created", "opened", "timings"], localOnly: ["server"] });
}
// **∴ 结构恒等式 ✓**（内核 ②：断关系而不是单点 ✓）：**「不含修正段」这一支必须真的少掉那几笔** ✓
{
  const noCorr = await callLocal("preview_preset", { preset_id: presetIds[0], include_correction: false });
  const withCorr = await callLocal("preview_preset", { preset_id: presetIds[0] });
  check(noCorr.body.op_count === first.style_op_count,
    `include_correction=false 的 preview 笔数（${noCorr.body.op_count}）应等于 style_op_count（${first.style_op_count}）✗ ⇒ 这个开关没被执行`);
  check(withCorr.body.op_count === first.style_op_count + first.correction_op_count,
    `默认 preview 笔数（${withCorr.body.op_count}）应等于 style+correction（${first.style_op_count}＋${first.correction_op_count}）✗`);
}
// **∴ 输入必须真的制造出「旋钮覆盖」这一支 ✗**（**四层齐第②层 ✓：空集合上的 every 恒真 ✓**）。
if (knobPreset) {
  const id = knobPreset.preset_id;
  const k = knobPreset.knobs[0];
  const lo = await callLocal("preview_preset", { preset_id: id, knobs: { [k.id]: k.min } });
  const hi = await callLocal("preview_preset", { preset_id: id, knobs: { [k.id]: k.max } });
  check(JSON.stringify(sortKeys(lo.body.ops)) !== JSON.stringify(sortKeys(hi.body.ops)),
    `旋钮「${k.id}」取 min 与取 max 展开完全相同 ⇒ 旋钮覆盖那一支根本没被执行（这几条比对在空转）✗`);
}
// **∴ 同样要确认「黑白 amount 中性值恒 0」那一支真的被执行到 ✗**（**否则上面的红/绿没有意义 ✓**）。
if (bwId) {
  const a = await callLocal("preview_preset", { preset_id: bwId, strength: 0 });
  const b = await callLocal("preview_preset", { preset_id: bwId, strength: 1 });
  const amountAt = (body) => {
    const op = (body.ops || []).find((o) => o.type === "black_and_white");
    return op && op.params ? op.params.amount : undefined;
  };
  const a0 = amountAt(a.body);
  const a1 = amountAt(b.body);
  check(a0 !== undefined && a1 !== undefined,
    `黑白预设 ${bwId} 的展开里没有 black_and_white 操作 ✗ ⇒ 那条规则没被测到（要改输入，不许改断言）`);
  check(a0 !== a1, `黑白 amount 在 strength=0 与 1 时相同（都是 ${a0}）✗ ⇒ 「恒按中性 0 插值」那一支没被执行`);
}

// **⑤ 落成比对 ✗**（**各自一份文档 ⇒ 比对「落了什么」而不是「第几个原子」✓**）。
const APPLY_SHARED = ["ok", "preset_id", "strength", "include_correction", "op_count",
                      "created_object_ids", "updated_object_ids", "revived_object_ids",
                      "revived", "updated"];
const APPLY_SERVER_ONLY = ["created", "opened", "timings", "atom_id", "changeset_id", "dirty_bbox",
                           "dirty_kind", "dirty_tiles", "duplicate", "head", "job_id", "job_status",
                           "preview", "seq", "snapshotted", "suggestions", "warnings"];
const APPLY_LOCAL_ONLY = ["server", "seq", "layer_id", "note"];
const LAYER = "layer_default";
const apply1Srv = await callServer("apply_preset", { preset_id: presetIds[0], layer_id: LAYER });
const apply1Loc = await callLocal("apply_preset", { preset_id: presetIds[0], layer_id: LAYER });
compare("apply 首次（全新建）", apply1Srv, apply1Loc, APPLY_SHARED,
  { serverOnly: APPLY_SERVER_ONLY, localOnly: APPLY_LOCAL_ONLY, divergent: ["created"] });
// **恒等式 ✓**（**内核 ②：断关系而不是断单点 ✓**）：**回执里的计数必须与 id 数组自洽 ✗**。
for (const [who, body] of [["服务端", apply1Srv.body], ["本地", apply1Loc.body]]) {
  check(body.updated === body.updated_object_ids.length && body.revived === body.revived_object_ids.length
    && body.op_count === body.created_object_ids.length + body.updated_object_ids.length + body.revived_object_ids.length,
    `${who}：apply 回执的计数与三段 id 数不自洽 ✗（op_count=${body.op_count}｜建 ${body.created_object_ids.length}／更 ${body.updated_object_ids.length}／活 ${body.revived_object_ids.length}）`);
}
// **★ 服务端缺陷留证 ✗**：HTTP 层对**每个**工具响应覆写 `created`／`opened` ⇒
//   `apply_preset` 自己的「建了几笔」被换成布尔 ✓（**实测 `created:false` 而 `created_object_ids` 有 11 条 ✓**）
//   ⇒ **∴ 判据不许比 `created` ✗**（两边含义不同：服务端布尔／本地数字 ✓）⇒ **调用方要读 `*_object_ids.length` ✓**。
check(Array.isArray(apply1Srv.body.created_object_ids) && apply1Srv.body.created_object_ids.length > 0
  && apply1Srv.body.created === false,
  "服务端 apply 的 created 覆写行为变了（不再是布尔）⇒ 这条留证断言要跟权威一起改，不是改本地 ✗");
check(apply1Loc.body.created === apply1Loc.body.created_object_ids.length,
  "本地 apply 的 created 应等于新建 id 数 ✗（本地没有 HTTP 层覆写，就该给真数字）");

const apply2Srv = await callServer("apply_preset", { preset_id: presetIds[0], layer_id: LAYER });
const apply2Loc = await callLocal("apply_preset", { preset_id: presetIds[0], layer_id: LAYER });
compare("apply 第二次（应全部走更新）", apply2Srv, apply2Loc, APPLY_SHARED,
  { serverOnly: APPLY_SERVER_ONLY, localOnly: APPLY_LOCAL_ONLY, divergent: ["created"] });
check(apply2Loc.body.updated_object_ids.length === apply2Loc.body.op_count
  && apply2Loc.body.created_object_ids.length === 0,
  `本地重跑同一预设没有「全部更新」✗（建 ${apply2Loc.body.created_object_ids.length}／更 ${apply2Loc.body.updated_object_ids.length}）⇒ 确定性 object_id 没起作用 ✓`);

compareError("apply 图层不存在", await callServer("apply_preset", { preset_id: presetIds[0], layer_id: "layer_不存在的层" }),
  await callLocal("apply_preset", { preset_id: presetIds[0], layer_id: "layer_不存在的层" }), "reference_not_found");
// **⑥ 参数冲突也要逐字 ✗**（**∴ 这一支服务端在 `write_apply_preset` 里 ✓：同 id 已是别的类型／别的效果 ⇒ 拒绝覆盖 ✓**）。
//   **∴ 必须用一套**还没落过**的预设 ✗**：`presetIds[0]` 已经被 ⑤ 落成过 ⇒ 它的确定性 id 已被**真对象**占住 ✓
//   ⇒ 占位对象**建不进去** ⇒ 这一支在空转 ✓（第一版就是这么坏的：判据自己报「占位对象没建成功」✓）。
const conflictPreset = presetIds.find((id) => id !== presetIds[0]);
{
  const pv = await callServer("preview_preset", { preset_id: conflictPreset, strength: 1 });
  const op0 = (pv.body.ops || [])[0];
  check(!!op0 && typeof op0.object_id === "string",
    `preview ${conflictPreset} 取不到第一条操作的 object_id ✗ ⇒ ⑥ 没法造「同 id 异类型」`);
  if (op0 && typeof op0.object_id === "string") {
    // 预设那笔是 adjustment ⇒ 占位放 filter（反之亦然）⇒ 类型一致这一关必被拒 ✓
    const ph = op0.kind === "adjustment"
      ? { tool: "add_filter", args: { layer_id: LAYER, filter_name: "clarity", object_id: op0.object_id, params: { amount: 0.1 } } }
      : { tool: "add_adjustment", args: { layer_id: LAYER, adjustment_type: "exposure", object_id: op0.object_id, params: { ev: 0.1 } } };
    const srv = await callServer(ph.tool, ph.args);
    const loc = await callLocal(ph.tool, ph.args);
    check(!!srv.body && !!loc.body && srv.body.ok === true && loc.body.ok === true,
      `占位对象没建成功 ✗ 服务端 ${JSON.stringify(srv.body).slice(0, 120)}｜本地 ${JSON.stringify(loc.body).slice(0, 120)}`);
    const srv2 = await callServer("apply_preset", { preset_id: conflictPreset, layer_id: LAYER });
    const loc2 = await callLocal("apply_preset", { preset_id: conflictPreset, layer_id: LAYER });
    compareError("apply 撞上别的类型", srv2, loc2, "invalid_argument");
  }
}
// **⑦ 图层墓碑（存在但已删）✗**：**两阶段口径要分清 ✓** —— 第一阶段 `state.layers.contains_key` 放行 ✓
//   （所以**不是** 404「图层不存在」✗），但**第二阶段落 `create_object` 时折叠层拒绝** ✓：
//   实测服务端 **409／`precondition_failed`／「图层 X 已删除，create_object 被拒绝」** ✓
//   ⇒ **本地必须同一状态码同一句话** ✓。
//   **∴ 第一版判据把这里写成「两边都该接受」✗** —— 那是**我的期望**错，不是产品错 ✓
//   （「我的测量比被测对象更容易错」再记一次 ✓）。
//   **∴ 两边的层 id 取**同一个名字**✓**（错误文案里含层 id ⇒ 一边 `psrv-*`／一边 `ploc-*` 就永远比不出一致 ✗）。
const DEAD_LAYER = "preset-dead";
const deadPreset = presetIds.find((id) => id !== presetIds[0] && id !== conflictPreset);
await callServer("create_layer", { layer_id: DEAD_LAYER, name: "要删的层" });
await callLocal("create_layer", { layer_id: DEAD_LAYER, name: "要删的层" });
await callServer("delete_layer", { layer_id: DEAD_LAYER });
await callLocal("delete_layer", { layer_id: DEAD_LAYER });
{
  const srv = await callServer("apply_preset", { preset_id: deadPreset, layer_id: DEAD_LAYER });
  const loc = await callLocal("apply_preset", { preset_id: deadPreset, layer_id: DEAD_LAYER });
  compareError("apply 到已删除的图层", srv, loc, "precondition_failed");
  check(srv.status === 409,
    `服务端对「已删但存在」的图层给的状态码变了（实测 ${srv.status}）✗ ⇒ 判据口径要跟权威改，不是改本地`);
}
// **⑧ 跨层复活：对象随层删除 ⇒ 落到另一层应当「复活」而不是拒绝 ✗**（**`revive_effect_in_place` ✓**）。
//   **∴ 输入要真的制造出「级联墓碑」✗**：先落到一层 ⇒ **再删那层**（`tombstone_layer` 级联把对象也标死 ✓）
//   ⇒ 才叫「孤儿墓碑」✓。**第一版把顺序写成「先删层再落预设」** ⇒ 对象是在**死层上新建**的、
//   **从未被级联** ⇒ 服务端与本地都报「全部更新」✓ ⇒ 判据自己的期望错了（**「我的测量比被测对象更容易错」第 N 次 ✓**）。
const revivePreset = presetIds.find(
  (id) => ![presetIds[0], conflictPreset, deadPreset].includes(id));
const REVIVE_LAYER = "preset-revive";
await callServer("create_layer", { layer_id: REVIVE_LAYER, name: "复活测试层" });
await callLocal("create_layer", { layer_id: REVIVE_LAYER, name: "复活测试层" });
{
  const seedSrv = await callServer("apply_preset", { preset_id: revivePreset, layer_id: REVIVE_LAYER });
  const seedLoc = await callLocal("apply_preset", { preset_id: revivePreset, layer_id: REVIVE_LAYER });
  check(!!seedSrv.body && seedSrv.body.ok === true && !!seedLoc.body && seedLoc.body.ok === true,
    `⑧ 的种子（把预设落到复活测试层）没成功 ✗ 服务端 ${JSON.stringify(seedSrv.body).slice(0, 120)}｜本地 ${JSON.stringify(seedLoc.body).slice(0, 120)}`);
  await callServer("delete_layer", { layer_id: REVIVE_LAYER });
  await callLocal("delete_layer", { layer_id: REVIVE_LAYER });
  const srv = await callServer("apply_preset", { preset_id: revivePreset, layer_id: LAYER });
  const loc = await callLocal("apply_preset", { preset_id: revivePreset, layer_id: LAYER });
  compare("apply 到默认层（对象随死层消失 ⇒ 应全部复活）", srv, loc, APPLY_SHARED,
    { serverOnly: APPLY_SERVER_ONLY, localOnly: APPLY_LOCAL_ONLY, divergent: ["created"] });
  check(loc.body.revived_object_ids.length === loc.body.op_count,
    `本地这一轮应当「全部复活」✗（revived=${loc.body.revived_object_ids.length}／op_count=${loc.body.op_count}）⇒ 级联墓碑口径没对上 ✓`);
}

// **⑨ `preview` 带 render ⇒ 本地这条只剩**两种诚实出口**✓**（**第 六百七十五 节改口径**）。
//   **∴ 以前这里断的是「必须 501 ＋ 理由写着本地做不到」✗ 而那个理由是错的**✗
//   （**单层裸像素早就有 `p2LayerScopeAtoms`；「临时文档」＝第二个内核实例 ✓**）⇒ 现在补成了真实现 ✓
//   **∴ 而这份判据跑在 stub 环境**✗（**`pwa-node-stub.mjs` 没有真 wasm ⇒ 拿不到 `WasmKernel` ✓**）
//   ⇒ **∴ 所以它只能断「出口的形态」**✗ **不能断像素**✓：**逐字节比对在 `tool-pwa-preset-render-parity.mjs`**
//   （**「同一事实只写一处」✓**）。**红的情形**：那条 501 的理由又变回「本地渲染不出来」✗
//   或者 200 却不带 `server:false`／缺图 ✗
//   （**⇒ 调用方会把「内核没就绪」读成「产品做不到」⇒ 那正是形态③的反向 ✓**）。
{
  const srv = await callServer("preview_preset", { preset_id: presetIds[0], layer_id: LAYER, render: { max_edge: 256 } });
  const loc = await callLocal("preview_preset", { preset_id: presetIds[0], layer_id: LAYER, render: { max_edge: 256 } });
  check(!!srv.body && srv.body.ok === true, `服务端这条本该成功 ✗ ${JSON.stringify(srv.body ? srv.body.context : null)}`);
  const reason = String((loc.body && (loc.body.reason || (loc.body.context && loc.body.context.detail))) || "");
  if (loc.status === 501) {
    // **∴ 两个都算诚实出口 ✓**：**`kernel_unavailable`**（**这份桩里连 wasm 模块都取不到 ⇒ 第②步取源像素就报 ✓**）
    //   与 **`not_implemented_locally`**（**模块取到了、但没有 `WasmKernel` ⇒ 临时文档建不起来 ✓**）。
    check(loc.body && (loc.body.error === "not_implemented_locally" || loc.body.error === "kernel_unavailable"),
      `本地这条 501 的错误码不是「内核没就绪」那两个 ✗（实测 ${JSON.stringify(loc.body && loc.body.error)}｜reason「${reason.slice(0, 120)}」）`);
    check(loc.body && loc.body.server === false, "本地 501 没标 server:false ✗ 调用方会以为是服务端给的");
    check(/内核.*(未加载|没就绪|不可用)/.test(reason),
      `本地这条 501 的理由不是「内核没就绪」✗（实测「${reason.slice(0, 120)}」）⇒ 又退回那句已被实测证伪的「本地渲染不出来」✓`);
  } else {
    check(loc.status === 200 && !!loc.body && loc.body.ok === true && loc.body.server === false,
      `本地这条既不是 501 也不是干净的 200 ✗（状态 ${loc.status}｜${JSON.stringify(loc.body).slice(0, 160)}）`);
    check(typeof loc.body.image_base64 === "string" && loc.body.image_base64.length > 0,
      `本地 200 却没带回图 ✗（有 ok:true 而无 image_base64 ⇒ 空口成功 ✓）`);
    check(srv.body.image_width === loc.body.image_width && srv.body.image_height === loc.body.image_height,
      `本地代理尺寸与服务端不同 ✗ 服务端 ${srv.body.image_width}×${srv.body.image_height}｜本地 ${loc.body.image_width}×${loc.body.image_height}`);
  }
}

// **⑩ C/S 客户端模式 ⇒ 必须**代理**到服务端，而不是用内联库**遮蔽**✗**（**静默降级形态③ ✓**）。
{
  stub("window", { __pwaLocalOnly: false, yanshiStats: { token: TOKEN } });
  stub("fetch", (input, init) => {
    const url = String(input);
    return nativeFetch(url.startsWith("/") ? BASE + url + (url.includes("token=") ? "" : `&token=${TOKEN}`) : url, init);
  });
  const srv = await callServer("list_presets", {});
  // **∴ 代理过去的 `doc` 查询串必须是**服务端存在的文档 ✗**（实测：服务端**以查询串优先** ✓ ——
  //   查询串里的 doc 不存在 ⇒ 404「文档 … 未打开」✓，哪怕 body 里的 `doc_id` 是好的 ✗）。
  const res = await localApi(new Request(`https://x/api/tools/list_presets?doc=${SERVER_DOC}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: SERVER_DOC }),
  }));
  const loc = await res.json().catch(() => null);
  check(res.headers.get("x-yanshi-source") === "server-proxy",
    "C/S 模式下 list_presets 没走代理（响应头缺 x-yanshi-source:server-proxy）✗ ⇒ 本地内联库遮蔽了服务端库 ✓");
  check(!!(srv.body && srv.body.timings) && !!(loc && loc.timings),
    "C/S 模式这一条要比的是「两边都拿到服务端响应」✗（有一边没有 timings）");
  check(!loc || loc.server === undefined, "C/S 模式的响应不该带 server:false ✗（带了说明它其实是本地算的）");
  // **∴ 每一条本地端点分支的 `!__csClientMode()` 闸都要**各有一条断言**✗**：
  //   第一版只测 `list_presets` ⇒ **把 `preview_preset` 的闸删掉，判据照样全绿** ✗
  //   （实测：M2 变异 ⇒ 退出码 0 ✓）—— **「变异粒度是断言不是判据」的又一次实例 ✓**。
  const resPreview = await localApi(new Request(`https://x/api/tools/preview_preset?doc=${SERVER_DOC}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: SERVER_DOC, preset_id: presetIds[0] }),
  }));
  check(resPreview.headers.get("x-yanshi-source") === "server-proxy",
    "C/S 模式下 preview_preset 没走代理 ✗ ⇒ 本地内联展开遮蔽了服务端（两者口径从此可能漂移）✓");
  stub("window", { __pwaLocalOnly: true, fetch: () => Promise.resolve(new Response("{}", { status: 200 })) });
  stub("fetch", nativeFetch);
}

if (bad.length) {
  console.log(`✗ PWA 预设四端点与服务端同口径：${bad.length} 条不符`);
  for (const m of bad) console.log("   · " + m);
  process.exit(1);
}
console.log(`✓ PWA 预设四端点与服务端逐字段同口径（${presetIds.length} 套库／展开三档／错误文案逐字／落成计数／带 render 的两种诚实出口／C-S 代理）`);
