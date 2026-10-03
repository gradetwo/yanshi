#!/usr/bin/env node
// **第 53 轮起：按 600ms 节流 + 每帧直接补小块实现** ✓（判据不变 ✓）。
// ⚠ 第 50–52 轮它记录的是"未做到"✗ ——
//   服务端那半边**已经通了** ✓（实测：拖动中提交 2 帧 ✓、抬手后对象仍只有 1 个 ✓），
//   但**画布拿不到新像素** ✗（刚提交后服务端 `render_region` 仍返回旧/空白图 ✓ ——
//   与第 44 轮"服务端空白却盖在有墨的画布上"**同一个缺陷** ✓）。
//   ⇒ 那 ③b 的实现**整段撤回**了 ✗（不发布看不见效果的行为 ✓）；这个探针留着当**入口** ✓：
//   等"提交后短时间取图返回旧图"修掉（判据：提交后立刻取图与 500ms 后取图必须**逐字节相同** ✓），
//   它就会自然转绿 ✓。
//
// **拖动期就用真笔刷效果** ✓ —— 用户第 3 条后半（原话："运笔过程中，先显示的是实心画笔的效果，
// 运笔结束才瞬间换成最终画笔效果，这个能不能一开始就是画笔画的效果，少掉这个突变，例如 spray 这种
// 前后差异就很突兀"✓）。
//
// **判据（都能红 ✓）**：
//   ① **指针还按着的时候**，画布上就已经有墨 ✓，而且**服务端已经有这一笔** ✓
//      （修复前 `.myb` 那条路拖动期**什么都不显示** ✗ ⇒ 两半都红 ✓）；
//   ② 抬手之后**对象只有一个** ✓ —— 拖动期那些帧是被 supersede 掉的 ✓，不是 N 个对象 ✓；
//   ③ **撤销一步**就回到画之前 ✓（中间帧包在 changeset 里 ✓；不包的话要按 N 次 ✗）；
//   ④ 零控制台错误 ✓。
//
// 用法：node scripts/browser-live-brush.mjs "http://127.0.0.1:<port>/?doc=<id>&token=<token>"
const url = process.argv[2];
const debugPort = process.env.CDP_PORT || "9333";
const shotsDir = process.env.SHOT_DIR || "/tmp/yanshi-live-brush";
if (!url) {
  console.error("用法: node scripts/browser-live-brush.mjs <viewer-url>");
  process.exit(2);
}
const parsed = new URL(url);
const docId = parsed.searchParams.get("doc");
const token = parsed.searchParams.get("token");
const api = async (tool, args) => {
  const response = await fetch(`${parsed.origin}/api/tools?doc=${docId}&token=${token}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ tool, arguments: args || {} }),
  });
  return response.json();
};
const list = await fetch(`http://127.0.0.1:${debugPort}/json/list`).then((r) => r.json());
const target =
  list.find((t) => t.type === "page" && t.url.startsWith(parsed.origin + parsed.pathname)) ||
  list.find((t) => t.type === "page");
if (!target) {
  console.error(`未找到调试目标（Chromium 是否以 --remote-debugging-port=${debugPort} 启动？）`);
  process.exit(2);
}
const ws = new WebSocket(target.webSocketDebuggerUrl);
let id = 1;
const pending = new Map();
const consoleLines = [];
ws.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);
  if (message.method === "Runtime.consoleAPICalled") {
    consoleLines.push((message.params.args || []).map((a) => a.value ?? "").join(" "));
  }
  if (message.method === "Runtime.exceptionThrown") {
    const details = message.params.exceptionDetails || {};
    consoleLines.push("exception: " + (details.exception?.description || details.text || ""));
  }
  if (message.id && pending.has(message.id)) {
    pending.get(message.id)(message);
    pending.delete(message.id);
  }
});
await new Promise((resolve) => ws.addEventListener("open", resolve));
const send = (method, params = {}) =>
  new Promise((resolve) => {
    const current = id++;
    pending.set(current, resolve);
    ws.send(JSON.stringify({ id: current, method, params }));
  });
