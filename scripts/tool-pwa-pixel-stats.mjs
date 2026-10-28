#!/usr/bin/env node
// **判据：PWA 本地的 `analyze_region`／`estimate_dehaze`／`compare_with_reference`
// 与服务端**同一请求、同一答案** ✓**（台账「第 六百六十二 节」）。
//
// **∴ 它测什么 ✗**（**内核 ①：断言必须能证明自己能红 ✓**）：
//   同一个请求分别打到真服务端与本地 `api-local` ✓ ⇒ 比解析后的 JSON ✓
//   （整数严格、非整数 1e-12 相对误差 ✓；**不比字节** ✓）。
//   三条独立期望由**判据自己算** ✗（**∴ 不能只断「两边相等」**：两边同错时相等照样绿 ✓）：
//   ① **统计口径**：16³ 桶（计数 ＋ 桶内通道和）／整数除法回桶内均值／
//      f64 亮度 `0.299r+0.587g+0.114b` 按除以像素再除以 255／冷暖按 `r>b` 与 `b>r` ✓；
//   ② **裁剪口径 = 服务端 `clamp_region`** ✗（不是内核的 `clamp_to_canvas` ✓）：
//      负坐标 `x=-4,w=20` **只覆盖 0..16（16 列 ✓）** ⇒ 像素数 256 而不是 400 ✓
//      —— **∴ 这两个口径在负坐标上真的不同 ✓** ⇒ 判据自己算 256／192／64 ✓；
//   ③ **暗通道估计**：`take = ceil(n×0.001)`、`sum` 累加的是**暗通道 min** ✗ 不是红通道、
//      `dark_mean = 2688/3072 = 0.875`（f32 精确）⇒ `omega` 被 0.95 夹住 ✓。
//   ④ **参考图比较**：参考图**由本地渲染的裸像素现场编成 PNG** ✓ ⇒ 两边 ΔE 必须**恰为 0** ✓
//      （**∴ 这同时是**「本地渲染与服务端渲染逐像素一致**」的判据 ✓**）；
//      黑参考图 ⇒ 均值/极值都 > 0；尺寸不一致／没设／不是 PNG／**CRC 坏** ⇒ 四种如实作答 ✓。
//
// **∴ 前提 ✗**：需要**活着的服务端**（`run-criteria-local.sh` 起一个并把地址放进 argv[2] ✓）
//   ＋ **不过期的真 wasm 内核**（`web/wasm/` 是构建产物 ✓ ⇒ 旧了就 `exit(3)` ✓，
//   **前提不成立 ≠ 产品失败 ✓**；**陈旧内核会让每条像素判据去追上一版的缺陷 ✗**）。
//
// **∴ 变异（四层齐第④层 ✓ 逐条实测见台账「六百六十二」）**：
//   ① `sum` 累加红通道（不是暗通道 min）⇒ §H `dark_channel_mean` 0.875／1 ⇒ 红 ✓；
//   ② `p2cRenderRgba` 不先按 `clamp_region` 裁、直接把请求盒交给内核 ⇒ §D2 变 20×20 ⇒ 红 ✓；
//   ③ 亮度系数 0.299→0.298 ⇒ §A／§B `avg_brightness` 不同 ⇒ 红 ✓；
//   ④ 暖冷判据对调（`r<b`）⇒ §A `warm_pixels` 384／0 ⇒ 红 ✓；
//   ⑤ `omega` 不夹上限 ⇒ 本地 1.125 vs 服务端 0.95 ⇒ 红 ✓；
//   ⑥ 去掉参考图尺寸校验 ⇒ §I2 变 `ok:true`（且 `delta_e_mean:null`）⇒ 红 ✓；
//   ⑦ 解码不校验 CRC ⇒ §I6 本地 `ok:true` 而服务端「不是能解码的 PNG」⇒ 红 ✓；
//   ⑧ **去掉 `compare_with_reference` 的布尔类型校验** ⇒ §G3 本地 200 而服务端 400 ⇒ 红 ✓
//      （**∴ 这条守的是「静默降级形态 4」✗**：`"yes"` 静默当没请求 ✓）；
//   ⑨ **把 `omega` 输出改回 f32 最短小数**（`0.95`）⇒ 服务端 `0.949999988079071` ⇒ 红 ✓
//      （**∴ 守的是「`json!` 把 f32 拓宽成 f64」这条序列化口径 ✓**）；
//   ⑩ 不相交文案不补小数位（`100` 而不是 `100.0`）⇒ §E1 逐字不同 ⇒ 红 ✓；
//   ⑪ `region` 解析文案改短 ⇒ §F3 逐字不同 ⇒ 红 ✓；
//   ⑫ **把桩的 `put` 键退回 `id ?? hash ?? doc`**（漏 `prefs` 的 `key`）⇒
//      `set_reference` 回 `ok:true` 而比较端读不到 ⇒ §I1–§I6 全红 ✓
//      （**∴ 这条是**判据自身**的变异 ✗**：**∴ 桩不完备会造出「产品对、判据红」✓**）；
//   ⑬ **盲区写明 ✓**：本 fixture 的 air 是三通道 1 ⇒ **阻尼（`damp_air_chromaticity`）
//      在这一份输入上不可观测 ✗**（**∴ 它只在**非中性大气光**下才改数值 ✓**）。
process.on("uncaughtException", (error) => {
  console.error(String(error && error.stack || error).slice(0, 1200));
  process.exit(3);
});
import { pathToFileURL } from "node:url";
import { deflateSync } from "node:zlib";

