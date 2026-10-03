#!/usr/bin/env node
// **导出路径安全**判据（第三方代码审计 P0 第 1 条：`export_png`/`export_project` 原先写客户端给的任意路径）。
//
// 判据（**正反都能红** ✓）：
//   * 绝对路径 ⇒ **必须被拒** ✗；
//   * 含 `..` 的相对路径 ⇒ **必须被拒** ✗；
//   * 合法的相对路径 ⇒ **必须成功** ✓（否则就是把功能一起关掉了 ✗）。
// 用法：node scripts/tool-export-path-safety.mjs <server-base> [exportDir]
import { existsSync } from "node:fs";
const base = process.argv[2];
const exportDir = process.argv[3] || "exports";
if (!base) { console.error("用法: node scripts/tool-export-path-safety.mjs <server-base> [exportDir]"); process.exit(2); }
const post = async (path, payload) => (await fetch(base + path, {
  method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(payload || {}),
})).json();
const token = (await post("/api/documents", { doc_id: "safepath", width: 120, height: 90 })).token;
if (!token) { console.error("拿不到 token ⇒ 判据无法运行（不是通过）"); process.exit(1); }
const tool = (name, args) => post("/api/tools?doc=safepath&token=" + token, { tool: name, arguments: args });
let bad = 0;
const check = (label, ok, detail) => { console.log((ok ? "  ✓ " : "  ✗ ") + label + (detail ? "｜" + detail : "")); if (!ok) bad += 1; };
const detailOf = (reply) => String((reply.context || {}).detail || "");
for (const [label, path] of [["目录外的绝对路径", "/etc/yanshi-should-not-exist.png"],
                             ["路径穿越", "../../yanshi-should-not-exist.png"]]) {
  const reply = await tool("export_png", { path });
  check(`export_png ${label}「${path}」必须被拒`, reply.ok !== true,
    reply.ok === true ? "竟然成功了 ✗" : detailOf(reply).slice(0, 70));
}
const good = await tool("export_png", { path: "safety-ok.png" });
check("export_png 合法相对路径必须成功", good.ok === true, detailOf(good).slice(0, 70));
const project = await tool("export_project", { path: "safety-ok.yanshi" });
check("export_project 合法相对路径必须成功", project.ok === true, detailOf(project).slice(0, 70));
for (const file of ["safety-ok.png", "safety-ok.yanshi"]) {
  const full = exportDir.replace(/\/$/, "") + "/" + file;
  console.log(`     （导出目录内 ${full}：${existsSync(full) ? "存在 ✓" : "不存在 ✗"}）`);
}
console.log(bad ? `  结论：${bad} 条不成立 ✗` : "  结论：导出路径安全成立 ✓");
process.exit(bad ? 1 : 0);
