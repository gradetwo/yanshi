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
// **草稿模式** ✓：只把**笔距**放大 ✓（构图、笔尖尺度、明暗全都真实 ✓）。
//
// 这是我上一轮那个"缩小画布"办法的**修正** ✗：把画面缩小 ⇒ 笔尖相对画面变得巨大 ✓
// ⇒ 出来全是糊的 ✓（我因此还把糊当成了风格 ✗）。**正确做法**是画面不动 ✓、
// 只把笔距拉开 ✓ ⇒ 草稿仍然**构图正确、比例正确** ✓，只是笔触更疏 ✓。
const draft = Number(argOf("--draft", "1"));
// **目标文档 id 可覆盖** ✓（`--doc sample-lake` ✓）：同一件作品可以落到不同文档 ✓
// —— 生成新示例时**不必覆盖**用户已有的画 ✓（不降级、不破坏 ✓）。
const docOverride = argOf("--doc", null);

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
  const count = Math.max(1, Math.ceil(span / gapOf(gap)));
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

// ---- 专业构图用的助手 ✓（"偃师造人"这一幅要求的是**作品** ✓，方框铺色不够用 ✗）----

/// **多边形扫描线填充** ✓：把任意多边形用**平行笔触**铺满 ✓（每条扫描线按交点裁到多边形内部 ✓）。
///
/// **为什么要它** ✓：方框铺色画不出**人体轮廓** ✗（此前那些"画"都是方块拼的 ✓）。
/// 扫描线填充让每一笔都落在轮廓里 ✓ ⇒ 远看是形体、近看是笔触 ✓ —— 这正是油画该有的样子 ✓。
/// `colorAt(t, edge)` 给出**沿填充方向的渐变** ✓（受光←→背光 ✓，是"有体积"的关键 ✓）。
function fillPolygon(points, colorAt, size, gap, wetness, angle = 0) {
  const cos = Math.cos(-angle);
  const sin = Math.sin(-angle);
  const rotated = points.map(([x, y]) => [x * cos - y * sin, x * sin + y * cos]);
  let minY = Infinity;
  let maxY = -Infinity;
  let minX = Infinity;
  let maxX = -Infinity;
  for (const [x, y] of rotated) {
    minY = Math.min(minY, y); maxY = Math.max(maxY, y);
    minX = Math.min(minX, x); maxX = Math.max(maxX, x);
  }
  const strokes = [];
  const count = Math.max(1, Math.ceil((maxY - minY) / gapOf(gap)));
  const back = (x, y) => [x * cos + y * sin, -x * sin + y * cos];   // 逆旋转 ✓（cos/sin 已取负角 ✓）
  for (let row = 0; row <= count; row++) {
    const y = minY + (row / count) * (maxY - minY);
    // 求这条水平线与多边形各边的交点 ✓。
    const hits = [];
    for (let index = 0; index < rotated.length; index++) {
      const [x1, y1] = rotated[index];
      const [x2, y2] = rotated[(index + 1) % rotated.length];
      if ((y1 > y) !== (y2 > y)) hits.push(x1 + ((y - y1) / (y2 - y1)) * (x2 - x1));
    }
    hits.sort((a, b) => a - b);
    for (let span = 0; span + 1 < hits.length; span += 2) {
      const from = hits[span] + 1;
      const to = hits[span + 1] - 1;
      if (to - from < 1) continue;
      // **每一笔都带一点抖动与偏移** ✓：手绘感来自"平行但不机械" ✓。
      const wobble = (rng() * 2 - 1) * gap * 0.35;
      const start = back(from, y + wobble);
      const end = back(to, y + wobble);
      const t = (y - minY) / Math.max(1, maxY - minY);
      strokes.push({ points: [start, end], size, wetness, color: colorAt(t, span) });
    }
  }
  return strokes;
}

/// **径向光晕** ✓：一圈圈由内向外变淡的同心笔触 ✓（油灯的暖光靠它 ✓）。
function glow(cx, cy, radius, inner, outer, steps, size) {
  const strokes = [];
  for (let step = steps; step >= 1; step--) {
    const t = step / steps;
    const r = radius * t;
    const color = mix(outer, inner, Math.pow(1 - t, 0.6));
    const sides = Math.max(8, Math.round(12 + t * 10));
    for (let side = 0; side < sides; side++) {
      const a0 = (side / sides) * Math.PI * 2;
      const a1 = ((side + 1) / sides) * Math.PI * 2;
      strokes.push({
        points: [
          [cx + Math.cos(a0) * r, cy + Math.sin(a0) * r * 0.92],
          [cx + Math.cos((a0 + a1) / 2) * r * 1.02, cy + Math.sin((a0 + a1) / 2) * r * 0.94],
          [cx + Math.cos(a1) * r, cy + Math.sin(a1) * r * 0.92],
        ],
        size,
        // 外圈更湿 ✓ ⇒ 边缘化开 ✓；内圈干 ✓ ⇒ 留住光核 ✓。
        wetness: 0.35 + t * 0.5,
        color,
      });
    }
  }
  return strokes;
}

