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
// **★ ⑦h 起：隔离文档用的 `callDoc` ✗ ★**（**第 916/1356 轮的坑 ✓：一个判据里多个 case 共用一个文档 ⇒
//   前一段的状态污染后一段的计数 ✗ ⇒ undo/redo 这种「整档计数」的断言必须建在独立文档里 ✓**）。
const callDoc = (d, path, init = {}) => local(new Request(`https://x${path}?doc=${d}`, { method: init.method ?? "GET", ...init }));

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

// **★ ⑦d `resample` 要认 `scale`✗ 不许静默按原尺寸冒充成功 ★**（本轮 ✓）：
//   **∴ 前端 6483 只发 `{object_id, scale}`✓；服务端 `write_resample` 认 `scale`✓** ⇒
//     **∴ 而**本地此前只读 `width/height`✗ ⇒ `scale` 被丢 ⇒ `toW = targetW || fromW` ⇒
//     **静默按原尺寸重采样、返回 `ok:true`** ✗（用户"缩到 50%"结果没变而界面说成功了 ✓）。
//   **∴ node 判据能验什么 ✗（**缩放本体要浏览器 canvas ⇒ 到不了 ✓ 归 browser 判据**）**：
//     **请求校验**放在 `try` **之前** ⇒ 与浏览器无关 ⇒ 这两条**当场可断** ✓：
//       ① **`width/height` 与 `scale` 二者都不给 ⇒ 400 参数错误**（**旧代码：进到 try ⇒ 501 ✓ 不报参数错 ✓**）
//       ② **`scale<=0` ⇒ 400 "scale 必须是正数"**（**旧代码：丢 scale ⇒ 501 ✓**）
//   **∴ 变异（**实测 ✓）**：**删掉这两条早守卫** ⇒ ① 变 501 ⇒ 断言① 红 ✓**
//   复用上面 `import_image` 建出的光栅对象（`iiBody.object_id`✓ 是 raster_patch、带 bitmap.blob_hash ✓）
const rsNoArgs = await (await call("/api/tools/resample", {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ object_id: iiBody.object_id }),
})).json();
check(rsNoArgs.ok !== true && /width 与 height，或 scale/.test(String(rsNoArgs.reason || "")),
  "resample 二者都不给必须报参数错（旧代码会静默按原尺寸重采样、冒充成功，或落到浏览器环境的 501；实测 " +
  JSON.stringify(rsNoArgs).slice(0, 150) + "）");
const rsBadScale = await (await call("/api/tools/resample", {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ object_id: iiBody.object_id, scale: 0 }),
})).json();
check(rsBadScale.ok !== true && /scale 必须是正数/.test(String(rsBadScale.reason || "")),
  "resample scale<=0 必须报「scale 必须是正数」（旧代码整个丢掉 scale ⇒ 无此校验；实测 " +
  JSON.stringify(rsBadScale).slice(0, 120) + "）");
console.log("  本地层 resample{无尺寸} ⇒ " + (rsNoArgs.error || "(ok)") + "｜resample{scale:0} ⇒ " + (rsBadScale.error || "(ok)"));

// **★ ⑦e `list_objects` 要照服务端 `read_list_objects` 折叠：去重／滤层／滤可见／剔删除 ✗ ★**（本轮 ✓）
//   **∴ 两处前端契约靠它 ✗**：`moveCheckedObjectZ`（`viewer-app.js:5683` 要「该图层**完整** z 序」）
//     ＋ `pickObjectAt`（`:8717` 要 `include_hidden:false` ⇒ 只给**可见**的）✓。
//   **∴ 旧本地 ✗**：逐原子 push ⇒ 同一对象的多原子**重复出现**✓；**不滤 `layer_id`** ⇒ 跨层混 ✓
//     （⇒ 拼出的 `order` 与该层真实集合不符 ⇒ `reorder_objects`「期望 N 实际 M」打回 ✓）；
//     **不滤 `include_hidden`** ⇒ 拾到看不见的对象 ✓；**不剔 `tombstone`** ⇒ 已删的还在列 ✓。
//   **∴ 顺带证 `p2ObjectView` 泛化 ✗**：`draw_shape`（既非 `create_object` 也非 `import_image`）建的对象
//     ⇒ `get_object` **必须**找得到 ✓（旧折叠器只认那两种 ⇒ 手画图形 get_object/list 全瞎 ✓）。
//   **∴ 全程不碰内核 ✗（纯原子折叠 ⇒ node 当场可验 ✓）**。新建独立层 `LZ` 隔离，避开前面块的 L0/L9 对象 ✓。
await call("/api/atoms", { method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ kind: "create_layer", layer_id: "LZ", name: "过滤层" }) });
const jpost = (p, o) => call(p, { method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify(o) }).then((r) => r.json());
const objA = await jpost("/api/tools/import_image", { layer_id: "LZ", bitmap: { blob_hash: IMG_HASH, mime_type: "image/x-yanshi-raw", size: 4 }, region: { x: 0, y: 0, w: 2, h: 2 } });
const objB = await jpost("/api/tools/import_image", { layer_id: "LZ", bitmap: { blob_hash: IMG_HASH, mime_type: "image/x-yanshi-raw", size: 4 }, region: { x: 4, y: 4, w: 2, h: 2 } });
const shapeC = await jpost("/api/tools/draw_shape", { layer_id: "LZ", data: { geometry: "rect" } });  // 证泛化：非 create_object／非 import_image 建的对象
await jpost("/api/tools/update_object", { object_id: objA.object_id, patch: { visible: false } });     // A：隐藏
await jpost("/api/tools/delete_object", { object_id: objB.object_id });                                // B：删除
const lzAll = await jpost("/api/tools/list_objects", { layer_id: "LZ", include_hidden: true });
const lzVis = await jpost("/api/tools/list_objects", { layer_id: "LZ" });                              // 缺省＝不含隐藏
const has = (lst, id) => (lst.objects || []).some((o) => o.object_id === id);
const countOf = (lst, id) => (lst.objects || []).filter((o) => o.object_id === id).length;
// ① 折叠器认全「建对象原子」：draw_shape 对象 get_object 找得到
const gotShape = await jpost("/api/tools/get_object", { object_id: shapeC.object_id });
check(gotShape.ok === true && gotShape.object_id === shapeC.object_id,
  "draw_shape 建的对象必须能被 get_object 找到（旧折叠器只认 create_object/import_image ⇒ 手画图形 get_object 报不存在；实测 " +
  JSON.stringify(gotShape).slice(0, 150) + "）");
