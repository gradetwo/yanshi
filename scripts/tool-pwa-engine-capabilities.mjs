#!/usr/bin/env node
// **判据：PWA 本地的能力表与属性写入端点，与服务端同答案** ✓（台账「第 六百七十一 节」）。
//
// **∴ 这一片修的是什么 ✗**（每个都是**用户能看见**的分歧 ✓）：
//   · `set_layer_blend` 本地**只认 11 种混合模式** ✗ ⇒ 图层面板选 `linear_light`／`soft_light`
//     时**本地 400、云端能改** ✓（**清单抄漏两项** ⇒ 离线模式少两个功能 ✓）；
//   · `set_layer_blend`／`lock_layer`／`unlock_layer`／`set_property` 对**不存在的图层／对象**
//     **照写原子并回 ok** ✗ ⇒ 折叠层把它丢掉 ⇒ **回执说成功、状态没变** ✓（静默降级形态③）；
//   · `update_layer`／`set_property` **不校验值类型** ✗ ⇒ `visible:"yes"` 被接受 ✓
//     （服务端第 1440 轮已在提交层拒绝 ✓）；
//   · `set_property` **缺 `value` 时本地回 400** ✗ 而服务端**照收并写 `null`** ✓；
//   · 本地**没有** `list_engine_capabilities` ✗ ⇒ 前端问「这个引擎支持什么」只能 501 ✓。
//
// **∴ 它断什么 ✗**（每条都要能红 ✓）：
//   §A 能力表**整份逐字同形** ✓（信封字段除外 ✓）＋ `counts.X === X.length` 恒等式 ✓
//      ＋ `retouch_kind_tools` 的键集合 ＝ `retouch_kinds` ✓ ＋ `brush_settings` 无重复 ✓；
//   §B **服务端能力表里的每一种混合模式**两边都收 ✓（**清单从服务端读 ✗ 不在判据里写死 ✓**，
//      否则上游增减一项判据就过期 ✓）＋ 大写／混合大小写归一化 ✓ ＋ 八种 400／404／409 文案逐字 ✓；
//   §C `lock_layer`／`unlock_layer` 成对语义 ＋ 六种错误逐字 ✓；
//   §D `set_property`：成功（含**缺省 value ⇒ null** ✓）＋ 九种值类型／引用错误逐字 ✓
//      ＋ **引用先于值类型** ✓ ＋ 锁层里**对象**被拒而**图层自身属性**放行 ✓；
//   §E `update_layer`：成功计数／原子数恒等式 ✓ ＋ 十种错误逐字 ✓
//      ＋ **已记录的分歧**：服务端 changeset **逐条落盘**（半写 ✓）而本地**整批拒绝** ✓。
//
// **∴ 前提 ✗**：活服务端（`run-criteria-local.sh` ✓）＋ 不过期的真 wasm 内核（旧了 `exit(3)` ✓）。
process.on("uncaughtException", (error) => {
  console.error(String((error && error.stack) || error).slice(0, 1200));
  process.exit(3);
});
import { pathToFileURL } from "node:url";

