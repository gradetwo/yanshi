//! 最小 Web 查看器（设计文档 6.2 / 7 章 / 10 章 / 12.8）。
//!
//! 单页 HTML + 内联 JS，无任何前端依赖：
//!
//! - 通过 `POST /api/documents` 打开/新建文档，把 capability token 放进 URL；
//! - 通过 WebSocket 订阅：控制流（全部原子元数据）实时进入日志面板，
//!   数据流（tile/缩略图）按视口驱动重渲染；
//! - 画笔/矩形/椭圆直接调用工具层，服务端返回 10.1 响应与预览地址；
//! - 撤销/重做走 `revert` / `reapply`，时间旅行走 `revert_to`。
//!
//! 该页面是 Phase 1 的「最小 Web 查看器」交付物；完整编辑器属后续阶段。

/// 查看器页面（HTML + CSS + JS）。
pub const PAGE: &str = r##"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>偃师 Yanshi 查看器</title>
<link rel="icon" type="image/svg+xml" href="/favicon.svg" />
<link rel="icon" type="image/png" sizes="32x32" href="/favicon.png" />
<link rel="apple-touch-icon" href="/brand/png/favicon-180.png" />
<style>
  :root { color-scheme: light dark; --line: #8884; }
  * { box-sizing: border-box; }
  body { margin: 0; font: 13px/1.5 system-ui, "Noto Sans CJK SC", sans-serif; }
  header { display: flex; gap: 8px; align-items: center; padding: 8px 12px; border-bottom: 1px solid var(--line); flex-wrap: wrap; }
  header h1 { font-size: 15px; margin: 0 12px 0 0; display: flex; align-items: center; gap: 6px; }
  .brand-mark { width: 22px; height: 22px; border-radius: 5px; }
  /* justify-items: start 让舞台收缩到 canvas 自身尺寸：否则栅格会把 .stage 拉到整列宽，
     右侧露出一块灰色死区，点击落在 .stage 上而不是 canvas 上（用户报告的「右边一块没法用」）。 */
  main { display: grid; grid-template-columns: 1fr 320px; gap: 12px; padding: 12px; align-items: start; }
  /* 只让舞台按内容收缩（否则右侧留出灰色死区、点击落在 stage 上）；右侧面板保持 320px 列宽，
     不能一起收缩，否则工具按钮会溢出窗口。 */
  .stage { justify-self: start; max-width: 100%; }
  .stage { position: relative; border: 1px solid var(--line); border-radius: 6px; overflow: hidden; background: #f5f5f5; }
  /* 单一几何：内容画布 #board 决定尺寸（文档分辨率位图 + 固有宽高比）；
     #overlay 只画拖动中的笔迹预览，位置与尺寸由 JS 同步为 board 的显示矩形。
     两层分离的原因：此前预览与内容共用一个画布，重绘预览时会把内容一起清空，
     提交后画布变空白（刷新才恢复）。 */
  #board { display: block; width: auto; height: auto; max-width: 100%; max-height: calc(100vh - 96px); touch-action: none; cursor: crosshair; background: #fff; image-rendering: pixelated; }
  #overlay { position: absolute; left: 0; top: 0; pointer-events: none; image-rendering: pixelated; }
  aside { display: grid; gap: 12px; }
  #history { max-height: 220px; overflow: auto; font-family: ui-monospace, monospace; font-size: 11px; }
  #history .row { display: flex; gap: 6px; align-items: center; padding: 1px 0; }
  #history .row button { padding: 0 5px; font-size: 11px; }
  #history .seq { opacity: .6; min-width: 34px; }
  #history .kind { min-width: 86px; }
  #history .actor { opacity: .75; }
  .card { border: 1px solid var(--line); border-radius: 6px; padding: 8px 10px; }
  .card h2 { font-size: 12px; margin: 0 0 6px; text-transform: uppercase; letter-spacing: .06em; opacity: .7; }
  button { font: inherit; padding: 4px 9px; border-radius: 5px; border: 1px solid var(--line); background: transparent; cursor: pointer; }
  button[aria-pressed="true"] { background: #4a7dff22; border-color: #4a7dff; }
  input, select { font: inherit; padding: 3px 6px; border-radius: 5px; border: 1px solid var(--line); background: transparent; }
  #thumb { display: block; border: 1px solid var(--line); border-radius: 5px; background: #fff; width: 128px; height: 128px; object-fit: contain; }
  #log { max-height: 240px; overflow: auto; font-family: ui-monospace, monospace; font-size: 11px; }
  #log div { white-space: nowrap; }
  .status { display: flex; gap: 10px; flex-wrap: wrap; font-family: ui-monospace, monospace; font-size: 11px; opacity: .85; }
  .dot { display: inline-block; width: 8px; height: 8px; border-radius: 50%; background: #c33; vertical-align: middle; }
  .dot.on { background: #2a2; }
</style>
</head>
<body>
<header>
  <h1><img class="brand-mark" src="/brand/svg/icon-light.svg" alt="" />偃师 Yanshi</h1>
  <span id="identity"></span>
  <button id="newDoc">新建</button>
  <button id="openDoc">打开</button>
  <span class="status">
    <span><span class="dot" id="conn"></span> <span id="connText">未连接</span></span>
    <span>head <b id="head">0</b></span>
    <span>rendered <b id="rendered">0</b></span>
    <span>dirty <b id="dirty">0</b></span>
    <span>缩放 <b id="zoom">100%</b></span>
    <span id="undoDepth">撤销 0 / 重做 0</span>
  </span>
</header>
<main>
  <div class="stage">
    <canvas id="board"></canvas>
    <canvas id="overlay"></canvas>
  </div>
  <aside>
    <div class="card">
      <h2>工具</h2>
      <div style="display:flex; gap:6px; flex-wrap:wrap">
        <button data-tool="brush" aria-pressed="true">画笔</button>
        <button data-tool="rect">矩形</button>
        <button data-tool="ellipse">椭圆</button>
        <button data-tool="erase">橡皮</button>
        <button data-tool="clone_stamp">仿制</button>
        <button data-tool="heal_stamp">修复</button>
        <button data-tool="smudge">涂抹</button>
        <button data-tool="liquify_push">液化推</button>
        <button data-tool="liquify_twirl">液化旋</button>
        <button data-tool="liquify_pinch">液化缩</button>
        <button data-tool="undo">撤销</button>
        <button data-tool="redo">重做</button>
        <button data-tool="refresh">刷新</button>
        <button data-tool="check">一致性自检</button>
        <button id="addLayer">＋ 图层</button>
        <button id="exportPng">导出 PNG</button>
        <button id="zoomFit">适配</button>
        <button id="zoomActual">1:1</button>
      </div>
      <div style="display:flex; gap:6px; margin-top:8px; align-items:center">
        <label>粗细 <input id="size" type="range" min="1" max="64" value="6" /></label>
        <input id="color" type="color" value="#222222" />
        <label>强度 <input id="strength" type="range" min="1" max="100" value="40" /></label>
        <label>图层 <select id="layer"></select></label>
      </div>
    </div>
    <div class="card">
      <h2>WASM 计算内核</h2>
      <div class="status">
        <span>本地乐观渲染 <b id="wasmState">检测中…</b></span>
        <span>首笔 <b id="firstStroke">—</b></span>
        <span>首帧 <b id="firstPaint">—</b></span>
        <span>内核预热 <b id="kernelWarm">—</b></span>
        <span>bit-exact <b id="bitExact">—</b></span>
      </div>
    </div>
    <div class="card">
      <h2>调整 / 滤镜</h2>
      <div style="display:flex; gap:6px; margin-bottom:6px; flex-wrap:wrap">
        <select id="effectKind">
          <option value="adjustment">调整</option>
          <option value="filter">滤镜</option>
        </select>
        <select id="effectName"></select>
        <button id="effectApply">应用</button>
      </div>
      <div style="display:flex; gap:6px; align-items:center; margin-bottom:6px">
        <input id="effectParams" value="{}" style="flex:1; font-family:ui-monospace,monospace" />
      </div>
      <div id="effectsList" style="font-family:ui-monospace,monospace;font-size:11px;max-height:120px;overflow:auto"></div>
    </div>
    <div class="card">
      <h2>历史（原子日志）</h2>
      <div style="display:flex; gap:6px; margin-bottom:6px; flex-wrap:wrap">
        <select id="historyKind"><option value="">全部类型</option></select>
        <select id="historyActor"><option value="">全部操作者</option></select>
        <button id="historyReload">重新载入</button>
      </div>
      <div id="history"></div>
    </div>
    <div class="card">
      <h2>缩略图</h2>
      <img id="thumb" alt="缩略图" />
    </div>
    <div class="card">
      <h2>原子日志（控制流）</h2>
      <div id="log"></div>
    </div>
    <div class="card">
      <h2>最近一次响应</h2>
      <div id="last" style="font-family:ui-monospace,monospace;font-size:11px"></div>
    </div>
    <div class="card">
      <h2>反馈</h2>
      <div class="status" style="flex-direction:column; align-items:flex-start; gap:6px">
        <span>问题反馈、协作沟通、缺陷上报：</span>
        <a id="contact" href="mailto:yanshi@wangda.today?subject=%5BYanshi%5D%20"
           style="color:#3f7fd4; font-family:ui-monospace,monospace; font-size:12px">yanshi@wangda.today</a>
        <span style="opacity:.7">安全漏洞请勿开公开 issue，直接发邮件。</span>
      </div>
    </div>
  </aside>
</main>
<script>
// 服务端会把 yanshi://blob/<hash> 改写成 /api/blob/<hash>?doc=..&token=..
// 这里保留一个显式助手，便于直接用 CAS 哈希取回 PNG。
const blobUrl = (hash) => api("/api/blob/" + hash);
// 供 CDP / 自动化验收读取的统计（Phase 2 出口条件：bit-exact 与首笔 < 16ms）。
window.yanshiStats = {
  wasm: false, kernelHead: 0, serverHead: 0,
  firstStrokeMs: null, firstPaintMs: null, kernelWarmMs: null,
  lastApplyMs: null, lastRenderMs: null, lastPutMs: null, lastArea: 0, applies: 0,
  bitExact: null, resyncs: 0,
};

const params = new URLSearchParams(location.search);
const state = {
  docId: params.get("doc") || "default",
  token: params.get("token") || "",
  tool: "brush",
  layerId: null,
  // 撤销/重做双栈：存的是**原始原子 id**。
  // 语义（fold.rs：`revert(revert(x)) ≡ reapply(x)`）⇒ 撤销 = revert(原始)，
  // 重做 = reapply(原始)；因此重做栈里必须放原始 id，而不是 revert 原子自身的 id。
  undoStack: [],
  redoStack: [],
  socket: null,
  docSize: { w: 1024, h: 1024 },
  viewport: { x: 0, y: 0, w: 1024, h: 1024 },
  // 显示缩放（1 = 整幅适配容器）。视口是**文档坐标**子矩形，内核按 1:1 渲染它，
  // CSS 把它放大到容器尺寸 —— 与设计的 viewport/tile 数据流一致（6.6/6.7）。
  zoom: 1,
  // 仿制图章 / 修复画笔的源点（文档坐标）：Alt+点击设置（与常见图像编辑器一致）。
  sourcePoint: null,
  dragging: null,
  points: [],
  wasm: null,
  kernel: null,
  localSeq: 0,
  pending: null,
};

const $ = (id) => document.getElementById(id);
const api = (path) => path + (path.includes("?") ? "&" : "?") + "doc=" + state.docId + "&token=" + state.token;
const board = $("board");
const overlay = $("overlay");
const ctx = board.getContext("2d");
const octx = overlay.getContext("2d");
// 服务端渲染结果用离屏图像承载，**画进内容画布**（不再用覆盖 <img>，避免出现
// 「看到的像素来自被拉伸的 img、点击落在下面的 canvas」这种几何不一致）。
const preview = new Image();

/// 内容层尺寸变化时同步覆盖层的显示矩形（画布按 CSS 缩放，覆盖层必须精确对齐它）。
function syncOverlayGeometry() {
  const stage = board.parentElement;
  const rect = board.getBoundingClientRect();
  const stageRect = stage.getBoundingClientRect();
  overlay.style.left = Math.round(rect.left - stageRect.left) + "px";
  overlay.style.top = Math.round(rect.top - stageRect.top) + "px";
  overlay.style.width = Math.round(rect.width) + "px";
  overlay.style.height = Math.round(rect.height) + "px";
}

/// 设定**视口**尺寸（内容画布与覆盖层同尺寸、同坐标系），并清空两层。
///
/// 画布内部分辨率 = 视口的文档像素数（内核 1:1 渲染），CSS 显示尺寸由 `applyDisplaySize` 决定：
/// 缩放后画布像素变少、显示尺寸不变，于是看得更细（`image-rendering: pixelated` 保持清晰）。
function sizeBoards(width, height) {
  board.width = width;
  board.height = height;
  overlay.width = width;
  overlay.height = height;
  // 用**文档背景色**铺底而不是留透明：切换文档/等待内核期间画布不会出现透明空洞
  // （此前表现为「操作后画布空白」，且在冷启动的临时实例上间歇复现）。
  const background = state.backgroundCss || "#ffffff";
  ctx.fillStyle = background;
  ctx.fillRect(0, 0, width, height);
  octx.clearRect(0, 0, width, height);
  state.viewport.w = width;
  state.viewport.h = height;
  applyDisplaySize();
  syncOverlayGeometry();
}

/// 画布/覆盖层的 CSS 尺寸 = 视口 × 显示缩放（上限为可用区域，避免溢出）。
function applyDisplaySize() {
  const available = availableArea();
  const scale = state.displayScale || 1;
  const width = Math.max(32, Math.min(Math.round(state.viewport.w * scale), available.w));
  const height = Math.max(32, Math.min(Math.round(state.viewport.h * scale), available.h));
  board.style.width = width + "px";
  board.style.height = height + "px";
  overlay.style.width = width + "px";
  overlay.style.height = height + "px";
}

/// 舞台可用区域（主栅格第一列减去右侧面板、间隙与内边距）。
function availableArea() {
  const main = document.querySelector("main");
  const aside = document.querySelector("aside");
  const styles = main ? getComputedStyle(main) : null;
  const gap = styles ? parseFloat(styles.columnGap || "12") : 12;
  const padding = styles ? parseFloat(styles.paddingLeft || "12") * 2 : 24;
  const asideWidth = aside ? aside.getBoundingClientRect().width : 320;
  const width = (main ? main.clientWidth : window.innerWidth) - asideWidth - gap - padding;
  const height = Math.max(240, window.innerHeight - 96);
  return { w: Math.max(160, Math.floor(width)), h: Math.floor(height) };
}

/// 按当前缩放与文档尺寸重新计算视口（以 `center` 为中心，缺省用视口中心）。
function clampViewport(center) {
  const { w: docW, h: docH } = state.docSize;
  const available = availableArea();
  // 显示缩放：整幅适配容器的比例 × 用户缩放。
  const fit = Math.min(available.w / docW, available.h / docH);
  state.displayScale = fit * state.zoom;
  const viewW = Math.min(docW, Math.max(32, Math.floor(available.w / state.displayScale)));
  const viewH = Math.min(docH, Math.max(32, Math.floor(available.h / state.displayScale)));
  const focus = center || {
    x: state.viewport.x + state.viewport.w / 2,
    y: state.viewport.y + state.viewport.h / 2,
  };
  const x = Math.max(0, Math.min(docW - viewW, Math.round(focus.x - viewW / 2)));
  const y = Math.max(0, Math.min(docH - viewH, Math.round(focus.y - viewH / 2)));
  state.viewport = { x, y, w: viewW, h: viewH };
}

/// 文档坐标 → 画布坐标。
function toCanvas(point) {
  return { x: point.x - state.viewport.x, y: point.y - state.viewport.y };
}

/// 重新渲染当前视口（缩放/平移后调用）。
function renderViewport() {
  if (!kernelReady()) return;
  const zoomLabel = $("zoom");
  if (zoomLabel) zoomLabel.textContent = Math.round((state.displayScale || 1) * 100) + "%";
  const { x, y, w, h } = state.viewport;
  sizeBoards(w, h);
  state.kernel.set_viewport(x, y, w, h);
  drawKernelRegion(x, y, w, h);
  subscribeViewport();
  redraw();
}

/// 文档背景（原子里的 `{r,g,b,a}`）转 CSS 颜色；缺省白色。
function backgroundToCss(background) {
  if (!background || typeof background !== "object") return "#ffffff";
  const channel = (value) => Math.max(0, Math.min(255, Math.round(Number(value) || 0)));
  return "rgb(" + channel(background.r) + "," + channel(background.g) + "," + channel(background.b) + ")";
}

function log(line, cls) {
  const el = document.createElement("div");
  el.textContent = line;
  if (cls) el.style.color = cls;
  $("log").prepend(el);
  while ($("log").childElementCount > 200) $("log").lastChild.remove();
}

function setStatus(patch) {
  if (patch.head !== undefined) $("head").textContent = patch.head;
  if (patch.rendered !== undefined) $("rendered").textContent = patch.rendered;
  if (patch.dirty !== undefined) $("dirty").textContent = patch.dirty;
}

async function callTool(name, args, options = {}) {
  const response = await fetch(api("/api/tools/" + name), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(args || {}),
  });
  const value = await response.json();
  $("last").textContent = JSON.stringify(value).slice(0, 600);
  if (value.ok) {
    if (value.head !== undefined) setStatus({ head: value.head, dirty: (value.dirty_tiles || 0) });
    if (options.refresh !== false) {
      // `revert` / `reapply` 自身不入撤销栈：它们由撤销/重做逻辑显式管理栈。
      const trackable = name !== "revert" && name !== "reapply" ? value.atom_id : null;
      afterMutation(trackable);
    }
  } else {
    log("错误 " + value.error_code + "：" + ((value.context && value.context.detail) || ""), "#c33");
  }
  return value;
}

let thumbTimer = null;
let thumbInFlight = false;
// 去抖 + 合并：WS 缩略图事件可能密集到达，逐个 fetch 会把连接打满（曾出现成片 Failed to fetch）。
function scheduleThumbRefresh() {
  if (thumbTimer) return;
  thumbTimer = setTimeout(() => {
    thumbTimer = null;
    refreshThumb();
  }, 400);
}

async function refreshThumb() {
  if (thumbInFlight) return;
  thumbInFlight = true;
  try {
  const value = await fetch(api("/api/tools/get_document"), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: "{}",
  }).then((r) => r.json());
  if (value.thumb_url) $("thumb").src = value.thumb_url + "&t=" + Date.now();
  if (value.head_seq !== undefined) setStatus({ head: value.head_seq, rendered: value.rendered_seq });
  if (value.width && value.height) state.docSize = { w: value.width, h: value.height };
  if (value.background) state.backgroundCss = backgroundToCss(value.background);
  } finally { thumbInFlight = false; }
}

// —— WASM 计算内核（本地乐观渲染，13.3） ——

const CROCKFORD = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";
function ulid() {
  let time = Date.now();
  let out = "";
  for (let index = 9; index >= 0; index--) {
    out = CROCKFORD[time % 32] + out;
    time = Math.floor(time / 32);
  }
  for (let index = 0; index < 16; index++) out += CROCKFORD[Math.floor(Math.random() * 32)];
  return out;
}

async function sha256Hex(bytes) {
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

function kernelReady() {
  return !!(state.kernel && state.wasm);
}

function setWasmState(text, color) {
  const element = $("wasmState");
  element.textContent = text;
  element.style.color = color || "";
}

// 页面依赖的内核方法清单。**必须与 crates/yanshi-wasm/src/lib.rs 的导出逐一对应**：
// 曾经查看器调用了一个从未实现的 `render_region_direct_rgba`，浏览器抛
// "not a function" 被事件处理器吞掉，表现为「拖动无反馈、操作后画布空白」。
// 这里在启动时显式校验：版本不匹配时给出**可操作**的提示，而不是静默失效。
const REQUIRED_KERNEL_METHODS = [
  "render_region_rgba",
  "render_region_direct_rgba",
  "apply_atom_json",
  "extend_preview_stroke",
  "commit_preview",
  "set_viewport",
  "head_seq",
];

function verifyKernelSurface(kernel) {
  const missing = REQUIRED_KERNEL_METHODS.filter((name) => typeof kernel[name] !== "function");
  if (missing.length === 0) return true;
  const message = "WASM 内核与页面版本不一致，缺少方法：" + missing.join(", ") +
    "。请强制刷新（Ctrl+Shift+R / Cmd+Shift+R）。";
  log(message, "#c33");
  setWasmState("版本不匹配", "#c33");
  setStatus({ kernelError: message });
  window.yanshiStats.kernelSurfaceError = message;
  return false;
}

async function initWasm() {
  try {
    const health = await (await fetch("/health")).json();
    if (!health.wasm) throw new Error("服务端未启用（--no-wasm 或产物缺失）");
    // 动态 import：不需要打包器，直接吃 wasm-bindgen --target web 的输出。
    const module = await import("/wasm/yanshi_wasm.js");
    await module.default();
    state.wasm = module;
    window.yanshiStats.wasm = true;
    setWasmState("已加载", "#2a2");
    log("WASM 计算内核已加载：" + module.WasmKernel.name);
  } catch (error) {
    setWasmState("不可用", "#c33");
    log("WASM 内核不可用，退化为服务端渲染：" + error.message, "#c33");
  }
}

async function loadKernel(since = 0) {
  if (!state.wasm) return false;
  const { w, h } = state.docSize;
  const atoms = await fetch(api("/api/atoms") + "&since=" + since).then((r) => r.json());
  if (!atoms.ok) {
    log("读取原子失败：" + JSON.stringify(atoms).slice(0, 160), "#c33");
    return false;
  }
  if (!state.kernel || since === 0) {
    state.kernel = new state.wasm.WasmKernel(state.docId, 256, w, h, 64 * 1024 * 1024);
    // 诊断句柄：仅在 `?debug=1` 时挂到 window 上，供 scripts/browser-kernel-perf.mjs
    // 直接测量内核区域渲染成本（默认不暴露，避免把内部对象变成事实上的公开 API）。
    if (new URLSearchParams(location.search).has("debug")) window.yanshiKernel = state.kernel;
    verifyKernelSurface(state.kernel);
    window.yanshiKernelReady = true;
    const loaded = JSON.parse(state.kernel.load_atoms_json(JSON.stringify(atoms.atoms)));
    if (!loaded.ok) {
      log("内核装载失败：" + JSON.stringify(loaded).slice(0, 160), "#c33");
      state.kernel = null;
      return false;
    }
  } else {
    for (const atom of atoms.atoms) {
      const response = JSON.parse(state.kernel.apply_atom_json(JSON.stringify(atom)));
      if (!response.ok) { log("内核增量折叠失败：" + JSON.stringify(response).slice(0, 160), "#c33"); return false; }
    }
  }
  state.localSeq = atoms.head_seq;
  window.yanshiStats.kernelHead = atoms.head_seq;
  window.yanshiStats.serverHead = atoms.head_seq;
  if (typeof refreshContactLink === "function") refreshContactLink();
  state.kernel.set_viewport(0, 0, w, h);
  return true;
}

/// 渲染文档坐标区域 `(x,y,w,h)` 并画进画布（画布坐标 = 文档坐标 − 视口原点）。
function drawKernelRegion(x, y, w, h) {
  // 裁剪到视口：视口外像素不渲染也不上传（与设计的数据流过滤一致）。
  const vx = state.viewport.x;
  const vy = state.viewport.y;
  const x0 = Math.max(x, vx);
  const y0 = Math.max(y, vy);
  const x1 = Math.min(x + w, vx + board.width);
  const y1 = Math.min(y + h, vy + board.height);
  if (x1 <= x0 || y1 <= y0) return;
  const cw = Math.round(x1 - x0);
  const ch = Math.round(y1 - y0);
  const started = performance.now();
  const rgba = state.kernel.render_region_rgba(x0, y0, cw, ch);
  if (!rgba || rgba.length < cw * ch * 4) return;
  const renderedAt = performance.now();
  // putImageData 不做 CSS 缩放：画布内部分辨率与视口文档像素一一对应。
  ctx.putImageData(
    new ImageData(new Uint8ClampedArray(rgba), cw, ch),
    Math.round(x0 - vx),
    Math.round(y0 - vy)
  );
  const elapsed = performance.now() - started;
  window.yanshiStats.lastRenderMs = renderedAt - started;
  window.yanshiStats.lastPutMs = performance.now() - renderedAt;
  window.yanshiStats.lastArea = w * h;
  window.yanshiStats.lastApplyMs = elapsed;
  if (window.yanshiStats.firstStrokeMs === null && state.dragging) {
    window.yanshiStats.firstStrokeMs = elapsed;
    $("firstStroke").textContent = elapsed.toFixed(2) + "ms";
  }
}

// 拖动中的笔迹重绘：区域通常只有几十像素见方，直接渲染比「按 tile 组合」便宜得多
// （后者哪怕 1px 变化也要重算整块 256² tile）。两者数值逐位一致。
// 把绘制异常变成可见信息（日志 + 状态栏 + window.yanshiStats），只报一次以免刷屏。
function reportPaintError(where, error) {
  const message = where + "失败：" + (error && error.message ? error.message : String(error));
  if (window.yanshiStats.lastPaintError === message) return;
  window.yanshiStats.lastPaintError = message;
  window.yanshiStats.paintErrors = (window.yanshiStats.paintErrors || 0) + 1;
  log(message, "#c33");
  setStatus({ paintError: message });
}

function drawKernelBoxDirect(bbox) {
  if (window.yanshiStats.tracePaints) log("direct bbox=" + JSON.stringify(bbox) + " board=" + board.width + "x" + board.height);
  if (!bbox) return;
  const started = performance.now();
  const vx = state.viewport.x;
  const vy = state.viewport.y;
  const x0 = Math.max(Math.floor(bbox[0]), vx);
  const y0 = Math.max(Math.floor(bbox[1]), vy);
  const x1 = Math.min(Math.ceil(bbox[0] + bbox[2]), vx + board.width);
  const y1 = Math.min(Math.ceil(bbox[1] + bbox[3]), vy + board.height);
  const w = Math.round(x1 - x0);
  const h = Math.round(y1 - y0);
  if (x1 <= x0 || y1 <= y0 || w <= 0 || h <= 0) return;
  const rgba = state.kernel.render_region_direct_rgba(x0, y0, w, h);
  if (window.yanshiStats.tracePaints) log("direct 渲染 " + x0 + "," + y0 + " " + w + "x" + h + " len=" + (rgba ? rgba.length : "null") + " 期望=" + (w * h * 4));
  if (!rgba || rgba.length < w * h * 4) return;
  const renderedAt = performance.now();
  ctx.putImageData(
    new ImageData(new Uint8ClampedArray(rgba), w, h),
    Math.round(x0 - vx),
    Math.round(y0 - vy)
  );
  const elapsed = performance.now() - started;
  window.yanshiStats.lastDirectMs = renderedAt - started;
  window.yanshiStats.lastDirectArea = w * h;
  return elapsed;
}

function drawKernelBox(bbox) {
  if (!bbox) return;
  const x = Math.max(0, Math.floor(bbox[0]));
  const y = Math.max(0, Math.floor(bbox[1]));
  const w = Math.max(1, Math.ceil(bbox[2]));
  const h = Math.max(1, Math.ceil(bbox[3]));
  // 裁剪交给 drawKernelRegion（按视口裁剪，而不是按画布像素数）。
  drawKernelRegion(x, y, w, h);
}

function drawKernelDirty(report) {
  drawKernelBox(report && report.dirty_bbox);
}

// 拖动中的笔迹：**增量盖章**（只处理新增笔段），并只重绘该段区域。
async function updatePreviewOverlay(pending) {
  const started = performance.now();
  // 任何绘制异常都要**显式可见**：此前 TypeError 被事件处理器吞掉，
  // 现象只是「画布空白」，排查代价很高。
  try {
    return await updatePreviewOverlayInner(pending, started);
  } catch (error) {
    reportPaintError("覆盖层绘制", error);
    return undefined;
  }
}

async function updatePreviewOverlayInner(pending, started) {
  const response = JSON.parse(state.kernel.extend_preview_stroke(JSON.stringify(previewObject(pending))));
  if (!response.ok) { log("覆盖层应用失败：" + JSON.stringify(response).slice(0, 160), "#c33"); return; }
  window.yanshiStats.previewApplies = (window.yanshiStats.previewApplies || 0) + 1;
  if (window.yanshiStats.tracePaints) log("盖章返回 dirty_bbox=" + JSON.stringify(response.dirty_bbox) + " keys=" + Object.keys(response).join(","));
  drawKernelBoxDirect(response.dirty_bbox);
  const elapsed = performance.now() - started;
  window.yanshiStats.lastOverlayMs = elapsed;
  window.yanshiStats.overlayApplies = (window.yanshiStats.overlayApplies || 0) + 1;
  if (typeof window.yanshiStats.firstStrokeMs !== "number") {
    window.yanshiStats.firstStrokeMs = elapsed;
    $("firstStroke").textContent = elapsed.toFixed(2) + "ms";
  }
}

// 覆盖层对象（最小字段：layer_id/type/data；内核会补 z 序与可见性）。
function previewObject(pending) {
  const color = colorCss();
  const size = Number($("size").value);
  const points = state.points.map((p) => [p.x, p.y]);
  if (pending.tool === "rect" || pending.tool === "ellipse") {
    const [a, b] = state.points;
    return {
      layer_id: pending.layerId,
      type: "shape",
      data: {
        geometry: {
          kind: pending.tool,
          bbox: {
            x: Math.min(a.x, b.x), y: Math.min(a.y, b.y),
            w: Math.max(1, Math.abs(b.x - a.x)), h: Math.max(1, Math.abs(b.y - a.y)),
          },
        },
        color,
      },
    };
  }
  if (pending.tool === "erase") {
    return { layer_id: pending.layerId, type: "stroke", data: { points, size: size * 1.5, color: { r: 255, g: 255, b: 255, a: 255 } } };
  }
  return { layer_id: pending.layerId, type: "stroke", data: { points, size, color, hardness: 0.7 } };
}

// 把客户端构造的原子立刻应用到本地内核（乐观渲染），返回是否成功。
function applyLocal(atom) {
  const response = JSON.parse(state.kernel.apply_atom_json(JSON.stringify(atom)));
  window.yanshiStats.applies += 1;
  if (response.ok) {
    drawKernelDirty(response.report);
    state.localSeq = response.report.head;
    window.yanshiStats.kernelHead = response.report.head;
    return true;
  }
  if (response.error_code === "out_of_order") return false;
  log("本地应用被拒：" + JSON.stringify(response).slice(0, 160), "#c33");
  return false;
}

// 服务端校正：seq 预测错了（别人插了原子）或提交被拒时，全量重建本地状态。
async function resync() {
  window.yanshiStats.resyncs += 1;
  if (await loadKernel(0)) await refreshPreview(true);
}

/// **提交后的收尾动作，集中在这里**（缩略图、历史列表、撤销/重做栈）。
///
/// 曾经这些动作散落在 `callTool` 与 `submitAtom` 两条路径里 ✗，结果是"笔迹路径漏刷历史/漏入栈"
/// 这类疏漏出现了两次 ✓。现在两条路径都只调这一个函数 ✓；将来再加收尾动作也只改这里。
function afterMutation(atomId) {
  if (atomId) {
    state.undoStack.push(atomId);
    state.redoStack.length = 0;
    updateUndoStatus();
  }
  scheduleThumbRefresh();
  void refreshHistory();
}

async function submitAtom(atom) {
  const response = await fetch(api("/api/atoms"), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(atom),
  }).then((r) => r.json());
  $("last").textContent = JSON.stringify(response).slice(0, 600);
  window.yanshiStats.serverHead = response.head ?? window.yanshiStats.serverHead;
  setStatus({ head: response.head, dirty: (response.dirty_tiles || []).length });
  if (!response.ok) {
    log("提交被拒（回滚本地乐观渲染）：" + response.error_code, "#c33");
    await resync();
    return null;
  }
  if (response.seq !== undefined && response.seq > state.localSeq + 1) {
    // 服务端把原子排在了本地预测之后（有并发原子），补齐缺口。
    await resync();
  }
  // 笔迹走 `/api/atoms`（不经 callTool），但收尾动作与其它提交**完全一致** ✓。
  afterMutation(response.atom_id);
  return response;
}

async function checkBitExact() {
  if (!kernelReady()) { log("没有 WASM 内核，无法自检", "#c33"); return; }
  // 以**服务端返回的尺寸**为准：`state.docSize` 可能因刷新时序而滞后，
  // 那样会出现「尺寸不一致」的误报（实测遇到）。
  const { w, h } = state.docSize;
  // 取服务端**原始像素**（而不是哈希）：哈希相等无法说明差多少，
  // 而跨「客户端预览 / 服务端权威」路径的比较在设计上属 D1（允许 ±1 LSB）。
  const server = await callTool("render_region", {
    region: { x: 0, y: 0, w, h },
    raw: true,
  }, { refresh: false });
  if (!server.raw_url) { log("服务端未返回原始像素，无法自检", "#c33"); return; }
  const response = await fetch(api(server.raw_url.replace("yanshi://blob/", "/api/blob/")));
  const serverPixels = new Uint8Array(await response.arrayBuffer());
  // 用服务端实际渲染的尺寸请求本地像素，避免双方尺寸口径不同。
  const width = server.width || w;
  const height = server.height || h;
  const localPixels = state.kernel.render_region_rgba(0, 0, width, height);
  if (localPixels.length === 0) {
    // 内核没产出像素（未就绪 / 文档尺寸不符 / 内核处于错误状态）：明确报出来，
    // 不要伪装成「尺寸不一致」，否则会误导排查方向。
    window.yanshiStats.bitExact = false;
    $("bitExact").textContent = "内核无输出";
    log(
      `自检失败：本地内核未产出像素（请求 ${width}×${height}）；` +
      `内核 HEAD ${window.yanshiStats.kernelHead}，文档尺寸 ${state.docSize.w}×${state.docSize.h}`,
      "#c33"
    );
    return;
  }
  if (localPixels.length !== serverPixels.length) {
    window.yanshiStats.bitExact = false;
    $("bitExact").textContent = "尺寸不一致";
    log(`自检失败：本地 ${localPixels.length} 字节 vs 服务端 ${serverPixels.length} 字节`, "#c33");
    return;
  }
  let diffPixels = 0;
  let maxDelta = 0;
  for (let index = 0; index < serverPixels.length; index += 4) {
    let pixelDiffers = false;
    for (let channel = 0; channel < 4; channel++) {
      const delta = Math.abs(serverPixels[index + channel] - localPixels[index + channel]);
      if (delta > 0) { pixelDiffers = true; }
      if (delta > maxDelta) { maxDelta = delta; }
    }
    if (pixelDiffers) diffPixels += 1;
  }
  const total = serverPixels.length / 4;
  const ratio = total > 0 ? diffPixels / total : 0;
  // D0（同一路径）应逐位相同；跨路径按 D1 允许 ±1 LSB，且只允许极少数像素踩到舍入边界。
  //
  // 判据由设计决策更新：设计 6.1 原本把**滤镜**列在 D0，实测其成本（方框模糊 O(radius)/像素）是
  // 全项目最大的性能瓶颈，经设计方批准，**模糊族滤镜**放宽到 D1（±1 LSB）。因此跨路径比较不再
  // 要求逐字节相同，而是：**最大通道差 ≤1 LSB** 且 **差异像素占比极少**。
  // 像素数阈值改为**与画布成比例**（0.01%，下限 64）而不是写死 16 —— 写死的绝对值在大画布上过严、
  // 在小画布上过松；比例判据对 1024² 允许约 105 个像素，仍能抓住"大面积 ±1 漂移"这类真实缺陷。
  // 上限取画布的 0.05%（下限 64 像素）。依据实测（1024²）：
  //   * 含大量模糊族滤镜的文档：186 像素（0.018%）
  //   * 含单个 clarity/dehaze 的文档：18 像素（0.0017%）
  //   * **不含模糊族**的文档（形状 + 笔触 + 曝光 + 色彩平衡）：**0 像素（逐位相同）**
  // 即偏差严格限制在模糊族；0.05% 相对实测最差值留约 2.8× 余量，
  // 而"大面积 ±1 漂移"或任何 >1 LSB 的差异仍会被判不通过。
  const allowedDiffPixels = Math.max(64, Math.floor(total * 0.0005));
  const pass = maxDelta <= 1 && diffPixels <= allowedDiffPixels;
  window.yanshiStats.bitExact = pass;
  window.yanshiStats.diffPixels = diffPixels;
  window.yanshiStats.maxChannelDelta = maxDelta;
  window.yanshiStats.localHash = null;
  window.yanshiStats.serverHash = server.blob_hash || null;
  window.yanshiStats.allowedDiffPixels = allowedDiffPixels;
  $("bitExact").textContent = pass
    ? (diffPixels === 0 ? "逐位相同" : `±1 LSB × ${diffPixels}`)
    : `差异 ${diffPixels} 像素 / 最大 ${maxDelta}（上限 ${allowedDiffPixels}）`;
  log(
    `自检：差异像素 ${diffPixels}/${total}（${(ratio * 100).toFixed(4)}%），最大通道差 ${maxDelta}，` +
    `允许上限 ${allowedDiffPixels}；判定 ${pass ? "通过（D1 允许 ±1 LSB）" : "不通过"}`,
    pass ? "#2a7" : "#c33"
  );
  refreshThumb();
}

/// 新建文档：生成新的文档 id（服务端按需创建），并重置本地视图状态。
async function newDocument() {
  const suffix = Date.now().toString(36);
  await switchDocument("yanshi-" + suffix);
}

/// 打开文档：提示输入文档 id（本地工具，缺省空即用当前）。
async function promptDocument() {
  const input = window.prompt("要打开的文档 id（不存在则新建）：", state.docId || "default");
  if (input === null) return;
  const docId = input.trim();
  if (!docId) return;
  await switchDocument(docId);
}

/// 切换文档：关闭旧连接、清空日志与本地状态，再走一遍打开流程。
///
/// 此前「打开 / 新建文档」按钮只是用**当前** doc_id 再调一次 `/api/documents`，
/// 因此点了等于没点（用户报告「点击后没有打开或者创建新的功能」）。
async function switchDocument(docId) {
  if (state.socket) {
    const previous = state.socket;
    state.socket = null; // 先置空，onclose 便不会重连
    try { previous.close(); } catch (_) { /* 已关闭 */ }
  }
  window.yanshiKernelReady = false;
  state.docId = docId;
  state.token = "";
  state.localSeq = 0;
  state.undoStack = [];
  state.redoStack = [];
  updateUndoStatus();
  state.dragging = null;
  state.points = [];
  $("log").innerHTML = "";
  $("last").textContent = "";
  // 画布立刻清空，避免切换期间仍显示上一个文档的内容。
  state.zoom = 1;
  state.displayScale = null;
  state.viewport = { x: 0, y: 0, w: 1024, h: 1024 };
  sizeBoards(1024, 1024);
  await ensureDocument();
}

async function ensureDocument() {
  const response = await fetch("/api/documents", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: state.docId, width: 1024, height: 1024, actor: "human:web" }),
  });
  const value = await response.json();
  if (!value.ok) { log("打开文档失败：" + JSON.stringify(value), "#c33"); return; }
  state.token = value.token;
  state.docId = value.doc_id;
  const url = new URL(location.href);
  url.searchParams.set("doc", state.docId);
  url.searchParams.set("token", state.token);
  history.replaceState(null, "", url);
  $("identity").textContent = state.docId + " · " + value.token.slice(0, 8) + "…";
  await refreshLayers();
  await refreshThumb();
  // 6.2「打开即图片」：先用服务端渲染（含 HEAD 渲染缓存）出像素，
  // WASM 内核在后台预热，就绪后再换成客户端渲染——首帧因此不等待内核折叠。
  const bootStarted = performance.now();
  window.yanshiStats.bootAt = bootStarted;
  await refreshPreview();
  const bootFirstPaint = $("firstPaint");
  if (bootFirstPaint) bootFirstPaint.textContent = "…";
  connect();
  refreshContactLink();
  void warmKernel();
}