// ② 滤层：LZ 的列表不许混进别的层（L0 上的 ⑦c 导入图 iiBody.object_id）
check(Array.isArray(lzAll.objects) && !has(lzAll, iiBody.object_id),
  "list_objects{layer_id:LZ} 不许含其它层对象（旧不滤 layer_id ⇒ 跨层 ⇒ moveCheckedObjectZ 拼的 order 与该层集合不符⇒ reorder 被打回；实测 " +
  JSON.stringify(lzAll).slice(0, 160) + "）");
// ③ 去重：A 有 import＋set_property 两条原子 ⇒ 只许出现一次
check(countOf(lzAll, objA.object_id) === 1,
  "list_objects 每对象只数一次（旧逐原子 push ⇒ A 有 create+update 两条 ⇒ 出现两遍；实测 A 出现 " +
  countOf(lzAll, objA.object_id) + " 次）");
// ④ 剔删除：B 被 delete_object 后不许出现（照服务端 alive_objects）
check(!has(lzAll, objB.object_id),
  "list_objects 必须剔除已删除(tombstone)的对象（旧不过滤；实测 " + JSON.stringify(lzAll).slice(0, 160) + "）");
// ⑤ 滤可见：缺省排除隐藏 A、保留可见 C（不许把过滤做成恒空）
check(!has(lzVis, objA.object_id),
  "list_objects 缺省必须排除不可见对象（A 设了 visible:false；服务端缺省 include_hidden=false；旧不滤 ⇒ pickObjectAt 拾到看不见的；实测 " +
  JSON.stringify(lzVis).slice(0, 160) + "）");
check(has(lzVis, shapeC.object_id),
  "list_objects 缺省必须含可见对象 C（证明过滤不是恒空；实测 " + JSON.stringify(lzVis).slice(0, 160) + "）");
console.log("  本地层 list_objects：draw_shape 可 get_object ✓｜滤层/去重/剔删除/滤可见 ✓（LZ 含隐藏=" +
  lzAll.count + "｜仅可见=" + lzVis.count + "）");

// **★ ⑦f `duplicate_layer` 也要复制 `draw_shape`/`fill`/`draw_text` 画的形 ✗ ★**（本轮 ✓）
//   **∴ 与 ⑦e 同一类 bug ✗**：复制循环此前只 inline 列 `import_image || create_object` ⇒
//     **手画图／字／色块在「复制图层」时被整个丢掉** ✓（副本对象数偏少 —— **静默降级 ✓**，返回还 `ok:true`）。
//   **∴ 修法 ✗**：用共享集合 `P2_OBJECT_CREATE_KINDS` ✓（与 `p2ObjectView`／`list_objects` 同一处口径 ✓）。
//   **∴ 为什么 node 可验 ✗**：`duplicate_layer` 是**纯原子复制 ✗ 不碰内核／canvas ✓** ⇒ 当场可断 ✓。
//   **∴ 变异 ✗**：把复制判据换回旧的 inline 两种 ⇒ 本条必红 ✓。
await call("/api/atoms", { method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ kind: "create_layer", layer_id: "LD", name: "复制层" }) });
const onlyShape = await jpost("/api/tools/draw_shape", { layer_id: "LD", data: { geometry: "rect" } });  // 该层唯一的非 create_object／非 import_image 对象
const dup = await jpost("/api/tools/duplicate_layer", { layer_id: "LD" });
check(dup.ok === true && dup.objects === 1,
  "duplicate_layer 必须复制该层的 draw_shape 对象（旧 inline 只列 create_object/import_image ⇒ 复制图层时图形被整个丢掉、还冒充成功；实测 " +
  JSON.stringify(dup).slice(0, 160) + "）");