const BASE = process.argv[2];
if (!BASE || !/^https?:\/\//.test(BASE)) {
  console.log("⊘ 不可判：argv[2] 缺服务端地址 ⇒ 请用 bash scripts/run-criteria-local.sh scripts/tool-pwa-pixel-stats.mjs 跑"
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
const check = (cond, msg) => { if (!cond) bad.push(msg); };
const UNIQ = "p" + Date.now().toString(36);
const SERVER_DOC = `psrv-${UNIQ}`;
const LOCAL_DOC = `ploc-${UNIQ}`;
const DOC_W = 64, DOC_H = 48;
const RECT = { x: 8, y: 8, w: 24, h: 16 };

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
  body: JSON.stringify({ doc_id: LOCAL_DOC, width: DOC_W, height: DOC_H }),
})).then((r) => r.json().catch(() => null));
if (!localCreated || localCreated.ok !== true) {
  console.log(`⊘ 不可判：本地建文档失败（${JSON.stringify(localCreated)?.slice(0, 160)}）⇒ 前提不成立，不是产品问题`);
  process.exit(3);
}

/** **比对器 ✓**（与 `tool-pwa-transform-object` 同一口径：断结构不断字节 ✓）。 */
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
const show = (value) => String(JSON.stringify(value)).slice(0, 260);
const pick = (body, keys) => {
  const out = {};
  for (const key of keys) out[key] = body ? body[key] : undefined;
  return out;
};
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
function compareError(label, srv, loc, status, code, expectedDetail) {
  if (!srv.body || !loc.body) { check(false, `${label}：有一边读不出 JSON ✗`); return; }
  check(srv.status === status, `${label}：服务端状态码应为 ${status} ✗ 实测 ${srv.status}｜${show(srv.body)}`);
  check(loc.status === status, `${label}：本地状态码应为 ${status} ✗ 实测 ${loc.status}｜${show(loc.body)}`);
  check(srv.body.error_code === code, `${label}：服务端错误码应为 ${code} ✗ 实测 ${srv.body.error_code} ⇒ 判据口径要跟着权威改`);
  check(loc.body.error_code === code, `${label}：本地错误码应为 ${code} ✗ 实测 ${loc.body.error_code}`);
  const sd = srv.body.context && srv.body.context.detail;
  const ld = loc.body.context && loc.body.context.detail;
  check(typeof ld === "string" && ld.length > 0, `${label}：本地没给 context.detail ⇒ 调用方读不到原因 ✗`);
  if (expectedDetail !== undefined) {
    check(sd === expectedDetail,
      `${label}：服务端文案与判据的期望不同 ✗（权威口径变了就改判据）\n     期望 ${expectedDetail}\n     实测 ${sd}`);
  }
  check(sd === ld, `${label}：错误文案不同 ✗\n     服务端 ${sd}\n     本地     ${ld}`);
}
/** **成功响应逐字段比 ✓**：先断两边 200 ＋ 服务端确有这些字段 ✓，再逐字段比对 ✓。 */
async function statsCase(label, tool, args, keys) {
  const [s, l] = await both(tool, args);
  // **∴ 服务端的 `estimate_dehaze` 载荷没有 `ok` 字段 ✗**（`read_estimate_dehaze` ✓）
  //   ⇒ **∴ 这里不能要求 `s.body.ok === true`**（那会把一条正确的实现判红 ✓）。
  check(s.status === 200 && s.body && s.body.ok !== false,
    `${label}：服务端应 200 ✗ 状态 ${s.status}｜${show(s.body)}`);
  check(l.status === 200 && l.body && l.body.ok === true,
    `${label}：本地应 200 ✗ 状态 ${l.status}｜${show(l.body)}`);
  if (!s.body || !l.body) { check(false, `${label}：有一边读不出 JSON ✗`); return null; }
  for (const key of keys) {
    check(key in s.body, `${label}：服务端响应里没有「${key}」✗ ⇒ 权威口径变了，要改判据`);
    check(same(s.body[key], l.body[key]),
      `${label}：字段「${key}」不同 ✗ 服务端 ${show(s.body[key])}｜本地 ${show(l.body[key])}`);
  }
  return [s.body, l.body];
}

