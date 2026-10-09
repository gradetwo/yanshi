#!/usr/bin/env node
// **★ 三段分解的切点必须落在"当前层"上 ✓ ★**（**目标第 4 条 ✓**；第 524 轮 ✓）
//
// **它判什么** ✓（**带对照 ✓，因此有区分力 ✓**）：**5 层文档 ⇒ 改**中间层**** ⇒ 同一区域重渲：
//   **① 传 `active_layer` ⇒ `below_reuse` **必须自增**✓**（**∴ 该层以下的合成被复用 ✓**）；
//   **② 不传 ⇒ **不应自增**✗**（**∴ 切点在"最上层以外"⇒ 中间层的变化使指纹变 ⇒ 全量重算 ✓**）。
//
// **为什么这是新能力的关键判据** ✗：**旧的切点（**最上层以外 ✗**）在改中间层时会把它以上的
//   全部层重渲 ✓**（**白算 ✓**）⇒ **∴ 只有切在该层上才能"只重渲该层 ＋ 复用下方" ✓**。
//
// **变异** ✗：**服务端忽略 `active_layer`（**恒 `None` ✓**）⇒ **∴ 断言 ① 必红 ✓**。
//
// 用法：node scripts/tool-active-layer-split.mjs <base-url>
const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-active-layer-split.mjs <base-url>"); process.exit(2); }
const call = async (doc, token, tool, args) => (await (await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
  method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}),
})).json());
const reuse = async () => Number((await (await fetch(`${base}/health`)).json()).below_reuse ?? -1);