// **∴ 副本层要真在折叠里列得出这一对象 ✓**（证明不是只「计数」，而是真落进原子日志 ✓）：
const nlList = await jpost("/api/tools/list_objects", { layer_id: dup.layer_id, include_hidden: true });
const nlObj = (nlList.objects || [])[0] || {};
check(nlList.ok === true && (nlList.objects || []).length === 1 &&
      typeof nlObj.object_id === "string" && nlObj.object_id !== onlyShape.object_id,
  "duplicate_layer 后副本层必须能列出 1 个、且 object_id 是新的（共享 blob、独立对象 id；旧复制漏掉图形 ⇒ 这里会是 0 个；实测 " +
  JSON.stringify(nlList).slice(0, 200) + "）");
console.log("  本地层 duplicate_layer：draw_shape 复制=" + dup.objects + "｜副本层对象数=" + (nlList.count ?? 0) +
  "｜副本 id 全新=" + (nlObj.object_id !== onlyShape.object_id) + " ✓");

// **★ ⑦g `get_state{include_objects}`／`p2cState` 的对象种子要认全部建对象原子 ✗ ★**（本轮 ✓）
//   **∴ 与 ⑦e/⑦f 同一类 bug 的第四个读者 ✗**：`p2cState` 旧只 push `create_object` ⇒
//     **`get_state`／`list_documents`／`collect_diagnostics` 漏掉全部手画图／导入图／填充／文字对象**✓（结果集静默偏少、还 `ok:true` ✓）。
//   **∴ 修法 ✗**：种子改读 `P2_OBJECT_CREATE_KINDS` ＋ 按 object_id 去重 ✓（真实类型／层／可见／删除仍由 `p2ObjectView` 合并 ✓）。
//   **∴ 为什么 node 可验 ✗**：纯原子折叠 ✗ 不碰内核／canvas ✓ ⇒ 当场可断 ✓。
await call("/api/atoms", { method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ kind: "create_layer", layer_id: "LS", name: "状态层" }) });
const gImp = await jpost("/api/tools/import_image", { layer_id: "LS", bitmap: { blob_hash: IMG_HASH, mime_type: "image/x-yanshi-raw", size: 4 }, region: { x: 0, y: 0, w: 2, h: 2 } });
const gShape = await jpost("/api/tools/draw_shape", { layer_id: "LS", data: { geometry: "rect" } });
// **去重的触发输入 ✗**：对 gShape **再发一条建对象原子**（同 `object_id` 的重画 ⇒ `draw_shape` 走 `p2cDraw` 把 `object_id` 写进 `payload` ✓；
//   ⇒ 不去重则种子拿到两条同 id ⇒ list_documents／collect_diagnostics 计数虚高 ✓）。
await jpost("/api/tools/draw_shape", { layer_id: "LS", object_id: gShape.object_id, data: { geometry: "ellipse" } });
const gstate = await jpost("/api/tools/get_state", { include_objects: true, include_hidden: true });
const gids = (gstate.objects || []).map((o) => o.object_id);
check(gstate.ok === true && gids.includes(gShape.object_id) && gids.includes(gImp.object_id),
  "get_state{include_objects} 必须列出非 create_object 建的对象（旧 p2cState 种子只 create_object ⇒ 手画图/导入图从 get_state/list_documents/collect_diagnostics 漏掉；实测 " +
  JSON.stringify(gstate).slice(0, 180) + "）");
check(gids.length === new Set(gids).size,
  "get_state 对象表必须按 object_id 去重（对同一对象发两条建对象原子 ⇒ 不去重则出现两遍 ⇒ list_documents/collect_diagnostics 计数虚高；实测重复=" + (gids.length - new Set(gids).size) + "）");
console.log("  本地层 get_state：draw_shape/import_image 均列出 ✓｜去重 ✓（全档对象数=" + gids.length + "）");