// ── 公共前序：图层 ＋ 一个红色矩形（8,8,24,16）──────────────────────────────
await setup("建图层", "create_layer", { layer_id: "L1" });
await setup("画红色矩形 S1", "draw_shape", {
  layer_id: "L1", object_id: "S1",
  data: { geometry: { kind: "rect", bbox: RECT }, color: [1, 0, 0, 1] },
});

const ANALYZE_KEYS = ["ok", "width", "height", "pixels", "comparison_with_reference",
  "dominant_colors", "avg_brightness", "warm_pixels", "cool_pixels", "warm_cool_ratio"];

// ══ §A 整幅：统计口径的独立期望 ══
// 白底 64×48 = 3072 像素，其中 24×16 = 384 是红矩形 ⇒ 白 2688（share 0.875）。
{
  const got = await statsCase("§A 整幅 analyze_region", "analyze_region",
    { region: { x: 0, y: 0, w: DOC_W, h: DOC_H } }, ANALYZE_KEYS);
  if (got) {
    for (const [who, body] of [["服务端", got[0]], ["本地", got[1]]]) {
      check(body.width === DOC_W && body.height === DOC_H && body.pixels === DOC_W * DOC_H,
        `§A（${who}）宽高/像素数应为 ${DOC_W}/${DOC_H}/${DOC_W * DOC_H} ✗ 实测 ${show([body.width, body.height, body.pixels])}`);
      const dom = Array.isArray(body.dominant_colors) ? body.dominant_colors : [];
      check(dom.length === 2,
        `§A（${who}）应只有两个桶（白 2688 ＋ 红 384）✗ 实测 ${dom.length} 个｜${show(dom)}`);
      if (dom.length >= 2) {
        const [first, second] = dom;
        check(first.count === 2688 && same(first.share, 2688 / 3072),
          `§A（${who}）最大桶应是白底 2688（share 0.875）✗ 实测 ${show(first)}`);
        check(second.count === 384 && same(second.share, 384 / 3072),
          `§A（${who}）第二桶应是红矩形 384（share 0.125）✗ 实测 ${show(second)}`);
        check(second.r >= 250 && second.g <= 5 && second.b <= 5 && second.hex === "#ff0000",
          `§A（${who}）红矩形的桶内均值应≈#ff0000 ✗ 实测 ${show(second)}`);
      }
      // **∴ 冷暖 ✗**：白底 r==b ⇒ 不进任何一边 ✓；只有矩形记暖 ✓。
      check(body.warm_pixels === 384 && body.cool_pixels === 0 && body.warm_cool_ratio === 1,
        `§A（${who}）暖/冷应为 384/0/1 ✗ 实测 ${show([body.warm_pixels, body.cool_pixels, body.warm_cool_ratio])}`);
      // **∴ 亮度 ✗**：白 = 1、红 ≈ 0.299 ⇒ 均值 ≈ (2688 + 384×0.299)/3072 ≈ 0.912375 ✓。
      check(body.avg_brightness > 0.90 && body.avg_brightness < 0.93,
        `§A（${who}）avg_brightness 应≈0.9124 ✗ 实测 ${body.avg_brightness}`);
    }
  }
}