// 后台预热 WASM 内核：装载原子并切换为客户端渲染；失败则保持服务端渲染。
async function warmKernel() {
  const started = performance.now();
  await initWasm();
  if (!state.wasm) return;
  const ok = await loadKernel(0);
  window.yanshiStats.kernelWarmMs = performance.now() - started;
  const warm = $("kernelWarm");
  if (warm) warm.textContent = window.yanshiStats.kernelWarmMs.toFixed(0) + "ms";
  if (ok && kernelReady()) {
    // 注意：`refreshPreview(true)` 表示「从服务端取像素」，这里要的是内核路径。
    await refreshPreview();
  }
}

/// 撤销一次（可连续）。栈空时给出明确提示，而不是静默无反应。
async function undoOnce() {
  const atomId = state.undoStack.pop();
  if (!atomId) {
    log("没有可撤销的操作");
    updateUndoStatus();
    return;
  }
  const value = await callTool("revert", { atom_id: atomId }, { refresh: false });
  if (!value.ok) {
    // 撤销失败：把 id 放回去，保持栈与实际状态一致。
    state.undoStack.push(atomId);
    log("撤销失败：" + (value.error_code || "unknown"), "#c33");
  } else {
    state.redoStack.push(atomId);
  }
  updateUndoStatus();
  await refreshPreview();
}

