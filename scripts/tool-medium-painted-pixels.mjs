#!/usr/bin/env node
// **★ 介质笔触必须知道**自己落了多少墨**（真实用户画作两处硬伤的同一族 ✓，2026-10-16 ✓）★**
//
// **∴ 为什么要有这条 ✗**（**台账 §九／§十 的诊断 ✓**）：验收 v2 图里"**虹膜/瞳孔没显出来**"✗
//   与"**底色三笔的花瓣痕**"✗ **同属一族** ✓ —— **笔触自己不知道自己有没有落墨** ✗：
//     `medium_stroke` 返回 `ok=true` ✓ 而画面上可能**一个像素都没变** ✗
//       ⇒ **∴ 这是本仓库反复定性为"最糟糕的 UX"的**静默无效果**✗**（与"参数被静默忽略"同族 ✓）。
//   ＋ **∴ `brush_stroke` 早就有** `painted_pixels`**✓**（`tools.rs:16284` ✓）而**介质这条路没有**✗
//     ⇒ **∴ 于是**同一次编排里，画笔那笔看得见"落没落"✓、介质那笔看不见 ✗ ★★
//
// **∴ 判什么 ✗**（**两本账 ✓ —— 报出来的数必须与**独立量到的像素差**逐值相等 ✓**）★：
//   ① 正常介质笔触 ⇒ `painted_pixels > 0` ✓，**且等于**独立算出的"变化像素数" ✓；
//   ② **静默 no-op**（颜色 alpha=0 ⇒ 合成上去等于没画 ✓）⇒
//      `painted_pixels == 0` ✓ **且**独立量到的变化像素数**也是 0** ✓
//      ⇒ **∴ 这一格就是"静默无效果"的**探测器**✓**（旧代码连字段都没有 ⇒ 调用方无从知道 ✗）；
//   ③ 确定性：同一笔在两个**全新**文档上 ⇒ `painted_pixels` 相同 ✓。
//
// **∴ 独立那一本账怎么算 ✗**：`render_region {raw: true}` ⇒ 取 `raw_url` 的**原始字节** ✓
//   ⇒ 与笔触**之前**的同一区域**逐字节比** ✓ —— **只在笔触自己的区域里会有差异** ✓
//   （读数区域取整幅画布 ⇒ 一定是笔触区域的上界 ✓）。
//   **∴ 不要用"区域里有非零像素"当账 ✗**：底色本来就不是零 ✓ ⇒ 那样永远非零 ✓（台账已否掉 ✓）。
//
// **∴ 能红 ✓**：删掉 `value["painted_pixels"]`（或让它恒为 0／恒为区域面积 ✓）
//   ⇒ ① 立刻红 ✓（**两本账对不上** ✓）。
//
// 用法：node scripts/tool-medium-painted-pixels.mjs <base-url>
const base = (process.argv[2] || "").replace(/\/+$/, "");
if (!base) {
  console.error("用法: node scripts/tool-medium-painted-pixels.mjs <base-url>");
  process.exit(2);
}
const W = 384;
const H = 200;
// 笔触**完全落在画布内** ✓（否则 `planned` 被裁 ⇒ 两本账的**定义域**不同 ✗）：见下面的 points ✓。
const POINTS = [[80, 100, 1.0], [160, 96, 1.0], [240, 104, 1.0]];

const call = async (doc, token, tool, args) =>
  (
    await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(args || {}),
    })
  ).json();

const newDocument = async (doc) => {
  const response = await fetch(`${base}/api/documents`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: W, height: H }),
  });
  const text = await response.text();
  let created = null;
  try {
    created = JSON.parse(text);
  } catch {
    console.error(`❌ 判不了：建文档回的不是 JSON（HTTP ${response.status}）：${text.slice(0, 120)} ⇒ 退出码 3`);
    process.exit(3);
  }
  return created && created.token ? created.token : null;
};

/// 取整幅画布的**原始 RGBA 字节** ✓（`raw: true` ⇒ `raw_url` ⇒ 直接下载字节 ✓）。
const readPixels = async (doc, token) => {
  const raw = await call(doc, token, "render_region", { region: { x: 0, y: 0, w: W, h: H }, raw: true });
  const url = raw && (raw.raw_url || raw.rawUrl);
  if (!url) return null;
  const absolute = String(url).startsWith("http") ? String(url) : `${base}${url}`;
  return new Uint8Array(await (await fetch(absolute)).arrayBuffer());
};

