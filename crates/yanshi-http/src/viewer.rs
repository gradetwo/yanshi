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
<style>
  :root { color-scheme: light dark; --line: #8884; }
  * { box-sizing: border-box; }
  body { margin: 0; font: 13px/1.5 system-ui, "Noto Sans CJK SC", sans-serif; }
  header { display: flex; gap: 8px; align-items: center; padding: 8px 12px; border-bottom: 1px solid var(--line); flex-wrap: wrap; }
  header h1 { font-size: 15px; margin: 0 12px 0 0; }
  main { display: grid; grid-template-columns: 1fr 320px; gap: 12px; padding: 12px; align-items: start; }
  .stage { position: relative; border: 1px solid var(--line); border-radius: 6px; overflow: hidden; background: #f5f5f5; }
  /* 布局由 canvas 驱动（文档分辨率位图 + 固有宽高比）；#preview 绝对定位覆盖其上作为服务端渲染兜底。 */
  #board { display: block; width: auto; height: auto; max-width: 100%; max-height: calc(100vh - 96px); touch-action: none; cursor: crosshair; background: #fff; image-rendering: pixelated; }
  #preview { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: contain; image-rendering: pixelated; display: block; }
  aside { display: grid; gap: 12px; }
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
  <h1>偃师 Yanshi</h1>
  <span id="identity"></span>
  <button id="open">打开 / 新建文档</button>
  <span class="status">
    <span><span class="dot" id="conn"></span> <span id="connText">未连接</span></span>
    <span>head <b id="head">0</b></span>
    <span>rendered <b id="rendered">0</b></span>
    <span>dirty <b id="dirty">0</b></span>
  </span>
</header>
<main>
  <div class="stage">
    <img id="preview" alt="文档预览" />
    <canvas id="board"></canvas>
  </div>
  <aside>
    <div class="card">
      <h2>工具</h2>
      <div style="display:flex; gap:6px; flex-wrap:wrap">
        <button data-tool="brush" aria-pressed="true">画笔</button>
        <button data-tool="rect">矩形</button>
        <button data-tool="ellipse">椭圆</button>
        <button data-tool="erase">橡皮</button>
        <button data-tool="undo">撤销</button>
        <button data-tool="redo">重做</button>
        <button data-tool="refresh">刷新</button>
        <button data-tool="check">一致性自检</button>
      </div>
      <div style="display:flex; gap:6px; margin-top:8px; align-items:center">
        <label>粗细 <input id="size" type="range" min="1" max="64" value="6" /></label>
        <input id="color" type="color" value="#222222" />
        <label>图层 <select id="layer"></select></label>
      </div>
    </div>
    <div class="card">
      <h2>WASM 计算内核</h2>
      <div class="status">
        <span>本地乐观渲染 <b id="wasmState">检测中…</b></span>
        <span>首笔 <b id="firstStroke">—</b></span>
        <span>bit-exact <b id="bitExact">—</b></span>
      </div>
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
  </aside>
</main>
<script>
// 服务端会把 yanshi://blob/<hash> 改写成 /api/blob/<hash>?doc=..&token=..
// 这里保留一个显式助手，便于直接用 CAS 哈希取回 PNG。
const blobUrl = (hash) => api("/api/blob/" + hash);
// 供 CDP / 自动化验收读取的统计（Phase 2 出口条件：bit-exact 与首笔 < 16ms）。
window.yanshiStats = {
  wasm: false, kernelHead: 0, serverHead: 0,
  firstStrokeMs: null, lastApplyMs: null, lastRenderMs: null, lastPutMs: null, lastArea: 0, applies: 0,
  bitExact: null, resyncs: 0,
};

const params = new URLSearchParams(location.search);
const state = {
  docId: params.get("doc") || "default",
  token: params.get("token") || "",
  tool: "brush",
  layerId: null,
  lastAtom: null,
  reverted: [],
  socket: null,
  docSize: { w: 1024, h: 1024 },
  viewport: { x: 0, y: 0, w: 1024, h: 1024 },
  dragging: null,
  points: [],
  wasm: null,
  kernel: null,
  localSeq: 0,
  pending: null,
};

const $ = (id) => document.getElementById(id);
const api = (path) => path + (path.includes("?") ? "&" : "?") + "doc=" + state.docId + "&token=" + state.token;
const preview = $("preview");
const board = $("board");
const ctx = board.getContext("2d");

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
    if (value.atom_id) {
      state.lastAtom = value.atom_id;
      if (name === "revert") state.reverted.push(value.atom_id);
    }
    if (value.preview && value.preview.thumb_url) {
      preview.src = value.preview.thumb_url + (value.preview.thumb_url.includes("?") ? "&" : "?") + "t=" + Date.now();
    }
    if (options.refresh !== false) refreshThumb();
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
  state.kernel.set_viewport(0, 0, w, h);
  return true;
}

function drawKernelRegion(x, y, w, h) {
  const started = performance.now();
  const rgba = state.kernel.render_region_rgba(x, y, w, h);
  if (!rgba || rgba.length < w * h * 4) return;
  const renderedAt = performance.now();
  // putImageData 不做 CSS 缩放：画布与文档同分辨率，坐标一一对应。
  ctx.putImageData(new ImageData(new Uint8ClampedArray(rgba), w, h), x, y);
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

function drawKernelBox(bbox) {
  if (!bbox) return;
  const x = Math.max(0, Math.floor(bbox[0]));
  const y = Math.max(0, Math.floor(bbox[1]));
  const w = Math.max(1, Math.ceil(bbox[2]));
  const h = Math.max(1, Math.ceil(bbox[3]));
  drawKernelRegion(x, y, Math.min(w, board.width - x), Math.min(h, board.height - y));
}

function drawKernelDirty(report) {
  drawKernelBox(report && report.dirty_bbox);
}

// 拖动中的笔迹：**增量盖章**（只处理新增笔段），并只重绘该段区域。
async function updatePreviewOverlay(pending) {
  const started = performance.now();
  const response = JSON.parse(state.kernel.extend_preview_stroke(JSON.stringify(previewObject(pending))));
  if (!response.ok) { log("覆盖层应用失败：" + JSON.stringify(response).slice(0, 160), "#c33"); return; }
  window.yanshiStats.previewApplies = (window.yanshiStats.previewApplies || 0) + 1;
  drawKernelBox(response.dirty_bbox);
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
  return response;
}

async function checkBitExact() {
  if (!kernelReady()) { log("没有 WASM 内核，无法自检", "#c33"); return; }
  const { w, h } = state.docSize;
  const localPng = state.kernel.render_region_png(0, 0, w, h);
  const localHash = "sha256:" + (await sha256Hex(localPng));
  const server = await callTool("render_region", { region: { x: 0, y: 0, w, h } }, { refresh: false });
  const matches = server.blob_hash === localHash;
  window.yanshiStats.bitExact = matches;
  $("bitExact").textContent = matches ? "通过" : "不一致";
  $("bitExact").style.color = matches ? "#2a2" : "#c33";
  log(
    matches
      ? "bit-exact 自检通过：本地 " + localHash.slice(0, 22) + "… 与服务端一致"
      : "bit-exact 不一致：本地 " + localHash + " 服务端 " + server.blob_hash,
    matches ? undefined : "#c33"
  );
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
  await initWasm();
  if (state.wasm) { await loadKernel(0); if (kernelReady()) preview.style.visibility = "hidden"; }
  await refreshPreview();
  connect();
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
    board.width = w;
    board.height = h;
    state.viewport = { x: 0, y: 0, w, h };
    state.kernel.set_viewport(0, 0, w, h);
    drawKernelRegion(0, 0, w, h);
    subscribeViewport();
    return;
  }
  const value = await callTool("render_region", {
    region: { x: 0, y: 0, w, h },
  }, { refresh: false });
  if (value.thumb_url) {
    preview.style.visibility = "visible";
    preview.onload = () => {
      board.width = preview.naturalWidth;
      board.height = preview.naturalHeight;
      state.viewport = { x: 0, y: 0, w: preview.naturalWidth, h: preview.naturalHeight };
      setStatus({});
      if (state.socket && state.socket.readyState === 1) subscribeViewport();
      redraw();
    };
    preview.src = value.thumb_url + "&t=" + Date.now();
    setStatus({ rendered: value.head_seq !== undefined ? value.head_seq : undefined, dirty: 0 });
  }
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
    $("conn").className = "dot";
    $("connText").textContent = "已断开";
    setTimeout(() => { if (state.token) connect(); }, 1500);
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
      log("tiles " + (message.event.keys || []).length + " 个失效");
      setStatus({ dirty: (message.event.keys || []).length });
    } else if (message.type === "event" && message.event && message.event.event === "thumbnail") {
      scheduleThumbRefresh();
    } else if (message.type === "ack") {
      const result = message.result || {};
      if (result.preview && result.preview.thumb_url) preview.src = result.preview.thumb_url + "&t=" + Date.now();
    }
  };
}

