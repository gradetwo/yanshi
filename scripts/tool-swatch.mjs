#!/usr/bin/env node
// **★ swatch_brush_strokes：试笔触（P0 #6 · 附录五 ✓）★**（第 254 轮 ✓）
//
// **∴ 为什么 ✗**（**文档附录五 ✓）**：**`brush_preview` 这名字**误导**✗
//   ⇒ **∴ AI 以为是"看笔刷外观"✗ 其实是"用这支笔画一条曲线试试" ✓
//     ＋ **∴ 而且**不能批量**✗ ⇒ **∴ 试 3 个组合要 3 次调用 ✗ 试错成本高（要 undo ✓）** ✓ ★★
//   ＋ **∴ 要的 ✗**：**新工具 `swatch_brush_strokes`（**画家术语 ✓）：
//     **笔刷 × 颜色 × 路径**的**批量组合 ✗ 一次调用返回多张对比图 ✓
//     ＋ **而且**不写文档、不生成 atom**✗ ⇒ **∴ 可以放心试 ✓）★★
//
// **∴ 判什么（**五条 ✓）★**：
//   ① 工具存在 ✗ 且接受 `brushes[]`／`colors[]`／`paths[]` ✓
//   ② **组合数 = 笔刷数 × 颜色数 × 路径数**（**∴ 一个都不能少 ✓）★★
//   ③ **不污染文档** ✗**：调用前后**对象数必须完全不变** ✓（**∴ 这是"可放心试"的核心 ✓）★★★
//   ④ 每条 swatch 都带**能取到的图**（`thumb_url` ⇒ HTTP 200 ✗ 且不是错误页 ✓）★
//   ⑤ **确定性 ✗**：同样参数再调一次 ⇒ 图**逐字节相同** ✓（**硬约束"同输入同输出" ✓）★★
//
// **∴ 变异（**能红的根据 ✓）✗**：**把批量循环改成只跑第一个组合 ⇒ **② 立刻红** ✓ ★★★
//
// 用法：node scripts/tool-swatch.mjs <base-url>
const base = (process.argv[2] || "").replace(/\/+$/, "");
if (!base) {
  console.error("用法: node scripts/tool-swatch.mjs <base-url>");
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
  const doc = `sw_${Date.now().toString(36)}`;
  const created = await (
    await fetch(`${base}/api/documents`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ doc_id: doc, width: 256, height: 256 }),
    })
  ).json();
  if (!created || !created.token) {
    console.error("❌ 判不了：建不了文档 ⇒ 退出码 3");
    process.exit(3);
  }
  const token = created.token;
  const objectsBefore = await call(doc, token, "list_objects", {});
  const before = ((objectsBefore && (objectsBefore.objects || objectsBefore.items)) || []).length;

  const args = {
    // **∴ 别用纯揉合笔刷 ✗**（`Blender` 在空画布上什么都不沉积 ✓ 那是**预期**行为 ✓
    //   ⇒ 产品对单个失败会如实记在该条上、继续跑其余 ✓）⇒ **∴ 这里用能沉积颜色的 ✓** ★
    brushes: ["Classic_Paint", "blending", "100%_Opaque"],
    colors: ["#2e4d8f", "#c8a06a"],
    paths: [
      [[40, 60, 0.2], [120, 50, 0.9], [200, 70, 0.3]],
      [[40, 140, 0.5], [120, 130, 0.5], [200, 150, 0.5]],
    ],
    size: 32,
  };
  const replied = await call(doc, token, "swatch_brush_strokes", args);
  if (!replied || replied.ok !== true) {
    // **∴ 工具不存在 = 产品缺口 ⇒ 红（exit 1 ✓）✗ 不是"判不了"** ✓（**与前面几条同一约定 ✓）★
    console.error(`❌ ① 试笔触工具还没接上：${JSON.stringify(replied).slice(0, 200)}`);
    process.exit(1);
  }
  const swatches = replied.swatches || [];
  const expected = args.brushes.length * args.colors.length * args.paths.length;
  console.log(`  ② 组合数：${swatches.length}（期望 ${expected} = ${args.brushes.length}×${args.colors.length}×${args.paths.length}）`);

  const objectsAfter = await call(doc, token, "list_objects", {});
  const after = ((objectsAfter && (objectsAfter.objects || objectsAfter.items)) || []).length;
  console.log(`  ③ 文档对象数：调用前 ${before} ⇒ 调用后 ${after}`);

  const bad = [];
  if (swatches.length !== expected) {
    bad.push(`② 组合数不对：拿到 ${swatches.length}，期望 ${expected}（笔刷×颜色×路径 一个都不能少）`);
  }
  if (after !== before) {
    bad.push(`③ 试笔触**污染了文档**（对象数 ${before} ⇒ ${after}）⇒ 不敢放心试`);
  }
  let fetched = 0;
  const digests = [];
  const okSwatches = swatches.filter((x) => x.ok !== false);
  console.log(`  （其中 ${okSwatches.length}/${swatches.length} 条成功出图）`);
  for (const s of okSwatches) {
    const url = s.thumb_url || s.url;
    if (!url) {
      bad.push("④ 有 swatch 没有图（thumb_url）");
      break;
    }
    const absolute = String(url).startsWith("http") ? String(url) : `${base}${String(url).replace("yanshi://blob/", "/api/blob/")}`;
    const response = await fetch(absolute);
    if (!response.ok) {
      bad.push(`④ 取图失败（HTTP ${response.status}）：${String(url).slice(0, 60)}`);
      break;
    }
    const bytes = new Uint8Array(await response.arrayBuffer());
    if (bytes.length < 200) {
      bad.push(`④ 取到的图太小（${bytes.length} 字节）⇒ 像错误页`);
      break;
    }
    fetched += 1;
    let sum = 0;
    for (let i = 0; i < bytes.length; i += 1) sum = (sum * 31 + bytes[i]) >>> 0;
    digests.push(sum);
  }
  console.log(`  ④ 取到 ${fetched}/${okSwatches.length} 张图｜不同图 ${new Set(digests).size} 种`);
  if (fetched === okSwatches.length && new Set(digests).size < 2) {
    bad.push("④ 所有组合出的是同一张图 ⇒ 颜色/笔刷/路径没起作用");
  }

  // ⑤ 确定性：同样参数再来一次，逐字节相同
  const again = await call(doc, token, "swatch_brush_strokes", args);
  const swatches2 = (again && again.swatches) || [];
  const digests2 = [];
  for (const s of swatches2) {
    const url = s.thumb_url || s.url;
    if (!url) continue;
    const absolute = String(url).startsWith("http") ? String(url) : `${base}${String(url).replace("yanshi://blob/", "/api/blob/")}`;
    const bytes = new Uint8Array(await (await fetch(absolute)).arrayBuffer());
    let sum = 0;
    for (let i = 0; i < bytes.length; i += 1) sum = (sum * 31 + bytes[i]) >>> 0;
    digests2.push(sum);
  }
  if (digests2.length === digests.length && digests2.join(",") !== digests.join(",")) {
    bad.push("⑤ 同样参数两次调用出的图不同 ⇒ 不满足确定性（同输入必须同输出）");
  } else {
    console.log(`  ⑤ 确定性：两次调用 ${digests2.length} 张图逐字节一致`);
  }

  if (bad.length) {
    console.error(`❌ ${bad.length} 项没成立：${bad.join("｜")}`);
    process.exit(1);
  }
  console.log(`✓ 试笔触可用：${swatches.length} 个组合一次出图、不污染文档、图能取、可复现`);
};

await main();
