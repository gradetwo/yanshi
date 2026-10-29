#!/usr/bin/env node
// **判据：PWA 本地 `create_range_mask` ⇒ 与服务端烘出**同一张遮罩**✓（第 六百七十六 节）。
//
// **∴ 它测什么 ✗**（**内核 ①：断言必须能证明自己能红 ✓**）：
//   这个端点把「按**明度带／色相带**从某个图层**烘焙**出一张 L8 遮罩」放进本地层 ✗
//   ⇒ **两份实现**（Rust 的 `write_create_range_mask` ＋ JS 的本地分支）⇒
//   **∴ 唯一可信的说法还是「同一输入同一输出」✓ 而这次的「输出」有两半 ✓**：
//     ① **回执**（`coverage_*`／`baked`／`note`）✓；② **落盘的原子 ＋ 它指向的那张 L8 图**✓
//   —— **只比 ① 会被骗**✗：回执说「选到 8192 像素」而网格是空的 ⇒ 画面什么都不变 ✓
//   而**没有任何信号**（**形态 ⑤：「做了 0 次」冒充「不需要做」**✗）。
//   ⇒ **∴ 所以这里把回执里的每个数**从网格当场重算一遍**✗ 再比 ✓（**恒等式优于单点断言 ✓**）：
//     `coverage_matched_pixels` ＝ 网格里 >0 的格数 ✓／`coverage_matched_ratio` ＝ 那个数 ÷ 格数 ✓／
//     `coverage_mean` ＝ 网格之和 ÷ (格数 × 255) ✓ —— **三条同时把「回执」与「落盘」焊在一起**✓
//     （**任一边虚报 ⇒ 必红 ✓ 实测见台账的变异 M2/M3**）。
//
// **∴ 为什么比**解出的网格**✗ 而不是 PNG 容器字节 ✓**（**这条是本判据最容易被写错的地方 ✓**）：
//   服务端用 `miniz_oxide`（`png.rs encode_png_l8` ⇒ level 7）✓ 而浏览器只有
//   `CompressionStream("deflate")`（**缺省级别**✗ 不可配 ✓）⇒ **同一串网格 ⇒ 两条不同的 zlib 流**✓
//   （**实测：七张图**逐字节不同的偏移都是 42**✗ 那正是 zlib 的 FLG 字节 ✓** ⇒ 差在压缩级别提示 ✓
//     而**解码后 16384 格全部相等 ✓**）。
//   ⇒ **∴ 于是**「哈希相等／容器相等」**不是**契约 ✓ **解出的像素才是**✓（遮罩消费者是内核的
//   `decode_png_l8`✗ 它读的是像素 ✓）⇒ **这里断像素 ✓ 容器只打印不断 ✓**（**台账「第 六百七十五 节」同一条纪律 ✓**）。
//
// **∴ L8 这一条为什么单独断 ✗**：`CompiledMask::build` 只吃 **8 位灰度**（`decode_png_l8` ✓）⇒
//   **写成 RGBA 也能上传成功 ✓ 也能算出哈希 ✓ 而 patch 在重放时被静默跳过**✗
//   ⇒「回执说挂上了、画面没遮罩」= **形态 ③**✓ ⇒ **所以断「两边解出来都是 1 通道」✓**（**结构量 ✓**）。
//
// **∴ 数字分两类 ✓**（AGENTS「判据里的数字分两类」✓）：文档尺寸与那几支矩形是**判据造的输入**✓
//   能写死；`coverage_*`／`baked`／**网格里到底是多少**是**被测系统决定的**✗ ⇒ **一律与服务端比 ✓**
//   （**权威在对方 ⇒ 判据不写死它 ✓ 服务端哪天改了口径 ⇒ 这条红并指着服务端 ✓**）。
//
// **∴ 前提 ✗**：需要**活着的服务端**（`run-criteria-local.sh` 起并把地址放进 argv[2] ✓）
//   **＋ 真内核 wasm**（`kernelStaleness` ✓）。拿不到 ⇒ `exit(3)`（判据前提不成立 ≠ 产品失败 ✓）。
//
// **∴ 变异（四层齐第④层 ✓ 逐条实测 ⇒ 全部退出码 1 ✓）**：改法与实测退出码记在台账「第 六百七十六 节」✓
//   **⇒ 每条变异改完先过 ESM 语法闸 ✗ 否则「语法崩」会伪装成「断言红」✓**。
process.on("uncaughtException", (error) => {
  console.error(String(error && error.stack || error).slice(0, 1200));
  process.exit(3);
});
import { createHash } from "node:crypto";
import { pathToFileURL } from "node:url";