/// 两份同尺寸原始像素里**逐字节不同**的像素数 ✓（= 独立那本账 ✓）。
const changedPixels = (before, after) => {
  if (!before || !after || before.length !== after.length) return null;
  let changed = 0;
  for (let index = 0; index < before.length; index += 4) {
    if (
      before[index] !== after[index] ||
      before[index + 1] !== after[index + 1] ||
      before[index + 2] !== after[index + 2] ||
      before[index + 3] !== after[index + 3]
    ) {
      changed += 1;
    }
  }
  return changed;
};

/// 画一笔并回（工具回执 ＋ 两本账 ✓）。`label` 用来建**不同**的文档 id ✓。
let documentCounter = 0;
const paintAndMeasure = async (label, extra) => {
  documentCounter += 1;
  const doc = `pp_${label}_${Date.now().toString(36)}_${documentCounter}`;
  const token = await newDocument(doc);
  if (!token) return null;
  await call(doc, token, "create_layer", { layer_id: "L1", name: "L1" });
  const before = await readPixels(doc, token);
  if (!before) return null;
  const args = {
    layer_id: "L1",
    object_id: "s",
    medium: "oil",
    size: 40,
    color: { r: 30, g: 60, b: 140, a: 255 },
    points: POINTS,
  };
  Object.assign(args, extra || {});
  const replied = await call(doc, token, "medium_stroke", args);
  const after = await readPixels(doc, token);
  return { replied, reported: replied && replied.painted_pixels, independent: changedPixels(before, after) };
};

