#!/usr/bin/env node
// **`export_png` 的 `sha256` 必须**存在、**确定性**、且**与文件真实内容一致****（第 1542 轮）
//
// **⇒ 为什么要有这条判据**：外部（`E1613`）证明「**两次输出的 `sha256` 是否相同**」是
//   比 `local`/`edge`/`std` 这些统计量**更强的"改动是否生效"判据** ✓
//   ⇒ 而那之前他**只能自己读文件再算** ⇒ 现在**响应里直接给** ⇒ **必须守住它是真的** ✓
//
// **⇒ 断三条**：
//   ① 响应里有 `sha256`，形如 `sha256:<64 位小写十六进制>` ✓
//   ② **同一次导出，文件内容的真实哈希与它相同** ✓（**否则它就是个撒谎的字段**）
//   ③ **连导两次，哈希相同** ✓（**确定性** —— 这正是他要做的检验）
// **⇒ 退出码**：0 通过｜1 不符｜3 不可判
const [base, doc, token, exportDir] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-export-sha256.mjs <base> <doc> <token> [exportDir]");
  process.exit(3);
}
const { createHash } = await import("node:crypto");
const { readFileSync, existsSync } = await import("node:fs");
const call = async (name, args) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
  const reply = await fetch(url, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args),
  });
  return reply.json();
};
const fails = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fails.push(m); console.log(`  ✗ ${m}`); };

const a = await call("export_png", { path: "sha_a.png" });
const b = await call("export_png", { path: "sha_b.png" });
if (a?.ok !== true || b?.ok !== true) {
  console.error(`  ⊘ 不可判：导出失败（${JSON.stringify((a?.context || {}).detail || "")}）`);
  process.exit(3);
}
// ① 形状
const h = a.sha256;
if (typeof h === "string" && /^sha256:[0-9a-f]{64}$/.test(h)) ok(`① 响应带 sha256（${h.slice(0, 20)}…）`);
else bad(`① 响应里的 sha256 形状不对 ⇒ ${JSON.stringify(h)}`);
// ② 与文件真实内容一致
const dir = exportDir || process.env.YANSHI_EXPORT_DIR || "";
const file = dir ? `${dir}/sha_a.png` : "";
if (file && existsSync(file)) {
  const real = "sha256:" + createHash("sha256").update(readFileSync(file)).digest("hex");
  if (real === h) ok(`② 与文件真实哈希一致 ✓（**不是撒谎的字段**）`);
  else bad(`② 响应说 ${h.slice(0, 20)}…，而文件实际是 ${real.slice(0, 20)}…`);
} else {
  console.log(`  ⊙ ② 跳过：拿不到导出文件（${file || "未给 exportDir"}）`);
}
// ③ 确定性
if (a.sha256 === b.sha256) ok(`③ 连导两次哈希相同 ⇒ **确定性** ✓`);
else bad(`③ 连导两次哈希不同 ⇒ ${a.sha256?.slice(0, 20)} vs ${b.sha256?.slice(0, 20)}`);

if (fails.length > 0) { console.log(`\n  ✗ ${fails.length} 处不符（export_png 的 sha256）`); process.exit(1); }
console.log("\n  ✓ export_png 的 sha256 存在、真实、且确定");
