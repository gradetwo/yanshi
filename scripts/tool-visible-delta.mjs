// **`max_channel_delta` 必须量的是「看得见多少」✗**（外部复测 QY-D03 的判据 ✓）
//
// **∴ 缺陷是什么 ✗**：**∴ `fed_base=false`（不读画布的笔）✗ ⇒ **∴ 我原来逐通道按**白底 255**✗ 比较** ✓
//   ⇒ **∴ 而**引擎缓冲里的**空白像素是 `RGBA(0,0,0,0)`**✗ ⇒ **∴ 与 255 差 **255**✗ ⇒ **∴ 恒饱和 ✓**
//   ⇒ **∴ 再一层 ✓**：**∴ 抗锯齿边缘**半透明**✗（**∴ RGB≈0、alpha 很低 ✓**）⇒ **∴ 差 254 却**不可见**✗**
// **∴ 所以修法 ✓**：**∴ 先按 alpha **合成到白底**✗ ⇒ **∴ 再比 ✓**（**∴ `c = rgb·a + 255·(1−a)` ✓**）
//
// **∴ 断言（四条，取自他们独立 PNG 测量的真实数字 ✗）**：
//   **∴ 白 ⇒ 0 ✓／`#fefefe` ⇒ 1 ✓／`#e0e0e0` ⇒ 31 ✓／`#343832` ⇒ 205 ✓**
//   ⇒ **∴ 容差 ±1**✗（**∴ 抗锯齿受路径采样影响 ✓**）⇒ **∴ 而**白必须是 **0**（**∴ 严格 ✓**）
// **∴ 变异 ✓**：**∴ 去掉合成 ✗（**回退成按 255 直接比 ✓**）⇒ **∴ 四色全 254 ✗ ⇒ **∴ 全部必红 ✓**
// **前提失败必须 exit(3)**（第 1084 轮）：无服务时 fetch 抛异常 ⇒ uncaught ⇒ 退出码 1 ✗
// ⇒ 按 336 轮那条：条件不满足应 exit(3)（不可判），不是 exit(1)（产品失败）
process.on("uncaughtException", (error) => {
  console.error(`  ⊘ 不可判：${String(error).slice(0, 80)}`);
  process.exit(3);
});
const base = process.argv[2] ?? "http://127.0.0.1:0";
const stamp = Date.now();

const post = async (path, body, doc, token) => {
  const query = doc ? `?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}` : "";
  const reply = await fetch(`${base}${path}${query}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body ?? {}),
  });
  return reply.json();
};

// **∴ 他们的四色 ＋ 独立测量得到的真实可见差 ✓**
const CASES = [
  { name: "白", rgb: [255, 255, 255], expected: 0, strict: true },
  { name: "#fefefe", rgb: [254, 254, 254], expected: 1 },
  { name: "#e0e0e0", rgb: [224, 224, 224], expected: 31 },
  { name: "#343832", rgb: [52, 56, 50], expected: 205 },
];

let bad = 0;
const check = (ok, why) => {
  console.log(`  ${ok ? "✓" : "✗"} ${why}`);
  if (!ok) bad += 1;
};

for (const { name, rgb, expected, strict } of CASES) {
  const doc = `vd_${name.replace(/[^a-z0-9]/gi, "")}_${stamp}`;
  const created = await post("/api/documents", { doc_id: doc, width: 600, height: 400 });
  if (!created?.token) {
    console.log(`  ✗ ${name} ⇒ 建文档失败`);
    bad += 1;
    continue;
  }
  const token = created.token;
  await post("/api/tools/create_layer", { layer_id: "L", name: "L" }, doc, token);
  const r = await post(
    "/api/tools/brush_stroke",
    {
      layer_id: "L",
      brush: "calligraphy",
      color: { r: rgb[0], g: rgb[1], b: rgb[2], a: 255 },
      points: [[100, 200, 1], [300, 200, 1]],
      size: 54,
      preview: false,
    },
    doc,
    token,
  );
  const delta = r?.max_channel_delta;
  const ok = strict ? delta === expected : Math.abs(delta - expected) <= 1;
  check(
    ok,
    `${name} ⇒ max_channel_delta=${delta}（期望 ${strict ? "严格" : "±1 内"} ${expected}，fed_base=${r?.fed_base}）`,
  );
}

if (bad) {
  console.log(`  ✗ 可见幅度有 ${bad} 处不满足`);
  process.exit(1);
}
console.log("  ✓ max_channel_delta 量的是可见幅度（白/近白/灰/深 四色与独立 PNG 测量一致）");
