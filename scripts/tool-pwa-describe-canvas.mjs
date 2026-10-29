#!/usr/bin/env node
// **判据：PWA 本地的 `describe_canvas`／`delete_mask`／`create_mask` 重复守卫与服务端同答案** ✓（台账 §672）。
//
// **∴ 它测什么 ✗**（**内核 ①：断言必须能证明自己能红 ✓**）：
//   同一个请求分别打到真服务端与本地 `api-local` ✓ ⇒ 比解析后的 JSON ✓（断结构不断字节 ✓）。
//   `describe_canvas` 的响应里 **RSS 两量是服务端-rooted** ✓（`server_rss_mb`／
//   `bytes_per_atom_avg`）⇒ **比对时剥掉** ✓，本地那两份**单独断**它的哨兵值 ✓
//   （`null`／`0` = 不可判 ⇒ 编个数字才是撒谎 ✓）。
//   **∴ 三件事必须同时成立 ✗**：
//   ① **全字段逐字节结构一致**（layers 的十个字段含 `kind`≡`type` 别名 ✓、
//      selections **过滤墓碑** ✓、masks **不过滤** ✓ —— 服务端就是这么露出已删蒙版的 ⇒ 照抄 ✗ 不顺手修好 ✓）；
//   ② **期望值由判据自己按输入算** ✗（每层对象数、`estimated = 活对象×180`、
//      selection/mask 的 mode/feather/invert/shape 回显、错误文案逐字）
//      —— **∴ 不能只断「两边相等」**：两边同错时相等断言照样绿 ✓ ⇒ 安慰剂 ✓；
//   ③ **恒等式** ✓：`counts.layers === layers.length`（selections／masks 同理）✓、
//      `Σ layers[].objects × 180 === estimated_memory_bytes` ✓、`kind === type` ✓
//      —— 每边各自验 ⇒ 「组合错」一次封住 ✓。
//
// **∴ 覆盖的面 ✗→✓**：
//   §A 空文档（默认层身份／零对象／零估计）；§B 层与对象计数（建层、跨层移动、删对象）；
//   §C `delete_layer` 级联（**层上的对象随层死 ⇒ 估计量变小 ✓** —— 这正是本地折叠里
//      `killLayer` 那段的存在理由 ✗ 漏了它 §C 红 ✓）；§D 选区（回显＋墓碑过滤）；
//   §E 蒙版生命周期（建 → 重复 409「蒙版 M_B 已存在」→ 删 → **墓碑仍列出** →
//      **重删 404「引用的蒙版 M_B 不存在」**（权威是 `log.rs` 提交期引用检查，不是列候选形 ✗）
//      → 重建被允许【墓碑不占 id】→ 删未知 404 同一句）；
//   §F 入口校验（未知键／缺 mask_id／类型错 ⇒ 400 逐字 ✓）。
//
// **∴ 前提 ✗**：需要**活着的服务端**（`run-criteria-local.sh` 起一个并把地址放进 argv[2] ✓）。
//   拿不到 ⇒ `exit(3)`（判据前提不成立 ≠ 产品失败 ✓）。服务端拒绝前序 ⇒ 同样 `exit(3)` ✓。
//
// **∴ 变异（逐条对应第 N 层断言 ✓，实测记录见台账「第 六百七十二 节」）**：
//   ① `estimated_memory_bytes` 恒 0 ⇒ §B／§C 恒等式红 ✓；
//   ② selections 去掉墓碑过滤 ⇒ §D 两边不一致红 ✓；
//   ③ masks 加上墓碑过滤（顺手「修好」服务端怪癖）⇒ §E「删后仍列出」红 ✓；
//   ④ 去掉 `create_mask` 重复守卫 ⇒ §E 重复 409 本地 200 红 ✓；
//   ⑤ 去掉 `delete_mask` 存在性检查 ⇒ §E 重删／删未知 404 变 200 红 ✓；
//   ⑥ `delete_mask` 把「已删」也当存在（`!existing` 才 404）⇒ §E 重删红 ✓；
//   ⑦ `killLayer` 不级联对象 ⇒ §C 恒等式与层缺席断言红 ✓；
//   ⑧ `countInLayer` 无视 layer_id（数全部）⇒ §B 每层期望红 ✓；
//   ⑨ 去掉 `kind` 别名 ⇒ §A/§B 全字段比对红 ✓；
//   ⑩ masks 列表漏掉按 id 升序（用插入序）⇒ §E8 红 ✓
//     （**∴ 建序刻意反着起 M_B 先／M_A 后 ⇒ 插入序与升序不同 ✓**；
//      **原计划的「`mask_id ?? null` 改裸值」实测是**假变异**✗：`p2cState` 本就初始化 null ⇒ 行为不变 ⇒ 弃用 ✓**）。
process.on("uncaughtException", (error) => {
  console.error(String(error && error.stack || error).slice(0, 1200));
  process.exit(3);
});
import { pathToFileURL } from "node:url";

