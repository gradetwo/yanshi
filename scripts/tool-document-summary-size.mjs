#!/usr/bin/env node
// **`GET /api/documents/<id>` 必须报出文档尺寸** ✓
//   ⇒ **∴ 第 392 轮实测**✗**：它返回的 keys 只有 `document`／`ok`**✗** ⇒ **∴ `width=None`**✗**
//   ⇒ **∴ 而** `get_document` **工具**✗ 返回真的 `width`／`height`**✗** ⇒ **∴ 两者**不同**✗** ✓
//   ⇒ **∴ 所以**：**∴ 调用方**读 `GET`**✗ 拿不到尺寸**✗** ⇒ **∴ 只能**再调一个工具**✗** ✓✓
const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-document-summary-size.mjs <base-url>"); process.exit(2); }
const failures = [];
const docId = `sum-${process.pid}-${Date.now() % 100000}`;
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: docId, width: 400, height: 300 }),
})).json();
const token = created.token;
if (!token) { console.error("判据自身前提不成立：拿不到 token（不是产品问题）"); process.exit(3); }
const call = async (tool, args) => (await fetch(`${base}/api/tools?doc=${docId}&token=${token}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ tool, arguments: args }),
})).json();

// **① `GET` 的摘要必须带尺寸** ✓
const summary = await (await fetch(`${base}/api/documents/${docId}?token=${token}`)).json();
console.log(`  GET 摘要 ⇒ keys=${JSON.stringify(Object.keys(summary))}｜width=${summary.width}｜height=${summary.height}`);
if (summary.width !== 400 || summary.height !== 300) {
  failures.push(`GET 摘要的尺寸不对（实测 ${summary.width}×${summary.height}，期望 400×300）⇒ 调用方拿不到尺寸 ✗`);
}

// **② 与 `get_document` 工具必须一致** ✓（**∴ 两个入口说同一个事实 ✓**）
const doc = await call("get_document", {});
console.log(`  get_document ⇒ ${doc.width}×${doc.height}`);
if (doc.width !== 400 || doc.height !== 300) failures.push(`get_document 的尺寸不对（实测 ${doc.width}×${doc.height}）✗`);
if (summary.width !== doc.width || summary.height !== doc.height) {
  failures.push("GET 摘要与 get_document 的尺寸不一致 ⇒ 两个入口说不同的数 ✗");
}

if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ GET 摘要带尺寸且与 get_document 一致（400×300）");