const evaluate = async (expression) =>
  (await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true })).result
    ?.result?.value;
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const waitFor = async (expression, label, timeoutMs = 30000) => {
  const until = Date.now() + timeoutMs;
  while (Date.now() < until) {
    if (await evaluate(expression)) return true;
    await sleep(200);
  }
  console.error(`  ⏱ 等待超时：${label}`);
  return false;
};
const capture = async (name) => {
  const shot = await send("Page.captureScreenshot", { format: "png" });
  const data = shot.result?.data;
  if (!data) return null;
  const fs = await import("node:fs/promises");
  await fs.mkdir(shotsDir, { recursive: true });
  const path = `${shotsDir}/${name}.png`;
  await fs.writeFile(path, Buffer.from(data, "base64"));
  return path;
};
// **画布指纹** ✓（WYSIWYG 判据用）：在页内对整块画布的 RGBA 做 FNV-1a ✓ ⇒ 一次往返就能比"是不是同一批像素" ✓。
const canvasDigest = () =>
  evaluate(`(() => {
    const board = document.getElementById("board");
    const data = board.getContext("2d").getImageData(0, 0, board.width, board.height).data;
    let hash = 2166136261;
    for (let i = 0; i < data.length; i += 1) { hash ^= data[i]; hash = Math.imul(hash, 16777619) >>> 0; }
    return hash >>> 0;
  })()`);

const inkOnCanvas = () =>
  evaluate(`(() => {
    const board = document.getElementById("board");
    const ctx = board.getContext("2d");
    const data = ctx.getImageData(0, 0, board.width, board.height).data;
    let ink = 0;
    for (let i = 0; i < data.length; i += 4) {
      if (data[i + 3] > 32 && !(data[i] > 245 && data[i + 1] > 245 && data[i + 2] > 245)) ink += 1;
    }
    return ink;
  })()`);