/// **轮廓光** ✓：沿一条边给一道细亮笔触 ✓（把人从暗背景里"抠"出来 ✓）。
function rimLight(points, color, size, wetness) {
  const strokes = [];
  for (let index = 0; index + 1 < points.length; index++) {
    strokes.push({ points: [points[index], points[index + 1]], size, wetness, color });
  }
  return strokes;
}

/// 一条**手绘线** ✓：把折线细分成带抖动的短笔 ✓（细线要走铅笔介质 ✓）。
function handLine(points, color, size, wetness, jitterAmount = 1.4) {
  const out = [];
  for (let index = 0; index + 1 < points.length; index++) {
    const [x1, y1] = points[index];
    const [x2, y2] = points[index + 1];
    const steps = Math.max(1, Math.round(Math.hypot(x2 - x1, y2 - y1) / 14));
    for (let step = 0; step < steps; step++) {
      const t0 = step / steps;
      const t1 = (step + 1) / steps;
      out.push({
        points: [
          [x1 + (x2 - x1) * t0 + jitter(jitterAmount), y1 + (y2 - y1) * t0 + jitter(jitterAmount)],
          [x1 + (x2 - x1) * t1 + jitter(jitterAmount), y1 + (y2 - y1) * t1 + jitter(jitterAmount)],
        ],
        size,
        wetness,
        color,
      });
    }
  }
  return out;
}

/// 椭圆采样 ✓（脸、肩、器物都用得上 ✓）。
function ellipse(cx, cy, rx, ry, steps = 24) {
  const points = [];
  for (let index = 0; index < steps; index++) {
    const angle = (index / steps) * Math.PI * 2;
    points.push([cx + Math.cos(angle) * rx, cy + Math.sin(angle) * ry]);
  }
  return points;
}

