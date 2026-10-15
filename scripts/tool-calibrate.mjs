#!/usr/bin/env node
// **★ `calibrate()`：坐标校准（Muse 建议 2，P1）★**
//
// **为什么需要它**（用户的定位 ✓，与我最初的理解不同 ✗）：**不是为了找 MCP 的坐标 bug** ✓ ——
// 那是**已经实测排除了**的 ✓（Railway 上四角+中心 5 点全对 ✓）。它要解决的是**排查时只能靠猜** ✗：
// 一旦画面错位，AI 无法自证"**是工具坐标系错了**"还是"**是我估错了**" ✓。
// ⇒ `calibrate` 把"画 → 渲染 → 在像素里量回来"这条**闭环**跑一遍 ✓：
//   **偏差为 0（或小于容差）意味着"工具这条链没有引入偏移"** ✓ ⇒ AI 才能把注意力放回自己的估算 ✓。
//
// **它测的是整条链** ✓：`draw_shape` 写原子 → 折叠 → 栅格化 → `render_region{raw}` 取像素 → 在像素里定位 ✓。
// 所以**任何一个环节**引入平移/缩放/翻转，都会被它抓到 ✓（这正是"MCP 有没有坐标 bug"的可复现答案 ✓）。
//
// **判据要证的四件事**：
//   ① 工具存在且**回报每个标记的预期 vs 实测**坐标（`dx/dy` + 像素计数 ✓）；
//   ② **五点的偏差都在容差内** ✓（-0 像素级 ✓）—— 这是"MCP 坐标系无 bug"的**判据化** ✓；
//   ③ **它自己会清理**（`cleanup` 缺省 true ✓）⇒ 跑完之后**画布上的对象数回到跑之前** ✓
//      （不然校准会污染正要画的画 ✗）；
//   ④ **可复现**：跑两次，两点结果**逐字节相同** ✓（确定性硬约束 ✓）。
//
// 用法：node scripts/tool-calibrate.mjs <base-url>
const base = (process.argv[2] || "").replace(/\/+$/, "");
if (!base) {
  console.error("用法: node scripts/tool-calibrate.mjs <base-url>");
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

const newDoc = async (tag) => {
  const doc = `cal_${tag}_${Date.now().toString(36)}`;
  const created = await (
    await fetch(`${base}/api/documents`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ doc_id: doc, width: 640, height: 480 }),
    })
  ).json();
  if (!created || !created.token) {
    console.error("❌ 判不了：建不出文档 ⇒ 退出码 3");
    process.exit(3);
  }
  return [doc, created.token];
};

const listObjects = async (doc, token) => {
  const r = await call(doc, token, "list_objects", {});
  return (r.objects || []).length;
};

const main = async () => {
  const bad = [];
  const [doc, token] = await newDoc("a");

  const before = await listObjects(doc, token);
  const first = await call(doc, token, "calibrate", {});
  if (!first || first.ok !== true) {
    console.error(
      `❌ ① 工具不存在或失败：${JSON.stringify(first).slice(0, 200)}\n` +
        "   ⇒ 这是产品缺口（Muse 建议 2 尚未实现），退出码 1",
    );
    process.exit(1);
  }

  // ① 回报结构
  const marks = first.marks || [];
  console.log(`  ① 标记数=${marks.length}｜容差=${first.tolerance}｜最大偏差=${first.max_deviation}`);
  for (const m of marks) {
    console.log(
      `     (${m.expected.x},${m.expected.y}) ⇒ 实测 ` +
        `${m.actual ? `(${m.actual.x},${m.actual.y})` : "未找到"}` +
        `｜dx=${m.dx === undefined ? "?" : m.dx} dy=${m.dy === undefined ? "?" : m.dy}` +
        `｜像素=${m.pixels === undefined ? "?" : m.pixels}`,
    );
  }
  if (marks.length < 5) bad.push(`① 至少要有 4 角 + 中心共 5 个标记，实际 ${marks.length}`);
  for (const m of marks) {
    if (!m.actual) bad.push(`① (${m.expected.x},${m.expected.y}) 的标记在像素里没找到 ✗`);
  }

  // ② 偏差在容差内（"MCP 坐标系无 bug"的判据化）
  const tolerance = typeof first.tolerance === "number" ? first.tolerance : 1.0;
  const worst = marks.reduce(
    (acc, m) => Math.max(acc, Math.abs(m.dx || 0), Math.abs(m.dy || 0)),
    0,
  );
  console.log(`  ② 最大绝对偏差=${worst}px｜容差=${tolerance}px`);
  if (!(worst <= tolerance)) {
    bad.push(
      `② 有标记的偏差超过容差（最大 ${worst}px > ${tolerance}px）` +
        " ⇒ **这条链引入了坐标偏移** ✗（这正是要抓的情形 ✓）",
    );
  }
  if (first.passed !== true) bad.push("② `passed` 应当为 true（偏差都在容差内 ✓）");

  // ③ 自清理：对象数回到跑之前
  const after = await listObjects(doc, token);
  console.log(`  ③ 对象数：跑之前 ${before} ⇒ 跑之后 ${after}`);
  if (after !== before) {
    bad.push(`③ 校准**污染了画布** ✗（对象数 ${before} ⇒ ${after}）⇒ ` + "cleanup 缺省应当清干净 ✓");
  }

  // ④ 确定性：跑两次结果一致
  const second = await call(doc, token, "calibrate", {});
  const key = (r) =>
    JSON.stringify((r.marks || []).map((m) => [m.expected.x, m.expected.y, m.actual && m.actual.x, m.actual && m.actual.y]));
  console.log(`  ④ 两次结果一致=${key(first) === key(second)}`);
  if (key(first) !== key(second)) bad.push("④ 两次 `calibrate` 结果不同 ⇒ 违反确定性 ✗");

  const afterSecond = await listObjects(doc, token);
  if (afterSecond !== before) bad.push(`④ 第二次也没清干净（${afterSecond} ≠ ${before}）✗`);

  if (bad.length) {
    console.error(`❌ ${bad.length} 项没成立：${bad.join("｜")}`);
    process.exit(1);
  }
  console.log(
    "✓ 坐标校准成立：五点偏差都在容差内（工具链无偏移 ✓）、自清理 ✓、可复现 ✓",
  );
  console.log("  另：`calibrate` 的定位是**排除工具**（不是找 bug）⇒ 它证明无误时，错位就该回到 AI 自己的估算上 ✓");
};

await main();
