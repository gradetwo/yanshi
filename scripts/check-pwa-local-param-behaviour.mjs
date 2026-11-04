#!/usr/bin/env node
// **PWA 本地层「声明参数」的行为级判据**（2026-11-04）。
//
// **为什么需要它** ✗：`check-three-way-alignment.mjs` 的「声明参数本地未读」与
//   `check-pwa-local-params.mjs` 的白名单都只是**静态**判据 ✓
//   —— 它们只能证明「参数名在文件里出现过」✗，**证明不了行为变了** ✓。
//   本判据在 node 里真的执行 `web/api-local.js` 的 `makeLocalApi` ✓，对每个已接的参数
//   断言三件事：
//     (a) **不传 ⇒ Rust 缺省行为**（缺省值必须与服务端一致，不是「本地以前那样」✓）；
//     (b) **传非缺省值 ⇒ 输出可观察地不同**（否则它只是「读了但没用」✗）；
//     (c) **非法值 ⇒ 400 `invalid_argument` 形状**（`error`／`error_code`／`context.detail` ✓）。
//   外加**恒等式**（内核 ②：单点断言只对一次输入成立 ✓）。
//
// **覆盖的参数**（4 个工具 / 7 个参数 ✓）：
//   · `batch.slim_receipt`                → `tools.rs:23591`
//   · `smudge.deposit`                    → `tools.rs:13836`
//   · `brush_preview.include_image`       → `tools.rs:1535`（`wants_return_image`）
//   · `brush_preview.return_image`        → 同上
//   · `brush_preview.max_px`              → `tools.rs:1542`（钳 1..=8192）
//   · `create_mask.from_selection`        → `tools_mask.rs:638`
//   · `create_selection.from_mask`        → `tools_selection.rs:82`
//
// **不测什么**：不测像素（`brush_preview` 用注入的假内核 ⇒ 只验「图怎么给、给多大」✓；
//   抹匀的暗化是共享 Rust 渲染器 `render.rs:1260` 的事，本地只负责把 `deposit` 写进原子 ✓）。
//
// 退出码：0 = 全绿｜1 = 断言红｜3 = 判据前提不成立（自身出错／环境缺失，不是产品问题）
process.on("uncaughtException", (error) => {
  console.error(`  ⊘ 判据自身出错（不是产品问题）：${String(error).slice(0, 200)}`);
  process.exit(3);
});
import { pathToFileURL } from "node:url";

const { makeIndexedDB } = await import(
  pathToFileURL(process.cwd() + "/scripts/lib/pwa-node-stub.mjs").href
);

const stub = (name, value) =>
  Object.defineProperty(globalThis, name, { value, writable: true, configurable: true });
stub("indexedDB", makeIndexedDB());
stub("location", { origin: "https://yanshi-online.wangda.today",
                   href: "https://yanshi-online.wangda.today/" });
stub("document", { fonts: { forEach() {} } });
stub("navigator", { gpu: undefined });
stub("window", { fetch: () => Promise.resolve(new Response("{}", { status: 200 })) });
// `brush_preview` 的 `thumb_url` 走 `URL.createObjectURL`（node 里没有 ✓ ⇒ 打桩 ✓）。
try {
  if (typeof URL.createObjectURL !== "function") {
    URL.createObjectURL = () => "blob:stub";
  }
} catch (e) { /* 不可写就算了：下面会以 500 暴露 ⇒ 判据红 */ }

const base = pathToFileURL(`${process.cwd()}/web/`).href;
const api = await import(base + "api-local.js");
const { open } = await import(base + "store.js");

// ---- 假内核（`brush_preview` 只要一条「画出了 RGBA」的路 ✓）----
api.__setLocalApiLoadersForTest({
  wasm: async () => ({
    default: async () => {},
    WasmKernel: class { constructor(doc, tile, w, h) { this.doc = doc; this.w = w; this.h = h; } },
  }),
  brushLocal: async () => ({
    fetchBrushText: async () => "#stub myb",
    // **签名照 `brush-local.js:124`**：`paintWithKernel(kernel, req)` ✓（两个参数 ✓）。
    paintWithKernel: async (_kernel, { region }) => {
      const out = new Uint8Array(region.w * region.h * 4);
      for (let i = 0; i < out.length; i += 4) {
        out[i] = 200; out[i + 1] = 40; out[i + 2] = 10; out[i + 3] = 255;
      }
      return out;
    },
  }),
});

