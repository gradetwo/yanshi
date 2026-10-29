#!/usr/bin/env node
// **判据：PWA 本地 `preview_preset{render}` ⇒ 必须与服务端**渲同一张图 ✓ ★**（第 六百七十五 节）。
//
// **∴ 它测什么 ✗**（**内核 ①：断言必须能证明自己能红 ✓**）：
//   这条端点以前是**诚实 501**✗，理由写着「本地没有临时文档、没有单层裸像素通道」✓
//   **∴ 而那个理由是错的**✗（**单层裸像素在本地早已成立 ✓ `p2LayerScopeAtoms`；
//     「临时文档」= 第二个内核实例 ✓**）⇒ 所以补成了真实现 ⇒ **两份实现**（Rust 的
//   `render_preview_image` ／ JS 的 `p2PresetRenderPreview`）⇒ 唯一可信的说法还是「同一输入同一输出」✓
//   ⇒ **∴ 这里不比代码 ✗ 比像素 ✓**（**解码后逐字节比 ✓ 不是比 base64 ✓**——PNG 编码本身
//     两边一致，但**断「解析后的像素」才与产品语义同层 ✓**）。
//
// **∴ 数字分两类 ✓**（AGENTS「判据里的数字分两类」✓）：文档尺寸／`max_edge`／采样点是**判据自己造的输入**✓
//   能写死；`image_width`／`image_height`／`ops_applied` 是**被测系统决定的**✗ ⇒ **一律与服务端比 ✓**
//   （**权威在对方 ⇒ 判据不写死它 ✓**）。
//
// **∴ 像素容差为什么存在 ✗**（**不是「差不多就行」✓**）：
//   · **纯 adjustment 的预设 ⇒ 断**逐字节全等**（实测 0 处不同 ✓）**；
//   · **含 noise／film_grain 的预设 ⇒ 断「不同字节 ≤ 0.5% ∧ 单通道最大差 ≤ 2」**（实测 81/480000、差 1 ✓）
//     ——那 1 位来自 f32 舍入落在 8bit 量化边界上 ✗ 不是算法不同 ✓
//   ⇒ **∴ 所以容差只给「已知来源的那一类」✓ 不是全局放宽 ✓**（**全局放宽＝安慰剂 ✗**）。
//
// **∴ 前提 ✗**：需要**活着的服务端**（`run-criteria-local.sh` 起并把地址放进 argv[2] ✓）
//   **＋ 真内核 wasm**（`kernelStaleness` ✓）。拿不到 ⇒ `exit(3)`（判据前提不成立 ≠ 产品失败 ✓）。
//
// **∴ 变异（四层齐第④层 ✓ 逐条实测 ⇒ 全部退出码 1 ✓）**：改法与实测退出码记在台账「第 六百七十五 节」✓
//   （**⇒ 变异脚本是一次性临时工具 ✗ 跑完就删 ⇒ 所以这里只指台账，不指一个将来不存在的文件名 ✓**）
//   **⇒ 每条变异改完先过 ESM 语法闸 ✗ 否则「语法崩」会伪装成「断言红」✓（台账「第 六百七十四 节」的规矩 ✓）**。
process.on("uncaughtException", (error) => {
  console.error(String(error && error.stack || error).slice(0, 1200));
  process.exit(3);
});
import { pathToFileURL } from "node:url";

const BASE = process.argv[2];
if (!BASE || !/^https?:\/\//.test(BASE)) {
  console.log("⊘ 不可判：argv[2] 缺服务端地址 ⇒ 请用 bash scripts/run-criteria-local.sh scripts/tool-pwa-preset-render-parity.mjs 跑"
    + "（判据前提不成立，不是产品问题）");
  process.exit(3);
}
const kernelLib = await import(pathToFileURL(process.cwd() + "/scripts/lib/pwa-node-kernel.mjs").href);
const staleness = kernelLib.kernelStaleness();
if (!staleness.fresh) {
  console.log(`⊘ 不可判：${staleness.reason}`);
  for (const line of staleness.stale) console.log("   · " + line);
  process.exit(3);
}

const bad = [];
const check = (cond, msg) => { if (!cond) bad.push(msg); };
const show = (v) => JSON.stringify(v)?.slice(0, 240);
const UNIQ = "pr" + Date.now().toString(36);
const SERVER_DOC = `prs-${UNIQ}`;
const LOCAL_DOC = `prl-${UNIQ}`;
const DOC_W = 400, DOC_H = 300;
const IMG_W = 40, IMG_H = 30;
const INK = { x: 60, y: 40, w: IMG_W, h: IMG_H };

