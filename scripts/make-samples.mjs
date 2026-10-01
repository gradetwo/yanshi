//! **示例作品生成器** ✓（用户要求："示例里要能看到真实的创作" ✓）。
//!
//! **为什么要一个脚本** ✓：示例此前是当年临时造、**没进版本库**的 ✗ ⇒ 既不可复现 ✗、也没法评审 ✓。
//! 而介质插件是**浏览器里的 WASM** ✓（服务端画不出来 ✗）⇒ 生成必须**经过查看器** ✓。
//! 所以：构图在这里写成**代码** ✓（可 diff、可评审 ✓），由查看器里的
//! `window.yanshiApplyScore` 走**真实介质路径**回放 ✓。
//!
//! **确定性** ✓：所有随机都走这里的 `rng` ✓（自带 PRNG ✓）⇒ 同一份代码画出的画**完全一致** ✓，
//! 重跑不会得到"另一幅画" ✓。
//!
//! 用法 ✓：`node scripts/make-samples.mjs [--only sample-oil] [--port 8110] [--cdp 9333]`

import { writeFile } from "node:fs/promises";

const args = process.argv.slice(2);
const argOf = (name, fallback) => {
  const index = args.indexOf(name);
  return index >= 0 && args[index + 1] ? args[index + 1] : fallback;
};
const only = argOf("--only", null);
const port = Number(argOf("--port", "8110"));
const cdp = Number(argOf("--cdp", "9333"));
const shots = argOf("--shots", "/tmp/yanshi-samples");
// **迭代用的缩放** ✓：小图 ⇒ 笔数随之下降 ✓ ⇒ 秒级出图 ✓。
// 画法里的所有尺寸/间距都乘了 `k = width / 900` ✓ ⇒ 缩小画布**自动**减少笔数 ✓。
const scale = Number(argOf("--scale", "1"));

// ---- 确定性随机 ✓（不用 Math.random ✗）----
let seed = 0x9e3779b9;
const rng = () => {
  seed = (seed * 1664525 + 1013904223) >>> 0;
  return seed / 4294967296;
};
const jitter = (amount) => (rng() * 2 - 1) * amount;

// ---- 颜色助手 ✓（构图需要"渐变"而不是几个死色块 ✓）----
const hex = (value) => value.replace("#", "");
const toRgb = (color) => {
  const text = hex(color);
  return [0, 2, 4].map((index) => parseInt(text.slice(index, index + 2), 16));
};
const toHex = (rgb) =>
  "#" + rgb.map((value) => Math.max(0, Math.min(255, Math.round(value))).toString(16).padStart(2, "0")).join("");
const mix = (a, b, t) => toHex(toRgb(a).map((value, index) => value + (toRgb(b)[index] - value) * t));
const shade = (color, t) => mix(color, t < 0 ? "#000000" : "#ffffff", Math.abs(t));

// ---- 构图助手 ✓（都返回"笔" ✓，由回放器逐笔执行 ✓）----
const stroke = (medium, color, size, wetness, points) => ({ medium, color, size, wetness, points });

/// 一条起伏的线 ✓（山脊 / 岸线 / 波浪 ✓）。
function ridge(from, to, y, amplitude, steps, wobble = 0) {
  const points = [];
  for (let index = 0; index <= steps; index++) {
    const t = index / steps;
    const x = from + (to - from) * t;
    const wave = Math.sin(t * Math.PI * 3.1) * amplitude + Math.sin(t * Math.PI * 7.3) * amplitude * 0.35;
    points.push([x, y + wave + jitter(wobble)]);
  }
  return points;
}

