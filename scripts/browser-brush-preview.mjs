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
// **必须强制重新取页面** ✗ —— 实测教训 ✓：调试浏览器里常常已经有同一个 URL 的页面 ✓，
// 只 `navigate` 会命中**内存缓存** ✓ ⇒ 跑的是**旧界面的 JS** ✓
//（现象：服务端明明已经改了 ✓，探针却一直看到旧行为 ✗ —— 与"我改了却没生效"同类 ✓，代价极高 ✓）。
await send("Page.reload", { ignoreCache: true });
await sleep(1500);
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
  // **墨量探针**（第 418 轮 ✓）：数"可见像素"（`a >= 40` ✓），**不看颜色** ✓。
  // 为什么需要它：`红带(红 0 / 蓝 0)` 有三种解释 ✗ —— **没画** ✓ / **画在取样区外** ✓ /
  // **画了但颜色不对**（默认黑 ⇒ 红蓝计数天然为 0 ✓）。**颜色计数分不开这三者** ✗，
  // 而 `ink` 一次就分开：`ink > 0` ⇒ **画了** ✓ ⇒ 问题在**颜色** ✓；`ink == 0` ⇒ **这里没墨** ✓。
  let ink = 0;
  for (let index = 0; index + 3 < bytes.length; index += 4) {
    const [r, g, b, a] = [bytes[index], bytes[index + 1], bytes[index + 2], bytes[index + 3]];
    if (a < 40) continue;
    ink += 1; // **可见像素** ✓（不看颜色 ✓）
    if (r > 150 && r > g + 60 && r > b + 60) red += 1;
    if (b > 150 && b > r + 60 && b > g + 60) blue += 1;
  }
  return { red, blue, ink };
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
  `  ⑤ 落笔颜色：红带(红 ${redBand && redBand.red} / 蓝 ${redBand && redBand.blue} / 墨 ${redBand && redBand.ink}) ` +
    `蓝带(红 ${blueBand && blueBand.red} / 蓝 ${blueBand && blueBand.blue} / 墨 ${blueBand && blueBand.ink})`,
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

// ⑥ **平滑开关真的传下去了** ✓（界面里加的东西必须真的生效 ✗ —— 本项目头号病症 ✓）。
//    走**内置画笔**那条（它落的是 `stroke` 对象 ✓ ⇒ `data.smooth` 直接可查 ✓）。
await evaluate("window.yanshi.setBrush('')");
await evaluate("window.yanshi.setTool('brush')");
await evaluate("window.yanshi.setSmooth(true)");
await sleep(300);
await dragStroke(0.5, 21);
await sleep(2500);
const objects = await callTool("list_objects", {});
const newest = Array.isArray(objects && objects.objects) ? objects.objects.slice(-1)[0] : null;
let smoothFlag = null;
if (newest && newest.object_id) {
  const got = await callTool("get_object", { object_id: newest.object_id });
  smoothFlag = got && got.data ? got.data.smooth : null;
  console.log("  ⑥ 对象详情：points=" + JSON.stringify((got && got.data && got.data.points || []).length) +
    " size=" + (got && got.data && got.data.size) + " smooth=" + smoothFlag);
}
console.log(`  ⑥ 平滑开关：界面 state.smooth=${await evaluate("window.yanshi.state().smooth")}` +
  `  最新对象 ${newest && newest.object_id} 的 data.smooth=${smoothFlag}`);
if (smoothFlag !== true) {
  console.error("❌ 勾了「平滑」，对象里却没有 data.smooth=true ⇒ 开关没传下去");
  await capture("smooth-flag-failed");
  process.exit(1);
}
const shotSmooth = await capture("brush-smooth-flag");

// ⑥b **关掉开关 ⇒ 对象里必须是 false** ✓ —— 这一半才是"能红"的那一半 ✓：
// 只查"勾上 ⇒ true"挡不住"写死成 true" ✗（那正是本轮在查看器里找到的毛病 ✓：
// 有两条客户端路径**把 `smooth: true` 写死** ✓ ⇒ 界面里根本关不掉 ✗）。
await evaluate("window.yanshi.setSmooth(false)");
await sleep(300);
await dragStroke(0.75, 22);
await sleep(2500);
const objectsOff = await callTool("list_objects", {});
const newestOff = Array.isArray(objectsOff && objectsOff.objects) ? objectsOff.objects.slice(-1)[0] : null;
let smoothOff = null;
if (newestOff && newestOff.object_id) {
  const got = await callTool("get_object", { object_id: newestOff.object_id });
  smoothOff = got && got.data ? got.data.smooth : null;
}
console.log(`  ⑥b 开关关掉：最新对象 ${newestOff && newestOff.object_id} 的 data.smooth=${smoothOff}`);
if (smoothOff !== false) {
  console.error("❌ 关掉「平滑」之后对象里仍然不是 false ⇒ 开关要么没传、要么被写死");
  await capture("smooth-off-failed");
  process.exit(1);
}

