#!/usr/bin/env node
// **笔刷预览的浏览器验收** ✓（用户："201 支笔刷只有一个名字 ⇒ 选笔全凭猜，**web 上也是**" ✗）。
//
// 它走的是**编辑器自己的真实路径** ✓（与真人一样：装载笔刷 → 选笔 → 看图 ✓），
// 不另写一套比对逻辑 ✗。
//
// 判据四条（每条都能红 ✓）：
//   ① **选中一支笔刷后，`#brushPreview` 真的加载了一张图** ✓（`naturalWidth > 0` ✓）
//      —— 写成"src 设了但图是坏的 / 留个空框" ✗ 会被抓住 ✓；
//   ② **换一支笔刷 ⇒ 图必须换** ✓（src 改变 ✓ + 新图**也真的加载成功** ✓）
//      —— "预览永远画同一支" ✗ 会被抓住 ✓；
//   ③ **换颜色 ⇒ 图也必须换** ✓ —— 这条证明 **Web 把工具条上的颜色真的传进了同一个工具** ✓
//      （此前是 `color: undefined` ✗ ⇒ 界面里改了颜色、预览/落笔都不动 ✗）；
//   ④ **全程没有控制台错误** ✓。
//
// 前置：服务端在跑（`scripts/serve.sh` ✓）+ 带远程调试的 Chromium：
//   chromium --remote-debugging-port=9333 --headless=new about:blank
// 用法：node scripts/browser-brush-preview.mjs "http://127.0.0.1:8110/?doc=ui&token=<token>"
const url = process.argv[2];
const debugPort = process.env.CDP_PORT || "9333";
if (!url) {
  console.error("用法: node scripts/browser-brush-preview.mjs <viewer-url>");
  process.exit(2);
}
const shotsDir = process.env.SHOT_DIR || "/tmp/yanshi-brush-preview";

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
    consoleLines.push(
      (message.params.args || []).map((arg) => arg.value ?? "").join(" "),
    );
  }
  if (message.method === "Runtime.exceptionThrown") {
    consoleLines.push("exception: " + JSON.stringify(message.params.exceptionDetails?.text || ""));
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
const waitFor = async (expression, label, timeoutMs = 30000) => {
  const until = Date.now() + timeoutMs;
  while (Date.now() < until) {
    if (await evaluate(expression)) return true;
    await new Promise((resolve) => setTimeout(resolve, 150));
  }
  console.error(`  ⏱ 等待超时：${label}`);
  return false;
};
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
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
await sleep(1200);
if (!(await waitFor("typeof window.yanshi === 'object' && !!document.getElementById('brush')", "查看器就绪"))) {
  process.exit(3);
}
// **笔刷列表** ✓：与"点开下拉"同一条实现 ✓（`loadBrushes` ✓）。
await evaluate("window.yanshi.loadBrushes()");
if (!(await waitFor("document.getElementById('brush').options.length > 2", "笔刷列表装载"))) {
  process.exit(3);
}
const brushes = await evaluate(
  "Array.from(document.getElementById('brush').options).map((o) => o.value).filter(Boolean)",
);
if (!brushes || brushes.length < 2) {
  console.error(`❌ 笔刷不足两支，无法证明"预览跟着笔刷走"：${JSON.stringify(brushes)}`);
  process.exit(1);
}
// **挑两支"能画的"** ✓（已知会出墨的 ✓ —— 涂抹类在空画布上本来就该被拒绝 ✓）。
const preferred = ["2B_pencil", "100%_Opaque", "classic-pen", "deevad-2B_pencil"];
const picked = preferred.filter((name) => brushes.includes(name));
const brushA = picked[0] || brushes[0];
const brushB = picked[1] || brushes.find((name) => name !== brushA);

const previewState = async () =>
  evaluate(`(() => {
    const img = document.getElementById('brushPreview');
    return {
      src: img.getAttribute('src') || '',
      complete: img.complete,
      width: img.naturalWidth,
      height: img.naturalHeight,
      visible: img.style.display !== 'none',
      hint: (document.getElementById('brushPreviewHint') || {}).textContent || '',
      dataset: (img.dataset && img.dataset.key) || '',
    };
  })()`);

// ① 第一支笔刷 ⇒ 真的加载出一张图 ✓
await evaluate(`window.yanshi.setBrush(${JSON.stringify(brushA)})`);
const loadedA = await waitFor(
  `(() => { const img = document.getElementById('brushPreview');
            return !!img.getAttribute('src') && img.complete && img.naturalWidth > 0; })()`,
  `「${brushA}」的预览图加载`,
);
const stateA = await previewState();
console.log(`  ① ${brushA}: loaded=${loadedA} ${stateA.width}×${stateA.height} hint=${stateA.hint}`);
if (!loadedA) {
  console.error("❌ 选中笔刷后预览图没有真的加载出来（空框 / 坏图）");
  await capture("brush-preview-failed");
  process.exit(1);
}
const shotA = await capture(`brush-preview-${brushA.replace(/[^\w.-]/g, "_")}`);

// ② 换一支 ⇒ 图必须换 ✓（而且新图也要真的加载 ✓）
await evaluate(`window.yanshi.setBrush(${JSON.stringify(brushB)})`);
const loadedB = await waitFor(
  `(() => { const img = document.getElementById('brushPreview');
            return !!img.getAttribute('src') && img.complete && img.naturalWidth > 0
                   && img.getAttribute('src') !== ${JSON.stringify(stateA.src)}; })()`,
  `「${brushB}」的预览图换成另一张`,
);
const stateB = await previewState();
console.log(`  ② ${brushB}: loaded=${loadedB} ${stateB.width}×${stateB.height} src-changed=${stateB.src !== stateA.src}`);
if (!loadedB || stateB.src === stateA.src) {
  console.error("❌ 换笔刷没有换预览（或者新图加载失败）");
  process.exit(1);
}
const shotB = await capture(`brush-preview-${brushB.replace(/[^\w.-]/g, "_")}`);

// ③ 换颜色 ⇒ 图也必须换 ✓（证明"工具条上的颜色真的传进了这个工具" ✓）
await evaluate(`window.yanshi.setColor('#00ff00')`);
await evaluate("window.yanshi.previewBrush()");
const loadedC = await waitFor(
  `(() => { const img = document.getElementById('brushPreview');
            return !!img.getAttribute('src') && img.complete && img.naturalWidth > 0
                   && img.getAttribute('src') !== ${JSON.stringify(stateB.src)}; })()`,
  "换色后的预览图",
);
const stateC = await previewState();
console.log(`  ③ 换色: loaded=${loadedC} src-changed=${stateC.src !== stateB.src} hint=${stateC.hint}`);
if (!loadedC || stateC.src === stateB.src) {
  console.error("❌ 换了颜色，预览没变 ⇒ Web 没把颜色传进工具层");
  process.exit(1);
}
const shotC = await capture("brush-preview-green");

// ④ 控制台
const errors = consoleLines.filter(
  (line) => /error|uncaught|exception|failed/i.test(line) && !/favicon/i.test(line),
);

// ⑤ **真实拖动两笔 ⇒ 画面里必须出现两种工具条颜色** ✓
//    —— 这条走的正是用户那条路：**选色 → 画布拖动 → 服务端落笔** ✓。
//    此前查看器落笔时写的是 `color: undefined` ✗ ⇒ **选了色也画不上** ✓（界面里有的东西不生效 ✗）。
//
//    判据用"**两个不同的输入 ⇒ 两个不同的输出**" ✓：
//    红笔画在 y=0.35、蓝笔画在 y=0.65 ✓ ⇒ **红带里只许有红、蓝带里只许有蓝** ✓。
//    （**不能**只看"画完有红像素" ✗ —— 上一轮的笔迹会把它骗过 ✓，老坑 ✓；
//      也不能只看"红像素有没有变多" ✗ —— 同一笔重画在同一处 ⇒ 像素**逐字节相同** ✓。）
const docId = new URL(url).searchParams.get("doc") || "default";
const token = new URL(url).searchParams.get("token") || "";
const callTool = async (tool, args) =>
  fetch(`http://127.0.0.1:${new URL(url).port}/api/tools?doc=${docId}&token=${token}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ tool, arguments: args }),
  }).then((response) => response.json());

const countBand = async (fromY, toY) => {
  const raw = await callTool("render_region", {
    region: { x: 0, y: fromY, w: 400, h: toY - fromY },
    raw: true,
  });
  if (!raw || !raw.ok || !raw.raw_url) return null;
  const bytes = new Uint8Array(
    await fetch(`http://127.0.0.1:${new URL(url).port}${raw.raw_url}`).then((r) => r.arrayBuffer()),
  );
  let red = 0;
  let blue = 0;
  for (let index = 0; index + 3 < bytes.length; index += 4) {
    const [r, g, b, a] = [bytes[index], bytes[index + 1], bytes[index + 2], bytes[index + 3]];
    if (a < 40) continue;
    if (r > 150 && r > g + 60 && r > b + 60) red += 1;
    if (b > 150 && b > r + 60 && b > g + 60) blue += 1;
  }
  return { red, blue };
};

