#!/usr/bin/env node
// **retouch 三兄弟在画布外必须报错** ✓（第 1000+ 轮 ✓；YANSHI_ISSUES §22-24 的 P0 #3）
//
// **∴ 为什么这条判据必须存在 ✓**：`smudge`／`clone_stamp`／`heal_stamp` 是**提交 atom**✗，
//   不是即时渲染 ⇒ **∴ 它们曾经在画布外**静默返回 `ok:true`**✗**（**∴ "什么都没做，但看起来做了"✗**）
//   ⇒ **∴ 报告的人为此查了 4 轮 ✗**（E372：写意块面恒 840 笔、输出字节完全相同 ✓）。
//
// **∴ 构型 ✓**：画布 200×200，点在 (900,900) —— ****根本不在画布上**✗**（**∴ 不是"在边缘"✓**）
//   ⇒ **∴ 而修复后应当 `ok:false` ✓**，且文案要说清"不会有任何像素被揉到"✓
//
// **∴ 反向也要判 ✓**：画布内（100,100）**必须 `ok:true`**✗（**∴ 否则就是修过头 ✓**）
import { readFileSync } from "node:fs";

const [base] = process.argv.slice(2);
if (!base) { console.error("用法: node scripts/tool-retouch-outside.mjs <base-url>"); process.exit(3); }

const boot = await fetch(`${base}/api/documents`, {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc: `retouch_oob_${Date.now()}`, width: 200, height: 200 }),
});
const bootBody = await boot.json().catch(() => ({}));
const token = bootBody.token;
if (!token) { console.log(`  ✗ 建文档失败：${JSON.stringify(bootBody).slice(0, 140)}`); process.exit(3); }
const doc = bootBody.doc ?? bootBody.doc_id ?? "retouch_oob";

const call = async (name, args) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
  const reply = await fetch(url, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args) });
  return await reply.json();
};
await call("create_layer", { layer_id: "ro_L", name: "ro_L" });

const OUTSIDE = [[900, 900, 1], [980, 980, 1]];
const INSIDE = [[80, 100, 1], [120, 100, 1]];
const CASES = [
  ["smudge", {}],
  ["clone_stamp", { source_offset: [-100, -50] }],
  ["heal_stamp", { source_offset: [-100, -50] }],
];

let bad = 0;
for (const [name, extra] of CASES) {
  const out = await call(name, { layer_id: "ro_L", points: OUTSIDE, ...extra });
  const detail = String((out.context ?? {}).detail ?? "");
  const refused = out.ok === false;
  console.log(`  ${refused ? "✓" : "✗"} ${name} 画布外 ⇒ ok=${out.ok}${refused ? `｜${detail.slice(0, 80)}` : "（**必须 false** ✗）"}`);
  if (!refused) bad += 1;

  const ins = await call(name, { layer_id: "ro_L", points: INSIDE, ...extra });
  const accepted = ins.ok === true;
  console.log(`  ${accepted ? "✓" : "✗"} ${name} 画布内 ⇒ ok=${ins.ok}${accepted ? "" : "（**不能回归** ✗）"}`);
  if (!accepted) bad += 1;
}

if (bad) { console.log(`  ✗ ${bad} 处不符（画布外必须报错、画布内必须通过）`); process.exit(1); }
console.log("  ✓ 三种 retouch 在画布外都报错、在画布内都通过");