const BASE = process.argv[2];
if (!BASE || !/^https?:\/\//.test(BASE)) {
  console.log("⊘ 不可判：argv[2] 缺服务端地址 ⇒ 请用 bash scripts/run-criteria-local.sh scripts/tool-pwa-describe-canvas.mjs 跑"
    + "（判据前提不成立，不是产品问题）");
  process.exit(3);
}

const bad = [];
let okCount = 0;
const check = (cond, msg) => { if (cond) okCount += 1; else bad.push(msg); };
const UNIQ = "dc" + Date.now().toString(36);
const SERVER_DOC = `dcsrv-${UNIQ}`;
const LOCAL_DOC = `dcloc-${UNIQ}`;

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
/** **本地抛异常记成产品失败 ✗ 不是判据坏了 ✓**（与 `tool-pwa-transform-object` 同一纪律 ✓）。 */
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

const show = (value) => String(JSON.stringify(value)).slice(0, 260);
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
/** **服务端-rooted 的环境字段 ✗ 剥掉再比 ✓**：`created/opened/timings` 是 HTTP 信封 ✓，
 *  `server` 是本地标记 ✓，**`server_rss_mb`／`bytes_per_atom_avg` 是服务端进程的真测量** ✗
 *  （本地没有服务端进程 ⇒ 哨兵值单独断 ✓，**编数字才是撒谎 ✗**）。
 *  **`ms_per_stroke_avg` 同属计时派生量**：服务端按真耗时算，本地桩无计时 ⇒ 剥掉再比 ✓，
 *  服务端出字段本身由 tools_tests 回执测试守住，不由本判据逐字节比 ✓。 */
const ENV_KEYS = ["created", "opened", "timings", "server", "server_rss_mb", "bytes_per_atom_avg", "ms_per_stroke_avg"];
const strip = (body) => {
  const out = { ...(body || {}) };
  for (const key of ENV_KEYS) delete out[key];
  return out;
};
const detailOf = (res) => (res.body && res.body.context && res.body.context.detail) ?? null;

