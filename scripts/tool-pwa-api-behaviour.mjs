#!/usr/bin/env node
// **★ PWA 本地 API 的**行为级**判据 ✓ ★**（第 617 轮 ✓）
//
// **∴ 为什么需要它 ✗**：**`tool-pwa-assets.mjs` 只能证明"文件里写了什么"✗**（**字符串级 ✓**）
//   ⇒ **∴ 它证明不了"真的能用"✗** ⇒ **∴ 而"lazy 不许变成撒谎"要求**能跑的证据 ✗**
//   ⇒ **∴ 本判据在 node 里**打桩 `window`／`indexedDB`✗** ⇒ **∴ 真的执行
//   `installLocalApi` ＋ `makeLocalApi` ✗** ⇒ **∴ 断言**真实返回值 ✓****。
//
// **∴ 覆盖 ✗**：**① 静态资源走原 fetch ✓；② `/health` 如实报"无服务器"✗；
//   ③ `/api/documents` 创建 ✓；④ `/api/atoms` 写读 ✓；⑤ `list_layers` 从原子推导 ✓；
//   ⑥ `get_document` 报 `preview_state=pending` ＋ 后端 ＋ 原因 ✓；
//   ⑦ **未实现端点 ⇒ 501 ＋ 端点名 ＋ 原因 ✗**（**∴ 不静默失败 ✓**）** ✓✓
//
// **变异** ✗：**去掉 501 分支 ⇒ ⑦ 报红 ✓**（**已实测 ✓**）。
import { pathToFileURL } from "node:url";

// **★ 最小 IndexedDB 桩 ✓ ★**：**够用即可 ✗**（**只支持本层用到的 put／get／getAll ✓**）。
class MemStore {
  constructor() { this.map = new Map(); }
  // **∴ 键按 `store.js` 的 `keyPath` 语义取 ✗**：**`atoms`⇒`id` ✓｜`blobs`⇒`hash` ✓｜`docs`⇒`doc` ✓**。
  put(v) { this.map.set(v.id ?? v.hash ?? v.doc, v); return ok(); }
  get(k) { return ok(this.map.get(k)); }
  getAll() { return ok([...this.map.values()]); }
  createIndex() {}
}
const ok = (result) => { const r = { result, onsuccess: null, onerror: null }; queueMicrotask(() => r.onsuccess && r.onsuccess()); return r; };
function makeIndexedDB() {
  const stores = new Map();
  return {
    open() {
      const req = { result: null, onupgradeneeded: null, onsuccess: null, onerror: null };
      const db = {
        objectStoreNames: { contains: (n) => stores.has(n) },
        // **∴ 必须返回带 `createIndex` 的对象 ✗**（**`store.js` 会为 `atoms` 建索引 ✓**）。
        createObjectStore: (n) => { stores.set(n, new MemStore()); return { createIndex() {} }; },
        transaction: (n) => ({ objectStore: () => stores.get(n) ?? (stores.set(n, new MemStore()), stores.get(n)) }),
      };
      req.result = db;
      queueMicrotask(() => { req.onupgradeneeded && req.onupgradeneeded(); req.onsuccess && req.onsuccess(); });
      return req;
    },
  };
}

// **★ 打桩浏览器环境 ✓ ★**
const calls = [];
// **★ 打桩要用 `defineProperty` ✗ ★**（**实测：node 26 的 `globalThis.navigator` **只有 getter**✗
// ⇒ **∴ 直接赋值会 `TypeError: Cannot set property navigator` ✓**）。
const stub = (name, value) =>
  Object.defineProperty(globalThis, name, { value, writable: true, configurable: true });
stub("indexedDB", makeIndexedDB());
// The fetch override resolves relative paths against location.href, so the stub needs
// it as well as origin (its absence surfaced as ERR_INVALID_URL on "[object Object]").
stub("location", {
  origin: "https://yanshi-online.wangda.today",
  href: "https://yanshi-online.wangda.today/",
});
stub("navigator", { gpu: undefined });   // **∴ 模拟「无 WebGPU」⇒ 后端应为 cpu ✓**
stub("window", {
  fetch: (input) => { calls.push(String(input.url ?? input)); return Promise.resolve(new Response("{}", { status: 200 })); },
});

const base = pathToFileURL(`${process.cwd()}/web/`).href;
const { installLocalApi, makeLocalApi, LOCAL_IMPLEMENTED } = await import(base + "api-local.js");
const { open } = await import(base + "store.js");

const db = await open();
const local = await makeLocalApi(db);
const bad = [];
const check = (c, m) => { if (!c) bad.push(m); };
const call = (path, init = {}) => local(new Request(`https://x${path}?doc=d1`, { method: init.method ?? "GET", ...init }));