const db = await open();
const local = await api.makeLocalApi(db);
const failures = [];
const check = (ok, label, detail) => {
  console.log(`  ${ok ? "✓" : "✗"} ${label}${detail ? `（${detail}）` : ""}`);
  if (!ok) failures.push(label + (detail ? `：${detail}` : ""));
};
const post = (path, body) =>
  local(new Request(`https://x${path}`, { method: "POST", body: JSON.stringify(body) }));
const get = (path) => local(new Request(`https://x${path}`));
const jpost = async (path, body) => {
  const res = await post(path, body);
  return { status: res.status, body: await res.json() };
};
const isInvalid = (r) =>
  r.status === 400 && r.body.ok === false && r.body.error === "invalid_argument"
  && r.body.error_code === "invalid_argument" && typeof r.body.context?.detail === "string";

const DOC = "d-local-params";
const created = await jpost(`/api/documents?doc=${DOC}`, { doc_id: DOC, width: 120, height: 90 });
check(created.body.ok === true, "前置：创建文档", JSON.stringify(created.body).slice(0, 120));

// ============================ ① batch.slim_receipt ============================
console.log("\n① batch.slim_receipt");
const batchArgs = {
  calls: [
    { tool: "create_layer", args: { layer_id: "L_a", name: "a" } },
    { tool: "create_layer", args: { layer_id: "L_b", name: "b" } },
  ],
};
const normal = await jpost(`/api/tools/batch?doc=${DOC}`, batchArgs);
check(normal.body.ok === true && Array.isArray(normal.body.results)
  && normal.body.results.length === 2, "不传 ⇒ 完整逐笔回执（results 2 条）",
  `executed=${normal.body.executed} results=${normal.body.results?.length}`);
check(!("slim_receipt" in normal.body) && !("omitted_calls" in normal.body),
  "不传 ⇒ 不出现 slim_receipt／omitted_calls（缺省 false 不留痕）");

const slimArgs = { ...batchArgs, slim_receipt: true,
                   calls: [
                     { tool: "create_layer", args: { layer_id: "L_c", name: "c" } },
                     { tool: "create_layer", args: { layer_id: "L_d", name: "d" } },
                   ] };
const slim = await jpost(`/api/tools/batch?doc=${DOC}`, slimArgs);
check(slim.body.ok === true && slim.body.slim_receipt === true,
  "slim_receipt:true ⇒ 回执带 slim_receipt:true", JSON.stringify(slim.body).slice(0, 140));
check(slim.body.results.length === 0 && slim.body.calls.length === 0,
  "slim_receipt:true ⇒ 逐笔回执清空（results 与 calls 都是 []）",
  `results=${slim.body.results.length} calls=${slim.body.calls.length}`);
check(slim.body.omitted_calls === slim.body.executed && slim.body.omitted_calls === 2,
  "恒等式：omitted_calls === executed === 2",
  `omitted=${slim.body.omitted_calls} executed=${slim.body.executed}`);
check(slim.body.omitted_calls + slim.body.results.length === slim.body.executed,
  "恒等式：omitted_calls + results.length === executed");
// **执行照旧** ✓：slim 只砍回执，子调用照常落笔 ✓。
const layersAfter = await (await get(`/api/tools/list_layers?doc=${DOC}`)).json();
const layerIds = (layersAfter.layers || []).map((l) => l.layer_id);
check(layerIds.includes("L_c") && layerIds.includes("L_d"),
  "slim_receipt:true 只改回执形状、不改执行（L_c／L_d 都建了）", JSON.stringify(layerIds));

const badSlim = await jpost(`/api/tools/batch?doc=${DOC}`, { ...batchArgs, slim_receipt: "yes" });
check(isInvalid(badSlim), "非法 slim_receipt（字符串）⇒ 400 invalid_argument",
  `${badSlim.status} ${JSON.stringify(badSlim.body).slice(0, 120)}`);

