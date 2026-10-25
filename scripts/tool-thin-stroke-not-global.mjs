// **细笔零落墨时，不许断言「整支笔刷不可用」✗**（外部复测 QY-D02 的判据 ✓）
//
// **∴ 缺陷 ✓**：**∴ 同一支 `calligraphy` ✗ ⇒ **∴ `size=3.456` ＋ 33 点 ⇒ **3/3 无像素**✗
//   ⇒ **∴ 而**消息说「`dabs_per_basic_radius=0` ⇒ **它在当前引擎下画不出来，换一支**」✗
//   ⇒ **∴ 而** `size=8／16` **都成功**✗** ⇒ **∴ 所以**"整支不可用"✗ **是错的**✗ ✓
// **∴ 后果 ✓**：**∴ 创作者**换笔**✗** ⇒ **∴ 《清游》那种细笔触**就画不出来**✗**
//
// **∴ 断言（四条 ✗）**：
//   ① **∴ 细笔（`size=3.456`、33 点）✗ ⇒ **∴ 不许出现**"画不出来，**换一支**"✗ 这种**唯一出路**✗**
//   ② **∴ 必须**明说「**不代表整支笔刷不可用**」✗**（**∴ 或等价说法 ✓**）
//   ③ **∴ 必须**报出**本笔参数**✗：`size` ✗ ＋ **点数**✗ ＋ **路径长**✗
//   ④ **∴ 同刷 `size=16` 必须**成功落墨**✗**（**∴ 证明"整支可用"✗**）
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

// **∴ 他们的构型 ✓**：33 个点、有起伏压力 ✓
const points = Array.from({ length: 33 }, (_, i) => [40 + 8 * i, 200 + 18 * ((i % 3) - 1), Number((0.4 + 0.02 * (i % 7)).toFixed(3))]);

const draw = async (size, pts = points) => {
  const doc = `thin_${String(size).replace(".", "_")}_${stamp}`;
  const c = await post("/api/documents", { doc_id: doc, width: 600, height: 400 });
  if (!c?.token) return { setupFailed: true };
  await post("/api/tools/create_layer", { layer_id: "L", name: "L" }, doc, c.token);
  const r = await post(
    "/api/tools/brush_stroke",
    { layer_id: "L", brush: "calligraphy", color: { r: 30, g: 40, b: 90, a: 255 }, points: pts, size, preview: false },
    doc,
    c.token,
  );
  const text = JSON.stringify(r?.context ?? r ?? {});
  return { ok: r?.ok, painted: r?.painted_pixels, text };
};

let bad = 0;
const check = (ok, why) => {
  console.log(`  ${ok ? "✓" : "✗"} ${why}`);
  if (!ok) bad += 1;
};

const thin = await draw(3.456);
// **★ P2-13（画画线期 2）之后，size=3.456 已经能落墨** ✓：
//   旧判据的前提（细笔 3/3 零像素）**被产品修掉了** ⇒ 再拿它当"零墨样本"会**假红** ✗
//   ⇒ 现在：① 细笔能落墨本身就是回归断言 ✓；② 零墨提示改用**真零墨样本**
//      （size=3.456、2 点、路径 0.4px —— 路径撑不满 dab 间距 ✓）⇒ 消息仍必须
//      "不代表整支" + 报 size/点数/路径长 ✓。
const zeroPoints = [[300, 200], [300.4, 200]];
const zero = await draw(3.456, zeroPoints);
const control = await draw(16);
if (thin.setupFailed || zero.setupFailed || control.setupFailed) {
  console.log("  ✗ 建文档失败（判据前提不成立）");
  process.exit(3);
}
console.log(`  size=3.456（33 点长路径）⇒ ok=${thin.ok}｜painted=${thin.painted}`);
console.log(`  size=3.456（2 点 0.4px） ⇒ ok=${zero.ok}｜painted=${zero.painted}`);
console.log(`  size=16    ⇒ ok=${control.ok}｜painted=${control.painted}`);

check(
  typeof thin.painted === "number" && thin.painted > 0,
  `size=3.456 细笔必须能落墨（P2-13 已修；旧缺陷是 3/3 零像素）—— 实测 painted=${thin.painted}`,
);
check(
  !/画不出来[，,]?\s*\**换一支/.test(zero.text),
  `零墨消息不许出现"画不出来，换一支"这种唯一出路（出现了即误判整支不可用）`,
);
check(
  // **∴ 必须是**明确的否定**✗**（**∴ 变异教训 ✓**：原来还接受"加大 size／加长路径"✗
  //   ⇒ **∴ 而**旧文案也有这两句出路 ✗ ⇒ **∴ 于是**这一条**被满足了**✗ ⇒ **∴ 没有分辨力**✗）
  /不代表整支|不是整支|并非整支|整支笔刷不(是|代表)/.test(zero.text),
  `零墨消息必须**明确否定**"整支不可用"（"不代表整支／不是整支"），只有出路不算`,
);
check(
  /size\D{0,12}3\.456/.test(zero.text) && /2\s*(个)?点/.test(zero.text) && /路径总长/.test(zero.text),
  `零墨消息必须报出本笔参数：size、点数、路径长（实测文本长度 ${zero.text.length}）`,
);
check(
  control.ok === true && typeof control.painted === "number" && control.painted > 0,
  `同刷 size=16 必须成功落墨（ok=${control.ok}｜painted=${control.painted}）—— 这证明整支笔刷可用`,
);

if (bad) {
  console.log(`  ✗ 细笔提示有 ${bad} 处不满足`);
  process.exit(1);
}
console.log("  ✓ 细笔已能落墨；真零墨只描述本次事实，不断言整支笔刷不可用，且同刷更大 size 能画");
