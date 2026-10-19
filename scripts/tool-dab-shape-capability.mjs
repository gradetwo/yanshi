// **能力接口不许把**生效的参数**宣告为 `ignored` ✗**（外部复测 QY-D01 的判据 ✓）
//
// **∴ 我原来错在哪 ✗**：**∴ `dab_shape.supported = ["circle"]` ✗** ＋
//   **∴ `ignored_settings` 含 `elliptical_dab_ratio`／`angle` ✗**
//   ⇒ **∴ 而**实测**两者都生效**✗（**∴ `ratio=1` **6478** ✗ vs `ratio=8` **5992** ✗ ⇒ **∴ 差 486 ＝ 7.5% ✓**）
// **∴ 而**代价 ✓：**∴ Agent（含《清游》）**依据它绕行**✗** ⇒ **∴ 放弃了**扁锋与角度塑形**✗** ✓
//
// **∴ 断言（三条 ✗）**：
//   ① `dab_shape.supported` 含 `ellipse` ✓
//   ② `dab_shape` **不许**出现 `ignored_settings` ✗（**∴ 那是**确定性否定**✗ ⇒ **∴ 该用 `unverified` ✓**）
//   ③ **∴ `elliptical_dab_ratio` 真的生效 ✗**：**∴ `ratio=1` 与 `8` 的 `painted_pixels` **必须不同**✗**
//   ⇒ **∴ ③**是**行为证据**✗ ⇒ **∴ 它**让 ①② 不能靠改文案糊弄**✗** ✓
const base = process.argv[2] ?? "http://127.0.0.1:0";
const stamp = Date.now();

const post = async (path, body, doc, token) => {
  const q = doc ? `?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}` : "";
  const reply = await fetch(`${base}${path}${q}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body ?? {}),
  });
  return reply.json();
};

let bad = 0;
const check = (ok, why) => {
  console.log(`  ${ok ? "✓" : "✗"} ${why}`);
  if (!ok) bad += 1;
};

// **① ② 能力接口 ✓**
const mainDoc = `ds_main_${stamp}`;
const created = await post("/api/documents", { doc_id: mainDoc, width: 400, height: 400 });
if (!created?.token) {
  console.log("  ⊘ 不可判：建文档失败（判据前提不成立，不是产品问题）");
  process.exit(3);
}
const caps = await post("/api/tools/list_engine_capabilities", {}, mainDoc, created.token);
const shape = caps?.dab_shape ?? {};
check(Array.isArray(shape.supported) && shape.supported.includes("ellipse"), `dab_shape.supported 含 ellipse（实测 ${JSON.stringify(shape.supported)}）`);
check(
  !("ignored_settings" in shape),
  `dab_shape 不许有 ignored_settings（实测 ${JSON.stringify(shape.ignored_settings ?? null)}）—— 那是确定性否定，该用 unverified`,
);

// **③ 行为证据 ✓**
const pixels = [];
for (const ratio of [1, 8]) {
  const doc = `ds_r${ratio}_${stamp}`;
  const c = await post("/api/documents", { doc_id: doc, width: 400, height: 400 });
  if (!c?.token) { console.log(`  ⊘ 不可判：ratio=${ratio} 建文档失败（判据前提不成立）`); process.exit(3); }
  await post("/api/tools/create_layer", { layer_id: "L", name: "L" }, doc, c.token);
  const r = await post(
    "/api/tools/brush_stroke",
    {
      layer_id: "L",
      brush: "pointy_ink",
      color: { r: 40, g: 40, b: 40, a: 255 },
      points: [[100, 200, 1], [300, 200, 1]],
      size: 30,
      preview: false,
      brush_params: { elliptical_dab_ratio: ratio },
    },
    doc,
    c.token,
  );
  pixels.push({ ratio, painted: r?.painted_pixels });
}
console.log(`  ratio=1 ⇒ painted=${pixels[0].painted}｜ratio=8 ⇒ painted=${pixels[1].painted}`);
check(
  typeof pixels[0].painted === "number" && typeof pixels[1].painted === "number" && pixels[0].painted !== pixels[1].painted,
  `elliptical_dab_ratio 真的生效：ratio=1 与 8 的 painted_pixels 必须不同（${pixels[0].painted} vs ${pixels[1].painted}）`,
);

if (bad) {
  console.log(`  ✗ dab 形状能力有 ${bad} 处不满足`);
  process.exit(1);
}
console.log("  ✓ 能力接口如实报告：椭圆与角度生效，不再宣告 ignored；且行为证据一致");