const slimFailed = await jpost(`/api/tools/batch?doc=${DOC}`, {
  slim_receipt: true,
  calls: [{ tool: "no_such_tool", args: {} }],
});
check(slimFailed.body.failed_calls.length === 1 && slimFailed.body.ok === false,
  "slim 也保留失败项（failed_calls 1 条 ⇒ 不是「砍成什么都看不见」）",
  JSON.stringify(slimFailed.body).slice(0, 160));

// ============================ ② smudge.deposit ============================
console.log("\n② smudge.deposit");
const smudgeBase = { layer_id: "L_a", points: [[10, 10], [20, 20]], size: 8 };
const noDeposit = await jpost(`/api/tools/smudge?doc=${DOC}`, smudgeBase);
check(noDeposit.body.ok === true, "不传 ⇒ 成功", JSON.stringify(noDeposit.body).slice(0, 120));
const withDeposit = await jpost(`/api/tools/smudge?doc=${DOC}`,
  { ...smudgeBase, deposit: 0.75 });

const readRetouch = async () => {
  const log = await (await get(`/api/tools/get_log?doc=${DOC}&kind=retouch&order=asc`)).json();
  return (log.entries || log.atoms || []).map((e) => e.atom || e);
};
const retouch = await readRetouch();
const d0 = retouch[0]?.payload?.data?.deposit;
const d1 = retouch[1]?.payload?.data?.deposit;
check(d0 === 0, "不传 deposit ⇒ 原子 data.deposit === 0（Rust 缺省 0.0）", `deposit=${d0}`);
check(d1 === 0.75, "deposit:0.75 ⇒ 原子 data.deposit === 0.75（渲染器据此按 1-d*0.35 暗化）",
  `deposit=${d1}`);
check(retouch.length === 2 && retouch.every((a) => typeof a.payload?.data?.deposit === "number"),
  "恒等式：每条 retouch 原子都带数字 deposit（不再静默缺字段）", `条数=${retouch.length}`);
check(retouch.every((a) => a.payload?.data?.smudge_length === 12),
  "既有缺省未被改动：smudge_length 仍是 12");

const badDeposit = await jpost(`/api/tools/smudge?doc=${DOC}`,
  { ...smudgeBase, deposit: "abc" });
check(isInvalid(badDeposit), "非法 deposit（字符串）⇒ 400 invalid_argument",
  `${badDeposit.status} ${JSON.stringify(badDeposit.body).slice(0, 120)}`);

// ================== ③ brush_preview.include_image / return_image / max_px ==================
console.log("\n③ brush_preview.include_image／return_image／max_px");
const previewBase = { brush: "round.myb", size: 24 };
const plain = await jpost(`/api/tools/brush_preview?doc=${DOC}`, previewBase);
check(plain.body.ok === true && plain.body.image === undefined,
  "不传 ⇒ 不回图（image 不出现，只有 thumb_url／blob_hash）",
  JSON.stringify(Object.keys(plain.body)));
check(typeof plain.body.thumb_url === "string" && typeof plain.body.blob_hash === "string",
  "不传 ⇒ thumb_url／blob_hash 照旧");

const inc = await jpost(`/api/tools/brush_preview?doc=${DOC}`,
  { ...previewBase, include_image: true });
check(inc.body.image?.mime_type === "image/png" && inc.body.image.data.length > 0,
  "include_image:true ⇒ 内嵌 image{data}", `data=${inc.body.image?.data?.length}`);
check(inc.body.image.width === inc.body.width && inc.body.image.height === inc.body.height,
  "恒等式：image.width/height === 预览的 width/height（未超 max_px ⇒ 原尺寸）",
  `${inc.body.image?.width}×${inc.body.image?.height} vs ${inc.body.width}×${inc.body.height}`);

const ret = await jpost(`/api/tools/brush_preview?doc=${DOC}`,
  { ...previewBase, return_image: true });
check(ret.body.image?.width === ret.body.width && ret.body.scaled_from === undefined,
  "return_image:true 且不超 512 ⇒ 原尺寸内嵌、无 scaled_from",
  JSON.stringify(ret.body.image && { w: ret.body.image.width }));

const big = await jpost(`/api/tools/brush_preview?doc=${DOC}`,
  { ...previewBase, return_image: true, max_px: 8 });
