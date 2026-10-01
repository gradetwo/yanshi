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
import { statfsSync } from "node:fs";

const args = process.argv.slice(2);
const argOf = (name, fallback) => {
  const index = args.indexOf(name);
  return index >= 0 && args[index + 1] ? args[index + 1] : fallback;
};
const only = argOf("--only", null);
const port = Number(argOf("--port", "8110"));
const cdp = Number(argOf("--cdp", "9333"));
const shots = argOf("--shots", "/tmp/yanshi-samples");

/// **开画前先看剩余空间** ✓ —— 这条来自一次真实的翻车 ✓：
/// 一幅全尺寸作品要往 CAS 里写**成百上千张印章位图** ✓（`--draft` 只是拉开笔距 ✓，
/// 不改画面尺寸 ✓ ⇒ 单张位图并不变小 ✓），写到一半 `ENOSPC` ✓ ⇒
/// 不但这幅画废了 ✓，**整台机器上的工具全部失效** ✓（连"删除"都执行不了 ✓ ——
/// 因为工具要先建输出文件才运行命令 ✗）。**宁可开画前拒绝，也不要画到一半把磁盘写满** ✓。
/// **每一个"要写东西"的文件系统都要看** ✓ —— 只看其中一个是不够的 ✗：
/// 2026-10-01 那次翻车正是这样 ✓ —— 根分区还有 **68 GB** ✓，而 **`/tmp` 只剩 1.81 GB** ✗
///（`/tmp` 在本机是**另一个、小得多的**文件系统 ✓），我却把渲染的整个工作区放在 `/tmp` ✗
/// ⇒ 撑爆 `/tmp` ✓；而**工具自己的输出文件也走 `/tmp`** ✗ ⇒ 连"删除"都执行不了 ✓，
/// 整个开发流程一起趴下 ✓。教训：**先看清数据落在哪个挂载点** ✓，再谈"磁盘够不够" ✓。
/// **判据要分清"谁需要多大"** ✓：真正吃空间的是**工作区**（几百张印章位图 ✓）⇒ 它所在的
/// 文件系统要留足 ✓；而 `/tmp` 只需要装得下**小文件**（工具的输出捕获 ✓、node 编译缓存 ✓）
/// ⇒ 对它要求 3 GB 会**误伤**本来没问题的渲染 ✗（本机 `/tmp` 是个约 2 GB 的小挂载点 ✓）。
const REQUIRED_FOR_WORKSPACE = 3 * 1024 * 1024 * 1024;   // 3 GB ✓
const REQUIRED_FOR_TMP = 256 * 1024 * 1024;              // 256 MB ✓
const seen = new Set();
const mounts = [];
for (const path of [".", "/tmp"]) {
  try {
    const stats = statfsSync(path);
    if (seen.has(path)) continue;
    seen.add(path);
    mounts.push({
      path,
      bytes: stats.bavail * stats.bsize,
      required: path === "/tmp" ? REQUIRED_FOR_TMP : REQUIRED_FOR_WORKSPACE,
    });
  } catch (_) { /* 这个路径读不到就跳过 ✓ */ }
}
const starved = mounts.filter((mount) => mount.bytes < mount.required);
for (const mount of mounts) {
  const needed = mount.required / 1024 ** 3;
  console.log(
    `开画前检查：${mount.path} 剩余 ${(mount.bytes / 1024 ** 3).toFixed(2)} GB` +
    `（需要 ${needed < 1 ? Math.round(mount.required / 1024 ** 2) + " MB" : needed.toFixed(0) + " GB"}）` +
    `${mount.bytes < mount.required ? " ✗" : " ✓"}`);
}
if (starved.length > 0) {
  console.error(
    `剩余空间不足：${starved.map((m) => `${m.path} 只剩 ${(m.bytes / 1024 ** 3).toFixed(2)} GB`).join("；")}。` +
    `注意：渲染的工作区与 `+"`--shots`"+` 输出都**必须放在空间充足的那个文件系统上** ✓ ——` +
    `把它们放进一个小挂载点（例如本机的 /tmp）会把那个挂载点写满，` +
    `连带让所有依赖 /tmp 的工具一起失效。`);
  process.exit(2);
}
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
/// **笔距必须小于笔尖** ✓ —— 这条同样是实拍换来的 ✗：
/// 铺色层的笔距若接近或超过笔尖 ✓ ⇒ 覆盖率只剩一两成 ✓ ⇒ **底子整片露白** ✓（第三版实拍就是"白底划痕" ✓）。
/// 而且我犯这个错的方式很隐蔽 ✓：**把两个参数写反了** ✓（`fillPolygon` 的第 3、4 位是"笔尖、笔距" ✓），
/// 于是"150px 的大笔"变成了"150px 的笔距" ✗ ⇒ 一眼看不出，只有渲染出来才知道 ✗。
/// 所以：**比例不对就当场报错** ✓，连"参数写反"这种错也一并拦住 ✓。
const MAX_GAP_RATIO = 0.5;
function fillPolygon(points, colorAt, size, gap, wetness, angle = 0) {
  if (!(size > 0) || !(gap > 0)) {
    throw new Error("fillPolygon 的笔尖/笔距必须是正数（收到 size=" + size + " gap=" + gap + "）");
  }
  if (gap > size * MAX_GAP_RATIO) {
    throw new Error(
      "fillPolygon 的笔距 " + gap + "px 超过笔尖 " + size + "px 的 " +
      Math.round(MAX_GAP_RATIO * 100) + "%：覆盖率会低到露出底子（第三版实拍如此）。" +
      "铺色请把笔距压到笔尖的三分之一左右；另外别忘了参数顺序是 (…, size, gap, …)。");
  }
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
/// **安全半径** ✓ —— 这条不是口味问题，是**两次实拍**换来的规矩 ✗：
/// `glow` 是用**一圈一圈的几何环**假装柔光的 ✓，而**不透明的鬃毛笔会把环本身画出来** ✓。
/// 半径小的时候（几十像素 ✓）环与环叠在一起 ✓ 看不出 ✓；半径一大（几百像素 ✓）就成**靶心** ✗ ——
/// 第二版与这一版各中一次 ✓（我诊断过一次还再犯 ✓ ⇒ 所以规则必须**落在代码里** ✓，不能靠记性 ✗）。
/// **要画大范围的光，正确做法是"大量短笔 + 逐笔湿度"** ✓（见罩染层的写法 ✓），不是加大半径 ✗。
const GLOW_SAFE_RADIUS = 90;
function glow(cx, cy, radius, inner, outer, steps, size) {
  if (radius > GLOW_SAFE_RADIUS) {
    throw new Error(
      "glow 半径 " + Math.round(radius) + "px 超过安全上限 " + GLOW_SAFE_RADIUS + "px：" +
      "大半径的几何环会被不透明介质画成靶心（已实测两次）。要画大范围的光，" +
      "请改用大量短笔 + 逐笔湿度（罩染层的做法）。");
  }
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

// ---- 作品：《偃师造人》 ✓（按真实油画工序重画 ✓）----
//
// **立意** ✓（《列子·汤问》）：匠人偃师献"倡者"——能歌善舞的**造人**；
// 王怒其挑逗侍妾，偃师剖之，竟**皮革木胶漆**所制 ✓。
//
// ## 这一版为什么重写 ✓
//
// 之前几版是"**一笔一形状**"堆出来的 ✓：每个部件各自填色、各自加高光 ✓ ⇒
// 渲染出来是**一堆贴纸** ✗（我自己看图能指出的毛病 ✓：明暗没有设计 ✓、人物没有内部塑形 ✓、
// 构图静止 ✓、冷暖不分 ✓、边缘一样虚 ✗）。
// 现在按**真实油画的工序**来 ✓，这也是用户点出的办法 ✓：**先大块** ✓，再逐步收 ✓。
//
// ## 工序（每步一个图层 ✓ —— 图层多不是浪费 ✓，是能分别改 ✓）
//
// | 层 | 干什么 | 笔 |
// |---|---|---|
// | `ground` 底色 | 暖灰褐**薄涂**打底 ✓（不是白布 ✗：白底上直接画暗部会"浮" ✓） | 特大笔、稀 |
// | `dark_mass` 暗块 | **先压大暗部** ✓：房间、地面、两人身后的暗 ✓ | 大笔、薄 |
// | `block_in` 大块 | **两大块中间调** ✓：造人（暖）与偃师（冷）**只铺平均色** ✓，不看细节 ✓ | 大笔 |
// | `light_side` 受光 | 灯光**照到的那些面** ✓：肩、胸缘、额、颧、偃师的侧脸边 ✓ | 中大笔、**亮而干净** ✓ |
// | `halftone` 过渡 | 亮面与暗面之间的**中间调** ✓（少画 ✓，画多了就糊 ✗） | 中笔 |
// | `darks` 压深 | 最深的那几处 ✓：胸腔内、颌下、衣褶、脚下 ✓ | 中笔、少 |
// | `accents` 点睛 | **只有这几笔是硬的** ✓：两枚铜枢、灯芯、眉眼、偃师的轮廓光 ✓ | 小笔、厚 |
// | `glaze` 罩染 | 靠灯的暖罩 + 四边压暗 ✓（统一画面 ✓） | 大笔、极薄 |
//
// **画法纪律** ✓（都写进参数里 ✓）：
// * **笔尖先大后小** ✓（大块用 110~150px ✓、点睛用 6~12px ✓）—— 这是"先大块"在代码里的样子 ✓；
// * **不铺满** ✗：大块之间留底色透出来 ✓（画满就死 ✗）；
// * **硬边只留给焦点** ✓（胸腔与铜枢 ✓，其余一律柔 ✓）；
// * **冷暖分开** ✓：灯侧一律 `warm` ✓、影侧一律 `cool` ✓ —— 夜室内景的命门 ✓。
function yanshiAutomaton(width, height) {
  const W = width;
  const H = height;
  const X = (f) => W * f;
  const Y = (f) => H * f;
  const lamp = [X(0.845), Y(0.40)];
  const distToLamp = (x, y) => Math.hypot(x - lamp[0], y - lamp[1]) / (W * 0.95);
  // **统一光照** ✓：一切明暗由"离灯多远"推出 ✓（受光暖 ✓、背光冷 ✓）。
  const lit = (base, x, y, strength = 1) => {
    const t = Math.min(1, distToLamp(x, y) * strength);
    return mix(shade(base, -0.55 - 0.25 * t), base, Math.pow(1 - t, 1.4));
  };
  const warm = (base, amount) => mix(base, "#ffcf8a", amount);
  const cool = (base, amount) => mix(base, "#2a2f45", amount);

  // ==== ① 底色（薄涂 ✓ 留出笔痕 ✓）====
  const ground = fillPolygon(
    [[-40, -40], [W + 40, -40], [W + 40, H + 40], [-40, H + 40]],
    (t) => mix("#2a2119", "#4a3a28", Math.pow(t, 1.3)),
    150, 48, 0.3, Math.PI / 2,
  );
  ground.push(...fillPolygon(
    [[-40, -40], [W + 40, -40], [W + 40, H + 40], [-40, H + 40]],
    (t) => mix("#241d16", "#43331f", Math.pow(t, 1.3)),
    130, 46, 0.26, 0.35,
  ));

  // ==== ② 大暗块（房间与地面 ✓ 先压住 ✓）====
  const darkMass = [];
  darkMass.push(...fillPolygon(
    [[-40, -40], [W * 0.62, -40], [W * 0.58, H + 40], [-40, H + 40]],
    () => cool("#191922", 0.3), 142, 46, 0.28, Math.PI / 2,
  ));
  darkMass.push(...fillPolygon(
    [[W * 0.60, -40], [W + 40, -40], [W + 40, H + 40], [W * 0.56, H + 40]],
    (t) => mix("#241a14", "#2b2118", 1 - Math.min(1, t)), 132, 44, 0.26, Math.PI / 2,
  ));
  darkMass.push(...fillPolygon(
    [[-40, Y(0.86)], [W + 40, Y(0.845)], [W + 40, H + 40], [-40, H + 40]],
    (t) => mix("#3a2c1e", "#16110c", Math.pow(t, 1.1)), 112, 38, 0.3, Math.PI / 2,
  ));

  // ==== ③ 大块（只看平均色 ✓ 不看细节 ✓ —— "先大块"的核心 ✓）====
  const blockIn = [];
  // 造人：**暖** ✓（朱漆在暖光下 ✓）。
  blockIn.push(...fillPolygon(
    [[X(0.300), Y(0.372)], [X(0.492), Y(0.360)], [X(0.512), Y(0.560)],
     [X(0.486), Y(0.700)], [X(0.318), Y(0.706)], [X(0.288), Y(0.556)]],
    (t, span) => (span === 0 ? cool("#6b2c22", 0.4) : lit("#9c4630", X(0.40), Y(0.38) + t * (Y(0.70) - Y(0.38)), 1.0)),
    98, 32, 0.4, 0,
  ));
  // 下裳 + 腿：一块更暗的中间调 ✓。
  blockIn.push(...fillPolygon(
    [[X(0.306), Y(0.698)], [X(0.492), Y(0.694)], [X(0.536), Y(0.888)], [X(0.272), Y(0.892)]],
    (t) => lit("#7a3628", X(0.40), Y(0.70) + t * (Y(0.89) - Y(0.70)), 1.05),
    106, 35, 0.38, 0,
  ));
  // 头：**一整块**平均色 ✓（先在下面画一个大椭圆 ✓，五官最后再说 ✓）。
  blockIn.push(...fillPolygon(
    ellipse(X(0.395), Y(0.302), W * 0.076, H * 0.058, 26),
    (t, span) => (span === 0 ? cool("#8a6a4a", 0.35) : lit("#b98f63", X(0.395), Y(0.302), 0.8)),
    76, 26, 0.42, 0,
  ));
  // 偃师：**冷**的大块 ✓（深蓝灰 ✓）—— 与造人的暖形成全画的冷暖对撞 ✓。
  blockIn.push(...fillPolygon(
    [[X(0.628), Y(0.318)], [X(0.772), Y(0.300)], [X(0.936), Y(0.520)],
     [X(0.992), Y(0.985)], [X(0.660), Y(0.998)], [X(0.606), Y(0.632)]],
    (t) => cool(lit("#2a2c3a", X(0.80), Y(0.32) + t * (Y(0.99) - Y(0.32)), 1.25), 0.42),
    134, 44, 0.3, 0,
  ));
  blockIn.push(...fillPolygon(profileish(X, Y, W, H),
    () => cool("#2b2b38", 0.5), 64, 22, 0.4, 0,
  ));

  // ==== ④ 受光面（**亮而干净** ✓ —— 靠"面"而不是渐变 ✓）====
  const lightSide = [];
  const face = (points, color) => lightSide.push(...fillPolygon(
    points, () => color, 54, 18, 0.42, 0,
  ));
  // 造人的肩、胸缘、下裳的受光面 ✓。
  face([[X(0.452), Y(0.362)], [X(0.492), Y(0.360)], [X(0.512), Y(0.560)],
        [X(0.470), Y(0.586)], [X(0.452), Y(0.470)]], warm("#c06a3e", 0.35));
  face([[X(0.318), Y(0.372)], [X(0.352), Y(0.368)], [X(0.344), Y(0.470)],
        [X(0.302), Y(0.470)]], warm("#b25c34", 0.22));
  face([[X(0.360), Y(0.700)], [X(0.470), Y(0.696)], [X(0.492), Y(0.792)],
        [X(0.352), Y(0.798)]], warm("#a8542f", 0.18));
  // 头：**朝灯的那半张脸** ✓（右半 ✓）+ 额与颧各一小块 ✓ —— 三块面就够 ✓。
  face([[X(0.398), Y(0.256)], [X(0.436), Y(0.268)], [X(0.444), Y(0.316)],
        [X(0.412), Y(0.344)], [X(0.396), Y(0.330)]], warm("#e2bb8c", 0.4));
  face(ellipse(X(0.424), Y(0.284), W * 0.026, H * 0.018, 12), warm("#f0d0a4", 0.3));
  face(ellipse(X(0.428), Y(0.312), W * 0.020, H * 0.014, 12), warm("#e8c091", 0.3));
  // 偃师侧脸的受光边 ✓（窄窄一道 ✓ —— 剪影靠这一道才立得住 ✓）。
  face([[X(0.716), Y(0.222)], [X(0.734), Y(0.244)], [X(0.740), Y(0.286)],
        [X(0.726), Y(0.300)], [X(0.712), Y(0.270)]], warm("#7d6a6e", 0.35));

  // ==== ⑤ 中间调过渡（**少画** ✓ 画多了就糊 ✗）====
  const halftone = [];
  halftone.push(...fillPolygon(
    [[X(0.352), Y(0.560)], [X(0.452), Y(0.556)], [X(0.446), Y(0.700)], [X(0.346), Y(0.702)]],
    (t) => mix("#6a3226", "#8f4430", t), 60, 20, 0.35, 0,
  ));
  halftone.push(...fillPolygon(
    [[X(0.646), Y(0.400)], [X(0.760), Y(0.372)], [X(0.836), Y(0.560)],
     [X(0.742), Y(0.660)], [X(0.640), Y(0.600)]],
    (t) => cool(mix("#22242e", "#33364a", t), 0.35), 78, 26, 0.3, 0,
  ));

  // ==== ⑥ 压深（只在最深的几处 ✓）====
  const darks = [];
  darks.push(...fillPolygon(
    [[X(0.336), Y(0.478)], [X(0.478), Y(0.472)], [X(0.472), Y(0.628)], [X(0.342), Y(0.634)]],
    (t) => mix("#241610", "#100b08", t), 58, 19, 0.4, 0,
  ));
  darks.push(...fillPolygon(
    [[X(0.352), Y(0.344)], [X(0.404), Y(0.352)], [X(0.386), Y(0.372)], [X(0.344), Y(0.362)]],
    () => "#3d2a1c", 36, 12, 0.4, 0,
  ));
  for (let fold = 0; fold < 4; fold++) {
    const fx = 0.322 + fold * 0.042;
    darks.push(...handLine(
      [[X(fx), Y(0.712)], [X(fx + 0.012), Y(0.800)], [X(fx - 0.004), Y(0.878)]],
      cool("#4a1f16", 0.2), 16, 0.32, 2.4,
    ));
  }
  darks.push(...fillPolygon(
    [[-40, Y(0.945)], [W + 40, Y(0.930)], [W + 40, H + 40], [-40, H + 40]],
    () => "#0e0b09", 84, 28, 0.35, 0,
  ));

  // ==== ⑦ 点睛（**全画只有这几笔是硬的** ✓）====
  const accents = [];
  // 胸腔的木肋 ✓（细 ✓ 直 ✓）+ **两枚铜枢** ✓（最高对比 ✓ = 焦点 ✓）。
  for (let rib = 0; rib < 6; rib++) {
    const y = Y(0.492) + rib * 22;
    accents.push(...handLine([[X(0.348), y], [X(0.466), y - 4]],
      lit(rib % 2 ? "#9a6d3c" : "#7a5530", X(0.42), y, 0.85), 6, 0.35, 1.4));
  }
  accents.push(...handLine([[X(0.402), Y(0.482)], [X(0.406), Y(0.630)]], lit("#a87c46", X(0.40), Y(0.55), 0.9), 8, 0.35, 1.0));
  for (const spec of [[0.374, 0.590, 16], [0.438, 0.610, 12]]) {
    accents.push(...fillPolygon(ellipse(X(spec[0]), Y(spec[1]), spec[2], spec[2] * 0.9, 14),
      () => warm("#e0b45c", 0.35), 10, 3.0, 0.4, 0));
    accents.push(...rimLight(ellipse(X(spec[0]), Y(spec[1]), spec[2] * 0.72, spec[2] * 0.62, 10), "#fff0c0", 4, 0.4));
    for (let tooth = 0; tooth < 6; tooth++) {
      const angle = (tooth / 6) * Math.PI * 2;
      accents.push(...handLine(
        [[X(spec[0]) + Math.cos(angle) * spec[2] * 0.85, Y(spec[1]) + Math.sin(angle) * spec[2] * 0.75],
         [X(spec[0]) + Math.cos(angle) * spec[2] * 1.18, Y(spec[1]) + Math.sin(angle) * spec[2] * 1.0]],
        "#8a6a2c", 4, 0.4, 0.6,
      ));
    }
  }
  // 眉眼与唇 ✓（**几笔** ✓，不是"画五官" ✗）。
  accents.push(...handLine([[X(0.372), Y(0.286)], [X(0.398), Y(0.290)]], "#40291c", 5, 0.45, 0.7));
  accents.push(...handLine([[X(0.408), Y(0.288)], [X(0.432), Y(0.294)]], "#40291c", 5, 0.45, 0.7));
  accents.push(...handLine([[X(0.386), Y(0.306)], [X(0.400), Y(0.310)]], "#5c3a24", 4, 0.45, 0.6));
  accents.push(...handLine([[X(0.376), Y(0.326)], [X(0.404), Y(0.328)]], "#7a4436", 4, 0.45, 0.6));
  // 灯芯与外圈 ✓（第二焦点 ✓）。
  accents.push(...glow(lamp[0], lamp[1] - 4, 34, "#fffaf0", "#ffc46a", 4, 22));
  accents.push(...fillPolygon(
    [[lamp[0] - 24, lamp[1] + 20], [lamp[0] + 24, lamp[1] + 20],
     [lamp[0] + 15, lamp[1] + 46], [lamp[0] - 15, lamp[1] + 46]],
    (t) => lit("#8a6a34", lamp[0], lamp[1] + 20 + t * 26, 0.7), 16, 7, 0.45, 0,
  ));
  // 偃师的轮廓光 ✓（从暗背景里"抠"出来 ✓）+ 面部几笔 ✓。
  accents.push(...rimLight(
    [[X(0.646), Y(0.286)], [X(0.628), Y(0.330)], [X(0.610), Y(0.470)],
     [X(0.606), Y(0.632)], [X(0.648), Y(0.800)], [X(0.688), Y(0.930)], [X(0.700), Y(0.980)]],
    warm("#c98a4a", 0.5), 9, 0.45,
  ));
  accents.push(...handLine([[X(0.686), Y(0.262)], [X(0.712), Y(0.268)]], "#12121a", 6, 0.4, 0.8));

  // ==== ⑧ 罩染（靠灯的暖罩 ✓ + 四边压暗 ✓ —— 统一画面 ✓）====
  const glaze = [];
  // **大范围的暖雾：大量短笔，而不是大半径的 glow** ✗ —— 见 `GLOW_SAFE_RADIUS` 的说明 ✓。
  // 画法照搬真画家"扫"的动作 ✓：从灯心向外一圈圈扫 ✓，每笔都短 ✓、越远越淡越冷 ✓，
  // 笔触之间**故意留缝** ✓ ⇒ 出来是雾 ✓ 不是环 ✓。
  for (let ring = 1; ring <= 7; ring++) {
    const radius = W * 0.045 * ring;
    const t = ring / 7;
    const color = mix(mix("#ffdc9e", "#8a6a44", t), "#241d16", Math.pow(t, 1.5));
    const rays = 22 + ring * 6;
    for (let ray = 0; ray < rays; ray++) {
      const angle = (ray / rays) * Math.PI * 2 + ring * 0.21;
      const r0 = radius - W * 0.03;
      const cx = lamp[0] + Math.cos(angle) * r0;
      const cy = lamp[1] + Math.sin(angle) * r0 * 0.95;
      const cx1 = lamp[0] + Math.cos(angle) * radius;
      const cy1 = lamp[1] + Math.sin(angle) * radius * 0.95;
      glaze.push(...handLine([[cx, cy], [cx1, cy1]], color, 34 - ring * 2, 0.22, 1.0));
    }
  }
  for (let ring = 0; ring < 2; ring++) {
    const inset = ring * 20;
    const color = mix("#0a0b10", "#141620", ring);
    glaze.push(...fillPolygon([[-40, -40], [W + 40, -40], [W + 40, inset + 20], [-40, inset + 20]],
      () => color, 130, 44, 0.7, 0));
    glaze.push(...fillPolygon([[-40, H - inset - 20], [W + 40, H - inset - 20], [W + 40, H + 40], [-40, H + 40]],
      () => color, 130, 44, 0.7, 0));
    glaze.push(...fillPolygon([[-40, -40], [inset + 20, -40], [inset + 20, H + 40], [-40, H + 40]],
      () => color, 130, 44, 0.7, 0));
    glaze.push(...fillPolygon([[W - inset - 20, -40], [W + 40, -40], [W + 40, H + 40], [W - inset - 20, H + 40]],
      () => color, 130, 44, 0.7, 0));
  }

  return [
    { id: "ground", name: "① 底色（薄涂）", strokes: asMedium("oil", 0.5, ground) },
    { id: "dark_mass", name: "② 大暗块", strokes: asMedium("oil", 0.55, darkMass) },
    { id: "block_in", name: "③ 大块（先大块）", strokes: asMedium("oil", 0.5, blockIn) },
    { id: "light_side", name: "④ 受光面", strokes: asMedium("oil", 0.45, lightSide) },
    { id: "halftone", name: "⑤ 中间调", strokes: asMedium("oil", 0.5, halftone) },
    { id: "darks", name: "⑥ 压深", strokes: asMedium("oil", 0.45, darks) },
    { id: "accents", name: "⑦ 点睛", strokes: asMedium("pencil", 0.4, accents) },
    { id: "glaze", name: "⑧ 罩染", strokes: asMedium("oil", 0.6, glaze) },
  ];
}

/// 偃师的**侧脸轮廓** ✓（额→眉→鼻→唇→颏→颈 ✓）—— 抽成函数是因为"大块"与"受光面"两层都要用它 ✓。
function profileish(X, Y, W, H) {
  void W;
  void H;
  return [
    [X(0.690), Y(0.196)], [X(0.716), Y(0.206)], [X(0.726), Y(0.238)],
    [X(0.734), Y(0.262)], [X(0.722), Y(0.268)], [X(0.736), Y(0.292)],
    [X(0.724), Y(0.302)], [X(0.732), Y(0.318)], [X(0.712), Y(0.330)],
    [X(0.700), Y(0.348)], [X(0.676), Y(0.352)], [X(0.652), Y(0.330)],
    [X(0.646), Y(0.280)],
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
/// **导出的是"文档渲染"，不是"页面截图"** ✓ —— 这条是真实的翻车换来的 ✗：
/// 这里原本用 `Page.captureScreenshot` ✓ ⇒ 存下来的是**整个查看器界面** ✓（工具栏、面板全在画里 ✓）。
/// 我又把它当成示例图 `cp` 进了 `assets/samples/` ✗ ⇒ **两个后果一起发生** ✓：
/// ① 仓库里的示例图带着工具栏 ✗；② 查看器把它**种入文档** ✓ ⇒ 连文档的像素里都有工具栏 ✗（渲染实拍 ✓）。
/// 一个"导出错了对象"的小错 ✓ 同时污染了资源与文档 ✓ —— 所以这里改成取**文档渲染** ✓，
/// PNG 交给浏览器自己编码 ✓（`toDataURL` ✓，不必在 Node 里手写编码器 ✓）。
const shot = async (name) => {
  const message = await send("Runtime.evaluate", {
    expression: `(async () => {
      const size = state.docSize;
      const value = await callTool("render_region",
        { region: { x: 0, y: 0, w: size.w, h: size.h }, raw: true }, { refresh: false });
      const url = value.raw_url || value.url;
      if (!url) return null;
      const absolute = url.indexOf("http") === 0 ? url
        : url + (url.indexOf("?") >= 0 ? "&" : "?") + "token=" + state.token;
      const bytes = new Uint8ClampedArray(await fetch(absolute).then((r) => r.arrayBuffer()));
      if (bytes.length !== size.w * size.h * 4) return null;
      const canvas = document.createElement("canvas");
      canvas.width = size.w;
      canvas.height = size.h;
      canvas.getContext("2d").putImageData(new ImageData(bytes, size.w, size.h), 0, 0);
      return canvas.toDataURL("image/png");
    })()`,
    returnByValue: true,
    awaitPromise: true,
  });
  const data = message?.result?.result?.value;
  if (!data || String(data).indexOf("data:image/png") !== 0) return null;
  const path = `${shots}/${name}.png`;
  await writeFile(path, Buffer.from(String(data).split(",")[1], "base64"));
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
  // **改读页面自己公布的进度** ✓ —— 不再用"服务端原子数"推笔数 ✗：
  // 批量提交之后"一次提交 = 一批笔" ✓ ⇒ 原子数远小于笔数 ✓ ⇒ 按原子数判断会**提前收工** ✗
  //（本轮实测：只画了约 690 笔就截图 ✓，日志还写着"约 23 笔" ✗ —— 记账口径错了 ✓）。
  const progress = () => evaluate("(window.yanshiStats && window.yanshiStats.score) || null");
  let last = { strokesDone: 0, total: strokes, done: false, layersDone: 0 };
  let quiet = 0;
  for (let tick = 0; tick < 1800; tick++) {
    await new Promise((resolve) => setTimeout(resolve, 2000));
    const now = (await progress()) || last;
    if (now.strokesDone > last.strokesDone) quiet = 0; else quiet += 1;
    last = now;
    if (tick % 10 === 0) console.log(`    进度：${now.strokesDone}/${now.total} 笔`);
    if (now.done) break;
    // 兜底：页面真卡住了（120 秒毫无进展 ✓）就停，别无限等 ✗。
    if (quiet >= 60 && now.strokesDone > 0) break;
  }
  const atomCountNow = await atomCount();
  console.log(`    完成：${last.strokesDone}/${last.total} 笔（原子 ${atomCountNow}，脚本侧 ${Math.round((Date.now() - started) / 1000)} 秒）`);
  if (!last.done) console.log("    ✗ 注意：画谱**没有报告完成** ⇒ 下面这张截图可能是半成品 ✗");
  const path = await shot(docOverride ? docId : workId);
  console.log(`    截图：${path || "（失败）"}`);
}
ws.close();
process.exit(0);