// **① 静态资源走原 fetch ✓**
await window.fetch({ url: "/wasm/yanshi_wasm.js" });
check(calls.some((u) => u.includes("/wasm/")), "静态资源未走原 fetch ✗");

// **②／③／④／⑤／⑥ 真实调用 ✓**
const created = await (await call("/api/documents", { method: "POST", body: JSON.stringify({ doc_id: "d1", width: 800, height: 600 }) })).json();
check(created.ok === true && created.server === false, `创建文档失败 ✗：${JSON.stringify(created)}`);
await call("/api/atoms", { method: "POST", body: JSON.stringify({ kind: "create_layer", layer_id: "L0", name: "图" }) });
const atoms = await (await call("/api/atoms")).json();
check(atoms.count >= 2, `原子写入后读回数量应 ≥ 2 ✗（实测 ${atoms.count}）`);
const layers = await (await call("/api/tools/list_layers")).json();
// **★ 必须断言**前端真正读的字段 ✗ ★**（第 928 轮 ✓）：**∴ 原来断言的是 `l.id` ✗**
//   ⇒ **∴ 而**前端读的是 `layer.layer_id` ✓（**`ensurePaintLayer` ✓）
//     ⇒ **∴ 那个错字段**让界面永远以为"**没有图层**" ✗ ⇒ **∴ 于是**它去建层 ⇒
//       **∴ 而**建层端点**当时**也没映射 ⇒ **∴ 画不了画 ✓**
//   ⇒ **∴ 所以**：**这条判据**改成断言 `layer_id` ✗** —— **∴ 它**更强 ✗**，
//     **且**它**本来就能**抓住这个 bug ✓**** ✓✓
check(layers.layers.some((l) => l.layer_id === "L0"),
  `list_layers 未从前端读的字段（layer_id）给出 L0 ✗：${JSON.stringify(layers)}`);
check(layers.derived_from === "atoms", "list_layers 未标明由原子推导 ✗");
const gd = await (await call("/api/tools/get_document")).json();
check(gd.width === 800 && gd.height === 600, `get_document 尺寸应取自创建原子 ✗（实测 ${gd.width}×${gd.height}）`);
check(gd.preview_state === "pending", `get_document 的 preview_state 应为 pending ✗（实测 ${gd.preview_state}）`);
check(gd.render_backend === "cpu", `无 WebGPU 时后端应为 cpu ✗（实测 ${gd.render_backend}）`);
check(gd.gpu_adapter_note === "host_has_no_webgpu", `应报 GPU 不可用原因 ✗（实测 ${gd.gpu_adapter_note}）`);

// **⑥b 快照分支：**冷启动必缺快照 ⇒ 必须**如实**说"该重算"✗，**不许返回旧图 ✓**
// **★ 要么真图、要么如实说该重算 ✗ ★**（第 630 轮 ✓）：**∴ 在本桩里快照可能命中 ⇒ 返回 PNG ✗**
// ⇒ **∴ 若仍死板地 `json()` 解析 ⇒ **∴ 会抛 "not valid JSON" ✗****（**实测 ✓**）。
// **∴ 断言改成**两者都接受**✗，**而**绝不允许第三种（**如空响应 ✓）**：
const rrRaw = await call("/api/tools/render_region", { method: "POST" });
const rrType = rrRaw.headers.get("content-type") || "";
if (rrType.startsWith("image/png")) {
  const bytes = (await rrRaw.arrayBuffer()).byteLength;
  check(bytes > 0, `快照路径应给出非空 PNG ✗（实测 ${bytes} 字节）`);
  console.log(`  快照路径给出真 PNG ✓（${bytes} 字节 ✓）`);
} else {
  const rr = await rrRaw.json();
  check(rr && rr.error === "needs_render", `内核不可用时应如实报 needs_render ✗（实测 ${JSON.stringify(rr).slice(0, 80)}）`);
  check(!!rr.reason && rr.reason.length > 4, `needs_render 应给出可读原因 ✗（实测 ${rr.reason}）`);
  console.log("  内核不可用 ⇒ 如实 needs_render ＋ 原因 ✓");
}

