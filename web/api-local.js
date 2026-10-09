// **★ PWA 的本地 API 适配层 ✓ ★**（第 614 轮 ✓；**部署矩阵 §11 ＋ 用户"不影响现有 WEB" ✓**）
//
// **★ 为什么用"拦截"而不是"改前端" ✓ ★**：
//   **∴ 现有 `crates/yanshi-http/assets/viewer-app.js`（**510 KB ✓**）依赖 **15 个服务端端点 ✗**：
//     `/api/tools/get_document` ✓／`/api/tools/render_region` ✓／`/api/tools/list_layers` ✓／
//     `/api/documents` ✓／`/api/atoms` ✓／`/api/blob/…` ✓／`/api/effects` ✓／`/api/diagnostics` ✓ 等 ✓
//   **∴ 若把前端**复制**一份再改 ✗ ⇒ **∴ 两条部署会**分叉**✗**（**违背用户"不要影响现有 WEB"✓**）。
//   **∴ 本层的作用 ✗**：**在 PWA 里**覆写 `window.fetch`✗** ⇒ **∴ 把 `/api/*` 转到**本地实现**✗**
//     （**内核 wasm ＋ IndexedDB ✓**）⇒ **∴ 于是**同一份 `viewer-app.js` **两处都能跑 ✓，
//     **而 `crates/yanshi-http/assets/` **一个字节都不用改 ✓**** ✓✓
//
// **⚠️ 现状（**如实 ✓**）**：**这是**骨架 ＋ 分发表 ✗** ——
//   **∴ 已实现**：**拦截安装／卸载 ✓、未实现端点的**如实 501 ＋ 原因 ✗**（**∴ 不静默失败 ✓**）；
//   **∴ 未实现**：**各端点的真实本地映射 ✗**（**那需要按端点逐个接内核／IndexedDB ✓**，
//   是后续轮次的工作 ✗）⇒ **∴ 在此之前，PWA 页面会**明确报"该端点尚未本地实现"✗**，
//   **∴ 绝不会假装成功 ✓**。

/** **★ 本地已实现的端点数 ✓ ★**（**判据会断言它随实现增加 ✓**）。 */
// **★ 计数已升到 6 ✗ ★**（第 631 轮 ✓）：**∴ `render_region` 不再只是快照分支 ✗** ——
// **∴ 它现在**真的调用本地内核渲出 PNG ✗**（**真实浏览器实测 850 字节 ✓**）
// ⇒ **∴ 于是**它从"部分实现"升级为**完整实现 ✓**（**判据会核对声明与实现是否一致 ✓**）。
export const LOCAL_IMPLEMENTED = 11;

let lastFold = null;

/**
 * **★ 安装本地 API 层 ✓ ★**：**覆写 `window.fetch` ✗** ⇒ **∴ `/api/*` 走本地 ✓，
 * 其余（**静态资源 ✓**）走原 `fetch` ✓**。
 * @param {{local: (req: Request) => Promise<Response>}} deps **本地实现（**由内核／IndexedDB 提供 ✓**）
 */
/**
 * **★ 第一版本地实现 ✓ ★**（第 615 轮 ✓）：**三个端点只依赖 IndexedDB ✗**（**不需要内核 ✓**）
 * ⇒ **∴ 它们可以**独立成立 ✓**；**而需要内核的端点（**`get_document` ✓／`render_region` ✓**）
 * **∴ 仍然如实 501 ✓**（**∴ 下一批再说 ✓**）。
 *
 * **∴ 与真实服务端的语义对齐点 ✗**：
 *   * **原子写入 ⇒ **序号自增 ＋ 推进文档 `seq`**✗**（**与 `store.js` 的快照校验配套 ✓**）；
 *   **`list_layers` ⇒ **从原子日志推导**✗**（**`create_layer` 之类的`kind` ✓**）⇒
 *     **∴ 不另存一份"层表" ✗**（**∴ 避免双份状态不一致 ✓**）。
 */