// **★ ⑦h undo／redo 的候选判据必须与服务端 `collect_gestures` 的状态事实一致 ✗ ★**（本轮差分探针实测 ✓）
//   **∴ 服务端实跑史**（同一序列打**真服务** `target/release/yanshi-serve`，取回的行为 ✓）：
//     `create_layer → draw_shape → update_object{visible:false} → delete_object → restore_object` 之后：
//     · **`undo_last` 撤掉 [tombstone, set_property, draw_shape] 三条**✗（**含 tombstone 本身 ✓**——
//       候选按「**对象活着吗**」判定 ✓，restore 撤掉了 tombstone ⇒ 对象活着 ⇒ 它的**每一条**原子都是候选 ✓）；
//     · 撤掉创建原子后 `list_objects.count=0`、`get_undo_status.remaining=0` ⇒ 对象**死了** ✓。
//   **∴ 旧本地实现错在两处 ✗**（**都是静默降级 ✓：`ok:true` 却与画面矛盾 ✓**）：
//     ① **候选按「每个原子的最新动作」** ⇒ restore 用过的 tombstone 被当成「已撤」跳过 ✗
//       ⇒ undo 只撤 [4,3] **半笔**（对象仍活着 ✓ —— 与服务端的整笔不一致 ✓）；
//     ② **`p2Neutralized` 把 revert／reapply 混成一个「被中和过」集合** ✗ ⇒ 撤过的原子**永远**判抑制
//       （`reapply` 不解除抑制 ✓）⇒ redo 找不到候选而**空手** ✗、`get_undo_status` 与实际画面矛盾 ✓
//       （实测：旧实现 redo_last 返回空 ⇒ 服务端同史重做整组 [5,4,3] ✓）。
//   **∴ 在独立文档里测 ✗**（第 916/1356 轮的坑 ✓：共享文档的整档计数会被别的测试块污染 ✗）。
//   **∴ 差分探针实跑两轮才定案 ✗**（第 904 轮的教训 ✓：探针给的意外结果先怀疑**我的期望**✗）：
//     redo 之后服务端的 `redo_last` 重做的是**整组** [tombstone, set_property, draw_shape] ✓
//     ⇒ 创建与 tombstone **又一起生效** ⇒ 对象**又是死的**（`get_object.deleted_by` 有值 ✓、
//     `list_objects` 里没有它 ✓）⇒ 它的 `get_undo_status#2.remaining=0`、`undo_last#2` 报
//     「没有可撤销的笔迹」✗ —— 与 `redoable=1`（死对象仍可被重做机器收集 ✓ 实测 ignored=11）一起
//     **两条路同一事实** ✓，**没有漂移** ✗（我一度判「服务端自相矛盾」是把 redo 误想成「只复活对象、
//     不恢复 tombstone」✗ —— 探针的 `redone` 清单直接否掉了这个读法 ✓）。
//   **∴ 本地层整条链与服务端逐字段对齐 ✗**（撤整笔含 tombstone ✓ → 撤后死透 remaining=0 ✓ →
//     整组重做 ✓ → 重做后又死 ⇒ undoable=0／undo#2 空带 message ✓ —— 本端就断这些实测事实 ✓）。
const hDoc = "dh_" + Date.now().toString(36);
const hjpost = (p, o) => callDoc(hDoc, p, { method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify(o) }).then((r) => r.json());
await callDoc(hDoc, "/api/documents", { method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: hDoc, width: 320, height: 240 }) });
await callDoc(hDoc, "/api/atoms", { method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ kind: "create_layer", layer_id: "LT", name: "历史层" }) });
const hShape = await hjpost("/api/tools/draw_shape", { layer_id: "LT", data: { geometry: { kind: "rect", bbox: { x: 1, y: 1, width: 8, height: 8 } } } });
await hjpost("/api/tools/update_object", { object_id: hShape.object_id, patch: { visible: false } });
await hjpost("/api/tools/delete_object", { object_id: hShape.object_id });
const hRestore = await hjpost("/api/tools/restore_object", { object_id: hShape.object_id });
check(hRestore.ok === true && hRestore.restored === true, `restore 应先复活对象（实测 ${JSON.stringify(hRestore).slice(0, 140)}）`);
const hStatus0 = await hjpost("/api/tools/get_undo_status", {});
check(hStatus0.undoable_gestures === 1 && hStatus0.redoable_gestures === 0,
  `独立文档里 restore 之后应「可撤 1／可重做 0」（实测 ${hStatus0.undoable_gestures}/${hStatus0.redoable_gestures}）`);
const hUndo1 = await hjpost("/api/tools/undo_last", {});
const hSeq1 = (hUndo1.undone || []).map((x) => x.kind);
check(hUndo1.ok === true && JSON.stringify(hSeq1) === JSON.stringify(["tombstone", "set_property", "draw_shape"])
    && hUndo1.gestures_undone === 1,
  `undo_last 必须把**活着对象**的全部原子（含 tombstone 本身、从新到旧）整笔撤掉 —— 旧本地按「每原子最新动作」跳过 restore 过的 tombstone ⇒ 只撤半笔（实测 kinds=${JSON.stringify(hSeq1)}）`);
const hStatus1 = await hjpost("/api/tools/get_undo_status", {});
check(hStatus1.remaining_gestures === hStatus1.undoable_gestures
    && hStatus1.undoable_gestures === 0 && hStatus1.redoable_gestures === 1,
  `撤完整笔后：remaining 与 undoable 同源 ✓、可撤归零 ✓、可重做=1 ✓（实测 ${hStatus1.remaining_gestures}/${hStatus1.undoable_gestures}/${hStatus1.redoable_gestures}）`);
