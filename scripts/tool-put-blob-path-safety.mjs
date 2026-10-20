#!/usr/bin/env node
// **`put_blob` 的路径安全**（第 1231 轮；我自己引入的洞，必须有判据守着）
//
// **∴ 背景**：为了让外部用户能「把本地 PNG 变成 blob_hash」我加了 `put_blob`，
//   而它第一版直接 `std::fs::read(args["path"])` ⇒ **任意文件读** ✗
//   ⇒ 第 1230 轮堵掉了**更常见的那一类**：含 `..` 的路径穿越
//
// **∴ 这条判据守什么**（两条断言，各验一个方向）：
//   ① **反向**：`path` 含 `..` ⇒ 必须**明确拒绝**（`ok:false` + 错误说「..」）
//      ⇒ 否则调用方能读工作区外的任意文件（`../../etc/passwd`）
//   ② **正向**：**工作区内**的文件 ⇒ 必须**成功**并返回 `blob_hash`（防误拒）
//   ③ **反向**：**工作区外**的文件 ⇒ 必须**被拒**（第 1232 轮加的 root 沙箱）
//
// **∴ 变异**：把 `..` 检查删掉 ⇒ ① 必红 ✓
//
// **∴ 第 1232 轮**：root 沙箱（canonicalize + 前缀校验）已做 ⇒ 所以断言 ③ 现在**成立** ✓
//   （上一轮它还不成立，那时我把这条风险**只写进注释**、不写进断言）
import { writeFileSync, rmSync } from "node:fs";
import { join, dirname } from "node:path";

const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-put-blob-path-safety.mjs <base> <doc> <token>");
  process.exit(3);
}

const call = async (name, args) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
  const reply = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(args),
    signal: AbortSignal.timeout(20_000),
  });
  return await reply.json();
};

const fails = [];
const check = (ok, msg) => { if (!ok) fails.push(msg); console.log(`  ${ok ? "✓" : "✗"} ${msg}`); };

// ① 反向：含 .. 的路径必须被拒
const traversal = await call("put_blob", { path: "/tmp/../etc/passwd" });
const detail = String(traversal.context?.detail ?? "");
check(traversal.ok === false, `①a 含 .. 的路径必须被拒 ⇒ ok=${traversal.ok}`);
check(/\.\./.test(detail), `①b 错误信息必须提到 ..（可诊断）⇒ ${JSON.stringify(detail).slice(0, 80)}`);

// ② 正向：**工作区内**的文件必须能读（root 从 YANSHI_EXPORT_DIR 反推）
const exportDir = process.env.YANSHI_EXPORT_DIR;
if (!exportDir) {
  console.error("  ⊘ 不可判：没设 YANSHI_EXPORT_DIR（判据要靠它反推工作区 root）");
  process.exit(3);
}
const root = dirname(exportDir);
const insidePath = join(root, "putblob_probe.txt");
try {
  writeFileSync(insidePath, "put_blob probe\n");
} catch (e) {
  console.error(`  ⊘ 不可判：写不进工作区（${insidePath}）：${e.message}`);
  process.exit(3);
}
const good = await call("put_blob", { path: insidePath });
check(good.ok === true, `②a 工作区内的文件必须成功（不误拒）⇒ ok=${good.ok}｜${JSON.stringify(good.context?.detail ?? "").slice(0, 80)}`);
check(typeof good.blob_hash === "string" && good.blob_hash.startsWith("sha256:"),
  `②b 必须返回 sha256: 开头的 blob_hash ⇒ ${String(good.blob_hash).slice(0, 24)}…`);

// ③ 反向：**工作区外**的文件必须被拒（第 1232 轮加的 root 沙箱）
const outside = await call("put_blob", { path: "/etc/hostname" });
const od = String(outside.context?.detail ?? "");
check(outside.ok === false, `③a 工作区外的文件必须被拒 ⇒ ok=${outside.ok}`);
check(/工作区根目录之内|工作区外/.test(od), `③b 错误必须说「必须落在工作区根目录之内」⇒ ${JSON.stringify(od).slice(0, 90)}`);
try { rmSync(insidePath); } catch { /* 清理失败不影响判据 */ }

if (fails.length) {
  console.log(`\n  ✗ ${fails.length} 处不符（put_blob 的路径安全）`);
  process.exit(1);
}
console.log("\n  ✓ put_blob 拒绝路径穿越，且不误拒正常路径");
