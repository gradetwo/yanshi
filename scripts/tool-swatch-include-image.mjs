// **批量试笔必须能**直接给 MCP Agent 交付图片**✗**（外部复测 QY-G01 的判据 ✓）
//
// **∴ 缺陷 ✓**：**∴ `swatch_brush_strokes` 每个笔刷只给 `thumb_url` ✗**（**∴ URL ✓**）
//   ⇒ **∴ 而**原生 stdio 的 MCP Agent **看不到图**✗** ⇒ **∴ 于是**它只能**猜**✗ 哪个好 ✓
//   ⇒ **∴ 而** `brush_preview(include_image=true)` **能** ✓** ⇒ **∴ 缺口只在**批量**✗** ✓
//
// **∴ 断言（四条 ✗）**：
//   ① **∴ 缺省（不给 `include_image`）✗ ⇒ **∴ 0 个 image 块**✗**（**∴ 与今天逐字节相同 ✓**）
//   ② **∴ `include_image=true` ✗ ⇒ **∴ 每个成功的 swatch**都有 image 块**✗**
//   ③ **∴ 每张 image 的 base64 长度**≤ **64 KB**✗**（**∴ 实测 2524／3372 ✓，余量 19 倍 ✓**）
//   ④ **∴ 总 base64 长度**≤ **256 KB**✗**（**∴ 8 张上界 ✓**）
const base = process.argv[2] ?? "http://127.0.0.1:0";
const stamp = Date.now();