function firstDiff(a, b, path = "") {
  if (same(a, b)) return null;
  if (a && b && typeof a === "object" && typeof b === "object") {
    const keys = Array.from(new Set([...Object.keys(a), ...Object.keys(b)])).sort();
    for (const key of keys) {
      if (!(key in a)) return `${path}.${key}：服务端缺（本地 ${show(b[key])}）`;
      if (!(key in b)) return `${path}.${key}：本地缺（服务端 ${show(a[key])}）`;
      const hit = firstDiff(a[key], b[key], `${path}.${key}`);
      if (hit) return hit;
    }
    return null;
  }
  if (Array.isArray(a) && Array.isArray(b) && a.length === b.length) {
    for (let i = 0; i < a.length; i += 1) {
      const hit = firstDiff(a[i], b[i], `${path}[${i}]`);
      if (hit) return hit;
    }
    return null;
  }
  return `${path}：服务端 ${show(a)}｜本地 ${show(b)}`;
}
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
/** **错误逐字比 ✓**（服务端是权威 ⇒ 它的状态码／错误码／文案都是期望本身 ✓）。 */
function compareError(label, srv, loc, status, code, expectDetail) {
  if (!srv.body || !loc.body) { check(false, `${label}：有一边读不出 JSON ✗`); return; }
  check(srv.status === status,
    `${label}：服务端状态码应为 ${status} ✗ 实测 ${srv.status}｜${show(srv.body)} ⇒ 权威口径变了，要改判据`);
  check(loc.status === status, `${label}：本地状态码应为 ${status} ✗ 实测 ${loc.status}｜${show(loc.body)}`);
  check(srv.body.error_code === code, `${label}：服务端错误码应为 ${code} ✗ 实测 ${srv.body.error_code}`);
  check(loc.body.error_code === code, `${label}：本地错误码应为 ${code} ✗ 实测 ${loc.body.error_code}`);
  const sd = detailOf(srv), ld = detailOf(loc);
  check(typeof ld === "string" && ld.length > 0, `${label}：本地没给 context.detail ⇒ 调用方读不到原因 ✗`);
  check(sd === ld, `${label}：错误文案不同 ✗\n     服务端 ${sd}\n     本地     ${ld}`);
  if (expectDetail !== undefined) {
    check(sd === expectDetail,
      `${label}：服务端文案应为 ${expectDetail} ✗ 实测 ${sd} ⇒ 权威口径变了，要改判据`);
    check(ld === expectDetail, `${label}：本地文案应为 ${expectDetail} ✗ 实测 ${ld}`);
  }
}
/** **一次成功变换… 不 —— 一次 `describe_canvas` 全量比对 ✓**：
 *  全字段结构一致 ✓ ＋ **每边各自**过三条恒等式 ✓ ＋ 本地哨兵值 ✓ ＋ 可选的独立期望 ✓。 */
async function describeCase(label, expect) {
  const [s, l] = await both("describe_canvas", {});
  check(s.status === 200 && s.body && s.body.ok === true,
    `${label}：服务端应 200 ✗ 状态 ${s.status}｜${show(s.body)}`);
  check(l.status === 200 && l.body && l.body.ok === true,
    `${label}：本地应 200 ✗ 状态 ${l.status}｜${show(l.body)}`);
  if (!s.body || !l.body) { check(false, `${label}：有一边读不出 JSON ✗`); return null; }
  const vs = strip(s.body), vl = strip(l.body);
  check(same(vs, vl), `${label}：全字段不一致 ✗ 首个差异 ${firstDiff(vs, vl)}`);
  for (const [who, b] of [["服务端", vs], ["本地", vl]]) {
    check(b.layers.length === b.counts.layers,
      `${label}（${who}）counts.layers 应等于 layers 数组长度 ✗ ${b.counts.layers} vs ${b.layers.length}`);
    check(b.selections.length === b.counts.selections,
      `${label}（${who}）counts.selections 应等于 selections 数组长度 ✗`);
    check(b.masks.length === b.counts.masks,
      `${label}（${who}）counts.masks 应等于 masks 数组长度 ✗`);
    check(b.layers.every((x) => x.kind === x.type),
      `${label}（${who}）每个图层都该有 kind ≡ type 别名 ✗ ${show(b.layers)}`);
    const sum = b.layers.reduce((n, x) => n + x.objects, 0);
    check(sum * 180 === b.estimated_memory_bytes,
      `${label}（${who}）Σ对象×180 应等于 estimated_memory_bytes ✗ Σ=${sum} est=${b.estimated_memory_bytes}`);
    check(Number.isInteger(b.counts.atoms) && b.counts.atoms >= 1,
      `${label}（${who}）counts.atoms 应为 ≥1 的整数 ✗ ${show(b.counts.atoms)}`);
    if (expect) expect(who, b);
  }
  check(l.body.server_rss_mb === null,
    `本地应报哨兵 null（服务端-rooted 测量不可判）✗ 实测 ${show(l.body.server_rss_mb)}`);
  check(l.body.bytes_per_atom_avg === 0,
    `本地应报哨兵 0（同上）✗ 实测 ${show(l.body.bytes_per_atom_avg)}`);
  return vs;
}
const rectData = {
  geometry: { kind: "rect", bbox: { x: 0, y: 0, w: 4, h: 4 } },
  color: [1, 0, 0, 1], stroke_width: 0,
};
const objsOf = (b) => Object.fromEntries(b.layers.map((x) => [x.layer_id, x.objects]));
const idsOf = (b) => b.layers.map((x) => x.layer_id);