const boot = await kernelLib.bootLocalWithRealKernel({ origin: BASE });
const localApi = boot.local;
const { decodePng } = await import(pathToFileURL(process.cwd() + "/scripts/lib/png.mjs").href);
const nativeFetch = globalThis.fetch;

const createdBody = await nativeFetch(`${BASE}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: SERVER_DOC, width: DOC_W, height: DOC_H }),
}).then((r) => r.json());
if (createdBody.ok !== true || !createdBody.token) {
  console.log(`⊘ 不可判：服务端建文档失败（${show(createdBody)}）⇒ 前提不成立，不是产品问题`);
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
/** **本地抛异常记成产品失败 ✗ 不是判据坏了 ✓**（与其它 PWA 判据同一纪律 ✓）。 */
const callLocal = async (tool, args) => {
  try {
    const res = await localApi(new Request(`https://x/api/tools/${tool}?doc=${LOCAL_DOC}`, {
      method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}),
    }));
    return { status: res.status, body: await res.json().catch(() => null) };
  } catch (error) {
    return { status: 0, body: { ok: false, error: "local_threw", reason: String((error && error.message) || error) } };
  }
};
const localDoc = await localApi(new Request(`https://x/api/documents?doc=${LOCAL_DOC}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: LOCAL_DOC, width: DOC_W, height: DOC_H }),
})).then((r) => r.json().catch(() => null));
if (!localDoc || localDoc.ok !== true) {
  console.log(`⊘ 不可判：本地建文档失败（${show(localDoc)}）⇒ 前提不成立，不是产品问题`);
  process.exit(3);
}

// ---- 装配：两边**同一串字节**的位图（代理图的输入必须同源 ✓）----
const raw = Buffer.alloc(IMG_W * IMG_H * 4);
for (let y = 0; y < IMG_H; y++) for (let x = 0; x < IMG_W; x++) {
  const i = (y * IMG_W + x) * 4;
  raw[i] = Math.round(x * 255 / (IMG_W - 1));
  raw[i + 1] = Math.round(y * 255 / (IMG_H - 1));
  raw[i + 2] = 128;
  raw[i + 3] = 255;
}
const sUp = await nativeFetch(`${BASE}/api/blob?doc=${encodeURIComponent(SERVER_DOC)}&token=${TOKEN}`,
  { method: "POST", headers: { "content-type": "application/octet-stream" }, body: raw }).then((r) => r.json());
const lUp = await localApi(new Request(`https://x/api/blob?doc=${LOCAL_DOC}`,
  { method: "POST", headers: { "content-type": "application/octet-stream" }, body: raw })).then((r) => r.json());
check(sUp.ok === true && lUp.ok === true, `两边的 blob 上传必须都成功 ✗ 服务端 ${show(sUp)}｜本地 ${show(lUp)}`);
// **内容寻址同源 ✓**（同一串字节 ⇒ 同一个哈希 ⇒ 代理图的字节口径没有分裂 ✓）
check(typeof sUp.blob_hash === "string" && sUp.blob_hash === lUp.blob_hash,
  `两边上传同一串字节却拿到不同哈希 ✗ 服务端 ${show(sUp.blob_hash)}｜本地 ${show(lUp.blob_hash)}`);
const LAYER = "layer_default";
const seedSrv = await callServer("import_image", {
  layer_id: LAYER, bitmap: { blob_hash: sUp.blob_hash, mime_type: "image/x-yanshi-raw", size: raw.length },
  region: { x: INK.x, y: INK.y, w: INK.w, h: INK.h },
});
const seedLoc = await callLocal("import_image", {
  layer_id: LAYER, bitmap: { blob_hash: lUp.blob_hash, mime_type: "image/x-yanshi-raw", size: raw.length },
  region: { x: INK.x, y: INK.y, w: INK.w, h: INK.h },
});
check(seedSrv.body && seedSrv.body.ok === true && seedLoc.body && seedLoc.body.ok === true,
  `种子图（往源图层导入位图）没成功 ✗ 服务端 ${show(seedSrv.body)}｜本地 ${show(seedLoc.body)} ⇒ 输入没造出来，后面的断言全是安慰剂`);

// ---- 取两边的预设库并挑两类：**纯 adjustment**（断全等）／**含 noise 或 film_grain**（断容差）----
const libSrv = await callServer("list_presets", {});
const libLoc = await callLocal("list_presets", {});
const ids = (libSrv.body && libSrv.body.presets || []).map((p) => p.preset_id);
check(Array.isArray(ids) && ids.length > 0, `服务端预设库为空 ✗（实测 ${show(libSrv.body)}）⇒ 前提不成立`);
check(libLoc.body && Array.isArray(libLoc.body.presets) && libLoc.body.presets.length === ids.length,
  `本地预设库套数与服务端不同 ✗ 服务端 ${ids.length}｜本地 ${show(libLoc.body && libLoc.body.presets && libLoc.body.presets.length)}`);
