#!/usr/bin/env node
// **预览白名单的成对守卫** ✓（第 252 轮 ✓）—— 判"元数据原子不重渲"有没有变成**撒谎** ✗。
//
// **背景** ✓：`finish_mutation` 对**不改像素**的原子（保守白名单 ✓）跳过渲染 ✓ 并复用最近一次
// 预览地址 ✓ ⇒ 实测 8K 新建空层 **7759.5 → 1.2 ms ≈ 6000×** ✓（第 240 轮 ✓）。
// **∴ 但"跳过"必须有守卫** ✗ —— 否则某天白名单写宽了 ⇒ **画面不更新却报 ok** ✗（正是"撒谎" ✓）。
//
// **两条判据（成对 ✓）**：
//  ① **白名单内 ⇒ 画面必须**不变****（建空层 ✓）：`render_region` 的 `blob_hash` 前后**相同** ✓；
//  ② **白名单外且真影响画面 ⇒ 画面必须**变****（逐层隐藏 ✓）：**至少有一层**能改变 `blob_hash` ✓。
// **变异** ✗：把"可见性"塞进白名单 ⇒ ② 必红 ✓；把白名单**取消** ⇒ ① 仍需成立（不红，但**性能**判据会红 ✓）。
const [, , BASE, PROJECT] = process.argv;
if (!BASE || !PROJECT) {
  console.error("用法: node tool-preview-whitelist-guard.mjs <base> <project.yanshi>");
  process.exit(2);
}
import { readFileSync } from "node:fs";
let doc = "", token = "";
async function call(tool, args) {
  const r = await fetch(`${BASE}/api/tools/${tool}?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args),
  });
  return await r.json();
}
function check(ok, what, detail) {
  console.log(`  ${ok ? "✓" : "✗"} ${what}（${detail}）`);
  return ok;
}
async function regionHash() {
  const d = await call("render_region", { region: [0, 0, 256, 256], include_image: true });
  return d.blob_hash;
}
const begin = await (await fetch(`${BASE}/api/documents/import?begin=1`, { method: "POST" })).json();
await fetch(`${BASE}/api/documents/import?upload=${begin.upload_id}&offset=0`, {
  method: "POST", headers: { "content-type": "application/octet-stream" },
  body: readFileSync(PROJECT),
});
const fin = await (await fetch(`${BASE}/api/documents/import?upload=${begin.upload_id}&finish=1`, { method: "POST" })).json();
doc = fin.doc_id; token = fin.token;
const st = await call("get_state", {});
const layers = st.layers || [];
const base = await regionHash();
// ① **白名单内 ⇒ 画面不变** ✓
await call("create_layer", { layer_id: "guard_probe" });
const afterCreate = await regionHash();
const a = check(afterCreate === base, "**白名单内（建空层）画面必须不变** ✓",
  afterCreate === base ? "blob 相同 ✓" : "blob 变了 ✗ —— 说明白名单判错了 ✗");
// ② **至少一层可见性能改画面** ✓
let hits = [];
for (const L of layers) {
  const lid = L.layer_id;
  await call("update_layer", { layer_id: lid, patch: { visible: false } });
  if ((await regionHash()) !== base) hits.push(lid);
  await call("update_layer", { layer_id: lid, patch: { visible: true } });
}
const b = check(hits.length >= 1, "**至少一层的可见性能改变画面** ✓",
  hits.length ? `${hits.length}/${layers.length} 层生效 ✓` : "0 层生效 ✗ —— 渲染侧没看 visible ✗");
console.log(`  结论：成对守卫 ${a && b ? "**成立 ✓**" : "**未成立 ✗**"}`);
process.exit(a && b ? 0 : 1);