await send("Runtime.enable");
await send("Page.enable");
await send("Page.navigate", { url });
await send("Page.reload", { ignoreCache: true });
await sleep(1600);
consoleLines.length = 0;
await waitFor(
  "document.getElementById('board').width > 400 && window.yanshi.state().serverBlits > 0",
  "画布尺寸与首帧",
  20000,
);
// **先选中"画笔"工具** ✗ —— 默认工作区预设不是它 ✓（本轮实测：不选它 ⇒ 走的是内核覆盖层那条 ✓
// 与"拖动期提交"完全无关 ✓，而画布上照样有墨 ✓ ⇒ 差点又被判据骗过 ✓）。
await evaluate(`(() => {
  const button = document.querySelector('button[data-tool="brush"]');
  if (button) button.click();
  return true;
})()`);
await sleep(400);
// **先等笔刷列表载入，再设值** ✗ —— 列表没载入时设值是**静默失败** ✓（产品侧也一并修了 ✓）。
await waitFor("document.querySelectorAll('#brush option').length > 5", "笔刷列表", 20000);
// **选一支 .myb 笔刷** ✓（spray 最能体现"实心线 vs 真笔刷"的差别 ✓）。
const requestedBrush = process.env.BRUSH || "spray";
// **画笔库的笔刷要走"点行"那条路** ✗ —— 实测：`.myb` 名字在 `#brush` 下拉框里**根本不存在**
// （值始终为空 ✓，只有 `spray` 这类**工具名**在里面 ✓）；而**已验证**的那条探针
//（`scripts/browser-brush-list.mjs` ✓）是用 `#brushLibraryList .brush-lib-row` 点行来选 ✓
// 并用 `highlighted` 校验 ✓。所以这里先试画笔库 ✓，找不到再退回 `setBrush`（覆盖工具名 ✓）。
let pickedFromLibrary = null;
try {
  // **先打开画笔库面板** ✗ —— 实测：面板默认 `hidden` ✓，**行只在打开时才渲染**
  //（`setupBrushLibrary()` 里 `setOpen(true)` 才去拉笔刷列表 ✓，见 `viewer.rs:6843` ✓）。
  await evaluate(`(() => {
    const panel = document.getElementById("brushLibrary");
    const open = document.getElementById("brushLibraryOpen");
    if (panel && panel.hidden && open) open.click();
    return !!(panel && !panel.hidden);
  })()`);
  await waitFor(`window.yanshi.brushLibraryState().rows > 20`, "画笔库列表渲染", 20000);
  pickedFromLibrary = await evaluate(`(() => {
    const wanted = ${JSON.stringify(requestedBrush)};
    const strip = (v) => (v || "").toLowerCase().replace(/\.myb$/, "");
    const rows = Array.from(document.querySelectorAll("#brushLibraryList .brush-lib-row"));
    const row = rows.find((r) => r.dataset.brush === wanted)
      || rows.find((r) => strip(r.dataset.brush) === strip(wanted));
    if (!row) return null;
    row.click();
    return row.dataset.brush;
  })()`);
} catch (error) {
  // **不许默默吞掉** ✗（上一版就是这样 ⇒ 我根本不知道是"行找不到"还是"下拉框没更新" ✗）：
  // 把实际看到的东西打出来 ✓ —— 有几行、`dataset.brush` 长什么样 ✓。
  pickedFromLibrary = null;
  const seen = await evaluate(`(() => {
    const rows = Array.from(document.querySelectorAll("#brushLibraryList .brush-lib-row"));
    return { count: rows.length, sample: rows.slice(0, 4).map((r) => r.dataset.brush || "(无 dataset.brush)") };
  })()`).catch(() => null);
  console.log("  （画笔库入口没走通：" + JSON.stringify(seen) + " ⇒ 退回 setBrush）");
}
if (pickedFromLibrary) {
  await sleep(400);
} else {
  // **没匹配到也要说清** ✗（只在异常分支打印是不够的 ✓ —— 上一版就是这样又盲了一次 ✗）。
  const seen = await evaluate(`(() => {
    const rows = Array.from(document.querySelectorAll("#brushLibraryList .brush-lib-row"));
    return { count: rows.length, sample: rows.slice(0, 5).map((r) => r.dataset.brush || "(无)") };
  })()`).catch(() => null);
  console.log("  （画笔库没匹配到 " + JSON.stringify(requestedBrush) + "：" + JSON.stringify(seen) + "）");
  await evaluate(`window.yanshi.setBrush(${JSON.stringify(requestedBrush)})`);
}
await sleep(600);
const brushName = await evaluate(`document.getElementById("brush").value`);
const toolName = await evaluate(`(window.yanshi.state() || {}).tool || "?"`);
console.log(`  工具=${toolName} 笔刷=${brushName}`);
// **断言不能写死 `spray`** ✗ —— 脚本原本只跑默认笔刷 ✓，我加 `BRUSH=` 时没同步改这里 ✓
// ⇒ 每次换笔刷都被这句误判成"没能选中" ✗（**是我的 bug ✓，不是笔刷选不中 ✓**）。
// 库笔刷（`.myb`）本来就不经过 `#brush` 下拉框（那个框里是工具名 ✓）⇒ 它留空是正常的 ✓，
// 以**落笔结果**为准 ✓。
const wantedBrush = requestedBrush.replace(/\.myb$/, "");
const actualBrush = (brushName || "").replace(/\.myb$/, "");
if (actualBrush !== wantedBrush) {
  if (requestedBrush.endsWith(".myb") && !brushName) {
    console.log("  （库笔刷不经 #brush 下拉框 ⇒ 以落笔结果为准）");
  } else {
    console.error(`❌ 没能选中笔刷：请求「${requestedBrush}」，实测「${brushName}」✗`);
    process.exit(1);
  }
}
const before = await inkOnCanvas();
const objectsBefore = (await api("list_objects", {})).count;