// ══ §A 空文档：默认层身份 ＋ 零对象 ＋ 零估计 ══
await describeCase("§A 空文档", (who, b) => {
  check(b.width === 400 && b.height === 300,
    `${who} 尺寸应为 400×300 ✗ ${b.width}×${b.height}`);
  check(b.layers.length === 1 && b.layers[0].layer_id === "layer_default"
    && b.layers[0].objects === 0 && b.layers[0].mask_id === null
    && b.layers[0].visible === true && b.layers[0].locked === false,
    `${who} 默认层身份不对 ✗ ${show(b.layers)}`);
  check(b.estimated_memory_bytes === 0 && b.selections.length === 0 && b.masks.length === 0,
    `${who} 空文档应为零对象／零选区／零蒙版 ✗ ${show(b.counts)}`);
});

// ══ §B 层与对象计数：建层、跨层移动、删对象 ══
await setup("建层 LA", "create_layer", { layer_id: "LA" });
await setup("建层 LB", "create_layer", { layer_id: "LB" });
await setup("O0 默认层", "draw_shape", { layer_id: "layer_default", object_id: "O0", data: rectData });
await setup("O1 层 LA", "draw_shape", { layer_id: "LA", object_id: "O1", data: rectData });
await setup("O2 层 LA", "draw_shape", { layer_id: "LA", object_id: "O2", data: rectData });
await setup("O3 层 LB", "draw_shape", { layer_id: "LB", object_id: "O3", data: rectData });
// 移动 O2 ⇒ 期望 layer_default:1 ｜ LA:1 ｜ LB:2 ⇒ 4 个活对象 ×180 = 720 ✓。
await setup("把 O2 移到 LB", "set_property", { object_id: "O2", key: "layer_id", value: "LB" });
await describeCase("§B 移动后计数", (who, b) => {
  check(JSON.stringify(objsOf(b)) === JSON.stringify({ layer_default: 1, LA: 1, LB: 2 }),
    `${who} 每层对象数应为 默认1／LA1／LB2 ✗ ${show(objsOf(b))}`);
  check(b.estimated_memory_bytes === 720, `${who} 4 个活对象应估 720 ✗ ${b.estimated_memory_bytes}`);
});
// 删 O1 ⇒ LA:0 ｜ 3 个活对象 ⇒ 540 ✓（恒等式随删随验 ✓）。
await setup("删 O1", "delete_object", { object_id: "O1" });
await describeCase("§B2 删对象后计数", (who, b) => {
  check(JSON.stringify(objsOf(b)) === JSON.stringify({ layer_default: 1, LA: 0, LB: 2 }),
    `${who} 删 O1 后应为 默认1／LA0／LB2 ✗ ${show(objsOf(b))}`);
  check(b.estimated_memory_bytes === 540, `${who} 3 个活对象应估 540 ✗ ${b.estimated_memory_bytes}`);
});