const BASE = process.argv[2];
if (!BASE || !/^https?:\/\//.test(BASE)) {
  console.log("⊘ 不可判：argv[2] 缺服务端地址 ⇒ 请用 bash scripts/run-criteria-local.sh scripts/tool-pwa-engine-capabilities.mjs 跑"
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
// **服务端 HTTP 信封 ✗ 不是工具契约** ✓（`created`／`opened`／`timings` 逐次不同 ✓）⇒ 比对前两边都剥掉 ✓。
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
/** **写端点只比契约字段 ✗ 不比整份响应 ✓**：服务端的 `applied`／`atoms` 里带
 *  `atom_id`／`preview.thumb_url`／`timings`（**逐次不同 ✓，本地没有也不该编** ✗）
 *  ⇒ 所以写端点断「状态 ＋ ok ＋ 选定的契约字段」✓（**选定字段必须是两边都有的 ✓**）。 */
function compareSuccess(label, srv, loc, pick = []) {
  check(srv.status === 200 && srv.body && srv.body.ok === true,
    `${label}：服务端应 200/ok ✗ 状态 ${srv.status}｜${show(srv.body)}`);
  check(loc.status === 200 && loc.body && loc.body.ok === true,
    `${label}：本地应 200/ok ✗ 状态 ${loc.status}｜${show(loc.body)}`);
  if (!srv.body || !loc.body) return;
  check(loc.body.server === false, `${label}：本地应标 server:false ✗ ${show(loc.body.server)}`);
  for (const key of pick) {
    check(same(srv.body[key], loc.body[key]),
      `${label}：${key} 不一致 ✗ 服务端 ${show(srv.body[key])}｜本地 ${show(loc.body[key])}`);
  }
}
function compareError(label, srv, loc, code, status) {
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
/** **把服务端的完整原子按 seq 顺序喂进本地文档 ✓**（判据装配，见文件头；**只能播一次** ✓）。 */
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

const RED = [1, 0, 0, 1];
const rect = (objectId, layerId) => ["draw_shape", { layer_id: layerId, object_id: objectId,
  data: { geometry: { kind: "rect", bbox: { x: 0, y: 0, w: 5, h: 5 } }, color: RED } }];

/** **一个 case 开一份服务端文档 ＋ 一份本地文档** ✓（前序失败 ⇒ `exit(3)`：那是前提 ✗ 不是产品 ✓）。 */
async function makeCase(key, ops) {
  const serverDoc = `ecs-${key}-${UNIQ}`;
  const localDoc = `ecl-${key}-${UNIQ}`;
  const token = await createDoc(serverDoc, 96, 72);
  for (const [tool, args] of ops) {
    const res = await callServer(tool, args, token, serverDoc);
    if (res.status !== 200 || !res.body || res.body.ok === false) {
      console.log(`⊘ 不可判：前序 ${tool}（${key}）服务端拒绝（${res.status} ${show(res.body)}）`
        + " ⇒ 前提不成立，不是产品问题");
      process.exit(3);
    }
  }
  await seedLocal(serverDoc, token, localDoc);
  return {
    token, serverDoc, localDoc,
    server: (tool, args) => callServer(tool, args, token, serverDoc),
    local: (tool, args) => localCall(tool, args, localDoc),
    both: async (tool, args) => [await callServer(tool, args, token, serverDoc),
      await localCall(tool, args, localDoc)],
  };
}

// ══ §A 能力表：整份同形 ＋ counts 恒等式 ═══════════════════════════════════════
{
  const ctx = await makeCase("a", [
    ["create_layer", { layer_id: "V1", kind: "vector", name: "矢量层" }],
  ]);
  const [s, l] = await ctx.both("list_engine_capabilities", {});
  compareBody("§A 能力表", s, l);
  const b = s.body || {};
  const sides = [["服务端", b], ["本地", l.body || {}]];
  const COUNT_KEYS = ["adjustments", "blend_modes", "brush_settings", "filters",
                      "liquify_kinds", "retouch_kinds"];
  check(same(Object.keys(b.counts || {}).sort(), COUNT_KEYS.slice().sort()),
    `§A counts 的键集合不对 ✗ ${show(b.counts)}`);
  for (const key of COUNT_KEYS) {
    for (const [who, body] of sides) {
      check(Array.isArray(body[key]) && body.counts && body[key].length === body.counts[key],
        `§A counts 恒等式（${who}）：${key} 的条数应等于 counts.${key} ✗ `
        + `条数 ${show(body[key] && body[key].length)}｜counts ${show(body.counts && body.counts[key])}`);
    }
  }
  // **UI 真的会写这两项** ✓（图层面板的 13 种模式里有它们 ✓）—— 值断言，不是条数快照 ✓。
  for (const [who, body] of sides) {
    check(Array.isArray(body.blend_modes) && body.blend_modes.includes("linear_light")
      && body.blend_modes.includes("soft_light"),
    `§A blend_modes（${who}）应含 linear_light 与 soft_light ✗ ${show(body.blend_modes)}`);
    check(same(Object.keys(body.retouch_kind_tools || {}).sort(),
      (body.retouch_kinds || []).slice().sort()),
    `§A retouch_kind_tools（${who}）的键集合应等于 retouch_kinds ✗ ${show(body.retouch_kind_tools)}`);
    check(Array.isArray(body.brush_settings)
      && new Set(body.brush_settings).size === body.brush_settings.length,
    `§A brush_settings（${who}）不应有重复项 ✗ ${show(body.brush_settings)}`);
  }
  // 入口校验：零参数工具的「可用参数：无」也要逐字 ✓。
  const [sBad, lBad] = await ctx.both("list_engine_capabilities", { foo: 1 });
  compareError("§A2 未知参数", sBad, lBad, "invalid_argument", 400);
}

// ══ §B set_layer_blend：13 种模式 ＋ 归一化 ＋ 八种错误 ═══════════════════════════
{
  const ctx = await makeCase("b", [
    ["create_layer", { layer_id: "L1", kind: "raster", name: "层一" }],
    ["create_layer", { layer_id: "L2", kind: "raster", name: "层二" }],
    ["delete_layer", { layer_id: "L2" }],
  ]);
  // **清单从服务端读 ✗ 不在判据里写死 ✓**（上游加一种模式 ⇒ 判据自动覆盖 ✓，不会过期 ✓）。
  const caps = await ctx.server("list_engine_capabilities", {});
  const modes = caps.body && Array.isArray(caps.body.blend_modes) ? caps.body.blend_modes : [];
  if (!modes.length) {
    console.log("⊘ 不可判：服务端能力表没有 blend_modes ⇒ 前提不成立，不是产品问题");
    process.exit(3);
  }
  for (const mode of modes) {
    const [s, l] = await ctx.both("set_layer_blend", { layer_id: "L1", mode });
    compareSuccess(`§B1 模式 ${mode}`, s, l, ["layer_id", "mode"]);
    check(l.body && l.body.mode === mode,
      `§B1 模式 ${mode}：本地回执 mode 应为 ${mode} ✗ ${show(l.body && l.body.mode)}`);
  }
  // 大写／混合大小写 ⇒ 服务端 `to_ascii_lowercase` ⇒ 回执是小写 ✓。
  for (const [raw, want] of [["SCREEN", "screen"], ["Soft_Light", "soft_light"]]) {
    const [s, l] = await ctx.both("set_layer_blend", { layer_id: "L1", mode: raw });
    compareSuccess(`§B2 归一化 ${raw}`, s, l, ["mode"]);
    check(s.body && s.body.mode === want,
      `§B2 ${raw}：服务端回执 mode 应为 ${want} ✗ ${show(s.body && s.body.mode)}`);
    check(l.body && l.body.mode === want,
      `§B2 ${raw}：本地回执 mode 应为 ${want} ✗ ${show(l.body && l.body.mode)}`);
  }
  const errors = [
    ["§B3 未知模式", { layer_id: "L1", mode: "bogus" }, "invalid_argument", 400],
    ["§B4 mode 非字符串", { layer_id: "L1", mode: 5 }, "invalid_argument", 400],
    ["§B5 mode 为 null", { layer_id: "L1", mode: null }, "invalid_argument", 400],
    ["§B6 layer_id 非字符串", { layer_id: 123, mode: "screen" }, "invalid_argument", 400],
    ["§B7 缺 mode", { layer_id: "L1" }, "invalid_argument", 400],
    ["§B8 缺 layer_id", { mode: "screen" }, "invalid_argument", 400],
    ["§B9 未知参数", { layer_id: "L1", mode: "screen", foo: 1 }, "invalid_argument", 400],
    ["§B10 图层不存在", { layer_id: "NOPE", mode: "screen" }, "reference_not_found", 404],
    ["§B11 图层已删", { layer_id: "L2", mode: "screen" }, "precondition_failed", 409],
    // **空串是「给了 id」✓**（`optional_str` 只查类型 ⇒ `Some("")` ⇒ 提交期 404 ✓）。
    ["§B12 空 layer_id", { layer_id: "", mode: "screen" }, "reference_not_found", 404],
  ];
  for (const [label, args, code, status] of errors) {
    const [s, l] = await ctx.both("set_layer_blend", args);
    compareError(label, s, l, code, status);
  }
  const [sMissing] = await ctx.both("set_layer_blend", { layer_id: "NOPE", mode: "screen" });
  check(String(detailOf(sMissing)).includes("现有图层："),
    `§B10 文案应列出候选图层 ✗ ${show(detailOf(sMissing))}`);
  const [sUnknown] = await ctx.both("set_layer_blend", { layer_id: "L1", mode: "bogus" });
  for (const mode of modes) {
    check(String(detailOf(sUnknown)).includes(mode),
      `§B3 未知模式的文案应列出可用模式 ${mode} ✗ ${show(detailOf(sUnknown))}`);
  }
}

// ══ §C lock_layer／unlock_layer ═════════════════════════════════════════════════
{
  const ctx = await makeCase("c", [
    ["create_layer", { layer_id: "L1", kind: "raster", name: "层一" }],
    ["create_layer", { layer_id: "L2", kind: "raster", name: "层二" }],
    ["delete_layer", { layer_id: "L2" }],
  ]);
  const [sl, ll] = await ctx.both("lock_layer", { layer_id: "L1" });
  compareSuccess("§C1 锁定", sl, ll);
  check(ll.body && ll.body.locked === true,
    `§C1 本地回执应带 locked:true ✗ ${show(ll.body)}`);
  const [su, lu] = await ctx.both("unlock_layer", { layer_id: "L1" });
  compareSuccess("§C2 解锁", su, lu);
  check(lu.body && lu.body.locked === false,
    `§C2 本地回执应带 locked:false ✗ ${show(lu.body)}`);
  const errors = [
    ["§C3 缺 layer_id", {}, "invalid_argument", 400],
    ["§C4 layer_id 非字符串", { layer_id: 123 }, "invalid_argument", 400],
    ["§C5 layer_id 为 null", { layer_id: null }, "invalid_argument", 400],
    ["§C6 未知参数", { layer_id: "L1", x: 1 }, "invalid_argument", 400],
    ["§C7 图层不存在", { layer_id: "NOPE" }, "reference_not_found", 404],
    ["§C8 图层已删", { layer_id: "L2" }, "precondition_failed", 409],
    ["§C10 空 layer_id", { layer_id: "" }, "reference_not_found", 404],
  ];
  for (const [label, args, code, status] of errors) {
    const [s, l] = await ctx.both("lock_layer", args);
    compareError(label, s, l, code, status);
  }
  // **`unlock_layer` 的错误文案带的是它自己的工具名** ✓（分支共用时最容易写死成 lock ✓）。
  const [sU, lU] = await ctx.both("unlock_layer", { layer_id: "L1", x: 1 });
  compareError("§C9 unlock 未知参数", sU, lU, "invalid_argument", 400);
  check(String(detailOf(sU)).startsWith("unlock_layer "),
    `§C9 文案应以 unlock_layer 开头 ✗ ${show(detailOf(sU))}`);
}

// ══ §D set_property：成功 ＋ 值类型／引用／锁 ═══════════════════════════════════
{
  const ctx = await makeCase("d", [
    ["create_layer", { layer_id: "L1", kind: "raster", name: "层一" }],
    ["create_layer", { layer_id: "LK", kind: "raster", name: "锁层" }],
    ["create_layer", { layer_id: "L2", kind: "raster", name: "层二" }],
    rect("P1", "L1"),
    rect("PK", "LK"),
    rect("PG", "L1"),
    ["delete_object", { object_id: "PG" }],
    ["lock_layer", { layer_id: "LK" }],
    ["delete_layer", { layer_id: "L2" }],
  ]);
  const [s1, l1] = await ctx.both("set_property", { layer_id: "L1", key: "visible", value: false });
  compareSuccess("§D1 图层 visible", s1, l1);
  // **缺省 value ⇒ null ✓**（服务端 `unwrap_or(Null)` ✓；`name` 不在值类型校验里 ⇒ 收 ✓）。
  const [s2, l2] = await ctx.both("set_property", { layer_id: "L1", key: "name" });
  compareSuccess("§D2 缺省 value", s2, l2);
  check(l2.body && l2.body.value === null,
    `§D2 本地回执的 value 应为 null ✗ ${show(l2.body && l2.body.value)}`);
  const [s3, l3] = await ctx.both("set_property", { layer_id: "L1", key: "type", value: "vector" });
  compareSuccess("§D3 合法图层类型", s3, l3);
  const [s4, l4] = await ctx.both("set_property", { layer_id: "L1", key: "z_index", value: 3 });
  compareSuccess("§D4 z_index 整数", s4, l4);
  const [s5, l5] = await ctx.both("set_property", { object_id: "P1", key: "type", value: "shape" });
  compareSuccess("§D5 合法对象类型", s5, l5);
  const errors = [
    ["§D6 visible 非布尔", { layer_id: "L1", key: "visible", value: "yes" }, "invalid_argument", 400],
    ["§D7 visible 缺省（null）", { layer_id: "L1", key: "visible" }, "invalid_argument", 400],
    ["§D8 z_index 非整数", { layer_id: "L1", key: "z_index", value: 1.5 }, "invalid_argument", 400],
    ["§D9 z_index 超出 i64", { layer_id: "L1", key: "z_index", value: 1e19 }, "invalid_argument", 400],
    ["§D10 opacity 非数值", { layer_id: "L1", key: "opacity", value: "x" }, "invalid_argument", 400],
    ["§D11 图层类型未知", { layer_id: "L1", key: "type", value: "bogus" }, "invalid_argument", 400],
    ["§D12 对象类型未知", { object_id: "P1", key: "type", value: "bogus" }, "invalid_argument", 400],
    ["§D13 对象 visible 非布尔", { object_id: "P1", key: "visible", value: "yes" }, "invalid_argument", 400],
    ["§D14 缺少 key", { value: true, layer_id: "L1" }, "invalid_argument", 400],
    ["§D15 key 非字符串", { layer_id: "L1", key: 5, value: true }, "invalid_argument", 400],
    ["§D16 key 为 null", { layer_id: "L1", key: null, value: true }, "invalid_argument", 400],
    ["§D17 两个 id 都没给", { key: "visible", value: true }, "invalid_argument", 400],
    ["§D18 未知参数", { layer_id: "L1", key: "visible", value: true, foo: 1 }, "invalid_argument", 400],
    ["§D19 图层不存在", { layer_id: "NOPE", key: "visible", value: true }, "reference_not_found", 404],
    ["§D20 对象不存在", { object_id: "NOPE", key: "visible", value: true }, "reference_not_found", 404],
    ["§D21 图层已删", { layer_id: "L2", key: "visible", value: false }, "precondition_failed", 409],
    ["§D22 对象已删", { object_id: "PG", key: "visible", value: false }, "precondition_failed", 409],
    ["§D23 锁层里的对象被拒", { object_id: "PK", key: "opacity", value: 0.5 }, "permission_denied", 403],
    // **∴ 引用先于值类型 ✓**（`validate_commit` 的顺序 ✓）⇒ 两条都错时报的是引用 ✓。
    ["§D24 引用先于值类型", { layer_id: "NOPE", key: "visible", value: "yes" }, "reference_not_found", 404],
    // **空串是「给了 id」✓** ⇒ 走引用检查（不是「两个 id 都没给」✗）。
    ["§D27 空 layer_id", { layer_id: "", key: "visible", value: true }, "reference_not_found", 404],
    ["§D28 空 object_id", { object_id: "", key: "visible", value: true }, "reference_not_found", 404],
  ];
  for (const [label, args, code, status] of errors) {
    const [s, l] = await ctx.both("set_property", args);
    compareError(label, s, l, code, status);
  }
  const [s23] = await ctx.both("set_property", { object_id: "PK", key: "opacity", value: 0.5 });
  check(detailOf(s23) === "不能修改 PK：所在图层已锁定（先解锁再操作；改 locked/visible 不受限制）",
    `§D23 文案不对 ✗ ${show(detailOf(s23))}`);
  // **例外：锁层里的对象改 visible 放行 ✓**（否则永远解不开 ✓）。
  const [s25, l25] = await ctx.both("set_property", { object_id: "PK", key: "visible", value: false });
  compareSuccess("§D25 锁层里改 visible", s25, l25);
  // **图层自身的属性不受图层锁影响 ✓**（锁的是内容 ✓）——服务端实测放行 ✓。
  const [s26, l26] = await ctx.both("set_property", { layer_id: "LK", key: "opacity", value: 0.5 });
  compareSuccess("§D26 锁层的图层属性", s26, l26);
}

// ══ §E update_layer：计数恒等式 ＋ 错误 ＋ 半写分歧 ═══════════════════════════════
{
  const ctx = await makeCase("e", [
    ["create_layer", { layer_id: "L1", kind: "raster", name: "层一" }],
    ["create_layer", { layer_id: "LK", kind: "raster", name: "锁层" }],
    ["create_layer", { layer_id: "L2", kind: "raster", name: "层二" }],
    ["delete_layer", { layer_id: "L2" }],
    ["lock_layer", { layer_id: "LK" }],
  ]);
  async function success(label, patch, wantCount) {
    const [s, l] = await ctx.both("update_layer", { layer_id: "L1", patch });
    compareSuccess(label, s, l, ["count"]);
    check(s.body && s.body.count === wantCount,
      `${label}：服务端 count 应为 ${wantCount} ✗ ${show(s.body && s.body.count)}`);
    check(l.body && l.body.count === wantCount,
      `${label}：本地 count 应为 ${wantCount} ✗ ${show(l.body && l.body.count)}`);
    check(Array.isArray(s.body && s.body.atoms) && s.body.atoms.length === s.body.count,
      `${label}：服务端 atoms 条数应等于 count ✗ ${show(s.body && { c: s.body.count, n: s.body.atoms && s.body.atoms.length })}`);
    check(Array.isArray(l.body && l.body.atoms) && l.body.atoms.length === l.body.count,
      `${label}：本地 atoms 条数应等于 count ✗ ${show(l.body && { c: l.body.count, n: l.body.atoms && l.body.atoms.length })}`);
  }
  await success("§E1 单个键", { visible: false }, 1);
  await success("§E2 两个键", { opacity: 0.5, visible: false }, 2);
  await success("§E3 混合模式", { blend_mode: "screen" }, 1);
  const [sL, lL] = await ctx.both("update_layer", { layer_id: "LK", patch: { opacity: 0.5 } });
  compareSuccess("§E4 锁层的图层属性", sL, lL, ["count"]);
  const errors = [
    ["§E5 混合模式未知", { layer_id: "L1", patch: { blend_mode: "BOGUS" } }, "invalid_argument", 400],
    // **`update_layer` 不做大小写归一 ✗**（服务端 `BlendMode::parse` 严格 ✓；`set_layer_blend` 才归一 ✓）。
    ["§E6 混合模式大写被拒", { layer_id: "L1", patch: { blend_mode: "SCREEN" } }, "invalid_argument", 400],
    ["§E7 混合模式非字符串", { layer_id: "L1", patch: { blend_mode: 5 } }, "invalid_argument", 400],
    ["§E8 visible 非布尔", { layer_id: "L1", patch: { visible: "yes" } }, "invalid_argument", 400],
    ["§E9 z_index 非整数", { layer_id: "L1", patch: { z_index: 1.5 } }, "invalid_argument", 400],
    ["§E10 只有未知键", { layer_id: "L1", patch: { type: "bogus" } }, "invalid_argument", 400],
    ["§E11 空 patch", { layer_id: "L1", patch: {} }, "invalid_argument", 400],
    ["§E12 patch 非对象", { layer_id: "L1", patch: "x" }, "invalid_argument", 400],
    ["§E13 缺 layer_id", { patch: { visible: false } }, "invalid_argument", 400],
    ["§E14 未知顶层参数", { layer_id: "L1", patch: { visible: false }, foo: 1 }, "invalid_argument", 400],
    ["§E15 图层不存在", { layer_id: "NOPE", patch: { visible: false } }, "reference_not_found", 404],
    ["§E16 图层已删", { layer_id: "L2", patch: { visible: false } }, "precondition_failed", 409],
    ["§E20 空 layer_id", { layer_id: "", patch: { visible: false } }, "reference_not_found", 404],
  ];
  for (const [label, args, code, status] of errors) {
    const [s, l] = await ctx.both("update_layer", args);
    compareError(label, s, l, code, status);
  }
  // **★ 已记录的分歧 ✗ ★**：服务端 changeset 是**逐条提交** ✓ ⇒ 第 1 个键已落盘、第 2 个键被拒 ✓
  //   ⇒ **半写**；本地**先全查后全写** ✓ ⇒ 整批拒绝 ✓（**不照抄半写 ✓**）。
  //   **∴ 先把 L1 的 visible 置回 true ✓**（两边都写 ✓），让半写有可观测的差异 ✓。
  const [sBack, lBack] = await ctx.both("update_layer", { layer_id: "L1", patch: { visible: true } });
  compareSuccess("§E17 归位 visible", sBack, lBack, ["count"]);
  const [sHalf, lHalf] = await ctx.both("update_layer",
    { layer_id: "L1", patch: { visible: false, opacity: "x" } });
  compareError("§E18 半写：两边都 400", sHalf, lHalf, "invalid_argument", 400);
  const [sLayers, lLayers] = await ctx.both("list_layers", {});
  const sVis = sLayers.body && (sLayers.body.layers || []).find((x) => x.layer_id === "L1");
  const lVis = lLayers.body && (lLayers.body.layers || []).find((x) => x.layer_id === "L1");
  check(sVis && sVis.visible === false && lVis && lVis.visible === true,
    `§E19 半写分歧（已记录）：服务端先落 visible=false，本地整批拒绝 ⇒ 本地应仍为 true ✗ `
    + `服务端 ${show(sVis && sVis.visible)}｜本地 ${show(lVis && lVis.visible)}`);
}

if (bad.length) {
  console.log(`✗ 失败 ${bad.length} 条（通过 ${okCount}）：`);
  for (const line of bad) console.log("   " + line);
  process.exit(1);
}
console.log(`✓ tool-pwa-engine-capabilities：${okCount} 条断言全过`
  + "（能力表／混合模式／锁定／属性写入与服务端同答案 ✓）");