// **慢慢拖** ✓（每步 450ms ⇒ 300ms 的节流一定会触发 ✓）。
await evaluate(`(() => {
  const board = document.getElementById("board");
  const rect = board.getBoundingClientRect();
  window.__livePoints = [];
  const at = (fx, fy) => ({ clientX: rect.left + rect.width * fx, clientY: rect.top + rect.height * fy });
  const fire = (type, point) => board.dispatchEvent(new PointerEvent(type, {
    bubbles: true, cancelable: true, pointerId: 77, pointerType: "mouse",
    isPrimary: true, buttons: type === "pointerup" ? 0 : 1, ...point,
  }));
  window.__fire = fire;
  window.__at = at;
  fire("pointerdown", at(0.3, 0.5));
})()`);
for (const fx of [0.36, 0.42, 0.48, 0.54, 0.6]) {
  await evaluate(`window.__fire("pointermove", window.__at(${fx}, 0.5))`);
  await sleep(450);
}
// ① **还没抬手**：画布上必须已经有墨 ✓ + 服务端已经有这一笔 ✓。
// **先等一会儿** ✗ —— 实时帧的代价是 300–500ms（等布局稳定 → 服务端渲染 → 取图 ✓），
// 抬手前必须给它落地的时间 ✓（第 52 轮就是在"刚发完最后一步就采样"上误判成"看不见"✗）。
await sleep(1200);
const midInk = await inkOnCanvas();
const midObjects = (await api("list_objects", {})).count;
const midState = await evaluate("window.yanshi.state()");
// **失败原因也必须带出来** ✗（第 61 轮的教训 ✓：只有"实时帧 0"这一个数还不够 ✓，分不清
// "根本没被调用" 与 "调用了但抛了" ✓ —— 而这两者的排查方向完全不同 ✓）。
const midDiag = await evaluate(`(() => {
  const stats = window.yanshiStats || {};
  const logText = (document.getElementById("log") || {}).textContent || "";
  return {
    frames: stats.localBrushFrames || 0,
    calls: stats.localBrushCalls || 0,
    errors: stats.localBrushErrors || 0,
    loadFailures: /笔刷本地渲染模块没加载上/.test(logText),
    logTail: logText.slice(-160),
  };
})()`);
console.log(
  `  诊断：本地渲染 调用 ${midDiag.calls} / 出帧 ${midDiag.frames} / 报错 ${midDiag.errors}` +
    `｜模块加载失败过=${midDiag.loadFailures}｜日志尾：${midDiag.logTail}`,
);
console.log(
  `  ① 拖动中（未抬手）：画布墨 ${before} ⇒ ${midInk}｜服务端对象 ${objectsBefore} ⇒ ${midObjects}｜` +
    `实时帧 ${midState.liveStroke ? midState.liveStroke.frames : "—"}`,
);
const shotMid = await capture("live-brush-mid-drag");
if (!(midInk > before + 50)) {
  console.error("❌ 指针还按着的时候画布上没有新墨 ⇒「一开始就是画笔画的效果」没做到 ✗");
  process.exit(1);
}
// **"拖动中服务端必须有对象"这条断言已过时** ✗（第 66 轮 ✓）：它是为旧的
// "拖动中节流发真 `brush_stroke`"那套设计写的 ✓ —— 而那套正是被 300–500ms 往返打败的 ✓。
// 现在拖动期由**本地 wasm** 画（门面与服务端逐字节相同 ✓）⇒ **服务端此刻本来就不该有对象** ✓；
// 对象在**抬手提交**时才出现 ✓（判据 ② 仍然验它 ✓）。⇒ 这里改成**打印**，不再当失败条件 ✓。
console.log(
  `  （拖动中服务端对象 ${objectsBefore} ⇒ ${midObjects}：本地渲染方案下**预期为 0** ✓，抬手才提交 ✓）`,
);
// ② 抬手 ⇒ 提交最终一笔；对象必须仍然只有一个 ✓
// **把中途的像素留在页内** ✓（只留指纹不够 ✗ —— 定位根因要知道"哪些像素不同、差多少" ✓）。
await evaluate(`(() => {
  const board = document.getElementById("board");
  window.__midPixels = board.getContext("2d").getImageData(0, 0, board.width, board.height).data.slice();
  return true;
})()`);
const wysiwygMidDigest = await canvasDigest();
const wysiwygMidInk = await inkOnCanvas();
await evaluate(`window.__fire("pointerup", window.__at(0.6, 0.5))`);
await waitFor("!window.yanshi.state().liveStroke", "实时笔触收尾", 20000);
await sleep(1500);
const wysiwygFinalInk = await inkOnCanvas();
const finalObjects = (await api("list_objects", {})).count;
const finalState = await evaluate("window.yanshi.state()");
// **WYSIWYG**：抬手前 vs 服务端回填后，是不是**同一批像素**？
// 非读画布笔刷应当**完全相同**（EXPECT_EXACT=1 时判红）；读画布笔刷预期不同 ⇒ 先把差异**量出来**记档。
await sleep(600);
const wysiwygFinalDigest = await canvasDigest();
const finalInk = await inkOnCanvas();
const wysiwygSame = wysiwygMidDigest === wysiwygFinalDigest && wysiwygMidInk === wysiwygFinalInk;
const diff = await evaluate(`(() => {
  const board = document.getElementById("board");
  const now = board.getContext("2d").getImageData(0, 0, board.width, board.height).data;
  const before = window.__midPixels;
  if (!before || before.length !== now.length) return null;
  let count = 0, maxDelta = 0;
  let minX = 1e9, minY = 1e9, maxX = -1, maxY = -1;
  const width = board.width;
  for (let i = 0; i < now.length; i += 4) {
    let delta = 0;
    for (let c = 0; c < 4; c += 1) delta = Math.max(delta, Math.abs(now[i + c] - before[i + c]));
    if (!delta) continue;
    count += 1;
    maxDelta = Math.max(maxDelta, delta);
    const p = (i / 4) % width, q = Math.floor(i / 4 / width);
    if (p < minX) minX = p; if (p > maxX) maxX = p;
    if (q < minY) minY = q; if (q > maxY) maxY = q;
  }
  // **墨点的平均颜色/不透明度** ✓（中途 vs 最终 ✓）—— 用来区分"整体偏淡/偏浓" ✓ 与"某通道偏移" ✓。
  const meanOf = (data) => {
    let n = 0, r = 0, g = 0, b = 0, a = 0;
    for (let i = 0; i < data.length; i += 4) {
      // **"墨"的判据要和 inkOnCanvas 一致** ✗ —— 画布是不透明白底（alpha 255），
      // 只按 alpha 过滤会把 900×640=576000 个像素全算进来 ✓（实测就是这样，均值全被白底拉平 ✗）。
      if (data[i + 3] <= 32) continue;
      if (data[i] > 245 && data[i + 1] > 245 && data[i + 2] > 245) continue;
      n += 1; r += data[i]; g += data[i + 1]; b += data[i + 2]; a += data[i + 3];
    }
    if (!n) return null;
    return { n, r: +(r / n).toFixed(1), g: +(g / n).toFixed(1), b: +(b / n).toFixed(1), a: +(a / n).toFixed(1) };
  };
  return { count, maxDelta, box: count ? { x: minX, y: minY, w: maxX - minX + 1, h: maxY - minY + 1 } : null,
           meanMid: meanOf(before), meanFinal: meanOf(now),
           canvas: { w: width, h: board.height } };
})()`);
if (diff) {
  console.log(`  ④b 差异：${diff.count} 个像素不同｜最大通道差 ${diff.maxDelta}｜包围盒 ` +
    (diff.box ? `${diff.box.w}×${diff.box.h} @(${diff.box.x},${diff.box.y})` : "无") +
    `｜画布 ${diff.canvas.w}×${diff.canvas.h}`);
  console.log(`  ④c 墨点均值：中途 ${JSON.stringify(diff.meanMid)} vs 最终 ${JSON.stringify(diff.meanFinal)}`);
}
console.log(`  ④ WYSIWYG：中途(指纹 ${wysiwygMidDigest}, 墨 ${wysiwygMidInk}) vs 最终(指纹 ${wysiwygFinalDigest}, 墨 ${wysiwygFinalInk}) ⇒ ` +
  (wysiwygSame ? "**完全相同** ✓" : `**不同** ✗（墨差 ${wysiwygFinalInk - wysiwygMidInk}）`));