// ══ §C `delete_layer` 级联：层没了 ⇒ 层上的对象也随层死 ⇒ 估计量变小 ══
// **∴ 这一段守的是本地折叠里 `killLayer` 的存在理由 ✗**：对象没有自己的墓碑 ✓
//   （服务端是 `tombstone_layer` 给它们盖 `deleted_by` ⇒ 级联在删层那一刻发生 ✓）
//   ⇒ **若本地漏了级联 ⇒ 层没了、对象计数还挂在估计里 ✗ ⇒ §C 恒等式与期望双双红 ✓**。
await setup("删层 LB", "delete_layer", { layer_id: "LB" });
await describeCase("§C 删层级联", (who, b) => {
  check(!idsOf(b).includes("LB"), `${who} LB 应从图层列表消失 ✗ ${show(idsOf(b))}`);
  check(b.counts.layers === 2, `${who} 删层后应剩 2 层 ✗ ${b.counts.layers}`);
  check(JSON.stringify(objsOf(b)) === JSON.stringify({ layer_default: 1, LA: 0 }),
    `${who} LB 上的 2 个对象应随层死（估计里看不见）✗ ${show(objsOf(b))}`);
  check(b.estimated_memory_bytes === 180,
    `${who} 级联后只该剩 O0 一个活对象 ⇒ 180 ✗ ${b.estimated_memory_bytes}`);
});

// ══ §D 选区：mode／feather／invert／shape 回显 ＋ 墓碑过滤 ══
const SEL_SHAPE = { kind: "rect", bbox: { x: 1, y: 2, w: 3, h: 4 } };
await setup("建选区 SA", "create_selection", {
  selection_id: "SA", shape: SEL_SHAPE, feather: 2.5, invert: true,
});
await setup("建选区 SB", "create_selection", { selection_id: "SB", shape: SEL_SHAPE });
await describeCase("§D 两个选区", (who, b) => {
  check(b.selections.length === 2, `${who} 应列出 2 个选区 ✗ ${show(b.selections)}`);
  const sa = b.selections.find((x) => x.selection_id === "SA");
  check(sa && sa.mode === "new" && same(sa.feather, 2.5) && sa.invert === true
    && same(sa.shape, SEL_SHAPE),
    `${who} SA 应回显 mode:new（缺省）／feather:2.5／invert:true／shape 逐字 ✗ ${show(sa)}`);
  const sb = b.selections.find((x) => x.selection_id === "SB");
  check(sb && sb.feather === 0 && sb.invert === false && sb.mode === "new",
    `${who} SB 缺省应是 new／0／false ✗ ${show(sb)}`);
});
await setup("删选区 SB", "delete_selection", { selection_id: "SB" });
// **∴ 选区删了就不列 ✓**（服务端有 `!is_deleted` 过滤 ⇒ 蒙版没有 ⇒ 两段口径**故意不同** ✓）
await describeCase("§D2 选区删除后", (who, b) => {
  check(b.selections.length === 1 && b.selections[0].selection_id === "SA",
    `${who} 已删选区不应再列出 ✗ ${show(b.selections)}`);
});

