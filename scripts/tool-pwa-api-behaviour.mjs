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
// **前提失败必须 exit(3)**（第 1084 轮）：无服务时 fetch 抛异常 ⇒ uncaught ⇒ 退出码 1 ✗
// ⇒ 按 336 轮那条：条件不满足应 exit(3)（不可判），不是 exit(1)（产品失败）
process.on("uncaughtException", (error) => {
  console.error(`  ⊘ 判据自身出错（不是产品问题）：${String(error).slice(0, 200)}`);
  process.exit(3);
});
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
        // **★ 桩必须模拟真 IndexedDB 的两个行为**（2026-10-20，本判据在 CI 上 exit 13 / 读回 0）：
        //   ① `transaction([a, b])` **接受数组**（真 API 就是这样；D-01 的单事务要用它）
        //      ⇒ 原来的桩只 `stores.get(n)` ⇒ 传数组时拿到 undefined ⇒ **另建一个以数组为 key 的 store**
        //        ⇒ 原子的 write 全进了那个假 store ⇒ 读回 0 笔触 ✗
        //   ② 事务是**异步提交**的，会触发 `oncomplete`
        //      ⇒ 原来的桩没有它 ⇒ 产品里 `await done` **永远不 resolve** ⇒ 判据挂死（Node exit 13）✗
        //   ⇒ 桩不完备会让「产品对、判据红」⇒ 所以我修桩，**不**迁就桩去改产品 ✓
        transaction: (n) => {
          const names = Array.isArray(n) ? n : [n];
          const storeFor = (name) => {
            if (!names.includes(name)) {
              throw new Error(`事务没有包含 store「${name}」（现有：${names.join(",")}）`);
            }
            if (!stores.has(name)) stores.set(name, new MemStore());
            return stores.get(name);
          };
          const t = { objectStore: (name) => storeFor(name) };
          // 真 API 在微任务量级内提交 ⇒ 用 setTimeout 0（`oncomplete` 属性那时已赋好）
          setTimeout(() => {
            if (typeof t.oncomplete === "function") t.oncomplete();
          }, 0);
          return t;
        },
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
const { open, putBlob } = await import(base + "store.js");

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
// **★ 第 1440 轮审计补：`writeSnapshot` 必须单事务 + 校验序号 ✗ ★**（**回归 D-01 同族 ✓**）：
//   **∴ 旧写法 ✗**：`writeSnapshot` 用**两个事务**（readonly 读 → readwrite 写）✗ ⇒
//     **∴ 中间若有 `putAtom` 提交** ⇒ 这次 `put` 把 `meta.seq` **拖回**陈旧 `seq` ✗ ⇒
//     **∴ 下一次 `putAtom` 读到同一 seq ⇒ 主键 `${doc}:${seq}` 相撞 ⇒ 已提交原子被静默覆盖 ✗**。
//   **∴ 修法 ✓**：单事务包住读改写 ✓ ＋ **`seq ≠ 当前 head` ⇒ 拒写并返回 false** ✓ ⇒
//     **∴ 调用方据此重算 ✗，绝不让旧图冒充新状态 ✓**（目标第 6 条 ✓）。
//   **∴ 判据要制造"读回 head ＋ 陈旧 seq 两个 case"** ⇒ 断 `true`/`false` 与"陈旧写不得覆盖 ✓"。
const { writeSnapshot, readSnapshot, FORMAT_VERSION, tx: stx, wrap: swrap } = await import(base + "store.js");
const snapBytes = new Uint8Array([1, 2, 3, 4]);
const headD1 = (await swrap(stx(db, "docs", "readonly").get("d1")))?.seq ?? 0;
// ① 序号＝head ⇒ 写成功 ＋ 读回命中 ✓。
const wOk = await writeSnapshot(db, "d1", headD1, { bytes: snapBytes });
check(wOk === true, `writeSnapshot 序号匹配 head(${headD1}) 应返回 true ✗（实测 ${wOk}）`);
check(!!(await readSnapshot(db, "d1", headD1)), "序号匹配时应能读到快照 ✗");
check((await readSnapshot(db, "d1", headD1 + 1)) === null,
  "序号**不**匹配时**必须**返回 null ✗（**∴ 否则就是拿旧图冒充 ✗**）");
// ② 制造陈旧：先写 head ⇒ 再追加一条原子把 head 顶上去 ⇒ 用**旧 seq** 覆写 ⇒ **必须拒 ✓**。
const staleSeq = headD1;
const c2 = await call("/api/atoms", { method: "POST",
  body: JSON.stringify({ kind: "create_layer", layer_id: "L9", name: "顶高序号" }) });