/// 重做一次（按原始顺序：最近一次被撤销的最先重做）。
async function redoOnce() {
  const atomId = state.redoStack.pop();
  if (!atomId) {
    log("没有可重做的操作");
    updateUndoStatus();
    return;
  }
  const value = await callTool("reapply", { atom_id: atomId }, { refresh: false });
  if (!value.ok) {
    state.redoStack.push(atomId);
    log("重做失败：" + (value.error_code || "unknown"), "#c33");
  } else {
    state.undoStack.push(atomId);
  }
  updateUndoStatus();
  await refreshPreview();
}

/// 在状态栏显示撤销/重做深度，并同步按钮可用性。
function updateUndoStatus() {
  const label = $("undoDepth");
  if (label) {
    label.textContent = "撤销 " + state.undoStack.length + " / 重做 " + state.redoStack.length;
  }
  const undo = document.querySelector('button[data-tool="undo"]');
  const redo = document.querySelector('button[data-tool="redo"]');
  if (undo) undo.disabled = state.undoStack.length === 0;
  if (redo) redo.disabled = state.redoStack.length === 0;
}

/// 效果目录来自服务端 `/api/effects`（内容就是内核的 `ADJUSTMENT_NAMES` / `FILTER_NAMES`），
/// 因此查看器**不硬编码效果名** ✓，也就不会与内核漂移 ✓。
let effectCatalog = { adjustment: [], filter: [] };