// ══ §E 蒙版生命周期（含三个服务端怪癖的照抄 ✓） ══
const MASK_SHAPE = { kind: "rect", bbox: { x: 5, y: 5, w: 10, h: 10 } };
await setup("建蒙版 M_B", "create_mask", { mask_id: "M_B", shape: MASK_SHAPE });
await describeCase("§E 建蒙版后", (who, b) => {
  check(b.masks.length === 1 && b.masks[0].mask_id === "M_B"
    && b.masks[0].mode === "new" && b.masks[0].feather === 0
    && same(b.masks[0].shape, MASK_SHAPE),
    `${who} M_B 应回显缺省 new／0 与逐字 shape ✗ ${show(b.masks)}`);
  check(b.masks.length === 0 || !("invert" in b.masks[0]),
    `${who} masks 项不应有 invert（服务端就没有 ✗ 照抄 ✓）`);
});
// E2 重复建 ⇒ 409「蒙版 M_B 已存在」✓（**服务端无 mode 门 ⇒ 一律拒 ✗ 与选区的「new 才撞」不同 ✓**）。
{
  const [s, l] = await both("create_mask", { mask_id: "M_B", shape: MASK_SHAPE });
  compareError("§E2 重复建蒙版", s, l, 409, "precondition_failed", "蒙版 M_B 已存在");
}
await setup("建蒙版 M_A", "create_mask", { mask_id: "M_A", shape: MASK_SHAPE });
// E3 删 M_B ⇒ 200 ✓，**而它继续被列出**（服务端 masks 不过滤 `is_deleted` ⇒ 照抄 ✓）：
//   **∴ 为什么断「继续列出」而不是「消失」✗**：消失才是**分叉**（两边不一致 ⇒ §E4 比对红 ✓）。
await setup("删蒙版 M_B", "delete_mask", { mask_id: "M_B" });
await describeCase("§E3 删蒙版后仍列出", (who, b) => {
  check(b.masks.length === 2 && b.masks.some((x) => x.mask_id === "M_B"),
    `${who} 已删蒙版**应仍被列出**（服务端就是这么露出的 ✗ 顺手修好=分叉 ✓）${show(b.masks)}`);
});
// E5 重删已删的 ⇒ 404 ✓。**权威文案实测是提交期引用检查（`log.rs:565`）那一句 ✓**
//   —— **不是** `fold.rs check_auxiliary_refs` 的「列候选」形 ✗（**前者先跑 ⇒ 后者永远轮不到 ✓**）。
{
  const [s, l] = await both("delete_mask", { mask_id: "M_B" });
  compareError("§E5 重删已删蒙版", s, l, 404, "reference_not_found",
    "引用的蒙版 M_B 不存在");
}
// E6 **墓碑不占 id** ⇒ 删过的 M_B 可以重建 ✓（`fold.rs` 的守卫是 `mask_alive` 不是 `contains_key` ✓）。
await setup("重建 M_B", "create_mask", { mask_id: "M_B", shape: MASK_SHAPE });
// E7 删未知 ⇒ 404 ✓ **从未存在与已删同一句** ✓（`is_none_or` ⇒ 不区分 ✓）。
{
  const [s, l] = await both("delete_mask", { mask_id: "NOPE" });
  compareError("§E7 删未知蒙版", s, l, 404, "reference_not_found",
    "引用的蒙版 NOPE 不存在");
}
await describeCase("§E8 蒙版终态", (who, b) => {
  check(b.masks.length === 2 && b.masks[0].mask_id === "M_A" && b.masks[1].mask_id === "M_B",
    `${who} 终态应恰好**按 id 升序**列出 M_A／M_B（建序是反的 ⇒ 漏 sort 就露馅 ✓） ✗ ${show(b.masks)}`);
});

// ══ §F 入口校验（逐字比 ✓） ══
{
  const f1 = await both("describe_canvas", { foo: 1 });
  compareError("§F1 描述未知键", f1[0], f1[1], 400, "invalid_argument");
  const f2 = await both("delete_mask", {});
  compareError("§F2 删蒙版缺 mask_id", f2[0], f2[1], 400, "invalid_argument");
  const f3 = await both("delete_mask", { mask_id: 5 });
  compareError("§F3 删蒙版 mask_id 非字符串", f3[0], f3[1], 400, "invalid_argument");
  const f4 = await both("delete_mask", { mask_id: "M_B", extra: 1 });
  compareError("§F4 删蒙版未知键", f4[0], f4[1], 400, "invalid_argument");
}

// ══ §G 全字段终检：整个文档在「层＋对象＋选区＋蒙版」都齐时仍逐字段一致 ══
{
  const b = await describeCase("§G 终态全量", (who, x) => {
    check(x.selections.length === 1 && x.masks.length === 2 && x.counts.layers === 2,
      `${who} 终态三类实体数不对 ✗ ${show(x.counts)}`);
  });
  if (b) console.log(`  终态：layers=${b.counts.layers} selections=${b.counts.selections} masks=${b.counts.masks} atoms=${b.counts.atoms} est=${b.estimated_memory_bytes}`);
}

if (bad.length) {
  console.log(`✗ 本地描述/蒙版与服务端不一致 ✗ ${bad.length} 条（另 ${okCount} 条过 ✓）`);
  for (const line of bad) console.log("   · " + line);
  process.exit(1);
}
console.log(`✓ describe_canvas／delete_mask／create_mask 守卫：与服务端同答案 ✓（${okCount} 条断言全过 ✓）`);
