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
 *   **★ 后续更新 ✗（第 六百六十三 轮 ✓）**：**`restore_checkpoint` 已落地 ✓**（**`declare_head{base:{type:"checkpoint"}}`
 *     ＋ 本地 `state@seq` 窗口 ✓**）⇒ **∴ 上面那句里的 501 名单已过期 ✗**（**∴ 以本节与实现分支为准 ✓**）。
 *   **★ 后续更新 ✗（第 六百六十四 轮 ✓）**：**`preview_suggestion`／`accept_suggestion`／`accept_suggestions`
 *     已落地 ✓**（**预览＝逐步 `p2CheckSingleStep` ＋ 影响预估 ✓；接受＝全量预检 ⇒ 按序重放
 *     ⇒ 运行期失败逐条 `revert` 回滚 ✓；批量＝逐条、个别失败不中断 ✓**）⇒ **∴ 上面那句里的 501 名单又少 3 个 ✗**
 *     （**∴ 以本节与实现分支为准 ✓**）。
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
 *   **★ 后续更新 ✗**：**`estimate_dehaze` 已落地 ✓**（第 六百六十二 轮 ✓，f32 逐位移植 ✓）⇒ **∴ 名单里它已过期 ✗**。
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
//   **∴ 本轮再重数 ✗**（锚点三件套 ✓ —— `set_anchor`／`get_anchors`／`delete_anchor` 落地 ⇒ **+3 ✓**；
//   **同轮 `reorder_objects` ✓ —— `tools.rs:4029` 的校验序列落地 ⇒ 再 +1 ✓，总数 164 ✓**；
//   **本轮 `revert_atoms`＋`revert_group` ✓ —— `tools.rs:19361`／`:19261` 落地 ⇒ 再 +2 ✓，总数 166 ✓**；
//   **第 2 批 风格预设四端点 ✓ —— `list_presets`／`get_preset`／`preview_preset`／`apply_preset` 落地 ⇒ 再 +4 ✓**；
//   **第 4 批 读测三端点 ✓ —— `list_effects`／`sample_colors`／`get_histogram` 落地 ⇒ 再 +3 ✓**；
//   **第 5 批 原子日志两端点 ✓ —— `get_stroke_log`／`list_atoms` 落地 ⇒ 再 +2 ✓**；
//   **第 6 批 阶段回退 ✓ —— `revert_to_seq` 落地 ⇒ 再 +1 ✓**；
//   **第 7 批 矢量命中 ✓ —— `vector_hit_test` 落地 ⇒ 再 +1 ✓**；
//   **第 8 批 路径读回 ✓ —— `get_path` 落地 ⇒ 再 +1 ✓**；
//   **第 9 批 路径节点编辑 ✓ —— `edit_path` 落地 ⇒ 再 +1 ✓**）：
//   **∴ 判据口径 `actualImpl`（**机械重数：`(?:url\.pathname|path) === "`**）⇒ 本数与它同步 ✓（非拍脑袋 ✗）**
//   **∴ 数它用：`node -e 'const a=require("fs").readFileSync("web/api-local.js","utf8");console.log(((a.match(/(?:url\.pathname|path) === "/g)||[]).length))'` ✓**
const LOCAL_IMPLEMENTED = 179; // 路径节点编辑落地 ⇒ 机械重数 178 + 1 ✓

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

/** **state@seq 求值窗口 ✗**（**照 `yanshi-core/src/seq.rs` 的 `state_at` ✓**，设计 5.5 ✓）：
 *   **∴ `state@seq_n := fold(A_n, base_state(H_n))` ✗**：
 *     `H_n` = `seq ≤ n` 中**最近一次** `declare_head`；`A_n` = `(H_n.seq, n]`；
 *     `base_state(H)` = base 指向的状态（atom ⇒ 该原子时刻的 state@seq；
 *     checkpoint ⇒ `payload.anchor_seq ?? atom.seq` 的 state@seq ✓，递归 seq 严格递减 ✓）。
 *   **∴ 于是「日志里的原子」≠「状态里的原子」✗**：`declare_head` 自身与 base 到它之间的
 *     那段原子**都不在状态里** ✓（`seq.rs` 模块文档 ✓）。
 *   **∴ 为什么本地必须有这一层 ✗**：`restore_checkpoint` 的语义就是写一条
 *     `declare_head{base:{type:"checkpoint"}}` ✓ —— 而本地**所有状态推导**
 *     （`p2Neutralized`／`p2ObjectView`／`p2PresetLayers`／`p1LayerOrder`／`p2cState`）
 *     此前**只看全量日志** ✗ ⇒ 恢复后**回执说恢复了、面板还是旧状态** ✓
 *     （形态③：回执 ok 而读回旧 ✓）。
 *   **∴ 窗口 = 链底 `(0, 最内层 base]` ∪ 每个 `(H.seq, upto]`（升序拼接 ✓）**
 *     —— 与 `state_at` 的三阶段折叠顺序等价 ✓。**段内 `revert` 不会跨过求值起点**
 *     （`log.rs` 提交期拒绝 ✓）⇒ 段内有效集自洽 ⇒ 拼接后倒序扫描仍是同一口径 ✓。
 *   **∴ 无 `declare_head` ⇒ 原样返回同一数组（下游零成本 ✓）**；memo 按数组身份 ✓。 */
const p2StateWindowCache = new WeakMap();
function p2StateWindow(rows) {
  if (!Array.isArray(rows) || rows.length === 0) return rows || [];
  const memo = p2StateWindowCache.get(rows);
  if (memo) return memo;
  const seqOf = new Map();
  for (const r of rows) {
    const id = r && r.atom && r.atom.id;
    if (typeof id === "string" && typeof r.seq === "number") seqOf.set(id, r.seq);
  }
  const headUpto = (upto) => {
    for (let i = rows.length - 1; i >= 0; i--) {
      const r = rows[i];
      if (!r || typeof r.seq !== "number" || r.seq > upto) continue;
      if (r.atom && r.atom.kind === "declare_head") return r;
    }
    return null;
  };
  const baseSeqOf = (headRow) => {
    const base = (headRow.atom.payload && headRow.atom.payload.base) || {};
    if (base.type === "checkpoint") {
      if (typeof base.id !== "string" || !base.id) return null;
      // **检查点从日志解析 ✓**（`seq.rs:checkpoint_anchor_seq` ✓：`seq ≤ H.seq` 内**最后一个**同名 ✓）
      const ckpt = p2FindCheckpointAtom(rows, base.id, headRow.seq);
      if (!ckpt) return null;
      const anchor = (ckpt.atom.payload || {}).anchor_seq;
      return (typeof anchor === "number" && Number.isInteger(anchor) && anchor >= 0) ? anchor : ckpt.seq;
    }
    const s = seqOf.get(base.id); // atom / declare_head ⇒ 该原子的 seq ✓（`HeadBase::Atom` ✓）
    return typeof s === "number" ? s : null;
  };
  let tailUpto = rows[rows.length - 1].seq;
  const ranges = []; // [from, to] ⇒ 可见区间 (from, to] ✓
  let head = headUpto(tailUpto);
  while (head) {
    const base = baseSeqOf(head);
    // **∴ base 解不出／不早于自身 ⇒ 写入口已经拒绝**（`validate_declare_head` ✓）⇒
    //   **这里的兜底只是别把坏日志算成「有跳变」**（停在链底 ✓，不是静默放过 ✓）。
    if (typeof base !== "number" || base >= head.seq) break;
    ranges.push([head.seq, tailUpto]);
    tailUpto = base;
    head = headUpto(tailUpto);
  }
  if (ranges.length === 0) {
    p2StateWindowCache.set(rows, rows);
    return rows;
  }
  const visible = rows.filter((r) => {
    if (typeof r.seq !== "number") return false;
    if (r.seq <= tailUpto) return true;
    for (const range of ranges) if (r.seq > range[0] && r.seq <= range[1]) return true;
    return false;
  });
  p2StateWindowCache.set(rows, visible);
  return visible;
}
/** **当前求值起点 seq ✗**（**`state.eval_origin_seq()` ✓ = 最近一次 `declare_head` 的 seq ✓**；
 *   没有跳变 ⇒ 0 ✓）。**∴ `revert`／`reapply` 的目标不许早于它** ✗（`log.rs` ✓）。 */
function p2EvalOriginSeq(rows) {
  let origin = 0;
  for (const r of rows || []) {
    const a = (r && r.atom) || {};
    if (a.kind === "declare_head" && typeof r.seq === "number" && r.seq > origin) origin = r.seq;
  }
  return origin;
}
/** **检查点原子 ✗**（**`seq ≤ upto` 内最后一个同名 ✓**，照 `seq.rs:checkpoint_anchor_seq` ✓：
 *  `checkpoint_id` 缺省 ⇒ `ckpt_{atom.id}` ✓；服务端写的是 `create_checkpoint` ✓，
 *  本地写的是 `checkpoint` ✓ —— **两种都认** ✓）。**`upto` 缺省 = 全量日志 ✓**。 */
function p2FindCheckpointAtom(rows, checkpointId, upto) {
  const limit = typeof upto === "number" ? upto : Infinity;
  for (let i = (rows || []).length - 1; i >= 0; i--) {
    const r = rows[i];
    const a = (r && r.atom) || {};
    if (typeof r.seq !== "number" || r.seq > limit) continue;
    if (a.kind !== "checkpoint" && a.kind !== "create_checkpoint") continue;
    const pl = a.payload || {};
    const key = typeof pl.checkpoint_id === "string" ? pl.checkpoint_id : ("ckpt_" + a.id);
    if (key === checkpointId) return r;
  }
  return null;
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
  rows = p2StateWindow(rows); // **`declare_head` 跳变之后的图层状态 ✗**（见 `p2StateWindow` ✓）
  const layers = new Map();
  const order = [];
  // **★ `set_property` 折叠**（`update_layer` ✓）：图层属性（visible/locked/opacity/
  //   blend_mode/name **以及第 2 批补上的 z_index／parent_id** ✓）存在 `set_property` 原子里
  //   ⇒ 按顺序应用，否则前端读到旧值 ✓。
  //   **∴ 旧清单只有 8 个键 ✗** ⇒ `update_layer{z_index}`／`{parent_id}` **写完回执 ok**
  //   而 `list_layers` 里**看不见** ⇒ 形态③（输入不被采纳）✓ —— 本轮判据抓出的真缺口 ✓。
  const PROP_KEYS = ["name", "visible", "opacity", "blend_mode",
                     "locked", "alpha_lock", "clipping_mask", "mask_id",
                     "z_index", "parent_id", "type"];
  for (const r of rows || []) {
    const a = (r && r.atom) || {};
    const pl = (a && a.payload) || {};
    if (a.kind === "create_document") {
      // **新文档自带一个默认图层 ✗**（`yanshi-core/src/fold.rs:1004` ✓）：
      //   **∴ 服务端 `list_layers` 从初始状态起就返回 `layer_default`** ✓
      //     ⇒ **∴ 本地只从 `create_layer` 原子推导** ⇒ **凭空少一层** ✗
      //       ⇒ **∴ 实测：新文档 `list_layers` 回 `count:0`** ✓
      //         ⇒ **∴ 面板点眼睛时 `layers.find(id)` 找不到目标** ⇒ **`return` 掉** ✗
      //           ⇒ **∴ 隐藏点了没反应** ✓（ui-check 第 11 轮：10726 → 10726 ✓）。
      //   ⇒ **∴ 与初始状态对齐**：名字／z／默认值照 `fold.rs` 的 `Layer` ✓。
      if (!layers.has("layer_default")) {
        layers.set("layer_default", {
          layer_id: "layer_default", name: "图层 1",
          type: "raster", kind: "raster", parent_id: null, z_index: -1,
          visible: true, opacity: 1, blend_mode: "normal", locked: false,
          alpha_lock: false, clipping_mask: false, mask_id: null, objects: 0,
        });
        order.unshift("layer_default");
      }
      continue;
    }
    if (a.kind === "create_layer") {
      // **★ 兼容旧原子**：顶层 `layer_id` 回退（判据 POST 的原子不带 `payload` 包装）。
      const id = (typeof pl.layer_id === "string" && pl.layer_id) ||
                 (typeof a.layer_id === "string" && a.layer_id);
      if (typeof id !== "string" || !id || layers.has(id)) continue;
      // **★ 缺省字段照 `fold.rs` 的 `CreateLayer` 逐条抄 ✗ ★**（本轮补 ✓）：
      //   **∴ 旧实现只写 6 个字段 ✗** ⇒ `list_layers` 的新层**没有 `type`／`parent_id`／
      //   `z_index`／`alpha_lock`／`clipping_mask`／`mask_id`／`objects`** ✗
      //   ⇒ 与 `layer_default`（13 个字段）**同一条响应里字段集不一致** ✓
      //   ⇒ 前端读 `layer.type`／拖进组读 `parent_id` 时**静默拿到 undefined** ✗（形态①）。
      //   **∴ z 缺省 = max(存活 z + 1) ✗**（`fold.rs:1051` ✓，不是「插入序」✓）。
      const zRaw = Number(pl.z_index);
      const zDefault = order.reduce((mx, lid) => {
        const l = layers.get(lid);
        return l && Number.isFinite(l.z_index) ? Math.max(mx, l.z_index + 1) : mx;
      }, 0);
      const type = typeof pl.type === "string" && pl.type ? pl.type : "raster";
      layers.set(id, {
        layer_id: id, name: pl.name ?? a.name ?? id,
        type, kind: type,
        parent_id: typeof pl.parent_id === "string" ? pl.parent_id : null,
        z_index: Number.isFinite(zRaw) ? zRaw : zDefault,
        visible: true, opacity: 1, blend_mode: "normal", locked: false,
        alpha_lock: false, clipping_mask: false, mask_id: null, objects: 0,
      });
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
      if (ok) {
        // **★ 折叠要给 z_index 赋值 ✗ ★**（`fold.rs:1296` ✓：`order[i] ⇒ z_index = i`）：
        //   **∴ 旧实现只换 `order` 数组 ✗** ⇒ `update_layer{z_index}` 与重排混用时
        //   **两个来源各说一套** ✓ —— 现在只有一个权威来源（`z_index`）✓，`order` 由它导出 ✓。
        ord.forEach((id, i) => {
          const layer = layers.get(id);
          if (layer) layer.z_index = i;
        });
      }
    } else if (a.kind === "set_property" && typeof pl.layer_id === "string" &&
               typeof pl.key === "string" && PROP_KEYS.indexOf(pl.key) >= 0) {
      const layer = layers.get(pl.layer_id);
      if (layer) {
        // **`type` 与 `kind` 同源 ✓**（`read_list_layers` 两个字段都回 `layer.layer_type` ✓）。
        if (pl.key === "type") { layer.type = pl.value; layer.kind = pl.value; }
        else layer[pl.key] = pl.value;
      }
    }
  }
  // **★ 返回顺序 = 服务端 `alive_layers()` ✗ ★**（`state.rs:699` ✓：`(z_index, id)` 升序 ✓）：
  //   **∴ 旧实现按推导序（≈ 建层序）✗** ⇒ `update_layer{z_index}` 改了层级**界面不跟着动** ✗
  //   ⇒ 同一文档在服务端与离线模式里**图层顺序不同** ✓（形态③）。
  //   **∴ 用显式比较器 ✗ 不用 `localeCompare` ✓**（后者随 locale 变 ⇒ 判据跨机器会飘 ✓）。
  order.sort((a, b) => {
    const la = layers.get(a), lb = layers.get(b);
    const za = la && Number.isFinite(la.z_index) ? la.z_index : 0;
    const zb = lb && Number.isFinite(lb.z_index) ? lb.z_index : 0;
    if (za !== zb) return za - zb;
    return a < b ? -1 : (a > b ? 1 : 0);
  });
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
  // **★ 先**抽读端**✗ ★**（实测 ✓）：`CompressionStream` 的**写端**在**读端开始消费**之前
  //   只会缓冲有限的输入 ✗ ⇒ 原来"**写完再读**"在真浏览器里**任何尺寸都挂死** ✓
  //   （探针 `encodePngRgba(2,2)` 8 秒不返回 ✓ ⇒ `brush_preview` 超时 ⇒ 预览图永远空框 ✗）。
  //   ⇒ 把读端**并行**抽起来 ✓，再写、关、等读端收完 ✓。
  const chunks = [];
  const pump = (async () => {
    const r = cs.readable.getReader();
    for (;;) {
      const { done, value } = await r.read();
      if (done) break;
      chunks.push(value);
    }
  })();
  const w = cs.writable.getWriter();
  await w.write(bytes);
  await w.close();
  await pump;
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
/** **工具规格表 ✗**（**服务端「工具 → 参数」的本地镜像 ✓**）：
 *   **∴ 由 `scripts/gen-pwa-tool-specs.mjs` 生成 ✓** —— **∴ 手抄必漂移**✗
 *   ⇒ **∴ 生成器 `--check` 挂在判据里看守（过期 ⇒ 红，不是静默漂移 ✓）**。
 *   格式：`name: [mutating?1:0, [["参数","类型码",必填?1:0], …]]`
 *   **参数顺序 = 注册表声明顺序 ✓**（`validate_args` 的「可用参数：…」按它 join ✓）
 *   类型码：`s`=string `i`=integer `n`=number `b`=boolean `o`=object `a`=array
 *   `x`=any（**服务端 `Any` 不写 type ✓**）
 *   **数据源**：① 参数名/类型/必填 = `GET /api/tools`（与 MCP `tools/list` 逐字相同 ✓，
 *   `server.rs:701`）；② `mutating` 位 ＋ 参数顺序 = `crates/yanshi-server/src/tools_registry.rs`
 *   （**HTTP 清单不暴露这两样 ✓**；`serde_json` 的 Map 是 BTreeMap ⇒ 键按字母序 ✗）。
 */
const P2_TOOL_SPECS = {
// <gen:tool-specs>
  abort_changeset: [1, []],
  accept_suggestion: [1, [["suggestion_id","s",1]]],
  accept_suggestions: [1, [["suggestion_ids","a",1]]],
  add_adjustment: [1, [["layer_id","s",1],["adjustment_type","s",1],["params","o",0],["opacity","n",0],["z_index","i",0],["name","s",0],["object_id","s",0]]],
  add_filter: [1, [["layer_id","s",1],["filter_name","s",1],["params","o",0],["opacity","n",0],["z_index","i",0],["name","s",0],["object_id","s",0]]],
  add_to_group: [1, [["group_id","s",1],["object_id","s",1]]],
  analyze_region: [0, [["region","x",1],["compare_with_reference","b",0]]],
  apply_adjustments: [1, [["adjustments","a",1],["layer_id","s",0]]],
  apply_curve_preset: [0, [["brush","s",1],["shape","s",1],["target","s",0]]],
  apply_preset: [1, [["preset_id","s",1],["layer_id","s",1],["strength","n",0],["knobs","o",0],["include_correction","b",0]]],
  apply_stash: [1, [["stash_id","s",1]]],
  auto_retouch: [1, [["layer_id","s",0],["strength","n",0],["groups","a",0],["dry_run","b",0],["force","b",0]]],
  batch: [1, [["calls","a",1],["preview_every_n_strokes","n",0],["preview_interval_ms","n",0],["preview","b",0],["message","s",0],["silent","b",0]]],
  begin_changeset: [0, []],
  begin_transaction: [0, []],
  blob_gc: [1, [["dry_run","b",0],["confirm","b",0],["ttl_days","n",0],["demote","b",0]]],
  brush_preview: [0, [["brush","s",1],["size","n",0],["color","x",0],["points","a",0],["color_to","x",0],["color_space","s",0],["smooth","b",0],["opacity","n",0],["hardness","n",0],["background","x",0],["read_canvas","b",0],["brush_params","o",0],["include_image","b",0],["output_path","s",0]]],
  brush_stroke: [1, [["brush_params","o",0],["hand_tremor","n",0],["speed_ink","n",0],["color_breath","n",0],["paint_push","n",0],["pressure_shape","n",0],["stroke_taper","n",0],["dry_brush","n",0],["edge_soft","n",0],["tilt_shade","n",0],["scatter_texture","n",0],["edge","s",0],["pressure_shape","n",0],["path_guide","o",0],["stroke_taper","n",0],["preview","b",0],["wait_for_render","b",0],["sensors","o",0],["layer_id","s",1],["symmetry","s",0],["anchor_mode","s",0],["grid_spacing","n",0],["humanize","b",0],["pressure_curve","s",0],["dwell","n",0],["tremor","n",0],["tremor_seed","i",0],["glaze","b",0],["glaze_passes","i",0],["brush","s",1],["points","a",1],["size","n",0],["color","x",0],["color_mixing","s",0],["color_to","x",0],["color_space","s",0],["opacity","n",0],["hardness","n",0],["smooth","b",0],["object_id","s",0],["style","s",0],["clip_to_selection","s",0],["label","s",0],["group_id","s",0]]],
  calibrate: [1, [["size","n",0],["points","x",0],["tolerance","n",0],["layer_id","s",0],["cleanup","b",0]]],
  cancel_job: [0, [["job_id","s",1]]],
  cancel_operation: [0, [["doc_id","s",0]]],
  checkpoint: [1, [["name","s",0],["message","s",0]]],
  clear_reference: [0, []],
  clone_stamp: [1, [["layer_id","s",1],["points","a",1],["source_offset","a",1],["size","n",0],["hardness","n",0],["opacity","n",0],["object_id","s",0],["z_index","i",0]]],
  collect_diagnostics: [0, [["path","s",0],["include_thumbnail","b",0],["max_bytes","i",0]]],
  collect_garbage: [0, [["confirm","b",0],["ttl_seconds","i",0]]],
  combine_shapes: [1, [["object_ids","a",1],["mode","s",0],["result_id","s",0],["simplify","n",0],["layer_id","s",0],["color","x",0],["engine","s",0],["tolerance","n",0]]],
  comment: [1, [["text","s",1],["target_atom","s",0],["object_id","s",0]]],
  commit_batch: [1, [["attempt_ids","x",0],["attempt_id","s",0]]],
  commit_changeset: [0, []],
  commit_stroke: [1, [["attempt_id","s",1]]],
  commit_transaction: [0, []],
  convert_to_path: [1, [["object_id","s",1],["path_id","s",0]]],
  convert_to_shape: [1, [["object_id","s",1],["shape_id","s",0]]],
  create_annotation: [0, [["type","s",1],["intent","s",1],["target","o",1],["content","s",0],["suggestion_id","s",0]]],
  create_group: [1, [["group_id","s",1],["layer_id","s",1],["members","a",0]]],
  create_instance: [1, [["instance_id","s",1],["layer_id","s",1],["master_id","s",1],["local_transform","o",0]]],
  create_layer: [1, [["layer_id","s",0],["name","s",0],["type","s",0],["kind","s",0],["parent_id","s",0],["z_index","i",0],["if_exists","s",0]]],
  create_mask: [1, [["mask_id","s",1],["shape","o",0],["feather","n",0],["invert","b",0],["mode","s",0],["linked_layer","s",0],["from_selection","s",0]]],
  create_range_mask: [1, [["mask_id","s",1],["layer_id","s",1],["range","s",0],["low","n",0],["high","n",0],["hue","n",0],["hue_width","n",0],["smoothness","n",0],["invert","b",0]]],
  create_selection: [1, [["selection_id","s",1],["shape","o",0],["feather","n",0],["invert","b",0],["mode","s",0],["linked_layer","s",0],["from_mask","s",0]]],
  declare_head: [1, [["base_type","s",1],["base_id","s",1],["reason","s",0]]],
  delete_anchor: [1, [["name","s",1]]],
  delete_annotation: [0, [["annotation_id","s",1]]],
  delete_document: [1, [["document_id","s",1]]],
  delete_layer: [1, [["layer_id","s",1]]],
  delete_mask: [1, [["mask_id","s",1]]],
  delete_object: [1, [["object_id","s",1]]],
  delete_selection: [1, [["selection_id","s",1]]],
  describe_canvas: [0, []],
  detach_instance: [1, [["instance_id","s",1],["object_id","s",0]]],
  discard_stash: [1, [["stash_id","s",1]]],
  discard_stroke: [0, [["attempt_id","s",1]]],
  dodge_burn_setup: [1, [["layer_id","s",1],["mode","s",0],["dodge_mid","n",0],["burn_mid","n",0],["name","s",0]]],
  draw_shape: [1, [["label","s",0],["group_id","s",0],["layer_id","s",1],["data","o",1],["object_id","s",0]]],
  draw_stroke: [1, [["label","s",0],["group_id","s",0],["layer_id","s",1],["data","o",1],["object_id","s",0]]],
  draw_text: [1, [["label","s",0],["group_id","s",0],["layer_id","s",1],["data","o",1],["object_id","s",0]]],
  duplicate_layer: [1, [["layer_id","s",1],["new_layer_id","s",0],["name","s",0]]],
  edit_path: [1, [["object_id","s",1],["op","s",1],["subpath","i",1],["node","i",0],["segment","i",0],["t","n",0],["side","s",0],["to","x",0]]],
  erase: [1, [["label","s",0],["group_id","s",0],["layer_id","s",1],["data","o",1],["object_id","s",0]]],
  estimate_dehaze: [0, []],
  explain_tool: [0, [["name","s",0],["tool","s",0]]],
  export_book_pngs: [0, [["doc_ids","x",1],["dir","s",1]]],
  export_png: [0, [["path","s",1],["region","x",0],["lod","i",0],["width","i",0],["height","i",0],["max_edge","i",0],["filter","s",0],["deterministic","b",0],["layer_id","s",0]]],
  export_project: [0, [["path","s",0],["doc_id","s",0],["include_bitmaps","b",0]]],
  export_psd_layers: [0, [["filename","s",0],["output_path","s",0]]],
  export_svg: [0, [["path","s",1],["validate","b",0],["flatten_opacity","b",0]]],
  export_timelapse: [0, [["path","s",1],["doc_id","s",0],["fps","i",0],["frame_every","i",0],["duration_sec","n",0],["width","i",0],["crf","i",0],["max_frames","i",0]]],
  fill: [1, [["label","s",0],["group_id","s",0],["layer_id","s",1],["data","o",1],["object_id","s",0]]],
  fill_region: [1, [["layer_id","s",1],["shape","o",1],["feather","n",0],["color","x",0],["opacity","n",0],["texture","x",0]]],
  find_atom: [0, [["kind","s",0],["actor","s",0],["object_id","s",0],["layer_id","s",0],["since_seq","i",0],["limit","i",0]]],
  fit_background_field: [1, [["layer_id","s",1],["mask","o",1],["degree","n",0],["sample_stride","n",0],["margin","n",0],["object_id","s",0]]],
  frequency_separation: [1, [["layer_id","s",1],["radius","n",0],["name","s",0]]],
  get_ancestors: [0, [["object_id","s",1]]],
  get_anchors: [0, []],
  get_annotation: [0, [["annotation_id","s",1]]],
  get_atom: [0, [["atom_id","s",1]]],
  get_brush_dynamics: [0, [["brush","s",1],["format","s",0]]],
  get_brush_settings: [0, [["brush","s",1]]],
  get_changesets: [0, [["limit","i",0]]],
  get_checkpoints: [0, []],
  get_dependency_graph: [0, [["object_id","s",1]]],
  get_descendants: [0, [["object_id","s",1]]],
  get_diff: [0, [["from_seq","i",1],["to_seq","i",0],["limit","i",0]]],
  get_document: [0, [["preview_size","x",0]]],
  get_histogram: [0, [["layer_id","s",0],["region","o",0]]],
  get_inflight: [0, [["doc_id","s",0]]],
  get_job: [0, [["job_id","s",1]]],
  get_log: [0, [["since_seq","i",0],["limit","i",0],["kind","s",0],["actor","s",0],["order","s",0]]],
  get_object: [0, [["object_id","s",1],["include_history","b",0]]],
  get_object_history: [0, [["object_id","s",1]]],
  get_path: [0, [["object_id","s",1]]],
  get_preferences: [0, [["keys","a",0]]],
  get_preset: [0, [["preset_id","s",1]]],
  get_render_status: [0, [["atom_id","s",1]]],
  get_resolved_state: [0, [["object_id","s",1]]],
  get_state: [0, [["include_objects","b",0],["include_hidden","b",0],["preview_size","x",0]]],
  get_stroke_log: [0, [["limit","n",0],["offset","n",0],["since_seq","n",0],["group_id","s",0],["layer_id","s",0],["object_id","s",0],["order","s",0]]],
  get_undo_status: [0, []],
  gradient_blend: [1, [["layer_id","s",1],["from","x",1],["to","o",1],["brush","s",1],["size","n",1],["steps","n",0],["smooth","b",0]]],
  gradient_fill: [1, [["layer_id","s",1],["from","o",1],["to","o",1],["kind","s",0],["angle","n",0],["center","o",0],["radius","n",0],["region","x",0],["object_id","s",0]]],
  heal_stamp: [1, [["layer_id","s",1],["points","a",1],["source_offset","a",1],["size","n",0],["hardness","n",0],["opacity","n",0],["object_id","s",0],["z_index","i",0]]],
  import_asset: [0, [["kind","s",1],["name","s",1],["path","s",0],["blob","o",0],["overwrite","b",0],["data_base64","s",0]]],
  import_image: [1, [["layer_id","s",1],["bitmap","o",1],["region","x",1],["object_id","s",0],["medium","o",0],["text_provenance","o",0]]],
  import_project: [1, [["path","s",1],["doc_id","s",0]]],
  import_psd: [1, [["layer_id","s",1],["blob_hash","s",1],["object_id","s",0]]],
  import_psd_layers: [1, [["blob_hash","s",1],["name_prefix","s",0]]],
  link_to_master: [1, [["instance_id","s",1],["master_id","s",1]]],
  liquify_pinch: [1, [["layer_id","s",1],["points","a",1],["size","n",0],["strength","n",0],["object_id","s",0],["z_index","i",0]]],
  liquify_push: [1, [["layer_id","s",1],["points","a",1],["direction","a",1],["size","n",0],["strength","n",0],["object_id","s",0],["z_index","i",0]]],
  liquify_twirl: [1, [["layer_id","s",1],["points","a",1],["size","n",0],["strength","n",0],["object_id","s",0],["z_index","i",0]]],
  list_annotations: [0, [["status","s",0],["actor","s",0],["intent","s",0],["object_id","s",0],["suggestion_id","s",0]]],
  list_assets: [0, [["kind","s",1],["tag","s",0]]],
  list_atoms: [0, [["limit","i",0],["offset","i",0],["label","s",0],["layer_id","s",0],["brush","s",0],["since_seq","i",0],["group_id","s",0],["bbox","x",0],["thumb_size","i",0],["want_thumb","b",0],["include_non_stroke","b",0]]],
  list_attempts: [0, [["limit","i",0],["label","s",0],["status","s",0]]],
  list_brushes: [0, [["filter","s",0],["medium","s",0]]],
  list_comments: [0, [["limit","i",0],["since_seq","i",0],["actor","s",0],["object_id","s",0]]],
  list_documents: [0, []],
  list_effects: [0, [["layer_id","s",0]]],
  list_engine_capabilities: [0, []],
  list_fonts: [0, []],
  list_layers: [0, []],
  list_objects: [0, [["layer_id","s",0],["type","s",0],["include_hidden","b",0]]],
  list_palette_colors: [0, [["palette","s",1],["limit","i",0]]],
  list_presets: [0, [["tag","s",0],["subject","s",0],["status","s",0]]],
  list_selections: [0, []],
  list_stashes: [0, []],
  list_suggestions: [0, [["status","s",0],["since_seq","i",0],["limit","i",0],["offset","i",0]]],
  list_textures: [0, []],
  lock_layer: [1, [["layer_id","s",1]]],
  medium_stroke: [1, [["layer_id","s",1],["medium","s",1],["points","a",1],["size","n",0],["color","x",0],["color_to","x",0],["color_space","s",0],["color_mixing","s",0],["smooth","b",0],["symmetry","s",0],["load","n",0],["wetness","n",0],["texture","n",0],["object_id","s",0]]],
  move_object: [1, [["object_id","s",1],["delta","o",0],["transform","o",0]]],
  new_document: [1, [["doc_id","s",0],["width","i",0],["height","i",0],["background","o",0]]],
  paint_mask: [1, [["mask_id","s",1],["points","a",1],["mode","s",0],["mask_value","n",0],["size","n",0],["softness","n",0],["opacity","n",0],["flow","n",0],["humanize","b",0],["preset","s",0]]],
  patch: [1, [["layer_id","s",1],["source_region","o",1],["target","a",1],["opacity","n",0],["object_id","s",0],["z_index","i",0]]],
  path_edit: [1, [["op","s",1],["object_id","s",1],["other_id","s",0],["at","i",0],["right_id","s",0],["shape_id","s",0],["path_id","s",0],["mode","s",0],["result_id","s",0]]],
  preview_preset: [0, [["preset_id","s",1],["strength","n",0],["knobs","o",0],["include_correction","b",0],["layer_id","s",0],["render","o",0]]],
  preview_suggestion: [0, [["suggestion_id","s",0],["patch","a",0]]],
  put_blob: [0, [["path","s",0],["data_base64","s",0],["mime_type","s",0]]],
  reapply: [1, [["atom_id","s",1]]],
  redo_last: [1, [["count","i",0]]],
  reject_annotation: [0, [["annotation_id","s",1]]],
  reject_suggestion: [1, [["suggestion_id","s",1],["reason","s",0]]],
  reject_suggestions: [1, [["suggestion_ids","a",1],["reason","s",0]]],
  remove_from_group: [1, [["group_id","s",1],["object_id","s",1]]],
  render_region: [0, [["region","x",1],["raw","b",0],["output_path","s",0],["show_coords","b",0],["include_image","b",0],["max_px","n",0],["layer_id","s",0],["active_layer","s",0]]],
  reorder_layers: [1, [["order","a",1]]],
  reorder_objects: [1, [["layer_id","s",1],["order","a",1]]],
  replace_object_data: [1, [["object_id","s",1],["data","o",1],["type","s",0]]],
  resample: [1, [["object_id","s",1],["width","i",0],["height","i",0],["scale","n",0],["filter","s",0]]],
  resolve_annotation: [0, [["annotation_id","s",1],["atom_id","s",0]]],
  resolve_conflict: [1, [["resolution","s",1],["conflict_layer_id","s",0],["formal_layer_id","s",0],["opponent_atom_id","s",0]]],
  restore_checkpoint: [1, [["checkpoint_id","s",1]]],
  restore_object: [1, [["object_id","s",1]]],
  revert: [1, [["atom_id","s",1]]],
  revert_atoms: [1, [["atom_ids","x",1]]],
  revert_changeset: [1, [["changeset_id","s",1]]],
  revert_group: [1, [["group_id","s",1]]],
  revert_to: [1, [["atom_id","s",1]]],
  revert_to_seq: [1, [["seq","i",1]]],
  revoke_token: [1, [["token","s",1]]],
  sample_color: [0, [["x","n",1],["y","n",1]]],
  sample_colors: [0, [["points","a",1]]],
  save_palette: [0, [["name","s",1],["colors","a",1]]],
  scatter_strokes: [1, [["layer_id","s",1],["seed","n",1],["area","o",1],["palette","a",1],["count","n",0],["brush","s",1],["size_range","a",0],["opacity_range","a",0],["direction","s",0],["clip_to_selection","s",0]]],
  select_opaque_pixels: [1, [["layer_id","s",1],["selection_id","s",0],["min_alpha","n",0],["binarize","b",0],["close_pinholes","b",0],["mode","s",0],["feather","n",0]]],
  set_anchor: [1, [["name","s",1],["x","n",1],["y","n",1],["note","s",0]]],
  set_brush_curve: [0, [["brush","s",1],["setting","s",1],["input","s",1],["points","a",1]]],
  set_brush_dynamics: [0, [["brush","s",1],["curve","o",1]]],
  set_brush_setting: [0, [["brush","s",1],["setting","s",1],["value","n",1]]],
  set_group_transform: [1, [["group_id","s",1],["delta","o",1]]],
  set_layer_blend: [1, [["layer_id","s",1],["mode","s",1]]],
  set_preferences: [1, [["values","o",1]]],
  set_property: [1, [["key","s",1],["value","x",0],["object_id","s",0],["layer_id","s",0]]],
  set_reference: [0, [["blob_hash","s",1],["opacity","n",0],["position","o",0]]],
  smart_blend: [1, [["shape","o",1],["color_a","a",1],["color_b","a",1],["blend_direction","n",0],["direction_guides","a",0],["direction_from","s",0],["brush","s",0],["size","n",0],["layer_id","s",1],["wait_for_render","b",0],["clip_to_selection","s",0]]],
  smart_edge_soften: [1, [["edge_path","a",0],["path_guide","o",0],["shape","o",0],["soften_width","n",0],["soften_strength","n",0],["max_steps","i",0],["soften_direction","s",0],["color","a",0],["brush","s",0],["size","n",0],["layer_id","s",1],["wait_for_render","b",0],["clip_to_selection","s",0]]],
  smart_erase_shape: [1, [["layer_id","s",1],["shape","o",1],["invert","b",0],["feather","n",0],["opacity","n",0],["size","n",0]]],
  smart_fill: [1, [["shape","o",1],["fill_direction","n",0],["direction_guides","a",0],["direction_from","s",0],["clip_to_shape","b",0],["direction_jitter","n",0],["color","a",1],["brush","s",0],["size","n",0],["mode","s",0],["layer_id","s",1],["wait_for_render","b",0],["clip_to_selection","s",0]]],
  smart_glaze: [1, [["shape","o",1],["color","a",1],["layers","n",0],["target_opacity","n",0],["layer_opacity","n",0],["brush","s",0],["size","n",0],["fill_direction","n",0],["direction_jitter","n",0],["direction_rotate","n",0],["layer_id","s",1],["wait_for_render","b",0],["clip_to_selection","s",0]]],
  smart_hatch_follow: [1, [["guides","a",1],["width","n",0],["shape","o",0],["density","n",0],["size","n",0],["color","a",1],["brush","s",0],["layer_id","s",1],["wait_for_render","b",0],["clip_to_selection","s",0]]],
  smart_sample: [1, [["ref","s",1],["shape","o",1],["ref_bbox","a",0],["samples","n",0],["texture_type","s",0],["density","n",0],["size","n",0],["brush","s",0],["layer_id","s",1],["wait_for_render","b",0],["clip_to_selection","s",0]]],
  smart_select: [1, [["selection_id","s",0],["seed","a",0],["seeds","a",0],["tolerance","n",0],["contiguous","b",0],["refine_edges","b",0],["layer_id","s",0],["mode","s",0],["feather","n",0],["preview","b",0]]],
  smart_texture: [1, [["shape","o",1],["texture_type","s",1],["direction_guides","a",0],["direction_from","s",0],["density","n",0],["neutral","b",0],["size","n",0],["color","a",0],["brush","s",0],["layer_id","s",1],["wait_for_render","b",0],["clip_to_selection","s",0]]],
  smudge: [1, [["layer_id","s",1],["points","a",1],["size","n",0],["smudge_length","n",0],["deposit","n",0],["hardness","n",0],["opacity","n",0],["object_id","s",0],["z_index","i",0]]],
  stroke_path: [1, [["label","s",0],["group_id","s",0],["layer_id","s",1],["path_object_id","s",0],["subpaths","a",0],["d","s",0],["brush","s",1],["size","n",0],["color","x",0],["brush_params","o",0],["pressure_profile","s",0],["pressure_max","n",0],["ramp_ratio","n",0],["virtual_speed","n",0],["preview","b",0],["wait_for_render","b",0],["opacity","n",0],["clip_to_selection","s",0]]],
  submit_offline: [1, [["atoms","a",1]]],
  suggest: [1, [["patch","a",1],["annotation_id","s",0],["summary","s",0],["priority","i",0]]],
  swatch_brush_strokes: [0, [["brushes","a",1],["brush_params","o",0],["preview_size","s",0],["colors","a",0],["paths","a",0],["size","n",0],["background","x",0],["read_canvas","b",0],["opacity","n",0],["hardness","n",0],["color_to","x",0],["smooth","b",0],["include_image","b",0],["output_path","s",0]]],
  texture_background: [1, [["texture","s",1],["mode","s",0],["region","x",0],["layer_id","s",0],["object_id","s",0]]],
  transform_object: [1, [["object_id","s",1],["rotate","o",0],["scale","o",0],["translate","o",0],["anchor","o",0],["compose","b",0]]],
  transform_selection: [1, [["selection_id","s",1],["translate","a",0],["scale","a",0],["rotate_deg","n",0],["pivot","a",0]]],
  try_batch: [0, [["layer_id","s",1],["strokes","x",1],["label","s",0]]],
  try_stroke: [0, [["layer_id","s",0],["brush","s",0],["points","x",0],["size","n",0],["color","x",0],["color_space","s",0],["opacity","n",0],["hardness","n",0],["sensors","x",0],["want_image","b",0],["output_path","s",0],["margin","i",0],["label","s",0],["based_on","s",0]]],
  undo_last: [1, [["count","i",0]]],
  unlock_layer: [1, [["layer_id","s",1]]],
  update_adjustment: [1, [["object_id","s",1],["params","o",1],["opacity","n",0]]],
  update_annotation: [0, [["annotation_id","s",1],["content","s",0],["intent","s",0],["status","s",0]]],
  update_filter: [1, [["object_id","s",1],["params","o",1],["opacity","n",0]]],
  update_layer: [1, [["layer_id","s",1],["patch","o",1]]],
  update_object: [1, [["object_id","s",1],["patch","o",1]]],
  update_override: [1, [["instance_id","s",1],["transform","o",0]]],
  update_stroke: [1, [["object_id","s",1],["core","o",0],["preset","s",0],["advanced","o",0]]],
  update_sync_policy: [1, [["instance_id","s",1],["policy","s",1]]],
  vector_erase: [1, [["layer_id","s",1],["points","a",1],["radius","n",1]]],
  vector_hit_test: [0, [["layer_id","s",1],["x","n",1],["y","n",1],["tolerance","n",0]]],
  verify_package: [0, [["path","s",1]]],
// </gen:tool-specs>
};
/** **测试专用**：返回嵌入的工具表（判据拿它与服务端清单逐项比对 ✓）。 */
export function __toolSpecsForTest() { return P2_TOOL_SPECS; }

/** **工具规格查表 ✗**（**类型码 ⇒ `p2PresetSchemaCheck` 的 kind 名 ✓**）：
 *   `x`（服务端 `Any`）⇒ `"any"`（**恒匹配 ✓**）；表里没有 ⇒ `null`（**调用方报「未知工具」✓**）。 */
const P2_KIND_NAMES = { s: "string", i: "integer", n: "number", b: "boolean", o: "object", a: "array", x: "any" };
function p2ToolSpec(name) {
  const row = Object.prototype.hasOwnProperty.call(P2_TOOL_SPECS, name) ? P2_TOOL_SPECS[name] : null;
  if (!row) return null;
  return {
    name,
    mutating: row[0] === 1,
    params: row[1].map(([param, code, required]) =>
      ({ name: param, kind: P2_KIND_NAMES[code], required: required === 1 })),
  };
}
/** **入口参数校验 ✗**（**与 HTTP 层 `ToolRegistry::call` 同一道闸 ✓**）：
 *   **∴ 返回** `null`（通过）**或** `{code, detail}` ✓ —— **∴ 不抛 ✗**（**调用方决定 400 还是记成「步骤无效」✓**）。 */
function p2ValidateArgsFor(tool, args) {
  const spec = p2ToolSpec(tool);
  if (!spec) return { code: "invalid_argument", detail: `未知工具 ${tool}` };
  try {
    p2PresetSchemaCheck(tool, spec.params, args);
  } catch (e) {
    const info = e && e.__presetError;
    if (!info) throw e;
    return { code: info.code, detail: info.detail };
  }
  return null;
}
/** **`format!("{:?}", Option<String>)` ✓**（**`preview_suggestion` 的 `error` 是 `Some("…")` 形态 ✓**）：
 *   **∴ 服务端**直接 Debug 打印 `Option` ✗ 不是裸字符串 ✓ ⇒ **∴ 判据读原样也要照抄 ✓**。 */
function p2DebugOptionString(detail) { return `Some(${JSON.stringify(detail)})`; }

/** **效果名白名单 ✓**（**照 `yanshi-render/src/filter.rs:3291` / `:3321` 逐字 ✓**，
 *   **顺序也照抄** —— 错误文案里的「支持 …」由它 join 出来 ✓）。 */
const P2_ADJUSTMENT_NAMES = [
  "brightness_contrast", "saturation", "invert", "levels", "exposure", "white_balance",
  "curves", "hsl", "posterize", "color_balance", "split_toning", "vibrance",
  "black_and_white", "channel_mixer", "calibrate_primaries", "gradient_map",
  "shadows_highlights", "selective_color", "color_lut", "red_eye", "denoise",
  "lens", "perspective", "dehaze", "clarity", "texture",
];
const P2_FILTER_NAMES = [
  "box_blur", "blur", "gaussian_blur", "motion_blur", "sharpen", "noise", "vignette",
  "glow", "clarity", "dehaze", "film_grain", "brightness_contrast", "saturation",
  "invert", "relief",
];
/** **B 期项 13 的本地镜像 ✓**：与服务端 `list_effects` 回执的 `filter_aliases` 同条目 ✓
 *   （`tools.rs:13236`；条目只写别名关系，不暗示 impasto 可用 ⇒ relief 高度源是 C 期的事 ✓）。 */
const P2_FILTER_ALIASES = [{ name: "impasto", alias_of: "relief", note: "同 relief" }];
/** **数值键范围表 ✓**（**照 `tools_effect.rs:58–131` 逐条 ✓**，**`kind/名` 键 ✓**）：
 *   **∴ 空表 ≠ 缺省**✗ ⇒ **∴ 两边都等价于「不查范围、也不拒未知键」✓**（**服务端 `checks.is_empty()` ✓**）。 */
const P2_EFFECT_RANGES = {
  "adjustment/brightness_contrast": [["brightness", -1, 1], ["contrast", 0, 8]],
  "adjustment/saturation": [["amount", 0, 8]],
  "adjustment/invert": [],
  "adjustment/posterize": [["levels", 2, 64]],
  "adjustment/red_eye": [["strength", 0, 1], ["threshold", 0.5, 10]],
  "adjustment/vibrance": [["amount", 0, 2]],
  "adjustment/color_balance": [["amount", 0, 1]],
  "adjustment/split_toning": [["balance", -1, 1], ["amount", 0, 1]],
  "adjustment/denoise": [["luminance", 0, 1], ["detail", 0, 1], ["color", 0, 1]],
  "adjustment/lens": [["k1", -1, 1], ["k2", -1, 1], ["tca_r", 0.9, 1.1],
    ["tca_b", 0.9, 1.1], ["vignette_strength", 0, 1]],
  "adjustment/perspective": [["k1", -1, 1], ["k2", -1, 1]],
  "adjustment/levels": [["black", 0, 1], ["white", 0, 1], ["gamma", 0.01, 10]],
  "adjustment/exposure": [["ev", -10, 10]],
  "adjustment/white_balance": [["temperature", -1, 1], ["tint", -1, 1]],
  "adjustment/shadows_highlights": [["shadows", -1, 1], ["highlights", -1, 1]],
  "adjustment/dehaze": [["amount", -1, 1]],
  "adjustment/clarity": [["amount", -1, 1]],
  "adjustment/texture": [["amount", -1, 1]],
  "adjustment/curves": [],
  "adjustment/hsl": [["hue", -360, 360], ["saturation", 0, 8], ["lightness", -1, 1]],
  "filter/box_blur": [["radius", 1, 128], ["passes", 1, 8]],
  "filter/gaussian_blur": [["sigma", 0.05, 128]],
  "filter/motion_blur": [["angle", -360, 360], ["distance", 0, 512], ["samples", 2, 64]],
  "filter/sharpen": [["amount", 0, 5], ["radius", 1, 8]],
  // **`seed` 上界 = `u64::MAX as f64` ✓**（**照抄成 f64 能表示的那个数 ✓**）
  "filter/noise": [["amount", 0, 1], ["seed", 0, 18446744073709551616]],
  "filter/clarity": [["amount", 0, 2], ["radius", 2, 64]],
  "filter/dehaze": [["omega", 0, 1], ["floor", 0.02, 0.8]],
  "filter/film_grain": [["amount", 0, 1], ["size", 1, 8]],
  "filter/glow": [["threshold", 0, 1], ["radius", 1, 64], ["intensity", 0, 4]],
  "filter/vignette": [["strength", 0, 1], ["radius", 0, 2], ["softness", 0.02, 2]],
  "filter/brightness_contrast": [["brightness", -1, 1], ["contrast", 0, 8]],
  "filter/saturation": [["amount", 0, 8]],
  "filter/invert": [],
};
/** **非数值键白名单 ✓**（**照 `tools_effect.rs:159–188` 逐字 ✓**）：
 *   **∴ 只有「表的数值键非空」时才查未知键 ✗** ⇒ **∴ `curves`／`invert` 等空表效果**不拒任何键 ✓**。 */
const P2_EFFECT_NON_NUMERIC = [
  "adjust", "air", "angle", "auto_crop", "balance", "channel", "color", "constants",
  "detail", "highlights", "hue", "levels", "matrix", "midtones", "omega", "path",
  "points", "quad", "ranges", "samples", "seed", "shadows", "size", "stops", "table",
  "tca_b", "tca_r", "tint", "weights",
];
/** **效果参数校验 ✗**（**照 `validate_effect` 的**白名单／对象形状／数值范围／未知键**四段 ✓**；
 *   **∴ 未移植的深层结构检查 ✓**（**`levels` 的 channel／black<white、`dehaze` 的 air、`quad` 凸性、
 *   `matrix`／`weights`／`stops`／`tint` 等数组类**）⇒ **∴ 本地比服务端**宽**✗** ——
 *   **∴ 如实登记为残留差异 ✓**（**`preview_suggestion` 的判据只覆盖已移植的四段 ✓**）。 */
function p2ValidateEffect(kind, name, params) {
  const supported = kind === "adjustment" ? P2_ADJUSTMENT_NAMES : P2_FILTER_NAMES;
  const aliased = (kind === "filter" && (name === "emboss" || name === "impasto")) ||
                  (kind === "adjustment" && name === "vibrance_natural");
  if (!supported.includes(name) && !aliased) {
    return { code: "invalid_argument",
             detail: `内核未实现该${kind === "adjustment" ? "调整类型" : "滤镜"}：${name}；`
               + `支持 ${supported.join(" / ")}` };
  }
  if (params === null || params === undefined) return null;
  if (typeof params !== "object" || Array.isArray(params)) {
    return { code: "invalid_argument", detail: "params 必须是 JSON 对象" };
  }
  const canonical = name === "vibrance_natural" ? "vibrance" : name;
  const checks = P2_EFFECT_RANGES[`${kind}/${canonical}`] || [];
  for (const [key, low, high] of checks) {
    const value = typeof params[key] === "number" ? params[key] : null;
    if (value === null || !Number.isFinite(value)) continue;
    if (value < low || value > high) {
      return { code: "invalid_argument",
               detail: `${canonical} 的 ${key} 必须在 [${p2RustNum(low)}, ${p2RustNum(high)}] 内，`
                 + `得到 ${p2RustNum(value)}` };
    }
  }
  if (checks.length > 0) {
    const allowed = checks.map((c) => c[0]).sort();
    for (const key of Object.keys(params).sort()) {
      if (allowed.includes(key) || P2_EFFECT_NON_NUMERIC.includes(key)) continue;
      return { code: "invalid_argument",
               detail: `${canonical} 不认识参数 \`${key}\`（**不会静默忽略**）⇒ `
                 + `数值键：${allowed.join(" / ")}；另有若干数组类键`
                 + `（如 air／points／matrix／weights）按各自说明使用` };
    }
  }
  return null;
}
/** **预览的效果校验 ✗**（**照 `preview_effect_check` ✓**：**只对 `add_adjustment`／`add_filter` ✓**）。 */
function p2PreviewEffectCheck(tool, args) {
  let kind, key;
  if (tool === "add_adjustment") { kind = "adjustment"; key = "adjustment_type"; }
  else if (tool === "add_filter") { kind = "filter"; key = "filter_name"; }
  else return null;
  const name = args && typeof args === "object" ? args[key] : undefined;
  if (typeof name !== "string") return null;
  // **缺席 ⇒ `{}`；显式 `null` ⇒ `null`（`validate_effect` 对 null 放行 ✓）**
  const params = args.params === undefined ? {} : args.params;
  return p2ValidateEffect(kind, name, params);
}

/** **包围盒 ✓**（**照 `yanshi-core/src/conflict.rs:37–95` 逐条 ✓**：
 *   `from_value` 认 `{x,y,w|width,h|height}` ✓、`w<=0||h<=0` 视为空盒 ✓、
 *   `union` 遇空盒取另一边 ✓、`intersects` 空盒永不相交 ✓）。 */
function p2BboxFromValue(value) {
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;
  const num = (v) => (typeof v === "number" ? v : null);
  const x = num(value.x), y = num(value.y);
  const w = value.w !== undefined ? num(value.w) : num(value.width);
  const h = value.h !== undefined ? num(value.h) : num(value.height);
  if (x === null || y === null || w === null || h === null) return null;
  return { x, y, w, h };
}
function p2BboxIsEmpty(b) { return b.w <= 0 || b.h <= 0; }
function p2BboxRight(b) { return b.x + b.w; }
function p2BboxBottom(b) { return b.y + b.h; }
function p2BboxIntersects(a, b) {
  if (p2BboxIsEmpty(a) || p2BboxIsEmpty(b)) return false;
  return a.x < p2BboxRight(b) && b.x < p2BboxRight(a)
    && a.y < p2BboxBottom(b) && b.y < p2BboxBottom(a);
}
function p2BboxUnion(a, b) {
  if (p2BboxIsEmpty(a)) return b;
  if (p2BboxIsEmpty(b)) return a;
  const x = Math.min(a.x, b.x), y = Math.min(a.y, b.y);
  return { x, y,
    w: Math.max(p2BboxRight(a), p2BboxRight(b)) - x,
    h: Math.max(p2BboxBottom(a), p2BboxBottom(b)) - y };
}

/** **按 patch 顺序检查引用有效性 ✓**（**照 `check_step_order`（`tools.rs:13051`）逐条 ✓**）：
 *   **∴ 创建型步骤把目标加进集合 ✗；引用型要求**此刻存在** ✓；删除型把目标移出 ✓**。 */
const P2_CREATES_OBJECT_TOOLS = new Set([
  "create_object", "import_image", "draw_stroke", "draw_shape", "draw_text", "fill",
  "erase", "retouch", "liquify", "clone_stamp", "heal_stamp", "smudge", "patch",
  "add_adjustment", "add_filter",
]);
function p2CheckStepOrder(tool, args, layers, objects) {
  const str = (v) => (typeof v === "string" ? v : null);
  const layerId = args && typeof args === "object" ? str(args.layer_id) : null;
  const objectId = args && typeof args === "object" ? str(args.object_id) : null;
  const createsLayer = tool === "create_layer";
  const createsObject = P2_CREATES_OBJECT_TOOLS.has(tool);
  const deletesLayer = tool === "delete_layer";
  const deletesObject = tool === "delete_object" || tool === "tombstone";
  if (!createsLayer && !deletesLayer && layerId !== null && !layers.has(layerId)) {
    return { code: "reference_not_found",
             detail: `该步骤引用的图层 ${layerId} 在此时尚未创建（补丁顺序问题）` };
  }
  if (!createsObject && !deletesObject && objectId !== null && !objects.has(objectId)) {
    return { code: "reference_not_found",
             detail: `该步骤引用的对象 ${objectId} 在此时不存在（补丁顺序问题）` };
  }
  if (createsLayer && layerId !== null) layers.add(layerId);
  if (createsObject && objectId !== null) objects.add(objectId);
  if (deletesLayer && layerId !== null) layers.delete(layerId);
  if (deletesObject && objectId !== null) objects.delete(objectId);
  return null;
}

/** **单步预检 ✓**（**照 `check_single_step`（`tools.rs:12987`）四段顺序 ✓**）：
 *   未知工具 ⇒ 只读 ⇒ `validate_args` ⇒ 效果范围 ⇒ 引用顺序 ✓（**预览与接受共用 ✓**）。 */
function p2CheckSingleStep(tool, args, layers, objects) {
  const spec = p2ToolSpec(tool);
  if (!spec) return { code: "invalid_argument", detail: `未知工具 ${tool}` };
  if (!spec.mutating) {
    return { code: "invalid_argument", detail: `${tool} 是只读工具，不能作为补丁步骤` };
  }
  const bad = p2ValidateArgsFor(tool, args);
  if (bad) return bad;
  const effect = p2PreviewEffectCheck(tool, args);
  if (effect) return effect;
  return p2CheckStepOrder(tool, args, layers, objects);
}

/** **影响预估 ✓**（**照 `estimate_step_impact`（`tools.rs:13134`）逐条 ✓**）：
 *   **∴ 返回** `{affects, region}` ✓（**`affects` ∈ document／layer／region／unknown ✓**）。 */
function p2EstimateStepImpact(tool, args) {
  const A = args && typeof args === "object" && !Array.isArray(args) ? args : null;
  const num = (v) => (typeof v === "number" ? v : null);
  const number = (key) => (A ? num(A[key]) : null);
  const data = A && A.data && typeof A.data === "object" ? A.data : null;
  const lookup = (key) => (A && A[key] !== undefined ? A[key]
    : (data && data[key] !== undefined ? data[key] : null));
  const points_bbox = (margin) => {
    const points = lookup("points");
    if (!Array.isArray(points)) return null;
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    for (const point of points) {
      if (!Array.isArray(point) || point.length < 2) return null;
      const x = num(point[0]), y = num(point[1]);
      if (x === null || y === null) return null;
      minX = Math.min(minX, x); minY = Math.min(minY, y);
      maxX = Math.max(maxX, x); maxY = Math.max(maxY, y);
    }
    if (!Number.isFinite(minX)) return null;
    const m = Math.max(margin, 0);
    return { x: minX - m, y: minY - m, w: (maxX - minX) + m * 2, h: (maxY - minY) + m * 2 };
  };
  const region_bbox = () => {
    const fromRegion = A ? p2BboxFromValue(A.region) : null;
    if (fromRegion) return fromRegion;
    const fromBbox = p2BboxFromValue(lookup("bbox"));
    if (fromBbox) return fromBbox;
    const fromGeometry = data && data.geometry ? p2BboxFromValue(data.geometry.bbox) : null;
    if (fromGeometry) return fromGeometry;
    const x = number("x"), y = number("y"), w = number("w"), h = number("h");
    if (x === null || y === null || w === null || h === null) return null;
    return { x, y, w, h };
  };
  const object_bbox_from_args = () => {
    if (!A) return null;
    return p2BboxFromValue(A.region) || p2BboxFromValue(A.bbox);
  };
  if (tool === "create_layer" || tool === "reorder_layers" || tool === "reorder_objects"
      || tool === "delete_layer") {
    return { affects: "document", region: null };
  }
  if (tool.startsWith("add_adjustment") || tool.startsWith("add_filter")
      || tool.startsWith("update_adjustment") || tool.startsWith("update_filter")) {
    return { affects: "layer", region: null };
  }
  if (tool === "set_property") {
    const key = A && typeof A.key === "string" ? A.key : "";
    if (["z_index", "visible", "layer_id", "mask_id", "style_id", "type"].includes(key)) {
      return { affects: "layer", region: null };
    }
    return { affects: "region", region: object_bbox_from_args() };
  }
  if (tool.startsWith("clone_stamp") || tool.startsWith("heal_stamp") || tool === "smudge") {
    const size = number("size") !== null ? number("size") : 24;
    const offset = number("source_offset");
    const smudge = number("smudge_length");
    const reach = Math.max(offset === null ? 0 : Math.abs(offset), smudge === null ? 0 : smudge);
    return { affects: "region", region: points_bbox(size / 2 + reach + 2) };
  }
  if (tool.startsWith("liquify")) {
    const size = number("size") !== null ? number("size") : 80;
    const raw = number("strength") !== null ? number("strength") : 0.5;
    const strength = Math.min(Math.abs(raw), 2);
    return { affects: "region", region: points_bbox(size / 2 + strength * size + 2) };
  }
  if (tool === "patch") {
    const source = A ? p2BboxFromValue(A.source_region) : null;
    const width = source ? source.w : 0, height = source ? source.h : 0;
    const target = A && Array.isArray(A.target) ? A.target : null;
    if (!target) return { affects: "region", region: null };
    const x = num(target[0]) === null ? 0 : num(target[0]);
    const y = num(target[1]) === null ? 0 : num(target[1]);
    return { affects: "region", region: { x, y, w: width, h: height } };
  }
  if (tool.startsWith("draw_") || tool === "fill" || tool === "erase" || tool === "import_image") {
    const size = number("size") !== null ? number("size") : 0;
    return { affects: "region", region: points_bbox(size / 2 + 2) || region_bbox() };
  }
  if (tool.startsWith("move_") || tool === "transform" || tool.startsWith("delete_")) {
    return { affects: "region", region: object_bbox_from_args() };
  }
  if (tool.startsWith("create_mask")) return { affects: "layer", region: null };
  return { affects: "unknown", region: null };
}

/** **补丁步骤的粗分类 ✓**（**照 `tool_class`（`tools.rs:13264`）逐条 ✓**）。 */
function p2ToolClass(tool) {
  if (tool.startsWith("add_adjustment") || tool.startsWith("add_filter")
      || tool.startsWith("update_adjustment") || tool.startsWith("update_filter")) {
    return "effect";
  }
  if (tool.startsWith("clone_stamp") || tool.startsWith("heal_stamp") || tool === "smudge"
      || tool === "patch" || tool.startsWith("liquify")) {
    return "retouch";
  }
  if (tool.startsWith("draw_") || tool === "fill" || tool === "erase"
      || tool.startsWith("move_") || tool === "transform") {
    return "geometry";
  }
  if (tool.startsWith("create_") || tool.startsWith("reorder_") || tool === "set_property"
      || tool.startsWith("delete_")) {
    return "structure";
  }
  return "other";
}

/** **建议的作用域 ✗**（**照 `suggestion_scope`（`tools.rs:12919`）✓**：逐步预估 ＋ 图层／对象集合 ✓）。 */
function p2SuggestionScope(patch) {
  const scope = { affects: "none", region: null, layers: new Set(), objects: new Set() };
  for (const step of patch) {
    if (!step || typeof step !== "object" || Array.isArray(step)) continue;
    const tool = typeof step.tool === "string" ? step.tool : "";
    const args = step.arguments !== undefined ? step.arguments : {};
    const { affects, region } = p2EstimateStepImpact(tool, args);
    if (affects === "document") scope.affects = "document";
    else if (affects === "layer" && scope.affects !== "document") scope.affects = "layer";
    else if (affects === "region" && scope.affects === "none") scope.affects = "region";
    if (region) scope.region = scope.region ? p2BboxUnion(scope.region, region) : region;
    const A = args && typeof args === "object" && !Array.isArray(args) ? args : null;
    if (A && typeof A.layer_id === "string") scope.layers.add(A.layer_id);
    if (A && typeof A.object_id === "string") scope.objects.add(A.object_id);
  }
  return scope;
}
/** **两条建议是否冲突 ✓**（**照 `scopes_conflict`（`tools.rs:12991`）逐条 ✓**，**文案也逐字 ✓**）。 */
function p2ScopesConflict(a, b) {
  if (a.affects === "document" || b.affects === "document") {
    return "两者之一会改变图层结构（整文档失效）";
  }
  const sameObject = [...a.objects].some((id) => b.objects.has(id));
  if (sameObject) return "两者修改同一个对象";
  const layerLevel = (scope) => scope.affects === "layer";
  if (layerLevel(a) || layerLevel(b)) {
    const sameLayer = [...a.layers].some((id) => b.layers.has(id));
    if (sameLayer) return "同图层上存在作用于整层的改动（调整/滤镜）";
  }
  if (a.region && b.region && p2BboxIntersects(a.region, b.region)) {
    return "两者的影响区域相交";
  }
  return null;
}

export async function makeLocalApi(db) {
  // **★ `putAtom` 走本地薄包装 ✗ 不直接用 store 的那一个 ✓ ★**（第 2 批 ✓）：
  //   **∴ 服务端把「锁定」校验放在**提交层** ✗**（`log.rs:292 validate_commit` ⇒ 每条原子都过 ✓）
  //   ⇒ **∴ 本地照同一层挂闸 ✓**（见下面的 `putAtom` 包装 ＋ 模块级 `p2LockGuard` ✓）
  //     —— **∴ 不照「每个写入口各写一遍」✗**（**台账第 9 面「闸挂必经之路而非入口」✓**）。
  const { putAtom: __p2PutAtomRaw, atomsOf: atomsOfLocal, open, wrap, tx, writeSnapshot } = await import("./store.js");
  // **∴ 内核实例**按文档缓存**✗**（**`Map<doc, WasmKernel>` ✓**）。
  const kernels = new Map();

  // **★ C/S 客户端渲染模式：文档状态在服务端 ✗ ★**（第 10 轮 ✓）
  //   **∴ 「关闭服务器渲染」关的是**像素路径**✗**（落笔仍然 POST `/api/atoms` ✓ —— 见
  //     `viewer-app.js` 的 `submitAtom`）⇒ **∴ 此时本地原子日志是**空的 / 陈旧的**✗
  //     ⇒ **∴ 只折叠本地日志** ⇒ 内核拿到 **0 条原子** ⇒ 文档 **0×0**
  //       ⇒ **∴ 于是**每次 `render_region` 都 `needs_render`（**实测 ✓**：落笔后补画流水
  //         `no-url [143,117,26,8]` ✓、新建图层后面板 `state=` ✓、重载后 heavy 文档全白 ✓）。
  //   **∴ 修法**：C/S 模式下**读原子以服务端权威日志为准**（同源 GET `/api/atoms` ✓，
  //     带页面已有的 capability token ✓）；PWA（`__pwaLocalOnly === true`）仍读 IndexedDB ✓；
  //     网络不可用（离线 ✓）⇒ 回落本地日志 ✓（与修之前行为一致 ✓，不新挂死 ✓）。
  //   **∴ 为什么放在 `atomsOf` 这一层**：几十个只读端点都经它派生状态 ✓ ⇒
  //     一处改 ⇒ `list_layers`／`list_objects`／`get_log`／`render_region` **一起**看到真文档 ✓。
  const __csClientMode = () => {
    try {
      return typeof window !== "undefined" && window.__pwaLocalOnly !== true &&
        !!window.yanshiStats && typeof window.yanshiStats.token === "string" &&
        window.yanshiStats.token.length > 0;
    } catch (e) { return false; }
  };
  /** **服务端权威日志 ⇒ `atomsOf` 同形**（`{atom, seq}` ✓）；失败返回 `null` ✓。 */
  async function __serverAtoms(doc) {
    try {
      const stats = (typeof window !== "undefined" && window.yanshiStats) || {};
      const token = stats.token || "";
      if (!token || !doc) return null;
      const init = {};
      if (typeof AbortSignal !== "undefined" && typeof AbortSignal.timeout === "function") {
        init.signal = AbortSignal.timeout(8000);
      }
      const res = await fetch("/api/atoms?doc=" + encodeURIComponent(doc) +
                              "&token=" + encodeURIComponent(token), init);
      if (!res || !res.ok) return null;
      const body = await res.json();
      if (!body || body.ok !== true || !Array.isArray(body.atoms)) return null;
      // **截断对账** ✓（第三方审计 P2-N3 回归判据）：`/api/atoms` 缺省 limit=10,000
      // 且按 seq 升序 **take** —— 超过上限时**被丢弃的是最新的原子** ✗（服务端
      // `head_seq` 是日志末条 seq ✓）。本地内核若照常折叠这前一万条 ⇒ 画布
      // **静默停在旧状态** ✗（本仓判据就记录过 39,354 笔触的文档 ✓）。
      // ⇒ 末条 seq 落后于 head_seq（或日志非空却回 0 条）即认定截断：
      // **放弃本地渲染、回落服务端权威路径**（与 heavy 分支同路 ✓，不谎报成功 ✓）。
      // 该判定对 seq 非连续的日志同样成立 ✓（比"head_seq > atoms.length"更严 ✓）。
      const headSeq = Number(body.head_seq || 0);
      if (headSeq > 0) {
        const last = body.atoms.length
          ? Number(body.atoms[body.atoms.length - 1].seq || 0)
          : 0;
        if (last < headSeq) {
          console.warn("[yanshi] atom log truncated at limit (" + body.atoms.length +
                       " of head_seq " + headSeq + "), falling back to server render");
          return null;
        }
      }
      return body.atoms.map((atom) => ({ atom, seq: Number(atom.seq || 0) }));
    } catch (e) {
      return null; // 网络不可用 ⇒ 回落本地日志 ✓
    }
  }
  async function atomsOf(db, doc) {
    if (doc && __csClientMode()) {
      const rows = await __serverAtoms(doc);
      if (rows) return rows;
    }
    return await atomsOfLocal(db, doc);
  }
  /** **★ 写原子的必经之路：锁闸 ＋ 落盘 ✗ ★**（**本地镜像 `append_validated` ✓**）。
   *   **∴ 为什么包在这里 ✗**：40＋ 个写入口都经 `putAtom` ✓（**画／滤镜／图层／对象／撤销…**）
   *   ⇒ **∴ 一处挂闸 ⇒ 全部覆盖 ✓**；反之在每个入口各写一遍 ⇒ **将来新加的入口一定漏** ✗
   *   （**台账第 9 面 ✓：「闸挂必经之路而非入口」**）。
   *   **∴ 拒绝时**抛一个带 `__p2Failure` 的错误 ✗ ⇒ **由 `local()` 顶层 catch 换成 403 响应 ✓**
   *   （**∴ 不在每个 handler 各写一遍 try/catch ✓**；`p2bCatch` 见到它必须**再抛** ✗ 不许吞 ✓）。 */
  async function putAtom(db, doc, atom) {
    // **服务端 `append_validated` 给净荷补 `origin` ✓**（`tools.rs:574`）⇒ **本地同一处盖戳 ✓**：
    //   **∴ `get_log`／`list_atoms` 的 `stored_origin` 才会两边同形 ✓**（否则本地恒 null ✗）。
    //   **∴ 只在键不存在时盖 ✓**（`null` 也算已写 ✓ —— 服务端 `payload.get("origin").is_none()` 同义 ✓）。
    if (atom && atom.payload && typeof atom.payload === "object"
        && !Array.isArray(atom.payload)
        && !Object.prototype.hasOwnProperty.call(atom.payload, "origin")) {
      atom.payload.origin = p2AtomOriginOf(atom.kind, atom.payload);
    }
    const guard = p2LockGuard(await atomsOf(db, doc), atom);
    if (guard) {
      const err = new Error(guard.detail);
      err.__p2Failure = p2ToolFailure(guard.status, guard.code, guard.detail, guard.extra);
      throw err;
    }
    return await __p2PutAtomRaw(db, doc, atom);
  }
  /** **heavy 判定与服务端 `Atom::is_heavy` 同义** ✓（`yanshi-core/src/atom.rs:739` ✓）：
   *  retouch／liquify／declare_head，以及 payload.type = raster_patch／filter 的
   *  create_object／supersede／import_image。本地内核折叠 heavy 只会得到**空白补丁** ✗
   *  ⇒ 这些文档的像素只能由**服务端**出 ✓。 */
  function __atomHeavy(atom) {
    const kind = atom && atom.kind;
    if (kind === "retouch" || kind === "liquify" || kind === "declare_head") return true;
    if (kind === "create_object" || kind === "supersede" || kind === "import_image") {
      const type = atom.payload && atom.payload.type;
      return type === "raster_patch" || type === "filter";
    }
    return false;
  }

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
  // **★ `p2cNewObjectId` 的计数器必须放在 `makeLocalApi` 层 ✗ ★**（**2026-10-28 实测**：
  //   原来放在请求处理闭包里 ⇒ **每个请求都从 0 重新计数** ⇒ 同毫秒两笔生成**同一个 id**
  //   （实测 `obj_mvrx8335_1` ×2）⇒ 原子日志互相覆盖 ⇒ `tool-pwa-api-behaviour` ⑦l/⑧ 抖动红 ✓。
  //   **∴ 与 `__p2ctr` 同一纪律：跨请求存活 ✓**）
  let __p2cCtr = 0;
  function p2AtomId() {
    __p2ctr += 1;
    return "01P2" + String(Date.now()).padStart(13, "0") + String(__p2ctr).padStart(4, "0");
  }
  function p2Envelope(kind, payload, groupId) {
    const atom = {
      actor: "human:web",
      id: p2AtomId(),
      kind,
      payload,
      schema_version: 1,
      seq: 0, // **∴ `putAtom` 会自增覆盖 ✗**（**∴ 调用方给的 `seq` 一律忽略 ✓**）
      session: "session:web",
      timestamp: Date.now(),
    };
    if (typeof groupId === "string") atom.group_id = groupId;
    return atom;
  }
  /** **服务端 `origin_of` 的本地镜像 ✗**（**`tools.rs:17563` ✓**）：
   *   **∴ 口径 = 来源，不是存储形态 ✓**（笔触落成 `import_image` ✓ 但来源是 drawn ✓）：
   *   `source.brush` 有 ⇒ drawn ✓；`draw_stroke` ⇒ drawn ✓；
   *   `import_image` 且**没有** `bitmap` 键 ⇒ drawn ✓（**`bitmap:null` 算有 ✓** —— 服务端 `Value::get` 同义）；
   *   带 `bitmap` 的 `import_image` ⇒ imported ✓；建文档／建图层 ⇒ system ✓；其余 generated ✓。 */
  function p2AtomOriginOf(kind, payload) {
    const pl = payload || {};
    const src = pl.source;
    if (src && typeof src === "object" && typeof src.brush === "string") return "drawn";
    if (kind === "draw_stroke") return "drawn";
    if (kind === "import_image" && pl.bitmap === undefined) return "drawn";
    if (kind === "import_image") return "imported";
    if (kind === "create_document" || kind === "create_layer") return "system";
    return "generated";
  }
  /** **服务端 `payload_str` 的本地镜像 ✗**（**`tools.rs:20767` ✓**）：
   *   **∴ 先看净荷顶层 ✓，再看 `source` 里 ✓**（**两个都是字符串才算 ✓**）。 */
  function p2PayloadStr(atom, key) {
    const pl = (atom && atom.payload) || {};
    if (typeof pl[key] === "string") return pl[key];
    const src = pl.source;
    return src && typeof src === "object" && typeof src[key] === "string" ? src[key] : null;
  }
  /** **只看净荷顶层**（`atom_layer_id`／`atom_object_id` ✓ —— **它们不回落到 `source`** ✗）。 */
  function p2PayloadTopStr(atom, key) {
    const pl = (atom && atom.payload) || {};
    return typeof pl[key] === "string" ? pl[key] : null;
  }
  /** **服务端 `normalize_color` 的本地镜像 ✗**（**`tools.rs:8797` ✓**）：
   *   **∴ 数组（≥3 项）⇒ `{r,g,b,a}` ✓**，通道取**无符号整数**（读不出 ⇒ 0／alpha 缺省 255 ✓）；
   *   **∴ 非数组原样 ✓**（字符串／对象照回 ✓）。 */
  function p2NormalizeColor(value) {
    if (value === undefined) return null;
    if (Array.isArray(value) && value.length >= 3) {
      const channel = (index, fallback) => {
        const v = value[index];
        return typeof v === "number" && Number.isInteger(v) && v >= 0 ? v : fallback;
      };
      return { r: channel(0, 0), g: channel(1, 0), b: channel(2, 0), a: channel(3, 255) };
    }
    return value;
  }
  /** **`revertible` 的服务端镜像 ✗**（**`log.rs:881`：`is_state_effect ∪ {revert, reapply}` ✓**；
   *   **`is_state_effect` 的 24 种取自 `atom.rs:451–479` ✓**——
   *   **∴ `set_anchor`／`delete_anchor`／协作原子**不在其中**✗ ⇒ revert 它们必须逐条报失败 ✓**
   *   （**实测：真服务端对 `set_anchor` 回 `invalid_argument: 不能 revert 协作原子 …（set_anchor）` ✓**）。 */
  const P2_REVERTIBLE_KINDS = new Set([
    "create_document", "create_layer", "create_object", "create_selection", "create_mask",
    "update_mask", "delete_mask", "create_style", "create_checkpoint", "import_image",
    "draw_stroke", "fill", "draw_shape", "draw_text", "erase", "retouch", "liquify",
    "supersede", "move", "transform", "set_property", "reorder_layers", "reorder_objects",
    "tombstone", "revert", "reapply",
  ]);
  /** **「失效集」（suppressed）—— 服务端 `EffectiveActions::from_atoms` 的本地镜像 ✗**
   *   （**`yanshi-core/src/fold.rs:211–259` ✓ —— 一个事实只写一处 ✓**）：
   *   **∴ 规则是「每个目标上 seq 最大的生效动作胜出」✗**（**倒序扫描 + 首见登记 ✓**）：
   *     `revert` ⇒ 目标失效 ✓；**更晚的 `reapply` ⇒ 目标又生效** ✓
   *     （**旧本地实现把 revert／reapply 混成一个「被中和过」集合 ✗ ⇒ redo 过的对象
   *       永远死着 ⇒ undo 栈与画面互相矛盾 —— 本轮差分探针实测抓出的 ✓**）；
   *   **∴「撤销撤销」级联 ✓**：动作自己也被有效 revert 撤掉时它失效（**登记在动作 id 上 ✓**），
   *     于是目标上更早的动作重新胜出 ✓（**照 `fold.rs:213–247` 的两个分支逐字 ✓**）；
   *   **∴ 返回的集合 = 「当前不再生效的原子」**，`p2ObjectView`／`p2History`／`p2cState`／
   *     `__p2CollectGestures` 共用同一份口径 ✓。 */
  function p2Neutralized(rows) {
    rows = p2StateWindow(rows);
    const atoms = rows.map((r) => (r && r.atom) || r || {});
    const actions = new Map(); // 目标 id -> 最新**生效**动作（"revert" | "reapply"）
    const suppressed = new Set(); // 当前不再生效的原子
    const isHistory = (k) => k === "revert" || k === "reapply";
    for (let i = atoms.length - 1; i >= 0; i--) {
      const a = atoms[i];
      const target = isHistory(a.kind) && a.payload ? a.payload.target : null;
      if (a.kind === "revert") {
        if (actions.get(a.id) === "revert") suppressed.add(a.id); // revert(revert(x)) ≡ reapply(x)
        else if (typeof target === "string" && target) {
          if (!actions.has(target)) actions.set(target, "revert");
        }
      } else if (a.kind === "reapply") {
        if (actions.get(a.id) === "revert") suppressed.add(a.id); // revert(reapply(x)) ⇒ 维持 x 被抑制
        else if (typeof target === "string" && target) {
          if (!actions.has(target)) actions.set(target, "reapply");
        }
      } else if (actions.get(a.id) === "revert") {
        suppressed.add(a.id);
      }
    }
    return suppressed;
  }
  /** **哪些原子「建了一个可寻址的对象」—— 一个事实只写一处 ✗ 的权威集合 ✓**（本轮 ✓）：
   *   **∴ 出处 = 服务端折叠器**（**`yanshi-core/src/fold.rs:1089` 与 `1116–1151` ✓**）：
   *     `CreateObject ∣ ImportImage` **总是建**✓；
   *     `DrawStroke ∣ DrawShape ∣ DrawText ∣ Fill ∣ Erase ∣ Retouch ∣ Liquify ∣ Supersede`
   *     **对**没见过**的 `object_id` 也建**✓（**见过**则 supersede ✓ ⇒ 本折叠器取**第一条**建它的原子即等价 ✓**）。
   *   **∴ 为什么必须共享 ✓**：**`p2ObjectView`／`list_objects`／`duplicate_layer`** 三处此前各写各的口径 ✗
   *     （**只认 `create_object`✗／只认 `import_image∣∣create_object`✗／任何带 `object_id` 的都算✗**）
   *     ⇒ 于是**同一类 bug 在三个地方犯了三遍**✓ —— 折叠器漏认 ⇒ 对该对象的所有工具端点一律「对象不存在」✓
   *     （**导入图 ✗、画的形／字／填充 ✗、离线笔触 ✗**）。**收敛成一个集合后 ✗ 一处对齐 ⇒ 三处同时修好 ✓**。
   */
  const P2_OBJECT_CREATE_KINDS = new Set([
    "create_object", "import_image", "draw_stroke", "draw_shape", "draw_text",
    "fill", "erase", "retouch", "liquify", "supersede",
  ]);
  /** **`object_data` 的本地等价 ✗**（**照 `fold.rs:1553` 逐字 ✓**：「有 `data` ⇒ 用 `data`✓，否则 ⇒ 用整个净荷 ✓」）：
   *   **∴ `import_image`/`draw_*`/`fill` 把净荷写在**顶层还是 `data` 子对象 ✗** 都由这一条兜住 ✓**
   *   （**`import_image` 无 `data` ⇒ 用净荷 ✓；`create_object`/`draw_shape` 有 `data` ⇒ 用 `data` ✓**）。 */
  function p2ObjectData(createPayload) {
    return createPayload.data !== undefined ? createPayload.data : createPayload;
  }
  /** **对象类型（折叠口径）✗**（**照 `fold.rs:1528 parse_object_type` ＋ `:1549 object_type_for` 逐字 ✓**）：
   *   **∴ `kind` 覆盖**优先于净荷声明的 `type` ✓（`draw_shape⇒shape`／`draw_text⇒text`／
   *   `retouch⇒retouch`／`erase⇒retouch`／`liquify⇒liquify` ✓），其余按声明 ✓、未知 ⇒ `stroke` ✓。
   *   **∴ `set_property{type}` 可以再覆盖**它 ✓（`fold.rs:1923` ✓；**非字符串 ⇒ 服务端 `parse_object_type(None)`
   *   ⇒ `stroke` ✓** —— 不是「保持旧值」✗）。
   *   **∴ 为什么不用 `p2ObjectView().type` ✗**：那个只取「声明的 `type`」✗ ⇒ `draw_shape`
   *   （服务端**不写** `payload.type` ✓）在本地会读成 `null` ✓ ⇒ **图形被当成笔迹**✗
   *   （`vector_hit_test` 的 `strokes_examined` 会虚高 ✓）。
   */
  const P2_OBJECT_TYPE_OF_KIND = {
    draw_shape: "shape", draw_text: "text", retouch: "retouch",
    erase: "retouch", liquify: "liquify",
  };
  const P2_OBJECT_TYPE_DECLARED = {
    shape: "shape", text: "text", adjustment: "adjustment", filter: "filter",
    raster_patch: "raster_patch", retouch: "retouch", liquify: "liquify",
    instance: "instance", group: "group", path: "path",
  };
  function p2ParseObjectType(declared) {
    return typeof declared === "string" && P2_OBJECT_TYPE_DECLARED[declared]
      ? P2_OBJECT_TYPE_DECLARED[declared] : "stroke";
  }
  /** **对象在折叠后的类型 ✗**（**创建**取第一条建它的原子 ✓ ＋ **对后续 `set_property{type}` 生效**
   *   ✓ —— **被中和的那条不算** ✓，与折叠同口径 ✓）。**没建过 ⇒ `null`** ✓。 */
  function p2ObjectTypeOf(rows, objectId) {
    rows = p2StateWindow(rows);
    const neutral = p2Neutralized(rows);
    let type = null;
    for (const r of rows) {
      const a = (r && r.atom) || {};
      const pl = a.payload || {};
      if (type === null) {
        if (P2_OBJECT_CREATE_KINDS.has(a.kind) && pl.object_id === objectId) {
          type = P2_OBJECT_TYPE_OF_KIND[a.kind]
            || p2ParseObjectType(typeof pl.type === "string" ? pl.type : null);
        }
        continue;
      }
      if (a.kind === "set_property" && pl.object_id === objectId && pl.key === "type"
          && !neutral.has(a.id)) {
        type = p2ParseObjectType(typeof pl.value === "string" ? pl.value : null);
      }
    }
    return type;
  }
  /** **图层类型（折叠口径）✗**（**照 `layer_type_of`（`tools_vector.rs:15`）＋ `parse_layer_type`
   *   （`fold.rs:1516`）逐字 ✓**）：**∴ 服务端查的是 `state.layers.get()` ✗ 不是 `alive_layers()` ✓**
   *   ⇒ **已 tombstone 的层仍解析得类型** ✓（它的对象**全随层死** ⇒ 读数是 0 而不是 404 ✓）。
   *   **∴ 返回** `null`（层不在状态里 ⇒ 404 ✓）**或** `"raster"｜"vector"｜"layer_group"｜"mask"` ✓。
   *   **∴ `set_property{type}` 照 `apply_property_to_layer` ✓**：**非字符串 ⇒ `parse_layer_type(None)`
   *   ⇒ `raster` ✓**（不是保持旧值 ✗）。
   */
  function p2LayerTypeFold(rows, layerId) {
    rows = p2StateWindow(rows);
    const neutral = p2Neutralized(rows);
    const parse = (value) => (value === "vector" ? "vector" : value === "layer_group" ? "layer_group"
      : value === "mask" ? "mask" : "raster");
    let found = false;
    let type = "raster";
    for (const r of rows) {
      const a = (r && r.atom) || {};
      const pl = a.payload || {};
      if (a.kind === "create_document") {
        if (layerId === "layer_default" && !found) { found = true; type = "raster"; }
        continue;
      }
      if (a.kind === "create_layer" && !neutral.has(a.id)) {
        const id = (typeof pl.layer_id === "string" && pl.layer_id)
          || (typeof a.layer_id === "string" && a.layer_id) || null;
        if (id === layerId && !found) {
          found = true;
          type = typeof pl.type === "string" && pl.type ? parse(pl.type) : "raster";
          continue;
        }
      }
      if (a.kind === "set_property" && pl.layer_id === layerId && pl.key === "type"
          && !neutral.has(a.id) && found) {
        type = typeof pl.value === "string" ? parse(pl.value) : "raster";
      }
    }
    return found ? type : null;
  }
  /** **矢量笔迹几何 ✗**（**照 `brush.rs:40 StrokeGeometry::from_value` 逐字 ✓**）：
   *   **∴ 有一颗坏点 ⇒ 整个 `null`** ✓（与 Rust 的 `?` 一致：**不是**跳过那一颗 ✗）——
   *   调用方**照样把它算进 `strokes_examined`** ✓，只是不出命中 ✓。
   *   **∴ 压力缺省 1.0** ✓（数组第 3 元／对象的 `pressure` ✓，非数字也按缺省 ✓）；
   *   **∴ `smooth` 只认字面 `true`** ✓（`as_bool` ⇒ `"yes"` 不算 ✓）。
   */
  function p2VectorGeometry(data) {
    if (!data || typeof data !== "object" || Array.isArray(data)) return null;
    const list = data.points;
    if (!Array.isArray(list)) return null;
    const points = [];
    for (const item of list) {
      if (Array.isArray(item) && item.length >= 2) {
        if (typeof item[0] !== "number" || typeof item[1] !== "number") return null;
        points.push({ x: item[0], y: item[1],
                      pressure: typeof item[2] === "number" ? item[2] : 1 });
        continue;
      }
      if (!item || typeof item !== "object" || Array.isArray(item)) return null;
      if (typeof item.x !== "number" || typeof item.y !== "number") return null;
      points.push({ x: item.x, y: item.y,
                    pressure: typeof item.pressure === "number" ? item.pressure : 1 });
    }
    if (points.length === 0) return null;
    return { points, smooth: data.smooth === true };
  }
  /** **Catmull-Rom 采样 ✓**（**照 `brush.rs:106 catmull_rom_smooth` 逐字 ✓**：端点**反射**补齐 ✓、
   *   只输出每段**内部**采样 ✓、随后补段端点 ✓；**公式与运算顺序也照抄** ✓ ⇒ 同一个双精度结果 ✓）。 */
  function p2VectorSmooth(points, subdivisions) {
    if (points.length < 3 || subdivisions < 2) return points.slice();
    const first = points[0], second = points[1];
    const last = points[points.length - 1], prev = points[points.length - 2];
    const control = [
      { x: 2 * first.x - second.x, y: 2 * first.y - second.y, pressure: first.pressure },
      ...points,
      { x: 2 * last.x - prev.x, y: 2 * last.y - prev.y, pressure: last.pressure },
    ];
    const out = [first];
    const interp = (a, b, c, d, t, t2, t3) => 0.5 * ((2 * b) + (-a + c) * t
      + (2 * a - 5 * b + 4 * c - d) * t2 + (-a + 3 * b - 3 * c + d) * t3);
    for (let index = 0; index < control.length - 3; index++) {
      const p0 = control[index], p1 = control[index + 1];
      const p2 = control[index + 2], p3 = control[index + 3];
      for (let step = 1; step < subdivisions; step++) {
        const t = step / subdivisions;
        const t2 = t * t, t3 = t * t * t;
        out.push({
          x: interp(p0.x, p1.x, p2.x, p3.x, t, t2, t3),
          y: interp(p0.y, p1.y, p2.y, p3.y, t, t2, t3),
          pressure: interp(p0.pressure, p1.pressure, p2.pressure, p3.pressure, t, t2, t3),
        });
      }
      out.push(p2);
    }
    return out;
  }
  /** **脊线半径 ✗**（**照 `vector.rs:34 spine_radius` 逐字 ✓**）：
   *   **∴ `pressure_size` 恒视为真** ✓ —— `BrushSpec::from_value` **从不读**这个键 ✗
   *   （**∴ 缺省 `true` ✓**）⇒ **本地也不许读它** ✗（否则同一笔两边半径不同 ✗）。 */
  function p2VectorSpineRadius(brush, pressure) {
    const p = Math.min(1, Math.max(0, pressure));
    const scale = brush.minSizeRatio + (1 - brush.minSizeRatio) * p;
    return Math.max(0, brush.size / 2 * scale);
  }
  /** **`BrushSpec::from_value` 里影响半径的两个键 ✓**（`size` 缺省 4、下限 0.05 ✓；
   *   `min_size_ratio` 缺省 0.25、夹到 [0,1] ✓）。 */
  function p2VectorBrush(data) {
    const d = data && typeof data === "object" && !Array.isArray(data) ? data : {};
    const size = typeof d.size === "number" ? Math.max(d.size, 0.05) : 4;
    const ratio = typeof d.min_size_ratio === "number"
      ? Math.min(1, Math.max(0, d.min_size_ratio)) : 0.25;
    return { size, minSizeRatio: ratio };
  }
  /** **点到线段最近点 ✗**（**照 `vector.rs:57 segment_closest` 逐字 ✓**，
   *   含 `len2 <= 1e-24 ⇒ t = 0` 的退化保护 ✓ —— **零长线段**不会除出 `NaN` ✗）。 */
  function p2VectorSegmentClosest(a, b, qx, qy) {
    const abx = b.x - a.x, aby = b.y - a.y;
    const len2 = abx * abx + aby * aby;
    let t = 0;
    if (!(len2 <= 1e-24)) {
      t = ((qx - a.x) * abx + (qy - a.y) * aby) / len2;
      t = Math.min(1, Math.max(0, t));
    }
    const dx = qx - (a.x + abx * t), dy = qy - (a.y + aby * t);
    return { t, distance: Math.sqrt(dx * dx + dy * dy) };
  }
  /** **脊线命中 ✗**（**照 `vector.rs:142 spine_hit` 逐字 ✓**）：**有效脊线**在 `smooth` 时是
   *   渲染端同一份 Catmull-Rom 采样 ✓（`SMOOTH_SUBDIVISIONS = 8` ✓）；**单点笔**按点距算 ✓；
   *   **多段取「余量最小」** ✓（**严格 `<`** ⇒ 并列时先到者留 ✓）。
   */
  function p2VectorSpineHit(brush, geometry, qx, qy) {
    const spine = geometry.smooth ? p2VectorSmooth(geometry.points, 8) : geometry.points;
    if (spine.length === 0) return null;
    let best = null;
    const consider = (distance, pressure) => {
      const radius = p2VectorSpineRadius(brush, pressure);
      const hit = { distance, radius, clearance: distance - radius };
      if (best === null || hit.clearance < best.clearance) best = hit;
    };
    if (spine.length === 1) {
      const p = spine[0];
      const dx = qx - p.x, dy = qy - p.y;
      consider(Math.sqrt(dx * dx + dy * dy), p.pressure);
      return best;
    }
    for (let i = 0; i + 1 < spine.length; i++) {
      const a = spine[i], b = spine[i + 1];
      const seg = p2VectorSegmentClosest(a, b, qx, qy);
      consider(seg.distance, a.pressure + (b.pressure - a.pressure) * seg.t);
    }
    return best;
  }
  /** **路径子路径校验 ✗**（**照 `parse_path_subpaths`（`object.rs:630`）的**接受集合**逐字 ✓**：
   *   **`fill_rule` 键存在但非 `evenodd`／`nonzero` 字符串 ⇒ 非法** ✓（**`null` 也算存在 ✗**
   *   ⇒ 「键缺失 ⇒ 缺省 evenodd」与「显式 null」是两回事 ✓）；`subpaths` 必须**非空数组** ✓、
   *   每条 `segments` 必须**非空** ✓、点必须是**恰好两元的有限数对** ✓、
   *   `closed` 缺省 `false`、给了就必须是布尔 ✓。**非法 ⇒ `null`**（调用方报「不是 path 形状」✓）。
   *   **∴ 输出是**入库同构**的规范形 ✓**（`subpaths_to_json` ✓：`start`／`closed`／`segments` ✓，
   *   段只带该算子要的键 ✓）。
   */
  function p2PathSubpaths(geometry) {
    if (!geometry || typeof geometry !== "object" || Array.isArray(geometry)) return null;
    let fillRule = "evenodd";
    if (Object.prototype.hasOwnProperty.call(geometry, "fill_rule")) {
      const value = geometry.fill_rule;
      if (value !== "evenodd" && value !== "nonzero") return null;
      fillRule = value;
    }
    const list = geometry.subpaths;
    if (!Array.isArray(list) || list.length === 0) return null;
    const point = (value) => {
      if (!Array.isArray(value) || value.length !== 2) return null;
      if (!p2Num(value[0]) || !p2Num(value[1])) return null;
      return [value[0], value[1]];
    };
    const subpaths = [];
    for (const sub of list) {
      if (!sub || typeof sub !== "object" || Array.isArray(sub)) return null;
      const start = point(sub.start);
      if (!start) return null;
      let closed = false;
      if (Object.prototype.hasOwnProperty.call(sub, "closed")) {
        if (typeof sub.closed !== "boolean") return null;
        closed = sub.closed;
      }
      if (!Array.isArray(sub.segments) || sub.segments.length === 0) return null;
      const segments = [];
      for (const seg of sub.segments) {
        if (!seg || typeof seg !== "object" || Array.isArray(seg)) return null;
        if (seg.op === "L") {
          const to = point(seg.to);
          if (!to) return null;
          segments.push({ op: "L", to });
        } else if (seg.op === "C") {
          const c1 = point(seg.c1), c2 = point(seg.c2), to = point(seg.to);
          if (!c1 || !c2 || !to) return null;
          segments.push({ op: "C", c1, c2, to });
        } else {
          return null;
        }
      }
      subpaths.push({ start, closed, segments });
    }
    return { fillRule, subpaths };
  }
  /** **三次 Bézier 霍纳求值 ✓**（**照 `path.rs:77 cubic_at` 逐字 ✓**：系数与运算顺序照抄 ✓）。 */
  function p2CubicAt(p0, p1, p2, p3, t) {
    const evalAxis = (a, b, c, d) => {
      const a3 = -a + 3 * b - 3 * c + d;
      const a2 = 3 * a - 6 * b + 3 * c;
      const a1 = -3 * a + 3 * b;
      return ((a3 * t + a2) * t + a1) * t + a;
    };
    return [evalAxis(p0[0], p1[0], p2[0], p3[0]), evalAxis(p0[1], p1[1], p2[1], p3[1])];
  }
  /** **Wang 的 L ✗**（**照 `path.rs:88 wang_l` 逐字 ✓**：两个二阶差分向量的 L1 范数取大者 ✓）。 */
  function p2WangL(p0, p1, p2, p3) {
    const d1 = Math.abs(p0[0] - 2 * p1[0] + p2[0]) + Math.abs(p0[1] - 2 * p1[1] + p2[1]);
    const d2 = Math.abs(p1[0] - 2 * p2[0] + p3[0]) + Math.abs(p1[1] - 2 * p2[1] + p3[1]);
    return Math.max(d1, d2);
  }
  /** **Wang 定采样数 ✗**（**照 `path.rs:96 wang_steps_raw` 逐字 ✓**：`ceil(sqrt(3L/(4·tol)))` ✓，
   *   `L <= 0 ⇒ 1` ✓）。 */
  function p2WangStepsRaw(l, tol) {
    if (l <= 0) return 1;
    return Math.ceil(Math.sqrt(3 * l / (4 * tol)));
  }
  /** **de Casteljau 对半分治 ✓**（**照 `path.rs:108 split_cubic_half` 逐字 ✓**：`t=0.5` 的除以 2
   *   在二进制浮点下**精确** ✓ ⇒ 拆分点确定 ✓）。 */
  function p2SplitCubicHalf(p0, p1, p2, p3) {
    const mid = (a, b) => [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2];
    const q0 = mid(p0, p1), q1 = mid(p1, p2), q2 = mid(p2, p3);
    const r0 = mid(q0, q1), r1 = mid(q1, q2), m = mid(r0, r1);
    return [[p0, q0, r0, m], [m, r1, q2, p3]];
  }
  const P2_MAX_STEPS_PER_SEG = 256;
  /** **一段三次 Bézier 采样进 `out`（不含起点、含终点 ✓）**（**照 `path.rs:129 flatten_cubic_into` 逐字 ✓**：
   *   `N` 超上限时**先对半分治再各自定 N** ✓（钳 N 会让误差界失效 ✓）、深度上限 8 ✓、
   *   末点**直出 `p3`** ✓（Horner 在 t=1 有 1-ulp 漂移 ✓）。 */
  function p2FlattenCubic(out, p0, p1, p2, p3, tol, depth) {
    const raw = p2WangStepsRaw(p2WangL(p0, p1, p2, p3), tol);
    if (raw > P2_MAX_STEPS_PER_SEG && depth > 0) {
      const [a, b] = p2SplitCubicHalf(p0, p1, p2, p3);
      p2FlattenCubic(out, a[0], a[1], a[2], a[3], tol, depth - 1);
      p2FlattenCubic(out, b[0], b[1], b[2], b[3], tol, depth - 1);
      return;
    }
    const n = raw <= 1 ? 1
      : raw >= P2_MAX_STEPS_PER_SEG ? P2_MAX_STEPS_PER_SEG : Math.trunc(raw);
    for (let i = 1; i < n; i++) out.push(p2CubicAt(p0, p1, p2, p3, i / n));
    out.push(p3);
  }
  /** **子路径展平 ✓**（**照 `path.rs:170 flatten_subpath` 逐字 ✓**：闭合且末点离起点 `> 1e-9`
   *   ⇒ 追加回首点 ✓；`L` 直接输出终点 ✓）。 */
  function p2FlattenSubpath(sub, tol) {
    const t = Number.isFinite(tol) && tol > 0 ? tol : 0.1;
    const out = [sub.start];
    let cur = sub.start;
    for (const seg of sub.segments) {
      if (seg.op === "L") {
        out.push(seg.to);
      } else {
        p2FlattenCubic(out, cur, seg.c1, seg.c2, seg.to, t, 8);
      }
      cur = seg.to;
    }
    if (sub.closed) {
      const last = out[out.length - 1] || sub.start;
      const dx = last[0] - sub.start[0], dy = last[1] - sub.start[1];
      if (dx * dx + dy * dy > 1e-18) out.push(sub.start);
    }
    return out;
  }
  /** **折线总长 ✓**（**照 `path.rs:202 polyline_length` 逐字 ✓**：逐段 `sqrt` **顺序累加** ✓）。 */
  function p2PolylineLength(points) {
    let sum = 0;
    for (let i = 1; i < points.length; i++) {
      const dx = points[i][0] - points[i - 1][0];
      const dy = points[i][1] - points[i - 1][1];
      sum += Math.sqrt(dx * dx + dy * dy);
    }
    return sum;
  }
  /** **控制点包围盒 ✓**（**照 `path.rs:764 path_control_bbox` 逐字 ✓**：起点＋每段 `c1`／`c2`／`to` ✓，
   *   **禁止用展平点列算** ✓）。 */
  function p2PathControlBbox(subpaths) {
    let any = false;
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    const visit = (p) => {
      any = true;
      if (p[0] < minX) minX = p[0];
      if (p[1] < minY) minY = p[1];
      if (p[0] > maxX) maxX = p[0];
      if (p[1] > maxY) maxY = p[1];
    };
    for (const sub of subpaths) {
      visit(sub.start);
      for (const seg of sub.segments) {
        if (seg.op === "L") {
          visit(seg.to);
        } else {
          visit(seg.c1);
          visit(seg.c2);
          visit(seg.to);
        }
      }
    }
    return any ? [minX, minY, maxX - minX, maxY - minY] : null;
  }
  /** **`f64` 的 Rust `Display` 形 ✓**（`path.rs:459` 的 `d` 用 `format!` ✓）：
   *   **最短往返数字 ＋ 无科学计数法** ✓（`1e-7 ⇒ 0.0000001`、`1e21 ⇒ 1000…` ✓）。
   *   **JS `String(v)` 给出同一批最短数字** ✓（ECMAScript 规定最短往返 ✓；与 Rust 实测同 ✓）⇒
   *   **只需把指数形展开成十进制** ✓；`-0 ⇒ "-0"` ✓。 */
  function p2RustF64(value) {
    // **`-0.0` 的 `Display` 是 `-0` ✓**（`String(-0)` 给 `0` ✗ —— 用倒数判别 ✓）。
    if (value === 0 && 1 / value === -Infinity) return "-0";
    const text = String(value);
    if (!/e/i.test(text)) return text;
    const match = /^(-?)(\d+)(?:\.(\d+))?e([+-]?\d+)$/i.exec(text);
    if (!match) return text;
    const sign = match[1] || "";
    const digits = match[2] + (match[3] || "");
    const point = match[2].length + parseInt(match[4], 10);
    if (point <= 0) return sign + "0." + "0".repeat(-point) + digits;
    if (point >= digits.length) return sign + digits + "0".repeat(point - digits.length);
    return sign + digits.slice(0, point) + "." + digits.slice(point);
  }
  /** **等价 SVG `d` ✓**（**照 `path.rs:459 subpaths_to_svg_d` 逐字 ✓**：绝对 `M/L/C/Z` ✓，
   *   子路径之间一个空格 ✓，闭合以 `Z` 收尾 ✓）。 */
  function p2PathSvgD(subpaths) {
    const parts = [];
    for (const sub of subpaths) {
      let text = `M ${p2RustF64(sub.start[0])} ${p2RustF64(sub.start[1])}`;
      for (const seg of sub.segments) {
        if (seg.op === "L") {
          text += ` L ${p2RustF64(seg.to[0])} ${p2RustF64(seg.to[1])}`;
        } else {
          text += ` C ${p2RustF64(seg.c1[0])} ${p2RustF64(seg.c1[1])}`
            + ` ${p2RustF64(seg.c2[0])} ${p2RustF64(seg.c2[1])}`
            + ` ${p2RustF64(seg.to[0])} ${p2RustF64(seg.to[1])}`;
        }
      }
      if (sub.closed) text += " Z";
      parts.push(text);
    }
    return parts.join(" ");
  }
  /** **一条子路径的节点列 ✓**（**照 `path.rs:437 node_anchors` 逐字 ✓**）：节点 0 = `start` ✓、
   *   节点 k = 第 k-1 段的终点 ✓ ⇒ 总数恒为 `segments.length + 1` ✓（**闭合回首段不是节点 ✗**）。
   *   **∴ 这是 `edit_path` 的统一下标口径 ✗**（与服务端同一个 ✓）。 */
  function p2PathNodeAnchors(sub) {
    const anchors = [sub.start];
    for (const seg of sub.segments) anchors.push(seg.to);
    return anchors;
  }
  /** **两点线性插值 ✓**（**照 `path.rs:450 lerp_point` ✓**：纯四则 ⇒ 逐位确定性 ✓）。 */
  function p2PathLerp(a, b, t) {
    return [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
  }
  /** **节点下标越界文案 ✓**（**照 `path.rs` 三处同一句 ✓**：`0..=len-1` 走 `saturating_sub` ✓）。 */
  function p2PathNodeRangeError(node, count) {
    return `节点下标 ${node} 越界（本子路径共 ${count} 个节点，下标 0..=${Math.max(0, count - 1)}）`;
  }
  /** **移动节点 ✓**（**照 `path.rs:504 move_path_node` 逐字 ✓**）：
   *   **∴ 相连的两个存储控制柄按**同一位移**刚性平移 ✗**（`node-1` 段的 `c2` 与 `node` 段的 `c1` ✓）；
   *   **∴ 闭合回首段是隐式直段 ⇒ 没有存储柄 ✓**（只移锚点 ✓）；其余控制点逐位不动 ✓。 */
  function p2PathMoveNode(sub, node, to) {
    if (!Number.isFinite(to[0]) || !Number.isFinite(to[1])) {
      return { error: `目标点必须是有限数，得到 (${p2RustF64(to[0])}, ${p2RustF64(to[1])})` };
    }
    const anchors = p2PathNodeAnchors(sub);
    if (node >= anchors.length) return { error: p2PathNodeRangeError(node, anchors.length) };
    const old = anchors[node];
    const delta = [to[0] - old[0], to[1] - old[1]];
    if (node === 0) {
      sub.start = to;
    } else {
      const seg = sub.segments[node - 1];
      if (seg.op === "C") {
        seg.c2 = [seg.c2[0] + delta[0], seg.c2[1] + delta[1]];
        seg.to = to;
      } else {
        seg.to = to;
      }
    }
    if (node < sub.segments.length) {
      const seg = sub.segments[node];
      if (seg.op === "C") seg.c1 = [seg.c1[0] + delta[0], seg.c1[1] + delta[1]];
    }
    return { ok: true };
  }
  /** **插入节点 ✓**（**照 `path.rs:543 insert_path_node` 逐字 ✓**）：
   *   **∴ 直段取插值点拆两段 ✓、三次段按 de Casteljau 在 t 处切分 ✓**（**几何逐位不变 ✓**）；
   *   **∴ 闭合子路径给 `segment === 段数` ⇒ 作用于隐式回首直段 ✗**（新节点落成末尾显式直段 ✓，
   *   返回值是**旧节点数** ✓）；`t` 必须**开区间** (0,1) ✓。 */
  function p2PathInsertNode(sub, segment, t) {
    if (!Number.isFinite(t) || t <= 0 || t >= 1) {
      return { error: `t 必须在 (0, 1) 开区间内（端点就是已有节点），得到 ${p2RustF64(t)}` };
    }
    const anchors = p2PathNodeAnchors(sub);
    if (segment === sub.segments.length && sub.closed && sub.segments.length > 0) {
      const mid = p2PathLerp(anchors[anchors.length - 1], sub.start, t);
      sub.segments.push({ op: "L", to: mid });
      return { ok: true, index: anchors.length };
    }
    if (segment >= sub.segments.length) {
      return { error: `段下标 ${segment} 越界（本子路径共 ${sub.segments.length} 段，下标 0..=${Math.max(0, sub.segments.length - 1)}）` };
    }
    const seg = sub.segments[segment];
    const p0 = anchors[segment];
    if (seg.op === "L") {
      const mid = p2PathLerp(p0, seg.to, t);
      sub.segments[segment] = { op: "L", to: mid };
      sub.segments.splice(segment + 1, 0, { op: "L", to: seg.to });
    } else {
      const q0 = p2PathLerp(p0, seg.c1, t);
      const q1 = p2PathLerp(seg.c1, seg.c2, t);
      const q2 = p2PathLerp(seg.c2, seg.to, t);
      const r0 = p2PathLerp(q0, q1, t);
      const r1 = p2PathLerp(q1, q2, t);
      const m = p2PathLerp(r0, r1, t);
      sub.segments[segment] = { op: "C", c1: q0, c2: r0, to: m };
      sub.segments.splice(segment + 1, 0, { op: "C", c1: r1, c2: q2, to: seg.to });
    }
    return { ok: true, index: segment + 1 };
  }
  /** **删除节点 ✓**（**照 `path.rs:599 delete_path_node` 逐字 ✓**）：
   *   **∴ 节点 0 ⇒ 丢第一段、新 `start` = 原第 1 个锚点 ✓**；**末节点 ⇒ 丢最后一段 ✓**；
   *   **中间节点 ⇒ 相邻两段合并 ✓**：两段都是直段 ⇒ 直段 ✓；**否则三次段 ✓** ——
   *   `c1` 取入段的 `c1`、`c2` 取出段的 `c2`，**直段那侧取**被删锚点** ✓**（形状会变 ✓，不假装无损 ✓）。
   *   **只剩一段 ⇒ 拒绝 ✓**（删完会零段、违反入库校验 ✓）。 */
  function p2PathDeleteNode(sub, node) {
    const anchors = p2PathNodeAnchors(sub);
    if (node >= anchors.length) return { error: p2PathNodeRangeError(node, anchors.length) };
    if (sub.segments.length < 2) {
      return { error: "子路径只剩一段，删节点会让它没有段（先 insert_node 或整条重画）" };
    }
    if (node === 0) {
      const newStart = anchors[1];
      sub.segments.shift();
      sub.start = newStart;
      return { ok: true };
    }
    if (node === anchors.length - 1) {
      sub.segments.pop();
      return { ok: true };
    }
    const incoming = sub.segments[node - 1];
    const outgoing = sub.segments[node];
    const deleted = anchors[node];
    const target = outgoing.to;
    if (incoming.op === "L" && outgoing.op === "L") {
      sub.segments[node - 1] = { op: "L", to: target };
    } else {
      const c1 = incoming.op === "C" ? incoming.c1 : deleted;
      const c2 = outgoing.op === "C" ? outgoing.c2 : deleted;
      sub.segments[node - 1] = { op: "C", c1, c2, to: target };
    }
    sub.segments.splice(node, 1);
    return { ok: true };
  }
  /** **设置一侧控制柄 ✓**（**照 `path.rs:660 set_path_handle` 逐字 ✓**）：
   *   **∴ 该侧已是三次段 ⇒ 直接改控制点 ✓**；**是直段 ⇒ 升格为三次段 ✗** ——
   *   给定的柄落在 `to` ✓、**另一侧控制点取段上 1/3（`In` 升格取 `c1`）／2/3（`Out` 升格取 `c2`）✓**；
   *   **∴ 闭合回首段 ⇒ 落成显式三次段（终点 = `start`）追加在末尾 ✓**；
   *   **∴ 开放子路径首节点 `In`／末节点 `Out` 不存在 ⇒ 明确报错 ✓**（不静默 ✓）。 */
  function p2PathSetHandle(sub, node, side, to) {
    if (!Number.isFinite(to[0]) || !Number.isFinite(to[1])) {
      return { error: `控制点必须是有限数，得到 (${p2RustF64(to[0])}, ${p2RustF64(to[1])})` };
    }
    const anchors = p2PathNodeAnchors(sub);
    if (node >= anchors.length) return { error: p2PathNodeRangeError(node, anchors.length) };
    if (side === "out") {
      if (node < sub.segments.length) {
        const seg = sub.segments[node];
        if (seg.op === "C") {
          seg.c1 = to;
        } else {
          sub.segments[node] = { op: "C", c1: to,
                                 c2: p2PathLerp(anchors[node], seg.to, 2 / 3), to: seg.to };
        }
        return { ok: true };
      }
      if (sub.closed && sub.segments.length > 0) {
        const last = anchors[node];
        sub.segments.push({ op: "C", c1: to,
                            c2: p2PathLerp(last, sub.start, 2 / 3), to: sub.start });
        return { ok: true };
      }
      return { error: `节点 ${node} 是开放子路径的末节点，没有 out 控制柄（其后没有段）` };
    }
    if (node > 0) {
      const seg = sub.segments[node - 1];
      if (seg.op === "C") {
        seg.c2 = to;
      } else {
        sub.segments[node - 1] = { op: "C", c1: p2PathLerp(anchors[node - 1], seg.to, 1 / 3),
                                   c2: to, to: seg.to };
      }
      return { ok: true };
    }
    if (sub.closed && sub.segments.length > 0) {
      const last = anchors[anchors.length - 1];
      sub.segments.push({ op: "C", c1: p2PathLerp(last, sub.start, 1 / 3),
                          c2: to, to: sub.start });
      return { ok: true };
    }
    return { error: `节点 ${node} 是开放子路径的首节点，没有 in 控制柄（其前没有段）` };
  }
  /** **变换净荷解析 ✗**（**照 `fold.rs:1753 transform_from_payload` 逐字 ✓**）：
   *   **没给 `transform`（或 `matrix` 不是 6 元）⇒ 单位阵 ✓**；
   *   **给了 6 元 ⇒ 从**全零**起逐槽取数 ✓**（**非数字 ⇒ 0 ✓**，**不是**保留单位阵的槽 ✗ ——
   *   **∴ 这两个缺省不同，各写各的才不漂移 ✓**）。
   *   **∴ 用途**：创建净荷与 `move`／`transform` 的**绝对**变换（折叠层同一函数 ✓）。 */
  function p2TransformFromPayload(payload) {
    const v = payload && payload.transform;
    if (v && Array.isArray(v.matrix) && v.matrix.length === 6) {
      const matrix = v.matrix.map((x) => (Number.isFinite(Number(x)) ? Number(x) : 0));
      let pivot = [0, 0];
      if (Array.isArray(v.pivot) && v.pivot.length === 2) {
        pivot = [Number.isFinite(Number(v.pivot[0])) ? Number(v.pivot[0]) : 0,
                 Number.isFinite(Number(v.pivot[1])) ? Number(v.pivot[1]) : 0];
      }
      return { matrix, pivot };
    }
    return { matrix: [1, 0, 0, 1, 0, 0], pivot: [0, 0] };
  }
  /** **变换 JSON 解析 ✗**（**照 `tools.rs:6501 transform_from_json` 逐字 ✓**）：
   *   **从**单位阵**起，`matrix` 6 元才覆盖 ✓、`pivot` 2 元才覆盖 ✓**；
   *   **∴ 与 `p2TransformFromPayload` 的「从零起」不同 ✗** —— **∴ 服务端就是两个函数 ✓**，
   *   一个给折叠净荷（`transform_from_payload` ✓），一个给 `local_transform`（这里 ✓）。 */
  function p2TransformFromJson(value) {
    const t = { matrix: [1, 0, 0, 1, 0, 0], pivot: [0, 0] };
    const m = value && value.matrix;
    if (Array.isArray(m) && m.length === 6) {
      for (let i = 0; i < 6; i++) t.matrix[i] = Number.isFinite(Number(m[i])) ? Number(m[i]) : 0;
    }
    const p = value && value.pivot;
    if (Array.isArray(p) && p.length === 2) {
      for (let i = 0; i < 2; i++) t.pivot[i] = Number.isFinite(Number(p[i])) ? Number(p[i]) : 0;
    }
    return t;
  }
  /** **仿射合成 ✓ `outer ∘ inner`（先前内后外 ✓，`pivot` 先折进矩阵 ✓）**：
   *   **照 `tools.rs:6521 compose_transforms` 逐字 ✓，连运算顺序都同 ✗**
   *   （**∴ 都是 f64 IEEE 运算 ✓ ⇒ **∴ 逐位相同 ✓** —— **`detach_instance` 的变换要靠这一条 ✓**）。 */
  function p2ComposeTransforms(outer, inner) {
    const flatten = (t) => {
      const m = t.matrix, px = t.pivot[0], py = t.pivot[1];
      return [m[0], m[1], m[2], m[3],
              m[4] + px - (m[0] * px + m[2] * py),
              m[5] + py - (m[1] * px + m[3] * py)];
    };
    const a = flatten(outer), b = flatten(inner);
    return {
      matrix: [
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
        a[0] * b[4] + a[2] * b[5] + a[4],
        a[1] * b[4] + a[3] * b[5] + a[5],
      ],
      pivot: [0, 0],
    };
  }
  /** **数值判定 ✓**（**照 serde_json 的 `as_f64` ✓**：只认 number ✓，字符串不算 ✓）。 */
  const p2Num = (value) => typeof value === "number" && Number.isFinite(value);
  const p2JsonTypeName = (value) => {
    if (value === null) return "null";
    if (Array.isArray(value)) return "array";
    return typeof value;
  };
  /** **`{x,y,w,h}` 解析 ✓**（**照 `Bbox::from_value` ✓**：只认对象 ✓、`w|width` 与 `h|height` 都认 ✓）。 */
  function p2BboxFrom(value) {
    if (!value || typeof value !== "object" || Array.isArray(value)) return null;
    const w = value.w !== undefined ? value.w : value.width;
    const h = value.h !== undefined ? value.h : value.height;
    if (!p2Num(value.x) || !p2Num(value.y) || !p2Num(w) || !p2Num(h)) return null;
    return { x: value.x, y: value.y, w, h };
  }
  /** **对象上「不进 `data`」的属性名 ✓**（**照 `apply_property_to_object` 的具名分支 ✓**：
   *  其余键（`override`／`sync_policy`／`master_ref`／`snapshot` …）都被**写进 `object.data`** ✓）。 */
  const P2_OBJECT_FIELD_PROPS = new Set([
    "visible", "enabled", "locked", "z_index", "layer_id", "type", "metadata", "transform",
  ]);
  /** **折叠后的 `object.data` ✓**（**照 `apply_property_to_object` 的 `other => data.insert` ✓**）：
   *  服务端 `state.objects[id].data` **已经含** `set_property` 写进去的键 ✓ ⇒ 本地视图的 `data`
   *  只有创建／`supersede` 的净荷 ✗ ⇒ **∴ 凡按 `data` 解析语义的地方（实例解析／图元解析）
   *  都必须经这里 ✓**，否则 `set_property{key:"master_ref"|"override"|"sync_policy"}` 会看不见 ✓。 */
  function p2EffectiveData(view) {
    const raw = view.data;
    const base = (raw && typeof raw === "object" && !Array.isArray(raw)) ? { ...raw } : {};
    const props = view.__props || {};
    for (const key of Object.keys(props)) {
      if (P2_OBJECT_FIELD_PROPS.has(key)) continue;
      base[key] = props[key];
    }
    // **`master_ref` 以视图字段为准 ✓**（它已经按「`set_property` 覆盖优先」算过 ✓）；
    //   **被覆盖成 `null` 时必须显式写 `null`** ✓（否则创建净荷里那份会留下来 ⇒ 看见旧引用 ✗）。
    if (props.master_ref !== undefined) base.master_ref = view.master_ref;
    return base;
  }
  /** **仿射映射一点 ✓**（**照 `Transform::apply_point` ✓**：**先减 pivot 再乘再补回 ✓**）。 */
  function p2ApplyPoint(transform, x, y) {
    const m = transform.matrix, px = transform.pivot[0], py = transform.pivot[1];
    const dx = x - px, dy = y - py;
    return [px + m[0] * dx + m[2] * dy + m[4], py + m[1] * dx + m[3] * dy + m[5]];
  }
  /** **恒等判定 ✓**（**照 `Transform::is_identity` ✓**：**只看矩阵、不看 pivot ✓**）。 */
  function p2IsIdentityMatrix(transform) {
    const m = transform.matrix;
    return m[0] === 1 && m[1] === 0 && m[2] === 0 && m[3] === 1 && m[4] === 0 && m[5] === 0;
  }
  /** **笔刷直径／抖动 ✓**（**照 `BrushSpec::from_value` 的缺省 ✓**：`size` 缺省 **4** ✓，
   *  下限 **0.05** ✓；`jitter` 缺省 0 ✓、负值按 0 ✓）—— **∴ 包围盒的 margin 要用它们 ✓**。 */
  function p2BrushSize(data) {
    return p2Num(data && data.size) ? Math.max(data.size, 0.05) : 4.0;
  }
  function p2BrushJitter(data) {
    return p2Num(data && data.jitter) ? Math.max(data.jitter, 0) : 0;
  }
  /** **笔迹点列 ✓**（**照 `StrokeGeometry::from_value` ✓**：`[[x,y,…]]` 与 `[{x,y}]` 都认 ✓，
   *  **任一点不合法 ⇒ 整个图元 Unsupported ✓**（不是跳过那一点 ✗））。 */
  function p2StrokePoints(data) {
    const array = data && data.points;
    if (!Array.isArray(array)) return null;
    const points = [];
    for (const item of array) {
      if (Array.isArray(item) && item.length >= 2) {
        if (!p2Num(item[0]) || !p2Num(item[1])) return null;
        points.push([item[0], item[1]]);
        continue;
      }
      if (!item || typeof item !== "object" || Array.isArray(item)) return null;
      if (!p2Num(item.x) || !p2Num(item.y)) return null;
      points.push([item.x, item.y]);
    }
    return points.length ? points : null;
  }
  /** **点列的「只认数组」解析 ✓**（**照 `parse_object` 的 `Retouch`／`Liquify` 分支 ✓**：
   *  它们**只**收 `[x, y]` ✓、**跳过**坏点 ✓（不是整条 Unsupported ✗）—— **∴ 与笔迹的
   *  `StrokeGeometry::from_value` 口径不同 ✓**（那里任一点坏 ⇒ 整条 Unsupported ✓），
   *  **所以不能共用一个函数 ✓**）。空点列 ⇒ `null` ✓（分支落 Unsupported ✓）。 */
  function p2PairPoints(data) {
    const array = data && data.points;
    if (!Array.isArray(array)) return null;
    const points = [];
    for (const item of array) {
      if (!Array.isArray(item) || item.length < 2) continue;
      if (!p2Num(item[0]) || !p2Num(item[1])) continue;
      points.push([item[0], item[1]]);
    }
    return points.length ? points : null;
  }
  /** **路径铺平 ✓**（**照 `flatten_path` ✓**：零柄段**只发起点** ✓、非零柄段固定 16 段 ✓）。 */
  function p2FlattenPath(nodes, closed) {
    const parsed = [];
    for (const node of nodes) {
      if (!node || typeof node !== "object" || Array.isArray(node)) continue;
      if (!p2Num(node.x) || !p2Num(node.y)) continue;
      const handle = (key, index) => {
        const pair = node[key];
        return Array.isArray(pair) && p2Num(pair[index]) ? pair[index] : 0;
      };
      parsed.push([node.x, node.y, handle("in", 0), handle("in", 1), handle("out", 0), handle("out", 1)]);
    }
    if (parsed.length < 2) return parsed.map(([x, y]) => [x, y]);
    const segments = closed ? parsed.length : parsed.length - 1;
    const out = [];
    for (let index = 0; index < segments; index++) {
      const [ax, ay, , , aox, aoy] = parsed[index];
      const [bx, by, bix, biy] = parsed[(index + 1) % parsed.length];
      if (aox === 0 && aoy === 0 && bix === 0 && biy === 0) { out.push([ax, ay]); continue; }
      const c1x = ax + aox, c1y = ay + aoy, c2x = bx + bix, c2y = by + biy;
      for (let step = 0; step < 16; step++) {
        const t = step / 16, mt = 1 - t, t2 = t * t, t3 = t * t * t;
        const mt2 = mt * mt, mt3 = mt * mt * mt;
        out.push([
          mt3 * ax + 3 * mt2 * t * c1x + 3 * mt * t2 * c2x + t3 * bx,
          mt3 * ay + 3 * mt2 * t * c1y + 3 * mt * t2 * c2y + t3 * by,
        ]);
      }
    }
    if (closed) out.push([parsed[0][0], parsed[0][1]]);
    else out.push([parsed[parsed.length - 1][0], parsed[parsed.length - 1][1]]);
    return out;
  }
  /** **`kind=path` 的 subpaths 校验 ✓**（**照 `parse_path_subpaths` 的接受集合 ✓**；
   *  只关心「合法与否」✓ —— 包围盒只用**控制点** ✓）。返回**控制点列**或 `null`（非法 ✓）。 */
  function p2PathControlPoints(geometry) {
    const fillRule = geometry.fill_rule;
    if (fillRule !== undefined && fillRule !== null &&
        !(typeof fillRule === "string" && (fillRule === "evenodd" || fillRule === "nonzero"))) {
      return null;
    }
    const subpaths = geometry.subpaths;
    if (!Array.isArray(subpaths) || subpaths.length === 0) return null;
    const point = (value) => {
      if (!Array.isArray(value) || value.length !== 2) return null;
      if (!p2Num(value[0]) || !p2Num(value[1])) return null;
      return [value[0], value[1]];
    };
    const controls = [];
    for (const sub of subpaths) {
      if (!sub || typeof sub !== "object" || Array.isArray(sub)) return null;
      const start = point(sub.start);
      if (!start) return null;
      controls.push(start);
      if (sub.closed !== undefined && sub.closed !== null && typeof sub.closed !== "boolean") return null;
      if (!Array.isArray(sub.segments) || sub.segments.length === 0) return null;
      for (const seg of sub.segments) {
        if (!seg || typeof seg !== "object" || Array.isArray(seg)) return null;
        if (seg.op === "L") {
          const to = point(seg.to);
          if (!to) return null;
          controls.push(to);
        } else if (seg.op === "C") {
          const c1 = point(seg.c1), c2 = point(seg.c2), to = point(seg.to);
          if (!c1 || !c2 || !to) return null;
          controls.push(c1, c2, to);
        } else {
          return null;
        }
      }
    }
    return controls;
  }
  /** **blob 哈希合法性 ✓**（**照 `BlobHash::from_str` ✓**：认 `sha256:`／URL 形态 ✓，
   *  **取最后一个 `sha256:` 之后、截到 71 字符、64 位十六进制** ✓）。 */
  function p2BlobHashOk(value) {
    if (typeof value !== "string") return false;
    const index = value.lastIndexOf("sha256:");
    let candidate = index >= 0 ? "sha256:" + value.slice(index + 7) : value;
    candidate = candidate.split(/[?#&/]/)[0].slice(0, 71);
    if (candidate.length !== 71 || !candidate.startsWith("sha256:")) return false;
    return /^[0-9a-fA-F]{64}$/.test(candidate.slice(7));
  }
  /** **纹理净荷合法性 ✓**（**照 `parse_shape_texture` 的**全部**拒绝条件 ✓**）：
   *  **不合法 ⇒ 服务端 `Unsupported` ⇒ 没有包围盒 ✓**（**∴ 这里也要同样判 ✓** ——
   *  **任何一条少判 ✗ ⇒ 本地给出缺省 anchor 而服务端 409 ✓**）。
   *  逐项照抄：`type`／`blob_hash`／`source` 非空／`width`·`height` 为正整数 ✓，
   *  `jitter` 在 `0..=1` ✓，`mode` 只认 tile/patch ✓，`patch` 在 `4..=256` ✓、
   *  `overlap < patch` ✓、`candidates` 在 `1..=4096` ✓、`seam` 只认 linear/min_error ✓、
   *  `brightness_ramp` 存在则必须是对象、`axis ∈ x/y`、两端亮度在 `0..=255` ✓。 */
  function p2TextureUsable(texture) {
    if (!texture || typeof texture !== "object" || Array.isArray(texture)) return false;
    if (texture.type !== "bitmap") return false;
    if (!p2BlobHashOk(texture.blob_hash)) return false;
    const source = p2BboxFrom(texture.source);
    if (!source || source.w <= 0 || source.h <= 0) return false;
    const positiveInt = (value) => Number.isInteger(value) && value > 0;
    if (!positiveInt(texture.width) || !positiveInt(texture.height)) return false;
    if (texture.jitter !== undefined) {
      if (!p2Num(texture.jitter) || texture.jitter < 0 || texture.jitter > 1) return false;
    }
    if (texture.mode !== undefined && texture.mode !== null &&
        texture.mode !== "tile" && texture.mode !== "patch") return false;
    const u = (value, fallback) => (Number.isInteger(value) && value >= 0 ? value : fallback);
    const patch = u(texture.patch, 24);
    const overlap = u(texture.overlap, 6);
    const candidates = u(texture.candidates, 200);
    if (patch < 4 || patch > 256) return false;
    if (overlap >= patch) return false;
    if (candidates < 1 || candidates > 4096) return false;
    if (texture.seam !== undefined && texture.seam !== null &&
        texture.seam !== "linear" && texture.seam !== "min_error") return false;
    const ramp = texture.brightness_ramp;
    if (ramp !== undefined && ramp !== null) {
      if (typeof ramp !== "object" || Array.isArray(ramp)) return false;
      if (ramp.axis !== undefined && ramp.axis !== "x" && ramp.axis !== "y") return false;
      for (const key of ["at_start", "at_end"]) {
        if (ramp[key] === undefined) continue;
        if (!p2Num(ramp[key]) || ramp[key] < 0 || ramp[key] > 255) return false;
      }
    }
    return true;
  }
  /** **图元解析 ✓**（**照 `parse_object` 的分支 ✓**）⇒ 一个**可算包围盒**的中间表示 ✓，
   *  解析不出（服务端 `Unsupported` ✓）⇒ `null` ✓。 */
  function p2ParsePrimitive(type, data) {
    const d = (data && typeof data === "object" && !Array.isArray(data)) ? data : {};
    if (type === "stroke") {
      const points = p2StrokePoints(d);
      if (!points) return null;
      return { t: "stroke", points, size: p2BrushSize(d), jitter: p2BrushJitter(d) };
    }
    if (type === "path") {
      if (!Array.isArray(d.nodes)) return null;
      const points = p2FlattenPath(d.nodes, d.closed === true);
      if (points.length < 2) return null;
      return { t: "stroke", points, size: p2BrushSize(d), jitter: p2BrushJitter(d) };
    }
    if (type === "shape") {
      const geometry = (d.geometry && typeof d.geometry === "object" && !Array.isArray(d.geometry))
        ? d.geometry : null;
      const g = (key) => (geometry ? geometry[key] : undefined);
      let kind = "rect";
      let pathCtrl = null;
      if (g("kind") === "ellipse") kind = "ellipse";
      else if (g("kind") === "polygon") kind = "polygon";
      else if (g("kind") === "path") {
        pathCtrl = p2PathControlPoints(geometry);
        if (!pathCtrl) return null;
        kind = "path";
      }
      let bbox = p2BboxFrom(g("bbox"));
      if (!bbox) {
        const flat = [g("x"), g("y"), g("w"), g("h")];
        if (flat.every(p2Num)) bbox = { x: flat[0], y: flat[1], w: flat[2], h: flat[3] };
      }
      if (!bbox) bbox = p2BboxFrom(d.bbox);
      if (!bbox) bbox = { x: 0, y: 0, w: 0, h: 0 };
      if (kind === "path" && bbox.w <= 0 && bbox.h <= 0 && pathCtrl.length) {
        const mm = p2PointsMinMax(pathCtrl);
        if (mm) bbox = { x: mm[0], y: mm[1], w: mm[2] - mm[0], h: mm[3] - mm[1] };
      }
      const points = [];
      if (Array.isArray(g("points"))) {
        for (const item of g("points")) {
          if (Array.isArray(item) && item.length >= 2 && p2Num(item[0]) && p2Num(item[1])) {
            points.push([item[0], item[1]]);
          }
        }
      }
      if (d.texture !== undefined && d.texture !== null && !p2TextureUsable(d.texture)) return null;
      const strokeWidth = p2Num(d.stroke_width) ? Math.max(d.stroke_width, 0) : 0;
      return { t: "shape", kind, bbox, points, pathCtrl, strokeWidth };
    }
    if (type === "text") {
      // **照 Rust 的 `data.get("position").map(...)` ✓**：**只要 `position` 这个键存在 ✓**
      //   （**含 `null` ✓**）就**不再看 `bbox`** ✓ —— 数组认前两元 ✓、其余（对象/null/标量）
      //   取 `x`／`y`、读不出按 0 ✓（**∴ 标量不是「没给」⇒ 不能掉到 bbox 分支 ✓**）。
      const position = (() => {
        if (d.position !== undefined) {
          if (Array.isArray(d.position)) {
            return [p2Num(d.position[0]) ? d.position[0] : 0, p2Num(d.position[1]) ? d.position[1] : 0];
          }
          if (d.position !== null && typeof d.position === "object") {
            return [p2Num(d.position.x) ? d.position.x : 0, p2Num(d.position.y) ? d.position.y : 0];
          }
          return [0, 0];
        }
        const bbox = p2BboxFrom(d.bbox);
        if (bbox) return [bbox.x, bbox.y];
        return [0, 0];
      })();
      return {
        t: "text",
        text: typeof d.text === "string" ? d.text : "",
        size: p2Num(d.size) ? Math.max(d.size, 1) : 24,
        position,
      };
    }
    if (type === "retouch") {
      const points = p2PairPoints(d);
      if (!points) return null;
      return {
        t: "retouch", points,
        size: p2Num(d.size) ? d.size : 24,
        jitter: p2Num(d.jitter) ? d.jitter : 0,
      };
    }
    if (type === "liquify") {
      const points = p2PairPoints(d);
      if (!points) return null;
      // **照 Rust 的 `get("liquify_type").or_else(get("mode")).and_then(as_str).unwrap_or("push")` ✓**：
      //   **`liquify_type` 只要**存在**就**不再看 `mode` ✓**（哪怕它不是字符串 ⇒ 落缺省 `push` ✓）。
      const rawMode = d.liquify_type !== undefined ? d.liquify_type : d.mode;
      const mode = typeof rawMode === "string" ? rawMode : "push";
      if (mode !== "push" && mode !== "twirl" && mode !== "pinch") return null;
      const direction = Array.isArray(d.direction) ? d.direction : [];
      const dx = p2Num(direction[0]) ? direction[0] : 0;
      const dy = p2Num(direction[1]) ? direction[1] : 0;
      if (mode === "push" && dx === 0 && dy === 0) return null;
      return {
        t: "liquify", points,
        size: p2Num(d.size) ? d.size : 80,
        strength: p2Num(d.strength) ? d.strength : 0.5,
      };
    }
    if (type === "raster_patch") {
      const bitmap = (d.bitmap && typeof d.bitmap === "object" && !Array.isArray(d.bitmap)) ? d.bitmap : null;
      if (!bitmap || !p2BlobHashOk(bitmap.blob_hash)) return null;
      // **`raw_develop` 给了但**不是对象** ⇒ 服务端 Unsupported ✓**（**畸形落账不许静默当没有 ✓**）。
      if (d.raw_develop !== undefined && d.raw_develop !== null &&
          (typeof d.raw_develop !== "object" || Array.isArray(d.raw_develop))) return null;
      const region = p2BboxFrom(d.region);
      const asU32 = (value) => (Number.isInteger(value) && value >= 0 ? value : null);
      const width = asU32(d.width) ?? (region ? Math.max(Math.trunc(region.w), 0) : 0);
      const height = asU32(d.height) ?? (region ? Math.max(Math.trunc(region.h), 0) : 0);
      if (!width || !height) return null;
      return { t: "raster", width, height, offset: region ? [region.x, region.y] : [0, 0] };
    }
    if (type === "adjustment" || type === "filter") {
      const key = type === "adjustment" ? "adjustment_type" : "filter_name";
      const name = typeof d[key] === "string" ? d[key] : "";
      return name ? { t: "adjust" } : null;
    }
    return null;
  }
  /** **点列包围盒 ✓** ⇒ `[minX, minY, maxX, maxY]` 或 `null` ✓。 */
  function p2PointsMinMax(points) {
    if (!points.length) return null;
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    for (const [x, y] of points) {
      minX = Math.min(minX, x); minY = Math.min(minY, y);
      maxX = Math.max(maxX, x); maxY = Math.max(maxY, y);
    }
    if (!Number.isFinite(minX)) return null;
    return [minX, minY, maxX, maxY];
  }
  /** **施加对象变换 ✓**（**照 `transform_primitive` ✓**：笔迹／修图映射点 ✓、
   *  形状映射 bbox 两角＋点＋路径控制点 ✓、文本只映射位置 ✓、位图只映射偏移 ✓、
   *  **液化**不施加 ✓（内核如此 ✓））。 */
  function p2TransformPrimitive(primitive, transform) {
    if (!primitive || p2IsIdentityMatrix(transform)) return primitive;
    const map = (x, y) => p2ApplyPoint(transform, x, y);
    if (primitive.t === "stroke" || primitive.t === "retouch") {
      primitive.points = primitive.points.map(([x, y]) => map(x, y));
    } else if (primitive.t === "shape") {
      const b = primitive.bbox;
      const [x0, y0] = map(b.x, b.y);
      const [x1, y1] = map(b.x + b.w, b.y + b.h);
      let w = Math.max(Math.abs(x1 - x0), Math.abs(Math.min(b.w, 0)));
      let h = Math.max(Math.abs(y1 - y0), Math.abs(Math.min(b.h, 0)));
      if (w === 0) w = Math.abs(x1 - x0);
      if (h === 0) h = Math.abs(y1 - y0);
      primitive.bbox = { x: Math.min(x0, x1), y: Math.min(y0, y1), w, h };
      primitive.points = primitive.points.map(([x, y]) => map(x, y));
      if (primitive.pathCtrl) primitive.pathCtrl = primitive.pathCtrl.map(([x, y]) => map(x, y));
    } else if (primitive.t === "text") {
      primitive.position = map(primitive.position[0], primitive.position[1]);
    } else if (primitive.t === "raster") {
      primitive.offset = map(primitive.offset[0], primitive.offset[1]);
    }
    return primitive;
  }
  /** **文本度量 ✓**（**照 `font::measure_sized` ✓**：ASCII 走 5×7 ✓、含 CJK 走 16×16 ✓，
   *  **两处的 `scale` 分母不同（7 vs 16）✗ ⇒ 不能共用 ✓**）。 */
  function p2MeasureSized(text, size) {
    const chars = [...text];
    const isControl = (code) => code < 0x20 || (code >= 0x7f && code <= 0x9f);
    const usesAtlas = chars.some((ch) => {
      const code = ch.codePointAt(0);
      return !isControl(code) && !(code >= 0x20 && code <= 0x7e);
    });
    if (!usesAtlas) {
      const scale = Math.min(Math.max(Math.round(Math.max(size, 1) / 7), 1), 64);
      let lines = 0, widest = 0;
      for (const line of text.split("\n")) {
        lines += 1;
        const count = [...line].length;
        if (count > 0) widest = Math.max(widest, count * 5 * scale);
      }
      const ink = 7 * scale;
      return [widest, (lines - 1) * (ink + scale) + ink];
    }
    const scale = Math.min(Math.max(Math.round(Math.max(size, 1) / 16), 1), 64);
    const cell = 16 * scale;
    const split = text.split("\n");
    let widest = 0;
    for (const line of split) widest = Math.max(widest, [...line].length * cell);
    return [widest, cell * split.length + scale * (split.length - 1)];
  }
  /** **图元包围盒 ✓**（**照 `object_bbox` 的分支 ✓**）。 */
  function p2PrimitiveBBox(primitive) {
    if (!primitive) return null;
    if (primitive.t === "text") {
      const [width, height] = p2MeasureSized(primitive.text, primitive.size);
      return { x: primitive.position[0], y: primitive.position[1], w: width, h: height };
    }
    if (primitive.t === "liquify") {
      const mm = p2PointsMinMax(primitive.points);
      if (!mm) return null;
      const reach = primitive.size / 2 + Math.min(Math.max(Math.abs(primitive.strength), 0), 2) * primitive.size;
      return { x: mm[0] - reach, y: mm[1] - reach,
               w: (mm[2] - mm[0]) + reach * 2, h: (mm[3] - mm[1]) + reach * 2 };
    }
    if (primitive.t === "retouch" || primitive.t === "stroke") {
      const mm = p2PointsMinMax(primitive.points);
      if (!mm) return null;
      const margin = primitive.size / 2 + primitive.jitter;
      return { x: mm[0] - margin, y: mm[1] - margin,
               w: (mm[2] - mm[0]) + margin * 2, h: (mm[3] - mm[1]) + margin * 2 };
    }
    if (primitive.t === "shape") {
      let base = primitive.bbox;
      if (primitive.kind === "path" && primitive.pathCtrl && primitive.pathCtrl.length) {
        const mm = p2PointsMinMax(primitive.pathCtrl);
        if (mm) base = { x: mm[0], y: mm[1], w: mm[2] - mm[0], h: mm[3] - mm[1] };
      } else if (primitive.kind === "polygon" && primitive.points.length) {
        const mm = p2PointsMinMax(primitive.points);
        if (mm) base = { x: mm[0], y: mm[1], w: mm[2] - mm[0], h: mm[3] - mm[1] };
      }
      const margin = primitive.strokeWidth / 2;
      return { x: base.x - margin, y: base.y - margin,
               w: base.w + margin * 2, h: base.h + margin * 2 };
    }
    if (primitive.t === "raster") {
      return { x: primitive.offset[0], y: primitive.offset[1], w: primitive.width, h: primitive.height };
    }
    if (primitive.t === "adjust") return { x: 0, y: 0, w: 0, h: 0 };
    return null;
  }
  /** **实例解析 ✓**（**照 `resolve_instance` ✓**）⇒ `{data, transform}` 或 `null` ✓。
   *  **∴ 顺序照抄 ✗**：实例自身 ✓ → `override.transform`（最外 ✓）→ 链上每层的
   *  `local_transform` ✓ → master 自身 ✓；`sync_policy:"none"` 用**快照** ✓。 */
  function p2ResolveInstance(rows, view, layers, neutral) {
    let composed = view.transform;
    const data = p2EffectiveData(view);
    const override = data.override;
    if (override && typeof override === "object" && override.transform !== undefined) {
      composed = p2ComposeTransforms(p2TransformFromJson(override.transform), composed);
    }
    if (data.sync_policy === "none") {
      if (data.snapshot === undefined || data.snapshot === null) return null;
      const snapshotTransform = data.snapshot_transform !== undefined
        ? p2TransformFromJson(data.snapshot_transform) : p2TransformFromJson(null);
      return { data: data.snapshot, transform: p2ComposeTransforms(composed, snapshotTransform) };
    }
    let cursor = view;
    for (let depth = 0; depth < 64; depth++) {
      const cursorData = p2EffectiveData(cursor);
      const masterRef = cursorData.master_ref;
      const masterId = masterRef && typeof masterRef === "object" &&
        typeof masterRef.object_id === "string" ? masterRef.object_id : null;
      if (!masterId) return null;
      if (masterRef.local_transform !== undefined) {
        composed = p2ComposeTransforms(p2TransformFromJson(masterRef.local_transform), composed);
      }
      const master = p2ObjectView(rows, masterId);
      if (!master.found || neutral.has(master.__create_id)) return null;
      if (p2ObjectDeath(layers, master).deleted) return null;
      if (master.type === "instance") {
        composed = p2ComposeTransforms(master.transform, composed);
        cursor = master;
        continue;
      }
      return { data: p2EffectiveData(master), transform: p2ComposeTransforms(master.transform, composed) };
    }
    return null;
  }
  /** **实例最终解析到的 master 对象 ✓**（**照 `resolved_master` ✓**：跳过中间实例 ✓，
   *  **`is_deleted` ⇒ None ✓**）。**∴ 与 `p2ResolveInstance` 分开 ✓** —— 服务端就是两个函数 ✓，
   *  且 `object_bbox_in` **只用这一个**取 `data` ✓（`resolve_instance` 返回的图元被丢掉 ✓）。 */
  function p2ResolvedMaster(rows, view, layers, neutral) {
    let cursor = view;
    for (let depth = 0; depth < 64; depth++) {
      const data = p2EffectiveData(cursor);
      const ref = data.master_ref;
      const masterId = ref && typeof ref === "object" &&
        typeof ref.object_id === "string" ? ref.object_id : null;
      if (!masterId) return null;
      const master = p2ObjectView(rows, masterId);
      if (!master.found || neutral.has(master.__create_id)) return null;
      if (p2ObjectDeath(layers, master).deleted) return null;
      if (master.type === "instance") { cursor = master; continue; }
      return master;
    }
    return null;
  }
  /** **对象包围盒 ✓**（**照 `object_bbox_in` ✓**：实例**先解析成 master** ✓）。
   *  **∴ 两条路径照抄 ✓**：变换取 `resolve_instance` 的合成结果 ✓；`data` 取 **master 的折叠
   *  `data`** ✓（**取不到才退回实例自己的 `data`** ✓ —— 服务端 `synthetic.data` 就是这个分支 ✓）。
   *  **∴ 于是 master 的 `data` 被当**形状**解析 ✓**（服务端构造的合成对象就是 `Shape` ✓）——
   *  笔触／文本 master 的包围盒也走形状分支 ✓；这是**权威口径**，不是笔误 ✓。 */
  function p2ObjectBBox(rows, view, layers, neutral) {
    if (view.type !== "instance") {
      const primitive = p2ParsePrimitive(view.type, p2EffectiveData(view));
      return p2PrimitiveBBox(p2TransformPrimitive(primitive, view.transform));
    }
    const resolved = p2ResolveInstance(rows, view, layers, neutral);
    if (!resolved) return null;
    const master = p2ResolvedMaster(rows, view, layers, neutral);
    const primitive = p2ParsePrimitive("shape", master ? p2EffectiveData(master) : p2EffectiveData(view));
    return p2PrimitiveBBox(p2TransformPrimitive(primitive, resolved.transform));
  }
  /** **`transform_object` 的参数面 ✓**（**照 `tools_registry` 的 spec ＋ `validate_args` ✓**）：
   *  未知参数**响亮** ✓（旧的本地实现收 `transform`／`dx`／`dy` ✗ ⇒ 与服务端是**两套契约** ✗）。 */
  const P2_TRANSFORM_PARAMS = [
    ["object_id", "string", true],
    ["rotate", "object", false],
    ["scale", "object", false],
    ["translate", "object", false],
    ["anchor", "object", false],
    ["compose", "boolean", false],
  ];
  const P2_FRAMEWORK_PARAMS = ["doc_id", "actor", "session"];
  /** **对象类型名的服务端写法 ✓**（**照 `format!("{object_type:?}").to_lowercase()` ✓**）：
   *  **∴ 只有 `raster_patch` 与本地写法不同（本地带下划线 ✗）** ✓ —— 其余一一对应 ✓。 */
  const P2_SERVER_TYPE_NAME = {
    stroke: "stroke", shape: "shape", text: "text", adjustment: "adjustment",
    filter: "filter", raster_patch: "rasterpatch", retouch: "retouch",
    liquify: "liquify", instance: "instance", group: "group", path: "path",
  };
  function p2TransformArgError(body) {
    const object = (body && typeof body === "object" && !Array.isArray(body)) ? body : {};
    for (const key of Object.keys(object)) {
      if (P2_FRAMEWORK_PARAMS.includes(key)) continue;
      if (!P2_TRANSFORM_PARAMS.some(([name]) => name === key)) {
        return "transform_object 不接受参数 " + key + "（拼写错误？）；可用参数：" +
          P2_TRANSFORM_PARAMS.map(([name]) => name).join(", ") +
          "；框架级参数：" + P2_FRAMEWORK_PARAMS.join(", ");
      }
    }
    for (const [name, kind, required] of P2_TRANSFORM_PARAMS) {
      const value = object[name];
      if (value === undefined) {
        if (required) return "缺少必填参数 " + name;
        continue;
      }
      if (value === null && !required) continue;
      const matches = kind === "string" ? typeof value === "string"
        : kind === "object" ? (value !== null && typeof value === "object" && !Array.isArray(value))
        : kind === "boolean" ? typeof value === "boolean"
        : true;
      if (!matches) return "参数 " + name + " 类型应为 " + kind + "，实际为 " + p2JsonTypeName(value);
    }
    return null;
  }
  /** **从日志推导对象视图 ✗**（**∴ 存在性／类型／属性覆盖／删除状态／当前变换 ✓**）：
   *   **∴ `type`／`layer_id` 等**可被 `set_property` 覆盖 ✗**（**∴ 取最后一次 ✓**）；
   *   **∴ `data` 可被 `supersede` 整体替换 ✗**（**`replace_object_data` 的语义 ✓**）；
   *   **∴ `members`／`group_transform`／`master_ref` 先看创建 `data` ✗**，
   *     **再看 `set_property` 覆盖 ✗**（**组与实例的语义 ✓**）。
   */
  function p2ObjectView(rows, objectId) {
    rows = p2StateWindow(rows); // 跳变窗口内折叠 ✓（`create`／属性／墓碑都只看状态里的原子 ✓）
    let create = null;
    let createSeq = null;
    const props = {};
    let supersededData = null;
    const tombstoneIds = [];
    // **`reorder_objects` 的最新生效位置 ✗**（**照 `fold.rs:1305–1313` 逐字 ✓**：
    //   `order[index] ⇒ z_index = index`；**被 revert 抑制的那条跳过 ✓** ⇒ 撤一次重排 = 回到旧 z ✓）。
    let reorderZ = null;
    /** **生效的 `move`／`transform` ✗**（**照 `fold.rs` 的 `Move | Transform` ✓**）：
     *   **∴ 带行号 ✗** —— **折叠是顺序的 ✓ ⇒ **落在创建之前的那条**折叠层根本看不到对象 ✓
     *   （**∴ 非法日志；本地也跳过 ✓ 与折叠同序 ✓**）。 */
    const moveOps = [];
    let createAt = -1;
    const neutralEarly = p2Neutralized(rows);
    for (let ri = 0; ri < rows.length; ri++) {
      const r = rows[ri];
      const a = (r && r.atom) || {};
      const pl = a.payload || {};
      // **建对象的原子 = 共享集合 ✓**（**不再各自发明 ✗**，见 `P2_OBJECT_CREATE_KINDS`）：
      //   **∴ 旧 bug ✗**：此前**只认 `create_object`**（后补 `import_image`✗）⇒
      //     `draw_shape`／`fill`／`draw_text` 建的图形**同样看不见**✓ ⇒ `get_object`／`restore_object`／`list_objects`
      //     对**手画的形／字／色块**也报「对象不存在」✓（与导入图是**同一类**✗ 只是没被 CI 指到 ✓）。
      //   **∴ 取「第一条」建它的原子 ✓**（**与服务端「没见过才建、见过就 supersede」等价 ✓**）。
      if (P2_OBJECT_CREATE_KINDS.has(a.kind) && pl.object_id === objectId && !create) {
        create = a;
        createSeq = typeof r.seq === "number" ? r.seq : null;
        createAt = ri;
      }
      else if (a.kind === "set_property" && pl.object_id === objectId
               && !neutralEarly.has(a.id)) {
        props[pl.key] = pl.value;
        // **`set_property{key:"transform"}` 也是变换来源 ✗**（**照 `apply_property_to_object` ✓**：
        // 它把值当**绝对**变换写进 `object.transform` ✓）⇒ **∴ 与 `move` 同序重放 ✓**；
        // 否则 `get_object.transform` 与包围盒都停在旧值（形态③：回执 ok、读回旧 ✓）。
        if (pl.key === "transform" && !neutralEarly.has(a.id)) {
          moveOps.push({ pl: { transform: pl.value }, at: ri });
        }
      }
      // **被撤销的 `supersede` 不算 ✗**（**照服务端折叠 ✓：失效集里的原子整条跳过 ✓** ——
      //   旧本地实现无条件取最后一条 ⇒ **撤销编辑后 `get_path` 仍读到编辑后的数据** ✗，
      //   而 `get_object`／`list_objects` 也一起错 ⇒ 与服务端**写读两条口径同时分裂** ✓）。
      else if (a.kind === "supersede" && pl.object_id === objectId
               && !neutralEarly.has(a.id)) supersededData = pl.data;
      else if (a.kind === "tombstone" && pl.object_id === objectId) tombstoneIds.push(a.id);
      else if ((a.kind === "move" || a.kind === "transform") && pl.object_id === objectId) {
        if (!neutralEarly.has(a.id)) moveOps.push({ pl, at: ri });
      }
      else if (a.kind === "reorder_objects" && Array.isArray(pl.order) && pl.order.indexOf(objectId) >= 0) {
        if (!neutralEarly.has(a.id)) reorderZ = pl.order.indexOf(objectId);
      }
    }
    if (!create) return { found: false };
    const neutral = neutralEarly; // **同一份失效集只算一次 ✓**（一个事实一处来源 ✓）
    const cpl = create.payload || {};
    // **当前变换 = 创建净荷 ＋ 逐条生效的 `move`／`transform` ✗**（**照 `fold.rs:1154` ✓**）：
    //   **`delta`（增移）⇒ 在**当前**矩阵的平移分量上叠加 ✓**；
    //   **`transform`（绝对）⇒ **直接赋值** ✓**（**连单位矩阵也赋值 ✓** ——
    //   **∴ 旧缺陷「单位变换被丢掉 ⇒ 对象回不到原点」已由服务端修 ✓，本地照抄 ✓**）。
    let transform = p2TransformFromPayload(cpl);
    for (const op of moveOps) {
      if (op.at < createAt) continue;
      const pl = op.pl;
      if (pl.delta && typeof pl.delta === "object") {
        const dx = Number(pl.delta.dx), dy = Number(pl.delta.dy);
        transform.matrix[4] += Number.isFinite(dx) ? dx : 0;
        transform.matrix[5] += Number.isFinite(dy) ? dy : 0;
      } else if (pl.transform !== undefined) {
        transform = p2TransformFromPayload(pl);
      }
    }
    const view = {
      found: true,
      object_id: objectId,
      /** **建它的原子 id ✗**（**`__p2Alive` 判「创建是否被撤掉」用 ✓**——
       *   **∴ 折叠器里 `Revert{create}` ⇒ 对象整个从状态消失 ✓，与 `deleted` 是两回事 ✗**）。 */
      __create_id: create.id,
      /** **建它的原子 seq ✗**（**服务端 `objects_in_layer` 的第二排序键是「加入顺序」✓ `state.rs`
       *   2026-10-15 的教训 ⇒ 本地用原子 seq 当同一事实 ✓**）。 */
      __create_seq: createSeq,
      layer_id: props.layer_id !== undefined ? props.layer_id : (cpl.layer_id ?? null),
      type: props.type !== undefined ? props.type : (cpl.type ?? null),
      // **`data` 的取法照服务端 `object_data` ✓**（**「有 `data` 子对象 ⇒ 用 `data`✓，否则 ⇒ 用整个净荷 ✓」**，
      //   `fold.rs:1553` 逐字 ✓）：**`import_image` 把 `bitmap`/`width`/`height`/`region` 写在净荷顶层 ✗**
      //   （**无 `data` 子对象 ✓**）⇒ 只取 `cpl.data` ⇒ `data` 恒 `null`✓ ⇒ 认出对象后 `resample` 仍报「没有位图」✓
      //   （**修一半 ✗**）；而 `create_object`/`draw_shape`/`fill` **有** `data` ✓ ⇒ 取 `data` ✓。
      //   **∴ 一个 `p2ObjectData` 同时兜住两类 ✗**（**不各写各的 ✓**）。
      data: supersededData !== null && supersededData !== undefined
              ? supersededData
              : p2ObjectData(cpl),
      transform,
      // **原始属性覆盖表 ✗**（**内部字段 ✓**：`p2EffectiveData` 用它拼出「服务端折叠后的 `data`」✓）。
      __props: props,
      visible: props.visible ?? null,
      // **`enabled` 照 `fold.rs` 的缺省 ✓**（创建净荷不吃时是 **true** ✓，与 `visible` 的 `null` 语义不同 ✗）
      //   ⇒ `update_object{enabled:false}` 必须能在 `list_objects` 里看见 ✓（否则又是形态③）。
      enabled: props.enabled !== undefined ? props.enabled
        : (cpl.enabled !== undefined ? cpl.enabled : true),
      locked: props.locked ?? null,
      // **z 的口径 ✗**（**照服务端折叠的应用顺序 ✓**：`reorder_objects` 给层内每个对象**重新赋值** ✓
      //   （`fold.rs:1311 object.z_index = index`），而 `set_property z_index` 是**单点补丁** ✓
      //   （`:1907`）⇒ 本地取「`set_property` 优先、其次最新生效重排的位置、再次创建净荷、缺省 0 ✓」
      //   —— **与「重排后 `list_objects` 顺序必须变」这条可观测事实一致 ✓**）。
      z_index: props.z_index !== undefined ? props.z_index
        : (reorderZ !== null ? reorderZ : (cpl.z_index ?? null)),
      metadata: props.metadata ?? null,
      tombstone_ids: tombstoneIds,
      deleted: tombstoneIds.some((id) => !neutral.has(id)),
      /** **死因 ✗**（**∴ 照服务端折叠的 `Object::deleted_by` ✓**——**每条生效的 `tombstone` 都覆写 ✓
       *   ⇒ 取**最新一条未被抑制的** ✓；`tools.rs:3188` 把它原样给 `get_object` ✓**）。
       *   **∴ 为什么本地也要有 ✗**：undo 机器只看「死没死」⇒「创建被抑制」与「tombstone 生效」
       *   **两种错法计数相同 ✓ 分不出来 ✗**（M-G 变异实测）⇒ **死因字段把两者分开 ✓**。 */
      __deleted_by: (() => {
        const live = tombstoneIds.filter((id) => !neutral.has(id));
        return live.length ? live[live.length - 1] : null;
      })(),
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
  /** **一笔**＝ `(object_id, [ {seq, atom_id, kind} ])`（**照服务端 `type Gesture` ✓，`tools.rs:8685`**）。 */
  /** **对象在状态里是否活着 ✗**（**本地等价 `alive_objects()` ✓ —— 状态事实 = 「折叠后还在状态里」✓，
   *   **不是**「没被 tombstone 删过」✗**）：折叠器对 `Revert{create 原子}` 会**连对象一起消失** ✓
   *   （**实测：服务端撤掉 `draw_shape` 后 `undo_last` 报「没有可撤销的笔迹」✓**）⇒
   *   **∴ 三条合取 ✓**：有创建原子 ✓、**创建原子未被最新历史动作中和 ✗**、**没有生效中的 tombstone ✓**。
   *   **∴ 「最新动作」而非「出现过 revert」**✓（**redo 过 ⇒ `reapply` 覆盖更早的 `revert`✓**——
   *     **照折叠层对 `Reapply{Revert{x}} ≡ x 生效 ✓` 的语义（设计 793 ✓）**）。 */
  function __p2Alive(rows, view, layers, neutral) {
    if (view.found !== true || !view.__create_id) return false;
    const n = neutral || p2Neutralized(rows);
    if (n.has(view.__create_id)) return false;
    // **★ 图层级联删也算死 ✗ ★**（**`fold.rs:1988 tombstone_layer` 会给层上每个对象写
    //   `deleted_by` ⇒ `alive_objects()`／`objects_in_layer()` 把它们剔除 ✓**）：
    //   **∴ 旧实现只看对象自己那条 tombstone ✗** ⇒ 删掉的图层里的对象在 `list_objects`／
    //   `reorder_objects`／undo 候选里**仍然算活着** ✓（**服务端列表里没有它 ✓ ⇒ 面板与撤销条数两端不一致** ✗）。
    //   **∴ 用折叠口径 `p2ObjectDeath` ✗**（自己死＋随层死 ✓，一个谓词 ✓ 不各算一遍 ✓）。
    const ls = layers || p2PresetLayers(rows, n);
    return !p2ObjectDeath(ls, view).deleted;
  }
  /** **对象列表摘要 ✗**（**`list_objects` 与 `get_state{include_objects}` 共用一份 ✓**）：
   *   **∴ 字段集照服务端 `summary_object`（`tools.rs:2823`）✓**：11 个里本地能给 8 个 ✓
   *   （`bbox` 要渲染层算包围盒／`current_version`＋`versions` 要版本链 ⇒ 本地**如实不给** ✓，
   *     判据把它们列入 serverOnly ✓）。
   *   **∴ 过滤／排序照 `read_list_objects` ✓**：`alive_objects()` ⇒ `(layer_id, z_index, id)` ✓；
   *   `include_hidden` 缺省 false ⇒ 只给 `visible` ✓（**∴ 隐藏是对象属性 `visible` ✓，
   *   不是「有墓碑」✗** —— 旧 `get_state` 把 `deleted` 当可见性过滤，口径错 ✓）。
   *   **∴ 存活 = 创建未被撤销 ＋ 自己未墓碑 ＋ 未随图层级联删 ✗**（`__p2Alive` ✓）。 */
  function p2ObjectSummaries(rows, opts) {
    rows = p2StateWindow(rows);
    const o = opts || {};
    const layerFilter = typeof o.layerFilter === "string" && o.layerFilter ? o.layerFilter : null;
    const typeFilter = typeof o.typeFilter === "string" && o.typeFilter ? o.typeFilter : null;
    const includeHidden = o.includeHidden === true; // 缺省 false（照 `unwrap_or(false)` ✓）
    const cmpId = (a, b) => (a < b ? -1 : a > b ? 1 : 0);
    const neutral = p2Neutralized(rows);
    const layers = p2PresetLayers(rows, neutral);
    const seen = new Set();
    const objects = [];
    for (const r of rows) {
      const a = (r && r.atom) || {};
      const id = a.payload && a.payload.object_id;
      if (typeof id !== "string" || !id || seen.has(id)) continue; // **按 object_id 去重 ✗**
      seen.add(id);
      const v = p2ObjectView(rows, id);
      if (!__p2Alive(rows, v, layers, neutral)) continue;            // 只列存活对象 ✗
      if (layerFilter !== null && v.layer_id !== layerFilter) continue;
      if (typeFilter !== null && v.type !== typeFilter) continue;
      const hidden = (v.visible === false);                          // `null`＝从未设＝可见 ✓
      if (!includeHidden && hidden) continue;
      objects.push({
        object_id: id,
        layer_id: v.layer_id ?? null,
        type: v.type ?? null,
        visible: !hidden,
        enabled: v.enabled !== false,
        locked: v.locked === true,
        z_index: v.z_index ?? 0,
        medium: v.data && v.data.source && v.data.source.medium ? v.data.source.medium : null,
      });
    }
    objects.sort((a, b) => cmpId(String(a.layer_id), String(b.layer_id))
      || (a.z_index - b.z_index)
      || cmpId(String(a.object_id), String(b.object_id)));
    return objects;
  }
  /** **状态里存在的对象 id ✗**（**本地等价服务端 `state.objects.keys()` ✓**）：
   *   **∴ 判据是「在状态里」✗ 不是「活着」✓** —— 折叠层**保留墓碑** ✓（删除可撤销的基础 ✓）⇒
   *   `check_object_exists` 报错时列的候选**含已删对象** ✓（`log.rs:1173` 不过滤 `is_deleted` ✓
   *   而图层那侧**过滤** ✓ `:1198` ⇒ **两侧不同，不能共用一个谓词 ✓**）。
   *   **∴ 唯一要排除的是「创建被 revert」** ✓（折叠把对象整个拿掉 ⇒ 它不在 `objects` 里 ✓）。
   *   **∴ 排序** ✓：服务端是 `BTreeMap` ⇒ 键**升序** ✓，候选列表文案随之确定 ✓。
   */
  function p2StateObjectIds(rows, neutral) {
    rows = p2StateWindow(rows); // 跳变窗口内 ✓（被跳过的「建对象」不在状态里 ✓）
    const created = new Map();
    for (const r of rows) {
      const a = (r && r.atom) || {};
      const pl = a.payload || {};
      if (!P2_OBJECT_CREATE_KINDS.has(a.kind)) continue;
      if (typeof pl.object_id !== "string" || !pl.object_id) continue;
      if (!created.has(pl.object_id)) created.set(pl.object_id, a.id);
    }
    return [...created.entries()]
      .filter(([, atomId]) => !neutral.has(atomId))
      .map(([objectId]) => objectId)
      .sort();
  }
  /** **图层状态 ✗**（**本地等价服务端折叠的 `state.layers` ✓**）：
   *   **∴ 折叠**保留**被 tombstone 的层 ✓**（`state.rs` 的 `layer_alive` 是**另一个**谓词 ✓
   *   ⇒ `contains_key` ≠ 活着 ✓），删除可撤销正是靠这个「留墓碑」✓。
   *   **∴ 级联必须照 `fold.rs:1988 tombstone_layer` 逐条 ✓**：一条「图层 tombstone」会把
   *   **本层 ＋ 全部子孙层**（按 `parent_id` BFS ✓ 带 `seen` 防环 ✓）以及**这些层上的对象**
   *   一起标成已删除 ✓，而规则是 `deleted_by.is_none()` ⇒ **先到先得 ✓**
   *   （**所以取「本层与祖先链上 log 序最早的那条生效墓碑」**✓ 不是「自己那条」✗）。
   *   **∴ 此前本地只查自己那条 ✗** ⇒ 父层被删而子层没被删时 ⇒ 本地认为子层还活着 ✓
   *   服务端 409 ⇒「回执 ok 而画面没变」的另一种形态 ✓。 */
  function p2PresetLayers(rows, neutral) {
    rows = p2StateWindow(rows); // 跳变窗口内折叠 ✓（层创建／墓碑／父链都只看状态里的原子 ✓）
    const seen = new Map();
    let order = 0;
    for (const r of rows || []) {
      order += 1;
      const a = (r && r.atom) || {};
      const pl = a.payload || {};
      if (a.kind === "create_document") {
        // **新文档自带默认层 ✓**（`fold.rs:1004` ⇒ 与 `p1LayerOrder` 同一事实 ✓）。
        if (!seen.has("layer_default")) {
          seen.set("layer_default", { created_id: a.id, tombstones: [], parent: null });
        }
      } else if (a.kind === "create_layer") {
        const id = (typeof pl.layer_id === "string" && pl.layer_id) ||
                   (typeof a.layer_id === "string" && a.layer_id) || null;
        if (id && !seen.has(id)) {
          seen.set(id, { created_id: a.id, tombstones: [],
                         parent: typeof pl.parent_id === "string" && pl.parent_id ? pl.parent_id : null });
        }
      } else if (a.kind === "tombstone" && !pl.object_id &&
                 typeof pl.layer_id === "string" && seen.has(pl.layer_id)) {
        seen.get(pl.layer_id).tombstones.push({ id: a.id, order });
      } else if (a.kind === "set_property" && pl.key === "parent_id" &&
                 typeof pl.layer_id === "string" && seen.has(pl.layer_id)) {
        seen.get(pl.layer_id).parent =
          typeof pl.value === "string" && pl.value ? pl.value : null;
      }
    }
    const inState = new Map();
    for (const [id, rec] of seen) {
      if (neutral.has(rec.created_id)) continue; // 创建被撤 ⇒ 状态里没有这一层 ✓
      inState.set(id, rec);
    }
    const layers = new Map();
    for (const id of inState.keys()) {
      let best = null;
      const visited = new Set();
      let cursor = id;
      while (typeof cursor === "string" && cursor && !visited.has(cursor)) {
        visited.add(cursor);
        const up = inState.get(cursor);
        if (!up) break; // 祖先不在状态里 ⇒ 折叠不会经它级联 ✓
        for (const t of up.tombstones) {
          if (neutral.has(t.id)) continue; // 该删除已被撤销（redo 过 `reapply` ⇒ 生效集已含）✓
          if (!best || t.order < best.order) best = t;
        }
        cursor = up.parent;
      }
      layers.set(id, { alive: !best, deleted_by: best ? best.id : null });
    }
    return layers;
  }

  /** **图层渲染属性 ✗**（**`get_histogram` 的图层作用域要「只看这一层」✓**）：
   *   **∴ 只取渲染器读的四个字段 ✗**：`visible`／`opacity`／`clipping_mask`／`parent_id` ✓
   *   （**照 `fold.rs` 的 `CreateLayer` 缺省 ＋ `apply_property_to_layer` 的取值规则 ✓**：
   *     **非布尔／非数字 ⇒ 保持旧值 ✓**，不猜默认 ✓）。
   *   **∴ 存活性不在这里算 ✗** —— **`alive`／`deleted_by` 直接取 `p2PresetLayers` ✓**
   *   （**父链级联删是同一事实 ⇒ 不写第二份 ✓** 内核 ②）。
   */
  function p2LayerRenderProps(rows) {
    rows = p2StateWindow(rows);
    const neutral = p2Neutralized(rows);
    const live = p2PresetLayers(rows, neutral);
    const props = new Map();
    const base = () => ({ visible: true, opacity: 1, clipping_mask: false, parent_id: null });
    for (const r of rows) {
      const a = (r && r.atom) || {};
      const pl = a.payload || {};
      if (a.kind === "create_document") {
        if (!props.has("layer_default")) props.set("layer_default", base());
        continue;
      }
      if (a.kind === "create_layer") {
        const id = (typeof pl.layer_id === "string" && pl.layer_id) ||
                   (typeof a.layer_id === "string" && a.layer_id) || null;
        if (!id || props.has(id)) continue;
        props.set(id, {
          visible: typeof pl.visible === "boolean" ? pl.visible : true,
          opacity: typeof pl.opacity === "number" && Number.isFinite(pl.opacity) ? pl.opacity : 1,
          clipping_mask: pl.clipping_mask === true,
          parent_id: typeof pl.parent_id === "string" && pl.parent_id ? pl.parent_id : null,
        });
        continue;
      }
      if (a.kind === "set_property" && !pl.object_id &&
          typeof pl.layer_id === "string" && typeof pl.key === "string") {
        const p = props.get(pl.layer_id);
        if (!p) continue;
        if (pl.key === "visible" && typeof pl.value === "boolean") p.visible = pl.value;
        else if (pl.key === "opacity" && typeof pl.value === "number" && Number.isFinite(pl.value)) p.opacity = pl.value;
        else if (pl.key === "clipping_mask" && typeof pl.value === "boolean") p.clipping_mask = pl.value;
        else if (pl.key === "parent_id") {
          p.parent_id = typeof pl.value === "string" && pl.value ? pl.value : null;
        }
      }
    }
    for (const id of [...props.keys()]) {
      const l = live.get(id);
      if (!l) { props.delete(id); continue; } // 创建被 revert ⇒ 状态里没有这一层 ✓
      props.get(id).alive = l.alive;
      props.get(id).deleted_by = l.deleted_by;
    }
    return props;
  }

  /** **图层类型 ✗**（**`update_layer{parent_id}` 要判「父必须是图层组」✓**，`tools.rs:3739` ✓）：
   *   **∴ 类型只在 `create_layer` 的净荷里 ✗**（`update_layer` 的 10 个键**不含** `type` ✓）
   *   ⇒ **∴ 取该层**最后一条** `create_layer` ✓**（重建层会带新类型 ✓）；缺省 `raster` ✓
   *   （`create_document` 自带的 `layer_default` 也是 `raster` ✓）。 */
  function p2LayerTypeOf(rows, layerId) {
    let type = layerId === "layer_default" ? "raster" : null;
    for (const r of rows || []) {
      const a = (r && r.atom) || {};
      if (a.kind !== "create_layer") continue;
      const pl = a.payload || {};
      const id = (typeof pl.layer_id === "string" && pl.layer_id) ||
                 (typeof a.layer_id === "string" && a.layer_id) || null;
      if (id !== layerId) continue;
      type = typeof pl.type === "string" && pl.type ? pl.type : "raster";
    }
    return type || "raster";
  }

  /** **对象在状态里是否「算死」✗**（**照折叠的两条路 ✓**）：
   *   ① 对象**自己**有生效 `tombstone` ⇒ 死 ✓ 且 `fold.rs:1318` 是**无条件覆盖** ✓
   *     ⇒ 死因取**最新一条**（`p2ObjectView.__deleted_by` 已经是这个口径 ✓ 不再各算一遍 ✓）；
   *   ② 自己没死但**所属图层链**死了 ⇒ 随层级联一起死 ✓（`fold.rs:1320`）⇒ 死因是那条**图层**原子 ✓
   *     这一条是「孤儿对象复活」要撤的目标 ✓。 */
  function p2ObjectDeath(layers, view) {
    if (view.deleted === true) return { deleted: true, by: view.__deleted_by, cascade: false };
    const layerId = typeof view.layer_id === "string" && view.layer_id ? view.layer_id : null;
    const layer = layerId && layers.has(layerId) ? layers.get(layerId) : null;
    if (!layer || !layer.alive) {
      return { deleted: true, by: layer && layer.deleted_by ? layer.deleted_by : null, cascade: true };
    }
    return { deleted: false, by: null, cascade: false };
  }

  /** **确定性 `object_id` 撞上了什么 ✗**（**照服务端 `ensure_object_slot` 的三态 ✓**：
   *   `free`（**没有它 / 创建原子被撤**⇒ 可以再建 ✓）｜`alive`｜`tombstoned`（**含级联删 ✓**）。 */
  function p2PresetSlot(rows, layers, neutral, objectId) {
    const view = p2ObjectView(rows, objectId);
    if (!view.found || neutral.has(view.__create_id)) {
      return { slot: "free", view: null, dead_by: null, cascade: false };
    }
    const death = p2ObjectDeath(layers, view);
    if (!death.deleted) return { slot: "alive", view, dead_by: null, cascade: false };
    return { slot: "tombstoned", view, dead_by: death.by, cascade: death.cascade };
  }

  /** **提交前的引用校验 ✗**（**本地镜像 `log.rs:292 validate_commit` 的 `validate_primary_refs` ✓**
   *   —— **∴ 顺序也是服务端顺序 ✓**：对象引用**先**于图层引用 ✓（`log.rs:339` vs `:424` ✓）⇒
   *   一个原子同时踩两条时 ⇒ **报的是对象那条**✓。
   *   **∴ 为什么此前本地没有这一层 ✗**：各端点各查各的（有的完全不查 ✗）⇒
   *   「对象已被 tombstone／图层已删除」这类写在本地**静默成功**✓ 而折叠层把它丢掉 ✓
   *   ⇒ **回执说 ok ✓ 画面没变 ✓** = 静默降级形态 3。
   *   **∴ 返回** `null`（可提交）**或** `{status, code, detail, extra}` ✓（**文案逐字对齐服务端 ✓**）。
   */
  function p2RefGuard(rows, layers, neutral, kind, payload) {
    const MUST_EXIST_OBJECT = new Set(["supersede", "move", "transform", "set_property", "tombstone"]);
    const objectId = typeof payload.object_id === "string" && payload.object_id ? payload.object_id : null;
    // **`create_object`／`import_image` = `MustNotExist` ✗**（**`log.rs:100` 逐字 ✓**）：
    //   判的是**服务端 `state.objects.contains_key`** ✓（`state.rs:546`）——
    //   **∴ 墓碑对象仍在 `objects` 里**（`fold.rs:1315` 只置 `deleted_by` ✓）⇒ **撞墓碑也拒 ✓**
    //   （**不是**「活着才拒」✗）；而**创建被 revert ⇒ 折叠层根本没把它建出来** ✓ ⇒ 该 id 可复用 ✓
    //   —— 本地同判：**视图存在（含墓碑）就 409**，与 `neutral` 无关 ✓。
    if (objectId !== null && (kind === "create_object" || kind === "import_image")) {
      if (p2ObjectView(rows, objectId).found === true) {
        return { status: 409, code: "precondition_failed",
                 detail: `对象 ${objectId} 已存在`, extra: { object_id: objectId } };
      }
    }
    if (objectId !== null && MUST_EXIST_OBJECT.has(kind)) {
      const ids = p2StateObjectIds(rows, neutral);
      const view = p2ObjectView(rows, objectId);
      if (!view.found || neutral.has(view.__create_id)) {
        return { status: 404, code: "reference_not_found",
                 detail: p2ObjectMissingMessage(ids, objectId), extra: { object_id: objectId } };
      }
      const death = p2ObjectDeath(layers, view);
      if (death.deleted) {
        return { status: 409, code: "precondition_failed",
                 detail: `对象 ${objectId} 已被 tombstone，${kind} 被拒绝`, extra: { object_id: objectId } };
      }
    }
    // `set_property` 的属性值引用（服务端 `log.rs:358`）：本地端点只会写 `layer_id`／`parent_id`
    // ⇒ **这两条要查 ✓**；`mask_id`／`style_id` 本地没有任何端点会写 ⇒ **不存在静默跳过 ✓**。
    if (kind === "set_property" && typeof payload.value === "string" && payload.value
        && (payload.key === "layer_id" || payload.key === "parent_id")) {
      const ref = payload.value;
      const layer = layers.has(ref) ? layers.get(ref) : null;
      if (!layer || !layer.alive) {
        return { status: 404, code: "reference_not_found",
                 detail: `set_property ${payload.key} 引用的实体 ${ref} 不存在或已删除`, extra: null };
      }
    }
    const layerId = typeof payload.layer_id === "string" && payload.layer_id ? payload.layer_id : null;
    if (layerId !== null && kind !== "create_layer" && kind !== "create_document") {
      if (!layers.has(layerId)) {
        return { status: 404, code: "reference_not_found",
                 detail: p2LayerMissingMessage(layers, layerId), extra: { layer_id: layerId } };
      }
      if (!layers.get(layerId).alive) {
        return { status: 409, code: "precondition_failed",
                 detail: `图层 ${layerId} 已删除，${kind} 被拒绝`, extra: { layer_id: layerId } };
      }
    }
    return null;
  }
  /** **图层 `locked` 的日志口径 ✓**（**只给「层已被 tombstone ⇒ `p1LayerOrder` 里没有它」兜底 ✓**）：
   *   **∴ 为什么需要 ✗**：服务端 `state.layers` **保留**墓碑层 ✓（`fold.rs:1320 tombstone_layer`
   *     只置 `deleted_by` ✓，`locked` 原样留着 ✓）⇒ **∴ 孤儿对象复活时仍要过它的锁 ✓**；
   *   而本地 `p1LayerOrder` 把删掉的层**从表里拿掉** ✓（那是 `list_layers` 该有的口径 ✓）。
   *   **∴ 取「最后一条生效的 `set_property{locked}」✓**（与 `p1LayerOrder` 同一套简化 ✓，不各发明 ✓）。 */
  function p2LayerLockInLog(rows, neutral, layerId) {
    if (typeof layerId !== "string" || !layerId) return false;
    let locked = false;
    for (const r of rows || []) {
      const a = (r && r.atom) || {};
      const pl = a.payload || {};
      if (a.kind === "set_property" && pl.layer_id === layerId && pl.key === "locked" &&
          typeof pl.value === "boolean" && !neutral.has(a.id)) locked = pl.value;
    }
    return locked;
  }
  /** **★ 锁定必须被强制 ✗ ★**（**本地镜像 `crates/yanshi-core/src/log.rs:786 validate_lock` ✓**）。
   *   **∴ 为什么此前本地一条都没有 ✗**：`locked` 在本地**只被记录与回报** ✓
   *   （`p1LayerOrder`／`list_layers`／`set_property` ✓）⇒ **锁住的图层照样能画、能删** ✓
   *   —— 与服务端注释里点名的同一个「静默失效」✓（**第 2 批判据 §C 抓出：A1 在锁的层里，
   *   服务端 403、本地 200 ✗**）。
   *   **规则逐字照服务端 ✓**（**顺序也一样：引用／存在性由 `p2RefGuard` 先判 ✓，锁在其后 ✓**）：
   *   · 原子指向**状态里已有的对象** ✓（**含墓碑 ✓** —— `fold.rs:1315` 只置 `deleted_by` ✓
   *     ⇒ `state.objects` 仍留着它 ✓）⇒ 对象**自己**或**它所在图层**被锁 ⇒ 403 `permission_denied`；
   *   · 对象**还不在状态里**（⇒ 这条原子要**新建**它 ✓）且净荷带 `layer_id` ⇒ 该层被锁 ⇒ 403；
   *   · **例外：`set_property` 改 `locked`／`visible` 永远放行 ✓**（否则解锁永远做不到 ✓）；
   *   · **图层自身的管理原子**（净荷只有 `layer_id`）不受图层锁影响 ✓（锁的是**内容** ✓）。
   *   **∴ 返回** `null`（可写）或 `{status, code, detail, extra}` ✓（**文案逐字对齐服务端 ✓**）。 */
  function p2LockGuard(rows, atom) {
    const pl = (atom && atom.payload) || {};
    const objectId = typeof pl.object_id === "string" && pl.object_id ? pl.object_id : null;
    if (objectId === null) return null;
    if (atom.kind === "set_property" && (pl.key === "locked" || pl.key === "visible")) return null;
    const neutral = p2Neutralized(rows);
    const { layers } = p1LayerOrder(rows);
    const layerLocked = (layerId) => {
      if (typeof layerId !== "string" || !layerId) return false;
      const l = layers.get(layerId);
      return l ? l.locked === true : p2LayerLockInLog(rows, neutral, layerId);
    };
    const view = p2ObjectView(rows, objectId);
    if (view.found === true && !neutral.has(view.__create_id)) {
      const blocked = view.locked === true ? "对象自身已锁定"
        : (layerLocked(view.layer_id) ? "所在图层已锁定" : null);
      if (!blocked) return null;
      return { status: 403, code: "permission_denied",
               detail: `不能修改 ${objectId}：${blocked}（先解锁再操作；改 locked/visible 不受限制）`,
               extra: { object_id: objectId } };
    }
    // **对象还不在状态里 ⇒ 这条原子要新建它 ✗**（**服务端同一个 `object_id` 分支 ✓**，
    //   **∴ 判的是**净荷形状**✗ 不是原子种类 ✓** —— 服务端注释里记着「枚举种类永远会漏」✓）。
    if (!layerLocked(pl.layer_id)) return null;
    return { status: 403, code: "permission_denied",
             detail: `不能在锁定的图层 ${pl.layer_id} 上新建对象（先解锁）`,
             extra: { layer_id: pl.layer_id } };
  }
  /** **服务端 `collect_gestures` 的本地镜像 ✗**（**`tools.rs:8714` ✓ —— 权威判据在此**）：
   *   **∴ 候选按「对象活着吗」✗**（undo ⇒ 活对象的全部原子（**含它的 `tombstone` 本身 ✓**）；
   *     redo ⇒ 死对象的原子）。**∴ 不是**「每个原子的最新动作」✗ —— 旧本地实现用后者 ✓，
   *     于是 **restore 过的 `tombstone` 被跳过 ⇒ undo 半笔 ⇒ 画面没跟着变 ✓**
   *     （**∴ 正是服务端注释里写明的教训 ✓：「对象已经不在 alive_objects() 里 ⇒ 这一笔早就被撤了」**）。
   *   **∴ 与服务端逐条对齐的跳过规则 ✓**：`create_document` 不进候选也不计数 ✓；
   *     无对象归属的原子（**含 `revert`／`reapply` —— 它们的净荷只有 `target`✓**）进 `ignored` ✓；
   *     `update_mask` 是**唯一**没 `object_id` 也进候选的 ✓（**有效性＝未被 revert ✓，
   *     照 `compute_suppressed` 补集 ✓**）；连续同对象归并成一笔 ✓（**中间隔着的无主原子不断笔 ✓**）。
   */
  function __p2CollectGestures(rows, wantAlive) {
    const neutral = p2Neutralized(rows);
    // **层折叠也只算一次 ✓**（`__p2Alive` 含级联删 ⇒ 与服务端 `alive_objects()` 同口径 ✓）。
    const layersCG = p2PresetLayers(rows, neutral);
    const aliveCache = new Map();
    const aliveOf = (oid) => {
      if (!aliveCache.has(oid)) {
        aliveCache.set(oid, __p2Alive(rows, p2ObjectView(rows, oid), layersCG, neutral));
      }
      return aliveCache.get(oid);
    };
    const gestures = [];
    let ignored = 0;
    for (let i = rows.length - 1; i >= 0; i--) {
      const a = (rows[i] && rows[i].atom) || {};
      if (a.kind === "create_document") continue;
      const pl = a.payload || {};
      let object = typeof pl.object_id === "string" && pl.object_id ? pl.object_id : null;
      if (object === null && a.kind === "update_mask") {
        if (typeof pl.mask_id === "string" && pl.mask_id) object = "mask:" + pl.mask_id;
      }
      if (object === null) { ignored++; continue; }
      const current = object.startsWith("mask:") ? !neutral.has(a.id) : aliveOf(object);
      if (current !== wantAlive) { ignored++; continue; }
      const entry = { seq: rows[i].seq, atom_id: a.id, kind: a.kind };
      const last = gestures[gestures.length - 1];
      if (last && last[0] === object) last[1].push(entry);
      else gestures.push([object, [entry]]);
    }
    return { gestures, ignored };
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
  /** **★ 本地错误的权威形状 ✓**：服务端 `http.rs` 落的是
   *   `{ok:false, error_code, retryable:false, context:{detail}}` ⇒ **前端 `callToolChecked`
   *   读的就是 `error_code` 与 `context.detail`**（`web/viewer-app.js:7024`）。
   *   **∴ 旧写法只给 `error`／`reason`** ✗ ⇒ **UI 一律显示「unknown」**✓（第 1487 轮查明）。
   *   **∴ 现在**：**两边都给** —— `error_code`／`context.detail` 与服务端同形，
   *   `error`／`reason` **保留**（本仓既有判据在读它们，删掉会红 ⇒ 加字段而不是换字段）。
   */
  function p2ToolFailure(status, code, detail, extra) {
    const context = { detail };
    if (extra) Object.assign(context, extra);
    return json({ ok: false, error: code, error_code: code, reason: detail,
                  context, retryable: false, server: false }, status);
  }
  /** **参数错误 ⇒ 400 ✗**（**照服务端 `InvalidArgument` 的口径 ✓**）。 */
  function p2Bad(reason) {
    return p2ToolFailure(400, "invalid_argument", reason);
  }
  /** **工具级 `ok:false` 的 HTTP 状态 ✓**（**逐条照 `server.rs:2872-2885` 的映射 ✓**）。
   *   **∴ 为什么 ✗**：本地以前**一律 200** ✗ ⇒ 批量撤销里「一条坏、其余成」这种
   *   **`ok:false` 但带成功计数**的响应，服务端落到 **400** 而本地 200 ✓ ⇒
   *   调用方按状态码分支时会看到两种客户端两种答案 ✗（与「同答案」的判据直接对不上 ✓）。
   *   **∴ 服务端按 `error_code` 选状态 ✗ 缺省 400** ✓（`revert_to_seq` 的批量失败体**没有**
   *   `error_code` ✓ ⇒ 落 `_ => 400` ✓）；这里**同一个 switch** ✓。
   */
  function p2ToolStatusOf(body) {
    if (!body || body.ok !== false) return 200;
    switch (body.error_code) {
      case "invalid_argument": return 400;
      case "permission_denied": return 403;
      case "reference_not_found": case "job_not_found": return 404;
      case "precondition_failed": case "conflict": case "busy": case "cancelled": return 409;
      case "resource_exhausted": case "degraded": return 503;
      case "job_pending": return 202;
      default: return 400;
    }
  }
  /** **计数族参数**（`limit`／`offset`／`since_seq`／`from_seq`／`to_seq`）**的读取 ✓**
   *   （**∴ 与服务端 `optional_count` 同形 ✗**，第 1441 轮 ✓）。
   *   **∴ 为什么 ✗**：旧写法 `Number(x) || 0` 把 `-1`／`"abc"`／`2.5` 一律**静默变成 0** ✗
   *     ⇒ 「只看序号大于 0 的」= **返回全量**，而回执里一切正常 ⇒ **静默降级形态 4** ✓。
   *   **∴ 现在**：读不出 ⇒ **400 ＋ 点名参数 ＋ 说清合法区间** ✓。
   *   **∴ 注**：查询串（`?limit=2`）本来就是文本 ⇒ 这里接受**能无损转成非负整数**的写法 ✓。
   *   **∴ 返回**：`{value}`（没给 ⇒ `undefined`）**或** `{error}`（**直接把 error return 出去**✓）。
   */
  function p2Count(raw, key) {
    if (raw == null || raw === "") return { value: undefined };
    const n = Number(raw);
    if (!Number.isFinite(n) || !Number.isInteger(n) || n < 0) {
      return { error: p2Bad(`参数 ${key} 必须是不小于 0 的整数（它是计数／序号，不能是负数或小数），`
        + `得到 ${JSON.stringify(raw)}；要「从头开始」就**别传这个参数**（缺省即 0），传 0 也可以`) };
    }
    return { value: n };
  }
  /** **引用不存在 ⇒ 404 ✗**（**照服务端 `ReferenceNotFound` 的口径 ✓**）。 */
  function p2Missing(reason) {
    return p2ToolFailure(404, "reference_not_found", reason);
  }
  /** **前置条件不成立 ⇒ 409 ✗**（**照 `http.rs:392 PreconditionFailed ⇒ 409` ✓**）。 */
  function p2Precon(reason) {
    return p2ToolFailure(409, "precondition_failed", reason);
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
  // **最近一次标注落盘的结果 ✓**（第 1440 轮审计 ✓）：key: doc → `{ persisted, reason }`。
  //   **∴ 为什么 ✗**：旧版 `p2bAnnChannel()` 只看 localStorage **存不存在** ✗ ⇒ `setItem`
  //   被**配额**打回时异常被吞 ✗ ⇒ 响应仍报 `channel:"localStorage"` ✗（数据只活在内存 ⇒ 撒谎 ✓）。
  const p2bAnnSaveState = new Map(); // key: doc → { persisted, reason }
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
    // **∴ 作用域模型与服务端同口径 ✓**（**`suggestion_scope` ＋ `scopes_conflict` ✓**）——
    //   旧本地只比「同 layer_id／object_id／region 矩形」✗ ⇒ **∴ 整文档级（`create_layer` 等）
    //   与整层级（`add_adjustment`）的变化此前**检出不来** ✗**（**判据里的 `conflicts` 会漏 ✓**）。
    return p2ScopesConflict(p2SuggestionScope(patchA), p2SuggestionScope(patchB));
  }

  /** **请求分发本体 ✗**（**不含兜底 catch ✓**：**兜底在下面 `await` 它的那一层 ✓**）。
   *   **∴ 为什么单独抽出来 ✗**：`try { return dispatch(req); } catch` **抓不到异步拒绝** ✗
   *   —— **∴ `return` 一个 Promise 时 try 不 await 它 ✓** ⇒ **∴ 锁闸的 `throw` 会穿到调用方 ✓**。
   *   **实测**（第 2 批 ✓）：`update_object` 有入口预检所以 403 ✓，
   *   而 `update_adjustment` 直接 `putAtom` ⇒ **`throw` 穿出去 ✓** ⇒ 判据/前端拿到的是 rejection ✗。
   *   ⇒ **∴ 所以兜底必须 `return await dispatch(req)` ✓**（**await 才会进 catch ✓**）。 */
  async function localDispatch(req) {
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
    const p2Norm = rawPath.length > 1 && rawPath.endsWith("/") ? rawPath.slice(0, -1) : rawPath;
    // **★ 工具名别名 ✗（**∴ 照服务端 `tools.rs:917` 的唯一收口 ✓**）**：`undo`／`redo`
    //   是「最自然的写法」（真机报告 P0 #2 ✓），服务端把它们映射到 `undo_last`／`redo_last` ✓。
    //   **∴ 旧本地没有这一层 ✗** ⇒ 前端撤销按钮打 `/api/tools/undo` **落到未实现分支 ⇒ 501** ✓
    //   （**静默家族 ✓：按钮按下去了，画什么都不动 ✓**）。
    const path = p2Norm === "/api/tools/undo" ? "/api/tools/undo_last"
      : p2Norm === "/api/tools/redo" ? "/api/tools/redo_last" : p2Norm;
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
    /** **重放一步工具调用 ✗**（**走**同一台分发器** ✓** —— 与人工操作完全同路径 ✓）：
     *   **∴ 必须经 `localDispatch` ✗ 不直接调 handler ✓**（**否则分支条件会漏 ✓**）。
     *   **∴ 锁闸的 `__p2Failure` 是一个**响应**✗ 不是程序错误 ✓ ⇒ **∴ 原样带回来 ✓**。 */
    async function p2bReplay(tool, args) {
      const target = new URL("/api/tools/" + tool, url.origin);
      target.searchParams.set("doc", doc);
      const request = new Request(target.href, {
        method: "POST", headers: { "content-type": "application/json" },
        body: JSON.stringify(args === undefined ? {} : args),
      });
      let res;
      try {
        res = await localDispatch(request);
      } catch (e) {
        if (e && e.__p2Failure) res = e.__p2Failure;
        else throw e;
      }
      const out = await res.json().catch(() => null);
      return { res, body: out };
    }
    /** **回滚到 `startSeq` ✗**（**本次重放写下的**状态效果**原子 ⇒ 各一条 `revert{target}` ✓**）：
     *   **∴ 为什么逐条 revert 而不是删原子 ✗**：**日志只追加 ✓**（**删了 seq 就断 ✓**），
     *   **`revert(revert(x)) ≡ reapply(x)` 由折叠层保证 ✓** ⇒ **∴ 这就是「撤销」的同一条路 ✓**。
     *   **∴ 不可 revert 的原子**（**`declare_head` 等历史原子 ✗**，`atom.rs:450` ✓）
     *   **⇒ 如实列出来 ⇒ `ok:false` ✓**（**∴ 不许假装「已经全撤了」✗**）。 */
    async function p2bRollbackTo(startSeq) {
      const rows = await atomsOf(handle, doc);
      const targets = [], skipped = [];
      for (const r of rows) {
        const a = (r && r.atom) || {};
        if (!(Number(r.seq) > startSeq) || !a.id) continue;
        if (P2_REVERTIBLE_KINDS.has(a.kind)) targets.push(a.id);
        else skipped.push({ atom_id: a.id, kind: a.kind || null, seq: r.seq });
      }
      const reverted = [];
      try {
        for (const target of targets) {
          await putAtom(handle, doc, p2bAtom("revert", { target }));
          reverted.push(target);
        }
      } catch (e) {
        return { ok: false, ids: reverted, pending: targets.slice(reverted.length),
                 skipped, detail: String((e && e.message) || e) };
      }
      return { ok: skipped.length === 0, ids: reverted, pending: [], skipped, detail: null };
    }
    /** **接受单条建议 ✗**（**照 `accept_one`（`tools.rs:13354`）逐条 ✓**）：
     *   ① 取 patch（**必须是一条 `suggest` 原子** ✓）⇒ ② 空 patch 拒 ✓ ⇒ ③ 状态闸门
     *   （**已接受 ⇒ 幂等回执 ✓；已拒绝 ⇒ 409 ✓**）⇒ ④ **全量预检**（**任一步不合法 ⇒ 整体不写 ✓**）
     *   ⇒ ⑤ 按序重放（**走 `localDispatch` ✓**）⇒ ⑥ 提交 `accept_suggestion` ＋ 关联标注置 resolved ✓。
     *   **∴ 本地**多一层**：重放中途运行期失败 ⇒ **把已写的状态原子逐条 `revert`** ✓
     *   （**服务端此时会留下部分应用 ✗** —— **∴ 这是本地更强的一处，如实登记 ✓**）。
     *   @returns `{ok, status, body}` */
    async function p2bAcceptOne(suggestionId) {
      const fail = (status, code, detail, extra) => ({ ok: false, status,
        body: { ok: false, error: code, error_code: code, reason: detail,
                context: { detail, ...(extra || {}) }, retryable: false, server: false } });
      const rows = await atomsOf(handle, doc);
      const atoms = rows.map((r) => r.atom).filter(Boolean);
      const target = atoms.find((a) => a.id === suggestionId);
      if (!target) return fail(404, "reference_not_found", "建议 " + suggestionId + " 不在日志中");
      if (target.kind !== "suggest") {
        return fail(400, "invalid_argument",
          suggestionId + " 是 " + target.kind + " 原子，不是建议（suggest）");
      }
      const pl = target.payload || {};
      const patch = Array.isArray(pl.patch) ? pl.patch : [];
      const annotationId = typeof pl.annotation_id === "string" ? pl.annotation_id : null;
      if (patch.length === 0) return fail(400, "invalid_argument", "建议没有可重放的步骤");
      let acceptedBy = null, rejectedBy = null;
      for (const a of atoms) {
        if ((a.payload || {}).target_atom_id !== suggestionId) continue;
        if (a.kind === "accept_suggestion") acceptedBy = a.id;
        else if (a.kind === "reject_suggestion") rejectedBy = a.id;
      }
      if (acceptedBy) {
        return { ok: true, status: 200,
                 body: { ok: true, suggestion_id: suggestionId, applied_atom_ids: [],
                         accept_atom_id: acceptedBy, resolved_annotations: [],
                         already_accepted: true,
                         note: "该建议此前已被接受，本次不重复施加效果", server: false } };
      }
      if (rejectedBy) {
        return fail(409, "precondition_failed",
          "建议 " + suggestionId + " 已被拒绝（原子 " + rejectedBy + "），不能接受已拒绝的建议");
      }
      // ── ④ 全量预检：任一步不合法 ⇒ 整体拒绝，**一步都不写** ✓ ──
      const knownLayers = new Set(p2cState(rows).layers.map((layer) => layer.layer_id));
      const knownObjects = new Set(
        p2ObjectSummaries(rows, { includeHidden: true }).map((object) => object.object_id));
      for (const step of patch) {
        const isObject = step && typeof step === "object" && !Array.isArray(step);
        const tool = isObject && typeof step.tool === "string" ? step.tool : null;
        if (tool === null) return fail(400, "invalid_argument", "补丁步骤缺少 tool");
        const stepArgs = isObject && step.arguments !== undefined ? step.arguments : {};
        const error = p2CheckSingleStep(tool, stepArgs, knownLayers, knownObjects);
        if (error) {
          return fail(error.code === "reference_not_found" ? 404 : 400, error.code, error.detail);
        }
      }
      // ── ⑤ 逐条重放 ──
      const startSeq = await p2bHeadSeq();
      let failure = null;
      for (let index = 0; index < patch.length; index++) {
        const step = patch[index];
        const tool = step.tool;
        const stepArgs = step.arguments !== undefined ? step.arguments : {};
        if (!p2ToolSpec(tool)) {
          failure = { status: 400, code: "invalid_argument",
                      detail: `patch[${index}] 引用了未知工具 ${tool}` };
          break;
        }
        const bad = p2ValidateArgsFor(tool, stepArgs);
        if (bad) { failure = { status: 400, code: bad.code, detail: bad.detail }; break; }
        let outcome;
        try {
          outcome = await p2bReplay(tool, stepArgs);
        } catch (e) {
          // **∴ 本地没有这个端点 ✗**（**`endpoint_not_local` ✓**）或其它未兜住的抛错 ✓
          failure = { status: 501, code: "not_implemented_locally",
                      detail: "patch[" + index + "] 的 " + tool + " 在本地层没有实现"
                        + "（服务端能执行，PWA 不能）⇒ " + String((e && e.message) || e),
                      extra: { step_index: index, tool } };
          break;
        }
        const out = outcome.body;
        if (!outcome.res.ok || !out || out.ok === false) {
          const status = outcome.res.status >= 400 ? outcome.res.status : 500;
          failure = { status,
                      code: (out && (out.error_code || out.error)) || "apply_failed",
                      detail: (out && out.context && out.context.detail)
                        || (out && out.reason)
                        || `patch[${index}] 的 ${tool} 失败（状态 ${outcome.res.status}）`,
                      extra: { step_index: index, tool } };
          break;
        }
      }
      if (failure) {
        const rollback = await p2bRollbackTo(startSeq);
        const body = {
          ok: false, error: failure.code, error_code: failure.code, reason: failure.detail,
          context: { detail: failure.detail, ...(failure.extra || {}) },
          retryable: false, server: false,
          rolled_back: rollback.ok && rollback.pending.length === 0,
          rolled_back_atom_ids: rollback.ids,
          rollback_skipped: rollback.skipped,
          local_rollback_note: "本地重放在中途失败 ⇒ 用 revert 原子撤销本次已写入的状态原子"
            + "（服务端此时会留下部分应用状态）；不可 revert 的原子会在 rollback_skipped 里列出",
        };
        if (!rollback.ok) {
          body.rollback_failed = rollback.detail
            || "有原子不可 revert（如 declare_head），部分应用残留";
          body.rollback_pending_atom_ids = rollback.pending;
        }
        return { ok: false, status: failure.status, body };
      }
      // ── ⑥ 提交接受原子 ＋ 解析关联标注 ──
      const rowsAfter = await atomsOf(handle, doc);
      const applied = [];
      for (const r of rowsAfter) {
        const a = (r && r.atom) || {};
        if (Number(r.seq) > startSeq && a.id && P2_REVERTIBLE_KINDS.has(a.kind)) applied.push(a.id);
      }
      const atom = p2bAtom("accept_suggestion", {
        target_atom_id: suggestionId, applied_atom_ids: applied, annotation_id: annotationId,
      });
      const seq = await putAtom(handle, doc, atom);
      const st = p2bAnnLoad();
      const resolved = [];
      for (const id of st.order) {
        const versions = st.versions[id];
        if (!versions || versions.length === 0) continue;
        const current = versions[versions.length - 1];
        if (current.suggestion_id === suggestionId && current.status === "pending") {
          resolved.push(p2bAnnTransition(st, id, "resolved", atom.id));
        }
      }
      p2bAnnSave();
      return { ok: true, status: 200,
               body: { ok: true, suggestion_id: suggestionId, applied_atom_ids: applied,
                       accept_atom_id: atom.id, resolved_annotations: resolved,
                       pending_annotations: p2bAnnPending(st), seq, server: false } };
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
      // **报"实际生效"的通道 ✓**（第 1440 轮审计 ✓）：localStorage 在、但**上一次写被配额打回** ⇒
      // 实际只剩内存通道 ⇒ 报 `memory` ✓（旧版报 `localStorage` ✗ = 撒谎）。下一次成功写会自愈 ✓。
      const last = p2bAnnSaveState.get(doc);
      try {
        if (typeof localStorage !== "undefined" && !(last && last.persisted === false)) {
          return "localStorage";
        }
      } catch (e) {}
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
      if (typeof localStorage === "undefined") {
        p2bAnnSaveState.set(doc, { persisted: false, reason: "localStorage 不可用 ⇒ 标注只活在内存（刷新即失）" });
        return;
      }
      try {
        localStorage.setItem("yanshi:annotations:" + doc, JSON.stringify(st));
        p2bAnnSaveState.set(doc, { persisted: true, reason: null });
      } catch (e) {
        // **∴ 配额等 ⇒ 内存里还有，但必须说出来 ✗**（**∴ 不抛 ✓**，第 1440 轮审计 ✓：
        // 旧版这里**静默吞异常** ✗ ⇒ `channel` 字段仍报 localStorage ✗ ⇒ 调用方以为已持久化 ✓）。
        const reason = "localStorage 写入失败（配额？）：" + String((e && e.message) || e);
        p2bAnnSaveState.set(doc, { persisted: false, reason });
        console.warn("标注未能持久化：" + reason + "（文档 " + doc + "）");
      }
    }
    function p2bFail(status, error, reason) {
      const e = new Error(reason); e.p2bStatus = status; e.p2bError = error; throw e;
    }
    function p2bCatch(e, endpoint) {
      // **★ 锁闸错误不许被兜成 500 ✗ ★**（**`putAtom` 的拒绝自带 `__p2Failure` ✓**）：
      //   **∴ 这里吞掉 ⇒ 服务端 403 会在本地变成 `annotation_failed:500`** ✓（**错误码谎报 ✓**）
      //   ⇒ **∴ 原样再抛 ⇒ 顶层 catch 换 403 ✓**。
      if (e && e.__p2Failure) throw e;
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
      //
      // **★ `since_seq`／`limit`／`kind`／`actor` 四条参数 ✗ ★**（第 879 轮 ✓；**判据「按类型筛选」实测红 ✓**）：
      //   **∴ 原来的错 ✗**：**这里**只认 `order` ✗** ⇒ **∴ 其余四条**被**静默忽略** ✓
      //     ⇒ **∴ 于是**：**用户选「按类型筛选」✗** ⇒ **∴ 列表**纹丝不动 ✓**（**与「筛选没生效」完全同形 ✓）
      //       ⇒ **∴ 而**服务端（`tools_read.rs:695` ✓）**是**认这四条**的 ✗**
      //         ⇒ **∴ 于是**：**同一次点击**在服务端／本地**给出不同结果 ✗**（**两条接口不同形的老坑 ✓）
      //   **∴ 现在**：**逐条对上服务端语义 ✓**：
      //     `since_seq`（默认 0，严格大于 ✓）、`limit`（默认 200，上限 2000，**在筛选之后截断 ✓**）、
      //     `kind`／`actor`（精确相等 ✓）、`order`（desc ⇒ 从新到旧 ✓）。
      //   **∴ 且**补 `head_seq`／`next_since` **两个回执字段 ✓**（服务端有 ✓ ⇒ 本地不给自己造第二套形状 ✓）。
      const pick = (name) => {
        if (body && body[name] != null && body[name] !== "") return body[name];
        if (q && q.get(name) != null && q.get(name) !== "") return q.get(name);
        return undefined;
      };
      const countArg = (name, fallback, max) => {
        const raw = pick(name);
        if (raw == null) return { value: fallback };
        const n = Number(raw);
        if (!Number.isFinite(n) || n < 0) return { error: name + " 必须是非负数" };
        const v = Math.floor(n);
        return { value: max == null ? v : Math.min(v, max) };
      };
      const sinceArg = countArg("since_seq", 0);
      const limitArg = countArg("limit", 200, 2000);
      const badArg = sinceArg.error || limitArg.error;
      if (badArg) {
        // **∴ 直接 400 ✗**（**不**抛 ✓）：**抛出去会被外层兜成 `not_implemented_locally` ✗**
        //   ⇒ **∴ 那**会让调用方以为"本地没实现"✗，而事实是**参数错** ✓。
        return json({ ok: false, error: "invalid_argument", detail: badArg, server: false }, 400);
      }
      const since = sinceArg.value;
      const limit = limitArg.value;
      const order = pick("order") || "";
      const newestFirst = order === "desc" || order === "newest_first";
      const kind = pick("kind");
      const actor = pick("actor");
      const seqOf = (a) => Number(a && a.seq);
      const headSeq = rows.reduce((m, r) => {
        const s = seqOf(r && r.atom);
        return Number.isFinite(s) && s > m ? s : m;
      }, 0);
      let atoms = rows.map((r) => (r && r.atom) || null).filter(Boolean);
      atoms = atoms.filter((a) => !(Number.isFinite(seqOf(a)) && seqOf(a) <= since));
      if (kind != null) atoms = atoms.filter((a) => a.kind === kind);
      if (actor != null) atoms = atoms.filter((a) => a.actor === actor);
      if (newestFirst) atoms = atoms.slice().reverse();
      atoms = atoms.slice(0, limit);
      const nextSince = atoms.reduce((m, a) => {
        const s = seqOf(a);
        return Number.isFinite(s) && s > m ? s : m;
      }, since);
      return json({ ok: true, atoms, count: atoms.length, head_seq: headSeq,
                    next_since: nextSince, server: false });
    }

    // **③ `/api/tools/list_objects` ✗**：**对象列表** ✗** ⇒ **∴ 从**声明了 `object_id` 的原子**推导 ✓
    //   **∴ 如实说明 ✗**：**这是**日志级**的对象表 ✓（**∴ 字段**取原子里**真的有的那些 ✓），
    //     **∴ 不**补算位置／包围盒 ✓（**∴ 那**要重放内核 ⇒ **∴ 留给**需要它的调用方 ✓）** ✓✓
    if (path === "/api/tools/list_objects") {
      // **口径照服务端 `read_list_objects`（`tools.rs:3148`）✓**：
      //   一对象一条（按 `object_id` 去重 ✓）、`layer_id`／`type`／`include_hidden` 三条过滤 ✓、
      //   只列存活对象（创建被撤销／已墓碑／随层级联删都算死 ✓）、按 `(layer_id, z_index, id)` 排序 ✓。
      //   **∴ 折叠细节统一在 `p2ObjectSummaries` ✓**（`get_state{include_objects}` 共用同一份 ✓，
      //     两处各写一遍 ⇒ 一定漂移 ✓）。**∴ 本端点纯原子折叠 ✗ 不碰内核 ✓**（`bbox` 等如实不补 ✓）。
      const rows = await atomsOf(handle, doc);
      const objects = p2ObjectSummaries(rows, {
        layerFilter: body.layer_id,
        typeFilter: body.type,
        includeHidden: body.include_hidden === true, // 缺省 false（照 `unwrap_or(false)` ✓）
      });
      return json({ ok: true, objects, count: objects.length, server: false,
                    derived_from: "atoms" });
    }

    // **P2c-71 `get_path` ✓**（**照 `read_get_path`（`tools_pathdata.rs:109`）逐字 ✓**）：
    //   **∴ 读的是对象**本地坐标**几何 ✗**（不应用 `object.transform` ✓，与 `get_object` 的
    //     `data` 同一口径 ✓）；**∴ 在状态里但已删 ⇒「已被删除」✓；创建被撤 ⇒「不存在」✓**
    //     （服务端 `state.objects` 与 `deleted_by` 的两个分支 ✓）；**不是 `Shape{Path}` ⇒ 一句 400** ✓
    //     （不静默回退 ✗）；**∴ `d` 用 Rust `Display` 形 ✓**（最短往返、无科学计数法 ✓）、
    //     **`length` 用生产容差 0.10 展平** ✓。
    if (path === "/api/tools/get_path") {
      const specBad = p2ValidateArgsFor("get_path", body);
      if (specBad) return p2ToolFailure(400, specBad.code, specBad.detail);
      const objectId = body.object_id;
      const rows = p2StateWindow(await atomsOf(handle, doc));
      const neutral = p2Neutralized(rows);
      const view = p2ObjectView(rows, objectId);
      if (!view.found || neutral.has(view.__create_id)) {
        return p2Missing(`对象 ${objectId} 不存在`);
      }
      const death = p2ObjectDeath(p2PresetLayers(rows, neutral), view);
      if (death.deleted) return p2Missing(`对象 ${objectId} 已被删除`);
      const notPath = () => p2Bad(
        `对象 ${objectId} 不是 path 形状（只认 draw_shape 建的 kind=path 对象）`);
      if (p2ObjectTypeOf(rows, objectId) !== "shape") return notPath();
      const data = view.data;
      const geometry = data && typeof data === "object" && !Array.isArray(data)
        ? data.geometry : null;
      if (!geometry || typeof geometry !== "object" || Array.isArray(geometry)) return notPath();
      if (geometry.kind !== "path") return notPath();
      const parsed = p2PathSubpaths(geometry);
      if (parsed === null) return notPath();
      const bbox = p2PathControlBbox(parsed.subpaths) || [0, 0, 0, 0];
      let length = 0;
      for (const sub of parsed.subpaths) length += p2PolylineLength(p2FlattenSubpath(sub, 0.1));
      return json({
        ok: true, object_id: objectId, layer_id: view.layer_id ?? null, kind: "path",
        fill_rule: parsed.fillRule,
        geometry: { kind: "path", fill_rule: parsed.fillRule, subpaths: parsed.subpaths },
        subpaths: parsed.subpaths,
        d: p2PathSvgD(parsed.subpaths),
        bbox: { x: bbox[0], y: bbox[1], w: bbox[2], h: bbox[3] },
        length,
        node_counts: parsed.subpaths.map((sub) => sub.segments.length + 1),
        server: false,
      });
    }

    // **P2c-72 `edit_path` ✓**（**照 `write_edit_path`（`tools_pathdata.rs:145`）逐字 ✓**）：
    //   **∴ 四个算子共用一条路 ✗**：改 subpaths 克隆 → **复验整份几何** → 写回
    //     `data.geometry`（**`subpaths` 与 `bbox` 同步 ✓**，**不许留旧 bbox 骗脏区 ✗**）→ `supersede` 落原子 ✓；
    //   **∴ 顺序照服务端 ✗**：入口校验 → 算子白名单 → 对象寻址（404／400）→ `subpath` 下标 →
    //     算子参数（`node`／`segment`／`t`／`side`／`to`）✓；**编辑失败 ⇒ 整单拒绝、不落原子 ✓**；
    //   **∴ 节点语义全在 `path.rs` 那四个纯函数里 ✓**（锚点下标、刚性平移、de Casteljau 插点、
    //     删点合并、直段升格补柄 ✓）—— 本地照抄，**不另立口径 ✗**。
    if (path === "/api/tools/edit_path") {
      const specBad = p2ValidateArgsFor("edit_path", body);
      if (specBad) return p2ToolFailure(400, specBad.code, specBad.detail);
      const objectId = body.object_id;
      const op = body.op;
      if (!["move_node", "insert_node", "delete_node", "set_handle"].includes(op)) {
        return p2Bad(`未知的 edit_path 算子 ${op}（已实现：move_node / insert_node / delete_node / set_handle）`);
      }
      const rows = await atomsOf(handle, doc);
      const neutral = p2Neutralized(rows);
      const view = p2ObjectView(rows, objectId);
      if (!view.found || neutral.has(view.__create_id)) {
        return p2Missing(`对象 ${objectId} 不存在`);
      }
      const death = p2ObjectDeath(p2PresetLayers(rows, neutral), view);
      if (death.deleted) return p2Missing(`对象 ${objectId} 已被删除`);
      const notPath = () => p2Bad(
        `对象 ${objectId} 不是 path 形状（只认 draw_shape 建的 kind=path 对象）`);
      if (p2ObjectTypeOf(rows, objectId) !== "shape") return notPath();
      const data = view.data;
      const geometry = data && typeof data === "object" && !Array.isArray(data)
        ? data.geometry : null;
      if (!geometry || typeof geometry !== "object" || Array.isArray(geometry)) return notPath();
      if (geometry.kind !== "path") return notPath();
      const parsed = p2PathSubpaths(geometry);
      if (parsed === null) return notPath();
      // **非负整数下标 ✓**（**照 `pathdata_index` ✓**：`as_u64` ⇒ 负数／非整数一律拒绝 ✓）。
      const indexOf = (key) => {
        const value = body[key];
        return typeof value === "number" && Number.isInteger(value) && value >= 0 ? value : null;
      };
      // **`[x, y]` 点 ✓**（**照 `pathdata_point` ✓**：必须恰好两元有限数 ⇒ 否则点名参数 ✓）。
      const pointOf = (key) => {
        const value = body[key];
        if (!Array.isArray(value) || value.length !== 2) return null;
        if (!p2Num(value[0]) || !p2Num(value[1])) return null;
        return [value[0], value[1]];
      };
      const subpathIndex = indexOf("subpath");
      if (subpathIndex === null) return p2Bad("参数 subpath 必须是非负整数下标");
      if (subpathIndex >= parsed.subpaths.length) {
        return p2Bad(`subpath 下标 ${subpathIndex} 越界（对象 ${objectId} 共 ${parsed.subpaths.length} 条子路径）`);
      }
      // **`p2PathSubpaths` 的产物是**新建对象** ✓**（点列也是新数组 ✓）⇒ 直接改不碰原子里的旧值 ✓。
      const subpaths = parsed.subpaths;
      const sub = subpaths[subpathIndex];
      let newNode = null;
      let outcome = null;
      if (op === "move_node") {
        const node = indexOf("node");
        if (node === null) return p2Bad("参数 node 必须是非负整数下标");
        const to = pointOf("to");
        if (!to) return p2Bad("参数 to 必须是 [x, y] 两个有限数");
        outcome = p2PathMoveNode(sub, node, to);
      } else if (op === "insert_node") {
        const segment = indexOf("segment");
        if (segment === null) return p2Bad("参数 segment 必须是非负整数下标");
        const t = typeof body.t === "number" ? body.t : 0.5;
        outcome = p2PathInsertNode(sub, segment, t);
        if (outcome.ok) newNode = outcome.index;
      } else if (op === "delete_node") {
        const node = indexOf("node");
        if (node === null) return p2Bad("参数 node 必须是非负整数下标");
        outcome = p2PathDeleteNode(sub, node);
      } else {
        const node = indexOf("node");
        if (node === null) return p2Bad("参数 node 必须是非负整数下标");
        const sideName = typeof body.side === "string" ? body.side : null;
        if (sideName === null) return p2Bad("缺少必填参数 side");
        if (sideName !== "in" && sideName !== "out") {
          return p2Bad(`side 只认 in / out，得到 ${sideName}`);
        }
        const to = pointOf("to");
        if (!to) return p2Bad("参数 to 必须是 [x, y] 两个有限数");
        outcome = p2PathSetHandle(sub, node, sideName, to);
      }
      if (!outcome || !outcome.ok) {
        return p2Bad(`edit_path ${op} 失败：${(outcome && outcome.error) || "未知错误"}`);
      }
      // **复验整份几何 ✓**（**同一份校验 ✓**：不合法宁可整单拒绝也不落库 ✓）。
      if (p2PathSubpaths({ fill_rule: parsed.fillRule, subpaths }) === null) {
        return p2Bad("编辑后几何非法（已拒绝落库）");
      }
      const bbox = p2PathControlBbox(subpaths) || [0, 0, 0, 0];
      const box = { x: bbox[0], y: bbox[1], w: bbox[2], h: bbox[3] };
      const newData = { ...data, geometry: { ...geometry, subpaths, bbox: box } };
      const seq = await putAtom(handle, doc,
        p2Envelope("supersede", { object_id: objectId, data: newData }));
      let length = 0;
      for (const each of subpaths) length += p2PolylineLength(p2FlattenSubpath(each, 0.1));
      return json({ ok: true, op, object_id: objectId, subpath: subpathIndex, new_node: newNode,
                    subpaths, d: p2PathSvgD(subpaths), bbox: box, length,
                    node_counts: subpaths.map((each) => each.segments.length + 1),
                    head: seq, server: false });
    }

    // **P2c-70 `vector_hit_test` ✓**（**照 `read_vector_hit_test`，`tools_vector.rs:288` 逐字 ✓**）：
    //   **∴ 纯原子折叠 ✗ 不碰内核 ✓** —— 命中的是**脊线 ＋ 剖面**（不是像素 ✓）；
    //   **∴ 有效脊线**：`smooth` 时是渲染端**同一份** Catmull-Rom 采样 ✓（`vector.rs:48` ✓）；
    //   **∴ `strokes_examined` 在几何解析之前就 +1** ✓（`tools_vector.rs:313` ✓）⇒
    //     **几何坏掉的对象照样算「查过」** ✓ 但不出命中 ✓（不是静默跳过 ✗）；
    //   **∴ 只查 `Stroke` 类型的对象** ✓（图形／文字／填充／组／路径…不看 ✓）；
    //   **∴ 隐藏对象照查** ✓（`objects_in_layer` 不过滤可见性 ✓ ⇒ 本地**不许**用 `includeHidden:false` ✗）；
    //   **∴ 已删图层仍解析得类型** ✓（`layer_type_of` 查 `state.layers.get()` ✓ 不是 `alive_layers()` ✓）
    //     —— 它的对象全随层死 ⇒ **读数是 0** ✓，**不是 404** ✗。
    if (path === "/api/tools/vector_hit_test") {
      const specBad = p2ValidateArgsFor("vector_hit_test", body);
      if (specBad) return p2ToolFailure(400, specBad.code, specBad.detail);
      const layerId = body.layer_id;
      const rows = await atomsOf(handle, doc);
      const layerType = p2LayerTypeFold(rows, layerId);
      if (layerType === null) return p2Missing(`图层 ${layerId} 不存在`);
      if (layerType !== "vector") {
        // **类型名照 Rust 的 `Debug` ✗**（`{other:?}` ⇒ `Raster`／`LayerGroup`／`Mask` ✓）。
        const debugName = { raster: "Raster", layer_group: "LayerGroup", mask: "Mask" }[layerType]
          || "Raster";
        return p2Bad(`图层 ${layerId} 是 ${debugName} 图层、不是矢量图层 => 矢量工具不作用于它`
          + `（建矢量层请用 create_layer 的 kind 参数传 vector）`);
      }
      const x = body.x, y = body.y;
      const tolerance = typeof body.tolerance === "number" ? body.tolerance : 0;
      if (tolerance < 0) return p2Bad("tolerance 不能为负");
      let strokesExamined = 0;
      const hits = [];
      for (const summary of p2ObjectSummaries(rows, { layerFilter: layerId, includeHidden: true })) {
        if (p2ObjectTypeOf(rows, summary.object_id) !== "stroke") continue;
        strokesExamined += 1;
        const view = p2ObjectView(rows, summary.object_id);
        const geometry = p2VectorGeometry(view.data);
        if (geometry === null) continue;
        const hit = p2VectorSpineHit(p2VectorBrush(view.data), geometry, x, y);
        if (hit === null || !(hit.clearance <= tolerance)) continue;
        hits.push({ object_id: summary.object_id, distance: hit.distance,
                    radius: hit.radius, clearance: hit.clearance });
      }
      // **排序照服务端 ✓**：`clearance` 升序，**并列按 `object_id` 升序** ✓。
      hits.sort((a, b) => (a.clearance - b.clearance) || (a.object_id < b.object_id ? -1
        : a.object_id > b.object_id ? 1 : 0));
      return json({ ok: true, layer_id: layerId, x, y, tolerance,
                    strokes_examined: strokesExamined, hits, count: hits.length, server: false });
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
      //     `blend_mode`／`locked`／`z_index`／`parent_id` 等属性如实返回 ✓。
      //   **★ `objects` 每层计数 ✗ ★**（`read_list_layers` 的最后一个字段 ✓）：
      //     **∴ 旧本地整条不给 ✗** ⇒ 图层面板显示每层对象数时**静默拿到 0/undefined** ✗
      //     ⇒ 这里用与 `list_objects` **同一份存活折叠**（`__p2Alive` 含图层级联删 ✓）✓。
      const rows = p2StateWindow(await atomsOf(handle, doc)); // 跳变窗口内 ✓（恢复后旧段不在状态里 ✗）
      const { layers, order } = p1LayerOrder(rows);
      const neutralLL = p2Neutralized(rows);
      const layersLL = p2PresetLayers(rows, neutralLL);
      const counts = new Map();
      const seenLL = new Set();
      for (const r of rows) {
        const a = (r && r.atom) || {};
        const id = a.payload && a.payload.object_id;
        if (typeof id !== "string" || !id || seenLL.has(id)) continue;
        seenLL.add(id);
        const v = p2ObjectView(rows, id);
        if (!__p2Alive(rows, v, layersLL, neutralLL)) continue;
        const lid = v.layer_id ?? null;
        if (lid !== null) counts.set(lid, (counts.get(lid) || 0) + 1);
      }
      // **存活照 `alive_layers()` ✓**：`p1LayerOrder` 只认**自己那条**墓碑 ✗ ⇒ 这里再叠
      // `p2PresetLayers`（**祖先级联删 ＋ 创建被撤销** ✓，与 `list_objects` 同一份口径 ✓）。
      // **∴ 缺省即死 ✗**（`p2PresetLayers` **不在表里 = 创建被撤销** ⇒ 状态里没有它 ✓）：
      //   旧写法 `!has ⇒ alive` ✗ 把「撤销了建层」当成活着 ⇒ `list_layers` 里那层还在 ✓
      // （本轮 `accept_suggestion` 回滚判据抓到；`p2cState` 同一行一并改 ✓）。
      const out = order.filter((id) => layers.has(id) &&
                                       layersLL.has(id) && layersLL.get(id).alive)
                       .map((id) => ({ ...layers.get(id), objects: counts.get(id) || 0 }));
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
      const arg = (key) => (body && body[key] !== undefined ? body[key] : pl[key]);
      const layerId = arg("layer_id") || "layer_" + String(Date.now());
      // **缺省名照服务端 `write_create_layer` ✓**（**取 id 末四位做短名 ✓**：
      //   **∴ 旧写法拿整个 `layer_id` 当名字 ✗** ⇒ 自动建的层叫 `layer_1730000000000` ✓
      //   而服务端叫「图层 0000」✓ ⇒ 同一份文档两种环境下**图层面板长得不一样** ✓）。
      const shortName = (id) => {
        const tail = String(id).slice(Math.max(0, String(id).length - 4));
        return tail ? `图层 ${tail}` : "图层";
      };
      const name = arg("name") || shortName(layerId);
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
      // **图层类型 ✗**（**照 `write_create_layer` 的 `kind`／`type` 双键与三条报错文案 ✓**：
      //   **∴ 此前本地一个字都不检查 ✗** 任意字符串照落 ⇒ 内核读不出 ⇒ 静默退化 ✓）。
      const TYPES = ["raster", "vector", "layer_group", "mask"];
      const kindRaw = arg("kind"), typeRaw = arg("type");
      if (kindRaw !== undefined && kindRaw !== null) {
        if (kindRaw !== "raster" && kindRaw !== "vector") {
          return p2Bad(`kind=「${kindRaw}」不认识：只收 raster / vector（图层组与蒙版请用 type ✓）`);
        }
        if (typeRaw !== undefined && typeRaw !== null && typeRaw !== kindRaw) {
          return p2Bad(`kind=「${kindRaw}」与 type=「${typeRaw}」不一致 ⇒ 说清你要哪种图层`);
        }
        atom.payload.type = kindRaw;
      } else if (typeRaw !== undefined && typeRaw !== null) {
        if (!TYPES.includes(typeRaw)) {
          return p2Bad(`type=「${typeRaw}」不认识：只收 ${TYPES.join(" / ")}`);
        }
        atom.payload.type = typeRaw;
      }
      // **`parent_id` 要**传下去**并校验 ✗**（**此前本地整个丢掉这个键 ✓**
      //   ⇒ 图层组在离线模式下**根本建不出来**✓ 而回执说 ok ✓ = 形态①「输入不被采纳」✓）。
      const parent = arg("parent_id");
      const rows = await atomsOf(handle, doc);
      const neutral = p2Neutralized(rows);
      const layers = p2PresetLayers(rows, neutral);
      if (typeof parent === "string" && parent) {
        atom.payload.parent_id = parent;
        if (parent === layerId) {
          return p2Bad(`图层父链成环：${layerId} 以 ${parent} 为父层时会回到自己`);
        }
        const parentLayer = layers.get(parent);
        if (!parentLayer || !parentLayer.alive) {
          // **折叠层用的就是这句 ✗**（**`fold.rs:489` 对**没活过**的父层报 `missing_reference("父图层")` ✓**）⇒
          //   **∴ 逐字照抄 ✓**。**∴ 而服务端 HTTP 层**没跑这条前置检查**✓（实测：建带死父层的层回 200 ✓
          //   而 `list_layers` 里根本没有它 ⇒ 后续写入 404 ✓ ⇒ **那是服务端的静默失败 ✓ 记在补丁封面 ✓**）
          //   ⇒ **∴ 本地不照抄那个坏行为 ✓**：写不进去的原子就不写 ✓ 并且当场说清为什么 ✓。
          const aliveKeys = [...layers.keys()].filter((id) => layers.get(id).alive);
          return p2ToolFailure(404, "reference_not_found",
            p2MissingReferenceMessage("父图层", parent, aliveKeys), { layer_id: parent });
        }
      }
      if (Number.isFinite(Number(arg("z_index")))) atom.payload.z_index = Number(arg("z_index"));
      // **槽位检查 ✗**（**照 `log.rs:429` 的 `MustNotExist` ＋ `fold.rs:482` 的「只与**存活**层冲突」✓**：
      //   **墓碑不占 id ✓**（删掉再建同名是合法的重建 ✓）⇒ 只有**活着**的同 id 才拒 ✓）。
      const existing = layers.get(layerId);
      if (existing && existing.alive) {
        if (arg("if_exists") === "reuse") {
          // **名字取折叠后的层视图 ✗**（**`p1LayerOrder` 会把 `update_layer{name}` 折进去 ✓**）
          //   ⇒ **∴ 与服务端 `state.layers[layer_id].name` 同源 ✓**（**不是**这次请求里的入参 ✓）。
          const viewName = (p1LayerOrder(rows).layers.get(layerId) || {}).name || name;
          return json({ ok: true, server: false, reused: true, layer_id: layerId, name: viewName,
                        detail: `图层 ${layerId} 已存在 ⇒ 按 if_exists=reuse **复用**它 ✓（没有新建、也没有报错 ✓）` });
        }
        return p2ToolFailure(409, "precondition_failed", `图层 ${layerId} 已存在`, { layer_id: layerId });
      }
      const aliveCount = [...layers.values()].filter((l) => l.alive).length;
      if (aliveCount >= 256) {
        return p2ToolFailure(409, "precondition_failed",
          `文档 ${doc} 已有 ${aliveCount} 个存活图层（上限 256）⇒ 先删掉不用的图层再建 ✓`, null);
      }
      const seq = await putAtom(handle, doc, atom);
      const out = { ok: true, seq, server: false, layer_id: layerId, name,
                    layer_type: atom.payload.type || "raster",
                    note: "已在本地原子日志写入 create_layer（内核重放时建层）" };
      return json(out);
    }

    // **★ `/api/tools/update_layer` ✗ ★**（2026-10-10 定位 ✓）：
    //   **∴ 为什么缺 ✗**：**本地层只有 `list_layers`／`create_layer` ✗**，**没有 `update_layer` ⇒
    //     **∴ 前端点"显示/隐藏"⇒ 501 `not_implemented_locally` ✓**（**线上实测 ✓）。
    //   **∴ 形状照服务端** ✗**（`tools.rs` 的 `write_update_layer` ✓）：**每个 patch key 写一个
    //     `{kind:"set_property", payload:{layer_id,key,value}}` 原子 ✓ —— **∴ 内核本来就支持折叠 ✓**
    //     （**`fold.rs` 的 `apply_property_to_layer` ✓，**隔离实验验证过 ✓），**∴ 不**自创格式 ✓**** ✓✓
    //   **∴ 合法 key 与服务端 `LAYER_PATCH_KEYS` 对齐**（8 个 ✓）。
    if (path === "/api/tools/update_layer") {
      // **★ 清单必须与服务端 `LAYER_PATCH_KEYS` 同源 ✗ ★**（**`tools.rs:3656` ✓ 10 个键 ✓**）：
      //   **∴ 旧本地少了 `z_index`／`parent_id` ✗** ⇒ 前端**改层级／拖进图层组**时被本地回
      //   「patch 中没有可修改的图层属性」✗（**而服务端能改 ✓ ⇒ 离线模式少两个功能 ✓**）= 功能缺口 ✓。
      const UPDATE_LAYER_KEYS = ["name", "visible", "opacity", "blend_mode",
                                 "locked", "alpha_lock", "clipping_mask", "mask_id",
                                 "z_index", "parent_id"];
      // **混合模式清单取自渲染层 ✓**（**`yanshi-render/src/blend.rs:72 NAMES` ✓ 13 项含别名 ✓**）。
      const P2_BLEND_NAMES = ["normal", "multiply", "screen", "overlay", "darken", "lighten",
                              "add", "linear_dodge", "subtract", "color_dodge", "difference",
                              "linear_light", "soft_light"];
      const objectBody = body && typeof body === "object" && !Array.isArray(body) ? body : {};
      // **① 未知参数 ⇒ 400 ✗**（**照服务端框架 `validate_args` 的文案 ✓**：拼错的键**不许静默忽略** ✓）。
      for (const key of Object.keys(objectBody).sort()) {
        if (P2_PRESET_FRAMEWORK.includes(key)) continue;
        if (key !== "layer_id" && key !== "patch") {
          return p2Bad(`update_layer 不接受参数 ${key}（拼写错误？）；可用参数：layer_id, patch；`
            + `框架级参数：${P2_PRESET_FRAMEWORK.join(", ")}`);
        }
      }
      const layerId = objectBody.layer_id;
      if (typeof layerId !== "string" || !layerId) return p2Bad("缺少必填参数 layer_id");
      const patch = objectBody.patch;
      if (!patch || typeof patch !== "object" || Array.isArray(patch)) {
        return p2Bad("缺少必填参数 patch");
      }
      const rows = await atomsOf(handle, doc);
      const neutral = p2Neutralized(rows);
      const layers = p2PresetLayers(rows, neutral);
      // **② `blend_mode` 必须校验 ✗**（**`tools.rs:3688` ✓**：写进不可变日志后渲染层会**静默退化为 normal** ✗
      //   ⇒ 「接受了却没用」⇒ 所以入口就拒 ✓ 并列出可用值 ✓）。
      if (patch.blend_mode !== undefined) {
        if (typeof patch.blend_mode !== "string") {
          return p2Bad(`blend_mode 必须是字符串 ⇒ 可用：${P2_BLEND_NAMES.join(" / ")}`);
        }
        if (P2_BLEND_NAMES.indexOf(patch.blend_mode) < 0) {
          return p2Bad(`不支持的混合模式 ${patch.blend_mode} ⇒ 可用：${P2_BLEND_NAMES.join(" / ")}`);
        }
      }
      // **③ `parent_id` 三条校验 ✗**（**`tools.rs:3715` ✓**：存在／必须是图层组／不许成环 ✓）。
      if (patch.parent_id !== undefined && patch.parent_id !== null) {
        if (typeof patch.parent_id !== "string") {
          return p2Bad("parent_id 必须是字符串（图层组 id）或 null（移出组）");
        }
        const parent = layers.get(patch.parent_id);
        if (!parent || !parent.alive) {
          return p2ToolFailure(404, "reference_not_found",
            `父图层组 ${patch.parent_id} 不存在`, { layer_id: patch.parent_id });
        }
        const parentType = p2LayerTypeOf(rows, patch.parent_id);
        if (parentType !== "layer_group") {
          return p2Bad(`${patch.parent_id} 的类型是 ${p2PresetTypeNameDebug(parentType)}`
            + " ⇒ parent_id 只能指向 layer_group");
        }
        if (patch.parent_id === layerId) return p2Bad("图层不能以自己为父");
        let cursor = parent.parent, hops = 0;
        while (typeof cursor === "string" && cursor) {
          if (cursor === layerId) return p2Bad(`${patch.parent_id} 已经在 ${layerId} 里面 ⇒ 会成环`);
          hops += 1;
          if (hops > 64) break;
          const up = layers.get(cursor);
          cursor = up ? up.parent : null;
        }
      }
      const keys = UPDATE_LAYER_KEYS.filter((k) => patch[k] !== undefined);
      if (!keys.length) return p2Bad("patch 中没有可修改的图层属性");
      // **④ 提交期：图层必须存在且活着 ✗**（**`log.rs` 的 `set_property` 引用检查 ✓**）。
      //   **∴ 此前本地一个字都不查 ✓** ⇒ 给已删层写属性**回执 ok 而折叠丢掉** ✗（形态③）。
      const guard = p2RefGuard(rows, layers, neutral, "set_property", { layer_id: layerId });
      if (guard) return p2ToolFailure(guard.status, guard.code, guard.detail, guard.extra);
      // **⑤ 先全部校验、再全部写 ✗**（**∴ 与 `update_object` 同一条纪律 ✓**：
      //   服务端的 changeset **不是**原子的（实测 `service.rs:2437` 逐条提交再 `attempt?` ✓）
      //   ⇒ **本地不照抄半写** ✗ ⇒ 一次校验、全过才写 ✓）。
      const seqs = [];
      for (const key of keys) {
        seqs.push(await putAtom(handle, doc, p2Envelope("set_property",
          { layer_id: layerId, key, value: patch[key] })));
      }
      return json({ ok: true, server: false, layer_id: layerId, updated: keys,
                    count: seqs.length, head: seqs[seqs.length - 1] ?? null,
                    changeset_id: "local-" + seqs[0] + "-" + seqs[seqs.length - 1],
                    atoms: seqs.map((seq) => ({ ok: true, seq })),
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
      // **两句话不能混 ✗**（**实测服务端 ✓**：**从未有过 ⇒ 404「图层 X 不存在（现有图层：…）」✓；
      //   已删 ⇒ 409「图层 X 已删除，tombstone 被拒绝」✓**）——
      //   **∴ 旧本地一律 404「不存在或已被删除」✗** ⇒ 调用方**分不清「拼错 id」与「已经删过」** ✓。
      const rows = await atomsOf(handle, doc);
      const neutral = p2Neutralized(rows);
      const layers = p2PresetLayers(rows, neutral);
      const guard = p2RefGuard(rows, layers, neutral, "tombstone", { layer_id: layerId });
      if (guard) return p2ToolFailure(guard.status, guard.code, guard.detail, guard.extra);
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
      // **照 `write_duplicate_layer` 的两句话 ✓**（**`tools.rs:3827`／`:3832` ✓**：
      //   从未有过 ⇒「图层 X 不存在」✓；已删 ⇒「图层 X 已被删除」✓ —— **状态码都是 404 ✓**）。
      //   **∴ 顺序 ✗**：`p1LayerOrder` 会**剔除**墓碑层 ✓ ⇒ 必须先查状态里的「存在但已删」✓。
      {
        const stateDup = p2PresetLayers(rows, p2Neutralized(rows));
        const rec = stateDup.get(layerId);
        if (rec && !rec.alive) return p2Missing("图层 " + layerId + " 已被删除");
      }
      const src = layers.get(layerId);
      if (!src) return p2Missing("图层 " + layerId + " 不存在");
      const newLayerId = body.new_layer_id || pl.new_layer_id || ("layer_" + Date.now().toString(36));
      if (layers.has(newLayerId)) return p2Bad("new_layer_id " + newLayerId + " 已存在");
      const name = body.name || pl.name || (src.name + " 副本");
      let seq = await putAtom(handle, doc, p2Envelope("create_layer", { layer_id: newLayerId, name }));
      let objects = 0;
      for (const r of rows) {
        const a = (r && r.atom) || {};
        const apl = (a && a.payload) || {};
        // **建对象的原子 = 共享集合 ✓**（**不再各自列 ✗**：此前这里只写 `import_image∣∣create_object` ⇒
        //   **`draw_shape`／`fill`／`draw_text` 画的形／字／色块在「复制图层」时被整个丢掉 ✓**
        //   （对象数偏少、副本缺东西 —— **与 `p2ObjectView` 同一类口径 bug ✗ 现在同一处对齐 ✓**）。
        //   **∴ 排除 `supersede`✗**：它引用的是别的创建原子 ✓（净荷多半无 `layer_id` ⇒ 本就匹配不上 ✓）
        //   ⇒ 显式挡掉以防**悬空复制**（复制一条 supersede 却不复制它的 create ✓）。
        if (P2_OBJECT_CREATE_KINDS.has(a.kind) && a.kind !== "supersede" &&
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
      // **★ 形状照前端与服务端 ✗ ★**（本轮 ✓）：前端 `viewer-app.js` 与
      //   服务端 `write_import_image`（`tools_bitmap.rs:152`）都用
      //   **`bitmap:{blob_hash,mime_type,size}` ＋ `region:{x,y,w,h}`** ✓
      //   ⇒ **∴ 而**本分支此前读的是 `body.blob_hash`／`body.x/y`✗ ⇒
      //     **前端发来的 `bitmap.blob_hash` 根本读不到 ⇒ 一律 `missing_blob_hash` ✓**
      //     （**离线导入图片整条路是坏的 ✓**）＋ 即便写成功，净荷缺 `bitmap`/`region`/`width`
      //     ⇒ 渲染器 `parse_raster_patch` 按**不支持的原语跳过 ⇒ 一个像素都不出**（服务端第 1223 轮的 P0 ✓）。
      //   **∴ 修法 ✗**：读**规范形状**（并兼容旧的平铺写法 ✓）＋ 写**渲染器认得的净荷**
      //     （`bitmap.blob_hash` ＋ `width/height` ＋ `region` ✓ 与 `brush_stroke` 那条 import_image 原子同形 ✓）
      const pl = (body && body.payload) || {};
      const bitmap = body.bitmap || pl.bitmap || {};
      const blobHash = bitmap.blob_hash || body.blob_hash || pl.blob_hash;
      if (typeof blobHash !== "string" || !blobHash) {
        return json({ ok: false, error: "missing_blob_hash", error_code: "missing_blob_hash",
                      reason: "import_image 需要 bitmap.blob_hash（先 POST /api/blob 存图再导入）",
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
      // **∴ region ＋ 尺寸 ✗**：**优先规范 `region`✗** 旧平铺 `x/y/width/height` 兼容 ✓
      const region = body.region || pl.region || {};
      const x = Number(region.x ?? body.x ?? pl.x ?? 0);
      const y = Number(region.y ?? body.y ?? pl.y ?? 0);
      const w = Number(region.w ?? body.width ?? pl.width ?? 0);
      const h = Number(region.h ?? body.height ?? pl.height ?? 0);
      // **∴ 尺寸缺失必须报错 ✗**（**不**写一条渲染不出来的原子 ✓ 照服务端 `parse_raster_patch` 的同一条 P0 ✓）
      if (!Number.isFinite(w) || !Number.isFinite(h) || w <= 0 || h <= 0) {
        return json({ ok: false, error: "missing_bitmap_size", error_code: "missing_bitmap_size",
                      reason: "import_image 需要位图尺寸（region.w/h 或 width/height）⇒ 否则渲染器把它当「不支持的原语」跳过 ⇒ 画面什么都不出",
                      server: false }, 400);
      }
      const objectId = body.object_id || pl.object_id ||
                       ("obj_" + Date.now().toString(36));
      const mime = String(bitmap.mime_type || (pl.bitmap && pl.bitmap.mime_type) || "image/x-yanshi-raw");
      // **★ 净荷形状 = 渲染器 `parse_raster_patch` 要的 ✓**：`bitmap.blob_hash` ＋ `width/height` ＋ `region`
      const atom = p2Envelope("import_image", {
        type: "raster_patch",
        bitmap: { blob_hash: blobHash, mime_type: mime, size: bytes.length },
        width: w, height: h,
        region: { x, y, w, h },
        layer_id: layerId,
        object_id: objectId,
      });
      const seq = await putAtom(handle, doc, atom);
      return json({ ok: true, atom_id: atom.id, seq, head: seq, object_id: objectId,
                    blob_hash: blobHash, layer_id: layerId,
                    x, y, width: w, height: h, server: false,
                    note: "已在本地原子日志写入 import_image（净荷含 bitmap＋region＋宽高 ⇒ 渲染器 parse_raster_patch 能取出并落位 ✓）" });
    }

    // **⑥ `get_undo_status` ✗**：**只读查询可撤销／可重做的笔数 ✓**（**照服务端 `read_get_undo_status` ✓**）：
    //   **∴ 与 `undo_last`／`redo_last` 走同一个 `__p2CollectGestures` ✗**（**∴ 服务端注释的原话 ✓：
    //     「undo 与只读状态查询必须报同一个数 ⇒ 各写一份必然漂移」✓**）；
    //   **∴ 判据是对象存活 ✗**（**不是**每原子最新动作 ✓ —— 见 `__p2CollectGestures` 的教训注 ✓）；
    //   **∴ `remaining_gestures`／`ignored_atoms` 与服务端同名同义 ✓**（**服务端只给这两个 ✓**；
    //     `undoable_gestures`／`redoable_gestures` 是本地层的便利字段 ✗，前端读它们 ✓）。
    if (path === "/api/tools/get_undo_status") {
      const rows = await atomsOf(handle, doc);
      const undoCollect = __p2CollectGestures(rows, true);
      const redoCollect = __p2CollectGestures(rows, false);
      return json({ ok: true,
                    undoable_gestures: undoCollect.gestures.length,
                    redoable_gestures: redoCollect.gestures.length,
                    remaining_gestures: undoCollect.gestures.length,
                    ignored_atoms: undoCollect.ignored, server: false,
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
    //   **∴ 提交期的两条拒绝也要照抄 ✓**（**`log.rs:1164 check_object_exists` ✓**）：
    //   不存在 ⇒ 404「对象 X 不存在（现有对象：…）」✓；已 tombstone ⇒ 409「对象 X 已被 tombstone，
    //   tombstone 被拒绝」✓。**此前本地一律写成功** ✗ ⇒ 重复删除/删不存在的 id **回执 ok 而状态没变** ✓
    //   （**形态 3 ✓ 且第二次删还会多留一条无用原子 ✗**）。
    if (path === "/api/tools/delete_object") {
      const objectId = body.object_id;
      if (typeof objectId !== "string" || !objectId) return p2Bad("object_id 必填（字符串）");
      const rows = await atomsOf(handle, doc);
      const neutral = p2Neutralized(rows);
      const guard = p2RefGuard(rows, p2PresetLayers(rows, neutral), neutral, "tombstone",
                               { object_id: objectId });
      if (guard) return p2ToolFailure(guard.status, guard.code, guard.detail, guard.extra);
      const seq = await putAtom(handle, doc, p2Envelope("tombstone", { object_id: objectId }));
      return json({ ok: true, seq, server: false, object_id: objectId,
                    note: "已在本地原子日志写入 tombstone（对象删除语义）" });
    }

    // **⑪ `update_object` ✗**：**照 `write_update_object` ✓**
    //   ⇒ **`patch` 里每个合法键各写一条 `set_property` 原子 ✗**
    //   （**只认 7 个键 ✓**：`visible`／`enabled`／`locked`／`z_index`／`layer_id`／`metadata`／`type` ✓）。
    if (path === "/api/tools/update_object") {
      const objectId = body.object_id;
      if (typeof objectId !== "string" || !objectId) return p2Bad("object_id 必填（字符串）");
      const patch = body.patch;
      if (!patch || typeof patch !== "object") return p2Bad("patch 必填（对象）");
      // **7 个键与服务端 `OBJECT_PATCH_KEYS` 同源 ✗**（**`tools.rs:5758` ✓**：旧本地漏了 `enabled` ✓）。
      const KEYS = ["visible", "enabled", "locked", "z_index", "layer_id", "metadata", "type"];
      const hits = KEYS.filter((k) => patch[k] !== undefined);
      if (!hits.length) return p2Bad("patch 中没有可修改的对象属性");
      // **★ 服务端的 changeset 其实**不是**原子的（实测 ✓，第 2 批发现）** ✗：
      //   `service.rs:2437 commit_changeset_timed` 是**逐条** `commit_as_timed(…)` 再 `attempt?`
      //   ⇒ **第 2 个键被拒时第 1 个键已经落盘并 journal 了** ✓（实测：`{visible:false, layer_id:"NOPE"}`
      //   回 404，而 `list_objects{include_hidden:true}` 里 `visible` 已变 `false` ✗ ⇒ 部分写入 ＋ 报错 ✓）。
      //   **∴ 本地不照抄那个坏行为 ✓**：**先全部校验再全部写** ✓（**⇒ 要么全落要么全不落** ✓）——
      //   与「死父层照 `missing_reference` 拒绝」同一条纪律：**服务端静默半写 ⇒ 本地响亮拒绝** ✓。
      //   **∴ 判据要把它当成「已记录的**分歧**」来断言 ✓**（服务端 `visible=false`／本地 `true`），
      //   而不是当「两边应相等」✗ —— 否则这条判据会逼本地去复制一个缺陷 ✓。
      const rows = await atomsOf(handle, doc);
      const neutral = p2Neutralized(rows);
      const layers = p2PresetLayers(rows, neutral);
      for (const k of hits) {
        const guard = p2RefGuard(rows, layers, neutral, "set_property",
                                 { object_id: objectId, key: k, value: patch[k] });
        if (guard) return p2ToolFailure(guard.status, guard.code, guard.detail, guard.extra);
        // **★ 锁闸也**先全查再写**✗**：**∴ 若边写边查，第 2 个键被锁拒时第 1 个键已落盘 ✓
        //   ⇒ **正是本地刻意避免的「半写」** ✓（`set_property{visible}` 在锁对象上合法、
        //   `{z_index}` 不合法 ⇒ 混在一只 patch 里就是这个形状 ✓）。
        const lock = p2LockGuard(rows, { kind: "set_property",
          payload: { object_id: objectId, key: k, value: patch[k] } });
        if (lock) return p2ToolFailure(lock.status, lock.code, lock.detail, lock.extra);
      }
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
                    // **∴ `created_by`／`deleted_by` ✗**（**∴ 照服务端 `get_object` 的字段 ✓**
                    //   （**`tools.rs:3188` ✓**）—— **死因与出身分开报 ✗ 才能把「创建被抑制」与
                    //   「tombstone 生效」两种「死」区分开 ✓**）。
                    created_by: v.__create_id, deleted_by: v.__deleted_by,
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
      if (typeof instanceId !== "string" || !instanceId) return p2Bad("缺少必填参数 instance_id");
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

    // **㉓ `detach_instance` ✗**：**照 `write_detach_instance` ✓** —— **∴ 两步：建副本 ✓ ＋ 删实例 ✓**
    //   ⇒ **∴ 都带**同一个 `changeset_id` ✓**（**∴ 本地**也有**变更集：`revert_changeset` 按封套上的
    //     `changeset_id` 撤销 ✓** —— **∴ 上一版说「本地层无 changeset」是**过期**的 ✗**）
    //   ⇒ **∴ 一把撤掉两步 ✓**，**但**不会自动回滚 ✗**（**与服务端同形 ✓，如实写在回执里 ✓**）。
    //   **∴ 变换从日志折叠 ✓**（`p2ObjectView` 现在重放 `move`／`transform` ✓）：
    //     **副本变换 = 实例自身 ∘ local_transform ∘ master 自身 ✓**（**与渲染同序 ✓**）；
    //     **副本数据 = master 的折叠 `data` ✓**；**`type` 取 `data.type`，缺省 `shape` ✓**
    //     （**照服务端 `master_data.get("type")` ✓，**不是** master 顶层的 `type` ✗**）。
    //   **∴ 错误口径逐条照抄 ✓**（**存在性看「创建未被撤」✗ —— 墓碑对象仍在状态里 ✓**）。
    if (path === "/api/tools/detach_instance") {
      const instanceId = body.instance_id;
      if (typeof instanceId !== "string" || !instanceId) return p2Bad("缺少必填参数 instance_id");
      const rows = await atomsOf(handle, doc);
      const neutral = p2Neutralized(rows);
      const iv = p2ObjectView(rows, instanceId);
      // **「在状态里」= 创建未被 revert ✗**（**折叠把对象整个拿掉 ✓**）；**墓碑对象仍在状态里 ✓**
      //   ⇒ **∴ 已删的实例也能脱离 ✓**（**与服务端 `state.objects.get` 同形 ✓**）。
      if (!iv.found || neutral.has(iv.__create_id)) {
        return p2Missing("实例 " + instanceId + " 不存在");
      }
      if (iv.type !== "instance") return p2Bad(instanceId + " 不是实例");
      // **`master_ref` 取**折叠后**的那个 ✗**（**∴ 服务端读 `object.data.master_ref` ✓，
      //   而 `set_property{key:"master_ref"}` 会**整体替换**它 ✓ —— **∴ 只读创建净荷 ✗**
      //   会看不见替换 ✓ ⇒ 「缺 object_id」这条校验**永远不触发** ✓（§C⑤ 实测）。
      const ref = iv.master_ref;
      const masterId = ref && typeof ref.object_id === "string" ? ref.object_id : null;
      if (!masterId) return p2Bad("实例缺少 master_ref.object_id");
      const mv = p2ObjectView(rows, masterId);
      if (!mv.found || neutral.has(mv.__create_id)) {
        return p2Missing("master " + masterId + " 不存在，无法脱离");
      }
      // **master 已删 ⇒ 409 ✓**（**`PreconditionFailed` ⇒ 服务端 HTTP 409 ✓**）；
      //   **∴ 用折叠口径判死 ✗**（**自己墓碑 ＋ 随层删 ✓，`fold.rs:1988` 给层里每个对象写 `deleted_by` ✓**）。
      if (p2ObjectDeath(p2PresetLayers(rows, neutral), mv).deleted) {
        return p2Precon("master " + masterId + " 已被删除，无法脱离");
      }
      // **★ 副本 data 必须是**折叠后**的那份 ✗（**2026-10-28 二期复核**）：**服务端 `tools.rs:6325`
      //   复制的是 `master.data.clone()` ✓ —— 那是**状态**里的 data ⇒ **已含** `set_property`
      //   写进 `object.data` 的键 ✓（`apply_property_to_object` 的 `other => data.insert` ✓）。
      //   **∴ 只读 `mv.data` ✗**（创建／supersede 净荷）⇒ master 被 set_property 过就分裂 ✓
      //   —— **正是 `p2EffectiveData` 头注点名的「凡按 data 解析语义的地方都必须经这里」**。
      const meff = p2EffectiveData(mv);
      const local = ref.local_transform ? p2TransformFromJson(ref.local_transform)
                                        : { matrix: [1, 0, 0, 1, 0, 0], pivot: [0, 0] };
      const composed = p2ComposeTransforms(iv.transform, p2ComposeTransforms(local, mv.transform));
      const newId = (typeof body.object_id === "string" && body.object_id)
        ? body.object_id : "detached_" + p2Ulid();
      const newType = (meff.type !== undefined) ? meff.type : "shape";
      const changesetId = p2Ulid();
      // **★ 建副本这一步也要过引用校验 ✗**（**服务端 `log.rs:100` 对 `create_object` 是
      //   `MustNotExist` ✓ ＋ `layer_ref_mode` 默认 `MustExist` ✓**，顺序在锁之前 ✗）。
      //   **∴ 此前本地只有 `p2LockGuard`（只管锁）⇒ 显式 `object_id` 撞活对象 / 层不存在时
      //   照写不误 ✓** 而后一条 `create_object` 被**折叠层丢弃**（`fold.rs:551`）⇒
      //   **回执 200、状态里没有副本 ＝ 静默降级形态③ ✓**（服务端同请求是 409/404 ✗）。
      const createGuard = p2RefGuard(rows, p2PresetLayers(rows, neutral), neutral,
                                     "create_object",
                                     { object_id: newId, layer_id: iv.layer_id });
      if (createGuard) {
        return p2ToolFailure(createGuard.status, createGuard.code, createGuard.detail, createGuard.extra);
      }
      const createAtom = p2Envelope("create_object", {
        object_id: newId, layer_id: iv.layer_id, type: newType,
        data: meff, transform: { matrix: composed.matrix, pivot: composed.pivot },
      });
      createAtom.changeset_id = changesetId;
      const created = await putAtom(handle, doc, createAtom);
      // **★ 第二步照服务端的提交期校验 ✓ ★**：**`Tombstone` 对已删对象 ⇒ 409「对象 X 已被 tombstone，
      //   tombstone 被拒绝」✓**（**`log.rs:1179` ✓**）。
      //   **∴ 次序必须与服务器相同 ✗**：**先建副本、再被拒** ⇒ 服务端**会留下 D4** ✓
      //   （**实测：409 之后 `get_object D4` 仍是 200 ✓**）⇒ **∴ 本地也照抄这个半成品 ✓ ——
      //   不是「本地新增的行为」✗，而是「同一个请求在两边留下同样的状态」✓**。
      const tombPayload = { object_id: instanceId };
      const tombGuard = p2RefGuard(rows, p2PresetLayers(rows, neutral), neutral,
                                   "tombstone", tombPayload);
      if (tombGuard) {
        return p2ToolFailure(tombGuard.status, tombGuard.code, tombGuard.detail, tombGuard.extra);
      }
      const tombAtom = p2Envelope("tombstone", tombPayload);
      tombAtom.changeset_id = changesetId;
      const removed = await putAtom(handle, doc, tombAtom);
      return json({ ok: true, detached_id: newId, removed_instance: instanceId,
                    changeset_id: changesetId, head: removed, created_head: created,
                    server: false,
                    note: "副本数据取自 master 的折叠 data，变换 = 实例自身 ∘ local ∘ master；" +
                          "两步同属一个变更集（revert_changeset 可整体撤销，但不会自动回滚）" });
    }

    // **㉔ `link_to_master` ✗**：**照 `write_link_to_master` ✓**
    //   ⇒ **`set_property{key:"master_ref", value:{object_id, local_transform}}` ✗**
    //   （**∴ 保留原 `local_transform` ✗**，**只换指向 ✓**）。
    //   **∴ 校验从日志推导 ✓**：**实例必须存在且是实例 ✗**；**master 必须存在 ✗**。
    if (path === "/api/tools/link_to_master") {
      const instanceId = body.instance_id, masterId = body.master_id;
      if (typeof instanceId !== "string" || !instanceId) return p2Bad("缺少必填参数 instance_id");
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
    //   **∴ 真要服务端能力的**（`import_psd` 的 PSD 解析 ✗／
    //     `medium_stroke` 的介质插件宿主 ✗／`set_brush_dynamics` 的服务端笔刷文件缓存 ✗）⇒ **∴ 明确 501 ✓**；
    //     **`estimate_dehaze` 已不再是 501 ✓**（第 六百六十二 轮起本地按 f32 逐位移植暗通道 ✓）。
    //   **∴ `archive_base64` 不是端点 ✗**（**它是 `export_png` 响应里的字段 ✓** ⇒ **∴ 本地 `export_png` 照样返回它 ✓**）；
    //   **∴ `push` 在服务端工具注册表里不存在 ✗**（**∴ 用 `p2NoServerEndpoint` 明确 501 ✓**）。
    //   **∴ 口径**：**真实映射的分支计入 `LOCAL_IMPLEMENTED` ✗**；**明确 501 的不计入 ✓**。
    const p2cKnownKinds = ["create_document","create_layer","create_object","create_selection",
      "create_mask","create_style","create_checkpoint","import_image","draw_stroke","fill",
      "draw_shape","draw_text","erase","retouch","liquify","supersede","move","transform",
      "set_property","reorder_layers","tombstone","revert","reapply","checkpoint","tag",
      "declare_head","comment","suggest","accept_suggestion","reject_suggestion"];
    // （**`__p2cCtr` 已提升到 `makeLocalApi` 层 ✗ ⇒ **这里**不再**每请求重新声明 ✓**）
    function p2cNewObjectId() {
      __p2cCtr += 1;
      // **∴ 分隔符必须有 ✗**（**2026-10-28 实测：无分隔时**时间戳尾字符**可以**等于计数器字符**
      //   ⇒ 同毫秒两笔生成**同一个 id**（`obj_mvrx4vjm1` ×2）⇒ 原子日志**互相覆盖** ✗
      //   ⇒ `tool-pwa-api-behaviour` ⑦l 抖动红 ✓。服务端用 ULID ⇒ **天然无此歧义 ✓**）
      return "obj_" + Date.now().toString(36) + "_" + __p2cCtr.toString(36);
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
        // **★ 走可注入的 `__loaders.wasm()` ✗ ★**（与 `/api/blob`／落笔分支同一条路 ✓）：
        //   **∴ 此前这里**绕过接缝**直接 import ✗** ⇒ **∴ 于是**注入假内核或预初始化的真内核
        //   （`initSync`）**都到不了这条路 ✗** ⇒ node 判据里**每个要像素的端点都只能 501** ✗。
        mod = await __loaders.wasm();
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
    /** **内核渲染一块区域为**裸 RGBA8**✗**（**第 六百六十二 轮 ✓**）。
     *
     *  **∴ 为什么要有它 ✗**：`analyze_region`／`estimate_dehaze` 在服务端走的是
     *  `render_region_raw`（**裸像素 ✓**），而本地只有 PNG 路线 ✗
     *  ⇒ **∴ 于是**统计只能靠浏览器解码 ✗（`createImageBitmap` ✓）⇒ **∴ node 判据无从验证 ✗**。
     *  内核**本来就导出** `render_region_rgba` ✓ ⇒ **∴ 直接用 ✓**（**同一次渲染 ✓**）。
     *
     *  **∴ 区域的裁剪口径**照服务端 `clamp_region` ✗（**不是**内核自己的 `clamp_to_canvas` ✓）：
     *  **∴ 两者在**负坐标**上不一致 ✗**（服务端 `x0 = floor(x).max(0)` ＋ `x1 = ceil(x+w).min(宽)`
     *  ⇒ **∴ 区域 `x=-3,w=10` 只覆盖 0..7（7 列 ✓）**；而内核把 `x` 挪到 0 却**保留 w=10** ⇒ 10 列 ✗）
     *  ⇒ **∴ 所以**这里**先按服务端口径裁剪**✗、再把**整数盒**交给内核 ✓
     *  ⇒ **∴ 内核的裁剪就成了恒等 ✓ ⇒ 宽高与像素都同服务端 ✓**。
     *  与服务端同样：**不相交 ⇒ `invalid_argument`**（**∴ 不给假统计 ✓**）。
     */
    async function p2cRenderRgba(x, y, w, h, extraAtoms = null) {
      const rows = await atomsOf(handle, doc);
      const { w: cw, h: ch } = p2cDocSize(rows);
      const x0 = Math.max(0, Math.floor(x));
      const y0 = Math.max(0, Math.floor(y));
      const x1 = Math.min(Math.ceil(x + w), cw);
      const y1 = Math.min(Math.ceil(y + h), ch);
      if (x1 <= x0 || y1 <= y0) {
        // **∴ 服务端是 `{bbox:?}`（Rust Debug）✗** ⇒ **∴ 小数位也照它 ✓**
        const dv = (v) => (Number.isInteger(v) ? v.toFixed(1) : String(v));
        return { error: "渲染区域 Bbox { x: " + dv(x) + ", y: " + dv(y) +
                        ", w: " + dv(w) + ", h: " + dv(h) + " } 与文档 " + cw + "×" + ch + " 不相交" };
      }
      const bw = x1 - x0, bh = y1 - y0;
      const { entry } = await p2cKernelFor();
      return await withKernel(doc, async () => {
        const k = entry.k;
        const atoms = await atomsOf(handle, doc);
        // **∴ `extraAtoms` 只喂给这一次 `load_atoms_json` ✗**（**不落盘 ✓**）：
        //   **图层作用域的「只看这一层」靠它 ✓**（见 `p2LayerScopeAtoms` ✓）。
        const loaded = atoms.map((r) => r.atom);
        if (Array.isArray(extraAtoms)) loaded.push(...extraAtoms);
        const fold = String(k.load_atoms_json(JSON.stringify(loaded)) || "");
        let rgba;
        try {
          rgba = k.render_region_rgba(x0, y0, bw, bh);
        } finally {
          // **∴ 合成原子只属于这一次渲染 ✗**（第 六百六十五 轮实测）：
          //   内核的增量折叠把「head_seq 相同」当成「日志没变」✓（`seq.rs` 的
          //   `last.seq == n` 直接回旧状态 ✓）⇒ **∴ 两次图层作用域若合成原子段
          //   落在同一段 seq 上 ⇒ 后一次会拿到前一次的状态 ✗**
          //   （实测：L2 的蓝矩形被当成上一次 L3 的空白、L5 的墓碑层混进 H1 的黄色 ✗）
          //   ⇒ **∴ 渲完必须折回权威日志 ✓** ⇒ 下一次合成从干净状态前推 ✓
          //   ⇒ **∴ 折回失败要报错 ✗**（**内核状态已被污染 ⇒ 不许静默 ✓**）。
          if (Array.isArray(extraAtoms)) {
            const back = String(k.load_atoms_json(JSON.stringify(atoms.map((r) => r.atom))) || "");
            let ok = false;
            try { ok = JSON.parse(back).ok === true; } catch (e) { ok = false; }
            if (!ok) throw new Error("合成原子渲染后无法折回权威日志（内核状态已污染）：" + back.slice(0, 200));
          }
        }
        const need = bw * bh * 4;
        if (!rgba || rgba.length < need) {
          throw new Error("内核未产出裸像素（got=" + (rgba ? rgba.length : 0) +
                          "，need=" + need + "；fold=" + fold.slice(0, 200) + "）");
        }
        return { rgba: rgba instanceof Uint8Array ? rgba : new Uint8Array(rgba),
                 w: bw, h: bh, fold };
      });
    }

    /** **图层作用域的合成原子 ✗**（**本地镜像服务端 `set_only_layer` ✓**，第 六百六十五 轮 ✓）。
     *
     *  **∴ 为什么不是「直接渲染这一层」✗**：**内核没有单层渲染通道 ✗**
     *  （`render_region_rgba` 只给整幅合成 ✓）⇒ **∴ 换成「把别的层都藏起来」✓**
     *  —— **服务端探针实测：它与 `set_only_layer` 逐像素相同 ✓**
     *  （覆盖：普通层／纯调整层／隐藏层／已删层／裁剪层／组内层 ✓）。
     *
     *  **∴ 三处必须补偿 ✗**（**都是 `set_only_layer` 与普通合成的语义差 ✓**）：
     *    ① **只认这一层、不看它自己的可见性** ⇒ **∴ 补 `visible:true` ✓**；
     *    ② **不继承组可见性／组不透明度** ⇒ **∴ 把父链断开 ✓**（`parent_id:null` ✓；
     *       探针实测：不断开时差在 `r[220]` ✓，断开后逐位相同 ✓）；
     *    ③ **纯调整层不播种下方内容** ⇒ **∴ 补 `clipping_mask:true` ✓**（播种被这条关掉 ✓；
     *       空缓冲上的调整 alpha 仍是 0 ⇒ 乘下方 alpha 是恒等 ✓；invert／blur／
     *       film_grain／glow／noise 探针逐位相同 ✓）。
     *
     *  **∴ 只影响这一次装载 ✗**：返回的原子**不落盘** ✓（`p2cRenderRgba` 的 `extraAtoms` ✓）。
     */
    function p2LayerScopeAtoms(rows, props, targetId) {
      const extra = [];
      let maxSeq = 0;
      for (const r of rows) if (typeof r.seq === "number" && r.seq > maxSeq) maxSeq = r.seq;
      const push = (payload) => {
        const atom = p2Envelope("set_property", payload);
        atom.seq = ++maxSeq; // 合成原子必须带权威序号 ✓（`AtomLog::with_atoms` 拒收 seq=0 ✓）
        extra.push(atom);
      };
      const target = props.get(targetId);
      if (!target || !target.alive) {
        // 已删除的层不在 `alive_layers()` 里 ⇒ 服务端只画背景 ✓ ⇒ 把所有活层藏掉 ✓
        for (const [id, p] of props) if (p.alive) push({ layer_id: id, key: "visible", value: false });
        return extra;
      }
      for (const [id, p] of props) {
        if (p.alive && id !== targetId) push({ layer_id: id, key: "visible", value: false });
      }
      push({ layer_id: targetId, key: "visible", value: true });
      if (target.parent_id) push({ layer_id: targetId, key: "parent_id", value: null });
      // **纯调整层：照 `layer_is_adjustment_only` 的定义 ✓**（只数**可见且启用**的对象 ✓）。
      let any = false;
      let only = true;
      for (const o of p2ObjectSummaries(rows, { layerFilter: targetId, includeHidden: true })) {
        if (!o.visible || !o.enabled) continue;
        if (o.type === "adjustment" || o.type === "filter") any = true;
        else { only = false; break; }
      }
      if (any && only && !target.clipping_mask) {
        push({ layer_id: targetId, key: "clipping_mask", value: true });
      }
      return extra;
    }
    /** **直方图 · 每通道 256 桶 ✗**（**照 `yanshi-render/src/histogram.rs` 逐字 ✓**）：
     *   **∴ 亮度是整数 Rec.601 ✓** ⇒ **∴ 跨平台逐位一致 ✓**（不用浮点舍入当判据 ✓）。
     */
    function p2cHistogramRgba8(rgba) {
      const r = new Array(256).fill(0), g = new Array(256).fill(0);
      const b = new Array(256).fill(0), a = new Array(256).fill(0);
      const luma = new Array(256).fill(0);
      let count = 0;
      for (let i = 0; i + 4 <= rgba.length; i += 4) {
        const pr = rgba[i], pg = rgba[i + 1], pb = rgba[i + 2], pa = rgba[i + 3];
        r[pr] += 1; g[pg] += 1; b[pb] += 1; a[pa] += 1;
        luma[Math.floor((299 * pr + 587 * pg + 114 * pb + 500) / 1000)] += 1;
        count += 1;
      }
      return { r, g, b, a, luma, count };
    }
    /** **下分位口径 ✓**（**照 `histogram::percentile` 逐字 ✓**）：
     *   **∴ 最小的、累计 ≥ ⌈percent/100·count⌉ 的桶 ✓**。
     */
    function p2cHistPercentile(buckets, count, percent) {
      if (count === 0 || percent === 0 || percent > 100) return 0;
      const rank = Math.ceil((percent * count) / 100);
      let cumulative = 0;
      for (let value = 0; value < 256; value++) {
        cumulative += buckets[value];
        if (cumulative >= rank) return value;
      }
      return 255;
    }
    /** **摘要 ✓**（**照 `histogram::summarize` 逐字 ✓**：均值／极值／p5／中位数／p95 ✓）。 */
    function p2cHistSummary(buckets, count) {
      if (count === 0) return { mean: 0, min: 0, max: 0, p5: 0, median: 0, p95: 0 };
      let weighted = 0;
      let min = 0;
      let max = 0;
      let seen = false;
      for (let value = 0; value < 256; value++) {
        const n = buckets[value];
        if (n === 0) continue;
        if (!seen) { min = value; seen = true; }
        max = value;
        weighted += value * n;
      }
      return {
        mean: weighted / count, min, max,
        p5: p2cHistPercentile(buckets, count, 5),
        median: p2cHistPercentile(buckets, count, 50),
        p95: p2cHistPercentile(buckets, count, 95),
      };
    }

    /** **`region` 解析 ✗**（**照服务端 `parse_bbox` ✓** ）：
     *  `{x,y,w|width,h|height}` 或 `[x,y,w,h]` ✓；读不出 ⇒ `null`（由调用方报错 ✓）。
     *  **∴ `w` 在但非法时不回退到 `width`**✗（**服务端 `.as_f64()?` 就是直接失败 ✓**）。
     */
    function p2cParseRegion(value) {
      const num = (v) => (typeof v === "number" && Number.isFinite(v) ? v : null);
      if (value && typeof value === "object" && !Array.isArray(value)) {
        const x = num(value.x), y = num(value.y);
        const w = num(value.w !== undefined ? value.w : value.width);
        const h = num(value.h !== undefined ? value.h : value.height);
        return (x === null || y === null || w === null || h === null) ? null : { x, y, w, h };
      }
      if (Array.isArray(value) && value.length === 4) {
        const nums = value.filter((v) => typeof v === "number" && Number.isFinite(v));
        if (nums.length === 4) return { x: nums[0], y: nums[1], w: nums[2], h: nums[3] };
      }
      return null;
    }

    /** **CRC32 表 ✗**（PNG 每块都要校验 ✓；**与服务端 `crc32_update` 同一多项式 ✓**）。 */
    let p2cCrcTable = null;
    function p2cCrc32(crc, bytes) {
      if (!p2cCrcTable) {
        p2cCrcTable = new Uint32Array(256);
        for (let n = 0; n < 256; n++) {
          let c = n;
          for (let k = 0; k < 8; k++) c = (c & 1) ? (0xEDB88320 ^ (c >>> 1)) : (c >>> 1);
          p2cCrcTable[n] = c >>> 0;
        }
      }
      let c = crc >>> 0;
      for (let i = 0; i < bytes.length; i++) {
        c = (p2cCrcTable[(c ^ bytes[i]) & 0xFF] ^ (c >>> 8)) >>> 0;
      }
      return c >>> 0;
    }

    /** **zlib 解压（带输出上限 ✓）✗**：`DecompressionStream("deflate")` 收的是**带 zlib 头**的流 ✓
     *  （**与 PNG 的 IDAT 一致 ✓**）。**∴ 上限 `cap`** ✗：**解压炸弹**在**长度检查之前**就吃内存 ✗
     *  ⇒ **∴ 超一个字节就取消读取 ✓**（**与服务端 `Take(expected+1)` 同一口径 ✓**）。
     */
    async function p2cInflateCapped(bytes, cap) {
      if (typeof DecompressionStream !== "function" || typeof Blob !== "function") {
        throw new Error("PNG 解码需要 DecompressionStream（当前环境没有）");
      }
      const stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream("deflate"));
      const reader = stream.getReader();
      const chunks = [];
      let total = 0;
      try {
        for (;;) {
          const { value, done } = await reader.read();
          if (done) break;
          total += value.length;
          if (total > cap) {
            try { await reader.cancel(); } catch (e) {}
            throw new Error("解压输出超出声明长度");
          }
          chunks.push(value);
        }
      } finally {
        try { reader.releaseLock(); } catch (e) {}
      }
      const out = new Uint8Array(total);
      let at = 0;
      for (const chunk of chunks) { out.set(chunk, at); at += chunk.length; }
      return out;
    }

    /** **PNG 解码（纯 JS）✗**：**接受集合与服务端 `decode_png` **逐条一致 ✓**（第 六百六十二 轮 ✓）。
     *
     *  **∴ 为什么不复用 `createImageBitmap` ✗**：那条路**接受容器格式**（调色板／16 位／隔行 ✓）
     *  ⇒ **∴ 同一张参考图**在浏览器里能比 ΔE ✗ 而服务端说「不是能解码的 PNG」✗
     *  ⇒ **∴ 两处结论不一致 ✓**（**这正是"同一件事两套判定"的老毛病 ✓**）。
     *  **∴ 范围**：8 位、无隔行、颜色类型 6（RGBA ✓）／2（RGB ⇒ 补 alpha=255 ✓）；
     *  逐块**校验 CRC32** ✓；IDAT 里的 zlib 流**显式检查** CM=8 且无 FDICT ✓；
     *  解压输出**恰等于** `(stride+1)*height` ✓；filter 0–4 全支持 ✓。其余 ⇒ `null` ✓。
     */
    async function p2cDecodePngStrict(bytes) {
      const u8 = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes || []);
      const SIG = [137, 80, 78, 71, 13, 10, 26, 10];
      if (u8.length < 8) return null;
      for (let i = 0; i < 8; i++) if (u8[i] !== SIG[i]) return null;
      const be32 = (a, o) => ((a[o] << 24) | (a[o + 1] << 16) | (a[o + 2] << 8) | a[o + 3]) >>> 0;
      const MAX_EDGE = 65535, MAX_PIXELS = 268435456, MAX_IDAT = 256 * 1024 * 1024;
      let pos = 8, width = 0, height = 0, bitDepth = 0, colorType = 0, interlace = 0;
      let seenHeader = false, idatLen = 0;
      const idatParts = [];
      for (;;) {
        if (pos + 8 > u8.length) return null;
        const length = be32(u8, pos);
        const kind = String.fromCharCode(u8[pos + 4], u8[pos + 5], u8[pos + 6], u8[pos + 7]);
        const start = pos + 8, end = start + length;
        if (end + 4 > u8.length) return null;
        const payload = u8.subarray(start, end);
        const expected = be32(u8, end);
        let crc = p2cCrc32(0xFFFFFFFF, [u8[pos + 4], u8[pos + 5], u8[pos + 6], u8[pos + 7]]);
        crc = p2cCrc32(crc, payload);
        if ((crc ^ 0xFFFFFFFF) >>> 0 !== expected) return null;
        if (kind === "IHDR") {
          if (length < 13) return null;
          width = be32(payload, 0); height = be32(payload, 4);
          bitDepth = payload[8]; colorType = payload[9]; interlace = payload[12];
          if (width === 0 || height === 0) return null;
          if (width > MAX_EDGE || height > MAX_EDGE) return null;
          if (width * height > MAX_PIXELS) return null;
          seenHeader = true;
        } else if (kind === "IDAT") {
          idatLen += length;
          if (idatLen > MAX_IDAT) return null;
          if (length) idatParts.push(payload.slice());
        } else if (kind === "IEND") {
          break;
        }
        pos = end + 4;
      }
      if (!seenHeader || idatLen === 0) return null;
      if (bitDepth !== 8 || interlace !== 0) return null;
      const channels = colorType === 6 ? 4 : colorType === 2 ? 3 : 0;
      if (!channels) return null;
      const idat = new Uint8Array(idatLen);
      let at = 0;
      for (const part of idatParts) { idat.set(part, at); at += part.length; }
      if (idat.length < 6 || (idat[0] & 0x0F) !== 8) return null;
      if ((idat[1] & 0x20) !== 0) return null;
      const stride = width * channels;
      const expectedLen = (stride + 1) * height;
      let raw;
      try { raw = await p2cInflateCapped(idat, expectedLen + 1); } catch (e) { return null; }
      if (!raw || raw.length !== expectedLen) return null;
      const out = new Uint8Array(width * height * channels);
      let prev = new Uint8Array(stride);
      let line = new Uint8Array(stride);
      for (let row = 0; row < height; row++) {
        const base = row * (stride + 1);
        const filter = raw[base];
        const src = raw.subarray(base + 1, base + 1 + stride);
        if (filter === 0) {
          line.set(src);
        } else if (filter === 1) {
          for (let i = 0; i < channels; i++) line[i] = src[i];
          for (let i = channels; i < stride; i++) line[i] = (src[i] + line[i - channels]) & 255;
        } else if (filter === 2) {
          for (let i = 0; i < stride; i++) line[i] = (src[i] + prev[i]) & 255;
        } else if (filter === 3) {
          for (let i = 0; i < channels; i++) line[i] = (src[i] + (prev[i] >> 1)) & 255;
          for (let i = channels; i < stride; i++) {
            line[i] = (src[i] + ((line[i - channels] + prev[i]) >> 1)) & 255;
          }
        } else if (filter === 4) {
          for (let i = 0; i < channels; i++) line[i] = (src[i] + prev[i]) & 255;
          for (let i = channels; i < stride; i++) {
            const a = line[i - channels], b = prev[i], c = prev[i - channels];
            const p = a + b - c;
            const pa = Math.abs(p - a), pb = Math.abs(p - b), pc = Math.abs(p - c);
            const pred = (pa <= pb && pa <= pc) ? a : (pb <= pc ? b : c);
            line[i] = (src[i] + pred) & 255;
          }
        } else {
          return null;
        }
        out.set(line, row * stride);
        const swap = prev; prev = line; line = swap;
      }
      if (channels === 4) return { w: width, h: height, data: out };
      const rgba = new Uint8Array(width * height * 4);
      for (let i = 0, j = 0; i < out.length; i += 3, j += 4) {
        rgba[j] = out[i]; rgba[j + 1] = out[i + 1]; rgba[j + 2] = out[i + 2]; rgba[j + 3] = 255;
      }
      return { w: width, h: height, data: rgba };
    }

    /** **sRGB 字节 → 线性光 ✗**（**照 `color::srgb_to_linear` 的 **f32** 语义 ✓**：
     *  **∴ 每一步 `Math.fround`** ✗ —— **否则** `delta_e` 的末几位与 Rust 不同 ✓）。
     */
    let p2cSrgbLut = null;
    function p2cByteToLinear(byte) {
      if (!p2cSrgbLut) {
        p2cSrgbLut = new Float32Array(256);
        const fr = Math.fround;
        for (let i = 0; i < 256; i++) {
          const v = fr(fr(i) / 255);
          p2cSrgbLut[i] = (v <= fr(0.04045))
            ? fr(v / fr(12.92))
            : fr(Math.pow(fr(fr(v + fr(0.055)) / fr(1.055)), fr(2.4)));
        }
      }
      return p2cSrgbLut[byte];
    }

    /** **线性 RGB → CIE Lab ✗**（**照 `color::linear_to_lab` 的 f32 语义 ✓**）。 */
    function p2cLinearToLab(r, g, b) {
      const fr = Math.fround;
      const x = fr(fr(fr(fr(0.4124) * r) + fr(fr(0.3576) * g)) + fr(fr(0.1805) * b));
      const y = fr(fr(fr(fr(0.2126) * r) + fr(fr(0.7152) * g)) + fr(fr(0.0722) * b));
      const z = fr(fr(fr(fr(0.0193) * r) + fr(fr(0.1192) * g)) + fr(fr(0.9505) * b));
      const xn = fr(0.95047), yn = fr(1.0), zn = fr(1.08883);
      const kappa = fr(0.008856), slope = fr(7.787), offset = fr(16 / 116);
      const f = (t) => (t > kappa ? fr(Math.cbrt(t)) : fr(fr(slope * t) + offset));
      const fx = f(fr(x / xn)), fy = f(fr(y / yn)), fz = f(fr(z / zn));
      return [fr(fr(fr(116.0) * fy) - fr(16.0)),
              fr(fr(500.0) * fr(fx - fy)),
              fr(fr(200.0) * fr(fy - fz))];
    }

    /** **ΔE（CIE76）✗**（**照 `color::delta_e_linear_rgb` ✓**；两线性色入、f32 出 ✓）。 */
    function p2cDeltaELinearRgb(a, b) {
      const fr = Math.fround;
      const la = p2cLinearToLab(a[0], a[1], a[2]);
      const lb = p2cLinearToLab(b[0], b[1], b[2]);
      const dl = fr(la[0] - lb[0]), da = fr(la[1] - lb[1]), db = fr(la[2] - lb[2]);
      return fr(Math.sqrt(fr(fr(fr(dl * dl) + fr(da * da)) + fr(db * db))));
    }

    /** **与参考图逐像素比 ΔE ✗**（**照 `read_analyze_region` ✓**）。
     *
     *  **∴ 返回 `{value}` 或 `{error}`（**error ⇒ 调用方报 `invalid_argument` ✓**）✗**：
     *  **∴ 比不了的三态**（没参考图 ✓／解不开 ✓／尺寸不一致 ✓）**一律如实作答 ✓**，
     *  返回 `{ok:false, reason}` 而不是 0 ✓（**"给个假数字比不给更糟" ✓**）。
     */
    async function p2cCompareWithReference(width, height, pixels) {
      const { tx: txFn, wrap: wrapFn, getBlob } = await import("./store.js");
      const row = await wrapFn(txFn(handle, "prefs", "readonly").get("reference.blob_hash"));
      const hash = row && row.value;
      if (typeof hash !== "string" || !hash) {
        return { value: { ok: false, reason: "没有参考图 ⇒ 先用 set_reference 设一张" } };
      }
      // **∴ 哈希合法性照 `BlobHash::from_str` ✗**：认四种写法 ✓，取**最后一个** `sha256:` ✓，
      //   截到 `? # & /` 之前 ✓、最多 71 字 ✓；**只收 64 位小写十六进制 ✓**。
      const tail = hash.includes("sha256:")
        ? "sha256:" + hash.slice(hash.lastIndexOf("sha256:") + 7) : hash;
      const candidate = tail.split(/[?#&/]/)[0].slice(0, 71);
      if (!(candidate.length === 71 && candidate.startsWith("sha256:") &&
            /^[0-9a-f]{64}$/.test(candidate.slice(7)))) {
        return { error: "reference.blob_hash 不是合法哈希" };
      }
      const bytes = await getBlob(handle, candidate);
      const ref = bytes ? await p2cDecodePngStrict(bytes) : null;
      if (!ref) return { value: { ok: false, reason: "参考图不是能解码的 PNG" } };
      if (ref.w !== width || ref.h !== height) {
        return { value: { ok: false, reason: "参考图 " + ref.w + "x" + ref.h +
                          " 与区域 " + width + "x" + height + " 尺寸不一致 ⇒ 无法逐像素比" } };
      }
      let sum = 0.0, worst = 0.0, worstAt = [0, 0];
      const count = width * height;
      for (let index = 0; index < count; index++) {
        const at = index * 4;
        const renderedLinear = [p2cByteToLinear(pixels[at]),
                                p2cByteToLinear(pixels[at + 1]),
                                p2cByteToLinear(pixels[at + 2])];
        const referenceLinear = [p2cByteToLinear(ref.data[at]),
                                 p2cByteToLinear(ref.data[at + 1]),
                                 p2cByteToLinear(ref.data[at + 2])];
        const delta = p2cDeltaELinearRgb(renderedLinear, referenceLinear);
        sum += delta;
        if (delta > worst) { worst = delta; worstAt = [index % width, Math.floor(index / width)]; }
      }
      return { value: { ok: true, delta_e_mean: sum / Math.max(1, count), delta_e_max: worst,
                        delta_e_max_at: worstAt, reference: { width: ref.w, height: ref.h } } };
    }

    /** **大气光估计 ✗**（**照 `filter::estimate_atmospheric_light` 的 **f32** 语义 ✓**）。
     *
     *  **∴ 为什么不整幅排序 ✗**：服务端用 `Vec<(f32,usize)>` **全排序**再取前 0.1% ✓
     *  ⇒ **∴ 逐项照抄在浏览器里**要排几百万项 ✗。**∴ 而** `dark = min(r,g,b)/255` ✗
     *  ⇒ **∴ 它只有 **256 个取值** ✓（**min 是个字节 ✓**）⇒ **∴ 直方图就能定出截断值 ✓**。
     *  **∴ 等价的取舍**：全排序（稳定 ✓）取前 k ⇒ **∴ 选中的就是**「暗通道值 > 截断」全部 ✓
     *  ＋「同一取值里**索引最小**的那几个」✓。**∴ 求和顺序也必须相同 ✗**
     *  （**f32 累加与顺序有关 ✓**）⇒ **∴ 按取值**降序**分组、组内**索引升序** ✓**。
     *  **∴ 选中总数 < k ⇒ 不可能 ✓**（**截断值就是按累计数定的 ✓**）。
     */
    function p2cEstimateAtmosphericLight(width, height, pixels) {
      const fr = Math.fround;
      const n = width * height;
      if (!(n > 0) || !pixels || pixels.length < n * 4) return null;
      const byteAt = (index, channel) => pixels[index * 4 + channel];
      // **∴ 第一遍：按 `min(r,g,b)` 直方图 ✗**（**∴ 桶号就是那个字节 ✓**）。
      const hist = new Float64Array(256);
      let sum = 0.0;
      for (let index = 0; index < n; index++) {
        const m = Math.min(byteAt(index, 0), byteAt(index, 1), byteAt(index, 2));
        hist[m] += 1;
        // **∴ 服务端累加的是**暗通道（`min` ✓）✗ 不是红通道 —— **∴ 这里也必须是 `m` ✓**。
        sum = fr(sum + fr(fr(m) / 255));
      }
      // **∴ `take = ceil(count as f32 * 0.001f32).max(1)` ✗**（**f32 乘法 ✓**）。
      const count = n;
      const take = Math.max(1, Math.ceil(fr(fr(count) * fr(0.001))));
      // **∴ 定截断值：从 255 往下累计 ✗**（**`dark` 降序 = 字节降序 ✓**）。
      let cutoff = 255, above = 0;
      while (cutoff >= 0 && above + hist[cutoff] < take) { above += hist[cutoff]; cutoff -= 1; }
      const tieNeed = take - above;
      // **∴ 第二遍：收集**高于截断的全部** ＋ 截断值里**最早的** tieNeed 个 ✓**。
      const groups = new Map();
      const ties = [];
      for (let index = 0; index < n; index++) {
        const m = Math.min(byteAt(index, 0), byteAt(index, 1), byteAt(index, 2));
        if (m > cutoff) {
          if (!groups.has(m)) groups.set(m, []);
          groups.get(m).push(index);
        } else if (m === cutoff && ties.length < tieNeed) {
          ties.push(index);
        }
      }
      // **∴ 按取值降序、组内索引升序累加 ✗**（**与全排序取前 k 的顺序一致 ✓**）。
      const air = [0.0, 0.0, 0.0];
      const values = [...groups.keys()].sort((a, b) => b - a);
      if (ties.length) values.push(cutoff);
      for (const m of values) {
        const picked = (m === cutoff) ? ties : groups.get(m);
        for (const index of picked) {
          for (let c = 0; c < 3; c++) air[c] = fr(air[c] + fr(byteAt(index, c) / 255));
        }
      }
      const scale = fr(1 / fr(take));
      for (let c = 0; c < 3; c++) air[c] = Math.min(1, Math.max(fr(0.05), fr(air[c] * scale)));
      // **∴ 色度阻尼（与恢复入口同口径 ✓）✗**。
      const luma = Math.max(fr(1e-3),
        fr(fr(fr(fr(0.2126) * air[0]) + fr(fr(0.7152) * air[1])) + fr(fr(0.0722) * air[2])));
      const damped = air.map((v) => fr(luma * Math.min(fr(1.05), Math.max(fr(0.95), fr(v / luma)))));
      const darkMean = fr(sum / fr(count));
      return { air: damped, dark_mean: darkMean };
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
    /** **从日志推导文档状态 ✗**（**`get_state` 等状态面共用 ✓**；**`get_resolved_state` 已改成逐对象 ✓ 不再用它 ✓**）：
     *   **∴ 图层**：**`create_layer` 建 ✗ ＋ `set_property{layer_id}` 覆盖 ✗ ＋
     *     `tombstone{layer_id}` 删（**被 `revert` 中和的不算 ✓**）**；
     *   **∴ 对象**：**建对象原子（=`P2_OBJECT_CREATE_KINDS` ✓）按 object_id 去重列 ✗**（**只作 id 种子 ✓；
     *     真实类型／层／可见／删除由调用方经 `p2ObjectView` 合并视图取 ✓**）。 */
    function p2cState(rows) {
      rows = p2StateWindow(rows); // `state@seq` 窗口 ✓（跳变后旧段不在状态里 ✓）
      const { w, h, color_space } = p2cDocSize(rows);
      const layers = [], layerIdx = {};
      const objects = [];
      const objSeen = new Set();  // 种子按 object_id 去重（与服务端 alive_objects 一对象一条 ✓）
      const checkpointIds = new Set(); // 服务端 `state.checkpoints` 是**按 id 的 Map** ✓ ⇒ 同 id 只算一个 ✓
      const neutral = p2Neutralized(rows);
      const cmpId = (a, b) => (a < b ? -1 : a > b ? 1 : 0);
      for (const r of rows) {
        const a = (r && r.atom) || {};
        const pl = a.payload || {};
        if (a.kind === "create_document") {
          // **新文档自带默认层 ✗**（`fold.rs:1020` ✓）⇒ `get_state`／`list_documents`
          // 从初始状态起就该有它 ✓（旧 `p2cState` 只认 `create_layer` ⇒ 凭空少一层 ✗）。
          if (!layerIdx["layer_default"]) {
            layerIdx["layer_default"] = {
              layer_id: "layer_default", name: "图层 1", type: "raster", z_index: -1,
              visible: true, opacity: 1, blend_mode: "normal", locked: false, mask_id: null,
            };
            layers.push(layerIdx["layer_default"]);
          }
        } else if (a.kind === "create_layer") {
          // **★ 兼容旧原子**：顶层 `layer_id` 回退（与 `p1LayerOrder` 同口径 ✓）。
          const id = (typeof pl.layer_id === "string" && pl.layer_id) ||
                     (typeof a.layer_id === "string" && a.layer_id) || null;
          if (!id || layerIdx[id]) continue;
          if (neutral.has(a.id)) continue; // 建层被撤销 ⇒ 状态里没有这一层 ✓
          // **z 缺省照 `fold.rs:1051` ✗**：`alive_layers()` 的 `max(z+1)`（不是「插入序」✗；已删层不计 ✓）。
          const zRaw = Number(pl.z_index);
          const zDefault = layers.reduce((mx, l) => (l.deleted ? mx : Math.max(mx, l.z_index + 1)), 0);
          const type = typeof pl.type === "string" && pl.type ? pl.type : "raster";
          layerIdx[id] = { layer_id: id, name: pl.name ?? id, type,
                           z_index: Number.isFinite(zRaw) ? zRaw : zDefault,
                           visible: true, opacity: 1, blend_mode: "normal", locked: false, mask_id: null };
          layers.push(layerIdx[id]);
        } else if (a.kind === "set_property" && pl.layer_id && layerIdx[pl.layer_id] &&
                   !neutral.has(a.id)) {
          if (pl.key === "type") { layerIdx[pl.layer_id].type = pl.value; }
          else layerIdx[pl.layer_id][pl.key] = pl.value;
        } else if (a.kind === "tombstone" && pl.layer_id && layerIdx[pl.layer_id] && !neutral.has(a.id)) {
          layerIdx[pl.layer_id].deleted = true;
        } else if (P2_OBJECT_CREATE_KINDS.has(a.kind) && typeof pl.object_id === "string" &&
                   !neutral.has(a.id) && !objSeen.has(pl.object_id)) {
          // **建对象原子 = 共享集合 ✓**（与 `p2ObjectView`／`list_objects`／`duplicate_layer` 同口径 ✓）；
          // 旧只 push `create_object` ⇒ `get_state`／`list_documents`／`collect_diagnostics` 漏掉
          // 手画图／导入图／填充／文字对象 ✓（静默少一整个结果集，还 `ok:true`）。按 object_id 去重 ⇒ supersede 再引用不重复计数 ✓。
          objSeen.add(pl.object_id);
          objects.push({ object_id: pl.object_id, layer_id: pl.layer_id ?? null, type: pl.type ?? null });
        } else if ((a.kind === "checkpoint" || a.kind === "create_checkpoint") &&
                   !neutral.has(a.id)) {
          checkpointIds.add(typeof pl.checkpoint_id === "string" && pl.checkpoint_id
            ? pl.checkpoint_id : ("ckpt_" + a.id));
        }
      }
      // **图层存活 = 自身墓碑 ＋ 祖先级联删 ＋ 创建被撤销 ✗**（`fold.rs:1988` ✓）⇒
      // 与 `list_layers`／`list_objects` 共用 `p2PresetLayers` 一份口径 ✓。
      const layersAlive = p2PresetLayers(rows, neutral);
      const aliveLayers = layers.filter((l) => !l.deleted &&
        layersAlive.has(l.layer_id) && layersAlive.get(l.layer_id).alive);
      // **顺序照 `alive_layers()` ✓**（`state.rs:701`：`(z_index, id)` 升序 ✓）。
      aliveLayers.sort((x, y) => (x.z_index - y.z_index) || cmpId(x.layer_id, y.layer_id));
      return { width: w, height: h, color_space,
               layers: aliveLayers,
               objects, checkpoints: checkpointIds.size };
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
      // **∴ `group_id` 盖章 ✗**（**∴ 照服务端 `write_draw` 唯一读它的地方 ✓ —— `tools.rs:5254`
      //   `ctx.pending_group_id = optional_str(args, "group_id")` ⇒ **∴ commit 落到原子封套 ✓**；
      //   **∴ 非字符串 ⇒ `optional_str` 得 `None` ⇒ **∴ 静默不盖章 ✗**（**∴ 与服务端同形 ✓**））。
      const groupId = typeof body.group_id === "string" ? body.group_id : undefined;
      const seq = await putAtom(handle, doc, p2Envelope(kind, payload, groupId));
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
    /** **缺省 `z_index` 的口径 ✓**（**照 `write_add_effect`：同层**存活**对象的最大 z ＋ 1，没有则 0 ✓**）。
     *   **∴ 为什么本地原来漏了这一条 ✗**：不写 `z_index` ⇒ 折叠层按缺省 0 落 ⇒
     *   **调整／滤镜只作用于「同层下方」**（设计 6.6）⇒ 新笔排在最底下 ⇒ 上面没有任何东西 ⇒
     *   **`ok:true` 而画面毫无变化 ✗**（服务端 11568 行的注释描述的正是这个症状 ✓）＝ 静默降级形态① ✓。
     *   **∴ 残余 ✗**：只扫「创建净荷里写着这一层」的对象 ⇒ 不跟随 `supersede.layer_id` 移层
     *   （**`p2ObjectView` 的既有缺口，台账「第 六百五十七 节」记 (b) ✓**）。 */
    function p2DefaultZInLayer(rows, layers, neutral, layerId) {
      const seeds = new Set();
      for (const r of rows || []) {
        const a = (r && r.atom) || {};
        const pl = a.payload || {};
        if (P2_OBJECT_CREATE_KINDS.has(a.kind) && pl.layer_id === layerId &&
            typeof pl.object_id === "string") seeds.add(pl.object_id);
      }
      let zTop = 0, hasAlive = false;
      for (const oid of seeds) {
        const slot = p2PresetSlot(rows, layers, neutral, oid);
        if (slot.slot !== "alive") continue;
        const z = Number.isFinite(Number(slot.view.z_index)) ? Number(slot.view.z_index) : 0;
        if (!hasAlive || z > zTop) { zTop = z; hasAlive = true; }
      }
      return hasAlive ? zTop + 1 : 0;
    }
    /** **滤镜／调整新增 ✗**（**照 `write_add_effect` ✓ ⇒ **`create_object{type}` ✓**）。
     *   **★ 槽位三态是这条路径的必经检查 ✓**（**`tools.rs:11536` 的 `ensure_object_slot` ✓**）：
     *   **∴ 本地原来一个字都没检查 ✗** ⇒ 显式 `object_id` 撞车时**照写不误**✓ 而后一条
     *   `create_object` 被**折叠层丢弃**（**`fold.rs:551` ⇒ 服务端是 409 ✓**）⇒
     *   **响应 `ok:true`、状态里却没有那一笔 ✗ ＝ 静默降级形态③ ✓**（覆盖而不说 ✗）。
     *   **∴ 墓碑支路不许报错 ✗**：`write_add_effect` 撞墓碑走**复活**（**「不许把『已存在』甩给调用方 ✓」**
     *   ⇒ 守卫与原子序照 `revive_effect_in_place` ✓ 见下面 `p2cReviveEffect` ✓）。 */
    async function p2cAddEffect(kind, request = body) {
      const layerId = request.layer_id;
      if (typeof layerId !== "string" || !layerId) return p2Bad("layer_id 必填（字符串）");
      const nameKey = kind === "filter" ? "filter_name" : "adjustment_type";
      const name = request[nameKey];
      if (typeof name !== "string" || !name) return p2Bad(nameKey + " 必填（字符串）");
      const params = (request.params && typeof request.params === "object" && !Array.isArray(request.params)) ? request.params : {};
      const opacity = request.opacity !== undefined ? Number(request.opacity) : 1;
      if (!Number.isFinite(opacity) || opacity < 0 || opacity > 1) return p2Bad("opacity 必须在 [0,1] 内");
      const data = { [nameKey]: name, params, opacity };
      if (typeof request.name === "string") data.name = request.name;
      const objectId = (typeof request.object_id === "string" && request.object_id) ? request.object_id : p2cNewObjectId();
      const rows = await atomsOf(handle, doc);
      const neutral = p2Neutralized(rows);
      const layers = p2PresetLayers(rows, neutral);
      const slot = p2PresetSlot(rows, layers, neutral, objectId);
      if (slot.slot === "tombstoned") {
        return p2cReviveEffect(kind, { layer_id: layerId, object_id: objectId, name, params, opacity,
                                       label: typeof request.name === "string" ? request.name : null,
                                       rows, layers, neutral, slot });
      }
      if (slot.slot === "alive") {
        return p2ToolFailure(409, "precondition_failed", `对象 ${objectId} 已存在`, { object_id: objectId });
      }
      // **层必须**活着**✓**（**`log.rs:1188 check_layer_exists` ✗ 两句话不能混用 ✓**：
      //   **从未有过 ⇒ 404 `missing_reference`（带可用名单 ✓）；存在但已删 ⇒ 409「已删除，create_object 被拒绝」✓**
      //   ——**∴ 笼统报「不存在」会把「刚删掉」说成「没有」✗ 而这两句自相矛盾 ✓**（同文件 479 行的教训 ✓）。
      //   **∴ 而 `apply_preset` 判的是 `contains_key` ✗**（**服务端两处口径本来就不同 ⇒ 照字面，不「顺手」统一 ✓**））。
      const layer = layers.get(layerId);
      if (!layer) {
        return p2ToolFailure(404, "reference_not_found", p2LayerMissingMessage(layers, layerId),
                           { layer_id: layerId });
      }
      if (!layer.alive) {
        return p2ToolFailure(409, "precondition_failed", `图层 ${layerId} 已删除，create_object 被拒绝`,
                           { layer_id: layerId });
      }
      const payload = { object_id: objectId, layer_id: layerId, type: kind, data,
                        z_index: Number.isFinite(Number(request.z_index))
                          ? Number(request.z_index) : p2DefaultZInLayer(rows, layers, neutral, layerId) };
      const seq = await putAtom(handle, doc, p2Envelope("create_object", payload));
      const out = { ok: true, seq, server: false, object_id: objectId,
                    note: "已写入 create_object{type:" + kind + "} 原子" };
      out[nameKey] = name;
      return json(out);
    }
    /** **撞墓碑 ⇒ 复活并整组换参数 ✗**（**照 `revive_effect_in_place` ✓ 三道守卫 ＋ 原子序逐字 ✓**：
     *   ① 类型不符 ⇒ 拒绝「以复活名义改型」✓；② 效果名不符 ⇒ 拒绝「复活改名」✓；
     *   ③ **层不符且源层还活着** ⇒ 拒绝「复活不跨层」✓（**歧义**：搬过去还是放回原层 ✗）；
     *   **∴ 而源层已被删 ⇒ 是孤儿墓碑 ⇒ 没有歧义 ✓**：撤那条**图层** tombstone（级联删的源头 ✓）
     *   ⇒ 撤对象自己的每条生效墓碑 ⇒ 一条 `supersede` **整组**换新参数（**不是键级合并 ✓**）
     *   ⇒ **再把用户删掉的那一层删回去** ✓（对象已搬到目标层 ⇒ 不受影响 ✓）。
     *   **∴ 成功必须点名 `revived:true` ✗**（**服务端 11762 行 ✓**：不许静默复活 ✓）。 */
    async function p2cReviveEffect(kind, intent) {
      const { layer_id: layerId, object_id: objectId, name, params, opacity, label,
              rows, layers, neutral, slot } = intent;
      const existing = slot.view;
      if (existing.type !== kind) {
        return p2ToolFailure(400, "invalid_argument",
          `对象 ${objectId} 已被删除且不是${kind}对象（实际 ${p2PresetTypeNameDebug(existing.type)}），`
          + "拒绝以复活名义改型；请先 restore_object 恢复或换 object_id", { object_id: objectId });
      }
      const effectKey = kind === "filter" ? "filter_name" : "adjustment_type";
      const raw = existing.data && typeof existing.data === "object" ? existing.data[effectKey] : undefined;
      const existingName = typeof raw === "string" ? raw : "";
      if (existingName !== name) {
        return p2ToolFailure(400, "invalid_argument",
          `对象 ${objectId} 已被删除且效果是「${existingName}」，与本次要落的「${name}」不一致，`
          + "拒绝复活改名；请先 restore_object 恢复或换 object_id", { object_id: objectId });
      }
      const ownerLayer = typeof existing.layer_id === "string" ? existing.layer_id : "";
      const orphan = ownerLayer !== layerId && !(layers.has(ownerLayer) && layers.get(ownerLayer).alive);
      if (ownerLayer !== layerId && !orphan) {
        return p2ToolFailure(400, "invalid_argument",
          `对象 ${objectId} 已被删除且属于图层 ${ownerLayer}（它还在），与本次目标图层 ${layerId} 不一致，`
          + "复活不跨层；请先 restore_object 恢复后移层，或换 object_id", { object_id: objectId });
      }
      if (orphan && slot.dead_by) {
        await putAtom(handle, doc, p2Envelope("revert", { target: slot.dead_by }));
      }
      for (const tid of existing.tombstone_ids) {
        if (neutral.has(tid)) continue;
        await putAtom(handle, doc, p2Envelope("revert", { target: tid }));
      }
      const data = { ...(existing.data && typeof existing.data === "object" &&
                       !Array.isArray(existing.data) ? existing.data : {}) };
      data.params = params;
      data.opacity = opacity;
      if (label !== null && label !== undefined) data.name = label;
      const targetLayer = orphan ? layerId : ownerLayer;
      // **`supersede` 之前要再过一次提交期校验 ✗**（**服务端就是这么做的 ✓**：
      //   `revive_object` 只撤**对象自己**的 tombstone ✓ 撤完照样 `ctx.commit(Supersede)` ✓
      //   而校验是在 commit 里跑的 ✓）。
      //   **∴ 少了这一步会怎样 ✗**：对象随**图层**级联而死（`fold.rs:1320` ✓）⇒
      //   撤不到那条图层原子 ⇒ 对象仍然死着 ✓ ⇒ 服务端 409「已被 tombstone，supersede 被拒绝」✗
      //   而本地**写出成功** ✓（**复活成功而层还是删着 ⇒ 那一笔落在死层上 = 形态③ ✓**）。
      const rowsAfter = await atomsOf(handle, doc);
      const neutralAfter = p2Neutralized(rowsAfter);
      const layersAfter = p2PresetLayers(rowsAfter, neutralAfter);
      const guard = p2RefGuard(rowsAfter, layersAfter, neutralAfter, "supersede",
                               { object_id: objectId, layer_id: targetLayer });
      if (guard) {
        return p2ToolFailure(guard.status, guard.code, guard.detail, guard.extra);
      }
      const seq = await putAtom(handle, doc, p2Envelope("supersede", {
        object_id: objectId, layer_id: targetLayer, data,
      }));
      if (orphan) {
        await putAtom(handle, doc, p2Envelope("tombstone", { layer_id: ownerLayer }));
      }
      const out = { ok: true, seq, server: false, object_id: objectId, revived: true,
                    note: "确定性 object_id 撞上墓碑 ⇒ 复活并整组换新参数（supersede，非键级合并）" };
      out[effectKey] = name;
      return json(out);
    }
    /** **滤镜／调整更新 ✗**（**照 `write_update_effect` ✓ ⇒ **`supersede` 合并 `params` ✓**）。
     *   **∴ 三条拒绝也要照抄 ✓**（**顺序 = 服务端顺序 ✓**）：
     *   ① **状态里没有** ⇒ 404 `对象 X 不存在` ✓（**`tools.rs:11972` 自己那句 ✗ 没有候选名单 ✓**
     *     —— 与 `delete_object` 走的**提交期**那句「（现有对象：…）」**是两句话 ✗ 别混 ✓**）；
     *   ② **类型不对** ⇒ 400 ＋ **指路**（**Y-07：只说「不是 XX 对象」不够 ✓ 要说该用哪个工具 ✓**）；
     *   ③ **对象已 tombstone／所属图层已删除** ⇒ 提交期 409 ✓（**此前本地静默写成功 ✗
     *     而折叠把这条 supersede 丢掉 ⇒ 回执 ok 而画面没变 ✓ = 形态 3 ✓**）。 */
    async function p2cUpdateEffect(kind, request = body) {
      const objectId = request.object_id;
      if (typeof objectId !== "string" || !objectId) return p2Bad("object_id 必填（字符串）");
      const patch = request.params;
      if (!patch || typeof patch !== "object" || Array.isArray(patch)) return p2Bad("params 必须是对象（必填）");
      const rows = await atomsOf(handle, doc);
      const neutral = p2Neutralized(rows);
      const v = p2ObjectView(rows, objectId);
      if (!v.found || neutral.has(v.__create_id)) return p2Missing(`对象 ${objectId} 不存在`);
      if (v.type !== kind) {
        const pointer = v.type === "filter" ? "请用 update_filter"
          : v.type === "adjustment" ? "请用 update_adjustment"
            : "请用 update_filter / update_adjustment";
        return p2Bad(`${objectId} 不是${kind}对象（它是${v.type}对象）；${pointer}`);
      }
      const data = (v.data && typeof v.data === "object" && !Array.isArray(v.data)) ? { ...v.data } : {};
      const oldParams = (data.params && typeof data.params === "object") ? data.params : {};
      data.params = { ...oldParams, ...patch };
      if (request.opacity !== undefined) {
        const op = Number(request.opacity);
        if (!Number.isFinite(op) || op < 0 || op > 1) return p2Bad("opacity 必须在 [0,1] 内");
        data.opacity = op;
      }
      const layers = p2PresetLayers(rows, neutral);
      const guard = p2RefGuard(rows, layers, neutral, "supersede",
        { object_id: objectId, layer_id: v.layer_id });
      if (guard) return p2ToolFailure(guard.status, guard.code, guard.detail, guard.extra);
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
    // **★ 第四轮补的三条本地／服务端口径差 ✗★**（`fill_region` 有了 Web 入口 ⇒ 这些从"看不见"变成"看得见"✓）：
    //   ① **`opacity` 原先整个被丢掉** ✗ ⇒ 服务端把 `opacity` 乘进颜色的 `a` ✓（三种颜色写法都吃 ✓）
    //      而这里原来只把 `color` 原样塞进 `data` ✗ ⇒ 关掉「服务器渲染」后同一个滑杆没有效果 ✗
    //      ⇒ 现在同一套：`p2cParseColor` 归一后 `a ×= opacity` ✓ 并把生效值**回显**在回执里 ✓。
    //   ② **`shape.type:"path"`** ✗：服务端第 1558 轮起认 `d`／`subpaths` ✓ 这里仍拒 ✗
    //      ⇒ 拒得响亮 ✓ 但把"为什么"说清（本地没有 SVG 路径解析器 ✗ 勾回服务器渲染可用 ✓）。
    //   ③ **`texture` 传对象**（位图／取画面这块）✗：服务端支持 ✓ 这里不支持 ✗
    //      ⇒ 原来 `String({...})` 变成 `texture=[object Object]` ✓ 看着像拼写错误 ✗
    //      ⇒ 现在分开说：那是**纹理来源**，不是笔触质感 ✓ 需要勾回「服务器渲染」✓。
    if (path === "/api/tools/fill_region") {
      const layerId = body.layer_id;
      if (typeof layerId !== "string" || !layerId) return p2Bad("layer_id 必填（字符串）");
      const shape = body.shape;
      if (!shape || typeof shape !== "object") return p2Bad("shape 必须是对象（必填）");
      if (body.color === undefined) return p2Bad("缺少 color（{r,g,b,a} 或 #rrggbb）");
      const texture = body.texture !== undefined ? body.texture : "smooth";
      if (texture !== "smooth") {
        return p2Bad(typeof texture === "object" && texture !== null
          ? "本地内核不支持纹理填充（`texture` 是**取色来源**：位图 blob 或画面那块矩形 ✓ 服务端才实现 ✓）"
            + " ⇒ 勾回「服务器渲染」再填这一笔；笔触质感（厚涂/干刷）是另一件事，另行排期"
          : "texture=" + String(texture) + " 暂不支持 ⇒ 目前只有缺省（= smooth）；厚涂/干刷肌理是独立的活，另行排期");
      }
      const opacity = body.opacity !== undefined ? Number(body.opacity) : 1;
      if (!Number.isFinite(opacity) || opacity < 0 || opacity > 1) return p2Bad("opacity 必须在 [0,1] 内");
      const parsed = p2cParseColor(body.color);
      if (!parsed) return p2Bad("color 解析不了 ⇒ 给 {r,g,b,a} / [r,g,b(,a)] / #rrggbb[aa]");
      // **与服务端同一处落点** ✓：`opacity` 折进颜色的 `a` ⇒ 原子的 `data.color` 就是生效颜色 ✓
      //   （服务端也没有单独的 `data.opacity` ✓ 两边逐字节同形 ⇒ 关掉服务器渲染不改变重放结果 ✓）。
      const color = {
        r: parsed[0], g: parsed[1], b: parsed[2],
        a: Math.max(0, Math.min(255, Math.round(parsed[3] * opacity))),
      };
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
      } else if (skind === "path") {
        return p2Bad("本地内核不解析 SVG 路径（`shape.d` ⇒ 需要路径解析器 ✓ 服务端有 ✓）"
          + " ⇒ 勾回「服务器渲染」再填这一笔，或改写成 polygon 顶点");
      } else {
        return p2Bad("shape.type 必须是 rect/ellipse/polygon（path 只有服务端认），得到 " + skind);
      }
      const objectId = (typeof body.object_id === "string" && body.object_id) ? body.object_id : p2cNewObjectId();
      const seq = await putAtom(handle, doc, p2Envelope("draw_shape", {
        object_id: objectId, layer_id: layerId, type: "shape",
        data: { geometry, color },
      }));
      // **回显生效值** ✓：`opacity` 与 `texture` 都写进回执 ✓ 与服务端同形 ✓
      // ⇒ Web 那张卡念的是回执里的数 ✗ 不是自己输入框里的数 ✓（两边都能核对参数有没有被吃掉 ✓）。
      return json({ ok: true, seq, server: false, object_id: objectId, shape: skind,
                    opacity, texture: null,
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

    // **P2c-19 `estimate_dehaze` ✓**：**照 `read_estimate_dehaze` ✓**（第 六百六十二 轮 ✓）。
    //   **∴ 此前是**明确 501 ✗**（"本地只产 PNG、没有裸像素通道" ✓）⇒ **∴ 而**那个理由
    //   **已经过期 ✗**：内核本来就导出 `render_region_rgba` ✓（**`p2cRenderRgba` ✓**）
    //   ⇒ **∴ 现在**整幅裸像素 ＋ 暗通道与大气光**按 f32 语义逐位同源 ✓**
    //   （**∴ 算法的取舍见 `p2cEstimateAtmosphericLight` ✓**）。
    if (path === "/api/tools/estimate_dehaze") {
      const rows = await atomsOf(handle, doc);
      const { w: cw, h: ch } = p2cDocSize(rows);
      let rendered;
      try {
        rendered = await p2cRenderRgba(0, 0, cw, ch);
      } catch (err) { return p2cKernel501(err); }
      if (rendered && rendered.error) return p2Bad(rendered.error);
      const est = p2cEstimateAtmosphericLight(rendered.w, rendered.h, rendered.rgba);
      if (!est) return p2Bad("像素数与尺寸不匹配，无法估计");
      const fr = Math.fround;
      // **∴ `(0.6 + dark_mean * 0.6).clamp(0.6, 0.95)` ✗** —— **每一步 f32 ✓**。
      const omega = Math.min(fr(0.95), Math.max(fr(0.6), fr(fr(0.6) + fr(est.dark_mean * fr(0.6)))));
      // **∴ 输出的是 **f32 拓宽成 f64** 的数 ✗**（**照服务端 `json!` ✓**）：
      //   **∴ `json!({"omega": f32})` 走 `Value::from(f32)` ⇒ `f32 as f64` ✓**
      //   ⇒ **∴ 序列化出的是**拓宽后的最短 f64 小数 ✗**（`0.95f32` ⇒ `0.949999988079071` ✓），
      //   **∴ 不是** f32 最短往返小数**✗**（那是 `serialize_f32` 的口径 ✓）。
      //   **∴ 所以**这里直接给 `Math.fround(x)` ✓（**JS 打印的就是那条 f64 ✓**）。
      return json({
        ok: true,
        server: false,
        air: est.air.map(fr),
        dark_channel_mean: fr(est.dark_mean),
        suggested: { omega: fr(omega), floor: 0.1 },
        note: "把 air/omega/floor 传给 add_filter {filter_name: \"dehaze\"}；渲染逐像素确定性",
      });
    }
    // **P2c-19a `list_effects` ✓**（**照 `read_list_effects` ✓**，第 六百六十五 轮 ✓）：
    //   **∴ 与 `list_objects` 的两处不同 ✗**：① **不过滤隐藏**（`alive_objects()` 含隐藏／禁用 ✓）；
    //   ② **字段取自折叠后的 `data` ✓**（`set_property` 覆盖也算 ✓ ⇒ 经 `p2EffectiveData` ✓）。
    //   **∴ 入口参数校验先行 ✗**（**照 `validate_args` ✓**：未知参数／必填／声明类型 ✓）——
    //   **∴ 实测服务端对 `layer_id:123` 回**「参数 layer_id 类型应为 string，实际为 number」✗
    //   ⇒ **∴ handler 里的 `optional_str` 根本走不到 ✓**（**∴ 别在 handler 里猜「非字符串就当没给」✗**）。
    if (path === "/api/tools/list_effects") {
      const specBad = p2ValidateArgsFor("list_effects", body);
      if (specBad) return p2ToolFailure(400, specBad.code, specBad.detail);
      const layerRaw = body.layer_id;
      const layerFilter = typeof layerRaw === "string" ? layerRaw : null; // 照 `optional_str` ✓
      const rows = p2StateWindow(await atomsOf(handle, doc));
      const neutral = p2Neutralized(rows);
      const layers = p2PresetLayers(rows, neutral);
      const seen = new Set();
      const effects = [];
      for (const r of rows) {
        const a = (r && r.atom) || {};
        const id = a.payload && a.payload.object_id;
        if (typeof id !== "string" || !id || seen.has(id)) continue;
        seen.add(id);
        const v = p2ObjectView(rows, id);
        if (!__p2Alive(rows, v, layers, neutral)) continue;
        if (v.type !== "adjustment" && v.type !== "filter") continue;
        if (layerFilter !== null && v.layer_id !== layerFilter) continue;
        const data = p2EffectiveData(v);
        const pick = (key) => (Object.prototype.hasOwnProperty.call(data, key) ? data[key] : null);
        effects.push({
          object_id: id,
          layer_id: v.layer_id ?? null,
          type: v.type,
          name: pick("name"),
          adjustment_type: pick("adjustment_type"),
          filter_name: pick("filter_name"),
          params: pick("params"),
          opacity: pick("opacity"),
          enabled: v.enabled !== false,
          __z: v.z_index ?? 0,
        });
      }
      const cmpId = (a, b) => (a < b ? -1 : a > b ? 1 : 0);
      // **排序照 `alive_objects()` ✓**：`(layer_id, z_index, id)` ✓。
      effects.sort((a, b) => cmpId(String(a.layer_id), String(b.layer_id))
        || (a.__z - b.__z) || cmpId(String(a.object_id), String(b.object_id)));
      for (const e of effects) delete e.__z;
      return json({ ok: true, server: false, effects, count: effects.length,
                    filter_aliases: P2_FILTER_ALIASES.map((e) => ({ ...e })),
                    supported: { adjustments: P2_ADJUSTMENT_NAMES.slice(),
                                 filters: P2_FILTER_NAMES.slice() } });
    }
    // **P2c-19b `sample_colors` ✓**（**照 `read_sample_colors` ✓**，第 六百六十五 轮 ✓）：
    //   **∴ 一次区域渲染 ＋ 逐点取下标 ✓**（**不是逐点渲染 ✓**）；**∴ 画布外的点 `rgba:null` ＋ 警告 ✓**
    //   （**不夹到边缘冒充成功 ✗**）。
    if (path === "/api/tools/sample_colors") {
      // **∴ 入口校验先行 ✗**：服务端对**缺参**回「缺少必填参数 points」✓、
      //   对**非数组**回「参数 points 类型应为 array，实际为 …」✓
      //   ⇒ **∴ 这两个口径都来自 `validate_args` ✗** ⇒ **∴ 不能拿 handler 里那句自定义文案顶替 ✓**
      //   （**∴ `read_sample_colors` 里那句「缺少 points ⇒ 形如 …」经 HTTP 走不到 ✓ 但留作内层兜底 ✓**）。
      const specBad = p2ValidateArgsFor("sample_colors", body);
      if (specBad) return p2ToolFailure(400, specBad.code, specBad.detail);
      const rawPoints = body.points;
      if (!Array.isArray(rawPoints)) {
        return p2Bad("缺少 points ⇒ 形如 [[120,64], [200,80]]（画布坐标，像素）");
      }
      const points = [];
      for (let i = 0; i < rawPoints.length; i++) {
        const pair = rawPoints[i];
        if (!Array.isArray(pair)) {
          return p2Bad("points[" + i + "] 应为 [x,y]，实际 " + JSON.stringify(pair));
        }
        const x = (typeof pair[0] === "number" && Number.isFinite(pair[0])) ? pair[0] : null;
        if (x === null) return p2Bad("points[" + i + "][0] 不是数字");
        const yv = pair.length > 1 ? pair[1] : undefined;
        const y = (typeof yv === "number" && Number.isFinite(yv)) ? yv : null;
        if (y === null) return p2Bad("points[" + i + "][1] 不是数字");
        points.push([x, y]);
      }
      if (points.length === 0) return p2Bad("points 不能为空");
      const rows = await atomsOf(handle, doc);
      const { w: cw, h: ch } = p2cDocSize(rows);
      let ax = Infinity, ay = Infinity, bx = -Infinity, by = -Infinity;
      for (const [x, y] of points) {
        ax = Math.min(ax, x); ay = Math.min(ay, y);
        bx = Math.max(bx, x); by = Math.max(by, y);
      }
      // **包围盒先裁到画布内 ✓**（**照服务端 §27 ✓**：否则 stride 偏大 ⇒ 画布内的点也会读成 null ✗）。
      const x0 = Math.max(0, Math.floor(ax));
      const y0 = Math.max(0, Math.floor(ay));
      const w = Math.max(1, Math.min(Math.floor(bx) - x0 + 1, Math.max(1, cw - x0)));
      const h = Math.max(1, Math.min(Math.floor(by) - y0 + 1, Math.max(1, ch - y0)));
      let rendered;
      try {
        rendered = await p2cRenderRgba(x0, y0, w, h);
      } catch (err) { return p2cKernel501(err); }
      if (rendered && rendered.error) return p2Bad(rendered.error);
      const stride = w * 4;
      const samples = [];
      let outsideCount = 0;
      for (let i = 0; i < points.length; i++) {
        const [x, y] = points[i];
        const outside = x < 0 || y < 0 || x >= cw || y >= ch;
        let rgba = null;
        if (outside) {
          outsideCount += 1;
        } else {
          const px = Math.max(0, Math.floor(x) - x0);
          const py = Math.max(0, Math.floor(y) - y0);
          const offset = py * stride + px * 4;
          if (offset + 4 <= rendered.rgba.length) {
            rgba = [rendered.rgba[offset], rendered.rgba[offset + 1],
                    rendered.rgba[offset + 2], rendered.rgba[offset + 3]];
          }
        }
        samples.push({ index: i, x, y, rgba, outside });
      }
      const warnings = outsideCount > 0 ? [
        outsideCount + " 个点在画布**外面** ⇒ 它们的 rgba 是 null ✓（画布 " + cw + "×" + ch +
        "，原点在**左上角** ✓）。这不是「画布边缘的颜色」✗ —— 那会让人以为取色成功 ✓。"
      ] : [];
      return json({ ok: true, server: false, count: samples.length,
                    outside_count: outsideCount, warnings,
                    region: { x: x0, y: y0, w, h }, samples });
    }
    // **P2c-19c `get_histogram` ✓**（**照 `read_get_histogram` ✓**，第 六百六十五 轮 ✓）：
    //   **∴ 四个作用域 ✓**（整文档／选区／图层／图层×选区 ✓）；**∴ 图层作用域用合成原子 ✓**
    //   （`p2LayerScopeAtoms` ✓ —— 服务端探针已证与服务端 `set_only_layer` 逐位相同 ✓）。
    if (path === "/api/tools/get_histogram") {
      // **∴ 入口校验先行 ✗**（**照 `validate_args` ✓**）：**`region` 声明为 `object` ✓**
      //   ⇒ **∴ 数组形式**过不了 HTTP 层 ✗（**`parse_bbox` 的数组分支经工具入口走不到 ✓**）；
      //   **`layer_id:123` 同理**回「参数 layer_id 类型应为 string」✓。
      const specBad = p2ValidateArgsFor("get_histogram", body);
      if (specBad) return p2ToolFailure(400, specBad.code, specBad.detail);
      const layerRaw = body.layer_id;
      const layerId = typeof layerRaw === "string" ? layerRaw : null; // 照 `optional_str` ✓
      const rows = await atomsOf(handle, doc);
      const { w: cw, h: ch } = p2cDocSize(rows);
      let region = null;
      if (body.region !== undefined && body.region !== null) {
        region = p2cParseRegion(body.region);
        if (!region) return p2Bad("region 必须是 {x,y,w,h} 或 [x,y,w,h]");
      }
      let props = null;
      if (layerId !== null) {
        props = p2LayerRenderProps(rows);
        // **存在性照 `state.layers.contains_key` ✓**（墓碑层仍算存在 ✓ —— `p2LayerRenderProps` 保留它 ✓）
        if (!props.has(layerId)) {
          return p2ToolFailure(404, "reference_not_found", "图层 " + layerId + " 不存在",
                               { layer_id: layerId });
        }
      }
      const bbox = region || { x: 0, y: 0, w: cw, h: ch };
      let rendered;
      try {
        const extra = layerId !== null ? p2LayerScopeAtoms(rows, props, layerId) : null;
        rendered = await p2cRenderRgba(bbox.x, bbox.y, bbox.w, bbox.h, extra);
      } catch (err) { return p2cKernel501(err); }
      if (rendered && rendered.error) return p2Bad(rendered.error);
      const hist = p2cHistogramRgba8(rendered.rgba);
      if (hist.count === 0) return p2Precon("这块作用域没有像素可统计");
      const summaryOf = (buckets) => p2cHistSummary(buckets, hist.count);
      const scope = layerId !== null
        ? (region ? "layer_region" : "layer")
        : (region ? "region" : "document");
      return json({
        ok: true, server: false,
        scope, layer_id: layerId,
        region: { x: bbox.x, y: bbox.y, w: bbox.w, h: bbox.h },
        width: rendered.w, height: rendered.h,
        pixels: hist.count,
        channels: { r: hist.r, g: hist.g, b: hist.b, a: hist.a },
        luma: hist.luma,
        summary: {
          r: summaryOf(hist.r), g: summaryOf(hist.g), b: summaryOf(hist.b),
          a: summaryOf(hist.a), luma: summaryOf(hist.luma),
        },
      });
    }
    // **P2c-20 `transform_object` ✓**：**与服务端同一套参数面 ✓**（`rotate`／`scale`／`translate`
    //   **恰好一个 ✓** ＋ `anchor`（缺省 = **折叠后包围盒中心** ✓）＋ `compose`（缺省 true ✓））。
    //   **∴ 旧实现收 `transform{matrix}`／`dx`／`dy` ✗** ⇒ **服务端根本不接受这些参数 ✗**
    //   ⇒ **∴ 离线 PWA 的变换面板（发 rotate／scale／translate ✓）必然 400 ✓**。
    //   **∴ 提交同服务端** `AtomKind::Move` ✓（绝对矩阵 ✓）。
    if (path === "/api/tools/transform_object") {
      const argError = p2TransformArgError(body);
      if (argError) return p2Bad(argError);
      const objectId = body.object_id;
      const rows = await atomsOf(handle, doc);
      const neutral = p2Neutralized(rows);
      const layers = p2PresetLayers(rows, neutral);
      const view = p2ObjectView(rows, objectId);
      // **服务端读的是 `state.objects` ✓**：墓碑对象**仍在状态里** ✓ ⇒ 这里**不是** 404 ✓
      //   （它在提交期被 409 拒绝 ✓）；只有「创建被 revert」才不在状态里 ✓。
      if (!view.found || neutral.has(view.__create_id)) {
        return p2Missing("对象 " + objectId + " 不存在");
      }
      const compose = typeof body.compose === "boolean" ? body.compose : true;
      const bbox = p2ObjectBBox(rows, view, layers, neutral);
      let anchor = null;
      const rawAnchor = body.anchor;
      if (rawAnchor !== undefined && rawAnchor !== null) {
        if (!p2Num(rawAnchor.x) || !p2Num(rawAnchor.y)) return p2Bad("anchor 需要 {x, y}");
        anchor = [rawAnchor.x, rawAnchor.y];
      } else if (bbox) {
        anchor = [bbox.x + bbox.w / 2, bbox.y + bbox.h / 2];
      } else {
        return p2Precon("对象当前没有可用的包围盒，无法推出缺省 anchor；请显式给出 anchor");
      }
      // **恰好一个 ✓**；**解析错误先于**计数判断 ✓（服务端就是先 if/else 解析、再判 `requested` ✓）。
      const present = (key) => body[key] !== undefined && body[key] !== null;
      const requested = ["rotate", "scale", "translate"].filter(present).length;
      let local = [1, 0, 0, 1, 0, 0];
      if (present("rotate")) {
        if (!p2Num(body.rotate.degrees)) return p2Bad("rotate 需要 {degrees}");
        const radians = body.rotate.degrees * Math.PI / 180;
        const sin = Math.sin(radians), cos = Math.cos(radians);
        local = [cos, sin, -sin, cos, 0, 0];
      } else if (present("scale")) {
        const sx = p2Num(body.scale.x) ? body.scale.x : 1.0;
        const sy = p2Num(body.scale.y) ? body.scale.y : sx;
        if (Math.abs(sx) < 1e-6 && Math.abs(sy) < 1e-6) {
          return p2Bad("scale 不能两个方向都为 0（会把对象压成空）");
        }
        local = [sx, 0, 0, sy, 0, 0];
      } else if (present("translate")) {
        local = [1, 0, 0, 1,
                 p2Num(body.translate.dx) ? body.translate.dx : 0.0,
                 p2Num(body.translate.dy) ? body.translate.dy : 0.0];
      }
      if (requested !== 1) {
        return p2Bad("请给出 rotate / scale / translate **其中之一**（同时给多个会让叠加顺序变成没说清的事）");
      }
      // **绕 anchor：`T(anchor) · op · T(-anchor)` ✓**（**与服务端同一算式 ✓**）。
      const aboutAnchor = {
        matrix: [local[0], local[1], local[2], local[3],
                 local[4] + anchor[0] - (local[0] * anchor[0] + local[2] * anchor[1]),
                 local[5] + anchor[1] - (local[1] * anchor[0] + local[3] * anchor[1])],
        pivot: [0, 0],
      };
      const finalTransform = compose
        ? p2ComposeTransforms(aboutAnchor, view.transform)
        : aboutAnchor;
      const payload = { object_id: objectId,
                        transform: { matrix: finalTransform.matrix, pivot: finalTransform.pivot } };
      const guard = p2RefGuard(rows, layers, neutral, "move", payload);
      if (guard) return p2ToolFailure(guard.status, guard.code, guard.detail, guard.extra);
      const seq = await putAtom(handle, doc, p2Envelope("move", payload));
      return json({ ok: true, object_id: objectId, anchor, composed: compose,
                    matrix: finalTransform.matrix, head: seq, server: false,
                    note: "已写入 move 原子（绝对矩阵；缺省 anchor = 折叠后包围盒中心）" });
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

    // **P2c-27 `analyze_region` ✓**：**照服务端 `read_analyze_region` 逐字段 ✓**（第 六百六十二 轮 ✓）。
    //   **∴ 此前本地返回的是**另一套载荷 ✗**（`region`／`mean_rgba`／`std_rgba`／
    //   `histogram_16bins` ✓）⇒ **∴ 同一端点两套契约 ✗** —— **∴ 调用方（AI 客户端 ✓）
    //   按服务端字段读 `dominant_colors[].share`／`avg_brightness` 时**在本地恒缺 ✗**。
    //   **∴ 现在**：裸像素走内核 `render_region_rgba` ✓（**不再解码 PNG ✓**），
    //   统计口径（16³ 桶 ✓／桶内均值 ✓／整数除法 ✓／f64 亮度 ✓／冷暖 ✓）与服务端**逐位同源 ✓**。
    if (path === "/api/tools/analyze_region") {
      // **∴ 报错口径照服务端 ✗**：`require_object` 缺 ⇒「缺少必填参数 region」✓；
      //   有但不是盒 ⇒「region 必须是 {x,y,w,h} 或 [x,y,w,h]」✓。
      if (body.region === undefined || body.region === null ||
          (typeof body.region !== "object")) {
        return p2Bad("缺少必填参数 region");
      }
      const region = p2cParseRegion(body.region);
      if (!region) return p2Bad("region 必须是 {x,y,w,h} 或 [x,y,w,h]");
      // **∴ `compare_with_reference` 只收布尔 ✗**（照服务端 `validate_args` 的
      //   `ParamKind::Boolean ⇒ value.is_boolean()` ✓）：**∴ 旧写法** `=== true`
      //   把 `"yes"` **静默当成没请求** ✗ —— **∴ 就是**「静默降级形态 4」✗
      //   （`read_canvas: "yes"` 静默当 false 的同一种 ✓）⇒ **∴ 服务端 400 点名参数，
      //   本地也必须 400 ✓**（**∴ 否则同一请求两边一个 400 一个 200 ✓**）。
      const flag = body.compare_with_reference;
      if (flag !== undefined && flag !== null && typeof flag !== "boolean") {
        const kind = Array.isArray(flag) ? "array" : typeof flag === "object" ? "object" : typeof flag;
        return p2Bad(`参数 compare_with_reference 类型应为 boolean，实际为 ${kind}`);
      }
      let rendered;
      try {
        rendered = await p2cRenderRgba(region.x, region.y, region.w, region.h);
      } catch (err) { return p2cKernel501(err); }
      if (rendered && rendered.error) return p2Bad(rendered.error);
      const pixels = rendered.rgba;
      const width = rendered.w, height = rendered.h;
      if (!pixels || pixels.length === 0) {
        // **∴ `PreconditionFailed ⇒ 409` ✗**（`http.rs:392` ✓）—— **∴ 不是 400 ✓**。
        return p2Precon("这块区域没有像素可分析");
      }
      // **∴ 16³ 桶 ✗**：每通道高 4 位 ✓；同时累计桶内**通道和** ✓（回桶内平均色 ✓）。
      const SIDE = 16;
      const counts = new Float64Array(SIDE * SIDE * SIDE);
      const sumsR = new Float64Array(SIDE * SIDE * SIDE);
      const sumsG = new Float64Array(SIDE * SIDE * SIDE);
      const sumsB = new Float64Array(SIDE * SIDE * SIDE);
      let brightness = 0.0;
      let warm = 0, cool = 0;
      const n4 = pixels.length - (pixels.length % 4);
      for (let index = 0; index < n4; index += 4) {
        const r = pixels[index], g = pixels[index + 1], b = pixels[index + 2];
        const bucket = ((r >> 4) << 8) | ((g >> 4) << 4) | (b >> 4);
        counts[bucket] += 1;
        sumsR[bucket] += r; sumsG[bucket] += g; sumsB[bucket] += b;
        // **∴ 运算顺序照 Rust ✗**：`((0.299r) + (0.587g)) + (0.114b)` ✓（**f64 逐位同源 ✓**）。
        brightness += 0.299 * r + 0.587 * g + 0.114 * b;
        if (r > b) warm += 1;
        else if (b > r) cool += 1;
      }
      const total = Math.max(1, Math.floor(pixels.length / 4));
      // **∴ 稳定排序（计数降序、桶号升序兜底 ✓）✗**：**∴ 不靠引擎的"稳定" ✓** ——
      //   **∴ 并列时谁在前必须与服务端（稳定 `sort_by`）一致 ✓** ⇒ **∴ 显式拿桶号兜底 ✓**。
      const order = [];
      for (let i = 0; i < counts.length; i++) if (counts[i] > 0) order.push(i);
      order.sort((a, b) => (counts[b] - counts[a]) || (a - b));
      const dominant = order.slice(0, 5).map((i) => {
        const count = Math.max(1, counts[i]);
        const r = Math.floor(sumsR[i] / count);
        const g = Math.floor(sumsG[i] / count);
        const b = Math.floor(sumsB[i] / count);
        const hex = "#" + [r, g, b].map((v) => v.toString(16).padStart(2, "0")).join("");
        return { r, g, b, hex, count, share: count / total };
      });
      // **∴ 与参考图比 ΔE ✗**（**只在明确要 ✓**；**比不了就说比不了 ✓**，**绝不静默给 0 ✗**）。
      let comparison = null;
      if (body.compare_with_reference === true) {
        const cmp = await p2cCompareWithReference(width, height, pixels);
        if (cmp && cmp.error) return p2Bad(/** @type {string} */ (cmp.error));
        comparison = cmp ? cmp.value : null;
      }
      return json({
        ok: true,
        server: false,
        width,
        height,
        pixels: total,
        comparison_with_reference: comparison,
        dominant_colors: dominant,
        // **0..1** ✓（÷255 ✓）
        avg_brightness: brightness / total / 255.0,
        warm_pixels: warm,
        cool_pixels: cool,
        // **暖/(暖+冷)** ✓ —— 恒在 0..1 ✓、不会除零 ✓（相等不计入任何一边 ✓）。
        warm_cool_ratio: (warm + cool === 0) ? 0.5 : warm / (warm + cool),
      });
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
      // **引用闭包与服务端同口径 ✓**（第 1440 轮审计：旧版只看 `payload.bitmap.blob_hash` ✗
      // ⇒ `import_image` 的图**不会被嵌进工程包** ✗ 且**不记 `skipped_blobs`** ✗ 静默丢数据 ✓）
      const refs = p2BlobRefsOfAtoms(atoms);
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
    //   `kind:"texture"`**优先存 `data_base64` 上传的字节 ✓**（**与前端与服务端口径一致 ✗
    //   `write_import_asset` 也存这同一份字节 ✓**）；**∴ 没给 `data_base64` 时**才退回按名字取随包纹理 ✓**
    //   （**并如实标 `source:"bundled"`✗ 不再冒充用户的上传 ✓**）；
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
      let bytes, source;
      if (typeof body.data_base64 === "string" && body.data_base64.length) {
        // **∴ 用户从本机选的字节 ✗**：**与前端 `importAssetFile` 与服务端 `write_import_asset` 同一口径 ✓**
        //   （**此前本分支**无视 `data_base64`✗ 去随包取同名文件** ⇒ **用户上传被静默丢弃 ✓ 是缺陷 ✗**）
        try {
          if (typeof Buffer !== "undefined" && Buffer.from) {
            bytes = new Uint8Array(Buffer.from(body.data_base64, "base64"));
          } else {
            const bin = atob(body.data_base64);
            bytes = new Uint8Array(bin.length);
            for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
          }
        } catch (err) {
          return json({ ok: false, error: "invalid_base64", server: false,
                        reason: "data_base64 无法解码 ⇒ " + String((err && err.message) || err) }, 400);
        }
        if (!bytes.length) {
          return json({ ok: false, error: "empty_upload", server: false,
                        reason: "data_base64 解出 0 字节" }, 400);
        }
        source = "upload";
      } else {
        // **∴ 没上传字节 ✗**：**才**按名字取随包纹理 ✓（**如实标 source ⇒ 不冒充用户的上传 ✓**）
        try {
          const res = await fetch("/textures/" + encodeURIComponent(name), { cache: "no-store" });
          if (!res.ok) {
            return json({ ok: false, error: "asset_not_found", server: false,
                          reason: name + " ⇒ HTTP " + res.status + "（且未提供 data_base64）" }, 404);
          }
          bytes = new Uint8Array(await res.arrayBuffer());
        } catch (err) {
          return json({ ok: false, error: "asset_not_found", server: false,
                        reason: name + " ⇒ " + String((err && err.message) || err) }, 404);
        }
        source = "bundled";
      }
      try {
        const { entry } = await p2cKernelFor();
        const raw = String(await withKernel(doc, () => entry.k.blob_put(bytes)) || "");
        let blobHash = raw;
        try { const o = JSON.parse(raw); if (o && o.blob_hash) blobHash = String(o.blob_hash); } catch (e) {}
        const { putBlob } = await import("./store.js");
        await putBlob(handle, blobHash, Array.from(bytes));
        return json({ ok: true, server: false, kind, name, source, blob_hash: blobHash, size: bytes.length,
                      note: source === "upload"
                        ? "已存进本地 blob 库的是你上传的字节（与前端与服务端口径一致）；texture_background 可直接用该 blob"
                        : "未提供 data_base64 ⇒ 存的是随包同名纹理" });
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
    //   **★ 三段参数校验照 `validate_args` ✓**（未知参数 → 必填 → 类型 ✓，见 `p2PresetSchemaCheck`）；
    //   **★ base 必须存在 ✓**：`log.rs:946 validate_declare_head` 在**提交期**查 ⇒
    //     原子缺失 `reference_not_found: declare_head 的 base 原子 X 不在日志中` ✓、
    //     检查点缺失 `检查点 X 不存在` ✓。**旧本地两样都不查** ✗ ⇒
    //     写进一条**解不出的跳变** ⇒ `state@seq` 在服务端会 400、本地会「看起来成功」 ✓。
    //   **★ 文案照服务端逐字 ✗**：类型错误那句**只说「atom 或 checkpoint」**（即使
    //     `declare_head` 也收 ✓）—— 照抄，不「顺手修正」✓。
    if (path === "/api/tools/declare_head") {
      let args;
      try {
        args = p2PresetSchemaCheck("declare_head", [
          { name: "base_type", kind: "string", required: true },
          { name: "base_id", kind: "string", required: true },
          { name: "reason", kind: "string", required: false },
        ], body);
      } catch (error) { const r = p2PresetResponse(error); if (r) return r; throw error; }
      const baseType = args.base_type;
      const baseId = args.base_id;
      if (["atom", "declare_head", "checkpoint"].indexOf(baseType) < 0) {
        return p2Bad("base_type 必须是 atom 或 checkpoint，得到 " + baseType);
      }
      const rows = await atomsOf(handle, doc);
      if (baseType === "checkpoint") {
        if (!p2FindCheckpointAtom(rows, baseId)) return p2Missing("检查点 " + baseId + " 不存在");
      } else if (!rows.some((r) => r.atom && r.atom.id === baseId)) {
        return p2Missing("declare_head 的 base 原子 " + baseId + " 不在日志中");
      }
      const payload = { base: { type: baseType, id: baseId } };
      if (typeof args.reason === "string") payload.reason = args.reason;
      const atom = p2Envelope("declare_head", payload);
      const seq = await putAtom(handle, doc, atom);
      return json({ ok: true, atom_id: atom.id, seq, head: seq, dirty_kind: "full", server: false,
                    note: "已写入 declare_head 原子；求值起点跳变由内核折叠层处理" });
    }

    // **P2c-37 `get_state` ✗**：**照 `read_get_state` ✓** ⇒ **从原子日志推导 ✓**
    //   （**`include_objects`／`include_hidden` 照服务端 ✓**）。
    if (path === "/api/tools/get_state") {
      const rows = await atomsOf(handle, doc);
      const st = p2cState(rows); // 内部按 `state@seq` 取窗口 ✓（跳变后旧段不在状态里 ✓）
      const includeObjects = body.include_objects === true;
      const includeHidden = body.include_hidden === true;
      const layers = st.layers.filter((l) => includeHidden || l.visible !== false);
      const out = { ok: true, server: false, doc_id: doc,
                    head_seq: rows.length ? rows[rows.length - 1].seq : 0,
                    width: st.width, height: st.height, color_space: st.color_space,
                    layers, checkpoints: st.checkpoints,
                    derived_from: "atoms" };
      // **`objects` 与 `list_objects` 同一份摘要 ✓**（服务端两处都走 `summary_object` ✓）：
      //   **∴ 旧本地给的是 5 字段小形状 ✗**（`visible` 常为 null、还带 `deleted`）
      //   ⇒ 同一对象在 `get_state` 与 `list_objects` 里**字段集不同** ✓；
      //   过滤也错：把 `deleted` 当可见性 ✗（服务端 `include_hidden` 过滤的是**对象属性 `visible`** ✓）。
      if (includeObjects) {
        out.objects = p2ObjectSummaries(rows, { includeHidden });
      }
      return json(out);
    }

    // **P2c-38 `get_resolved_state` ✓**：**照 `read_get_resolved_state` ✓** ——
    //   **必填 `object_id` ✓**，回**单个对象**的解析结果 ＋ **`bbox`** ✓
    //   （**∴ 旧实现回文档级载荷 ✗**：`object_id` 被无视、也没有 `bbox` ⇒ 与服务端**两套契约** ✗；
    //   **∴ 而 `bbox` 正是「包围盒宽高」唯一可观测面 ✓** —— `transform_object` 只回 `anchor` ✓）。
    //   **∴ `type` 照 `format!("{:?}").to_lowercase()` ✓** ⇒ **`raster_patch` ⇒ `rasterpatch` ✗**
    //   （**不能直接回本地 `view.type` ✓**）。
    //   **∴ 墓碑对象仍算「在状态里」✓**（`state.objects` 有它 ✓）⇒ 只有「创建被 revert」才是 404 ✓。
    if (path === "/api/tools/get_resolved_state") {
      for (const key of Object.keys(body)) {
        if (P2_FRAMEWORK_PARAMS.includes(key) || key === "object_id") continue;
        return p2Bad("get_resolved_state 不接受参数 " + key + "（拼写错误？）；可用参数：object_id" +
          "；框架级参数：" + P2_FRAMEWORK_PARAMS.join(", "));
      }
      const objectId = body.object_id;
      if (objectId === undefined) return p2Bad("缺少必填参数 object_id");
      if (typeof objectId !== "string") {
        return p2Bad("参数 object_id 类型应为 string，实际为 " + p2JsonTypeName(objectId));
      }
      const rows = await atomsOf(handle, doc);
      const neutral = p2Neutralized(rows);
      const layers = p2PresetLayers(rows, neutral);
      const view = p2ObjectView(rows, objectId);
      if (!view.found || neutral.has(view.__create_id)) {
        return p2Missing("对象 " + objectId + " 不存在");
      }
      const data = p2EffectiveData(view);
      const masterRef = (data.master_ref && typeof data.master_ref === "object" &&
        !Array.isArray(data.master_ref)) ? data.master_ref : null;
      const masterId = masterRef && typeof masterRef.object_id === "string"
        ? masterRef.object_id : null;
      const localTransform = masterRef && masterRef.local_transform !== undefined
        ? masterRef.local_transform : null;
      const bbox = p2ObjectBBox(rows, view, layers, neutral);
      return json({ ok: true, server: false, object_id: objectId,
                    type: P2_SERVER_TYPE_NAME[view.type] || String(view.type || "").toLowerCase(),
                    is_instance: view.type === "instance",
                    master_id: masterId,
                    local_transform: localTransform,
                    bbox: bbox ? [bbox.x, bbox.y, bbox.w, bbox.h] : null });
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
        // **用被渲染状态的序号盖章 ✓**（第 1440 轮审计：旧版**渲染完再重读 head** ✗
        // ⇒ 中间落了新原子 ⇒ 旧图盖新号 ⇒ 冒充新状态 ✓；`writeSnapshot` 现在
        // 单事务 CAS ⇒ 拒绝写时**如实报 `snapshot_written:false`** ✓，不静默 ✓）。
        const seqRendered = rows.length ? rows[rows.length - 1].seq : 0;
        const written = await writeSnapshot(handle, doc, seqRendered, { bytes: png });
        return json({ ok: true, server: false, doc_id: doc, seq: seqRendered,
                      width: w, height: h, bytes: png.length,
                      snapshot_written: written,
                      note: written ? "整幅预览已渲染并写入本地快照"
                                    : "快照未写：渲染之后又有新原子进来 ⇒ 调用方按最新 head 重算 ✓" });
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
    //   **引用集合从原子日志推导 ✓**（**与服务端同口径：递归收 `blob_hash`/`blob` 键 ✓**
    //   —— 第 1440 轮审计：旧版只看 `payload.bitmap.blob_hash` ✗ ⇒ `import_image` 的
    //   `payload.blob_hash` 等**全部隐形** ✗ ⇒ `confirm:true` **删在用的 blob** ✗ 数据丢失 ✓）；
    //   **`confirm:true` 才真删 ✓**。
    if (path === "/api/tools/collect_garbage") {
      const confirm = body.confirm === true;
      const rows = await atomsOf(handle, doc);
      const refs = p2BlobRefsOfAtoms(rows.map((r) => r.atom));
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
      const refs = p2BlobRefsOfAtoms(rows.map((r) => r.atom));
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
      const sinceArg = p2Count(body.since_seq, "since_seq");
      if (sinceArg.error) return sinceArg.error;
      const limitArg = p2Count(body.limit, "limit");
      if (limitArg.error) return limitArg.error;
      const since = sinceArg.value !== undefined ? sinceArg.value : 0;
      const requestedLimit = limitArg.value !== undefined ? limitArg.value : 200;
      const limit = Math.min(2000, requestedLimit);
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
                    total_matched: matched.length, truncated: matched.length > limit,
                    limit, limit_requested: requestedLimit,
                    limit_clamped: requestedLimit > limit,
                    next_since: atoms.length ? atoms[atoms.length - 1].seq : null });
    }

    // **P2c-51 `get_atom` ✗**：**照 `read_get_atom` ✓** ⇒ **按 `atom_id` 取整条原子 ✓**。
    if (path === "/api/tools/get_atom") {
      // **入口校验照服务端 `validate_args` ✓**（缺 `atom_id`／未知键都由入口拒 ✓）。
      const specBad = p2ValidateArgsFor("get_atom", body);
      if (specBad) return p2ToolFailure(400, specBad.code, specBad.detail);
      const atomId = body.atom_id;
      if (typeof atomId !== "string" || !atomId) return p2Bad("atom_id 必填（字符串）");
      const rows = await atomsOf(handle, doc);
      const found = rows.find((r) => r.atom && r.atom.id === atomId);
      if (!found) return p2Missing("原子 " + atomId + " 不在日志里");
      const a = found.atom;
      // **字段照服务端 `read_get_atom` ✓**（`tools.rs:21030`）：`message`／`label`／
      //   `changeset_id`／`parents` **一直都有** ✓ ⇒ **∴ 旧本地只给六个字段 ✗** ⇒ 调用方读不到标签 ✓。
      return json({ ok: true, server: false, atom_id: a.id, seq: found.seq, kind: a.kind,
                    actor: a.actor === undefined ? null : a.actor,
                    session: a.session === undefined ? null : a.session,
                    timestamp: a.timestamp === undefined ? null : a.timestamp,
                    message: a.message === undefined ? null : a.message,
                    label: a.label === undefined ? null : a.label,
                    changeset_id: a.changeset_id === undefined ? null : a.changeset_id,
                    parents: Array.isArray(a.parents) ? a.parents : [],
                    payload: a.payload === undefined ? null : a.payload });
    }

    // **P2c-56 `get_stroke_log` ✓**（**照 `read_get_stroke_log` ✓**，`tools.rs:8818`）：
    //   **∴ 一笔一行的可审计视图 ✓**：只列**笔触**（`source.kind` ∈ brush／medium／draw ✓），
    //   坐标原样回 ✓，压力给 min/max/first/last ✓；已撤销的按「最近一次 revert／reapply」排除 ✓。
    //   **∴ 与 `list_atoms` 的口径差 ✓**：这条**认 Reapply**（撤了又回来算 live ✓），
    //   `list_atoms` 只认 Revert（**那处不改** —— 服务端就是如此 ✓）。
    if (path === "/api/tools/get_stroke_log") {
      const specBad = p2ValidateArgsFor("get_stroke_log", body);
      if (specBad) return p2ToolFailure(400, specBad.code, specBad.detail);
      const countOf = (key, fallback, max) => {
        const arg = p2Count(body[key], key);
        if (arg.error) return arg;
        const v = arg.value === undefined ? fallback : arg.value;
        return { value: max == null ? v : Math.min(v, max) };
      };
      const limitArg = countOf("limit", 50, 500);
      if (limitArg.error) return limitArg.error;
      const offsetArg = countOf("offset", 0);
      if (offsetArg.error) return offsetArg.error;
      const sinceArg = countOf("since_seq", 0);
      if (sinceArg.error) return sinceArg.error;
      const limit = limitArg.value, offset = offsetArg.value, since = sinceArg.value;
      const strOf = (key) => (typeof body[key] === "string" ? body[key] : null);
      const layerFilter = strOf("layer_id");
      const objectFilter = strOf("object_id");
      const groupFilter = strOf("group_id");
      const newestFirst = strOf("order") === "newest";
      const rows = await atomsOf(handle, doc);
      // **最近一次动作胜出 ✓**（服务端 `last_action` ✓）：同一目标的 revert 后又 reapply ⇒ live ✓。
      const lastAction = new Map();
      for (const r of rows) {
        const a = (r && r.atom) || {};
        if (a.kind !== "revert" && a.kind !== "reapply") continue;
        const target = a.payload && a.payload.target;
        if (typeof target === "string") {
          lastAction.set(target, { seq: Number(r.seq) || 0, revert: a.kind === "revert" });
        }
      }
      const reverted = new Set();
      for (const [id, act] of lastAction) if (act.revert) reverted.add(id);
      let revertedExcluded = 0;
      const all = [];
      for (const r of rows) {
        const a = (r && r.atom) || {};
        const seq = Number(r.seq) || 0;
        if (seq <= since) continue;
        const pl = a.payload || {};
        let source = null;
        const ps = pl.source;
        if (ps && typeof ps === "object" && !Array.isArray(ps)) {
          source = ps;
        } else {
          // **普通矢量笔触的点列在 `payload.data` 里 ✓**（服务端第 2026-10-16 轮修的那条 ✓）。
          if (a.kind !== "draw_stroke" && a.kind !== "erase") continue;
          const data = pl.data;
          if (!(data && typeof data === "object" && !Array.isArray(data))) continue;
          source = {
            kind: "draw",
            points: Object.prototype.hasOwnProperty.call(data, "points") ? data.points : null,
            color: p2NormalizeColor(data.color),
            size: Object.prototype.hasOwnProperty.call(data, "size") ? data.size : null,
          };
        }
        const sourceKind = typeof source.kind === "string" ? source.kind : "";
        if (sourceKind !== "brush" && sourceKind !== "medium" && sourceKind !== "draw") continue;
        const layerId = p2PayloadTopStr(a, "layer_id");
        if (layerFilter !== null && layerId !== layerFilter) continue;
        if (objectFilter !== null && p2PayloadTopStr(a, "object_id") !== objectFilter) continue;
        if (groupFilter !== null && (a.group_id === undefined ? null : a.group_id) !== groupFilter) continue;
        if (reverted.has(a.id)) { revertedExcluded += 1; continue; }
        const points = Object.prototype.hasOwnProperty.call(source, "points") ? source.points : [];
        const pointList = Array.isArray(points) ? points : [];
        const coordinate = (item) => {
          if (item === undefined || item === null) return null;
          if (!Array.isArray(item)) return null;
          const num = (v) => (typeof v === "number" && Number.isFinite(v) ? v : null);
          return { x: num(item[0]), y: num(item[1]) };
        };
        const pressures = [];
        for (const item of pointList) {
          if (Array.isArray(item) && typeof item[2] === "number" && Number.isFinite(item[2])) {
            pressures.push(item[2]);
          }
        }
        const pressure = pressures.length === 0 ? null : {
          min: Math.min(...pressures), max: Math.max(...pressures),
          first: pressures[0], last: pressures[pressures.length - 1],
        };
        const opt = (obj, key) => (obj && Object.prototype.hasOwnProperty.call(obj, key)
                                    && obj[key] !== undefined ? obj[key] : null);
        all.push({
          seq,
          atom_id: a.id,
          kind: sourceKind,
          layer_id: layerId,
          object_id: p2PayloadTopStr(a, "object_id"),
          message: a.message === undefined ? null : a.message,
          brush: opt(source, "brush"),
          medium: opt(source, "medium"),
          size: opt(source, "size"),
          color: p2NormalizeColor(source.color),
          color_to: opt(source, "color_to"),
          hardness: opt(source, "hardness"),
          opacity: opt(source, "opacity"),
          smooth: opt(source, "smooth"),
          seed: opt(source, "seed"),
          points,
          points_count: pointList.length,
          bbox: Object.prototype.hasOwnProperty.call(pl, "region") ? pl.region : null,
          first_point: coordinate(pointList[0]),
          last_point: coordinate(pointList[pointList.length - 1]),
          pressure,
          timestamp: a.timestamp === undefined ? null : a.timestamp,
        });
      }
      if (newestFirst) all.reverse();
      const total = all.length;
      const page = all.slice(offset, offset + limit);
      const returned = page.length;
      return json({
        ok: true, server: false,
        entries: page,
        returned,
        total,
        reverted_excluded: revertedExcluded,
        offset,
        limit,
        next_offset: offset + returned < total ? offset + returned : null,
        truncated: offset + returned < total,
        note: "这是**原子日志**的视图 ✓ ⇒ 跟文档走（清 /tmp、换 session、重启都在 ✓）。"
          + "                 `bbox` 是原子里的 `region` ✓；逐点 `time_ms` **没有**保存在原子里 ✗"
          + " ⇒ 只有原子时间戳 ✓。                 已撤销的原子不列，`total` 只数 live 笔；"
          + "`reverted_excluded` 是被排除的已撤销笔数（`total + reverted_excluded` = 全部匹配数）✓。",
      });
    }

    // **P2c-57 `list_atoms` ✓**（**照 `read_list_atoms` ✓**，`tools.rs:20779`）：
    //   **∴ 按 seq 倒序 ✓**（新的在前 ✓）＋ label／layer／brush／group／bbox／since 六条筛选 ✓
    //   ＋ `offset`／`total`／`has_more` 真分页 ✓ ＋ `origin`／`stored_origin` 来源口径 ✓
    //   ＋ 非笔迹原子**缺省藏起来但**数出来 ✓（`hidden_non_stroke` —— 「不是没有，是被藏了」✓）。
    if (path === "/api/tools/list_atoms") {
      const specBad = p2ValidateArgsFor("list_atoms", body);
      if (specBad) return p2ToolFailure(400, specBad.code, specBad.detail);
      const countOf = (key, fallback, max) => {
        const arg = p2Count(body[key], key);
        if (arg.error) return arg;
        const v = arg.value === undefined ? fallback : arg.value;
        return { value: max == null ? v : Math.min(v, max) };
      };
      const limitArg = countOf("limit", 20, 5000);
      if (limitArg.error) return limitArg.error;
      const offsetArg = countOf("offset", 0);
      if (offsetArg.error) return offsetArg.error;
      const sinceArg = countOf("since_seq", 0);
      if (sinceArg.error) return sinceArg.error;
      const thumbArg = countOf("thumb_size", 0);
      if (thumbArg.error) return thumbArg.error;
      const limit = limitArg.value, offset = offsetArg.value, since = sinceArg.value;
      const askedThumb = thumbArg.value;
      // **不许 `as u32` 回绕成 0 ✗**（服务端第 738 轮；外部 S-12）⇒ 超限**响亮报错** ✓。
      if (askedThumb > 4294967295) {
        return p2Bad("thumb_size 太大（" + askedThumb + "）⇒ 缩略图边长上限 4294967295（不许回绕成 0）");
      }
      const thumbSize = askedThumb;
      const wantThumb = body.want_thumb === undefined || body.want_thumb === null
        ? true : body.want_thumb === true;
      const includeNonStroke = body.include_non_stroke === true;
      const strOf = (key) => (typeof body[key] === "string" ? body[key] : null);
      const wantLabel = strOf("label");
      const wantLayer = strOf("layer_id");
      const wantGroup = strOf("group_id");
      const wantBrush = strOf("brush") === null ? null : strOf("brush").toLowerCase();
      let wantBbox = null;
      if (Array.isArray(body.bbox) && body.bbox.length === 4) {
        const nums = body.bbox.filter((v) => typeof v === "number" && Number.isFinite(v));
        if (nums.length === 4) wantBbox = nums;
      }
      const rows = await atomsOf(handle, doc);
      // **已撤销的不列 ✓**（只认 `revert` ✓；`reapply` 回来的仍隐藏 ✓ —— 服务端原样 ✓）。
      const reverted = new Set();
      for (const r of rows) {
        const a = (r && r.atom) || {};
        if (a.kind !== "revert") continue;
        const target = a.payload && a.payload.target;
        if (typeof target === "string") reverted.add(target);
      }
      let matched = 0;
      let hiddenNonStroke = 0;
      const hits = [];
      for (let i = rows.length - 1; i >= 0; i -= 1) {
        const r = rows[i];
        const a = (r && r.atom) || {};
        const seq = Number(r.seq) || 0;
        const pl = a.payload || {};
        if (seq <= since) continue;
        const groupId = a.group_id === undefined ? null : a.group_id;
        if (wantGroup !== null && groupId !== wantGroup) continue;
        const label = a.label === undefined ? null : a.label;
        if (wantLabel !== null && label !== wantLabel) continue;
        if (wantLayer !== null && p2PayloadStr(a, "layer_id") !== wantLayer) continue;
        if (reverted.has(a.id)) continue;
        if (wantBbox !== null) {
          const region = pl.region;
          const num = (key) => {
            const v = region && typeof region === "object" ? region[key] : undefined;
            return typeof v === "number" ? v : NaN;
          };
          const [rx, ry, rw, rh] = [num("x"), num("y"), num("w"), num("h")];
          if (!Number.isFinite(rx) || !Number.isFinite(ry)
              || !Number.isFinite(rw) || !Number.isFinite(rh)) continue;
          const apart = rx + rw < wantBbox[0] || wantBbox[0] + wantBbox[2] < rx
            || ry + rh < wantBbox[1] || wantBbox[1] + wantBbox[3] < ry;
          if (apart) continue;
        }
        if (wantBrush !== null) {
          const brush = (p2PayloadStr(a, "brush") || "").toLowerCase();
          if (!brush.includes(wantBrush)) continue;
        }
        if (!includeNonStroke && pl.source === undefined) {
          hiddenNonStroke += 1;
          continue;
        }
        matched += 1;
        if (matched <= offset) continue;
        if (hits.length > limit) continue;
        const storedOrigin = p2PayloadStr(a, "origin");
        hits.push({
          atom_id: a.id,
          seq,
          kind: a.kind,
          origin: storedOrigin !== null ? storedOrigin : p2AtomOriginOf(a.kind, pl),
          stored_origin: storedOrigin,
          label,
          group_id: groupId,
          layer_id: p2PayloadStr(a, "layer_id"),
          brush: p2PayloadStr(a, "brush"),
          timestamp: a.timestamp === undefined ? null : a.timestamp,
          region: Object.prototype.hasOwnProperty.call(pl, "region") ? pl.region : null,
        });
      }
      // **缩略图 ✓**（服务端每条一次区域渲染 ✓）：单次最多 20 张 ✓、区域 >256×256 跳过 ✓、
      //   渲染失败**不列也不计入 skipped** ✓（服务端 `if let Ok(...)` 同义 ✓）。
      let thumbs = 0;
      const skipped = [];
      if (wantThumb) {
        for (const hit of hits) {
          const region = hit.region;
          const num = (key) => {
            const v = region && typeof region === "object" ? region[key] : undefined;
            return typeof v === "number" ? v : 0;
          };
          const rx = num("x"), ry = num("y"), rw = num("w"), rh = num("h");
          if (thumbs >= 20 || rw <= 0 || rh <= 0 || rw > 256 || rh > 256) {
            skipped.push(hit.atom_id);
            continue;
          }
          try {
            const rendered = await p2cRenderRgba(rx, ry, rw, rh);
            if (rendered && !rendered.error && rendered.w > 0 && rendered.h > 0) {
              let ew = rendered.w, eh = rendered.h, bytes = rendered.rgba;
              if (thumbSize > 0 && (ew > thumbSize || eh > thumbSize)) {
                const long = Math.max(ew, eh);
                const nw = Math.max(1, Math.floor(ew * thumbSize / long));
                const nh = Math.max(1, Math.floor(eh * thumbSize / long));
                const small = new Uint8Array(nw * nh * 4);
                for (let yy = 0; yy < nh; yy += 1) {
                  const sy = Math.min(eh - 1, Math.floor(yy * eh / nh));
                  for (let xx = 0; xx < nw; xx += 1) {
                    const sx = Math.min(ew - 1, Math.floor(xx * ew / nw));
                    const si = (sy * ew + sx) * 4;
                    const di = (yy * nw + xx) * 4;
                    small.set(bytes.subarray(si, si + 4), di);
                  }
                }
                ew = nw; eh = nh; bytes = small;
              }
              const png = await encodePngRgba(ew, eh, bytes);
              if (png) { hit.thumbnail = p2cB64(png); thumbs += 1; }
            }
          } catch (e) { /* 与服务端同义：渲染失败即跳过（不冒充缩略图）✓ */ }
        }
      }
      const hasMore = limit > 0 && hits.length > limit;
      hits.length = Math.min(hits.length, limit);
      return json({
        ok: true, server: false,
        atoms: hits,
        count: hits.length,
        total: matched,
        hidden_non_stroke: hiddenNonStroke,
        offset,
        truncated: hasMore,
        has_more: hasMore,
        limit,
        thumbnail_count: thumbs,
        thumbnail_skipped: skipped,
        note: "按 seq 倒序（新的在前）；用 `offset` 翻页，`truncated`/`has_more` 为真表示满页；"
          + "**`kind` 是存储形态**（笔触在库里也是 `import_image`）⇒ 判来源请看 `origin`"
          + "（drawn/imported/generated/system）",
      });
    }

    // **P2c-52 `get_ancestors` ✗**：**服务端走实例→master 链 ✓** ⇒ **∴ 本地从日志推导
    //   `master_ref` 链 ✓**（**成环即停并标记 ✓**）。
    //   **∴ 与服务端 `read_get_ancestors`（`tools.rs:8579`）逐条对齐 ✗**（本轮修 ✓）：
    //   · **对象不在折叠状态 ⇒ `reference_not_found` ✗**（**旧本地静默给 `ok:true, count:0` ✓**
    //     —— **「不存在」与「没有祖先」是两个答案 ✗ 混起来就是静默降级形态 4 ✓**）；
    //     **判定 = 有创建原子且创建未被抑制 ✗**（**∴ 照服务端 `state.objects.contains_key` ✓——
    //     折叠状态**含**被 tombstone 标记的对象 ✓：实测服务端对 `deleted_by` 有值的对象照样答 ✓**）；
    //   · **环检测照服务端 `visited`（初始为空 ✗ 不含查询对象自己 ✓）**——
    //     **∴ 自引用 master 服务端先记一条正常链再记 `cycle:true` ✗（两条 ✓）**，
    //     旧本地把 `seen` 起点设为自己 ⇒ 只给一条 ✓ 数都对不上 ✗。
    if (path === "/api/tools/get_ancestors") {
      const objectId = body.object_id;
      if (typeof objectId !== "string" || !objectId) return p2Bad("object_id 必填（字符串）");
      const rows = await atomsOf(handle, doc);
      const neutral = p2Neutralized(rows);
      const selfView = p2ObjectView(rows, objectId);
      if (!selfView.found || neutral.has(selfView.__create_id)) return p2Missing("对象 " + objectId + " 不存在");
      const chain = [];
      const visited = new Set();
      let cur = objectId;
      for (let i = 0; i < 64; i++) {
        const v = p2ObjectView(rows, cur);
        const master = v.found && v.master_ref && typeof v.master_ref.object_id === "string"
          ? v.master_ref.object_id : null;
        if (!master) break;
        if (visited.has(master)) { chain.push({ object_id: master, cycle: true }); break; }
        visited.add(master);
        chain.push({ object_id: master });
        cur = master;
      }
      return json({ ok: true, server: false, object_id: objectId, ancestors: chain,
                    count: chain.length, derived_from: "atoms" });
    }

    // **P2c-53 `get_descendants` ✗**：**`get_ancestors` 的反向 ✓** ⇒ **∴ 扫描所有对象的
    //   `master_ref` 找指向该对象的实例 ✓**。
    //   **∴ 与服务端 `read_get_descendants` + `dependents_of`（`tools.rs:8610`／
    //   `dirty.rs:473`）逐条对齐 ✗**（本轮修 ✓）：
    //   · **对象不在折叠状态 ⇒ `reference_not_found` ✗**（**∴ 同上 ✓**）；
    //   · **∴ 返回的是 id 字符串数组 ✗**（**`Vec<String>` ✓**）—— **旧本地给 `{object_id,layer_id}` 对象 ✓
    //     ⇒ 按服务端形状写的调用方在本地层拿到的是错形状 ✓（静默 ✗：`ok:true` 看着都对 ✓）**；
    //   · **∴ 只收 `type === "instance"` 的 ✓、传递闭包（BFS ≤ 64 层 ✓）✗**
    //     —— **旧本地只收一层 ✗（孙实例漏掉 ✓）**；**tombstone 标记的实例照服务端不排除 ✓**
    //     （**`dependents_of` 只看 `state.objects` 里有没有 ✗ 不看 `deleted_by` ✓**）。
    if (path === "/api/tools/get_descendants") {
      const objectId = body.object_id;
      if (typeof objectId !== "string" || !objectId) return p2Bad("object_id 必填（字符串）");
      const rows = await atomsOf(handle, doc);
      const neutral = p2Neutralized(rows);
      const selfView = p2ObjectView(rows, objectId);
      if (!selfView.found || neutral.has(selfView.__create_id)) return p2Missing("对象 " + objectId + " 不存在");
      const st = p2cState(rows);
      const views = new Map();
      for (const o of st.objects) {
        if (!views.has(o.object_id)) views.set(o.object_id, p2ObjectView(rows, o.object_id));
      }
      const masterOf = (id) => {
        const v = views.get(id);
        return v && v.found && !neutral.has(v.__create_id)
          && v.type === "instance" && v.master_ref && typeof v.master_ref.object_id === "string"
          ? v.master_ref.object_id : null;
      };
      const out = [];
      let frontier = [objectId];
      const visited = new Set([objectId]);
      for (let hop = 0; hop < 64 && frontier.length; hop++) {
        const next = [];
        for (const id of views.keys()) {
          if (visited.has(id)) continue;
          const m = masterOf(id);
          if (m !== null && frontier.includes(m)) {
            visited.add(id);
            out.push(id);
            next.push(id);
          }
        }
        frontier = next;
      }
      return json({ ok: true, server: false, object_id: objectId, descendants: out,
                    count: out.length, derived_from: "atoms" });
    }

    // **P2c-54 `get_diff` ✗**：**照 `read_get_diff` ✓** ⇒ **`(from_seq, to_seq]` 的日志差分 ✓** ＋
    //   **种类汇总 ＋ 涉及的对象／图层集合 ✓**。
    if (path === "/api/tools/get_diff") {
      const rows = await atomsOf(handle, doc);
      const head = rows.length ? rows[rows.length - 1].seq : 0;
      const fromArg = p2Count(body.from_seq, "from_seq");
      if (fromArg.error) return fromArg.error;
      const toArg = p2Count(body.to_seq, "to_seq");
      if (toArg.error) return toArg.error;
      const fromSeq = fromArg.value !== undefined ? fromArg.value : 0;
      const toSeq = toArg.value !== undefined ? toArg.value : head;
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
      // **字段名与服务端 `read_get_diff` 对齐** ✓（第 740 轮；外部审计 S-07 第 ④ 项）
      //   **⚠ 我第 735 轮改的是 `web/api-local.js`** ✗ —— 那个文件是 `pwa-sync-viewer.mjs`
      //   **从 `crates/yanshi-http/assets/` 拷出来的** ✓ ⇒ CI 一 sync 就把我的改动**覆盖回旧版** ✓
      //   ⇒ 判据在 CI 上红（`✗ atom_count`）而本机绿 ✓ ⇒ **改到源上**才是真的 ✓。
      const limitArg = p2Count(body.limit, "limit");
      if (limitArg.error) return limitArg.error;
      const page = limitArg.value !== undefined ? inRange.slice(0, limitArg.value) : inRange;
      return json({ ok: true, server: false, doc_id: doc, from_seq: fromSeq, to_seq: toSeq,
                    atoms: page.map((r) => ({ atom_id: r.atom.id, seq: r.seq, kind: r.atom.kind })),
                    head_seq: head,
                    atom_count: inRange.length, by_kind: byKind,
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
      // **★ 倍率 `scale` ✗ ★**（本轮 ✓）：**服务端 `write_resample` 认 `scale`（`tools.rs:9538` ✓）
      //   而前端 6483 只发 `scale` ✗ ⇒ 本地此前**只读 width/height**✗ ⇒ `scale` 被整个丢掉 ✓
      //   且下面 `toW = targetW || fromW` ⇒ **静默按原尺寸重采样、冒充 `ok:true`** ✗（又一个静默降级 ✓）。
      const scaleNum = (body.scale !== undefined && body.scale !== null) ? Number(body.scale) : null;
      if (scaleNum !== null && !(scaleNum > 0)) {
        return p2Bad("scale 必须是正数（给了就是倍率，> 1 放大、< 1 缩小）");
      }
      // **∴ 「width+height」与「scale」二者必居其一 ✗**（**照服务端 `write_resample` 的 match ✗ 都不给就报错 ✓ `tools.rs:9547`**）：
      //   **∴ 为什么放在 `try` 之前 ✗**：**这条**不依赖源尺寸**⇒ 纯请求校验 ⇒ node 判据也能验到 ✓**（**下面缩放要浏览器 canvas ✗ node 里到不了 ✓**）
      //   **∴ 而**旧行为 ✗：二者都不给 ⇒ `toW = targetW || fromW` ⇒ **静默按原尺寸重采样、冒充 `ok:true`** ✓（不报错、不缩放 ✓）
      if (!((targetW !== null && targetH !== null) || scaleNum !== null)) {
        return p2Bad("resample 需要 width 与 height，或 scale（二者之一，明确给）⇒ 二者都没给，不猜");
      }
      try {
        const g = typeof globalThis !== "undefined" ? globalThis : {};
        if (typeof g.createImageBitmap !== "function" || !g.document) {
          throw new Error("位图缩放需要浏览器环境的 createImageBitmap（当前环境没有）");
        }
        const { getBlob } = await import("./store.js");
        const blobBytes = await getBlob(handle, blobHash);
        if (!blobBytes) return p2Missing("blob " + blobHash + " 不在本地 blob 库里");
        // **原始 RGBA 位图不能当 PNG 喂给 `createImageBitmap`** ✓（第 742 轮；外部审计 S-11）
        //   **⇒ 缺陷**：本仓自己的位图是 `image/x-yanshi-raw`（**裸 RGBA**）✓，而这段代码
        //     **硬编码** `{ type: "image/png" }` ✗ ⇒ `createImageBitmap` **解析失败** ✓
        //     ⇒ 本地重采样对**自家位图永远不工作** ✓（错误还被 `p2cKernel501` 包成 501 ✓，
        //     看起来像"内核不支持"✓ —— 最误导的一种 ✓）。
        //   **⇒ 修法**：按 `bitmap.mime_type` 分流 ✓ —— raw ⇒ 自己拼 `ImageData`（尺寸取对象里的
        //     `width`/`height` ✓）；其它（真 PNG／JPEG）⇒ 仍走 `createImageBitmap` ✓。
        const mime = String((bitmap && bitmap.mime_type) || "");
        let srcBmp = null;
        let srcImageData = null;
        let fromW = 0, fromH = 0;
        if (mime === "image/x-yanshi-raw") {
          fromW = Math.floor(Number(data.width) || 0);
          fromH = Math.floor(Number(data.height) || 0);
          if (!(fromW > 0 && fromH > 0)) {
            return p2Bad("原始位图缺少 width/height ⇒ 无法确定尺寸（对象 " + objectId + "）");
          }
          const expected = fromW * fromH * 4;
          if (blobBytes.length !== expected) {
            return p2Bad(
              "原始位图大小不对：期望 " + expected + " 字节（" + fromW + "×" + fromH + "×4），实测 "
                + blobBytes.length + " ⇒ 拒绝按 PNG 猜",
            );
          }
          srcImageData = new g.ImageData(new Uint8ClampedArray(blobBytes), fromW, fromH);
        } else {
          srcBmp = await g.createImageBitmap(new Blob([new Uint8Array(blobBytes)], { type: mime || "image/png" }));
          fromW = srcBmp.width; fromH = srcBmp.height;
        }
        // **∴ 目标尺寸优先级 ✗**（**与服务端 `write_resample` 一致 ✓ `tools.rs:9541` ✓**）：
        //   **显式 width/height（两个都给）优先 ⇒ 其次 `scale`×源尺寸 ⇒ 二者都没给就明确报错 ✗ 不静默按原尺寸 ✓**
        let toW = targetW, toH = targetH;
        if ((toW === null || toH === null) && scaleNum !== null) {
          toW = Math.max(1, Math.round(fromW * scaleNum));
          toH = Math.max(1, Math.round(fromH * scaleNum));
        }
        if (toW === null || toH === null) {
          return p2Bad("resample 需要 width 与 height，或 scale（二者之一，明确给）⇒ 二者都没给，不猜（此前会静默按原尺寸重采样、冒充成功 ✓）");
        }
        // **∴ 目标尺寸也受画布上限约束 ✗**（**照服务端 S-06 ✓ `tools.rs:9563` ✓**）：
        //   **超大倍率会在浏览器里 `createElement("canvas")` 直接吃下巨大内存 ✓ ⇒ 同样该拒 ✗**
        const MAX_EDGE = 16384, MAX_PIXELS = 1 << 28;
        if (toW > MAX_EDGE || toH > MAX_EDGE || toW * toH > MAX_PIXELS) {
          return p2Bad("resample 目标 " + toW + "×" + toH + " 超过画布上限（单边 " + MAX_EDGE +
                       "／总像素 " + MAX_PIXELS + "）⇒ 这个尺寸必然把浏览器撑爆，所以不做");
        }

        const canvas = g.document.createElement("canvas");
        canvas.width = toW; canvas.height = toH;
        const c2d = canvas.getContext("2d", { willReadFrequently: true });
        c2d.imageSmoothingEnabled = filterName === "bilinear";
        if (srcImageData) {
          // 先把裸 RGBA 画进一张**源尺寸**的画布，再缩放画到目标画布 ✓
          const srcCanvas = g.document.createElement("canvas");
          srcCanvas.width = fromW; srcCanvas.height = fromH;
          srcCanvas.getContext("2d").putImageData(srcImageData, 0, 0);
          c2d.drawImage(srcCanvas, 0, 0, toW, toH);
        } else {
          c2d.drawImage(srcBmp, 0, 0, toW, toH);
          if (srcBmp.close) srcBmp.close();
        }
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
      let args;
      try {
        args = p2PresetSchemaCheck("revert",
          [{ name: "atom_id", kind: "string", required: true }], body);
      } catch (error) { const r = p2PresetResponse(error); if (r) return r; throw error; }
      const atomId = args.atom_id;
      const rows = await atomsOf(handle, doc);
      const target = rows.find((r) => r.atom && r.atom.id === atomId);
      if (!target) return p2Missing("目标原子 " + atomId + " 不在日志中");
      // **跨求值起点的撤销 ✗**（`log.rs:893` ✓）：目标在 `declare_head` 之前 ⇒
      //   **它已经烘进 base_state，折叠无法只凭表达式撤销它** ✓ ⇒ 服务端**提交期就拒绝** ✓。
      //   **∴ 本地旧实现不查** ✗ ⇒ `restore_checkpoint` 之后 revert 旧原子：服务端 400、本地「成功」✓。
      const origin = p2EvalOriginSeq(rows);
      if (typeof target.seq === "number" && target.seq <= origin) {
        return p2Bad("目标原子 " + atomId + " (seq " + target.seq + ") 位于当前求值起点 seq "
          + origin + " 之前，需先回到该起点或重新提交等价操作");
      }
      const seq = await putAtom(handle, doc, p2Envelope("revert", { target: atomId }));
      return json({ ok: true, seq, server: false, reverted: atomId,
                    note: "已写入 revert 原子；是否撤销由内核折叠层判定" });
    }

    // **P2c-60 `revert_to` ✗**：**照 `write_revert_to` ✓** ⇒ **`declare_head{base:{type:"atom", id}}` ✓**。
    //   **∴ 目标不存在时服务端报的是 `validate_declare_head` 那句**（`declare_head 的 base 原子 X 不在日志中` ✓），
    //   不是 `revert` 那句「目标原子 X 不在日志中」✗（`log.rs:867`）—— 旧本地抄错了对象 ✓。
    //   **∴ 本轮顺带修掉 `revert`／`reapply` 的缺目标文案 ✓**：旧本地写「不在日志里」✗ ⇒
    //   服务端 `validate_history_refs` 是「不在日志**中**」✓（`get_atom` 那类才用「里」✓，`tools.rs:20283`）。
    //   **∴ `head` 回**提交后的 head seq**✓**（`commit_response` 的语义 ✓），不是原子 id ✗。
    if (path === "/api/tools/revert_to") {
      let args;
      try {
        args = p2PresetSchemaCheck("revert_to",
          [{ name: "atom_id", kind: "string", required: true }], body);
      } catch (error) { const r = p2PresetResponse(error); if (r) return r; throw error; }
      const atomId = args.atom_id;
      const rows = await atomsOf(handle, doc);
      if (!rows.some((r) => r.atom && r.atom.id === atomId)) {
        return p2Missing("declare_head 的 base 原子 " + atomId + " 不在日志中");
      }
      const atom = p2Envelope("declare_head", {
        base: { type: "atom", id: atomId }, reason: "revert_to",
      });
      const seq = await putAtom(handle, doc, atom);
      return json({ ok: true, atom_id: atom.id, seq, head: seq, dirty_kind: "full", server: false,
                    note: "已写入 declare_head 原子（求值起点跳变由内核折叠层处理）" });
    }

    // **P2c-61 `reapply` ✗**：**照 `write_history_atom(…, Reapply, "atom_id")` ✓**。
    if (path === "/api/tools/reapply") {
      let args;
      try {
        args = p2PresetSchemaCheck("reapply",
          [{ name: "atom_id", kind: "string", required: true }], body);
      } catch (error) { const r = p2PresetResponse(error); if (r) return r; throw error; }
      const atomId = args.atom_id;
      const rows = await atomsOf(handle, doc);
      const target = rows.find((r) => r.atom && r.atom.id === atomId);
      if (!target) return p2Missing("目标原子 " + atomId + " 不在日志中");
      // **与 `revert` 同一条跨起点约束 ✓**（`validate_history_refs` 对 Revert／Reapply 都查 ✓）。
      const origin = p2EvalOriginSeq(rows);
      if (typeof target.seq === "number" && target.seq <= origin) {
        return p2Bad("目标原子 " + atomId + " (seq " + target.seq + ") 位于当前求值起点 seq "
          + origin + " 之前，需先回到该起点或重新提交等价操作");
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
      if (typeof instanceId !== "string" || !instanceId) return p2Bad("缺少必填参数 instance_id");
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
      if (typeof instanceId !== "string" || !instanceId) return p2Bad("缺少必填参数 instance_id");
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
        // **★ 字段名必须与 `/health` 一致 ✗ ★**（**第 493 轮 ✓）
        //   **∴ 为什么 ✗**：**服务端与内核**都用 `gpu_adapter_note`**✗
        //     （**第 463 轮改名 ✓；**第 466 轮**旧名已删 ✓）
        //     ＋ **∴ 而**本文件**还在用旧名**✗
        //       ⇒ **∴ 于是**：**判据**报「**后端不可判**」 ✓
        //         ＋ **∴ 那**是**改名时的遗漏** ✓ ★**** ✓✓
        //   **∴ 语义（**本部署无服务端 ✓）✗**：**webgpu ⇒ 没有"不可用原因"（`null` ✓）**
        //     ＋ **∴ 否则**如实说 `host_has_no_webgpu` ✓ ★**** ✓✓
        gpu_adapter_note: backend === "webgpu" ? null : "host_has_no_webgpu",
        // **∴ 旧名保留为**弃用别名**✗**（**∴ 兼容**外部读者** ✓）★**** ✓✓
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
          ...(typeof body.group_id === "string" ? { group_id: body.group_id } : {}),
          // **`label` 写进**原子顶层**✗**（**服务端 `Atom.label` ✓**；`list_atoms(label=…)`
          //   只认它 ✓）⇒ **∴ 旧本地**丢掉了它 ✗ ⇒ **∴ 带标签的笔在本地永远筛不到 ✓**。
          ...(typeof body.label === "string" ? { label: body.label } : {}),
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
    //   （`crates/yanshi-server/src/tools.rs:8803`／`:8876`）✓**：
    // **∴ 参数 `{count?}`（**缺省 1 ✓、**下限 1 ✓ —— **∴ 照服务端 `.max(1)` ✓**）；
    // **∴ 候选归并走共享的 `__p2CollectGestures` ✗**（**∴ 与服务端 `collect_gestures` 同一套判据 ✓：
    //   对象活着 ⇒ undo 候选（**含它的 `tombstone` ✓**）；死了 ⇒ redo 候选 ✓**）；
    // **∴ 无对象归属的原子不进候选只进 `ignored`**（**`create_document` 连 `ignored` 都不进 ✓，
    //   **∴ 照服务端 `continue` ✓**；`revert`／`reapply` 净荷只有 `target` ⇒ 天然无主 ✓ 不撤历史 ✓）；
    // **∴ 历史原子若仍进了笔 ⇒ `skipped` 并如实报原因 ✗**（**∴ 照服务端 `tools.rs:8834` 的分支 ✓**）；
    // **∴ 对选中笔的每个原子写一个历史原子**（**∴ kind 用 **snake_case** `revert`／`reapply` ✗ ——
    //   **∴ 与服务端序列化名一致（`atom.rs:356` ✓）⇒ **∴ 内核重放时认得 ✓**；
    //   **∴ payload 键用 `target` ✗**（**∴ 照服务端 `write_history_atom` 的 `{"target":…}`
    //     （`tools.rs:10451` ✓）—— **∴ 折叠层的 `target_atom()` 只读这个键（`atom.rs:641` ✓）
    //     ⇒ **∴ 否则内核**认不出**它是个撤销 ✓**）。
    async function __historyStep(isUndo, count) {
      const rows = await atomsOf(handle, doc);
      // **∴ `atomsOf` 已按 seq 升序 ✓**（**`store.js` ✓）⇒ **∴ 倒着扫 = 从新到旧 ✓**。
      const { gestures, ignored } = __p2CollectGestures(rows, isUndo);
      const histKind = isUndo ? "revert" : "reapply";
      const prefix = isUndo ? "01UNDO" : "01REDO";
      const itemsName = isUndo ? "undone" : "redone";
      const gesturesName = isUndo ? "gestures_undone" : "gestures_redone";
      const countName = isUndo ? "undone_count" : "redone_count";
      if (gestures.length === 0) {
        return json({ ok: true, [itemsName]: [], skipped: [], [countName]: 0,
                      remaining_gestures: 0, server: false,
                      message: isUndo
                        ? "没有可撤销的笔迹 ✗ ⇒ 这个文档里除了创建本身，还没有内容原子 ✓"
                        : "没有可重做的笔迹 ✗ ⇒ 没有哪一笔处于「被撤销」状态 ✓",
                      note: isUndo
                        ? "结构改动不在这里撤 ✓（例如建层/删层/重排 ✓）⇒ 用 revert 或 revert_changeset ✓"
                        : undefined });
      }
      const take = Math.min(count, gestures.length);
      const done = [], skipped = [];
      const uniq = String(Date.now()).padStart(13, "0") + Math.random().toString(36).slice(2, 7);
      for (let g = 0; g < take; g++) {
        const items = gestures[g][1];
        for (let k = 0; k < items.length; k++) {
          const it = items[k];
          if (it.kind === "revert" || it.kind === "reapply") {
            skipped.push({ seq: it.seq, atom_id: it.atom_id, kind: it.kind,
                           reason: isUndo ? "历史原子本身不撤（折叠层与校验层对嵌套撤销有分歧 ⇒ 不擅自决定）"
                                          : "历史原子本身不重做" });
            continue;
          }
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
          done.push({ seq: it.seq, atom_id: it.atom_id, kind: it.kind, object_id: gestures[g][0] });
        }
      }
      return json({
        ok: true, [itemsName]: done, skipped, [countName]: done.length,
        [gesturesName]: take, gestures_total: gestures.length,
        remaining_gestures: Math.max(gestures.length - take, 0),
        ignored_non_content_atoms: ignored,
        hint: isUndo ? "要恢复就对这些 atom_id 用 reapply；count 可以一次撤多笔 ✓"
                     : "再撤掉它们用 undo_last ✓",
        server: false,
      });
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

    // **★ `/api/tools/revert_atoms`／`/api/tools/revert_group` ✗ ★**（**ROI 记账层 ✓**）：
    // **∴ 语义照服务端 `write_revert_atoms`（`tools.rs:19361`）／`write_revert_group`
    //   （`tools.rs:19261`）✓**——**∴ 全部错误串与响应形状是当场打真服务端逐字取回的 ✓**
    //   （**∴ 不猜：探测脚本 `probe-revert.mjs` 输出随包 ✓**）。
    // **∴ 逐条失败只有三种**（**`log.rs:854–899` `validate_history_refs` ✓**）：
    // **∴ 目标不在日志 ⇒ `reference_not_found: 目标原子 X 不在日志中` ✓**；
    // **∴ 目标 kind 不满足 `revertible` ⇒ `invalid_argument: 不能 revert 协作原子 X（kind）—— …` ✓**；
    // **∴ 第三种（`declare_head` 求值起点）本轮补上 ✓**（**`log.rs:894` ✓**：`target.seq <= eval_origin`
    //   ⇒ `invalid_argument: 目标原子 X (seq s) 位于当前求值起点 seq o 之前，需先回到该起点或重新提交等价操作` ✓）。
    //   **∴ 本地求值起点 = 最新 `declare_head` 原子的 seq ✓**（`p2EvalOriginSeq` ✓，与 `seq.rs` 同口径 ✓）。
    // **∴ `revert_group` 的筛法是「曾被 revert 过」✗ 不是「当前生效动作是 revert」✓**
    //   （**∴ 服务端 `tools.rs:19269–19289` 用朴素 `HashSet` 扫全部 `revert` 原子 ✓**
    //     ⇒ **∴ 撤过又 redo 的笔**照样**被整组跳过 ✗**——**∴ 与 `p2Neutralized` 的**「最新动作胜出」**
    //     口径**有分歧 ✓**（服务端自身两层就分歧 ⇒ **∴ 照抄 ✗ 不替它「修好」✓**）。
    // **∴ 空集 ⇒ 幂等 `ok:true`／`reverted_count:0` ＋原因 ✓**（**`web/index.html:1206`
    //   的按钮可见性**就依赖**这个形状 ✓ ⇒ **∴ 撤过第二次不报错 ✗ 报错才是缺陷 ✓**）。
    /** **∴ 逐条撤销一批原子 ✗ ⇒ 回 `{ok, reverted, reverted_count, failed, failed_count, note}` ✓**
      （**∴ 服务端 `write_revert_atoms` 的循环体 ✓**：**∴ 一条坏**不**拖累其余 ✓**）。 */
    async function __p2RevertBatch(ids) {
      const rows = await atomsOf(handle, doc);
      const byId = new Map();
      const seqById = new Map();
      for (const r of rows) {
        const a = (r && r.atom) || r || {};
        if (a && a.id != null) {
          byId.set(String(a.id), a);
          if (typeof r.seq === "number") seqById.set(String(a.id), r.seq);
          else if (typeof a.seq === "number") seqById.set(String(a.id), a.seq);
        }
      }
      const origin = p2EvalOriginSeq(rows);
      const reverted = [];
      const failed = [];
      const uniq = String(Date.now()).padStart(13, "0") + Math.random().toString(36).slice(2, 7);
      for (let i = 0; i < ids.length; i++) {
        const id = String(ids[i]);
        const target = byId.get(id);
        if (!target) {
          failed.push({ atom_id: id, error: "reference_not_found: 目标原子 " + id + " 不在日志中" });
          continue;
        }
        if (!P2_REVERTIBLE_KINDS.has(target.kind)) {
          failed.push({ atom_id: id,
            error: "invalid_argument: 不能 revert 协作原子 " + id + "（" + String(target.kind) + "）—— 只有产生状态效果的原子与 revert/reapply 本身可以撤销" });
          continue;
        }
        const seq = seqById.get(id);
        if (typeof seq === "number" && seq <= origin) {
          failed.push({ atom_id: id,
            error: "invalid_argument: 目标原子 " + id + " (seq " + seq + ") 位于当前求值起点 seq "
              + origin + " 之前，需先回到该起点或重新提交等价操作" });
          continue;
        }
        await putAtom(handle, doc, {
          actor: "human:web",
          id: "01RVATM" + uniq + "_" + i,
          kind: "revert",
          payload: { target: id },
          schema_version: 1, seq: 0, session: "session:web", timestamp: Date.now(),
        });
        reverted.push(id);
      }
      return { ok: failed.length === 0, reverted, reverted_count: reverted.length,
               failed, failed_count: failed.length,
               note: "逐个撤销；有一条失败也照实报 failed，不假称全部成功" };
    }
    if (path === "/api/tools/revert_atoms") {
      const pl = (body && body.payload) || {};
      const ids0 = body.atom_ids ?? pl.atom_ids;
      // **∴ 校验顺序照服务端 ✓**：**∴ 先「是不是数组」✓**（`tools.rs:19363` 的 match ✓）
      //   ⇒ **∴ 再逐项「是不是字符串」✓**（`collect::<Result<_>>()?` ✓）⇒ **∴ 最后才查空 ✗**
      //   （**∴ `[]` 过得了逐项 ⇒ 落在 `is_empty` ✓** ⇒ **∴ 顺序错 ⇒ 空数组会报成「每项都要是字符串」✗**）。
      if (!Array.isArray(ids0)) return p2Bad("atom_ids 必须是数组（原子 id 列表）");
      for (const v of ids0) {
        if (typeof v !== "string") return p2Bad("atom_ids 里每一项都要是字符串（原子 id）");
      }
      if (ids0.length === 0) return p2Bad("atom_ids 不能为空");
      const out = await __p2RevertBatch(ids0);
      out.server = false;
      // **混合批的 `ok:false` ⇒ 400 ✓**（服务端 `server.rs:2872` 同映射 ✓；以前本地 200 ✗）。
      return json(out, p2ToolStatusOf(out));
    }
    if (path === "/api/tools/revert_group") {
      const pl = (body && body.payload) || {};
      const groupId = body.group_id ?? pl.group_id;
      // **∴ 服务端 `require_str`（`tools.rs:1209`）只查「在不在／是不是字符串」✗ 不查非空 ✓**
      //   ⇒ **∴ 空串**要**放行 ✓**（**∴ 它会去匹配那些被盖章成空串的原子 ✓ —— **∴ 不**擅自改成 400 ✓**）。
      if (typeof groupId !== "string") return p2Bad("缺少必填参数 group_id");
      const rows = await atomsOf(handle, doc);
      const everReverted = new Set();
      for (const r of rows) {
        const a = (r && r.atom) || r || {};
        if (a.kind === "revert" && a.payload && typeof a.payload.target === "string") {
          everReverted.add(a.payload.target);
        }
      }
      const ids = [];
      for (const r of rows) {
        const a = (r && r.atom) || r || {};
        if (a.group_id === groupId && !everReverted.has(String(a.id))) ids.push(String(a.id));
      }
      if (ids.length === 0) {
        // **∴ 实测：服务端这一支的响应里**没有** `reverted`／`failed` 两个数组 ✓**
        //   （**∴ 探测输出逐字对过 ✓ ⇒ **∴ 别**「好心」补成空数组 ✗ 形状变了调用方会看见 ✓**）。
        return json({ ok: true, group_id: groupId, reverted_count: 0, failed_count: 0,
                      note: "该组没有可撤销的原子（可能已撤过，或这个 group_id 从未用过）", server: false });
      }
      const out = await __p2RevertBatch(ids);
      out.group_id = groupId;
      out.server = false;
      return json(out, p2ToolStatusOf(out));
    }
    // **P2c-57 `revert_to_seq` ✓**（照 `write_revert_to_seq`，`tools.rs:20100`）：
    //   **∴ 一句「回到第 n 笔之后」✓**——**∴ 筛法与服务端逐条同 ✗**：
    //   · 候选 = **`seq > n` 且净荷有 `source` 键** ✓（`get("source").is_some()` ⇒ **`source:null` 也算有** ✓，
    //     用 `hasOwnProperty` 才对 ✓）；`draw_shape`／`create_layer` 这类没有 `source` 的**不碰** ✓；
    //   · **已撤过的按朴素集合跳过 ✗**（扫**全部** `revert` 原子 ✓ —— **`reapply` 回来的照样跳过** ✗，
    //     与 `revert_group` 同一口径 ✓；**∴ 不**替服务端「修好」✓）；
    //   · 空集支**没有 `reverted`／`failed` 两个数组** ✓（形状照实测 ✓，别好心补空数组 ✗）；
    //   · 非空支 = `revert_atoms` 的结果 ＋ `seq` ✓（失败逐条照报 ✓，`ok` 由 `failed` 决定 ✓）。
    if (path === "/api/tools/revert_to_seq") {
      const specBad = p2ValidateArgsFor("revert_to_seq", body);
      if (specBad) return p2ToolFailure(400, specBad.code, specBad.detail);
      // **服务端读的是 `args.seq` 本身 ✗ 不拆 `payload`**（与 `revert_atoms` 那两行不同 ✓）。
      const seqArg = p2Count(body.seq, "seq");
      if (seqArg.error) return seqArg.error;
      if (seqArg.value === undefined) return p2Bad("seq 必填（撤销 seq 大于它的笔迹）");
      const target = seqArg.value;
      const rows = await atomsOf(handle, doc);
      const alreadyReverted = new Set();
      for (const r of rows) {
        const a = (r && r.atom) || r || {};
        if (a.kind === "revert" && a.payload && typeof a.payload.target === "string") {
          alreadyReverted.add(a.payload.target);
        }
      }
      const ids = [];
      for (const r of rows) {
        const a = (r && r.atom) || r || {};
        const seq = typeof r.seq === "number" ? r.seq : (typeof a.seq === "number" ? a.seq : 0);
        if (!(seq > target)) continue;
        const pl = a.payload;
        if (!(pl && typeof pl === "object"
              && Object.prototype.hasOwnProperty.call(pl, "source"))) continue;
        const id = String(a.id);
        if (alreadyReverted.has(id)) continue;
        ids.push(id);
      }
      if (ids.length === 0) {
        return json({ ok: true, seq: target, reverted_count: 0, failed_count: 0,
                      note: "没有需要撤销的笔迹（seq 大于它的一笔都没有）", server: false });
      }
      const out = await __p2RevertBatch(ids);
      out.seq = target;
      out.server = false;
      // **逐条失败时 `ok:false` ⇒ 状态照服务端映射（400 ✓）** ✗ 不是 200 ✓。
      return json(out, p2ToolStatusOf(out));
    }

    // **★ 风格预设四端点 ✗ ★**（第 2 批 ✓；**纯计算在模块层 ✓（`p2PresetLibrary`／`p2ExpandPreset` …），
    //   这里只做「入口校验 ＋ 落成」✓**）。**∴ 服务端出处 `crates/yanshi-server/src/presets.rs` ✓**。
    //
    // **∴ 三条口径先写清楚 ✗**（**每条都是「不这样做就会静默降级」的地方 ✓**）：
    //   ① **C/S 客户端模式 ⇒ 不进本地分支 ✓**（**由链尾 `:5578` 那段代理到服务端 ✓**）——
    //     **∴ 否则**用户用 `YANSHI_PRESET_DIR` 装的新预设会被打包时那份**遮蔽**✗ 而页面 `ok:true` ✓
    //     （**静默降级形态③ ✓**：被覆盖而不说 ✗）。**PWA**（无服务器）⇒ 走本地内联库 ✓ 并用 `server:false` 如实标注 ✓。
    //   ② **`preview_preset` 的 `render` 分支 ⇒ 如实 501 ✗**（**本地内核没有「单层裸像素」通道 ✓
    //     ⇒ 盒式降采样＋临时文档渲染无法如实复现 ✗**）；**但校验先做完 ✓**（`render` 的形状与
    //     `layer_id` 的必填与服务端**同一段文案**✓ ⇒ 写错的请求拿到的是与服务端一致的 400 ✓，
    //     而不是把「参数拼错」也报成「本地不支持」✓）。
    //   ③ **`apply_preset` 的落成走既有原子路 ✓**（**`create_object`／`supersede`／复活三连 ✓**，
    //     **∴ 复用 `p2cAddEffect`／`p2cUpdateEffect` ✗ 不另造原子、不复制第二份落成语义 ✓**）。
    /** **四个端点的声明表 ✓**（**逐字照 `tools.rs` 的 `param!` 名单 ✓：键名、类型、必填性 ✓**；
     *   **∴ 为什么在本地也要 ✓**：没有它 ⇒ 拼错的参数**被静默忽略**✓（形态④），而服务端会 400 ✓）。 */
    const P2_PRESET_PARAMS = {
      list_presets: [
        { name: "tag", kind: "string", required: false },
        { name: "subject", kind: "string", required: false },
        { name: "status", kind: "string", required: false },
      ],
      get_preset: [
        { name: "preset_id", kind: "string", required: true },
      ],
      preview_preset: [
        { name: "preset_id", kind: "string", required: true },
        { name: "strength", kind: "number", required: false },
        { name: "knobs", kind: "object", required: false },
        { name: "include_correction", kind: "boolean", required: false },
        { name: "layer_id", kind: "string", required: false },
        { name: "render", kind: "object", required: false },
      ],
      apply_preset: [
        { name: "preset_id", kind: "string", required: true },
        { name: "layer_id", kind: "string", required: true },
        { name: "strength", kind: "number", required: false },
        { name: "knobs", kind: "object", required: false },
        { name: "include_correction", kind: "boolean", required: false },
      ],
    };

    /** **`kind` ⇒ 既有 `EffectKind` 路径 ✗**（**照 `effect_kind_of` ✓ 逐字文案 ✓**：
     *   **∴ 为什么本地也要 ✗**：库里若写进第三种 kind，`p2cAddEffect` 会把它**当 adjustment 落下去**✓
     *   （**`kind === "filter" ? … : …` 的 `else` 分支 ✓**）⇒ **参数齐全、回执 ok:true、效果不是那一笔 ✓**
     *   **＝ 静默降级形态④**——所以这里**响亮拒绝**，不交给三元运算符猜 ✓）。 */
    function p2PresetKind(op) {
      if (op.kind === "adjustment" || op.kind === "filter") return op.kind;
      p2PresetBad(`操作 ${op.object_id} 的 kind 非法：「${String(op.kind)}」`);
      return null;
    }


    /** **`Rust 的 ObjectType` 的 `{:?}` 名 ✗**（**报错文案「实际 {…}」用 ✓**：
     *   **折叠层存的是 snake_case ✗ 而 Debug 打印的是变体名 ✓**（**`state.rs:157` ✓**）⇒
     *   **∴ 这里做同一件「加首字母」的映射 ✗ 不新造名字 ✓**。 */
    function p2PresetTypeNameDebug(type) {
      if (typeof type !== "string" || !type) return "?";
      return type.split("_").map((part) =>
        part ? part.charAt(0).toUpperCase() + part.slice(1) : part).join("");
    }

    /** **把「返回 Response 的本地端点实现」当库用 ✗**（**∴ 复用同一份落成语义 ✓**）：
     *   取回 JSON 体 ⇒ `ok!==true` ⇒ **把原响应原样交回调用方**✓（**不二次包装 ✗ 不改状态码 ✓**）。 */
    async function p2PresetLand(response) {
      const landedBody = await response.json().catch(() => null);
      if (landedBody && landedBody.ok === true) return { ok: true, body: landedBody, response: null };
      // **★ 失败分支不许把**已读过的** Response 交回去 ✗ ★**（第 2 批判据实测 ✓）：
      //   上面 `.json()` 已经把 body 消费掉 ⇒ 调用方 `return landed.response` ⇒ 浏览器再读
      //   ⇒ **`TypeError: Body is unusable`** ✓ ⇒ **服务端的 409／文案在 PWA 里变成不可读的空响应** ✗
      //   ——这正是「静默降级形态③」的镜像：**错误码服务端有、本地层把它吃掉了** ✓。
      //   **∴ 用同一份已解析的 body 重建响应 ✓**（状态码、`ok:false`、`error_code`、`context.detail` 全保真 ✓）。
      return { ok: false, body: landedBody,
               response: json(landedBody ?? { ok: false, error: "empty_response", server: false },
                              response.status) };
    }

    // **① `list_presets` ✗**（**照 `read_list_presets` ✓：三个可选筛子 ＋ 摘要数组 ✓**。
    //   **∴ `subject` 是**子串**匹配（`x.contains(s)` ✓）而 `tag` 是**全等**（`x == t` ✓**）⇒ 本地照抄，不「顺手」统一 ✓**。
    if (path === "/api/tools/list_presets" && !__csClientMode()) {
      try {
        const args = p2PresetSchemaCheck("list_presets", P2_PRESET_PARAMS.list_presets, body);
        const library = p2PresetLibrary();
        const tag = typeof args.tag === "string" ? args.tag : null;
        const subject = typeof args.subject === "string" ? args.subject : null;
        const status = typeof args.status === "string" ? args.status : null;
        const presets = library.presets
          .filter((p) => tag === null || p.tags.some((x) => x === tag))
          .filter((p) => subject === null || p.suitable.some((x) => x.indexOf(subject) >= 0))
          .filter((p) => status === null || p.status === status)
          .map((p) => p2PresetSummary(p));
        // **∴ `ok: true` 在服务端由派发层注入 ✓**（**实测 `list_presets` 的成功响应里有它 ✓**）⇒
        //   **∴ 本地也要给 ✗**（**否则调用方「`if (!resp.ok) return`」那一支会把成功当失败 ✓**）。
        return json({ ok: true, presets, count: presets.length,
                      library_version: library.version,
                      library_source: library.source, server: false });
      } catch (error) { const r = p2PresetResponse(error); if (r) return r; throw error; }
    }

    // **② `get_preset` ✗**（**照 `read_get_preset` ✓：完整定义 ＋ `expanded_default`（strength=1 ✓）**。
    if (path === "/api/tools/get_preset" && !__csClientMode()) {
      try {
        const args = p2PresetSchemaCheck("get_preset", P2_PRESET_PARAMS.get_preset, body);
        const preset = p2FindPreset(String(args.preset_id));
        const expanded = p2ExpandPreset(preset, 1.0, {}, true);
        return json({ ok: true, preset, expanded_default: expanded, server: false });
      } catch (error) { const r = p2PresetResponse(error); if (r) return r; throw error; }
    }

    // **③ `preview_preset` ✗**（**照 `read_preview_preset` ✓ 顺序逐字：库 → preset_id → 展开 →
    //   → kind 校验 → `render` → `layer_id`**。
    //   **∴ 服务端在展开后还要过一遍 `validate_effect` ✗ 本地依据见台账「第 六百五十七 节」✓**
    //   （**内联库与源文件逐字同一份 ✓ 而它的 0/0.5/1 全档由服务端 Rust 测试
    //     `every_preset_expands_and_passes_validate_effect` 证明合法 ✓**）。
    if (path === "/api/tools/preview_preset" && !__csClientMode()) {
      try {
        const args = p2PresetSchemaCheck("preview_preset", P2_PRESET_PARAMS.preview_preset, body);
        const preset = p2FindPreset(String(args.preset_id));
        const parsed = p2ParsePresetExpandArgs(preset, args);
        const ops = p2ExpandPreset(preset, parsed.strength, parsed.knobs, parsed.includeCorrection);
        for (const op of ops) p2PresetKind(op);
        if (Object.prototype.hasOwnProperty.call(args, "render")) {
          // **先按服务端口径把 `render` 与 `layer_id` 校验完 ✓**（**再 501 ✗**）。
          const render = p2PresetParseRender(args);
          const layerId = args.layer_id;
          if (typeof layerId !== "string" || !layerId.trim()) {
            p2PresetBad("preview_preset 的 render 需要 layer_id（源图层）");
          }
          const scaleNotes = p2PresetScaleNotes(ops);
          return json({ ok: false, error: "not_implemented_locally",
                        endpoint: "/api/tools/preview_preset",
                        reason: `preview_preset 带 render（max_edge=${render.maxEdge}, format=${render.format}）：`
                          + "服务端在**临时文档**里取源图层裸像素、盒式降采样后渲染代理图；"
                          + "本地内核只出整幅 PNG，没有单层裸像素通道，无法如实复现 ⇒ 宁可 501 也不给一张**口径不同的**代理图 ✗"
                          + "（展开清单本身已按同一口径校验通过 ⇒ 去掉 render 或改用服务器渲染即可 ✓）"
                          + (scaleNotes ? `｜${scaleNotes}` : ""),
                        server: false }, 501);
        }
        return json({
          ok: true,
          preset_id: preset.id, name_zh: preset.name_zh, status: preset.status,
          strength: parsed.strength, include_correction: parsed.includeCorrection,
          ops, op_count: ops.length,
          notes: "只展开不落成、不渲染；落成请用 apply_preset（同一展开口径）",
          preset_notes: preset.notes, server: false,
        });
      } catch (error) { const r = p2PresetResponse(error); if (r) return r; throw error; }
    }

    // **④ `apply_preset` ✗**（**照 `write_apply_preset`：两阶段 ✓ 任一非法**整组不落**✓**）。
    if (path === "/api/tools/apply_preset" && !__csClientMode()) {
      try {
        const args = p2PresetSchemaCheck("apply_preset", P2_PRESET_PARAMS.apply_preset, body);
        const preset = p2FindPreset(String(args.preset_id));
        const layerId = String(args.layer_id);
        const parsed = p2ParsePresetExpandArgs(preset, args);
        const ops = p2ExpandPreset(preset, parsed.strength, parsed.knobs, parsed.includeCorrection);
        if (!ops.length) {
          // 服务端这里是 `.expect()` ⇒ panic（库保证风格段非空 ✓）；本地如实报错而不是静默成功 ✗。
          p2PresetBad(`预设「${preset.id}」展开为空（include_correction=${parsed.includeCorrection} 而风格段没有操作）⇒ 库数据非法`);
        }
        // ---- 第一阶段：全量校验（这一段一个原子都不提交 ✓ 照服务端 ✓）----
        const rows0 = await atomsOf(handle, doc);
        const neutral0 = p2Neutralized(rows0);
        const layers = p2PresetLayers(rows0, neutral0);
        if (!layers.has(layerId)) p2PresetGone(`图层 ${layerId} 不存在`, layerId);
        const slots = new Map();
        for (const op of ops) {
          const effectKey = p2PresetKind(op) === "filter" ? "filter_name" : "adjustment_type";
          const slot = p2PresetSlot(rows0, layers, neutral0, op.object_id);
          slots.set(op.object_id, slot);
          if (slot.slot === "tombstoned") {
            const ownerLayer = typeof slot.view.layer_id === "string" ? slot.view.layer_id : "";
            // **只有「源图层还在」才拒绝跨层复活 ✓**（**与 `revive_effect_in_place` 同一口径 ✓**：
            //   源图层已被删 ⇒ 孤儿墓碑 ⇒ 落位到目标层 ✓ 不再把整套预设永久判死 ✓）。
            if (ownerLayer !== layerId && layers.has(ownerLayer) && layers.get(ownerLayer).alive) {
              p2PresetBad(`对象 ${op.object_id} 已被删除且属于图层 ${ownerLayer}（它还在），`
                + `与预设要落的图层 ${layerId} 不一致，拒绝复活；请先 restore_object 恢复后移层，或换图层/文档`);
            }
          }
          if (slot.slot === "tombstoned" || slot.slot === "alive") {
            const existing = slot.view;
            if (existing.type !== op.kind) {
              p2PresetBad(`对象 ${op.object_id} 已存在但不是${op.kind}对象`
                + `（实际 ${p2PresetTypeNameDebug(existing.type)}），拒绝覆盖；请换图层/文档或先删除该对象`);
            }
            const raw = existing.data && typeof existing.data === "object"
              ? existing.data[effectKey] : undefined;
            const existingName = typeof raw === "string" ? raw : "";
            if (existingName !== op.type) {
              p2PresetBad(`对象 ${op.object_id} 已存在但效果是「${existingName}」，`
                + `与预设「${preset.id}」要落的「${op.type}」不一致，拒绝覆盖`);
            }
          }
        }
        // **缺省 z 的口径 ✓**（**与 `p2cAddEffect` 同一个 `p2DefaultZInLayer` ✗ 不复制第二份 ✓**；
        //   **∴ 逐笔落成时服务端会重算 ⇒ 本地用游标递增等价 ✓**（**每建一笔它自己就成了新的最大 ✓**）。
        let zCursor = p2DefaultZInLayer(rows0, layers, neutral0, layerId);
        // ---- 第二阶段：按序落成 ----
        const created = [], updated = [], revived = [];
        let lastSeq = null;
        for (const op of ops) {
          const slot = slots.get(op.object_id);
          const nameKey = op.kind === "filter" ? "filter_name" : "adjustment_type";
          if (slot.slot === "alive") {
            const landed = await p2PresetLand(await p2cUpdateEffect(op.kind, {
              object_id: op.object_id, params: op.params, opacity: 1.0,
            }));
            if (!landed.ok) return landed.response;
            updated.push(op.object_id);
            lastSeq = landed.body.seq;
            continue;
          }
          if (slot.slot === "tombstoned") {
            // **复活三连 ✓**（**照 `revive_effect_in_place` 的原子序 ✓**）：
            //   ① 孤儿墓碑先撤**那条图层 tombstone**（级联删除的源头 ✓ `tools.rs:11723` ✓）；
            //   ② 再撤对象自己每条生效的 tombstone ✓（`revive_object`／`tombstone_atom_ids` ✓）；
            //   ③ 一条 `supersede` 整组换新参数 ✓（**不是键级合并 ✓ 复活的调用方给的是完整新参数 ✓**）；
            //   ④ 孤儿 ⇒ 把用户删掉的那一层**再删回去** ✓（对象已搬到目标层 ⇒ 不受影响 ✓）。
            if (slot.cascade && slot.dead_by) {
              await putAtom(handle, doc, p2Envelope("revert", { target: slot.dead_by }));
            }
            for (const tid of slot.view.tombstone_ids) {
              if (neutral0.has(tid)) continue;
              await putAtom(handle, doc, p2Envelope("revert", { target: tid }));
            }
            const data = { ...(slot.view.data && typeof slot.view.data === "object" &&
                             !Array.isArray(slot.view.data) ? slot.view.data : {}) };
            data[nameKey] = op.type;
            data.params = op.params;
            data.opacity = 1.0;
            data.name = `preset:${preset.id}`;
            const ownerLayer = slot.cascade ? layerId : slot.view.layer_id;
            const seq = await putAtom(handle, doc, p2Envelope("supersede", {
              object_id: op.object_id, layer_id: ownerLayer, data,
            }));
            if (slot.cascade) {
              await putAtom(handle, doc, p2Envelope("tombstone", { layer_id: slot.view.layer_id }));
            }
            revived.push(op.object_id);
            lastSeq = seq;
            continue;
          }
          const landed = await p2PresetLand(await p2cAddEffect(op.kind, {
            layer_id: layerId, [nameKey]: op.type, params: op.params,
            object_id: op.object_id, name: `preset:${preset.id}`, opacity: 1.0,
            z_index: zCursor,
          }));
          if (!landed.ok) return landed.response;
          zCursor += 1;
          created.push(op.object_id);
          lastSeq = landed.body.seq;
        }
        return json({ ok: true, seq: lastSeq, server: false,
                      preset_id: preset.id, strength: parsed.strength,
                      include_correction: parsed.includeCorrection,
                      created_object_ids: created, updated_object_ids: updated,
                      revived_object_ids: revived,
                      revived: revived.length, created: created.length,
                      updated: updated.length, op_count: ops.length,
                      layer_id: layerId,
                      note: `已按同一展开口径落 ${ops.length} 笔到图层 ${layerId}：`
                        + `新建 ${created.length}／更新 ${updated.length}／复活 ${revived.length}` });
      } catch (error) { const r = p2PresetResponse(error); if (r) return r; throw error; }
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
        ...(typeof body.group_id === "string" ? { group_id: body.group_id } : {}),
      });
      return json({ ok: true, seq, server: false, object_id: objectId, layer_id: layerId,
                    note: "已在本地原子日志写入 erase 原子（折叠层按服务端语义把它归成 Retouch/erase 对象；get_log 可读）" });
    }

    if (path === "/api/tools/render_region") {
      // **★ C/S 客户端渲染：两类请求必须由**服务端**出图 ✗ ★**（第 10 轮 ✓）：
      //   ① **heavy 文档**（介质笔画 = `import_image`/`raster_patch`、修图、液化…）——
      //      本地内核折叠它只会得到**空白补丁** ✓（`heavyDocument` 那套注释 ✓）；
      //   ② **单层缩略图**（带 `layer_id` ✓）—— 本地内核只有整幅渲染 ✓。
      //   **∴ 其余（纯矢量文档的整幅/脏区）**在本地用**服务端原子日志**渲染 ✓（像素不出浏览器 ✓）。
      //   **∴ 走同源代理** ⇒ `localCalls` 计数不变 ✓（它仍是"本地适配器"在应答 ✓），
      //     而返回的 JSON 与服务端同形（`raw_url`／`thumb_url`／`head_seq` ✓）⇒ 调用方无需分叉 ✓。
      if (__csClientMode()) {
        const layerWanted = (body && body.layer_id) || q.get("layer_id") || null;
        let heavy = !!layerWanted;
        if (!heavy) {
          try {
            const rows0 = await atomsOf(handle, doc);
            heavy = rows0.some((r) => __atomHeavy(r.atom));
          } catch (e) { heavy = false; }
        }
        if (heavy) {
          try {
            const init = {
              method: "POST",
              headers: { "content-type": "application/json" },
              body: JSON.stringify(body || {}),
            };
            if (typeof AbortSignal !== "undefined" && typeof AbortSignal.timeout === "function") {
              init.signal = AbortSignal.timeout(20000);
            }
            const res = await fetch(url.pathname + url.search, init);
            const text = await res.text();
            const src = res.headers.get("content-type") || "application/json";
            return new Response(text, {
              status: res.status,
              headers: { "content-type": src, "cache-control": "no-store",
                         "x-yanshi-source": "server-proxy-heavy" },
            });
          } catch (e) {
            return json({ ok: false, error: "needs_render", server: false,
                          reason: "heavy 像素需服务端出图，但代理失败：" +
                                  String((e && e.message) || e) });
          }
        }
      }
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
        // **★ 内核一律走**可注入的 `__loaders.wasm()`** ✗ ★**（第 六百六十二 轮 ✓）：
        //   **∴ 此前这里**直接 `import("/wasm/yanshi_wasm.js")` ✗** ⇒ **∴ 绕过了接缝** ✗
        //   ⇒ **∴ node 判据里**注入的真内核到不了这条分支**（`p2cKernelFor` 走的才是接缝 ✓）
        //   ⇒ **∴ 于是**整幅渲染只能报 `needs_render: 本地内核不可用` ✓
        //   （**∴ 浏览器不受影响 ✓** —— **∴ 它的 `import` 本来就能成功 ✓**）。
        //   **∴ 尺寸也改由 `p2cKernelFor` 从 `payload` 取 ✓**（**∴ 旧码读 `atom.width` ✗
        //   ⇒ **∴ 恒取不到 ⇒ 恒按 1024 建内核 ✗**（**`create_document` 的宽高在 `payload` 里 ✓**））。
        // Everything below touches the wasm kernel, so it runs inside the per-document
        // serial chain: the warm-up and the preview can otherwise overlap and trip the
        // reentrant borrow check (device log: recursive use of an object ... aliasing).
        return await withKernel(doc, async () => {
          const { entry } = await p2cKernelFor();
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
          // **★ 补画路径（`raw:true`）必须回**原始 RGBA** ✗ ★**（第 10 轮 ✓）：
          //   `blitServerBox` 的契约是"`raw_url` 里是 RGBA8" ✓（服务端 `raw:true` 同形 ✓）；
          //   若这里给 PNG ⇒ 它按 `w*h*4` 长度检查 ⇒ 判定 `short-bytes` ⇒ 补画静默丢弃 ✗
          //   （实测流水 `no-url`／`short-bytes` ✓，画布墨 0 ✗）。⇒ 只对 `raw:true` 走这条 ✓，
          //   其余调用（缩略图之外的 PNG 用途 ✓）保持原行为 ✓。
          const rawWanted = (body && body.raw === true) ||
                            q.get("raw") === "1" || q.get("raw") === "true";
          if (rawWanted) {
            const bw = Math.ceil(box.w);
            const bh = Math.ceil(box.h);
            const rgba = k.render_region_rgba(Math.floor(box.x), Math.floor(box.y), bw, bh);
            const need = bw * bh * 4;
            if (!rgba || rgba.length < need) {
              return json({ ok: false, error: "needs_render", server: false,
                            reason: "内核未产出 RGBA 字节",
                            atom_count: atoms.length,
                            got: rgba ? rgba.length : 0, need });
            }
            return new Response(rgba, {
              status: 200,
              headers: { "content-type": "application/octet-stream",
                         "cache-control": "no-store",
                         "x-yanshi-source": "local-kernel-rgba" },
            });
          }
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
          // **∴ 写进快照（**带被渲染状态的序号 ✓**）⇒ **∴ 下次命中快照分支 ✓**。
          // **★ 不许回头再读 head ✗ ★**（第 1440 轮审计 ✓）：折叠与写快照之间若有原子提交 ⇒
          // 重读的 head 已经 +1 ✗ ⇒ 旧图会被盖上**新序号** ⇒ **拿旧图冒充新状态** ✗（目标第 6 条 ✓）。
          // **∴ 用真正折叠进内核的最后一个原子的 `seq`** ✓ ⇒ `writeSnapshot` 只在 head 没动时收下 ✓
          // ⇒ **并发时拒绝写**（返回 false ⇒ 响应头如实说 `skipped` ✓）⇒ 下个请求重算 ✓
          // —— **宁可多算一次，不许撒谎 ✓**。
          const seqRendered = atoms.length ? atoms[atoms.length - 1].seq : 0;
          const snapWritten = await writeSnapshot(handle, doc, seqRendered, { bytes: png });
          // **∴ 诊断：写快照时用的序号与是否写进 ✓**（第 662 轮 ✓；`written` 是第 1440 轮补的 ✓）。
          try { window.__snapSeq = seqRendered; window.__snapAt = Date.now(); window.__snapWritten = snapWritten; } catch (e) {}
          return new Response(png, {
            status: 200,
            headers: { "content-type": "image/png", "cache-control": "no-store",
                       "x-yanshi-source": "local-kernel",
                       "x-yanshi-snapshot": snapWritten ? "written" : "skipped" },
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
      const limitArg = p2Count((body && body.limit) ?? q.get("limit"), "limit");
      if (limitArg.error) return limitArg.error;
      const limit = Math.min(1000, Math.max(1, Math.floor(limitArg.value !== undefined ? limitArg.value : 100)));
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
      let args;
      try {
        args = p2PresetSchemaCheck("checkpoint", [
          { name: "name", kind: "string", required: false },
          { name: "message", kind: "string", required: false },
        ], body);
      } catch (error) { const r = p2PresetResponse(error); if (r) return r; throw error; }
      const checkpointId = "ckpt_" + p2Ulid();
      const name = args.name != null ? args.name : checkpointId;
      const anchorSeq = await p2bHeadSeq();
      const payload = { checkpoint_id: checkpointId, name, anchor_seq: anchorSeq };
      if (args.message != null) payload.message = args.message;
      const atom = p2bAtom("checkpoint", payload);
      const seq = await putAtom(handle, doc, atom);
      return json({ ok: true, atom_id: atom.id, seq, checkpoint_id: checkpointId,
                    name, anchor_seq: anchorSeq, snapshot_id: null, server: false,
                    note: "检查点即原子日志里的 checkpoint 原子；本地不生成检查点快照 ⇒ snapshot_id 恒为 null" });
    }
    // **⑦ `get_checkpoints` ✗**：**∴ 从原子日志推导 ✓**（**与 `list_layers` 同一纪律：不另存"检查点表" ✗**）。
    //   **∴ 三处口径照折叠器 ✓**（`fold.rs:1440–1455` ✓）：
    //     ① **只认当前 `state@seq` 窗口** ✓（跳变前的检查点不在状态里 ✗）；
    //     ② **被 revert 的检查点原子不出现** ✓（折叠跳过失效原子 ✓，`fold.rs:372` ✓）；
    //     ③ **`message` 取自原子封套 ✗ 不是净荷** ✓（`atom.message` ✓；净荷里那份是惰性的 ✓）、
    //        `anchor_seq` = 净荷 `anchor_seq` ?? **原子 seq** ✓、`name` 缺省 = id ✓。
    //   **∴ 六个键一个不少、缺值给 `null` ✗**（**服务端 `read_get_checkpoints` 是手写 `json!`
    //     不是结构体序列化 ✓** ⇒ `Option::None` 变成 `"message": null`／`"snapshot_id": null` ✓，
    //     **`checkpoint` 结构体上的 `skip_serializing_if` 在这条路径**不生效 ✗**）。
    //   **∴ 顺序 = `BTreeMap` 键序 ⇒ 按 `checkpoint_id` 升序 ✓**（同 id 后写胜出 ✓）。
    if (path === "/api/tools/get_checkpoints") {
      const rows = p2StateWindow(await atomsOf(handle, doc));
      const suppressed = p2Neutralized(rows);
      const byId = new Map();
      for (const r of rows) {
        const a = (r && r.atom) || {};
        if (a.kind !== "checkpoint" && a.kind !== "create_checkpoint") continue;
        if (suppressed.has(a.id)) continue;
        const pl = a.payload || {};
        const id = typeof pl.checkpoint_id === "string" ? pl.checkpoint_id : ("ckpt_" + a.id);
        const cp = { checkpoint_id: id,
                     name: typeof pl.name === "string" ? pl.name : id,
                     created_by: a.actor != null ? a.actor : null,
                     message: typeof a.message === "string" ? a.message : null,
                     anchor_seq: typeof pl.anchor_seq === "number" ? pl.anchor_seq : r.seq,
                     snapshot_id: typeof pl.snapshot_id === "string" ? pl.snapshot_id : null };
        byId.set(id, cp);
      }
      const checkpoints = [...byId.values()].sort((a, b) =>
        a.checkpoint_id < b.checkpoint_id ? -1 : a.checkpoint_id > b.checkpoint_id ? 1 : 0);
      return json({ ok: true, checkpoints, count: checkpoints.length, server: false,
                    derived_from: "atoms",
                    note: "从原子日志推导；六个键与服务端一致，缺值给 null（本地不生成检查点快照）" });
    }
    // **⑧ `restore_checkpoint` ✓（本轮落地 ✗ 不再是 501 ✓）**：**∴ 语义 = 提交 `declare_head` 原子做求值起点跳变 ✓**
    //   （**`state@seq` 公式，设计 5.5 ✓**；**服务端 `write_restore_checkpoint` 同形 ✓**：
    //     `{base:{type:"checkpoint", id}, reason:"restore_checkpoint"}` ✓）。
    //   **∴ 旧 501 的理由是错的 ✗**：它说「本地折叠器对 declare_head 只记警告、从不执行跳变」——
    //     `seq.rs` 的窗口求值**逐层取窗口** ✓，本地派生层（`p2StateWindow` ✓）同口径 ✓
    //     ⇒ **∴ 写了原子就生效 ✓**（`get_state`／`list_layers`／`list_objects` 随即只见检查点之后的状态 ✓）。
    //   **∴ 检查点存在性照 `validate_declare_head` ✓**：**在全量日志里找最新的同名检查点原子 ✓**
    //     （**`seq.rs:350 checkpoint_anchor_seq` 也不看窗口 ✓**）⇒ **缺失 ⇒ 404 `检查点 X 不存在` ✓**。
    if (path === "/api/tools/restore_checkpoint") {
      let args;
      try {
        args = p2PresetSchemaCheck("restore_checkpoint",
          [{ name: "checkpoint_id", kind: "string", required: true }], body);
      } catch (error) { const r = p2PresetResponse(error); if (r) return r; throw error; }
      const checkpointId = args.checkpoint_id;
      const rows = await atomsOf(handle, doc);
      if (!p2FindCheckpointAtom(rows, checkpointId)) {
        return p2Missing("检查点 " + checkpointId + " 不存在");
      }
      const atom = p2Envelope("declare_head", {
        base: { type: "checkpoint", id: checkpointId }, reason: "restore_checkpoint",
      });
      const seq = await putAtom(handle, doc, atom);
      return json({ ok: true, atom_id: atom.id, seq, head: seq, dirty_kind: "full", server: false,
                    note: "已写入 declare_head 原子（求值起点跳到检查点）；之后的派生视图只含该检查点之后的状态" });
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
    // **⑩ `accept_suggestion`／`accept_suggestions` ✓（本轮落地 ✗ 不再是 501 ✓）**：
    //   **∴ 语义照 `accept_one`（`tools.rs:13354`）✓**：状态闸门 ⇒ **全量预检**（**任一步不合法 ⇒ 一步都不写 ✓**）
    //   ⇒ 按序重放（**走 `localDispatch` ✓ —— 与人工操作同一条路 ✓**）⇒ 提交 `accept_suggestion` ＋ 标注 resolved ✓。
    //   **∴ 本地多一层回滚 ✗**：**重放中途运行期失败 ⇒ 已写的状态原子逐条 `revert` ✓**
    //   （**服务端此时会留下部分应用 ✗** —— **∴ 如实登记为「本地更强的一处」✓**，见 `local_rollback_note`）。
    if (path === "/api/tools/accept_suggestion") {
      try {
        const bad = p2ValidateArgsFor("accept_suggestion", body);
        if (bad) return p2ToolFailure(400, bad.code, bad.detail);
        const r = await p2bAcceptOne(body.suggestion_id);
        return json(r.body, r.status);
      } catch (e) { return p2bCatch(e, path); }
    }
    // **⑩b `accept_suggestions` ✓**：**逐条尝试、个别失败不中断 ✓**（**与服务端同形 ✓**）。
    if (path === "/api/tools/accept_suggestions") {
      try {
        const bad = p2ValidateArgsFor("accept_suggestions", body);
        if (bad) return p2ToolFailure(400, bad.code, bad.detail);
        const ids = body.suggestion_ids;
        if (ids.length === 0)
          return p2ToolFailure(400, "invalid_argument", "suggestion_ids 不能为空");
        if (ids.length > 64)
          return p2ToolFailure(400, "invalid_argument",
            "一次最多接受 64 条建议（收到 " + ids.length + "）");
        const results = [];
        let accepted = 0;
        for (const value of ids) {
          if (typeof value !== "string") {
            results.push({ suggestion_id: value, ok: false, error: "id 必须是字符串" });
            continue;
          }
          const r = await p2bAcceptOne(value);
          if (r.ok) {
            accepted += 1;
            results.push({ suggestion_id: value, ok: true,
                           applied_atom_ids: r.body.applied_atom_ids,
                           resolved_annotations: r.body.resolved_annotations });
          } else {
            results.push({ suggestion_id: value, ok: false,
                           error_code: r.body.error_code,
                           detail: r.body.context ? r.body.context.detail : r.body.reason });
          }
        }
        return json({ ok: true, results, accepted, failed: results.length - accepted,
                      pending_annotations: 0, server: false });
      } catch (e) { return p2bCatch(e, path); }
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
      const sinceArg = p2Count(argOf("since_seq"), "since_seq");
      if (sinceArg.error) return sinceArg.error;
      const sugLimitArg = p2Count(argOf("limit"), "limit");
      if (sugLimitArg.error) return sugLimitArg.error;
      const offsetArg = p2Count(argOf("offset"), "offset");
      if (offsetArg.error) return offsetArg.error;
      const since = sinceArg.value !== undefined ? sinceArg.value : 0;
      const limit = Math.min(500, Math.max(1, Math.floor(sugLimitArg.value !== undefined ? sugLimitArg.value : 50)));
      const offset = offsetArg.value !== undefined ? offsetArg.value : 0;
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
                    conflicts_note: "冲突检测与服务端同口径（suggestion_scope ＋ scopes_conflict）："
                      + "整文档／整层级／同对象／区域相交" });
    }
    // **⑭ `preview_suggestion` ✓（本轮落地 ✗ 不再是 501 ✓）**：**∴ 照 `read_preview_suggestion`
    //   （`tools.rs:12768`）逐条 ✓** —— ① `patch` 直接给 或 ② `suggestion_id` 取 `suggest` 原子的 patch ✓
    //   ⇒ **逐步**静态检查**（**与接受路径共用 `p2CheckSingleStep` ✓ ⇒ 两条标准不会分叉 ✓**）
    //   ＋ 逐步影响预估 ✓（**`estimated_region` 是**并集** ✓，`estimated_dirty` 取最大值 ✓**）。
    //   **∴ `error` 字段是 `format!("{:?}", Option<String>)` 的 `Some("…")` 形态 ✓**（**不是裸文本 ✗**）。
    if (path === "/api/tools/preview_suggestion") {
      try {
        const bad = p2ValidateArgsFor("preview_suggestion", body);
        if (bad) return p2ToolFailure(400, bad.code, bad.detail);
        const suggestionId = typeof body.suggestion_id === "string" ? body.suggestion_id : null;
        let patch;
        if (Object.prototype.hasOwnProperty.call(body, "patch")) {
          if (!Array.isArray(body.patch))
            return p2ToolFailure(400, "invalid_argument", "patch 必须是数组");
          patch = body.patch;
        } else {
          if (suggestionId === null)
            return p2ToolFailure(400, "invalid_argument", "需要 suggestion_id 或 patch");
          const all = await atomsOf(handle, doc);
          const atom = all.map((r) => r.atom).find((a) => a && a.id === suggestionId);
          if (!atom) return p2ToolFailure(404, "reference_not_found", "建议 " + suggestionId + " 不在日志中");
          if (atom.kind !== "suggest")
            return p2ToolFailure(400, "invalid_argument", suggestionId + " 不是建议（suggest）原子");
          const stored = (atom.payload || {}).patch;
          patch = Array.isArray(stored) ? stored : [];
        }
        const rows = await atomsOf(handle, doc);
        const knownLayers = new Set(p2cState(rows).layers.map((layer) => layer.layer_id));
        const knownObjects = new Set(
          p2ObjectSummaries(rows, { includeHidden: true }).map((object) => object.object_id));
        const steps = [];
        let invalid = 0, aggregateAffects = "none", aggregateRegion = null;
        for (let index = 0; index < patch.length; index++) {
          const step = patch[index];
          const isObject = step && typeof step === "object" && !Array.isArray(step);
          // **服务端 `step.get(...)` 不认非对象步骤 ✓ ⇒ 一律落到空串／空对象 ✓**
          const tool = isObject && typeof step.tool === "string" ? step.tool : "";
          const stepArgs = isObject && step.arguments !== undefined ? step.arguments : {};
          const { affects, region } = p2EstimateStepImpact(tool, stepArgs);
          const entry = {
            index, tool, class: p2ToolClass(tool), affects,
            estimated_region: region ? [region.x, region.y, region.w, region.h] : null,
            layer_id: isObject && stepArgs && typeof stepArgs === "object"
              && stepArgs.layer_id !== undefined ? stepArgs.layer_id : null,
            object_id: isObject && stepArgs && typeof stepArgs === "object"
              && stepArgs.object_id !== undefined ? stepArgs.object_id : null,
          };
          if (affects === "document") aggregateAffects = "document";
          else if (affects === "layer" && aggregateAffects !== "document") aggregateAffects = "layer";
          else if (affects === "region" && aggregateAffects === "none") aggregateAffects = "region";
          if (region) {
            aggregateRegion = aggregateRegion ? p2BboxUnion(aggregateRegion, region) : region;
          }
          const error = p2CheckSingleStep(tool, stepArgs, knownLayers, knownObjects);
          if (error) {
            invalid += 1;
            entry.valid = false;
            entry.error = p2DebugOptionString(error.detail);
          } else {
            entry.valid = true;
          }
          steps.push(entry);
        }
        return json({
          ok: true, // **∴ 服务端信封也有 `ok:true` ✓**（**其他本地端点同 ✓**）
          suggestion_id: suggestionId, steps, total_steps: patch.length, invalid_steps: invalid,
          applicable: invalid === 0 && patch.length > 0,
          estimated_dirty: aggregateAffects,
          estimated_region: aggregateRegion
            ? [aggregateRegion.x, aggregateRegion.y, aggregateRegion.w, aggregateRegion.h] : null,
          notes: "静态检查：工具存在性、mutating、参数形状与效果类取值范围；"
            + "个别工具的运行期限制（如外扩上限）在应用时最终判定",
          server: false,
          local_replay_note: "本地接受时会按序重放这些步骤；本地尚未实现的工具会在接受时如实报错并回滚"
            + "（预检本身与服务端同口径，不按本地实现裁剪）",
        });
      } catch (e) { return p2bCatch(e, path); }
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
    //   **∴ 分页**：交付「游标之后**最早**的 `limit` 条」（页内仍新的在前 ✓），
    //   `next_since` 只推进到**已交付**的最大 seq ⇒ 照着它轮询**一条不漏** ✓（第 1441 轮修 ✗：
    //   旧版把游标推到**全集**最大 ⇒ 更早的评论永久消失 ✓）**）。
    if (path === "/api/tools/list_comments") {
      const argOf = (k) => (body && body[k] != null ? body[k] : q.get(k));
      const sinceArg = p2Count(argOf("since_seq"), "since_seq");
      if (sinceArg.error) return sinceArg.error;
      const limitArg = p2Count(argOf("limit"), "limit");
      if (limitArg.error) return limitArg.error;
      const since = sinceArg.value !== undefined ? sinceArg.value : 0;
      const limit = Math.min(500, Math.max(1, limitArg.value !== undefined ? limitArg.value : 50));
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
      const truncated = count > limit;
      // **交付「最早的一页」，游标只推进到已交付的最大 seq** ✓（第 1441 轮与服务端同修 ✗）
      //   **∴ 旧版 ✗**：`next_since` 取**匹配全集**的最大 seq，交付的却是**最新一页**
      //     ⇒ 比这一页更早的评论**永远不会再被返回**（node 判据实测：5 条只收到 2 条 ✓）。
      const page = comments.slice(0, limit);
      const nextSince = page.length ? page[page.length - 1].seq : since;
      page.reverse();   // **页内仍按新的在前** ✓（既有显示语义不动 ⇒ 老前端不受影响 ✓）
      return json({ ok: true, comments: page, count, returned: page.length,
                    truncated, limit,
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

    // **★ 命名锚点三件套 ✓ ★**（**照服务端 `tools.rs:2217 write_set_anchor` ／
    //   `:2236 write_delete_anchor` ／ `:2483 read_get_anchors` ＋ `fold.rs:991–1005` 的折叠语义 ✓**）：
    //   **∴ 锚点存进原子日志**（`set_anchor`／`delete_anchor` 是内核认识的 `AtomKind` ✓，
    //   **不另存一份"锚点表" ✗** —— 与 `list_layers` 同一纪律：**状态从日志推导 ⇒ 不会双份漂移 ✓**）;
    //   **∴ 撤销天然可用 ✓**：`__historyStep` 写的 `revert` 原子经 `p2Neutralized` 抑制目标原子 ⇒
    //     「撤掉一次钉锚点」＝该 `set_anchor` 不再生效 ✓（与服务端 `EffectiveActions` 同口径 ✓）。
    /** **锚点折叠 ✗**（**照 `fold.rs:991–1005` 逐条 ✓**：顺序扫原子、`revert` 抑制的跳过、
     *   同名后来者覆盖、`delete_anchor` 移除 ✓）。返回 `Map<name, {x, y, note?}>` ✓。 */
    function p2AnchorsFold(rows) {
      const neutral = p2Neutralized(rows);
      const anchors = new Map();
      for (const r of rows) {
        const a = (r && r.atom) || {};
        if (neutral.has(a.id)) continue;
        const pl = a.payload || {};
        if (a.kind === "set_anchor") {
          const name = typeof pl.name === "string" ? pl.name : "";
          if (!name) continue; // 服务端 precondition 拒空名 ⇒ 折叠对历史里的脏原子保持容错 ✓
          const x = Number(pl.x), y = Number(pl.y);
          if (!Number.isFinite(x) || !Number.isFinite(y)) continue;
          const anchor = { x, y };
          if (typeof pl.note === "string") anchor.note = pl.note;
          anchors.set(name, anchor); // 同名覆盖 ✓（服务端 BTreeMap::insert ✓）
        } else if (a.kind === "delete_anchor") {
          anchors.delete(typeof pl.name === "string" ? pl.name : "");
        }
      }
      return anchors;
    }
    if (path === "/api/tools/set_anchor") {
      const pl = (body && body.payload) || {};
      const name = body.name ?? pl.name;
      const x = body.x ?? pl.x, y = body.y ?? pl.y;
      // **∴ 校验照 `write_set_anchor` ✓**：`name` 必填且非空 ✗、`x`／`y` 必须是数值 ✗（400 而非静默 0 ✓）。
      if (typeof name !== "string" || !name) return p2Bad("set_anchor 需要非空的 name");
      if (typeof x !== "number" || !Number.isFinite(x)) return p2Bad("set_anchor 需要数值 x");
      if (typeof y !== "number" || !Number.isFinite(y)) return p2Bad("set_anchor 需要数值 y");
      const payload = { name, x, y };
      const note = body.note ?? pl.note;
      if (typeof note === "string") payload.note = note; // **∴ 与服务端同 ✓**：note 可选，不给就不写键 ✓
      const atom = p2Envelope("set_anchor", payload);
      const seq = await putAtom(handle, doc, atom);
      return json({ ok: true, atom_id: atom.id, seq, head: seq, name, server: false,
                    note: "已写入 set_anchor 原子（同名覆盖；随日志持久，跨会话不漂移）" });
    }
    if (path === "/api/tools/get_anchors") {
      const anchors = p2AnchorsFold(await atomsOf(handle, doc));
      // **∴ 形状照 `read_get_anchors` ✓**：`anchors` 是**名字⇒对象的映射**（服务端 `BTreeMap` 序列化为对象 ✓）
      //   ⇒ 前端 `Object.keys(anchorCache).sort()` 直接吃这个形状 ✓。
      return json({ ok: true, anchors: Object.fromEntries(anchors), count: anchors.size, server: false });
    }
    if (path === "/api/tools/delete_anchor") {
      const pl = (body && body.payload) || {};
      const name = body.name ?? pl.name;
      if (typeof name !== "string" || !name) return p2Bad("delete_anchor 需要 name（字符串）");
      // **★ 工具层先查存在性 ✓ ★**（**照 `write_delete_anchor` 的实测教训 ✓**：折叠是容错的 ⇒
      //   只靠 `precondition` 的话「删不存在的锚点」会**返回 ok:true** ✗ 静默成功 ——
      //   服务端注释里端到端抓过同一个毛病 ✓，本地必须在端点层自己查 ✓）。
      const anchors = p2AnchorsFold(await atomsOf(handle, doc));
      if (!anchors.has(name)) {
        return p2Bad("找不到锚点 " + name + " ⇒ 现有锚点：" + [...anchors.keys()].sort().join(", "));
      }
      const atom = p2Envelope("delete_anchor", { name });
      const seq = await putAtom(handle, doc, atom);
      return json({ ok: true, atom_id: atom.id, seq, head: seq, name, server: false,
                    note: "已写入 delete_anchor 原子（get_anchors 会少这一条；撤销该删除经 revert 原子生效 ✓）" });
    }

    // **★ `reorder_objects` ✓ ★**（**照 `tools.rs:4029 write_reorder_objects` 的校验序列 ✓**，
    //   **折叠语义照 `fold.rs:758–837`（precondition）与 `:1305–1313`（apply：`order[i] ⇒ z=i`）✓**）：
    //   **∴ 校验顺序与错误码逐个对齐 ✗**（invalid_argument ⇒ 400 / reference_not_found ⇒ 404 /
    //     precondition_failed ⇒ 409，**`http.rs:392` 的映射 ✓**）——
    //     顺序本身也是事实：**重复先于未知**（fold.rs:772 注释「比集合差更具体」✓）。
    if (path === "/api/tools/reorder_objects") {
      const pl = (body && body.payload) || {};
      const layerId = body.layer_id ?? pl.layer_id;
      const order = body.order ?? pl.order;
      if (typeof layerId !== "string" || !layerId) return p2Bad("reorder_objects 需要 layer_id（字符串）");
      if (!Array.isArray(order)) return p2Bad("reorder_objects 需要 order（对象 id 字符串数组）");
      const ids = order.filter((x) => typeof x === "string" && x);
      if (ids.length !== order.length) return p2Bad("order 必须全部是对象 id 字符串");
      const rows = await atomsOf(handle, doc);
      const { layers } = p1LayerOrder(rows);
      const layer = layers.get(layerId);
      if (!layer || layer.deleted) return p2Missing("图层 " + layerId + " 不存在");
      // **该层全部存活对象 ✗**（照 `state.objects_in_layer`：`is_alive` ⇒ 创建未被撤 **且** 无生效 tombstone ✓
      //   ——**含**随图层级联删 ✓，见 `__p2Alive` 的注释 ✓）。
      const neutralRO = p2Neutralized(rows);
      const layersRO = p2PresetLayers(rows, neutralRO);
      const seenIds = new Set();
      const alive = [];
      for (const r of rows) {
        const a2 = (r && r.atom) || {};
        const id = a2.payload && a2.payload.object_id;
        if (typeof id !== "string" || !id || seenIds.has(id)) continue;
        seenIds.add(id);
        const v = p2ObjectView(rows, id);
        if (__p2Alive(rows, v, layersRO, neutralRO) && v.layer_id === layerId) alive.push(id);
      }
      // **工具层先查「无可重排」✓**（`tools.rs:4057` ⇒ invalid_argument ✓，先于折叠的逐条分类 ✓）。
      if (alive.length === 0) return p2Bad("图层 " + layerId + " 没有对象，无可重排");
      const aliveSet = new Set(alive);
      // ① **重复先报**（`fold.rs:772–792` ⇒ precondition_failed ✓「比集合差更具体」✓）。
      const dup = new Set(), dups = [];
      for (const id of ids) { if (dup.has(id)) dups.push(id); else dup.add(id); }
      if (dups.length) {
        return p2Precon("reorder_objects 的 order 含重复对象 id：" + [...new Set(dups)].join("、"));
      }
      // ② **逐个分类：未知 ⇒ 404 且点名现有对象 ✓／已删 ⇒ 409 ✓／跨层 ⇒ 409 ✓**（`fold.rs:801–819` ✓）。
      for (const id of ids) {
        const v = p2ObjectView(rows, id);
        if (!v.found || !__p2Alive(rows, v, layersRO, neutralRO)) {
          if (v.found) return p2Precon("对象 " + id + " 已被删除");
          return p2Missing("对象 " + id + " 不存在 ⇒ 现有对象：" + alive.slice().sort().join("、"));
        }
        if (v.layer_id !== layerId) {
          return p2Precon("对象 " + id + " 属于图层 " + v.layer_id + "，不在目标图层 " + layerId + " 内");
        }
      }
      // ③ **完整性：期望 = 该层全部存活对象**（`fold.rs:821–836` ⇒ precondition_failed ✓）。
      if (aliveSet.size !== ids.length) {
        const missing = [...aliveSet].filter((id) => ids.indexOf(id) < 0);
        return p2Precon("reorder_objects 必须携带完整目标 z 序（图层 " + layerId +
                        "：期望 " + aliveSet.size + " 个对象，实际 " + ids.length + " 个；缺失：" + missing.join("、") + "）");
      }
      const atom = p2Envelope("reorder_objects", { layer_id: layerId, order: ids });
      const seq = await putAtom(handle, doc, atom);
      return json({ ok: true, atom_id: atom.id, seq, head: seq, layer_id: layerId,
                    count: ids.length, server: false,
                    note: "已写入 reorder_objects 原子：order[i] ⇒ z_index=i（照 fold.rs:1311）；撤销经 revert 机器生效 ✓" });
    }

    // **★ C/S 客户端渲染：本地没实现的端点 ⇒ **只读代理**到服务端 ✗ ★**（第 10 轮 ✓）
    //   **∴ 为什么 ✗**：`list_effects` 等**只在服务端有**（`get_anchors` 曾在此列 ⇒ **现已本地实现 ✓**）⇒ 本地层原来抛
    //     `endpoint_not_local` ✓ ⇒ 页面初始化／效果面板**每次**打一行红字 ✓（实测日志 ✓），
    //     功能直接不可用 ✓。而 C/S 模式的文档本来就在服务端 ✓ ⇒ 这些**读**请求代理过去是
    //     如实作答 ✓（响应头 `x-yanshi-source: server-proxy` ✓，不假装是本地算的 ✓）。
    //   **∴ 只在这个模式**：PWA（`__pwaLocalOnly === true`）仍抛 `endpoint_not_local` ✓ ——
    //     没有服务器可代理 ✓。**写**请求不会走到这里 ✓（C/S 的写工具在 `callTool` 已路由服务端 ✓）。
    if (__csClientMode()) {
      try {
        const init = { method: req.method };
        const ct = (req.headers && req.headers.get && req.headers.get("content-type")) || "";
        if (req.method !== "GET" && req.method !== "HEAD") {
          init.headers = { "content-type": ct || "application/json" };
          init.body = JSON.stringify(body || {});
        }
        if (typeof AbortSignal !== "undefined" && typeof AbortSignal.timeout === "function") {
          init.signal = AbortSignal.timeout(15000);
        }
        const res = await fetch(url.pathname + url.search, init);
        const buf = await res.arrayBuffer();
        return new Response(buf, {
          status: res.status,
          headers: {
            "content-type": res.headers.get("content-type") || "application/json",
            "cache-control": "no-store",
            "x-yanshi-source": "server-proxy",
          },
        });
      } catch (e) {
        // 代理失败（离线等）⇒ 落到下面的如实 501 ✓
      }
    }
    // **∴ 其余端点**一律抛给上层 ⇒ 上层如实 501 ＋ 原因 ✗**（**∴ 不假装成功 ✓**）。
    throw new Error(`endpoint_not_local: ${url.pathname}`);
  }

  /** **★ 唯一的**兜底 catch ✗ ★**（**∴ 只换装 `putAtom` 锁闸的错误 ✓，其余一律**再抛** ✓）：
   *   **∴ 为什么在这里 ✗**：锁闸挂在 `putAtom`（必经之路 ✓），拒绝时**抛**出去 ✓
   *   ⇒ **∴ 这里把它换成本地统一错误形状的 403 ✓**（`p2ToolFailure` ✓ 与服务端同形 ✓）；
   *   **∴ 必须 `await` ✓**（**见 `localDispatch` 的注释：不 await 抓不到异步拒绝 ✓**）；
   *   **∴ 未实现端点的 `endpoint_not_local` 照旧抛给上层 501 ✓**（行为不变 ✓）。 */
  return async function local(req) {
    try {
      return await localDispatch(req);
    } catch (e) {
      if (e && e.__p2Failure) return e.__p2Failure;
      throw e;
    }
  };
}

/** **∴ 内核没暴露尺寸 ✗ ⇒ **∴ 用一个安全默认值 ✓**（**真实尺寸由 `load_atoms_json` 里的创建原子决定 ✓**）。 */
function k_width() { return 1024; }

/**
 * **★ blob 引用集合的**唯一**推导 ✓ ★**（第 1440 轮审计 ✓）。
 *
 * **∴ 为什么 ✗**：本地 `collect_garbage`／`blob_gc`／`export_project` 三处**各写一遍**
 * "只看 `payload.bitmap.blob_hash`" ✗ ⇒ `import_image` 的 `payload.blob_hash`、
 * 纹理对象的 `data.texture.blob_hash`……**凡是固定路径之外**的引用一律**看不见** ✗
 * ⇒ `confirm:true` 会删掉**仍在用**的 blob ✗ ⇒ 重启后图消失 ✗（真数据丢失 ✓）。
 * 服务端 `collect_refs`（`atom.rs` ✓）是**递归遍历**、收 `blob_hash`/`blob` 键下的
 * `sha256:` 字符串 ⇒ **这里与之同口径** ✓，且**同一事实只留这一处** ✓（内核 ② ✓）。
 */
function p2BlobRefsOfAtoms(atoms) {
  const refs = new Set();
  const walk = (value, key) => {
    if (Array.isArray(value)) { for (const item of value) walk(item, key); return; }
    if (value && typeof value === "object") {
      for (const [k, v] of Object.entries(value)) walk(v, k);
      return;
    }
    if ((key === "blob_hash" || key === "blob") &&
        typeof value === "string" && value.startsWith("sha256:")) {
      refs.add(value);
    }
  };
  for (const a of atoms || []) walk(a && a.payload, null);
  return refs;
}

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

// ============================================================================
// **★ 风格预设四端点的本地映射 ✓ ★**（第 2 批 ✓；**服务端出处 `crates/yanshi-server/src/presets.rs` ✓**）
//
// **∴ 这一节是纯函数层 ✓**（**∴ 只做「读库／展开／校验」✗ ⇒ **∴ 不碰 IndexedDB／内核 ✓**）⇒
//   **∴ 于是**判据可以直接 `import` 它们逐项比对服务端的输出 ✓（**`tool-pwa-preset-library-parity.mjs` ✓**）。
//   **∴ 端点分支在 `makeLocalApi` 里 ✓**（**落成要写原子 ✗ ⇒ **∴ 只有那半边需要闭包 ✓**）。
//
// **★ 库为什么内联而不是多一个资产文件 ✓ ★**：
//   **∴ `api-local.js` 已被 SW 预缓存 ✓**（**`tool-pwa-assets` 的清单 ✓**）⇒
//   **∴ 内联 ⇒ 离线必定可用 ✓，**而新增一个 `style-presets.json` 资产要同时改预缓存清单 ✗**
//   （**漏一处 ⇒ 离线时 `fetch` 它拿到 504 ⇒ **∴ 正是本仓最怕的「静默降级」✓**）。
//   **∴ 代价 ✓**：库文本有两份（**源文件 ＋ 内联串 ✓**）⇒ **∴ 所以配一条**逐字相等**判据 ✗**
//   （**`p2PresetRawText()` 必须逐字等于 `crates/yanshi-server/data/style-presets.json` ✓**）
//   ＋ **一条再生成脚本 ✓**（**`scripts/gen-pwa-preset-library.mjs` ✓**）。
//
// **∴ 数字文本的两处已知残余 ✗**（**如实标注 ✗ 不装作完全等价 ✓**）：
//   ① **JS `JSON.parse` 把 `2.0` 收成 `2`** ⇒ 「得到 {value}」这类**文案**里
//     `2.0` 会写成 `2`（**Rust 的 serde_json 保留 `2.0` ✓**）⇒ **解析后的数值仍相等 ✓**；
//   ② **`1e21` 这类极端量级**的展开位数与 ryu 的最短指数写法可能有差 ✓（**已按 `e+`→`e` 归一 ✓**）。
//   **∴ 所以判据断的是「解析后的深相等」✗ 不是「响应字节逐字相等」✓**（**这条口径写进判据注释 ✓**）。
// ============================================================================

/** **`f64` 的 Rust `Display` ✓**（**∴ 位置记法、不用科学计数 ✓**；**`inf`／`-inf`／`NaN` 逐字照 std ✓**）。 */
export function p2RustNum(value) {
  const v = Number(value);
  if (Number.isNaN(v)) return "NaN";
  if (v === Infinity) return "inf";
  if (v === -Infinity) return "-inf";
  const s = Object.is(v, -0) ? "-0" : String(v);
  if (!/[eE]/.test(s)) return s;
  return p2ExpandExp(s);
}

/** **把 JS 的科学计数串展开成位置记法 ✓**（**`1e-8 ⇒ 0.00000001`／`1.5e+21 ⇒ 1500000000000000000000` ✓**）。 */
function p2ExpandExp(str) {
  const neg = str.startsWith("-");
  const bare = neg ? str.slice(1) : str;
  const [mantissa, expText] = bare.split(/e/i);
  const exp = parseInt(expText, 10);
  const [ip, fp = ""] = mantissa.split(".");
  const digits = ip + fp;
  const point = ip.length + exp;
  let out;
  if (point <= 0) out = "0." + "0".repeat(-point) + digits;
  else if (point >= digits.length) out = digits + "0".repeat(point - digits.length);
  else out = digits.slice(0, point) + "." + digits.slice(point);
  return (neg ? "-" : "") + out;
}

/** **serde_json 的紧凑文本 ✓**（**`Value` 的 `Display` ⇒ 对象键**按 BTreeMap 排序 ✓**；
 *   **数值走 ryu ⇒ 指数写法没有 `+` ✓**；**非有限数序列化成 `null` ✓**）。 */
export function p2JsonText(value) {
  if (value === null || value === undefined) return "null";
  if (typeof value === "boolean") return value ? "true" : "false";
  if (typeof value === "number") {
    if (!Number.isFinite(value)) return "null";
    return String(value).replace("e+", "e");
  }
  if (typeof value === "string") return JSON.stringify(value);
  if (Array.isArray(value)) return "[" + value.map(p2JsonText).join(",") + "]";
  if (typeof value === "object") {
    return "{" + Object.keys(value).sort()
      .map((k) => JSON.stringify(k) + ":" + p2JsonText(value[k])).join(",") + "}";
  }
  return String(value);
}

/** **`Value == Value` ✓**（**∴ 照 serde_json：对象只看键集与值 ✗ 不看顺序 ✓**）。 */
function p2JsonDeepEqual(a, b) { return p2JsonText(a) === p2JsonText(b); }

/** **深拷贝 ＋ 键序归一 ✓**（**Rust 侧一切都是 `BTreeMap` ⇒ 输出必按键排序 ✓；
 *   **∴ 本地用插入序 ✗ ⇒ **∴ 出响应前统一走这一条 ✓**（**数值与解析后的结构不变 ✓**）。 */
export function p2SortedCopy(value) {
  if (Array.isArray(value)) return value.map(p2SortedCopy);
  if (value && typeof value === "object") {
    const out = {};
    for (const key of Object.keys(value).sort()) out[key] = p2SortedCopy(value[key]);
    return out;
  }
  return value;
}

/** **`StylePreset` 结构体的投影 ✓**（**∴ 服务端 `serde_json::to_value(preset)` 只输出结构体字段 ✗**
 *   （**`neutral`／`knobs` 有 `#[serde(default)]` ⇒ 缺席时补 `{}`／`[]` ✓；
 *     **`source` 由加载器回填成 `builtin` ✓ ⇒ 数据文件里那份即使写了也不算 ✓**）。 */
function p2PresetWire(preset) {
  // **`params`／`neutral` 在 Rust 侧是 `serde_json::Map`＝BTreeMap ⇒ 输出按**排序键**✓**
  //   （**∴ 数据文件里的书写序不算数 ✓ 本地要归一 ✓**；结构体字段本身按**声明序**✓ 不排序 ✗）。
  const opWire = (o) => ({
    kind: o.kind,
    type: o.type,
    params: p2SortedCopy(o.params),
    neutral: o.neutral === undefined ? {} : p2SortedCopy(o.neutral),
  });
  const targets = (k) => (k.targets || []).map((t) => ({ op: t.op, param: t.param, range: t.range }));
  const knob = (k) => ({ id: k.id, label: k.label, description: k.description,
                         min: k.min, max: k.max, default: k.default, targets: targets(k) });
  const correction = preset.correction || {};
  return {
    id: preset.id,
    name_zh: preset.name_zh,
    name_en: preset.name_en,
    status: preset.status,
    tags: preset.tags || [],
    suitable: preset.suitable || [],
    caution: preset.caution || [],
    description: preset.description,
    notes: preset.notes,
    correction: { baseline: correction.baseline, ops: (correction.ops || []).map(opWire) },
    style_ops: (preset.style_ops || []).map(opWire),
    knobs: (preset.knobs || []).map(knob),
    source: "builtin",
  };
}

let P2_PRESET_CACHE = null;
/** **内联库的原始文本 ✓**（**判据拿它与源文件逐字比 ✓ —— 「同一事实只写一处」的**看守**✓**）。 */
export function p2PresetRawText() { return P2_PRESET_RAW; }
/** **加载内联预设库 ✓**（**惰性解析一次 ✓**）。
 *  **∴ 不做 `validate_library` ✗**（**∴ 库与服务端**逐字同一份 ✓ 由 `tool-pwa-preset-library-parity` 看守 ✓，
 *    **∴ 而服务端自己的 Rust 测试 `every_preset_expands_and_passes_validate_effect` 已证明它合法 ✓** ⇒
 *    **∴ 本地再校验一遍只是**把同一句话抄第二次 ✗**）。 */
export function p2PresetLibrary() {
  if (!P2_PRESET_CACHE) {
    const raw = JSON.parse(P2_PRESET_RAW);
    P2_PRESET_CACHE = {
      version: raw.version,
      source: raw.source,
      presets: (raw.presets || []).map(p2PresetWire),
    };
  }
  return P2_PRESET_CACHE;
}

/** **预设系统的错误 ✗**：`status`／`code`／`detail` 挂在异常上 ⇒ 端点分支统一转成响应 ✓
 *   （**∴ 服务端全部走 `preset_err = InvalidArgument` ⇒ 400 ✓；
 *     **∴ 只有「图层不存在」是 `ReferenceNotFound` ⇒ 404 ＋ `context.layer_id` ✓**）。 */
function p2PresetThrow(status, code, detail, layerId) {
  const error = new Error(detail);
  error.__presetError = { status, code, detail, layerId: layerId || null };
  throw error;
}
const p2PresetBad = (detail) => p2PresetThrow(400, "invalid_argument", detail);
/** **「找不到」那一支 ✗**（**∴ 服务端 `ReferenceNotFound` ⇒ 404 ＋ `context.layer_id` ✓**）。 */
const p2PresetGone = (detail, layerId) => p2PresetThrow(404, "reference_not_found", detail, layerId);

/** **异常 ⇒ 响应 ✓**（**不是本层的错误 ⇒ 返回 `null` ⇒ 调用方继续抛 ✗**，
 *   **∴ 绝不把编程错误伪装成 400 ✓**）。 */
export function p2PresetResponse(error) {
  const info = error && error.__presetError;
  if (!info) return null;
  const body = { ok: false, error: info.code, error_code: info.code, reason: info.detail,
                 context: { detail: info.detail }, retryable: false, server: false };
  if (info.layerId) body.context.layer_id = info.layerId;
  return json(body, info.status);
}

/** **在库里按 id 找预设 ✗**（**照 `find_preset` ✓：找不到 ⇒ 400 且**列出全部可用 id**✓**）。 */
export function p2FindPreset(presetId) {
  const library = p2PresetLibrary();
  const preset = library.presets.find((p) => p.id === presetId);
  if (!preset) {
    p2PresetBad(`未知预设 preset_id：「${presetId}」；可用预设共 ${library.presets.length} 套：`
      + library.presets.map((p) => p.id).join(" / "));
  }
  return preset;
}

/** **`list_presets` 的摘要项 ✓**（**照 `preset_summary` ✗ 键集逐字 ✓**）。 */
export function p2PresetSummary(preset) {
  return {
    preset_id: preset.id,
    name_zh: preset.name_zh,
    name_en: preset.name_en,
    status: preset.status,
    tags: preset.tags,
    suitable: preset.suitable,
    caution: preset.caution,
    description: preset.description,
    correction_baseline: preset.correction.baseline,
    correction_op_count: preset.correction.ops.length,
    style_op_count: preset.style_ops.length,
    knobs: preset.knobs.map((k) => ({ id: k.id, label: k.label, min: k.min, max: k.max,
                                      default: k.default })),
    source: preset.source,
  };
}

/** **「X 不存在」那句话的通用形状 ✗**（**照 `error.rs:156 missing_reference` 逐字 ✓**：
 *   **上限 8 个候选 ＋ 第 9 个只用来决定要不要写「…」✓**；**空集是另一句话 ✓**。
 *   **∴ 一份实现，两处调用 ✓**（**图层 ✗ 与服务端 `log.rs:1198` 过滤存活层 ✓**，
 *   **对象 ✗ 与服务端 `log.rs:1173` 不过滤（墓碑也在状态里）✓** ⇒
 *   **谓词由调用方决定，文案只有一份 ✓**——服务端注释写明了这条规则：「各写一份必然漂移 ✗」）。
 *   **∴ 为什么值得逐字 ✗**：这句话是调用方**下一步做什么**的依据（「现有图层：…」＝可直接抄的名单 ✓）
 *   ⇒ 少一个逗号都会让人多查一轮 ✓。 */
export function p2MissingReferenceMessage(kindName, id, keys) {
  const LIMIT = 8;
  const ids = [...keys].sort().slice(0, LIMIT + 1);
  let hint;
  if (ids.length === 0) hint = `（当前没有任何${kindName}）`;
  else {
    const shown = ids.slice(0, LIMIT).join(", ");
    hint = ids.length > LIMIT ? `（现有${kindName}：${shown} …）` : `（现有${kindName}：${shown}）`;
  }
  return `${kindName} ${id} 不存在${hint}`;
}

/** **「图层不存在」那句话 ✗**（**候选＝**活着**的层并按 id 升序 ✓**，
 *   **`state.layers` 是 BTreeMap ⇒ `keys()` 天然有序 ✓**）。 */
export function p2LayerMissingMessage(layers, layerId) {
  const alive = [...layers.keys()].filter((id) => layers.get(id).alive);
  return p2MissingReferenceMessage("图层", layerId, alive);
}

/** **「对象不存在」那句话 ✗**（**候选＝状态里全部对象 ✓ 含墓碑 ✓**，见 `p2StateObjectIds`）。 */
export function p2ObjectMissingMessage(objectIds, objectId) {
  return p2MissingReferenceMessage("对象", objectId, objectIds);
}

/** **递归插值 ✓**（**照 `interpolate` ✗ 四条分支与三条报错文案逐字 ✓**；
 *   **∴ 对象分支按**排序键**遍历 ✓**（**Rust 的 `Map` 是 BTreeMap ✓** ⇒
 *   「缺键」时报的是哪个键 ✗ 与输出键序 **都**由它决定 ✓）。 */
export function p2Interpolate(recipe, neutral, t, what) {
  if (typeof recipe === "number" && typeof neutral === "number") {
    return neutral + (recipe - neutral) * t;
  }
  if (Array.isArray(recipe) && Array.isArray(neutral)) {
    if (recipe.length !== neutral.length) {
      p2PresetBad(`${what} 的配方值与中性值长度不一致（${recipe.length} vs ${neutral.length}），无法逐分量插值`);
    }
    return recipe.map((rv, i) => p2Interpolate(rv, neutral[i], t, `${what}[${i}]`));
  }
  if (recipe && neutral && typeof recipe === "object" && typeof neutral === "object") {
    const out = {};
    for (const key of Object.keys(recipe).sort()) {
      if (!Object.prototype.hasOwnProperty.call(neutral, key)) {
        p2PresetBad(`${what} 的中性值缺少键「${key}」，无法插值`);
      }
      out[key] = p2Interpolate(recipe[key], neutral[key], t, `${what}.${key}`);
    }
    return out;
  }
  if (p2JsonDeepEqual(recipe, neutral)) return p2SortedCopy(recipe);
  p2PresetBad(`${what} 是非数值参数（配方 ${p2JsonText(recipe)} vs 中性 ${p2JsonText(neutral)}），`
    + "不能用 strength 插值；请把它移出 neutral（作固定参数）或改配方让两端相等");
}

/** **旋钮映射的插值 ✓**（**照 `interpolate_range`：`interpolate(high, low, t)` ✓**）。 */
export function p2InterpolateRange(low, high, t, what) {
  return p2Interpolate(high, low, t, what);
}

/** **钳制表 ✓**（**照 `clamp_params` ✗ 与 `tools_effect.rs` 的数值检查同口径 ✓**；
 *   **∴ Rust 用 `match (kind, type_name)` 而 kind 全是 `_` ⇒ 表实际只按类型名取 ✓**。 */
const P2_PRESET_CLAMPS = {
  brightness_contrast: [["brightness", -1, 1], ["contrast", 0, 8]],
  saturation: [["amount", 0, 8]],
  vibrance: [["amount", 0, 2]],
  exposure: [["ev", -10, 10]],
  white_balance: [["temperature", -1, 1], ["tint", -1, 1]],
  shadows_highlights: [["shadows", -1, 1], ["highlights", -1, 1]],
  levels: [["black", 0, 0.99], ["white", 0.01, 1], ["gamma", 0.01, 10]],
  hsl: [["hue", -360, 360], ["saturation", 0, 8], ["lightness", -1, 1]],
  split_toning: [["balance", -1, 1], ["amount", 0, 1]],
  black_and_white: [["amount", 0, 1]],
  dehaze: [["amount", -1, 1]],
  clarity: [["amount", -1, 1]],
  texture: [["amount", -1, 1]],
  film_grain: [["amount", 0, 1], ["size", 1, 8]],
  noise: [["amount", 0, 1]],
  vignette: [["strength", 0, 1], ["radius", 0, 2], ["softness", 0.02, 2]],
  glow: [["threshold", 0, 1], ["radius", 1, 64], ["intensity", 0, 4]],
  sharpen: [["amount", 0, 5], ["radius", 1, 8]],
};

/** **把展开后的参数钳进允许范围 ✓**（**照 `clamp_params` ✗ 三条细则逐字 ✓**：
 *   **∴ 只认数值 ✓**（**`as_f64` 对串／布尔得 `None` ⇒ 不动 ✓**）；
 *   **∴ 钳到边界外才回写 ✓**（**`clamped != value` ⇒ 原位保留 ✓**）；
 *   **∴ 影调数组 `shadows`／`midtones`／`highlights` 逐分量钳 ±1 ✓**（**只有数组才处理 ✓**）。 */
export function p2ClampPresetParams(typeName, params) {
  const checks = P2_PRESET_CLAMPS[typeName] || [];
  if (!params || typeof params !== "object" || Array.isArray(params)) return params;
  const clampOne = (value, low, high) => (value < low ? low : value > high ? high : value);
  for (const [key, low, high] of checks) {
    const value = params[key];
    if (typeof value !== "number") continue;
    const clamped = clampOne(value, low, high);
    if (clamped !== value) params[key] = clamped;
  }
  for (const band of ["shadows", "midtones", "highlights"]) {
    const array = params[band];
    if (!Array.isArray(array)) continue;
    for (let i = 0; i < array.length; i++) {
      if (typeof array[i] === "number") array[i] = clampOne(array[i], -1, 1);
    }
  }
  return params;
}

/** **展开一套预设 ✓**（**照 `expand_preset` ✗ 纯函数 ✓**：
 *   **① strength 合法性 → ② 旋钮**全量**校验（按排序键 ✓）→ ③ 修正段原样＋钳制 →
 *     ④ 风格段逐参数插值（**`black_and_white` 的 `amount` 中性值恒 0 ✓**）→
 *     ⑤ 旋钮覆盖（只动风格段 ✓）→ ⑥ 风格段再统一钳制 ✓**。
 *   **∴ 返回的每一项已带确定性 `object_id` ✓**（**`preset_{id}_{c|s}_{NN}_{type}` ✓**。 */
export function p2ExpandPreset(preset, strength, knobs, includeCorrection) {
  if (!Number.isFinite(strength) || !(strength >= 0 && strength <= 1)) {
    p2PresetBad(`预设「${preset.id}」的 strength 必须是 [0, 1] 内的有限数，得到 ${p2RustNum(strength)}`);
  }
  const knobNames = Object.keys(knobs || {}).sort();
  for (const key of knobNames) {
    const knob = preset.knobs.find((k) => k.id === key);
    if (!knob) {
      p2PresetBad(`预设「${preset.id}」没有旋钮「${key}」；可用旋钮：${preset.knobs.map((k) => k.id).join(" / ")}`);
    }
    const value = knobs[key];
    if (typeof value !== "number") {
      p2PresetBad(`预设「${preset.id}」的旋钮「${key}」必须是数字，得到 ${p2JsonText(value)}`);
    }
    if (!Number.isFinite(value) || !(value >= knob.min && value <= knob.max)) {
      p2PresetBad(`预设「${preset.id}」的旋钮「${key}」必须在 [${p2RustNum(knob.min)}, ${p2RustNum(knob.max)}] 内，`
        + `得到 ${p2RustNum(value)}`);
    }
  }
  const ops = [];
  if (includeCorrection) {
    preset.correction.ops.forEach((op, index) => {
      const params = p2ClampPresetParams(op.type, p2SortedCopy(op.params));
      ops.push({ segment: "correction", index, kind: op.kind, type: op.type, params,
                 object_id: `preset_${preset.id}_c_${String(index).padStart(2, "0")}_${op.type}` });
    });
  }
  preset.style_ops.forEach((op, index) => {
    const recipe = op.params;
    if (!recipe || typeof recipe !== "object" || Array.isArray(recipe)) {
      p2PresetBad(`预设「${preset.id}」的 ${op.type} 操作 params 必须是对象`);
    }
    const expanded = {};
    for (const key of Object.keys(recipe).sort()) {
      const recipeValue = recipe[key];
      let value;
      if (Object.prototype.hasOwnProperty.call(op.neutral, key)) {
        value = p2Interpolate(recipeValue, op.neutral[key], strength,
          `预设「${preset.id}」${op.type} 的参数 ${key}`);
      } else if (op.type === "black_and_white" && key === "amount") {
        // 期 4 D2-2：黑白 op 的 amount 中性值恒 0（数据里写没写都按 0 算 ✓）。
        value = p2Interpolate(recipeValue, 0, strength,
          `预设「${preset.id}」${op.type} 的参数 ${key}`);
      } else {
        value = p2SortedCopy(recipeValue); // 固定参数：strength 不动
      }
      expanded[key] = value;
    }
    ops.push({ segment: "style", index, kind: op.kind, type: op.type, params: expanded,
               object_id: `preset_${preset.id}_s_${String(index).padStart(2, "0")}_${op.type}` });
  });
  for (const key of knobNames) {
    const knob = preset.knobs.find((k) => k.id === key);
    const v = knobs[key];
    const t = (v - knob.min) / (knob.max - knob.min);
    for (const target of knob.targets) {
      const op = ops.find((o) => o.segment === "style" && o.index === target.op);
      if (!op) {
        // 服务端这里是 `.expect()` ⇒ panic；本地如实报错而不是静默跳过 ✗。
        p2PresetBad(`预设「${preset.id}」的旋钮「${key}」指向风格段第 ${target.op} 笔，但库里没有它（库数据非法）`);
      }
      const mapped = p2InterpolateRange(target.range[0], target.range[1], t,
        `预设「${preset.id}」旋钮「${key}」的目标参数 ${target.param}`);
      op.params[target.param] = mapped;
    }
  }
  for (const op of ops) if (op.segment === "style") p2ClampPresetParams(op.type, op.params);
  return ops;
}

/** **解析 `strength`／`knobs`／`include_correction` ✓**（**照 `parse_expand_args` ✗ 三条文案逐字 ✓**；
 *   **∴ 显式 `null` 走到这里 ⇒ 「必须是数字／对象／布尔 ✗ 得到 null」✓**（**schema 只放行**可选**键的 null ✓**）。 */
export function p2ParsePresetExpandArgs(preset, args) {
  const body = args && typeof args === "object" && !Array.isArray(args) ? args : {};
  let strength = 1.0;
  if (body.strength !== undefined) {
    if (typeof body.strength !== "number") {
      p2PresetBad(`预设「${preset.id}」的 strength 必须是数字，得到 ${p2JsonText(body.strength)}`);
    }
    strength = body.strength;
  }
  let knobs = {};
  if (body.knobs !== undefined) {
    if (!body.knobs || typeof body.knobs !== "object" || Array.isArray(body.knobs)) {
      p2PresetBad(`预设「${preset.id}」的 knobs 必须是对象（旋钮 id → 数值），得到 ${p2JsonText(body.knobs)}`);
    }
    knobs = p2SortedCopy(body.knobs);
  }
  let includeCorrection = true;
  if (body.include_correction !== undefined) {
    if (typeof body.include_correction !== "boolean") {
      p2PresetBad(`预设「${preset.id}」的 include_correction 必须是布尔，得到 ${p2JsonText(body.include_correction)}`);
    }
    includeCorrection = body.include_correction;
  }
  return { strength, knobs, includeCorrection };
}

/** **框架级参数 ✓**（**照 `FRAMEWORK_PARAMS`：`doc_id`／`actor`／`session` ⇒ 校验时放行 ✓**）。 */
export const P2_PRESET_FRAMEWORK = ["doc_id", "actor", "session"];

/** **`json_type_name` ✓**（**照服务端 ✗ 六个名字逐字 ✓**。 */
function p2JsonTypeName(value) {
  if (value === null || value === undefined) return "null";
  if (typeof value === "boolean") return "boolean";
  if (typeof value === "number") return "number";
  if (typeof value === "string") return "string";
  if (Array.isArray(value)) return "array";
  if (typeof value === "object") return "object";
  return "null";
}

/** **入口参数校验 ✓**（**照 `validate_args` 的**三段顺序**✗ 实测过 ✓**：
 *   **① 未知参数**（按排序键取第一个 ✗ 框架级除外）**→ ② 必填缺席**（按声明顺序）**→
 *   ③ 声明类型不符**（按声明顺序）**；可选键显式传 `null` ⇒ 视为未提供 ✓**。
 *   @param {string} toolName 工具名（进错误文案 ✓）
 *   @param {Array<{name:string,kind:string,required:boolean}>} declared **声明顺序**的表 ✓
 *   @param {object} body 请求体 */
export function p2PresetSchemaCheck(toolName, declared, body) {
  const object = body && typeof body === "object" && !Array.isArray(body) ? body : {};
  for (const key of Object.keys(object).sort()) {
    if (P2_PRESET_FRAMEWORK.includes(key)) continue;
    if (!declared.some((p) => p.name === key)) {
      const accepted = declared.length ? declared.map((p) => p.name).join(", ") : "无";
      p2PresetBad(`${toolName} 不接受参数 ${key}（拼写错误？）；可用参数：${accepted}；`
        + `框架级参数：${P2_PRESET_FRAMEWORK.join(", ")}`);
    }
  }
  for (const param of declared) {
    if (!Object.prototype.hasOwnProperty.call(object, param.name)) {
      if (param.required) p2PresetBad(`缺少必填参数 ${param.name}`);
      continue;
    }
    const value = object[param.name];
    if (value === null && !param.required) continue;
    const matches = param.kind === "string" ? typeof value === "string"
      : param.kind === "number" ? typeof value === "number"
      // **∴ `Integer` 放行**整数形态的浮点**✗**（**照 `validate_args`：`fract() == 0` ✓**）
      : param.kind === "integer" ? typeof value === "number" && Number.isInteger(value)
      // **∴ `Any` ⇒ **恒匹配** ✓**（**服务端 `ParamKind::Any => true` ✓**，**连 `null` 也放行 ✓**）
      : param.kind === "any" ? true
      : param.kind === "boolean" ? typeof value === "boolean"
      : param.kind === "object" ? value !== null && typeof value === "object" && !Array.isArray(value)
      : param.kind === "array" ? Array.isArray(value)
      // 声明表里出现没实现的类型 ⇒ 响亮报错，不像 `true` 那样静默放行 ✗。
      : p2PresetBad(`本地预设 schema 未实现参数类型 ${param.kind}（请补，不要静默放行）`);
    if (!matches) {
      p2PresetBad(`参数 ${param.name} 类型应为 ${param.kind}，实际为 ${p2JsonTypeName(value)}`);
    }
  }
  return object;
}

/** **`render` 参数校验 ✓**（**照 `parse_preview_render` ✗ 五段文案逐字 ✓**：
 *   **缺席 ⇒ 不渲染 ✓；在场 ⇒ 严格校验 ✗ 未知键／非法 max_edge／非法 format 一律响亮拒绝 ✓**。
 *   **∴ 本地没有单层裸像素渲染 ⇒ 校验通过后端点仍回 501 ✗**（**先校验再 501 ⇒ 与服务的错误口径一致 ✓**。 */
export function p2PresetParseRender(args) {
  const body = args && typeof args === "object" && !Array.isArray(args) ? args : {};
  if (!Object.prototype.hasOwnProperty.call(body, "render")) return null;
  const render = body.render;
  if (!render || typeof render !== "object" || Array.isArray(render)) {
    p2PresetBad(`preview_preset 的 render 必须是对象（{max_edge, format}），得到 ${p2JsonText(render)}`);
  }
  for (const key of Object.keys(render).sort()) {
    if (key !== "max_edge" && key !== "format") {
      p2PresetBad(`preview_preset 的 render 不认键「${key}」；只认 max_edge / format`);
    }
  }
  let maxEdge = 512;
  if (render.max_edge !== undefined) {
    const value = render.max_edge;
    const isInt = typeof value === "number" && Number.isFinite(value) &&
      Math.floor(value) === value && value >= 0;
    if (!isInt) p2PresetBad(`preview_preset 的 render.max_edge 必须是整数，得到 ${p2JsonText(value)}`);
    if (!(value >= 128 && value <= 2048)) {
      p2PresetBad(`preview_preset 的 render.max_edge 必须在 [128, 2048] 内，得到 ${String(value)}`);
    }
    maxEdge = value;
  }
  let format = "jpeg";
  if (render.format !== undefined) {
    if (render.format !== "png" && render.format !== "jpeg") {
      p2PresetBad(`preview_preset 的 render.format 只收 png / jpeg，得到 ${p2JsonText(render.format)}`);
    }
    format = render.format;
  }
  return { maxEdge, format };
}

/** **尺度诚实标注 ✓**（**照 `preview_scale_notes` ✗ 名单与文案逐字 ✓**：
 *   **`clarity`／`texture`／`glow` ⇒ 点名 ✓；不含 ⇒ 空串 ✓**。 */
export const P2_PRESET_SCALE_SENSITIVE = ["clarity", "texture", "glow"];
export function p2PresetScaleNotes(ops) {
  const found = [];
  for (const op of ops || []) {
    if (P2_PRESET_SCALE_SENSITIVE.includes(op.type) && !found.includes(op.type)) found.push(op.type);
  }
  if (!found.length) return "";
  return `代理图按长边 ≤ max_edge 降采样后渲染：${found.join("、")} 在代理分辨率下效果与成品不同`
    + "（像素半径/邻域阈值随分辨率变化），逐像素颜色操作与几何归一化的 vignette 不受影响。";
}

// <<P2_PRESET_RAW:begin>>
const P2_PRESET_RAW = String.raw`{
 "version": 1,
 "source": "《修片风格体系速查与配方》(upload.html) + 2026-10-23 实测批次结论 (chatlog)",
 "presets": [
  {
   "id": "japanese_airy",
   "name_zh": "小清新",
   "name_en": "Japanese Airy",
   "status": "established",
   "tags": [
    "人像",
    "日光",
    "高调"
   ],
   "suitable": [
    "晴日户外人像",
    "海边",
    "白建筑",
    "校园"
   ],
   "caution": [
    "夜景（会变灰）",
    "肤色易被青污染"
   ],
   "description": "高调、青绿环境、透肤、空气感；比韩系更通透、比糖水更克制。",
   "notes": "已验证（补充找回的截图证据，2026-10-23）：以「JP LIGHT FINAL」在三素材上深挖验证——圣托里尼·伊亚小镇、罗弗敦·红房子（雾天变体格标注 JP CLEAN fog adapt）、树荫下人像。此前『蒙灰纱』问题已换轻校正基线。unsupported：局部眼睛锐化需蒙版工具，不在预设内。逐通道 HSL（绿向青/提明）并非引擎缺失（2026-10-25 纠错）：引擎已有 8 段 HSL（hsl_with_ranges，filter.rs:1263），本预设未使用，仍用全局 hsl + selective_color 近似。【全库说明】22 套预设 ops 里 ranges 与 hsl 类型均为零使用；重写 ops 真正用 ranges/hsl 待 v5 批量收尾 + 期 2 落地后另行。截图为 1170px 参考图，非原始渲染。半量烘焙为 strength-1 默认（2026-10-26）：1.0 在新语义下仍过强（风格步 Lstd −32.0%、p2 0.317 超 0.30 警告、1:1 一眼发灰发雾），风格段 9 ops 取半（levels black 0.04→0.02，其余与 v5 半量 ops 相同），修正段不动。",
   "correction": {
    "baseline": "light",
    "ops": [
     {
      "kind": "adjustment",
      "type": "exposure",
      "params": {
       "ev": 0.05
      },
      "neutral": {
       "ev": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "shadows_highlights",
      "params": {
       "shadows": 0.1,
       "highlights": -0.1
      },
      "neutral": {
       "shadows": 0,
       "highlights": 0
      }
     }
    ]
   },
   "style_ops": [
    {
     "kind": "adjustment",
     "type": "exposure",
     "params": {
      "ev": 0.225
     },
     "neutral": {
      "ev": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "brightness_contrast",
     "params": {
      "brightness": 0.0,
      "contrast": 0.94
     },
     "neutral": {
      "brightness": 0,
      "contrast": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "curves",
     "params": {
      "channel": "rgb",
      "points": [
       [
        0.0,
        0.00245
       ],
       [
        0.0052,
        0.010249999999999999
       ],
       [
        0.0143,
        0.02315
       ],
       [
        0.0293,
        0.0421
       ],
       [
        0.0509,
        0.06720000000000001
       ],
       [
        0.0796,
        0.09784999999999999
       ],
       [
        0.116,
        0.13415
       ],
       [
        0.1607,
        0.17715
       ],
       [
        0.214,
        0.22845
       ],
       [
        0.2765,
        0.2898
       ],
       [
        0.3485,
        0.3611
       ],
       [
        0.4304,
        0.4417
       ],
       [
        0.5225,
        0.53035
       ],
       [
        0.6252,
        0.6267499999999999
       ],
       [
        0.7388,
        0.73165
       ],
       [
        0.8637,
        0.8449500000000001
       ],
       [
        1.0,
        0.96655
       ]
      ]
     },
     "neutral": {
      "points": [
       [
        0.0,
        0.0
       ],
       [
        0.0052,
        0.0052
       ],
       [
        0.0143,
        0.0143
       ],
       [
        0.0293,
        0.0293
       ],
       [
        0.0509,
        0.0509
       ],
       [
        0.0796,
        0.0796
       ],
       [
        0.116,
        0.116
       ],
       [
        0.1607,
        0.1607
       ],
       [
        0.214,
        0.214
       ],
       [
        0.2765,
        0.2765
       ],
       [
        0.3485,
        0.3485
       ],
       [
        0.4304,
        0.4304
       ],
       [
        0.5225,
        0.5225
       ],
       [
        0.6252,
        0.6252
       ],
       [
        0.7388,
        0.7388
       ],
       [
        0.8637,
        0.8637
       ],
       [
        1.0,
        1.0
       ]
      ]
     }
    },
    {
     "kind": "adjustment",
     "type": "saturation",
     "params": {
      "amount": 0.96
     },
     "neutral": {
      "amount": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "vibrance",
     "params": {
      "amount": 0.175
     },
     "neutral": {
      "amount": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "white_balance",
     "params": {
      "temperature": 0.03,
      "tint": 0.02
     },
     "neutral": {
      "temperature": 0,
      "tint": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "shadows_highlights",
     "params": {
      "shadows": 0.175,
      "highlights": -0.15
     },
     "neutral": {
      "shadows": 0,
      "highlights": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "levels",
     "params": {
      "black": 0.02,
      "white": 1.0,
      "gamma": 1.01
     },
     "neutral": {
      "black": 0,
      "white": 1,
      "gamma": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "texture",
     "params": {
      "amount": -0.03
     },
     "neutral": {
      "amount": 0
     }
    }
   ],
   "knobs": [
    {
     "id": "fade",
     "label": "空气感",
     "description": "曲线首点抬黑点量（curves.points 首点输出），越大越柔越雾；注意 levels.black 是输入黑点、方向相反，不用它做抬升",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 2,
       "param": "points",
       "range": [
        [
         [
          0.0,
          0.0
         ],
         [
          0.0052,
          0.0064
         ],
         [
          0.0143,
          0.0185
         ],
         [
          0.0293,
          0.0376
         ],
         [
          0.0509,
          0.0637
         ],
         [
          0.0796,
          0.0961
         ],
         [
          0.116,
          0.1345
         ],
         [
          0.1607,
          0.1796
         ],
         [
          0.214,
          0.233
         ],
         [
          0.2765,
          0.2963
         ],
         [
          0.3485,
          0.3689
         ],
         [
          0.4304,
          0.4501
         ],
         [
          0.5225,
          0.5382
         ],
         [
          0.6252,
          0.6327
         ],
         [
          0.7388,
          0.734
         ],
         [
          0.8637,
          0.8416
         ],
         [
          1.0,
          0.9551
         ]
        ],
        [
         [
          0.0,
          0.0134
         ],
         [
          0.0052,
          0.0285
         ],
         [
          0.0143,
          0.0495
         ],
         [
          0.0293,
          0.0757
         ],
         [
          0.0509,
          0.1065
         ],
         [
          0.0796,
          0.1399
         ],
         [
          0.116,
          0.1756
         ],
         [
          0.1607,
          0.2155
         ],
         [
          0.214,
          0.2633
         ],
         [
          0.2765,
          0.3224
         ],
         [
          0.3485,
          0.3923
         ],
         [
          0.4304,
          0.4707
         ],
         [
          0.5225,
          0.5542
         ],
         [
          0.6252,
          0.6414
         ],
         [
          0.7388,
          0.734
         ],
         [
          0.8637,
          0.8313
         ],
         [
          1.0,
          0.9331
         ]
        ]
       ]
      }
     ]
    },
    {
     "id": "softness",
     "label": "柔和度",
     "description": "texture 负值量，越大越柔",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 8,
       "param": "amount",
       "range": [
        -0.25,
        0.0
       ]
      }
     ]
    }
   ]
  },
  {
   "id": "ccd",
   "name_zh": "模拟 CCD",
   "name_en": "CCD Simulation",
   "status": "established",
   "tags": [
    "夜景",
    "抓拍",
    "直闪"
   ],
   "suitable": [
    "直闪夜拍",
    "日常抓拍",
    "朋友聚会"
   ],
   "caution": [
    "大光比风光（宽容度对不上）"
   ],
   "description": "低宽容、硬切高光、冷闪偏色、数字噪点；模拟 2000 年代 CCD 卡片机成像链而非只加颗粒。",
   "notes": "已验证（补充找回的截图证据，2026-10-23）：以「CCD FINAL」在三素材上深挖验证——圣托里尼·伊亚小镇、罗弗敦·红房子、树荫下人像，与小清新同为当时深挖的两套。unsupported：直闪光效（拍摄端决定项）与真实色差/紫边（属导入期 raw_options）、PS 高光 glow 复制层法；此处以 noise 数字噪点 + 硬 S 曲线 + 冷偏近似。",
   "correction": {
    "baseline": "full",
    "ops": [
     {
      "kind": "adjustment",
      "type": "exposure",
      "params": {
       "ev": 0.1
      },
      "neutral": {
       "ev": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "shadows_highlights",
      "params": {
       "shadows": 0.2,
       "highlights": -0.25
      },
      "neutral": {
       "shadows": 0,
       "highlights": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "dehaze",
      "params": {
       "amount": 0.05
      },
      "neutral": {
       "amount": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "clarity",
      "params": {
       "amount": 0.05
      },
      "neutral": {
       "amount": 0
      }
     }
    ]
   },
   "style_ops": [
    {
     "kind": "adjustment",
     "type": "brightness_contrast",
     "params": {
      "brightness": 0.0,
      "contrast": 1.22
     },
     "neutral": {
      "brightness": 0,
      "contrast": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "curves",
     "params": {
      "channel": "rgb",
      "points": [
       [
        0,
        0
       ],
       [
        0.0509,
        0.0301
       ],
       [
        0.214,
        0.214
       ],
       [
        0.5225,
        0.6209
       ],
       [
        1,
        1
       ]
      ]
     },
     "neutral": {
      "points": [
       [
        0,
        0
       ],
       [
        0.0509,
        0.0509
       ],
       [
        0.214,
        0.214
       ],
       [
        0.5225,
        0.5225
       ],
       [
        1,
        1
       ]
      ]
     }
    },
    {
     "kind": "adjustment",
     "type": "white_balance",
     "params": {
      "temperature": -0.12,
      "tint": -0.06
     },
     "neutral": {
      "temperature": 0,
      "tint": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "saturation",
     "params": {
      "amount": 1.15
     },
     "neutral": {
      "amount": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "clarity",
     "params": {
      "amount": -0.15
     },
     "neutral": {
      "amount": 0
     }
    },
    {
     "kind": "filter",
     "type": "noise",
     "params": {
      "amount": 0.16,
      "seed": 42
     },
     "neutral": {
      "amount": 0
     }
    },
    {
     "kind": "filter",
     "type": "vignette",
     "params": {
      "strength": 0.16,
      "radius": 0.75,
      "softness": 0.8
     },
     "neutral": {
      "strength": 0
     }
    }
   ],
   "knobs": [
    {
     "id": "noise",
     "label": "数字噪点",
     "description": "noise.amount，CCD 彩噪/亮度噪的量",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 5,
       "param": "amount",
       "range": [
        0.0,
        0.35
       ]
      }
     ]
    },
    {
     "id": "cold",
     "label": "冷闪偏色",
     "description": "white_balance.temperature，越小越冷青",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 2,
       "param": "temperature",
       "range": [
        -0.3,
        0.05
       ]
      }
     ]
    }
   ]
  },
  {
   "id": "japanese_film",
   "name_zh": "日系胶片",
   "name_en": "Japanese Film",
   "status": "established",
   "tags": [
    "人像",
    "胶片",
    "阴天"
   ],
   "suitable": [
    "阴天",
    "窗光",
    "制服/校园题材"
   ],
   "caution": [
    "强顺光金色时刻（绿偏变脏）"
   ],
   "description": "清、淡、略青绿、皮肤偏白皙，Fuji 400H 系。",
   "notes": "未在实测批次中，未目检。unsupported：颗粒的粗糙度参数 Yanshi film_grain 只有 amount/size/seed。期 5 目检成立：真照片三格行（d5600_night 夜街）已验收。",
   "correction": {
    "baseline": "full",
    "ops": [
     {
      "kind": "adjustment",
      "type": "exposure",
      "params": {
       "ev": 0.1
      },
      "neutral": {
       "ev": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "shadows_highlights",
      "params": {
       "shadows": 0.2,
       "highlights": -0.25
      },
      "neutral": {
       "shadows": 0,
       "highlights": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "dehaze",
      "params": {
       "amount": 0.05
      },
      "neutral": {
       "amount": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "clarity",
      "params": {
       "amount": 0.05
      },
      "neutral": {
       "amount": 0
      }
     }
    ]
   },
   "style_ops": [
    {
     "kind": "adjustment",
     "type": "exposure",
     "params": {
      "ev": 0.35
     },
     "neutral": {
      "ev": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "brightness_contrast",
     "params": {
      "brightness": 0.02,
      "contrast": 0.88
     },
     "neutral": {
      "brightness": 0,
      "contrast": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "curves",
     "params": {
      "channel": "rgb",
      "points": [
       [
        0,
        0.0039
       ],
       [
        0.0509,
        0.0732
       ],
       [
        0.214,
        0.233
       ],
       [
        0.5225,
        0.5225
       ],
       [
        1,
        0.9114
       ]
      ]
     },
     "neutral": {
      "points": [
       [
        0,
        0
       ],
       [
        0.0509,
        0.0509
       ],
       [
        0.214,
        0.214
       ],
       [
        0.5225,
        0.5225
       ],
       [
        1,
        1
       ]
      ]
     }
    },
    {
     "kind": "adjustment",
     "type": "saturation",
     "params": {
      "amount": 0.9
     },
     "neutral": {
      "amount": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "white_balance",
     "params": {
      "temperature": -0.05,
      "tint": -0.04
     },
     "neutral": {
      "temperature": 0,
      "tint": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "split_toning",
     "params": {
      "shadows": [
       -0.03,
       0.05,
       0.03
      ],
      "highlights": [
       0.0,
       0.0,
       0.0
      ],
      "balance": 0.0,
      "amount": 0.35
     },
     "neutral": {
      "shadows": [
       0,
       0,
       0
      ],
      "highlights": [
       0,
       0,
       0
      ],
      "balance": 0
     }
    },
    {
     "kind": "filter",
     "type": "film_grain",
     "params": {
      "amount": 0.22,
      "size": 3,
      "seed": 7
     },
     "neutral": {
      "amount": 0
     }
    }
   ],
   "knobs": [
    {
     "id": "fade",
     "label": "褪色",
     "description": "曲线由 strength 主导；此旋钮改 split_toning 阴影青绿量",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 5,
       "param": "shadows",
       "range": [
        [
         -0.08,
         0.0,
         0.0
        ],
        [
         0.02,
         0.1,
         0.08
        ]
       ]
      }
     ]
    },
    {
     "id": "grain",
     "label": "颗粒",
     "description": "film_grain.amount",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 6,
       "param": "amount",
       "range": [
        0.0,
        0.4
       ]
      }
     ]
    }
   ]
  },
  {
   "id": "retro_film",
   "name_zh": "胶片复古",
   "name_en": "Retro Film",
   "status": "fixed_v2",
   "tags": [
    "人像",
    "胶片",
    "金色时刻"
   ],
   "suitable": [
    "婚礼",
    "金色时刻人像",
    "下坡路/街道日景"
   ],
   "caution": [
    "神社夜景（天空仍偏棕橙，v2 未完全修透）",
    "室内钨丝灯未校白平衡时暖上加暖"
   ],
   "description": "暖金、奶油肤、怀旧但不脏，Kodak Portra 系；v2 暖度减半。",
   "notes": "v2：白天定版；夜景对暖偏置格外敏感，天空仍棕橙，未完全修透，已如实标注。曝光键为 ev（曾写错键，已修正）。",
   "correction": {
    "baseline": "full",
    "ops": [
     {
      "kind": "adjustment",
      "type": "exposure",
      "params": {
       "ev": 0.1
      },
      "neutral": {
       "ev": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "shadows_highlights",
      "params": {
       "shadows": 0.2,
       "highlights": -0.25
      },
      "neutral": {
       "shadows": 0,
       "highlights": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "dehaze",
      "params": {
       "amount": 0.05
      },
      "neutral": {
       "amount": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "clarity",
      "params": {
       "amount": 0.05
      },
      "neutral": {
       "amount": 0
      }
     }
    ]
   },
   "style_ops": [
    {
     "kind": "adjustment",
     "type": "exposure",
     "params": {
      "ev": 0.1
     },
     "neutral": {
      "ev": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "brightness_contrast",
     "params": {
      "brightness": 0.0,
      "contrast": 0.88
     },
     "neutral": {
      "brightness": 0,
      "contrast": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "curves",
     "params": {
      "channel": "rgb",
      "points": [
       [
        0,
        0.003
       ],
       [
        0.0509,
        0.0621
       ],
       [
        0.214,
        0.2235
       ],
       [
        0.5225,
        0.5225
       ],
       [
        1,
        0.9557
       ]
      ]
     },
     "neutral": {
      "points": [
       [
        0,
        0
       ],
       [
        0.0509,
        0.0509
       ],
       [
        0.214,
        0.214
       ],
       [
        0.5225,
        0.5225
       ],
       [
        1,
        1
       ]
      ]
     }
    },
    {
     "kind": "adjustment",
     "type": "saturation",
     "params": {
      "amount": 0.94
     },
     "neutral": {
      "amount": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "white_balance",
     "params": {
      "temperature": 0.1,
      "tint": 0.02
     },
     "neutral": {
      "temperature": 0,
      "tint": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "split_toning",
     "params": {
      "shadows": [
       0.07,
       0.03,
       -0.03
      ],
      "highlights": [
       0.05,
       0.02,
       -0.02
      ],
      "balance": 0.05,
      "amount": 0.1
     },
     "neutral": {
      "shadows": [
       0,
       0,
       0
      ],
      "highlights": [
       0,
       0,
       0
      ],
      "balance": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "shadows_highlights",
     "params": {
      "shadows": 0.05,
      "highlights": -0.2
     },
     "neutral": {
      "shadows": 0,
      "highlights": 0
     }
    },
    {
     "kind": "filter",
     "type": "film_grain",
     "params": {
      "amount": 0.3,
      "size": 3,
      "seed": 11
     },
     "neutral": {
      "amount": 0
     }
    },
    {
     "kind": "filter",
     "type": "vignette",
     "params": {
      "strength": 0.11,
      "radius": 0.8,
      "softness": 0.9
     },
     "neutral": {
      "strength": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "levels",
     "params": {
      "black": 0.02,
      "white": 1.0,
      "gamma": 1.0
     },
     "neutral": {
      "black": 0,
      "white": 1,
      "gamma": 1
     }
    }
   ],
   "knobs": [
    {
     "id": "warmth",
     "label": "暖度",
     "description": "white_balance.temperature，夜景慎用大值",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 4,
       "param": "temperature",
       "range": [
        -0.05,
        0.22
       ]
      }
     ]
    },
    {
     "id": "grain",
     "label": "颗粒",
     "description": "film_grain.amount",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 7,
       "param": "amount",
       "range": [
        0.0,
        0.4
       ]
      }
     ]
    }
   ]
  },
  {
   "id": "hong_kong",
   "name_zh": "港风",
   "name_en": "Hong Kong",
   "status": "fixed_v2",
   "tags": [
    "夜景",
    "街景",
    "霓虹"
   ],
   "suitable": [
    "夜街",
    "招牌",
    "茶餐厅",
    "地铁"
   ],
   "caution": [
    "白天须降强度",
    "肤色发绿是头号失败模式"
   ],
   "description": "浓、潮、红绿霓虹对撞、潮湿夜街，王家卫式；v2 全局色偏减半并保红。",
   "notes": "v1 与国风同病：全局色偏开满。v2 已减半：split_toning 阴影青绿与全局饱和都按 v1 的约一半落值，红靠 selective_color 的 reds 保护。",
   "correction": {
    "baseline": "full",
    "ops": [
     {
      "kind": "adjustment",
      "type": "exposure",
      "params": {
       "ev": 0.1
      },
      "neutral": {
       "ev": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "shadows_highlights",
      "params": {
       "shadows": 0.2,
       "highlights": -0.25
      },
      "neutral": {
       "shadows": 0,
       "highlights": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "dehaze",
      "params": {
       "amount": 0.05
      },
      "neutral": {
       "amount": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "clarity",
      "params": {
       "amount": 0.05
      },
      "neutral": {
       "amount": 0
      }
     }
    ]
   },
   "style_ops": [
    {
     "kind": "adjustment",
     "type": "brightness_contrast",
     "params": {
      "brightness": 0.0,
      "contrast": 1.24
     },
     "neutral": {
      "brightness": 0,
      "contrast": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "saturation",
     "params": {
      "amount": 1.22
     },
     "neutral": {
      "amount": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "split_toning",
     "params": {
      "shadows": [
       -0.1,
       0.08,
       0.1
      ],
      "highlights": [
       0.1,
       0.04,
       -0.05
      ],
      "balance": 0.0,
      "amount": 0.16
     },
     "neutral": {
      "shadows": [
       0,
       0,
       0
      ],
      "highlights": [
       0,
       0,
       0
      ],
      "balance": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "selective_color",
     "params": {
      "adjust": {
       "reds": [
        -0.222,
        0.0,
        0.111,
        0.0
       ],
       "greens": [
        0.148,
        0.0,
        -0.148,
        0.037
       ]
      }
     },
     "neutral": {
      "adjust": {
       "reds": [
        0,
        0,
        0,
        0
       ],
       "greens": [
        0,
        0,
        0,
        0
       ]
      }
     }
    },
    {
     "kind": "adjustment",
     "type": "white_balance",
     "params": {
      "temperature": -0.08,
      "tint": 0.0
     },
     "neutral": {
      "temperature": 0,
      "tint": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "clarity",
     "params": {
      "amount": 0.2
     },
     "neutral": {
      "amount": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "shadows_highlights",
     "params": {
      "shadows": 0.05,
      "highlights": -0.35
     },
     "neutral": {
      "shadows": 0,
      "highlights": 0
     }
    },
    {
     "kind": "filter",
     "type": "film_grain",
     "params": {
      "amount": 0.2,
      "size": 2,
      "seed": 5
     },
     "neutral": {
      "amount": 0
     }
    },
    {
     "kind": "filter",
     "type": "vignette",
     "params": {
      "strength": 0.22,
      "radius": 0.75,
      "softness": 0.8
     },
     "neutral": {
      "strength": 0
     }
    }
   ],
   "knobs": [
    {
     "id": "neon",
     "label": "霓虹浓艳",
     "description": "saturation.amount，v2 基准 1.18",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 1,
       "param": "amount",
       "range": [
        1.0,
        1.45
       ]
      }
     ]
    },
    {
     "id": "teal",
     "label": "青绿阴影",
     "description": "split_toning.shadows 的青绿量",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 2,
       "param": "shadows",
       "range": [
        [
         -0.04,
         0.03,
         0.04
        ],
        [
         -0.18,
         0.14,
         0.18
        ]
       ]
      }
     ]
    }
   ]
  },
  {
   "id": "korean",
   "name_zh": "韩系",
   "name_en": "Korean Creamy",
   "status": "established",
   "tags": [
    "人像",
    "高调",
    "奶油"
   ],
   "suitable": [
    "棚拍",
    "探店",
    "情侣/闺蜜人像"
   ],
   "caution": [
    "硬光正午（糊成一片）",
    "男性/街头题材显假"
   ],
   "description": "奶油、粉彩、干净、白皙；柔焦比小清新敢下手。",
   "notes": "实测批次蒙灰纱，换轻校正基线重渲 2 张，待最终目检。期 5 目检成立：真照片三格行（cl_chateau 城堡）已验收。",
   "correction": {
    "baseline": "light",
    "ops": [
     {
      "kind": "adjustment",
      "type": "exposure",
      "params": {
       "ev": 0.05
      },
      "neutral": {
       "ev": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "shadows_highlights",
      "params": {
       "shadows": 0.1,
       "highlights": -0.1
      },
      "neutral": {
       "shadows": 0,
       "highlights": 0
      }
     }
    ]
   },
   "style_ops": [
    {
     "kind": "adjustment",
     "type": "exposure",
     "params": {
      "ev": 0.15
     },
     "neutral": {
      "ev": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "brightness_contrast",
     "params": {
      "brightness": 0.05,
      "contrast": 0.6
     },
     "neutral": {
      "brightness": 0,
      "contrast": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "levels",
     "params": {
      "black": 0.1,
      "white": 1.0,
      "gamma": 1.05
     },
     "neutral": {
      "black": 0,
      "white": 1,
      "gamma": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "shadows_highlights",
     "params": {
      "shadows": 0.45,
      "highlights": -0.45
     },
     "neutral": {
      "shadows": 0,
      "highlights": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "vibrance",
     "params": {
      "amount": 0.2
     },
     "neutral": {
      "amount": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "saturation",
     "params": {
      "amount": 1.02
     },
     "neutral": {
      "amount": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "texture",
     "params": {
      "amount": -0.14
     },
     "neutral": {
      "amount": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "clarity",
     "params": {
      "amount": -0.12
     },
     "neutral": {
      "amount": 0
     }
    }
   ],
   "knobs": [
    {
     "id": "cream",
     "label": "奶油柔焦",
     "description": "texture.amount 负值量",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 6,
       "param": "amount",
       "range": [
        -0.3,
        0.0
       ]
      }
     ]
    },
    {
     "id": "bright",
     "label": "明亮",
     "description": "exposure.ev",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 0,
       "param": "ev",
       "range": [
        -0.1,
        0.4
       ]
      }
     ]
    }
   ]
  },
  {
   "id": "western_cool",
   "name_zh": "欧美清冷",
   "name_en": "Western Cool",
   "status": "established",
   "tags": [
    "风光",
    "建筑",
    "人像"
   ],
   "suitable": [
    "建筑",
    "汽车",
    "健身",
    "男性人像",
    "雪景",
    "港口"
   ],
   "caution": [
    "婴幼儿/婚礼柔情题材显冷酷",
    "肤色变青先查黄色压过头"
   ],
   "description": "冷峻、硬朗、蓝灰、杂志感；研究名「欧美冷调」，实测批次名「欧美清冷」，同一套。",
   "notes": "实测本批最好：日落暖天配冷蓝阴影对撞干净；汉堡港微微去饱和的蓝灰，高级感成立。",
   "correction": {
    "baseline": "full",
    "ops": [
     {
      "kind": "adjustment",
      "type": "exposure",
      "params": {
       "ev": 0.1
      },
      "neutral": {
       "ev": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "shadows_highlights",
      "params": {
       "shadows": 0.2,
       "highlights": -0.25
      },
      "neutral": {
       "shadows": 0,
       "highlights": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "dehaze",
      "params": {
       "amount": 0.05
      },
      "neutral": {
       "amount": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "clarity",
      "params": {
       "amount": 0.05
      },
      "neutral": {
       "amount": 0
      }
     }
    ]
   },
   "style_ops": [
    {
     "kind": "adjustment",
     "type": "brightness_contrast",
     "params": {
      "brightness": 0.0,
      "contrast": 1.2
     },
     "neutral": {
      "brightness": 0,
      "contrast": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "white_balance",
     "params": {
      "temperature": -0.14,
      "tint": 0.0
     },
     "neutral": {
      "temperature": 0,
      "tint": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "split_toning",
     "params": {
      "shadows": [
       -0.08,
       0.02,
       0.14
      ],
      "highlights": [
       0.0,
       0.0,
       0.0
      ],
      "balance": -0.1,
      "amount": 0.35
     },
     "neutral": {
      "shadows": [
       0,
       0,
       0
      ],
      "highlights": [
       0,
       0,
       0
      ],
      "balance": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "saturation",
     "params": {
      "amount": 0.92
     },
     "neutral": {
      "amount": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "selective_color",
     "params": {
      "adjust": {
       "yellows": [
        0.0,
        0.0,
        -0.555,
        0.0
       ],
       "blues": [
        0.0,
        0.0,
        0.278,
        0.0
       ]
      }
     },
     "neutral": {
      "adjust": {
       "yellows": [
        0,
        0,
        0,
        0
       ],
       "blues": [
        0,
        0,
        0,
        0
       ]
      }
     }
    },
    {
     "kind": "adjustment",
     "type": "texture",
     "params": {
      "amount": 0.2
     },
     "neutral": {
      "amount": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "clarity",
     "params": {
      "amount": 0.2
     },
     "neutral": {
      "amount": 0
     }
    },
    {
     "kind": "filter",
     "type": "sharpen",
     "params": {
      "amount": 0.7,
      "radius": 2
     },
     "neutral": {
      "amount": 0
     }
    }
   ],
   "knobs": [
    {
     "id": "cold",
     "label": "冷感",
     "description": "white_balance.temperature，越小越冷",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 1,
       "param": "temperature",
       "range": [
        -0.3,
        0.05
       ]
      }
     ]
    },
    {
     "id": "hard",
     "label": "硬朗",
     "description": "clarity.amount",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 6,
       "param": "amount",
       "range": [
        0.0,
        0.4
       ]
      }
     ]
    }
   ]
  },
  {
   "id": "cinema",
   "name_zh": "电影感",
   "name_en": "Cinematic Teal-Orange",
   "status": "fixed_v2",
   "tags": [
    "夜景",
    "街景",
    "叙事"
   ],
   "suitable": [
    "旅行",
    "街头",
    "叙事人像",
    "夜景灯火"
   ],
   "caution": [
    "原片无橙色主体也无青色环境时硬套会假",
    "分级饱和过头卡通化"
   ],
   "description": "青影、橙肤、哑光、叙事；v2 青只压阴影、橙留亮部，不再全局泼色。",
   "notes": "v1 两张全泡进青色（威尼斯暖灯火被灭、罗弗敦海面成青汤）。v2 只用 split_toning 分段染色，全局饱和不再推高。",
   "correction": {
    "baseline": "full",
    "ops": [
     {
      "kind": "adjustment",
      "type": "exposure",
      "params": {
       "ev": 0.1
      },
      "neutral": {
       "ev": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "shadows_highlights",
      "params": {
       "shadows": 0.2,
       "highlights": -0.25
      },
      "neutral": {
       "shadows": 0,
       "highlights": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "dehaze",
      "params": {
       "amount": 0.05
      },
      "neutral": {
       "amount": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "clarity",
      "params": {
       "amount": 0.05
      },
      "neutral": {
       "amount": 0
      }
     }
    ]
   },
   "style_ops": [
    {
     "kind": "adjustment",
     "type": "brightness_contrast",
     "params": {
      "brightness": 0.0,
      "contrast": 1.26
     },
     "neutral": {
      "brightness": 0,
      "contrast": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "curves",
     "params": {
      "channel": "rgb",
      "points": [
       [
        0,
        0.0039
       ],
       [
        0.0509,
        0.047
       ],
       [
        0.214,
        0.214
       ],
       [
        0.5225,
        0.5542
       ],
       [
        1,
        0.9551
       ]
      ]
     },
     "neutral": {
      "points": [
       [
        0,
        0
       ],
       [
        0.0509,
        0.0509
       ],
       [
        0.214,
        0.214
       ],
       [
        0.5225,
        0.5225
       ],
       [
        1,
        1
       ]
      ]
     }
    },
    {
     "kind": "adjustment",
     "type": "split_toning",
     "params": {
      "shadows": [
       -0.12,
       0.03,
       0.14
      ],
      "highlights": [
       0.16,
       0.06,
       -0.1
      ],
      "balance": 0.1,
      "amount": 0.13
     },
     "neutral": {
      "shadows": [
       0,
       0,
       0
      ],
      "highlights": [
       0,
       0,
       0
      ],
      "balance": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "saturation",
     "params": {
      "amount": 1.08
     },
     "neutral": {
      "amount": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "vibrance",
     "params": {
      "amount": 0.25
     },
     "neutral": {
      "amount": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "levels",
     "params": {
      "black": 0.02,
      "white": 1.0,
      "gamma": 1.0
     },
     "neutral": {
      "black": 0,
      "white": 1,
      "gamma": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "clarity",
     "params": {
      "amount": 0.12
     },
     "neutral": {
      "amount": 0
     }
    },
    {
     "kind": "filter",
     "type": "film_grain",
     "params": {
      "amount": 0.12,
      "size": 2,
      "seed": 3
     },
     "neutral": {
      "amount": 0
     }
    },
    {
     "kind": "filter",
     "type": "vignette",
     "params": {
      "strength": 0.22,
      "radius": 0.8,
      "softness": 0.8
     },
     "neutral": {
      "strength": 0
     }
    }
   ],
   "knobs": [
    {
     "id": "teal_orange",
     "label": "青橙对峙",
     "description": "split_toning.shadows 青量（高光橙同向由配方固定比）",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 2,
       "param": "shadows",
       "range": [
        [
         -0.05,
         0.01,
         0.06
        ],
        [
         -0.2,
         0.05,
         0.22
        ]
       ]
      }
     ]
    },
    {
     "id": "matte",
     "label": "哑光",
     "description": "曲线首点抬黑点量（curves.points 首点输出），越大黑位越抬、越哑光；levels.black 是输入黑点、方向相反，不用它做抬升",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 1,
       "param": "points",
       "range": [
        [
         [
          0,
          0
         ],
         [
          0.0509,
          0.0451
         ],
         [
          0.214,
          0.214
         ],
         [
          0.5225,
          0.5462
         ],
         [
          1,
          0.9662
         ]
        ],
        [
         [
          0,
          0.01
         ],
         [
          0.0509,
          0.0593
         ],
         [
          0.214,
          0.2234
         ],
         [
          0.5225,
          0.5623
         ],
         [
          1,
          0.9441
         ]
        ]
       ]
      }
     ]
    }
   ]
  },
  {
   "id": "french",
   "name_zh": "法式",
   "name_en": "French Warm",
   "status": "established",
   "tags": [
    "人像",
    "窗光",
    "田园"
   ],
   "suitable": [
    "田园",
    "家居",
    "窗光人像",
    "婚礼细节"
   ],
   "caution": [
    "霓虹/都市夜景（冷光被暖脏）",
    "色温与分级双暖会把白墙变黄墙"
   ],
   "description": "慵懒、琥珀、奶油、橄榄绿、午后窗光。",
   "notes": "实测批次换轻校正基线重渲 2 张，待最终目检。期 5 目检成立：真照片三格行（cl_chateau 城堡）已验收。",
   "correction": {
    "baseline": "light",
    "ops": [
     {
      "kind": "adjustment",
      "type": "exposure",
      "params": {
       "ev": 0.05
      },
      "neutral": {
       "ev": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "shadows_highlights",
      "params": {
       "shadows": 0.1,
       "highlights": -0.1
      },
      "neutral": {
       "shadows": 0,
       "highlights": 0
      }
     }
    ]
   },
   "style_ops": [
    {
     "kind": "adjustment",
     "type": "exposure",
     "params": {
      "ev": 0.28
     },
     "neutral": {
      "ev": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "brightness_contrast",
     "params": {
      "brightness": 0.02,
      "contrast": 0.9
     },
     "neutral": {
      "brightness": 0,
      "contrast": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "curves",
     "params": {
      "channel": "rgb",
      "points": [
       [
        0,
        0.0031
       ],
       [
        0.0509,
        0.0684
       ],
       [
        0.214,
        0.233
       ],
       [
        0.5225,
        0.5382
       ],
       [
        1,
        0.9114
       ]
      ]
     },
     "neutral": {
      "points": [
       [
        0,
        0
       ],
       [
        0.0509,
        0.0509
       ],
       [
        0.214,
        0.214
       ],
       [
        0.5225,
        0.5225
       ],
       [
        1,
        1
       ]
      ]
     }
    },
    {
     "kind": "adjustment",
     "type": "white_balance",
     "params": {
      "temperature": 0.14,
      "tint": 0.02
     },
     "neutral": {
      "temperature": 0,
      "tint": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "split_toning",
     "params": {
      "shadows": [
       0.05,
       0.03,
       -0.02
      ],
      "highlights": [
       0.07,
       0.04,
       -0.03
      ],
      "balance": 0.05,
      "amount": 0.35
     },
     "neutral": {
      "shadows": [
       0,
       0,
       0
      ],
      "highlights": [
       0,
       0,
       0
      ],
      "balance": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "saturation",
     "params": {
      "amount": 0.94
     },
     "neutral": {
      "amount": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "clarity",
     "params": {
      "amount": -0.06
     },
     "neutral": {
      "amount": 0
     }
    },
    {
     "kind": "filter",
     "type": "film_grain",
     "params": {
      "amount": 0.14,
      "size": 2,
      "seed": 13
     },
     "neutral": {
      "amount": 0
     }
    }
   ],
   "knobs": [
    {
     "id": "warmth",
     "label": "琥珀暖",
     "description": "white_balance.temperature",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 3,
       "param": "temperature",
       "range": [
        0.0,
        0.28
       ]
      }
     ]
    },
    {
     "id": "grain",
     "label": "颗粒",
     "description": "film_grain.amount",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 7,
       "param": "amount",
       "range": [
        0.0,
        0.3
       ]
      }
     ]
    }
   ]
  },
  {
   "id": "forest",
   "name_zh": "森系",
   "name_en": "Forest",
   "status": "established",
   "tags": [
    "风光",
    "森林",
    "雾气"
   ],
   "suitable": [
    "森林",
    "溪流",
    "雾日",
    "森系人像"
   ],
   "caution": [
    "城市/建筑显脏绿",
    "林下人像肤色易被绿污染"
   ],
   "description": "墨绿、幽深、雾气、低饱和的绿海；绿要深不要亮。",
   "notes": "实测：台湾苔藓森林合格（绿更润更深、苔藓与树根质感都在、无伪影）。雾林素材已剔除（九成白雾），仅作解码验证。",
   "correction": {
    "baseline": "full",
    "ops": [
     {
      "kind": "adjustment",
      "type": "exposure",
      "params": {
       "ev": 0.1
      },
      "neutral": {
       "ev": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "shadows_highlights",
      "params": {
       "shadows": 0.2,
       "highlights": -0.25
      },
      "neutral": {
       "shadows": 0,
       "highlights": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "dehaze",
      "params": {
       "amount": 0.05
      },
      "neutral": {
       "amount": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "clarity",
      "params": {
       "amount": 0.05
      },
      "neutral": {
       "amount": 0
      }
     }
    ]
   },
   "style_ops": [
    {
     "kind": "adjustment",
     "type": "exposure",
     "params": {
      "ev": -0.3
     },
     "neutral": {
      "ev": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "brightness_contrast",
     "params": {
      "brightness": 0.0,
      "contrast": 1.12
     },
     "neutral": {
      "brightness": 0,
      "contrast": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "shadows_highlights",
     "params": {
      "shadows": 0.2,
      "highlights": -0.5
     },
     "neutral": {
      "shadows": 0,
      "highlights": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "saturation",
     "params": {
      "amount": 0.9
     },
     "neutral": {
      "amount": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "selective_color",
     "params": {
      "adjust": {
       "greens": [
        0.22,
        0.0,
        -0.38,
        0.12
       ],
       "yellows": [
        0.18,
        0.0,
        -0.45,
        0.08
       ]
      }
     },
     "neutral": {
      "adjust": {
       "greens": [
        0,
        0,
        0,
        0
       ],
       "yellows": [
        0,
        0,
        0,
        0
       ]
      }
     }
    },
    {
     "kind": "adjustment",
     "type": "split_toning",
     "params": {
      "shadows": [
       -0.06,
       0.06,
       0.05
      ],
      "highlights": [
       0.0,
       0.0,
       0.0
      ],
      "balance": 0.0,
      "amount": 0.12
     },
     "neutral": {
      "shadows": [
       0,
       0,
       0
      ],
      "highlights": [
       0,
       0,
       0
      ],
      "balance": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "dehaze",
     "params": {
      "amount": -0.03
     },
     "neutral": {
      "amount": 0
     }
    }
   ],
   "knobs": [
    {
     "id": "depth",
     "label": "幽深",
     "description": "exposure.ev，越小越深",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 0,
       "param": "ev",
       "range": [
        -0.7,
        -0.1
       ]
      }
     ]
    },
    {
     "id": "mist",
     "label": "雾气",
     "description": "dehaze.amount 负值留雾",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 6,
       "param": "amount",
       "range": [
        -0.25,
        0.1
       ]
      }
     ]
    }
   ]
  },
  {
   "id": "morandi",
   "name_zh": "莫兰迪",
   "name_en": "Morandi",
   "status": "established",
   "tags": [
    "静物",
    "室内",
    "产品"
   ],
   "suitable": [
    "静物",
    "极简室内",
    "服装平铺",
    "高级感产品",
    "港口建筑"
   ],
   "caution": [
    "运动/广告/夜景（没精神）",
    "降饱和过头肤色变灰"
   ],
   "description": "灰、粉、雾蓝、燕麦——所有颜色一起降饱和、明度向中高靠拢的粉彩。",
   "notes": "实测：汉堡港是教科书式莫兰迪（灰米色、低饱和、哑光）。雾林素材不参与评测。",
   "correction": {
    "baseline": "full",
    "ops": [
     {
      "kind": "adjustment",
      "type": "exposure",
      "params": {
       "ev": 0.1
      },
      "neutral": {
       "ev": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "shadows_highlights",
      "params": {
       "shadows": 0.2,
       "highlights": -0.25
      },
      "neutral": {
       "shadows": 0,
       "highlights": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "dehaze",
      "params": {
       "amount": 0.05
      },
      "neutral": {
       "amount": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "clarity",
      "params": {
       "amount": 0.05
      },
      "neutral": {
       "amount": 0
      }
     }
    ]
   },
   "style_ops": [
    {
     "kind": "adjustment",
     "type": "exposure",
     "params": {
      "ev": 0.2
     },
     "neutral": {
      "ev": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "brightness_contrast",
     "params": {
      "brightness": 0.03,
      "contrast": 0.75
     },
     "neutral": {
      "brightness": 0,
      "contrast": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "curves",
     "params": {
      "channel": "rgb",
      "points": [
       [
        0,
        0.006
       ],
       [
        0.0509,
        0.0783
       ],
       [
        0.214,
        0.2429
       ],
       [
        0.5225,
        0.5225
       ],
       [
        1,
        0.8689
       ]
      ]
     },
     "neutral": {
      "points": [
       [
        0,
        0
       ],
       [
        0.0509,
        0.0509
       ],
       [
        0.214,
        0.214
       ],
       [
        0.5225,
        0.5225
       ],
       [
        1,
        1
       ]
      ]
     }
    },
    {
     "kind": "adjustment",
     "type": "saturation",
     "params": {
      "amount": 0.72
     },
     "neutral": {
      "amount": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "vibrance",
     "params": {
      "amount": 0.0
     },
     "neutral": {
      "amount": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "clarity",
     "params": {
      "amount": -0.1
     },
     "neutral": {
      "amount": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "split_toning",
     "params": {
      "shadows": [
       -0.02,
       0.0,
       0.03
      ],
      "highlights": [
       0.03,
       0.01,
       0.0
      ],
      "balance": 0.0,
      "amount": 0.35
     },
     "neutral": {
      "shadows": [
       0,
       0,
       0
      ],
      "highlights": [
       0,
       0,
       0
      ],
      "balance": 0
     }
    }
   ],
   "knobs": [
    {
     "id": "gray",
     "label": "灰度",
     "description": "saturation.amount，越小越灰",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 3,
       "param": "amount",
       "range": [
        0.5,
        0.9
       ]
      }
     ]
    },
    {
     "id": "matte",
     "label": "哑光",
     "description": "曲线由 strength 主导；此旋钮改 contrast",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 1,
       "param": "contrast",
       "range": [
        0.6,
        0.95
       ]
      }
     ]
    }
   ]
  },
  {
   "id": "dark_moody",
   "name_zh": "暗黑情绪",
   "name_en": "Dark Moody",
   "status": "established",
   "tags": [
    "人像",
    "低调",
    "静物"
   ],
   "suitable": [
    "黄昏/阴天人像",
    "咖啡馆",
    "静物",
    "男性肖像"
   ],
   "caution": [
    "儿童/婚礼明快题材禁用",
    "只有黑没有层次 = 左端撞墙"
   ],
   "description": "低调、压抑、戏剧，一束光打在主体上。",
   "notes": "未在实测批次中，未目检。期 5 目检成立：真照片三格行（ojigi_temple 夜庙（竖幅修正版））已验收。",
   "correction": {
    "baseline": "full",
    "ops": [
     {
      "kind": "adjustment",
      "type": "exposure",
      "params": {
       "ev": 0.1
      },
      "neutral": {
       "ev": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "shadows_highlights",
      "params": {
       "shadows": 0.2,
       "highlights": -0.25
      },
      "neutral": {
       "shadows": 0,
       "highlights": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "dehaze",
      "params": {
       "amount": 0.05
      },
      "neutral": {
       "amount": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "clarity",
      "params": {
       "amount": 0.05
      },
      "neutral": {
       "amount": 0
      }
     }
    ]
   },
   "style_ops": [
    {
     "kind": "adjustment",
     "type": "exposure",
     "params": {
      "ev": -0.45
     },
     "neutral": {
      "ev": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "brightness_contrast",
     "params": {
      "brightness": 0.0,
      "contrast": 1.22
     },
     "neutral": {
      "brightness": 0,
      "contrast": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "shadows_highlights",
     "params": {
      "shadows": 0.3,
      "highlights": -0.6
     },
     "neutral": {
      "shadows": 0,
      "highlights": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "saturation",
     "params": {
      "amount": 0.82
     },
     "neutral": {
      "amount": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "split_toning",
     "params": {
      "shadows": [
       -0.08,
       0.02,
       0.1
      ],
      "highlights": [
       0.07,
       0.03,
       -0.03
      ],
      "balance": 0.0,
      "amount": 0.35
     },
     "neutral": {
      "shadows": [
       0,
       0,
       0
      ],
      "highlights": [
       0,
       0,
       0
      ],
      "balance": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "clarity",
     "params": {
      "amount": 0.15
     },
     "neutral": {
      "amount": 0
     }
    },
    {
     "kind": "filter",
     "type": "vignette",
     "params": {
      "strength": 0.24,
      "radius": 0.7,
      "softness": 0.8
     },
     "neutral": {
      "strength": 0
     }
    },
    {
     "kind": "filter",
     "type": "film_grain",
     "params": {
      "amount": 0.2,
      "size": 2,
      "seed": 17
     },
     "neutral": {
      "amount": 0
     }
    }
   ],
   "knobs": [
    {
     "id": "dark",
     "label": "压暗",
     "description": "exposure.ev",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 0,
       "param": "ev",
       "range": [
        -0.8,
        -0.1
       ]
      }
     ]
    },
    {
     "id": "vignette",
     "label": "暗角",
     "description": "vignette.strength",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 6,
       "param": "strength",
       "range": [
        0.0,
        0.5
       ]
      }
     ]
    }
   ]
  },
  {
   "id": "black_white",
   "name_zh": "黑白",
   "name_en": "B&W",
   "status": "established",
   "tags": [
    "街头",
    "建筑",
    "风光"
   ],
   "suitable": [
    "建筑",
    "街头",
    "纹理人像",
    "形态强的风光"
   ],
   "caution": [
    "靠色彩叙事的题材（花海/美食/霓虹）"
   ],
   "description": "形态、纹理、灰阶分离；强 S 曲线 + 银盐颗粒。",
   "notes": "已目检（期 5）：d700 桌游货架真照片三格行，全幅真黑白且灰阶分离优秀（红/绿/蓝/黄盒映射灰阶分明）；saturation 插值到 0 与真 black_and_white op（amount 1.0、缺省 Rec709 权重）像素等价（期 5 双配方对照，mean_abs_diff 0.0001），故保留 saturation 实现——是等价结论而非不得已。black_and_white 自期 4 起有 amount中性值（0 = 恒等），旧「无中性态」障碍已不存在；逐色 B&W Mix 权重仍为未来选项。",
   "correction": {
    "baseline": "full",
    "ops": [
     {
      "kind": "adjustment",
      "type": "exposure",
      "params": {
       "ev": 0.1
      },
      "neutral": {
       "ev": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "shadows_highlights",
      "params": {
       "shadows": 0.2,
       "highlights": -0.25
      },
      "neutral": {
       "shadows": 0,
       "highlights": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "dehaze",
      "params": {
       "amount": 0.05
      },
      "neutral": {
       "amount": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "clarity",
      "params": {
       "amount": 0.05
      },
      "neutral": {
       "amount": 0
      }
     }
    ]
   },
   "style_ops": [
    {
     "kind": "adjustment",
     "type": "saturation",
     "params": {
      "amount": 0.0
     },
     "neutral": {
      "amount": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "brightness_contrast",
     "params": {
      "brightness": 0.0,
      "contrast": 1.26
     },
     "neutral": {
      "brightness": 0,
      "contrast": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "curves",
     "params": {
      "channel": "rgb",
      "points": [
       [
        0,
        0
       ],
       [
        0.0509,
        0.0331
       ],
       [
        0.214,
        0.233
       ],
       [
        0.5225,
        0.6383
       ],
       [
        1,
        1
       ]
      ]
     },
     "neutral": {
      "points": [
       [
        0,
        0
       ],
       [
        0.0509,
        0.0509
       ],
       [
        0.214,
        0.214
       ],
       [
        0.5225,
        0.5225
       ],
       [
        1,
        1
       ]
      ]
     }
    },
    {
     "kind": "adjustment",
     "type": "clarity",
     "params": {
      "amount": 0.38
     },
     "neutral": {
      "amount": 0
     }
    },
    {
     "kind": "filter",
     "type": "sharpen",
     "params": {
      "amount": 0.8,
      "radius": 2
     },
     "neutral": {
      "amount": 0
     }
    },
    {
     "kind": "filter",
     "type": "film_grain",
     "params": {
      "amount": 0.2,
      "size": 3,
      "seed": 19
     },
     "neutral": {
      "amount": 0
     }
    }
   ],
   "knobs": [
    {
     "id": "contrast",
     "label": "灰阶对比",
     "description": "brightness_contrast.contrast",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 1,
       "param": "contrast",
       "range": [
        1.0,
        1.6
       ]
      }
     ]
    },
    {
     "id": "grain",
     "label": "银盐颗粒",
     "description": "film_grain.amount",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 5,
       "param": "amount",
       "range": [
        0.0,
        0.4
       ]
      }
     ]
    }
   ]
  },
  {
   "id": "cyberpunk",
   "name_zh": "赛博朋克",
   "name_en": "Cyberpunk",
   "status": "established",
   "tags": [
    "夜景",
    "霓虹",
    "都市"
   ],
   "suitable": [
    "雨夜霓虹（原片须真有霓虹/LED 光源）"
   ],
   "caution": [
    "日光照片硬套只会得到紫天毒绿"
   ],
   "description": "青 + 品红、霓虹、高反差夜、湿地面反光；放大已有色光而非无中生有。",
   "notes": "chatlog 点名未目检，须补目检才能升内置库。unsupported：LR 校准面板的越界移色用 channel_mixer 近似；霓虹 glow 复制层法用 glow 滤镜近似。期 5 目检成立：真照片三格行（d5600_night 夜街）已验收。",
   "correction": {
    "baseline": "full",
    "ops": [
     {
      "kind": "adjustment",
      "type": "exposure",
      "params": {
       "ev": 0.1
      },
      "neutral": {
       "ev": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "shadows_highlights",
      "params": {
       "shadows": 0.2,
       "highlights": -0.25
      },
      "neutral": {
       "shadows": 0,
       "highlights": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "dehaze",
      "params": {
       "amount": 0.05
      },
      "neutral": {
       "amount": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "clarity",
      "params": {
       "amount": 0.05
      },
      "neutral": {
       "amount": 0
      }
     }
    ]
   },
   "style_ops": [
    {
     "kind": "adjustment",
     "type": "exposure",
     "params": {
      "ev": -0.2
     },
     "neutral": {
      "ev": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "brightness_contrast",
     "params": {
      "brightness": 0.0,
      "contrast": 1.28
     },
     "neutral": {
      "brightness": 0,
      "contrast": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "shadows_highlights",
     "params": {
      "shadows": 0.35,
      "highlights": -0.4
     },
     "neutral": {
      "shadows": 0,
      "highlights": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "channel_mixer",
     "params": {
      "matrix": [
       [
        1.1,
        0.15,
        0.0
       ],
       [
        0.0,
        0.9,
        0.25
       ],
       [
        0.2,
        0.0,
        1.1
       ]
      ],
      "constants": [
       0.0,
       0.0,
       0.0
      ]
     },
     "neutral": {
      "matrix": [
       [
        1,
        0,
        0
       ],
       [
        0,
        1,
        0
       ],
       [
        0,
        0,
        1
       ]
      ],
      "constants": [
       0,
       0,
       0
      ]
     }
    },
    {
     "kind": "adjustment",
     "type": "split_toning",
     "params": {
      "shadows": [
       -0.1,
       0.0,
       0.16
      ],
      "highlights": [
       0.1,
       -0.02,
       0.12
      ],
      "balance": 0.0,
      "amount": 0.35
     },
     "neutral": {
      "shadows": [
       0,
       0,
       0
      ],
      "highlights": [
       0,
       0,
       0
      ],
      "balance": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "saturation",
     "params": {
      "amount": 1.28
     },
     "neutral": {
      "amount": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "clarity",
     "params": {
      "amount": 0.3
     },
     "neutral": {
      "amount": 0
     }
    },
    {
     "kind": "filter",
     "type": "glow",
     "params": {
      "threshold": 0.6,
      "radius": 8,
      "intensity": 0.5
     },
     "neutral": {
      "intensity": 0
     }
    }
   ],
   "knobs": [
    {
     "id": "neon_glow",
     "label": "霓虹晕",
     "description": "glow.intensity",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 7,
       "param": "intensity",
       "range": [
        0.0,
        1.2
       ]
      }
     ]
    },
    {
     "id": "magenta",
     "label": "品红",
     "description": "split_toning.highlights 品红量",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 4,
       "param": "highlights",
       "range": [
        [
         0.04,
         -0.01,
         0.05
        ],
        [
         0.18,
         -0.04,
         0.2
        ]
       ]
      }
     ]
    }
   ]
  },
  {
   "id": "y2k",
   "name_zh": "Y2K",
   "name_en": "Y2K",
   "status": "established",
   "tags": [
    "夜景",
    "派对",
    "抓拍"
   ],
   "suitable": [
    "派对",
    "镜面自拍",
    "夜出朋友合影"
   ],
   "caution": [
    "严肃场合/商业交付禁用",
    "不要做成暖胶片：Y2K 的黑是硬的、色是数码饱和的"
   ],
   "description": "闪光怼脸、低像素、饱和冷色；借 CCD 成像链再推一档。",
   "notes": "待补图目检。unsupported：日期戳符号与 JPEG 压缩感（导出层的事）不在预设内。期 5 目检成立：真照片三格行（d700_tabletop 桌游货架）已验收。",
   "correction": {
    "baseline": "full",
    "ops": [
     {
      "kind": "adjustment",
      "type": "exposure",
      "params": {
       "ev": 0.1
      },
      "neutral": {
       "ev": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "shadows_highlights",
      "params": {
       "shadows": 0.2,
       "highlights": -0.25
      },
      "neutral": {
       "shadows": 0,
       "highlights": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "dehaze",
      "params": {
       "amount": 0.05
      },
      "neutral": {
       "amount": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "clarity",
      "params": {
       "amount": 0.05
      },
      "neutral": {
       "amount": 0
      }
     }
    ]
   },
   "style_ops": [
    {
     "kind": "adjustment",
     "type": "brightness_contrast",
     "params": {
      "brightness": 0.03,
      "contrast": 1.3
     },
     "neutral": {
      "brightness": 0,
      "contrast": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "saturation",
     "params": {
      "amount": 1.32
     },
     "neutral": {
      "amount": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "white_balance",
     "params": {
      "temperature": -0.08,
      "tint": 0.0
     },
     "neutral": {
      "temperature": 0,
      "tint": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "shadows_highlights",
     "params": {
      "shadows": -0.15,
      "highlights": 0.1
     },
     "neutral": {
      "shadows": 0,
      "highlights": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "clarity",
     "params": {
      "amount": -0.08
     },
     "neutral": {
      "amount": 0
     }
    },
    {
     "kind": "filter",
     "type": "noise",
     "params": {
      "amount": 0.12,
      "seed": 23
     },
     "neutral": {
      "amount": 0
     }
    },
    {
     "kind": "filter",
     "type": "vignette",
     "params": {
      "strength": 0.13,
      "radius": 0.8,
      "softness": 0.8
     },
     "neutral": {
      "strength": 0
     }
    }
   ],
   "knobs": [
    {
     "id": "pop",
     "label": "数码浓艳",
     "description": "saturation.amount",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 1,
       "param": "amount",
       "range": [
        1.0,
        1.6
       ]
      }
     ]
    },
    {
     "id": "noise",
     "label": "低像素噪",
     "description": "noise.amount",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 5,
       "param": "amount",
       "range": [
        0.0,
        0.3
       ]
      }
     ]
    }
   ]
  },
  {
   "id": "guofeng",
   "name_zh": "国风",
   "name_en": "Chinese Ancient / Guochao",
   "status": "fixed_v2",
   "tags": [
    "古建",
    "人像",
    "园林"
   ],
   "suitable": [
    "汉服",
    "古建",
    "园林",
    "茶空间",
    "神社/寺庙"
   ],
   "caution": [
    "现代极简/玻璃建筑只剩偏色",
    "大红大绿同时拉满变影楼风"
   ],
   "description": "朱红、黛青、月白、鎏金；v2 青绿减半，朱红必须保住。",
   "notes": "v1 青绿过头：神社朱红大门被压发闷。v2 硬约束：红还得是红（selective_color reds 减青保红），青只进阴影。研究名「古风/国潮」，实测名「国风」，同一套。",
   "correction": {
    "baseline": "full",
    "ops": [
     {
      "kind": "adjustment",
      "type": "exposure",
      "params": {
       "ev": 0.1
      },
      "neutral": {
       "ev": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "shadows_highlights",
      "params": {
       "shadows": 0.2,
       "highlights": -0.25
      },
      "neutral": {
       "shadows": 0,
       "highlights": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "dehaze",
      "params": {
       "amount": 0.05
      },
      "neutral": {
       "amount": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "clarity",
      "params": {
       "amount": 0.05
      },
      "neutral": {
       "amount": 0
      }
     }
    ]
   },
   "style_ops": [
    {
     "kind": "adjustment",
     "type": "brightness_contrast",
     "params": {
      "brightness": 0.02,
      "contrast": 1.05
     },
     "neutral": {
      "brightness": 0,
      "contrast": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "curves",
     "params": {
      "channel": "rgb",
      "points": [
       [
        0,
        0.0031
       ],
       [
        0.0509,
        0.0637
       ],
       [
        0.214,
        0.2234
       ],
       [
        0.5225,
        0.5382
       ],
       [
        1,
        0.9331
       ]
      ]
     },
     "neutral": {
      "points": [
       [
        0,
        0
       ],
       [
        0.0509,
        0.0509
       ],
       [
        0.214,
        0.214
       ],
       [
        0.5225,
        0.5225
       ],
       [
        1,
        1
       ]
      ]
     }
    },
    {
     "kind": "adjustment",
     "type": "split_toning",
     "params": {
      "shadows": [
       -0.06,
       0.02,
       0.07
      ],
      "highlights": [
       0.07,
       0.03,
       -0.03
      ],
      "balance": 0.0,
      "amount": 0.14
     },
     "neutral": {
      "shadows": [
       0,
       0,
       0
      ],
      "highlights": [
       0,
       0,
       0
      ],
      "balance": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "selective_color",
     "params": {
      "adjust": {
       "reds": [
        -0.27,
        0.0,
        0.09,
        0.0
       ],
       "greens": [
        0.18,
        0.0,
        -0.18,
        0.09
       ],
       "magentas": [
        0.0,
        0.0,
        -0.36,
        0.0
       ]
      }
     },
     "neutral": {
      "adjust": {
       "reds": [
        0,
        0,
        0,
        0
       ],
       "greens": [
        0,
        0,
        0,
        0
       ],
       "magentas": [
        0,
        0,
        0,
        0
       ]
      }
     }
    },
    {
     "kind": "adjustment",
     "type": "saturation",
     "params": {
      "amount": 1.06
     },
     "neutral": {
      "amount": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "clarity",
     "params": {
      "amount": -0.02
     },
     "neutral": {
      "amount": 0
     }
    },
    {
     "kind": "filter",
     "type": "film_grain",
     "params": {
      "amount": 0.08,
      "size": 2,
      "seed": 9
     },
     "neutral": {
      "amount": 0
     }
    },
    {
     "kind": "filter",
     "type": "vignette",
     "params": {
      "strength": 0.09,
      "radius": 0.8,
      "softness": 0.9
     },
     "neutral": {
      "strength": 0
     }
    }
   ],
   "knobs": [
    {
     "id": "daiqing",
     "label": "黛青",
     "description": "split_toning.shadows 青量（v2 已减半，慎加大）",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 2,
       "param": "shadows",
       "range": [
        [
         -0.03,
         0.01,
         0.03
        ],
        [
         -0.1,
         0.04,
         0.12
        ]
       ]
      }
     ]
    },
    {
     "id": "zhuhong",
     "label": "朱红",
     "description": "selective_color.adjust 中 reds 的减青保红量",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 3,
       "param": "adjust",
       "range": [
        {
         "reds": [
          -0.05,
          0.0,
          0.02,
          0.0
         ],
         "greens": [
          0.1,
          0.0,
          -0.1,
          0.05
         ],
         "magentas": [
          0.0,
          0.0,
          -0.2,
          0.0
         ]
        },
        {
         "reds": [
          -0.25,
          0.0,
          0.08,
          0.0
         ],
         "greens": [
          0.1,
          0.0,
          -0.1,
          0.05
         ],
         "magentas": [
          0.0,
          0.0,
          -0.2,
          0.0
         ]
        }
       ]
      }
     ]
    }
   ]
  },
  {
   "id": "ins_minimal",
   "name_zh": "Ins 简约",
   "name_en": "INS Minimal",
   "status": "established",
   "tags": [
    "探店",
    "家居",
    "建筑"
   ],
   "suitable": [
    "探店",
    "好物分享",
    "家居",
    "留白构图",
    "港口"
   ],
   "caution": [
    "杂乱/多色场景极简不了（构图问题）",
    "白色过曝丢细节",
    "暗部勿压太狠"
   ],
   "description": "白、净、留白、低饱和；研究名「ins 极简」，实测名「Ins 简约」，同一套。",
   "notes": "实测一半成立：汉堡港干净简约味成立；圣托里尼暗部压太狠、悬崖成一团黑——本配方阴影只轻提不压，避开该失败模式。",
   "correction": {
    "baseline": "full",
    "ops": [
     {
      "kind": "adjustment",
      "type": "exposure",
      "params": {
       "ev": 0.1
      },
      "neutral": {
       "ev": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "shadows_highlights",
      "params": {
       "shadows": 0.2,
       "highlights": -0.25
      },
      "neutral": {
       "shadows": 0,
       "highlights": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "dehaze",
      "params": {
       "amount": 0.05
      },
      "neutral": {
       "amount": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "clarity",
      "params": {
       "amount": 0.05
      },
      "neutral": {
       "amount": 0
      }
     }
    ]
   },
   "style_ops": [
    {
     "kind": "adjustment",
     "type": "exposure",
     "params": {
      "ev": 0.4
     },
     "neutral": {
      "ev": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "brightness_contrast",
     "params": {
      "brightness": 0.0,
      "contrast": 1.0
     },
     "neutral": {
      "brightness": 0,
      "contrast": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "levels",
     "params": {
      "black": 0.03,
      "white": 1.0,
      "gamma": 1.0
     },
     "neutral": {
      "black": 0,
      "white": 1,
      "gamma": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "shadows_highlights",
     "params": {
      "shadows": 0.25,
      "highlights": -0.2
     },
     "neutral": {
      "shadows": 0,
      "highlights": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "saturation",
     "params": {
      "amount": 0.86
     },
     "neutral": {
      "amount": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "vibrance",
     "params": {
      "amount": 0.15
     },
     "neutral": {
      "amount": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "clarity",
     "params": {
      "amount": 0.08
     },
     "neutral": {
      "amount": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "texture",
     "params": {
      "amount": 0.1
     },
     "neutral": {
      "amount": 0
     }
    }
   ],
   "knobs": [
    {
     "id": "white",
     "label": "通透白",
     "description": "exposure.ev",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 0,
       "param": "ev",
       "range": [
        0.1,
        0.6
       ]
      }
     ]
    },
    {
     "id": "clean",
     "label": "低饱和",
     "description": "saturation.amount，越小越净",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 4,
       "param": "amount",
       "range": [
        0.7,
        1.0
       ]
      }
     ]
    }
   ]
  },
  {
   "id": "sweet",
   "name_zh": "糖水",
   "name_en": "Sweet Portrait",
   "status": "established",
   "tags": [
    "人像",
    "高调",
    "粉嫩"
   ],
   "suitable": [
    "少女写真",
    "闺蜜",
    "樱花/花海人像"
   ],
   "caution": [
    "男性、纪实、老人题材即灾难",
    "过度柔焦+过曝丢轮廓"
   ],
   "description": "甜、嫩、粉、亮——小清新的「肤色极大化」变体。",
   "notes": "实测遗留：去辉光对照实验未跑完（定斜切边缘是否辉光滤镜所致）；本配方默认不带 glow 滤镜，glow 只作 knob 可选项，默认 0。换轻校正基线重渲 1 张。期 5 目检成立：真照片三格行（verona_sunset 日落）已验收。",
   "correction": {
    "baseline": "light",
    "ops": [
     {
      "kind": "adjustment",
      "type": "exposure",
      "params": {
       "ev": 0.05
      },
      "neutral": {
       "ev": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "shadows_highlights",
      "params": {
       "shadows": 0.1,
       "highlights": -0.1
      },
      "neutral": {
       "shadows": 0,
       "highlights": 0
      }
     }
    ]
   },
   "style_ops": [
    {
     "kind": "adjustment",
     "type": "exposure",
     "params": {
      "ev": 0.65
     },
     "neutral": {
      "ev": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "brightness_contrast",
     "params": {
      "brightness": 0.05,
      "contrast": 0.75
     },
     "neutral": {
      "brightness": 0,
      "contrast": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "curves",
     "params": {
      "channel": "rgb",
      "points": [
       [
        0,
        0.0049
       ],
       [
        0.0509,
        0.089
       ],
       [
        0.214,
        0.2633
       ],
       [
        0.5225,
        0.5705
       ],
       [
        1,
        0.9331
       ]
      ]
     },
     "neutral": {
      "points": [
       [
        0,
        0
       ],
       [
        0.0509,
        0.0509
       ],
       [
        0.214,
        0.214
       ],
       [
        0.5225,
        0.5225
       ],
       [
        1,
        1
       ]
      ]
     }
    },
    {
     "kind": "adjustment",
     "type": "shadows_highlights",
     "params": {
      "shadows": 0.5,
      "highlights": -0.5
     },
     "neutral": {
      "shadows": 0,
      "highlights": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "levels",
     "params": {
      "black": 0.06,
      "white": 1.0,
      "gamma": 1.03
     },
     "neutral": {
      "black": 0,
      "white": 1,
      "gamma": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "saturation",
     "params": {
      "amount": 0.9
     },
     "neutral": {
      "amount": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "split_toning",
     "params": {
      "shadows": [
       0.0,
       0.0,
       0.0
      ],
      "highlights": [
       0.06,
       0.0,
       0.02
      ],
      "balance": 0.1,
      "amount": 0.35
     },
     "neutral": {
      "shadows": [
       0,
       0,
       0
      ],
      "highlights": [
       0,
       0,
       0
      ],
      "balance": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "texture",
     "params": {
      "amount": -0.2
     },
     "neutral": {
      "amount": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "clarity",
     "params": {
      "amount": -0.18
     },
     "neutral": {
      "amount": 0
     }
    },
    {
     "kind": "filter",
     "type": "glow",
     "params": {
      "threshold": 0.65,
      "radius": 8,
      "intensity": 0.0
     },
     "neutral": {
      "intensity": 0
     }
    }
   ],
   "knobs": [
    {
     "id": "glow",
     "label": "辉光",
     "description": "glow.intensity（斜切边缘嫌疑项，默认关，开启须目检边缘）",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 9,
       "param": "intensity",
       "range": [
        0.0,
        0.8
       ]
      }
     ]
    },
    {
     "id": "sweet",
     "label": "甜亮",
     "description": "exposure.ev",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 0,
       "param": "ev",
       "range": [
        0.3,
        0.9
       ]
      }
     ]
    }
   ]
  },
  {
   "id": "commercial",
   "name_zh": "商业",
   "name_en": "Commercial",
   "status": "established",
   "tags": [
    "产品",
    "广告",
    "形象照"
   ],
   "suitable": [
    "产品",
    "广告",
    "形象照"
   ],
   "caution": [
    "把情绪风格带进商业交付会被退片：黑点抬高=黑位不准、颗粒=噪点"
   ],
   "description": "准、净、贵——风格即「没有风格」；只纠偏不移色。",
   "notes": "chatlog 点名未目检。unsupported：美妆类频率分离/中性灰 D&B 是 PS 像素级技法，Yanshi 无对应专用工具，不在预设内。期 5 目检成立：真照片三格行（z6_stilllife 桌面静物）已验收。",
   "correction": {
    "baseline": "full",
    "ops": [
     {
      "kind": "adjustment",
      "type": "exposure",
      "params": {
       "ev": 0.1
      },
      "neutral": {
       "ev": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "shadows_highlights",
      "params": {
       "shadows": 0.2,
       "highlights": -0.25
      },
      "neutral": {
       "shadows": 0,
       "highlights": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "dehaze",
      "params": {
       "amount": 0.05
      },
      "neutral": {
       "amount": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "clarity",
      "params": {
       "amount": 0.05
      },
      "neutral": {
       "amount": 0
      }
     }
    ]
   },
   "style_ops": [
    {
     "kind": "adjustment",
     "type": "brightness_contrast",
     "params": {
      "brightness": 0.0,
      "contrast": 1.12
     },
     "neutral": {
      "brightness": 0,
      "contrast": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "saturation",
     "params": {
      "amount": 1.03
     },
     "neutral": {
      "amount": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "texture",
     "params": {
      "amount": 0.2
     },
     "neutral": {
      "amount": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "clarity",
     "params": {
      "amount": 0.15
     },
     "neutral": {
      "amount": 0
     }
    },
    {
     "kind": "filter",
     "type": "sharpen",
     "params": {
      "amount": 0.7,
      "radius": 1
     },
     "neutral": {
      "amount": 0
     }
    }
   ],
   "knobs": [
    {
     "id": "material",
     "label": "材质感",
     "description": "texture.amount",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 2,
       "param": "amount",
       "range": [
        0.0,
        0.4
       ]
      }
     ]
    },
    {
     "id": "sharp",
     "label": "锐利",
     "description": "sharpen.amount",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 4,
       "param": "amount",
       "range": [
        0.0,
        1.4
       ]
      }
     ]
    }
   ]
  },
  {
   "id": "hdr_landscape",
   "name_zh": "HDR 风光",
   "name_en": "HDR Landscape",
   "status": "fixed_v2",
   "tags": [
    "风光",
    "建筑",
    "大光比"
   ],
   "suitable": [
    "大光比风光",
    "建筑"
   ],
   "caution": [
    "雾景、人像禁用 HDR 式拉法",
    "清晰度/去朦胧过量出光晕"
   ],
   "description": "天地全有细节、通透、戏剧化；v2 降全局强度，必须留真黑否则必灰。",
   "notes": "v1 两张都糊：日落阴影硬抬成绿雾、天空橙溢出；罗弗敦压成灰泥，左边缘有可疑青斑（待单独验）。v2：阴影/高光拉量约减半、dehaze/clarity 小量、levels 保真黑。unsupported：真 HDR 需包围曝光合并 DNG，Yanshi 无合并工具，此为单张伪 HDR。",
   "correction": {
    "baseline": "full",
    "ops": [
     {
      "kind": "adjustment",
      "type": "exposure",
      "params": {
       "ev": 0.1
      },
      "neutral": {
       "ev": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "shadows_highlights",
      "params": {
       "shadows": 0.2,
       "highlights": -0.25
      },
      "neutral": {
       "shadows": 0,
       "highlights": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "dehaze",
      "params": {
       "amount": 0.05
      },
      "neutral": {
       "amount": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "clarity",
      "params": {
       "amount": 0.05
      },
      "neutral": {
       "amount": 0
      }
     }
    ]
   },
   "style_ops": [
    {
     "kind": "adjustment",
     "type": "shadows_highlights",
     "params": {
      "shadows": 0.22,
      "highlights": -0.3
     },
     "neutral": {
      "shadows": 0,
      "highlights": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "levels",
     "params": {
      "black": 0.03,
      "white": 0.99,
      "gamma": 1.0
     },
     "neutral": {
      "black": 0,
      "white": 1,
      "gamma": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "dehaze",
     "params": {
      "amount": 0.3
     },
     "neutral": {
      "amount": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "clarity",
     "params": {
      "amount": 0.3
     },
     "neutral": {
      "amount": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "brightness_contrast",
     "params": {
      "brightness": 0.0,
      "contrast": 1.15
     },
     "neutral": {
      "brightness": 0,
      "contrast": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "vibrance",
     "params": {
      "amount": 0.3
     },
     "neutral": {
      "amount": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "saturation",
     "params": {
      "amount": 1.08
     },
     "neutral": {
      "amount": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "split_toning",
     "params": {
      "shadows": [
       -0.05,
       0.02,
       0.08
      ],
      "highlights": [
       0.06,
       0.02,
       -0.04
      ],
      "balance": 0.0,
      "amount": 0.35
     },
     "neutral": {
      "shadows": [
       0,
       0,
       0
      ],
      "highlights": [
       0,
       0,
       0
      ],
      "balance": 0
     }
    }
   ],
   "knobs": [
    {
     "id": "drama",
     "label": "戏剧化",
     "description": "clarity.amount",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 3,
       "param": "amount",
       "range": [
        0.0,
        0.5
       ]
      }
     ]
    },
    {
     "id": "lift",
     "label": "暗部拉量",
     "description": "shadows_highlights.shadows（过量出绿雾/灰泥）",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 0,
       "param": "shadows",
       "range": [
        0.04,
        0.4
       ]
      }
     ]
    }
   ]
  },
  {
   "id": "lomo",
   "name_zh": "LOMO",
   "name_en": "Lomo",
   "status": "established",
   "tags": [
    "抓拍",
    "旅行",
    "生活"
   ],
   "suitable": [
    "生活抓拍",
    "聚会",
    "旅行手帐"
   ],
   "caution": [
    "需要色彩准确的场合禁用",
    "暗角偏重会吃左下细节（在风格本性内）"
   ],
   "description": "浓艳、重暗角、不可预测偏色；宝丽来支线见 notes。",
   "notes": "实测成立：下坡路糖果色与饱和就是 LOMO 本人；圣托里尼暗角偏重但在本性范围内。unsupported：宝丽来 milky fade 支线（白平衡 6000–7500K + 黑点 +30..60）与 Lomo 是两支，本预设只落 Lomo 支；手动加桶形畸变需 lens 调整逐张给，不进预设。",
   "correction": {
    "baseline": "full",
    "ops": [
     {
      "kind": "adjustment",
      "type": "exposure",
      "params": {
       "ev": 0.1
      },
      "neutral": {
       "ev": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "shadows_highlights",
      "params": {
       "shadows": 0.2,
       "highlights": -0.25
      },
      "neutral": {
       "shadows": 0,
       "highlights": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "dehaze",
      "params": {
       "amount": 0.05
      },
      "neutral": {
       "amount": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "clarity",
      "params": {
       "amount": 0.05
      },
      "neutral": {
       "amount": 0
      }
     }
    ]
   },
   "style_ops": [
    {
     "kind": "adjustment",
     "type": "saturation",
     "params": {
      "amount": 1.38
     },
     "neutral": {
      "amount": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "brightness_contrast",
     "params": {
      "brightness": 0.0,
      "contrast": 1.15
     },
     "neutral": {
      "brightness": 0,
      "contrast": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "split_toning",
     "params": {
      "shadows": [
       -0.06,
       0.04,
       0.08
      ],
      "highlights": [
       0.08,
       0.02,
       -0.04
      ],
      "balance": 0.0,
      "amount": 0.35
     },
     "neutral": {
      "shadows": [
       0,
       0,
       0
      ],
      "highlights": [
       0,
       0,
       0
      ],
      "balance": 0
     }
    },
    {
     "kind": "filter",
     "type": "vignette",
     "params": {
      "strength": 0.3,
      "radius": 0.65,
      "softness": 0.7
     },
     "neutral": {
      "strength": 0
     }
    },
    {
     "kind": "filter",
     "type": "film_grain",
     "params": {
      "amount": 0.18,
      "size": 3,
      "seed": 29
     },
     "neutral": {
      "amount": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "clarity",
     "params": {
      "amount": 0.12
     },
     "neutral": {
      "amount": 0
     }
    }
   ],
   "knobs": [
    {
     "id": "vignette",
     "label": "暗角",
     "description": "vignette.strength（圣托里尼教训：过重吃细节）",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 3,
       "param": "strength",
       "range": [
        0.1,
        0.55
       ]
      }
     ]
    },
    {
     "id": "candy",
     "label": "糖果色",
     "description": "saturation.amount",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 0,
       "param": "amount",
       "range": [
        1.05,
        1.55
       ]
      }
     ]
    }
   ]
  },
  {
   "id": "pastoral_cream",
   "name_zh": "田园奶油",
   "name_en": "Pastoral Cream",
   "status": "established",
   "tags": [
    "田园",
    "人像",
    "亲子"
   ],
   "suitable": [
    "田野",
    "野餐",
    "亲子",
    "花市",
    "田园婚纱"
   ],
   "caution": [
    "都市/夜景无田园信息可提",
    "黄提过头白裙/白墙泛奶黄"
   ],
   "description": "奶油田野、麦色阳光、柔绿、野餐布；法式的更亮更甜近亲，韩系的更暖更自然近亲。",
   "notes": "待补图目检。修正段用 light 基线（与法式/韩系同族，避免灰纱）。期 5 目检成立：真照片三格行（gredos_valley 河谷）已验收。",
   "correction": {
    "baseline": "light",
    "ops": [
     {
      "kind": "adjustment",
      "type": "exposure",
      "params": {
       "ev": 0.05
      },
      "neutral": {
       "ev": 0
      }
     },
     {
      "kind": "adjustment",
      "type": "shadows_highlights",
      "params": {
       "shadows": 0.1,
       "highlights": -0.1
      },
      "neutral": {
       "shadows": 0,
       "highlights": 0
      }
     }
    ]
   },
   "style_ops": [
    {
     "kind": "adjustment",
     "type": "exposure",
     "params": {
      "ev": 0.42
     },
     "neutral": {
      "ev": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "brightness_contrast",
     "params": {
      "brightness": 0.03,
      "contrast": 0.85
     },
     "neutral": {
      "brightness": 0,
      "contrast": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "curves",
     "params": {
      "channel": "rgb",
      "points": [
       [
        0,
        0.0031
       ],
       [
        0.0509,
        0.0732
       ],
       [
        0.214,
        0.2429
       ],
       [
        0.5225,
        0.5542
       ],
       [
        1,
        0.9114
       ]
      ]
     },
     "neutral": {
      "points": [
       [
        0,
        0
       ],
       [
        0.0509,
        0.0509
       ],
       [
        0.214,
        0.214
       ],
       [
        0.5225,
        0.5225
       ],
       [
        1,
        1
       ]
      ]
     }
    },
    {
     "kind": "adjustment",
     "type": "white_balance",
     "params": {
      "temperature": 0.16,
      "tint": 0.02
     },
     "neutral": {
      "temperature": 0,
      "tint": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "split_toning",
     "params": {
      "shadows": [
       0.04,
       0.02,
       -0.02
      ],
      "highlights": [
       0.07,
       0.04,
       -0.03
      ],
      "balance": 0.05,
      "amount": 0.35
     },
     "neutral": {
      "shadows": [
       0,
       0,
       0
      ],
      "highlights": [
       0,
       0,
       0
      ],
      "balance": 0
     }
    },
    {
     "kind": "adjustment",
     "type": "saturation",
     "params": {
      "amount": 0.92
     },
     "neutral": {
      "amount": 1
     }
    },
    {
     "kind": "adjustment",
     "type": "clarity",
     "params": {
      "amount": -0.09
     },
     "neutral": {
      "amount": 0
     }
    },
    {
     "kind": "filter",
     "type": "film_grain",
     "params": {
      "amount": 0.1,
      "size": 2,
      "seed": 31
     },
     "neutral": {
      "amount": 0
     }
    }
   ],
   "knobs": [
    {
     "id": "cream",
     "label": "奶油暖",
     "description": "white_balance.temperature",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 3,
       "param": "temperature",
       "range": [
        0.02,
        0.3
       ]
      }
     ]
    },
    {
     "id": "glow_soft",
     "label": "柔光",
     "description": "clarity.amount 负值量",
     "min": 0.0,
     "max": 1.0,
     "default": 0.5,
     "targets": [
      {
       "op": 6,
       "param": "amount",
       "range": [
        -0.25,
        0.0
       ]
      }
     ]
    }
   ]
  }
 ]
}`;
// <<P2_PRESET_RAW:end>>