if (process.env.EXPECT_EXACT === "1" && !wysiwygSame) {
  console.log("  ✗ 这支笔刷被判为「非读画布」⇒ 中途与最终必须相同（红了）");
  process.exitCode = 1;
}
console.log(
  `  ② 抬手后：画布墨 ${finalInk}｜服务端对象 ${objectsBefore} ⇒ ${finalObjects}｜` +
    `实时帧共 ${finalState.liveStroke ? finalState.liveStroke.frames : "—"}`,
);
if (finalObjects - objectsBefore !== 1) {
  console.error(
    `❌ 这一笔变成了 ${finalObjects - objectsBefore} 个对象 ⇒ 中间帧没有被 supersede ✗（应当只有 1 个 ✓）`,
  );
  await capture("live-brush-many-objects");
  process.exit(1);
}
const shotFinal = await capture("live-brush-after-commit");

// ③ 撤销**一步** ⇒ 回到画之前 ✓
await api("undo_last", { count: 1 });
await sleep(2200);
const undoneInk = await inkOnCanvas();
const undoneObjects = (await api("list_objects", {})).count;
console.log(
  `  ③ 撤销一步后：画布墨 ${undoneInk}（画之前 ${before}）｜服务端对象 ${undoneObjects}`,
);
if (undoneInk > before + 50) {
  console.error(
    `❌ 撤销一步之后画布上还剩 ${undoneInk - before} 个墨像素 ⇒ 中间帧没被收成一个变更集 ✗`,
  );
  await capture("live-brush-undo-incomplete");
  process.exit(1);
}