async function loadEffectCatalog() {
  const value = await fetch(api("/api/effects"), {
    method: "GET",
    headers: { "content-type": "application/json" },
  }).then((response) => response.json());
  effectCatalog = { adjustment: value.adjustments || [], filter: value.filters || [] };
  fillEffectNames();
}

/// 按当前「调整 / 滤镜」选择填充效果下拉。
function fillEffectNames() {
  const kind = $("effectKind").value;
  const select = $("effectName");
  const previous = select.value;
  select.innerHTML = "";
  for (const name of effectCatalog[kind] || []) select.appendChild(new Option(name, name));
  if (previous && (effectCatalog[kind] || []).includes(previous)) select.value = previous;
}

/// 应用当前效果到选中图层。
///
/// 参数缺省为 `{}` —— **不复制内核的默认值** ✓：缺参时由内核自己决定默认 ✓，
/// 应用后再用 `list_effects` 读回**实际生效的参数**展示，做到零漂移。
async function applyEffect() {
  const kind = $("effectKind").value;
  const name = $("effectName").value;
  if (!name) return;
  let params = {};
  const text = $("effectParams").value.trim();
  if (text) {
    try {
      params = JSON.parse(text);
    } catch (error) {
      log("参数不是合法 JSON：" + error.message, "#c33");
      return;
    }
  }
  const tool = kind === "adjustment" ? "add_adjustment" : "add_filter";
  const args =
    kind === "adjustment"
      ? { layer_id: state.layerId, adjustment_type: name, params }
      : { layer_id: state.layerId, filter_name: name, params };
  const value = await callTool(tool, args, { refresh: false });
  if (!value.ok) {
    log("应用失败：" + (value.error_code || "unknown") + " " +
        ((value.context && value.context.detail) || ""), "#c33");
    return;
  }
  log("已应用" + (kind === "adjustment" ? "调整" : "滤镜") + " " + name);
  await refreshEffects();
  await refreshPreview();
}

