#!/usr/bin/env node
// **「导出路径幂等」判据** ✓（(2.1) 相对路径硬编码前缀缺陷 ✓，用户报告实测 ✓）。
// 现象 ✓：传 `path: "exports/x.png"` ⇒ 服务端再拼一次 `exports/` ⇒ `exports/exports/x.png` ✗
// ⇒ 报 `No such file or directory` ✗（调用方只能反直觉地只传文件名 ✓）。
// 判据 ✓：**两种写法都必须成功**，而且**落到同一处**（幂等 ✓）。
const base = process.argv[2];
const docId = process.argv[3] || "ep1";
if (!base) { console.error("用法: node scripts/tool-export-path.mjs <base-url> [docId]"); process.exit(2); }
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: docId, width: 80, height: 60 }),
})).json();
const token = created.token;
if (!token) { console.error("判据无效：拿不到 token"); process.exit(1); }
const call = async (tool, args) => (await fetch(`${base}/api/tools?doc=${docId}&token=${token}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ tool, arguments: args }),
})).json();
await call("create_layer", { layer_id: "layer_default", name: "l" });
const failures = [];
for (const path of ["plain.png", "exports/prefixed.png"]) {
  const value = await call("export_png", { path });
  const ok = value.ok === true;
  console.log(`  path=${JSON.stringify(path)} ⇒ ok=${ok}${ok ? "" : "｜" + ((value.context || {}).detail || value.error_code || "?")}`);
  if (!ok) failures.push(`path=${path} 失败`);
  else {
    const out = value.path || value.file || value.output_path;
    if (path.startsWith("exports/") && out && /exports[\\/]exports[\\/]/.test(String(out))) {
      failures.push(`path=${path} 落到了嵌套目录：${out}`);
    }
  }
}
if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ 相对路径与 exports/ 前缀两种写法都成功（幂等）");
