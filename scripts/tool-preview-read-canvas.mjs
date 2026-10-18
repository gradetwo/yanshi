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

// **∴ ③ 响应必须**自己说清"还没好"**✗**（**∴ §62 ✓**）：
//   ⇒ **∴ 否则**对方**只读响应**✗ ⇒ **∴ 会**以为能用**✗** ⇒ **∴ 于是**按错前提改读数**✗** ✓
console.log(`  read_canvas 回显=${JSON.stringify(on?.read_canvas ?? null)}`);
console.log(`  read_canvas_note=${String(on?.read_canvas_note ?? "(缺)").slice(0, 90)}…`);
check(
  on?.read_canvas === true,
  `read_canvas=true 时必须回显 read_canvas: true（实测 ${JSON.stringify(on?.read_canvas ?? null)}）`,
);
check(
  typeof on?.read_canvas_note === "string" &&
    on.read_canvas_note.includes("底图") &&
    /仍未|未读入|未达成/.test(on.read_canvas_note),
  `响应必须自陈"底图仍未读入"（实测 ${JSON.stringify(String(on?.read_canvas_note ?? "").slice(0, 60))}）`,
);

// **∴ ④ 必须有一个**有明确真值的布尔**✗**（**∴ §63 ✓**）：**∴ 对方写的 `is None` 断言恒失败**✗**
//   ⇒ **∴ 因为** `note` 在成功时也有**✗** ⇒ **∴ 所以**机器**该读** `read_canvas_effective`**✗**
//   ⇒ **∴ 而**它**现在**必须**是 `false`**✗**（**∴ 底图确实没读入 ✓**）⇒ **∴ 若**有人**谎报** `true`**✗**
//     ⇒ **∴ 判据**红 ✓（**∴ 那**正是**保护对方**✗ 的东西 ✓）
console.log(`  read_canvas_effective=${JSON.stringify(on?.read_canvas_effective ?? null)}（现状：底图未读入 ⇒ 应为 false）`);
check(
  on?.read_canvas_effective === false,
  `read_canvas_effective 必须是 false（底图确实没读入）；若是 true 则是谎报（实测 ${JSON.stringify(on?.read_canvas_effective ?? null)}）`,
);
check(
  typeof on?.read_canvas_effective === "boolean",
  `read_canvas_effective 必须是布尔、恒存在（实测 ${typeof on?.read_canvas_effective}）—— 对方要写 assert ... is True`,
);

// **∴ ⑥ 互斥时**必须说清谁赢**✗**（**∴ 第 185 节实测 `background` 被静默忽略 ✓**）
//   ⇒ **∴ 两个参数**同时给**✗** ⇒ **∴ `background` **不生效**✗** ⇒ **∴ 响应**必须写明**✗**
//   ⇒ **∴ 而**只给 `background` 时**✗** ⇒ **∴ 那个字段**必须**不出现**✗**（**∴ "只在该出现时出现" ✓**）
const bg = { r: 200, g: 40, b: 40 };
const onlyBg = await post(
  "/api/tools/brush_preview",
  { brush: "thin_watercolor", size: 30, include_image: false, read_canvas: false, background: bg },
  doc,
  token,
);
const both = await post(
  "/api/tools/brush_preview",
  { brush: "thin_watercolor", size: 30, include_image: false, read_canvas: true, background: bg },
  doc,
  token,
);
console.log(`  只有背景 ⇒ background_applied=${JSON.stringify(onlyBg?.background_applied ?? null)}`);
console.log(`  背景+读画布 ⇒ background_applied=${JSON.stringify(both?.background_applied ?? null)}｜note 非空=${!!both?.background_note}`);
check(
  onlyBg?.background_applied == null,
  `只给 background 时 background_applied 不许出现（它是"被覆盖"的信号；实测 ${JSON.stringify(onlyBg?.background_applied ?? null)}）`,
);
check(
  both?.background_applied === false,
  `两个都给时 background_applied 必须是 false（background 被 read_canvas 覆盖；实测 ${JSON.stringify(both?.background_applied ?? null)}）`,
);
check(
  typeof both?.background_note === "string" && both.background_note.includes("覆盖"),
  `两个都给时必须自陈被覆盖（实测 ${JSON.stringify(String(both?.background_note ?? "").slice(0, 50))}）`,
);

// **∴ ⑦ 颜色空间必须**明说**✗**（**∴ G02 第 6 项 ✓**）：**∴ 它**恒为 `rgb`**✗** ⇒ **∴ 而**它**是**读数归因的前提**✗**
//   ⇒ **∴ 若**有人**改混色却不改这个字段**✗ ⇒ **∴ 读数**就会**归因错**✗** ✓
console.log(`  color_space=${JSON.stringify(on?.color_space ?? null)}｜note 非空=${!!on?.color_space_note}`);
check(
  on?.color_space === "rgb",
  `color_space 必须是 rgb（brush_preview 恒用 ColorMixing::Rgb；实测 ${JSON.stringify(on?.color_space ?? null)}）`,
);
check(
  typeof on?.color_space_note === "string" && /RGB|不会变/.test(on.color_space_note),
  `color_space_note 必须说清它是恒定的（实测 ${JSON.stringify(String(on?.color_space_note ?? "").slice(0, 50))}）`,
);

if (bad) { console.log(`  ✗ read_canvas 有 ${bad} 处不满足`); process.exit(1); }
console.log("  ✓ read_canvas 参数生效（改了渲染）；底图内容是否读入不在本判据范围");