const main = async () => {
  const doc = "al_" + Date.now().toString(36);
  const token = (await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: 768, height: 768 }),
  })).json()).token;
  const ids = ["S0", "S1", "S2", "S3", "S4"];          // **∴ S2 ＝ 中间层 ✓**
  for (const id of ids) await call(doc, token, "create_layer", { layer_id: id, name: id });
  const stroke = (layer, dx) => call(doc, token, "brush_stroke", {
    layer_id: layer, brush: "100%_Opaque", size: 30,
    points: [[150 + dx, 150, 1.0], [350 + dx, 350, 1.0]],
    color: { r: 180, g: 60, b: 200, a: 255 }, preview: false,
  });
  for (const id of ids) await stroke(id, 0);
  const region = { x: 0, y: 0, w: 256, h: 256 };
  const render = (extra) => call(doc, token, "render_region", Object.assign({ region, raw: true }, extra || {}));

  // **先填一次 below ✓**
  await render();

  // **★ 关键前置条件 ✓ ★**：**外层块缓存（`region_cache` ✓）的键含**全层指纹**✗**
  //   ⇒ **∴ 只要文档有任何改动它就失效 ✓** ⇒ **∴ 但**笔触预览**也会改文档 ✓ ⇒
  //   **∴ 若不显式顶掉它 ⇒ 我的 `render_region` 会**被外层命中**⇒ **∴ 根本进不到
  //   `render_accumulation` ✗**（**实测：探针里 `bbox=(0,0,256,256)` 出现 0 次 ✗**）
  //   ⇒ **∴ 于是判据测的是**外层缓存**✗，而不是切点 ✓**。
  // **∴ 修法** ✓：**每次渲染前在**顶层（S4）**画一小笔 ✓** ——
  //   **∴ 顶层改动在**两种切点**下都不影响各自的指纹 ✓**
  //   （**切点=2 ⇒ 指纹 S0..S1 ✓；切点=4 ⇒ 指纹 S0..S3 ✓**）⇒ **∴ 只顶掉外层 ✓**。
  const bump = async (dx) => { await stroke("S4", dx); };

  // **★ 语义依据 ✓ ★**（第 542 轮 ✓）：**读**该次渲染自己的 `below_reused`**✗** ——
  //   **∴ 而不是全局计数 ✗**（**它会被笔触预览等其它渲染污染 ✓**）。
  //
  // **① 改中间层（S2）＋ 传 `active_layer="S2"` ⇒ 指纹 ＝ `layer_default,S0,S1` ✓
  //   ⇒ **∴ 改 S2 **不在**指纹里 ⇒ **∴ 应为 `true` ✓****
  await stroke("S2", 9);
  await bump(30);
  // **★ 先**预热**一次（**建立该区域的 below ✓**）⇒ 再**测量** ✗**（第 547 轮 ✓）：
  // **∴ `bump` 本身是一次渲染 ✓ ⇒ **∴ 它把自己那份**小 bbox** 的 tile 存进 below ✗
  // ⇒ **∴ 若直接测量 ⇒ 要的 tile 大多不在 ⇒ **∴ 恒 `false` ✗**（**与"切点没生效"无法区分 ✗**）
  // ⇒ **∴ 必须先渲一次同区域 ⇒ **∴ 于是测量那一次才有意义 ✓****。
  await render({ active_layer: "S2" });
  const r1 = await render({ active_layer: "S2" });

  // **② 改中间层 ＋ **不传**（**切点在"最上层以外"✓**）⇒ 指纹含 S2 ✓
  //   ⇒ **∴ 改它使其变 ⇒ **∴ 应为 `false` ✓**（**对照 ✓**）**
  await stroke("S2", 18);
  await bump(40);
  // **★ ② 必须**改动之后直接测量** ✓ ★**（第 555 轮 ✓）：**∴ **不预热** ✗** ——
  //   **∴ 因为预热与测量之间**没有任何改动 ✗ ⇒ **∴ 两次指纹相同 ⇒ **必然命中 ⇒ 恒 `true` ✗
  //   （**第 554 轮我正是这么写的 ✓ ⇒ **∴ 那是**断言设计错**✗，不是功能错 ✓**）**。
  // **∴ 现在**：预热那次（**① ✓**）用的是**切点 3**的指纹 ✓ ⇒ **∴ 而这次是**切点 5**✗
  //   ⇒ **∴ 指纹不同 ⇒ **∴ 外层跳过时**必然 miss below ⇒ `false` ✓ ⇒ **∴ 这才是对照 ✓****。
  const r2 = await render();

  console.log(`  ① 传 active_layer=S2 ⇒ below_reused = ${r1.below_reused}｜frame_reused = ${r1.frame_reused}`);
  console.log(`  ② 不传（对照）      ⇒ below_reused = ${r2.below_reused}｜frame_reused = ${r2.frame_reused}`);
  const bad = [];
  // **★ 先要一个**有效样本 ✗ ★**（第 959 轮 ✓）：**∴ 一个布尔字段的"**假**"**有**多种成因**✗**
  //   （**没复用 ✓／**根本没渲染 ✓／**没命中 ✓）** ⇒ **∴ 必须先排除"**没渲染 ✓"** ✓**** ✓✓
  //   **∴ 依据 ✗**：`tools.rs:3941` 把 `below_reused` 定义成
  //     "**本次渲染**是否让 below 复用计数**增加** ✓" ⇒ **∴ 而**整幅命中时**渲染器不跑**✗**
  //       ⇒ **∴ 计数不动 ⇒ `below_reused=false`**✓**（**`document.rs:277` 的注释写明"**永远为假 ✓"）**
  //         ⇒ **★ 若不排除它 ✗**，**就会把"**没渲染**"**读成"**没复用 ✓" ✓ ★**** ✓✓
  if (r1.frame_reused === true) {
    bad.push("① 的样本无效：整幅缓存命中（`frame_reused=true`）⇒ 渲染器没跑 ⇒ " +
      "**∴ `below_reused=false` 说明的是「没渲染」✗，而不是「没复用」✓**");
  }
  if (r1.below_reused !== true) bad.push(`① 传 active_layer 时应为 true ✗（实测 ${r1.below_reused}）`);
  if (r2.below_reused !== false) bad.push(`② 不传时应为 false ✗（实测 ${r2.below_reused}）`);
  if (bad.length) {
    console.error("❌ " + bad.join("｜") +
      "（变异：服务端忽略 active_layer ⇒ 断言 ① 必红 ✓）");
    process.exit(1);
  }
  console.log("  ✓ 语义依据成立：传参 ⇒ 复用下方（true ✓）；不传 ⇒ 不复用（false ✓）");
};

main().catch((e) => { console.error("❌ 运行失败：" + String(e)); process.exit(2); });