// ══ §B 只框住红矩形：均匀色 ⇒ 单桶、share 1、只算暖 ══
{
  const got = await statsCase("§B 矩形区域", "analyze_region", { region: RECT }, ANALYZE_KEYS);
  if (got) {
    for (const [who, body] of [["服务端", got[0]], ["本地", got[1]]]) {
      check(body.width === RECT.w && body.height === RECT.h && body.pixels === RECT.w * RECT.h,
        `§B（${who}）宽高/像素数应为 ${RECT.w}/${RECT.h}/${RECT.w * RECT.h} ✗ 实测 ${show([body.width, body.height, body.pixels])}`);
      const dom = Array.isArray(body.dominant_colors) ? body.dominant_colors : [];
      check(dom.length === 1 && dom[0].count === RECT.w * RECT.h && dom[0].share === 1,
        `§B（${who}）应只有一个桶且 share=1 ✗ 实测 ${show(dom)}`);
      check(body.avg_brightness > 0.25 && body.avg_brightness < 0.35,
        `§B（${who}）纯红亮度应≈0.299 ✗ 实测 ${body.avg_brightness}`);
      check(body.warm_pixels === RECT.w * RECT.h && body.cool_pixels === 0 && body.warm_cool_ratio === 1,
        `§B（${who}）应全暖 ✗ 实测 ${show([body.warm_pixels, body.cool_pixels, body.warm_cool_ratio])}`);
    }
  }
}

// ══ §C 数组形态 `[x,y,w,h]` 与对象形态同解 ══
{
  const got = await statsCase("§C 数组形态 region", "analyze_region",
    { region: [RECT.x, RECT.y, RECT.w, RECT.h] }, ["width", "height", "pixels", "warm_pixels"]);
  if (got) {
    for (const [who, body] of [["服务端", got[0]], ["本地", got[1]]]) {
      check(body.width === RECT.w && body.height === RECT.h && body.pixels === RECT.w * RECT.h,
        `§C（${who}）数组形态解析应等于对象形态 ✗ 实测 ${show([body.width, body.height, body.pixels])}`);
    }
  }
}

// ══ §D 裁剪口径 = 服务端 `clamp_region` ══
// D1 小数盒 {8.5,8.5,23.5,15.5}：floor(8.5)=8、ceil(32)=32、ceil(24)=24 ⇒ **恰好**又是 24×16 ✓。
{
  const got = await statsCase("§D1 小数盒取整", "analyze_region",
    { region: { x: 8.5, y: 8.5, w: 23.5, h: 15.5 } }, ["width", "height", "pixels", "warm_pixels", "dominant_colors"]);
  if (got) {
    for (const [who, body] of [["服务端", got[0]], ["本地", got[1]]]) {
      check(body.width === 24 && body.height === 16 && body.pixels === 384 && body.warm_pixels === 384,
        `§D1（${who}）floor/ceil 后应等于 24×16 全红 ✗ 实测 ${show([body.width, body.height, body.pixels, body.warm_pixels])}`);
    }
  }
}
// D2 负坐标 {-4,-4,20,20}：x0=0、x1=ceil(16)=16 ⇒ **16×16（256 像素）**，其中左上 8×8 白、右下 8×8 红 ✓
//    **∴ 内核自己的 `clamp_to_canvas` 会把 x 挪到 0 却保留 w=20 ⇒ 20×20 ✗** ⇒ 这一条专门守那个分歧 ✓。
{
  const got = await statsCase("§D2 负坐标裁剪", "analyze_region",
    { region: { x: -4, y: -4, w: 20, h: 20 } }, ANALYZE_KEYS);
  if (got) {
    for (const [who, body] of [["服务端", got[0]], ["本地", got[1]]]) {
      check(body.width === 16 && body.height === 16 && body.pixels === 256,
        `§D2（${who}）负坐标应裁成 16×16=256 像素（服务端 clamp_region 口径）✗ 实测 ${show([body.width, body.height, body.pixels])}`
        + " ⇒ 很像把请求盒原样交给了内核（clamp_to_canvas 会保留 w=20）");
      const dom = Array.isArray(body.dominant_colors) ? body.dominant_colors : [];
      const byCount = dom.slice().sort((a, b) => b.count - a.count);
      check(byCount.length === 2 && byCount[0].count === 192 && byCount[1].count === 64,
        `§D2（${who}）16×16 里应是白 192 ＋ 红 64 ✗ 实测 ${show(dom)}`);
      check(body.warm_pixels === 64 && body.cool_pixels === 0,
        `§D2（${who}）暖像素应 64 ✗ 实测 ${show([body.warm_pixels, body.cool_pixels])}`);
    }
  }
}