const errors = consoleLines.filter(
  (line) => /error|uncaught|exception|failed/i.test(line) && !/favicon/i.test(line),
);
console.log(`  ④ 控制台错误：${errors.length}`);
if (errors.length > 0) {
  errors.slice(0, 4).forEach((line) => console.error("     " + line.slice(0, 160)));
  process.exit(1);
}
console.log(
  JSON.stringify(
    {
      ok: true,
      brush: brushName,
      ink: { before, mid: midInk, final: finalInk, undone: undoneInk },
      objects: { before: objectsBefore, mid: midObjects, final: finalObjects, undone: undoneObjects },
      screenshots: [shotMid, shotFinal],
    },
    null,
    2,
  ),
);
ws.close();

// **元判据：不同笔刷必须画出不同结果** ✗ —— 实测过一次**假绿** ✓：放宽断言后，
// 四支不同笔刷给出了**完全相同**的指纹与墨量（1045282525 / 1644 ✓）⇒ 说明**笔刷根本没换** ✗，
// 而脚本还是打了"完全相同 ✓" ✓。所以这里把"换了笔刷就该不一样"变成**会红**的一句话 ✓：
// 配合 `BRUSH_A` / `BRUSH_B` 跑两次，把两次的指纹贴进 `DIGEST=` 再比 ✓（由外部循环驱动 ✓）。
if (process.env.DIGEST_NOTE && process.env.PREVIOUS_DIGEST && process.env.PREVIOUS_DIGEST === String(wysiwygFinalDigest)) {
  console.log("  ✗ 与上一次（另一支笔刷）的指纹**相同** ⇒ 笔刷很可能没真的换 ✗（这次的测量无效）");
  process.exitCode = 1;
}