const BASE = process.argv[2];
if (!BASE || !/^https?:\/\//.test(BASE)) {
  console.log("⊘ 不可判：argv[2] 缺服务端地址 ⇒ 请用 bash scripts/run-criteria-local.sh scripts/tool-pwa-range-mask-parity.mjs 跑"
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
const show = (v) => String(JSON.stringify(v)).slice(0, 240);
/** **规范形 ✓**（**递归按键排序后 stringify ✗**）：**两边对象的键序天然不同**✓
 *   （**服务端 `serde_json` 不带 `preserve_order` ⇒ 落 BTreeMap ⇒ 字母序 ✓；本地是插入序 ✓**）⇒
 *   **比字符串会把「顺序不同」误报成「内容不同」**✗ 而**内容才是契约**✓。 */
const canon = (v) => JSON.stringify(sortDeep(v));
/** **把片段哈希换成占位符 ✓**（**§B③ 只比形状 ✗ 理由见文件头：压缩级别不同 ⇒ 哈希必然不同 ✓**）。
 *   **⇒ 为什么连 `refs.blobs` 也要占位**：它是**同一批哈希**的另一处写法 ✓
 *   **只换 `patches[].blob` 会让「引用集＝片段集」这条恒等式在比对里永远成立 ✗ 而在真实载荷里可能不成立 ✓**
 *   ——**所以占位只用于「形状比对」✗「引用集＝片段集」单独在换占位**之前**断 ✓**。 */
function payloadNoBlob(p) {
  const out = JSON.parse(JSON.stringify(p === undefined ? null : p));
  if (out && Array.isArray(out.patches)) out.patches = out.patches.map((pa) => ({ ...pa, blob: "<blob>" }));
  if (out && out.refs && Array.isArray(out.refs.blobs)) out.refs.blobs = out.refs.blobs.map(() => "<blob>");
  return out;
}
/** **报错时要展示的形状 ✓**（排序后 ✗ 一眼看出差在哪个键 ✓ 而不是差在键序 ✓）。 */
const canonPayloadShape = (p) => sortDeep(payloadNoBlob(p));
function sortDeep(v) {
  if (Array.isArray(v)) return v.map(sortDeep);
  if (v && typeof v === "object") {
    const out = {};
    for (const k of Object.keys(v).sort()) out[k] = sortDeep(v[k]);
    return out;
  }
  return v;
}

const UNIQ = "rm" + Date.now().toString(36);
const SD = `rm-srv-${UNIQ}`, LD = `rm-loc-${UNIQ}`;
const DOC_W = 128, DOC_H = 128;

const boot = await kernelLib.bootLocalWithRealKernel({ origin: BASE });
const localApi = boot.local;
const { decodePng } = await import(pathToFileURL(process.cwd() + "/scripts/lib/png.mjs").href);
const storeLib = await import(pathToFileURL(process.cwd() + "/web/store.js").href);
const nativeFetch = globalThis.fetch;

const createdBody = await nativeFetch(`${BASE}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: SD, width: DOC_W, height: DOC_H }),
}).then((r) => r.json());
if (createdBody.ok !== true || !createdBody.token) {
  console.log(`⊘ 不可判：服务端建文档失败（${show(createdBody)}）⇒ 前提不成立，不是产品问题`);
  process.exit(3);
}
const TOKEN = createdBody.token;
const callServer = async (tool, args) => {
  const res = await nativeFetch(`${BASE}/api/tools/${tool}?token=${TOKEN}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: SD, actor: "human:judge", session: "session:judge", ...args }),
  });
  return { status: res.status, body: await res.json().catch(() => null) };
};
/** **本地抛异常记成产品失败 ✗ 不是判据坏了 ✓**（与其它 PWA 判据同一纪律 ✓）。 */
const callLocal = async (tool, args) => {
  try {
    const res = await localApi(new Request(`https://x/api/tools/${tool}?doc=${LD}`, {
      method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}),
    }));
    return { status: res.status, body: await res.json().catch(() => null) };
  } catch (error) {
    return { status: 0, body: { ok: false, error: "local_threw", error_code: "local_threw",
                               reason: String((error && error.message) || error) } };
  }
};
/** **取两边的原子日志 ✓**（`{atoms:[…]}`；本地与服务端同一形状 ✓）。 */
const atomsSrv = () => nativeFetch(`${BASE}/api/atoms?doc=${SD}&since=0&token=${TOKEN}`).then((r) => r.json());
const atomsLoc = () => localApi(new Request(`https://x/api/atoms?doc=${LD}`)).then((r) => r.json());