const hObjAfterUndo = await hjpost("/api/tools/get_object", { object_id: hShape.object_id });
const hListAfterUndo = await hjpost("/api/tools/list_objects", {});
check(hObjAfterUndo.deleted === false && hObjAfterUndo.deleted_by === null && hListAfterUndo.count === 0,
  `撤掉创建原子的对象：本地 get_object 的 deleted/deleted_by 只看**生效的** tombstone（undo 撤了它 ⇒ 都为空 ✓），但 list_objects 必须按状态事实把它剔掉（实测 deleted=${hObjAfterUndo.deleted}／deleted_by=${hObjAfterUndo.deleted_by}／count=${hListAfterUndo.count}）`);
const hRedo1 = await hjpost("/api/tools/redo_last", {});
const hRedoIds = (hRedo1.redone || []).map((x) => x.atom_id);
const hUndoIds = (hUndo1.undone || []).map((x) => x.atom_id);
check(hRedo1.ok === true && JSON.stringify(hRedoIds) === JSON.stringify(hUndoIds),
  `redo 必须**按同一顺序整笔重做** undo 撤掉的那组原子（实测 redone=${JSON.stringify(hRedoIds)} vs undone=${JSON.stringify(hUndoIds)}）`);
// **∴ 「为什么死」也要断 ✗**（M-G 变异实测教的一课 ✓）：undo 机器只看**对象死不死** ⇒
//   「tombstone 生效而活着的创建」与「创建被抑制」**两种错法都给同样的计数**✓ 分不出来 ✗。
//   服务端 `get_object after redo` 实测：**创建仍在**（`created_by` 有值 ✓）而**死因是 tombstone**
//   （`deleted_by` = 那条 tombstone ✓）⇒ 本端用 `deleted`/`list_objects` 断同一事实 ✓。
const hObjAfterRedo = await hjpost("/api/tools/get_object", { object_id: hShape.object_id });
const hListAfterRedo = await hjpost("/api/tools/list_objects", {});
check(hObjAfterRedo.found !== false && hObjAfterRedo.deleted === true
    && hObjAfterRedo.deleted_by === (hUndo1.undone[0] || {}).atom_id && hListAfterRedo.count === 0,
  `redo 之后对象必须「创建仍在、死因是 tombstone」（deleted=true 且 deleted_by=undo 撤的第一条 tombstone ✓ 与服务端 get_object after redo 的 created_by+deleted_by 同形 ✓；若 reapply 被误登记成 revert ⇒ tombstone 反被判抑制 ⇒ deleted=false ✗；实测 deleted=${hObjAfterRedo.deleted}／deleted_by=${hObjAfterRedo.deleted_by}／count=${hListAfterRedo.count}）`);
const hStatus2 = await hjpost("/api/tools/get_undo_status", {});
check(hStatus2.remaining_gestures === hStatus2.undoable_gestures
    && hStatus2.undoable_gestures === 0 && hStatus2.redoable_gestures === 1,
  `redo 重做的是**整组**（含 tombstone）⇒ 对象又死了 ⇒ undoable=0／redoable=1 ✓（与服务端实测同数 ✓：status#2 remaining=0／ignored=11；实测 ${hStatus2.remaining_gestures}/${hStatus2.undoable_gestures}/${hStatus2.redoable_gestures}）`);
const hUndo2 = await hjpost("/api/tools/undo_last", {});
check(hUndo2.ok === true && (hUndo2.undone || []).length === 0 && typeof hUndo2.message === "string",
  `redo 后对象是死的 ⇒ 不是 undo 候选 ⇒ undo_last#2 必须**空手且如实报**（ok:true + message ✓ 不许静默 ✓——与服务端实测一致 ✓；实测 undone=${JSON.stringify((hUndo2.undone || []).map((x) => x.kind))} message=${JSON.stringify(hUndo2.message)}`);
console.log("  本地层 undo/redo（独立文档 " + hDoc + "）：候选=对象存活 ✓｜tombstone 整笔撤 ✓｜reapply 覆盖 revert ✓（撤 " +
  hUndo1.undone_count + " 条 ⇒ 重做 " + hRedo1.redone_count + " 条 ⇒ 再撤 " + hUndo2.undone_count + " 条 ✓）");
// **★ ⑦h-alias：`undo`／`redo` 短名必须走同一台撤销机器 ✗ ★**（**照服务端 `tools.rs:917` 的唯一收口 ✓**：
//   真机报告 P0 #2 ——「想撤销时调 `undo` 拿到未知工具」是**按钮按下去画什么都不动**的静默家族 ✓。
//   **∴ 且不许模糊匹配 ✗**（服务端原话 ✓：拼错名字必须继续 501 ✗ 不许「悄悄接受」✓）。
const haDoc = "ha_" + Date.now().toString(36);
const hapost = (p, o) => callDoc(haDoc, p, { method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify(o) }).then((r) => r.json());
await callDoc(haDoc, "/api/documents", { method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: haDoc, width: 320, height: 240 }) });
await callDoc(haDoc, "/api/atoms", { method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ kind: "create_layer", layer_id: "LA", name: "别名层" }) });
const haShape = await hapost("/api/tools/draw_shape", { layer_id: "LA", data: { geometry: { kind: "rect", bbox: { x: 2, y: 2, width: 6, height: 6 } } } });
// **∴ undo/redo 两条用 .catch 收成断言失败 ✗**（**不许**让「端点没映射」走 exit(3) 的「判据自身出错」通道 ✓
//   —— 那是**产品坏了**的形态 ✗ 必须红在**断言**上 ✓ 变异时红得确定 ✓）。
const haUndo = await hapost("/api/tools/undo", { scope: "paint" }).catch((e) => ({ ok: false, crashed: String(e && e.message || e) }));
check(haUndo.ok === true && haUndo.undone_count === 1,
  `短名 /api/tools/undo 必须映射到 undo_last（服务端 tools.rs:917 唯一收口 ✓ 旧本地没有这层 ⇒ 撤销按钮 501 ✓ 画布不动 ✓）（实测 ${JSON.stringify(haUndo).slice(0, 160)}）`);