/// 列出当前文档的调整/滤镜对象（含**实际生效的参数**与顺序）。
async function refreshEffects() {
  const value = await callTool("list_effects", {}, { refresh: false });
  const list = $("effectsList");
  if (!value.ok) {
    list.textContent = "读取失败：" + (value.error_code || "unknown");
    return;
  }
  const effects = value.effects || [];
  list.innerHTML = "";
  if (effects.length === 0) {
    list.textContent = "（当前文档没有调整/滤镜）";
    return;
  }
  for (const effect of effects) {
    const row = document.createElement("div");
    // 字段名以 `list_effects` 的响应为准：adjustment_type / filter_name / params / layer_id。
    const name = effect.adjustment_type || effect.filter_name || effect.name || "?";
    row.textContent =
      name + " " + JSON.stringify(effect.params || {}) +
      " @" + (effect.layer_id || "-");
    list.appendChild(row);
  }
}

/// 历史浏览（设计 13.2）：数据源是原子日志，支持按原子步进、按 actor / 类型筛选。
/// 工具层已有 `get_log`（`since_seq` / `limit` / `kind` / `actor`），这里只做界面。
async function refreshHistory() {
  const kind = $("historyKind").value;
  const actor = $("historyActor").value;
  const args = { limit: 200 };
  if (kind) args.kind = kind;
  if (actor) args.actor = actor;
  const value = await callTool("get_log", args, { refresh: false });
  const list = $("history");
  if (!value.ok) {
    list.textContent = "读取失败：" + (value.error_code || "unknown");
    return;
  }
  const atoms = value.atoms || [];
  historyAtoms = atoms;
  list.innerHTML = "";
  for (const atom of atoms) {
    const row = document.createElement("div");
    row.className = "row";
    const seq = document.createElement("span");
    seq.className = "seq";
    seq.textContent = "#" + atom.seq;
    const kindLabel = document.createElement("span");
    kindLabel.className = "kind";
    kindLabel.textContent = atom.kind;
    const actorLabel = document.createElement("span");
    actorLabel.className = "actor";
    actorLabel.textContent = atom.actor;
    const jump = document.createElement("button");
    jump.textContent = "回到此处";
    jump.addEventListener("click", async () => {
      // `revert_to` 通过 declare_head 回到该时刻；它本身也是一个原子，所以可被撤销。
      const result = await callTool("revert_to", { atom_id: atom.atom_id }, { refresh: false });
      if (!result.ok) {
        log("回到此处失败：" + (result.error_code || "unknown"), "#c33");
        return;
      }
      log("已回到 #" + atom.seq + "（可用撤销恢复）");
      await refreshPreview();
      await refreshHistory();
    });
    row.append(seq, kindLabel, actorLabel, jump);
    list.appendChild(row);
  }
  if (atoms.length === 0) list.textContent = "（没有匹配的原子）";
  fillHistoryFilters(atoms);
}