// **⑦ 未实现端点 ⇒ 501 ＋ 原因 ✓**（**这是"不撒谎"的核心 ✓**）
const wrapped = (() => { installLocalApi({ local }); return window.fetch; })();
// **∴ 用一个**确实仍未实现**的端点 ✗**（**`render_region` 已有快照分支 ✓ ⇒ 不再是 501 ✓**）。
// **★ ⑨ `/health` 必须声明 `wasm: true` ✗ ★**（第 911 轮 ✓；**用户从守卫反推出来的 ✓**）：
//   **∴ 为什么 ✗**：**viewer 的 `initWasm`** 有**一道守卫 ✗**：
//     ```js
//     const health = await (await fetch("/health")).json();
//     if (!health.wasm) throw new Error("服务端未启用（--no-wasm 或产物缺失）");
//     ```
//     ⇒ **∴ 而**本地层**只**声明了 `server:false` ✗** ⇒ **∴ 于是**：
//       **内核被**前端拒绝预热 ✗** ⇒ **∴ 整条本地路径**被跳过 ✓**** ✓✓
//       ⇒ **∴ 而**每一次落笔**都**报"**未知原因**" ✗**，**而**内核**其实可用 ✓**** ✓✓
//   **∴ 判据 ✗**：**`/health` 的响应**必须**含 `wasm: true` ✗**
//     ⇒ **∴ 变异（**手工 ✓）**：**删掉那个字段 ⇒ **∴ 本条**必红 ✓**** ✓✓
//   **∴ 它**守的是**契约字段** ✗**（**而不是**行为 ✗）⇒ **∴ 而**那**正是**这次断裂的地方 ✓**** ✓✓
// /health is answered by the fetch override, not by the dispatcher, so call it the way
// the page does (through the installed override) rather than through `local` directly.
// An absolute url is required here: the override builds a Request from it, and node
// (unlike a browser) rejects a relative one with ERR_INVALID_URL.
const healthRes = await window.fetch("https://yanshi-online.wangda.today/health");
const healthBody = await healthRes.json();
check(healthBody.wasm === true,
  "本地层 /health 必须声明 wasm:true（否则 viewer 的 initWasm 拒绝预热内核）（实测 " +
  JSON.stringify(healthBody).slice(0, 140) + "）");
check(healthBody.server === false,
  "本地层 /health 必须声明 server:false（如实：本部署没有服务端）（实测 " +
  String(healthBody.server) + "）");
console.log("    `/health` ⇒ wasm=" + healthBody.wasm + "｜server=" + healthBody.server +
  "｜backend=" + healthBody.render_backend);

// **★ ⑦ `/api/blob`：二进制上传必须成功 ✗ ★**（第 845 轮 ✓；**示例图上传正是这条路 ✓**）：
//   **∴ 为什么必须测它 ✗**：**`seedSampleIfEmpty` 靠它拿 `blob_hash` ✗**
//     ⇒ **∴ 而**它**曾经**因为**两个原因**失败 ✗**：
//       **a.** `req.json()` **把二进制 body 读成 `{}`** ✗（**第 841 轮修 ✓）**；
//       **b.** **本地层**根本没有这个分支**✗ ⇒ **501 ✓**（**第 844 轮修 ✓）** ✓✓
//   **∴ 判据 ✗**：**POST 一段**字节 ⇒ **期望 `ok:true` ＋ **`blob_hash` 以 `sha256:` 开头
//     ＋ **`size` 等于**字节数 ✗****（**∴ 三者缺一不可 ✓）** ✓✓
//   **∴ 变异（**手工 ✓）**：**删掉 `/api/blob` 分支 ⇒ **∴ 本断言**必红 ✓**
//     （**∴ 因为**它**会返回**501 ＋ `ok:false` ✓）** ✓✓
const pngBytes = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 1, 2, 3, 4]);
const blobRes = await local(
  new Request("https://x/api/blob?doc=d1&token=local", {
    method: "POST",
    headers: { "content-type": "image/x-yanshi-raw" },
    body: pngBytes,
  }),
);
const blobBody = await blobRes.json();
// **★ 断言形态改为"**成功 或 如实 501**" ✗ ★**（第 848 轮 ✓；**判据环境没有 /wasm/ ✓**）：
//   **∴ 为什么 ✗**：**判据在 node 里跑 ✗**（**没有 `/wasm/yanshi_wasm.js` ✓）**
//     ⇒ **∴ 于是**：**内核不可用 ⇒ **∴ 本地层**如实返回 501 ＋ `kernel_unavailable` ＋ 原因 ✓**
//       ⇒ **∴ 那**正是**目标总则"**lazy 不许变成撒谎 ✗**"**要求的形态 ✓**** ✓✓
//   **∴ 所以本判据断言两条互斥的**合法**结果 ✗**：
//     **a.** **内核可用 ⇒ 200 ＋ `ok:true` ＋ `blob_hash` 以 `sha256:` 开头 ＋ `size` 正确 ✗**；
//     **b.** **内核不可用 ⇒ 501 ＋ `error=kernel_unavailable` ＋ 非空 `reason` ✗**
//       （**∴ 二者必居其一 ✗；**都做不到**才报红 ✓）** ✓✓
//   **∴ 变异（**手工 ✓）**：**删掉整个 `/api/blob` 分支 ⇒ **∴ 它**会返回**501 但**没有 `kernel_unavailable`**✗
//     （**∴ 而是** `endpoint_not_local` ✓）⇒ **∴ 于是** a／b **都不成立** ⇒ **∴ 必红 ✓**** ✓✓
const blobOk = blobRes.status === 200 && blobBody.ok === true &&
  String(blobBody.blob_hash || "").indexOf("sha256:") === 0 && blobBody.size === pngBytes.length;