/** `preview_preset`（不带 render）⇒ 拿展开出来的 ops（**不写状态 ✓**）。 */
const opsOf = async (pid) => {
  const r = await callLocal("preview_preset", { preset_id: pid });
  return (r.body && Array.isArray(r.body.ops)) ? r.body.ops : null;
};
let pureId = null, grainId = null;
for (const pid of ids) {
  const ops = await opsOf(pid);
  if (!ops) continue;
  const hasGrain = ops.some((o) => o.kind === "filter" && (o.type === "noise" || o.type === "film_grain"));
  const onlyAdjust = ops.every((o) => o.kind === "adjustment");
  if (!pureId && onlyAdjust) pureId = pid;
  if (!grainId && hasGrain) grainId = pid;
  if (pureId && grainId) break;
}
check(!!pureId, "⊘ 输入造不出来：库里没有「全是 adjustment」的预设 ⇒ 无法断**逐字节全等**（不是放宽，是缺输入）");
check(!!grainId, "⊘ 输入造不出来：库里没有含 noise／film_grain 的预设 ⇒ 无法断那一条容差");
if (!pureId || !grainId) {
  console.log("⊘ 不可判：预设库里缺上面两类输入 ⇒ 判据无法制造它要检查的情况（前提不成立，不是产品问题）");
  process.exit(3);
}

// ---- 比对器 ----
/** **像素差 ✗**：返回 `{不同字节, 最大单通道差}`（**尺寸/通道不同 ⇒ 直接判红 ✓ 不比内容 ✓**）。 */
const pixelDiff = (a, b) => {
  if (!a || !b) return null;
  if (a.width !== b.width || a.height !== b.height || a.channels !== b.channels) {
    return { sizeMismatch: [a.width, a.height, a.channels, b.width, b.height, b.channels] };
  }
  let diff = 0, max = 0;
  for (let i = 0; i < a.data.length; i++) {
    const d = Math.abs(a.data[i] - b.data[i]);
    if (d) { diff++; if (d > max) max = d; }
  }
  return { diff, max, total: a.data.length };
};
/** **同一份 preview 请求打到两边 ✗**（**断的是**结构字段＋像素 ✓**）。 */
const previewBoth = async (label, args, opts) => {
  const srv = await callServer("preview_preset", args);
  const loc = await callLocal("preview_preset", args);
  if (opts && opts.expectFail === true) return [srv, loc];
  check(srv.status === 200 && loc.status === 200,
    `${label}：状态不是 200 ✗ 服务端 ${srv.status}｜本地 ${loc.status}（${show(srv.body)}｜${show(loc.body)}）`);
  check(!!srv.body && srv.body.ok === true && !!loc.body && loc.body.ok === true,
    `${label}：有一边 ok≠true ✗ 服务端 ${show(srv.body && srv.body.context)}｜本地 ${show(loc.body && (loc.body.reason || loc.body.context))}`);
  // **∴ 结构字段：`image_width`／`image_height`／`ops_applied`／`scale_notes` 两边必须相等 ✓**
  //   （**这些数量由被测系统决定 ⇒ 判据不写死 ✗ 与服务端比 ✓**）。
  for (const key of ["image_width", "image_height", "ops_applied", "scale_notes"]) {
    check(srv.body && loc.body && srv.body[key] === loc.body[key],
      `${label}：字段「${key}」两边不一致 ✗ 服务端 ${show(srv.body && srv.body[key])}｜本地 ${show(loc.body && loc.body[key])}`);
  }
  // **∴ 键集：本地多给的键必须是**写明白的那三个 ✓**（**少给服务端有的键 ⇒ 红 ✓**）。
  const sKeys = new Set(Object.keys(srv.body || {}));
  const extra = Object.keys(loc.body || {}).filter((k) => !sKeys.has(k) && k !== "name_zh" && k !== "preset_id");
  check(extra.every((k) => ["server", "source_layer", "proxy", "format_fallback"].includes(k)),
    `${label}：本地给了服务端没有的键 ${show(extra)} ⇒ 要么补进服务端口径，要么这里显式允许 ✓`);
  for (const k of ["image_base64", "image_format", "warnings"]) {
    check(Object.prototype.hasOwnProperty.call(loc.body || {}, k),
      `${label}：本地响应缺键「${k}」✗（服务端有 ⇒ 本地少给＝虚报少给 ✓）`);
  }
  if (!srv.body || !loc.body || typeof srv.body.image_base64 !== "string" || typeof loc.body.image_base64 !== "string") {
    return [srv, loc];
  }
  const imgSrv = decodePng(Buffer.from(srv.body.image_base64, "base64"));
  const imgLoc = decodePng(Buffer.from(loc.body.image_base64, "base64"));
  const dpx = pixelDiff(imgSrv, imgLoc);
  check(!dpx || !dpx.sizeMismatch,
    `${label}：两边解出来的图几何不同 ✗ ${show(dpx && dpx.sizeMismatch)}`);
  if (dpx && !dpx.sizeMismatch) {
    if (opts && opts.exact) {
      check(dpx.diff === 0,
        `${label}：这一类输入**没有理由**不同 ✗ 不同字节 ${dpx.diff}/${dpx.total}｜最大单通道差 ${dpx.max}`
        + "（纯 adjustment 的预设两边都是同一条定点路 ⇒ 容差会掩盖真 bug ✓）");
    } else {
      check(dpx.diff <= dpx.total * 0.005 && dpx.max <= 2,
        `${label}：超出「f32 舍入落在 8bit 量化边界」的量级 ✗ 不同字节 ${dpx.diff}/${dpx.total}｜最大差 ${dpx.max}`
        + "（那是 noise 的取整差 ⇒ 再大就不是取整 ✓）");
    }
  }
  return [srv, loc];
};