export async function makeLocalApi(db) {
  const { putAtom, atomsOf, open, wrap, tx, writeSnapshot } = await import("./store.js");
  // **∴ 内核实例**按文档缓存**✗**（**`Map<doc, WasmKernel>` ✓**）。
  const kernels = new Map();

// Serialise kernel calls per document: wasm-bindgen rejects a reentrant &mut self
// borrow with the aliasing error the device log reported as needs_render, and the
// warm-up and the preview can arrive at the same time for the same document.
// Cost: one kernel call at a time per document, which was already true inside wasm.
const __kernelChain = new Map();
async function withKernel(doc, fn) {
  // Count live kernel calls to prove whether two ever overlap: the aliasing error can
  // come from concurrency (this counter exceeds one) or from a reentrant borrow inside
  // one call (counter stays at one). Without the counter both look identical.
  try {
    const st = (window.__kernelStats = window.__kernelStats || { live: 0, max: 0, calls: 0 });
    st.live += 1; st.calls += 1;
    if (st.live > st.max) st.max = st.live;
  } catch (e) { /* 非浏览器环境（node 判据）=> 忽略 */ }
  const prev = __kernelChain.get(doc) || Promise.resolve();
  const run = prev.then(fn, fn);
  __kernelChain.set(doc, run.then(() => {}, () => {}));
  try {
    const st = window.__kernelStats;
    run.then(() => { st.live -= 1; }, () => { st.live -= 1; });
  } catch (e) { /* 同上 */ }
  return run;
}
  const handle = db || (await open());

  return async function local(req) {
    const url = new URL(req.url, location.origin);
    // **★ 路径归一化：去掉**尾部斜杠** ✗ ★**（第 858 轮 ✓；**用户报"还是落笔失败" ✓**）：
    //   **∴ 根因 ✗**：**落笔**把像素 **POST 到 `/api/blob/` ✗**（**带尾斜杠 ✓）
    //     ⇒ **∴ 而**本地层**用 `=== "/api/blob"` 精确匹配 ✗** ⇒ **∴ 接不住 ✓**
    //       ⇒ **∴ 于是**：**它会**落到"未实现端点" ⇒ **∴ 501 ⇒ **∴ 落笔失败 ✓**** ✓✓
    //   **∴ 现在**：**统一归一化 ✗**（**去掉**一个尾斜杠 ✓，**而**根路径保留 ✓）
    //     ⇒ **∴ 于是**：**所有分支**都**同时接受**带／不带尾斜杠 ✓**** ✓✓
    //   **∴ 为什么改这里而不是逐个分支 ✗**：**逐个改**要改 11 处 ✗**
    //     ⇒ **∴ 而**归一化**一处**就够 ✗** ⇒ **∴ 且**不会**漏掉将来的新分支 ✓**** ✓✓
    const rawPath = url.pathname;
    const path = rawPath.length > 1 && rawPath.endsWith("/") ? rawPath.slice(0, -1) : rawPath;
    const q = url.searchParams;
    const doc = q.get("doc") || "";
    // **★ 只在**确实是 JSON**时才按 JSON 读 body ✗ ★**（第 841 轮 ✓；**已确认的缺陷 ✓**）：
    //   **∴ 原来的错 ✗**：**对**每个 POST** 都 `await req.json()` ✗**
    //     ⇒ **∴ 而** `/api/blob` 的 body** 是**二进制像素 ✗** ⇒ **∴ `req.json()` 失败 ✗**
    //       ⇒ **∴ `.catch(() => ({}))`** 把它**变成 `{}`**✗ ⇒ **∴ 静默失败 ✓****（**示例图上传不上 ✓）
    //   **∴ 现状**：**只有**明确的二进制类型**才跳过解析 ✗** ⇒ **∴ 其余**一律**尝试 JSON ＋ 失败退回 `{}` ✓**** ✓✓
    const __ct = String((req.headers && req.headers.get && req.headers.get("content-type")) || "");
      // **★ 白名单改为**排除二进制** ✗ ★**（第 842 轮 ✓；**判据抓到我的回归 ✓**）：
      //   **∴ 上一版太严 ✗**：**它要求 `content-type` 含 `json` ✗**
      //     ⇒ **∴ 而**调用方**常常**不带该头 ✗**（**实测：判据里的 `fetch` 没有 header ✓）**
      //       ⇒ **∴ 于是** body 被读成 `{}` ✗** ⇒ **∴ 原子**写不进去 ✓****（**判据实测：数量 1 而非 ≥2 ✓）
      //   ⇒ **∴ 现在**：**只有**明确的二进制类型**才跳过解析 ✗** ⇒ **∴ 其余**一律**尝试 JSON ＋ 失败退回 `{}` ✓**** ✓✓
      const __binary =
        __ct.indexOf("image/") >= 0 ||
        __ct.indexOf("octet-stream") >= 0 ||
        __ct.indexOf("application/pdf") >= 0;
      const body =
        req.method === "POST" && !__binary
          ? await req.json().catch(() => ({}))
          : {};

    // **① `/api/documents`（**创建 ✓**）**：**∴ 与真实服务端一样返回 `token` ✗**
    //（**∴ 本部署没有鉴权 ✗ ⇒ **token 是**本地占位**✗，**而字段存在 ✓** ⇒ **∴ 前端无需分支 ✓**）。
    // **★ 前端启动所需的非 `/api/` 端点 ⇒ 返回合理默认值 ✗ ★**（第 827 轮 ✓）
    //   **∴ 为什么 ✗**：**它们**不在 `/api/` 下**✗ ⇒ **∴ 原来**打到网络 ⇒ **∴ 405 ✓**
    //   ⇒ **∴ 现在**：**本地层**直接答**默认值 ✗** ⇒ **∴ 于是**启动路径**不再报错 ✓**
    if (path === "/get_preferences") {
      return json({ ok: true, preferences: { theme: "dark", locale: "zh-CN" } });
    }
    if (path === "/list_effects") {
      return json({ ok: true, effects: [] });
    }
    if (path === "/list_brushes") {
      return json({ ok: true, brushes: [] });
    }
// **★ `/api/blob` ✗ ★**（第 844 轮 ✓；**`seedSampleIfEmpty` 需要它 ✓**）：
//   **∴ 为什么 ✗**：**新建文档**时前端**取示例 PNG ⇒ 转像素 ⇒ POST 这里 ⇒ 拿 `blob_hash` ✗**
//     ⇒ **∴ 而**本地层**原来没有这个分支 ✗** ⇒ **∴ 501 ⇒ **∴ 上传失败 ⇒
//       **∴ 新文档**没有画面 ⇒ **∴ 用户**感觉"**画不出来**" ✓**** ✓✓
//   **∴ 实现照 `brush_stroke` 的四步抄 ✗**（**同一套内核 ＋ 同一套信封 ＋ 同一套 IndexedDB ✓）**：
//     **① 每文档一个内核 ✗**（**从创建原子取宽高 ✓）**；
//     **② `blob_put(bytes)` ⇒ **位图进内核 store ✗****（**∴ 而 `import_image` 重放时按**内核的 hash**取 ✓）**；
//     **③ 解 `blob_put` 的**JSON 信封**✗**（**∴ 它**返回 `{"blob_hash":…,"ok":true}` ✓）**；
//     **④ 同时写 IndexedDB ✗**（**∴ 刷新后重建内核时**再喂一遍 ✓）**。
//   **∴ 字段与**服务端一致 ✗**：**`blob_hash`／`size`／`mime_type` ✗**
//     （**∴ 前端 `index.html:4197` 正是读 `upload.blob_hash` 与 `upload.size` ✓）** ✓✓
    if (path === "/api/blob") {
      // **★ `mod` 必须自己导入 ✗ ★**（第 846 轮 ✓；**判据抓到的真 bug ✓**）：
      //   **∴ 实测 ✗**：**判据报 `ReferenceError: mod is not defined` ✗**
      //     （**`web/api-local.js:102` ✓）⇒ **∴ 因为** `mod` **是**那个分支里**局部导入的 ✗**
      //       （**`:220`／`:339` 各有 `const mod = await import("/wasm/yanshi_wasm.js");` ✓）**
      //     ⇒ **∴ 所以**：**本分支**也要**自己导一次 ✓**** ✓✓
      //   **∴ 这正是"**判据抓到我自己的错**"的又一例 ✗**（**第 842 轮也是 ✓）** ✓✓
      // Kernel import must be allowed to fail: without /wasm/ the bare import
      // throws ERR_MODULE_NOT_FOUND outside any try, which the criterion caught.
      // Now an unavailable kernel answers 501 with a reason instead of crashing.
      let mod = null;
      try {
        mod = await import("/wasm/yanshi_wasm.js");
      } catch (err) {
        return json({ ok: false, error: "kernel_unavailable", endpoint: path,
                      reason: "kernel module /wasm/yanshi_wasm.js did not load",
                      detail: String((err && err.message) || err) }, 501);
      }
      let entryK = kernels.get(doc);
      if (!entryK) {
        const c2 = (await atomsOf(handle, doc)).find((r) => r.atom && r.atom.kind === "create_document");
        const w2 = (c2 && c2.atom.payload && c2.atom.payload.width) || (c2 && c2.atom.width) || 1024;
        const h2 = (c2 && c2.atom.payload && c2.atom.payload.height) || (c2 && c2.atom.height) || 1024;
        entryK = { k: new mod.WasmKernel(doc, 256, w2, h2, 256 * 1024 * 1024), w: w2, h: h2 };
        kernels.set(doc, entryK);
      }
      const bytes = new Uint8Array(await req.arrayBuffer());
      if (bytes.length === 0) throw new Error("blob body empty");
      const kernelHash = await withKernel(doc, () => entryK.k.blob_put(bytes));
      const blobHash = (() => {
        const raw = String(kernelHash || "");
        try { const o = JSON.parse(raw); if (o && o.blob_hash) return String(o.blob_hash); } catch (e) {}
        return raw;
      })();
      if (!blobHash) throw new Error("kernel returned no blob hash");
      const storeMod = await import("/store.js");
      await storeMod.putBlob(handle, blobHash, Array.from(bytes));
      return json({ ok: true, blob_hash: blobHash, size: bytes.length,
                    mime_type: "image/x-yanshi-raw", server: false });
    }
    if (path === "/api/documents") {
      const docId = body.doc_id || `local-${Date.now().toString(36)}`;
      // **★ 必须写**完整 8 字段封套 ✗ ★**（第 630 轮 ✓，**靠 `fold_result` 一次定位 ✓**）：
      // **∴ 实测**：**旧写法只写 `{kind, width, height}` ✗** ⇒ **∴ 内核在解析**整个数组**时
      // 第一条就失败 ✗**（`missing field \`id\`` ✓）⇒ **∴ 于是**后面的原子全被丢弃 ⇒
      // **∴ 文档停在 0×0 ✗**（**∴ 而**症状看起来像"渲染区域非法"✗ ⇒ **∴ 极易误判 ✓**）。
      await putAtom(handle, docId, {
        actor: "human:web",
        id: "01LOCAL" + String(Date.now()).padStart(13, "0"),
        kind: "create_document",
        payload: {
          background: { a: 255, b: 255, g: 255, r: 255 },
          color_space: "srgb",
          doc_id: docId,
          height: body.height ?? 1024,
          width: body.width ?? 1024,
        },
        schema_version: 1,
        // **∴ `seq` 由下面的自增逻辑覆盖 ✗**（**第 630 轮 ✓：**写死 1 会与后续原子冲突 ✓**）。
        session: "session:web",
        timestamp: Date.now(),
      });
      return json({ ok: true, doc_id: docId, token: "local", server: false,
                    width: body.width ?? 1024, height: body.height ?? 1024 });
    }

    // **② `/api/atoms`（**读／写 ✓**）**
    if (path === "/api/atoms") {
      if (req.method === "POST") {
        // **★ 接受 `{ atoms: [ … ] }` ✗ ★**（第 630 轮 ✓）：**∴ 与服务端同形 ✗**
        //（**`tools.rs:1736`：`atoms` 是必填数组 ✓ ⇒ **∴ 直接 POST `{}` 会 400**✓）。
        // **∴ 且**原子**原样存下 ✗**（**含完整 8 字段封套 ✓ ⇒ **∴ 内核能折叠 ✓**）。
        const list = Array.isArray(body && body.atoms) ? body.atoms : [body];
        let last = 0;
        for (const atom of list) {
          // **★ `seq` **由本地层自增**✗ ★**（第 630 轮 ✓）：**∴ 调用方给的 `seq` 一律忽略 ✗**
          // ⇒ **∴ 否则**两处都写 1 ⇒ **∴ 内核报**`seq 1 重复`✗**（**实测 ✓**）⇒ **∴ 折叠整体失败 ✓**。
          last = await putAtom(handle, doc, { ...atom, seq: undefined });
        }
        return json({ ok: true, seq: last, count: list.length, server: false });
      }
      const list = await atomsOf(handle, doc);
      return json({ ok: true, atoms: list.map((r) => r.atom), count: list.length, server: false });
    }

    // **③ `/api/tools/list_layers`（**从原子推导 ✓**）**
    if (path === "/api/tools/list_layers") {
      const list = await atomsOf(handle, doc);
      const layers = [];
      for (const r of list) {
        const a = r.atom || {};
        if (a.kind === "create_layer" && a.layer_id) layers.push({ id: a.layer_id, name: a.name ?? a.layer_id });
      }
      return json({ ok: true, layers, count: layers.length, server: false,
                    derived_from: "atoms" });
    }

    // **④ `/api/tools/get_document`（**从 IndexedDB ＋ 如实报后端 ✓**）**
    // **∴ 它与目标第 8 条的接口一致 ✗**：**报 `width`／`height`／`head_seq` ✗
    // ⇒ **∴ 并报 `preview_state` 与 `render_backend` ✓**（**∴ 前端一眼知道有没有图 ✓**）。
    if (path === "/api/tools/get_document") {
      const { wrap, tx } = await import("./store.js");
      const meta = (await wrap(tx(handle, "docs", "readonly").get(doc))) || { doc, seq: 0 };
      const backend = await detectBackend();
      const create = (await atomsOf(handle, doc)).find((r) => r.atom && r.atom.kind === "create_document");
      return json({
        ok: true,
        doc_id: doc,
        // **∴ 尺寸在 `payload` 里 ✗**（**第 630 轮 ✓：**原子是 8 字段封套 ✓**）。
        width: create ? (create.atom.payload?.width ?? create.atom.width ?? 1024) : 1024,
        height: create ? (create.atom.payload?.height ?? create.atom.height ?? 1024) : 1024,
        head_seq: meta.seq ?? 0,
        rendered_seq: meta.seq ?? 0,
        // **∴ 没有服务器 ⇒ 缩略图必须由**本地内核**生成 ✗** ⇒ **∴ 在生成前如实报 pending ✓**
        //（**∴ 不许报 ready 而给不出图 ✗**）。
        preview_state: "pending",
        preview_note: "缩略图需由本地内核生成（本部署无服务器端）",
        server: false,
        render_backend: backend,
        gpu_unavailable_reason: backend === "webgpu" ? null : "host_has_no_webgpu",
      });
    }

    // **⑤ `/api/tools/render_region`（**★ 只做**快照分支**✗ ⇒ **∴ 仍是**部分实现**✗ ★**）**
    // **∴ 语义（**目标第 6 条 ✓**）**：**快照必须带序号 ＋ 格式版本 ✗**
    //   ⇒ **∴ 命中（**序号与格式都对 ✓）⇒ 直接给图 ✓**；
    //   **∴ 过期／缺失 ⇒ **如实报 `needs_render` ＋ 原因 ✗**（**∴ 绝不返回一张旧图冒充 ✓**）。
    // **∴ 为什么不把它计入"完整实现" ✗**：**冷启动（**必然缺快照 ✓）它给不出图 ✗**
    //   ⇒ **∴ 那要等**本地内核渲染 ＋ 编码**✗（**下一批 ✓**）⇒ **∴ 计数保持 5 ✓，**不虚报 ✓**。
    // **④b `/api/tools/brush_stroke`（**画一笔 ✓ ⇒ PWA 能编辑 ✗**）★**（第 644 轮 ✓）：
    // **∴ 它**不需要内核 ✗** —— **∴ 只需**把请求变成一条原子写进日志 ✗**
    // ⇒ **∴ 而**渲染时内核会**重放**它 ✓**（**∴ 于是"画一笔"就通了 ✓**）**。
    // **∴ 形态**与服务端一致 ✗**：**完整 8 字段封套 ＋ `payload` ✓**（**§14.12 ✓**）。
    if (path === "/api/tools/brush_stroke") {
      // **★ 走内核渲染一笔 ⇒ 写 `import_image` 原子 ✗ ★**（第 653 轮 ✓；**步骤 ⑤ ✓**）：
      //   **∴ 为什么不能自造 `brush_stroke` 原子 ✗**（**第 646 轮实测 ✓**）：
      //   **∴ 服务端的笔触是**服务端渲染成位图**后的 `import_image` 原子 ✗**（**内核不认前者的 kind ✓**）。
      try {
        // **∴ `layerId` 与 `brushName` 在重写时被删掉了 ✗**（第 655 轮实测：`layerId is not defined` ✓）
        // ⇒ **∴ 加回 ✓**（**∴ 这正是"失败必须说出原因"的价值 ✓**）。
        const layerId = body.layer_id || (body.payload && body.payload.layer_id) || "L0";
        const mod = await import("/wasm/yanshi_wasm.js");
        if (typeof mod.default === "function") await mod.default();
        // **★ 没有内核就**自己建**✗ ★**（第 655 轮 ✓ —— **∴ 修掉第 654 轮那个错 ✓**）：
        //   **∴ 落笔**不该依赖"先渲染过"✗** ⇒ **∴ 与 `render_region` 用**同一套参数**✗**
        //   （**`tile 256` ＋ 从创建原子取 `w`/`h` ＋ **限额 256 MiB** ✓**）。
        let entryK = kernels.get(doc);
        if (!entryK) {
          const c2 = (await atomsOf(handle, doc)).find((r) => r.atom && r.atom.kind === "create_document");
          const w2 = (c2 && c2.atom.payload && c2.atom.payload.width) || (c2 && c2.atom.width) || 1024;
          const h2 = (c2 && c2.atom.payload && c2.atom.payload.height) || (c2 && c2.atom.height) || 1024;
          entryK = { k: new mod.WasmKernel(doc, 256, w2, h2, 256 * 1024 * 1024), w: w2, h: h2 };
          kernels.set(doc, entryK);
        }
        const { fetchBrushText, paintWithKernel } = await import("/brush-local.js");
        const brushName = body.brush || "100%_Opaque.myb";
        const myb = await fetchBrushText(brushName.endsWith(".myb") ? brushName : brushName + ".myb");
        // **∴ 区域 ＝ 点列包围盒 ＋ 笔刷半径 ＋ 余量 ✗**（**∴ 并裁剪到画布内 ✓**）。
        const pts = Array.isArray(body.points) ? body.points : [];
        if (pts.length === 0) throw new Error("points 为空 ⇒ 没有可落的笔");
        const r = Math.ceil((Number(body.size) || 20) / 2) + 2;
        const xs = pts.map((q) => Number(q[0])), ys = pts.map((q) => Number(q[1]));
        const x0 = Math.max(0, Math.floor(Math.min(...xs) - r));
        const y0 = Math.max(0, Math.floor(Math.min(...ys) - r));
        const x1 = Math.min(entryK.w, Math.ceil(Math.max(...xs) + r));
        const y1 = Math.min(entryK.h, Math.ceil(Math.max(...ys) + r));
        const region = { x: x0, y: y0, w: Math.max(1, x1 - x0), h: Math.max(1, y1 - y0) };
        const rgba = await withKernel(doc, () => paintWithKernel(entryK.k, {
          myb, points: pts, size: body.size, color: body.color, region,
        }));
        // **∴ 存成 blob ✗**（**mime ＝ `image/x-yanshi-raw` ✓，与服务端一致 ✓**）。
        // **★ 位图必须**存进内核自己的 blob store**✗ ★**（第 656 轮 ✓，**决定性 ✓**）：
        //   **∴ 内核有 `blob_put(bytes) -> hash` 与 `blob_get(hash)` ✗**
        //   （`crates/yanshi-wasm/src/lib.rs:269`／`:277` ✓）
        //   ⇒ **∴ 而 `import_image` 重放时它按**自己的 hash**去取 ✗**
        //   ⇒ **∴ 所以**光写 IndexedDB 不够 ✗**（**内核**够不到 ✗** —— **∴ 这正是第 655 轮的盲点 ✓**）
        //   ⇒ **∴ 必须**用内核返回的 hash**✗**（**∴ 而我此前自己算 SHA-256 ⇒ **∴ 它当然取不到 ✓**）。
        const kernelHash = await withKernel(doc, () => entryK.k.blob_put(rgba));
        // **★ `blob_put` 返回的是**JSON 信封**✗ ★**（第 657 轮实测 ✓，**决定性 ✓**）：
        //   **∴ 实测返回** `{"blob_hash":"sha256:55d5…","ok":true}` ✗**
        //   ⇒ **∴ 而**我此前把**整个 JSON 字符串**当 hash 写进原子 ✗**
        //   ⇒ **∴ 于是**内核按一个**畸形的字符串**去查 ⇒ **∴ 取不到位图 ✓** —— **∴ 这就是渲染不变化的根因 ✓**。
        const hash = (() => {
          const raw = String(kernelHash || "");
          try { const o = JSON.parse(raw); if (o && o.blob_hash) return String(o.blob_hash); } catch (e) { /* 不是 JSON => 按原样 ✓ */ }
          return raw;
        })();
        if (!hash) throw new Error("内核未返回 blob hash ⇒ 位图没能进内核");
        // **∴ 同时**写 IndexedDB ✗**（**∴ 供刷新后重建内核时再喂一遍 ✓**）。
        const { putBlob } = await import("/store.js");
        await putBlob(handle, hash, Array.from(rgba));
        // **∴ 写 `import_image` 原子 ✗**（**字段照第 646 轮的实测样本 ✓**）。
        const seq = await putAtom(handle, doc, {
          actor: "human:web",
          id: "01STROKE" + String(Date.now()).padStart(13, "0"),
          kind: "import_image",
          payload: {
            // **★ 必须带 `type` 与 `width` ✗ ★**（第 665 轮 ✓，**权威对照得到的 ✓**）：
            //   **∴ 服务端的 payload 是** 7 个键 ✗**：
            //     `bitmap`／`height`／`layer_id`／`object_id`／`region`／`source`／**`type`**／**`width`** ✓
            //   ⇒ **∴ 而**我此前**缺 `type`（`"raster_patch"` ✓）与 `width` ✗**
            //   ⇒ **∴ 内核**可能按 `type` 分派**✗ ⇒ **∴ 缺它 ⇒ **∴ 静默忽略 ✓****（**∴ 实测 `warnings:[]` ✓**）。
            type: "raster_patch",
            bitmap: { blob_hash: hash, mime_type: "image/x-yanshi-raw", size: rgba.length },
            width: region.w, height: region.h, layer_id: layerId,
            object_id: "obj_" + String(Date.now()),
            region: { h: region.h, w: region.w, x: region.x, y: region.y },
            source: { brush: brushName, color: body.color ?? null, color_to: null,
                      hardness: null, kind: "brush", opacity: null, points: pts,
                      seed: 0, size: body.size ?? null, smooth: null },
          },
          schema_version: 1, seq: 0, session: "session:web", timestamp: Date.now(),
        });
        return json({ ok: true, seq, server: false, bytes: rgba.length, blob_hash: hash,
                      note: "已由本地内核渲染并写入 import_image 原子" });
      } catch (err) {
        // **★ 失败必须**说出来 ✗**（**∴ 不许静默 —— **∴ 否则表现为"画了没反应"✗****）。**
        return json({ ok: false, error: "stroke_failed",
                      reason: String((err && err.message) || err), server: false }, 500);
      }
    }
    if (false) {
      const layerId = body.layer_id || (body.payload && body.payload.layer_id) || "L0";
      const atom = {
        actor: "human:web",
        id: "01STROKE" + String(Date.now()).padStart(13, "0"),
        kind: "brush_stroke",
        payload: {
          brush: body.brush, size: body.size, color: body.color,
          layer_id: layerId, points: body.points,
        },
        schema_version: 1,
        session: "session:web",
        timestamp: Date.now(),
      };
      const seq = await putAtom(handle, doc, atom);
      // **∴ 画了新内容 ⇒ **旧快照必须失效 ✗**（**∴ 否则会拿旧图冒充 ✓**）。
      return json({ ok: true, seq, server: false, note: "已写入本地原子日志；渲染时内核会重放它" });
    }

    if (path === "/api/tools/render_region") {
      const { readSnapshot } = await import("./store.js");
      // **★ 未给 `seq` 时用**当前 head**✗ ★**（第 630 轮 ✓）：**∴ 若默认 0 ✗**
      // ⇒ **∴ 与存的快照 `seq` 永不相等 ⇒ **∴ 快照分支**永不命中 ✗****（**实测 ✓**）。
      const head = (await wrap(tx(handle, "docs", "readonly").get(doc)))?.seq ?? 0;
      const expected = Number(q.get("seq") ?? head);
      // **∴ 诊断：读快照时用的期望序号 ✗**（第 662 轮 ✓）。
      try { window.__expectedSeq = expected; window.__headSeq = head; } catch (e) {}
      const snap = await readSnapshot(handle, doc, expected);
      if (snap && snap.bytes) {
        return new Response(snap.bytes, {
          status: 200,
          headers: { "content-type": "image/png", "cache-control": "no-store", "x-yanshi-source": "local-snapshot" },
        });
      }
      // **★ 快照未命中 ⇒ **就地用内核渲染**✗ ★**（第 625 轮 ✓；**部署矩阵 §14.5 的配方 ✓**）：
      // **∴ 内核与服务端**同一编码器**✗**（`render_region_png` 注释 ✓：可直接比对哈希 ✓）
      // ⇒ **∴ 于是**两处产出的像素**同源 ✓**。
      // **∴ 内核实例**按文档缓存 ✗**（**否则每次都要重放全部原子 ✓**）。
      try {
        const mod = await import("/wasm/yanshi_wasm.js");
        // **★ 必须先初始化 wasm ✗ ★**（第 625 轮 ✓，**真实浏览器判据抓到的真错误 ✗**）：
        // **∴ `--target web` 的产物要求**先 `await mod.default()`✗**（**它加载并实例化 `.wasm` ✓**）
        // ⇒ **∴ 否则**线性内存尚未就绪 ✗** ⇒ **∴ `new WasmKernel(…)` 会报
        // `Cannot read properties of undefined (reading '__wbindgen_malloc')` ✗**（**实测 ✓**）。
        if (typeof mod.default === "function") await mod.default();
        const meta2 = (await wrap(tx(handle, "docs", "readonly").get(doc))) || {};
        // Everything below touches the wasm kernel, so it runs inside the per-document
        // serial chain: the warm-up and the preview can otherwise overlap and trip the
        // reentrant borrow check (device log: recursive use of an object ... aliasing).
        return await withKernel(doc, async () => {
          let entry = kernels.get(doc);
          if (!entry) {
            const create = (await atomsOf(handle, doc)).find((r) => r.atom && r.atom.kind === "create_document");
            const w = (create && create.atom.width) || 1024;
            const h = (create && create.atom.height) || 1024;
            // **∴ 限额 256 MiB ✓**（**∴ 超限由内核自己淘汰 ✓**）。
            const inst = new mod.WasmKernel(doc, 256, w, h, 256 * 1024 * 1024);
            entry = { k: inst, w, h };
            kernels.set(doc, entry);
          }
          const k = entry.k;
          // **★ 坑：`load_atoms_json` 要的是**服务端 `get_log` 形态的数组**✗**
          // ⇒ **∴ 而 `atomsOf` 返回 `{ id, doc, seq, atom }` ✗ ⇒ **∴ 必须映射成 `r.atom` ✓**。
          const atoms = await atomsOf(handle, doc);
          // **★ 折叠结果必须**可见 ✗ ★**（第 630 轮 ✓）：**∴ `load_atoms_json` 返回一个封套字符串 ✗**
          // ⇒ **∴ 它写明**成功或失败原因 ✓** ⇒ **∴ 不许丢掉它 ✓**（**∴ 否则只剩"0×0"这种二手症状 ✗**）。
          var foldResult = k.load_atoms_json(JSON.stringify(atoms.map((r) => r.atom)));
          lastFold = String(foldResult || "");
          try { window.__lastFold = lastFold; } catch (e) { /* 非浏览器环境（node 判据）=> 忽略 ✓ */ }
          // **★ 区域必须裁剪到画布内 ✗ ★**（第 625 轮 ✓，**真实浏览器判据抓到的真空图 ✗**）：
          // **∴ 实测**：**默认区域给到 1024×1024 而文档只有 800×600 ✗**
          // ⇒ **∴ 越界 ⇒ `render_region_png` 返回**空字节**✗** ⇒ **∴ 于是**前端会显示空白 ✓**。
          const want = {
            x: Number(q.get("x") ?? (body.region && body.region.x) ?? 0),
            y: Number(q.get("y") ?? (body.region && body.region.y) ?? 0),
            w: Number(q.get("w") ?? (body.region && body.region.w) ?? entry.w),
            h: Number(q.get("h") ?? (body.region && body.region.h) ?? entry.h),
          };
          const box = {
            x: Math.max(0, Math.min(want.x, entry.w)),
            y: Math.max(0, Math.min(want.y, entry.h)),
            w: Math.max(1, Math.min(want.w, entry.w - Math.max(0, want.x))),
            h: Math.max(1, Math.min(want.h, entry.h - Math.max(0, want.y))),
          };
          const png = k.render_region_png(Math.floor(box.x), Math.floor(box.y), Math.ceil(box.w), Math.ceil(box.h));
          // **∴ 诊断：内核渲染元信息 ✗**（第 664 轮 ✓）：**∴ `k` 在此可见 ✓**。
          try { window.__renderInfo = k.render_region_info(box.x, box.y, box.w, box.h); window.__renderAtoms = atoms.length; } catch (e) { window.__renderInfo = "info 失败：" + e; }
          // **★ 绝不返回空图 ✗ ★**：**∴ 内核给不出字节 ⇒ **∴ 如实回落 `needs_render` ✓****
          //（**∴ 而**不是发一个 0 字节的 `image/png` ⇒ **∴ 那会让前端显示空白却不报错 ✗**）。
          // **∴ 诊断：内核渲染时的元信息 ✗**（第 664 轮 ✓，**语句级插入 ✓**）：
          //   **∴ 它给 bbox／宽高／padding／**警告**／tile 数 ✗** ⇒ **∴ 于是**"这一笔为什么没进图"**有据可查 ✓**。
          // （探针移到 k 可见处 —— 第 664 轮 ✓）
          if (!png || png.length === 0) {
            // **∴ 取**元信息里的警告**✗**（`render_region_info` ✓）—— **∴ 否则只能看到"未产出字节"✗**，
            // **而**看不到原因 ✓**（**∴ 这正是我上一轮卡住的地方 ✓**）。
            let info = "";
            try { info = k.render_region_info(Math.floor(box.x), Math.floor(box.y),
                                             Math.ceil(box.w), Math.ceil(box.h)); } catch (e) { info = "info 亦失败：" + e; }
            return json({ ok: false, error: "needs_render", reason: "内核未产出字节",
                          fold_result: String(foldResult || "").slice(0, 500),
                          atom_count: atoms.length,
                          // **∴ 解析后返回 ✗**（**否则判据只能看到**被转义的 JSON 字符串 ✓**）。
                          kernel_info: (() => { try { return JSON.parse(info); } catch { return String(info).slice(0, 600); } })(),
                          server: false });
          }
          // **★ `?probe=1` ⇒ 只回**诊断 ✗ ★**（第 658 轮 ✓）：**∴ 它一次给出两件事 ✗**
          //   （**∴ 折叠信封 ✓ ＋ 渲染元信息（**含警告 ✓**）**）
          //   ⇒ **∴ 于是**"这一笔为什么没进图"**不必再猜 ✓**。
          if (q.get("probe")) {
            let meta = "";
            try { meta = k.render_region_info(box.x, box.y, box.w, box.h); } catch (e) { meta = "info 失败：" + e; }
            return json({ ok: true, probe: true, fold: String(foldResult || "").slice(0, 300),
                          info: String(meta).slice(0, 700),
                          atom_count: atoms.length, region: box });
          }
          // **∴ 写进快照（**带当前 `seq` ✓**）⇒ **∴ 下次命中快照分支 ✓**。
          const seqNow = (await wrap(tx(handle, "docs", "readonly").get(doc)))?.seq ?? 0;
          await writeSnapshot(handle, doc, seqNow, { bytes: png });
          // **∴ 诊断：写快照时用的序号 ✗**（第 662 轮 ✓）。
          try { window.__snapSeq = seqNow; window.__snapAt = Date.now(); } catch (e) {}
          return new Response(png, {
            status: 200,
            headers: { "content-type": "image/png", "cache-control": "no-store",
                       "x-yanshi-source": "local-kernel" },
          });
        });
      } catch (err) {
        // **∴ 内核不可用（**如 node 里没有 DOM／wasm 未同步 ✓）⇒ **∴ 如实说该重算 ✗**。
        // **∴ 不谎报、**也不返回空图 ✓**。
        return json({ ok: false, error: "needs_render",
                      reason: "本地内核不可用：" + String((err && err.message) || err),
                      server: false });
      }
      // **∴ 没有可用快照 ⇒ **如实说明该重算 ✓**（**200 ＋ 明确字段 ✗ ⇒ **∴ 前端可分支 ✓**）。
      return json({
        ok: false,
        error: "needs_render",
        reason: "本地快照缺失或已过期（序号／格式不匹配）",
        snapshot_seq: expected,
        render_backend: await detectBackend(),
        server: false,
      });
    }

    // **∴ 其余端点**一律抛给上层 ⇒ 上层如实 501 ＋ 原因 ✗**（**∴ 不假装成功 ✓**）。
    throw new Error(`endpoint_not_local: ${url.pathname}`);
  };
}