// ══ §E 不相交 ⇒ invalid_argument，文案逐字（含 Rust Debug 的小数位）══
{
  const detail = (bbox) => "渲染区域 Bbox { x: " + bbox.x.toFixed(1) + ", y: " + bbox.y.toFixed(1)
    + ", w: " + bbox.w.toFixed(1) + ", h: " + bbox.h.toFixed(1) + " } 与文档 " + DOC_W + "×" + DOC_H + " 不相交";
  const cases = [
    ["§E1 区域在画布右侧外", { x: 100, y: 0, w: 10, h: 10 }],
    ["§E2 零面积", { x: 0, y: 0, w: 0, h: 10 }],
    ["§E3 小数负坐标仍不相交", { x: -10.5, y: -2, w: 10, h: 10 }],
  ];
  for (const [label, region] of cases) {
    const [s, l] = await both("analyze_region", { region });
    compareError(label, s, l, 400, "invalid_argument", detail(region));
  }
}

// ══ §F region 缺失／畸形 ══
{
  const f1 = await both("analyze_region", {});
  compareError("§F1 缺 region", f1[0], f1[1], 400, "invalid_argument", "缺少必填参数 region");
  // **∴ 字符串走 `require_object` ⇒ 也是「缺少必填参数」✗ 不是「必须是 {x,y,w,h}」✓**。
  const f2 = await both("analyze_region", { region: "abc" });
  compareError("§F2 region 是字符串", f2[0], f2[1], 400, "invalid_argument", "缺少必填参数 region");
  const f3 = await both("analyze_region", { region: { x: 1 } });
  compareError("§F3 region 缺 y/w/h", f3[0], f3[1], 400, "invalid_argument",
    "region 必须是 {x,y,w,h} 或 [x,y,w,h]");
  const f4 = await both("analyze_region", { region: [1, 2, 3] });
  compareError("§F4 region 数组只有 3 项", f4[0], f4[1], 400, "invalid_argument",
    "region 必须是 {x,y,w,h} 或 [x,y,w,h]");
  // **∴ 非数字项**（服务端 `as_f64` 失败 ⇒ 解析失败）✓
  const f5 = await both("analyze_region", { region: { x: "1", y: 2, w: 3, h: 4 } });
  compareError("§F5 region 含非数字", f5[0], f5[1], 400, "invalid_argument",
    "region 必须是 {x,y,w,h} 或 [x,y,w,h]");
}

// ══ §G compare_with_reference：缺省/null 不比较；非布尔值 400（不是静默当 false）══
// **∴ 服务端 `ParamKind::Boolean` 严格 ✗**：`"yes"` **在派发前就被拒** ✓
//   （`validate_args` ⇒「参数 compare_with_reference 类型应为 boolean，实际为 string」✓）。
//   **∴ 本地旧写法 `=== true` 会静默当「没请求」✗** —— **∴ 那是静默降级形态 4 ✗**，
//   **∴ 现在**两边都必须 400 且文案逐字相同 ✓。
for (const [label, flag] of [["§G1 缺省", undefined], ["§G2 显式 null", null]]) {
  const args = flag === undefined ? { region: RECT } : { region: RECT, compare_with_reference: flag };
  const got = await statsCase(label, "analyze_region", args, ["comparison_with_reference"]);
  if (got) {
    for (const [who, body] of [["服务端", got[0]], ["本地", got[1]]]) {
      check(body.comparison_with_reference === null,
        `${label}（${who}）未明确请求时 comparison_with_reference 应为 null ✗ 实测 ${show(body.comparison_with_reference)}`);
    }
  }
}
{
  const g3 = await both("analyze_region", { region: RECT, compare_with_reference: "yes" });
  compareError("§G3 compare_with_reference 是字符串", g3[0], g3[1], 400, "invalid_argument",
    "参数 compare_with_reference 类型应为 boolean，实际为 string");
  const g4 = await both("analyze_region", { region: RECT, compare_with_reference: 1 });
  compareError("§G4 compare_with_reference 是数字", g4[0], g4[1], 400, "invalid_argument",
    "参数 compare_with_reference 类型应为 boolean，实际为 number");
}