// **① 纯 adjustment 的预设 ⇒ 逐字节全等 ✗**（**这是最强的一条 ✓ 没有容差 ✓**）。
const [pure1Srv, pure1Loc] = await previewBoth(`全等（${pureId}@png512）`,
  { preset_id: pureId, layer_id: LAYER, render: { max_edge: 512, format: "png" } }, { exact: true });
check(pure1Srv.body.image_format === "png" && pure1Loc.body.image_format === "png",
  `显式要 png 时两边格式必须都是 png ✗ 服务端 ${show(pure1Srv.body.image_format)}｜本地 ${show(pure1Loc.body.image_format)}`);
check(Array.isArray(pure1Loc.body.warnings) && pure1Loc.body.warnings.length === 0,
  `本地的 warnings 必须是空数组 ✗（**键在 ✓ 但内容要说清**——本地没有「临时文档删失败」这一类状态 ✓）实测 ${show(pure1Loc.body.warnings)}`);
// **∴ 尺寸 = 文档尺寸（`max_edge` 512 > 400 ⇒ 不降采样 ✓**——**判据自己造的输入 ⇒ 能写死 ✓**）。
check(pure1Srv.body.image_width === DOC_W && pure1Srv.body.image_height === DOC_H,
  `不降采样时代理尺寸应当等于文档尺寸 ✗ 服务端给的是 ${pure1Srv.body.image_width}×${pure1Srv.body.image_height}（文档 ${DOC_W}×${DOC_H}）`);

// **② 效果必须**落到像素上 ✗**（**这条才是抓住「合成原子被折叠层丢弃」的那一条 ✓**）：
//   **∴ 为什么不能只比两边 ✓**：上面那次实例复用的 bug ⇒ 两边**都**返回陈旧图时比不出东西 ✗
//   （实测：本地第二条预览拿到的是**第一条**的图，而服务端那时是对的 ✓）
//   ⇒ **∴ 要断「同一边的 strength 0 与 strength 1 必须不同」✗ 并且两边的差值相等 ✓**。
const decode = (r) => decodePng(Buffer.from(r.body.image_base64, "base64"));
const SAMPLE = { x: INK.x + (INK.w >> 1), y: INK.y + (INK.h >> 1) };
const at = (img, x, y) => {
  const o = (y * img.width + x) * img.channels;
  return [img.data[o], img.data[o + 1], img.data[o + 2]];
};
const luma = (v) => v[0] + v[1] + v[2];
const [g0Srv, g0Loc] = await previewBoth(`容差（${grainId}@strength0）`,
  { preset_id: grainId, layer_id: LAYER, strength: 0, render: { max_edge: 512, format: "png" } }, {});
const [g1Srv, g1Loc] = await previewBoth(`容差（${grainId}@strength1）`,
  { preset_id: grainId, layer_id: LAYER, strength: 1, render: { max_edge: 512, format: "png" } }, {});
{
  const dSrv = Math.abs(luma(at(decode(g1Srv), SAMPLE.x, SAMPLE.y)) - luma(at(decode(g0Srv), SAMPLE.x, SAMPLE.y)));
  const dLoc = Math.abs(luma(at(decode(g1Loc), SAMPLE.x, SAMPLE.y)) - luma(at(decode(g0Loc), SAMPLE.x, SAMPLE.y)));
  check(dSrv > 0, `服务端 strength 0→1 在墨区同一点上**没有任何变化** ✗（Δ=${dSrv}）⇒ 这一条输入证明不了「效果落到像素」`);
  check(dLoc === dSrv,
    `本地 strength 0→1 的亮度变化与服务端不等 ✗ 本地 ${dLoc}｜服务端 ${dSrv}（采样点 ${SAMPLE.x},${SAMPLE.y}）`
    + " ⇒ 合成的 adjustment/filter 原子**根本没进渲染**（折叠层丢弃／z 序把它压到位图下面 ✓）");
}

