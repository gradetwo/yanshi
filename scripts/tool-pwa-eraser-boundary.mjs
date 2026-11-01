#!/usr/bin/env node
// **PWA 橡皮擦能力边界（Y26-001 路线 B）** ✗ —— 一条**差分包**，断的是「服务端能擦 / 本地如实不擦」这个**分歧本身** ✓
//
// **∴ 为什么单独开一条 ✗**：`tool-pwa-local-write-guards.mjs`（§A～§E）用的是**纯 node 桩**（`bootPwaLocalApi`）
//   ⇒ 它**不需要真内核** ✓（全是图层／对象／选区的折叠语义）。而本条要跑**真 wasm 门面**的
//   `brush_is_pure_eraser` ⇒ 必须 `bootLocalWithRealKernel` ＋ **内核新鲜度闸**（旧 ⇒ `exit(3)`）。
//   ⇒ **∴ 所以不并进 §A～§E**：那会把那五条**无关用例**绑死在「wasm 必须重建」上 ✗（一 stale 就整片 `exit(3)`，丢覆盖 ✓）。
//
// **∴ 三段各断什么 ✗**（**每段都能红**，见下「变异」）：
//   §1 服务端：铺一层墨后画**纯橡皮** ⇒ `200/ok` ＋ 该对象带 `data.blend:"erase"`（走 destination-out 擦除 ✓）；
//   §2 wasm 门面：**真内核**里判 `Hard_Eraser` ⇒ `pure_eraser:true`、`100%_Opaque` ⇒ `false`、非 .myb ⇒ `ok:false`
//      ⇒ 证明**本地守卫读的这把尺子与服务端同源**（`yanshi_render::brush::brush_is_pure_eraser` ✓）；
//   §3 本地 JS 守卫：`web/api-local.js` 的 `brush_stroke`/`draw_stroke` 落笔分支里，在真正 `paintWithKernel` **之前**
//      必须有一道「纯橡皮 ⇒ `501 not_implemented_locally` 且**没有写入任何原子**」的闸（读 §2 那个门面值 ⇒ 纯橡皮就回 501）。
//
// **∴ 为什么 §3 是「结构＋位置」而不是端到端 501 ✗**：落笔分支走 `paintWithKernel`（要 `.myb` 光栅 ＋ fetch `/brushes/…`）
//   ⇒ node 里**跑不到那一步**（`brush-local.js` 不在内核接缝里，见 `pwa-node-kernel.mjs` 只注入 `wasm`）。
//   ⇒ **∴ 于是**把「决策的输入」交给 §2（门面对纯橡皮**确实**回 `true` ✓）、把「读到 true 就 501、且写在落笔之前」交给 §3 ✓
//     —— 两段合起来 = 「纯橡皮不会在这里被静默画成覆盖」✓。**两面（AGENTS 第 3 条）**：
//     **收益**：不碰 shipped 的落笔分支、不扩接缝面 ✓；**代价**：§3 是位置断言，不是端到端 501 ✓。
//
// **∴ 变异（每条都验过能红 ✓）**：
//   · §1：把 `tools_brush.rs` 的 `is_erase` 门变成恒假 ⇒ 对象不带 `blend:"erase"` ⇒ §1 红 ✓；
//   · §2：改坏 `yanshi_render::brush::brush_is_pure_eraser`（如去掉 eraser 判定）⇒ 门面对 `Hard_Eraser` 回 `false` ⇒ §2 红 ✓；
//   · §3：删掉落笔分支里那道 501 守卫 ⇒ 「是纯橡皮」串不在「分支→落笔」窗口内 ⇒ §3 红 ✓。
import { pathToFileURL } from "node:url";
import { readFileSync } from "node:fs";

const BASE = process.argv[2];
if (!BASE || !/^https?:\/\//.test(BASE)) {
  console.log("⊘ 不可判：argv[2] 缺服务端地址 ⇒ 请用 bash scripts/run-criteria-local.sh scripts/tool-pwa-eraser-boundary.mjs 跑"
    + "（判据前提不成立，不是产品问题）");
  process.exit(3);
}

// **内核新鲜度闸 ✓**（照 `tool-pwa-atom-log.mjs` 同一口径 ✗）：web/wasm 比源码旧 ⇒ 测到的是上一版内核 ✗ ⇒ `exit(3)`。
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
const UNIQ = "e" + Date.now().toString(36);
const SERVER_DOC = `esrv-${UNIQ}`;
const nativeFetch = globalThis.fetch;

const created = await nativeFetch(`${BASE}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: SERVER_DOC, width: 400, height: 300 }),
}).then((r) => r.json());
if (created.ok !== true || !created.token) {
  console.log(`⊘ 不可判：服务端建文档失败（${JSON.stringify(created).slice(0, 160)}）⇒ 前提不成立，不是产品问题`);
  process.exit(3);
}
const TOKEN = created.token;
const callServer = async (tool, args) => {
  const res = await nativeFetch(`${BASE}/api/tools/${tool}?token=${TOKEN}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: SERVER_DOC, actor: "human:judge", session: "session:judge", ...args }),
  });
  return { status: res.status, body: await res.json().catch(() => null) };
};
const mybFile = (name) => readFileSync(`${process.cwd()}/assets/brushes/${name}`, "utf8");