// ══ §H estimate_dehaze：暗通道口径的独立期望 ══
// 白 2688 像素暗通道 1、红 384 像素暗通道 0 ⇒ take=ceil(3072×0.001)=4 ⇒ 全白 ⇒ air=[1,1,1]；
// dark_mean = 2688/3072 = **0.875**（f32 精确）；omega = clamp(0.6+0.875×0.6, 0.6, 0.95) = **0.95** ✓。
{
  const got = await statsCase("§H estimate_dehaze", "estimate_dehaze", {},
    ["air", "dark_channel_mean", "suggested", "note"]);
  if (got) {
    for (const [who, body] of [["服务端", got[0]], ["本地", got[1]]]) {
      check(same(body.air, [1, 1, 1]),
        `§H（${who}）air 应为 [1,1,1]（前 4 个暗通道最大者是白）✗ 实测 ${show(body.air)}`);
      check(body.dark_channel_mean === 0.875,
        `§H（${who}）dark_channel_mean 应为 0.875 ✗ 实测 ${show(body.dark_channel_mean)}`
        + " ⇒ 很像累加了红通道而不是暗通道 min（白/红两色的红通道都是 255 ⇒ 均值会变 1）");
      // **∴ omega 是 **f32 拓宽成 f64** 的数 ✗**（**服务端 `json!` ⇒ `Value::from(f32)` ✓**）
      //   ⇒ **∴ 期望值是** `Math.fround(0.95)` 这条 f64**✗**（`0.949999988079071` ✓），
      //   **∴ 不是**字面量 `0.95` ✗（那是 f32 最短往返小数的口径 ✓）。
      check(body.suggested && body.suggested.omega === Math.fround(0.95) && body.suggested.floor === 0.1,
        `§H（${who}）suggested 应为 {omega:${Math.fround(0.95)}, floor:0.1} ✗ 实测 ${show(body.suggested)}`);
      check(body.note === "把 air/omega/floor 传给 add_filter {filter_name: \"dehaze\"}；渲染逐像素确定性",
        `§H（${who}）note 文案不同 ✗ 实测 ${show(body.note)}`);
    }
  }
}

// ══ §I 参考图比较 ══
// **∴ 参考图怎么来 ✗**：先用**本地内核**渲染矩形区域拿**裸 RGBA** ✓ ⇒ 现场编成 PNG ✓
//   ⇒ 上传到**两边**（同一串字节 ⇒ 内容寻址 ⇒ 同一个 hash ✓）⇒ `set_reference` ✓
//   ⇒ 再 `analyze_region{compare_with_reference:true}` ⇒ **两边 ΔE 都必须恰为 0** ✓
//   （**∴ 它同时守着**「本地渲染 == 服务端渲染」✗ 逐像素 ✓）。
const CRC_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = (c & 1) ? (0xEDB88320 ^ (c >>> 1)) : (c >>> 1);
    table[n] = c >>> 0;
  }
  return table;
})();
function crc32(bytes) {
  let c = 0xFFFFFFFF;
  for (let i = 0; i < bytes.length; i++) c = (CRC_TABLE[(c ^ bytes[i]) & 0xFF] ^ (c >>> 8)) >>> 0;
  return (c ^ 0xFFFFFFFF) >>> 0;
}
function pngChunk(kind, payload) {
  const head = Buffer.alloc(8);
  head.writeUInt32BE(payload.length, 0);
  head.write(kind, 4, "latin1");
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(Buffer.concat([head.subarray(4), payload])), 0);
  return Buffer.concat([head, payload, crc]);
}
function pngFromRaw(w, h, rgba) {
  const stride = w * 4;
  const raw = Buffer.alloc((stride + 1) * h);
  for (let y = 0; y < h; y++) {
    raw[y * (stride + 1)] = 0;
    Buffer.from(rgba.buffer || rgba, rgba.byteOffset || 0, stride).copy(raw, y * (stride + 1) + 1);
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0); ihdr.writeUInt32BE(h, 4);
  ihdr[8] = 8; ihdr[9] = 6; ihdr[10] = 0; ihdr[11] = 0; ihdr[12] = 0;
  return Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    pngChunk("IHDR", ihdr),
    pngChunk("IDAT", deflateSync(raw)),
    pngChunk("IEND", Buffer.alloc(0)),
  ]);
}
function solidPng(w, h, rgba) {
  const pixels = new Uint8Array(w * h * 4);
  for (let i = 0; i < w * h; i++) pixels.set(rgba, i * 4);
  return pngFromRaw(w, h, pixels);
}
/** **往 IEND 之前塞一个 CRC 故意的 `tEXt` 块 ✓**（解压不受影响 ⇒ 只有 CRC 校验能挡 ✓）。 */
function pngWithBadCrc(png) {
  const bad = pngChunk("tEXt", Buffer.from("k\0v", "latin1"));
  bad.writeUInt32BE((bad.readUInt32BE(bad.length - 4) ^ 1) >>> 0, bad.length - 4);
  const cut = png.length - 12; // 最后一块就是 IEND（12 字节）✓
  return Buffer.concat([png.subarray(0, cut), bad, png.subarray(cut)]);
}
const uploadBoth = async (label, bytes) => {
  const sRes = await nativeFetch(`${BASE}/api/blob?doc=${encodeURIComponent(SERVER_DOC)}&token=${TOKEN}`, {
    method: "POST", headers: { "content-type": "application/octet-stream" }, body: bytes,
  });
  const sBody = await sRes.json().catch(() => null);
  const lRes = await localApi(new Request(`https://x/api/blob?doc=${LOCAL_DOC}`, {
    method: "POST", headers: { "content-type": "application/octet-stream" }, body: bytes,
  }));
  const lBody = await lRes.json().catch(() => null);
  check(sRes.status === 200 && sBody && sBody.ok === true,
    `${label}：服务端上传 blob 失败 ✗ ${sRes.status}｜${show(sBody)}`);
  check(lRes.status === 200 && lBody && lBody.ok === true,
    `${label}：本地上传 blob 失败 ✗ ${lRes.status}｜${show(lBody)}`);
  const sHash = sBody && sBody.blob_hash, lHash = lBody && lBody.blob_hash;
  // **∴ 内容寻址同源 ✓**：同一串字节两边必须得到同一个 hash ✓（SHA-256 前缀 `sha256:` ✓）。
  check(typeof sHash === "string" && sHash === lHash,
    `${label}：两边的 blob_hash 不同 ✗ 服务端 ${show(sHash)}｜本地 ${show(lHash)}`
    + " ⇒ 内容寻址的哈希口径分裂");
  return [sHash, lHash];
};
const setReference = async (label, sHash, lHash) => {
  const s = await callServer("set_reference", { blob_hash: sHash });
  const l = await localCall("set_reference", { blob_hash: lHash });
  check(s.status === 200 && s.body && s.body.ok === true,
    `${label}：服务端 set_reference 失败 ✗ ${s.status}｜${show(s.body)}`);
  check(l.status === 200 && l.body && l.body.ok === true,
    `${label}：本地 set_reference 失败 ✗ ${l.status}｜${show(l.body)}`);
};
/** **一次比较 ✓**：两边都 200、`comparison_with_reference` 逐字段相同 ✓。 */
async function compareCase(label, region, inspect) {
  const got = await statsCase(label, "analyze_region",
    { region, compare_with_reference: true }, ["comparison_with_reference"]);
  if (!got) return;
  const [s, l] = [got[0].comparison_with_reference, got[1].comparison_with_reference];
  check(same(s, l), `${label}：两边比较结果不同 ✗ 服务端 ${show(s)}｜本地 ${show(l)}`);
  if (typeof inspect === "function") inspect(s, "服务端");
  if (typeof inspect === "function") inspect(l, "本地");
}