// **③ 连续两次预览 ⇒ 第二次不许拿到第一次的陈旧状态 ✗**（**实例复用的直接判据 ✓**）：
//   **∴ 输入怎么造 ✓**：**同一个代理尺寸**（同一 scratchId ✓）**换一套预设**（像素必然不同 ✓）⇒
//   **∴ 复用实例 ⇒ 第二次返回第一次的图 ⇒ 与服务端不等 ✓ 必红 ✓**（实测：修之前正是这样 ✗）。
{
  const otherId = ids.find((pid) => pid !== grainId && pid !== pureId) || pureId;
  const [a0] = await previewBoth(`复用（${grainId} 第二次）`,
    { preset_id: grainId, layer_id: LAYER, render: { max_edge: 512, format: "png" } }, {});
  const [, b1] = await previewBoth(`复用（${otherId} 夹在中间）`,
    { preset_id: otherId, layer_id: LAYER, render: { max_edge: 512, format: "png" } }, {});
  const [a1] = await previewBoth(`复用（${grainId} 第三次）`,
    { preset_id: grainId, layer_id: LAYER, render: { max_edge: 512, format: "png" } }, {});
  check(a0.body.image_base64 === a1.body.image_base64,
    `同一请求两次预览的 PNG 字节不同 ✗ ⇒ 渲染状态被上一次污染（临时文档实例必须一次一用 ✓）`);
  check(a1.body.image_base64 !== b1.body.image_base64,
    `夹在中间那套预设（${otherId}）渲出来与 ${grainId} 一模一样 ✗（那说明第二次拿到的是陈旧状态，不是自己的图）`);
  void b1;
}

// **④ 盒式降采样的口径 ✗**（**长边 128 ⇒ 400×300 → 128×96 ✓ 与服务端比 ✓ 不写死自己算的数 ✓**）。
{
  const [dSrv, dLoc] = await previewBoth(`降采样（${pureId}@128）`,
    { preset_id: pureId, layer_id: LAYER, render: { max_edge: 128, format: "png" } }, { exact: true });
  check(dSrv.body.image_width === 128 && dSrv.body.image_height === 96,
    `服务端在 max_edge=128 下的代理尺寸变了（实测 ${dSrv.body.image_width}×${dSrv.body.image_height}）✗`
    + " ⇒ 是**服务端**的口径变了 ⇒ 要改的是这条判据＋台账，不是偷偷放宽本地");
  check(dLoc.body.proxy.source_width === DOC_W && dLoc.body.proxy.source_height === DOC_H,
    `本地回带的 proxy.source_* 应当是**源文档**尺寸 ✗（实测 ${show(dLoc.body.proxy)}）⇒ 说明没读过源图层尺寸 ✓`);
  check(dLoc.body.proxy.max_edge === 128,
    `本地 proxy.max_edge 要回显请求里的那个数 ✗（实测 ${show(dLoc.body.proxy.max_edge)}）`);
}

// **⑤ JPEG 缺省 ⇒ 本地如实按 png 回带 ＋ 明说 ✗**（**唯一的真能力缺口 ✓ 不许假装 ✓**）。
{
  const srv = await callServer("preview_preset", { preset_id: pureId, layer_id: LAYER, render: { max_edge: 256 } });
  const loc = await callLocal("preview_preset", { preset_id: pureId, layer_id: LAYER, render: { max_edge: 256 } });
  check(srv.body && srv.body.ok === true && loc.body && loc.body.ok === true,
    `缺省 render 两边都要成功 ✗ 服务端 ${show(srv.body && srv.body.context)}｜本地 ${show(loc.body && loc.body.reason)}`);
  check(srv.body.image_format === "jpeg" && loc.body.image_format === "png",
    `缺省格式：服务端 jpeg／本地 png 的分工变了 ✗ 实测 ${show(srv.body.image_format)}｜${show(loc.body.image_format)}`);
  check(typeof loc.body.format_fallback === "string" && /JPEG/.test(loc.body.format_fallback),
    `本地按 png 回带时**必须**用 format_fallback 说清原因 ✗（缺了就是「假装是服务端那份」——静默降级形态③ ✓）实测 ${show(loc.body.format_fallback)}`);
  // **∴ 几何必须仍然相等 ✓**（**差的只是编码 ✗ 不是尺寸 ✗**）。
  check(srv.body.image_width === loc.body.image_width && srv.body.image_height === loc.body.image_height,
    `png 回带改变了代理几何 ✗ 服务端 ${srv.body.image_width}×${srv.body.image_height}｜本地 ${loc.body.image_width}×${loc.body.image_height}`);
}