check(c2.ok === true, "追加原子应成功（制造 head 前进 ✗）");
const headAdvanced = (await swrap(stx(db, "docs", "readonly").get("d1")))?.seq ?? 0;
check(headAdvanced === staleSeq + 1, `head 应前进到 ${staleSeq + 1} ✗（实测 ${headAdvanced}）`);
const wStale = await writeSnapshot(db, "d1", staleSeq, { bytes: new Uint8Array([9, 9, 9, 9]) });
check(wStale === false, `陈旧序号(${staleSeq}) 覆写必须被拒、返回 false ✗（实测 ${wStale}）`);
// ③ 陈旧写**不得**污染 meta.seq（这是 lost-update 的实质 ✗）：head 必须仍是新值。
const metaAfter = await swrap(stx(db, "docs", "readonly").get("d1"));
check((metaAfter?.seq ?? 0) === headAdvanced,
  `陈旧 writeSnapshot 不得把 meta.seq 拖回 ${staleSeq} ✗（实测 ${metaAfter?.seq}）`);
console.log(`  快照：序号匹配 ⇒ 写✓读✓｜序号不匹配 ⇒ null ✓｜陈旧写 ⇒ 被拒且不动 head ✓（format=${FORMAT_VERSION}）`);

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

// **★ ⑦b `import_asset{kind:texture}` 必须采纳**上传的字节 ✗ 而不是**去随包里找同名文件 ★**
//   （**第 745 轮 ✓**；**前端 `viewer-app.js:importAssetFile` 永远发 `data_base64` ✓，
//     服务端 `write_import_asset` 也存这份字节 ✓** ⇒ **∴ 本地层此前**无视 `data_base64`✗
//     改从 `/textures/<name>` 取 ⇒ **用户上传被静默丢弃 ✓ 是缺陷 ✗**）**
//   **∴ 为什么 node 判据**能区分新旧 ✗ 而**不需要 wasm 内核 ✓**：
//     **∴ 新旧**唯一分岔**在**内核之前**✗（**旧代码**先 fetch 随包 ⇒ 名字不存在 ⇒ `asset_not_found`✗
//       **新代码**先解码上传 ⇒ 走到内核 ⇒ `kernel_unavailable`✓）**
//     ⇒ **∴ 所以**断言「**带 `data_base64` 的上传**绝不许**报 `asset_not_found`**」✓**
//       （**∴ 名字**故意取一个**随包里不存在**的 ⇒ 旧代码必红 ✓ 新代码不受名字影响 ✓）**
//   **∴ 变异（**已实测 ✓）**：**把 api-local 的分支退回旧的"只 fetch 随包"** ⇒ 本条红 ✓**
//   **∴ 诚实 ✗**：**成功路径（`ok:true`＋`source:"upload"`＋`size`）要真 wasm 内核** ⇒ **归 browser 判据 ✓**
//     ⇒ **本条**只守**"别再撒谎说没这个文件"** ✓**（**不冒充"完整验了上传"✓）** ✓✓
const uploadB64 = (typeof Buffer !== "undefined")
  ? Buffer.from([0x89, 0x50, 0x4e, 0x47, 1, 2, 3, 4]).toString("base64")
  : "qgBOQQECA4"; // 一段合法 PNG 头的 base64
const iaRes = await call("/api/tools/import_asset", {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ kind: "texture", name: "__no_such_texture_local__", data_base64: uploadB64 }),
});
const iaBody = await iaRes.json().catch(() => ({}));
const iaErr = String((iaBody && iaBody.error) || "");
check(iaErr !== "asset_not_found",
  "import_asset{texture} 带着 data_base64 时不许报 asset_not_found ⇒ 那说明它忽略了上传字节、去随包里找同名文件（用户上传被丢弃；实测 " +
  iaRes.status + "／" + JSON.stringify(iaBody).slice(0, 140) + "）");
// **∴ 且**它**必须是被映射的端点**✓**（**不许** endpoint_not_local／not_implemented_locally 对 texture ✓）**
check(iaErr !== "endpoint_not_local" && iaErr !== "not_implemented_locally",
  "import_asset{texture} 必须在本地层有实现分支（实测 error=" + iaErr + "）");
// **∴ 非 texture** 仍要**如实 501**✓**（**本地 brush/palette 是随包静态资源 ✓**）
const iaBrush = await (await call("/api/tools/import_asset", {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ kind: "brush", name: "x.myb", data_base64: uploadB64 }),
})).json();
check(iaBrush.error === "not_implemented_locally",
  "import_asset{brush} 必须如实 501（本地该类是随包静态资源；实测 " + JSON.stringify(iaBrush).slice(0, 120) + "）");
