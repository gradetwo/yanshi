#!/usr/bin/env node
// **★ 交替切点必须靠 `by_sig` 命中 ✗ ★**（**第 250 轮 ✓；**目标第 4 条 ✓）
//
// **∴ 为什么单独立一条 ✗**：**`tool-active-layer-split`** 覆盖的是"**改**切点**之上**的
//   那一层" ✗ ⇒ **∴ 那种情况**指纹根本没变 ✗ ⇒ **∴ 旧的单份字段就能命中** ✓
//     ＋ **∴ 而** `by_sig`（**按切点分组 ✓）**存在的**唯一理由**是**另一种情况** ✗：
//       **∴ 切点在两个值之间**来回切**✗ ⇒ **∴ 旧字段**每切一次就被覆盖一次** ✓
//         ⇒ **∴ 它**永远命不中 ✗ ⇒ **∴ 那正是**`by_sig` 要救的场** ✓ ★★
//     ＋ **∴ 但**（第 250 轮读代码发现 ✓）：**命中判定（`render.rs:1868`）**只看旧的
//       `c.sig` ✗** ⇒ **∴ `by_sig` 写了、探针也报了 ✗ 却**从不参与命中** ✓
//         ⇒ **∴ 于是**：**它的收益是** 0** ✓ ⇒ **∴ 本判据**就是来钉这一条的** ✓ ★★★
//
// **判据（两条，成对 ⇒ 有区分力 ✓）**：
//   ① **同一份下方内容 ✗ 切点在 S2 ↔ S4 之间来回切** ⇒ **∴ 回到 S2 那一次**必须
//      `below_reused === true` ✓（**∴ 那一组格在 `by_sig` 里还在 ✓**）；
//   ② **对照**：**改**切点**之下**的层（S0）⇒ **∴ `sig` 变了 ⇒ **哪一组都对不上**
//      ⇒ **∴ 必须 `below_reused === false`** ✓（**∴ 排除"**恒真**"这种假绿 ✓）★
//
// **变异（**能红的根据 ✓）**：**把命中判定退回只看 `c.sig`**（**第 250 轮之前的样子 ✓）
//   ⇒ **∴ 断言 ① 必红** ✓
//
// **为什么用 `below_reused` 而不是全局 `below_reuse`** ✓：**后者会被别的渲染污染** ✗
//   （**笔触预览等 ✓**）⇒ **∴ 只信**这一次渲染自己的读数** ✓
//
// 用法：node scripts/tool-below-sig-alternate.mjs <base-url>
const base = process.argv[2];
if (!base) {
  console.error("用法: node scripts/tool-below-sig-alternate.mjs <base-url>");
  process.exit(2);
}
const call = async (doc, token, tool, args) =>
  (
    await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(args || {}),
    })
  ).json();

const main = async () => {
  const doc = "sigalt_" + Date.now().toString(36);
  const created = await (
    await fetch(`${base}/api/documents`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ doc_id: doc, width: 768, height: 768 }),
    })
  ).json();
  if (!created || !created.token) {
    console.error(`❌ 判不了：建不了文档（${JSON.stringify(created).slice(0, 160)}）⇒ 退出码 3`);
    process.exit(3);
  }
  const token = created.token;
  const ids = ["S0", "S1", "S2", "S3", "S4"];
  for (const id of ids) await call(doc, token, "create_layer", { layer_id: id, name: id });
  const stroke = (layer, dx) =>
    call(doc, token, "brush_stroke", {
      layer_id: layer,
      brush: "100%_Opaque",
      size: 30,
      points: [
        [150 + dx, 150, 1.0],
        [350 + dx, 350, 1.0],
      ],
      color: { r: 180, g: 60, b: 200, a: 255 },
      preview: false,
    });
  for (const id of ids) await stroke(id, 0);
  const region = { x: 0, y: 0, w: 256, h: 256 };
  const render = (extra) =>
    call(doc, token, "render_region", Object.assign({ region, raw: true }, extra || {}));
  // **∴ 每次渲染前在**最顶层**（S4）画一小笔 ✗**：**∴ 只顶掉**外层区域缓存**
  //   ＋ **∴ 而**它对**两个切点**的指纹**都不影响**（**切点 2 ⇒ 指纹 S0..S1 ✓
  //     切点 4 ⇒ 指纹 S0..S3 ✓ ⇒ 都不含 S4 ✓）** ✓ —— 这是 `tool-active-layer-split`
  //   踩出来的前置条件 ✓，这里照抄 ✓。
  const bump = (dx) => stroke("S4", dx);

  // **∴ ① 切点 S2 ✗ 预热 ＋ 测量** ✓
  await bump(21);
  await render({ active_layer: "S2" });
  const first = await render({ active_layer: "S2" });

  // **∴ ② 切点 S4 ✗ 让旧字段被覆盖掉** ✓（**∴ 这一步就是"交替" ✓**）
  await bump(22);
  const other = await render({ active_layer: "S4" });

  // **∴ ③ 回到切点 S2 ✗**：**∴ 同一份下方内容 ⇒ **必须靠 `by_sig` 命中** ✓ ★★★
  await bump(23);
  const back = await render({ active_layer: "S2" });

  // **∴ ④ 对照 ✗**：**改**切点之下**的层（S0 ✓）⇒ **∴ `sig` 变 ⇒ **不该命中** ✓
  await stroke("S0", 12);
  await bump(24);
  const control = await render({ active_layer: "S2" });

  console.log(
    `  ① 切点 S2（预热后）    ⇒ below_reused = ${first.below_reused}｜frame_reused = ${first.frame_reused}`,
  );
  console.log(
    `  ② 切点 S4（覆盖旧字段）⇒ below_reused = ${other.below_reused}｜frame_reused = ${other.frame_reused}`,
  );
  console.log(
    `  ③ 回到切点 S2          ⇒ below_reused = ${back.below_reused}｜frame_reused = ${back.frame_reused}`,
  );
  console.log(
    `  ④ 对照：改 S0（切点之下）⇒ below_reused = ${control.below_reused}｜frame_reused = ${control.frame_reused}`,
  );

  const bad = [];
  // **∴ 先要一个**有效样本** ✗**：**一个布尔字段的"假"有多种成因** ✓
  //   （**没复用／根本没渲染／没命中 ✓）⇒ **∴ 先排除"**没渲染**"** ✓
  //     ＋ **∴ 而**外层命中时渲染器**不跑**✗ ⇒ **∴ 计数不动 ⇒ `below_reused=false`** ✓
  //       ⇒ **∴ 所以**：先确认①那次**真的进了渲染**（**`frame_reused !== true` ✓）★
  if (first.frame_reused === true) {
    console.error("❌ 判不了：① 那次被**外层**整幅命中 ⇒ 渲染器没跑 ⇒ 这条判据量不到 below");
    process.exit(3);
  }
  if (back.below_reused !== true) {
    bad.push("③ 回到原切点必须复用（`by_sig` 里那一组还在）");
  }
  if (control.below_reused === true) {
    bad.push("④ 对照：改了切点之下的层之后**不该**复用");
  }
  if (bad.length) {
    console.error(`❌ ${bad.length} 项没通过：${bad.join("｜")}`);
    process.exit(1);
  }
  console.log("✓ 交替切点靠 `by_sig` 命中；改了下方内容则不复用（对照也成立）");
};

await main();