// **⑥ render 参数的校验 ⇒ 两边同一句话 ✗**（**参数写错时报错必须一致 ✓ 否则 PWA 里的错误码与服务器不同 ✓**）。
const BAD_RENDERS = [
  ["max_edge 越界", { max_edge: 64 }],
  ["max_edge 不是数", { max_edge: "big" }],
  ["format 非法", { max_edge: 256, format: "tiff" }],
  ["未知键", { max_edge: 256, quality: 9 }],
  ["render 不是对象", 5],
];
for (const [label, render] of BAD_RENDERS) {
  const srv = await callServer("preview_preset", { preset_id: pureId, layer_id: LAYER, render });
  const loc = await callLocal("preview_preset", { preset_id: pureId, layer_id: LAYER, render });
  check(srv.status === loc.status, `${label}：状态不同 ✗ 服务端 ${srv.status}｜本地 ${loc.status}（body ${show(srv.body)}｜${show(loc.body)}）`);
  const sDetail = srv.body && srv.body.context && srv.body.context.detail;
  const lDetail = loc.body && loc.body.context && loc.body.context.detail;
  check(typeof sDetail === "string" && sDetail === lDetail,
    `${label}：错误文案不同 ✗ 服务端 ${show(sDetail)}｜本地 ${show(lDetail)}`);
  check(srv.status !== 200, `${label}：这一条输入两边都接受了 ✗（判据的输入没能触发拒绝 ⇒ 断言会恒绿 ✓）`);
}

// **⑦ 源图层「没有墨」的三种形态 ⇒ 服务端实测**全都 200 ＋ 一张底色图 ✓**（第 六百七十五 节实测纠正）。
//   **∴ 为什么会错 ✗**：`document.rs:1657` 那句 400 的**触发条件是渲染缓冲区为空**✗
//   **而单层渲染照样画出整幅底色 ⇒ 缓冲区永远非空 ✓**（**「图层不存在」走不到那句错误 ✗**）。
//   ⇒ **∴ 断言**：三种形态两边状态／ok／结构字段／像素**逐字节全等** ✓
//     ＋ **一条「它真的按图层筛过」的恒等式**：空层那张图必须**与有墨层那张不同** ✗
//     （**若本地把「藏掉其余活层」的合成分支丢了 ⇒ 空层会渲出有墨层的样子 ⇒ 这条红 ✓**）。
for (const label of ["从未有过的层", "存在但是空的层", "已删除的层"]) {
  const layerId = label === "从未有过的层" ? "layer_never_" + UNIQ
    : label === "存在但是空的层" ? "layer_empty" : "layer_gone";
  if (label !== "从未有过的层") {
    const cS = await callServer("create_layer", { layer_id: layerId, name: label });
    const cL = await callLocal("create_layer", { layer_id: layerId, name: label });
    check(cS.body && cS.body.ok === true && cL.body && cL.body.ok === true,
      `⊘ 输入造不出来：「${label}」建不出来 ✗ 服务端 ${show(cS.body)}｜本地 ${show(cL.body)}`);
    if (label === "已删除的层") {
      const dS = await callServer("delete_layer", { layer_id: layerId });
      const dL = await callLocal("delete_layer", { layer_id: layerId });
      check(dS.body && dS.body.ok === true && dL.body && dL.body.ok === true,
        `⊘ 输入造不出来：「${label}」删不掉 ✗ 服务端 ${show(dS.body)}｜本地 ${show(dL.body)}`);
    }
  }
  await previewBoth(`${label}（无墨层要渲成底色图）`,
    { preset_id: pureId, layer_id: layerId, render: { max_edge: 256, format: "png" } }, { exact: true });
}
// **∴ 「筛过层」那一条 ✗**：同一 `max_edge` 下，有墨层与空层的预览必须**不同** ✓。
{
  const inked = await callServer("preview_preset", { preset_id: pureId, layer_id: LAYER, render: { max_edge: 256, format: "png" } });
  const empty = await callServer("preview_preset", { preset_id: pureId, layer_id: "layer_empty", render: { max_edge: 256, format: "png" } });
  const inkedLoc = await callLocal("preview_preset", { preset_id: pureId, layer_id: LAYER, render: { max_edge: 256, format: "png" } });
  const emptyLoc = await callLocal("preview_preset", { preset_id: pureId, layer_id: "layer_empty", render: { max_edge: 256, format: "png" } });
  const four = [inked, empty, inkedLoc, emptyLoc];
  check(four.every((r) => r.body && r.body.ok === true && typeof r.body.image_base64 === "string"),
    `⊘ 不可判：这一节需要的四张图没都拿到 ✗ ${show(four.map((r) => r.status))}｜${show(four.map((r) => r.body && (r.body.context || r.body.reason)))}`);
  if (four.every((r) => r.body && typeof r.body.image_base64 === "string")) {
    const [iS, eS, iL, eL] = four.map((r) => decodePng(Buffer.from(r.body.image_base64, "base64")));
    const dInkEmptyS = pixelDiff(iS, eS);
    const dInkEmptyL = pixelDiff(iL, eL);
    check(dInkEmptyS && !dInkEmptyS.sizeMismatch && dInkEmptyS.diff > 0,
      `服务端**有墨层与空层渲出同一张图** ✗（不同字节 ${show(dInkEmptyS)}）⇒ 图层作用域没筛住，这一节的对照失效 ✓`);
    check(dInkEmptyL && !dInkEmptyL.sizeMismatch && dInkEmptyL.diff > 0,
      `本地**有墨层与空层渲出同一张图** ✗（不同字节 ${show(dInkEmptyL)}）⇒ 「藏掉其余活层」那条合成分支丢了（形态③：静默把整幅文档当成这一层 ✓）`);
    // **∴ 关系（不是单点 ✓）**：两边「筛住的幅度」应当是同一个量级（**全等已在 ⑦ 比过像素 ✓
    //   ⇒ 这里比的是「不同字节数」这个派生量相等 ✓**）。
    check(dInkEmptyS && dInkEmptyL && !dInkEmptyS.sizeMismatch && !dInkEmptyL.sizeMismatch
      && dInkEmptyS.diff === dInkEmptyL.diff,
      `两边「有墨 vs 无墨」的差异字节数不同 ✗ 服务端 ${show(dInkEmptyS && dInkEmptyS.diff)}｜本地 ${show(dInkEmptyL && dInkEmptyL.diff)}`);
  }
}

