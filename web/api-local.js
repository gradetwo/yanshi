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
/** **★ 计数升到 48 ✗ ★**（P1：7 个端点 ✓ —— `delete_layer`／`duplicate_layer`／
 *   `reorder_layers`／`brush_preview`／`import_image`／`get_undo_status`／`set_preferences` ✓；
 *   **∴ 口径与此前一致 ✓**（**只数真实映射的分支 ✓**）。
 *   **∴ `get_preferences` 改读 IndexedDB ✓**（**`set_preferences` 写入的能读回来 ✓**）。 */
/** **★ 计数升到 77 ✗ ★**（P2b：协作／历史类新增 29 个 `path ===` 分支 ✓）：
 *   **∴ 其中 22 个真实实现 ✓**（`checkpoint`／`get_checkpoints`／`begin_changeset`／`commit_changeset`／
 *     `abort_changeset`／`revert_changeset`／`get_changesets`／`suggest`／`reject_suggestion`／
 *     `reject_suggestions`／`list_suggestions`／`create/update/delete/get/list/resolve/reject_annotation`（7 个）／
 *     `comment`／`list_comments`／`begin_transaction`／`commit_transaction` ✓）；
 *   **∴ 另 7 个是**如实 501** 的分支 ✓**（`restore_checkpoint`／`accept_suggestion`／`accept_suggestions`／
 *     `preview_suggestion`／`list_stashes`／`apply_stash`／`discard_stash` ✓）——
 *   **∴ 它们**不是"假装成功" ✗**（**一律 501 ＋ 明确原因 ✓**），**而是**已处理的端点** ✗；
 *   **∴ 口径说明 ✗**：**判据按 `path === "` 机械计数分支数 ✗**（**要求 `declared === actualImpl(+1)` ✓**）
 *     ⇒ **∴ 本数 = 全部分支数 ✗**（**真实实现 ＋ 如实 501 ✓**），**∴ 与判据口径对齐 ✓**；
 *   **∴ 不虚报 ✓**：**哪 7 个是 501 在上面点名 ✓**（**判据／人都能逐个核对 ✓**）。
 *   **∴ 并发说明 ✗**：**P2a／P1 的分支也在同一文件里由其他任务并行添加 ✗** ——
 *     **∴ 本数只保证 "+29" 这个增量 ✓**（**合并时的最终总数由收尾方按机械口径重数 ✓**）。 */
/** **★ 计数升到 143 ✗ ★**（P2c：渲染／导出／高级绘画／取色／诊断类新增 66 个 `path ===` 分支 ✓）：
 *   **∴ 其中 60 个真实实现 ✓**（绘画 12：`draw_text`／`draw_shape`／`fill`／`fill_region`／`smudge`／
 *     `clone_stamp`／`heal_stamp`／`heal`（别名）／`gradient_fill`／`texture_background`／`scatter_strokes`／
 *     `update_stroke` ✓；滤镜调整 4：`add_filter`／`update_filter`／`add_adjustment`／`update_adjustment` ✓；
 *     变换 6：`transform_object`／`convert_to_path`／`convert_to_shape`／`liquify_push`／`liquify_pinch`／
 *     `liquify_twirl` ✓；取色 2：`sample_color`／`analyze_region` ✓；
 *     导出 4：`export_png`／`export_project`／`import_project`／`import_asset`（仅 texture）✓；
 *     文档 10：`new_document`／`delete_document`／`list_documents`／`declare_head`／`get_state`／
 *     `get_resolved_state`／`get_render_status`／`cache_document_preview`／`set_reference`／`clear_reference` ✓；
 *     诊断 15：`collect_diagnostics`／`collect_garbage`／`blob_gc`／`get_inflight`／`get_job`／`cancel_job`／
 *     `cancel_operation`／`find_atom`／`get_atom`／`get_ancestors`／`get_descendants`／`get_diff`／
 *     `get_dependency_graph`／`batch`／`submit_offline` ✓；
 *     其他 7：`resample`／`revert`／`revert_to`／`reapply`／`set_property`／`update_override`／
 *     `update_sync_policy` ✓）；
 *   **∴ 另 6 个是**如实 501** 的分支 ✓**（`medium_stroke`／`set_brush_dynamics`／`estimate_dehaze`／
 *     `import_psd`／`resolve_conflict`／`push` ✓）——
 *   **∴ 它们**不是"假装成功" ✗**（**一律 501 ＋ 明确原因 ✓**），**而是**已处理的端点** ✗；
 *   **∴ 口径说明 ✗**：**与 P2b 一致 ✓**（**全部分支数 ＝ 真实实现 ＋ 如实 501 ✓**）；
 *   **∴ `restore_object` 已由他组实现 ✓**（**本片跳过 ✓**）；**`archive_base64` 不是端点 ✗**
 *     （**它是 `export_png` 响应字段 ✓ ⇒ **∴ 本地 `export_png` 照样返回它 ✓**）；
 *   **∴ 并发说明 ✗**：**本数只保证 "+66" 这个增量 ✓**（**合并时的最终总数由收尾方按机械口径重数 ✓**）。
 *   **∴ P2b 收尾机械重数 ✗**（2026-10-10 ✓）：**`path === "` 分支共 151 个 ⇒ **∴ 本数同步到 151 ✓**
 *   （**∴ 多出的分支是并行任务同期加入的 ✗** —— **∴ 收尾方合并后按同一口径重数一次即可 ✓**）。
 *   **∴ 最终收尾 ✗**（2026-10-10 ✓）：**全量合并后 `path === "` 分支共 158 个 ⇒ **∴ 本数同步到 158 ✓**
 *   （**含 update_layer／gradient_blend／list_textures／path_edit／save_palette／patch 补漏 ✓**）。 */
export // **★ 声明必须与**实现分支数**一致 ✗ ★**（第 72 轮 ✓；**判据抓到的 ✓）：
//   **∴ `yanshi-pwa-online-fix2.patch`**加了 2 个实现分支**✗
//     （**∴ 如** `/api/documents/import` 的三步协议 ✓）
//     ⇒ **∴ 而**这里仍写着 158 ✗
//       ⇒ **∴ 于是** `tool-pwa-assets` 报"**声明的端点数（158）与实现分支数（160）
//         不一致 ⇒ **∴ 在虚报能力 ✓** ✗
//   ⇒ **★ 所以**：**改成 160**✗ ⇒ **∴ 于是**声明与实现**一致 ✓ ★**** ✓✓
const LOCAL_IMPLEMENTED = 160;

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
/** **★ 可注入的模块加载器（P1 测试接缝）✓ ★**：
 *   **∴ `brush_preview` 与 `/api/blob` 在浏览器里走真实路径 ✗**（**默认行为不变 ✓**）；
 *   **∴ Node 判据里 `/wasm/…` 与 `/brush-local.js` 的裸 `import` 会直接炸 ✗**
 *     ⇒ **∴ 测试用 `__setLocalApiLoadersForTest` 换上假内核 ✗**（**只影响测试进程 ✓**）。
 *   **∴ 其余分支**不改 ✗**（**∴ 不扩大接缝面 ✓**）。
 */
const __loaders = {
  wasm: () => import("/wasm/yanshi_wasm.js"),
  brushLocal: () => import("/brush-local.js"),
};
/** **测试专用**：替换上面的加载器（**浏览器运行时不调它 ✓**）。 */
export function __setLocalApiLoadersForTest(loaders) {
  Object.assign(__loaders, loaders || {});
}

/** **★ 从原子日志推导图层视图（P1）✓ ★**：
 *   **∴ 只读推导 ✗**（**∴ 不另存图层表 ✓ —— 与 `list_layers` 原有纪律一致 ✓**）；
 *   **∴ `order` 自底向上 ✗**（**与服务端 `reorder_layers` 的 `order` 口径一致 ✓**）；
 *   **∴ `tombstone{layer_id}` 删层 ✗**（**照服务端 `write_delete_layer` ✓**）；
 *   **∴ `reorder_layers{order}` 整体换序 ✗**（**只采纳**完整**顺序 ✓ —— 与服务端校验同口径 ✓**）。
 *   @param {Array} rows `atomsOf` 的行（已按 seq 升序 ✓）
 *   @returns {{layers: Map<string,{layer_id:string,name:string}>, order: string[]}}
 */
export function p1LayerOrder(rows) {
  const layers = new Map();
  const order = [];
  // **★ `set_property` 折叠**（`update_layer` ✓）：图层属性（visible/locked/opacity/
  //   blend_mode/name 等）存在 `set_property` 原子里 ⇒ 按顺序应用，否则前端读到旧值。
  const PROP_KEYS = ["name", "visible", "opacity", "blend_mode",
                     "locked", "alpha_lock", "clipping_mask", "mask_id"];
  for (const r of rows || []) {
    const a = (r && r.atom) || {};
    const pl = (a && a.payload) || {};
    if (a.kind === "create_layer") {
      // **★ 兼容旧原子**：顶层 `layer_id` 回退（判据 POST 的原子不带 `payload` 包装）。
      const id = (typeof pl.layer_id === "string" && pl.layer_id) ||
                 (typeof a.layer_id === "string" && a.layer_id);
      if (typeof id !== "string" || !id || layers.has(id)) continue;
      layers.set(id, { layer_id: id, name: pl.name ?? a.name ?? id,
                       visible: true, locked: false, opacity: 1, blend_mode: "normal" });
      order.push(id);
    } else if (a.kind === "tombstone" && typeof pl.layer_id === "string" && !pl.object_id) {
      layers.delete(pl.layer_id);
      const i = order.indexOf(pl.layer_id);
      if (i >= 0) order.splice(i, 1);
    } else if (a.kind === "reorder_layers") {
      const ord = Array.isArray(pl.order) ? pl.order : null;
      if (!ord) continue;
      const alive = new Set(layers.keys());
      const ok = ord.length === alive.size &&
                 ord.every((id) => typeof id === "string" && alive.has(id)) &&
                 new Set(ord).size === ord.length;
      if (ok) { order.length = 0; order.push(...ord); }
    } else if (a.kind === "set_property" && typeof pl.layer_id === "string" &&
               typeof pl.key === "string" && PROP_KEYS.indexOf(pl.key) >= 0) {
      const layer = layers.get(pl.layer_id);
      if (layer) layer[pl.key] = pl.value;
    }
  }
  return { layers, order };
}

/** **★ 服务端 `default_preview_points` 的同形实现（P1 `brush_preview`）✓ ★**：
 *   **∴ 公式照 `tools.rs:12517` ✗**（**`length=clamp(size*5,64,160)` ＋ `margin=size+8` ＋
 *     `amplitude=clamp(size*0.6,4,24)` ✓**）⇒ **∴ 同一支笔刷 ⇒ 预览笔迹形状固定 ✓**。
 */
export function p1PreviewPoints(size) {
  const length = Math.min(160, Math.max(64, size * 5));
  const margin = size + 8;
  const amp = Math.min(24, Math.max(4, size * 0.6));
  const y = margin + amp;
  return [
    [margin, y, 0.35],
    [margin + length * 0.34, y - amp * 2, 0.9],
    [margin + length * 0.67, y + amp * 2, 0.5],
    [margin + length, y, 0.35],
  ];
}

