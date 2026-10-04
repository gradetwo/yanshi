#!/usr/bin/env node
// 在真实 Chromium 里测量「拖动笔迹」的成本：走真实 pointer 事件与真实重绘路径。
//
// 关键点：**只测同步处理耗时**（dispatchEvent 返回即代表处理器完成）。
// 若在每段后 await requestAnimationFrame，量到的主要是 60fps 帧边界（~16.7ms），
// 会把真实工作量（~1ms）淹没 —— 第一版探针就踩了这个坑。
//
// 前置条件同 browser-pixel-check.mjs；用法：
//   node scripts/browser-drag-perf.mjs "http://127.0.0.1:8110/?doc=myDoc&token=..." [段数]

const url = process.argv[2];
const segments = Number(process.argv[3] || 20);
// **`Number(...)` 必须包在 `${}` 里** ✓：它在**普通字符串**里只是字面文本 ✗ ⇒ 拼出来的 URL 是
// `http://127.0.0.1:Number(process.env.CDP_PORT || 9333)/json/list` ✗ ⇒ 取目标列表必失败 ✓
//（这是我早先"去掉硬编码端口"时留下的 ✗ —— **改完必须看拼出来的东西** ✓）。
const list = await fetch(`http://127.0.0.1:${Number(process.env.CDP_PORT || 9333)}/json/list`).then((r) => r.json());
let target = list.find((t) => t.type === "page" && t.url.includes("127.0.0.1:8110"));
if (!target) {
  // **同一处的第二个实例** ✗（第 534 轮 ✓）：`:16` 在更早一轮修成了模板串 ✓，
  // 而这一行**仍是普通引号 + 拼接** ✗ ⇒ 端口表达式成了**字面文本** ✓
  // ⇒ 实测 `ERR_INVALID_URL` ✓（`input` 里就是那段字面文本 ✓）。
  // ⇒ 这正是"改一处之前先搜一遍"（第 452 轮立的规矩 ✓）当时漏掉的那一处 ✓。
  const created = await fetch(`http://127.0.0.1:${Number(process.env.CDP_PORT || 9333)}/json/new?` + encodeURIComponent(url), { method: "PUT" }).then((r) => r.json());
  target = created;
  await new Promise((r) => setTimeout(r, 3500));
}
const ws = new WebSocket(target.webSocketDebuggerUrl);
let id = 1; const pending = new Map();
ws.addEventListener("message", (e) => { const m = JSON.parse(e.data); if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); } });
await new Promise((r) => ws.addEventListener("open", r));
const send = (method, params = {}) => new Promise((r) => { const i = id++; pending.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
const evaluate = async (expr) => (await send("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true })).result?.result?.value;
await send("Runtime.enable");
await send("Page.navigate", { url });
await new Promise((r) => setTimeout(r, 6000)); // 等首帧与服务端渲染就绪

const result = await evaluate(`(async () => {
  const board = document.getElementById('board');
  if (!board) return { error: 'no canvas' };
  const rect = board.getBoundingClientRect();
  const startX = rect.left + rect.width * 0.2;
  const startY = rect.top + rect.height * 0.3;
  const stepX = (rect.width * 0.5) / ${segments};
  const fire = (type, x, y, buttons) => board.dispatchEvent(new PointerEvent(type, {
    clientX: x, clientY: y, buttons, bubbles: true, cancelable: true,
    pointerId: 1, pointerType: 'mouse', isPrimary: true,
  }));
  // 选画笔工具（若 UI 有对应按钮就点一下）
  const brush = [...document.querySelectorAll('button')].find((b) => /笔|brush/i.test(b.textContent || ''));
  if (brush) brush.click();
  await new Promise((r) => requestAnimationFrame(r));
  // 只测**同步处理耗时**：不等 rAF（等 rAF 量到的是 16.7ms 帧边界，不是工作量）。
  const costs = [];
  fire('pointerdown', startX, startY, 1);
  for (let i = 1; i <= ${segments}; i++) {
    const t0 = performance.now();
    fire('pointermove', startX + stepX * i, startY + (i % 4) * 3, 1);
    costs.push(performance.now() - t0);
  }
  const t1 = performance.now();
  fire('pointerup', startX + stepX * ${segments}, startY, 0);
  const commitMs = performance.now() - t1;
  // 再量一次「等到下一帧可见」的端到端成本（含合成）。
  const t2 = performance.now();
  await new Promise((r) => requestAnimationFrame(r));
  const nextFrameMs = performance.now() - t2;
  const sorted = [...costs].sort((a, b) => a - b);
  return {
    segments: costs.length,
    meanMs: costs.reduce((a, b) => a + b, 0) / costs.length,
    medianMs: sorted[Math.floor(sorted.length / 2)],
    p95Ms: sorted[Math.floor(sorted.length * 0.95)],
    maxMs: sorted[sorted.length - 1],
    firstMs: costs[0],
    commitMs,
    nextFrameMs,
  };
})()`);
console.log(JSON.stringify(result, null, 2));
ws.close();
