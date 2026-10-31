#!/usr/bin/env node
// **画布几何的入参判界** ✓（第 1364 轮实测：`crop_canvas` 传 `x=1e19` 会**静默
// 把画布裁成 1×1** —— f64→i64 的 `as` 饱和到 `i64::MAX`，release 下 `x + w`
// 回绕成负数 ⇒ 越界请求**看起来成功** ✗ ⇒ 断的是「拒绝」+「画布不变」✓）
//
// **判据**：
//   ① 越界 / 负数 / 非整数 crop ⇒ `ok:false` ＋ `invalid_argument`，画布尺寸不变；
//   ② 合法 crop ⇒ 画布变成区域尺寸；
//   ③ extend 负边距 / 全零 / 溢出边距 ⇒ 拒绝，尺寸不变；合法 extend ⇒ 尺寸相加；
//   ④ scale 巨大比例 / 未开 non_uniform 的非均匀 ⇒ 拒绝，尺寸不变；
//   ⑤ `resize_canvas_bounds` 的 `width = 2^32+300` ⇒ 拒绝（修前 `as u32` 折回
//      300 ⇒ 看起来成功），`0` / 超单边上限同样拒绝；
//   ⑥ rotate 非 90° 倍数 ⇒ 拒绝；90° ⇒ 宽高互换。
//
// **变异**（改坏实现必须红）：
//   · crop 判界改回 `x + w <= old_width`（去掉 checked_add）⇒ ① 的 1e19 臂变 ok ⇒ 红；
//   · extend 去掉 checked_add ⇒ ③ 的溢出臂变 ok ⇒ 红；
//   · resize 改回 `as u32` ⇒ ⑤ 的 2^32+300 臂变 ok（尺寸 300）⇒ 红；
//   · scale 去掉 f64 判界 ⇒ ④ 的巨大比例臂变 ok ⇒ 红。
const [base] = process.argv.slice(2);
if (!base) { console.error("用法: node scripts/tool-geometry-bounds.mjs <base-url>"); process.exit(3); }

const boot = await fetch(`${base}/api/documents`, {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: `geom_bounds_${Date.now()}`, width: 200, height: 150 }),
});
const bootBody = await boot.json().catch(() => ({}));
const token = bootBody.token;
if (!token) { console.log(`  ✗ 建文档失败：${JSON.stringify(bootBody).slice(0, 140)}`); process.exit(1); }
const doc = bootBody.doc ?? bootBody.doc_id;
if (!doc) { console.log(`  ✗ 建文档回执缺 doc id：${JSON.stringify(bootBody).slice(0, 140)}`); process.exit(1); }

const call = async (name, args) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
  const reply = await fetch(url, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args ?? {}) });
  return await reply.json();
};

let bad = 0;
const dims = async () => {
  const canvas = await call("describe_canvas", {});
  return [canvas.width, canvas.height];
};
const expectRejected = async (name, args, label) => {
  const before = await dims();
  const r = await call(name, args);
  const after = await dims();
  const rejected = r.ok !== true && r.error_code === "invalid_argument";
  const unchanged = before[0] === after[0] && before[1] === after[1];
  console.log(`  ${rejected && unchanged ? "✓" : "✗"} **${label}** ⇒ ok=${r.ok} error_code=${r.error_code ?? "无"}｜画布 ${before.join("×")}→${after.join("×")}（须拒绝且不变）`);
  if (!rejected || !unchanged) bad += 1;
};
const expectAccepted = async (name, args, want, label) => {
  const r = await call(name, args);
  const after = await dims();
  const ok = r.ok === true && after[0] === want[0] && after[1] === want[1];
  console.log(`  ${ok ? "✓" : "✗"} **${label}** ⇒ ok=${r.ok}｜画布 ${after.join("×")}（期望 ${want.join("×")}）`);
  if (!ok) bad += 1;
  return r;
};

// ---- ① 越界 / 负数 / 非整数 crop ----
await expectRejected("crop_canvas", { region: { x: 1e19, y: 0, w: 10, h: 10 } }, "crop x=1e19（修复前静默裁成 1×1）");
await expectRejected("crop_canvas", { region: { x: 190, y: 0, w: 20, h: 10 } }, "crop 右侧越界（190+20>200）");
await expectRejected("crop_canvas", { region: { x: -1, y: 0, w: 10, h: 10 } }, "crop 负 x");
await expectRejected("crop_canvas", { region: { x: 10.5, y: 0, w: 10, h: 10 } }, "crop 非整数 x");
// ---- ② 合法 crop ----
await expectAccepted("crop_canvas", { region: { x: 10, y: 10, w: 50, h: 40 } }, [50, 40], "合法 crop 10,10 50×40");

// ---- ③ extend ----
await expectRejected("extend_canvas", { margins: { left: -1, top: 0, right: 0, bottom: 0 } }, "extend 负边距");
await expectRejected("extend_canvas", { margins: { left: 0, top: 0, right: 0, bottom: 0 } }, "extend 全零边距");
await expectRejected("extend_canvas", { margins: { left: 1e19, top: 0, right: 0, bottom: 0 } }, "extend 边距溢出");
await expectAccepted("extend_canvas", { margins: { left: 5, top: 5, right: 5, bottom: 5 } }, [60, 50], "合法 extend 四边各 5");

// ---- ④ scale ----
await expectRejected("scale_canvas", { scale: 2.1e7 }, "scale=2.1e7（超单边上限）");
await expectRejected("scale_canvas", { scale_x: 2, scale_y: 1 }, "非均匀缩放未开 non_uniform");
await expectRejected("scale_canvas", { scale: 0 }, "scale=0");

// ---- ⑤ resize_canvas_bounds ----
await expectRejected("resize_canvas_bounds", { width: 2 ** 32 + 300, height: 50 }, "width=2^32+300（修前 as u32 折回 300）");
await expectRejected("resize_canvas_bounds", { width: 0, height: 50 }, "width=0");
await expectRejected("resize_canvas_bounds", { width: 20000, height: 50 }, "width=20000（超单边上限 16384）");
await expectAccepted("resize_canvas_bounds", { width: 70, height: 60 }, [70, 60], "合法 resize 70×60");

// ---- ⑥ rotate ----
await expectRejected("rotate_canvas", { degrees: 45 }, "rotate 45°（非 90° 倍数）");
await expectAccepted("rotate_canvas", { degrees: 90 }, [60, 70], "rotate 90° 宽高互换");

if (bad) { console.log(`  ✗ ${bad} 处不符（画布几何判界）`); process.exit(1); }
console.log("  ✓ 画布几何入参判界：越界/溢出/非法一律拒绝且画布不变，合法操作尺寸正确");
