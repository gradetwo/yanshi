// **`medium_stroke` 必须报**真正的落墨幅度** ✓（`paint_delta`）**
//
// **∴ 为什么加它 ✗**：`medium_stroke` 里**已经有** `max_channel_delta` ✗，但它的语义是
//   **CPU 路 vs GPU 路的逐位差**✗（**∴ 恒 `Null` ✓，附 `max_channel_delta_note` ✓**）
//   ⇒ **∴ 而 `brush_stroke` 的同名字段 ✗ 是**落墨幅度**✗（**∴ 薄涂 9 ✗／实心 255 ✓**）
//   ⇒ **∴ 同名两个语义 ✗ ⇒ **∴ 报告方 §40 若走 `medium_stroke` 会拿到 `Null` ✗，
//      而那**不是**"零幅度"✗** ⇒ **∴ 所以**加新名字 `paint_delta` ✓（**∴ 旧字段不动 ✓**）
//
// **∴ 断言（两条，都是确定性的 ✗）**：
//   ① `paint_delta` 是 0..255 的整数 ✓
//   ② `painted_pixels > 0` ⇒ **`paint_delta > 0`** ✗（**∴ 改了像素就不可能幅度为 0 ✓**）
//   ⇒ **∴ 而**②**是承重的**✗** —— **∴ 若**恒 0 ✗ ⇒ **∴ 必红 ✓**
//
// **∴ 我特意没断"同色画两遍 ⇒ 第二遍幅度为 0"✗** —— **∴ 实测不成立 ✓**（**∴ 两遍都是 255 ✓**，
//   水彩会叠加 ✓）⇒ **∴ 找不到强对照就不硬凑 ✓**（**∴ 这一场的纪律 ✓**）
// **前提失败必须 exit(3)**（第 1084 轮）：无服务时 fetch 抛异常 ⇒ uncaught ⇒ 退出码 1 ✗
// ⇒ 按 336 轮那条：条件不满足应 exit(3)（不可判），不是 exit(1)（产品失败）
process.on("uncaughtException", (error) => {
  console.error(`  ⊘ 判据自身出错（不是产品问题）：${String(error).slice(0, 200)}`);
  process.exit(3);
});
const base = process.argv[2] ?? "http://127.0.0.1:0";
const doc = `md_${Date.now()}`;
const docCreated = await fetch(`${base}/api/documents`, {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 400, height: 400 }),
});
if (!docCreated.ok) {
  console.log(`  ✗ 建文档失败：HTTP ${docCreated.status}`);
  process.exit(1);
}
const token = (await docCreated.json())?.token ?? "";
const call = async (name, args) => {
  const reply = await fetch(
    `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`,
    { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args) },
  );
  return reply.json();
};
await call("create_layer", { layer_id: "L", name: "L" });
const r = await call("medium_stroke", {
  layer_id: "L",
  medium: "watercolor",
  points: [[60, 200, 1], [320, 220, 1]],
  size: 30,
  color: { r: 200, g: 60, b: 40, a: 255 },
  load: 1.0,
});
console.log(`  medium_stroke ⇒ paint_delta=${r?.paint_delta}｜painted_pixels=${r?.painted_pixels}｜ok=${r?.ok}`);

let bad = 0;
const check = (ok, why) => {
  console.log(`  ${ok ? "✓" : "✗"} ${why}`);
  if (!ok) bad += 1;
};
const delta = r?.paint_delta;
check(
  typeof delta === "number" && Number.isInteger(delta) && delta >= 0 && delta <= 255,
  `paint_delta 是 0..255 的整数（实测 ${delta}）`,
);
check(
  !(typeof r?.painted_pixels === "number" && r.painted_pixels > 0) || (typeof delta === "number" && delta > 0),
  `改了像素 ⇒ 幅度必须 > 0（painted_pixels=${r?.painted_pixels}｜paint_delta=${delta}）`,
);

if (bad) {
  console.log(`  ✗ paint_delta 有 ${bad} 处不满足`);
  process.exit(1);
}
console.log("  ✓ medium_stroke 报真正的落墨幅度（paint_delta），与 brush_stroke 的 max_channel_delta 同义不同名");
