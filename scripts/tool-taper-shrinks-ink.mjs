#!/usr/bin/env node
// **`stroke_taper` 必须让墨量变小**（第 710 轮实测它反而大 8.7%）
//  收锋的语义是「两头变细」⇒ 整笔墨面积必须**小于**不加 taper 的那一笔。
//  用网格采样量面积（不数单列像素：dab 是 aspect=5.46 的椭圆，单列量不准）。
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-taper-shrinks-ink.mjs <base-url>"); process.exit(2); }
const doc = `tp-${process.pid}-${Date.now() % 100000}`;
trackTemp(mkdtempSync(join(tmpdir(), "taper-ink-")));
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 400, height: 400 }),
})).json();
const token = created.token;
if (!token) { console.error("判据自身前提不成立：拿不到 token（不是产品问题）"); process.exit(3); }
const call = (tool, args) => fetch(`${base}/api/tools?doc=${doc}&token=${token}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ tool, arguments: args }),
}).then((r) => r.json());

const pts = [];
for (let y = 150; y <= 250; y += 2) for (let x = 55; x <= 345; x += 2) pts.push([x, y]);
const ink = async (layer, extra) => {
  await call("create_layer", { layer_id: layer, name: layer });
  await call("brush_stroke", {
    layer_id: layer, brush: "calligraphy", size: 30,
    color: [0.05, 0.05, 0.05, 1.0], points: [[60, 200, 0.9], [340, 200, 0.9]],
    preview: false, ...extra,
  });
  const r = await call("sample_colors", { points: pts });
  return (r.samples ?? []).filter((z) => z.rgba && z.rgba[0] < 128).length;
};
const plain = await ink("k0", {});
const taper = await ink("k1", { stroke_taper: 0.8 });
const ratio = taper / Math.max(plain, 1);
console.log(`  不加 taper 墨面积=${plain}｜加 taper=${taper}｜比=${ratio.toFixed(3)}`);
if (plain === 0) { console.error("判据自身前提不成立：基线没画出墨（不是产品问题）"); process.exit(3); }
if (!(ratio < 1.0)) {
  console.log(`  ✗ 加 taper 后墨面积没变小（比 ${ratio.toFixed(3)} ≥ 1）⇒ 收锋的方向是反的：它让笔画更粗`);
  process.exit(1);
}
console.log(`  ✓ 加 taper 后墨面积变小了（比 ${ratio.toFixed(3)}）`);