function subscribeViewport() {
  if (!state.socket || state.socket.readyState !== 1) return;
  state.socket.send(JSON.stringify({
    type: "subscribe",
    doc_id: state.docId,
    viewport: state.viewport,
    zoom: 1,
  }));
}

function colorCss() {
  const hex = $("color").value;
  return { r: parseInt(hex.slice(1, 3), 16), g: parseInt(hex.slice(3, 5), 16), b: parseInt(hex.slice(5, 7), 16), a: 255 };
}

function redraw() {
  ctx.clearRect(0, 0, board.width, board.height);
  if (!state.dragging) return;
  ctx.strokeStyle = $("color").value;
  ctx.lineWidth = Number($("size").value);
  if (state.tool === "rect" && state.points.length === 2) {
    const [a, b] = state.points;
    ctx.strokeRect(a.x, a.y, b.x - a.x, b.y - a.y);
  } else if (state.tool === "ellipse" && state.points.length === 2) {
    const [a, b] = state.points;
    ctx.beginPath();
    ctx.ellipse((a.x + b.x) / 2, (a.y + b.y) / 2, Math.abs(b.x - a.x) / 2, Math.abs(b.y - a.y) / 2, 0, 0, Math.PI * 2);
    ctx.stroke();
  } else {
    ctx.beginPath();
    state.points.forEach((point, index) => index ? ctx.lineTo(point.x, point.y) : ctx.moveTo(point.x, point.y));
    ctx.stroke();
  }
}

