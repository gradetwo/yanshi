#!/usr/bin/env node
// **像素权威的四条**静态**守卫**（2026-11-05 ✓）
//
// **为什么需要它** ✗：这一轮花了很久才定位「拖动时看得见、抬笔就白」：
//   四个根因**都不是语法错**，而是**调用关系**错 —— 静态审查看不出来，
//   只有真跑 client 模式 + CDP 才看见（`browser-live-brush` 那时还是 known-red ✓）。
//   ⇒ 把「四条必须保持的关系」写成**机检**，以后再改这条链路时**先看它红不红** ✓。
//
// **断言（都能红 ✓）**：
//   ① `drawKernelRegion` 的异步回调里**不得**再 `requestRepaint()`
//      （否则 drawKernelRegion → 异步 → repaint → drawKernelRegion 自激，
//       实测 1.2 s 71 次整幅 putImageData，并把正在拖的本地笔迹盖掉）；
//   ② heavy 文档必须**直接**走服务端像素（`needsServerPixels && heavyDocument` 早返回），
//      否则内核先画一层空白补丁 ⇒ 中间一帧白板；
//   ③ 历史步进与 `revert`/`reapply` 之后必须**放行空白**（`allowBlank: true`），
//      否则「防闪烁」守卫会把服务端正确的空白当成"还没渲染"跳过 ⇒ 画布留旧笔迹；
//   ④ `tiles` 失效事件必须**重同步**（`resync`），否则外部撤销/改动只改状态栏、
//      画布不更新（实测 API undo 后服务端对象 0、画布墨 1247）。
//
// **变异（证明尺子能红）**：`--self-test` 用四段「坏样本」逐条喂进判断函数，
//   每条都必须在**对应**的那一条上报红（且不误报其它条）。
//
// 退出码：0 = 四条都成立｜1 = 有断言红｜3 = 前提不成立（文件/锚点找不到）
import { readFileSync } from "node:fs";

const FILE = "crates/yanshi-http/assets/viewer-app.js";

/** 取一个锚点之间的片段；找不到返回 null。 */
function sliceBetween(source, begin, end) {
  const start = source.indexOf(begin);
  if (start < 0) return null;
  const stop = source.indexOf(end, start);
  if (stop < 0) return null;
  return source.slice(start, stop);
}