// **⑧ 预览**不写任何东西 ✗**（**形态③的守卫 ✓**：临时文档必须是**真的临时**——
//   用户文档的原子日志一条都不能多 ✓）。
{
  const sAtoms = await nativeFetch(`${BASE}/api/atoms?doc=${encodeURIComponent(SERVER_DOC)}&since=0&token=${TOKEN}`).then((r) => r.json());
  const lAtoms = await localApi(new Request(`https://x/api/atoms?doc=${LOCAL_DOC}`)).then((r) => r.json());
  check(Array.isArray(sAtoms.atoms) && Array.isArray(lAtoms.atoms),
    `⊘ 不可判：两边的原子日志读不出（${show(sAtoms)}｜${show(lAtoms)}）⇒ 前提不成立，不是产品问题`);
  const lBefore = lAtoms.atoms.length;
  const sBefore = sAtoms.atoms.length;
  const [, keepLoc] = await previewBoth(`日志不变（${pureId}@png512）`,
    { preset_id: pureId, layer_id: LAYER, render: { max_edge: 512, format: "png" } }, { exact: true });
  void keepLoc;
  const lAfter = await localApi(new Request(`https://x/api/atoms?doc=${LOCAL_DOC}`)).then((r) => r.json());
  const sAfter = await nativeFetch(`${BASE}/api/atoms?doc=${encodeURIComponent(SERVER_DOC)}&since=0&token=${TOKEN}`).then((r) => r.json());
  check(lAfter.atoms.length === lBefore,
    `本地预览往用户文档的日志里写了 ${lAfter.atoms.length - lBefore} 条原子 ✗（临时文档必须是**第二个实例**，不是**第二个文档**✓）`);
  check(sAfter.atoms.length === sBefore,
    `服务端预览也写了原子 ✗（${sAfter.atoms.length - sBefore} 条）⇒ 这条判据的**对照**坏了，先查权威实现 ✓`);
  // **∴ 临时实例也不能把合成原子留在 IndexedDB ✓**（**上面 `!lBefore` 只断「权威日志」✗
  //   这里断「本地文档折叠出来的对象表里没有 `preset:*`」✓**）。
  const objects = await callLocal("list_objects", { layer_id: LAYER });
  const leaked = (objects.body && objects.body.objects || [])
    .filter((o) => typeof o.name === "string" && o.name.startsWith("preset:"));
  check(leaked.length === 0,
    `本地文档里出现了 ${leaked.length} 个名字以 preset: 开头的对象 ✗ ⇒ 预览的合成原子漏进了用户文档（应当只在临时实例里 ✓）`);
}