const longEdge = Math.max(big.body.width, big.body.height);
const expectW = Math.max(1, Math.round(big.body.width * 8 / longEdge));
const expectH = Math.max(1, Math.round(big.body.height * 8 / longEdge));
check(big.body.image?.width <= 8 && big.body.image?.height <= 8,
  "max_px:8 ⇒ 内嵌图长边 ≤ 8", `${big.body.image?.width}×${big.body.image?.height}`);
check(big.body.image?.width === expectW && big.body.image?.height === expectH,
  "恒等式：缩放尺寸 = round(原尺寸 × max_px / 长边)",
  `期望 ${expectW}×${expectH} 实测 ${big.body.image?.width}×${big.body.image?.height}`);
check(big.body.scaled_from?.[0] === big.body.width && big.body.max_px === 8,
  "缩小必须说出来：scaled_from 是原尺寸、max_px 回显 8",
  `${JSON.stringify(big.body.scaled_from)} max_px=${big.body.max_px}`);

for (const [label, patch] of [
  ["max_px:-1", { max_px: -1 }],
  ["max_px:2.5", { max_px: 2.5 }],
  ["include_image:\"yes\"", { include_image: "yes" }],
]) {
  const bad = await jpost(`/api/tools/brush_preview?doc=${DOC}`, { ...previewBase, ...patch });
  check(isInvalid(bad), `非法 ${label} ⇒ 400 invalid_argument`,
    `${bad.status} ${JSON.stringify(bad.body).slice(0, 100)}`);
}

// ============ ④ create_mask.from_selection / create_selection.from_mask ============
console.log("\n④ create_mask.from_selection／create_selection.from_mask");
const rectShape = { kind: "rect", x: 10, y: 20, w: 30, h: 40 };
const mkSel = await jpost(`/api/tools/create_selection?doc=${DOC}`, {
  selection_id: "s_rect", shape: rectShape, feather: 5, invert: true,
});
check(mkSel.body.ok === true, "前置：create_selection 建 s_rect",
  JSON.stringify(mkSel.body).slice(0, 100));

const maskFromSel = await jpost(`/api/tools/create_mask?doc=${DOC}`,
  { mask_id: "m_from_sel", from_selection: "s_rect" });
check(maskFromSel.body.ok === true, "from_selection 建蒙版（不给 shape 也能成）",
  JSON.stringify(maskFromSel.body).slice(0, 140));

const canvas1 = await (await get(`/api/tools/describe_canvas?doc=${DOC}`)).json();
const m1 = (canvas1.masks || []).find((m) => m.mask_id === "m_from_sel");
check(m1 && JSON.stringify(m1.shape) === JSON.stringify(rectShape),
  "克隆了选区的 shape", JSON.stringify(m1?.shape));
check(m1?.feather === 5, "克隆了选区的 feather（5）", `feather=${m1?.feather}`);

const maskLatest = await jpost(`/api/tools/create_mask?doc=${DOC}`,
  { mask_id: "m_latest", from_selection: "latest" });
const canvas2 = await (await get(`/api/tools/describe_canvas?doc=${DOC}`)).json();
const mLatest = (canvas2.masks || []).find((m) => m.mask_id === "m_latest");
check(maskLatest.body.ok === true && JSON.stringify(mLatest?.shape) === JSON.stringify(rectShape),
  "from_selection:\"latest\" ⇒ 取最后创建的活选区（s_rect）", JSON.stringify(mLatest?.shape));

const ellipseShape = { kind: "ellipse", cx: 50, cy: 50, rx: 10, ry: 6 };
const maskExplicit = await jpost(`/api/tools/create_mask?doc=${DOC}`, {
  mask_id: "m_explicit", from_selection: "s_rect", shape: ellipseShape, feather: 2,
});
const canvas3 = await (await get(`/api/tools/describe_canvas?doc=${DOC}`)).json();
const mExp = (canvas3.masks || []).find((m) => m.mask_id === "m_explicit");
check(maskExplicit.body.ok === true && JSON.stringify(mExp?.shape) === JSON.stringify(ellipseShape)
  && mExp?.feather === 2, "显式 shape／feather 优先于克隆值", JSON.stringify(mExp?.shape));

const noSuchSel = await jpost(`/api/tools/create_mask?doc=${DOC}`,
  { mask_id: "m_nope", from_selection: "nope" });