/** **① 异步回调里没有再 requestRepaint** */
export function asyncCallbackHasNoSelfRepaint(source) {
  const block = sliceBetween(source, 'if (typeof asyncFn === "function")', "const rgbaView = state.kernel.render_region_rgba");
  if (block === null) return { ok: false, reason: "找不到 drawKernelRegion 的异步分支（锚点变了）", premise: true };
  const hasPaint = block.includes("paintRegionPixels(");
  const hasRepaint = /requestRepaint\s*\(/.test(block);
  return {
    ok: hasPaint && !hasRepaint,
    reason: hasPaint
      ? (hasRepaint ? "异步回调里又调了 requestRepaint ⇒ 自激循环" : "")
      : "异步回调里没有 paintRegionPixels（结果没人画）",
  };
}

/** **② heavy 直接走服务端** */
export function heavyPathSkipsKernel(source) {
  const block = sliceBetween(source, "function drawKernelRegion(", "if (!state.kernel || !state.wasm)");
  if (block === null) return { ok: false, reason: "找不到 drawKernelRegion 的 heavy 分支（锚点变了）", premise: true };
  const ok = /needsServerPixels\s*&&\s*heavyDocument/.test(block) && /queueServerBlit\(/.test(block);
  return { ok, reason: ok ? "" : "heavy 文档没有直接走 queueServerBlit（会先画一层空白补丁）" };
}

/** **③ 历史步进 / revert 放行空白** */
export function historyStepAllowsBlank(source) {
  const step = sliceBetween(source, "async function refreshAfterHistoryStep(", "void refreshHistory()");
  if (step === null) return { ok: false, reason: "找不到 refreshAfterHistoryStep（锚点变了）", premise: true };
  const revert = sliceBetween(source, 'if (event.kind === "revert"', "}") ;
  const stepOk = /queueServerBlit\([^)]*allowBlank:\s*true/.test(step);
  const revertOk = revert !== null && /queueServerBlit\([^)]*allowBlank:\s*true/.test(revert);
  return {
    ok: stepOk && revertOk,
    reason: stepOk && revertOk ? "" : `空白没有放行（历史步进=${stepOk}，revert=${revertOk}）`,
  };
}

/** **④ tiles 事件必须 resync** */
export function tilesEventResyncs(source) {
  const block = sliceBetween(source, 'message.event.event === "tiles"', 'message.event.event === "thumbnail"');
  if (block === null) return { ok: false, reason: "找不到 tiles 事件分支（锚点变了）", premise: true };
  const ok = /resync\s*\(/.test(block);
  return { ok, reason: ok ? "" : "tiles 只改状态/日志，没有 resync ⇒ 外部改动画布不更新" };
}

const CHECKS = [
  ["① 异步回调没有再 requestRepaint（不自激）", asyncCallbackHasNoSelfRepaint],
  ["② heavy 文档直接取服务端像素", heavyPathSkipsKernel],
  ["③ 历史步进／revert 放行空白", historyStepAllowsBlank],
  ["④ tiles 失效事件触发 resync", tilesEventResyncs],
];

const GOOD_SOURCE = [
  "function drawKernelRegion(x, y, w, h, allowBlank = false) {",
  "  if (needsServerPixels && heavyDocument) { return queueServerBlit([x, y, w, h]); }",
  "  if (!state.kernel || !state.wasm) { return queueServerBlit([x,y,w,h]); }",
  '  if (typeof asyncFn === "function") {',
  "    return Promise.resolve().then(() => asyncFn()).then((view) => {",
  "      paintRegionPixels(view, x0, y0, cw, ch, vx, vy, w, h, started);",
  "    });",
  "  }",
  "  const rgbaView = state.kernel.render_region_rgba(x0, y0, cw, ch);",
  "}",
  "async function refreshAfterHistoryStep() { await queueServerBlit(null, { allowBlank: true }); void refreshHistory(); }",
  "socket.onmessage = (event) => {",
  '  if (message.event.event === "tiles") { setStatus({ dirty: 1 }); void resync(); }',
  '  else if (message.event.event === "thumbnail") {}',
  '  if (event.kind === "revert") { needsServerPixels = true; void queueServerBlit(null, { allowBlank: true }); }',
  "};",
].join("\n");

const BAD_SOURCE = GOOD_SOURCE
  .replace("needsServerPixels && heavyDocument", "needsServerPixels")
  .replace("      paintRegionPixels(view, x0, y0, cw, ch, vx, vy, w, h, started);", "      paintRegionPixels(view, x0, y0, cw, ch, vx, vy, w, h, started); requestRepaint();")
  .replace(", { allowBlank: true }", "")
  .replace(" void resync();", "");

if (process.argv.includes("--self-test")) {
  let failures = 0;
  for (const [label, fn] of CHECKS) {
    const badResult = fn(BAD_SOURCE);
    const goodResult = fn(GOOD_SOURCE);
    const redOk = badResult.ok === false && !badResult.premise;
    const greenOk = goodResult.ok === true;
    if (!redOk || !greenOk) {
      failures += 1;
      console.error(`  ✗ 自检：${label}（坏样本红=${redOk} 好样本绿=${greenOk}）`);
    }
  }
  if (failures) process.exit(1);
  console.log(`  ✓ 自检：${CHECKS.length} 条都能在坏样本上红、在好样本上绿`);
  process.exit(0);
}

let source;
try {
  source = readFileSync(FILE, "utf8");
} catch (error) {
  console.error(`⊘ 不可判：读不到 ${FILE}：${error.message}`);
  process.exit(3);
}

let red = 0;
let premiseFailed = false;
for (const [label, fn] of CHECKS) {
  const result = fn(source);
  if (result.premise) {
    console.error(`  ⊘ ${label}：${result.reason}`);
    premiseFailed = true;
    continue;
  }
  console.log(`  ${result.ok ? "✓" : "✗"} ${label}${result.ok ? "" : `（${result.reason}）`}`);
  if (!result.ok) red += 1;
}
if (red > 0) {
  console.error(`  ✗ ${red} 条像素权威关系被破坏`);
  process.exit(1);
}
if (premiseFailed) {
  console.error("  ⊘ 有锚点找不到 ⇒ 本次不算绿（退出码 3）");
  process.exit(3);
}
console.log("  ✓ 像素权威四条关系都成立");
process.exit(0);