const blobHonest = blobRes.status === 501 && blobBody.error === "kernel_unavailable" &&
  String(blobBody.reason || "").length > 0 && blobBody.endpoint === "/api/blob";
check(blobOk || blobHonest,
  "`/api/blob` 应「成功」或「如实 501（kernel_unavailable ＋ 原因）」（实测 " + blobRes.status + " " +
  JSON.stringify(blobBody).slice(0, 140) + "）");
check(!(blobRes.status === 501 && blobBody.error === "endpoint_not_local"),
  "`/api/blob` 不能落到「未实现端点」分支 ⇒ 说明分支不存在（实测 " + JSON.stringify(blobBody).slice(0, 120) + "）");
console.log("    `/api/blob` ⇒ " + (blobOk ? "内核可用 ⇒ 200 ＋ sha256 hash ✓" : "内核不可用 ⇒ 如实 501 ＋ 原因 ✓"));
// **★ ⑧ 尾斜杠必须被接受 ✗ ★**（第 859 轮 ✓；**用户报"还是落笔失败" ✓**）：
//   **∴ 根因（**实测 ✓）✗**：**落笔**把像素 **POST 到 `/api/blob/` ✗**（**带尾斜杠 ✓）**
//     ⇒ **∴ 而**本地层**原来用 `url.pathname === "/api/blob"` 精确匹配 ✗**
//       ⇒ **∴ 于是**：**它**落到"未实现端点" ⇒ **∴ 501 ⇒ **∴ 落笔失败 ✓**** ✓✓
//   **∴ 修法 ✗**：**在 `local()` 里**归一化路径 ✗**（**去掉一个尾斜杠 ✓）
//     ⇒ **∴ 于是**：**11 个分支**都**同时接受**两种写法 ✓**** ✓✓
//   **∴ 判据 ✗**：**对**带尾斜杠**的 POST ✗** ⇒ **∴ 期望**：
//     **∴ 它**必须**被**同一个分支**处理 ✗** ⇒ **∴ 即**：**可以**成功（**200 ✓）
//       **或**如实报**内核不可用（**501 ＋ `kernel_unavailable` ✓）**
//       ⇒ **★ 但**绝不可以**是 `endpoint_not_local` ✗**（**∴ 那**说明**归一化失效 ✓）** ✓✓
//   **∴ 变异（**手工 ✓）**：**删掉那两行归一化 ⇒ **∴ `path` 不再定义／或**分支匹配不到**
//     ⇒ **∴ 本断言**必红 ✓**** ✓✓
const slashRes = await local(
  new Request("https://x/api/blob/?doc=d1&token=local", {
    method: "POST",
    headers: { "content-type": "image/x-yanshi-raw" },
    body: pngBytes,
  }),
);
const slashBody = await slashRes.json();
check(slashBody.error !== "endpoint_not_local",
  "带尾斜杠的 /api/blob/ 必须被同一分支处理，而不是落到未实现（实测 " +
  JSON.stringify(slashBody).slice(0, 120) + "）");
check(slashRes.status === 200 || slashBody.error === "kernel_unavailable",
  "带尾斜杠的 /api/blob/ 应成功或如实报内核不可用（实测 " + slashRes.status + "）");
console.log("    `/api/blob/`（尾斜杠）⇒ " +
  (slashRes.status === 200 ? "200 ＋ 已处理 ✓" : "501 ＋ " + slashBody.error + "（被同一分支处理 ✓）"));

console.log("    `/api/blob` 二进制上传 ✓：" + JSON.stringify(blobBody).slice(0, 110));