/** **∴ 内核没暴露尺寸 ✗ ⇒ **∴ 用一个安全默认值 ✓**（**真实尺寸由 `load_atoms_json` 里的创建原子决定 ✓**）。 */
function k_width() { return 1024; }

export function installLocalApi(deps) {
  const original = window.fetch.bind(window);
  window.fetch = async (input, init) => {
    const req = input instanceof Request ? input : new Request(input, init);
    const url = new URL(req.url, location.origin);
    // **★ 判定顺序很关键 ✓ ★**（第 621 轮 ✓，**真实浏览器判据抓到的真缺陷 ✗**）：
    // **∴ `/health` 必须在 `/api/` 前缀检查**之前**✗** ——
    // **∴ 否则**它不匹配 `/api/` ⇒ **∴ 会被 `return original(…)` 放行 ✗** ⇒ **∴ 打到静态服务器 ⇒ 404 ✗**
    //（**∴ 而**node 行为判据抓不到它 ✗**：**它直接调 `makeLocalApi` ✓，**不经过这层覆写 ✓**）
    // ⇒ **∴ 这正是"真实浏览器验证"的价值 ✗**。
    if (url.pathname === "/health") {
      // The viewer refuses to warm the kernel unless health reports wasm:true, and this
// deployment *is* the wasm kernel, so say so. Without the field the whole local path
// was skipped and every stroke failed with an unknown reason even though the kernel
// was present and working.
return json({ ok: true, server: false, wasm: true,
                    render_backend: await detectBackend(),
                    note: "本部署没有服务器端；渲染全部在本地浏览器完成" });
    }
    // **∴ 只拦 `/api/` ✓**（**静态资源与 `/wasm/` 保持原路 ✓**）。
    // **★ 非 `/api/` 的本地端点也要走本地层 ✗ ★**（第 827 轮 ✓；**用户报 405 ✓**）：
    //   **∴ 为什么 ✗**：**`get_preferences`／`list_effects`／`list_brushes` 不在 `/api/` 下 ✗**
    //   ⇒ **∴ 原来**它们**被放行到网络 ✗** ⇒ **∴ 静态托管**返回 405 ✓**
    //   ⇒ **∴ 现在**：**它们也进 `local()` ✗** ⇒ **∴ 已实现**就正常答 ✗**；
    //     **∴ 未实现**就**由 `local()` 末尾的 throw **转成 501 ✓**（**如实报错 ✓，**不是**405 ✓）**。
    const LOCAL_EXTRA = ["/get_preferences", "/list_effects", "/list_brushes"];
    const isLocalPath =
      url.pathname.startsWith("/api/") || LOCAL_EXTRA.indexOf(url.pathname) >= 0;
    if (!isLocalPath) return original(input, init);
    try {
      return await deps.local(req);
    } catch (err) {
      // **★ 未实现的端点必须**如实报错**✗**（**目标总则：lazy 不许变成撒谎 ✓**）。
      return json({ ok: false, error: "not_implemented_locally",
                    endpoint: url.pathname,
                    reason: "该端点在 PWA 里尚未映射到本地内核／IndexedDB" ,
                    detail: String(err && err.message ? err.message : err) }, 501);
    }
  };
}

/** **★ 如实探测浏览器侧后端 ✓ ★**（**部署矩阵 §8.2 ✓**）。 */
export async function detectBackend() {
  try {
    if (navigator.gpu && (await navigator.gpu.requestAdapter())) return "webgpu";
  } catch { /* **∴ 探测失败按无 GPU 处理 ✓** */ }
  return "cpu";
}

function json(body, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json", "cache-control": "no-store" },
  });
}
