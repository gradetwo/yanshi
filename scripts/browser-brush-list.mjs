#!/usr/bin/env node
// **笔刷库：列表里直接带效果图** ✓ —— 真实用户实测诉求（原文 ✓）：
// "笔刷这个列表里头都带个笔刷的效果图是不是更好，直接列表中就能找到想要的" ✓。
//
// **判据（都能红 ✓）**：
//   ① 打开面板 ⇒ 必须列出**一批**笔刷（> 20 行 ✓；修复前根本没有这个面板 ✗）；
//   ② **懒加载真的加载了图** ✓：滚进视口的行必须有 `src` ✓，而且**至少 3 张互不相同** ✓
//      （没接上 ⇒ `loaded` 一直是 0 ✗；拿占位图糊弄 ⇒ `distinctPreviews` = 1 ✗）；
//   ③ **点一行就换那支笔** ✓：`#brush` 的值与面板高亮都必须是那一行 ✓（只改一个 ⇒ 红 ✓）；
//   ④ 零控制台错误 ✓。
//
// 用法：node scripts/browser-brush-list.mjs "http://127.0.0.1:<port>/?doc=<id>&token=<token>"
const url = process.argv[2];
const debugPort = process.env.CDP_PORT || "9333";
const shotsDir = process.env.SHOT_DIR || "/tmp/yanshi-brush-list";
if (!url) {
  console.error("用法: node scripts/browser-brush-list.mjs <viewer-url>");
  process.exit(2);
}
const list = await fetch(`http://127.0.0.1:${debugPort}/json/list`).then((r) => r.json());
const viewerBase = url.split("?")[0];
const target =
  list.find((t) => t.type === "page" && t.url.startsWith(viewerBase)) ||
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

await send("Runtime.enable");
await send("Page.enable");
await send("Page.navigate", { url });
await send("Page.reload", { ignoreCache: true });
// **睡眠已删** ✓（第 900 轮 ✓）：下面 waitFor 已在正确地等 ✓，且等不到会 exit ✓。
// **从这里开始才算我们的** ✓ —— 浏览器标签里可能还留着**上一次运行**的旧页面 ✓
//（实测：它留下的 `ReferenceError: setupBrushLibrary is not defined` 被算到了本轮头上 ✗）。
consoleLines.length = 0;
if (!(await waitFor("typeof window.yanshi === 'object' && !!document.getElementById('brushLibrary')", "查看器就绪"))) {
  process.exit(3);
}

// ① 打开 ⇒ 必须有一批行 ✓（修复前没有这个面板 ✓）
await evaluate("window.yanshi.openBrushLibrary()");
await waitFor("window.yanshi.brushLibraryState().rows > 20", "笔刷列表渲染", 20000);
let state = await evaluate("window.yanshi.brushLibraryState()");
console.log(`  ① 打开笔刷库：${state.rows} 行（open=${state.open}）｜提示：${state.hint}`);
if (!state.open || state.rows <= 20) {
  console.error(`❌ 笔刷库没打开或没有列出笔刷（rows=${state.rows}）`);
  await capture("brush-library-empty");
  process.exit(1);
}

