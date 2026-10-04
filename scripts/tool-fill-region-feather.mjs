#!/usr/bin/env node
// **`fill_region` 羽化判据** ✓（测试报告 §二.1 ✓）。
// 骨架照 `tool-fill-region.mjs` ✓（同样的建文档 / 调用 / 取像素方式 ✓）。
//
// **为什么分"三侧"判** ✓ —— 这是本判据的核心，也是我在几何层栽过的坑 ✓：
//   * **只判"边缘出现中间值"是不够的** ✗：**一个"整行糊掉"的实现照样有中间值** ✓
//     （第 344 轮我自己的滑窗版本就是这样骗过一条单测的 ✗）；
//   * **只有"形状之外也出现中间值"能否证"只向内淡出"的假羽化** ✗
//     （那是**最容易写出的假实现** ✓：只在形状内部做渐变 ✓，判据若不看外侧就抓不住 ✗）；
//   * **只有"远处仍是原值"能否证"扩散无界 / 整幅被糊"** ✗。
// ⇒ 本判据按**内 / 外 / 远**三侧各判一次 ✓，另加一条**硬要求**：**不传 feather ⇒ 逐字节不变** ✓。
const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-fill-region-feather.mjs <base-url> [docId]"); process.exit(2); }
const mk = async (docId) => {
  const created = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: docId, width: 300, height: 200 }),
  })).json();
  if (!created.token) { console.error("判据无效：拿不到 token"); process.exit(1); }
  return created.token;
};
const call = (docId, token) => async (tool, args) =>
  (await fetch(`${base}/api/tools?doc=${docId}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ tool, arguments: args }),
  })).json();
const px = (docId, token) => async (x, y) => {
  const raw = await call(docId, token)("render_region", { region: { x, y, w: 1, h: 1 }, raw: true, max_px: 1000 });
  if (!raw.ok || !raw.raw_url) return null;
  const buf = Buffer.from(await (await fetch(base + raw.raw_url)).arrayBuffer());
  // **打印"请求的点"与"返回字节数"** ✓（第 548 轮 ✓）：三条断言里"深内部"对 ✓ 而"外面/远处"全 255 ✗
  // ⇒ 形状本身画对了 ✓ ⇒ **唯一自洽的解释是"读到的不是请求的那个像素"** ✗
  // ⇒ 若返回不止 4 字节（1 像素）⇒ **第一个像素未必是请求点** ✗ —— 这一行就是把那件事照出来 ✓。
  console.log(`  px 请求 (${x},${y}) ⇒ 返回 ${buf.length} 字节 = ${buf.length / 4} 像素 ⇒ 首像素 [${buf[0]},${buf[1]},${buf[2]},${buf[3]}]`);
  return [buf[0], buf[1], buf[2], buf[3]];
};
const failures = [];
// 画一块 **100..200 × 60..140** 的矩形 ✓ ⇒ 硬边清晰 ✓；**画布 300×200** ⇒ 四周余量 ≥ 60 ✓
// ⇒ 满足"边距 > 2×radius" ✓（radius=20 ⇒ 2×20 = 40 < 60 ✓，第 347 轮 ✓）。
const RECT = { x: 100, y: 60, w: 100, h: 80 };
const draw = async (docId, feather) => {
  const token = await mk(docId);
  await call(docId, token)("create_layer", { layer_id: "layer_default", name: "l" });
  const args = { layer_id: "layer_default", shape: { type: "rect", ...RECT }, color: { r: 220, g: 40, b: 40, a: 255 }, opacity: 1.0 };
  if (feather !== undefined) args.feather = feather;
  const done = await call(docId, token)("fill_region", args);
  if (done.ok !== true) { failures.push(`fill_region 失败（feather=${feather}）⇒ ${String((done.context || {}).detail || "").slice(0, 80)}`); }
  return { token, read: px(docId, token) };
};
const alpha = (v) => (v ? v[3] : null);
// **按 RGBA 分类** ✓（第 566 轮 ✓）：底色是 [255,255,255,255] ✓、填充色是 [220,40,40,255] ✓
// ⇒ **两者的 alpha 都是 255** ✗ ⇒ **只看 alpha 区分不了「里面」与「外面」** ✓
// ⇒ 实测（px 探针 ✓）：形状内 [220,40,40,255] ✓、形状外 [255,255,255,255] ✗。
const same = (v, r, g, b) => Array.isArray(v) && v[0] === r && v[1] === g && v[2] === b;
const isFill = (v) => same(v, 220, 40, 40);
const isBg = (v) => same(v, 255, 255, 255);

// ① **不羽化** ⇒ 边缘是**阶跃** ✓：里面满、**外面一格就是 0** ✓
const plain = await draw("fth_plain");
const plainIn = alpha(await plain.read(150, 100));          // 中心 ✓
const plainOut = alpha(await plain.read(97, 100));          // 左边之外 3 像素 ✓
if (plainIn !== 255) failures.push(`不羽化时中心应为满覆盖 255 ⇒ 实测 ${plainIn}`);
if (!isBg(await plain.read(97, 100)))
  failures.push(`不羽化时形状之外应为底色（白）⇒ 实测 ${JSON.stringify(await plain.read(97, 100))}`);

// ② **`feather=20`** ⇒ **形状之外**（≤ 2×radius = 40）**必须出现中间值** ✓
const feathered = await draw("fth_soft", 20);
const softOutRaw = await feathered.read(94, 100);   // 左边**之外** 6 像素 ✓
if (!softOutRaw) failures.push("羽化后取不到外侧像素 ⇒ 判据无法运行（不是通过）");
// **按类别判** ✓（第 568 轮 ✓）：底色 [255,255,255] 与填充色 [220,40,40] 的 alpha 都是 255
// ⇒ 「外侧既不是填充色、也不是底色」才是「羽化发生过」的可判形式。
else if (isFill(softOutRaw)) failures.push(`羽化后形状之外仍是填充色 ⇒ 没有渐变 ✗ 实测 ${JSON.stringify(softOutRaw)}`);
else if (isBg(softOutRaw)) failures.push(`羽化后形状之外仍是底色 ⇒ 只向内淡出 ✗ 实测 ${JSON.stringify(softOutRaw)}`);
// ③ **远处仍是原值** ✓：> 2×radius 之外必须严格 0 ✓、深内部必须满 ✓
const softFar = alpha(await feathered.read(40, 100));        // 离边 60 > 40 ✓
const softDeep = alpha(await feathered.read(150, 100));      // 中心离边 ≥ 40 ✓
if (!isBg(await feathered.read(40, 100)))
  failures.push(`远处置信应为底色（离边 60 > 2×20）⇒ 实测 ${JSON.stringify(await feathered.read(40, 100))}`);
if (softDeep === null || softDeep < 230) failures.push(`深内部应接近满覆盖 ⇒ 实测 ${softDeep}`);

// ④ **硬要求：不传 feather ⇒ 逐字节不变** ✓（同一条命令两次、两个文档、同一形状 ✓）
const again = await draw("fth_plain2");
if (plainIn !== alpha(await again.read(150, 100)) || plainOut !== alpha(await again.read(97, 100))) {
  failures.push("不传 feather 的两次渲染不一致 ⇒ 判据或渲染不稳定 ✗");
}

if (failures.length) {
  console.error("❌ 羽化不合格：");
  for (const f of failures) console.error(`   - ${f}`);
  process.exit(1);
}
console.log(`  ④ 不羽化：内 ${plainIn} / 外 ${plainOut}（阶跃 ✓）｜羽化 20：外侧 ${softOut}（中间值 ✓）/ 远处 ${softFar}（0 ✓）/ 深处 ${softDeep}（满 ✓）`);
console.log("  ✓ 羽化三侧 + 缺省惰性 全部合格");