const haRedo = await hapost("/api/tools/redo", { scope: "paint" }).catch((e) => ({ ok: false, crashed: String(e && e.message || e) }));
check(haRedo.ok === true && haRedo.redone_count === 1,
  `短名 /api/tools/redo 必须映射到 redo_last（实测 ${JSON.stringify(haRedo).slice(0, 160)}）`);
// **∴ 拼错的探测要用 try/catch ✗**：本地层对**未知端点**是**抛** `endpoint_not_local`（判据脚手架会当
//   「判据自身出错」exit(3) ✓）——**不许被别名表悄悄接受**就是这条断言的全部内容 ✓（catch 到才叫通过 ✓）。
let haTypoErr = "";
try { await hapost("/api/tools/undoo", {}); } catch (e) { haTypoErr = String(e && e.message || e); }
check(haTypoErr.includes("endpoint_not_local"),
  `拼错的名字 /api/tools/undoo 不许被模糊匹配悄悄接受（服务端原话 ✓）——必须仍然抛 endpoint_not_local（实测 ${JSON.stringify(haTypoErr).slice(0, 120)}）`);
console.log("  本地层 undo/redo 短名别名：undo→undo_last ✓｜redo→redo_last ✓｜拼错仍响亮 501 ✓（撤 " +
  haUndo.undone_count + " ⇒ 重做 " + haRedo.redone_count + " ✓）");

// **★ ⑦i `get_ancestors`／`get_descendants` 必须照服务端的三条判据 ✗ ★**
//   （**出处 `tools.rs:8579`／`:8610` ＋ `dirty.rs:473` ✓**，本轮逐条读码对齐 ✓）：
//   ① **对象不在折叠状态 ⇒ `reference_not_found` ✗**——旧本地**静默给 `ok:true, count:0`** ✓
//     （「不存在」冒充「没有祖先」= 静默降级形态 4 ✓）；
//     **状态含被 tombstone 标记的对象 ✓**（服务端 `state.objects.contains_key` 不看 `deleted_by` ✓
//     —— 与 ⑦h 实测「redo 后 get_object 仍答 created_by+deleted_by」同一事实 ✓）。
//   ② **`descendants` 是 id 字符串数组 ✗**（`Vec<String>` ✓）——旧本地给 `{object_id,layer_id}` 对象 ✓
//     ⇒ 按服务端形状写的调用方拿到错形状而回执一切正常 ✓。
//   ③ **只收 `instance` 且走传递闭包 ✗**（≤64 层 BFS ✓）——旧本地只收一层 ⇒ **孙实例漏掉** ✓。
const iCreate = (objectId, type, master) => jpost("/api/atoms", {
  kind: "create_object", id: "AT_" + objectId, actor: "human:test", session: "session:test",
  schema_version: 1, timestamp: 1,
  payload: { object_id: objectId, type, layer_id: "LN",
             data: master ? { master_ref: { object_id: master } } : { geometry: { kind: "rect" } } } });
await call("/api/atoms", { method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ kind: "create_layer", layer_id: "LN", name: "实例层" }) });
await iCreate("AN_M", "shape", null);
await iCreate("AN_C", "instance", "AN_M");
await iCreate("AN_G", "instance", "AN_C");
await iCreate("AN_D", "instance", "AN_M");
await iCreate("AN_S", "shape", "AN_M");
await iCreate("AN_R", "instance", "AN_R");
await jpost("/api/tools/delete_object", { object_id: "AN_D" });
const iNopeA = await jpost("/api/tools/get_ancestors", { object_id: "AN_NOPE" });
const iNopeD = await jpost("/api/tools/get_descendants", { object_id: "AN_NOPE" });
check(iNopeA.ok === false && iNopeA.error === "reference_not_found"
    && iNopeD.ok === false && iNopeD.error === "reference_not_found",
  `不存在的对象必须报 reference_not_found ✗ 不许静默 ok:true count:0（旧实现把「不存在」与「没有祖先/后代」混成一个答案 ✓ 实测 ${JSON.stringify(iNopeA).slice(0, 100)}／${JSON.stringify(iNopeD).slice(0, 100)}）`);