console.log("  本地层 import_asset{texture,data_base64} ⇒ " + iaRes.status + "｜error=" + (iaErr || "(无)") +
  "｜{brush} error=" + String(iaBrush.error || ""));

// **★ ⑦c `import_image` 要读**规范形状**并写出**渲染器认得的净荷 ✗ ★**（本轮 ✓）：
//   **∴ 前端 `viewer-app.js` 与服务端 `write_import_image` 都用** `bitmap:{blob_hash}` ＋ `region:{x,y,w,h}`✓
//     ⇒ **∴ 而**本地层此前读 `body.blob_hash`／`body.x/y`✗ ⇒ 规范请求**一律 `missing_blob_hash`**✓
//     （**离线导入图片整条路坏 ✓**）＋ 净荷缺 `bitmap`/`region`/`宽高` ⇒ `parse_raster_patch` 当不支持原语跳过✓
//   **∴ 判据 ✗（**这条不需要内核 ✓**——只要 blob 在 IndexedDB 里就能走通 ✓）**：
//     ① 先 `putBlob` 造一个 blob ⇒ 用**规范形状**调 `import_image` ⇒ 期望 **`ok:true`**（旧代码此输入必 `missing_blob_hash`✗）
//     ② **回显的落点/尺寸**取自 `region`✓**（**x=5/y=7/w=4/h=4✗ 不许退回 0 ✓**）
//     ③ 规范形状但 **w/h 缺** ⇒ `missing_bitmap_size`✓（**不写一条渲染不出的原子 ✓**）
//     ④ 规范形状但 **blob 不在库** ⇒ `blob_not_found`✓（**且**不是** `missing_blob_hash`✗ 那说明没读到 bitmap.blob_hash ✓**）
//   **∴ 变异（**实测 ✓）**：**把读侧退回 `body.blob_hash || pl.blob_hash`** ⇒ ① 必红（missing_blob_hash ✓）**
const IMG_HASH = "sha256:" + "ab".repeat(32);
await putBlob(db, IMG_HASH, Array.from([1, 2, 3, 4]));   // 4 字节占位（getBlob 只按字符串键取 ✓）
const iiRes = await call("/api/tools/import_image", {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ layer_id: "L0", bitmap: { blob_hash: IMG_HASH, mime_type: "image/x-yanshi-raw", size: 4 },
                         region: { x: 5, y: 7, w: 4, h: 4 } }),
});
const iiBody = await iiRes.json().catch(() => ({}));
check(iiBody.ok === true,
  "import_image 用规范形状（bitmap.blob_hash＋region）必须成功，不能再报 missing_blob_hash（那说明读错了键、离线导入图片整条路坏；实测 " +
  iiRes.status + "／" + JSON.stringify(iiBody).slice(0, 140) + "）");
check(iiBody.width === 4 && iiBody.height === 4 && iiBody.x === 5 && iiBody.y === 7,
  "import_image 的落点/尺寸必须取自 region（不许退回 0/缺省；实测 " +
  JSON.stringify({ x: iiBody.x, y: iiBody.y, w: iiBody.width, h: iiBody.height }) + "）");
// ③ 缺尺寸 ⇒ 如实报，而不是写一条渲染不出的原子
const iiNoSize = await (await call("/api/tools/import_image", {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ layer_id: "L0", bitmap: { blob_hash: IMG_HASH }, region: { x: 0, y: 0, w: 0, h: 0 } }),
})).json();
check(iiNoSize.error === "missing_bitmap_size",
  "import_image 缺位图尺寸必须报 missing_bitmap_size（否则净荷无 region/宽高 ⇒ 渲染器 parse_raster_patch 跳过 ⇒ 画面什么都不出；实测 " +
  JSON.stringify(iiNoSize).slice(0, 120) + "）");
// ④ 规范形状但库里没这个 blob ⇒ blob_not_found（不是 missing_blob_hash）
const iiNoBlob = await (await call("/api/tools/import_image", {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ layer_id: "L0", bitmap: { blob_hash: "sha256:" + "cd".repeat(32) }, region: { x: 0, y: 0, w: 2, h: 2 } }),
})).json();
check(iiNoBlob.error === "blob_not_found",
  "import_image 规范形状＋库中无此 blob ⇒ 应报 blob_not_found（若报 missing_blob_hash 说明没读到 bitmap.blob_hash；实测 " +
  JSON.stringify(iiNoBlob).slice(0, 120) + "）");
