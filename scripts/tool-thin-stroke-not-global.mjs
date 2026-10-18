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

const draw = async (size) => {
  const doc = `thin_${String(size).replace(".", "_")}_${stamp}`;
  const c = await post("/api/documents", { doc_id: doc, width: 600, height: 400 });
  if (!c?.token) return { setupFailed: true };
  await post("/api/tools/create_layer", { layer_id: "L", name: "L" }, doc, c.token);
  const r = await post(
    "/api/tools/brush_stroke",
    { layer_id: "L", brush: "calligraphy", color: { r: 30, g: 40, b: 90, a: 255 }, points, size, preview: false },
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
const control = await draw(16);
if (thin.setupFailed || control.setupFailed) {
  console.log("  ✗ 建文档失败（判据前提不成立）");
  process.exit(3);
}
console.log(`  size=3.456 ⇒ ok=${thin.ok}｜painted=${thin.painted}`);
console.log(`  size=16    ⇒ ok=${control.ok}｜painted=${control.painted}`);

check(
  !/画不出来[，,]?\s*\**换一支/.test(thin.text),
  `细笔消息不许出现"画不出来，换一支"这种唯一出路（出现了即误判整支不可用）`,
);
check(
  /不代表整支|不是整支|同一个?笔刷?.*(加大|更大)|加大\s*size|加长路径/.test(thin.text),
  `细笔消息必须给出"这不是整支笔刷不可用"的纠正或等价出路（加大 size／加长路径）`,
);
check(
  /size\D{0,12}3\.456/.test(thin.text) && /33\s*(个)?点/.test(thin.text) && /路径总长/.test(thin.text),
  `细笔消息必须报出本笔参数：size、点数、路径长（实测文本长度 ${thin.text.length}）`,
);
check(
  control.ok === true && typeof control.painted === "number" && control.painted > 0,
  `同刷 size=16 必须成功落墨（ok=${control.ok}｜painted=${control.painted}）—— 这证明整支笔刷可用`,
);

if (bad) {
  console.log(`  ✗ 细笔提示有 ${bad} 处不满足`);
  process.exit(1);
}
console.log("  ✓ 细笔零落墨只描述本次事实，不断言整支笔刷不可用，且同刷更大 size 能画");
