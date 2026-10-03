#!/usr/bin/env node
// ⚠ **模板字符串里绝对不要写反引号** ✗ —— 这个坑在本仓库已经吃掉三轮
//（注释里的反引号会**提前结束模板** ⇒ SyntaxError ✓）。要引名字就用「」。
// **本地预览（`default_preview_points`）与服务端 `brush_preview` 必须逐字节相同** ✓。
//
// **为什么放在页面里** ✓：服务端回的是 **PNG** ✓，而 Node 没有内置 PNG 解码 ✗、浏览器有 ✓
// ⇒ 在页面里 `fetch` 那张 PNG ✓ ⇒ `drawImage` 到离屏 ✓ ⇒ 读回 RGBA ✓ ⇒ 与门面比 ✓。
//
// **两条判据（都能红 ✓）**：
//   ① 服务端 PNG 的尺寸必须等于**按同一套算式算出的区域** ✓（`default_preview_points` + `size/2+4` ✓）；
//   ② 同一支笔、同一 `size`、同一个区域 ⇒ 两边 RGBA **逐字节相同** ✓。
//
// 用法：CDP_PORT=8534 node scripts/wasm-brush-preview-parity.mjs "<viewer-url>" <wasm 路径> [笔刷...]
import { readFileSync } from "node:fs";
const [url, wasmPath, ...brushes] = process.argv.slice(2);
const debugPort = process.env.CDP_PORT || "9334";
if (!url || !wasmPath) { console.error("用法: CDP_PORT=.. node scripts/wasm-brush-preview-parity.mjs <url> <wasm> [brush...]"); process.exit(2); }
const names = brushes.length ? brushes : ["100%_Opaque", "2B_pencil", "spray"];
const wasmBase64 = readFileSync(wasmPath).toString("base64");
const list = await fetch(`http://127.0.0.1:${debugPort}/json/list`).then((r) => r.json());
const target = list.find((t) => t.type === "page");
if (!target) { console.error("未找到调试目标"); process.exit(2); }
const ws = new WebSocket(target.webSocketDebuggerUrl);
let id = 1; const pending = new Map();
ws.addEventListener("message", (event) => { const m = JSON.parse(event.data); if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); } });
await new Promise((resolve) => ws.addEventListener("open", resolve));
const send = (method, params = {}) => new Promise((resolve) => { const i = id++; pending.set(i, resolve); ws.send(JSON.stringify({ id: i, method, params })); });
const evaluate = async (expression) => (await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true })).result?.result?.value;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
await send("Runtime.enable"); await send("Page.enable");
await send("Page.navigate", { url }); await send("Page.reload", { ignoreCache: true }); await sleep(1800);