// I1 自渲染参考图 ⇒ ΔE 恰为 0（同时守「本地渲染 == 服务端渲染」✓）
let zeroPngHash = null;
{
  const res = await localApi(new Request(`https://x/api/tools/render_region?doc=${LOCAL_DOC}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ region: RECT, raw: true }),
  }));
  const raw = new Uint8Array(await res.arrayBuffer());
  check(res.status === 200 && raw.length === RECT.w * RECT.h * 4,
    `§I1 取本地裸像素失败 ✗ 状态 ${res.status}｜字节 ${raw.length}（应 ${RECT.w * RECT.h * 4}）`);
  if (res.status === 200 && raw.length === RECT.w * RECT.h * 4) {
    const png = pngFromRaw(RECT.w, RECT.h, raw);
    const [sHash, lHash] = await uploadBoth("§I1 上传自渲染参考图", png);
    zeroPngHash = png;
    await setReference("§I1 设参考图", sHash, lHash);
    await compareCase("§I1 自渲染参考图", RECT, (cmp, who) => {
      check(cmp && cmp.ok === true, `§I1（${who}）应 ok:true ✗ 实测 ${show(cmp)}`);
      check(cmp && cmp.delta_e_mean === 0 && cmp.delta_e_max === 0,
        `§I1（${who}）ΔE 应恰为 0（参考图就是这块的渲染）✗ 实测 ${show(cmp)}`
        + " ⇒ 本地与服端的渲染不是逐像素一致");
      check(cmp && same(cmp.delta_e_max_at, [0, 0]) && same(cmp.reference, { width: RECT.w, height: RECT.h }),
        `§I1（${who}）delta_e_max_at/reference 不对 ✗ 实测 ${show(cmp)}`);
    });
  }
}

// I2 尺寸不一致 ⇒ 如实说“比不了”（**∴ 不是给 0 ✓**）
{
  const expect = `参考图 ${RECT.w}x${RECT.h} 与区域 ${DOC_W}x${DOC_H} 尺寸不一致 ⇒ 无法逐像素比`;
  await compareCase("§I2 尺寸不一致", { x: 0, y: 0, w: DOC_W, h: DOC_H }, (cmp, who) => {
    check(cmp && cmp.ok === false && cmp.reason === expect,
      `§I2（${who}）应如实拒绝比较 ✗ 期望 ${show(expect)}｜实测 ${show(cmp)}`);
  });
}

// I3 黑参考图 ⇒ ΔE > 0（结构：非零且有上界），两边逐位一致 ✓
{
  const png = solidPng(RECT.w, RECT.h, [0, 0, 0, 255]);
  const [sHash, lHash] = await uploadBoth("§I3 上传黑参考图", png);
  await setReference("§I3 设黑参考图", sHash, lHash);
  await compareCase("§I3 黑参考图", RECT, (cmp, who) => {
    check(cmp && cmp.ok === true, `§I3（${who}）应 ok:true ✗ 实测 ${show(cmp)}`);
    check(cmp && cmp.delta_e_mean > 0 && cmp.delta_e_max > 0,
      `§I3（${who}）红 vs 黑 ΔE 应 > 0 ✗ 实测 ${show(cmp)}`);
    check(cmp && cmp.delta_e_max >= cmp.delta_e_mean,
      `§I3（${who}）恒等式 delta_e_max ≥ delta_e_mean 不成立 ✗ 实测 ${show(cmp)}`);
  });
}

// I4 没设参考图 ⇒ 如实说（**∴ 两种说法都不许静默给 0 ✓**）
{
  const c = await both("clear_reference", {});
  check(c[0].status === 200 && c[0].body && c[0].body.ok === true,
    `§I4 服务端 clear_reference 失败 ✗ ${c[0].status}｜${show(c[0].body)}`);
  check(c[1].status === 200 && c[1].body && c[1].body.ok === true,
    `§I4 本地 clear_reference 失败 ✗ ${c[1].status}｜${show(c[1].body)}`);
  await compareCase("§I4 没有参考图", RECT, (cmp, who) => {
    check(cmp && cmp.ok === false && cmp.reason === "没有参考图 ⇒ 先用 set_reference 设一张",
      `§I4（${who}）应如实说没有参考图 ✗ 实测 ${show(cmp)}`);
  });
}

// I5 参考图不是 PNG ⇒ 如实说
{
  const [sHash, lHash] = await uploadBoth("§I5 上传非 PNG", Buffer.from([1, 2, 3, 4, 5]));
  await setReference("§I5 设非 PNG 参考图", sHash, lHash);
  await compareCase("§I5 不是 PNG", RECT, (cmp, who) => {
    check(cmp && cmp.ok === false && cmp.reason === "参考图不是能解码的 PNG",
      `§I5（${who}）应如实说解不开 ✗ 实测 ${show(cmp)}`);
  });
}

// I6 CRC 坏块 ⇒ 服务端拒收整张图（**∴ 只有逐块校验 CRC 才挡得住 ✓**）
if (zeroPngHash) {
  const png = pngWithBadCrc(zeroPngHash);
  const [sHash, lHash] = await uploadBoth("§I6 上传 CRC 坏图", png);
  await setReference("§I6 设 CRC 坏图", sHash, lHash);
  await compareCase("§I6 CRC 坏", RECT, (cmp, who) => {
    check(cmp && cmp.ok === false && cmp.reason === "参考图不是能解码的 PNG",
      `§I6（${who}）CRC 不符的 PNG 应被拒 ✗ 实测 ${show(cmp)}`
      + " ⇒ 很像解码时跳过了 CRC 校验");
  });
}

if (bad.length) {
  console.log(`✗ PWA 本地 analyze_region／estimate_dehaze 与服务端同口径：${bad.length} 条不符`);
  for (const m of bad) console.log("   · " + m);
  process.exit(1);
}
console.log("✓ PWA 本地 analyze_region／estimate_dehaze／compare_with_reference 与服务端同口径"
  + "（16³ 桶统计／服务端 clamp_region 裁剪（负数与小数的独立期望）／"
  + "七条报错逐字（含 Rust Debug 的小数位）／暗通道 min 累加与 omega 夹取／"
  + "参考图四态（自渲染 ΔE=0／黑图 >0／没设／非 PNG）＋ CRC 坏块拒收）");