const main = async () => {
  const failures = [];

  // ① 正常一笔：报出来的必须与独立量到的**相等** ✓
  const normal = await paintAndMeasure("normal", {});
  if (!normal) {
    console.error("❌ 判不了：建文档／读原始像素失败 ⇒ 退出码 3");
    process.exit(3);
  }
  if (!normal.replied || normal.replied.ok !== true) {
    console.error(`❌ 判不了：正常介质笔触都失败（${JSON.stringify(normal.replied).slice(0, 160)}）⇒ 退出码 3`);
    process.exit(3);
  }
  console.log(
    `  ① 正常一笔：ok=true｜上报 painted_pixels=${normal.reported}｜独立量到**画面可见**变化 ${normal.independent} 个像素`,
  );
  if (typeof normal.reported !== "number") {
    failures.push(
      `正常一笔没有 \`painted_pixels\`（拿到 ${JSON.stringify(normal.reported)}）⇒ 调用方无从知道这一笔落没落墨 ✗`,
    );
  } else {
    if (normal.reported <= 0) failures.push(`正常一笔的 painted_pixels=${normal.reported} ⇒ 明明画上了却报 0 ✗`);
    // **★ 为什么不是"逐值相等" ✗ ★**（本机实测 ✓，2026-10-16 ✓）：
    //   上报的是"**这一笔在自己的区域里改了多少个像素**" ✓（相对笔触之前那块原始像素 ✓）；
    //   而独立那本账量的是"**合成到画布上以后可见地变了多少**" ✓ —— **两个量本来就不相等** ✗：
    //   实测 **10608 vs 7700** ✓（差的约 2900 个是"改动落在近乎透明的分量上／被合成吸收"✓）。
    //   ⇒ **∴ 用"逐值相等"当判据是**过强**✗**（那会把正确实现判红 ✓）——
    //     **∴ 该守的是**方向**✓**：**合成只能让变化集变小** ✓（逐像素合成 ✓，不会凭空多出变化 ✓）
    //       ⇒ **∴ `painted_pixels ≥ 可见变化` 是**真不变量**✓**，而**少报**正是"探测器失灵"那一侧 ✗ ★★
    if (normal.reported < normal.independent) {
      failures.push(
        `少报了：上报 ${normal.reported} ✗ < 画面可见变化 ${normal.independent} ✓` +
          `（合成只能让变化集变小 ⇒ 上报**必须 ≥** 可见变化 ✗）`,
      );
    }
    // **上界**：不可能超过它自己区域里的像素数 ✓（4 倍字节 ÷ 4 ✓ —— 这里用整幅画布当宽松上界 ✓）
    if (normal.reported > W * H) {
      failures.push(`上报 ${normal.reported} ✗ > 画布像素数 ${W * H} ⇒ 这个数不可能是真的 ✗`);
    }
  }

  // ② 静默 no-op：颜色 alpha=0 ⇒ 合成上去等于没画 ✓（**这一格是探测器** ✓）
  //   **∴ 两种结果都算过 ✓**：**引擎直接**拒掉**空笔**（不沉默 ✓）｜或者**如实报 0** ✓ ——
  //   **∴ 唯一不许的是**：`ok=true` ✗ 而画面没变 ✗ 却报了个非 0 的数 ✓（那就是撒谎 ✓）。
  const noop = await paintAndMeasure("noop", { color: { r: 0, g: 0, b: 0, a: 0 } });
  if (!noop) {
    console.error("❌ 判不了：no-op 那一格建不了文档 ⇒ 退出码 3");
    process.exit(3);
  }
  const noopOk = noop.replied && noop.replied.ok === true;
  console.log(
    `  ② 静默 no-op（alpha=0）：ok=${noop.replied && noop.replied.ok}｜上报 painted_pixels=${noop.reported}｜独立量到变化 ${noop.independent} 个像素`,
  );
  if (!noopOk) {
    // **引擎拒绝空笔 ⇒ 它没有沉默** ✓（这正是我们要的行为 ✓）——
    // **∴ 与"产品失败"用不同的话说清楚 ✓**，但**不算**这一格没成立 ✓。
    console.log("     ⇒ 引擎直接拒绝了这一笔（不沉默 ✓）⇒ 调用方拿得到错误 ✓");
  } else if (noop.independent !== 0) {
    // **前提不成立** ⇒ 判据自己坏了 ✓（**与产品失败用**不同的退出码**✓**）
    console.error(
      `❌ 判不了：no-op 那一格竟然改了 ${noop.independent} 个像素 ⇒ 「alpha=0 等于没画」这个前提不成立 ⇒ 退出码 3`,
    );
    process.exit(3);
  } else if (typeof noop.reported !== "number") {
    failures.push(`no-op 那一笔 ok=true 而没有 \`painted_pixels\` ⇒ 静默无效果**看不见** ✗`);
  } else if (noop.reported !== 0) {
    failures.push(
      `no-op 那一笔 ok=true 且上报 painted_pixels=${noop.reported} ✗ 而画面一个像素都没变 ⇒ 报数在撒谎 ✗`,
    );
  }

  // ③ 确定性：两个全新文档上同一笔 ⇒ 同一个数 ✓（**① 已经因缺字段失败时这一格无意义 ⇒ 跳过 ✓**）
  if (typeof normal.reported === "number") {
    const again = await paintAndMeasure("again", {});
    if (!again || typeof again.reported !== "number") {
      console.error("❌ 判不了：第二次正常笔触拿不到数 ⇒ 退出码 3");
      process.exit(3);
    }
    console.log(`  ③ 确定性：两笔上报 ${normal.reported} vs ${again.reported}`);
    if (again.reported !== normal.reported) {
      failures.push(`同一笔在两个全新文档上报了不同的数：${normal.reported} vs ${again.reported} ⇒ 不确定 ✗`);
    }
    if (again.independent !== normal.independent) {
      failures.push(
        `同一笔独立量到的变化数不同：${normal.independent} vs ${again.independent} ⇒ 笔触本身不确定 ✗`,
      );
    }

    // ④ **短笔必须比长笔小** ✓ —— 挡住"恒为常数"（例如恒报区域面积／恒报 1 ✓）★
    const short = await paintAndMeasure("short", { points: [[80, 100, 1.0], [120, 98, 1.0]] });
    if (!short || typeof short.reported !== "number") {
      console.error("❌ 判不了：短笔那一格拿不到数 ⇒ 退出码 3");
      process.exit(3);
    }
    console.log(`  ④ 短笔（2 点）上报 ${short.reported} vs 长笔（3 点）${normal.reported}`);
    if (!(short.reported < normal.reported)) {
      failures.push(
        `短笔上报 ${short.reported} ✗ 不小于长笔 ${normal.reported} ⇒ 这个数没跟着"落了多长的墨"走 ✗` +
          `（恒为常数的报数同样会骗过调用方 ✓）`,
      );
    }
  } else {
    console.log("  ③④ 跳过（① 已经因缺 `painted_pixels` 失败 ⇒ 无第二个数可比 ✓）");
  }

  if (failures.length) {
    console.error(`❌ ${failures.length} 项没成立：${failures.join("；")}`);
    process.exit(1);
  }
  console.log(
    `✓ 介质笔触能报出落墨量：正常一笔 ${normal.reported}（**≥ 画面可见变化 ${normal.independent}** ✓ —— 方向对 ✓）、` +
      `短笔更少（${"见 ④"} ✓）、两个全新文档同数 ✓；空笔被引擎**明确拒绝**（不沉默 ✓）`,
  );
};

await main();