const un = await wrapped("https://x/api/effects?doc=d1", { method: "POST", body: "{}" });
check(un.status === 501, `未实现端点应返回 501 ✗（实测 ${un.status}）`);
const unBody = await un.json();
check(unBody.error === "not_implemented_locally", `未实现端点的 error 字段不对 ✗：${JSON.stringify(unBody)}`);
check(!!unBody.endpoint && unBody.endpoint.includes("effects"), "未实现端点应报出端点名 ✗");
check(!!unBody.reason, "未实现端点应报出原因 ✗");
// **★ 快照**过期**必须返回 null ✗ ★**（第 641 轮 ✓；**目标第 6 条 ✓**）：
//   **∴ 规则**：**快照带**序号 ＋ 格式版本**✗ ⇒ **∴ 二者任一不匹配 ⇒ **必须重算 ✗****
//   ⇒ **∴ 绝不许**拿旧图冒充**✗**（**∴ 这是能红判据的重点 ✓**）。
const { writeSnapshot, readSnapshot, FORMAT_VERSION } = await import(base + "store.js");
const snapBytes = new Uint8Array([1, 2, 3, 4]);
await writeSnapshot(db, "d1", 3, { bytes: snapBytes });
check(!!(await readSnapshot(db, "d1", 3)), "序号匹配时应能读到快照 ✗");
check((await readSnapshot(db, "d1", 4)) === null, "序号**不**匹配时**必须**返回 null ✗（**∴ 否则就是拿旧图冒充 ✗**）");
console.log(`  快照：序号匹配 ⇒ 命中 ✓｜序号不匹配 ⇒ null ✓（format=${FORMAT_VERSION}）`);
check(LOCAL_IMPLEMENTED >= 5, `声明实现数应 ≥ 5 ✗（实测 ${LOCAL_IMPLEMENTED}）`);

console.log(`  实际调用通过 ✓：创建 ✓｜原子 ${atoms.count} 条 ✓｜层 ${layers.count} 个 ✓｜`
  + `get_document ${gd.width}×${gd.height} pending ✓｜后端 ${gd.render_backend} ✓｜未实现端点 501 ✓`);
console.log(`  LOCAL_IMPLEMENTED = ${LOCAL_IMPLEMENTED} ✓`);
// **★ `draw_stroke` 必须在本地层有映射 ✗ ★**（用户报告的 P0 ✓）：
//   **∴ 为什么 ✗**：**前端落笔调的是 `draw_stroke`** ✓（**`viewer-app.js` 的两处落笔 ✓**），
//     而**本地层原来只认 `brush_stroke`** ✗ ⇒ **∴ 每一笔**返回
//     `not_implemented_locally` ✓ ⇒ **∴ 界面"落笔失败：unknown" ✗**、**画布 0 像素 ✓**
//     （**用户四轮真机复验一致 ✓）** ✓✓
//   **∴ 判据 ✗**：**POST 一笔到 `/api/tools/draw_stroke` ✗** ⇒ **∴ 不许**是
//     `endpoint_not_local` ✓（**∴ 且**要么 200 ✗，要么**如实** `kernel_unavailable` ✓）** ✓✓
//   **∴ 变异（**手工 ✓）**：**把 `draw_stroke` 从分支条件里删掉 ⇒ **∴ 本条**必红 ✓**
//     （**∴ 它**会退回 `endpoint_not_local` ✓）** ✓✓
const strokeRes = await call("/api/tools/draw_stroke", {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({
    layer_id: "L0",
    data: { points: [[2, 2], [12, 12]], size: 8, color: [0, 0, 0, 1] },
  }),
});
const strokeBody = await strokeRes.json().catch(() => ({}));
const strokeErr = String((strokeBody && strokeBody.error) || "");
check(strokeErr !== "endpoint_not_local" && strokeErr !== "not_implemented_locally",
  "draw_stroke 必须在本地层有映射（否则前端每一笔都失败：用户报告的 P0；实测 error=" + strokeErr + "）");
// **∴ 第二条只守"**失败必须说出原因 ✗**"**（**∴ 不**要求 200 ✗）：
//   **∴ 因为**在 node 判据环境里**没有 wasm 内核** ✓ ⇒ **∴ 这一笔**只能失败 ✓
//   ⇒ **∴ 若**要求 200 ✗** ⇒ **∴ 正常态**也红 ✗** ⇒ **∴ 变异**就**失去区分力 ✓**** ✓✓
check(strokeRes.status === 200 || String((strokeBody && strokeBody.reason) || "").length > 0,
  "draw_stroke 失败时必须说出原因（不许静默；实测 " + strokeRes.status + "／" +
    JSON.stringify(strokeBody).slice(0, 120) + "）");
console.log("  本地层 draw_stroke ⇒ " + strokeRes.status + "｜error=" + (strokeErr || "(无)"));

if (bad.length) { console.error("❌ " + bad.join("｜")); process.exit(1); }
console.log("  ✓ PWA 本地 API 在行为上成立（不是只看字符串 ✓）");
