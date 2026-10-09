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

  // **① 改中间层 ＋ 传 `active_layer` ⇒ 应自增 ✓**
  await stroke("S2", 9);
  const b1 = await reuse();
  await render({ active_layer: "S2" });
  const a1 = await reuse();

  // **② 改中间层 ＋ **不传** ⇒ 不应自增 ✗（**对照 ✓**）**
  await stroke("S2", 18);
  const b2 = await reuse();
  await render();
  const a2 = await reuse();

  console.log(`  ① 改中间层 ＋ 传 active_layer=S2 ⇒ below_reuse ${b1} ⇒ ${a1}`);
  console.log(`  ② 改中间层 ＋ 不传（对照）      ⇒ below_reuse ${b2} ⇒ ${a2}`);
  const bad = [];
  if (!(a1 > b1)) bad.push(`① 传 active_layer 时应自增 ✗（实测 ${b1} ⇒ ${a1}）`);
  if (a2 > b2) bad.push(`② 不传时**不应**自增 ✗（实测 ${b2} ⇒ ${a2} ⇒ 切点没退回"最上层以外" ✗）`);
  if (bad.length) {
    console.error("❌ " + bad.join("｜") +
      "（变异：服务端忽略 active_layer ⇒ 断言 ① 必红 ✓）");
    process.exit(1);
  }
  console.log("  ✓ 切点确实落在当前层：传参 ⇒ 复用下方；不传 ⇒ 退回旧行为（对照成立 ✓）");
};
main().catch((e) => { console.error("❌ 运行失败：" + String(e)); process.exit(2); });