/// **笔距** ✓：草稿模式按 `draft` 放大 ✓（越大 ⇒ 笔数越少 ⇒ 越快 ✓）。
/// **笔尖尺寸不参与** ✓ —— 尺寸是"画多大"的问题 ✓，与"画多快"无关 ✓。
const gapOf = (value) => Math.max(1, value * draft);

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
  for (let band = 0; band < 10; band++) {
    const t = band / 9;
    const y = 6 + (horizon - 12) * t;
    // **对比要拉开** ✓：天顶更深（#3f5f92 ✓）、地平线更暖（#f6e2c0 ✓）——
    // 上一版太灰 ✓ ⇒ 天地分不开 ✓（服务端渲染一眼看出 ✗）。
    // **上 2/3 必须是蓝的** ✓ —— 上一版的暖色在中段就压过来 ✓
    // 加上一大团云 ✓ ⇒ 天空整个变成白团 ✗。暖色只留给地平线附近 ✓（指数 2.6 ✓）。
    const color = mix("#3a5c92", "#f6e2c0", Math.pow(t, 2.6));
    sky.push(...wash({ x: 0, y: y - at(26), w: width, h: at(52) }, color, at(120), 0, at(28), 0.5, at(4)));
  }
  // **暖阳** ✓：几圈短笔触叠出光晕 ✓（比一个圆点更像油画里的光 ✓）。
  // **低太阳** ✓（贴近地平线 ✓）：既能解释天空的暖调 ✓、又不会和云抢画面 ✓。
  const sunX = width * 0.72;
  const sunY = horizon - at(52);
  for (const [radius, tone, size] of [[at(40), 0.3, at(34)], [at(24), 0.5, at(24)], [at(12), 0.7, at(16)]]) {
    sky.push(...wash({ x: sunX - radius, y: sunY - radius * 0.8, w: radius * 2, h: radius * 1.6 },
      mix("#ffe9b8", "#f6d79a", 1 - tone), size, 0, at(14), 0.65, at(3)));
  }
  // **云**：少量、偏暖、稍湿以柔化边缘 ✓。
  const clouds = [];
  // **云要小、要少** ✓：上一版两团横跨大半个天空 ✗ ⇒ 天空读成了"一团白云" ✗。
  // 现在只留**一朵** ✓（左上 ✓），其余留给蓝色与渐变的暖调 ✓。
  for (const [cx, cy, boxScale, tone] of [[width * 0.24, height * 0.16, 0.9, 0.3]]) {
    clouds.push(...wash({ x: cx - at(95) * boxScale, y: cy - at(16), w: at(190) * boxScale, h: at(32) },
      mix("#fff6e6", "#e6d0ae", tone), at(80), 0, at(26), 0.8, at(7)));
  }
  // **远山**：越远越淡越蓝 ✓（空气透视 ✓），山脊再压一道深色 ✓。
  const mountains = [];
  for (const [y, amplitude, color, size, wet] of [
    [horizon - at(70), at(24), "#9fb3c6", at(50), 0.55],
    [horizon - at(34), at(18), "#6d8296", at(58), 0.5],
  ]) {
    mountains.push(...wash({ x: 0, y: y - at(20), w: width, h: at(42) }, color, size, 0, at(52), wet, at(4)));
    mountains.push(...asMedium("oil", wet, [0, 1].map((line) => ({
      color: shade(color, -0.14 - line * 0.1),
      size: size * 0.5,
      points: ridge(0, width, y + line * at(6), amplitude, at(16), at(2)),
    }))));
  }
  // **远岸** ✓：地平线处压一道深色 ✓ —— 上一版天地分不开 ✗，就是缺这道线 ✓。
  const shore = [];
  shore.push(...asMedium("oil", 0.4, [
    { color: "#3d514c", size: at(15), points: [[0, horizon - at(2)], [width * 0.5, horizon + at(2)], [width, horizon - at(3)]] },
    { color: "#5d6f58", size: at(9), points: ridge(0, width, horizon + at(4), at(4), at(20), at(2)) },
    { color: "#7d8f6a", size: at(5), points: ridge(0, width, horizon + at(7), at(3), at(22), at(2)) },
  ]));
  // **湖面**：上暖下冷 ✓（倒映天空 ✓），反光要**短而少** ✓（第一版又长又亮 ✗ ⇒ 像白栅栏 ✓）。
  const water = [];
  const treeXRef = width * 0.24;   // 与树同一条竖线 ✓（倒影要对位 ✓）
  for (let band = 0; band < 6; band++) {
    const t = band / 5;
    const y = horizon + at(8) + (height - horizon - at(16)) * t;
    water.push(...wash({ x: 0, y: y - at(26), w: width, h: at(52) },
      mix("#8fb0c0", "#17323f", Math.pow(t, 0.75)), at(112), 0, at(30), 0.5, at(5)));
  }
  // **树的倒影** ✓：画在树的正下方 ✓（上一版反光位置随机 ✓ ⇒ 与主体无关 ✗）。
  for (let index = 0; index < 6; index++) {
    const y = horizon + at(14) + index * at(13);
    water.push(...asMedium("oil", 0.8, [{
      color: mix("#33556b", "#5d7a63", rng() * 0.7),
      size: at(16 + rng() * 12),
      points: [[treeXRef - at(18) + jitter(at(10)), y], [treeXRef + at(14) + jitter(at(10)), y]],
    }]));
  }
  for (let index = 0; index < 7; index++) {
    const y = horizon + at(18) + index * (height - horizon - at(36)) / 7;
    const x = width * (0.42 + rng() * 0.45);
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
  tree.push(...asMedium("oil", 0.3, [
    { color: "#42301f", size: at(38), points: [[treeX, treeBase], [treeX + jitter(at(4)), treeBase - at(90)], [treeX - at(10), treeBase - at(158)]] },
    { color: "#6b4a33", size: at(13), points: [[treeX - at(10), treeBase - at(20)], [treeX - at(16), treeBase - at(120)], [treeX - at(6), treeBase - at(156)]] },
    // 两根分枝 ✓：树冠才不像"插在杆子上" ✓。
    { color: "#42301f", size: at(12), points: [[treeX - at(6), treeBase - at(120)], [treeX - at(52), treeBase - at(168)]] },
    { color: "#42301f", size: at(11), points: [[treeX - at(4), treeBase - at(134)], [treeX + at(46), treeBase - at(176)]] },
  ]));
  tree.push(...asMedium("oil", 0.5, cluster(treeX - at(6), treeBase - at(184), at(84), at(20), "#33512c", at(34), -Math.PI / 2, 1.2)));
  tree.push(...asMedium("oil", 0.45, cluster(treeX - at(52), treeBase - at(156), at(54), at(12), "#3f5f33", at(30), -Math.PI / 2, 1.2)));
  tree.push(...asMedium("oil", 0.45, cluster(treeX + at(44), treeBase - at(166), at(50), at(11), "#456a37", at(28), -Math.PI / 2, 1.2)));
  tree.push(...asMedium("oil", 0.4, cluster(treeX - at(22), treeBase - at(206), at(46), at(10), "#9dbd63", at(22), -Math.PI / 2 + 0.2, 1.0)));
  // **前景**：暖草地 + 草叶 ✓（把画面压住 ✓）。
  const ground = [];
  ground.push(...wash({ x: 0, y: height - at(66), w: width, h: at(66) }, "#6f7c42", at(96), 0, at(38), 0.4, at(6)));
  ground.push(...asMedium("oil", 0.35, cluster(width * 0.6, height - at(34), at(160), at(22), mix("#93a355", "#d3c887", rng() * 0.6), at(16), -Math.PI / 2 + 0.25, 1.3)));
  ground.push(...asMedium("oil", 0.35, cluster(width * 0.16, height - at(30), at(120), at(16), "#5d6b38", at(14), -Math.PI / 2, 1.3)));
  return [
    { id: "sky", name: "天空", strokes: asMedium("oil", 0.45, sky).concat(asMedium("oil", 0.7, clouds)) },
    { id: "mountains", name: "远山与岸", strokes: asMedium("oil", 0.55, mountains).concat(shore) },
    { id: "water", name: "湖面", strokes: asMedium("oil", 0.5, water) },
    { id: "tree", name: "树", strokes: tree },
    { id: "ground", name: "前景", strokes: ground },
  ];
}