// **════ §1 服务端：纯橡皮笔触 ⇒ 擦除路（对象带 blend:"erase"）════**
{
  await callServer("create_layer", { layer_id: "L1", name: "橡皮层" });
  // 先铺一层不透明红墨 ⇒ 橡皮才有东西可擦（纯橡皮在空层上 `painted=0`，走的是「无墨诊断」那条，不是擦除对象 ✓）。
  const ink = await callServer("brush_stroke", {
    layer_id: "L1", brush: "100%_Opaque.myb", points: [[30, 30, 1], [120, 90, 1]], size: 40, color: "#ff0000",
  });
  check(ink.status === 200 && ink.body && ink.body.ok === true,
    `§1 前提：服务端铺墨失败 ✗ ${ink.status} / ${JSON.stringify(ink.body).slice(0, 120)}`);
  const erase = await callServer("brush_stroke", {
    layer_id: "L1", brush: "Hard_Eraser.myb", points: [[40, 40, 1], [110, 80, 1]], size: 30,
  });
  check(erase.status === 200 && erase.body && erase.body.ok === true,
    `§1 服务端对纯橡皮应 200/ok（走 destination-out 擦除）✗ 实测 ${erase.status} / `
    + `${erase.body && (erase.body.error_code || erase.body.ok)} ⇒ 若上游改了橡皮语义，该跟着改判据（不是改本地）`);
  // **擦除对象必须带 blend:"erase"** ✗（不带 ⇒ fold 侧按 source-over 把系数缓冲当黑墨贴回去 ✓）。
  let blend = null;
  const oid = erase.body && erase.body.object_id;
  if (oid) {
    const got = await callServer("get_object", { object_id: oid });
    blend = got.body && got.body.data && got.body.data.blend;
  }
  if (blend !== "erase") {
    // 回退：整层里找一个带 erase 的对象（服务端把擦除对象 id 挪到别处也不放过 ✓）。
    const ls = await callServer("list_objects", {});
    for (const o of ((ls.body && ls.body.objects) || [])) {
      const g = await callServer("get_object", { object_id: o.object_id });
      if (g.body && g.body.data && g.body.data.blend === "erase") { blend = "erase"; break; }
    }
  }
  check(blend === "erase",
    `§1 服务端的纯橡皮对象应带 data.blend:"erase"（擦除标记）✗ 实测 ${JSON.stringify(blend)}｜object_id=${oid}`);
}

// **════ §2 wasm 门面：与服务端同一把尺子（真内核里跑 brush_is_pure_eraser）════**
{
  const kernel = await kernelLib.realKernelModule();
  const k = new kernel.WasmKernel("erase-facade", 256, 400, 300, 8 * 1024 * 1024);
  const envE = JSON.parse(k.brush_is_pure_eraser(mybFile("Hard_Eraser.myb")));
  check(envE && envE.ok === true && envE.pure_eraser === true,
    `§2 wasm 门面应判 Hard_Eraser 为纯橡皮 ✗ 实测 ${JSON.stringify(envE)}`);
  const envN = JSON.parse(k.brush_is_pure_eraser(mybFile("100%_Opaque.myb")));
  check(envN && envN.ok === true && envN.pure_eraser === false,
    `§2 wasm 门面不该把普通不透明白笔误判成橡皮 ✗ 实测 ${JSON.stringify(envN)}`);
  const envBad = JSON.parse(k.brush_is_pure_eraser("这不是一支 .myb"));
  check(envBad && envBad.ok === false,
    `§2 门面遇到非 .myb 必须 ok:false（不能静默当普通笔放行）✗ 实测 ${JSON.stringify(envBad)}`);
  if (typeof k.free === "function") k.free();
}

// **════ §3 本地 JS 守卫：落笔分支里、真正落笔之前的「纯橡皮 ⇒ 501、不写原子」════**
{
  const apiSrc = readFileSync(`${process.cwd()}/web/api-local.js`, "utf8");
  const branch = apiSrc.indexOf('path === "/api/tools/brush_stroke"');
  check(branch > 0, "§3 找不到 brush_stroke/draw_stroke 落笔分支 ✗ ⇒ 上游改了路径分支，这条判据要跟着改");
  const paint = apiSrc.indexOf("paintWithKernel(entryK.k", branch);
  check(paint > branch, "§3 找不到该分支里的真正落笔（paintWithKernel(entryK.k…）✗ ⇒ 落笔写法变了，位置断言口径要跟着改");
  const window = apiSrc.slice(branch, paint > 0 ? paint : branch + 4000);
  check(window.includes("是纯橡皮"),
    "§3 落笔分支里必须在 paintWithKernel **之前**先判「纯橡皮」✗（删掉它 = 擦除被静默画成覆盖 ✓）");
  check(window.includes("not_implemented_locally"),
    "§3 纯橡皮在本地必须明确 501 not_implemented_locally ✗（不许回 200 假装擦过）");
  check(window.includes("brush_is_pure_eraser"),
    "§3 守卫必须读同源判定 brush_is_pure_eraser（wasm 门面）✗ 不能另起一把尺子");
  check(window.includes("没有写入任何原子"),
    "§3 守卫必须说清「没有写入任何原子」（画布保持原样）✗（否则调用方不知道状态到底动没动）");
}

if (bad.length) {
  console.log(`✗ PWA 橡皮擦能力边界（服务端擦除 / 本地如实 501 不写原子）：${bad.length} 条不符`);
  for (const m of bad) console.log("   · " + m);
  process.exit(1);
}
console.log("✓ PWA 橡皮擦能力边界：服务端纯橡皮带 blend:erase／wasm 门面同源判纯橡皮／本地守卫如实 501 且一个原子都不写");