const post = async (path, body, doc, token) => {
  const q = doc ? `?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}` : "";
  const reply = await fetch(`${base}${path}${q}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body ?? {}),
  });
  return reply.json();
};

const doc = `g01_${stamp}`;
const created = await post("/api/documents", { doc_id: doc, width: 400, height: 400 });
if (!created?.token) { console.log("  ✗ 建文档失败（判据前提不成立）"); process.exit(3); }
const token = created.token;

let bad = 0;
const check = (ok, why) => { console.log(`  ${ok ? "✓" : "✗"} ${why}`); if (!ok) bad += 1; };
const brushes = ["pointy_ink", "calligraphy"];

const run = async (includeImage) => {
  const args = { brushes, size: 24, preview_size: "single" };
  if (includeImage !== null) args.include_image = includeImage;
  return post("/api/tools/swatch_brush_strokes", args, doc, token);
};

const plain = await run(null);
const withImage = await run(true);
const plainSwatches = plain?.swatches ?? [];
const imgSwatches = withImage?.swatches ?? [];
const okSwatches = imgSwatches.filter((x) => x?.ok === true);
const withImg = okSwatches.filter((x) => x?.image && typeof x.image.data === "string");
const sizes = withImg.map((x) => x.image.data.length);
const total = sizes.reduce((a, b) => a + b, 0);

console.log(`  include_image=false ⇒ swatches=${plainSwatches.length}｜有 image 的=${plainSwatches.filter((x) => x?.image).length}`);
console.log(`  include_image=true  ⇒ swatches=${imgSwatches.length}｜成功=${okSwatches.length}｜有 image 的=${withImg.length}｜base64 长度=${JSON.stringify(sizes)}`);

check(
  plainSwatches.length > 0 && plainSwatches.every((x) => !x?.image),
  `缺省不许带 image 块（实测 ${plainSwatches.filter((x) => x?.image).length} 个）`,
);
check(
  okSwatches.length > 0 && withImg.length === okSwatches.length,
  `include_image=true 时每个成功的 swatch 都必须有 image 块（${withImg.length}/${okSwatches.length}）`,
);
check(sizes.every((n) => n <= 64 * 1024), `每张 base64 ≤ 64 KB（最大 ${Math.max(0, ...sizes)} 字符）`);
// **∴ 阈值从 256 KB 收紧到 128 KB**✗（**∴ 第 223 轮发现 ④ 在当前构型下不可能红 ✓**）
//   ⇒ **∴ 理由**：**∴ 2 张 × 64 KB ＝ **128 KB**✗** ⇒ **∴ 那**正是**正常的上界**✗**
//   ⇒ **∴ 而** 256 KB **是** 4 张的上界**✗** ⇒ **∴ 而**判据**只调 2 张**✗** ⇒ **∴ 所以**它**永远绿**✗** ✓
//   ⇒ **∴ 收紧后**：**∴ 正常 5896 字符 ✓ ⇒ 绿 ✓**；**∴ 变异（每张 +70000）⇒ 145896 > 131072 ⇒ 红 ✓**
check(total <= 128 * 1024, `总 base64 ≤ 128 KB＝2 张 × 64 KB 的上界（实测 ${total} 字符）`);

// **∴ ⑤ 上限真的生效 ✗**（**∴ 第 174 轮补 ✓**）：**∴ 9 支笔 ⇒ **只有前 8 支**有 image 块**✗**
//   ⇒ **∴ 因为**代码是 `if … && swatches.len() < 8` ✗** ⇒ **∴ 第 9 个**应该没有 ✓
//   ⇒ **∴ 而**这条**之前没测**✗** ⇒ **∴ 所以**"8 张上限"✗ 一直只是**我说的**✗** ✓
const many = await post(
  "/api/tools/swatch_brush_strokes",
  {
    brushes: ["pointy_ink", "calligraphy", "thin_watercolor", "pointy_ink", "calligraphy", "thin_watercolor", "pointy_ink", "calligraphy", "thin_watercolor"],
    size: 24,
    preview_size: "single",
    include_image: true,
  },
  doc,
  token,
);
const manySwatches = many?.swatches ?? [];
const withImage9 = manySwatches.filter((x) => x?.image && typeof x.image.data === "string").length;
console.log(`  9 支笔 ⇒ swatches=${manySwatches.length}｜有 image 的=${withImage9}（上限 8）`);
check(
  manySwatches.length === 9 && withImage9 === 8,
  `上限必须真的生效：9 支笔里只有前 8 支带 image 块（实测 ${withImage9}/${manySwatches.length}）`,
);
// **∴ ⑥ 超限必须**说原因**✗**（**∴ spec 承诺过 `image_skipped` ✓**）
//   ⇒ **∴ 否则**调用方**只看到**"这 5 张没图"**✗** ⇒ **∴ 不知道**是真的没图还是超限**✗** ✓
const skipped9 = manySwatches.filter((x) => typeof x?.image_skipped === "string");
console.log(`  9 支笔里带 image_skipped 的=${skipped9.length}（应 = 超限的那 1 支）`);
check(
  skipped9.length === 1 && /8|上限/.test(skipped9[0].image_skipped),
  `超限的那一支必须给出 image_skipped 与原因（实测 ${skipped9.length} 支，文案 ${JSON.stringify(skipped9[0]?.image_skipped ?? null)}）`,
);

// **∴ ⑦ 顶层汇总必须能用 ✗**（**∴ §60 ✓**）：**∴ 对方写的是** `assert not resp.get("image_skipped")`**✗**
//   ⇒ **∴ 若**只把 `image_skipped` 放在每个 swatch 里 ✗ ⇒ **∴ 顶层没那个键 ✗ ⇒ **∴ 断言**恒通过**✗**
//   ⇒ **∴ 所以**顶层必须有 `image_skipped_count`（**∴ 2 支 ⇒ 0 ✓**）与 `image_skipped`（**∴ 有跳过 ⇒ 数组 ✓**）
const plainSkipped = plain?.image_skipped_count;
const manySkipped = many?.image_skipped_count;
console.log(`  顶层 image_skipped_count：2 支 ⇒ ${plainSkipped}｜9 支 ⇒ ${manySkipped}`);
check(
  plainSkipped === 0 && (plain?.image_skipped === null || plain?.image_skipped === undefined),
  `2 支笔时顶层必须说"没跳过"（count=${plainSkipped}、image_skipped=${JSON.stringify(plain?.image_skipped ?? null)}）`,
);
check(
  manySkipped === 1 && Array.isArray(many?.image_skipped),
  `9 支笔时顶层必须说"跳过 1 张"且给出原因数组（count=${manySkipped}、image_skipped=${JSON.stringify(many?.image_skipped ?? null)}）`,
);

// **∴ ⑨ 必须**分开报"成功几张"与"失败几支"**✗**（**∴ 第 10 条形态 5，外部 §71 ✓**）
//   ⇒ **∴ 只报 `count`**✗ ⇒ **∴ 7 支失败**会被**看成**"都成功"**✗** ✓
console.log(`  9 支笔 ⇒ count=${many?.count}｜ok_count=${many?.ok_count}｜fail_count=${many?.fail_count}`);
check(
  many?.ok_count + many?.fail_count === many?.count,
  `ok_count + fail_count 必须等于 count（实测 ${many?.ok_count}+${many?.fail_count} vs ${many?.count}）—— 否则有笔没被计数`,
);
check(
  many?.fail_count === many?.count - many?.ok_count,
  `fail_count 必须等于 count - ok_count（实测 ${many?.fail_count} vs ${many?.count - many?.ok_count}）`,
);
// **∴ 注意**：这里 include_image=true ⇒ 9 支全成功（fail_count=0）；同一批笔在 false 时 7 支失败，
//   所以失败与「要图片」有关。不在这里断 fail_count > 0，只断两个恒等式。
check(
  typeof many?.fail_count === "number" && typeof many?.ok_count === "number",
  `fail_count 与 ok_count 必须都是数字（实测 ${typeof many?.fail_count}/${typeof many?.ok_count}）`,
);

if (bad) { console.log(`  ✗ 批量试笔的 image 交付有 ${bad} 处不满足`); process.exit(1); }
console.log("  ✓ 批量试笔能直接交付图片（有上限且上限真的生效），缺省行为不变");
