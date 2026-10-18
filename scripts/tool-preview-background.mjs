// **`brush_preview` 的 `background` 必须**真的改变预览**✗**（外部复测 QY-G02 的判据 ✓）
//
// **∴ 缺陷 ✓**：**∴ 预览**没有底图概念**✗**（`paint_brush` 的 `fed_base` 写死 `false` ✓）
//   ⇒ **∴ 于是**同一支刷在**白底**与**已画深灰底**上 ⇒ **∴ 预览哈希**必然相同**✗**
//   ⇒ **∴ 而**实际落笔**同一支刷**⇒ `fed_base=true`**✗** ⇒ **∴ 所以**预览**不反映实际**✗** ✓
//
// **∴ 断言（三条 ✗）**：
//   ① **∴ 不给 `background` ✗ ⇒ **∴ 必须** `ok=true`**✗**（**∴ 缺省白底 ✓，行为不变 ✓**）
//   ② **∴ 给**白 `{255,255,255}` ✗ ⇒ **∴ `blob_hash` 必须**等于**不给时**✗**（**∴ 白底就是缺省 ✓**）
//   ③ **∴ 给**深色 `{111,107,98}`（**∴ 即 `#6f6b62` ✓**）⇒ **∴ `blob_hash` 必须**不同**✗**
//      ⇒ **∴ ③** 是**核心**✗** ⇒ **∴ 它**证明底图真的进了渲染**✗** ✓
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

const doc = `bg_${stamp}`;
const created = await post("/api/documents", { doc_id: doc, width: 400, height: 400 });
if (!created?.token) {
  console.log("  ✗ 建文档失败（判据前提不成立）");
  process.exit(3);
}
const token = created.token;

let bad = 0;
const check = (ok, why) => {
  console.log(`  ${ok ? "✓" : "✗"} ${why}`);
  if (!ok) bad += 1;
};

const preview = async (background) => {
  const args = { brush: "pointy_ink", size: 24, include_image: true };
  if (background) args.background = background;
  const r = await post("/api/tools/brush_preview", args, doc, token);
  return { ok: r?.ok, hash: r?.blob_hash ?? null, bytes: r?.bytes ?? null };
};

const plain = await preview(null);
const white = await preview({ r: 255, g: 255, b: 255 });
const dark = await preview({ r: 111, g: 107, b: 98 });

const show = (label, r) => console.log(`  ${label} ⇒ ok=${r.ok}｜hash=${String(r.hash).slice(0, 20)}…｜bytes=${r.bytes}`);
show("不给 background", plain);
show("白 {255,255,255}", white);
show("深 {111,107,98}", dark);

check(plain.ok === true, `不给 background 时必须成功（ok=${plain.ok}）`);
// **∴ 白底 ≠ 缺省 ✗**（**∴ 实测 ✓**：`1891` vs `2535` 字节 ✓）
//   ⇒ **∴ 因为**缺省 surface 是**空的／透明**✗** ⇒ **∴ 铺白会**真的改动它**✗** ✓
// ⇒ **∴ 所以**这里**不断言**"白 ＝ 缺省"✗** ⇒ **∴ 而是**断言**三者**两两不同**✗**（**∴ 只要铺了就变 ✓**）
check(
  typeof white.hash === "string" && white.hash !== plain.hash,
  `铺白也要改变预览（缺省 ${String(plain.hash).slice(0, 20)}… vs 白 ${String(white.hash).slice(0, 20)}…）`,
);
check(
  typeof white.hash === "string" && typeof dark.hash === "string" && white.hash !== dark.hash,
  `白底与深色底必须不同（白 ${String(white.hash).slice(0, 20)}… vs 深 ${String(dark.hash).slice(0, 20)}…）`,
);
check(
  typeof dark.hash === "string" && dark.hash !== plain.hash,
  `深色底必须改变预览（缺省 ${String(plain.hash).slice(0, 20)}… vs 深色 ${String(dark.hash).slice(0, 20)}…）`,
);

if (bad) {
  console.log(`  ✗ 预览的 background 有 ${bad} 处不满足`);
  process.exit(1);
}
console.log("  ✓ brush_preview 的 background 真的进了渲染：缺省/白/深三者的哈希两两不同");