// ⑦ **一笔多色（Loaded Brush）真的走通了** ✓（用户："花瓣渐变只能分两笔，交界硬" ✗）。
//    走的正是用户那条路：选 `.myb` 画笔 → 勾「一笔多色」→ 选末端色 → 拖一笔 ✓。
//    判据两条：① 对象上的 `source.color_to` **必须存在** ✓（参数真的传下去了 ✓）；
//    ② **同一条笔迹的两端颜色必须不同** ✓（左半偏起点色、右半偏末端色 ✓）——
//    这条能挡住"参数收了但没落进画面" ✗（正是本项目头号病症 ✓）。
await evaluate("window.yanshi.setTool('brush')");
await evaluate(`window.yanshi.setBrush(${JSON.stringify(brushB)})`);
await evaluate("window.yanshi.setColor('#0000ff')");
await evaluate("window.yanshi.setDuoTone(true, '#ff0000')");
await sleep(300);
await dragStroke(0.9, 31);
await sleep(2500);
const objectsDuo = await callTool("list_objects", {});
const newestDuo = Array.isArray(objectsDuo && objectsDuo.objects) ? objectsDuo.objects.slice(-1)[0] : null;
let duoSource = null;
if (newestDuo && newestDuo.object_id) {
  const got = await callTool("get_object", { object_id: newestDuo.object_id });
  duoSource = got && got.data ? got.data.source || null : null;
}
const band = await callTool("render_region", {
  // **量在被画的那一带上** ✓：这一拖在 y≈0.9×300≈270 ✓、x 从 ≈100 到 ≈220 ✓。
  region: { x: 0, y: 250, w: 400, h: 40 },
  raw: true,
});
const counts = { leftBlue: 0, leftRed: 0, rightBlue: 0, rightRed: 0 };
if (band && band.ok && band.raw_url) {
  const bytes = new Uint8Array(
    await fetch(`http://127.0.0.1:${new URL(url).port}${band.raw_url}`).then((r) => r.arrayBuffer()),
  );
  for (let index = 0; index + 3 < bytes.length; index += 4) {
    const [r, g, b, a] = [bytes[index], bytes[index + 1], bytes[index + 2], bytes[index + 3]];
    if (a < 40) continue;
    const column = (index / 4) % 400;
    // **起点侧 / 末端侧** ✓（对着这一笔实际覆盖的 x 范围 ✓）。
    const left = column >= 95 && column < 150;
    const right = column >= 175 && column < 230;
    if (!left && !right) continue;
    if (b > 150 && b > r + 60 && b > g + 60) {
      if (left) counts.leftBlue += 1;
      else counts.rightBlue += 1;
    }
    if (r > 150 && r > g + 60 && r > b + 60) {
      if (left) counts.leftRed += 1;
      else counts.rightRed += 1;
    }
  }
}
console.log(
  `  ⑦ 一笔多色：对象 source.color_to=${duoSource && JSON.stringify(duoSource.color_to)}` +
    `  左半(蓝 ${counts.leftBlue}/红 ${counts.leftRed}) 右半(蓝 ${counts.rightBlue}/红 ${counts.rightRed})`,
);
if (!duoSource || duoSource.color_to === null || duoSource.color_to === undefined) {
  console.error("❌ 勾了「一笔多色」，对象上却没有 color_to ⇒ 参数没传下去");
  await capture("duotone-source-failed");
  process.exit(1);
}
if (!(counts.leftBlue > 0) || !(counts.rightRed > 0)) {
  console.error("❌ 一笔多色没有落到画面：左半应当偏起点色（蓝）、右半应当偏末端色（红）");
  await capture("duotone-pixels-failed");
  process.exit(1);
}
const shotDuo = await capture("brush-duotone");

// ⑧ **改粗细 ⇒ 预览会自己重画** ✓（用户："改完预览不跟着变" ✗）——
//    但要**防抖** ✓（拖滑杆时每一帧都真落一笔是肉眼可见的浪费 ✗）。
//    判据：改粗细之后**等一会儿**，`#brushPreview` 的 src 必须变 ✓（不变 ⇒ 还是没接上 ✗）。
await evaluate(`window.yanshi.setBrush(${JSON.stringify(brushB)})`);
await sleep(700);
const srcBeforeSize = await evaluate("document.getElementById('brushPreview').getAttribute('src') || ''");
await evaluate(`(() => {
  const size = document.getElementById('size');
  size.value = String(Math.max(4, Number(size.value) + 17));
  size.dispatchEvent(new Event('input', { bubbles: true }));
})()`);
// **在 node 这一侧轮询** ✓（比在页面里 eval 一段带占位符的字符串清楚得多 ✓）。
let srcAfterSize = null;
const sizeDeadline = Date.now() + 5000;
while (Date.now() < sizeDeadline) {
  const now = await evaluate("document.getElementById('brushPreview').getAttribute('src') || ''");
  if (now && now !== srcBeforeSize) {
    srcAfterSize = now;
    break;
  }
  await sleep(150);
}
console.log(`  ⑧ 改粗细后预览：${srcBeforeSize ? "有图" : "无图"} ⇒ ${srcAfterSize ? "换了新图" : "没变"}`);
if (!srcAfterSize || srcAfterSize === srcBeforeSize) {
  console.error("❌ 改了粗细，预览没有跟着重画 ⇒ 防抖那条没接上");
  await capture("preview-debounce-failed");
  process.exit(1);
}
const shotDebounce = await capture("brush-preview-after-size");

// ④ 控制台
const errors = consoleLines.filter(
  (line) => /error|uncaught|exception|failed/i.test(line) && !/favicon/i.test(line),
);

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
      smoothFlag: { object: newest && newest.object_id, dataSmooth: smoothFlag, screenshot: shotSmooth },
      smoothOff: { object: newestOff && newestOff.object_id, dataSmooth: smoothOff },
      duoTone: { object: newestDuo && newestDuo.object_id, source: duoSource, counts, screenshot: shotDuo },
      previewDebounce: { screenshot: shotDebounce },
      consoleErrors: errors.length,
    },
    null,
    2,
  ),
);
ws.close();