/// 用当前列表填充筛选下拉（保留已有选项，避免每次重建导致选择丢失）。
let historyAtoms = [];
function fillHistoryFilters(atoms) {
  for (const [id, key] of [["historyKind", "kind"], ["historyActor", "actor"]]) {
    const select = $(id);
    const current = select.value;
    const values = new Set(Array.from(select.options).map((option) => option.value).filter(Boolean));
    for (const atom of atoms) values.add(String(atom[key]));
    const wanted = Array.from(values).sort();
    if (wanted.length + 1 !== select.options.length) {
      select.innerHTML = "";
      select.appendChild(new Option("全部" + (key === "kind" ? "类型" : "操作者"), ""));
      for (const value of wanted) select.appendChild(new Option(value, value));
      select.value = current;
    }
  }
}

async function refreshLayers() {
  const value = await fetch(api("/api/tools/list_layers"), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: "{}",
  }).then((r) => r.json());
  const select = $("layer");
  select.innerHTML = "";
  for (const layer of value.layers || []) {
    const option = document.createElement("option");
    option.value = layer.layer_id;
    option.textContent = layer.name + " (#" + layer.layer_id + ")";
    select.appendChild(option);
  }
  if (!value.layers || value.layers.length === 0) {
    const created = await callTool("create_layer", { name: "paint", layer_id: "layer_paint" }, { refresh: false });
    if (created.ok) await refreshLayers();
  }
  state.layerId = select.value || "layer_paint";
  select.onchange = () => { state.layerId = select.value; };
}

async function refreshPreview(fromKernel = false) {
  // 渲染**整幅文档**（不是固定的 512×512 区域），否则大画布会被裁掉。
  const { w, h } = state.docSize;
  if (!fromKernel && kernelReady()) {
    // 本地乐观路径：直接由 WASM 内核出像素，不等服务端。
    state.docSize = { w, h };
    clampViewport();
    renderViewport();
    return;
  }
  // 只有在**没有内核**时才用服务端像素兜底（有内核时内核是主画布的唯一权威来源）。
  const value = await callTool("render_region", {
    region: { x: 0, y: 0, w, h },
  }, { refresh: false });
  if (value.thumb_url) {
    preview.style.visibility = "visible";
    preview.onload = () => {
      // 6.2「打开即图片」：服务端铺底像素**解码完成**才算首帧。
      if (window.yanshiStats.firstPaintMs === null && window.yanshiStats.bootAt) {
        window.yanshiStats.firstPaintMs = performance.now() - window.yanshiStats.bootAt;
        const firstPaint = $("firstPaint");
        if (firstPaint) firstPaint.textContent = window.yanshiStats.firstPaintMs.toFixed(0) + "ms";
      }
      // 统一走 sizeBoards：它会按文档背景铺底。直接改 board.width 会把画布清成**透明**
      // （配合图片加载失败/竞态就表现为「画布空白」）。
      state.docSize = { w: preview.naturalWidth, h: preview.naturalHeight };
      state.zoom = 1;
      clampViewport();
      sizeBoards(state.viewport.w, state.viewport.h);
      setStatus({});
      if (state.socket && state.socket.readyState === 1) subscribeViewport();
      redraw();
    };
    preview.src = value.thumb_url + "&t=" + Date.now();
    setStatus({ rendered: value.head_seq !== undefined ? value.head_seq : undefined, dirty: 0 });
  }
}

// 反馈邮件里预填文档 id 与 HEAD，便于定位问题（不包含任何画布内容）。
function refreshContactLink() {
  const link = $("contact");
  if (!link) return;
  const subject = encodeURIComponent(`[Yanshi] ${state.docId || "document"}`);
  const body = encodeURIComponent(
    `\n\n---\n文档: ${state.docId || "-"}\n本地 HEAD: ${window.yanshiStats.kernelHead}\n` +
    `服务端 HEAD: ${window.yanshiStats.serverHead}\n地址: ${location.href.split("?")[0]}\n`
  );
  link.href = `mailto:yanshi@wangda.today?subject=${subject}&body=${body}`;
}

function connect() {
  const scheme = location.protocol === "https:" ? "wss:" : "ws:";
  const socket = new WebSocket(scheme + "//" + location.host + "/ws?doc=" + state.docId + "&token=" + state.token);
  state.socket = socket;
  socket.onopen = () => {
    $("conn").className = "dot on";
    $("connText").textContent = "已连接";
    subscribeViewport();
  };
  socket.onclose = () => {
    // 只允许**当前**这条连接触发重连：切换文档时我们主动关闭旧连接，
    // 若它的 onclose 也去重连，就会同时存在多条订阅（表现为同一 atom 被处理多次）。
    if (state.socket !== socket) return;
    state.socket = null;
    $("conn").className = "dot";
    $("connText").textContent = "已断开";
    setTimeout(() => { if (state.token && !state.socket) connect(); }, 1500);
  };
  socket.onmessage = (event) => {
    const message = JSON.parse(event.data);
    if (message.type === "event" && message.event && message.event.event === "atom") {
      const event = message.event;
      log("atom seq=" + event.seq + " " + event.kind + (event.heavy ? " [heavy]" : ""));
      setStatus({ head: event.seq });
      // 13.3：其他客户端（以及自己）的原子经全局广播到达后增量折叠并重绘。
      if (kernelReady() && event.atom) {
        const response = JSON.parse(state.kernel.apply_atom_json(JSON.stringify(event.atom)));
        if (response.ok) {
          drawKernelDirty(response.report);
          state.localSeq = response.report.head;
          window.yanshiStats.kernelHead = response.report.head;
          window.yanshiStats.serverHead = event.seq;
        } else if (response.error_code === "out_of_order" || response.error_code === "precondition_failed") {
          resync();
        }
      }
    } else if (message.type === "event" && message.event && message.event.event === "tiles") {
      // 降噪：失效 tile 数只更新状态栏，日志最多每 2 秒一条（此前每个事件都写一行）。
      const count = (message.event.keys || []).length;
      setStatus({ dirty: count });
      const now = performance.now();
      if (now - (window.yanshiStats.lastTileLogMs || 0) > 2000) {
        window.yanshiStats.lastTileLogMs = now;
        window.yanshiStats.tileInvalidations = (window.yanshiStats.tileInvalidations || 0) + count;
        log("tiles " + count + " 个失效（累计 " + window.yanshiStats.tileInvalidations + "）");
      }
    } else if (message.type === "event" && message.event && message.event.event === "thumbnail") {
      scheduleThumbRefresh();
    } else if (message.type === "ack") {
      const result = message.result || {};
      // 注意：ack 里的 preview 是**缩略图级**的服务端预览，不能画进主画布。
      // 内核就绪时主画布的唯一权威来源是内核；把服务端预览覆盖上去会让刚提交的笔迹
      // 「看起来消失」（服务端预览可能早于该原子生成），这与用户报告的
      // 「操作后画布空白、刷新才可见」是同一个根因。
    }
  };
}

function subscribeViewport() {
  if (!state.socket || state.socket.readyState !== 1) return;
  state.socket.send(JSON.stringify({
    type: "subscribe",
    doc_id: state.docId,
    viewport: state.viewport,
    zoom: state.displayScale || 1,
  }));
}

function colorCss() {
  const hex = $("color").value;
  return { r: parseInt(hex.slice(1, 3), 16), g: parseInt(hex.slice(3, 5), 16), b: parseInt(hex.slice(5, 7), 16), a: 255 };
}

// 只重绘**覆盖层**（拖动中的笔迹/选区）。内容层绝不能被清空 —— 此前两者共用一个画布，
// 拖动结束的最后一次重绘会把已提交的内容一起擦掉，表现为「操作后画布空白，刷新才恢复」。
function redraw() {
  // 每次重绘前同步几何：窗口缩放、滚动或布局变化都会让覆盖层偏离内容层。
  syncOverlayGeometry();
  octx.clearRect(0, 0, overlay.width, overlay.height);
  redrawSourceMark();
  if (!state.dragging) return;
  octx.strokeStyle = $("color").value;
  octx.lineWidth = Number($("size").value);
  if (state.tool === "rect" && state.points.length === 2) {
    const [a, b] = state.points;
    const pa = toCanvas(a);
    const pb = toCanvas(b);
    octx.strokeRect(pa.x, pa.y, pb.x - pa.x, pb.y - pa.y);
  } else if (state.tool === "ellipse" && state.points.length === 2) {
    const [a, b] = state.points;
    const pa = toCanvas(a);
    const pb = toCanvas(b);
    octx.beginPath();
    octx.ellipse((pa.x + pb.x) / 2, (pa.y + pb.y) / 2, Math.abs(pb.x - pa.x) / 2, Math.abs(pb.y - pa.y) / 2, 0, 0, Math.PI * 2);
    octx.stroke();
  } else {
    octx.beginPath();
    state.points.forEach((point, index) => {
      const canvasPoint = toCanvas(point);
      return index ? octx.lineTo(canvasPoint.x, canvasPoint.y) : octx.moveTo(canvasPoint.x, canvasPoint.y);
    });
    octx.stroke();
  }
}

