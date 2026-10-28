#!/usr/bin/env node
// **B4 生效条件运行时回显**：`pressure_curve`/`humanize`/`color.a` 传了却不生效时，
// `brush_stroke` 回执的 `warnings` 必须出声（只回显，不改行为、不改缺省）。
//
// **判据**（每臂独立文档画像素臂，警告臂共用传入文档、各回执独立读）：
//   ① 点里带起伏压力 + `pressure_curve:"auto"` ⇒ `warnings` 含"不生效"信号；
//   ② 点里带恒定压力 + `auto` ⇒ 信号消失且包络生效（像素与 `flat` 不同——同一串点只改 `pressure_curve`）；
//   ③ **P-19 之后**：`color.a` 生效 ⇒ 不再出声（对象式、数组 4 元各一臂），`a=0` 按零墨规矩拒绝，
//      只有 `color_to` 的 `a` 仍被忽略、仍要出声；
//   ④ 不传这两个参数的普通一笔 ⇒ 不多出这类信号（`warnings` 里无"不生效"/"被忽略"字样）；
//   ⑤ `humanize:true` 显式传且包络未生效 ⇒ 与第 ① 条合并为一条警告（起伏压力臂、恒定压力+flat 缺省臂；
//      `auto` + `humanize:true` 同给时只一条含"不生效"的警告，不重复）。
//
// **变异**：删掉警告分支（信号恒缺）⇒ ①③c⑤ 红；把包络条件改回"只看有没有压力" ⇒ ② 红；
//   把 P-19 的 alpha 乘法改回忽略 ⇒ 挂回警告时 ③a/③b 红、即便静默不回警告也有 ③d/③e 红（零墨拒绝＋相对墨量）。
//
// 用法：node scripts/tool-pressure-curve-echo.mjs <base> <doc> <token>
const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-pressure-curve-echo.mjs <base> <doc> <token>");
  process.exit(2);
}
const call = async (d, t, name, args) =>
  (
    await fetch(`${base}/api/tools/${name}?doc=${encodeURIComponent(d)}&token=${encodeURIComponent(t)}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(args ?? {}),
    })
  ).json();
const fetchBytes = async (url) => {
  const absolute = String(url).startsWith("http") ? String(url) : `${base}${url}`;
  return new Uint8Array(await (await fetch(absolute)).arrayBuffer());
};

const W = 320;
const H = 240;
const BRUSH = "2B_pencil.myb";
const FLAT = [
  [60, 120, 0.8],
  [160, 120, 0.8],
  [260, 120, 0.8],
];
const VARIED = [
  [60, 120, 0.2],
  [160, 120, 1.0],
  [260, 120, 0.2],
];
const BLACK = { r: 0, g: 0, b: 0 };

const fails = [];
const check = (ok, msg) => {
  if (!ok) fails.push(msg);
  console.log(`  ${ok ? "✓" : "✗"} ${msg}`);
};
const warningsOf = (reply) => (Array.isArray(reply?.warnings) ? reply.warnings.map(String) : []);
const hasWord = (reply, word) => warningsOf(reply).some((w) => w.includes(word));

const paint = async (d, t, extra, points, color) => {
  // **同一文档上每笔 object_id 必须唯一**（复用会报"对象已存在"）。
  paint.n = (paint.n ?? 0) + 1;
  const replied = await call(d, t, "brush_stroke", {
    layer_id: "L1",
    object_id: `s${paint.n}`,
    brush: BRUSH,
    size: 40,
    color: color ?? BLACK,
    points,
    ...(extra ?? {}),
  });
  if (!replied || replied.ok !== true) {
    const detail = replied?.context?.detail ?? replied?.error ?? "?";
    console.error(`❌ 判不了：落笔失败（${String(detail).slice(0, 140)}）=> 退出码 3`);
    process.exit(3);
  }
  return replied;
};

// **像素臂必须用独立文档**（916 轮教训：共用文档会互相污染 ⇒ 对照不成立）。
const newDoc = async (tag) => {
  const id = `pce_${tag}_${Date.now().toString(36)}`;
  let created;
  try {
    created = await (
      await fetch(`${base}/api/documents`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ doc_id: id, width: W, height: H }),
      })
    ).json();
  } catch (error) {
    console.error(`❌ 判不了：建不了文档（${error}）=> 退出码 3`);
    process.exit(3);
  }
  if (!created || !created.token) {
    console.error("❌ 判不了：建不了文档 => 退出码 3");
    process.exit(3);
  }
  await call(id, created.token, "create_layer", { layer_id: "L1", name: "L1" });
  return { doc: id, token: created.token };
};
const pixels = async (d, t) => {
  const raw = await call(d, t, "render_region", { region: { x: 0, y: 0, w: W, h: H }, raw: true });
  const url = raw && (raw.raw_url || raw.rawUrl);
  if (!url) {
    console.error("❌ 判不了：拿不到像素 => 退出码 3");
    process.exit(3);
  }
  const bytes = await fetchBytes(url);
  let fingerprint = 0;
  let ink = 0;
  for (let i = 0; i < bytes.length; i += 4) {
    fingerprint = (fingerprint * 31 + bytes[i] + bytes[i + 1] * 3 + bytes[i + 2] * 7) >>> 0;
    // **墨 = 离纯白至少一档的像素** ✓（③e 比的是相对量：a=110 的半透明笔芯在 600 门槛下会数成 0 ✗
    //   —— 本机实测 1273 ⇒ 0；门槛放到 762 ⇒ 淡笔也数得到，相对关系仍成立 ✓）。
    if (bytes[i] + bytes[i + 1] + bytes[i + 2] < 762) ink += 1;
  }
  return { fingerprint, ink };
};

const main = async () => {
  // **先建 L1**（传入文档只有缺省图层，落笔要显式图层）。
  await call(doc, token, "create_layer", { layer_id: "L1", name: "L1" });
  // **① 起伏压力 + auto ⇒ "不生效"信号**（警告臂共用传入文档，回执各自独立）。
  const r1 = await paint(doc, token, { pressure_curve: "auto" }, VARIED);
  check(
    hasWord(r1, "不生效") && warningsOf(r1).some((w) => w.includes("pressure_curve")),
    `① 起伏压力 + auto ⇒ warnings 含"不生效"（实测 ${JSON.stringify(warningsOf(r1))}）`,
  );

  // **② 恒定压力 + auto ⇒ 信号消失且包络生效**（同一串点，只改 pressure_curve，比像素）。
  const docAuto = await newDoc("auto");
  const docFlat = await newDoc("flat");
  const rAuto = await paint(docAuto.doc, docAuto.token, { pressure_curve: "auto" }, FLAT);
  const rFlat = await paint(docFlat.doc, docFlat.token, { pressure_curve: "flat" }, FLAT);
  check(
    !hasWord(rAuto, "不生效"),
    `② 恒定压力 + auto ⇒ 无"不生效"信号（实测 ${JSON.stringify(warningsOf(rAuto))}）`,
  );
  const pxAuto = await pixels(docAuto.doc, docAuto.token);
  const pxFlat = await pixels(docFlat.doc, docFlat.token);
  check(
    pxAuto.fingerprint !== pxFlat.fingerprint,
    `② 恒定压力 + auto ⇒ 像素与 flat 不同（auto 指纹 ${pxAuto.fingerprint}｜flat 指纹 ${pxFlat.fingerprint}）`,
  );

  // **③ P-19：`color.a` 乘进整笔 opacity ⇒ 不再出声；`a=0` 按零墨规矩拒绝；`color_to.a` 仍忽略仍出声**。
  const r3a = await paint(doc, token, {}, VARIED, { r: 200, g: 30, b: 40, a: 110 });
  check(
    !warningsOf(r3a).some((w) => w.includes("color.a") && w.includes("被忽略")),
    `③a 对象式 color.a=110 生效 ⇒ warnings 不得再有 color.a"被忽略"（实测 ${JSON.stringify(warningsOf(r3a))}）`,
  );
  const r3b = await paint(doc, token, {}, VARIED, [200, 30, 40, 110]);
  check(
    !warningsOf(r3b).some((w) => w.includes("color.a") && w.includes("被忽略")),
    `③b 数组 4 元 color.a=110 生效 ⇒ 同上（实测 ${JSON.stringify(warningsOf(r3b))}）`,
  );
  const r3c = await call(doc, token, "brush_stroke", {
    layer_id: "L1",
    object_id: "s_color_to",
    brush: BRUSH,
    size: 40,
    color: BLACK,
    color_to: { r: 200, g: 30, b: 40, a: 110 },
    points: VARIED,
  });
  if (!r3c || r3c.ok !== true) {
    console.error(`❌ 判不了：color_to 落笔失败（${String(r3c?.context?.detail ?? "?").slice(0, 140)}）=> 退出码 3`);
    process.exit(3);
  }
  check(
    hasWord(r3c, "被忽略"),
    `③c color_to.a=110 ⇒ warnings 含"被忽略"（实测 ${JSON.stringify(warningsOf(r3c))}）`,
  );
  // **③d `a=0` 按零墨规矩拒绝** ✓（`precondition_failed`，与显式 `opacity: 0` 同款，不留原子）。
  const r3d = await call(doc, token, "brush_stroke", {
    layer_id: "L1",
    object_id: "s_zero_a",
    brush: BRUSH,
    size: 40,
    color: { r: 200, g: 30, b: 40, a: 0 },
    points: VARIED,
  });
  check(
    r3d?.ok === false && r3d?.error_code === "precondition_failed",
    `③d color.a=0 ⇒ precondition_failed 拒绝（实测 ${JSON.stringify({ ok: r3d?.ok, error_code: r3d?.error_code })}）`,
  );
  // **③e 生效臂** ✓：只"把话删了但 alpha 仍被忽略"也是静默降级 ✗ ⇒
  //   独立文档（916 轮教训）比 a=255 与 a=110 两笔的**相对墨量**（断关系不写像素数 ✓）。
  const docA255 = await newDoc("a255");
  const docA110 = await newDoc("a110");
  await paint(docA255.doc, docA255.token, {}, VARIED, { r: 200, g: 30, b: 40, a: 255 });
  await paint(docA110.doc, docA110.token, {}, VARIED, { r: 200, g: 30, b: 40, a: 110 });
  const ink255 = await pixels(docA255.doc, docA255.token);
  const ink110 = await pixels(docA110.doc, docA110.token);
  const docA0 = await newDoc("a0");
  const r0paint = await call(docA0.doc, docA0.token, "brush_stroke", {
    layer_id: "L1",
    object_id: "s_zero_a2",
    brush: BRUSH,
    size: 40,
    color: { r: 200, g: 30, b: 40, a: 0 },
    points: VARIED,
  });
  check(
    r0paint?.ok === false,
    `③e0 a=0 在独立文档上也被拒（实测 ok=${JSON.stringify(r0paint?.ok)}）`,
  );
  const ink0 = await pixels(docA0.doc, docA0.token);
  check(
    ink0.ink === 0,
    `③e1 a=0 的文档必须真零墨（实测 ${ink0.ink} 个有墨像素）`,
  );
  check(
    ink110.ink > 0 && ink110.ink < ink255.ink,
    `③e2 a=110 的墨应多于 0、少于 a=255（实测 ${ink0.ink} ⇒ ${ink110.ink} ⇒ ${ink255.ink}）`,
  );

  // **④ 普通一笔（不传 pressure_curve/humanize/color.a）⇒ 不多出这类信号**。
  const r4 = await paint(doc, token, {}, VARIED);
  check(
    !hasWord(r4, "不生效") && !hasWord(r4, "被忽略"),
    `④ 普通一笔 ⇒ warnings 无"不生效"/"被忽略"（实测 ${JSON.stringify(warningsOf(r4))}）`,
  );

  // **⑤ humanize:true 显式传且包络未生效 ⇒ 合并为一条警告**。
  const r5a = await paint(doc, token, { humanize: true }, VARIED);
  check(
    hasWord(r5a, "不生效"),
    `⑤a humanize:true + 起伏压力 ⇒ warnings 含"不生效"（实测 ${JSON.stringify(warningsOf(r5a))}）`,
  );
  const r5b = await paint(doc, token, { humanize: true }, FLAT);
  check(
    hasWord(r5b, "不生效"),
    `⑤b humanize:true + 恒定压力 + flat 缺省 ⇒ warnings 含"不生效"（实测 ${JSON.stringify(warningsOf(r5b))}）`,
  );
  const r5c = await paint(doc, token, { pressure_curve: "auto", humanize: true }, VARIED);
  const merged = warningsOf(r5c).filter((w) => w.includes("不生效"));
  check(
    merged.length === 1,
    `⑤c auto + humanize:true + 起伏压力 ⇒ 只一条"不生效"警告（实测 ${merged.length} 条：${JSON.stringify(merged)}）`,
  );

  if (fails.length) {
    console.error(`❌ ${fails.length} 项没成立：${fails.join("｜")}`);
    process.exit(1);
  }
  console.log("✓ B4 回显成立：auto 起伏压力出声、恒定压力包络生效且无信号、color.a 生效不出声（P-19）、a=0 零墨拒绝、color_to.a 仍出声、普通一笔不多话、humanize 合并一条");
};

await main();
