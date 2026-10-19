#!/usr/bin/env node
// **`blend_mode="multiply"` 必须在 sRGB 空间把像素乘起来**（第 854 轮；外部 §139）
//  外部要「整体压暗但保住对比度」：叠加式 `new = old×(1−a) + c×a` 会让 std 按 (1−a) 缩，
//  而乘法 `new = old×k` 让均值和 std 同时按 k 缩。他们说「没有 multiply」——
//  实测**有**，走 `set_property(layer_id, key="blend_mode", value="multiply")`。
//  本判据把那个实测固化成断言：下层 0.6 灰 + 上层 0.6 灰 ⇒ normal 留上层，
//  multiply 必须显著变暗且**落在 sRGB 乘法的预期带里**。
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-blend-mode-multiply.mjs <base-url>"); process.exit(2); }
const doc = `bm-${process.pid}-${Date.now() % 100000}`;
trackTemp(mkdtempSync(join(tmpdir(), "blend-mode-")));
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 64, height: 64 }),
})).json();
const token = created.token;
if (!token) { console.error("判据自身前提不成立：拿不到 token（不是产品问题）"); process.exit(3); }
const call = (tool, args) => fetch(`${base}/api/tools?doc=${doc}&token=${token}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ tool, arguments: args }),
}).then((r) => r.json());
const px = async () => {
  const r = await call("sample_colors", { points: [[32, 32]] });
  const s = (r.samples ?? [])[0];
  return s && s.rgba ? s.rgba[0] : null;
};

// 下层 0.6 灰、上层 0.6 灰
for (const [id, v] of [["bg", 0.6], ["fg", 0.6]]) {
  await call("create_layer", { layer_id: id, name: id });
  await call("fill_region", { layer_id: id, shape: { kind: "rect", x: 0, y: 0, w: 64, h: 64 }, color: [v, v, v, 1] });
}
const normal = await px();
const r = await call("set_property", { layer_id: "fg", key: "blend_mode", value: "multiply" });
if (r.ok !== true) {
  console.error(`set_property(blend_mode=multiply) 被拒 ⇒ ${JSON.stringify(r.context ?? {})}`);
  process.exit(1);
}
const multiplied = await px();
console.log(`  像素：normal=${normal} → multiply=${multiplied}`);

const failures = [];
if (!(multiplied < normal)) {
  failures.push(`multiply 之后像素没变暗（${normal} → ${multiplied}）⇒ blend_mode 没生效 ✗`);
}
// sRGB 空间乘法：期望 ≈ round(255 * (normal/255)^2)
const expect = Math.round(255 * (normal / 255) ** 2);
const tol = 12;   // 容差：AA/舍入/色彩空间细节
if (Math.abs(multiplied - expect) > tol) {
  failures.push(`multiply 实测 ${multiplied}，sRGB 乘法期望 ${expect}±${tol} ⇒ 不是按 sRGB 乘的 ✗`);
}
if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log(`  ✓ multiply 生效且在 sRGB 空间（实测 ${multiplied}，期望 ${expect}±${tol}）`);