/// **排线填充** ✓（一大片颜色用若干平行笔触铺出来 ✓，不是一个大色块 ✓）。
///
/// **约定** ✓：`angle = 0` 表示**横向**笔触 ✓（天空/水面就是横向扫 ✓）。
/// 我第一版把正余弦写反了 ✗ ⇒ `angle = 0` 画出的是**竖线** ✓
///（服务端渲染一看就是"一片竖栅栏" ✗，完全不像风景 ✓）—— 角度约定这种东西**必须写在注释里** ✓。
function wash(box, color, size, angle, gap, wetness, wobble = 0) {
  const strokes = [];
  const diagonal = Math.hypot(box.w, box.h);
  const cos = Math.cos(angle);
  const sin = Math.sin(angle);
  // **偏移跨度 = 框在"垂直于笔触"方向上的投影** ✓ —— 不是对角线 ✗！
  //
  // 我前两版都用对角线铺偏移 ✗ ⇒ 横向笔触的偏移范围是 ±对角线/2 ✓
  // ⇒ 笔触**画到框外很远** ✓（"前景草地"横贯整幅画、跑到天上去了 ✗）。
  // 这就是"看起来只是多画了几笔"如何毁掉整幅构图 ✓ —— 服务端渲染一眼看穿 ✓。
  const span = Math.abs(sin) * box.w + Math.abs(cos) * box.h;
  const count = Math.max(1, Math.ceil(span / gap));
  for (let index = 0; index < count; index++) {
    const offset = (index / (count - 1 || 1) - 0.5) * span;
    const cx = box.x + box.w / 2;
    const cy = box.y + box.h / 2;
    // **横向**：沿 x 轴铺开 ✓（`offset` 在 y 上 ✓）；`angle` 旋转 ✓。
    const from = [cx - cos * diagonal * 0.5 + sin * offset, cy - sin * diagonal * 0.5 - cos * offset];
    const to = [cx + cos * diagonal * 0.5 + sin * offset, cy + sin * diagonal * 0.5 - cos * offset];
    const points = [];
    for (let step = 0; step <= 6; step++) {
      const t = step / 6;
      points.push([from[0] + (to[0] - from[0]) * t, from[1] + (to[1] - from[1]) * t + jitter(wobble)]);
    }
    strokes.push({ points, size, color });
  }
  return strokes;
}

/// 一簇东西 ✓（树叶 / 草 / 花 ✓）：每笔短、方向散、位置带抖动 ✓。
function cluster(cx, cy, radius, count, color, size, angle, spread) {
  const strokes = [];
  for (let index = 0; index < count; index++) {
    const theta = rng() * Math.PI * 2;
    const distance = Math.sqrt(rng()) * radius;
    const x = cx + Math.cos(theta) * distance;
    const y = cy + Math.sin(theta) * distance * 0.8;
    const direction = angle + jitter(spread);
    const length = size * (1.4 + rng() * 2.2);
    strokes.push({
      points: [
        [x, y],
        [x + Math.cos(direction) * length * 0.6, y + Math.sin(direction) * length * 0.6 + jitter(size * 0.2)],
        [x + Math.cos(direction) * length, y + Math.sin(direction) * length],
      ],
      size,
      color,
    });
  }
  return strokes;
}

/// 把若干"笔"变成介质笔 ✓。
const asMedium = (medium, wetness, strokes) =>
  strokes.map((item) => ({ medium, wetness, color: item.color, size: item.size, points: item.points }));

