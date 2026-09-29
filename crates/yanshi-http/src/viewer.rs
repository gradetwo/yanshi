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
  #preview { display: block; width: 100%; height: auto; max-height: calc(100vh - 96px); object-fit: contain; image-rendering: pixelated; }
  #board { position: absolute; inset: 0; touch-action: none; cursor: crosshair; }
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
      </div>
      <div style="display:flex; gap:6px; margin-top:8px; align-items:center">
        <label>粗细 <input id="size" type="range" min="1" max="64" value="6" /></label>
        <input id="color" type="color" value="#222222" />
        <label>图层 <select id="layer"></select></label>
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

async function refreshThumb() {
  const value = await fetch(api("/api/tools/get_document"), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: "{}",
  }).then((r) => r.json());
  if (value.thumb_url) $("thumb").src = value.thumb_url + "&t=" + Date.now();
  if (value.head_seq !== undefined) setStatus({ head: value.head_seq, rendered: value.rendered_seq });
  if (value.width && value.height) state.docSize = { w: value.width, h: value.height };
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

async function refreshPreview() {
  // 渲染**整幅文档**（不是固定的 512×512 区域），否则大画布会被裁掉。
  const { w, h } = state.docSize;
  const value = await callTool("render_region", {
    region: { x: 0, y: 0, w, h },
  }, { refresh: false });
  if (value.thumb_url) {
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
      const atom = message.event;
      log("atom seq=" + atom.seq + " " + atom.kind + (atom.heavy ? " [heavy]" : ""));
      setStatus({ head: atom.seq });
    } else if (message.type === "event" && message.event && message.event.event === "tiles") {
      log("tiles " + (message.event.keys || []).length + " 个失效");
      setStatus({ dirty: (message.event.keys || []).length });
    } else if (message.type === "event" && message.event && message.event.event === "thumbnail") {
      refreshThumb();
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

board.addEventListener("pointerdown", (event) => {
  board.setPointerCapture(event.pointerId);
  state.dragging = event.pointerId;
  state.points = [localPoint(event)];
});

board.addEventListener("pointermove", (event) => {
  if (state.dragging !== event.pointerId) return;
  const point = localPoint(event);
  if (state.tool === "rect" || state.tool === "ellipse") state.points = [state.points[0], point];
  else state.points.push(point);
  redraw();
});

board.addEventListener("pointerup", async (event) => {
  if (state.dragging !== event.pointerId) return;
  state.dragging = null;
  if (state.tool === "rect" || state.tool === "ellipse") state.points.push(localPoint(event));
  await commitShape();
  state.points = [];
  redraw();
});

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
        assert!(script.contains("async function refreshPreview()"));
        assert!(names.len() > 15, "顶层声明数量异常：{names:?}");
    }
}