// ② 懒加载出图，且图必须**互不相同** ✓
await waitFor("window.yanshi.brushLibraryState().loaded >= 5", "可见行的预览图", 25000);
await waitFor("window.yanshi.brushLibraryState().distinctPreviews >= 3", "预览图互不相同", 25000);
state = await evaluate("window.yanshi.brushLibraryState()");
console.log(
  `  ② 预览：已加载 ${state.loaded}/${state.rows} 行，互不相同 ${state.distinctPreviews} 张` +
    `（服务端落笔 ${state.previews} 次，失败 ${state.errors} 次）`,
);
if (state.loaded < 5) {
  console.error("❌ 没有懒加载出预览图 ⇒ 列表里还是只有名字 ✗");
  await capture("brush-library-no-previews");
  process.exit(1);
}
if (state.distinctPreviews < 3) {
  console.error(
    `❌ 预览图只有 ${state.distinctPreviews} 种 ⇒ 不是"每支笔刷真实落一小笔" ✗（占位图/同一张图）`,
  );
  await capture("brush-library-same-previews");
  process.exit(1);
}
// **再加一条更硬的** ✓：前两张预览图的**字节**必须不同 ✓
//（只比 URL 会被"同一张图、不同 blob 地址"骗过 ✗ —— 这与本项目"判据要能红"的规矩一致 ✓）。
const bytesDiffer = await evaluate(`(async () => {
  const rows = Array.from(document.querySelectorAll("#brushLibraryList .brush-lib-row"));
  // **必须挑两支不同的笔刷** ✓ —— 第一版按 DOM 顺序取前两张 ✓，正好取到"同一支笔的两个入口"✗
  //（那时列表里还有重复行 ✓）⇒ 报出"字节相同"✗ —— **是判据错了，不是产品错了** ✓（已修两处 ✓）。
  const byName = new Map();
  for (const row of rows) {
    const img = row.querySelector("img");
    const src = img ? img.getAttribute("src") : "";
    if (src && !byName.has(row.dataset.brush)) byName.set(row.dataset.brush, src);
  }
  const sources = Array.from(byName.values()).slice(0, 2);
  if (sources.length < 2) return { ok: false, reason: "不足两张" };
  const loaded = await Promise.all(sources.map(async (src) => {
    const buffer = await fetch(src).then((r) => r.arrayBuffer());
    return new Uint8Array(buffer);
  }));
  const [first, second] = loaded;
  const sameLength = first.length === second.length;
  let sameBytes = sameLength;
  if (sameLength) {
    for (let i = 0; i < first.length; i += 1) { if (first[i] !== second[i]) { sameBytes = false; break; } }
  }
  return { ok: !sameBytes, firstLength: first.length, secondLength: second.length };
})()`);
console.log(`  ②b 前两张预览图字节：${JSON.stringify(bytesDiffer)}`);
if (!bytesDiffer || !bytesDiffer.ok) {
  console.error("❌ 前两张预览图的字节相同 ⇒ 那不是「每支笔刷真实落一小笔」✗");
  await capture("brush-library-identical-bytes");
  process.exit(1);
}
const shot = await capture("brush-library-with-previews");

// ③ 点一行 ⇒ 换那支笔 ✓（值 + 高亮必须一致 ✓）
const picked = await evaluate(`(() => {
  const rows = Array.from(document.querySelectorAll("#brushLibraryList .brush-lib-row"));
  const current = document.getElementById("brush").value;
  const row = rows.find((item) => item.dataset.brush !== current);
  if (!row) return null;
  const name = row.dataset.brush;
  row.click();
  return name;
})()`);
await sleep(900);
const afterPick = await evaluate(`(() => {
  const rows = Array.from(document.querySelectorAll("#brushLibraryList .brush-lib-row"));
  const selected = rows.find((row) => row.classList.contains("selected"));
  return {
    select: document.getElementById("brush").value,
    highlighted: selected ? selected.dataset.brush : null,
  };
})()`);
console.log(`  ③ 点了一行「${picked}」⇒ 下拉=${afterPick.select}、面板高亮=${afterPick.highlighted}`);
if (!picked || afterPick.select !== picked || afterPick.highlighted !== picked) {
  console.error("❌ 点一行之后「下拉的值」与「面板高亮」不一致 ⇒ 用户会以为换了笔、其实没换 ✗");
  await capture("brush-library-pick-mismatch");
  process.exit(1);
}
const shotPicked = await capture("brush-library-picked");

// ④ 控制台
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
      rows: state.rows,
      loaded: state.loaded,
      distinctPreviews: state.distinctPreviews,
      previewCalls: state.previews,
      picked: { name: picked, select: afterPick.select, highlighted: afterPick.highlighted },
      screenshots: [shot, shotPicked],
    },
    null,
    2,
  ),
);
ws.close();