function localPoint(event) {
  const rect = board.getBoundingClientRect();
  return {
    x: (event.clientX - rect.left) * board.width / rect.width,
    y: (event.clientY - rect.top) * board.height / rect.height,
  };
}

let pendingStroke = null;

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
  if (pendingStroke && state.points.length >= 2) {
    // 乐观渲染：拖动中只更新**本地覆盖层**（不进原子日志），落笔才提交最终原子。
    updatePreviewOverlay(pendingStroke);
  }
  redraw();
});

board.addEventListener("pointerup", async (event) => {
  if (state.dragging !== event.pointerId) return;
  state.dragging = null;
  if (state.tool === "rect" || state.tool === "ellipse") state.points.push(localPoint(event));
  if (pendingStroke && kernelReady()) {
    // 落笔：像素已由增量盖章画好，这里只合并本地日志（不整块重绘，避免抬手卡顿）。
    const atom = strokeAtom(pendingStroke, true);
    const committed = JSON.parse(state.kernel.commit_preview(JSON.stringify(atom)));
    if (committed.ok) {
      state.localSeq = committed.seq;
      window.yanshiStats.kernelHead = committed.seq;
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
      if (state.lastAtom) await callTool("revert", { atom_id: state.lastAtom });
      await refreshPreview();
      return;
    }
    if (tool === "redo") {
      const atom = state.reverted.pop();
      if (atom) await callTool("reapply", { atom_id: atom });
      await refreshPreview();
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

$("open").addEventListener("click", ensureDocument);

(async () => {
  window.addEventListener("resize", () => { if (state.socket) subscribeViewport(); });
  if (!state.token) {
    await ensureDocument();
  } else {
    $("identity").textContent = state.docId;
    await refreshLayers();
    await refreshThumb();
    await initWasm();
    if (state.wasm) { await loadKernel(0); if (kernelReady()) preview.style.visibility = "hidden"; }
    await refreshPreview();
    connect();
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
        assert!(script.contains("async function refreshPreview("));
        assert!(names.len() > 15, "顶层声明数量异常：{names:?}");
    }
}