const localDoc = await localApi(new Request(`https://x/api/documents?doc=${LD}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: LD, width: DOC_W, height: DOC_H }),
})).then((r) => r.json());
if (!localDoc || localDoc.ok !== true) {
  console.log(`⊘ 不可判：本地建文档失败（${show(localDoc)}）⇒ 前提不成立`);
  process.exit(3);
}

// ---- 装配：两边**同一串步骤**（图层作用域与「空层／纯调整层／已删层」三种形态都要有 ✓）----
const rect = (x, y, w, h, color) => ({
  geometry: { kind: "rect", bbox: { x, y, w, h } }, color, stroke_width: 0,
});
const SEED = [
  ["create_layer", { layer_id: "LS", name: "源层" }],
  ["draw_shape", { layer_id: "LS", object_id: "O_white", data: rect(0, 0, 64, 128, [1, 1, 1, 1]) }],
  ["draw_shape", { layer_id: "LS", object_id: "O_dark", data: rect(64, 0, 64, 128, [0.1, 0.1, 0.1, 1]) }],
  ["create_layer", { layer_id: "LE" }],
  ["create_layer", { layer_id: "LA" }],
  ["add_adjustment", { layer_id: "LA", adjustment_type: "brightness_contrast", object_id: "O_adj" }],
  ["create_layer", { layer_id: "LD", name: "删层" }],
  ["draw_shape", { layer_id: "LD", object_id: "O_del", data: rect(0, 0, 128, 128, [0.9, 0.1, 0.1, 1]) }],
  ["delete_layer", { layer_id: "LD" }],
  ["create_layer", { layer_id: "LT" }],
  ["draw_shape", { layer_id: "LT", object_id: "O_trans", data: rect(0, 0, 128, 128, [0, 0, 0, 0]) }],
  // **一张活蒙版 ✗**（**§A 的「重复蒙版」那条要用 ✓**：**提交期唯一会拒的一条**⇒
  //   **它的 `context.atom_id` 只有走这条路才断得到 ✓**）。
  ["create_mask", { mask_id: "MDUP", shape: { kind: "rect", bbox: { x: 0, y: 0, w: 64, h: 64 } } }],
  // **一张「R≠G≠B」的彩色层 ✗**（**四层齐的第②层：输入必须能触发「明度权重」这条断言 ✓**）：
  //   原来这份种子**全是灰**（R=G=B ⇒ `0.2126r+0.7152g+0.0722b` 与把 r/b 权重换过来的
  //   **错版本算出同一个数**✓）⇒ 变异 M21「Rec709 权重装反」**判据没红**（实测退出码 0 ✗）
  //   ⇒ **∴ 不是那条断言多余 ✗ 是输入没造出它要检查的情况 ✓**（AGENTS「四层齐」的原文症状）。
  //   **选色要在服务端那个域里算 ✗ 不是我手里那串浮点 ✓**（第 676 轮量出来的）：
  //   烘焙读的是**入库后的 u8 显示字节**（`tools_mask.rs:728 srgb_luma`）⇒ 参数里的
  //   `[r,g,b]` 是**线性值** ⇒ 要先 sRGB 编码加权重算明度；我按原始浮点连猜两次
  //   （`0.45`／`0.5` 那两支）**网格全格 255**✗ ⇒ 恒绿案 ⇒ 第三支改成**实测**（下面那条命令）
  //   拿到的：`low=0.5, smoothness=0.05` ⇒ 过渡带 `[0.45,0.55]`，取带心的 `g=0.1125`
  //   ⇒ **服务端真值：每格 `133`**（距两端各 122/133 ⇒ 不会被 ±1 的舍入推出过渡带 ✓）；
  //   而 r/b 权重互换后明度掉到 `0.436` ⇒ **在带外 ⇒ 格值 0 ✓** ⇒ **互换权重必然把网格改掉 ✓**。
  //   **∴ 但我仍不保证它 ✗**：下面 §C2 断「这一案的每一格都在 0 与 255 **之间**」⇒
  //   **这条案是不是真的在过渡带里 ✗ 判据自己会说 ✓**（将来上游改 gamma／改权重也不会静默变成恒绿 ✓）。
  //   重算这个数：`node -e 'const g=(v)=>v<=0.0031308?12.92*v:1.055*Math.pow(v,1/2.4)-0.055,q=(v)=>Math.round(g(v)*255),l=(r,gg,b)=>(0.2126*q(r)+0.7152*q(gg)+0.0722*q(b))/255;console.log(l(0.9,0.1125,0.2).toFixed(4),l(0.2,0.1125,0.9).toFixed(4))'`
  ["create_layer", { layer_id: "LLUMA", name: "彩色层" }],
  ["draw_shape", { layer_id: "LLUMA", object_id: "O_luma", data: rect(0, 0, 128, 128, [0.9, 0.1125, 0.2, 1]) }],
];
const setupFail = [];
for (const [tool, args] of SEED) {
  const s = await callServer(tool, args);
  const l = await callLocal(tool, args);
  if (!s.body || s.body.ok !== true) setupFail.push(`服务端 ${tool} ${show(s.body && (s.body.context || s.body))}`);
  if (!l.body || l.body.ok !== true) setupFail.push(`本地 ${tool} ${show(l.body && (l.body.reason || l.body.context))}`);
}
if (setupFail.length) {
  console.log(`⊘ 不可判：输入造不出来（前提不成立，不是产品问题）`);
  for (const line of setupFail) console.log("   · " + line);
  process.exit(3);
}

// **§A 错误族 ✗**：**状态／错误码／`context.detail` 逐字／`context` 键集**四样都要相等 ✓。
//   **∴ 为什么还断「本地 `reason` 必须等于 `context.detail`」✗**：`reason` 是**本地独有的键**✓
//   （**PWA 的界面读它 ✗ 服务端没有 ✓**）⇒ **一旦它与 detail 漂开 ✗ 界面显示的原因与真因就不是同一句**✗。
const ERR_CASES = [
  ["缺 mask_id", {}, 400, "invalid_argument", "缺少必填参数 mask_id"],
  ["缺 layer_id", { mask_id: "M_e1" }, 400, "invalid_argument", "缺少必填参数 layer_id"],
  ["range 非法", { mask_id: "M_e2", layer_id: "LS", range: "bogus" }, 400, "invalid_argument",
   "range 只收 luminance / color，得到 bogus"],
  ["low>high", { mask_id: "M_e3", layer_id: "LS", low: 0.8, high: 0.2 }, 400, "invalid_argument",
   "low 必须 ≤ high，得到 low=0.8、high=0.2"],
  ["low 超范围", { mask_id: "M_e4", layer_id: "LS", low: 1.5 }, 400, "invalid_argument",
   "low 必须在 [0，1] 内，得到 1.5"],
  ["smoothness 超范围（明度带）", { mask_id: "M_e5", layer_id: "LS", smoothness: 0.9 }, 400, "invalid_argument",
   "smoothness 必须在 [0，0.5] 内，得到 0.9"],
  ["color 缺 hue", { mask_id: "M_e6", layer_id: "LS", range: "color" }, 400, "invalid_argument", "hue 是必填参数"],
  ["hue 越界", { mask_id: "M_e7", layer_id: "LS", range: "color", hue: 400 }, 400, "invalid_argument",
   "hue 必须在 [0，360] 内，得到 400"],
  ["hue_width=0", { mask_id: "M_e8", layer_id: "LS", range: "color", hue: 10, hue_width: 0 }, 400, "invalid_argument",
   "hue_width 必须大于 0（窗全宽），得到 0"],
  ["low 是字符串", { mask_id: "M_e9", layer_id: "LS", low: "0.5" }, 400, "invalid_argument",
   "参数 low 类型应为 number，实际为 string"],
  ["未知键", { mask_id: "M_e10", layer_id: "LS", quality: 9 }, 400, "invalid_argument", null],
  ["层从未存在", { mask_id: "M_e11", layer_id: "NOPE" }, 404, "reference_not_found", "图层 NOPE 不存在"],
  ["layer_id 空串", { mask_id: "M_e12", layer_id: "" }, 404, "reference_not_found", "图层  不存在"],
  ["层已删除", { mask_id: "M_e13", layer_id: "LD" }, 404, "reference_not_found", "图层 LD（「删层」）已删除"],
  ["空层（无对象）", { mask_id: "M_e14", layer_id: "LE" }, 409, "precondition_failed",
   "图层 LE 没有任何对象 ⇒ 没有可采样的像素"],
  ["纯调整层", { mask_id: "M_e15", layer_id: "LA" }, 409, "precondition_failed",
   "图层 LA 只有调整/滤镜对象，没有可采样的像素"],
  // **★ 提交期唯一会拒的一条 ✗★**（**重复蒙版 ⇒ 409 ＋ `context.atom_id` ✓**）：
  //   **它是「原子已经封套、只是没落盘」的那个位置 ⇒ 服务端把**要提交的那条原子 id**放进 context ✓**
  //   ⇒ **本地少了就是虚报少给 ✓**（**而这条只能靠「上面所有错误分支都不带 atom_id」反衬 ✗
  //     所以上面 15 案＋这一案要**一起**断键集 ✓**）。
  ["重复蒙版（提交期 409）", { mask_id: "MDUP", layer_id: "LS" }, 409, "precondition_failed",
   "蒙版 MDUP 已存在"],
];
for (const [label, args, status, code, detail] of ERR_CASES) {
  const beforeS = (await atomsSrv()).atoms.length, beforeL = (await atomsLoc()).atoms.length;
  const srv = await callServer("create_range_mask", args);
  const loc = await callLocal("create_range_mask", args);
  check(srv.status === status,
    `§A ${label}：服务端这次不是 ${status}（实测 ${srv.status}）✗ ⇒ 权威口径变了 ⇒ 要改的是这条判据＋台账`);
  check(srv.status === loc.status, `§A ${label}：状态不同 ✗ 服务端 ${srv.status}｜本地 ${loc.status}`);
  check(srv.body && srv.body.ok === false && loc.body && loc.body.ok === false,
    `§A ${label}：有一边 ok≠false ✗ ${show(srv.body)}｜${show(loc.body)}`);
  check(!!srv.body && srv.body.error_code === code,
    `§A ${label}：服务端错误码不是 ${code}（实测 ${show(srv.body && srv.body.error_code)}）✗ ⇒ 权威变了`);
  check(!!loc.body && loc.body.error_code === code,
    `§A ${label}：本地错误码应为 ${code} ✗ 实测 ${show(loc.body && loc.body.error_code)}｜${show(loc.body && loc.body.reason)}`);
  const sDetail = srv.body && srv.body.context && srv.body.context.detail;
  const lDetail = loc.body && loc.body.context && loc.body.context.detail;
  if (detail !== null) {
    check(sDetail === detail, `§A ${label}：服务端文案变了（实测 ${show(sDetail)}）✗ ⇒ 权威口径变了 ⇒ 改判据＋台账`);
  }
  check(typeof lDetail === "string" && lDetail === sDetail,
    `§A ${label}：错误文案不同 ✗ 服务端 ${show(sDetail)}｜本地 ${show(lDetail)}`);
  const sCtx = Object.keys((srv.body && srv.body.context) || {}).sort().join(",");
  const lCtx = Object.keys((loc.body && loc.body.context) || {}).sort().join(",");
  check(sCtx === lCtx,
    `§A ${label}：context 键集不同 ✗ 服务端 [${sCtx}]｜本地 [${lCtx}]（**少给＝虚报少给 ✓**）`);
  check(loc.body.reason === lDetail,
    `§A ${label}：本地的 reason 与 context.detail 不是同一句 ✗ ${show(loc.body.reason)}｜${show(lDetail)}`);
  // **★ 拒绝必须「一个新原子都没写」✗ ★**（**回执说没成、日志却多了一条 ⇒ 画面从此与回执矛盾 ✓**）：
  const afterS = (await atomsSrv()).atoms.length, afterL = (await atomsLoc()).atoms.length;
  check(afterS === beforeS, `§A ${label}：服务端被拒却写了原子 ✗ ${beforeS}⇒${afterS}（判据输入踩到了权威缺陷 ⇒ 报出来）`);
  check(afterL === beforeL,
    `§A ${label}：本地被拒却写了原子 ✗ ${beforeL}⇒${afterL} ⇒「回执 4xx ＋ 状态已改」＝静默降级形态③ ✓`);
}

// **§B 成功族 ✗**：**回执逐键比 ＋ 落盘的 L8 网格逐字节比 ✗ ＋ 回执里的数从网格重算 ✓**。
const OK_CASES = [
  ["明度带 0.5–1.0", { mask_id: "Mok1", layer_id: "LS", low: 0.5, high: 1.0 }],
  ["色相带（选不到东西）", { mask_id: "Mok2", layer_id: "LS", range: "color", hue: 0, hue_width: 60 }],
  ["明度带 invert", { mask_id: "Mok3", layer_id: "LS", low: 0.5, high: 1.0, invert: true }],
  ["全带", { mask_id: "Mok4", layer_id: "LS", low: 0, high: 1 }],
  ["窄带 smoothness 0", { mask_id: "Mok5", layer_id: "LS", low: 0.9, high: 0.95, smoothness: 0 }],
  ["全幅透明层", { mask_id: "Mok6", layer_id: "LT", low: 0.5, high: 1.0 }],
  ["mask_id 空串（服务端照收）", { mask_id: "", layer_id: "LS", low: 0.5, high: 1.0 }],
  ["明度权重（R≠G≠B 的边界色）", { mask_id: "Mok7", layer_id: "LLUMA", low: 0.5, high: 1.0 }],
  // **★ 合并侧加的一案 ✗ ★**（第 1245 轮复审：本地原来在**收窄成 f32 之前**比 `low > high` ✓
  //   而服务端 `number_in(..) as f32` 之后就比 ⇒ **f64 里不等、f32 里相等的一对**（`0.3` 的下一个 f64）
  //   服务端**放行**而本地**拒**✗ ⇒ 这条案在旧实现下红、在新实现下绿 ✓（**这条断言自己能证 ✓**）。
  ["f32 等值（f64 不等）", { mask_id: "Mok8", layer_id: "LS",
    low: 0.30000000000000004, high: 0.3 }],
];
/** **两边的 `create_mask` 原子（按 mask_id 找 ✗ 日志里最后一条 ✓）**。 */
const maskAtom = async (which, maskId) => {
  const rows = which === "srv" ? (await atomsSrv()).atoms : (await atomsLoc()).atoms;
  for (let i = rows.length - 1; i >= 0; i--) {
    const a = rows[i].atom || rows[i];
    if (a.kind === "create_mask" && a.payload && a.payload.mask_id === maskId) return a;
  }
  return null;
};
/** **取一张遮罩片段并解成 L8 网格 ✓**（服务端走 HTTP GET ⇒ **本地走 `store.getBlob` ✗ 因为本地层只实现 `POST /api/blob` ✓**）。 */
const gridOf = async (which, atom) => {
  const hash = atom && atom.payload && atom.payload.patches && atom.payload.patches[0] && atom.payload.patches[0].blob;
  if (typeof hash !== "string" || !hash) return { error: "没有 patch.blob ⇒ 原子形状不对：" + show(atom && atom.payload) };
  let bytes;
  if (which === "srv") {
    const res = await nativeFetch(`${BASE}/api/blob/${hash}?doc=${encodeURIComponent(SD)}&token=${TOKEN}`);
    if (!res.ok) return { error: "服务端取 blob 失败 " + res.status };
    bytes = Buffer.from(await res.arrayBuffer());
  } else {
    const raw = await storeLib.getBlob(await storeLib.open(), hash);
    if (!raw) return { error: "本地 blob 库里没有这条哈希 ⇒ putBlob 没落地：" + hash };
    bytes = Buffer.from(raw);
  }
  try { return { hash, bytes, img: decodePng(bytes) }; }
  catch (e) { return { hash, bytes, error: "解码失败：" + e.message }; }
};
const LOCAL_EXTRA_KEYS = ["server"];   // **本地独有的回执键 ✗（`server:false` 是 PWA 的诚实标记 ✓）**
/** **本端点自己的契约键 ✓**（**两边都必须给 ✓ 值必须相等 ✓**）。 */
const SHARED_KEYS = ["ok", "seq", "head", "range", "source_layer_id", "coverage_matched_pixels",
                     "coverage_matched_ratio", "coverage_mean", "baked", "note"];
/** **服务端每个写工具都会多给的外层字段 ✓**（**HTTP／日志／渲染层加的 ⇒ 不是本端点的契约 ✓**）
 *   —— **本地给不出`preview`（要缩略图渲染）／`timings`（要服务端计时）✓**，
 *   **口径与 `scripts/tool-pwa-local-write-guards.mjs` 的 `NOISE` 一致 ✓**。
 *   **∴ 仍然逐条断「服务端确实还在给」✗**：哪天外层删了某个字段 ⇒ 下面那条红并指着外层 ✓
 *   （**不是本地偷偷少给 ✓**）。 */
const SERVER_ONLY = ["atom_id", "changeset_id", "created", "dirty_bbox", "dirty_kind", "dirty_tiles",
                     "duplicate", "job_id", "job_status", "opened", "preview", "snapshotted",
                     "suggestions", "timings", "warnings"];
/** **`head` 是原子 id ⇒ 两边的 id 空间不同 ✗ 只断「都在」✓**（同 `write-guards` 的 `divergent` 口径 ✓）。 */
const DIVERGENT_KEYS = ["head"];
const seenSrvKeys = new Set();
const grids = {};                      // **`标签 → {srv,loc}` 的网格缓存 ✓ 后面几条要复用 ✓**
for (const [label, args] of OK_CASES) {
  const srv = await callServer("create_range_mask", args);
  const loc = await callLocal("create_range_mask", args);
  check(srv.status === 200 && srv.body && srv.body.ok === true,
    `§B ${label}：服务端没成功（实测 ${show(srv.body && (srv.body.context || srv.body))}）✗ ⇒ 输入没造出来`);
  check(loc.status === 200 && loc.body && loc.body.ok === true,
    `§B ${label}：本地没成功 ✗ ${show(loc.body && (loc.body.reason || loc.body.context))}`);
  if (!srv.body || !loc.body || srv.body.ok !== true || loc.body.ok !== true) continue;
  // **① 键集 ✗**（**两栏允许清单，与 `scripts/tool-pwa-local-write-guards.mjs` 同一口径 ✓**）：
  //   服务端每个键要么在**契约**里（⇒ 本地必须有 ✓）、要么在**外层**里（⇒ 允许本地没有 ✓）；
  //   **两边都不在 ⇒ 未登记 ✗**（要么补进本地，要么写进 `SERVER_ONLY` 并说明为什么本地给不出 ✓）。
  const sKeys = Object.keys(srv.body), lKeys = Object.keys(loc.body);
  for (const k of sKeys) seenSrvKeys.add(k);
  for (const k of sKeys) {
    check(SHARED_KEYS.includes(k) || SERVER_ONLY.includes(k),
      `§B ${label}：服务端多出一个未登记的键「${k}」✗（值 ${show(srv.body[k])}）⇒ 要么补进本地契约，要么写进 SERVER_ONLY 并说明为什么本地给不出`);
    // **契约键 ⇒ 本地必须有 ✓**；**外层键 ⇒ 只允许缺 ✓（但下面 §B⑥ 逐条断服务端还在给 ✓）**。
    if (SHARED_KEYS.includes(k)) {
      check(lKeys.includes(k),
        `§B ${label}：本地回执少给契约键「${k}」✗（服务端 ${show(srv.body[k])}）⇒ 虚报少给 ✓`);
    }
  }
  for (const k of lKeys) {
    check(SHARED_KEYS.includes(k) || LOCAL_EXTRA_KEYS.includes(k),
      `§B ${label}：本地多给了未登记的键「${k}」✗ ⇒ 双份口径：要么删，要么在这里承认并说明`);
  }
  // **`seq` 两边比 ✓**（**同一条种子 ⇒ 落盘的原子数必须同 ✓**）：
  //   **不等 ⇒ 有一边多写或少写了一条原子 ✗ 而回执照样说「成功」**（形态⑤ ✓）。
  check(srv.body.seq === loc.body.seq,
    `§B ${label}：两边写入日志的序号不同 ✗ 服务端 seq ${show(srv.body.seq)}｜本地 seq ${show(loc.body.seq)}`);
  // **② 数值逐键相等 ✗**（**权威在对方 ⇒ 判据一个数都不写死 ✓**）。
  for (const k of SHARED_KEYS) {
    if (k === "ok" || k === "seq" || DIVERGENT_KEYS.includes(k)) continue;
    check(canon(srv.body[k]) === canon(loc.body[k]),
      `§B ${label}：字段「${k}」两边不一致 ✗ 服务端 ${show(srv.body[k])}｜本地 ${show(loc.body[k])}`);
  }
  // **③ 落盘的原子形状 ✗**（**`CompiledMask` 的 patch 重放口径 ⇒ 键序无关 ✓ 内容必须同 ✓**）。
  const aS = await maskAtom("srv", args.mask_id), aL = await maskAtom("loc", args.mask_id);
  check(!!aS && !!aL, `§B ${label}：有一边没写出 create_mask 原子 ✗ 服务端 ${!!aS}｜本地 ${!!aL}`);
  if (aS && aL) {
    // **哈希不进这条比对 ✗**（**原因见文件头：容器字节差在 zlib FLG ⇒ 哈希必然不同 ✓**）——
    //   **这里比的是「形状」：`origin`／`shape`／`feather`／`mode`／`invert`／`op`／`target_u8`／`bbox` ✓**，
    //   **而「那张图对不对」由下面两条单独管 ✓**（**逐字节网格 ✓ ＋ 哈希＝自身内容的 sha256 ✓**）。
    check(canon(payloadNoBlob(aS.payload)) === canon(payloadNoBlob(aL.payload)),
      `§B ${label}：create_mask 原子载荷不同 ✗\n     服务端 ${show(canonPayloadShape(aS.payload))}\n     本地     ${show(canonPayloadShape(aL.payload))}`);
    // **④ L8 闸 ✗**（**写错颜色类型 ⇒ patch 被静默跳过 ⇒ 形态③ ✓**）。
    const gS = await gridOf("srv", aS), gL = await gridOf("loc", aL);
    check(!gS.error && !gL.error, `§B ${label}：网格取不出来 ✗ 服务端 ${show(gS.error)}｜本地 ${show(gL.error)}`);
    if (gS.img && gL.img) {
      // **内容寻址自洽 ✗**：原子写的哈希必须等于**它自己那份字节**的 sha256 ✓——
      //   这条同时封掉两种作弊：**「哈希贴错对象」**（取到的是另一张图 ✗）与
      //   **「换了字节没换哈希」**（本地重放时会读到旧 blob ✓ 画面从此与原子不一致 ✗）。
      for (const [which, g] of [["服务端", gS], ["本地", gL]]) {
        const selfHash = "sha256:" + createHash("sha256").update(g.bytes).digest("hex");
        check(g.hash === selfHash,
          `§B ${label}：${which}的 patch.blob 与实际取回的字节对不上 ✗ 原子写 ${g.hash}｜字节算出 ${selfHash}`
          + " ⇒「内容寻址」被骗过 ⇒ 重放时会静默拿到另一张图（形态③）✓");
        // **引用集＝片段集 ✗**（**漏登记 ⇒ GC 会把这个 blob 删掉 ⇒ 遮罩从此取不到像素 ✓**）。
        const atom = which === "服务端" ? aS : aL;
        const patchBlobs = (atom.payload.patches || []).map((p) => p.blob);
        check(canon((atom.payload.refs && atom.payload.refs.blobs) || []) === canon(patchBlobs),
          `§B ${label}：${which}的 refs.blobs 与 patches[].blob 不是同一批 ✗ ${show(atom.payload.refs)}｜${show(patchBlobs)}`
          + " ⇒ 片段指向的 blob 没进引用集 ⇒ 会被回收（回执说挂上了、画面没遮罩 ✓ 形态③）");
      }
      check(gS.img.channels === 1 && gL.img.channels === 1,
        `§B ${label}：遮罩片段不是**灰度 L8** ✗ 服务端 ${gS.img.channels} 通道｜本地 ${gL.img.channels} 通道`
        + " ⇒ `decode_png_l8` 会**静默跳过**这张 patch（回执说挂上了、画面没遮罩 ✓ 形态③）");
      check(gS.img.width === DOC_W && gS.img.height === DOC_H && gL.img.width === DOC_W && gL.img.height === DOC_H,
        `§B ${label}：网格尺寸不等于文档尺寸 ✗ 服务端 ${gS.img.width}×${gS.img.height}｜本地 ${gL.img.width}×${gL.img.height}（文档 ${DOC_W}×${DOC_H}）`);
      check(gS.img.width === gL.img.width && gS.img.height === gL.img.height,
        `§B ${label}：两边网格几何不同 ✗ ${gS.img.width}×${gS.img.height}｜${gL.img.width}×${gL.img.height}`);
      let diff = 0;
      const n = Math.min(gS.img.data.length, gL.img.data.length);
      for (let i = 0; i < n; i++) if (gS.img.data[i] !== gL.img.data[i]) diff++;
      check(gS.img.data.length === gL.img.data.length && diff === 0,
        `§B ${label}：**烘焙出的遮罩像素不同** ✗ ${diff}/${n} 格不一致 ⇒ 取整／窗口／invert 有一条走岔了 ✓`);
      // **容器只打印不断 ✗**（**原因见文件头：zlib 级别不可配 ✓ 差在 FLG 字节 ✓**）。
      let off = -1;
      for (let i = 0; i < Math.min(gS.bytes.length, gL.bytes.length); i++) { if (gS.bytes[i] !== gL.bytes[i]) { off = i; break; } }
      console.log(`   · ${label}：网格 ${gS.img.width}×${gS.img.height} 逐字节同=${diff === 0}`
        + `｜PNG 容器 ${gS.bytes.length}/${gL.bytes.length} 字节｜首个不同偏移 ${off}`
        + (off === -1 ? "（容器也相同 ✓）" : "（**zlib FLG／级别差 ⇒ 只算打印，不断言 ✓**）"));
      grids[label] = { srv: gS.img, loc: gL.img, body: { srv: srv.body, loc: loc.body }, args };
      // **⑤ 恒等式：每一边的回执都能从**它自己**那张网格重算 ✗**（**这条把「回执」与「落盘」焊死 ✓**）——
      //   **∴ 为什么按边各算一遍 ✗ 而不是拿服务端的网格去校本地 ✓**：
      //   「两边网格相同」由上面那条单独断 ✓ ⇒ 两条合起来才是「**回执 ⇄ 像素**」在**每一边**都自洽 ✓
      //   （**只算一边 ⇒ 两边一起错 + 回执一起吹 ⇒ 全绿 ✗**）。
      const cells = gS.img.width * gS.img.height;
      for (const [which, img, body] of [["服务端", gS.img, srv.body], ["本地", gL.img, loc.body]]) {
        let matched = 0, sum = 0;
        for (let i = 0; i < img.data.length; i++) { if (img.data[i] > 0) matched++; sum += img.data[i]; }
        check(body.coverage_matched_pixels === matched,
          `§B ${label}：${which}回执的 coverage_matched_pixels 与**它自己**的网格数出来的不等 ✗ `
          + `回执 ${show(body.coverage_matched_pixels)}｜网格实际 ${matched}`
          + (which === "服务端"
            ? "（判据的算法与权威不同 ⇒ 报出来，别把它当成 bug 迁就 ✓）"
            : " ⇒「做了 0 次」冒充「不需要做」的形态⑤ ✓（画面与回执对不上）"));
        check(Math.abs(body.coverage_matched_ratio - matched / cells) < 1e-12,
          `§B ${label}：${which} coverage_matched_ratio ≠ 选中格数 ÷ 总格数 ✗ ${show(body.coverage_matched_ratio)}｜${matched}/${cells}`);
        check(Math.abs(body.coverage_mean - sum / (cells * 255)) < 1e-12,
          `§B ${label}：${which} coverage_mean ≠ 网格均值 ✗ ${show(body.coverage_mean)}｜${sum}/${cells * 255}`);
      }
    }
  }
}
// **⑥ 外层字段的「权威确实还在给」✗**（**逐条断 ✓ 不是注释里说说 ✓**）：
//   `SERVER_ONLY` 是为了「本地给不出」而**允许本地缺**的清单 ✗
//   ⇒ **若服务端哪天不再给某个字段**（外层删了 ⇒ 那字段本来就不是契约 ✓）**⇒ 这条红并指着外层 ✓**，
//   **而不是让它悄悄留在清单里**（**那会让清单越来越宽 = 越来越不管事 ✓**）。
for (const k of SERVER_ONLY) {
  check(seenSrvKeys.has(k),
    `§B：服务端这几案**一次都没给**外层字段「${k}」✗ ⇒ 外层口径变了（或它本来不归这个端点）⇒ 要改的是这条判据＋台账`);
}

// **§C「选不到东西」必须**照实说**✗**（**形态⑤的专项守卫 ✓**）：
//   那支色相带在这份文档里一个像素都不选 ⇒ **回执必须 matched=0 ＋ 网格必须全 0**✗
//   （**若本地把「全 0 网格」换成「整幅 255」来「让功能看起来能用」**✓ 回执与落盘就互相打脸 ✗）。
{
  const key = "色相带（选不到东西）";
  const g = grids[key];
  if (!g) {
    console.log(`⊘ 不可判：§C 需要的「${key}」那一张没拿到（前提不成立，不是产品问题）`);
  } else {
    check(g.body.srv.coverage_matched_pixels === 0 && g.body.loc.coverage_matched_pixels === 0,
      `§C：两边回执说选到了像素 ✗ 服务端 ${show(g.body.srv.coverage_matched_pixels)}｜本地 ${show(g.body.loc.coverage_matched_pixels)}`
      + " ⇒ 这份输入本不该选到任何东西 ⇒ 判据的输入或权威口径变了 ✓");
    const allZero = (d) => d.every((v) => v === 0);
    check(allZero(g.srv.data) && allZero(g.loc.data),
      `§C：网格不是全 0 ✗ 服务端全 0=${allZero(g.srv.data)}｜本地全 0=${allZero(g.loc.data)}`
      + " ⇒「选不到却画出遮罩」＝静默把整幅文档当成被选中 ✓ 形态③");
  }
}

// **§C2「明度权重」那条断言的**输入闸**✗**（**四层齐的第②层写成断言 ✓ 不写在注释里 ✓**）：
//   **⇒ 为什么要有这一条**：变异 M21（把 Rec709 的 r/b 权重互换）**第一次没红**✗
//   ⇒ 查出来是**这份种子全是灰**（R=G=B ⇒ 任何权重组合都算出同一个明度 ✓）**⇒ 断言恒绿 ✗ 什么也没验**✓
//   —— 这正是 AGENTS「四层齐」列的那个症状**第二次**发生（第一次是第 261 轮的 `every()` 空数组 ✓）。
//   **∴ 所以**断**「那张彩色案的网格落在软边过渡带里**（每格严格介于 0 与 255 ✗ 既非选中也非排除）**」**✓
//   ⇒ **只有处在过渡带里，权重才**改变格值**✓ 也才可能把「权重装反」量出来 ✓**。
{
  const key = "明度权重（R≠G≠B 的边界色）";
  const g = grids[key];
  if (!g) {
    console.log(`⊘ 不可判：§C2 需要的「${key}」那一张没拿到（前提不成立，不是产品问题）`);
  } else {
    for (const which of ["srv", "loc"]) {
      const d = g[which].data;
      const mid = d.filter((v) => v > 0 && v < 255).length;
      check(mid === d.length,
        `§C2 ${which === "srv" ? "服务端" : "本地"}：那张「边界色」的网格有 ${d.length - mid}/${d.length} 格落在 0 或 255 ✗`
        + " ⇒ 这条案**没处在软边过渡带里** ⇒「明度权重装反」在它身上量不出来 ⇒ 要改的是判据的输入（不是放宽断言 ✓）");
    }
  }
}

// **§D invert 的关系 ✗**（**恒等式优于单点断言 ✓**）：
//   **∴ 断的不是「invert 那张图长什么样」✗ 而是「它与不 invert 那张逐格互补」**✓
//   （**`grid_inv = 255 − grid` 在服务端与本地**同时**成立 ⇒ 跨输入、跨机器 ✓**）。
{
  const plain = grids["明度带 0.5–1.0"], inv = grids["明度带 invert"];
  if (!plain || !inv) {
    console.log("⊘ 不可判：§D 需要的两张图没都拿到（前提不成立，不是产品问题）");
  } else {
    for (const which of ["srv", "loc"]) {
      const a = plain[which], b = inv[which];
      let worst = 0;
      for (let i = 0; i < Math.min(a.data.length, b.data.length); i++) {
        const d = Math.abs(a.data[i] + b.data[i] - 255);
        if (d > worst) worst = d;
      }
      check(a.data.length === b.data.length && worst === 0,
        `§D ${which === "srv" ? "服务端" : "本地"}：invert 与原图**不互补** ✗ 最大偏离 ${worst}`
        + " ⇒ invert 没有烤进网格（或烤了两次 ✓ 抵消掉）——形态③：参数被静默忽略 ✓");
    }
  }
}

// **§E 图层作用域 ✗**（**这条抓的是「把整幅文档当成这一层」✓ 形态③**）：
//   **① 空层那张图必须与有墨层那张不同 ✓**（透明层只贡献文档底色 ⇒ 网格与 LS 的双色网格必然不同 ✓）；
//   **② 再在上面盖一层不透明矩形 ⇒ 源层的烘焙**不该**变 ✗**（**服务端 `render_region_raw_layer` 只渲那一层 ✓**）
//     ——**若本地走了合成分支 ✗ 第二次会连上面那层一起渲 ⇒ 网格变了 ⇒ 这条红 ✓**。
{
  const inked = grids["明度带 0.5–1.0"], blank = grids["全幅透明层"];
  if (inked && blank) {
    for (const which of ["srv", "loc"]) {
      let diff = 0;
      const a = inked[which], b = blank[which];
      for (let i = 0; i < Math.min(a.data.length, b.data.length); i++) if (a.data[i] !== b.data[i]) diff++;
      check(diff > 0,
        `§E ${which === "srv" ? "服务端" : "本地"}：有墨层与透明层烘出**同一张**遮罩 ✗`
        + " ⇒ 图层没筛住（把整幅文档当成这一层 ✓ 形态③）");
    }
  }
  // **② 盖上顶再烘一次 ✗**
  const cover = { layer_id: "LCOVER", name: "顶盖" };
  const s2 = await callServer("create_layer", cover);
  const l2 = await callLocal("create_layer", cover);
  const rectData = rect(0, 0, DOC_W, DOC_H, [0.95, 0.05, 0.05, 1]);
  const s3 = await callServer("draw_shape", { layer_id: "LCOVER", object_id: "O_cover", data: rectData });
  const l3 = await callLocal("draw_shape", { layer_id: "LCOVER", object_id: "O_cover", data: rectData });
  check(s2.body && s2.body.ok === true && l2.body && l2.body.ok === true,
    `⊘ 输入造不出来：§E 的顶盖图层建不出来 ✗ ${show(s2.body && s2.body.context)}｜${show(l2.body && l2.body.reason)}`);
  check(s3.body && s3.body.ok === true && l3.body && l3.body.ok === true,
    `⊘ 输入造不出来：§E 顶盖上的矩形画不上去 ✗ ${show(s3.body && s3.body.context)}｜${show(l3.body && l3.body.reason)}`);
  if (s3.body && s3.body.ok === true && l3.body && l3.body.ok === true) {
    const againS = await callServer("create_range_mask", { mask_id: "Mscope1", layer_id: "LS", low: 0.5, high: 1.0 });
    const againL = await callLocal("create_range_mask", { mask_id: "Mscope1", layer_id: "LS", low: 0.5, high: 1.0 });
    const aS = await maskAtom("srv", "Mscope1"), aL = await maskAtom("loc", "Mscope1");
    const gS = await gridOf("srv", aS), gL = await gridOf("loc", aL);
    if (gS.img && gL.img && inked) {
      const same = (x, y) => x.data.length === y.data.length && x.data.every((v, i) => v === y.data[i]);
      check(same(gS.img, inked.srv),
        "§E：服务端多盖一层后**源层的遮罩变了** ✗ ⇒ 是权威口径变了（单层渲染不再只渲那一层）⇒ 改判据＋台账");
      check(same(gL.img, inked.loc),
        "§E：**本地多盖一层后源层的遮罩变了** ✗ 上面那层被一起渲进来了 ⇒ 走了合成分支 ✓（形态③：静默把别的层算进这一层）");
      check(same(gS.img, gL.img), "§E：加了顶盖之后两边烘出的遮罩不再相同 ✗（上面两条的交叉印证 ✓）");
      check(againS.body.coverage_matched_pixels === againL.body.coverage_matched_pixels,
        `§E：加了顶盖后回执的选中数不等 ✗ 服务端 ${show(againS.body.coverage_matched_pixels)}｜本地 ${show(againL.body.coverage_matched_pixels)}`);
    } else {
      console.log(`⊘ 不可判：§E 的对照网格没拿到（${show(gS.error || gL.error)}）`);
    }
  }
}

// **§F 挂接（`set_property{mask_id}`）✗**：**面板 `commitRangeMask` 走的就是这条路 ✓**。
//   **∴ 三种形态都要比 ✗**：活蒙版 ⇒ 200 ✓／不存在 ⇒ 404 ✓／**已删除** ⇒ 404 ✓
//   （**服务端谓词是 `masks.get(v).is_some_and(!is_deleted)` ✗「建过又删」不等于「没有」✓**）
//   ＋ **回执之外还要断「原子真的写了同一条 set_property」**✓（**挂接是不是落地 ✗ 只有日志知道 ✓**）。
const attachCases = [
  ["挂接活蒙版", "Mok1", 200, null],
  ["挂接不存在的蒙版", "M_never", 404, "set_property mask_id 引用的实体 M_never 不存在或已删除"],
];
{
  const delS = await callServer("delete_mask", { mask_id: "Mok4" });
  const delL = await callLocal("delete_mask", { mask_id: "Mok4" });
  check(delS.body && delS.body.ok === true && delL.body && delL.body.ok === true,
    `⊘ 输入造不出来：§F 需要的「已删蒙版」没删掉 ✗ ${show(delS.body && delS.body.context)}｜${show(delL.body && delL.body.reason)}`);
  attachCases.push(["挂接已删除的蒙版", "Mok4", 404, "set_property mask_id 引用的实体 Mok4 不存在或已删除"]);
}
for (const [label, value, status, detail] of attachCases) {
  const beforeL = (await atomsLoc()).atoms.length;
  const srv = await callServer("set_property", { layer_id: "LS", key: "mask_id", value });
  const loc = await callLocal("set_property", { layer_id: "LS", key: "mask_id", value });
  check(srv.status === status, `§F ${label}：服务端不是 ${status}（实测 ${srv.status}）✗ ⇒ 权威口径变了 ⇒ 改判据＋台账`);
  check(srv.status === loc.status, `§F ${label}：状态不同 ✗ 服务端 ${srv.status}｜本地 ${loc.status}`);
  const sDetail = srv.body && srv.body.context && srv.body.context.detail;
  const lDetail = loc.body && loc.body.context && loc.body.context.detail;
  if (detail !== null) check(sDetail === detail, `§F ${label}：服务端文案变了 ✗ 实测 ${show(sDetail)}`);
  check(lDetail === sDetail, `§F ${label}：错误文案不同 ✗ 服务端 ${show(sDetail)}｜本地 ${show(lDetail)}`);
  const sCtx = Object.keys((srv.body && srv.body.context) || {}).sort().join(",");
  const lCtx = Object.keys((loc.body && loc.body.context) || {}).sort().join(",");
  check(sCtx === lCtx,
    `§F ${label}：context 键集不同 ✗ 服务端 [${sCtx}]｜本地 [${lCtx}]`
    + "（**服务端 `log.rs:378` 的 `with_atom` 会带 `atom_id` ✓ 本地少了就是虚报少给 ✓**）");
  if (status === 404) {
    const afterL = (await atomsLoc()).atoms.length;
    check(afterL === beforeL, `§F ${label}：本地挂接被拒却写了原子 ✗ ${beforeL}⇒${afterL}`);
  }
}
{
  // **∴「挂上去」的判据不是回执 ✗ 是日志里那条 `set_property` 的载荷 ✓**。
  const lastSet = async (which) => {
    const rows = which === "srv" ? (await atomsSrv()).atoms : (await atomsLoc()).atoms;
    for (let i = rows.length - 1; i >= 0; i--) {
      const a = rows[i].atom || rows[i];
      if (a.kind === "set_property" && a.payload && a.payload.key === "mask_id") return a.payload;
    }
    return null;
  };
  const pS = await lastSet("srv"), pL = await lastSet("loc");
  check(!!pS && !!pL && canon(pS) === canon(pL),
    `§F：两边最后一条 mask_id 挂接原子不同 ✗ 服务端 ${show(pS)}｜本地 ${show(pL)}`
    + " ⇒ 回执都 ok 而挂的东西不一样＝形态③ ✓");
}

// **§G 已知分叉 ✗**（**打印出来 ✓ 不假装比过 ✓**——AGENTS「静默降级」反面就是「静默不提」✗）：
console.log("\n### 已知分叉（不断言，只登记）");
console.log("   · **PNG 容器字节**：服务端 `miniz_oxide` level 7 vs 浏览器 `CompressionStream("
  + "deflate)` 缺省级别 ⇒ **同一张网格两条流**✓ 实测首个不同偏移 = 42（zlib 的 FLG）⇒"
  + " **本判据比的是解出的像素 ✓ 容器只打印**（**blob 哈希因此两边不同 ⇒ 也不断言 ✓**）");
console.log("   · **内存预算闸（`resource_exhausted` ⇒ 503）本地不可达**✓：本地画布上限 16384 像素 ⇒"
  + " 估算最大 1280 MB < 缺省预算 2048 MB ✓ 而浏览器里没有 `YANSHI_DOC_MEMORY_BUDGET_MB` ⇒"
  + " **那条分支在本地是死路 ⇒ 判据不编造输入去触发它 ✓**（服务端侧同一句话由 `tool-*` 判据与服务端单测守着 ✓）");
console.log("   · **`update_layer{patch:{mask_id:…}}` 那条拒绝仍少 `context.atom_id`**✗（**它逐键组原子 ⇒"
  + " 守卫跑在封套之前 ✓**）：`log.rs:378` 带 `with_atom` ⇒ 已记进台账；"
  + "**`set_property` 端点（面板挂接的主路径）已补齐 ✓ 上面 §F 在断它 ✓**");

if (bad.length) {
  console.log(`\n✗ ${bad.length} 条断言不满足（本案数：错误 ${ERR_CASES.length}＋成功 ${OK_CASES.length}＋挂接 ${attachCases.length}）：`);
  for (const line of bad) console.log("  ✗ " + line);
  process.exit(1);
}
console.log(`\n✓ create_range_mask 本地／服务端逐像素与逐键同（错误 ${ERR_CASES.length} 案＋成功 ${OK_CASES.length} 案＋挂接 ${attachCases.length} 案）`);
