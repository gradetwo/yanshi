#!/usr/bin/env node
// ⚠ **这个探针现在是红的 ✗，而且它就该是红的** ✓ —— 它是第 50 轮那个**卡点**的记录 ✓：
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
await evaluate(`window.yanshi.setBrush("spray")`);
await sleep(600);
const brushName = await evaluate(`document.getElementById("brush").value`);
const toolName = await evaluate(`(window.yanshi.state() || {}).tool || "?"`);
console.log(`  工具=${toolName} 笔刷=${brushName}`);
if (brushName !== "spray") {
  console.error(`❌ 没能选中 .myb 笔刷（实测「${brushName}」）✗`);
  process.exit(1);
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
// ① **还没抬手**：画布上必须已经有墨 ✓ + 服务端已经有这一笔 ✓
const midInk = await inkOnCanvas();
const midObjects = (await api("list_objects", {})).count;
const midState = await evaluate("window.yanshi.state()");
console.log(
  `  ① 拖动中（未抬手）：画布墨 ${before} ⇒ ${midInk}｜服务端对象 ${objectsBefore} ⇒ ${midObjects}｜` +
    `实时帧 ${midState.liveStroke ? midState.liveStroke.frames : "—"}`,
);
const shotMid = await capture("live-brush-mid-drag");
if (!(midInk > before + 50)) {
  console.error("❌ 指针还按着的时候画布上没有新墨 ⇒「一开始就是画笔画的效果」没做到 ✗");
  process.exit(1);
}
if (!(midObjects > objectsBefore)) {
  console.error("❌ 拖动中服务端还没有这一笔 ⇒ 拖动期提交没生效 ✗");
  process.exit(1);
}
// ② 抬手 ⇒ 提交最终一笔；对象必须仍然只有一个 ✓
await evaluate(`window.__fire("pointerup", window.__at(0.6, 0.5))`);
await waitFor("!window.yanshi.state().liveStroke", "实时笔触收尾", 20000);
await sleep(1500);
const finalInk = await inkOnCanvas();
const finalObjects = (await api("list_objects", {})).count;
const finalState = await evaluate("window.yanshi.state()");
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
