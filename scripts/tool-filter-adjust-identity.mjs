#!/usr/bin/env node
// **恒等参数必须让画面不变**（第 1515 轮；外部实测被 `add_filter` 的 `contrast:0` 坑成全黑）
//
// **⇒ 要守的恒等式两条**：
//   ① `add_filter` + `brightness_contrast` + `{brightness:0, contrast:1}` ⇒ **均值不变** ✓
//   ② `add_adjustment` + `brightness_contrast` + `{}` ⇒ **均值不变** ✓
//   **⇒ ⇒ 而 `{brightness:0, contrast:0}` **不是恒等**（它全黑）⇒ 判据**断言它变黑**，
//     以**钉住这个约定**（免得哪天有人"顺手改成恒等" ⇒ 反而破坏兼容）✓
//
// **⇒ 为什么用均值而不是逐像素**：均值是**结构量**，跨机稳定 ✓（外部与本机都测 128 基线）
// **⇒ 退出码**：0 通过｜1 不符｜3 不可判
const [base] = process.argv.slice(2);
if (!base) { console.error("用法: node scripts/tool-filter-adjust-identity.mjs <base>"); process.exit(3); }
const stamp = Date.now();
const fails = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fails.push(m); console.log(`  ✗ ${m}`); };

const setup = async (tag) => {
  const doc = `id_${tag}_${stamp}`;
  const made = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: 200, height: 150,
      background: { r: 180, g: 150, b: 120, a: 255 } }),
  })).json();
  if (!made.token) return null;
  const call = async (tool, args) => (await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${made.token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args ?? {}),
  })).json();
  await call("create_layer", { layer_id: "L", name: "L" });
  await call("fill_region", { layer_id: "L",
    shape: { type: "rect", x: 10, y: 10, w: 180, h: 130 }, color: [128, 128, 128], opacity: 1.0 });
  const mean = async () => {
    const s = await call("sample_colors", { points: [[50,50],[100,75],[150,100],[60,110],[140,40]] });
    const vals = (s?.samples || []).map((z) => {
      const c = (z || {}).rgba || [0, 0, 0];
      return (c[0] + c[1] + c[2]) / 3;
    });
    return vals.length ? vals.reduce((a, b) => a + b, 0) / vals.length : null;
  };
  return { call, mean };
};

const ref = await setup("ref");
if (!ref) { console.log("  ○ 不可判：建不出文档"); process.exit(3); }
const base0 = await ref.mean();
if (Math.abs(base0 - 128) > 3) { console.log(`  ○ 不可判：基线均值 ${base0} 不等于 128`); process.exit(3); }

// ① add_filter 的恒等参数（contrast=1）
{
  const h = await setup("f");
  const r = await h.call("add_filter", { layer_id: "L", filter_name: "brightness_contrast",
    params: { brightness: 0, contrast: 1 } });
  const m = await h.mean();
  if (r?.ok === true && Math.abs(m - base0) <= 3) ok(`① add_filter {brightness:0,contrast:1} ⇒ 均值 ${m}（恒等 ✓）`);
  else bad(`① add_filter 的恒等参数应不变 ⇒ ok=${r?.ok}、均值 ${m}（基线 ${base0}）`);
}
// ② add_filter 的 {0,0} 必须**变黑**（钉住约定，不是恒等）
{
  const h = await setup("fb");
  const r = await h.call("add_filter", { layer_id: "L", filter_name: "brightness_contrast",
    params: { brightness: 0, contrast: 0 } });
  const m = await h.mean();
  if (r?.ok === true && m < 5) ok(`② add_filter {brightness:0,contrast:0} ⇒ 均值 ${m}（**全黑，约定成立** ✓）`);
  else bad(`② {brightness:0,contrast:0} 应是全黑（contrast 是乘法增益）⇒ 实测 ${m} ⇒ **约定变了就说明实现改了**`);
}
// ③ add_adjustment 的默认参数必须恒等
{
  const h = await setup("a");
  const r = await h.call("add_adjustment", { layer_id: "L", adjustment_type: "brightness_contrast", params: {} });
  const m = await h.mean();
  if (r?.ok === true && Math.abs(m - base0) <= 3) ok(`③ add_adjustment brightness_contrast({}) ⇒ 均值 ${m}（恒等 ✓）`);
  else bad(`③ add_adjustment 的默认参数应恒等 ⇒ ok=${r?.ok}、均值 ${m}`);
}
// ④ add_adjustment levels 默认必须恒等
{
  const h = await setup("lv");
  const r = await h.call("add_adjustment", { layer_id: "L", adjustment_type: "levels", params: {} });
  const m = await h.mean();
  if (r?.ok === true && Math.abs(m - base0) <= 3) ok(`④ add_adjustment levels({}) ⇒ 均值 ${m}（恒等 ✓）`);
  else bad(`④ add_adjustment levels 默认应恒等 ⇒ ok=${r?.ok}、均值 ${m}`);
}

if (fails.length > 0) { console.log(`\n  ✗ ${fails.length} 处不符（恒等参数）`); process.exit(1); }
console.log("\n  ✓ 恒等参数的正确性已钉住（两条路 + 两套约定）");