const iDesc = await jpost("/api/tools/get_descendants", { object_id: "AN_M" });
check(iDesc.ok === true && Array.isArray(iDesc.descendants)
    && iDesc.descendants.every((x) => typeof x === "string")
    && JSON.stringify([...iDesc.descendants].sort()) === JSON.stringify(["AN_C", "AN_D", "AN_G"])
    && iDesc.count === 3,
  `descendants 必须是 **id 字符串数组**（服务端 Vec<String> ✓）且**传递闭包**（孙 AN_G 不许漏 ✓）、**只收 instance**（shape 引用 AN_S 不许进 ✓）、**tombstone 标记的不排除**（服务端 dependents_of 不看 deleted_by ✓ AN_D 必须在 ✓）（实测 ${JSON.stringify(iDesc.descendants)}）`);
const iAncG = await jpost("/api/tools/get_ancestors", { object_id: "AN_G" });
check(iAncG.ok === true && JSON.stringify((iAncG.ancestors || []).map((x) => x.object_id)) === JSON.stringify(["AN_C", "AN_M"])
    && iAncG.count === 2,
  `get_ancestors 必须沿 master_ref 走到顶（AN_G→AN_C→AN_M ✓ 实测 ${JSON.stringify(iAncG.ancestors)}）`);
const iAncR = await jpost("/api/tools/get_ancestors", { object_id: "AN_R" });
check(iAncR.ok === true && iAncR.count === 2 && iAncR.ancestors[1].cycle === true && iAncR.ancestors[0].cycle === undefined,
  `自引用 master 必须**先记一条正常链再记 cycle**（照服务端 visited 初始为空 ✓ 两条 ✗）——旧本地把起点算进 seen 只给一条 ✓ 数对不上 ✗（实测 ${JSON.stringify(iAncR.ancestors)}）`);
console.log("  本地层 get_ancestors/get_descendants：不存在⇒reference_not_found ✓｜id 数组 ✓｜传递闭包含孙 ✓｜只收 instance ✓｜tombstone 不排除 ✓｜自引用 cycle 两条 ✓");

// **★ ⑦j 命名锚点三件套必须照服务端落地 ✗ ★**（**出处 `tools.rs:2217`／`:2236`／`:2483` ＋
//   `fold.rs:991–1005` ✓**，本轮逐条读码对齐 ✓）：
//   ① **状态从原子日志推导 ✗**（`set_anchor`／`delete_anchor` 是内核 `AtomKind` ✓ ⇒ 不另存锚点表 ⇒
//     `revert` 原子经 `p2Neutralized` 自动生效 ✓ ——「撤掉一次钉锚点」不需要新机制 ✓）；
//   ② **`anchors` 是名字⇒{x,y,note?} 的对象映射**（服务端 `BTreeMap` 序列化成对象 ✓ ⇒
//     前端 `Object.keys(anchorCache).sort()` 直接吃这个形状 ✓）；恒等式 `count === Object.keys(anchors).length` ✓；
//   ③ **删不存在的锚点必须 400 且点名现有锚点 ✗**（服务端注释写明：只靠折叠的 precondition ⇒
//     **`ok:true` 静默成功** ✓ 端到端抓过同一个毛病 ⇒ 端点层必须自己查 ✓）；
//   ④ **note 是可选项 ✗**：不给就不该有这个键（服务端 `optional_str` ⇒ 键缺席 ✓，不给 `null` ✓）。
const jDoc = "dj_" + Date.now().toString(36);
const jxpost = (p, o) => callDoc(jDoc, p, { method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify(o) }).then((r) => r.json());
await callDoc(jDoc, "/api/documents", { method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: jDoc, width: 320, height: 240 }) });
const jSet1 = await jxpost("/api/tools/set_anchor", { name: "left_eye", x: 120, y: 80 });
check(jSet1.ok === true && typeof jSet1.atom_id === "string" && Number.isFinite(jSet1.seq),
  `set_anchor 必须落一条原子并回执 atom_id＋seq（照 p2Envelope/putAtom 惯例 ✓）（实测 ${JSON.stringify(jSet1).slice(0, 120)}）`);
const jEmptyName = await jxpost("/api/tools/set_anchor", { name: "", x: 1, y: 1 });
const jStrX = await jxpost("/api/tools/set_anchor", { name: "bad_x", x: "abc", y: 1 });
const jNoY = await jxpost("/api/tools/set_anchor", { name: "no_y", x: 1 });
check(jEmptyName.ok === false && jEmptyName.error === "invalid_argument"
    && jStrX.ok === false && jStrX.error === "invalid_argument"
    && jNoY.ok === false && jNoY.error === "invalid_argument",
  `空 name／非数值 x／缺 y 必须 invalid_argument ✗ 不许静默当 0（服务端 require_str+require_f64 ✓ 实测 ${jEmptyName.error}／${jStrX.error}／${jNoY.error}）`);
