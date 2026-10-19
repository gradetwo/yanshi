#!/usr/bin/env node
// **`create_selection` 的三种写法必须**只有一种**有效、另两种**报错**（外部 §96 ✓）**
//   ⇒ **∴ 他们**报**✗**：A 顶层 `kind` **成**了**✗**，B/C **静默无效**✗**
//   ⇒ **∴ 我**实测**✗**：**A/C 报 HTTP 400**✗**，**只有 `shape.kind` 成**✓**
//   ⇒ **∴ 所以**这条判据**固化**实测**✗**：**∴ 一种**成功**✗** ＋ **两种**报错**✗** ⇒ **∴ 不许**静默**✗**
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-create-selection-schema.mjs <base-url>"); process.exit(2); }
const docId = `sel-${process.pid}-${Date.now() % 100000}`;
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: docId, width: 400, height: 400 }),
})).json();
const token = created.token;
if (!token) { console.error("判据自身前提不成立：拿不到 token（不是产品问题）"); process.exit(3); }
const call = async (tool, args) => {
  const res = await fetch(`${base}/api/tools?doc=${docId}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ tool, arguments: args }),
  });
  let body = {};
  try { body = await res.json(); } catch { /* 非 JSON */ }
  return { status: res.status, body };
};

// **★ 先打印**构建/版本号**✗ ★**（外部 §97 的建议 ✓）
//   ⇒ **∴ 他们**说**✗**：**「**在判据脚本里把构建/版本号一并打印，否则这种分歧无法归因」**✗** ✓✓
//   ⇒ **∴ 第 423 轮**✗ 就是**因为**这一点**✗ 而**与他们对不上**✗（**∴ 他们**用 `clip_to_selection` 验到 schema A 真生效 ✓**）
const { execFileSync } = await import("node:child_process");
let build = "unknown";
try {
  build = execFileSync(process.env.YANSHI_SERVE_BIN || "target/release/yanshi-serve", ["--version"], {
    encoding: "utf8", timeout: 10000,
  }).trim();
} catch { /* 拿不到就报 unknown，不假装 */ }
console.log(`  build ⇒ ${build}`);

const pts = [[50, 50], [200, 50], [200, 200]];
const failures = [];

// **① 正确写法**：`shape: {kind: "polygon", points}` ⇒ 成功 ＋ 真的在 list 里 ✓
const ok = await call("create_selection", { selection_id: "sOk", shape: { kind: "polygon", points: pts } });
const listed = (await call("list_selections", {})).body?.selections ?? [];
const ids = listed.map((s) => s.selection_id);
console.log(`  shape.kind ⇒ status=${ok.status}｜ok=${ok.body?.ok}｜list=${JSON.stringify(ids)}`);
if (ok.body?.ok !== true) failures.push(`shape.kind 该成功（实测 status=${ok.status}）✗`);
if (!ids.includes("sOk")) failures.push("shape.kind 说成功了但 list_selections 里没有 ⇒ 静默无效 ✗");

// **② 另两种写法**：必须**报错**，不许静默成功 ✗
for (const [name, args] of [
  ["顶层 kind", { selection_id: "sKind", kind: "polygon", points: pts }],
  ["顶层 type", { selection_id: "sType", type: "polygon", points: pts }],
  ["嵌套两层", { selection_id: "sNest", shape: { shape: { kind: "polygon", points: pts } } }],
]) {
  const got = await call("create_selection", args);
  const isError = got.status >= 400 || got.body?.ok === false;
  const after = (await call("list_selections", {})).body?.selections ?? [];
  const leaked = after.some((s) => s.selection_id === args.selection_id);
  console.log(`  ${name} ⇒ status=${got.status}｜ok=${got.body?.ok}｜建上了=${leaked}`);
  if (!isError) failures.push(`${name} 既没报错 ⇒ 静默接受 ✗（§96 那一类）`);
  if (leaked) failures.push(`${name} 报错了却仍然建上了选区 ⇒ 状态与错误不一致 ✗`);
}

if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ 只有 shape.kind 成功且真的建成；另三种写法都报错、都没建上");