const script = `(async () => {
  const mybCache = new Map();
  const bytes = Uint8Array.from(atob(${JSON.stringify(wasmBase64)}), (c) => c.charCodeAt(0));
  const { instance } = await WebAssembly.instantiate(bytes, {});
  const api = instance.exports;
  const size = 24;
  // **可选覆盖**（第 42 轮）：环境变量 HARDNESS / OPACITY 给值 ✓，**两边喂同一个值** ✓
  // ⇒ 一次只动一个变量 ✓（前几轮的假设都是这样被否掉的 ✓）。
  const hardness = ${process.env.HARDNESS ? Number(process.env.HARDNESS) : "null"};
  const opacity = ${process.env.OPACITY ? Number(process.env.OPACITY) : "null"};
  // **服务端那份固定笔迹**（tools.rs:10158 的「default_preview_points」✓，一字不差 ✓）。
  const length = Math.min(Math.max(size * 5.0, 64.0), 160.0);
  const margin = size + 8.0;
  const amplitude = Math.min(Math.max(size * 0.6, 4.0), 24.0);
  const y = margin + amplitude;
  // **整幕平移**（第 44 轮）：把笔迹挪到**远离边界**处再做一次同样的比对 ——
  // 区域是由点算出来的，所以挪点即挪区域（两边都挪，变量只有一个：离边界的远近）。
  const shift = ${process.env.SHIFT ? Number(process.env.SHIFT) : 0};
  const points = [[margin + shift, y + shift, 0.35], [margin + length*0.34 + shift, y - amplitude*2.0 + shift, 0.9],
                  [margin + length*0.67 + shift, y + amplitude*2.0 + shift, 0.5], [margin + length + shift, y + shift, 0.35]];
  const half = size / 2 + 4;
  const xs = points.map((p) => p[0]); const ys = points.map((p) => p[1]);
  const region = { x: Math.floor(Math.min(...xs) - half), y: Math.floor(Math.min(...ys) - half),
                   w: Math.ceil(Math.max(...xs) + half) - Math.floor(Math.min(...xs) - half),
                   h: Math.ceil(Math.max(...ys) + half) - Math.floor(Math.min(...ys) - half) };
  const token = new URLSearchParams(location.search).get("token");
  const doc = new URLSearchParams(location.search).get("doc");
  const out = [];
  for (const brush of ${JSON.stringify(names)}) {
    const made = await fetch("/api/tools?doc=" + doc + "&token=" + token, { method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ tool: "brush_preview", arguments: Object.assign({ brush, size },
        hardness === null ? {} : { hardness }, opacity === null ? {} : { opacity }) }) }).then((r) => r.json());
    if (!made.ok) { out.push({ brush, error: (made.context || {}).detail || "被拒" }); continue; }
    const bitmap = await fetch("/api/blob/" + made.blob_hash + "?doc=" + doc + "&token=" + token).then((r) => r.arrayBuffer());
    const bitmapBlob = new Blob([bitmap], { type: "image/png" });
    const image = await createImageBitmap(bitmapBlob);
    const canvas = document.createElement("canvas");
    canvas.width = image.width; canvas.height = image.height;
    const context = canvas.getContext("2d");
    context.drawImage(image, 0, 0);
    const server = context.getImageData(0, 0, image.width, image.height).data;
    const myb = mybCache.get(brush) || await fetch("/brushes/" + brush + ".myb").then((r) => r.text());
    mybCache.set(brush, myb);
    const request = JSON.stringify({ myb, points, size, color: null, opacity, hardness, region });
    const payload = new TextEncoder().encode(request);
    const pointer = api.yanshi_brush_alloc(payload.length);
    new Uint8Array(api.memory.buffer, pointer, payload.length).set(payload);
    const outLen = api.yanshi_brush_paint(pointer, payload.length);
    api.yanshi_brush_free(pointer, payload.length);
    const facade = outLen ? new Uint8Array(api.memory.buffer, api.yanshi_brush_out_ptr(), outLen).slice() : null;
    const sameSize = made.width === region.w && made.height === region.h;
    // **枚数对照** ✓（第 40 轮）：门面那份「stamp」的规则是"首点播种 1 枚 + 每段 ceil(距离/2) 枚
    // （夹在 1..4096）" ✓ ⇒ 用同一条规则算一遍 ✓，与服务端回的 steps 比 ✓。
    // 若枚数不同 ⇒ "抽取顺序/枚数不同"成立 ✓（offset_by_random 会让后续 dab 整体偏移 ✓）；
    // 若枚数相同 ⇒ 差在每一枚 dab 的位置抖动上 ✓。**两种结果指向完全不同的修法** ✓。
    let expectedSteps = 0;
    for (let i = 0; i < points.length; i += 1) {
      if (i === 0) { expectedSteps += 1; continue; }
      const [px, py] = points[i - 1]; const [x, y] = points[i];
      const distance = Math.hypot(x - px, y - py);
      expectedSteps += Math.min(Math.max(Math.ceil(distance / 2.0), 1), 4096);
    }
    let differing = -1, maxDelta = 0, firstDiff = -1;
    // **差在哪也要带出来** ✓（第 38 轮只给了字节偏移 ✓，这次给**坐标 + 两侧 RGBA + 差值分布** ✓
    // ⇒ "1 LSB 的边缘"与"系统性偏差"一眼可分 ✓，这正是本项目反复吃过的亏 ✓）。
    const samples = [];
    const oneBit = { one: 0, small: 0, big: 0 };
    if (facade && facade.length === server.length) {
      differing = 0;
      for (let i = 0; i < server.length; i += 1) {
        const d = Math.abs(server[i] - facade[i]);
        if (!d) continue;
        differing += 1;
        if (d === 1) oneBit.one += 1; else if (d <= 8) oneBit.small += 1; else oneBit.big += 1;
        if (firstDiff < 0) firstDiff = i;
        if (d > maxDelta) maxDelta = d;
        const pixel = Math.floor(i / 4);
        if (samples.length < 3 && (samples.length === 0 || samples[samples.length - 1].pixel !== pixel)) {
          samples.push({ pixel, x: pixel % region.w, y: Math.floor(pixel / region.w), channel: i % 4,
                         server: server[i], facade: facade[i] });
        }
      }
    }
    out.push({ brush, serverW: made.width, serverH: made.height, region,
               sameSize, serverBytes: server.length, facadeBytes: facade ? facade.length : 0,
               differing, firstDiff, maxDelta, samples, oneBit, painted: made.painted_pixels,
               serverSteps: made.steps, expectedSteps });
  }
  return out;
})()`;
const results = await evaluate(script);
let allGood = true;
for (const row of results) {
  if (row.error) { console.log(`  ${row.brush.padEnd(14)} 服务端拒绝：${String(row.error).slice(0, 50)}`); allGood = false; continue; }
  const ok = row.sameSize && row.differing === 0 && row.facadeBytes === row.serverBytes;
  if (!ok) allGood = false;
  console.log(`  ${row.brush.padEnd(14)} 服务端 ${row.serverW}×${row.serverH} vs 算出的区域 ${row.region.w}×${row.region.h}` +
    `｜字节 ${row.serverBytes} vs ${row.facadeBytes}｜不同 ${row.differing}` +
    (row.firstDiff >= 0 ? `（首个 @${row.firstDiff}，最大差 ${row.maxDelta}）` : "") + `｜${ok ? "**逐字节相同** ✓" : "有差异 ✗"}`);
  console.log(`      覆盖：hardness=${JSON.stringify(process.env.HARDNESS || null)} opacity=${JSON.stringify(process.env.OPACITY || null)} shift=${JSON.stringify(process.env.SHIFT || 0)}`);
  console.log(`      枚数：服务端 steps=${row.serverSteps} vs 门面规则算得 ${row.expectedSteps}` +
    `｜${row.serverSteps === row.expectedSteps ? "相同 ✓" : "**不同** ✗ ⇒ 抽取顺序/枚数就是差异来源 ✓"}`);
  if (!ok && row.oneBit) {
    console.log(`      差值分布：差 1 的通道 ${row.oneBit.one} ✓、≤8 的 ${row.oneBit.small}、>8 的 ${row.oneBit.big}`);
    for (const sample of row.samples || []) {
      console.log(`      首个不同像素 (${sample.x}, ${sample.y}) 通道 ${sample.channel}：服务端 ${sample.server} vs 门面 ${sample.facade}`);
    }
  }
}
console.log(allGood ? "结论：预览逐字节相同 ✓" : "结论：存在差异 ✗");
ws.close();
process.exit(allGood ? 0 : 1);