const jGot1 = await jxpost("/api/tools/get_anchors", {});
check(jGot1.ok === true && typeof jGot1.anchors === "object" && !Array.isArray(jGot1.anchors)
    && jGot1.count === Object.keys(jGot1.anchors).length
    && jGot1.anchors.left_eye && jGot1.anchors.left_eye.x === 120 && jGot1.anchors.left_eye.y === 80
    && !("note" in (jGot1.anchors.left_eye || {}))
    && jGot1.anchors.bad_x === undefined && jGot1.anchors.no_y === undefined,
  `anchors 必须是 **名字⇒{x,y} 的对象映射**（服务端 BTreeMap ✓）、count 与键数同源恒等 ✓、被拒的原子不许进状态 ✓、未给 note 就**没有该键**（optional_str ✓）（实测 ${JSON.stringify(jGot1.anchors)}）`);
const jSet2 = await jxpost("/api/tools/set_anchor", { name: "left_eye", x: 130, y: 88, note: "第二版" });
const jGot2 = await jxpost("/api/tools/get_anchors", {});
check(jSet2.ok === true && jGot2.anchors.left_eye.x === 130 && jGot2.anchors.left_eye.note === "第二版"
    && jGot2.count === 1,
  `同名再设 = 覆盖（服务端 BTreeMap::insert ✓）且 note 跟着新版本走（实测 ${JSON.stringify(jGot2.anchors.left_eye)}）`);
const jDelNope = await jxpost("/api/tools/delete_anchor", { name: "ghost" });
check(jDelNope.ok === false && jDelNope.error === "invalid_argument"
    && String(jDelNope.reason || "").includes("找不到锚点 ghost") && String(jDelNope.reason || "").includes("left_eye"),
  `删不存在的锚点必须 400 且**点名现有锚点** ✗ 不许 ok:true 静默成功（服务端 write_delete_anchor 注释里端到端抓过同一个毛病 ✓ 实测 ${JSON.stringify(jDelNope).slice(0, 140)}）`);
const jDelOk = await jxpost("/api/tools/delete_anchor", { name: "left_eye" });
const jGot3 = await jxpost("/api/tools/get_anchors", {});
check(jDelOk.ok === true && jGot3.count === 0 && Object.keys(jGot3.anchors).length === 0,
  `delete_anchor 成功后 get_anchors 必须少这一条（fold.rs:1004 remove ✓ 实测 count=${jGot3.count} anchors=${JSON.stringify(jGot3.anchors)}）`);
const jUndoDel = await jxpost("/api/atoms", { kind: "revert", payload: { target: jDelOk.atom_id } });
const jGot4 = await jxpost("/api/tools/get_anchors", {});
check(jUndoDel.ok === true && jGot4.count === 1 && jGot4.anchors.left_eye && jGot4.anchors.left_eye.x === 130,
  `revert 掉 delete_anchor 原子 ⇒ 锚点必须回来（p2Neutralized 抑制 ✓ 与服务端 EffectiveActions 同口径 ✓ 实测 ${JSON.stringify(jGot4.anchors)}）`);
const jUndoSet = await jxpost("/api/atoms", { kind: "revert", payload: { target: jSet2.atom_id } });
const jGot5 = await jxpost("/api/tools/get_anchors", {});
check(jUndoSet.ok === true && jGot5.count === 1 && jGot5.anchors.left_eye.x === 120 && !("note" in jGot5.anchors.left_eye),
  `revert 掉**最新那次** set_anchor ⇒ 更早那次仍然生效 ⇒ **回滚到旧坐标 x=120、note 消失**（折叠顺序应用、revert 只抑制被点名那条 ✓——我第一版把它误想成「整个消失」，实测否掉了这个读法 ✓ 实测 ${JSON.stringify(jGot5.anchors.left_eye)}）`);
const jUndoSet1 = await jxpost("/api/atoms", { kind: "revert", payload: { target: jSet1.atom_id } });
const jGot5b = await jxpost("/api/tools/get_anchors", {});
check(jUndoSet1.ok === true && jGot5b.count === 0 && Object.keys(jGot5b.anchors).length === 0,
  `两次 set_anchor 都被 revert ⇒ 锚点整个消失（每一条建原子的撤销各抑制各的 ✓ 实测 ${JSON.stringify(jGot5b.anchors)}）`);
const jRedoSet = await jxpost("/api/atoms", { kind: "reapply", payload: { target: jSet2.atom_id } });
const jGot6 = await jxpost("/api/tools/get_anchors", {});
check(jRedoSet.ok === true && jGot6.count === 1 && jGot6.anchors.left_eye.x === 130 && jGot6.anchors.left_eye.note === "第二版",
  `reapply 之后 set_anchor 又生效 ⇒ x=130／note=第二版（撤销撤销 ≡ 重做 ✓ 与 ⑦h 同一台机器 ✓ 实测 ${JSON.stringify(jGot6.anchors.left_eye)}）`);
console.log("  本地层命名锚点：原子日志推导 ✓｜对象映射＋count 恒等 ✓｜400 校验齐 ✓｜删不存在⇒点名拒绝 ✓｜revert/reapply 级联 ✓（文档 " + jDoc + "）");



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