// ---- 作品：油画 · 湖光山色 ✓（分层 ✓，让图层面板也有用武之地 ✓）----
//
// **两条画法上的实测教训** ✓（都是看了服务端渲染才发现的 ✗）：
// ① **湿度不能一路开到 0.85+** ✗：油画插件会与底色**大量混合** ✓ ⇒ 整幅混成一片灰 ✗。
//    正确用法是"铺色时**干**一些（0.35~0.5 ✓）、过渡时才湿（0.7~0.9 ✓）" ✓；
// ② **一切尺寸都要随画面宽度成比例** ✓：第一版把笔数与尺寸写死 ✓ ⇒ 全尺寸 394 笔要 12 分钟 ✗，
//    根本没法迭代 ✓。改成按 `k = width / 900` 缩放 ✓ 之后，小图（450×320）笔数骤降 ✓ ⇒ 迭代可控 ✓。
function oilLandscape(width, height) {
  const k = width / 900;                      // 尺度因子 ✓
  const at = (value) => Math.max(1, Math.round(value * k));
  const horizon = height * 0.47;
  const sky = [];
  // **天空**：上冷下暖的横向罩染 ✓（铺色要干一些 ✓，否则与底色混成灰 ✗）。
  for (let band = 0; band < 7; band++) {
    const t = band / 6;
    const y = 8 + (horizon - 16) * t;
    const color = mix("#4b6d9e", "#f0dcbc", Math.pow(t, 1.15));
    sky.push(...wash({ x: 0, y: y - at(34), w: width, h: at(68) }, color, at(120), 0, at(52), 0.45, at(5)));
  }
  // **云**：少量、偏暖、稍湿以柔化边缘 ✓。
  const clouds = [];
  for (const [cx, cy, boxScale, tone] of [
    [width * 0.66, height * 0.15, 1.5, 0.4],
    [width * 0.28, height * 0.21, 1.0, 0.25],
  ]) {
    clouds.push(...wash({ x: cx - at(180) * boxScale, y: cy - at(24), w: at(360) * boxScale, h: at(48) },
      mix("#fff6e6", "#e0c9a6", tone), at(104), 0, at(40), 0.7, at(8)));
  }
  // **远山**：越远越淡越蓝 ✓（空气透视 ✓），山脊再压一道深色 ✓。
  const mountains = [];
  for (const [y, amplitude, color, size, wet] of [
    [horizon - at(70), at(24), "#9fb3c6", at(50), 0.55],
    [horizon - at(34), at(18), "#6d8296", at(58), 0.5],
  ]) {
    mountains.push(...wash({ x: 0, y: y - at(20), w: width, h: at(42) }, color, size, 0, at(90), wet, at(4)));
    mountains.push(...asMedium("oil", wet, [0, 1].map((line) => ({
      color: shade(color, -0.14 - line * 0.1),
      size: size * 0.5,
      points: ridge(0, width, y + line * at(6), amplitude, at(16), at(2)),
    }))));
  }
  // **湖面**：上暖下冷 ✓（倒映天空 ✓），反光要**短而少** ✓（第一版又长又亮 ✗ ⇒ 像白栅栏 ✓）。
  const water = [];
  for (let band = 0; band < 6; band++) {
    const t = band / 5;
    const y = horizon + at(8) + (height - horizon - at(16)) * t;
    water.push(...wash({ x: 0, y: y - at(30), w: width, h: at(60) },
      mix("#cbd9dd", "#33556b", Math.pow(t, 0.85)), at(112), 0, at(58), 0.45, at(6)));
  }
  for (let index = 0; index < 7; index++) {
    const y = horizon + at(18) + index * (height - horizon - at(36)) / 7;
    const x = width * (0.32 + rng() * 0.5);
    water.push(...asMedium("oil", 0.75, [{
      color: mix("#f6ecd4", "#cbd9dd", rng() * 0.6),
      size: at(9 + rng() * 7),
      points: [[x - at(30 - rng() * 20), y], [x + at(20 + rng() * 30), y + jitter(at(3))]],
    }]));
  }
  // **树**：树干 + 背光/受光两簇树冠 ✓（体积感 ✓）。
  const treeX = width * 0.24;
  const treeBase = height * 0.9;
  const tree = [];
  tree.push(...asMedium("oil", 0.35, [
    { color: "#4a3527", size: at(26), points: [[treeX, treeBase], [treeX + jitter(at(5)), treeBase - at(84)], [treeX - at(12), treeBase - at(150)]] },
    { color: "#6b4a33", size: at(9), points: [[treeX - at(8), treeBase - at(20)], [treeX - at(14), treeBase - at(120)]] },
  ]));
  tree.push(...asMedium("oil", 0.5, cluster(treeX + at(4), treeBase - at(176), at(74), at(14), "#3d5c33", at(40), -Math.PI / 2, 1.1)));
  tree.push(...asMedium("oil", 0.45, cluster(treeX - at(40), treeBase - at(146), at(52), at(9), "#4d6b3a", at(34), -Math.PI / 2, 1.1)));
  tree.push(...asMedium("oil", 0.4, cluster(treeX - at(18), treeBase - at(196), at(40), at(7), "#a8c46a", at(24), -Math.PI / 2 + 0.2, 0.9)));
  // **前景**：暖草地 + 草叶 ✓（把画面压住 ✓）。
  const ground = [];
  ground.push(...wash({ x: 0, y: height - at(66), w: width, h: at(66) }, "#6f7c42", at(96), 0, at(70), 0.4, at(7)));
  ground.push(...asMedium("oil", 0.35, cluster(width * 0.6, height - at(34), at(160), at(22), mix("#93a355", "#d3c887", rng() * 0.6), at(16), -Math.PI / 2 + 0.25, 1.3)));
  ground.push(...asMedium("oil", 0.35, cluster(width * 0.16, height - at(30), at(120), at(16), "#5d6b38", at(14), -Math.PI / 2, 1.3)));
  return [
    { id: "sky", name: "天空", strokes: asMedium("oil", 0.45, sky).concat(asMedium("oil", 0.7, clouds)) },
    { id: "mountains", name: "远山", strokes: asMedium("oil", 0.55, mountains) },
    { id: "water", name: "湖面", strokes: asMedium("oil", 0.5, water) },
    { id: "tree", name: "树", strokes: tree },
    { id: "ground", name: "前景", strokes: ground },
  ];
}

const WORKS = {
  "sample-oil": { width: 900, height: 640, build: oilLandscape },
};

