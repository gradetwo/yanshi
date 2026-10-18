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
check(total <= 256 * 1024, `总 base64 ≤ 256 KB（实测 ${total} 字符）`);

if (bad) { console.log(`  ✗ 批量试笔的 image 交付有 ${bad} 处不满足`); process.exit(1); }
console.log("  ✓ 批量试笔能直接交付图片（有上限），缺省行为不变");