// **⑨ 「一次调用＝一个实例＝一次释放」这条纪律要能被数出来 ✗**（**结构量，不是像素 ✓**）。
//   **∴ 为什么还要这一节 ✗**：第 ③ 节比的是**图**（**复用实例 ⇒ 拿到陈旧图 ✓**）⇒
//   而**「忘了 `free()`」不改变任何一张图**✗ ⇒ 它只会**每拖一次滑杆漏一个 wasm 实例**✓
//   **∴ 所以**像素断言**看不见**这个缺陷 ⇒ **必须**换成数出来的**恒等式** ✓：
//   **`新建实例数 === 预览调用数`** ＆ **`free 次数 === 新建实例数`** ✓
//   （**⇒ 这两条同时抓住两种坏：把临时实例做成缓存 ⇒ 前者红 ✗ 删掉 `finally free()` ⇒ 后者红 ✗**）。
//   **∴ 做法 ✓**：**换掉注入给 `api-local` 的 wasm 模块**（**同一个已加载的 glue ⇒ 只是套一层计数子类 ✓**
//   **不重新 initSync ⇒ 不会引入第二份 wasm 内存这种新变量 ✓**）。
{
  const api = await import(pathToFileURL(process.cwd() + "/web/api-local.js").href);
  if (typeof api.__setLocalApiLoadersForTest !== "function") {
    console.log("⊘ 不可判：web/api-local.js 没有 __setLocalApiLoadersForTest ⇒ 注入点没了，这一节测不了（前提不成立）");
    process.exit(3);
  }
  const Orig = boot.kernel.WasmKernel;
  if (typeof Orig !== "function" || typeof Orig.prototype.free !== "function") {
    console.log("⊘ 不可判：内核 glue 没有 WasmKernel.prototype.free ⇒ 这一节的「释放」口径无从可数（前提不成立）");
    process.exit(3);
  }
  let ctor = 0, freed = 0;
  const originalFree = Orig.prototype.free;
  Orig.prototype.free = function (...a) { freed++; return originalFree.apply(this, a); };
  class CountedKernel extends Orig {
    constructor(...a) { ctor++; super(...a); }
  }
  api.__setLocalApiLoadersForTest({ wasm: async () => ({ ...boot.kernel, WasmKernel: CountedKernel }) });
  const CALLS = 3;
  for (let i = 0; i < CALLS; i++) {
    const r = await callLocal("preview_preset",
      { preset_id: pureId, layer_id: LAYER, render: { max_edge: 256, format: "png" } });
    check(r.status === 200 && !!r.body && r.body.ok === true,
      `第 ${i + 1} 次预览没成功 ✗（${r.status}｜${show(r.body && (r.body.context || r.body.reason))}）⇒ 这一节的计数前提不成立`);
  }
  check(ctor === CALLS,
    `本地 ${CALLS} 次预览新建了 ${ctor} 个内核实例 ✗（**应当每次一个**：多于＝每步建两个，少于＝实例被缓存复用）`);
  check(freed === ctor,
    `建了 ${ctor} 个实例却只 free 了 ${freed} 个 ✗ ⇒ 「临时文档」在本地泄漏 wasm 内存（服务端删失败只留痕，本地连删都没删）`);
  // **∴ 恒等式的另一半 ✓**：**用户文档自己的实例**不该被卷进这一次次新建 ⇒ 用「再来一次不同参数」验
  //   （**代理尺寸变了 ⇒ scratchId 变了 ⇒ 仍旧是「一次一实例」✓**）。
  const before2 = { ctor, freed };
  await callLocal("preview_preset", { preset_id: pureId, layer_id: LAYER, render: { max_edge: 128, format: "png" } });
  check(ctor === before2.ctor + 1 && freed === before2.freed + 1,
    `换尺寸再预览一次 ⇒ 应当各 +1 ✗（实测 ctor ${before2.ctor}→${ctor}｜free ${before2.freed}→${freed}）`);
  api.__setLocalApiLoadersForTest({ wasm: async () => boot.kernel });
  Orig.prototype.free = originalFree;
}

if (bad.length) {
  console.log(`✗ PWA 本地 preview_preset{render} 与服务端像素同口径：${bad.length} 条不符`);
  for (const m of bad) console.log("   · " + m);
  process.exit(1);
}
console.log(`✓ 本地 preview_preset{render} 与服务端逐像素同口径（全等一类／容差一类／效果落像素／实例不复用／无墨层三种形态／降采样／png 回带＋format_fallback／参数与错误文案／日志不变／实例数与 free 数恒等）`);