const dragStroke = async (fractionY, pointerId) =>
  evaluate(`(async () => {
    const board = document.getElementById("board");
    const rect = board.getBoundingClientRect();
    const at = (fx) => ({ clientX: rect.left + rect.width * fx, clientY: rect.top + rect.height * ${fractionY} });
    const fire = (type, point) => board.dispatchEvent(new PointerEvent(type, {
      bubbles: true, cancelable: true, pointerId: ${pointerId}, pointerType: "mouse",
      isPrimary: true, buttons: type === "pointerup" ? 0 : 1, ...point,
    }));
    fire("pointerdown", at(0.25));
    await new Promise((r) => setTimeout(r, 60));
    for (const fx of [0.35, 0.45, 0.55]) { fire("pointermove", at(fx)); await new Promise((r) => setTimeout(r, 60)); }
    fire("pointerup", at(0.55));
  })()`);

await evaluate("window.yanshi.setTool('brush')");
await evaluate(`window.yanshi.setBrush(${JSON.stringify(brushB)})`);
await evaluate("window.yanshi.setColor('#ff0000')");
await dragStroke(0.35, 11);
await sleep(2000);
await evaluate("window.yanshi.setColor('#0000ff')");
await dragStroke(0.65, 12);
await sleep(2000);

const redBand = await countBand(90, 120);
const blueBand = await countBand(195, 225);
console.log(
  `  ⑤ 落笔颜色：红带(红 ${redBand && redBand.red} / 蓝 ${redBand && redBand.blue}) ` +
    `蓝带(红 ${blueBand && blueBand.red} / 蓝 ${blueBand && blueBand.blue})`,
);
if (
  !redBand ||
  !blueBand ||
  !(redBand.red > 0) ||
  !(blueBand.blue > 0) ||
  redBand.blue !== 0 ||
  blueBand.red !== 0
) {
  console.error("❌ 红蓝两笔的颜色没有各就各位 ⇒ 工具条上的颜色没落到画面里");
  await capture("brush-paint-colour-failed");
  process.exit(1);
}
const shotPaint = await capture("brush-paint-two-colours");

console.log(`  ④ 控制台错误：${errors.length}`);
if (errors.length > 0) {
  errors.slice(0, 6).forEach((line) => console.error("     " + line.slice(0, 160)));
  process.exit(1);
}
console.log(
  JSON.stringify(
    {
      ok: true,
      brushA,
      brushB,
      previewA: { width: stateA.width, height: stateA.height, screenshot: shotA },
      previewB: { width: stateB.width, height: stateB.height, screenshot: shotB },
      previewColour: { screenshot: shotC },
      paintedTwoColours: { redBand, blueBand, screenshot: shotPaint },
      consoleErrors: errors.length,
    },
    null,
    2,
  ),
);
ws.close();