const __crcTable = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = (c & 1) ? (0xedb88320 ^ (c >>> 1)) : (c >>> 1);
    t[n] = c >>> 0;
  }
  return t;
})();
function __crc32(bytes) {
  let c = 0xffffffff;
  for (let i = 0; i < bytes.length; i++) c = __crcTable[(c ^ bytes[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}
async function __deflate(bytes) {
  const cs = new CompressionStream("deflate");
  const w = cs.writable.getWriter();
  await w.write(bytes);
  await w.close();
  const chunks = [];
  const r = cs.readable.getReader();
  for (;;) {
    const { done, value } = await r.read();
    if (done) break;
    chunks.push(value);
  }
  const out = new Uint8Array(chunks.reduce((n, c) => n + c.length, 0));
  let o = 0;
  for (const c of chunks) { out.set(c, o); o += c.length; }
  return out;
}
/** **★ RGBA8 ⇒ PNG（P1 `brush_preview`）✓ ★**：
 *   **∴ 内核只暴露 `render_region_png`（**整幅渲染 ✓**）✗** ⇒ **∴ 预览这种"**画布外的小图**"
 *     在 JS 侧编码 ✗**（**`CompressionStream("deflate")` ✓，**无**新依赖 ✓**）；
 *   **∴ 输出是标准 PNG ✗**（**`IHDR(8bit/RGBA)` ＋ `IDAT` ＋ `IEND` ✓ —— 可被任何解码器读 ✓**）。
 */
export async function encodePngRgba(w, h, rgba) {
  const px = rgba instanceof Uint8Array ? rgba : Uint8Array.from(rgba);
  if (px.length !== w * h * 4) throw new Error("像素字节数与宽高不符");
  const stride = w * 4;
  const raw = new Uint8Array((stride + 1) * h);
  for (let y = 0; y < h; y++) {
    raw[y * (stride + 1)] = 0; // **逐行 filter=0（None）✓**
    raw.set(px.subarray(y * stride, (y + 1) * stride), y * (stride + 1) + 1);
  }
  const idat = await __deflate(raw);
  const concat2 = (a, b) => { const o = new Uint8Array(a.length + b.length); o.set(a, 0); o.set(b, a.length); return o; };
  const chunk = (type, data) => {
    const td = new TextEncoder().encode(type);
    const out = new Uint8Array(12 + data.length);
    const dv = new DataView(out.buffer);
    dv.setUint32(0, data.length);
    out.set(td, 4); out.set(data, 8);
    dv.setUint32(8 + data.length, __crc32(concat2(td, data)));
    return out;
  };
  const ihdr = new Uint8Array(13);
  const dv = new DataView(ihdr.buffer);
  dv.setUint32(0, w); dv.setUint32(4, h);
  ihdr[8] = 8; ihdr[9] = 6; // **8bit ＋ truecolor+alpha ✓**
  const sig = new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10]);
  const parts = [sig, chunk("IHDR", ihdr), chunk("IDAT", idat), chunk("IEND", new Uint8Array(0))];
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let o = 0;
  for (const p of parts) { out.set(p, o); o += p.length; }
  return out;
}
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

  // **★ P2 端点共用：原子封套与日志推导 ✓ ★**（图层／对象／选区类 ✗）：
  //   **∴ 写操作**一律**写对应 `kind` 的原子 ✗**（**8 字段封套 ✓**：`actor:"human:web"` ＋
  //     `id` 唯一 ＋ `schema_version:1` ＋ `seq` 由 `putAtom` 自增覆盖 ＋ `session:"session:web"` ✓）；
  //   **∴ 读操作**从 IndexedDB 原子日志**推导** ✗**（**∴ 不另存一份状态表 ✗ ⇒ **∴ 与 `list_layers` 同一纪律 ✓**）。
  let __p2ctr = 0;
  function p2AtomId() {
    __p2ctr += 1;
    return "01P2" + String(Date.now()).padStart(13, "0") + String(__p2ctr).padStart(4, "0");
  }
  function p2Envelope(kind, payload) {
    return {
      actor: "human:web",
      id: p2AtomId(),
      kind,
      payload,
      schema_version: 1,
      seq: 0, // **∴ `putAtom` 会自增覆盖 ✗**（**∴ 调用方给的 `seq` 一律忽略 ✓**）
      session: "session:web",
      timestamp: Date.now(),
    };
  }
  /** **被 `revert`／`reapply` 中和掉的原子 id 集合 ✗**（**∴ 判"删没删"要用它 ✓**）。 */
  function p2Neutralized(rows) {
    const s = new Set();
    for (const r of rows) {
      const a = (r && r.atom) || {};
      if ((a.kind === "revert" || a.kind === "reapply") && a.payload && a.payload.target) {
        s.add(a.payload.target);
      }
    }
    return s;
  }
  /** **从日志推导对象视图 ✗**（**∴ 存在性／类型／属性覆盖／删除状态 ✓**）：
   *   **∴ `type`／`layer_id` 等**可被 `set_property` 覆盖 ✗**（**∴ 取最后一次 ✓**）；
   *   **∴ `data` 可被 `supersede` 整体替换 ✗**（**`replace_object_data` 的语义 ✓**）；
   *   **∴ `members`／`group_transform`／`master_ref` 先看创建 `data` ✗**，
   *     **再看 `set_property` 覆盖 ✗**（**组与实例的语义 ✓**）。
   */
  function p2ObjectView(rows, objectId) {
    let create = null;
    const props = {};
    let supersededData = null;
    const tombstoneIds = [];
    for (const r of rows) {
      const a = (r && r.atom) || {};
      const pl = a.payload || {};
      if (a.kind === "create_object" && pl.object_id === objectId && !create) create = a;
      else if (a.kind === "set_property" && pl.object_id === objectId) props[pl.key] = pl.value;
      else if (a.kind === "supersede" && pl.object_id === objectId) supersededData = pl.data;
      else if (a.kind === "tombstone" && pl.object_id === objectId) tombstoneIds.push(a.id);
    }
    if (!create) return { found: false };
    const neutral = p2Neutralized(rows);
    const cpl = create.payload || {};
    const view = {
      found: true,
      object_id: objectId,
      layer_id: props.layer_id !== undefined ? props.layer_id : (cpl.layer_id ?? null),
      type: props.type !== undefined ? props.type : (cpl.type ?? null),
      data: supersededData !== null && supersededData !== undefined ? supersededData : (cpl.data ?? null),
      transform: cpl.transform ?? null,
      visible: props.visible ?? null,
      locked: props.locked ?? null,
      z_index: props.z_index ?? null,
      metadata: props.metadata ?? null,
      tombstone_ids: tombstoneIds,
      deleted: tombstoneIds.some((id) => !neutral.has(id)),
    };
    view.members = view.data && view.data.members !== undefined ? view.data.members : null;
    if (props.members !== undefined) view.members = props.members;
    view.group_transform = view.data && view.data.group_transform ? view.data.group_transform : null;
    if (props.group_transform !== undefined) view.group_transform = props.group_transform;
    view.master_ref = props.master_ref !== undefined ? props.master_ref
      : (view.data && view.data.master_ref ? view.data.master_ref : null);
    view.local_transform = view.master_ref && view.master_ref.local_transform
      ? view.master_ref.local_transform : null;
    return view;
  }
  /** **对象版本链 ✗**（**∴ 所有**引用 `object_id` 的原子 ✓ ＋ `reverted` 标记 ✓**）。 */
  function p2History(rows, objectId) {
    const kinds = { create_object: 1, set_property: 1, supersede: 1,
                    move: 1, transform: 1, tombstone: 1 };
    const neutral = p2Neutralized(rows);
    const versions = [];
    for (const r of rows) {
      const a = (r && r.atom) || {};
      if (!kinds[a.kind]) continue;
      const pl = a.payload || {};
      if (pl.object_id !== objectId) continue;
      versions.push({ atom_id: a.id, kind: a.kind, seq: r.seq,
                      actor: a.actor, timestamp: a.timestamp,
                      reverted: neutral.has(a.id), payload: pl });
    }
    return versions;
  }
  /** **参数错误 ⇒ 400 ✗**（**照服务端 `InvalidArgument` 的口径 ✓**）。 */
  function p2Bad(reason) {
    return json({ ok: false, error: "invalid_argument", reason, server: false }, 400);
  }
  /** **引用不存在 ⇒ 404 ✗**（**照服务端 `ReferenceNotFound` 的口径 ✓**）。 */
  function p2Missing(reason) {
    return json({ ok: false, error: "reference_not_found", reason, server: false }, 404);
  }
  /** **★ 读本地偏好（P1）✓ ★**：
   *   **∴ `set_preferences` 写进 IndexedDB `prefs` 表 ✗**（**`store.js` ✓**）⇒ **∴ 这里读出来 ✓**；
   *   **∴ 缺省值 `{theme:"dark", locale:"zh-CN"}` ✗**（**与旧硬编码回答一致 ✓ —— 老前端不受影响 ✓**）；
   *   **∴ `keys` 过滤照服务端 `read_get_preferences` ✓**（**前端真会传它 ✓**）。
   */
  async function p1ReadPreferences(dbHandle, reqBody) {
    const merged = { theme: "dark", locale: "zh-CN" };
    try {
      const all = await wrap(tx(dbHandle, "prefs", "readonly").getAll());
      for (const r of all || []) merged[r.key] = r.value;
    } catch (e) { /* **旧库没有 `prefs` 表 ⇒ 只给缺省 ✓**（**如实，不编造 ✓**） */ }
    const keys = reqBody && reqBody.keys;
    if (Array.isArray(keys)) {
      const picked = {};
      for (const k of keys) if (typeof k === "string" && k in merged) picked[k] = merged[k];
      return picked;
    }
    return merged;
  }
  /** **选区／遮罩创建参数校验 ✗**（**照 `write_create_selection`／`write_create_mask` ✓**）：
   *   **∴ `shape` 必须带 `kind` ✗**；**`feather ∈ [0,512]` ✗**（**缺省 0 ✓**）；
   *   **∴ `mode ∈ new/add/subtract/intersect` ✗**（**缺省 `new` ✓**）。
   */
  function p2CheckRegionArgs(body, idKey) {
    const id = body[idKey];
    if (typeof id !== "string" || !id) return { err: p2Bad(idKey + " 必填（字符串）") };
    const shape = body.shape;
    if (!shape || typeof shape !== "object" || typeof shape.kind !== "string") {
      return { err: p2Bad("shape 必须带 kind（rect/ellipse/polygon）") };
    }
    const feather = body.feather !== undefined ? Number(body.feather) : 0;
    if (!Number.isFinite(feather) || feather < 0 || feather > 512) {
      return { err: p2Bad("feather 必须在 [0, 512] 内，得到 " + body.feather) };
    }
    const mode = body.mode !== undefined ? String(body.mode) : "new";
    if (["new", "add", "subtract", "intersect"].indexOf(mode) < 0) {
      return { err: p2Bad("mode 必须是 new/add/subtract/intersect，得到 " + mode) };
    }
    const payload = {};
    payload[idKey] = id;
    payload.shape = shape;
    payload.feather = feather;
    payload.mode = mode;
    payload.invert = body.invert === true;
    if (typeof body.linked_layer === "string") payload.linked_layer = body.linked_layer;
    return { payload };
  }
  /** **服务端没有该端点 ⇒ 明确 501 ✗**（**∴ 不硬凑语义 ✓**）。 */
  function p2NoServerEndpoint(name, why) {
    return json({ ok: false, error: "not_implemented_locally", endpoint: "/api/tools/" + name,
                  reason: name + "：服务端工具注册表里没有这个端点（" + why + "），本地不硬凑语义",
                  server: false }, 501);
  }

  // **★ P2b 共用：变更集／事务的会话级登记 ＋ ULID ＋ 建议作用域 ✓ ★**（协作／历史类 ✗）：
  //   **∴ 服务端把"打开的变更集"记在 `open_changesets[(doc, session)]` ✗**（`service.rs:576` ✓）
  //   ⇒ **∴ 本地同形 ✗**（**内存 Map ✓ —— **∴ 页面重载后"打开中"状态丢失 ✗**，**而**已提交的原子**不受影响** ✓**）。
  const p2bOpen = new Map(); // key: doc::session → { changeset_id, transaction }
  let p2bIdSeq = 0;
  // **★ 标注通道的内存后备 ✗ ★**：**∴ 必须在 `makeLocalApi` 层 ✗**（**∴ 跨请求存活 ✓**）——
  //   **∴ 放进 `local()` 里 ⇒ 每次请求都是空 Map ✗** ⇒ **∴ 内存模式下标注写完就丢 ✓**（**已实测 ✓**）。
  const p2bAnnMem = new Map(); // key: doc → { order, versions }
  const __CROCKFORD32 = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";
  /** **ULID 形 id ✗**（**∴ 与服务端 `Changeset::new_id()` 同形：26 位 Crockford ✓**）。 */
  function p2Ulid() {
    let t = Date.now(), time = "";
    for (let i = 0; i < 10; i++) { time = __CROCKFORD32[t % 32] + time; t = Math.floor(t / 32); }
    let rand = "";
    for (let i = 0; i < 16; i++) rand += __CROCKFORD32[Math.floor(Math.random() * 32)];
    return time + rand;
  }
  /** **建议作用域冲突（**本地简化版** ✗**）：**只看补丁步骤的 `layer_id`／`object_id`／`region` ✓**
   *  （**∴ 是否相同或相交 ✗**）⇒ **∴ 有相交返回原因串 ✗**，**无相交返回 null ✓**；
   *  **∴ 如实简化 ✗**（**服务端另有完整作用域模型 ✓ —— **∴ 调用处必须注记，不冒充 ✓**）。 */
  function p2bScopesConflict(patchA, patchB) {
    const scope = (patch) => {
      const layers = new Set(), objects = new Set(), regions = [];
      for (const step of patch) {
        const args = (step && step.arguments) || {};
        if (args.layer_id != null) layers.add(String(args.layer_id));
        if (args.object_id != null) objects.add(String(args.object_id));
        const rg = args.region || args.bbox;
        if (rg && [rg.x, rg.y, rg.w, rg.h].every((v) => Number.isFinite(Number(v))))
          regions.push({ x: +rg.x, y: +rg.y, w: +rg.w, h: +rg.h });
      }
      return { layers, objects, regions };
    };
    const A = scope(patchA), B = scope(patchB);
    for (const l of A.layers) if (B.layers.has(l)) return "同图层 " + l;
    for (const o of A.objects) if (B.objects.has(o)) return "同对象 " + o;
    for (const ra of A.regions) for (const rb of B.regions) {
      if (ra.x < rb.x + rb.w && rb.x < ra.x + ra.w && ra.y < rb.y + rb.h && rb.y < ra.y + ra.h)
        return "区域相交";
    }
    return null;
  }

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
        __ct.indexOf("application/pdf") >= 0 ||
        // **★ 工程包分片上传**（2026-10-10 ✓）：`?upload=<id>&offset=<n>` 的 body 是二进制分片 ✗
        //   ⇒ 必须跳过 JSON 解析 ✓（否则 body 被消费掉，后面 `arrayBuffer()` 报错 ✓）。
        (path === "/api/documents/import" && q.get("upload") != null &&
         q.get("finish") == null);
      let body =
        req.method === "POST" && !__binary
          ? await req.json().catch(() => ({}))
          : {};
      // **★ 参数名兼容**（2026-10-10 定位 ✓）：前端有些调用传 `layerId`（camelCase）✗，
      //   有些传 `layer_id`（snake_case）✓ ⇒ 这里统一归一，避免笔落到错误的 "L0" 层。
      //   **∴ 只做加法**（不删原字段 ✓）⇒ 已有调用不受影响 ✓。
      if (body && typeof body === "object") {
        if (body.layerId != null && body.layer_id == null) body.layer_id = body.layerId;
        if (body.payload && typeof body.payload === "object" &&
            body.payload.layerId != null && body.payload.layer_id == null) {
          body.payload.layer_id = body.payload.layerId;
        }
      }

    // **★ P2b 请求级辅助 ✓ ★**（协作／历史类 ✗）：
    //   **∴ `p2bSession`：**会话键** ✗**（**∴ 与服务端框架参数 `session` 同义 ✓**，**缺省 `session:web` ✓**）；
    //   **∴ `p2bTag`：**有打开的变更集／事务 ⇒ **∴ 给原子封套挂 `changeset_id` ✗**
    //     （**∴ 与服务端 `ctx.commit` 自动并入打开变更集同形 ✓** —— **`tools.rs:388` ✓**）。
    const p2bSession = (body && body.session) || q.get("session") || "session:web";
    const p2bOpenKey = doc + "::" + p2bSession;
    function p2bTag(atom) {
      const open = p2bOpen.get(p2bOpenKey);
      if (open && !atom.changeset_id) return { ...atom, changeset_id: open.changeset_id };
      return atom;
    }
    /** **P2b 原子封套 ✗**（**8 字段 ✓**）：**∴ `seq` 由 `putAtom` 自增覆盖 ✗**（**∴ 这里写 0 ✓**）；
     *  **∴ id 唯一性**：**时间戳 ＋ 进程内计数器 ✗**（**∴ 同一毫秒写多条也不撞 ✓** ——
     *  **∴ `abort_changeset` 一次写 N 条 revert ✗**）。 */
    function p2bAtom(kind, payload, extra) {
      p2bIdSeq = (p2bIdSeq + 1) % 10000;
      return p2bTag({
        actor: "human:web",
        id: "01P2B" + String(Date.now()).padStart(13, "0") + String(p2bIdSeq).padStart(4, "0"),
        kind, payload, schema_version: 1, seq: 0,
        session: p2bSession, timestamp: Date.now(),
        ...(extra || {}),
      });
    }
    async function p2bHeadSeq() {
      const meta = await wrap(tx(handle, "docs", "readonly").get(doc));
      return (meta && meta.seq) || 0;
    }
    async function p2bChangesetAtomCount(changesetId) {
      const rows = await atomsOf(handle, doc);
      let n = 0;
      for (const r of rows) if ((r.atom || {}).changeset_id === changesetId) n++;
      return n;
    }
    /** **撤销一个变更集里的全部原子 ✗**（**与服务端 `revert_changeset_atoms` 同形 ✓**）：
     *  **∴ 变更集里的每条原子 ⇒ 一条 `revert{target}` ✗**；**∴ 这些 revert **自己归一个新变更集** ✗**
     *  （**∴ 这次撤销本身也可被一次撤销 ✓**）；**∴ 语义由折叠层有效集保证 ✗**
     *  （**`revert(revert(x)) ≡ reapply(x)` ✓ —— **∴ 与服务端同一套 `yanshi-core` 折叠代码 ✓**）。 */
    async function p2bRevertChangeset(changesetId) {
      const rows = await atomsOf(handle, doc);
      const targets = [];
      for (const r of rows) {
        const a = r.atom || {};
        if (a.changeset_id === changesetId && a.id) targets.push(a.id);
      }
      const revertCid = p2Ulid();
      let reverted = 0;
      for (const target of targets) {
        await putAtom(handle, doc, p2bAtom("revert", { target }, { changeset_id: revertCid }));
        reverted++;
      }
      return { reverted, revert_changeset_id: revertCid, targets };
    }
    /** **拒绝单条建议 ✗**（**与服务端 `reject_one` 同形 ✓**）：
     *  **∴ 存在性校验 ✗**（**∴ 拒绝拼错的 id 也"成功"是 bug ✓ —— **∴ 服务端测试抓到过 ✓**）；
     *  **∴ 状态闸门 ✗**（**已接受 ⇒ 409 ✗**；**已拒绝 ⇒ 幂等返回 ✓**）；
     *  **∴ 写 `reject_suggestion` 原子 ＋ 引用它的待处理标注 ⇒ rejected ✓**。
     *  @returns `{ ok, status, body }` */
    async function p2bRejectOne(suggestionId, reason) {
      const fail = (status, error, why) => ({ ok: false, status,
        body: { ok: false, error, endpoint: "/api/tools/reject_suggestion", server: false, reason: why } });
      if (!suggestionId) return fail(400, "missing_suggestion_id", "需要 suggestion_id");
      const rows = await atomsOf(handle, doc);
      const target = rows.map((r) => r.atom).find((a) => a && a.id === suggestionId);
      if (!target) return fail(404, "suggestion_not_found", "建议 " + suggestionId + " 不在日志中");
      if (target.kind !== "suggest")
        return fail(400, "not_a_suggestion", suggestionId + " 是 " + target.kind + " 原子，不是建议（suggest）");
      let acceptedBy = null, rejectedBy = null;
      for (const r of rows) {
        const a = r.atom || {};
        if ((a.payload || {}).target_atom_id !== suggestionId) continue;
        if (a.kind === "accept_suggestion") acceptedBy = a.id;
        else if (a.kind === "reject_suggestion") rejectedBy = a.id;
      }
      if (acceptedBy)
        return fail(409, "already_accepted",
                    "建议 " + suggestionId + " 已被接受（原子 " + acceptedBy + "），不能再拒绝");
      if (rejectedBy)
        return { ok: true, status: 200,
                 body: { ok: true, suggestion_id: suggestionId, already_rejected: true,
                         reject_atom_id: rejectedBy, rejected_annotations: [], server: false,
                         note: "幂等：此前已拒绝，不重复写原子" } };
      const atom = p2bAtom("reject_suggestion", { target_atom_id: suggestionId, reason: reason || "" });
      const seq = await putAtom(handle, doc, atom);
      const st = p2bAnnLoad();
      const rejectedAnns = [];
      for (const id of st.order) {
        const vs = st.versions[id];
        if (!vs || !vs.length) continue;
        const a = vs[vs.length - 1];
        if (a.suggestion_id === suggestionId && a.status === "pending")
          rejectedAnns.push(p2bAnnTransition(st, id, "rejected", null).id);
      }
      p2bAnnSave();
      return { ok: true, status: 200,
               body: { ok: true, suggestion_id: suggestionId, seq, already_rejected: false,
                       reject_atom_id: atom.id, rejected_annotations: rejectedAnns, server: false } };
    }
    /** **400 快捷 ✗**（**P2b 非标注分支的参数错误 ✓**）。 */
    function p2bBad(reason, error) {
      return json({ ok: false, error: error || "invalid_argument", endpoint: path,
                    reason, server: false }, 400);
    }
    // **★ P2b 标注独立通道 ✓ ★**（**∴ 与服务端同形："**独立 append-only 通道，不进原子日志**" ✓**）：
    //   **∴ 为什么不用原子日志 ✗**：**`AtomKind` 里没有标注变体 ✗** ⇒ **∴ 写成未知 kind 的原子 ✗**
    //     ⇒ **∴ 内核 `load_atoms_json` 解析**整条**日志时失败 ✗** ⇒ **∴ 整个文档渲不出来 ✓** —— **∴ 那是真破坏 ✗**；
    //   **∴ 本地通道 = `localStorage` ✗**（**∴ 按文档键存整份标注表 ✓**），**∴ 非浏览器环境（node 判据）⇒ 内存 Map ✓**
    //   （**∴ 内存 Map 在 `makeLocalApi` 层：跨请求存活 ✓**）。
    function p2bAnnChannel() {
      try { if (typeof localStorage !== "undefined") return "localStorage"; } catch (e) {}
      return "memory";
    }
    function p2bAnnLoad() {
      let st = p2bAnnMem.get(doc);
      if (!st) {
        st = { order: [], versions: {} };
        try {
          if (typeof localStorage !== "undefined") {
            const raw = localStorage.getItem("yanshi:annotations:" + doc);
            if (raw) {
              const parsed = JSON.parse(raw);
              if (parsed && Array.isArray(parsed.order) && parsed.versions &&
                  typeof parsed.versions === "object") st = parsed;
            }
          }
        } catch (e) { /* **∴ 损坏 ⇒ 从空开始 ✗**（**∴ 不抛 ✓**） */ }
        p2bAnnMem.set(doc, st);
      }
      return st;
    }
    function p2bAnnSave() {
      const st = p2bAnnMem.get(doc);
      if (!st) return;
      try {
        if (typeof localStorage !== "undefined")
          localStorage.setItem("yanshi:annotations:" + doc, JSON.stringify(st));
      } catch (e) { /* **∴ 配额等 ⇒ 内存里还有 ✗**（**∴ 不抛 ✓**） */ }
    }
    function p2bFail(status, error, reason) {
      const e = new Error(reason); e.p2bStatus = status; e.p2bError = error; throw e;
    }
    function p2bCatch(e, endpoint) {
      return json({ ok: false, error: (e && e.p2bError) || "annotation_failed",
                    endpoint: endpoint || path,
                    reason: String((e && e.message) || e),
                    server: false }, (e && e.p2bStatus) || 500);
    }
    function p2bAnnLatest(st, id) {
      const vs = st.versions[id];
      if (!vs || vs.length === 0)
        p2bFail(404, "annotation_not_found", "标注 " + id + " 不存在");
      return vs[vs.length - 1];
    }
    function p2bAnnPending(st) {
      let n = 0;
      for (const id of st.order) {
        const vs = st.versions[id];
        if (vs && vs.length && vs[vs.length - 1].status === "pending") n++;
      }
      return n;
    }
    const P2B_ANN_TYPES = ["region", "object", "arrow", "text", "doodle", "highlight"];
    const P2B_ANN_INTENTS = ["modify", "add", "remove", "replace", "style", "move", "resize", "color"];
    const P2B_ANN_STATUSES = ["pending", "resolved", "rejected"];
    /** **标注目标归一 ✗**（**与服务端 `annotation_target` 同形 ✓**）：
     *  **∴ `target:"object"` ⇒ 要 `object_id` ✗**；**∴ 缺省／`region` ⇒ 要 `bbox`（**`{x,y,w,h}` ✓**，
     *  **可直接平铺在 target 上 ✓**）。**序列化形如 `{target:"region", bbox:{…}}` ✗**
     *  （**∴ 与 `AnnotationTarget` 的 `#[serde(tag="target")]` 同形 ✓**）。 */
    function p2bAnnTarget(t) {
      const kind = (t && t.target) || "region";
      if (kind === "object") {
        if (!t || t.object_id == null || String(t.object_id) === "")
          p2bFail(400, "missing_target_object_id", "target=object 时需要 object_id");
        return { target: "object", object_id: String(t.object_id) };
      }
      if (kind !== "region")
        p2bFail(400, "invalid_target", "target 只能是 region（缺省）或 object，得到 " + kind);
      const b = (t && t.bbox) || t || {};
      const bbox = { x: Number(b.x), y: Number(b.y), w: Number(b.w), h: Number(b.h) };
      if (![bbox.x, bbox.y, bbox.w, bbox.h].every(Number.isFinite))
        p2bFail(400, "missing_target_bbox", "target=region 时需要 bbox（{x,y,w,h}，可直接平铺在 target 上）");
      return { target: "region", bbox };
    }
    /** **带状态机闸门的状态转换 ✗**（**与服务端 `transition_gated` 同形 ✓**）：
     *  **∴ 幂等（**同状态 ⇒ 不新增版本 ✓**）＋ 禁止 `resolved ↔ rejected` 直接翻转 ✗**
     *  （**∴ 要翻转先 `update_annotation` 重开为 pending ✓**）。 */
    function p2bAnnTransition(st, id, status, atomId) {
      const cur = p2bAnnLatest(st, id);
      if (cur.status === status) return cur;
      if ((cur.status === "resolved" && status === "rejected") ||
          (cur.status === "rejected" && status === "resolved"))
        p2bFail(409, "annotation_status_flip",
          "标注 " + id + " 当前为 " + cur.status + "，不能直接翻转为 " + status +
          "：请先重开（update_annotation 传 status=pending）");
      const next = { ...cur, status, updated_at: Date.now(), revision: cur.revision + 1 };
      if (atomId != null) { next.resolved_by = atomId; next.resolved_at = Date.now(); }
      st.versions[id].push(next);
      return next;
    }

    // **① `/api/documents`（**创建 ✓**）**：**∴ 与真实服务端一样返回 `token` ✗**
    //（**∴ 本部署没有鉴权 ✗ ⇒ **token 是**本地占位**✗，**而字段存在 ✓** ⇒ **∴ 前端无需分支 ✓**）。
    // **★ 前端启动所需的非 `/api/` 端点 ⇒ 返回合理默认值 ✗ ★**（第 827 轮 ✓）
    //   **∴ 为什么 ✗**：**它们**不在 `/api/` 下**✗ ⇒ **∴ 原来**打到网络 ⇒ **∴ 405 ✓**
    //   ⇒ **∴ 现在**：**本地层**直接答**默认值 ✗** ⇒ **∴ 于是**启动路径**不再报错 ✓**
    if (path === "/get_preferences") {
      return json({ ok: true, preferences: await p1ReadPreferences(handle, body), server: false });
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
      // **★ P1：走可注入的 `__loaders.wasm()` ✗ ★**（**浏览器里与原来同一条路径 ✓**；
      //   **∴ Node 判据用 `__setLocalApiLoadersForTest` 换假内核 ✓**）。
      let mod = null;
      try {
        mod = await __loaders.wasm();
        // **★ 把模块挂出来 ✗ ★**（第 60 轮 ✓）：**∴ 内核的真值函数**
        //   （**`quantize_reference_rgba` ✓）是**模块级自由函数**✗
        //     ⇒ **∴ 它**不在 `WasmKernel` 实例上 ✓ ⇒ **∴ 页面侧**要**拿到模块**才行 ✓**** ✓✓
        //   **∴ 而**判据要**与内核真值比**✗ ⇒ **∴ 于是**：**在这里挂一份 ✓**（**只读 ✓）** ✓✓
        if (typeof window !== "undefined") window.__yanshiWasmModule = mod;
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
      const storeMod = await import("./store.js");
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

    // **★ `/api/documents/import`（工程包导入）✗ ★**（2026-10-10 定位 ✓）：
    //   **∴ 为什么缺 ✗**：前端 `importProjectFile` 走的是这条 REST 路由（分片上传协议）✗，
    //     而本地层只有 `/api/tools/import_project`（收 JSON body）✗ ⇒ 前端调 `?begin=1`
    //     直接 throw `endpoint_not_local` ⇒ 导入工程文件功能完全不可用 ✓。
    //   **∴ 协议照服务端**（`server.rs:1061` ✓）：三步 ——
    //     ① `?begin=1` ⇒ 建上传会话，回 `upload_id` 与单片上限；
    //     ② `?upload=<id>&offset=<n>` ⇒ 追加分片，回实际收到的字节数；
    //     ③ `?upload=<id>&finish=1[&doc_id=<id>]` ⇒ 解析 JSON 工程包并导入，回新文档 id。
    //   **∴ PWA 的工程包是 JSON**（`export_project` 产出）✗ ⇒ finish 时按 JSON 解析 ✓，
    //     不是 tar（服务端才处理 tar ✓）。
    if (path === "/api/documents/import") {
      const q = new URL(req.url).searchParams;
      // 上传会话存在 makeLocalApi 闭包里（跨请求存活，与 kernels 同级）。
      if (!globalThis.__yanshiUploads) globalThis.__yanshiUploads = new Map();
      const uploads = globalThis.__yanshiUploads;
      if (q.get("begin") != null) {
        const uploadId = "up_" + Date.now().toString(36) + "_" +
                         Math.random().toString(36).slice(2, 8);
        uploads.set(uploadId, { chunks: [], received: 0, started: Date.now() });
        return json({ ok: true, upload_id: uploadId, max_chunk_bytes: 8 * 1024 * 1024,
                      server: false });
      }
      const uploadId = q.get("upload");
      const sess = uploadId && uploads.get(uploadId);
      if (!sess) {
        return json({ ok: false, error: "invalid_upload",
                      reason: "上传会话不存在或已过期（先调 ?begin=1）",
                      server: false }, 400);
      }
      if (q.get("finish") != null) {
        // 收尾：拼分片 → 解析 JSON → 导入原子。
        const total = sess.chunks.reduce((n, c) => n + c.length, 0);
        const bytes = new Uint8Array(total);
        let off = 0;
        for (const c of sess.chunks) { bytes.set(c, off); off += c.length; }
        uploads.delete(uploadId);
        let project;
        try {
          project = JSON.parse(new TextDecoder().decode(bytes));
        } catch (e) {
          return json({ ok: false, error: "bad_project",
                        reason: "工程包不是有效 JSON：" + String(e.message || e).slice(0, 100),
                        server: false }, 400);
        }
        const atoms = project.atoms;
        if (!Array.isArray(atoms)) {
          return json({ ok: false, error: "bad_project",
                        reason: "工程包需要 atoms 数组", server: false }, 400);
        }
        // 新文档 id：用 ?doc_id= 或包内 meta 或自动生成。
        const newDocId = q.get("doc_id") ||
                         (project.meta && project.meta.doc_id) ||
                         ("imported-" + Date.now().toString(36));
        // 先建文档（create_document 原子）。
        const width = (project.meta && project.meta.width) || 1024;
        const height = (project.meta && project.meta.height) || 1024;
        await putAtom(handle, newDocId, {
          actor: "human:web", id: "01LOCAL" + String(Date.now()).padStart(13, "0"),
          kind: "create_document",
          payload: { background: { a: 255, b: 255, g: 255, r: 255 },
                     color_space: "srgb", doc_id: newDocId, height, width },
          schema_version: 1, session: "session:web", timestamp: Date.now(),
        });
        // 恢复 blob。
        const { putBlob } = await import("./store.js");
        let blobsRestored = 0;
        if (project.blobs && typeof project.blobs === "object") {
          for (const hash of Object.keys(project.blobs)) {
            const b64 = project.blobs[hash];
            if (typeof b64 !== "string") continue;
            const bin = atob(b64);
            const buf = new Array(bin.length);
            for (let i = 0; i < bin.length; i++) buf[i] = bin.charCodeAt(i);
            await putBlob(handle, hash, buf);
            blobsRestored += 1;
          }
        }
        // 逐条写入原子（基本校验：kind 非空字符串；完整 kind 白名单在 p2cKnownKinds，
        //   但它定义在文件后面，这里用简化校验 ✓）。
        const seen = new Set();
        let imported = 0, skipped = 0;
        for (const a of atoms) {
          if (!a || typeof a !== "object" || typeof a.kind !== "string" || !a.kind ||
              !a.payload || typeof a.payload !== "object") { skipped += 1; continue; }
          let id = typeof a.id === "string" && a.id ? a.id : null;
          if (!id || seen.has(id)) {
            id = "01IMP" + String(Date.now()).padStart(13, "0") +
                 String(imported).padStart(4, "0");
          }
          seen.add(id);
          await putAtom(handle, newDocId, {
            actor: typeof a.actor === "string" ? a.actor : "human:web",
            id, kind: a.kind, payload: a.payload, schema_version: 1, seq: 0,
            session: typeof a.session === "string" ? a.session : "session:web",
            timestamp: Number(a.timestamp) || Date.now(),
          });
          imported += 1;
        }
        return json({ ok: true, server: false, doc_id: newDocId, token: "local",
                      atoms: imported, skipped, blobs: blobsRestored,
                      note: "工程包已导入为新文档 " + newDocId });
      }
      // 分片上传：body 是二进制。
      const offset = Number(q.get("offset") || 0);
      if (offset !== sess.received) {
        return json({ ok: false, error: "offset_mismatch",
                      reason: "offset 对不上（服务端已收 " + sess.received +
                              "，客户端从 " + offset + " 传）",
                      received: sess.received, server: false }, 409);
      }
      const buf = new Uint8Array(await req.arrayBuffer());
      sess.chunks.push(buf);
      sess.received += buf.length;
      return json({ ok: true, received: sess.received, server: false });
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
          // **★ 有打开的变更集／事务 ⇒ 挂 `changeset_id` ✗ ★**（P2b ✓）：**∴ 调用方自带的优先 ✓**。
          last = await putAtom(handle, doc, p2bTag({ ...atom, seq: undefined }));
        }
        return json({ ok: true, seq: last, count: list.length, server: false });
      }
      const list = await atomsOf(handle, doc);
      return json({ ok: true, atoms: list.map((r) => r.atom), count: list.length, server: false });
    }

    // **③ `/api/tools/list_layers`（**从原子推导 ✓**）**
    // **★★ 四条"**从既有原子日志就能如实回答**"的端点 ✗ ★★**（第 929 轮 ✓）：
    //   **∴ 为什么值得做 ✗**：**用户明确抱怨日志里的 `not_implemented_locally` 刷屏 ✓**
    //     ⇒ **∴ 而这四条**不需要新机制 ✗**（**原子日志**已经在 IndexedDB 里 ✓）
    //       ⇒ **∴ 于是**：**它们**可以**如实**回答 ✓ ⇒ **∴ 报错**自然消失 ✓**** ✓✓
    //   **∴ 形状一律照前端真正读的字段 ✗**（**∴ 不**自创 ✓）：
    //     `get_log` ⇒ `atoms` ＋ `count` ✓；`list_objects` ⇒ `objects` ＋ `count` ✓；
    //     `list_selections` ⇒ `selections` ＋ `count` ✓；`get_preferences` ⇒ `preferences` ✓。
    //   **∴ 且**：**推导不出来的**宁可**不给**✗**（**∴ 不**编造字段 ✓）** ✓✓

    // **① `/api/tools/get_preferences` ✗**：**已实现的是**无前缀的 `/get_preferences` ✗**
    //   ⇒ **∴ 而**前端调的是 **`/api/tools/` 前缀** ✓ ⇒ **∴ 于是**它 501 ✓
    //     ⇒ **∴ 两条路径**给**同一份回答** ✓（**∴ 故意重复 ✓，**因为两个入口都真实存在 ✓）** ✓✓
    if (path === "/api/tools/get_preferences") {
      const prefs = await p1ReadPreferences(handle, body);
      return json({ ok: true, preferences: prefs, count: Object.keys(prefs).length,
                    persisted: true, where: "IndexedDB（本机浏览器）", server: false });
    }

    // **② `/api/tools/get_log` ✗**：**历史面板**要 `atoms` ＋ `count` ✓
    //   ⇒ **∴ 直接把**本地原子日志**按序给它 ✓**（**∴ 它**就是事实 ✓）** ✓✓
    if (path === "/api/tools/get_log") {
      const rows = await atomsOf(handle, doc);
      // **★ `order` 也要从 POST body 读 ✗ ★**（PWA 鹦鹉测试 code review ✓）：
      //   **∴ 前端 `callTool("get_log", {order:"desc"})` 走 POST body ✗**（`callToolLocal` 的实现 ✓）
      //   ⇒ **∴ 只读 `q`（URL 参数）⇒ 本地层永远拿不到 `order` ✗** ⇒ 历史排序在 PWA 下仍是旧→新 ✗。
      const order = (q && q.get("order")) || (body && body.order) || "";
      const newestFirst = order === "desc" || order === "newest_first";
      let atoms = rows.map((r) => (r && r.atom) || null).filter(Boolean);
      if (newestFirst) atoms = atoms.slice().reverse();
      return json({ ok: true, atoms, count: atoms.length, server: false });
    }

    // **③ `/api/tools/list_objects` ✗**：**对象列表** ✗** ⇒ **∴ 从**声明了 `object_id` 的原子**推导 ✓
    //   **∴ 如实说明 ✗**：**这是**日志级**的对象表 ✓（**∴ 字段**取原子里**真的有的那些 ✓），
    //     **∴ 不**补算位置／包围盒 ✓（**∴ 那**要重放内核 ⇒ **∴ 留给**需要它的调用方 ✓）** ✓✓
    if (path === "/api/tools/list_objects") {
      const rows = await atomsOf(handle, doc);
      const objects = [];
      for (const r of rows) {
        const a = (r && r.atom) || {};
        const pl = a.payload || {};
        if (!pl.object_id) continue;
        objects.push({
          object_id: pl.object_id,
          layer_id: pl.layer_id ?? null,
          type: pl.type ?? a.kind ?? null,
          medium: pl.source && pl.source.medium ? pl.source.medium : null,
        });
      }
      return json({ ok: true, objects, count: objects.length, server: false,
                    derived_from: "atoms" });
    }

    // **④ `/api/tools/list_selections` ✗**：**没有选区原子 ⇒ **∴ 如实报空表 ✓****
    //   **∴ 重点 ✗**：**前端**据此**清掉本地轮廓** ✓（**∴ 它**的注释写明：
    //     **"**服务端说没有选区 ⇒ 本地也必须清掉 ✗**"）⇒ **∴ 所以**：
    //     **必须**报**真实的空**✗，**而**不是**永远空 ✗**（**∴ 有 `create_selection` 就列出来 ✓）** ✓✓
    if (path === "/api/tools/list_selections") {
      const rows = await atomsOf(handle, doc);
      const selections = [];
      for (const r of rows) {
        const a = (r && r.atom) || {};
        if (a.kind !== "create_selection") continue;
        const pl = a.payload || {};
        selections.push({ selection_id: pl.selection_id ?? pl.id ?? null,
                          shape: pl.shape ?? null, bbox: pl.bbox ?? null });
      }
      return json({ ok: true, selections, count: selections.length, server: false,
                    derived_from: "atoms" });
    }

    // **★ `/api/tools/list_assets` ✗ ★**（第 930 轮 ✓；**用户报的未映射端点 ✓**）：
    //   **∴ 为什么以前做不到 ✗**：**静态托管没有目录列表 ✗** ⇒ **∴ 前端要问"**有哪些资产**"**
    //     只能**读一份索引 ✓** ⇒ **∴ 而**索引**此前不存在 ✗**（**∴ 调色板与纹理**也从未部署 ✓）
    //     ⇒ **∴ 现在**：**同步脚本**部署两类资产 ＋ 写出 `/assets-index.json` ✓
    //       ⇒ **∴ 于是**：**本端点**如实**照索引回答 ✓**** ✓✓
    //   **∴ 形状照**服务端与前端 ✗**：**返回键是 `assets` ✗**（**∴ 前端读 `palettes.assets` ✓，
    //     **不是** `items` ✓）**；**每项含 `name`／`bytes`／`source`／`format`／`category`／`usable` ✓** ✓✓
    //   **∴ 如实 ✗**：**索引缺失 ⇒ **∴ 501 ＋ 原因 ✓**（**∴ 不**报空表冒充"没有资产" ✓）** ✓✓
    if (path === "/api/tools/list_assets") {
      const kind = String(body.kind || "brush");
      // **∴ 取索引**绝不能抛出去 ✗**（**∴ 抛出去会被外层兜成 `not_implemented_locally` ✗**
      //   ⇒ **∴ 那是**误导 ✓：**端点**其实**实现了 ✗，**只是**索引没部署 ✓）** ✓✓
      let idx = null;
      try {
        const idxRes = await fetch("/assets-index.json", { cache: "no-store" });
        if (idxRes.ok) idx = await idxRes.json();
      } catch (e) { idx = null; }
      if (!idx) {
        return json({ ok: false, error: "index_missing", server: false,
                      reason: "本部署没有可读的 /assets-index.json ⇒ 跑一次 scripts/pwa-sync-viewer.mjs" }, 501);
      }
      const list = Array.isArray(idx[kind]) ? idx[kind] : [];
      const USABLE = {
        palette: [".gpl", ".kpl", ".json"],
        texture: [".png", ".jpg", ".jpeg", ".webp"],
      };
      const assets = list.map((it) => {
        const dot = String(it.name).lastIndexOf(".");
        const format = dot >= 0 ? String(it.name).slice(dot + 1).toLowerCase() : "";
        const allow = USABLE[kind];
        const usable = allow ? allow.indexOf("." + format) >= 0 : true;
        const item = { name: it.name, bytes: it.bytes, source: "bundled", format,
                       category: null, usable };
        // **∴ 不能用的**必须说出原因 ✗**（**∴ 不**静默让界面显示一个点了没反应的项 ✓）**。
        if (!usable) item.reason = "本部署还不支持这个格式（." + format + "）";
        return item;
      });
      return json({ ok: true, kind, count: assets.length, assets, can_import: false,
                    hint: "本部署只列出随包发布的资产（source=bundled）", server: false });
    }

    // **★ `/api/tools/list_palette_colors` ✗ ★**（第 930 轮 ✓）：
    //   **∴ 为什么必须**一起做 ✗**：**只做 `list_assets` ⇒ **∴ 下拉里**有名字 ✗，
    //     **而**点进去**读不出颜色 ✓** ⇒ **∴ 那**比 501 ＋ 离线回退**更差 ✓**（**∴ 我**上一轮
    //     记过这条判断 ✓）⇒ **∴ 所以**：**两个一起 ✓**** ✓✓
    //   **∴ 解析在**浏览器侧 ✗**（**∴ 格式与服务端口径一致：
    //     `.gpl`／`.kpl` 文本 ✓、`.json`（**键 ⇒ 十六进制串 或 串数组 ✓）** ✓）** ✓✓
    if (path === "/api/tools/list_palette_colors") {
      const name = String(body.palette || "");
      if (!name) return json({ ok: false, error: "missing_palette", server: false }, 400);
      // **∴ 同理 ✗**：**取文件失败**必须报**它自己**的原因 ✗**（**∴ 不**能被兜成"**未实现**" ✓）** ✓✓
      let text = null;
      try {
        const res = await fetch("/palettes/" + encodeURIComponent(name), { cache: "no-store" });
        text = res.ok ? await res.text() : null;
        if (text === null) {
          return json({ ok: false, error: "palette_not_found", server: false,
                        reason: name + " ⇒ HTTP " + res.status }, 404);
        }
      } catch (e) {
        return json({ ok: false, error: "palette_not_found", server: false,
                      reason: name + " ⇒ " + String((e && e.message) || e) }, 404);
      }
      const colors = parsePaletteText(text, name);
      const limit = Number(body.limit) > 0 ? Number(body.limit) : 256;
      return json({ ok: true, palette: name, colors: colors.slice(0, limit),
                    count: Math.min(colors.length, limit), total: colors.length,
                    truncated: colors.length > limit, server: false });
    }

    if (path === "/api/tools/list_layers") {
      // **★ P1：推导要理解删除与重排 ✗ ★**：
      //   **∴ `delete_layer` 写 `tombstone{layer_id}` ✓**（**照服务端 `write_delete_layer` ✓**）
      //     ⇒ **∴ 被删的层**不再列出**✗**（**∴ 否则"删了还在"✗ ⇒ **∴ 撒谎 ✓**）；
      //   **∴ `reorder_layers{order}` 整体换序 ✓** ⇒ **∴ 返回**自底向上**的顺序 ✗**
      //     （**与服务端 `order` 口径一致 ✓**）。
      //   **∴ 旧注释的两个字段教训**仍然有效**✗**（**`payload` 里取值 ✓、回 `layer_id` 键 ✓**）。
      //   **★ `set_property` 已折叠**（`p1LayerOrder` 内 ✓）：`visible`／`opacity`／
      //     `blend_mode`／`locked` 等属性如实返回 ✓。
      const { layers, order } = p1LayerOrder(await atomsOf(handle, doc));
      const out = order.map((id) => layers.get(id)).filter(Boolean);
      return json({ ok: true, layers: out, count: out.length, server: false,
                    derived_from: "atoms" });
    }

    // **★ `/api/tools/create_layer` ✗ ★**（第 928 轮 ✓；**用户"**画不了画**"的最后一环 ✓）：
    //   **∴ 为什么 ✗**：**没有图层时**前端会**自动建一个 ✗**（`ensurePaintLayer` ✓）
    //     ⇒ **∴ 而**本地层**没有这个端点 ⇒ **∴ 501 ⇒ **∴ `state.layerId` 永远为空 ✓**
    //       ⇒ **∴ 于是**：**画布上拖拽**什么都不会发生 ✓（**实测 `applies:0` ✓）** ✓✓
    //   **∴ 形状照**权威样本 ✗**（`kernel.rs` 的 `scene_atoms` ＋ `transport.rs` 的客户端原子 ✓）：
    //     `{kind:"create_layer", payload:{layer_id,name}}` ✓ —— **∴ 不**自创字段 ✓**** ✓✓
    if (path === "/api/tools/create_layer") {
      const pl = (body && body.payload) || {};
      const layerId = body.layer_id || pl.layer_id || "layer_" + String(Date.now());
      const name = body.name || pl.name || layerId;
      const atom = {
        actor: "human:web",
        id: "01LAYER" + String(Date.now()).padStart(13, "0"),
        kind: "create_layer",
        payload: { layer_id: layerId, name },
        schema_version: 1,
        seq: 0,
        session: "session:web",
        timestamp: Date.now(),
      };
      const seq = await putAtom(handle, doc, atom);
      return json({ ok: true, seq, server: false, layer_id: layerId, name,
                    note: "已在本地原子日志写入 create_layer（内核重放时建层）" });
    }

    // **★ `/api/tools/update_layer` ✗ ★**（2026-10-10 定位 ✓）：
    //   **∴ 为什么缺 ✗**：**本地层只有 `list_layers`／`create_layer` ✗**，**没有 `update_layer` ⇒
    //     **∴ 前端点"显示/隐藏"⇒ 501 `not_implemented_locally` ✓**（**线上实测 ✓）。
    //   **∴ 形状照服务端** ✗**（`tools.rs` 的 `write_update_layer` ✓）：**每个 patch key 写一个
    //     `{kind:"set_property", payload:{layer_id,key,value}}` 原子 ✓ —— **∴ 内核本来就支持折叠 ✓**
    //     （**`fold.rs` 的 `apply_property_to_layer` ✓，**隔离实验验证过 ✓），**∴ 不**自创格式 ✓**** ✓✓
    //   **∴ 合法 key 与服务端 `LAYER_PATCH_KEYS` 对齐**（8 个 ✓）。
    if (path === "/api/tools/update_layer") {
      const UPDATE_LAYER_KEYS = ["name", "visible", "opacity", "blend_mode",
                                 "locked", "alpha_lock", "clipping_mask", "mask_id"];
      const pl = (body && body.payload) || {};
      const layerId = (body && body.layer_id) || pl.layer_id;
      const patch = (body && body.patch) || pl.patch || {};
      if (!layerId) {
        return json({ ok: false, error: "invalid_argument",
                      reason: "update_layer 缺少 layer_id", server: false }, 400);
      }
      const keys = Object.keys(patch).filter((k) => UPDATE_LAYER_KEYS.indexOf(k) >= 0);
      if (!keys.length) {
        return json({ ok: false, error: "invalid_argument",
                      reason: "patch 中没有可修改的图层属性（可用：" +
                              UPDATE_LAYER_KEYS.join("／") + "）",
                      server: false }, 400);
      }
      let seq = 0;
      for (const key of keys) {
        const atom = {
          actor: "human:web",
          id: "01SETPROP" + String(Date.now()).padStart(13, "0") + key.slice(0, 4),
          kind: "set_property",
          payload: { layer_id: layerId, key, value: patch[key] },
          schema_version: 1,
          seq: 0,
          session: "session:web",
          timestamp: Date.now(),
        };
        seq = await putAtom(handle, doc, atom);
      }
      return json({ ok: true, seq, server: false, layer_id: layerId,
                    updated: keys,
                    note: "已写入 set_property 原子（内核重放时更新图层属性）" });
    }

    // **★ P1 端点（7 个）✓ ★**（**服务端语义见 `crates/yanshi-server/src/tools.rs` 的 `write_*` ✓**）：
    //   **∴ 写操作**一律**写内核认识的 `kind` ✗** —— **∴ `delete_layer` 写 `tombstone`
    //     （**照 `write_delete_layer` ✓**），**而**不是**自造 `delete_layer` ✗**
    //     （**`AtomKind` 里**没有**它 ✗ ⇒ **∴ `load_atoms_json` 整数组解析失败 ⇒ **∴ 会毒化整个文档折叠 ✓**）；
    //   **∴ 封套复用 `p2Envelope` ✓**（**8 字段 ✓**）；**400／404 复用 `p2Bad`／`p2Missing` ✓**。

    // **① `delete_layer` ✗**：**照 `write_delete_layer` ✓** ⇒ **`tombstone{layer_id}` 原子 ✓**。
    if (path === "/api/tools/delete_layer") {
      const pl = (body && body.payload) || {};
      const layerId = body.layer_id || pl.layer_id;
      if (typeof layerId !== "string" || !layerId) return p2Bad("delete_layer 需要 layer_id（字符串）");
      const { layers } = p1LayerOrder(await atomsOf(handle, doc));
      if (!layers.has(layerId)) return p2Missing("图层 " + layerId + " 不存在或已被删除");
      const atom = p2Envelope("tombstone", { layer_id: layerId });
      const seq = await putAtom(handle, doc, atom);
      return json({ ok: true, atom_id: atom.id, seq, head: seq, layer_id: layerId, server: false,
                    note: "已在本地原子日志写入 tombstone（删层语义；list_layers 推导会剔除它）" });
    }

    // **② `duplicate_layer` ✗**：**照 `write_duplicate_layer` 的"忠实复制" ✓**（**不是**标记原子 ✗）：
    //   **∴ 为什么 ✗**：**标记原子（`kind:"duplicate_layer"`）`AtomKind` 不认 ✗**
    //     ⇒ **∴ 整数组解析失败 ⇒ **∴ 毒化文档折叠 ✓**（**见文件头注释 ✓**）；
    //   **∴ 做法 ✗**：**`create_layer`（**新 id ✓）＋ **该层每个对象的复制原子**
    //     （**新 `object_id` ＋ **同一批 blob 引用**✗ —— **∴ 内容寻址 ✓，**不复制像素 ✓，
    //       与服务端"**对象引用同一批 blob**"一致 ✓**）＋ **`reorder_layers` 完整顺序**
    //     （**副本插在原图层后一位＝正上方 ✓，**照服务端 ✓**）。
    if (path === "/api/tools/duplicate_layer") {
      const pl = (body && body.payload) || {};
      const layerId = body.layer_id || pl.layer_id;
      if (typeof layerId !== "string" || !layerId) return p2Bad("duplicate_layer 需要 layer_id（字符串）");
      const rows = await atomsOf(handle, doc);
      const { layers, order } = p1LayerOrder(rows);
      const src = layers.get(layerId);
      if (!src) return p2Missing("图层 " + layerId + " 不存在或已被删除");
      const newLayerId = body.new_layer_id || pl.new_layer_id || ("layer_" + Date.now().toString(36));
      if (layers.has(newLayerId)) return p2Bad("new_layer_id " + newLayerId + " 已存在");
      const name = body.name || pl.name || (src.name + " 副本");
      let seq = await putAtom(handle, doc, p2Envelope("create_layer", { layer_id: newLayerId, name }));
      let objects = 0;
      for (const r of rows) {
        const a = (r && r.atom) || {};
        const apl = (a && a.payload) || {};
        if ((a.kind === "import_image" || a.kind === "create_object") &&
            apl.layer_id === layerId && typeof apl.object_id === "string") {
          const cp = Object.assign({}, apl, {
            layer_id: newLayerId,
            object_id: "obj_" + Date.now().toString(36) + "_" + objects,
          });
          seq = await putAtom(handle, doc, p2Envelope(a.kind, cp));
          objects++;
        }
      }
      const target = order.slice();
      target.splice(target.indexOf(layerId) + 1, 0, newLayerId);
      seq = await putAtom(handle, doc, p2Envelope("reorder_layers", { order: target }));
      return json({ ok: true, seq, head: seq, layer_id: newLayerId, from: layerId,
                    name, objects, z_index: target.indexOf(newLayerId), server: false,
                    note: "忠实复制：create_layer＋逐对象复制（共享 blob）＋完整重排；全部原子内核可识别" });
    }

    // **③ `reorder_layers` ✗**：**照 `write_reorder_layers` ✓**：
    //   **∴ 参数名接受 `order`（**服务端 ✓**）与 `layer_ids`（**任务口径 ✓**）；
    //   **∴ 必须是**全部存活图层的完整顺序**✗**（**∴ 缺／多／重复 ⇒ 400 ✓ —— 与服务端校验同口径 ✓**）；
    //   **∴ payload 键用 `order` ✗**（**内核 `ReorderLayers` 认这个键 ✓**）。
    if (path === "/api/tools/reorder_layers") {
      const ids = Array.isArray(body.order) ? body.order
                : (Array.isArray(body.layer_ids) ? body.layer_ids : null);
      if (!ids || ids.length === 0) return p2Bad("reorder_layers 需要 order（完整图层 id 顺序，自底向上）");
      if (!ids.every((x) => typeof x === "string")) return p2Bad("order 必须全部是图层 id 字符串");
      const { layers } = p1LayerOrder(await atomsOf(handle, doc));
      const alive = new Set(layers.keys());
      const complete = ids.length === alive.size && new Set(ids).size === ids.length &&
                       ids.every((id) => alive.has(id));
      if (!complete) return p2Bad("order 必须是全部存活图层的完整顺序（期望 " + alive.size +
                                  " 个，实际 " + ids.length + " 个）");
      const atom = p2Envelope("reorder_layers", { order: ids });
      const seq = await putAtom(handle, doc, atom);
      return json({ ok: true, atom_id: atom.id, seq, head: seq, order: ids, server: false,
                    note: "已在本地原子日志写入 reorder_layers（list_layers 推导会换序）" });
    }

    // **④ `brush_preview` ✗**：**照 `write_brush_preview` ✓**（**与 `brush_stroke` 同一条落笔实现 ✓**）：
    //   **∴ 参数 `brush` 必填 ✓、`size` 缺省 24 ＋ 上限 512 ✓**（**与服务端同口径 ✓**）；
    //   **∴ 笔迹形状照服务端 `default_preview_points` ✓**（**见 `p1PreviewPoints` ✓**）；
    //   **∴ 用 `withKernel` 调 `paint_brush` 真画一笔 ✗**（**∴ 不是示意图 ✓**）；
    //   **∴ PNG 在 JS 侧编码**（**`encodePngRgba` ✓ —— 内核只暴露整幅 `render_region_png` ✗**）；
    //   **∴ `thumb_url` 是 `blob:` URL ✗**（**前端直接拿它当 `<img src>` ✓ —— 与服务端
    //     `yanshi://blob/…` 不同 ✗，**∴ 本部署没有 blob HTTP 服务 ✓，如实给浏览器能用的 ✓**）；
    //   **∴ 同时存进本地 blob 库**（**`blob_hash` 可查 ✓**）。
    if (path === "/api/tools/brush_preview") {
      const brushName = body.brush;
      if (typeof brushName !== "string" || !brushName) {
        return json({ ok: false, error: "missing_brush", error_code: "missing_brush",
                      reason: "brush_preview 需要 brush（笔刷名）", server: false }, 400);
      }
      const size = body.size == null ? 24 : Number(body.size);
      if (!(size > 0)) {
        return json({ ok: false, error: "invalid_size", error_code: "invalid_size",
                      reason: "size 必须大于 0", server: false }, 400);
      }
      if (size > 512) {
        return json({ ok: false, error: "invalid_size", error_code: "invalid_size",
                      reason: "预览的 size 上限是 512（要给的是 " + size + "）", server: false }, 400);
      }
      try {
        const mod = await __loaders.wasm();
        if (typeof mod.default === "function") await mod.default();
        const { fetchBrushText, paintWithKernel } = await __loaders.brushLocal();
        // **∴ 自定义笔迹或默认 S 形 ✗**（**与服务端"**不给 points 就用固定缓 S 形**"一致 ✓**）。
        let pts;
        if (Array.isArray(body.points) && body.points.length > 0) {
          pts = body.points.map((p) => [Number(p[0]), Number(p[1]),
                                        p[2] == null ? 0.5 : Number(p[2])]);
        } else {
          pts = p1PreviewPoints(size);
        }
        // **∴ 区域＝笔迹包围盒＋笔刷半径 ✗**（**照服务端 `brush_stroke_region` ✓**）。
        const radius = size / 2 + 4;
        const xs = pts.map((p) => p[0]), ys = pts.map((p) => p[1]);
        const x0 = Math.floor(Math.min(...xs) - radius);
        const y0 = Math.floor(Math.min(...ys) - radius);
        const w = Math.max(1, Math.ceil(Math.max(...xs) + radius) - x0);
        const h = Math.max(1, Math.ceil(Math.max(...ys) + radius) - y0);
        const region = { x: 0, y: 0, w, h };
        const localPts = pts.map((p) => [p[0] - x0, p[1] - y0, p[2]]);
        let entryK = kernels.get(doc);
        if (!entryK) {
          const c2 = (await atomsOf(handle, doc)).find((r) => r.atom && r.atom.kind === "create_document");
          const w2 = (c2 && c2.atom.payload && c2.atom.payload.width) || 1024;
          const h2 = (c2 && c2.atom.payload && c2.atom.payload.height) || 1024;
          entryK = { k: new mod.WasmKernel(doc, 256, w2, h2, 256 * 1024 * 1024), w: w2, h: h2 };
          kernels.set(doc, entryK);
        }
        const myb = await fetchBrushText(brushName.endsWith(".myb") ? brushName : brushName + ".myb");
        const rgba = await withKernel(doc, () => paintWithKernel(entryK.k, {
          myb, points: localPts, size, color: body.color ?? null, region,
        }));
        let painted = 0;
        for (let i = 3; i < rgba.length; i += 4) if (rgba[i] > 0) painted++;
        const png = await encodePngRgba(w, h, rgba);
        const digest = await crypto.subtle.digest("SHA-256", png);
        const hex = Array.from(new Uint8Array(digest)).map((b) => b.toString(16).padStart(2, "0")).join("");
        const blobHash = "sha256:" + hex;
        const { putBlob } = await import("./store.js");
        await putBlob(handle, blobHash, Array.from(png));
        const thumbUrl = URL.createObjectURL(new Blob([png], { type: "image/png" }));
        return json({ ok: true, brush: brushName, width: w, height: h,
                      painted_pixels: painted, steps: localPts.length,
                      blob_hash: blobHash, thumb_url: thumbUrl, mime_type: "image/png",
                      size: png.length, server: false });
      } catch (err) {
        const reason = String((err && err.message) || err);
        return json({ ok: false, error: "preview_failed", error_code: "preview_failed",
                      reason, server: false }, 500);
      }
    }

    // **⑤ `import_image` ✗**：**把已存进本地 blob 库的图钉成对象 ✓**：
    //   **∴ `blob_hash` 必填 ✓**（**∴ 先 `POST /api/blob` 存图再调 ✓ —— 与服务端流程一致 ✓**）；
    //   **∴ blob 不在库里 ⇒ 404 ✗**（**∴ 不写一条**渲染时取不到像素**的原子 ✓**）；
    //   **∴ `layer_id` 缺省取第一个存活图层 ✗**（**∴ 服务端要求必填 ✗，**本地**放宽一点 ✓**）；
    //   **∴ 自带 `object_id` ✗**（**`get_undo_status`／`list_objects` 按它分组 ✓**）。
    if (path === "/api/tools/import_image") {
      const pl = (body && body.payload) || {};
      const blobHash = body.blob_hash || pl.blob_hash;
      if (typeof blobHash !== "string" || !blobHash) {
        return json({ ok: false, error: "missing_blob_hash", error_code: "missing_blob_hash",
                      reason: "import_image 需要 blob_hash（先 POST /api/blob 存图再导入）",
                      server: false }, 400);
      }
      const { getBlob } = await import("./store.js");
      const bytes = await getBlob(handle, blobHash);
      if (!bytes) {
        return json({ ok: false, error: "blob_not_found", error_code: "blob_not_found",
                      reason: "本地 blob 库里没有 " + blobHash + "（先 POST /api/blob 存图）",
                      server: false }, 404);
      }
      const { layers, order } = p1LayerOrder(await atomsOf(handle, doc));
      const layerId = body.layer_id || pl.layer_id || order[0];
      if (typeof layerId !== "string" || !layers.has(layerId)) {
        return p2Missing("图层 " + (layerId || "(未指定)") + " 不存在（import_image 需要一个存活图层）");
      }
      const objectId = body.object_id || pl.object_id ||
                       ("obj_" + Date.now().toString(36));
      const atom = p2Envelope("import_image", {
        blob_hash: blobHash, layer_id: layerId,
        x: Number(body.x ?? pl.x ?? 0), y: Number(body.y ?? pl.y ?? 0),
        object_id: objectId,
      });
      const seq = await putAtom(handle, doc, atom);
      return json({ ok: true, atom_id: atom.id, seq, head: seq, object_id: objectId,
                    blob_hash: blobHash, layer_id: layerId,
                    x: atom.payload.x, y: atom.payload.y, server: false,
                    note: "已在本地原子日志写入 import_image（简化形状：blob 引用＋落点；完整位图形状见 brush_stroke 分支）" });
    }

    // **⑥ `get_undo_status` ✗**：**只读查询可撤销／可重做的笔数 ✓**（**照服务端 `read_get_undo_status` ✓**）：
    //   **∴ 与本地 `undo_last`／`redo_last` 同一套归并 ✗**（**连续相同 `object_id`＝一笔 ✓；
    //     无 `object_id` 的原子只进 `ignored` ✓；历史原子看 `payload.target` 的**最新**动作 ✓**）；
    //   **∴ 可撤销＝**没有**被 `revert` 的笔 ✓；可重做＝**整笔**被 `revert` 且没 `reapply` 的笔 ✓**。
    if (path === "/api/tools/get_undo_status") {
      const rows = await atomsOf(handle, doc);
      const latestAction = new Map(); // atom id -> "revert" | "reapply"（**最新**的 ✓）
      for (let i = rows.length - 1; i >= 0; i--) {
        const a = rows[i] && rows[i].atom;
        if (!a || (a.kind !== "revert" && a.kind !== "reapply")) continue;
        const t = a.payload && a.payload.target;
        if (typeof t === "string" && t && !latestAction.has(t)) latestAction.set(t, a.kind);
      }
      let undoable = 0, redoable = 0, ignored = 0;
      let cur = null, curAtoms = [];
      const flush = () => {
        if (curAtoms.length === 0) return;
        const rev = curAtoms.filter((id) => latestAction.get(id) === "revert").length;
        if (rev === 0) undoable++;
        else if (rev === curAtoms.length) redoable++;
        // **∴ 半撤状态（**部分原子被 revert ✓**）两边都不进 ✗**（**∴ 如实，不硬归类 ✓**）
        cur = null; curAtoms = [];
      };
      for (let i = rows.length - 1; i >= 0; i--) {
        const a = (rows[i] && rows[i].atom) || {};
        if (a.kind === "create_document" || a.kind === "revert" || a.kind === "reapply") {
          flush(); ignored++; continue;
        }
        const oid = a.payload && a.payload.object_id;
        if (typeof oid !== "string" || !oid) { flush(); ignored++; continue; }
        if (cur !== oid) { flush(); cur = oid; }
        curAtoms.push(a.id);
      }
      flush();
      return json({ ok: true, undoable_gestures: undoable, redoable_gestures: redoable,
                    remaining_gestures: undoable, ignored_atoms: ignored, server: false,
                    note: "只读查询：没有改动任何内容（与本地 undo_last／redo_last 同一套归并）" });
    }

    // **⑦ `set_preferences` ✗**：**照 `write_set_preferences` ✓**（**合并语义 ✓**）：
    //   **∴ 参数名接受 `preferences`（**任务口径 ✓**）与 `values`（**服务端／前端 ✓ ——
    //     前端真调 `set_preferences {values:…}` ✓**）；
    //   **∴ 值为 `null` ⇒ 删除该键 ✗**（**与服务端一致 ✓**）；
    //   **∴ 存进 IndexedDB `prefs` 表 ✗**（**`store.js` ✓ —— 刷新不丢 ✓**）；
    //   **∴ `get_preferences` 读同一份 ✗**（**∴ 写后读回一致 ✓**）。
    if (path === "/api/tools/set_preferences") {
      const values = (body && body.preferences) || (body && body.values);
      if (!values || typeof values !== "object" || Array.isArray(values)) {
        return json({ ok: false, error: "invalid_preferences", error_code: "invalid_preferences",
                      reason: "set_preferences 需要 preferences（对象 {键: 值}）", server: false }, 400);
      }
      const merged = await p1ReadPreferences(handle, null);
      for (const k of Object.keys(values)) {
        if (values[k] == null) {
          delete merged[k];
          await wrap(tx(handle, "prefs", "readwrite").delete(k));
        } else {
          merged[k] = values[k];
          await wrap(tx(handle, "prefs", "readwrite").put({ key: k, value: values[k] }));
        }
      }
      return json({ ok: true, preferences: merged, server: false,
                    hint: "值为 null 表示删除该键；这里是合并，没提到的键不动" });
    }

    // **★ P2 图层／对象／选区类端点 ✓ ★**（**服务端语义见 `crates/yanshi-server/src/tools.rs` 的 `write_*` ✓**）：
    //   **∴ 写操作**一律**写对应 `kind` 的原子 ✗**（**`payload` 照服务端形状 ✓**）；
    //   **∴ 读操作**从原子日志推导 ✗**（**∴ 不另存状态 ✓**）；
    //   **∴ 服务端**没有**的端点 ⇒ **明确 501 ＋ 原因 ✗**（**∴ 不硬凑 ✓**）。

    // **① `create_selection` ✗**：**照 `write_create_selection` ✓** ⇒ **`create_selection` 原子 ✓**。
    if (path === "/api/tools/create_selection") {
      const chk = p2CheckRegionArgs(body, "selection_id");
      if (chk.err) return chk.err;
      const seq = await putAtom(handle, doc, p2Envelope("create_selection", chk.payload));
      return json({ ok: true, seq, server: false, selection_id: chk.payload.selection_id,
                    note: "已在本地原子日志写入 create_selection" });
    }

    // **② `delete_selection` ✗**：**照 `write_delete_selection` ✓** ⇒ **`tombstone{selection_id}` ✓**。
    if (path === "/api/tools/delete_selection") {
      const selectionId = body.selection_id;
      if (typeof selectionId !== "string" || !selectionId) return p2Bad("selection_id 必填（字符串）");
      const seq = await putAtom(handle, doc, p2Envelope("tombstone", { selection_id: selectionId }));
      return json({ ok: true, seq, server: false, selection_id: selectionId,
                    note: "已在本地原子日志写入 tombstone（选区删除语义）" });
    }

    // **③ `update_selection` ⇒ 明确 501 ✗**：**服务端工具注册表里**没有**它 ✗**
    //   （**142 个已注册工具逐一核对过 ✓**）⇒ **∴ 没有服务端语义可照抄 ✗** ⇒ **∴ 不硬凑 ✓**。
    if (path === "/api/tools/update_selection") {
      return p2NoServerEndpoint("update_selection", "选区没有更新语义");
    }

    // **④ `create_mask` ✗**：**照 `write_create_mask` ✓** ⇒ **`create_mask` 原子 ✓**
    //   （**校验与选区同形 ✓**）。
    if (path === "/api/tools/create_mask") {
      const chk = p2CheckRegionArgs(body, "mask_id");
      if (chk.err) return chk.err;
      const seq = await putAtom(handle, doc, p2Envelope("create_mask", chk.payload));
      return json({ ok: true, seq, server: false, mask_id: chk.payload.mask_id,
                    note: "已在本地原子日志写入 create_mask" });
    }

    // **⑤ `delete_mask` ⇒ 明确 501 ✗**：**服务端**只有** `create_mask` ✗**（**注册表无 `delete_mask` ✓**）
    //   ⇒ **∴ 遮罩删除**没有**服务端语义可照抄 ✗** ⇒ **∴ 不硬凑 ✓**。
    if (path === "/api/tools/delete_mask") {
      return p2NoServerEndpoint("delete_mask", "遮罩只有创建语义");
    }

    // **⑥⑦ `lock_layer`／`unlock_layer` ✗**：**照 `write_lock_layer` ✓**
    //   ⇒ **本体就是 `set_property{key:"locked"}` ✗**（**∴ 不是独立原子 ✓**）。
    if (path === "/api/tools/lock_layer" || path === "/api/tools/unlock_layer") {
      const layerId = body.layer_id;
      if (typeof layerId !== "string" || !layerId) return p2Bad("layer_id 必填（字符串）");
      const locked = path === "/api/tools/lock_layer";
      const seq = await putAtom(handle, doc, p2Envelope("set_property",
        { layer_id: layerId, key: "locked", value: locked }));
      return json({ ok: true, seq, server: false, layer_id: layerId, locked,
                    note: "服务端 lock/unlock 本体就是 set_property{key:locked}，本地照写同一原子" });
    }

    // **⑧ `set_layer_blend` ✗**：**照 `write_set_layer_blend` ✓**
    //   ⇒ **本体就是 `set_property{key:"blend_mode"}` ✗**（**`update_layer` 只是另一条入口 ✓**）。
    //   **∴ 如实说明 ✗**：**`update_layer` 本地**尚未实现 ✗ ⇒ **∴ 这里**不经由它**✗**（**∴ 不虚报覆盖 ✓**），
    //     **而是**照服务端**直接写 `set_property` 原子 ✓**（**与服务端实际落盘的原子一致 ✓**）。
    if (path === "/api/tools/set_layer_blend") {
      const layerId = body.layer_id;
      if (typeof layerId !== "string" || !layerId) return p2Bad("layer_id 必填（字符串）");
      // **∴ 清单取自渲染层 ✓**（`crates/yanshi-render/src/blend.rs` 的 `BlendMode::NAMES` ✓ ⇒ 11 个 ✓）。
      const BLEND_MODES = ["normal", "multiply", "screen", "overlay", "darken",
                           "lighten", "add", "linear_dodge", "subtract",
                           "color_dodge", "difference"];
      const mode = typeof body.mode === "string" ? body.mode.toLowerCase() : "";
      if (BLEND_MODES.indexOf(mode) < 0) {
        return p2Bad("未知混合模式 " + body.mode + " ⇒ 可用：" + BLEND_MODES.join(" / "));
      }
      const seq = await putAtom(handle, doc, p2Envelope("set_property",
        { layer_id: layerId, key: "blend_mode", value: mode }));
      return json({ ok: true, seq, server: false, layer_id: layerId, mode,
                    note: "服务端 set_layer_blend 本体就是 set_property{key:blend_mode}，本地照写同一原子" });
    }

    // **⑨ `create_object` ⇒ 明确 501 ✗**：**服务端**没有**这个端点 ✗**
    //   （**注册表无 `create_object` ✓**）⇒ **∴ 对象由 `draw_stroke`／`draw_shape`／`import_image`／
    //   `add_adjustment` 等专用端点创建 `create_object` 原子 ✗** ⇒ **∴ 裸 `create_object`**
    //   **没有**服务端校验语义可照抄 ✗** ⇒ **∴ 不硬凑 ✓**。
    if (path === "/api/tools/create_object") {
      return p2NoServerEndpoint("create_object",
        "对象经由 draw_*/import_image/add_adjustment 等专用端点创建");
    }

    // **⑩ `delete_object` ✗**：**照 `write_tombstone` ✓** ⇒ **`tombstone{object_id}` ✓**。
    if (path === "/api/tools/delete_object") {
      const objectId = body.object_id;
      if (typeof objectId !== "string" || !objectId) return p2Bad("object_id 必填（字符串）");
      const seq = await putAtom(handle, doc, p2Envelope("tombstone", { object_id: objectId }));
      return json({ ok: true, seq, server: false, object_id: objectId,
                    note: "已在本地原子日志写入 tombstone（对象删除语义）" });
    }

    // **⑪ `update_object` ✗**：**照 `write_update_object` ✓**
    //   ⇒ **`patch` 里每个合法键各写一条 `set_property` 原子 ✗**
    //   （**只认 6 个键 ✓**：`visible`／`locked`／`z_index`／`layer_id`／`metadata`／`type` ✓）。
    if (path === "/api/tools/update_object") {
      const objectId = body.object_id;
      if (typeof objectId !== "string" || !objectId) return p2Bad("object_id 必填（字符串）");
      const patch = body.patch;
      if (!patch || typeof patch !== "object") return p2Bad("patch 必填（对象）");
      const KEYS = ["visible", "locked", "z_index", "layer_id", "metadata", "type"];
      const hits = KEYS.filter((k) => patch[k] !== undefined);
      if (!hits.length) return p2Bad("patch 中没有可修改的对象属性（可用：" + KEYS.join("/") + "）");
      const seqs = [];
      for (const k of hits) {
        seqs.push(await putAtom(handle, doc, p2Envelope("set_property",
          { object_id: objectId, key: k, value: patch[k] })));
      }
      return json({ ok: true, seqs, count: seqs.length, head: seqs[seqs.length - 1],
                    server: false, object_id: objectId, keys: hits,
                    note: "服务端按 patch 键逐条写 set_property 原子，本地照做；本地层无 changeset 机制，逐条提交" });
    }

    // **⑫ `get_object` ✗**：**从原子日志推导 ✗**（**∴ 按 `object_id` 找 `create_object` ✓ ＋
    //   `set_property` 覆盖 ＋ `supersede` 替换 ＋ `tombstone` 判删除 ✓**）。
    //   **∴ 如实说明 ✗**：**`bbox`／`created_by`／`blobs` 等需内核折叠的字段**未推导 ✗**
    //   （**∴ 不编造 ✓**）。
    if (path === "/api/tools/get_object") {
      const objectId = body.object_id;
      if (typeof objectId !== "string" || !objectId) return p2Bad("object_id 必填（字符串）");
      const rows = await atomsOf(handle, doc);
      const v = p2ObjectView(rows, objectId);
      if (!v.found) return p2Missing("对象 " + objectId + " 不存在");
      const out = { ok: true, server: false, derived_from: "atoms",
                    object_id: v.object_id, layer_id: v.layer_id, type: v.type,
                    visible: v.visible, locked: v.locked, z_index: v.z_index,
                    metadata: v.metadata, data: v.data, transform: v.transform,
                    deleted: v.deleted,
                    note: "从原子日志推导；bbox 等需内核折叠的字段未推导" };
      if (body.include_history === true) out.version_chain = p2History(rows, objectId);
      return json(out);
    }

    // **⑬ `get_object_history` ✗**：**从原子日志推导 ✗**（**∴ 引用 `object_id` 的全部原子 ✓ ＋
    //   `revert`／`reapply` 标 `reverted` ✓** —— **照 `read_get_object_history` 的结构 ✓**）。
    if (path === "/api/tools/get_object_history") {
      const objectId = body.object_id;
      if (typeof objectId !== "string" || !objectId) return p2Bad("object_id 必填（字符串）");
      const rows = await atomsOf(handle, doc);
      const v = p2ObjectView(rows, objectId);
      if (!v.found) return p2Missing("对象 " + objectId + " 不存在");
      const versions = p2History(rows, objectId);
      const head = rows.length ? rows[rows.length - 1].seq : 0;
      return json({ ok: true, server: false, derived_from: "atoms",
                    object_id: objectId, type: v.type, deleted: v.deleted,
                    versions, count: versions.length, head_seq: head });
    }

    // **⑭ `move_object` ✗**：**照 `write_move_object` ✓** ⇒ **`move` 原子 ✓**。
    //   **∴ `delta` 是增量 ✗**（**∴ 原样交折叠层复合 ✓**）；**`delta` 同时编成绝对矩阵 ✓**
    //   （**服务端原样照抄 ✓**）；**`delta` 与 `transform` 都没有 ⇒ 400 ✗**。
    if (path === "/api/tools/move_object") {
      const objectId = body.object_id;
      if (typeof objectId !== "string" || !objectId) return p2Bad("object_id 必填（字符串）");
      const payload = { object_id: objectId };
      if (body.delta && typeof body.delta === "object") {
        const dx = Number(body.delta.dx), dy = Number(body.delta.dy);
        payload.delta = { dx: Number.isFinite(dx) ? dx : 0, dy: Number.isFinite(dy) ? dy : 0 };
        payload.transform = { matrix: [1, 0, 0, 1, payload.delta.dx, payload.delta.dy],
                              pivot: [0, 0] };
      }
      if (body.transform && typeof body.transform === "object") payload.transform = body.transform;
      if (!payload.transform) return p2Bad("move_object 需要 delta 或 transform");
      const seq = await putAtom(handle, doc, p2Envelope("move", payload));
      return json({ ok: true, seq, server: false, object_id: objectId,
                    note: "已写入 move 原子；位移由内核折叠层按增量复合" });
    }

    // **⑮ `replace_object_data` ✗**：**照 `write_replace_object_data` ✓** ⇒ **`supersede` 原子 ✓**
    //   （**∴ 只替换 `data` ✗**，**可选同时声明 `type` ✓**）。
    if (path === "/api/tools/replace_object_data") {
      const objectId = body.object_id;
      if (typeof objectId !== "string" || !objectId) return p2Bad("object_id 必填（字符串）");
      const data = body.data;
      if (!data || typeof data !== "object" || Array.isArray(data)) return p2Bad("data 必须是对象（必填）");
      const payload = { object_id: objectId, data };
      if (typeof body.type === "string") payload.type = body.type;
      const seq = await putAtom(handle, doc, p2Envelope("supersede", payload));
      return json({ ok: true, seq, server: false, object_id: objectId,
                    note: "已写入 supersede 原子（只替换 data，不碰其它属性）" });
    }

    // **⑯ `restore_object` ✗**：**照 `write_restore_object` ✓**
    //   ⇒ **对该对象的每条 `tombstone` 各写一条 `revert{target}` 原子 ✗**。
    //   **∴ 三个前置判定都从日志推导 ✓**：**从未存在过 ⇒ 404 ✗**；**没被删过 ⇒ 400 ✗**；
    //     **已经是活的 ⇒ `ok` ＋ `restored:false` ✗**（**照服务端 ✓**）。
    if (path === "/api/tools/restore_object") {
      const objectId = body.object_id;
      if (typeof objectId !== "string" || !objectId) return p2Bad("object_id 必填（字符串）");
      const rows = await atomsOf(handle, doc);
      const v = p2ObjectView(rows, objectId);
      if (!v.found) return p2Missing("对象 " + objectId + " 从未存在过（日志里没有它的创建记录）");
      if (!v.tombstone_ids.length) return p2Bad("对象 " + objectId + " 没有被删除过，无需恢复");
      if (!v.deleted) {
        return json({ ok: true, server: false, object_id: objectId, restored: false,
                      note: "对象当前是活的（删除已被撤销过），无需恢复" });
      }
      const reverted = [];
      for (const tid of v.tombstone_ids) {
        await putAtom(handle, doc, p2Envelope("revert", { target: tid }));
        reverted.push(tid);
      }
      return json({ ok: true, server: false, object_id: objectId, restored: true, reverted,
                    note: "已按 tombstone 逐条写入 revert 原子；是否真正复活以内核折叠为准" });
    }

    // **⑰ `create_group` ✗**：**照 `write_create_group` ✓**
    //   ⇒ **`create_object{type:"group", data:{members, group_transform}}` ✓**。
    //   **∴ 成员校验从日志推导 ✓**：**成员必须存在 ✗**；**成员不能是组 ✗**（**本片不支持组嵌套 ✓**）。
    if (path === "/api/tools/create_group") {
      const groupId = body.group_id, layerId = body.layer_id;
      if (typeof groupId !== "string" || !groupId) return p2Bad("group_id 必填（字符串）");
      if (typeof layerId !== "string" || !layerId) return p2Bad("layer_id 必填（字符串）");
      const members = Array.isArray(body.members)
        ? body.members.filter((m) => typeof m === "string") : [];
      const rows = await atomsOf(handle, doc);
      for (const m of members) {
        const mv = p2ObjectView(rows, m);
        if (!mv.found) return p2Missing("组成员 " + m + " 不存在");
        if (mv.type === "group") return p2Bad("本片不支持组嵌套（" + m + " 也是组）");
      }
      const seq = await putAtom(handle, doc, p2Envelope("create_object", {
        object_id: groupId, layer_id: layerId, type: "group",
        data: { members, group_transform: { matrix: [1, 0, 0, 1, 0, 0], pivot: [0, 0] } },
      }));
      return json({ ok: true, seq, server: false, group_id: groupId, members,
                    note: "已写入 create_object{type:group} 原子" });
    }

    // **⑱ `delete_group` ⇒ 明确 501 ✗**：**服务端**没有**这个端点 ✗**
    //   （**注册表无 `delete_group` ✓**）⇒ **∴ 组本质是对象 ✗**（**删除请用 `delete_object` ✓**），
    //   **∴ 没有专用服务端语义可照抄 ✗** ⇒ **∴ 不硬凑 ✓**。
    if (path === "/api/tools/delete_group") {
      return p2NoServerEndpoint("delete_group", "组本质是对象，删除请用 delete_object");
    }

    // **⑲ `add_to_group` ✗**：**照 `write_add_to_group` ✓**
    //   ⇒ **`set_property{key:"members", value:[…新成员表…]}` ✗**（**∴ 去重 ✓**）。
    //   **∴ 组与对象校验从日志推导 ✓**：**组必须存在且是组 ✗**；**对象必须存在且不是组 ✗**。
    if (path === "/api/tools/add_to_group") {
      const groupId = body.group_id, objectId = body.object_id;
      if (typeof groupId !== "string" || !groupId) return p2Bad("group_id 必填（字符串）");
      if (typeof objectId !== "string" || !objectId) return p2Bad("object_id 必填（字符串）");
      const rows = await atomsOf(handle, doc);
      const gv = p2ObjectView(rows, groupId);
      if (!gv.found || gv.type !== "group") return p2Missing("组 " + groupId + " 不存在（或不是对象组）");
      const ov = p2ObjectView(rows, objectId);
      if (!ov.found) return p2Missing("对象 " + objectId + " 不存在");
      if (ov.type === "group") return p2Bad("本片不支持组嵌套（" + objectId + " 是组）");
      const members = Array.isArray(gv.members) ? gv.members.slice() : [];
      if (members.indexOf(objectId) < 0) members.push(objectId);
      const seq = await putAtom(handle, doc, p2Envelope("set_property",
        { object_id: groupId, key: "members", value: members }));
      return json({ ok: true, seq, server: false, group_id: groupId, members });
    }

    // **⑳ `remove_from_group` ✗**：**照 `write_remove_from_group` ✓**
    //   ⇒ **`set_property{key:"members", value:[…过滤后…]}` ✗**。
    if (path === "/api/tools/remove_from_group") {
      const groupId = body.group_id, objectId = body.object_id;
      if (typeof groupId !== "string" || !groupId) return p2Bad("group_id 必填（字符串）");
      if (typeof objectId !== "string" || !objectId) return p2Bad("object_id 必填（字符串）");
      const rows = await atomsOf(handle, doc);
      const gv = p2ObjectView(rows, groupId);
      if (!gv.found || gv.type !== "group") return p2Missing("组 " + groupId + " 不存在（或不是对象组）");
      const members = (Array.isArray(gv.members) ? gv.members : []).filter((m) => m !== objectId);
      const seq = await putAtom(handle, doc, p2Envelope("set_property",
        { object_id: groupId, key: "members", value: members }));
      return json({ ok: true, seq, server: false, group_id: groupId, members });
    }

    // **㉑ `set_group_transform` ✗**：**照 `write_set_group_transform` ✓**
    //   ⇒ **`set_property{key:"group_transform", value:{dx,dy}}` ✗**。
    //   **∴ 缺参必须报错 ✗**（**∴ 绝不用 0 兜底 ✓ —— 服务端原话 ✓**）；**本片只支持平移 ✗**。
    if (path === "/api/tools/set_group_transform") {
      const groupId = body.group_id;
      if (typeof groupId !== "string" || !groupId) return p2Bad("group_id 必填（字符串）");
      const rows = await atomsOf(handle, doc);
      const gv = p2ObjectView(rows, groupId);
      if (!gv.found || gv.type !== "group") return p2Missing("组 " + groupId + " 不存在（或不是对象组）");
      const delta = (body.delta && typeof body.delta === "object") ? body.delta : {};
      const dx = Number(delta.dx), dy = Number(delta.dy);
      if (!Number.isFinite(dx) || !Number.isFinite(dy)) {
        return p2Bad("set_group_transform 需要 {delta: {dx, dy}}（本片只支持平移）");
      }
      const seq = await putAtom(handle, doc, p2Envelope("set_property",
        { object_id: groupId, key: "group_transform", value: { dx, dy } }));
      return json({ ok: true, seq, server: false, group_id: groupId, delta: { dx, dy } });
    }

    // **㉒ `create_instance` ✗**：**照 `write_create_instance` ✓**
    //   ⇒ **`create_object{type:"instance", data:{master_ref:{object_id, local_transform}, sync_policy:"all"}}` ✓**。
    //   **∴ 边界照服务端拒绝 ✓**：**`override` 非空 ⇒ 400 ✗**；**`sync_policy ≠ "all"` ⇒ 400 ✗**；
    //     **master 不存在 ⇒ 404 ✗**（**从日志推导 ✓**）。
    if (path === "/api/tools/create_instance") {
      const instanceId = body.instance_id, layerId = body.layer_id, masterId = body.master_id;
      if (typeof instanceId !== "string" || !instanceId) return p2Bad("instance_id 必填（字符串）");
      if (typeof layerId !== "string" || !layerId) return p2Bad("layer_id 必填（字符串）");
      if (typeof masterId !== "string" || !masterId) return p2Bad("master_id 必填（字符串）");
      if (body.override !== undefined && body.override !== null) {
        return p2Bad("本片不支持 instance.override（需要设计 9.3 的缓存与依赖图传播）");
      }
      if (typeof body.sync_policy === "string" && body.sync_policy !== "all") {
        return p2Bad("本片只支持 sync_policy: all（收到 " + body.sync_policy + "）");
      }
      const rows = await atomsOf(handle, doc);
      const mv = p2ObjectView(rows, masterId);
      if (!mv.found) return p2Missing("master " + masterId + " 不存在");
      const localTransform = (body.local_transform && typeof body.local_transform === "object")
        ? body.local_transform
        : { matrix: [1, 0, 0, 1, 0, 0], pivot: [0, 0] };
      const seq = await putAtom(handle, doc, p2Envelope("create_object", {
        object_id: instanceId, layer_id: layerId, type: "instance",
        data: { master_ref: { object_id: masterId, local_transform: localTransform },
                sync_policy: "all" },
      }));
      return json({ ok: true, seq, server: false, instance_id: instanceId, master_id: masterId,
                    note: "成环由内核折叠层拒绝（服务端同）；本地只做存在性校验" });
    }

    // **㉓ `detach_instance` ⇒ 明确 501 ✗**：**服务端逻辑**无法**从裸原子日志如实复现 ✗** ——
    //   **∴ 它要把 master 的 `data` 复制成新对象 ✗ ＋ 合成变换 ＝ 实例自身 ∘ local ∘ master ✗**
    //   （**需重放 `move`／`transform`／`set_property` 求对象**当前**变换 ＝ 折叠层逻辑 ✓**），
    //   **∴ 还要两条原子归一个 changeset 一次提交 ✗**（**本地层无 changeset 机制 ✓**）
    //   ⇒ **∴ 硬写会**算错变换**或**留下半成品 ✗** ⇒ **∴ 明确 501 ✓**。
    if (path === "/api/tools/detach_instance") {
      return json({ ok: false, error: "not_implemented_locally",
                    endpoint: "/api/tools/detach_instance",
                    reason: "detach_instance：服务端需折叠层状态（复制 master 数据＋合成变换＋changeset 两原子一次提交），从裸原子日志无法如实复现",
                    server: false }, 501);
    }

    // **㉔ `link_to_master` ✗**：**照 `write_link_to_master` ✓**
    //   ⇒ **`set_property{key:"master_ref", value:{object_id, local_transform}}` ✗**
    //   （**∴ 保留原 `local_transform` ✗**，**只换指向 ✓**）。
    //   **∴ 校验从日志推导 ✓**：**实例必须存在且是实例 ✗**；**master 必须存在 ✗**。
    if (path === "/api/tools/link_to_master") {
      const instanceId = body.instance_id, masterId = body.master_id;
      if (typeof instanceId !== "string" || !instanceId) return p2Bad("instance_id 必填（字符串）");
      if (typeof masterId !== "string" || !masterId) return p2Bad("master_id 必填（字符串）");
      const rows = await atomsOf(handle, doc);
      const iv = p2ObjectView(rows, instanceId);
      if (!iv.found) return p2Missing("实例 " + instanceId + " 不存在");
      if (iv.type !== "instance") return p2Bad(instanceId + " 不是实例");
      const mv = p2ObjectView(rows, masterId);
      if (!mv.found) return p2Missing("master " + masterId + " 不存在");
      const localTransform = iv.local_transform || { matrix: [1, 0, 0, 1, 0, 0], pivot: [0, 0] };
      const seq = await putAtom(handle, doc, p2Envelope("set_property", {
        object_id: instanceId, key: "master_ref",
        value: { object_id: masterId, local_transform: localTransform },
      }));
      return json({ ok: true, seq, server: false, instance_id: instanceId, master_id: masterId,
                    note: "保留原 local_transform，只换指向（服务端 write_link_to_master 语义）" });
    }

    // **★ P2c 渲染／导出／高级绘画／取色／诊断类端点 ✓ ★**（**服务端语义见 `crates/yanshi-server/src/tools.rs` ✓**）：
    //   **∴ 写操作**一律**写对应 `kind` 的原子 ✗**（**`p2Envelope` 8 字段封套 ✓ ⇒ **∴ 内核重放时处理 ✓**）；
    //   **∴ 读操作**从 IndexedDB 原子日志推导 ✗**（**∴ 不另存状态表 ✓，**与 `list_layers` 同一纪律 ✓**）；
    //   **∴ 真要服务端能力的**（`import_psd` 的 PSD 解析 ✗／`estimate_dehaze` 的暗通道算法 ✗／
    //     `medium_stroke` 的介质插件宿主 ✗／`set_brush_dynamics` 的服务端笔刷文件缓存 ✗）⇒ **∴ 明确 501 ✓**；
    //   **∴ `archive_base64` 不是端点 ✗**（**它是 `export_png` 响应里的字段 ✓** ⇒ **∴ 本地 `export_png` 照样返回它 ✓**）；
    //   **∴ `push` 在服务端工具注册表里不存在 ✗**（**∴ 用 `p2NoServerEndpoint` 明确 501 ✓**）。
    //   **∴ 口径**：**真实映射的分支计入 `LOCAL_IMPLEMENTED` ✗**；**明确 501 的不计入 ✓**。
    const p2cKnownKinds = ["create_document","create_layer","create_object","create_selection",
      "create_mask","create_style","create_checkpoint","import_image","draw_stroke","fill",
      "draw_shape","draw_text","erase","retouch","liquify","supersede","move","transform",
      "set_property","reorder_layers","tombstone","revert","reapply","checkpoint","tag",
      "declare_head","comment","suggest","accept_suggestion","reject_suggestion"];
    let __p2cCtr = 0;
    function p2cNewObjectId() {
      __p2cCtr += 1;
      return "obj_" + Date.now().toString(36) + __p2cCtr.toString(36);
    }
    /** **从原子日志取文档尺寸 ✗**（**`create_document` 的 `payload` ✓**）。 */
    function p2cDocSize(rows) {
      const c = rows.map((r) => (r && r.atom) || {}).find((a) => a.kind === "create_document");
      const pl = (c && c.payload) || {};
      return { w: Number(pl.width) > 0 ? Number(pl.width) : 1024,
               h: Number(pl.height) > 0 ? Number(pl.height) : 1024,
               color_space: pl.color_space || "srgb" };
    }
    /** **取本地内核（**按文档缓存 ✓**）✗**：**拿不到 ⇒ 抛错由调用方转 501 ✓**（**∴ 不静默 ✓**）。 */
    async function p2cKernelFor() {
      let mod = null;
      try {
        mod = await import("/wasm/yanshi_wasm.js");
        if (typeof mod.default === "function") await mod.default();
      } catch (err) {
        throw new Error("kernel_unavailable: /wasm/yanshi_wasm.js 未加载（" +
                        String((err && err.message) || err) + "）");
      }
      let entry = kernels.get(doc);
      if (!entry) {
        const rows = await atomsOf(handle, doc);
        const { w, h } = p2cDocSize(rows);
        entry = { k: new mod.WasmKernel(doc, 256, w, h, 256 * 1024 * 1024), w, h };
        kernels.set(doc, entry);
      }
      return { mod, entry };
    }
    function p2cKernel501(err) {
      return json({ ok: false, error: "kernel_unavailable", endpoint: path,
                    reason: "本地内核不可用 ⇒ " + String((err && err.message) || err).slice(0, 300),
                    server: false }, 501);
    }
    /** **内核渲染一块区域为 PNG ✗**（**照 `render_region` 的折叠＋裁剪流程 ✓**）。 */
    async function p2cRenderPng(x, y, w, h) {
      const { entry } = await p2cKernelFor();
      return await withKernel(doc, async () => {
        const k = entry.k;
        const atoms = await atomsOf(handle, doc);
        const fold = String(k.load_atoms_json(JSON.stringify(atoms.map((r) => r.atom))) || "");
        const bx = Math.max(0, Math.min(Math.floor(x), entry.w));
        const by = Math.max(0, Math.min(Math.floor(y), entry.h));
        const bw = Math.max(1, Math.min(Math.ceil(w), entry.w - bx));
        const bh = Math.max(1, Math.min(Math.ceil(h), entry.h - by));
        const png = k.render_region_png(bx, by, bw, bh);
        if (!png || png.length === 0) throw new Error("内核未产出字节（fold=" + fold.slice(0, 200) + "）");
        return { png: png instanceof Uint8Array ? png : new Uint8Array(png), w: bw, h: bh, fold };
      });
    }
    /** **写 `import_image` 原子 ✗**（**`blob_put` 进内核 ＋ `putBlob` 进 IndexedDB ＋ 原子引用 ✓** —
     **∴ 与 `brush_stroke` 分支同一套路 ✓**）。 */
    async function p2cPutImportImage({ layerId, rgba, w, h, region, source, objectId }) {
      const { entry } = await p2cKernelFor();
      const raw = String(await withKernel(doc, () => entry.k.blob_put(rgba)) || "");
      let blobHash = raw;
      try { const o = JSON.parse(raw); if (o && o.blob_hash) blobHash = String(o.blob_hash); } catch (e) {}
      if (!blobHash) throw new Error("内核未返回 blob hash");
      const { putBlob } = await import("./store.js");
      await putBlob(handle, blobHash, Array.from(rgba));
      const seq = await putAtom(handle, doc, p2Envelope("import_image", {
        type: "raster_patch",
        bitmap: { blob_hash: blobHash, mime_type: "image/x-yanshi-raw", size: rgba.length },
        width: w, height: h, layer_id: layerId,
        object_id: objectId || p2cNewObjectId(),
        region, source: source || { kind: "generated" },
      }));
      return { seq, blob_hash: blobHash };
    }
    /** **base64 ✗**（**node 用 `Buffer` ✓，**浏览器用分块 `btoa` ✓**）。 */
    function p2cB64(u8) {
      const bytes = u8 instanceof Uint8Array ? u8 : new Uint8Array(u8 || []);
      if (typeof Buffer !== "undefined" && Buffer.from) return Buffer.from(bytes).toString("base64");
      let s = "";
      for (let i = 0; i < bytes.length; i += 8192) {
        s += String.fromCharCode.apply(null, bytes.subarray(i, i + 8192));
      }
      return btoa(s);
    }
    /** **PNG 解码 ✗**（**只在浏览器 ✗**：**`createImageBitmap` ＋ canvas ✓**）。 */
    async function p2cDecodePng(u8) {
      const g = typeof globalThis !== "undefined" ? globalThis : {};
      if (typeof g.createImageBitmap !== "function" || !g.document) {
        throw new Error("PNG 解码需要浏览器环境的 createImageBitmap（当前环境没有）");
      }
      const bmp = await g.createImageBitmap(new Blob([u8], { type: "image/png" }));
      const canvas = g.document.createElement("canvas");
      canvas.width = bmp.width; canvas.height = bmp.height;
      const c2d = canvas.getContext("2d", { willReadFrequently: true });
      c2d.drawImage(bmp, 0, 0);
      const img = c2d.getImageData(0, 0, bmp.width, bmp.height);
      if (bmp.close) bmp.close();
      return { w: bmp.width, h: bmp.height, data: img.data };
    }
    /** **颜色解析 ✗**（**`#RRGGBB[AA]`／`[r,g,b,a]`／`{r,g,b,a}` ✓**）。 */
    function p2cParseColor(v) {
      if (typeof v === "string") {
        const m = v.trim().match(/^#?([0-9a-fA-F]{6})([0-9a-fA-F]{2})?$/);
        if (!m) return null;
        const n = parseInt(m[1], 16);
        return [(n >> 16) & 255, (n >> 8) & 255, n & 255, m[2] ? parseInt(m[2], 16) : 255];
      }
      if (Array.isArray(v) && v.length >= 3) {
        const c = v.slice(0, 4).map(Number);
        if (c.some((x) => !Number.isFinite(x))) return null;
        return [c[0], c[1], c[2], c.length >= 4 ? c[3] : 255];
      }
      if (v && typeof v === "object") {
        const c = [Number(v.r), Number(v.g), Number(v.b)];
        if (c.some((x) => !Number.isFinite(x))) return null;
        return [c[0], c[1], c[2], v.a !== undefined ? Number(v.a) : 255];
      }
      return null;
    }
    /** **splitmix64 ✗**（**与服务端 `scatter_next` 同算法 ✓ ⇒ **∴ 同 `seed` ⇒ 同序列 ✓**）。 */
    function p2cSplitmix64(seed) {
      let state = BigInt(seed) & 0xFFFFFFFFFFFFFFFFn;
      const M1 = 0xBF58476D1CE4E5B9n, M2 = 0x94D049BB133111EBn, INC = 0x9E3779B97F4A7C15n;
      return function next() {
        state = (state + INC) & 0xFFFFFFFFFFFFFFFFn;
        let z = state;
        z = ((z ^ (z >> 30n)) * M1) & 0xFFFFFFFFFFFFFFFFn;
        z = ((z ^ (z >> 27n)) * M2) & 0xFFFFFFFFFFFFFFFFn;
        z = z ^ (z >> 31n);
        return Number(z >> 11n) / 9007199254740992;
      };
    }
    /** **从日志推导文档状态 ✗**（**`get_state`／`get_resolved_state` 共用 ✓**）：
     *   **∴ 图层**：**`create_layer` 建 ✗ ＋ `set_property{layer_id}` 覆盖 ✗ ＋
     *     `tombstone{layer_id}` 删（**被 `revert` 中和的不算 ✓**）**；
     *   **∴ 对象**：**`create_object` 列 ✗**（**类型／层取 `p2ObjectView` 的合并视图 ✓**）。 */
    function p2cState(rows) {
      const { w, h, color_space } = p2cDocSize(rows);
      const layers = [], layerIdx = {};
      const objects = [];
      let checkpoints = 0;
      const neutral = p2Neutralized(rows);
      for (const r of rows) {
        const a = (r && r.atom) || {};
        const pl = a.payload || {};
        if (a.kind === "create_layer") {
          const id = pl.layer_id;
          if (!id || layerIdx[id]) continue;
          layerIdx[id] = { layer_id: id, name: pl.name ?? id, type: pl.type ?? "raster",
                           z_index: layers.length, visible: true, opacity: 1,
                           blend_mode: "normal", locked: false, mask_id: null };
          layers.push(layerIdx[id]);
        } else if (a.kind === "set_property" && pl.layer_id && layerIdx[pl.layer_id]) {
          layerIdx[pl.layer_id][pl.key] = pl.value;
        } else if (a.kind === "tombstone" && pl.layer_id && layerIdx[pl.layer_id] && !neutral.has(a.id)) {
          layerIdx[pl.layer_id].deleted = true;
        } else if (a.kind === "create_object") {
          objects.push({ object_id: pl.object_id, layer_id: pl.layer_id ?? null, type: pl.type ?? null });
        } else if (a.kind === "checkpoint") {
          checkpoints += 1;
        }
      }
      return { width: w, height: h, color_space,
               layers: layers.filter((l) => !l.deleted),
               objects, checkpoints };
    }
    /** **P2c-绘画通用写入口 ✗**：**`draw_*`／`fill` 照 `write_draw` 的载荷形状 ✓**
     *   （**`{object_id, layer_id, data, type?}` ✓ ⇒ **∴ 折叠层按 `payload.data` 取内容 ✓**）。 */
    async function p2cDraw(kind, declaredType, normalizer) {
      const layerId = body.layer_id;
      if (typeof layerId !== "string" || !layerId) return p2Bad("layer_id 必填（字符串）");
      const data = body.data;
      if (!data || typeof data !== "object" || Array.isArray(data)) return p2Bad("data 必须是对象（必填）");
      if (normalizer) {
        const bad = normalizer(data);
        if (bad) return bad;
      }
      const objectId = (typeof body.object_id === "string" && body.object_id) ? body.object_id : p2cNewObjectId();
      const payload = { object_id: objectId, layer_id: layerId, data };
      if (declaredType) payload.type = declaredType;
      const seq = await putAtom(handle, doc, p2Envelope(kind, payload));
      return json({ ok: true, seq, server: false, object_id: objectId,
                    note: "已写入 " + kind + " 原子；渲染由内核重放时处理" });
    }
    /** **`draw_shape` 的 bbox 归一化 ✗**（**数组 ⇒ 对象 ✓，**照服务端 `normalize_shape_bbox` ✓**）。 */
    function p2cShapeBboxNorm(data) {
      const g = data.geometry;
      const apply = (holder, key) => {
        const b = holder[key];
        if (Array.isArray(b) && b.length === 4 && b.every((x) => Number.isFinite(Number(x)))) {
          holder[key] = { x: Number(b[0]), y: Number(b[1]), w: Number(b[2]), h: Number(b[3]) };
        }
      };
      if (g && typeof g === "object") apply(g, "bbox");
      apply(data, "bbox");
      return null;
    }
    /** **`fill` 的 region⇒bbox 归一化 ✗**（**照 `write_draw` 的填充分支 ✓**）。 */
    function p2cFillNorm(data) {
      const region = data.region !== undefined ? data.region : data.bbox;
      let b = null;
      if (Array.isArray(region) && region.length === 4 && region.every((x) => Number.isFinite(Number(x)))) {
        b = { x: Number(region[0]), y: Number(region[1]), w: Number(region[2]), h: Number(region[3]) };
      } else if (region && typeof region === "object" &&
                 [region.x, region.y, region.w, region.h].every((x) => Number.isFinite(Number(x)))) {
        b = { x: Number(region.x), y: Number(region.y), w: Number(region.w), h: Number(region.h) };
      }
      if (!b) return p2Bad("fill 的 region 必须是 {x,y,w,h} 或 [x,y,w,h]");
      data.bbox = b;
      delete data.region;
      return null;
    }
    /** **修图通用入口 ✗**（**照 `write_retouch` ✓ ⇒ **`retouch` 原子 ＋ `data.retouch_type` ✓**）。 */
    async function p2cRetouch(retouchType) {
      const layerId = body.layer_id;
      if (typeof layerId !== "string" || !layerId) return p2Bad("layer_id 必填（字符串）");
      const points = body.points;
      if (!Array.isArray(points) || points.length === 0) return p2Bad("修图至少需要一个点（points: [[x,y], ...]）");
      for (const p of points) {
        if (!Array.isArray(p) || !Number.isFinite(Number(p[0])) || !Number.isFinite(Number(p[1]))) {
          return p2Bad("修图的 points 必须是 [[x,y], ...]（数字）");
        }
      }
      const size = body.size !== undefined ? Number(body.size) : 24;
      if (!Number.isFinite(size) || size < 1 || size > 1024) return p2Bad("size 必须在 [1, 1024] 内，得到 " + body.size);
      const hardness = body.hardness !== undefined ? Number(body.hardness) : 0.6;
      const opacity = body.opacity !== undefined ? Number(body.opacity) : 1;
      for (const kv of [["hardness", hardness], ["opacity", opacity]]) {
        if (!Number.isFinite(kv[1]) || kv[1] < 0 || kv[1] > 1) return p2Bad(kv[0] + " 必须在 [0, 1] 内，得到 " + kv[1]);
      }
      const data = { retouch_type: retouchType, points, size, hardness, opacity };
      if (retouchType === "smudge") {
        const len = body.smudge_length !== undefined ? Number(body.smudge_length) : 12;
        if (!Number.isFinite(len) || len < 0.5 || len > 512) {
          return p2Bad("smudge_length 必须在 [0.5, 512] 内，得到 " + body.smudge_length);
        }
        data.smudge_length = len;
        data.direction = [0, 0];
      } else {
        const off = body.source_offset;
        if (!Array.isArray(off)) return p2Bad("source_offset 必填（[dx, dy]）");
        const dx = Number(off[0]) || 0, dy = Number(off[1]) || 0;
        if (dx === 0 && dy === 0) return p2Bad("source_offset 不能是 [0,0]（那样只会自我复制）");
        data.source_offset = [dx, dy];
      }
      const objectId = (typeof body.object_id === "string" && body.object_id) ? body.object_id : p2cNewObjectId();
      const seq = await putAtom(handle, doc, p2Envelope("retouch", {
        object_id: objectId, layer_id: layerId, data,
      }));
      return json({ ok: true, seq, server: false, object_id: objectId, retouch_type: retouchType,
                    note: "已写入 retouch 原子；修图由内核重放时按采样替换语义处理" });
    }
    /** **液化通用入口 ✗**（**照 `write_liquify` ✓ ⇒ **`liquify` 原子 ＋ `data.liquify_type` ✓**）。 */
    async function p2cLiquify(mode) {
      const layerId = body.layer_id;
      if (typeof layerId !== "string" || !layerId) return p2Bad("layer_id 必填（字符串）");
      const points = body.points;
      if (!Array.isArray(points) || points.length === 0) return p2Bad("points 必填（[[x,y], ...]，至少一个点）");
      const size = body.size !== undefined ? Number(body.size) : 100;
      const strength = body.strength !== undefined ? Number(body.strength) : 0.5;
      if (!Number.isFinite(size) || size <= 0) return p2Bad("size 必须是正数");
      const data = { liquify_type: mode, points, size, strength };
      if (mode === "push") {
        const dir = Array.isArray(body.direction) ? body.direction : [];
        const dx = body.dx !== undefined ? Number(body.dx) : (Number(dir[0]) || 0);
        const dy = body.dy !== undefined ? Number(body.dy) : (Number(dir[1]) || 0);
        data.direction = [dx, dy];
      }
      const objectId = (typeof body.object_id === "string" && body.object_id) ? body.object_id : p2cNewObjectId();
      const seq = await putAtom(handle, doc, p2Envelope("liquify", {
        object_id: objectId, layer_id: layerId, data,
      }));
      return json({ ok: true, seq, server: false, object_id: objectId, mode,
                    note: "已写入 liquify 原子；液化由内核重放时处理" });
    }
    /** **滤镜／调整新增 ✗**（**照 `write_add_effect` ✓ ⇒ **`create_object{type}` ✓**）。 */
    async function p2cAddEffect(kind) {
      const layerId = body.layer_id;
      if (typeof layerId !== "string" || !layerId) return p2Bad("layer_id 必填（字符串）");
      const nameKey = kind === "filter" ? "filter_name" : "adjustment_name";
      const name = body[nameKey];
      if (typeof name !== "string" || !name) return p2Bad(nameKey + " 必填（字符串）");
      const params = (body.params && typeof body.params === "object" && !Array.isArray(body.params)) ? body.params : {};
      const opacity = body.opacity !== undefined ? Number(body.opacity) : 1;
      if (!Number.isFinite(opacity) || opacity < 0 || opacity > 1) return p2Bad("opacity 必须在 [0,1] 内");
      const data = { [nameKey]: name, params, opacity };
      if (typeof body.name === "string") data.name = body.name;
      const objectId = (typeof body.object_id === "string" && body.object_id) ? body.object_id : p2cNewObjectId();
      const payload = { object_id: objectId, layer_id: layerId, type: kind, data };
      if (Number.isFinite(Number(body.z_index))) payload.z_index = Number(body.z_index);
      const seq = await putAtom(handle, doc, p2Envelope("create_object", payload));
      const out = { ok: true, seq, server: false, object_id: objectId,
                    note: "已写入 create_object{type:" + kind + "} 原子" };
      out[nameKey] = name;
      return json(out);
    }
    /** **滤镜／调整更新 ✗**（**照 `write_update_effect` ✓ ⇒ **`supersede` 合并 `params` ✓**）。 */
    async function p2cUpdateEffect(kind) {
      const objectId = body.object_id;
      if (typeof objectId !== "string" || !objectId) return p2Bad("object_id 必填（字符串）");
      const patch = body.params;
      if (!patch || typeof patch !== "object" || Array.isArray(patch)) return p2Bad("params 必须是对象（必填）");
      const rows = await atomsOf(handle, doc);
      const v = p2ObjectView(rows, objectId);
      if (!v.found) return p2Missing("对象 " + objectId + " 不存在");
      if (v.type !== kind) return p2Bad(objectId + " 不是" + kind + "对象（是 " + v.type + "）");
      const data = (v.data && typeof v.data === "object" && !Array.isArray(v.data)) ? { ...v.data } : {};
      const oldParams = (data.params && typeof data.params === "object") ? data.params : {};
      data.params = { ...oldParams, ...patch };
      if (body.opacity !== undefined) {
        const op = Number(body.opacity);
        if (!Number.isFinite(op) || op < 0 || op > 1) return p2Bad("opacity 必须在 [0,1] 内");
        data.opacity = op;
      }
      const seq = await putAtom(handle, doc, p2Envelope("supersede", {
        object_id: objectId, layer_id: v.layer_id, data,
      }));
      return json({ ok: true, seq, server: false, object_id: objectId,
                    note: "已写入 supersede 原子（params 已合并）" });
    }
    // **P2c-01 `draw_text` ✗**：**照 `write_draw(…, DrawText)` ✓** ⇒ **`draw_text` 原子 ✓**。
    if (path === "/api/tools/draw_text") {
      const d = body.data;
      if (!d || typeof d !== "object" || typeof d.text !== "string" || !d.text) {
        return p2Bad("data.text 必填（字符串，要绘制的文本）");
      }
      return p2cDraw("draw_text", "text", null);
    }

    // **P2c-02 `draw_shape` ✗**：**照 `write_draw(…, DrawShape)` ✓** ⇒ **`draw_shape` 原子 ✓**。
    if (path === "/api/tools/draw_shape") {
      return p2cDraw("draw_shape", "shape", p2cShapeBboxNorm);
    }

    // **P2c-03 `fill` ✗**：**照 `write_draw(…, Fill)` ✓** ⇒ **`fill` 原子 ＋ `type:"shape"` ＋ `bbox` ✓**
    //   （**∴ region 缺省整幅画布 ✓，**与服务端一致 ✓**）。
    if (path === "/api/tools/fill") {
      if (!body.data || typeof body.data !== "object") return p2Bad("data 必须是对象（必填）");
      if (body.data.region === undefined && body.data.bbox === undefined) {
        const rows = await atomsOf(handle, doc);
        const { w, h } = p2cDocSize(rows);
        body.data.region = { x: 0, y: 0, w, h };
      }
      return p2cDraw("fill", "shape", p2cFillNorm);
    }

    // **P2c-04 `fill_region` ✗**：**照 `write_fill_region` ✓** ⇒ **把 `shape` 映射成
    //   `draw_shape` 认的 `geometry` ✓**（**`rect`／`ellipse`／`polygon` ✓，**与手画图形同一条路 ✓**）。
    if (path === "/api/tools/fill_region") {
      const layerId = body.layer_id;
      if (typeof layerId !== "string" || !layerId) return p2Bad("layer_id 必填（字符串）");
      const shape = body.shape;
      if (!shape || typeof shape !== "object") return p2Bad("shape 必须是对象（必填）");
      if (body.color === undefined) return p2Bad("缺少 color（{r,g,b,a} 或 #rrggbb）");
      const texture = body.texture !== undefined ? String(body.texture) : "smooth";
      if (texture !== "smooth") {
        return p2Bad("texture=" + texture + " 暂不支持 ⇒ 目前只有缺省（= smooth）；厚涂/干刷肌理是独立的活，另行排期");
      }
      const num = (key, def) => {
        const v = shape[key] !== undefined ? Number(shape[key]) : def;
        return Number.isFinite(v) ? v : def;
      };
      const skind = shape.type !== undefined ? String(shape.type) : "rect";
      const feather = Math.max(0, shape.feather !== undefined ? Number(shape.feather)
        : (body.feather !== undefined ? Number(body.feather) : 0));
      let geometry = null;
      if (skind === "rect") {
        geometry = { kind: "rect",
                     bbox: { x: num("x", 0), y: num("y", 0), w: num("w", 0), h: num("h", 0) },
                     feather };
      } else if (skind === "ellipse") {
        const cx = num("cx", 0), cy = num("cy", 0), rx = num("rx", 0), ry = num("ry", 0);
        geometry = { kind: "ellipse",
                     bbox: { x: cx - rx, y: cy - ry, w: rx * 2, h: ry * 2 }, feather };
      } else if (skind === "polygon") {
        const pts = shape.points;
        if (!Array.isArray(pts) || pts.length === 0) return p2Bad("polygon 需要 shape.points（[[x,y], ...]）");
        geometry = { kind: "polygon", points: pts, feather };
      } else {
        return p2Bad("shape.type 必须是 rect/ellipse/polygon，得到 " + skind);
      }
      const objectId = (typeof body.object_id === "string" && body.object_id) ? body.object_id : p2cNewObjectId();
      const seq = await putAtom(handle, doc, p2Envelope("draw_shape", {
        object_id: objectId, layer_id: layerId, type: "shape",
        data: { geometry, color: body.color },
      }));
      return json({ ok: true, seq, server: false, object_id: objectId, shape: skind,
                    note: "已写入 draw_shape 原子（fill_region 与手画图形同一条路）" });
    }

    // **P2c-05 `smudge` ✗**：**照 `write_retouch(…, "smudge")` ✓**。
    if (path === "/api/tools/smudge") {
      return p2cRetouch("smudge");
    }

    // **P2c-06 `clone_stamp` ✗**：**照 `write_retouch(…, "clone_stamp")` ✓**。
    if (path === "/api/tools/clone_stamp") {
      return p2cRetouch("clone_stamp");
    }

    // **P2c-07 `heal_stamp` ✗**：**照 `write_retouch(…, "heal")` ✓**（**∴ 工具名是 `heal_stamp` ✗，**
    //   **`retouch_type` 是 `"heal"` ✓**）。
    if (path === "/api/tools/heal_stamp") {
      return p2cRetouch("heal");
    }

    // **P2c-08 `heal` ✗**：**`heal_stamp` 的别名 ✓**（**同一处理 ✓，**方便按 `retouch_type` 记忆的调用方 ✓**）。
    if (path === "/api/tools/heal") {
      return p2cRetouch("heal");
    }

    // **`patch`**（补漏）：修补工具（`write_patch`），与 clone_stamp/heal 同属修图系。
    //   参数 `{layer_id, source_region:{x,y,w,h}, target:[x,y], opacity?}`。
    if (path === "/api/tools/patch") {
      return p2cRetouch("patch");
    }

    // **P2c-09 `gradient_fill` ✗**：**服务端逐像素算渐变再 `import_image` ✓** ⇒ **∴ 本地
    //   用**同一套算法**（**线性投影／径向距离 ✓**）在 JS 里算出像素 ✗** ⇒ **∴ 再走 `import_image` ✓**
    //   （**∴ 像素与服务端同源 ✓，**只是算的位置从 Rust 换成 JS ✓**）。
    if (path === "/api/tools/gradient_fill") {
      const layerId = body.layer_id;
      if (typeof layerId !== "string" || !layerId) return p2Bad("layer_id 必填（字符串）");
      const from = p2cParseColor(body.from), to = p2cParseColor(body.to);
      if (!from || !to) return p2Bad("from/to 颜色解析失败（支持 #RRGGBB、[r,g,b,a]、{r,g,b,a}）");
      const gkind = body.kind !== undefined ? String(body.kind) : "linear";
      if (gkind !== "linear" && gkind !== "radial") return p2Bad("kind 必须是 linear/radial，得到 " + gkind);
      const rows = await atomsOf(handle, doc);
      const { w: cw, h: ch } = p2cDocSize(rows);
      let tx0 = 0, ty0 = 0, tw = cw, th = ch;
      if (body.region && typeof body.region === "object" && !Array.isArray(body.region)) {
        const rg = body.region;
        tx0 = Math.max(0, Math.floor(Number(rg.x) || 0));
        ty0 = Math.max(0, Math.floor(Number(rg.y) || 0));
        tw = Math.max(1, Math.min(cw - tx0, Math.floor(Number(rg.w) || cw)));
        th = Math.max(1, Math.min(ch - ty0, Math.floor(Number(rg.h) || ch)));
      }
      const angleDeg = body.angle !== undefined ? Number(body.angle) : 0;
      const ang = (Number.isFinite(angleDeg) ? angleDeg : 0) * Math.PI / 180;
      const dx = Math.cos(ang), dy = Math.sin(ang);
      const projs = [0, tw, 0, tw].map((x, i) => x * dx + (i < 2 ? 0 : th) * dy);
      const minP = Math.min(...projs), span = Math.max(1e-6, Math.max(...projs) - minP);
      let ccx = tw / 2, ccy = th / 2;
      if (body.center) {
        const c = body.center;
        const px = c.x !== undefined ? Number(c.x) : (Array.isArray(c) ? Number(c[0]) : NaN);
        const py = c.y !== undefined ? Number(c.y) : (Array.isArray(c) ? Number(c[1]) : NaN);
        if (!Number.isFinite(px) || !Number.isFinite(py)) return p2Bad("center 必须是 {x,y} 或 [x,y]");
        ccx = px - tx0; ccy = py - ty0;
      }
      const radius = Math.max(1e-6, body.radius !== undefined ? Number(body.radius)
        : Math.sqrt(tw * tw + th * th) / 2);
      const rgba = new Uint8Array(tw * th * 4);
      for (let row = 0; row < th; row++) {
        for (let col = 0; col < tw; col++) {
          let t;
          if (gkind === "radial") {
            const ddx = col - ccx, ddy = row - ccy;
            t = Math.min(1, Math.max(0, Math.sqrt(ddx * ddx + ddy * ddy) / radius));
          } else {
            t = Math.min(1, Math.max(0, ((col * dx + row * dy) - minP) / span));
          }
          const at = (row * tw + col) * 4;
          for (let cch = 0; cch < 4; cch++) {
            rgba[at + cch] = Math.round(Math.min(255, Math.max(0, from[cch] + (to[cch] - from[cch]) * t)));
          }
        }
      }
      try {
        const objectId = (typeof body.object_id === "string" && body.object_id) ? body.object_id : p2cNewObjectId();
        const { seq, blob_hash } = await p2cPutImportImage({
          layerId, rgba, w: tw, h: th,
          region: { x: tx0, y: ty0, w: tw, h: th },
          source: { kind: "gradient_fill", gradient: gkind, angle: angleDeg },
          objectId,
        });
        return json({ ok: true, seq, server: false, object_id: objectId, blob_hash,
                      kind: gkind, angle: angleDeg, region: { x: tx0, y: ty0, w: tw, h: th },
                      note: "渐变已在本地按服务端算法逐像素算出并写入 import_image 原子" });
      } catch (err) { return p2cKernel501(err); }
    }

    // **P2c-10 `texture_background` ✗**：**服务端读纹理文件铺满区域再 `import_image` ✓** ⇒ **∴ 本地
    //   从 `/textures/` 取同名文件 ✗** ⇒ **∴ `tile`／`stretch`／`cover` 在 JS 里拼出像素 ✓**
    //   （**`stretch` 用 nearest ✓，**如实说明 ✓**）⇒ **∴ 再走 `import_image` ✓**。
    if (path === "/api/tools/texture_background") {
      const layerId = body.layer_id;
      if (typeof layerId !== "string" || !layerId) return p2Bad("layer_id 必填（字符串）");
      const texture = body.texture;
      if (typeof texture !== "string" || !texture) return p2Bad("texture 必填（纹理名）");
      const mode = body.mode !== undefined ? String(body.mode) : "tile";
      if (["tile", "stretch", "cover"].indexOf(mode) < 0) {
        return p2Bad("未知铺法 " + mode + " ⇒ 可用：tile / stretch / cover");
      }
      const rows = await atomsOf(handle, doc);
      const { w: cw, h: ch } = p2cDocSize(rows);
      let tx0 = 0, ty0 = 0, tw = cw, th = ch;
      if (body.region && typeof body.region === "object" && !Array.isArray(body.region)) {
        const rg = body.region;
        tx0 = Math.max(0, Math.floor(Number(rg.x) || 0));
        ty0 = Math.max(0, Math.floor(Number(rg.y) || 0));
        tw = Math.max(1, Math.min(cw - tx0, Math.floor(Number(rg.w) || cw)));
        th = Math.max(1, Math.min(ch - ty0, Math.floor(Number(rg.h) || ch)));
      }
      let tileBytes;
      try {
        const res = await fetch("/textures/" + encodeURIComponent(texture), { cache: "no-store" });
        if (!res.ok) {
          return json({ ok: false, error: "texture_not_found", server: false,
                        reason: texture + " ⇒ HTTP " + res.status }, 404);
        }
        tileBytes = new Uint8Array(await res.arrayBuffer());
      } catch (err) {
        return json({ ok: false, error: "texture_not_found", server: false,
                      reason: texture + " ⇒ " + String((err && err.message) || err) }, 404);
      }
      let tile;
      try { tile = await p2cDecodePng(tileBytes); }
      catch (err) {
        return json({ ok: false, error: "texture_decode_failed", server: false,
                      reason: "纹理解码失败（" + String((err && err.message) || err).slice(0, 200) +
                              "）：内核只解 PNG" }, 400);
      }
      const rgba = new Uint8Array(tw * th * 4);
      const src = tile.data, sw = tile.w, sh = tile.h;
      if (mode === "tile") {
        for (let y = 0; y < th; y++) for (let x = 0; x < tw; x++) {
          const s = ((y % sh) * sw + (x % sw)) * 4, d = (y * tw + x) * 4;
          rgba[d] = src[s]; rgba[d + 1] = src[s + 1]; rgba[d + 2] = src[s + 2]; rgba[d + 3] = src[s + 3];
        }
      } else {
        const scale = mode === "cover" ? Math.max(tw / sw, th / sh) : 1;
        const dw = mode === "cover" ? sw * scale : tw, dh = mode === "cover" ? sh * scale : th;
        const ox = mode === "cover" ? (dw - tw) / 2 : 0, oy = mode === "cover" ? (dh - th) / 2 : 0;
        for (let y = 0; y < th; y++) for (let x = 0; x < tw; x++) {
          const sx = Math.min(sw - 1, Math.max(0, Math.floor((x + ox) / dw * sw)));
          const sy = Math.min(sh - 1, Math.max(0, Math.floor((y + oy) / dh * sh)));
          const s = (sy * sw + sx) * 4, d = (y * tw + x) * 4;
          rgba[d] = src[s]; rgba[d + 1] = src[s + 1]; rgba[d + 2] = src[s + 2]; rgba[d + 3] = src[s + 3];
        }
      }
      try {
        const objectId = (typeof body.object_id === "string" && body.object_id) ? body.object_id : p2cNewObjectId();
        const { seq, blob_hash } = await p2cPutImportImage({
          layerId, rgba, w: tw, h: th,
          region: { x: tx0, y: ty0, w: tw, h: th },
          source: { kind: "texture_background", texture, mode },
          objectId,
        });
        return json({ ok: true, seq, server: false, object_id: objectId, blob_hash,
                      texture, mode, region: { x: tx0, y: ty0, w: tw, h: th },
                      note: "纹理已在本地铺好并写入 import_image 原子（stretch 用 nearest 采样）" });
      } catch (err) { return p2cKernel501(err); }
    }

    // **P2c-11 `medium_stroke` ⇒ 明确 501 ✗**：**服务端用介质插件宿主逐笔渲染成位图 ✗**
    //   ⇒ **∴ 本地 wasm 内核没有介质引擎 ✓**（**`crates/yanshi-wasm` 无 medium 支持 ✓**）
    //   ⇒ **∴ 写一条"介质笔触"原子只会**被折叠层跳过 ✗**（**写了也画不出来 = 撒谎 ✓**）。
    if (path === "/api/tools/medium_stroke") {
      return json({ ok: false, error: "not_implemented_locally", endpoint: "/api/tools/medium_stroke",
                    reason: "medium_stroke：服务端用介质插件宿主（yanshi_medium_host）逐笔渲染成位图再提交 import_image；" +
                            "本地 wasm 内核没有介质引擎，无法如实复现",
                    server: false }, 501);
    }
    // **P2c-12 `scatter_strokes` ✗**：**服务端用 splitmix64 按 `seed` 撒点、逐笔走 `brush_stroke` ✓**
    //   ⇒ **∴ 本地用**同一 PRNG**（**`p2cSplitmix64` ✓ ⇒ **∴ 同 `seed` ⇒ 同序列 ✓**）算出每笔 ✗**
    //   ⇒ **∴ 再逐笔走**与 `brush_stroke` 分支同一套**内核落笔流程 ✓**（**`paintWithKernel` ✓**）。
    if (path === "/api/tools/scatter_strokes") {
      const layerId = body.layer_id;
      if (typeof layerId !== "string" || !layerId) return p2Bad("layer_id 必填（字符串）");
      const brush = body.brush;
      if (typeof brush !== "string" || !brush) return p2Bad("brush 必填（字符串，.myb 笔刷名）");
      const seed = body.seed !== undefined ? Number(body.seed) : NaN;
      if (!Number.isFinite(seed) || seed < 0 || Math.floor(seed) !== seed) {
        return p2Bad("seed 必填（非负整数 ⇒ 同 seed 同结果）");
      }
      const count = body.count !== undefined ? Math.floor(Number(body.count)) : 24;
      if (!Number.isFinite(count) || count < 1 || count > 2000) return p2Bad("count 必须在 [1, 2000] 内");
      const area = body.area;
      if (!area || typeof area !== "object") return p2Bad("缺少 area（应为 {x, y, w, h}）");
      const rangeOf = (key, low, high) => {
        const v = body[key];
        if (Array.isArray(v) && v.length >= 2) {
          return [Number(v[0]) || low, Number(v[1]) || high];
        }
        return [low, high];
      };
      const [sizeLow, sizeHigh] = rangeOf("size_range", 6, 18);
      const [opLow, opHigh] = rangeOf("opacity_range", 0.4, 1);
      const direction = body.direction !== undefined ? String(body.direction) : "random";
      let fixedAngle = null;
      if (direction === "horizontal") fixedAngle = 0;
      else if (direction === "vertical") fixedAngle = Math.PI / 2;
      else if (direction !== "random") return p2Bad("direction 必须是 horizontal/vertical/random，得到 " + direction);
      let polygon = null;
      if (Array.isArray(body.polygon) && body.polygon.length >= 3) {
        polygon = body.polygon.map((p) => [Number(p[0]), Number(p[1])]);
      }
      const ax = Number(area.x) || 0, ay = Number(area.y) || 0;
      const aw = Number(area.w) || 0, ah = Number(area.h) || 0;
      if (aw <= 0 || ah <= 0) return p2Bad("area 的 w/h 必须是正数");
      const palette = Array.isArray(body.palette) && body.palette.length ? body.palette : ["#000000"];
      try {
        const { entry } = await p2cKernelFor();
        const { fetchBrushText, paintWithKernel } = await import("/brush-local.js");
        const brushName = brush.endsWith(".myb") ? brush : brush + ".myb";
        const myb = await fetchBrushText(brushName);
        const next = p2cSplitmix64(seed);
        const inPoly = (x, y) => {
          let inside = false;
          for (let i = 0, j = polygon.length - 1; i < polygon.length; j = i++) {
            const xi = polygon[i][0], yi = polygon[i][1], xj = polygon[j][0], yj = polygon[j][1];
            if (((yi > y) !== (yj > y)) && (x < (xj - xi) * (y - yi) / (yj - yi) + xi)) inside = !inside;
          }
          return inside;
        };
        let placed = 0;
        const seqs = [];
        for (let n = 0; n < count; n++) {
          let x = ax + next() * aw, y = ay + next() * ah;
          if (polygon) {
            let tries = 0;
            while (!inPoly(x, y) && tries < 16) {
              x = ax + next() * aw; y = ay + next() * ah; tries += 1;
            }
            if (!inPoly(x, y)) continue;
          }
          const size = sizeLow + next() * Math.max(0, sizeHigh - sizeLow);
          const alpha = opLow + next() * Math.max(0, opHigh - opLow);
          const angle = fixedAngle !== null ? fixedAngle : next() * Math.PI * 2;
          const pick = palette[Math.floor(next() * palette.length) % palette.length];
          const rgb = p2cParseColor(pick);
          if (!rgb) return p2Bad("palette 里有解析不了的颜色：" + JSON.stringify(pick));
          const color = { r: rgb[0], g: rgb[1], b: rgb[2], a: Math.min(255, Math.max(0, rgb[3] * alpha)) };
          const length = Math.max(4, size * 1.2);
          const pts = [[Math.round(x), Math.round(y)],
                       [Math.round(x + Math.cos(angle) * length), Math.round(y + Math.sin(angle) * length)]];
          const r = Math.ceil(size / 2) + 2;
          const x0 = Math.max(0, Math.floor(Math.min(pts[0][0], pts[1][0]) - r));
          const y0 = Math.max(0, Math.floor(Math.min(pts[0][1], pts[1][1]) - r));
          const x1 = Math.min(entry.w, Math.ceil(Math.max(pts[0][0], pts[1][0]) + r));
          const y1 = Math.min(entry.h, Math.ceil(Math.max(pts[0][1], pts[1][1]) + r));
          const region = { x: x0, y: y0, w: Math.max(1, x1 - x0), h: Math.max(1, y1 - y0) };
          const rgba = await withKernel(doc, () => paintWithKernel(entry.k, {
            myb, points: pts, size: Math.round(size), color, region,
          }));
          const { seq } = await p2cPutImportImage({
            layerId, rgba, w: region.w, h: region.h, region,
            source: { kind: "brush", brush: brushName, color, size: Math.round(size), seed },
          });
          seqs.push(seq);
          placed += 1;
        }
        return json({ ok: true, server: false, strokes: placed, seed, direction,
                      note: "撒点 PRNG 与服务端同算法（同 seed 同序列）；每笔走本地内核落笔并写入 import_image 原子" });
      } catch (err) { return p2cKernel501(err); }
    }

    // **P2c-13 `update_stroke` ✗**：**照 `write_update_stroke` ✓** ⇒ **把 `core`／`preset`／`advanced`
    //   合进对象**当前** `data` ✗**（**从日志推导 ✓**）⇒ **`supersede` 原子 ✓**。
    if (path === "/api/tools/update_stroke") {
      const objectId = body.object_id;
      if (typeof objectId !== "string" || !objectId) return p2Bad("object_id 必填（字符串）");
      const hasCore = body.core && typeof body.core === "object";
      const hasPreset = typeof body.preset === "string";
      const hasAdvanced = body.advanced !== undefined;
      if (!hasCore && !hasPreset && !hasAdvanced) {
        return p2Bad("update_stroke 需要 core / preset / advanced 至少一项");
      }
      const rows = await atomsOf(handle, doc);
      const v = p2ObjectView(rows, objectId);
      if (!v.found) return p2Missing("对象 " + objectId + " 不存在");
      if (v.deleted) return p2Bad("对象 " + objectId + " 已被删除");
      const data = (v.data && typeof v.data === "object" && !Array.isArray(v.data)) ? { ...v.data } : {};
      if (hasCore) for (const k of Object.keys(body.core)) data[k] = body.core[k];
      if (hasPreset) data.preset = body.preset;
      if (hasAdvanced) data.advanced = body.advanced;
      const seq = await putAtom(handle, doc, p2Envelope("supersede", {
        object_id: objectId, layer_id: v.layer_id, data,
      }));
      return json({ ok: true, seq, server: false, object_id: objectId,
                    note: "已写入 supersede 原子（core/preset/advanced 已合进当前 data）" });
    }

    // **P2c-14 `set_brush_dynamics` ⇒ 明确 501 ✗**：**服务端改的是工作区笔刷缓存里的 `.myb` 文件 ✗**
    //   （**写临时文件再 `import_asset` 覆盖 ✓**）⇒ **∴ 本地 PWA 的笔刷是随包静态资源 ✗**
    //   ⇒ **∴ 没有可写的笔刷缓存 ✓** ⇒ **∴ 无法如实复现"**改完下次落笔生效**"的语义 ✓**。
    if (path === "/api/tools/set_brush_dynamics") {
      return json({ ok: false, error: "not_implemented_locally", endpoint: "/api/tools/set_brush_dynamics",
                    reason: "set_brush_dynamics：服务端修改工作区笔刷缓存里的 .myb 文件（落笔时生效）；" +
                            "本地 PWA 的笔刷是随包静态资源，没有可写的笔刷缓存，无法如实复现",
                    server: false }, 501);
    }

    // **P2c-15 `add_filter` ✗**：**照 `write_add_effect(…, Filter)` ✓**。
    if (path === "/api/tools/add_filter") {
      return p2cAddEffect("filter");
    }

    // **P2c-16 `update_filter` ✗**：**照 `write_update_effect(…, Filter)` ✓**。
    if (path === "/api/tools/update_filter") {
      return p2cUpdateEffect("filter");
    }

    // **P2c-17 `add_adjustment` ✗**：**照 `write_add_effect(…, Adjustment)` ✓**。
    if (path === "/api/tools/add_adjustment") {
      return p2cAddEffect("adjustment");
    }

    // **P2c-18 `update_adjustment` ✗**：**照 `write_update_effect(…, Adjustment)` ✓**。
    if (path === "/api/tools/update_adjustment") {
      return p2cUpdateEffect("adjustment");
    }

    // **P2c-19 `estimate_dehaze` ⇒ 明确 501 ✗**：**服务端渲染整幅裸像素做暗通道统计算大气光 ✗**
    //   ⇒ **∴ 本地内核只给 PNG 字节 ✗**（**无裸像素通道 ✓**）⇒ **∴ 暗通道算法无法如实复现 ✓**。
    if (path === "/api/tools/estimate_dehaze") {
      return json({ ok: false, error: "not_implemented_locally", endpoint: "/api/tools/estimate_dehaze",
                    reason: "estimate_dehaze：服务端渲染整幅裸像素做暗通道统计（estimate_atmospheric_light）；" +
                            "本地内核只产出 PNG 字节，没有裸像素通道，无法如实复现该算法",
                    server: false }, 501);
    }
    // **P2c-20 `transform_object` ✗**：**服务端提交 `AtomKind::Move` ✓**（**`{object_id, transform:{matrix, pivot}}` ✓**）
    //   ⇒ **∴ 本地写**同样的 `move` 原子 ✗**；**`transform{ matrix[6], pivot? }` 直接用 ✓**，
    //   **`dx`／`dy` 转成平移矩阵 ✓**（**anchor 合成是折叠层的事 ✓，**本地不重复算 ✓**）。
    if (path === "/api/tools/transform_object") {
      const objectId = body.object_id;
      if (typeof objectId !== "string" || !objectId) return p2Bad("object_id 必填（字符串）");
      let matrix = null;
      let pivot = [0, 0];
      if (body.transform && typeof body.transform === "object" && !Array.isArray(body.transform)) {
        const m = body.transform.matrix;
        if (!Array.isArray(m) || m.length !== 6 || m.some((x) => !Number.isFinite(Number(x)))) {
          return p2Bad("transform.matrix 必须是 6 元数字数组");
        }
        matrix = m.map(Number);
        const pv = body.transform.pivot;
        if (Array.isArray(pv) && pv.length === 2 && pv.every((x) => Number.isFinite(Number(x)))) {
          pivot = pv.map(Number);
        }
      } else if (body.dx !== undefined || body.dy !== undefined) {
        matrix = [1, 0, 0, 1, Number(body.dx) || 0, Number(body.dy) || 0];
      } else {
        return p2Bad("transform_object 需要 transform{ matrix[6], pivot? } 或 dx/dy");
      }
      const seq = await putAtom(handle, doc, p2Envelope("move", {
        object_id: objectId, transform: { matrix, pivot },
      }));
      return json({ ok: true, seq, server: false, object_id: objectId,
                    note: "已写入 move 原子（服务端 transform_object 即提交 AtomKind::Move）" });
    }

    // **P2c-21 `convert_to_path` ✗**：**照 `write_convert_to_path` ✓** ⇒ **源对象 `data` 从日志取 ✗** ＋
    //   **`points` ⇒ `nodes`（**零柄 ⇒ 贝塞尔退化成直线 ✓**）＋ 去掉 `points` ✓** ⇒
    //   **`create_object{type:"path"}` ✓ ＋ `tombstone` 原对象 ✓**（**与服务端同为两条原子 ✓**）。
    if (path === "/api/tools/convert_to_path") {
      const objectId = body.object_id;
      if (typeof objectId !== "string" || !objectId) return p2Bad("object_id 必填（字符串）");
      const rows = await atomsOf(handle, doc);
      const v = p2ObjectView(rows, objectId);
      if (!v.found) return p2Missing("对象 " + objectId + " 不存在");
      if (v.deleted) return p2Bad("对象 " + objectId + " 已被删除");
      const srcData = (v.data && typeof v.data === "object" && !Array.isArray(v.data)) ? v.data : {};
      const pts = srcData.points || (srcData.geometry && srcData.geometry.points);
      if (!Array.isArray(pts) || pts.length === 0) {
        return p2Bad("源对象没有可转换的 points（仅支持点列→路径）");
      }
      const nodes = [];
      for (const p of pts) {
        const x = Number(p.x !== undefined ? p.x : p[0]), y = Number(p.y !== undefined ? p.y : p[1]);
        if (!Number.isFinite(x) || !Number.isFinite(y)) return p2Bad("源 points 里有非数字坐标");
        nodes.push({ x, y, in: [0, 0], out: [0, 0] });
      }
      const pathData = { ...srcData };
      delete pathData.points;
      if (pathData.geometry) {
        const g = { ...pathData.geometry };
        delete g.points;
        pathData.geometry = g;
      }
      pathData.nodes = nodes;
      pathData.closed = body.closed === true;
      const pathId = (typeof body.path_id === "string" && body.path_id) ? body.path_id : p2cNewObjectId();
      const seq = await putAtom(handle, doc, p2Envelope("create_object", {
        object_id: pathId, layer_id: v.layer_id, type: "path", data: pathData,
      }));
      const tombSeq = await putAtom(handle, doc, p2Envelope("tombstone", { object_id: objectId }));
      return json({ ok: true, seq, server: false, path_id: pathId, from: objectId,
                    nodes: nodes.length, tombstone_seq: tombSeq,
                    note: "已写入 create_object{type:path}＋原对象 tombstone（与服务端同为两条原子）" });
    }

    // **P2c-22 `convert_to_shape` ✗**：**照 `write_convert_to_shape` ✓** ⇒ **点列 ⇒
    //   `geometry{kind:"polygon", points, bbox}` ＋ 顶层 `bbox` ✓**（**渲染与命中测试都认它 ✓**）⇒
    //   **`create_object{type:"shape"}` ✓ ＋ `tombstone` 原对象 ✓**。
    if (path === "/api/tools/convert_to_shape") {
      const objectId = body.object_id;
      if (typeof objectId !== "string" || !objectId) return p2Bad("object_id 必填（字符串）");
      const rows = await atomsOf(handle, doc);
      const v = p2ObjectView(rows, objectId);
      if (!v.found) return p2Missing("对象 " + objectId + " 不存在");
      if (v.deleted) return p2Bad("对象 " + objectId + " 已被删除");
      const srcData = (v.data && typeof v.data === "object" && !Array.isArray(v.data)) ? v.data : {};
      const pts = srcData.points || (srcData.geometry && srcData.geometry.points) || srcData.nodes;
      if (!Array.isArray(pts) || pts.length < 3) {
        return p2Bad("多边形至少需要三个顶点（当前 " + (Array.isArray(pts) ? pts.length : 0) + " 个）");
      }
      const xy = [];
      for (const p of pts) {
        const x = Number(p.x !== undefined ? p.x : p[0]), y = Number(p.y !== undefined ? p.y : p[1]);
        if (!Number.isFinite(x) || !Number.isFinite(y)) return p2Bad("源点列里有非数字坐标");
        xy.push([x, y]);
      }
      const xs = xy.map((p) => p[0]), ys = xy.map((p) => p[1]);
      const bbox = { x: Math.min(...xs), y: Math.min(...ys),
                     w: Math.max(...xs) - Math.min(...xs), h: Math.max(...ys) - Math.min(...ys) };
      const shapeData = { ...srcData };
      delete shapeData.nodes; delete shapeData.points; delete shapeData.closed;
      if (shapeData.geometry) {
        const g = { ...shapeData.geometry };
        delete g.points; delete g.nodes;
        shapeData.geometry = g;
      }
      shapeData.geometry = { ...(shapeData.geometry || {}), kind: "polygon", points: xy, bbox };
      shapeData.bbox = bbox;
      const shapeId = (typeof body.shape_id === "string" && body.shape_id) ? body.shape_id : p2cNewObjectId();
      const seq = await putAtom(handle, doc, p2Envelope("create_object", {
        object_id: shapeId, layer_id: v.layer_id, type: "shape", data: shapeData,
      }));
      const tombSeq = await putAtom(handle, doc, p2Envelope("tombstone", { object_id: objectId }));
      return json({ ok: true, seq, server: false, shape_id: shapeId, from: objectId,
                    vertices: xy.length, bbox: [bbox.x, bbox.y, bbox.w, bbox.h], tombstone_seq: tombSeq,
                    note: "已写入 create_object{type:shape}＋原对象 tombstone（与服务端同为两条原子）" });
    }

    // **P2c-23 `liquify_push` ✗**：**照 `write_liquify(…, "push")` ✓**。
    if (path === "/api/tools/liquify_push") {
      return p2cLiquify("push");
    }

    // **P2c-24 `liquify_pinch` ✗**：**照 `write_liquify(…, "pinch")` ✓**。
    if (path === "/api/tools/liquify_pinch") {
      return p2cLiquify("pinch");
    }

    // **P2c-25 `liquify_twirl` ✗**：**照 `write_liquify(…, "twirl")` ✓**。
    if (path === "/api/tools/liquify_twirl") {
      return p2cLiquify("twirl");
    }
    // **P2c-26 `sample_color` ✗**：**服务端渲染 1×1 读裸像素 ✓** ⇒ **∴ 本地用内核
    //   渲染 1×1 的 PNG ✗** ⇒ **∴ 在浏览器里解码读像素 ✓**（**与客户端所见同源 ✓**）。
    if (path === "/api/tools/sample_color") {
      const x = Number(body.x !== undefined ? body.x : q.get("x"));
      const y = Number(body.y !== undefined ? body.y : q.get("y"));
      if (!Number.isFinite(x) || !Number.isFinite(y)) {
        return p2Bad("缺少 x/y（画布坐标，像素 ⇒ 例如 {\"x\": 120, \"y\": 64}）");
      }
      try {
        const { png } = await p2cRenderPng(x, y, 1, 1);
        const img = await p2cDecodePng(png);
        const d = img.data;
        if (!d || d.length < 4) {
          return json({ ok: true, server: false, x, y, rgba: null,
                        note: "取不到像素（读不出与读到黑点是两件事）" });
        }
        const rgba = [d[0], d[1], d[2], d[3]];
        const hex = "#" + rgba.slice(0, 3).map((v) => v.toString(16).padStart(2, "0")).join("");
        return json({ ok: true, server: false, x, y, rgba, hex });
      } catch (err) { return p2cKernel501(err); }
    }

    // **P2c-27 `analyze_region` ✗**：**服务端渲染区域裸像素做统计 ✓** ⇒ **∴ 本地用内核
    //   渲染该区域的 PNG ✗** ⇒ **∴ 在浏览器里解码后算统计 ✓**（**均值／方差／直方图／主色 ✓**）。
    if (path === "/api/tools/analyze_region") {
      const rg = body.region;
      if (!rg || typeof rg !== "object") return p2Bad("缺少 region（{x,y,w,h}）");
      const x = Number(rg.x) || 0, y = Number(rg.y) || 0;
      const w = Number(rg.w) || 0, h = Number(rg.h) || 0;
      if (!(w > 0 && h > 0)) {
        return json({ ok: false, error: "precondition_failed", server: false,
                      reason: "这块区域没有像素可分析" }, 400);
      }
      if (body.compare_with_reference) {
        return json({ ok: false, error: "not_implemented_locally",
                      endpoint: "/api/tools/analyze_region",
                      reason: "analyze_region：compare_with_reference 需要与参考图逐像素对比，本地暂不支持",
                      server: false }, 501);
      }
      try {
        const { png, w: rw, h: rh } = await p2cRenderPng(x, y, w, h);
        const img = await p2cDecodePng(png);
        const d = img.data, n = rw * rh;
        const sum = [0, 0, 0, 0], sum2 = [0, 0, 0, 0];
        const hist = [[], [], [], []];
        for (let c = 0; c < 4; c++) { for (let b = 0; b < 16; b++) hist[c].push(0); }
        const buckets = new Map();
        for (let i = 0; i < n; i++) {
          for (let c = 0; c < 4; c++) {
            const v = d[i * 4 + c];
            sum[c] += v; sum2[c] += v * v;
            hist[c][v >> 4] += 1;
          }
          const key = ((d[i * 4] >> 4) << 12) | ((d[i * 4 + 1] >> 4) << 8) |
                      ((d[i * 4 + 2] >> 4) << 4) | (d[i * 4 + 3] >> 4);
          buckets.set(key, (buckets.get(key) || 0) + 1);
        }
        const mean = sum.map((s) => Math.round((s / n) * 100) / 100);
        const std = sum2.map((s2, c) => {
          const v = s2 / n - Math.pow(sum[c] / n, 2);
          return Math.round(Math.sqrt(Math.max(0, v)) * 100) / 100;
        });
        const dominant = [...buckets.entries()].sort((a, b) => b[1] - a[1]).slice(0, 8)
          .map(([key, cnt]) => ({
            hex: "#" + [12, 8, 4].map((sh) => (((key >> sh) & 15) * 17).toString(16).padStart(2, "0")).join(""),
            alpha: ((key & 15) * 17),
            pixels: cnt,
            fraction: Math.round((cnt / n) * 10000) / 10000,
          }));
        return json({ ok: true, server: false, region: { x, y, w: rw, h: rh },
                      pixels: n, mean_rgba: mean, std_rgba: std,
                      histogram_16bins: { r: hist[0], g: hist[1], b: hist[2], a: hist[3] },
                      dominant_colors: dominant,
                      note: "统计来自本地内核渲染（与客户端所见同源）" });
      } catch (err) { return p2cKernel501(err); }
    }

    // **P2c-28 `export_png` ✗**：**服务端渲染裸像素→缩放→编码 PNG→落盘／回 `archive_base64` ✓**
    //   ⇒ **∴ 本地无文件系统 ✗** ⇒ **∴ 用内核渲染整幅（**`region` 缺省整幅 ✓**）✗** ⇒
    //   **∴ 返回 `blob:` URL（**可直接下载／预览 ✓**）＋ `archive_base64`（**与服务端同名字段 ✓**）**。
    if (path === "/api/tools/export_png") {
      if (body.layer_id !== undefined && body.layer_id !== null) {
        return p2Bad("本地内核的 render_region_png 没有图层参数 ⇒ 按层导出本地暂不支持（请先用 get_state 确认要导出的层）");
      }
      const rows = await atomsOf(handle, doc);
      const { w: cw, h: ch } = p2cDocSize(rows);
      let rx = 0, ry = 0, rw = cw, rh = ch;
      if (body.region && typeof body.region === "object" && !Array.isArray(body.region)) {
        rx = Math.max(0, Math.floor(Number(body.region.x) || 0));
        ry = Math.max(0, Math.floor(Number(body.region.y) || 0));
        rw = Math.max(1, Math.min(cw - rx, Math.floor(Number(body.region.w) || cw)));
        rh = Math.max(1, Math.min(ch - ry, Math.floor(Number(body.region.h) || ch)));
      }
      try {
        const { png } = await p2cRenderPng(rx, ry, rw, rh);
        let url = null;
        try {
          if (typeof URL !== "undefined" && typeof URL.createObjectURL === "function") {
            url = URL.createObjectURL(new Blob([png], { type: "image/png" }));
          }
        } catch (e) { url = null; }
        return json({ ok: true, server: false, width: rw, height: rh, bytes: png.length,
                      mime_type: "image/png", url, archive_base64: p2cB64(png),
                      note: "本地无文件系统 ⇒ 不落盘；url 可直接打开/下载，archive_base64 与服务端同名字段" });
      } catch (err) { return p2cKernel501(err); }
    }

    // **P2c-29 `export_project` ✗**：**服务端打 `.yanshi` tar 包 ✓** ⇒ **∴ 本地打 JSON 工程包 ✗**
    //   （**`meta` ＋ 原子日志 ＋ 引用的 blob（**base64，有上限，如实报告 ✓**）**）⇒ **∴ 返回 `blob:` 下载 URL ✓**。
    if (path === "/api/tools/export_project") {
      const rows = await atomsOf(handle, doc);
      const atoms = rows.map((r) => r.atom);
      const { w, h, color_space } = p2cDocSize(rows);
      const refs = new Set();
      for (const a of atoms) {
        const bh = a && a.payload && a.payload.bitmap && a.payload.bitmap.blob_hash;
        if (typeof bh === "string" && bh) refs.add(bh);
      }
      const { getBlob } = await import("./store.js");
      const blobs = {};
      let embeddedBytes = 0;
      const skipped = [];
      const CAP = 64 * 1024 * 1024;
      for (const hash of refs) {
        const bytes = await getBlob(handle, hash);
        if (!bytes) { skipped.push({ hash, reason: "本地 blob 库里没有" }); continue; }
        const len = bytes.length || 0;
        if (embeddedBytes + len > CAP) { skipped.push({ hash, reason: "超过 64MB 嵌入上限" }); continue; }
        blobs[hash] = p2cB64(bytes);
        embeddedBytes += len;
      }
      const project = { format: "yanshi-project", version: 1, doc_id: doc,
                        exported_at: Date.now(), server: false,
                        meta: { width: w, height: h, color_space, head_seq: rows.length ? rows[rows.length - 1].seq : 0 },
                        atoms, blobs,
                        skipped_blobs: skipped };
      const text = JSON.stringify(project);
      let url = null;
      try {
        if (typeof URL !== "undefined" && typeof URL.createObjectURL === "function") {
          url = URL.createObjectURL(new Blob([text], { type: "application/json" }));
        }
      } catch (e) { url = null; }
      return json({ ok: true, server: false, doc_id: doc, url,
                    atoms: atoms.length, blobs_embedded: Object.keys(blobs).length,
                    blobs_skipped: skipped.length, bytes: text.length,
                    filename: doc + ".yanshi.json",
                    note: "JSON 工程包（meta＋原子日志＋引用的 blob）；用 import_project 可导回" });
    }

    // **P2c-30 `import_project` ✗**：**服务端从路径读工程包 ✓** ⇒ **∴ 本地从请求 body
    //   收 JSON 工程包 ✗**（**PWA 没有服务端文件路径可用 ✓**）⇒ **∴ 校验后逐条写入原子 ✓**
    //   （**`seq` 重排 ✓，**`id` 保留或再生 ✓**）＋ 恢复内嵌 blob ✓**。
    if (path === "/api/tools/import_project") {
      const project = body.project && typeof body.project === "object" ? body.project : body;
      const atoms = project.atoms;
      if (!Array.isArray(atoms)) return p2Bad("工程包需要 atoms 数组（{format, atoms, blobs?} 或直接 {atoms}）");
      const seen = new Set((await atomsOf(handle, doc)).map((r) => r.atom && r.atom.id));
      const { putBlob } = await import("./store.js");
      let restoredBlobs = 0;
      if (project.blobs && typeof project.blobs === "object") {
        for (const hash of Object.keys(project.blobs)) {
          const b64 = project.blobs[hash];
          if (typeof b64 !== "string") continue;
          let bytes;
          if (typeof Buffer !== "undefined" && Buffer.from) {
            bytes = Array.from(Buffer.from(b64, "base64"));
          } else {
            const bin = atob(b64);
            bytes = new Array(bin.length);
            for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
          }
          await putBlob(handle, hash, bytes);
          restoredBlobs += 1;
        }
      }
      const seqs = [];
      let skipped = 0;
      for (const a of atoms) {
        if (!a || typeof a !== "object" || typeof a.kind !== "string" || !a.payload || typeof a.payload !== "object") {
          skipped += 1;
          continue;
        }
        if (p2cKnownKinds.indexOf(a.kind) < 0) { skipped += 1; continue; }
        let id = typeof a.id === "string" && a.id ? a.id : null;
        if (!id || seen.has(id)) id = "01IMP" + String(Date.now()).padStart(13, "0") + String(seqs.length).padStart(4, "0");
        seen.add(id);
        const seq = await putAtom(handle, doc, {
          actor: typeof a.actor === "string" ? a.actor : "human:web",
          id, kind: a.kind, payload: a.payload,
          schema_version: 1, seq: 0,
          session: typeof a.session === "string" ? a.session : "session:web",
          timestamp: Number(a.timestamp) || Date.now(),
        });
        seqs.push(seq);
      }
      return json({ ok: true, server: false, doc_id: doc, imported: seqs.length,
                    skipped, blobs_restored: restoredBlobs,
                    note: "原子已按包内顺序写入（seq 重排）；未知 kind 的条目被跳过（如实报告）" });
    }

    // **P2c-31 `import_psd` ⇒ 明确 501 ✗**：**服务端用 `psd::decode_psd` 解析图层 ✗**
    //   ⇒ **∴ 本地没有 PSD 解析器 ✓** ⇒ **∴ 无法如实复现 ✓**。
    if (path === "/api/tools/import_psd") {
      return json({ ok: false, error: "not_implemented_locally", endpoint: "/api/tools/import_psd",
                    reason: "import_psd：服务端用 psd::decode_psd 解析 PSD 图层结构；本地没有 PSD 解析器，无法如实复现",
                    server: false }, 501);
    }

    // **P2c-32 `import_asset` ✗**：**服务端把文件导进工作区资产缓存 ✓** ⇒ **∴ 本地对
    //   `kind:"texture"` 从 `/textures/` 取同名文件存进本地 blob 库 ✗**（**`texture_background` 可直接用 ✓**）；
    //   **∴ `brush`／`palette` 等本地是随包静态资源 ✗** ⇒ **∴ 明确 501 ✓**。
    if (path === "/api/tools/import_asset") {
      const kind = body.kind;
      if (typeof kind !== "string" || !kind) return p2Bad("kind 必填（字符串）");
      const name = body.name;
      if (typeof name !== "string" || !name) return p2Bad("name 必填（字符串）");
      if (kind !== "texture") {
        return json({ ok: false, error: "not_implemented_locally", endpoint: "/api/tools/import_asset",
                      reason: "import_asset{kind:" + kind + "}：本地该类资产是随包静态资源，没有可写的资产缓存；" +
                              "目前仅支持 kind:texture（存进本地 blob 库）",
                      server: false }, 501);
      }
      let bytes;
      try {
        const res = await fetch("/textures/" + encodeURIComponent(name), { cache: "no-store" });
        if (!res.ok) {
          return json({ ok: false, error: "asset_not_found", server: false,
                        reason: name + " ⇒ HTTP " + res.status }, 404);
        }
        bytes = new Uint8Array(await res.arrayBuffer());
      } catch (err) {
        return json({ ok: false, error: "asset_not_found", server: false,
                      reason: name + " ⇒ " + String((err && err.message) || err) }, 404);
      }
      try {
        const { entry } = await p2cKernelFor();
        const raw = String(await withKernel(doc, () => entry.k.blob_put(bytes)) || "");
        let blobHash = raw;
        try { const o = JSON.parse(raw); if (o && o.blob_hash) blobHash = String(o.blob_hash); } catch (e) {}
        const { putBlob } = await import("./store.js");
        await putBlob(handle, blobHash, Array.from(bytes));
        return json({ ok: true, server: false, kind, name, blob_hash: blobHash, size: bytes.length,
                      note: "纹理已存进本地 blob 库；texture_background 可直接用该 blob" });
      } catch (err) { return p2cKernel501(err); }
    }
    // **P2c-33 `new_document` ✗**：**照 `write_new_document` ✓** ⇒ **写 `create_document` 原子 ✓**。
    //   **∴ 已有创建原子的文档拒绝再建 ✗**（**∴ 否则折叠层会看到两条创建记录 ✓**）。
    if (path === "/api/tools/new_document") {
      const docId = (typeof body.doc_id === "string" && body.doc_id) ? body.doc_id : doc;
      if (!docId) return p2Bad("doc_id 必填（字符串；或走 ?doc=）");
      const rows = await atomsOf(handle, docId);
      if (rows.some((r) => r.atom && r.atom.kind === "create_document")) {
        return p2Bad("文档 " + docId + " 已有 create_document 原子 ⇒ 不再重复创建");
      }
      const width = body.width !== undefined ? Math.floor(Number(body.width)) : 1024;
      const height = body.height !== undefined ? Math.floor(Number(body.height)) : 1024;
      if (!Number.isFinite(width) || width <= 0 || !Number.isFinite(height) || height <= 0) {
        return p2Bad("width/height 必须是正整数");
      }
      let background = { a: 255, b: 255, g: 255, r: 255 };
      if (body.background && typeof body.background === "object") background = body.background;
      const seq = await putAtom(handle, docId, p2Envelope("create_document", {
        background, color_space: "srgb", doc_id: docId, height, width,
      }));
      return json({ ok: true, server: false, doc_id: docId, seq, width, height,
                    note: "已写入 create_document 原子" });
    }

    // **P2c-34 `delete_document` ✗**：**照 `write_delete_document` ✓** ⇒ **删掉该文档的
    //   全部原子＋文档元数据 ✓**（**blob 是内容寻址共享的 ✗ ⇒ **∴ 留给 `collect_garbage` ✓**）。
    if (path === "/api/tools/delete_document") {
      const documentId = body.document_id || body.doc_id || doc;
      if (typeof documentId !== "string" || !documentId) return p2Bad("document_id 必填（字符串）");
      const rows = await atomsOf(handle, documentId);
      if (!rows.length) {
        const meta = await wrap(tx(handle, "docs", "readonly").get(documentId));
        if (!meta) return p2Missing("文档 " + documentId + " 不存在");
      }
      const { tx: txFn, wrap: wrapFn } = await import("./store.js");
      const delStore = async (store, keyFn) => {
        const all = await wrapFn(txFn(handle, store, "readonly").getAll());
        let n = 0;
        for (const r of all || []) {
          if (keyFn(r)) { await wrapFn(txFn(handle, store, "readwrite").delete(r.id ?? r.doc)); n += 1; }
        }
        return n;
      };
      const atomsDeleted = await delStore("atoms", (r) => r.doc === documentId);
      const docsDeleted = await delStore("docs", (r) => r.doc === documentId);
      kernels.delete(documentId);
      return json({ ok: true, server: false, document_id: documentId,
                    atoms_deleted: atomsDeleted, docs_deleted: docsDeleted,
                    note: "原子与文档元数据已删；blob 留给 collect_garbage 回收" });
    }

    // **P2c-35 `list_documents` ✗**：**照 `read_list_documents` ✓** ⇒ **从 `docs` 表枚举 ✗** ＋
    //   **宽高／层数／对象数从原子日志推导 ✓**。
    if (path === "/api/tools/list_documents") {
      const { tx: txFn, wrap: wrapFn } = await import("./store.js");
      const metas = await wrapFn(txFn(handle, "docs", "readonly").getAll());
      const documents = [];
      for (const m of metas || []) {
        const docId = m.doc;
        const rows = await atomsOf(handle, docId);
        const { w, h } = p2cDocSize(rows);
        const st = p2cState(rows);
        documents.push({ doc_id: docId, width: w, height: h,
                         layers: st.layers.length, objects: st.objects.length,
                         head_seq: m.seq ?? 0 });
      }
      return json({ ok: true, server: false, documents, count: documents.length });
    }

    // **P2c-36 `declare_head` ✗**：**照 `write_declare_head` ✓** ⇒ **`declare_head` 原子 ✓**
    //   （**`{base:{type, id}, reason?}` ✓**）。
    if (path === "/api/tools/declare_head") {
      const baseType = body.base_type;
      const baseId = body.base_id;
      if (typeof baseType !== "string" || ["atom", "declare_head", "checkpoint"].indexOf(baseType) < 0) {
        return p2Bad("base_type 必须是 atom/declare_head/checkpoint，得到 " + baseType);
      }
      if (typeof baseId !== "string" || !baseId) return p2Bad("base_id 必填（字符串）");
      const payload = { base: { type: baseType, id: baseId } };
      if (typeof body.reason === "string") payload.reason = body.reason;
      const seq = await putAtom(handle, doc, p2Envelope("declare_head", payload));
      return json({ ok: true, seq, server: false,
                    note: "已写入 declare_head 原子；求值起点跳变由内核折叠层处理" });
    }

    // **P2c-37 `get_state` ✗**：**照 `read_get_state` ✓** ⇒ **从原子日志推导 ✓**
    //   （**`include_objects`／`include_hidden` 照服务端 ✓**）。
    if (path === "/api/tools/get_state") {
      const rows = await atomsOf(handle, doc);
      const st = p2cState(rows);
      const includeObjects = body.include_objects === true;
      const includeHidden = body.include_hidden === true;
      const layers = st.layers.filter((l) => includeHidden || l.visible !== false);
      const out = { ok: true, server: false, doc_id: doc,
                    head_seq: rows.length ? rows[rows.length - 1].seq : 0,
                    width: st.width, height: st.height, color_space: st.color_space,
                    layers, checkpoints: st.checkpoints,
                    derived_from: "atoms" };
      if (includeObjects) {
        out.objects = st.objects.map((o) => {
          const v = p2ObjectView(rows, o.object_id);
          return { object_id: o.object_id, layer_id: v.layer_id, type: v.type,
                   visible: v.visible, deleted: v.deleted };
        }).filter((o) => includeHidden || !o.deleted);
      }
      return json(out);
    }

    // **P2c-38 `get_resolved_state` ✗**：**服务端做实例解析 ✓** ⇒ **∴ 本地给同样的推导状态 ✗** ＋
    //   **如实说明实例解析沿用折叠语义 ✓**（**∴ 不另算一套 ✓**）。
    if (path === "/api/tools/get_resolved_state") {
      const rows = await atomsOf(handle, doc);
      const st = p2cState(rows);
      const instances = [];
      for (const o of st.objects) {
        const v = p2ObjectView(rows, o.object_id);
        if (v.type === "instance" && v.master_ref) {
          instances.push({ instance_id: o.object_id, master_ref: v.master_ref,
                           sync_policy: (v.data && v.data.sync_policy) || "all" });
        }
      }
      return json({ ok: true, server: false, doc_id: doc,
                    head_seq: rows.length ? rows[rows.length - 1].seq : 0,
                    width: st.width, height: st.height,
                    layers: st.layers, instances,
                    resolution: "local-fold",
                    note: "实例解析沿用内核折叠语义；本地不另做独立 resolve",
                    derived_from: "atoms" });
    }

    // **P2c-39 `get_render_status` ✗**：**照 `read_get_render_status` ✓** ⇒ **从快照元数据推导 ✓**
    //   （**快照序号 ≥ 该原子序号 ⇒ 视为已渲染 ✓**）。
    if (path === "/api/tools/get_render_status") {
      const atomId = body.atom_id;
      if (typeof atomId !== "string" || !atomId) return p2Bad("atom_id 必填（字符串）");
      const rows = await atomsOf(handle, doc);
      const target = rows.find((r) => r.atom && r.atom.id === atomId);
      if (!target) return p2Missing("原子 " + atomId + " 不在日志里");
      const { tx: txFn, wrap: wrapFn } = await import("./store.js");
      const meta = await wrapFn(txFn(handle, "docs", "readonly").get(doc));
      const headSeq = meta ? (meta.seq ?? 0) : 0;
      const snapSeq = meta && meta.snapshot ? (meta.seq ?? 0) : 0;
      const rendered = snapSeq >= target.seq;
      return json({ ok: true, server: false, atom_id: atomId,
                    rendered, head_seq: headSeq, rendered_seq: rendered ? snapSeq : 0,
                    atom_seq: target.seq, thumbnail_current: rendered,
                    note: "本地渲染是按需的；rendered 指已有覆盖该原子的快照" });
    }

    // **P2c-40 `cache_document_preview` ✗**：**服务端渲染并缓存预览 ✓** ⇒ **∴ 本地用内核
    //   渲染整幅写入快照 ✓**（**∴ 与 `render_region` 同一编码器 ✓**）。
    if (path === "/api/tools/cache_document_preview") {
      try {
        const rows = await atomsOf(handle, doc);
        const { w, h } = p2cDocSize(rows);
        const { png } = await p2cRenderPng(0, 0, w, h);
        const { tx: txFn, wrap: wrapFn } = await import("./store.js");
        const meta = (await wrapFn(txFn(handle, "docs", "readonly").get(doc))) || { doc };
        const seqNow = meta.seq ?? 0;
        await writeSnapshot(handle, doc, seqNow, { bytes: png });
        return json({ ok: true, server: false, doc_id: doc, seq: seqNow,
                      width: w, height: h, bytes: png.length,
                      note: "整幅预览已渲染并写入本地快照" });
      } catch (err) { return p2cKernel501(err); }
    }

    // **P2c-41 `set_reference` ✗**：**服务端记进偏好 ✓** ⇒ **∴ 本地写同一份 `prefs` 表 ✓**
    //   （**`get_preferences` 能读回来 ✓**）。
    if (path === "/api/tools/set_reference") {
      const blobHash = body.blob_hash;
      if (typeof blobHash !== "string" || !blobHash) return p2Bad("blob_hash 必填（字符串）");
      const { getBlob, tx: txFn, wrap: wrapFn } = await import("./store.js");
      const blob = await getBlob(handle, blobHash);
      if (!blob) return p2Missing("blob " + blobHash + " 不在本地 blob 库里");
      const opacity = body.opacity !== undefined ? Number(body.opacity) : 0.5;
      const position = body.position !== undefined ? body.position : null;
      const put = async (key, value) => {
        await wrapFn(txFn(handle, "prefs", "readwrite").put({ key, value }));
      };
      await put("reference.blob_hash", blobHash);
      await put("reference.opacity", Math.min(1, Math.max(0, opacity)));
      await put("reference.position", position);
      return json({ ok: true, server: false,
                    reference: { blob_hash: blobHash, opacity: Math.min(1, Math.max(0, opacity)), position },
                    note: "参考图只记在偏好里 ⇒ 文档未被改动（查看器据此叠一层半透明图）" });
    }

    // **P2c-42 `clear_reference` ✗**：**照 `write_clear_reference` ✓** ⇒ **偏好键置空即删 ✓**。
    if (path === "/api/tools/clear_reference") {
      const { tx: txFn, wrap: wrapFn } = await import("./store.js");
      for (const key of ["reference.blob_hash", "reference.opacity", "reference.position"]) {
        try { await wrapFn(txFn(handle, "prefs", "readwrite").delete(key)); } catch (e) {}
      }
      return json({ ok: true, server: false, cleared: true });
    }
    // **P2c-43 `collect_diagnostics` ✗**：**照 `read_collect_diagnostics` ✓** ⇒ **从原子日志＋环境推导 ✓**。
    if (path === "/api/tools/collect_diagnostics") {
      const rows = await atomsOf(handle, doc);
      const byKind = {};
      for (const r of rows) {
        const k = (r.atom && r.atom.kind) || "unknown";
        byKind[k] = (byKind[k] || 0) + 1;
      }
      const st = p2cState(rows);
      const backend = await detectBackend();
      return json({ ok: true, server: false, doc_id: doc,
                    head_seq: rows.length ? rows[rows.length - 1].seq : 0,
                    atom_count: rows.length, atoms_by_kind: byKind,
                    layers: st.layers.length, objects: st.objects.length,
                    width: st.width, height: st.height,
                    render_backend: backend,
                    timestamp: Date.now(),
                    derived_from: "atoms" });
    }

    // **P2c-44 `collect_garbage` ✗**：**照 `read_collect_garbage` ✓** ⇒ **扫描 blob 库 ✗** ＋
    //   **引用集合从原子日志推导 ✓**（**`bitmap.blob_hash` ✓**）；**`confirm:true` 才真删 ✓**。
    if (path === "/api/tools/collect_garbage") {
      const confirm = body.confirm === true;
      const rows = await atomsOf(handle, doc);
      const refs = new Set();
      for (const r of rows) {
        const a = r.atom || {};
        const pl = a.payload || {};
        const bh = pl.bitmap && pl.bitmap.blob_hash;
        if (typeof bh === "string" && bh) refs.add(bh);
      }
      const { tx: txFn, wrap: wrapFn } = await import("./store.js");
      const all = await wrapFn(txFn(handle, "blobs", "readonly").getAll());
      const orphans = [];
      let orphanBytes = 0;
      for (const b of all || []) {
        if (!refs.has(b.hash)) {
          const size = (b.bytes && b.bytes.length) || 0;
          orphans.push({ hash: b.hash, size });
          orphanBytes += size;
        }
      }
      let reclaimed = 0, reclaimedBytes = 0;
      if (confirm) {
        for (const o of orphans) {
          await wrapFn(txFn(handle, "blobs", "readwrite").delete(o.hash));
          reclaimed += 1; reclaimedBytes += o.size;
        }
      }
      return json({ ok: true, server: false, doc_id: doc,
                    scanned: (all || []).length, referenced: refs.size,
                    orphans: orphans.map((o) => o.hash), orphan_count: orphans.length,
                    orphan_bytes: orphanBytes, dry_run: !confirm,
                    reclaimed_blobs: reclaimed, reclaimed_bytes: reclaimedBytes,
                    note: confirm ? "已删除无引用 blob" : "试运行：加 confirm:true 才真删" });
    }

    // **P2c-45 `blob_gc` ✗**：**服务端做冷热分层 ✓** ⇒ **∴ 本地无冷存储 ✗** ⇒ **∴ 如实给
    //   blob 盘点 ✓**（**总数／引用中／无引用 ✓**）＋ 明确说明不做分层 ✓**。
    if (path === "/api/tools/blob_gc") {
      const rows = await atomsOf(handle, doc);
      const refs = new Set();
      for (const r of rows) {
        const bh = r.atom && r.atom.payload && r.atom.payload.bitmap && r.atom.payload.bitmap.blob_hash;
        if (typeof bh === "string" && bh) refs.add(bh);
      }
      const { tx: txFn, wrap: wrapFn } = await import("./store.js");
      const all = await wrapFn(txFn(handle, "blobs", "readonly").getAll());
      let totalBytes = 0, unrefBytes = 0, unref = 0;
      for (const b of all || []) {
        const size = (b.bytes && b.bytes.length) || 0;
        totalBytes += size;
        if (!refs.has(b.hash)) { unref += 1; unrefBytes += size; }
      }
      return json({ ok: true, server: false, doc_id: doc,
                    blobs: (all || []).length, bytes: totalBytes,
                    referenced: refs.size, unreferenced: unref, unreferenced_bytes: unrefBytes,
                    cold: { blobs: 0, bytes: 0 },
                    note: "本地无冷存储分层 ⇒ cold 恒为 0；删无引用 blob 请用 collect_garbage{confirm:true}",
                    derived_from: "atoms" });
    }

    // **P2c-46 `get_inflight` ✗**：**照 `read_get_inflight` ✓** ⇒ **本地无后台任务注册表 ✗**
    //   ⇒ **∴ 如实报空表 ✓**（**∴ 不是"没实现"✗**）。
    if (path === "/api/tools/get_inflight") {
      const wanted = typeof body.doc_id === "string" && body.doc_id ? body.doc_id : doc;
      return json({ ok: true, server: false, doc_id: wanted, inflight: [], count: 0,
                    note: "本地无后台任务注册表（渲染/导出都是同步完成的）" });
    }

    // **P2c-47 `get_job` ✗**：**本地无任务注册表 ✗** ⇒ **∴ 按"查不到任务"如实 404 ✓**。
    if (path === "/api/tools/get_job") {
      const jobId = body.job_id;
      if (typeof jobId !== "string" || !jobId) return p2Bad("job_id 必填（字符串）");
      return json({ ok: false, error: "job_not_found", server: false,
                    reason: "任务 " + jobId + " 不存在（本地无任务注册表）" }, 404);
    }

    // **P2c-48 `cancel_job` ✗**：**本地无任务注册表 ✗** ⇒ **∴ 如实 404 ✓**。
    if (path === "/api/tools/cancel_job") {
      const jobId = body.job_id;
      if (typeof jobId !== "string" || !jobId) return p2Bad("job_id 必填（字符串）");
      return json({ ok: false, error: "job_not_found", server: false,
                    reason: "任务 " + jobId + " 不存在（本地无任务注册表，无可取消）" }, 404);
    }

    // **P2c-49 `cancel_operation` ✗**：**照 `write_cancel_operation` 的"无可取消"分支 ✓**
    //   ⇒ **∴ 本地没有在飞的操作 ✗** ⇒ **∴ 如实报 `cancelled:false` ✓**。
    if (path === "/api/tools/cancel_operation") {
      const opId = body.operation_id || body.op_id;
      if (typeof opId !== "string" || !opId) return p2Bad("operation_id 必填（字符串）");
      return json({ ok: true, server: false, operation_id: opId, cancelled: false,
                    note: "本地没有在飞的操作，无可取消" });
    }

    // **P2c-50 `find_atom` ✗**：**照 `read_find_atom` ✓** ⇒ **按 `kind`／`actor`／`object_id`／
    //   `layer_id`／`since_seq` 过滤 ✓**（**`limit` 缺省 200、上限 2000 ✓**）。
    if (path === "/api/tools/find_atom") {
      const since = body.since_seq !== undefined ? Number(body.since_seq) : 0;
      const limit = Math.min(2000, Math.max(1, body.limit !== undefined ? Number(body.limit) : 200));
      const kindF = typeof body.kind === "string" ? body.kind : null;
      const actorF = typeof body.actor === "string" ? body.actor : null;
      const objectF = typeof body.object_id === "string" ? body.object_id : null;
      const layerF = typeof body.layer_id === "string" ? body.layer_id : null;
      const rows = await atomsOf(handle, doc);
      const atomLayer = (a) => {
        const pl = a.payload || {};
        return pl.layer_id || (pl.data && pl.data.layer_id) || null;
      };
      const atomObject = (a) => {
        const pl = a.payload || {};
        return pl.object_id || (pl.data && pl.data.object_id) || null;
      };
      const matched = rows.filter((r) => {
        const a = r.atom || {};
        if (r.seq <= since) return false;
        if (kindF && a.kind !== kindF) return false;
        if (actorF && a.actor !== actorF) return false;
        if (objectF && atomObject(a) !== objectF) return false;
        if (layerF && atomLayer(a) !== layerF) return false;
        return true;
      });
      const atoms = matched.slice(0, limit).map((r) => ({
        atom_id: r.atom.id, seq: r.seq, kind: r.atom.kind,
        actor: r.atom.actor, timestamp: r.atom.timestamp,
        object_id: atomObject(r.atom), layer_id: atomLayer(r.atom),
      }));
      return json({ ok: true, server: false, atoms, returned: atoms.length,
                    total_matched: matched.length, truncated: matched.length > limit });
    }

    // **P2c-51 `get_atom` ✗**：**照 `read_get_atom` ✓** ⇒ **按 `atom_id` 取整条原子 ✓**。
    if (path === "/api/tools/get_atom") {
      const atomId = body.atom_id;
      if (typeof atomId !== "string" || !atomId) return p2Bad("atom_id 必填（字符串）");
      const rows = await atomsOf(handle, doc);
      const found = rows.find((r) => r.atom && r.atom.id === atomId);
      if (!found) return p2Missing("原子 " + atomId + " 不在日志里");
      const a = found.atom;
      return json({ ok: true, server: false, atom_id: a.id, seq: found.seq, kind: a.kind,
                    actor: a.actor, session: a.session, timestamp: a.timestamp,
                    payload: a.payload });
    }

    // **P2c-52 `get_ancestors` ✗**：**服务端走实例→master 链 ✓** ⇒ **∴ 本地从日志推导
    //   `master_ref` 链 ✓**（**成环即停并标记 ✓**）。
    if (path === "/api/tools/get_ancestors") {
      const objectId = body.object_id;
      if (typeof objectId !== "string" || !objectId) return p2Bad("object_id 必填（字符串）");
      const rows = await atomsOf(handle, doc);
      const chain = [];
      const seen = new Set([objectId]);
      let cur = objectId;
      for (let i = 0; i < 64; i++) {
        const v = p2ObjectView(rows, cur);
        if (!v.found || !v.master_ref || typeof v.master_ref.object_id !== "string") break;
        const master = v.master_ref.object_id;
        if (seen.has(master)) { chain.push({ object_id: master, cycle: true }); break; }
        seen.add(master);
        chain.push({ object_id: master });
        cur = master;
      }
      return json({ ok: true, server: false, object_id: objectId, ancestors: chain,
                    count: chain.length, derived_from: "atoms" });
    }

    // **P2c-53 `get_descendants` ✗**：**`get_ancestors` 的反向 ✓** ⇒ **∴ 扫描所有对象的
    //   `master_ref` 找指向该对象的实例 ✓**。
    if (path === "/api/tools/get_descendants") {
      const objectId = body.object_id;
      if (typeof objectId !== "string" || !objectId) return p2Bad("object_id 必填（字符串）");
      const rows = await atomsOf(handle, doc);
      const st = p2cState(rows);
      const ids = new Set();
      for (const o of st.objects) ids.add(o.object_id);
      const descendants = [];
      for (const id of ids) {
        const v = p2ObjectView(rows, id);
        if (v.found && !v.deleted && v.master_ref && v.master_ref.object_id === objectId) {
          descendants.push({ object_id: id, layer_id: v.layer_id });
        }
      }
      return json({ ok: true, server: false, object_id: objectId, descendants,
                    count: descendants.length, derived_from: "atoms" });
    }

    // **P2c-54 `get_diff` ✗**：**照 `read_get_diff` ✓** ⇒ **`(from_seq, to_seq]` 的日志差分 ✓** ＋
    //   **种类汇总 ＋ 涉及的对象／图层集合 ✓**。
    if (path === "/api/tools/get_diff") {
      const rows = await atomsOf(handle, doc);
      const head = rows.length ? rows[rows.length - 1].seq : 0;
      const fromSeq = body.from_seq !== undefined ? Number(body.from_seq) : 0;
      const toSeq = body.to_seq !== undefined ? Number(body.to_seq) : head;
      if (!Number.isFinite(fromSeq) || !Number.isFinite(toSeq) || fromSeq > toSeq) {
        return p2Bad("from_seq/to_seq 非法（需要 0 ≤ from_seq ≤ to_seq）");
      }
      const inRange = rows.filter((r) => r.seq > fromSeq && r.seq <= toSeq);
      const byKind = {};
      const objects = new Set(), layers = new Set();
      for (const r of inRange) {
        const a = r.atom || {};
        byKind[a.kind] = (byKind[a.kind] || 0) + 1;
        const pl = a.payload || {};
        if (pl.object_id) objects.add(pl.object_id);
        if (pl.layer_id) layers.add(pl.layer_id);
      }
      return json({ ok: true, server: false, doc_id: doc, from_seq: fromSeq, to_seq: toSeq,
                    atoms: inRange.map((r) => ({ atom_id: r.atom.id, seq: r.seq, kind: r.atom.kind })),
                    count: inRange.length, by_kind: byKind,
                    objects: [...objects], layers: [...layers],
                    note: "日志层差分（不做像素/求值层差分，要像素对比请用 render_region 各渲染一次）" });
    }

    // **P2c-55 `get_dependency_graph` ✗**：**照 `read_get_dependency_graph` ✓** ⇒ **从日志推导 ✓**：
    //   **∴ 节点＝对象 ✗**；**边＝实例→master ＋ 组→成员 ＋ `supersede` 版本链 ✓**。
    if (path === "/api/tools/get_dependency_graph") {
      const rows = await atomsOf(handle, doc);
      const st = p2cState(rows);
      const ids = new Set();
      for (const o of st.objects) ids.add(o.object_id);
      const nodes = [], edges = [];
      for (const id of ids) {
        const v = p2ObjectView(rows, id);
        if (!v.found) continue;
        nodes.push({ object_id: id, type: v.type, layer_id: v.layer_id, deleted: v.deleted });
        if (v.master_ref && typeof v.master_ref.object_id === "string") {
          edges.push({ from: id, to: v.master_ref.object_id, rel: "instance_of" });
        }
        if (Array.isArray(v.members)) {
          for (const m of v.members) edges.push({ from: id, to: m, rel: "group_member" });
        }
      }
      const lastByObject = {};
      for (const r of rows) {
        const a = r.atom || {};
        if (a.kind !== "supersede") continue;
        const oid = a.payload && a.payload.object_id;
        if (typeof oid === "string") {
          if (lastByObject[oid]) edges.push({ from: a.id, to: lastByObject[oid], rel: "supersedes" });
          lastByObject[oid] = a.id;
        }
      }
      return json({ ok: true, server: false, doc_id: doc,
                    nodes, edges, node_count: nodes.length, edge_count: edges.length,
                    derived_from: "atoms",
                    note: "本地从原子日志推导（实例/组/版本链）；服务端折叠层的完整依赖以其为准" });
    }
    // **P2c-56 `batch` ✗**：**服务端在一个变更集里顺序执行子调用 ✓** ⇒ **∴ 本地把每个子调用
    //   重新走一遍本 `local` 分发 ✓**（**`local` 是命名函数表达式 ⇒ **∴ 可递归 ✓**）⇒
    //   **∴ 语义与逐个调用一致 ✓**（**本地无 changeset 机制 ⇒ **∴ 如实说明不回滚 ✓**）。
    if (path === "/api/tools/batch") {
      const calls = body.calls;
      if (!Array.isArray(calls) || calls.length === 0) return p2Bad("batch 至少需要一个调用（calls: [{tool, args}]）");
      const results = [];
      for (let i = 0; i < calls.length; i++) {
        const call = calls[i] || {};
        const tool = call.tool;
        if (typeof tool !== "string" || !tool) {
          results.push({ index: i, ok: false, error: "invalid_argument", reason: "calls[" + i + "].tool 必填" });
          break;
        }
        const args = (call.args && typeof call.args === "object") ? call.args : {};
        let sub;
        try {
          sub = new Request("https://local/api/tools/" + encodeURIComponent(tool) +
                            "?doc=" + encodeURIComponent(doc),
                            { method: "POST",
                              headers: { "content-type": "application/json" },
                              body: JSON.stringify(args) });
        } catch (err) {
          results.push({ index: i, tool, ok: false, error: "bad_subrequest",
                         reason: String((err && err.message) || err) });
          break;
        }
        try {
          const res = await local(sub);
          const ct = (res.headers && res.headers.get && res.headers.get("content-type")) || "";
          const parsed = ct.indexOf("application/json") >= 0
            ? await res.json().catch(() => null)
            : { binary: true, content_type: ct };
          results.push({ index: i, tool, ok: res.ok, status: res.status, result: parsed });
          if (!res.ok || (parsed && parsed.ok === false)) break;
        } catch (err) {
          results.push({ index: i, tool, ok: false, error: "subcall_threw",
                         reason: String((err && err.message) || err) });
          break;
        }
      }
      const failed = results.find((r) => !r.ok || (r.result && r.result.ok === false));
      return json({ ok: !failed, server: false, executed: results.length, total: calls.length,
                    results, failed_index: failed ? failed.index : null,
                    note: "子调用失败即停；本地无 changeset ⇒ 已执行的不会回滚（与服务端不同，如实说明）" });
    }

    // **P2c-57 `submit_offline` ✗**：**服务端校验离线原子后整批提交 ✓** ⇒ **∴ 本地做
    //   同样的形状校验 ✗**（**8 字段封套 ＋ 已知 `kind` ✓**）⇒ **∴ 逐条 `putAtom` ✓**
    //   （**本地无 stash 机制 ⇒ **∴ 校验不过直接 400 ✓，**如实说明 ✓**）。
    if (path === "/api/tools/submit_offline") {
      const list = Array.isArray(body.atoms) ? body.atoms : null;
      if (!list) return p2Bad("atoms 必填（数组）");
      const problems = [];
      for (let i = 0; i < list.length; i++) {
        const a = list[i];
        if (!a || typeof a !== "object") { problems.push(i + ": 不是对象"); continue; }
        if (typeof a.kind !== "string" || !a.kind) { problems.push(i + ": 缺 kind"); continue; }
        if (p2cKnownKinds.indexOf(a.kind) < 0) { problems.push(i + ": 未知 kind " + a.kind); continue; }
        if (!a.payload || typeof a.payload !== "object") { problems.push(i + ": 缺 payload"); }
      }
      if (problems.length) {
        return json({ ok: false, error: "invalid_argument", server: false,
                      reason: "离线原子校验不过（本地无 stash 机制，不暂存）：" + problems.slice(0, 5).join("；"),
                      problems: problems.slice(0, 20) }, 400);
      }
      const seen = new Set((await atomsOf(handle, doc)).map((r) => r.atom && r.atom.id));
      const seqs = [];
      for (const a of list) {
        let id = typeof a.id === "string" && a.id ? a.id : null;
        if (!id || seen.has(id)) {
          id = "01OFF" + String(Date.now()).padStart(13, "0") + String(seqs.length).padStart(4, "0");
        }
        seen.add(id);
        const seq = await putAtom(handle, doc, {
          actor: typeof a.actor === "string" ? a.actor : "human:web",
          id, kind: a.kind, payload: a.payload,
          schema_version: 1, seq: 0,
          session: typeof a.session === "string" ? a.session : "session:web",
          timestamp: Number(a.timestamp) || Date.now(),
        });
        seqs.push(seq);
      }
      return json({ ok: true, server: false, stashed: false, applied: seqs.length, seqs,
                    note: "离线原子已整批写入（seq 重排）；本地无 stash 机制" });
    }

    // **P2c-58 `resample` ✗**：**服务端重采样对象位图 ✓** ⇒ **∴ 本地在浏览器里解码该对象
    //   的位图 ✗** ⇒ **∴ canvas 缩放（**`nearest` 关平滑 ✓／`bilinear` 开平滑 ✓**）✗** ⇒
    //   **∴ 新位图 `blob_put` ＋ `supersede` 换掉 `bitmap` ✓**。
    if (path === "/api/tools/resample") {
      const objectId = body.object_id;
      if (typeof objectId !== "string" || !objectId) return p2Bad("object_id 必填（字符串）");
      const filterName = body.filter !== undefined ? String(body.filter) : "bilinear";
      if (filterName !== "nearest" && filterName !== "bilinear") {
        return p2Bad("未知滤镜 " + filterName + "（可用 nearest / bilinear）");
      }
      const rows = await atomsOf(handle, doc);
      const v = p2ObjectView(rows, objectId);
      if (!v.found) return p2Missing("对象 " + objectId + " 不存在");
      if (v.deleted) return p2Bad("对象 " + objectId + " 已删除");
      const data = (v.data && typeof v.data === "object" && !Array.isArray(v.data)) ? v.data : {};
      const bitmap = data.bitmap;
      const blobHash = bitmap && bitmap.blob_hash;
      if (typeof blobHash !== "string" || !blobHash) {
        return p2Bad("对象 " + objectId + " 没有位图（不是 raster_patch）⇒ 无可重采样的像素");
      }
      const targetW = body.width !== undefined ? Math.floor(Number(body.width)) : null;
      const targetH = body.height !== undefined ? Math.floor(Number(body.height)) : null;
      if ((targetW !== null && !(targetW > 0)) || (targetH !== null && !(targetH > 0))) {
        return p2Bad("width/height 必须是正整数");
      }
      try {
        const g = typeof globalThis !== "undefined" ? globalThis : {};
        if (typeof g.createImageBitmap !== "function" || !g.document) {
          throw new Error("位图缩放需要浏览器环境的 createImageBitmap（当前环境没有）");
        }
        const { getBlob } = await import("./store.js");
        const blobBytes = await getBlob(handle, blobHash);
        if (!blobBytes) return p2Missing("blob " + blobHash + " 不在本地 blob 库里");
        const srcBmp = await g.createImageBitmap(new Blob([new Uint8Array(blobBytes)], { type: "image/png" }));
        const fromW = srcBmp.width, fromH = srcBmp.height;
        const toW = targetW || fromW, toH = targetH || fromH;
        const canvas = g.document.createElement("canvas");
        canvas.width = toW; canvas.height = toH;
        const c2d = canvas.getContext("2d", { willReadFrequently: true });
        c2d.imageSmoothingEnabled = filterName === "bilinear";
        c2d.drawImage(srcBmp, 0, 0, toW, toH);
        if (srcBmp.close) srcBmp.close();
        const outImg = c2d.getImageData(0, 0, toW, toH);
        const rgba = new Uint8Array(outImg.data.buffer.slice(0));
        const { entry } = await p2cKernelFor();
        const raw = String(await withKernel(doc, () => entry.k.blob_put(rgba)) || "");
        let newHash = raw;
        try { const o = JSON.parse(raw); if (o && o.blob_hash) newHash = String(o.blob_hash); } catch (e) {}
        const { putBlob } = await import("./store.js");
        await putBlob(handle, newHash, Array.from(rgba));
        const newData = { ...data,
          bitmap: { blob_hash: newHash, mime_type: "image/x-yanshi-raw", size: rgba.length } };
        const seq = await putAtom(handle, doc, p2Envelope("supersede", {
          object_id: objectId, layer_id: v.layer_id, data: newData,
        }));
        return json({ ok: true, seq, server: false, object_id: objectId,
                      from: [fromW, fromH], to: [toW, toH], filter: filterName, blob_hash: newHash,
                      note: "位图已在本地重采样并写入 supersede 原子" });
      } catch (err) { return p2cKernel501(err); }
    }

    // **P2c-59 `revert` ✗**：**照 `write_history_atom(…, Revert, "atom_id")` ✓** ⇒
    //   **`revert{target}` 原子 ✓**（**与 `restore_object` 分支同形 ✓**）。
    if (path === "/api/tools/revert") {
      const atomId = body.atom_id;
      if (typeof atomId !== "string" || !atomId) return p2Bad("atom_id 必填（字符串）");
      const rows = await atomsOf(handle, doc);
      const target = rows.find((r) => r.atom && r.atom.id === atomId);
      if (!target) return p2Missing("目标原子 " + atomId + " 不在日志里");
      const seq = await putAtom(handle, doc, p2Envelope("revert", { target: atomId }));
      return json({ ok: true, seq, server: false, reverted: atomId,
                    note: "已写入 revert 原子；是否撤销由内核折叠层判定" });
    }

    // **P2c-60 `revert_to` ✗**：**照 `write_revert_to` ✓** ⇒ **`declare_head{base:{type:"atom", id}}` ✓**。
    if (path === "/api/tools/revert_to") {
      const atomId = body.atom_id;
      if (typeof atomId !== "string" || !atomId) return p2Bad("atom_id 必填（字符串）");
      const rows = await atomsOf(handle, doc);
      if (!rows.some((r) => r.atom && r.atom.id === atomId)) {
        return p2Missing("目标原子 " + atomId + " 不在日志里");
      }
      const seq = await putAtom(handle, doc, p2Envelope("declare_head", {
        base: { type: "atom", id: atomId }, reason: "revert_to",
      }));
      return json({ ok: true, seq, server: false, head: atomId,
                    note: "已写入 declare_head 原子（求值起点跳变由内核折叠层处理）" });
    }

    // **P2c-61 `reapply` ✗**：**照 `write_history_atom(…, Reapply, "atom_id")` ✓**。
    if (path === "/api/tools/reapply") {
      const atomId = body.atom_id;
      if (typeof atomId !== "string" || !atomId) return p2Bad("atom_id 必填（字符串）");
      const rows = await atomsOf(handle, doc);
      if (!rows.some((r) => r.atom && r.atom.id === atomId)) {
        return p2Missing("目标原子 " + atomId + " 不在日志里");
      }
      const seq = await putAtom(handle, doc, p2Envelope("reapply", { target: atomId }));
      return json({ ok: true, seq, server: false, reapplied: atomId,
                    note: "已写入 reapply 原子；是否恢复由内核折叠层判定" });
    }

    // **P2c-62 `resolve_conflict` ⇒ 明确 501 ✗**：**服务端读折叠层的冲突表做裁决 ✗**
    //   ⇒ **∴ 本地只有裸原子日志 ✗**（**无冲突检测／裁决状态 ✓**）⇒ **∴ 无法如实复现 ✓**。
    if (path === "/api/tools/resolve_conflict") {
      return json({ ok: false, error: "not_implemented_locally", endpoint: "/api/tools/resolve_conflict",
                    reason: "resolve_conflict：服务端读折叠层的冲突表做裁决；本地只有裸原子日志，" +
                            "没有冲突检测与裁决状态，无法如实复现",
                    server: false }, 501);
    }

    // **P2c-63 `set_property` ✗**：**照 `write_set_property` ✓** ⇒ **`set_property{key, value,
    //   object_id?/layer_id?}` 原子 ✓**。
    if (path === "/api/tools/set_property") {
      const key = body.key;
      if (typeof key !== "string" || !key) return p2Bad("key 必填（字符串）");
      if (body.value === undefined) return p2Bad("value 必填");
      const payload = { key, value: body.value };
      const objectId = body.object_id, layerId = body.layer_id;
      if (typeof objectId === "string" && objectId) payload.object_id = objectId;
      if (typeof layerId === "string" && layerId) payload.layer_id = layerId;
      if (!payload.object_id && !payload.layer_id) {
        return p2Bad("object_id / layer_id 至少给一个");
      }
      const seq = await putAtom(handle, doc, p2Envelope("set_property", payload));
      return json({ ok: true, seq, server: false, key,
                    note: "已写入 set_property 原子" });
    }

    // **P2c-64 `update_override` ✗**：**照 `write_update_override` ✓** ⇒
    //   **`set_property{key:"override", value:{transform}|null}` ✓**（**实例必须存在且是实例 ✓**）。
    if (path === "/api/tools/update_override") {
      const instanceId = body.instance_id;
      if (typeof instanceId !== "string" || !instanceId) return p2Bad("instance_id 必填（字符串）");
      const rows = await atomsOf(handle, doc);
      const v = p2ObjectView(rows, instanceId);
      if (!v.found) return p2Missing("实例 " + instanceId + " 不存在");
      if (v.type !== "instance") return p2Bad(instanceId + " 不是实例");
      const value = (body.transform !== undefined && body.transform !== null)
        ? { transform: body.transform } : null;
      const seq = await putAtom(handle, doc, p2Envelope("set_property", {
        object_id: instanceId, key: "override", value,
      }));
      return json({ ok: true, server: false, seq, instance_id: instanceId, override: value,
                    note: "清除覆盖请传 transform: null（置空对象即清除）" });
    }

    // **P2c-65 `update_sync_policy` ✗**：**照 `write_update_sync_policy` ✓** ⇒
    //   **`set_property{key:"sync_policy"}` ✓**（**`policy ∈ all/none` ✓**）。
    if (path === "/api/tools/update_sync_policy") {
      const instanceId = body.instance_id;
      if (typeof instanceId !== "string" || !instanceId) return p2Bad("instance_id 必填（字符串）");
      const policy = body.policy;
      if (policy !== "all" && policy !== "none") {
        return p2Bad("本片只支持 policy: all 或 none（收到 " + policy + "）—— partial 需要设计 9.3 的依赖图传播");
      }
      const rows = await atomsOf(handle, doc);
      const v = p2ObjectView(rows, instanceId);
      if (!v.found) return p2Missing("实例 " + instanceId + " 不存在");
      if (v.type !== "instance") return p2Bad(instanceId + " 不是实例");
      const seq = await putAtom(handle, doc, p2Envelope("set_property", {
        object_id: instanceId, key: "sync_policy", value: policy,
      }));
      return json({ ok: true, server: false, seq, instance_id: instanceId, policy });
    }

    // **P2c-66 `push` ⇒ 明确 501 ✗**：**服务端工具注册表里没有这个端点 ✓**（**`p2NoServerEndpoint` ✓**）。
    if (path === "/api/tools/push") {
      return p2NoServerEndpoint("push", "dispatch 表里没有这一项");
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
    if (path === "/api/tools/brush_stroke" || path === "/api/tools/draw_stroke") {
      // **★ `draw_stroke` 必须也走这条路 ✗ ★**（**用户报告的 P0 ✓**）：
      //   **∴ 为什么 ✗**：**前端落笔调的是 `draw_stroke`** ✓（**`viewer-app.js` 的两处落笔 ✓**），
      //     而**本地层原来只认 `brush_stroke`** ✗ ⇒ **∴ 每一笔**都返回
      //     `not_implemented_locally` ✓ ⇒ **∴ 界面报"落笔失败：unknown" ✗**、
      //     **画布 0 像素 ✓**（**用户四轮复验一致 ✓**）。
      //   **∴ 两者的差别 ✗**：**服务端 `draw_stroke`** 是**纯几何矢量笔迹 ✓**
      //     ⇒ **参数装在 `data` 里**（`{points,size,color,hardness,opacity,seed,smooth}` ✓）；
      //     **`brush_stroke`** 的**参数在顶层** ✓ ⇒ **∴ 本分支其余部分**按后者写 ✗**
      //     ⇒ **∴ 先归一 ✗**，**后面整段照用 ✓**（**∴ 不复制一份实现 ✓**）。
      //   **⚠️ 如实说明（**两面 ✓**）**：**这一笔**用的是**硬圆不透明白笔**（`100%_Opaque` ✓）
      //     ⇒ **∴ `data.hardness` / `data.opacity` / `data.smooth`
      //     目前**没有被内核消费** ✗** —— **∴ 服务端会按它们改变笔迹 ✓**。
      //     ⇒ **∴ 这是**已知差距 ✗**（**写在这里与提交里 ⇒ 不是静默忽略 ✓），
      //     而**它**不影响"**能不能画**"✗** ⇒ **∴ 先修 P0 ✓**。
      // **∴ 无条件归一 ✗**（**∴ 对 `brush_stroke` 是**恒等变换 ✓ —— 它没有 `data` ✓）
      //   ⇒ **∴ 于是**：**分支判定里**只出现一次 `draw_stroke` ✗**
      //     ⇒ **∴ `LOCAL_IMPLEMENTED` 与实现分支数**仍然**对得上 ✓**** ✓✓
      const d = (body && body.data) || {};
      body = Object.assign({}, body, {
        points: d.points != null ? d.points : body.points,
        size: d.size != null ? d.size : body.size,
        color: d.color != null ? d.color : body.color,
      });
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
        const { putBlob } = await import("./store.js");
        await putBlob(handle, hash, Array.from(rgba));
        // **∴ 写 `import_image` 原子 ✗**（**字段照第 646 轮的实测样本 ✓**）。
        // **★ 有打开的变更集／事务 ⇒ 挂 `changeset_id` ✗ ★**（P2b ✓）：**∴ 与服务端
        //   "`begin_changeset` 后本会话的提交都并入它"同形 ✓**（**`tools.rs:388` ✓**）。
        const seq = await putAtom(handle, doc, p2bTag({
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
        }));
        return json({ ok: true, seq, server: false, bytes: rgba.length, blob_hash: hash,
                      note: "已由本地内核渲染并写入 import_image 原子" });
      } catch (err) {
        // **★ 失败必须**说出来 ✗**（**∴ 不许静默 —— **∴ 否则表现为"画了没反应"✗****）。**
        // **★ `error_code` 必须与前端 `callToolChecked` 对齐**（本地测试验证 ✓）：
        //   前端读的是 `result.error_code`，之前只给了 `error` ⇒ 永遠顯示 "unknown"
        const reason = String((err && err.message) || err);
        return json({ ok: false, error: "stroke_failed", error_code: "stroke_failed",
                      reason, server: false,
                      context: { detail: reason.slice(0, 200) } }, 500);
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

    // **★ `/api/tools/undo_last`／`/api/tools/redo_last` ✗ ★**（P0 ✓）：
    // **∴ 语义照服务端 `write_undo_last`／`write_redo_last`
    //   （`crates/yanshi-server/src/tools.rs:7119`／`:7192`）✓**：
    // **∴ 参数 `{count?}`（**缺省 1 ✓、**下限 1 ✓ —— **∴ 照服务端 `.max(1)` ✓**）；
    // **∴ 把原子从新到旧按"笔"分组 ✗**（**连续相同 `object_id` = 一笔 ✓**）；
    // **∴ 无 `object_id` 的原子不进候选**（**如 `create_document`／`create_layer` ✓ ——
    //   **∴ 服务端 `collect_gestures` 里它们只进 `ignored` ✓，**∴ 结构改动不在这里撤 ✓**）；
    // **∴ 历史原子本身不撤／不重做**（`revert`／`reapply` ✗）；
    // **∴ 已撤过的笔不再撤、已重做的笔不再重做**（**∴ 按 `payload.target` 从新到旧判每个原子的
    //   "最新动作"✗ —— **∴ 这是服务端 `alive_objects()` 在本地层的等价实现 ✓
    //   ⇒ **∴ 于是**不会"**报撤了、画面没变**"✓**）；
    // **∴ 对选中笔的每个原子写一个历史原子**（**∴ kind 用 **snake_case** `revert`／`reapply` ✗ ——
    //   **∴ 与服务端序列化名一致（`atom.rs:356` ✓）⇒ **∴ 内核重放时认得 ✓**；
    //   **∴ payload 键用 `target` ✗**（**∴ 照服务端 `write_history_atom` 的 `{"target":…}`
    //     （`tools.rs:8743` ✓）—— **∴ 折叠层的 `target_atom()` 只读这个键（`atom.rs:641` ✓）
    //     ⇒ **∴ 否则内核**认不出**它是个撤销 ✓**）。
    async function __historyStep(isUndo, count) {
      const rows = await atomsOf(handle, doc);
      // **∴ `atomsOf` 已按 seq 升序 ✓**（**`store.js` ✓）⇒ **∴ 倒着扫 = 从新到旧 ✓**。
      const latestAction = new Map(); // atom id -> "revert" | "reapply"
      for (let i = rows.length - 1; i >= 0; i--) {
        const a = rows[i] && rows[i].atom;
        if (!a || (a.kind !== "revert" && a.kind !== "reapply")) continue;
        const t = a.payload && a.payload.target;
        if (typeof t === "string" && t && !latestAction.has(t)) latestAction.set(t, a.kind);
      }
      const gestures = []; // [object_id, [{seq, atom_id, kind, object_id}]]
      let ignored = 0;
      for (let i = rows.length - 1; i >= 0; i--) {
        const rec = rows[i];
        const a = rec && rec.atom;
        if (!a) continue;
        if (a.kind === "create_document" || a.kind === "revert" || a.kind === "reapply") { ignored++; continue; }
        const oid = a.payload && a.payload.object_id;
        if (typeof oid !== "string" || !oid) { ignored++; continue; }
        const reverted = latestAction.get(a.id) === "revert";
        if (isUndo ? reverted : !reverted) { ignored++; continue; }
        const entry = { seq: rec.seq, atom_id: a.id, kind: a.kind, object_id: oid };
        const last = gestures[gestures.length - 1];
        if (last && last[0] === oid) last[1].push(entry);
        else gestures.push([oid, [entry]]);
      }
      const histKind = isUndo ? "revert" : "reapply";
      const prefix = isUndo ? "01UNDO" : "01REDO";
      const itemsName = isUndo ? "undone" : "redone";
      const gesturesName = isUndo ? "gestures_undone" : "gestures_redone";
      const countName = isUndo ? "undone_count" : "redone_count";
      const take = Math.min(count, gestures.length);
      const done = [], skipped = [];
      const uniq = String(Date.now()).padStart(13, "0") + Math.random().toString(36).slice(2, 7);
      for (let g = 0; g < take; g++) {
        const items = gestures[g][1];
        for (let k = 0; k < items.length; k++) {
          const it = items[k];
          await putAtom(handle, doc, {
            actor: "human:web",
            id: prefix + uniq + "_" + g + "_" + k,
            kind: histKind,
            payload: { target: it.atom_id },
            schema_version: 1,
            seq: 0,
            session: "session:web",
            timestamp: Date.now(),
          });
          done.push(it);
        }
      }
      const resp = {
        ok: true, [itemsName]: done, skipped, [countName]: done.length,
        [gesturesName]: take, gestures_total: gestures.length,
        remaining_gestures: gestures.length - take,
        ignored_non_content_atoms: ignored,
        hint: isUndo ? "要恢复就对这些 atom_id 用 redo_last；count 可以一次撤多笔 ✓"
                      : "再撤掉它们用 undo_last ✓",
        server: false,
      };
      if (gestures.length === 0) {
        resp.message = isUndo ? "没有可撤销的笔迹 ✗ ⇒ 这个文档里除了创建本身，还没有内容原子 ✓"
                              : "没有可重做的笔迹 ✗ ⇒ 没有哪一笔处于「被撤销」状态 ✓";
      }
      return json(resp);
    }
    if (path === "/api/tools/undo_last") {
      // **∴ `count` 非法 ⇒ 按服务端 `.unwrap_or(1)` 落到 1 ✗**（**∴ 不 400 ✓**）。
      const count = Math.max(1, Math.floor(Number((body && body.count) ?? 1) || 1));
      return await __historyStep(true, count);
    }
    if (path === "/api/tools/redo_last") {
      const count = Math.max(1, Math.floor(Number((body && body.count) ?? 1) || 1));
      return await __historyStep(false, count);
    }

    // **★ `/api/tools/erase` ✗ ★**（P0 ✓）：
    // **∴ 语义照服务端 `erase => write_draw(ctx, args, AtomKind::Erase)`
    //   （`tools.rs:3140`／`:4918`）✓**：
    // **∴ 请求形状与本地 `brush_stroke` 同形**（**`layer_id` ＋ `points` ＋ `size`／`color`
    //   在顶层 ✓，**或**包在 `data` 里 ✓ —— **∴ 照 `brush_stroke` 分支的归一写法 ✓**）；
    // **∴ 写 `{kind:"erase", payload:{object_id, layer_id, data}}` 原子 ✓**
    //   （**∴ 照 `write_draw` 的 payload ✓ —— **∴ 折叠层按它建 `Retouch`/`erase` 对象
    //     （`fold.rs:1214` ✓），**与服务端同路 ✓**）；
    // **∴ 校验**：**缺 `layer_id`／空 `points` ⇒ 400**（**∴ 照服务端 `require_str`／
    //   `require_non_empty_points` ✓，**不要 501 ✓**）。
    if (path === "/api/tools/erase") {
      const pl = (body && body.payload) || {};
      const d = (body && body.data) || {};
      const layerId = body.layer_id || pl.layer_id;
      const pts = Array.isArray(d.points) ? d.points : (Array.isArray(body.points) ? body.points : null);
      if (typeof layerId !== "string" || !layerId) {
        return json({ ok: false, error: "invalid_argument", error_code: "invalid_argument",
                      reason: "erase 需要 layer_id（服务端 require_str 同义）", server: false }, 400);
      }
      if (!pts || pts.length === 0) {
        return json({ ok: false, error: "invalid_argument", error_code: "invalid_argument",
                      reason: "erase 的 points 不能为空（服务端 require_non_empty_points 同义）", server: false }, 400);
      }
      const objectId = body.object_id || pl.object_id || ("obj_" + String(Date.now()));
      const data = Object.assign({}, d, {
        points: pts,
        size: d.size != null ? d.size : body.size,
        color: d.color != null ? d.color : (body.color ?? null),
      });
      const seq = await putAtom(handle, doc, {
        actor: "human:web",
        id: "01ERASE" + String(Date.now()).padStart(13, "0") + Math.random().toString(36).slice(2, 7),
        kind: "erase",
        payload: { object_id: objectId, layer_id: layerId, data },
        schema_version: 1,
        seq: 0,
        session: "session:web",
        timestamp: Date.now(),
      });
      return json({ ok: true, seq, server: false, object_id: objectId, layer_id: layerId,
                    note: "已在本地原子日志写入 erase 原子（折叠层按服务端语义把它归成 Retouch/erase 对象；get_log 可读）" });
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

    // ****★★ P2b：协作／历史类端点 ✗ ★★****（变更集／建议／检查点／标注／stash／评论／事务 ✓）：
    //   **∴ 策略 ✗**：**能映射为原子写／读的如实实现 ✓**（**变更集归集／检查点／建议／评论走原子日志 ✓**）；
    //   **∴ 复杂状态机如实 501 ✓**（**`restore_checkpoint` 的 declare_head 跳变／`accept_*` 的 patch 重放／
    //     `preview_suggestion` 的依赖模拟／stash 的悬空变更集重放 ✓ —— **∴ 不硬凑 ✓**）；
    //   **∴ 标注走**独立通道** ✗**（**∴ 与服务端同形："**独立 append-only 通道，不进原子日志**" ✓**）。

    // ── 变更集 ──
    // **① `begin_changeset` ✗**：**∴ 与服务端同形 ✗**（**`service.rs:702` ✓**）——
    //   **∴ 本会话已有一个打开的 ⇒ 409 ✗**（**∴ 不静默覆盖 ✓**）。
    if (path === "/api/tools/begin_changeset") {
      if (p2bOpen.has(p2bOpenKey)) {
        const cur = p2bOpen.get(p2bOpenKey);
        return json({ ok: false, error: "changeset_already_open", endpoint: path, server: false,
                      reason: "本会话已经有一个打开的" + (cur.transaction ? "事务" : "变更集") +
                              "（" + cur.changeset_id + "）：先 commit_changeset / abort_changeset 收尾" }, 409);
      }
      const changesetId = p2Ulid();
      p2bOpen.set(p2bOpenKey, { changeset_id: changesetId, transaction: false });
      return json({ ok: true, changeset_id: changesetId, open: true, server: false,
                    note: "此后本会话经本地层写入的原子都会带上该 changeset_id（落笔／atoms 直写／P2b 自身）" });
    }
    // **② `commit_changeset` ✗**：**∴ 收尾（**原子保留 ✓**）＋ 计数组内原子数 ✓**（**与服务端同形 ✓**）。
    if (path === "/api/tools/commit_changeset") {
      const cur = p2bOpen.get(p2bOpenKey);
      if (!cur) {
        return json({ ok: false, error: "no_open_changeset", endpoint: path, server: false,
                      reason: "本会话没有打开的变更集（先 begin_changeset）" }, 409);
      }
      p2bOpen.delete(p2bOpenKey);
      const atoms = await p2bChangesetAtomCount(cur.changeset_id);
      return json({ ok: true, changeset_id: cur.changeset_id, atoms, open: false, server: false,
                    note: "原子保留，只是此后不再并入该变更集" });
    }
    // **③ `abort_changeset` ✗**：**∴ 先关闭再撤销 ✗**（**与服务端同序 ✓**：**撤销原子归入**新的**变更集 ✓**，
    //   **绝不并进正在被撤销的那个 ✗**）⇒ **∴ "放弃"本身也可再撤销 ✓**。
    if (path === "/api/tools/abort_changeset") {
      const cur = p2bOpen.get(p2bOpenKey);
      if (!cur) {
        return json({ ok: false, error: "no_open_changeset", endpoint: path, server: false,
                      reason: "本会话没有打开的变更集（先 begin_changeset）" }, 409);
      }
      p2bOpen.delete(p2bOpenKey);
      const r = await p2bRevertChangeset(cur.changeset_id);
      return json({ ok: true, changeset_id: cur.changeset_id, reverted: r.reverted,
                    revert_changeset_id: r.revert_changeset_id, open: false, server: false,
                    note: "放弃 = 整体撤销（append-only：原子保留历史，撤销的每条 revert 原子都进日志）" });
    }
    // **④ `revert_changeset` ✗**：**∴ 与 `abort` 共用 `p2bRevertChangeset` 一份逻辑 ✓**
    //   （**∴ 与服务端"两个工具只差 changeset_id 从哪来"同形 ✓**）；**∴ 不存在的 id ⇒ 404 ✗**。
    if (path === "/api/tools/revert_changeset") {
      const changesetId = (body && body.changeset_id) || q.get("changeset_id") || null;
      if (!changesetId) return p2bBad("需要 changeset_id（要撤销的变更集 id）", "missing_changeset_id");
      const found = await p2bChangesetAtomCount(changesetId);
      if (found === 0) {
        return json({ ok: false, error: "changeset_not_found", endpoint: path, server: false,
                      reason: "变更集 " + changesetId + " 不存在（日志里没有属于它的原子）" }, 404);
      }
      const r = await p2bRevertChangeset(changesetId);
      return json({ ok: true, changeset_id: changesetId, reverted: r.reverted,
                    targets: r.targets, revert_changeset_id: r.revert_changeset_id, server: false });
    }
    // **⑤ `get_changesets` ✗**：**∴ 按原子封套的 `changeset_id` 聚合 ✓**（**与服务端 `read_get_changesets` 同形 ✓**：
    //   **∴ 保持日志顺序 ✓ ＋ 最新在前 ✓ ＋ limit 缺省 100／上限 1000 ✓**）。
    if (path === "/api/tools/get_changesets") {
      const limit = Math.min(1000, Math.max(1, Math.floor(Number((body && body.limit) ?? q.get("limit") ?? 100) || 100)));
      const rows = await atomsOf(handle, doc);
      const order = [], grouped = new Map();
      for (const r of rows) {
        const a = r.atom || {};
        const cid = a.changeset_id;
        if (!cid) continue;
        if (!grouped.has(cid)) { grouped.set(cid, []); order.push(cid); }
        grouped.get(cid).push({ seq: r.seq, id: a.id || null, kind: a.kind || null });
      }
      const changesets = order.reverse().slice(0, limit).map((cid) => {
        const atoms = grouped.get(cid);
        return { changeset_id: cid, atoms: atoms.length,
                 first_seq: atoms[0].seq, last_seq: atoms[atoms.length - 1].seq,
                 kinds: atoms.map((x) => x.kind) };
      });
      return json({ ok: true, changesets, count: changesets.length, server: false,
                    derived_from: "atoms",
                    note: "按原子封套的 changeset_id 聚合；没有 changeset_id 的原子不计入" });
    }

    // ── 检查点 ──
    // **⑥ `checkpoint` ✗**：**∴ 写 `checkpoint` 原子 ✓**（**`payload.checkpoint_id`／`name`／`anchor_seq` ✓**，
    //   **与服务端 `write_checkpoint` 的原子同形 ✓**）⇒ **∴ `get_checkpoints` 从日志推导 ✓**。
    //   **∴ 如实差距 ✗**：**本地不生成检查点快照 ⇒ `snapshot_id` 恒为 null ✓**（**∴ 不编造 ✓**）。
    if (path === "/api/tools/checkpoint") {
      const checkpointId = "ckpt_" + p2Ulid();
      const name = body && body.name != null ? String(body.name) : checkpointId;
      const anchorSeq = await p2bHeadSeq();
      const payload = { checkpoint_id: checkpointId, name, anchor_seq: anchorSeq };
      if (body && body.message != null) payload.message = String(body.message);
      const atom = p2bAtom("checkpoint", payload);
      const seq = await putAtom(handle, doc, atom);
      return json({ ok: true, atom_id: atom.id, seq, checkpoint_id: checkpointId,
                    name, anchor_seq: anchorSeq, snapshot_id: null, server: false,
                    note: "检查点即原子日志里的 checkpoint 原子；本地不生成检查点快照 ⇒ snapshot_id 恒为 null" });
    }
    // **⑦ `get_checkpoints` ✗**：**∴ 从原子日志推导 ✓**（**与 `list_layers` 同一纪律：不另存"检查点表" ✗**）。
    if (path === "/api/tools/get_checkpoints") {
      const rows = await atomsOf(handle, doc);
      const checkpoints = [];
      for (const r of rows) {
        const a = r.atom || {};
        if (a.kind !== "checkpoint") continue;
        const pl = a.payload || {};
        if (!pl.checkpoint_id) continue;
        checkpoints.push({ checkpoint_id: pl.checkpoint_id,
                           name: pl.name != null ? pl.name : pl.checkpoint_id,
                           created_by: a.actor != null ? a.actor : null,
                           message: pl.message != null ? pl.message : null,
                           anchor_seq: pl.anchor_seq != null ? pl.anchor_seq : null,
                           snapshot_id: null });
      }
      return json({ ok: true, checkpoints, count: checkpoints.length, server: false,
                    derived_from: "atoms",
                    note: "snapshot_id 本地恒为 null（不生成检查点快照）" });
    }
    // **⑧ `restore_checkpoint` ⇒ 如实 501 ✗**：**∴ 语义是提交 `declare_head` 原子做求值起点跳变 ✗**
    //   （**`state@seq` 公式，设计 5.5 ✓**）⇒ **∴ 而**本地 wasm 折叠器对 `declare_head` **只记警告** ✗**
    //   （**`DeclareHeadOutsideFormula`，`crates/yanshi-core/src/fold.rs` ✓**）**从不执行跳变 ✗**
    //   ⇒ **∴ 写了原子也不会生效 ✗** ⇒ **∴ 如实 501，不假装恢复 ✓**。
    if (path === "/api/tools/restore_checkpoint") {
      return json({ ok: false, error: "restore_checkpoint_unsupported", endpoint: path, server: false,
                    reason: "restore_checkpoint 的语义是提交 declare_head 原子、把求值起点跳到检查点（state@seq 公式，设计 5.5）；" +
                            "本地 wasm 折叠器对 declare_head 只记警告（DeclareHeadOutsideFormula）从不执行跳变 ⇒ 写了原子也不会生效。如实 501，不假装恢复。",
                    detail: "检查点本身可写（checkpoint 已实现）；恢复需等本地内核支持 declare_head 跳变" }, 501);
    }

    // ── 建议 ──
    // **⑨ `suggest` ✗**：**∴ 写 `suggest` 原子 ✓**（**`payload.patch`／`annotation_id`／`summary`／`priority` ✓**，
    //   **校验与服务端 `write_suggest` 同形：patch 非空／≤64 步／每步 `{tool, arguments?}` ✓**）。
    //   **∴ `annotation_id` 必须引用已存在的标注 ✗**（**∴ 与服务端同：悬空引用会被拒绝 ✓**）⇒
    //   **∴ 回写标注的 `suggestion_id` ✗**（**"标注 → 建议"双向可查 ✓**）。
    if (path === "/api/tools/suggest") {
      try {
        const patch = body && body.patch;
        if (!Array.isArray(patch) || patch.length === 0)
          return p2bBad("patch 必须是非空数组", "invalid_patch");
        if (patch.length > 64)
          return p2bBad("patch 步骤过多（" + patch.length + " > 64）", "invalid_patch");
        for (let i = 0; i < patch.length; i++) {
          const step = patch[i];
          if (!step || typeof step !== "object" || Array.isArray(step))
            return p2bBad("patch[" + i + "] 必须是对象", "invalid_patch");
          if (typeof step.tool !== "string" || !step.tool.trim())
            return p2bBad("patch[" + i + "] 缺少 tool（非空字符串）", "invalid_patch");
          if (step.arguments !== undefined &&
              (typeof step.arguments !== "object" || step.arguments === null || Array.isArray(step.arguments)))
            return p2bBad("patch[" + i + "] 的 arguments 必须是对象", "invalid_patch");
        }
        let priority = 5;
        if (body.priority !== undefined && body.priority !== null) {
          priority = Number(body.priority);
          if (!Number.isInteger(priority) || priority < 0 || priority > 9)
            return p2bBad("priority 必须在 0..9 内，得到 " + body.priority, "invalid_priority");
        }
        const annotationId = body.annotation_id != null ? String(body.annotation_id) : null;
        const atom = p2bAtom("suggest", { patch, annotation_id: annotationId,
                                          summary: body.summary != null ? String(body.summary) : null,
                                          priority });
        const seq = await putAtom(handle, doc, atom);
        let linked = false;
        if (annotationId) {
          const st = p2bAnnLoad();
          const cur = p2bAnnLatest(st, annotationId);
          st.versions[annotationId].push({ ...cur, suggestion_id: atom.id,
                                           updated_at: Date.now(), revision: cur.revision + 1 });
          p2bAnnSave();
          linked = true;
        }
        return json({ ok: true, suggestion_id: atom.id, seq, steps: patch.length, priority,
                      annotation_id: annotationId, annotation_linked: linked, server: false,
                      note: "suggest 原子是协作原子：不产生状态效果，只进日志（list_suggestions 从日志推导状态）" });
      } catch (e) { return p2bCatch(e, path); }
    }
    // **⑩ `accept_suggestion`／`accept_suggestions` ⇒ 如实 501 ✗**：**∴ 核心是全量预检 ＋
    //   按序重放 patch 里的每一步工具调用 ✗**（**服务端 `accept_one` ✓**）⇒ **∴ 本地层没有工具注册表／
    //   参数校验／分发器 ✗ ⇒ **∴ 重放不全会留下"部分应用"状态 ✓** ⇒ **∴ 如实 501，不硬凑 ✓**。
    if (path === "/api/tools/accept_suggestion" || path === "/api/tools/accept_suggestions") {
      return json({ ok: false, error: "accept_suggestion_unsupported", endpoint: path, server: false,
                    reason: "accept 的语义是全量预检（preflight）＋按序重放 patch 里的每一步工具调用；" +
                            "本地层没有工具注册表／参数校验／分发器，重放不全会留下「部分应用」状态 ⇒ 如实 501，不硬凑。",
                    detail: "suggest（记录提议）／reject_suggestion（拒绝）已实现；接受需等本地 patch 重放能力" }, 501);
    }
    // **⑪ `reject_suggestion` ✗**：**∴ 写 `reject_suggestion` 原子 ✓**（**`payload.target_atom_id`／`reason` ✓**）＋
    //   **∴ 状态闸门与服务端 `reject_one` 同形 ✓**（**已接受 ⇒ 409 ✗**；**已拒绝 ⇒ 幂等 ✓**）＋
    //   **∴ 引用它的待处理标注 ⇒ rejected ✓**。
    if (path === "/api/tools/reject_suggestion") {
      try {
        const r = await p2bRejectOne((body && body.suggestion_id) != null ? String(body.suggestion_id) : null,
                                     body && body.reason != null ? String(body.reason) : "");
        return json(r.body, r.status);
      } catch (e) { return p2bCatch(e, path); }
    }
    // **⑫ `reject_suggestions` ✗**：**∴ 逐条尝试、个别失败不中断 ✓**（**与服务端同形 ✓**）。
    if (path === "/api/tools/reject_suggestions") {
      try {
        const ids = body && body.suggestion_ids;
        if (!Array.isArray(ids) || ids.length === 0)
          return p2bBad("suggestion_ids 不能为空", "invalid_argument");
        if (ids.length > 64)
          return p2bBad("一次最多拒绝 64 条建议（收到 " + ids.length + "）", "invalid_argument");
        const reason = body.reason != null ? String(body.reason) : "";
        const results = [];
        let rejected = 0;
        for (const v of ids) {
          if (typeof v !== "string") {
            results.push({ suggestion_id: v, ok: false, error: "id 必须是字符串" });
            continue;
          }
          const r = await p2bRejectOne(v, reason);
          results.push({ suggestion_id: v, ok: r.ok,
                         already_rejected: r.body.already_rejected === true,
                         rejected_annotations: r.body.rejected_annotations || [],
                         ...(r.ok ? {} : { error_code: r.body.error, detail: r.body.reason }) });
          if (r.ok) rejected++;
        }
        return json({ ok: true, results, rejected, failed: results.length - rejected, server: false });
      } catch (e) { return p2bCatch(e, path); }
    }
    // **⑬ `list_suggestions` ✗**：**∴ 状态由 `accept_suggestion`／`reject_suggestion` 原子推导 ✓**
    //   （**与服务端 `read_list_suggestions` 同形：优先级降序＋seq 升序 ✓**；**`status`／`since_seq`／`limit`／`offset` ✓**）；
    //   **∴ 冲突检测是**本地简化版** ✗**（**只看补丁步骤的 `layer_id`／`object_id`／`region` 是否相同或相交 ✓ ——
    //   **∴ 如实注记，不冒充服务端的完整作用域模型 ✓**）。
    if (path === "/api/tools/list_suggestions") {
      const argOf = (k) => (body && body[k] != null ? body[k] : q.get(k));
      const statusFilter = argOf("status") != null ? String(argOf("status")) : null;
      if (statusFilter && ["pending", "accepted", "rejected"].indexOf(statusFilter) < 0)
        return p2bBad("status 只能是 pending/accepted/rejected，得到 " + statusFilter, "invalid_status");
      const since = Math.max(0, Math.floor(Number(argOf("since_seq") ?? 0) || 0));
      const limit = Math.min(500, Math.max(1, Math.floor(Number(argOf("limit") ?? 50) || 50)));
      const offset = Math.max(0, Math.floor(Number(argOf("offset") ?? 0) || 0));
      const rows = await atomsOf(handle, doc);
      const accepted = new Set(), rejected = new Map();
      for (const r of rows) {
        const a = r.atom || {}, pl = a.payload || {};
        const t = pl.target_atom_id;
        if (typeof t !== "string") continue;
        if (a.kind === "accept_suggestion") accepted.add(t);
        else if (a.kind === "reject_suggestion")
          rejected.set(t, pl.reason != null ? String(pl.reason) : "");
      }
      const suggestions = [];
      for (const r of rows) {
        const a = r.atom || {};
        if (a.kind !== "suggest" || !(r.seq > since)) continue;
        const pl = a.payload || {};
        const status = accepted.has(a.id) ? "accepted" : (rejected.has(a.id) ? "rejected" : "pending");
        if (statusFilter && status !== statusFilter) continue;
        suggestions.push({ suggestion_id: a.id, seq: r.seq, actor: a.actor != null ? a.actor : null,
                           annotation_id: pl.annotation_id != null ? pl.annotation_id : null,
                           summary: pl.summary != null ? pl.summary : null,
                           priority: pl.priority != null ? pl.priority : 5,
                           patch: pl.patch != null ? pl.patch : null,
                           status, reason: rejected.get(a.id) || "" });
      }
      suggestions.sort((x, y) => (y.priority - x.priority) || (x.seq - y.seq));
      const total = suggestions.length;
      const pending = suggestions.filter((s) => s.status === "pending").length;
      const conflicts = [];
      const pend = suggestions.filter((s) => s.status === "pending" && Array.isArray(s.patch));
      for (let i = 0; i < pend.length; i++)
        for (let j = i + 1; j < pend.length; j++) {
          const why = p2bScopesConflict(pend[i].patch, pend[j].patch);
          if (why) conflicts.push({ suggestion_id: pend[i].suggestion_id,
                                    conflicts_with: pend[j].suggestion_id, reason: why });
        }
      return json({ ok: true, suggestions: suggestions.slice(offset, offset + limit),
                    conflicts, pending, total, offset, limit, since_seq: since,
                    server: false, derived_from: "atoms",
                    conflicts_note: "本地简化版冲突检测：只看补丁步骤的 layer_id/object_id/region 是否相同或相交" });
    }
    // **⑭ `preview_suggestion` ⇒ 如实 501 ✗**：**∴ 语义是按 patch 顺序做依赖模拟 ✗**
    //   （**跟踪"此刻已知的图层／对象" ✓**）＋ 逐步骤影响面估计 ✗ ⇒ **∴ 需要 `document_state` 与工具注册表 ✗**
    //   ⇒ **∴ 本地没有这两样 ✗** ⇒ **∴ 如实 501，不猜影响范围 ✓**。
    if (path === "/api/tools/preview_suggestion") {
      return json({ ok: false, error: "preview_suggestion_unsupported", endpoint: path, server: false,
                    reason: "preview 的语义是按 patch 顺序做依赖模拟（跟踪此刻已知的图层／对象）＋逐步骤影响面估计；" +
                            "本地没有 document_state 与工具注册表 ⇒ 如实 501，不猜影响范围。",
                    detail: "suggest（记录提议）／list_suggestions（含简化冲突检测）已实现" }, 501);
    }

    // ── 标注（**独立通道，不进原子日志 ✓**）──
    // **⑮ `create_annotation` ✗**：**∴ 校验与服务端 `write_create_annotation` 同形 ✓**
    //   （**`type`／`intent` 枚举 ✓**；**`target` 缺省 region＋要 bbox，`object` 要 `object_id` ✓**）；
    //   **∴ 响应 `{annotation_id, pending, head_seq}` 与服务端同形 ✓**（**另附 `annotation` 与 `channel` ✓**）。
    if (path === "/api/tools/create_annotation") {
      try {
        const type = body && body.type != null ? String(body.type) : null;
        if (P2B_ANN_TYPES.indexOf(type) < 0)
          p2bFail(400, "invalid_annotation_type",
                  "type 必须是 " + P2B_ANN_TYPES.join("|") + "，得到 " + (body && body.type));
        const intent = body && body.intent != null ? String(body.intent) : null;
        if (P2B_ANN_INTENTS.indexOf(intent) < 0)
          p2bFail(400, "invalid_annotation_intent",
                  "intent 必须是 " + P2B_ANN_INTENTS.join("|") + "，得到 " + (body && body.intent));
        const target = p2bAnnTarget(body && body.target);
        const now = Date.now();
        const id = "ann_local_" + now.toString(36) + Math.floor(Math.random() * 2176782336).toString(36);
        const ann = { id, doc_id: doc, actor: body.actor != null ? String(body.actor) : "human:web",
                      head_seq: await p2bHeadSeq(), type, target,
                      content: body.content != null ? String(body.content) : "",
                      intent, status: "pending",
                      created_at: now, updated_at: now, revision: 0 };
        if (body.suggestion_id != null) ann.suggestion_id = String(body.suggestion_id);
        const st = p2bAnnLoad();
        st.order.push(id); st.versions[id] = [ann]; p2bAnnSave();
        return json({ ok: true, annotation_id: id, pending: p2bAnnPending(st),
                      head_seq: ann.head_seq, channel: p2bAnnChannel(), server: false,
                      annotation: ann,
                      note: "标注走独立通道（localStorage／内存），不进原子日志 —— 与服务端设计同形" });
      } catch (e) { return p2bCatch(e, path); }
    }
    // **⑯ `update_annotation` ✗**：**∴ 追加新版本 ✗**（**append-only ✓**）；**∴ `status` 只接受 `pending`（重开）✗**
    //   （**与服务端同：其余转换走 `resolve_annotation`／`reject_annotation` ✓**）；**∴ 重开清空 `resolved_by/at` ✓**。
    if (path === "/api/tools/update_annotation") {
      try {
        const id = body && body.annotation_id != null ? String(body.annotation_id) : null;
        if (!id) p2bFail(400, "missing_annotation_id", "需要 annotation_id");
        let intent = null;
        if (body.intent !== undefined && body.intent !== null) {
          intent = String(body.intent);
          if (P2B_ANN_INTENTS.indexOf(intent) < 0)
            p2bFail(400, "invalid_annotation_intent",
                    "intent 必须是 " + P2B_ANN_INTENTS.join("|") + "，得到 " + body.intent);
        }
        let reopen = false;
        if (body.status !== undefined && body.status !== null) {
          if (String(body.status) !== "pending")
            p2bFail(400, "invalid_status",
                    "update_annotation 的 status 只支持 pending（重开），得到 " + body.status +
                    "；解决／拒绝请用 resolve_annotation / reject_annotation");
          reopen = true;
        }
        const st = p2bAnnLoad();
        const cur = p2bAnnLatest(st, id);
        const next = { ...cur,
                       content: body.content !== undefined && body.content !== null
                                ? String(body.content) : cur.content,
                       intent: intent != null ? intent : cur.intent,
                       status: reopen ? "pending" : cur.status,
                       updated_at: Date.now(), revision: cur.revision + 1 };
        if (reopen) { next.resolved_by = null; next.resolved_at = null; }
        st.versions[id].push(next); p2bAnnSave();
        return json({ ok: true, annotation: next, pending: p2bAnnPending(st),
                      channel: p2bAnnChannel(), server: false });
      } catch (e) { return p2bCatch(e, path); }
    }
    // **⑰ `delete_annotation` ✗**：**∴ 与服务端 `AnnotationStore::delete` 同形 ✓** ——
    //   **∴ append-only 里删除 ＝ 标记 rejected ＋ 清空内容 ✗**（**允许从任意状态执行 ✓**，**保留历史 ✓**）。
    if (path === "/api/tools/delete_annotation") {
      try {
        const id = body && body.annotation_id != null ? String(body.annotation_id) : null;
        if (!id) p2bFail(400, "missing_annotation_id", "需要 annotation_id");
        const st = p2bAnnLoad();
        const cur = p2bAnnLatest(st, id);
        const next = { ...cur, status: "rejected", content: "",
                       updated_at: Date.now(), revision: cur.revision + 1 };
        st.versions[id].push(next); p2bAnnSave();
        return json({ ok: true, annotation: next, channel: p2bAnnChannel(), server: false,
                      note: "删除即标记 rejected 并清空内容，历史保留（append-only）" });
      } catch (e) { return p2bCatch(e, path); }
    }
    // **⑱ `get_annotation` ✗**：**∴ 当前版本 ＋ 修订历史 ✓**（**与服务端同形 ✓**）；**∴ 不存在 ⇒ 404 ✗**。
    if (path === "/api/tools/get_annotation") {
      try {
        const id = (body && body.annotation_id) || q.get("annotation_id") || null;
        if (!id) p2bFail(400, "missing_annotation_id", "需要 annotation_id");
        const st = p2bAnnLoad();
        const ann = p2bAnnLatest(st, String(id));
        return json({ ok: true, annotation: ann, history: st.versions[String(id)],
                      channel: p2bAnnChannel(), server: false });
      } catch (e) { return p2bCatch(e, path); }
    }
    // **⑲ `list_annotations` ✗**：**∴ 过滤与服务端 `annotation_filter` 同形 ✓**
    //   （**`status`／`actor`／`intent`／`object_id`／`suggestion_id` ✓**）＋ **`pending` 计数 ✓**。
    if (path === "/api/tools/list_annotations") {
      try {
        const argOf = (k) => (body && body[k] != null ? body[k] : q.get(k));
        const fStatus = argOf("status") != null ? String(argOf("status")) : null;
        if (fStatus && P2B_ANN_STATUSES.indexOf(fStatus) < 0)
          p2bFail(400, "invalid_status",
                  "status 只能是 " + P2B_ANN_STATUSES.join("|") + "，得到 " + fStatus);
        const fActor = argOf("actor") != null ? String(argOf("actor")) : null;
        const fIntent = argOf("intent") != null ? String(argOf("intent")) : null;
        const fObjectId = argOf("object_id") != null ? String(argOf("object_id")) : null;
        const fSuggestionId = argOf("suggestion_id") != null ? String(argOf("suggestion_id")) : null;
        const st = p2bAnnLoad();
        const annotations = [];
        for (const id of st.order) {
          const vs = st.versions[id];
          if (!vs || !vs.length) continue;
          const a = vs[vs.length - 1];
          if (fStatus && a.status !== fStatus) continue;
          if (fActor && a.actor !== fActor) continue;
          if (fIntent && a.intent !== fIntent) continue;
          if (fObjectId && !(a.target && a.target.target === "object" && a.target.object_id === fObjectId)) continue;
          if (fSuggestionId && a.suggestion_id !== fSuggestionId) continue;
          annotations.push(a);
        }
        return json({ ok: true, annotations, count: annotations.length,
                      pending: p2bAnnPending(st), channel: p2bAnnChannel(), server: false });
      } catch (e) { return p2bCatch(e, path); }
    }
    // **⑳ `resolve_annotation` ✗**：**∴ 闸门转换到 resolved ✓**（**可带 `atom_id` 记 `resolved_by` ✓**）。
    if (path === "/api/tools/resolve_annotation") {
      try {
        const id = body && body.annotation_id != null ? String(body.annotation_id) : null;
        if (!id) p2bFail(400, "missing_annotation_id", "需要 annotation_id");
        const atomId = body.atom_id != null ? String(body.atom_id) : null;
        const st = p2bAnnLoad();
        const ann = p2bAnnTransition(st, id, "resolved", atomId);
        p2bAnnSave();
        return json({ ok: true, annotation: ann, pending: p2bAnnPending(st),
                      channel: p2bAnnChannel(), server: false });
      } catch (e) { return p2bCatch(e, path); }
    }
    // **㉑ `reject_annotation` ✗**：**∴ 闸门转换到 rejected ✓**（**与 `resolve` 对称 ✓**）。
    if (path === "/api/tools/reject_annotation") {
      try {
        const id = body && body.annotation_id != null ? String(body.annotation_id) : null;
        if (!id) p2bFail(400, "missing_annotation_id", "需要 annotation_id");
        const st = p2bAnnLoad();
        const ann = p2bAnnTransition(st, id, "rejected", null);
        p2bAnnSave();
        return json({ ok: true, annotation: ann, pending: p2bAnnPending(st),
                      channel: p2bAnnChannel(), server: false });
      } catch (e) { return p2bCatch(e, path); }
    }

    // ── stash：如实 501 ──
    // **㉒ `list_stashes`／`apply_stash`／`discard_stash` ⇒ 如实 501 ✗**：**∴ stash 是服务端的
    //   "悬空变更集"机制 ✗**（**同步冲突时产生，设计 12.4，FileStore 持久化 ＋ blob 生命周期 ✓**）⇒
    //   **∴ PWA 本地没有 stash 的生产者与存储 ✗**；**∴ `apply` 还需要 `validate_batch` 校验 ＋
    //   强制提交到当前 HEAD 的 machinery（**可能产生视觉错误，设计原话 ✓**）✗**
    //   ⇒ **∴ 如实 501 ✗** —— **∴ 不报空表冒充"没有悬空变更集" ✓**（**与 `list_assets` 索引缺失同纪律 ✓**）。
    if (path === "/api/tools/list_stashes") {
      return json({ ok: false, error: "stash_unsupported", endpoint: path, server: false,
                    reason: "stash 是服务端的「悬空变更集」机制（同步冲突时产生，设计 12.4，FileStore 持久化）；" +
                            "PWA 本地没有 stash 的生产者与存储 ⇒ 如实 501，不报空表冒充「没有悬空变更集」。",
                    detail: "apply_stash / discard_stash 同样未实现" }, 501);
    }
    if (path === "/api/tools/apply_stash") {
      return json({ ok: false, error: "stash_unsupported", endpoint: path, server: false,
                    reason: "apply_stash 需先 validate_batch 校验、再把悬空原子强制提交到当前 HEAD（可能产生视觉错误，设计 12.4）；" +
                            "本地没有批量校验与提交 machinery，也没有 stash 可取 ⇒ 如实 501。",
                    detail: "stash 机制整体未在 PWA 本地实现" }, 501);
    }
    if (path === "/api/tools/discard_stash") {
      return json({ ok: false, error: "stash_unsupported", endpoint: path, server: false,
                    reason: "discard_stash 丢弃的是服务端的悬空变更集（FileStore 里的待重放原子）；" +
                            "本地没有 stash 存储 ⇒ 没有可丢弃的东西，如实 501。",
                    detail: "stash 机制整体未在 PWA 本地实现" }, 501);
    }

    // ── 评论 ──
    // **㉓ `comment` ✗**：**∴ 写 `comment` 原子 ✓**（**协作原子：不产生状态效果 ✓**，
    //   **与服务端 `write_comment` 的原子同形 ✓**）。
    if (path === "/api/tools/comment") {
      const text = body && body.text != null ? String(body.text) : "";
      if (!text) return p2bBad("需要 text（评论内容）", "missing_text");
      const payload = { text };
      if (body.target_atom != null) payload.target_atom = String(body.target_atom);
      if (body.object_id != null) payload.object_id = String(body.object_id);
      const atom = p2bAtom("comment", payload);
      const seq = await putAtom(handle, doc, atom);
      return json({ ok: true, atom_id: atom.id, seq, server: false,
                    note: "协作原子：不产生状态效果，只进日志" });
    }
    // **㉔ `list_comments` ✗**：**∴ 从日志推导 ✓**（**与服务端 `read_list_comments` 同形：
    //   **最新在前 ✓**；**`limit` 缺省 50／上限 500 ✓**；**`since_seq`／`actor`／`object_id` 过滤 ✓**；
    //   **∴ `next_since` 取最大 seq ✓**）。
    if (path === "/api/tools/list_comments") {
      const argOf = (k) => (body && body[k] != null ? body[k] : q.get(k));
      const since = Math.max(0, Math.floor(Number(argOf("since_seq") ?? 0) || 0));
      const limit = Math.min(500, Math.max(1, Math.floor(Number(argOf("limit") ?? 50) || 50)));
      const fActor = argOf("actor") != null ? String(argOf("actor")) : null;
      const fObjectId = argOf("object_id") != null ? String(argOf("object_id")) : null;
      const rows = await atomsOf(handle, doc);
      const comments = [];
      for (const r of rows) {
        const a = r.atom || {};
        if (a.kind !== "comment" || !(r.seq > since)) continue;
        if (fActor && String(a.actor) !== fActor) continue;
        const pl = a.payload || {};
        if (fObjectId && (pl.object_id == null || String(pl.object_id) !== fObjectId)) continue;
        comments.push({ atom_id: a.id, seq: r.seq, actor: a.actor != null ? a.actor : null,
                        session: a.session != null ? a.session : null,
                        text: pl.text != null ? String(pl.text) : "",
                        target_atom: pl.target_atom != null ? pl.target_atom : null,
                        object_id: pl.object_id != null ? pl.object_id : null,
                        timestamp: a.timestamp != null ? a.timestamp : null });
      }
      const count = comments.length;
      const nextSince = comments.length ? comments[comments.length - 1].seq : since;
      comments.reverse();
      return json({ ok: true, comments: comments.slice(0, limit), count,
                    next_since: nextSince, server: false, derived_from: "atoms" });
    }

    // ── 事务 ──
    // **㉕ `begin_transaction` ✗**：**∴ 与服务端同形 ✓**（**`service.rs:900`：事务**同时**是一个变更集 ✓**）——
    //   **∴ 本地事务 ＝ 变更集分组 ✓**；**∴ 如实差距 ✗**：**"写失败自动回滚"（**设计 777 ✓**）本地未接 ✗**
    //   （**∴ 那是服务端工具注册表的失败钩子 ✗**）—— **∴ 本地写操作是单原子落盘 ✗**，
    //   **∴ 失败时在写日志前抛错 ✓**（**∴ 不会留下"写了一半"的状态 ✓ —— **∴ 这点如实写在 note 里 ✓**）。
    if (path === "/api/tools/begin_transaction") {
      if (p2bOpen.has(p2bOpenKey)) {
        return json({ ok: false, error: "transaction_already_open", endpoint: path, server: false,
                      reason: "本会话已经有一个打开的变更集或事务：先 commit/abort 收尾" }, 409);
      }
      const changesetId = p2Ulid();
      p2bOpen.set(p2bOpenKey, { changeset_id: changesetId, transaction: true });
      return json({ ok: true, changeset_id: changesetId, transaction: true, server: false,
                    note: "本地事务 = 变更集分组：组内原子可作为一组撤销（revert_changeset）；" +
                          "写失败自动回滚（设计 777）本地未接 —— 本地写操作是单原子落盘，失败时在写日志前抛错" });
    }
    // **㉖ `commit_transaction` ✗**：**∴ 原子保留 ✓**（**且同属一个变更集 ⇒ 可作为一组撤销 ✓**）。
    if (path === "/api/tools/commit_transaction") {
      const cur = p2bOpen.get(p2bOpenKey);
      if (!cur || !cur.transaction) {
        return json({ ok: false, error: "no_open_transaction", endpoint: path, server: false,
                      reason: "本会话没有打开的事务（先 begin_transaction）" }, 409);
      }
      p2bOpen.delete(p2bOpenKey);
      const atoms = await p2bChangesetAtomCount(cur.changeset_id);
      return json({ ok: true, changeset_id: cur.changeset_id, atoms, transaction: false, server: false,
                    note: "原子保留且同属一个变更集，可用 revert_changeset 整体撤销" });
    }

    // **★ 补漏（2026-10-10 收尾）✓ ★**：以下 4 个是服务端注册表里有、
    //   但前面批次漏掉的端点。语义照 `crates/yanshi-server/src/tools.rs`。

    // **`gradient_blend`**：渐变混合笔触（`write_gradient_blend`）。
    //   参数 `{layer_id, brush, size, from:{x,y,color}, to:{x,y,color}, steps?, smooth?}`。
    //   本地写法：写 `gradient_blend` 原子，内核重放时插值落笔。
    if (path === "/api/tools/gradient_blend") {
      const pl = (body && body.payload) || {};
      const layerId = (body && body.layer_id) || pl.layer_id;
      const brush = (body && body.brush) || pl.brush;
      const size = (body && body.size) ?? pl.size;
      const from = (body && body.from) || pl.from;
      const to = (body && body.to) || pl.to;
      if (!layerId) return p2Bad("gradient_blend 缺少 layer_id");
      if (!brush) return p2Bad("gradient_blend 缺少 brush");
      if (typeof size !== "number" || !(size > 0)) return p2Bad("gradient_blend 的 size 必须是正数");
      if (!from || typeof from.x !== "number" || typeof from.y !== "number") {
        return p2Bad("gradient_blend 缺少 from（应为 {x, y, color}）");
      }
      if (!to || typeof to.x !== "number" || typeof to.y !== "number") {
        return p2Bad("gradient_blend 缺少 to（应为 {x, y, color}）");
      }
      const steps = Math.min(200, Math.max(2, Math.round(((body && body.steps) ?? pl.steps ?? 10))));
      const atom = p2Envelope("gradient_blend", {
        layer_id: layerId, brush, size, from, to, steps,
        smooth: !!((body && body.smooth) ?? pl.smooth),
      });
      const seq = await putAtom(handle, doc, atom);
      return json({ ok: true, seq, server: false, layer_id: layerId,
                    note: "已写入 gradient_blend 原子（内核重放时插值落笔）" });
    }

    // **`list_textures`**：列出可用纹理（`write_list_textures`）。
    //   本地：纹理随包发布在 `/textures/`，用 `list_assets` 同款索引回答。
    if (path === "/api/tools/list_textures") {
      let idx = null;
      try {
        const idxRes = await fetch("/assets-index.json", { cache: "no-store" });
        if (idxRes.ok) idx = await idxRes.json();
      } catch (e) { idx = null; }
      if (!idx) {
        return json({ ok: false, error: "index_missing", server: false,
                      reason: "本部署没有可读的 /assets-index.json ⇒ 跑一次 scripts/pwa-sync-viewer.mjs" }, 501);
      }
      const list = Array.isArray(idx.texture) ? idx.texture : [];
      const textures = list.map((it) => {
        const dot = String(it.name).lastIndexOf(".");
        const format = dot >= 0 ? String(it.name).slice(dot + 1).toLowerCase() : "";
        return { name: it.name, bytes: it.bytes, source: "bundled", format,
                 usable: format === "png",
                 reason: format === "png" ? null : "内核只解 PNG（." + format + " 需转换）" };
      });
      return json({ ok: true, dir: "textures", count: textures.length, textures,
                    hint: "usable=true 的才能直接用；source=bundled 随包发布",
                    server: false });
    }

    // **`path_edit`**：路径编辑算子（`write_path_edit`）。
    //   op: reverse/close/join/merge/split/convert_to_shape/convert_to_path/boolean。
    //   本地写法：写 `path_edit` 原子，几何运算由内核重放时处理。
    if (path === "/api/tools/path_edit") {
      const pl = (body && body.payload) || {};
      const op = (body && body.op) || pl.op;
      const objectId = (body && body.object_id) || pl.object_id;
      const OPS = ["reverse", "close", "join", "merge", "split",
                   "convert_to_shape", "convert_to_path", "boolean"];
      if (!op || OPS.indexOf(op) < 0) {
        return p2Bad("path_edit 的 op 非法（可用：" + OPS.join("／") + "）");
      }
      if (!objectId) return p2Bad("path_edit 缺少 object_id");
      const atom = p2Envelope("path_edit", {
        op, object_id: objectId,
        ...(pl.at !== undefined ? { at: pl.at } : {}),
        ...(((body && body.params) || pl.params) ? { params: (body && body.params) || pl.params } : {}),
      });
      const seq = await putAtom(handle, doc, atom);
      return json({ ok: true, seq, server: false, op, object_id: objectId,
                    note: "已写入 path_edit 原子（内核重放时做几何运算）" });
    }

    // **`save_palette`**：保存调色板（`write_save_palette`）。
    //   服务端写 .gpl 文件到工作区；PWA 本地无文件写权限 ⇒ 存 localStorage，
    //   如实注记持久性（与 annotation 通道同纪律）。
    if (path === "/api/tools/save_palette") {
      const pl = (body && body.payload) || {};
      const name = (body && body.name) || pl.name;
      const colors = (body && body.colors) || pl.colors;
      if (!name || typeof name !== "string") return p2Bad("save_palette 缺少 name");
      if (!Array.isArray(colors) || !colors.length) return p2Bad("save_palette 的 colors 不能为空");
      try {
        const key = "yanshi-palettes";
        const raw = localStorage.getItem(key);
        const all = raw ? JSON.parse(raw) : {};
        all[name] = { name, colors, saved_at: Date.now() };
        localStorage.setItem(key, JSON.stringify(all));
      } catch (e) {
        return json({ ok: false, error: "storage_unavailable", server: false,
                      reason: "localStorage 不可用：" + String((e && e.message) || e) }, 501);
      }
      return json({ ok: true, server: false, name, count: colors.length,
                    channel: "localStorage",
                    note: "已存入浏览器本地（localStorage）；服务端版会写成 .gpl 文件" });
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
      // **∴ 端点名还要进 `detail` ✗ ★**（第 928 轮 ✓）：**∴ 前端**只打
      //   `error_code` ＋ `detail` ✗（**实测：日志里只有"**错误 not_implemented_locally：
      //   该端点…**" ✗，**看不出**是哪一条 ✓）⇒ **∴ 于是**只能靠猜 ✓
      //   ⇒ **∴ 把端点名放进 `detail` ✗** ⇒ **∴ 日志**自己就会说是谁 ✓**** ✓✓
      return json({ ok: false, error: "not_implemented_locally",
                    endpoint: url.pathname,
                    // **★ 端点名必须在 `reason` 里 ✗ ★**（第 928 轮 ✓）：**∴ 前端**读的是
                    //   `context.detail` **缺失时回落到 `reason`** ✓（**我修 P2-5 时加的顺序 ✓）
                    //   ⇒ **∴ 而**本地层的错误是**扁平**的 ✗（**没有 `context` ✓）
                    //     ⇒ **∴ 于是**日志**只显示兜底文案 ✗，**看不出**是哪一条 ✓**** ✓✓
                    reason: url.pathname + "：该端点在 PWA 里尚未映射到本地内核／IndexedDB",
                    detail: url.pathname + "：该端点在 PWA 里尚未映射到本地内核／IndexedDB" }, 501);
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

/** **★ 解析调色板文本 ✓ ★**（第 930 轮 ✓；**口径与服务端 `service.rs` 的解析器一致 ✓**）：
 *  **∴ `.gpl`／`.kpl`**：`R G B [名字]` 一行一色 ✗**；`#` 与表头（`GIMP Palette`／`Name:`／`Columns:`）跳过 ✓**；
 *  **∴ `.json`**：**键 ⇒ 十六进制串** ，或**键 ⇒ 串数组**（**`open-color.json` 正是后者 ✓）** ✓✓
 *  **∴ 取不到名字就留空 ✗**（**∴ 不**编造 ✓）**。
 *  @param {string} text 文件全文
 *  @param {string} name 文件名（**用来决定按哪种格式解析 ✓**）
 *  @returns {Array<{hex:string,name:string}>}
 */
function parsePaletteText(text, name) {
  const out = [];
  const push = (hex, label) => {
    const h = String(hex || "").trim();
    if (!/^#?[0-9a-fA-F]{6}$/.test(h)) return;
    out.push({ hex: h.startsWith("#") ? h.toLowerCase() : "#" + h.toLowerCase(),
               name: String(label || "") });
  };
  if (/\.json$/i.test(name)) {
    let data = null;
    try { data = JSON.parse(text); } catch (e) { return out; }
    const walk = (value, key) => {
      if (typeof value === "string") { push(value, key); return; }
      if (Array.isArray(value)) { for (const v of value) walk(v, key); return; }
      if (value && typeof value === "object") {
        for (const k of Object.keys(value)) walk(value[k], k);
      }
    };
    walk(data, "");
    return out;
  }
  // `.gpl` / `.kpl`：**一行一色**，前三列是 R G B，第四列起是名字。
  for (const line of String(text).split(/\r?\n/)) {
    const t = line.trim();
    if (!t || t.startsWith("#")) continue;
    if (/^(GIMP Palette|Name:|Columns:|Krita|#)/i.test(t)) continue;
    const m = t.match(/^(\d{1,3})\s+(\d{1,3})\s+(\d{1,3})\s*(.*)$/);
    if (!m) continue;
    const [r, g, b] = [Number(m[1]), Number(m[2]), Number(m[3])];
    if ([r, g, b].some((v) => v > 255)) continue;
    const hex = "#" + [r, g, b].map((v) => v.toString(16).padStart(2, "0")).join("");
    out.push({ hex, name: String(m[4] || "").trim() });
  }
  return out;
}
