// **归档膨胀判据**（报告 2.6）：**反复覆盖同一块画布 ⇒ 被替代的历史 blob 不应留在导出的 `.yanshi` 里** ✓。
// 原理 ✓：`.yanshi` 是未压缩 tar ✓（`archive.rs` 开头写了理由 ✓）；若被覆盖的历史 Tile/blob 不回收 ✓
// ⇒ **体积随"覆盖次数"线性增长** ✗ ⇒ 与**只画一次**的对照组一比就看得出来 ✓。
// 期望（修好之后）✓：12 次覆盖与 1 次覆盖的体积**同量级** ✓（差异 < 1.5× 之类 ✓）；
// 今天预期**红** ✓ —— 这正是它能证明修复有效的原因 ✓。
const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-archive-bloat.mjs <base-url>"); process.exit(2); }

const call = async (doc, token, tool, args) => {
  const response = await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}),
  });
  return await response.json();
};
const makeDoc = async (doc) => {
  const value = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: 320, height: 240 }),
  })).json();
  return value.token;
};
const fullFill = (i) => ({
  layer_id: "layer_default",
  shape: { type: "rect", x: 0, y: 0, w: 320, h: 240 },
  color: { r: (i * 37) % 255, g: (i * 91) % 255, b: (i * 53) % 255, a: 255 },
});

const stamp = Date.now().toString(36);
const control = `bloat_one_${stamp}`;
const heavy = `bloat_many_${stamp}`;
const controlToken = await makeDoc(control);
const heavyToken = await makeDoc(heavy);

// 对照组：覆盖 1 次。
const one = await call(control, controlToken, "fill_region", fullFill(1));
if (!one || one.ok === false) { console.error("对照组填充失败：" + JSON.stringify(one)); process.exit(2); }
// 被测组：覆盖 12 次（每次颜色不同 ⇒ 每次都产生新的 blob ⇒ 旧的成为"被替代"）。
const turns = 12;
let last = null;
for (let i = 0; i < turns; i++) {
  last = await call(heavy, heavyToken, "fill_region", fullFill(i + 2));
  if (!last || last.ok === false) { console.error(`第 ${i + 1} 次填充失败：` + JSON.stringify(last)); process.exit(2); }
}

const exportIt = async (doc, token, name) => {
  const value = await call(doc, token, "export_project", { path: `${name}.yanshi` });
  return value;
};
const oneExport = await exportIt(control, controlToken, control);
const manyExport = await exportIt(heavy, heavyToken, heavy);
const sizeOf = (v) => (v && (v.bytes || v.size || (v.archive && v.archive.bytes))) || null;
console.log(`  对照组（覆盖 1 次）：${JSON.stringify(oneExport).slice(0, 160)}`);
console.log(`  被测组（覆盖 ${turns} 次）：${JSON.stringify(manyExport).slice(0, 160)}`);
const a = sizeOf(oneExport), b = sizeOf(manyExport);
if (a === null || b === null) {
  console.error("  ✗ 导出没返回体积 ⇒ 判据无法成立（先看导出的返回字段）");
  process.exit(1);
}
const ratio = b / a;
console.log(`  体积：1 次 = ${a} B｜${turns} 次 = ${b} B｜比值 = ${ratio.toFixed(2)}×`);
if (ratio > 1.5) {
  console.error(`  ✗ 归档随覆盖次数膨胀（${turns} 次是 1 次的 ${ratio.toFixed(2)} 倍）⇒ 被替代的历史 blob 未回收`);
  process.exit(1);
}
console.log("  ✓ 归档不随覆盖次数膨胀 ⇒ 被替代的 blob 已被回收");
