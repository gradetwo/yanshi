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
//   ② **正向**：正常绝对路径 ⇒ 必须**成功**并返回 `blob_hash`
//      ⇒ 防误拒（防护不能把正常用法挡掉）
//
// **∴ 变异**：把 `..` 检查删掉 ⇒ ① 必红 ✓
//
// **∴ 已知剩余风险**（本判据**不**覆盖，因为它是「待修」不是「已修」）：
//   **绝对路径**仍能读工作区外的文件（② 就是证据）⇒ 要彻底堵住需要
//   `--root` 的 canonicalize + 前缀校验。这里**不**把「绝对路径也被拒」写成断言，
//   因为那会让判据红而修复还没做（**判据不许承诺实现没做到的事**）✓
import { readFileSync, existsSync } from "node:fs";

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

// ② 正向：一个确定存在的文件必须能读（用 /etc/hostname；不存在就换 /etc/os-release）
const candidates = ["/etc/hostname", "/etc/os-release", "/proc/self/cmdline"];
let okFile = null;
for (const p of candidates) {
  if (existsSync(p)) { okFile = p; break; }
}
check(okFile !== null, `②a 找一个确定存在的文件（用于正向断言）⇒ ${okFile}`);
if (okFile) {
  const good = await call("put_blob", { path: okFile });
  check(good.ok === true, `②b 正常路径必须成功（不误拒）⇒ ok=${good.ok}｜${JSON.stringify(good.context?.detail ?? "").slice(0, 80)}`);
  check(typeof good.blob_hash === "string" && good.blob_hash.startsWith("sha256:"),
    `②c 必须返回 sha256: 开头的 blob_hash ⇒ ${String(good.blob_hash).slice(0, 24)}…`);
}

if (fails.length) {
  console.log(`\n  ✗ ${fails.length} 处不符（put_blob 的路径安全）`);
  process.exit(1);
}
console.log("\n  ✓ put_blob 拒绝路径穿越，且不误拒正常路径");
