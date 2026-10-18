// **`brush_preview` 必须声明并接受 `brush_params` ✗**（外部复测 QY-G02 的判据 ✓）
//
// **∴ 缺陷 ✓**：**∴ handler **早就**转发它了**✗**（**∴ 第 254 轮修过 ✓**）
//   ⇒ **∴ 但** spec **没声明**✗** ⇒ **∴ schema 校验**直接拒绝**✗ ⇒ **∴ `invalid_argument`** ✗**
//   ⇒ **∴ 于是**"用预览试 `brush_params`"✗ **做不到**✗** ⇒ **∴ 而**落笔却能**✗** ⇒ **∴ 预览与落笔**不一致 ✓**
//
// **∴ 断言（两条 ✗）**：
//   ① **∴ 带 `brush_params: {opaque: 0.2}` ✗ ⇒ **∴ `ok=true`** ✗（**∴ 不许** `invalid_argument` ✓**）
//   ② **∴ 且**它**真的生效**✗：**∴ 带与不带的两张预览**像素必须不同**✗**（**∴ 否则**声明了也没转发 ✓**）
const base = process.argv[2] ?? "http://127.0.0.1:0";
const stamp = Date.now();

const post = async (path, body, doc, token) => {
  const q = doc ? `?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}` : "";
  const reply = await fetch(`${base}${path}${q}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body ?? {}),
  });
  return reply.json();
};

const doc = `g2_${stamp}`;
const created = await post("/api/documents", { doc_id: doc, width: 400, height: 400 });
if (!created?.token) { console.log("  ✗ 建文档失败（判据前提不成立）"); process.exit(3); }
const token = created.token;

const preview = async (brushParams) => {
  const args = { brush: "pointy_ink", size: 24, include_image: false };
  if (brushParams) args.brush_params = brushParams;
  return post("/api/tools/brush_preview", args, doc, token);
};

let bad = 0;
const check = (ok, why) => { console.log(`  ${ok ? "✓" : "✗"} ${why}`); if (!ok) bad += 1; };

const plain = await preview(null);
const withParams = await preview({ opaque: 0.2 });
console.log(`  不带 ⇒ ok=${plain?.ok}｜painted=${plain?.painted_pixels}`);
console.log(`  带 opaque:0.2 ⇒ ok=${withParams?.ok}｜painted=${withParams?.painted_pixels}｜err=${withParams?.error_code ?? null}`);

check(
  withParams?.ok === true,
  `带 brush_params 的预览必须成功（不许 invalid_argument；实测 ok=${withParams?.ok}｜err=${withParams?.error_code ?? null}）`,
);
check(
  typeof plain?.painted_pixels === "number" && typeof withParams?.painted_pixels === "number" &&
    plain.painted_pixels !== withParams.painted_pixels,
  `brush_params 必须真的生效：带与不带的两张预览像素必须不同（${plain?.painted_pixels} vs ${withParams?.painted_pixels}）`,
);

if (bad) { console.log(`  ✗ 预览的 brush_params 有 ${bad} 处不满足`); process.exit(1); }
console.log("  ✓ brush_preview 声明并真的转发了 brush_params（与落笔同一套）");
