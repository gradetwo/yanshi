// **`max_channel_delta` 必须能区分「画了」与「看得见」** ✓
//
// **∴ 为什么要有它 ✗**：`painted_pixels` 只数"字节变了"✗ ⇒ **∴ 差 1 LSB 也算 ✓**
//   ⇒ **∴ 用户实测 ✓**：`thin_watercolor` 在 size=20 下 `painted_pixels=146` ✗ 而**肉眼全白**✗
//      ⇒ **∴ 那是**静默失败**✗** ✓
// **∴ 而真正的信号 ✓**：**∴ 落墨幅度 ✓** —— 水彩最大只差 **8~9/255** ✗、实心笔 **255/255** ✓
//
// **∴ 断言（两个都是**数量级**断言 ✗，不是精确值 ✓）**：
//   ① 薄涂笔：`painted_pixels > 0` ✗ 但 `max_channel_delta <= 16` ✗（**∴ 画了、看不见 ✓**）
//   ② 实心笔：`max_channel_delta >= 200` ✓（**∴ 画了、也看得见 ✓**）
//   ⇒ **∴ 两条一起才说明这个字段**有区分力**✗**；**∴ 只断一条 ⇒ **∴ 恒 0 也能过 ✓**
const base = process.argv[2] ?? "http://127.0.0.1:0";
// **∴ 自己建文档 ✗**：**∴ 这样运行器只传 base ✓**，不依赖外部令牌 ✓
//   ⇒ **∴ 因为**这个判据要**换笔刷画两笔**✗，用别人的文档会污染它 ✓
const doc = `mcd_${Date.now()}`;
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
if (!token) {
  console.log("  ✗ 建文档没拿到 token");
  process.exit(1);
}

const call = async (name, args) => {
  const reply = await fetch(
    `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`,
    { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args) },
  );
  return { ok: reply.ok, body: await reply.json() };
};

const created = await call("create_layer", { layer_id: "mcd_L", name: "mcd_L" });
if (created.body?.ok !== true) {
  console.log(`  ✗ 建层失败：${JSON.stringify(created.body).slice(0, 160)}`);
  process.exit(1);
}

let bad = 0;
const check = (ok, why) => {
  console.log(`  ${ok ? "✓" : "✗"} ${why}`);
  if (!ok) bad += 1;
};

const stroke = async (brush) => {
  const points = [];
  for (let i = 0; i < 10; i += 1) points.push([20 + (100 * i) / 9, 20 + (100 * i) / 9, 1]);
  const r = await call("brush_stroke", {
    layer_id: "mcd_L",
    brush,
    color: { r: 30, g: 60, b: 120, a: 255 },
    points,
    size: 20,
    opacity: 0.5,
    preview: false,
    wait_for_render: true,
  });
  return r.body ?? {};
};

const thin = await stroke("thin_watercolor");
const ink = await stroke("pointy_ink");

console.log(
  `  薄涂 thin_watercolor ⇒ painted=${thin.painted_pixels}｜max_channel_delta=${thin.max_channel_delta}`,
);
console.log(
  `  实心 pointy_ink      ⇒ painted=${ink.painted_pixels}｜max_channel_delta=${ink.max_channel_delta}`,
);

check(
  typeof thin.max_channel_delta === "number" && thin.painted_pixels > 0 && thin.max_channel_delta <= 16,
  `薄涂笔：画了（painted>0）但幅度很小（max_channel_delta ≤ 16，实测 ${thin.max_channel_delta}）`,
);
check(
  typeof ink.max_channel_delta === "number" && ink.max_channel_delta >= 200,
  `实心笔：幅度很大（max_channel_delta ≥ 200，实测 ${ink.max_channel_delta}）`,
);
check(
  typeof thin.max_channel_delta === "number" &&
    typeof ink.max_channel_delta === "number" &&
    ink.max_channel_delta > thin.max_channel_delta,
  `两者有区分力（${ink.max_channel_delta} > ${thin.max_channel_delta}）`,
);

if (bad) {
  console.log(`  ✗ max_channel_delta 有 ${bad} 处不满足`);
  process.exit(1);
}
console.log("  ✓ max_channel_delta 能区分「画了」与「看得见」");