function localPoint(event) {
  const rect = board.getBoundingClientRect();
  // 画布内部像素 = 视口文档像素；再加视口原点得到文档坐标。
  return {
    x: state.viewport.x + (event.clientX - rect.left) * board.width / rect.width,
    y: state.viewport.y + (event.clientY - rect.top) * board.height / rect.height,
  };
}

let pendingStroke = null;
let panState = null;

/// 这些工具不走 `draw_stroke` 原子，而是调用同名工具（工具层会构造正确的原子）。
/// 拖动中仍用覆盖层显示笔迹（不给本地乐观像素 —— 内核目前只为 `draw_stroke` 提供增量预览）。
const RETOUCH_TOOLS = new Set([
  "clone_stamp", "heal_stamp", "smudge",
  "liquify_push", "liquify_twirl", "liquify_pinch",
]);

/// Alt+点击设置仿制/修复的源点。
board.addEventListener("pointerdown", (event) => {
  if (!event.altKey || !RETOUCH_TOOLS.has(state.tool)) return;
  const point = localPoint(event);
  state.sourcePoint = { x: Math.round(point.x), y: Math.round(point.y) };
  log("已设置源点 (" + state.sourcePoint.x + ", " + state.sourcePoint.y + ")");
  redrawSourceMark();
  event.preventDefault();
  event.stopPropagation();
}, true);

/// 在覆盖层上标出源点（只是提示，不写入任何原子）。
function redrawSourceMark() {
  const point = state.sourcePoint;
  if (!point || !RETOUCH_TOOLS.has(state.tool)) return;
  const canvasPoint = toCanvas(point);
  if (canvasPoint.x < 0 || canvasPoint.y < 0) return;
  octx.save();
  octx.strokeStyle = "#4a7dff";
  octx.lineWidth = 1;
  octx.beginPath();
  octx.arc(canvasPoint.x, canvasPoint.y, 5, 0, Math.PI * 2);
  octx.moveTo(canvasPoint.x - 8, canvasPoint.y);
  octx.lineTo(canvasPoint.x + 8, canvasPoint.y);
  octx.moveTo(canvasPoint.x, canvasPoint.y - 8);
  octx.lineTo(canvasPoint.x, canvasPoint.y + 8);
  octx.stroke();
  octx.restore();
}

/// 提交一次修图/液化操作：把拖动点列交给对应工具（服务端构造原子）。
async function commitRetouch() {
  const points = state.points.map((point) => [Math.round(point.x), Math.round(point.y)]);
  if (points.length === 0) return;
  const size = Number($("size").value);
  const strength = Number($("strength").value) / 100;
  const tool = state.tool;
  let args = { layer_id: state.layerId, points, size };
  if (tool === "clone_stamp" || tool === "heal_stamp") {
    if (!state.sourcePoint) {
      log("请先按住 Alt 点击设置源点（仿制/修复需要源点）", "#c33");
      return;
    }
    args.source_offset = [
      state.sourcePoint.x - points[0][0],
      state.sourcePoint.y - points[0][1],
    ];
  } else if (tool === "liquify_push") {
    // 方向取整条笔迹的首末向量；点数不足时用最后一点相对前一点的走向。
    const first = points[0];
    const last = points[points.length - 1];
    let direction = [last[0] - first[0], last[1] - first[1]];
    if (direction[0] === 0 && direction[1] === 0) direction = [1, 0];
    args.direction = direction;
    args.strength = strength;
  } else if (tool === "smudge") {
    args.smudge_length = Math.max(1, Math.round(size));
  } else {
    args.strength = strength;
  }
  const value = await callTool(tool, args, { refresh: false });
  if (!value.ok) {
    log("操作失败：" + (value.error_code || "unknown") + " " +
        ((value.context && value.context.detail) || ""), "#c33");
    return;
  }
  await refreshPreview();
  redraw();
}

// 滚轮缩放：以光标下的文档点为锚点（图像编辑器的常规行为）。
board.addEventListener("wheel", (event) => {
  if (!kernelReady()) return;
  event.preventDefault();
  const before = localPoint(event);
  const factor = Math.exp(-event.deltaY * 0.0015);
  state.zoom = Math.max(0.1, Math.min(16, state.zoom * factor));
  clampViewport(before);
  renderViewport();
}, { passive: false });

// 中键拖动平移。
board.addEventListener("pointerdown", (event) => {
  if (event.button !== 1) return;
  event.preventDefault();
  panState = { startX: event.clientX, startY: event.clientY, originX: state.viewport.x, originY: state.viewport.y };
  try { board.setPointerCapture(event.pointerId); } catch (_) { /* 合成事件无 pointerId */ }
});
board.addEventListener("pointermove", (event) => {
  if (!panState) return;
  const scale = (state.displayScale || 1);
  const rect = board.getBoundingClientRect();
  const perPixel = state.viewport.w / Math.max(1, rect.width);
  const center = {
    x: panState.originX - (event.clientX - panState.startX) * perPixel + state.viewport.w / 2,
    y: panState.originY - (event.clientY - panState.startY) * perPixel + state.viewport.h / 2,
  };
  void scale;
  clampViewport(center);
  renderViewport();
});
board.addEventListener("pointerup", (event) => {
  if (panState && event.button === 1) panState = null;
});

// 键盘：+ / - 缩放，0 复位到整幅。
window.addEventListener("keydown", (event) => {
  if (!kernelReady() || event.target instanceof HTMLInputElement) return;
  if (event.key === "+" || event.key === "=") state.zoom = Math.min(16, state.zoom * 1.25);
  else if (event.key === "-") state.zoom = Math.max(0.1, state.zoom / 1.25);
  else if (event.key === "0") state.zoom = 1;
  else return;
  event.preventDefault();
  clampViewport();
  renderViewport();
});

board.addEventListener("pointerdown", (event) => {
  try { board.setPointerCapture(event.pointerId); } catch (_) { /* 合成事件无 pointerId */ }
  state.dragging = event.pointerId;
  state.points = [localPoint(event)];
  pendingStroke = kernelReady()
    ? { atomId: ulid(), objectId: "obj_" + ulid(), layerId: state.layerId, tool: state.tool, base: 0 }
    : null;
});

board.addEventListener("pointermove", (event) => {
  if (state.dragging !== event.pointerId) return;
  const point = localPoint(event);
  if (state.tool === "rect" || state.tool === "ellipse") state.points = [state.points[0], point];
  else state.points.push(point);
  if (pendingStroke && state.points.length >= 2 && !RETOUCH_TOOLS.has(state.tool)) {
    // 乐观渲染：拖动中只更新**本地覆盖层**（不进原子日志），落笔才提交最终原子。
    updatePreviewOverlay(pendingStroke);
  }
  redraw();
});

board.addEventListener("pointerup", async (event) => {
  if (state.dragging !== event.pointerId) return;
  state.dragging = null;
  if (RETOUCH_TOOLS.has(state.tool)) {
    state.points.push(localPoint(event));
    await commitRetouch();
    state.points = [];
    redraw();
    return;
  }
  if (state.tool === "rect" || state.tool === "ellipse") state.points.push(localPoint(event));
  if (pendingStroke && kernelReady()) {
    // 落笔：内核合并本地日志并失效重算受影响 tile，这里**按返回的脏区重绘**。
    // （不要假定覆盖层像素已在 tile 里：覆盖层与提交原子落在不同图层时不成立，
    //   那正是「操作后画布空白、刷新才可见」的原因。）
    const atom = strokeAtom(pendingStroke, true);
    const committed = JSON.parse(state.kernel.commit_preview(JSON.stringify(atom)));
    if (committed.ok) {
      state.localSeq = committed.seq;
      window.yanshiStats.kernelHead = committed.seq;
      if (committed.report) {
        drawKernelDirty(committed.report);
        window.yanshiStats.commits = (window.yanshiStats.commits || 0) + 1;
      }
    } else {
      await resync();
    }
    const response = await submitAtom(atom);
    if (response && response.seq !== undefined && response.seq !== committed.seq) {
      // 服务端把原子排在了别处（并发），以权威日志为准重建。
      await resync();
    }
    pendingStroke = null;
  } else {
    await commitShape();
  }
  state.points = [];
  redraw();
});

// 由当前指针轨迹构造 draw_stroke / draw_shape / erase 原子（客户端 ULID，5.1）。
function strokeAtom(pending, final) {
  const color = colorCss();
  const size = Number($("size").value);
  const points = state.points.map((p) => [Math.round(p.x), Math.round(p.y)]);
  let kind = "draw_stroke";
  let data = { points, size, color, hardness: 0.7 };
  if (pending.tool === "rect" || pending.tool === "ellipse") {
    const [a, b] = state.points;
    const bbox = {
      x: Math.round(Math.min(a.x, b.x)), y: Math.round(Math.min(a.y, b.y)),
      w: Math.max(1, Math.round(Math.abs(b.x - a.x))), h: Math.max(1, Math.round(Math.abs(b.y - a.y))),
    };
    kind = "draw_shape";
    data = { geometry: { kind: pending.tool, bbox }, color };
  } else if (pending.tool === "erase") {
    kind = "erase";
    data = { points, size: size * 1.5, color: { r: 0, g: 0, b: 0, a: 0 } };
  }
  return {
    id: pending.atomId,
    kind,
    actor: "human:web",
    session: "session:wasm",
    timestamp: Date.now(),
    payload: { object_id: pending.objectId, layer_id: pending.layerId, data },
  };
}

