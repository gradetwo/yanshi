#!/usr/bin/env node
// **P2-10：`dodge_burn_setup` 的 `mask_ids` 标注 dodge/burn 角色**（期 2 块 2）
//
// **⇒ 缺陷**：`mask_ids` 只有数组、无角色——调用方只能按 `[dodge, burn]` 顺序猜 ✗；
//   复用路径上数组还是按 id 排过的集合，顺序更不可靠 ✗。
//
// **⇒ 判据**：
//   ① 新建 curves 组 ⇒ `masks.dodge == mask_ids[0]`、`masks.burn == mask_ids[1]` ✓；
//   ② 二次调用（already_exists）⇒ 同一张映射 ✓（复用路径按名认角色）；
//   ③ gray 组 ⇒ `masks` 两格都是 null，且 `mask_ids` 为空 ✓；
//   ④ `mask_ids` 数组本身保留（兼容）✓。
//
// **变异**：`masks` 两格对调 ⇒ ① 红 ✓；复用路径不给 `masks` ⇒ ② 红 ✓。
// **⇒ 退出码**：0 通过｜1 不符｜**3 不可判**
const [base] = process.argv.slice(2);
if (!base) {
  console.error("用法: node scripts/tool-dodge-burn-masks.mjs <base>");
  process.exit(3);
}
const mkDoc = async (doc) => {
  const made = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: 64, height: 48 }),
  })).json();
  if (!made.token) { console.error("  ⊘ 不可判：建不出文档"); process.exit(3); }
  return made.token;
};
const post = async (token, doc, tool, args) => {
  const r = await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args ?? {}),
  });
  return r.json();
};
const fails = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fails.push(m); console.log(`  ✗ ${m}`); };

// 先铺一张底图当减淡加深的源图层（import 一张小图进 L 层）
const setupSource = async (token, doc) => {
  await post(token, doc, "create_layer", { layer_id: "L", name: "L" });
  // 64×48 全灰底：直接用 fill_region 铺色，比拼 base64 省事
  const filled = await post(token, doc, "fill_region", {
    layer_id: "L",
    shape: { type: "rect", x: 0, y: 0, w: 64, h: 48 },
    color: { r: 128, g: 128, b: 128, a: 255 },
  });
  if (filled?.ok !== true) { console.error("  ⊘ 不可判：铺底图失败"); process.exit(3); }
};

const doc = `db_masks_${Date.now()}`;
const token = await mkDoc(doc);
await setupSource(token, doc);
const first = await post(token, doc, "dodge_burn_setup", { layer_id: "L", mode: "curves" });
if (first?.ok !== true) { console.error(`  ⊘ 不可判：dodge_burn_setup 失败 ⇒ ${JSON.stringify(first).slice(0, 200)}`); process.exit(3); }
if (Array.isArray(first.mask_ids) && first.mask_ids.length === 2) ok("④ mask_ids 数组保留（2 张蒙版）");
else bad(`④ mask_ids 应保留 2 张，实测 ${JSON.stringify(first.mask_ids)}`);
if (first.masks?.dodge === first.mask_ids?.[0] && first.masks?.burn === first.mask_ids?.[1]) {
  ok(`① masks.dodge/burn 与 mask_ids[0]/[1] 一致（${first.masks.dodge}／${first.masks.burn}）`);
} else {
  bad(`① masks 必须按角色标注（dodge=${first.masks?.dodge}／burn=${first.masks?.burn}，mask_ids=${JSON.stringify(first.mask_ids)}）`);
}
const second = await post(token, doc, "dodge_burn_setup", { layer_id: "L", mode: "curves" });
if (second?.already_exists !== true) bad(`② 二次调用应 already_exists，实测 ${JSON.stringify(second).slice(0, 160)}`);
else if (JSON.stringify(second.masks) === JSON.stringify(first.masks)) ok("② 复用路径 masks 与新建时一致");
else bad(`② 复用路径 masks 必须与新建时一致（新建 ${JSON.stringify(first.masks)}／复用 ${JSON.stringify(second.masks)}）`);

// gray 组：另起文档（同名组会复用 curves 组）
const grayDoc = `db_masks_gray_${Date.now()}`;
const grayToken = await mkDoc(grayDoc);
await setupSource(grayToken, grayDoc);
const gray = await post(grayToken, grayDoc, "dodge_burn_setup", { layer_id: "L", mode: "gray" });
if (gray?.ok !== true) bad(`③ gray 调用失败 ⇒ ${JSON.stringify(gray).slice(0, 160)}`);
else if (gray.masks?.dodge === null && gray.masks?.burn === null) ok("③ gray 组 masks 两格为 null（没有蒙版）");
else bad(`③ gray 组 masks 两格应为 null，实测 ${JSON.stringify(gray.masks)}`);

if (fails.length > 0) {
  console.log(`\n  ✗ ${fails.length} 处不符（dodge_burn masks 角色）`);
  process.exit(1);
}
console.log("\n  ✓ dodge_burn_setup：masks 按角色标注，新建／复用一致，gray 为 null");