// ---- 作品：《偃师造人》 ✓（真作画法：统一光源 + 明暗交界 + 轮廓塑形 ✓）----
//
// **立意** ✓（《列子·汤问》）：周穆王西巡，匠人偃师献"倡者"——能歌善舞的**造人**；
// 王怒其挑逗侍妾，偃师剖之，竟是皮革木胶漆所制 ✓。
//
// **三条画法纪律** ✓（上一版的失败之处 ✗ ⇒ 这三条是"专业感"的来源 ✓）：
// ① **一切明暗服从同一盏灯** ✓：画面右侧一盏小油灯 ✓ ⇒ 每个形体的受光/背光/轮廓光
//    都由"离灯多远、面朝哪边"决定 ✓（`lit()` ✓）。上一版每块各自为政 ✗ ⇒ 像贴纸 ✓。
// ② **笔尖用绝对尺寸** ✓（820×1080 的画上，铺色 60~110px ✓、塑形 18~40px ✓、细节 5~9px ✓）——
//    上一版按画幅等比缩放 ✗ ⇒ 小图上笔尖占画面 1/10 ✓ ⇒ 全体糊成一团 ✓。
// ③ **形体靠"面"而不是"线"** ✓：每个部分都由若干**带明暗的面**拼出来 ✓，
//    轮廓只用来收边（`rimLight` ✓）—— 这是油画与贴纸的分界 ✓。
function yanshiAutomaton(width, height) {
  const W = width;
  const H = height;
  const X = (f) => W * f;
  const Y = (f) => H * f;
  // **灯** ✓（右侧偏上 ✓）：后面所有 `lit()` 都以它为准 ✓。
  const lamp = [X(0.845), Y(0.40)];
  const distToLamp = (x, y) => Math.hypot(x - lamp[0], y - lamp[1]) / (W * 0.95);
  // **统一光照** ✓：`base` 是固有色 ✓、`t` 是"离灯的归一距离" ✓ ⇒ 越远越暗越冷 ✓。
  const lit = (base, x, y, strength = 1) => {
    const t = Math.min(1, distToLamp(x, y) * strength);
    return mix(shade(base, -0.55 - 0.25 * t), base, Math.pow(1 - t, 1.4));
  };
  // 受光的一侧更暖 ✓（暖光照明 ✓）；背光的一侧偏冷紫 ✓（环境色 ✓）—— 冷暖对撞才"活" ✓。
  const warm = (base, amount) => mix(base, "#ffcf8a", amount);
  const cool = (base, amount) => mix(base, "#2a2f45", amount);

  // ---- 暗室 ✓（横向渐变：越靠灯越暖 ✓；再补一层地面 ✓）----
  const room = fillPolygon(
    [[-40, -40], [W + 40, -40], [W + 40, H + 40], [-40, H + 40]],
    (t) => mix("#0f1118", "#332718", Math.pow(t, 1.7)),
    150, 44, 0.35, Math.PI / 2,
  );
  room.push(...fillPolygon(
    [[-40, Y(0.885)], [W + 40, Y(0.87)], [W + 40, H + 40], [-40, H + 40]],
    (t) => mix("#3b2f22", "#1d1712", Math.pow(t, 1.1)),
    140, 40, 0.4, Math.PI / 2,
  ));

  // ---- 灯与光 ✓ ----
  const lampLight = glow(lamp[0], lamp[1], W * 0.70, "#ffe9bd", "#2b2218", 8, 54);
  lampLight.push(...fillPolygon(   // 灯盏（青铜 ✓，受光在下缘 ✓）
    [[lamp[0] - 24, lamp[1] + 20], [lamp[0] + 24, lamp[1] + 20],
     [lamp[0] + 15, lamp[1] + 46], [lamp[0] - 15, lamp[1] + 46]],
    (t) => mix("#8a6a34", "#3a2c18", t), 18, 7, 0.45, 0,
  ));
  lampLight.push(...glow(lamp[0], lamp[1] - 4, 40, "#fffaf0", "#ffc46a", 4, 24));

  // ---- 造人 ✓（先骨架、再受光面、再背光面 ✓）----
  const figure = [];
  const headC = [X(0.395), Y(0.300)];
  const headRX = W * 0.070;
  const headRY = H * 0.055;
  const chest = [X(0.408), Y(0.545)];
  // 躯干（朱漆袍 ✓）：肩宽腰收 ✓ ⇒ 人不是柱子 ✓。
  const torso = [
    [X(0.330), Y(0.400)], [X(0.475), Y(0.392)], [X(0.500), Y(0.560)],
    [X(0.470), Y(0.700)], [X(0.325), Y(0.706)], [X(0.300), Y(0.560)],
  ];
  figure.push(...fillPolygon(torso, (t) => {
    const y = Y(0.40) + t * (Y(0.706) - Y(0.40));
    return lit("#96412f", X(0.42), y, 1.0);
  }, 84, 20, 0.5, 0));
  // **受光面** ✓：靠灯的一侧（右侧 ✓）一层更暖更亮 ✓ —— 明暗交界由此产生 ✓。
  figure.push(...fillPolygon(
    [[X(0.452), Y(0.394)], [X(0.476), Y(0.392)], [X(0.500), Y(0.560)],
     [X(0.470), Y(0.700)], [X(0.428), Y(0.703)], [X(0.432), Y(0.560)]],
    (t) => warm(lit("#b8613c", X(0.47), Y(0.40) + t * (Y(0.70) - Y(0.40)), 0.9), 0.35),
    64, 15, 0.42, 0,
  ));
  // 背光面 ✓（左侧偏冷紫 ✓）。
  figure.push(...fillPolygon(
    [[X(0.330), Y(0.401)], [X(0.372), Y(0.398)], [X(0.360), Y(0.700)],
     [X(0.325), Y(0.706)], [X(0.300), Y(0.560)]],
    (t) => cool(lit("#5e2a22", X(0.32), Y(0.40) + t * (Y(0.70) - Y(0.40)), 1.15), 0.4),
    70, 17, 0.45, 0,
  ));
  // 下裳 ✓（朱漆 + 暗褶 ✓）。
  figure.push(...fillPolygon(
    [[X(0.318), Y(0.700)], [X(0.478), Y(0.696)], [X(0.520), Y(0.885)],
     [X(0.285), Y(0.888)],
    ],
    (t) => lit("#7d3527", X(0.40), Y(0.70) + t * (Y(0.888) - Y(0.70)), 1.05),
    96, 22, 0.5, 0,
  ));
  for (let fold = 0; fold < 5; fold++) {   // 衣褶 ✓（一笔一笔的暗线 ✓）
    const fx = 0.33 + fold * 0.042;
    figure.push(...handLine(
      [[X(fx), Y(0.712)], [X(fx + 0.012), Y(0.80)], [X(fx - 0.004), Y(0.878)]],
      cool("#4a1f16", 0.25), 12, 0.35, 3,
    ));
  }
  // **腰带** ✓：把上下身分开 ✓，也让腰"收"进去 ✓。
  figure.push(...fillPolygon(
    [[X(0.303), Y(0.663)], [X(0.487), Y(0.657)], [X(0.492), Y(0.703)], [X(0.300), Y(0.709)]],
    (t) => lit("#2f2018", X(0.40), Y(0.68) + t * 8, 1.0), 34, 9, 0.4, 0,
  ));
  figure.push(...rimLight([[X(0.487), Y(0.658)], [X(0.492), Y(0.702)]], warm("#8a5a2c", 0.4), 7, 0.4));
  // 颈 ✓ + 面部底面 ✓（下颌的暗面 ✓）。
  figure.push(...fillPolygon(
    [[X(0.375), Y(0.336)], [X(0.418), Y(0.334)], [X(0.428), Y(0.400)], [X(0.366), Y(0.402)]],
    (t) => cool(lit("#a97a52", X(0.40), Y(0.336) + t * (Y(0.40) - Y(0.336)), 0.7), 0.35),
    32, 8, 0.4, 0,
  ));
  // **头** ✓：颅 + 面 + 颌三块面 ✓（漆面平滑 ✓ ⇒ 面与面之间的过渡要柔 ✓）。
  figure.push(...fillPolygon(   // 颅（背光 ✓）
    ellipse(headC[0] - 6, headC[1] - 6, headRX * 1.02, headRY * 1.05, 28),
    (t, span) => (span === 0 ? cool(lit("#c9a077", headC[0] - headRX, headC[1], 0.8), 0.25)
                             : lit("#d8b183", headC[0], headC[1], 0.75)),
    34, 9, 0.42, 0,
  ));
  figure.push(...fillPolygon(   // 面（朝右下 ✓ = 朝灯 ✓）
    [[X(0.352), Y(0.276)], [X(0.424), Y(0.272)], [X(0.436), Y(0.318)],
     [X(0.412), Y(0.348)], [X(0.362), Y(0.344)], [X(0.344), Y(0.308)]],
    (t, span) => (span === 0 ? lit("#e6c193", X(0.40), Y(0.30), 0.8)
                             : warm(lit("#f0d2a6", X(0.42), Y(0.30), 0.6), 0.25)),
    28, 7, 0.4, 0,
  ));
  figure.push(...fillPolygon(   // 颌与颧的暗面 ✓
    [[X(0.344), Y(0.308)], [X(0.362), Y(0.344)], [X(0.398), Y(0.352)],
     [X(0.372), Y(0.362)], [X(0.344), Y(0.340)]],
    () => cool("#b5875c", 0.45), 24, 6, 0.4, 0,
  ));
  // 发髻 ✓（暗金铜色 ✓，受光在右 ✓）。
  figure.push(...fillPolygon(
    [[X(0.330), Y(0.258)], [X(0.398), Y(0.244)], [X(0.432), Y(0.264)],
     [X(0.410), Y(0.288)], [X(0.336), Y(0.296)]],
    (t) => lit("#241a14", X(0.36), Y(0.26) + t * 12, 0.9), 26, 8, 0.35, 0,
  ));
  figure.push(...rimLight([[X(0.400), Y(0.245)], [X(0.432), Y(0.263)]], warm("#8a6a3a", 0.5), 6, 0.4));
  // 手臂 ✓（前臂搭在膝上 ✓）；手是"木"的 ✓ ⇒ 偏赭、少光泽 ✓。
  for (const side of [-1, 1]) {
    const sx = X(0.40) + side * 72;
    figure.push(...fillPolygon(
      [[sx - 26, Y(0.420)], [sx + 26, Y(0.424)], [sx + 34, Y(0.630)], [sx - 30, Y(0.636)]],
      (t) => lit(side > 0 ? "#a95f3c" : "#6d3a28", sx, Y(0.42) + t * (Y(0.63) - Y(0.42)), 1.0),
      52, 13, 0.45, 0,
    ));
    figure.push(...fillPolygon(
      ellipse(sx, Y(0.672), 40, 22, 16),
      (t, span) => (span === 0 && side > 0 ? warm("#c08b5c", 0.3) : lit("#a97b52", sx, Y(0.672), 0.95)),
      34, 9, 0.4, 0,
    ));
  }
  // 膝 ✓（下裳上的两个受光块 ✓ ⇒ 坐姿读得出来 ✓）。
  for (const side of [-1, 1]) {
    figure.push(...fillPolygon(
      ellipse(X(0.40) + side * 78, Y(0.760), 74, 34, 18),
      (t, span) => (span === 0 && side > 0 ? warm(lit("#b06a42", X(0.40) + side * 78, Y(0.76), 0.8), 0.25)
                                           : lit("#8a4530", X(0.40) + side * 78, Y(0.76), 1.05)),
      60, 15, 0.45, 0,
    ));
  }

  // ---- 机枢 ✓（故事的核心 ✓：胸腔开启、木骨铜枢 ✓）----
  const mechanism = [];
  // 腔体（很暗 ✓，但**不是纯黑** ✓ —— 暗部要留一点温度 ✓）。
  mechanism.push(...fillPolygon(
    [[X(0.330), Y(0.482)], [X(0.478), Y(0.478)], [X(0.472), Y(0.628)], [X(0.336), Y(0.632)]],
    (t) => mix("#2b1a12", "#150d09", t), 64, 14, 0.35, 0,
  ));
  // **被打开的漆面盖板** ✓（叙事关键 ✓：它不是"一个洞" ✓，是"能开合的盖" ✓）。
  mechanism.push(...fillPolygon(
    [[X(0.470), Y(0.470)], [X(0.560), Y(0.452)], [X(0.596), Y(0.596)], [X(0.500), Y(0.618)]],
    (t) => lit("#8f3a2a", X(0.53), Y(0.47) + t * (Y(0.60) - Y(0.47)), 1.0),
    44, 11, 0.45, 0,
  ));
  mechanism.push(...rimLight([[X(0.562), Y(0.452)], [X(0.598), Y(0.594)]], warm("#d08a4a", 0.5), 7, 0.4));
  // 肋骨 ✓（一列细木条 ✓，越靠灯越亮 ✓）。
  for (let rib = 0; rib < 6; rib++) {
    const y = Y(0.494) + rib * 22;
    mechanism.push(...handLine(
      [[X(0.344), y], [X(0.466), y - 4]],
      lit(rib % 2 ? "#8a6136" : "#6d4a2a", X(0.42), y, 0.85), 7, 0.35, 1.6,
    ));
  }
  // 脊柱与吊索 ✓（几根竖直与斜向的细线 ✓ ⇒ "机构"读得出来 ✓）。
  mechanism.push(...handLine([[X(0.400), Y(0.486)], [X(0.404), Y(0.626)]], lit("#9a7040", X(0.40), Y(0.55), 0.9), 9, 0.35, 1.2));
  for (const [x0, y0, x1, y1] of [[0.352, 0.500, 0.396, 0.540], [0.396, 0.540, 0.452, 0.506], [0.360, 0.560, 0.400, 0.596], [0.400, 0.596, 0.446, 0.566]]) {
    mechanism.push(...handLine([[X(x0), Y(y0)], [X(x1), Y(y1)]], "#6a4a2c", 5, 0.35, 1.0));
  }
  // 铜枢 ✓（两枚 ✓，亮黄铜在暗腔里 = 画面的"眼" ✓）。
  for (const [gx, gy, r] of [[0.372, 0.592, 15], [0.436, 0.612, 11]]) {
    mechanism.push(...fillPolygon(ellipse(X(gx), Y(gy), r, r * 0.9, 14),
      () => lit("#c9a24a", X(gx), Y(gy), 0.6), 12, 3.4, 0.4, 0));
    mechanism.push(...rimLight(ellipse(X(gx), Y(gy), r * 0.72, r * 0.62, 10), "#ffe6a8", 4, 0.4));
    for (let tooth = 0; tooth < 6; tooth++) {
      const angle = (tooth / 6) * Math.PI * 2;
      mechanism.push(...handLine(
        [[X(gx) + Math.cos(angle) * r * 0.85, Y(gy) + Math.sin(angle) * r * 0.75],
         [X(gx) + Math.cos(angle) * r * 1.15, Y(gy) + Math.sin(angle) * r * 1.0]],
        "#8a6a2c", 4, 0.4, 0.6,
      ));
    }
  }
  // **面部细节** ✓（走铅笔 ✓：闭目、眉、鼻、唇 ✓ —— 细线是"造人"的非人感来源 ✓）。
  mechanism.push(...handLine([[X(0.362), Y(0.296)], [X(0.390), Y(0.302)]], "#5c3f2b", 5, 0.4, 0.7));
  mechanism.push(...handLine([[X(0.404), Y(0.292)], [X(0.430), Y(0.300)]], "#5c3f2b", 5, 0.4, 0.7));
  mechanism.push(...handLine([[X(0.396), Y(0.300)], [X(0.390), Y(0.322)]], "#6d4a33", 4, 0.4, 0.6));
  mechanism.push(...handLine([[X(0.380), Y(0.328)], [X(0.404), Y(0.330)]], "#7a4a3a", 4, 0.45, 0.6));
  mechanism.push(...handLine([[X(0.372), Y(0.283)], [X(0.400), Y(0.278)]], "#4a3222", 4, 0.4, 0.6));
  // 额上一点高光 ✓（漆器的反光 ✓）。
  mechanism.push(...glow(X(0.418), Y(0.278), 16, "#fff0cc", "#c79a68", 2, 12));

  // ---- 偃师 ✓（侧脸剪影 + 轮廓光 ✓ —— 侧面轮廓是"人"读得出来的关键 ✓）----
  const artisan = [];
  const aFace = [X(0.680), Y(0.215)];
  // **侧脸轮廓** ✓：额 → 眉 → 鼻 → 唇 → 颏 → 颈 ✓（这一步比什么都重要 ✓）。
  const profile = [
    [X(0.690), Y(0.196)], [X(0.716), Y(0.206)], [X(0.726), Y(0.238)],
    [X(0.734), Y(0.262)], [X(0.722), Y(0.268)], [X(0.736), Y(0.292)],
    [X(0.724), Y(0.302)], [X(0.732), Y(0.318)], [X(0.712), Y(0.330)],
    [X(0.700), Y(0.348)], [X(0.676), Y(0.352)], [X(0.652), Y(0.330)],
    [X(0.646), Y(0.280)],
  ];
  artisan.push(...fillPolygon(profile,
    (t, span) => (span === 0 ? cool("#2c2c38", 0.35) : lit("#3a3540", X(0.69), Y(0.20) + t * (Y(0.35) - Y(0.20)), 1.15)),
    34, 9, 0.35, 0,
  ));
  // 身躯 ✓（深衣 ✓，几乎全在暗部 ✓ ⇒ 只给边缘一道光 ✓）。
  artisan.push(...fillPolygon(
    [[X(0.628), Y(0.330)], [X(0.760), Y(0.300)], [X(0.930), Y(0.520)],
     [X(0.985), Y(0.985)], [X(0.660), Y(0.995)], [X(0.612), Y(0.640)]],
    (t) => cool(lit("#22242f", X(0.80), Y(0.33) + t * (Y(0.99) - Y(0.33)), 1.25), 0.3),
    120, 26, 0.3, 0,
  ));
  // 前臂 ✓（伸向造人的胸口 ✓）。
  artisan.push(...fillPolygon(
    [[X(0.640), Y(0.470)], [X(0.700), Y(0.452)], [X(0.610), Y(0.548)], [X(0.556), Y(0.532)]],
    (t) => lit("#6d4a38", X(0.63), Y(0.47) + t * (Y(0.55) - Y(0.47)), 0.9),
    44, 11, 0.4, 0,
  ));
  // **手** ✓（受光最亮的一小块 ✓ = 画面的"第二眼" ✓）：掌 + 三根指 ✓。
  artisan.push(...fillPolygon(
    ellipse(X(0.560), Y(0.556), 40, 26, 16),
    (t, span) => (span === 0 ? warm("#d8a976", 0.35) : lit("#c08f60", X(0.56), Y(0.556), 0.7)),
    30, 8, 0.38, 0,
  ));
  for (let finger = 0; finger < 3; finger++) {
    artisan.push(...handLine(
      [[X(0.520), Y(0.540) + finger * 13], [X(0.470), Y(0.548) + finger * 14]],
      warm(lit("#c99a6c", X(0.50), Y(0.545), 0.7), 0.2), 9, 0.4, 1.0,
    ));
  }
  // 轮廓光 ✓（沿左侧全程 ✓ ⇒ 从暗背景里"抠"出来 ✓）。
  artisan.push(...rimLight(
    [[X(0.646), Y(0.282)], [X(0.630), Y(0.330)], [X(0.614), Y(0.470)],
     [X(0.610), Y(0.640)], [X(0.648), Y(0.800)], [X(0.690), Y(0.930)],
     [X(0.700), Y(0.980)]],
    warm("#c98a4a", 0.55), 10, 0.45,
  ));
  artisan.push(...rimLight([[X(0.690), Y(0.196)], [X(0.716), Y(0.206)], [X(0.726), Y(0.240)]],
    warm("#e0a86a", 0.5), 8, 0.45));

  // ---- 台与器 ✓ ----
  const bench = [];
  bench.push(...fillPolygon(
    [[-40, Y(0.885)], [W + 40, Y(0.870)], [W + 40, Y(0.925)], [-40, Y(0.940)]],
    (t) => lit("#4a3a2a", X(0.5), Y(0.88) + t * (Y(0.94) - Y(0.88)), 1.1),
    80, 20, 0.4, 0,
  ));
  bench.push(...rimLight([[-40, Y(0.885)], [W + 40, Y(0.870)]], warm("#a8763c", 0.45), 9, 0.45));
  bench.push(...handLine([[X(0.16), Y(0.975)], [X(0.30), Y(0.930)]], lit("#9a8a70", X(0.22), Y(0.95), 0.9), 11, 0.35, 2.0));
  bench.push(...handLine([[X(0.30), Y(0.930)], [X(0.345), Y(0.950)]], "#6d5a44", 8, 0.35, 1.2));
  for (let loop = 0; loop < 4; loop++) {
    bench.push(...handLine(ellipse(X(0.115), Y(0.958), 44 + loop * 7, 15, 16), "#7a6648", 5, 0.35, 1.2));
  }

  // ---- 提点与暗角 ✓ ----
  const light = [];
  light.push(...glow(lamp[0], lamp[1] - 30, 44, "#fffdf2", "#ffd18a", 3, 18));
  // 受光边缘的几处**小提亮** ✓（视线落点 ✓）。
  for (const [fx, fy, r] of [[0.474, 0.480, 20], [0.560, 0.556, 18], [0.418, 0.278, 14], [0.492, 0.676, 16]]) {
    light.push(...glow(X(fx), Y(fy), r, "#fff0cc", "#c98a5a", 2, 11));
  }
  // **暗角** ✓：沿四条边压暗 ✓ —— **不是四个大方块** ✗（上一版就是那样把画面糊掉的 ✓）。
  // 做法是"贴着边、由外向内几道半透明深色笔触" ✓ —— 与真画家收边同一个意思 ✓。
  for (let ring = 0; ring < 4; ring++) {
    const inset = ring * 26;
    const color = mix("#0c0e15", "#1a1a24", ring / 3);
    const band = (points) => light.push(...fillPolygon(points, () => color, 120, 46, 0.85, 0));
    band([[-40, -40], [W + 40, -40], [W + 40, inset + 26], [-40, inset + 26]]);
    band([[-40, H - inset - 26], [W + 40, H - inset - 26], [W + 40, H + 40], [-40, H + 40]]);
    band([[-40, -40], [inset + 26, -40], [inset + 26, H + 40], [-40, H + 40]]);
    band([[W - inset - 26, -40], [W + 40, -40], [W + 40, H + 40], [W - inset - 26, H + 40]]);
  }

  return [
    { id: "room", name: "暗室", strokes: asMedium("oil", 0.5, room) },
    { id: "lamp", name: "油灯与光", strokes: asMedium("oil", 0.7, lampLight) },
    { id: "artisan", name: "偃师", strokes: asMedium("oil", 0.5, artisan) },
    { id: "figure", name: "造人", strokes: asMedium("oil", 0.5, figure) },
    { id: "mechanism", name: "机枢", strokes: asMedium("pencil", 0.35, mechanism) },
    { id: "bench", name: "台与器", strokes: asMedium("oil", 0.45, bench) },
    { id: "light", name: "提点与暗角", strokes: asMedium("oil", 0.7, light) },
  ];
}

const WORKS = {
  "sample-oil": { width: 900, height: 640, build: oilLandscape },
  // **《偃师造人》** ✓（用户点题 ✓）：竖幅 ✓ ⇒ 两个人物的高度与"俯视"的关系才立得住 ✓。
  "sample-yanshi": { width: 820, height: 1080, build: yanshiAutomaton },
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
for (const [workId, work] of Object.entries(WORKS)) {
  if (only && only !== workId) continue;
  const docId = docOverride || workId;
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
  const path = await shot(docOverride ? docId : workId);
  console.log(`    截图：${path || "（失败）"}`);
}
ws.close();
process.exit(0);