// ---- CDP 驱动 ✓ ----
const list = await fetch(`http://127.0.0.1:${cdp}/json/list`).then((r) => r.json());
const target = list.find((item) => item.type === "page");
if (!target) throw new Error("找不到调试页面（CDP " + cdp + "）");
const ws = new WebSocket(target.webSocketDebuggerUrl);
let id = 1;
const pending = new Map();
ws.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);
  if (message.id && pending.has(message.id)) { pending.get(message.id)(message); pending.delete(message.id); }
});
await new Promise((resolve) => ws.addEventListener("open", resolve));
const send = (method, params = {}) =>
  new Promise((resolve) => { const current = id++; pending.set(current, resolve); ws.send(JSON.stringify({ id: current, method, params })); });
const evaluate = async (expression) =>
  (await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true })).result?.result?.value;
const shot = async (name) => {
  const message = await send("Page.captureScreenshot", { format: "png" });
  const data = message?.result?.data;
  if (!data) return null;
  const path = `${shots}/${name}.png`;
  await writeFile(path, Buffer.from(data, "base64"));
  return path;
};

await send("Runtime.enable");
await send("Page.enable");
await send("Network.enable", {});
await send("Network.setCacheDisabled", { cacheDisabled: true });
await send("Page.navigate", { url: `http://127.0.0.1:${port}/` });
await send("Page.bringToFront", {});
try { await send("Emulation.setFocusEmulationEnabled", { enabled: true }); } catch (_) {}
for (let attempt = 0; attempt < 100; attempt++) {
  if (await evaluate(`typeof window.yanshiApplyScore === "function"`)) break;
  await new Promise((resolve) => setTimeout(resolve, 250));
}

await import("node:fs/promises").then((fs) => fs.mkdir(shots, { recursive: true }));
for (const [docId, work] of Object.entries(WORKS)) {
  if (only && only !== docId) continue;
  // **重置种子** ✓：同一份作品每次都画出**同一幅画** ✓。
  seed = 0x9e3779b9;
  const layers = work.build(Math.round(work.width * scale), Math.round(work.height * scale));
  const strokes = layers.reduce((sum, layer) => sum + layer.strokes.length, 0);
  console.log(`  ${docId}：${layers.length} 层、${strokes} 笔 ⇒ 开始画…`);
  const started = Date.now();
  // 打开或新建 ✓，然后回放画谱 ✓。
  const opened = await evaluate(`(async () => {
    const existing = await fetch("/api/documents").then((r) => r.json()).catch(() => ({}));
    const known = (existing.documents || []).some((doc) => doc.doc_id === ${JSON.stringify(docId)});
    if (known) { await switchDocument(${JSON.stringify(docId)}); return "opened"; }
    const created = await fetch("/api/documents", { method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ doc_id: ${JSON.stringify(docId)},
        width: ${Math.round(work.width * scale)}, height: ${Math.round(work.height * scale)} }) })
      .then((r) => r.json());
    if (created.token) await switchDocument(created.doc_id, created.token);
    return "created";
  })()`);
  console.log(`    文档：${opened} ✓`);
  // **点火后轮询** ✓ —— 我第一版直接 `await` 整个画谱 ✗（十分钟的 `Runtime.evaluate` ✓）
  // ⇒ CDP 连接中途断掉 ✓、Node **静默退出**（退出码 0、没有截图 ✓），极难归因 ✗。
  // 改成：让页面自己跑 ✓，脚本从**服务端原子数**读进度 ✓（每笔 2 个原子 ✓ ⇒ 进度一目了然 ✓）。
  await evaluate(`void window.yanshiApplyScore(${JSON.stringify({ layers })})`);
  const atomCount = async () => {
    const listed = await fetch(`http://127.0.0.1:${port}/api/documents`).then((r) => r.json())
      .catch(() => ({ documents: [] }));
    // `docId` 是 **Node 侧**变量 ✓（这里不是模板字符串 ✓ ⇒ 不能写 `${...}` ✗）。
    const found = (listed.documents || []).find((doc) => doc.doc_id === docId);
    return found ? found.atoms : 0;
  };
  let last = 0;
  let stable = 0;
  for (let tick = 0; tick < 900; tick++) {
    await new Promise((resolve) => setTimeout(resolve, 2000));
    const now = await atomCount();
    if (now > last) { last = now; stable = 0; } else { stable += 1; }
    if (tick % 15 === 0) console.log(`    进度：${Math.round(now / 2)} 笔`);
    // 连续 6 次（12 秒）没有新增 ⇒ 视为画完 ✓；同时兜住"卡住不动"的情况 ✓。
    if (stable >= 6 && now > 0) break;
  }
  console.log(`    完成：原子 ${last}（约 ${Math.round(last / 2)} 笔，脚本侧 ${Math.round((Date.now() - started) / 1000)} 秒）`);
  const path = await shot(docId);
  console.log(`    截图：${path || "（失败）"}`);
}
ws.close();
process.exit(0);