check(noSuchSel.status === 404 && noSuchSel.body.error_code === "reference_not_found"
  && noSuchSel.body.reason === "选区 nope 不存在",
  "from_selection 指向不存在的选区 ⇒ 404 ＋ 逐字「选区 nope 不存在」",
  `${noSuchSel.status} ${noSuchSel.body.reason}`);

const noShapeNoFrom = await jpost(`/api/tools/create_mask?doc=${DOC}`, { mask_id: "m_bare" });
check(isInvalid(noShapeNoFrom), "既不传 shape 也不传 from_selection ⇒ 400（旧行为不变）",
  `${noShapeNoFrom.status} ${JSON.stringify(noShapeNoFrom.body).slice(0, 100)}`);

const maskEllipse = await jpost(`/api/tools/create_mask?doc=${DOC}`, {
  mask_id: "m_ellipse", shape: ellipseShape, feather: 3, invert: false,
});
const selFromMask = await jpost(`/api/tools/create_selection?doc=${DOC}`,
  { selection_id: "s_from_mask", from_mask: "m_ellipse" });
const selList = await (await get(`/api/tools/list_selections?doc=${DOC}`)).json();
const sFrom = (selList.selections || []).find((s) => s.selection_id === "s_from_mask");
check(maskEllipse.body.ok === true && selFromMask.body.ok === true,
  "from_mask 建选区（不给 shape 也能成）", JSON.stringify(selFromMask.body).slice(0, 120));
check(sFrom && JSON.stringify(sFrom.shape) === JSON.stringify(ellipseShape),
  "克隆了蒙版的 shape", JSON.stringify(sFrom?.shape));
check(sFrom?.feather === 3 && sFrom?.invert === false,
  "克隆了蒙版的 feather（3）与 invert（false）",
  `feather=${sFrom?.feather} invert=${sFrom?.invert}`);

const selExplicitInvert = await jpost(`/api/tools/create_selection?doc=${DOC}`,
  { selection_id: "s_from_mask_inv", from_mask: "m_ellipse", invert: true });
const selList2 = await (await get(`/api/tools/list_selections?doc=${DOC}`)).json();
const sInv = (selList2.selections || []).find((s) => s.selection_id === "s_from_mask_inv");
check(selExplicitInvert.body.ok === true && sInv?.invert === true,
  "显式 invert:true 优先于克隆值（false）", `invert=${sInv?.invert}`);

const noSuchMask = await jpost(`/api/tools/create_selection?doc=${DOC}`,
  { selection_id: "s_nope", from_mask: "nope" });
check(noSuchMask.status === 404 && noSuchMask.body.error_code === "reference_not_found"
  && noSuchMask.body.reason === "蒙版 nope 不存在",
  "from_mask 指向不存在的蒙版 ⇒ 404 ＋ 逐字「蒙版 nope 不存在」",
  `${noSuchMask.status} ${noSuchMask.body.reason}`);

// **墓碑不参与 "latest"** ✓（服务端 `filter(|s| !s.is_deleted())` ✓）。
await jpost(`/api/tools/delete_selection?doc=${DOC}`, { selection_id: "s_from_mask" });
const maskAfterTombstone = await jpost(`/api/tools/create_mask?doc=${DOC}`,
  { mask_id: "m_latest2", from_selection: "latest" });
const canvas4 = await (await get(`/api/tools/describe_canvas?doc=${DOC}`)).json();
const mLatest2 = (canvas4.masks || []).find((m) => m.mask_id === "m_latest2");
check(maskAfterTombstone.body.ok === true
  && JSON.stringify(mLatest2?.shape) === JSON.stringify(ellipseShape),
  "latest 跳过已删选区（s_from_mask 墓碑后仍取到活着的 s_from_mask_inv 的椭圆）",
  JSON.stringify(mLatest2?.shape));

// ============================ 汇总 ============================
if (failures.length > 0) {
  console.error(`\n✗ PWA 本地声明参数判据红 ${failures.length} 条`);
  process.exit(1);
}
console.log("\n✓ PWA 本地声明参数行为与恒等式均成立（batch.slim_receipt／smudge.deposit／"
  + "brush_preview.include_image·return_image·max_px／create_mask.from_selection／"
  + "create_selection.from_mask）");