console.log("  本地层 import_image{规范形状} ⇒ " + iiRes.status + "｜ok=" + iiBody.ok +
  "｜落点/尺寸=(" + iiBody.x + "," + iiBody.y + "," + iiBody.width + "," + iiBody.height + ")");



// **★ ⑧ 评论的游标不能越过**没交付**的那几条 ✗ ★**（与服务端 `read_list_comments` 同形 ✓）
//   **∴ 为什么 ✗**：本地层原来把 `next_since` 设成**匹配全集**的最大 seq，
//     而交付的却是**最新一页** ✗ ⇒ 客户端照着游标轮询 ⇒
//     **∴ 比这一页更早的评论永远收不到**（服务端同病，第 1441 轮已修 ✓）。
//   **∴ 判据 ✗**：5 条评论、`limit:2` ⇒ 照游标轮询**必须收齐 5 条** ✓。
//   **∴ 变异 ✗**：把游标改回「截断之前取最大」⇒ 本条必红 ✓。
const postComment = (text) => call("/api/tools/comment", {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ text }),
});
for (let index = 0; index < 5; index++) await postComment(`巡检 ⑧ ${index}`);
let cursor = 0;
const received = [];
for (let round = 0; round < 10; round++) {
  const page = await (await call("/api/tools/list_comments", {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ since_seq: cursor, limit: 2 }),
  })).json();
  const items = page.comments || [];
  if (!items.length) break;
  for (const item of items) received.push(item.text);
  cursor = page.next_since;
}
check(new Set(received).size >= 5,
  `照 next_since 轮询必须收齐 5 条评论（实测只收到 ${new Set(received).size} 条 ⇒ 游标越过了未交付的评论 ✗）`);
// **⑧′ 满页信号 ＋ 恒等式**（结构量，不断条数 ✓）：`truncated` 必须等于「匹配总数 > 本页交付数」。
const page8 = await (await call("/api/tools/list_comments", {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ limit: 2 }),
})).json();
check(page8.truncated === (page8.count > (page8.comments || []).length),
  `truncated 必须等于 count > 本页条数 ✗（实测 ${page8.truncated}／count=${page8.count}）`);
check(page8.returned === (page8.comments || []).length,
  `returned 必须等于本页条数 ✗（实测 ${page8.returned}）`);

// **★ ⑨ `find_atom` 必须回显**它真正用的 limit**✗ ★**（服务端 `read_find_atom` 同形 ✓）
//   **∴ 为什么 ✗**：`.min(2000)` 静默生效 ⇒ 调用方请求 5000、拿到 2000，
//     而回执里**既没有 `limit`、也没有游标** ⇒ 第 2001 条之后**永远取不到** ✗。
const faBig = await (await call("/api/tools/find_atom", {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ limit: 5000 }),
})).json();
check(faBig.limit === 2000 && faBig.limit_requested === 5000 && faBig.limit_clamped === true,
  `被夹过要留痕 ✗（实测 limit=${faBig.limit}／limit_requested=${faBig.limit_requested}／limit_clamped=${faBig.limit_clamped}）`);
const faSmall = await (await call("/api/tools/find_atom", {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ limit: 2 }),
})).json();
check(faSmall.truncated === true && typeof faSmall.next_since === "number",
  `截断时必须给游标，否则剩下的一条取不到 ✗（实测 ${JSON.stringify(faSmall).slice(0, 120)}）`);

// **★ ⑩ 计数族参数读不出 ⇒ 必须响亮 ✗ ★**（服务端 `optional_count` 同形 ✓）
//   **∴ 为什么 ✗**：`Number(body.since_seq)` 对 `-1`／`"abc"` 都能算出个数 ⇒
//     旧版把 `-1` 静默当 0（= 返回全量）✗（静默降级形态 4 ✓）。
const negRes = await call("/api/tools/find_atom", {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ since_seq: -1 }),
});
const negBody = await negRes.json().catch(() => ({}));
check(negBody.ok === false && String(negBody.reason || "").includes("since_seq"),
  `since_seq:-1 必须被拒绝并点名参数（不许静默当 0 ⇒ 返回全量 ✗；实测 ${negRes.status}／${JSON.stringify(negBody).slice(0, 120)}）`);

if (bad.length) { console.error("❌ " + bad.join("｜")); process.exit(1); }
console.log("  ✓ PWA 本地 API 在行为上成立（不是只看字符串 ✓）");