async function commitShape() {
  const color = colorCss();
  const size = Number($("size").value);
  if (state.tool === "brush") {
    if (state.points.length < 2) return;
    await callTool("draw_stroke", {
      layer_id: state.layerId,
      data: { points: state.points.map((p) => [p.x, p.y]), size, color, hardness: 0.7 },
    });
  } else if (state.tool === "erase") {
    if (state.points.length < 2) return;
    await callTool("erase", {
      layer_id: state.layerId,
      data: { points: state.points.map((p) => [p.x, p.y]), size: size * 1.5, color: { r: 0, g: 0, b: 0, a: 0 } },
    });
  } else {
    const [a, b] = state.points;
    if (!a || !b) return;
    const bbox = {
      x: Math.min(a.x, b.x), y: Math.min(a.y, b.y),
      w: Math.abs(b.x - a.x) || 1, h: Math.abs(b.y - a.y) || 1,
    };
    await callTool("draw_shape", {
      layer_id: state.layerId,
      data: { geometry: { kind: state.tool, bbox }, color },
    });
  }
  await refreshPreview();
}

for (const button of document.querySelectorAll("button[data-tool]")) {
  button.addEventListener("click", async () => {
    const tool = button.dataset.tool;
    if (tool === "undo") {
      await undoOnce();
      return;
    }
    if (tool === "redo") {
      await redoOnce();
      return;
    }
    if (tool === "refresh") { await refreshPreview(); refreshThumb(); return; }
    if (tool === "check") { await checkBitExact(); return; }
    state.tool = tool;
    for (const other of document.querySelectorAll("button[data-tool]")) {
      other.setAttribute("aria-pressed", String(other === button));
    }
  });
}

$("addLayer").addEventListener("click", async () => {
  const layerId = "layer_" + ulid();
  const created = await callTool("create_layer", { layer_id: layerId, name: "layer" }, { refresh: false });
  if (!created.ok) {
    log("新建图层失败：" + (created.error_code || "unknown"), "#c33");
    return;
  }
  await refreshLayers();
  const select = $("layer");
  select.value = layerId;
  state.layerId = layerId;
  log("已新建图层 " + layerId);
});

/// 导出整幅 PNG：显式请求整幅区域渲染（设计 A 下整幅 PNG 只在**显式导出**时生成），
/// 再把服务端改写过的可直接 GET 的地址交给浏览器下载。
$("exportPng").addEventListener("click", async () => {
  const { w, h } = state.docSize;
  if (!w || !h) {
    log("导出失败：文档尺寸未知", "#c33");
    return;
  }
  const value = await callTool("render_region", { region: { x: 0, y: 0, w, h } }, { refresh: false });
  if (!value.ok || !value.thumb_url) {
    log("导出失败：" + (value.error_code || "no url"), "#c33");
    return;
  }
  if (value.width !== w || value.height !== h) {
    log("导出警告：返回 " + value.width + "×" + value.height + "，期望 " + w + "×" + h, "#c33");
  }
  const link = document.createElement("a");
  link.href = value.thumb_url;
  link.download = (state.docId || "yanshi") + ".png";
  document.body.appendChild(link);
  link.click();
  link.remove();
  window.yanshiStats.lastExport = { url: value.thumb_url, width: value.width, height: value.height, bytes: value.bytes };
  log("已导出 PNG：" + value.width + "×" + value.height + "（" + (value.bytes || 0) + " 字节）");
});

$("zoomFit").addEventListener("click", () => {
  state.zoom = 1;
  clampViewport();
  renderViewport();
});

$("zoomActual").addEventListener("click", () => {
  // 1:1：显示比例 1 像素文档 = 1 CSS 像素。
  if (!state.docSize) return;
  state.zoom = 1 / Math.max(0.0001, state.displayScale || 1);
  clampViewport();
  renderViewport();
});

$("effectKind").addEventListener("change", fillEffectNames);
$("effectApply").addEventListener("click", applyEffect);
$("historyReload").addEventListener("click", refreshHistory);
$("historyKind").addEventListener("change", refreshHistory);
$("historyActor").addEventListener("change", refreshHistory);

$("newDoc").addEventListener("click", newDocument);
$("openDoc").addEventListener("click", promptDocument);

(async () => {
  window.addEventListener("resize", () => {
    if (kernelReady() && state.docSize) {
      clampViewport();
      renderViewport();
    } else {
      syncOverlayGeometry();
    }
    if (state.socket) subscribeViewport();
  });
  window.addEventListener("scroll", syncOverlayGeometry, { passive: true });
  if (!state.token) {
    await ensureDocument();
  } else {
    $("identity").textContent = state.docId;
    await refreshLayers();
    await refreshThumb();
    await loadEffectCatalog();
    await refreshEffects();
    await refreshHistory();
    window.yanshiStats.bootAt = performance.now();
    await refreshPreview();
    connect();
    refreshContactLink();
    void warmKernel();
  }
})();
</script>
</body>
</html>
"##;

/// 页面长度（测试与可观测性）。
pub fn page_len() -> usize {
    PAGE.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_is_self_contained_and_uses_the_documented_endpoints() {
        assert!(PAGE.starts_with("<!DOCTYPE html>"));
        assert!(PAGE.trim_end().ends_with("</html>"));
        for needle in [
            "/api/documents",
            "/api/tools/",
            "/api/blob",
            "/ws?doc=",
            "subscribe",
            "render_region",
            "draw_stroke",
            "draw_shape",
            "revert",
            "reapply",
            "token",
            "viewport",
        ] {
            assert!(PAGE.contains(needle), "查看器缺少 {needle}");
        }
        // 无外部依赖（不加载 CDN 脚本或字体）。
        assert!(!PAGE.contains("http://cdn"));
        assert!(!PAGE.contains("https://cdn"));
        assert!(!PAGE.contains("<script src="));
        assert!(page_len() > 4000);
    }

    /// 回归：内联脚本里 `const preview = $("preview")` 与 `async function preview()`
    /// 曾经同名冲突，导致整页 JS 直接 SyntaxError（浏览器里白屏）。
    ///
    /// 只检查**顶层**（花括号深度 0）声明：函数/块内的同名变量是合法的遮蔽。
    #[test]
    fn viewer_script_has_no_duplicate_top_level_declarations() {
        let script = PAGE
            .split_once("<script>")
            .and_then(|(_, rest)| rest.split_once("</script>"))
            .map(|(script, _)| script)
            .expect("页面含内联脚本");

        let mut names: Vec<&str> = Vec::new();
        let mut depth: i32 = 0;
        for raw_line in script.lines() {
            // 去掉行注释后统计花括号深度（跳过字符串字面量里的括号）。
            let line = match raw_line.split_once("//") {
                Some((code, _)) if !code.contains('"') && !code.contains('\'') => code,
                _ => raw_line,
            };
            if depth == 0 {
                let trimmed = line.trim_start();
                for prefix in ["const ", "let ", "var ", "async function ", "function "] {
                    if let Some(rest) = trimmed.strip_prefix(prefix) {
                        let name = rest
                            .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
                            .next()
                            .unwrap_or("");
                        if !name.is_empty() {
                            names.push(name);
                        }
                        break;
                    }
                }
            }
            let mut in_string: Option<char> = None;
            let mut escaped = false;
            for character in line.chars() {
                match in_string {
                    Some(quote) => {
                        if escaped {
                            escaped = false;
                        } else if character == '\\' {
                            escaped = true;
                        } else if character == quote {
                            in_string = None;
                        }
                    }
                    None => match character {
                        '"' | '\'' => in_string = Some(character),
                        '{' => depth += 1,
                        '}' => depth -= 1,
                        _ => {}
                    },
                }
            }
        }

        let mut sorted = names.clone();
        sorted.sort_unstable();
        let mut duplicates: Vec<&str> = Vec::new();
        for pair in sorted.windows(2) {
            if pair[0] == pair[1] && !duplicates.contains(&pair[1]) {
                duplicates.push(pair[1]);
            }
        }
        assert!(
            duplicates.is_empty(),
            "查看器脚本顶层存在重复声明（会导致 SyntaxError）：{duplicates:?}"
        );
        // **JS ↔ wasm 接口面一致性**：查看器里调用的每个 `state.kernel.<方法>` 都必须在
        // wasm 绑定里真实存在。实际缺陷：查看器长期调用 `render_region_direct_rgba`，而该方法
        // 从未实现 —— 浏览器抛 "not a function" 被事件处理器吞掉，表现为「拖动无反馈、
        // 操作后画布空白」。这类名字不匹配在 Rust 侧编译期发现不了，必须在这里拦住。
        let wasm_bindings = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../yanshi-wasm/src/lib.rs"),
        )
        .expect("应能读取 wasm 绑定源码");
        let exported: Vec<&str> = wasm_bindings
            .lines()
            .filter_map(|line| line.trim().strip_prefix("pub fn "))
            .filter_map(|rest| rest.split(['(', '<']).next())
            .collect();
        let mut missing: Vec<String> = Vec::new();
        let mut rest = script;
        while let Some(index) = rest.find("state.kernel.") {
            rest = &rest[index + "state.kernel.".len()..];
            let name: String = rest
                .chars()
                .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
                .collect();
            // 只检查方法调用（后面紧跟括号），跳过属性读取。
            if rest[name.len()..].starts_with('(')
                && !name.is_empty()
                && !exported.contains(&name.as_str())
                && !missing.contains(&name)
            {
                missing.push(name);
            }
        }
        assert!(
            missing.is_empty(),
            "查看器调用了 wasm 绑定里不存在的方法：{missing:?}（浏览器里会抛 not a function）"
        );
        assert!(script.contains("async function refreshPreview("));
        assert!(names.len() > 15, "顶层声明数量异常：{names:?}");
    }
}
