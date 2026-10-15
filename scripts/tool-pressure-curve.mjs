#!/usr/bin/env node
// **★ `pressure_curve`：压感包络的判据（真机报告 P1 / 新 P0）★**（第 254 轮）
//
// **需求原话**："感觉你的笔触还是太硬，压力变化之类你没用？"
// **根因**：程序生成的 pressure 是**恒定值**（`0.8` 这种 ✓），而包络原来**只在"完全没给 pressure"时**才加 ✗
// => 给了恒定值就**绕过了**包络 ✓ => 笔触像矢量图 ✗。
//
// **本判据要证三条**：
//   ① **`auto`（缺省）对"恒定 pressure"有牙** ✓：同一笔，`auto` 与 `pressure_curve: flat`
//      的画面**必须不同**（实测 `2B_pencil`：墨量 **1023 vs 3470** ✓）；
//   ② **`auto` 对"有起伏的 pressure"原样尊重** ✓：那两笔必须**逐字节相同**（不叠两层 ✗）；
//   ③ **`humanize: false` ⇒ 一律不动** ✓（硬约束：`humanize:false` 时老行为逐字节不变 ✓）。
//
// **变异（能红的根据）**：把 `envelope_wanted` 写死成 `!all_points_have_pressure(args)`
// （即改回旧行为）⇒ ① 红；把 `humanize` 判断去掉 ⇒ ③ 红；让包络对任何 pressure 都动手 ⇒ ② 红。
//
// 用法：node scripts/tool-pressure-curve.mjs <base-url>
const base = (process.argv[2] || "").replace(/\/+$/, "");
if (!base) {
  console.error("用法: node scripts/tool-pressure-curve.mjs <base-url>");
  process.exit(2);
}
const call = async (doc, token, tool, args) =>
  (
    await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(args || {}),
    })
  ).json();
const fetchBytes = async (url) => {
  const absolute = String(url).startsWith("http") ? String(url) : `${base}${url}`;
  return new Uint8Array(await (await fetch(absolute)).arrayBuffer());
};

const W = 320;
const H = 240;
// **★ 笔刷要选"压力→尺寸敏感"的 ★**：实测 `100%_Opaque` 的映射是**平的** ✗
//   => 包络在它身上**看不出差别**（我先用错了笔刷 ⇒ 差点以为包络没生效 ✗）。
//   `2B_pencil`／`brushkit-pen` 的差别非常大（墨量 1023 vs 3470 ／ 844 vs 4960 ✓）。
const BRUSH = "2B_pencil.myb";

const paint = async (tag, extra, points) => {
  const doc = `pcv_${tag}_${Date.now().toString(36)}`;
  let created;
  try {
    created = await (
      await fetch(`${base}/api/documents`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ doc_id: doc, width: W, height: H }),
      })
    ).json();
  } catch (error) {
    console.error(`❌ 判不了：连不上（${error}）=> 退出码 3`);
    process.exit(3);
  }
  if (!created || !created.token) {
    console.error("❌ 判不了：建不了文档 => 退出码 3");
    process.exit(3);
  }
  const token = created.token;
  await call(doc, token, "create_layer", { layer_id: "L1", name: "L1" });
  const painted = await call(doc, token, "brush_stroke", {
    layer_id: "L1",
    object_id: "s",
    brush: BRUSH,
    size: 40,
    color: { r: 0, g: 0, b: 0, a: 255 },
    points,
    ...extra,
  });
  if (!painted || painted.ok !== true) {
    const detail = painted && painted.context && painted.context.detail;
    console.error(`❌ 判不了：落笔失败（${detail ? String(detail).slice(0, 140) : "?"}）=> 退出码 3`);
    process.exit(3);
  }
  const raw = await call(doc, token, "render_region", {
    region: { x: 0, y: 0, w: W, h: H },
    raw: true,
  });
  const url = raw && (raw.raw_url || raw.rawUrl);
  if (!url) {
    console.error("❌ 判不了：拿不到像素 => 退出码 3");
    process.exit(3);
  }
  const bytes = await fetchBytes(url);
  let fingerprint = 0;
  let ink = 0;
  for (let i = 0; i < bytes.length; i += 4) {
    fingerprint = ((fingerprint * 31 + bytes[i] + bytes[i + 1] * 3 + bytes[i + 2] * 7) >>> 0);
    if (bytes[i] + bytes[i + 1] + bytes[i + 2] < 600) ink += 1;
  }
  return { fingerprint, ink };
};

const main = async () => {
  const flat = [
    [60, 120, 0.8],
    [160, 120, 0.8],
    [260, 120, 0.8],
  ];
  const varied = [
    [60, 120, 0.2],
    [160, 120, 1.0],
    [260, 120, 0.2],
  ];
  const bad = [];

  const autoFlat = await paint("auto_flat", {}, flat);
  const explicitFlat = await paint("flat_flat", { pressure_curve: "flat" }, flat);
  const autoVaried = await paint("auto_var", {}, varied);
  const flatVaried = await paint("flat_var", { pressure_curve: "flat" }, varied);
  const humanizeOff = await paint("hz_off", { humanize: false }, flat);
  const humanizeOffFlat = await paint("hz_off_flat", { humanize: false, pressure_curve: "flat" }, flat);

  console.log(
    `  ① 恒定 0.8：auto 墨 ${autoFlat.ink}｜flat 墨 ${explicitFlat.ink}（` +
      `${autoFlat.fingerprint === explicitFlat.fingerprint ? "相同 ✗" : "不同 ✓"}）`,
  );
  console.log(
    `  ② 有起伏：auto 指纹 ${autoVaried.fingerprint}｜flat 指纹 ${flatVaried.fingerprint}（` +
      `${autoVaried.fingerprint === flatVaried.fingerprint ? "相同 ✓" : "不同 ✗"}）`,
  );
  console.log(
    `  ③ humanize:false：缺省 vs flat（${humanizeOff.fingerprint === humanizeOffFlat.fingerprint ? "相同 ✓" : "不同 ✗"}）`,
  );

  if (autoFlat.fingerprint === explicitFlat.fingerprint) {
    bad.push(
      "① **对「恒定 pressure」没牙**：`auto`（缺省）与 `pressure_curve: flat` 的画面**逐字节相同**" +
        " ⇒ 包络没有作用在程序生成的恒定压感上（这正是「笔触太硬」的根因 ✗）",
    );
  }
  if (autoVaried.fingerprint !== flatVaried.fingerprint) {
    bad.push("② 对**有起伏**的 pressure 也动了手 ⇒ 叠了两层包络（应当**原样尊重** ✗）");
  }
  if (humanizeOff.fingerprint !== humanizeOffFlat.fingerprint) {
    bad.push("③ `humanize:false` 时仍然动了 pressure ⇒ 违反硬约束（老行为必须逐字节不变 ✗）");
  }
  if (bad.length) {
    console.error(`❌ ${bad.length} 项没成立：${bad.join("｜")}`);
    process.exit(1);
  }
  console.log(
    "✓ 压感包络成立：恒定压感被补上起收笔（①）、有起伏的原样尊重（②）、humanize:false 一律不动（③）",
  );
};

await main();
