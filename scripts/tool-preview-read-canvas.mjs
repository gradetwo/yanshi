// **`brush_preview` 的 `read_canvas` 必须**真的改变渲染**✗**（外部复测 G02 第三项的判据 ✓）
//
// **∴ 现状 ✓**：**∴ `read_canvas` **改了** `feed_base`**✗** ⇒ **∴ 对**会读画布的笔**✗**
//   ⇒ **∴ 输出**确实不同**✗**（**∴ `thin_watercolor`：**2703 → 3826** 字节 ✓**）
//   ⇒ **∴ 但**底图**读不到**✗**（**∴ 空白 vs 有笔触**同图 ✓**）⇒ **∴ 那**是**渲染层**✗（**∴ 边界外 ✓**）
//
// **∴ 所以判据只能断**"参数生效"**✗** ⇒ **∴ 那**是**弱断言**✗**
//   ⇒ **∴ 按 §61**，弱断言**必须**证明自己能红**✗** ⇒ **∴ 变异**：**∴ 忽略 `read_canvas` ✗**
//     ⇒ **∴ 字节数**退回 2703**✗** ⇒ **∴ 判据**红 ✓
const base = process.argv[2] ?? "http://127.0.0.1:0";
const stamp = Date.now();

const post = async (path, body, doc, token) => {
  const q = doc ? `?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}` : "";
  const reply = await fetch(`${base}${path}${q}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body ?? {}),
  });
  return reply.json();
};

const doc = `rc_${stamp}`;
const created = await post("/api/documents", { doc_id: doc, width: 400, height: 400 });
if (!created?.token) { console.log("  ✗ 建文档失败（判据前提不成立）"); process.exit(3); }
const token = created.token;

let bad = 0;
const check = (ok, why) => { console.log(`  ${ok ? "✓" : "✗"} ${why}`); if (!ok) bad += 1; };

// **∴ 用**会读画布的笔**✗**（**∴ `pointy_ink` 不读 ⇒ 四种组合全同 ✓，所以它**不能**用来测这条 ✓**）
const preview = async (readCanvas) =>
  post(
    "/api/tools/brush_preview",
    { brush: "thin_watercolor", size: 30, include_image: true, read_canvas: readCanvas },
    doc,
    token,
  );

const off = await preview(false);
const on = await preview(true);
console.log(`  read_canvas=false ⇒ bytes=${off?.bytes}｜hash=${String(off?.blob_hash ?? "").slice(0, 18)}`);
console.log(`  read_canvas=true  ⇒ bytes=${on?.bytes}｜hash=${String(on?.blob_hash ?? "").slice(0, 18)}`);

check(off?.ok === true && on?.ok === true, `两种取值都必须成功（false=${off?.ok}｜true=${on?.ok}）`);
check(
  typeof off?.bytes === "number" && typeof on?.bytes === "number" && off.bytes !== on.bytes,
  `read_canvas 必须真的改变渲染（false=${off?.bytes} B vs true=${on?.bytes} B）—— 若相等，说明参数被忽略`,
);
// **∴ 而**这条**同时记下**"底图还没读到"**✗**（**∴ 不把它当通过，只当现状 ✓**）
if (off?.blob_hash === on?.blob_hash) {
  console.log("  ⚠ 注意：两者哈希相同 ⇒ 底图仍未读入（渲染层，边界外）");
}

if (bad) { console.log(`  ✗ read_canvas 有 ${bad} 处不满足`); process.exit(1); }
console.log("  ✓ read_canvas 参数生效（改了渲染）；底图内容是否读入不在本判据范围");
