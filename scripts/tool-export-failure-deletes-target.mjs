#!/usr/bin/env node
// **★ 导出失败删已存在目标文件（期 2 块 3，P2-12）** ✓
//   哨兵文件 + 失败导出（`export_project` 叫不存在的文档；
//   `export_png` 只给 width 不给 height）⇒ 哨兵必须消失；
//   成功导出 ⇒ 文件必须留着（守卫解除）。
//
// **变异点**：把守卫的 `Drop` 删掉 ⇒ 哨兵还在 ⇒ 红；把 `disarm` 删掉 ⇒
// 成功导出也删文件 ⇒ 正例红 ✓。
//
// 用法：node scripts/tool-export-failure-deletes-target.mjs [--rounds 1]

// **前提失败必须 exit(3)**（第 1084 轮）：无服务时 fetch 抛异常 ⇒ uncaught ⇒ 退出码 1 ✗
// ⇒ 按 336 轮那条：条件不满足应 exit(3)（不可判），不是 exit(1)（产品失败）
process.on("uncaughtException", (error) => {
  console.error(`  ⊘ 判据自身出错（不是产品问题）：${String(error).slice(0, 200)}`);
  process.exit(3);
});
import { spawn } from "node:child_process";
import { mkdtempSync, existsSync, statSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const pick = () => {
  const c = [];
  for (const root of [process.env.CARGO_TARGET_DIR, "target"].filter(Boolean)) {
    for (const k of ["release", "debug"]) {
      const p = join(root, k, "yanshi-serve");
      if (existsSync(p)) c.push(p);
    }
  }
  c.sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs);
  return c[0];
};
const BIN = pick();
console.log(`  二进制：${BIN}（mtime ${statSync(BIN).mtime.toISOString()}）`);
const port = 18700 + Math.floor(Math.random() * 120);
const ROOT = trackTemp(mkdtempSync(join(tmpdir(), "expfaildel-")));
const base = `http://127.0.0.1:${port}`;
const proc = spawn(BIN, ["--root", ROOT, "--bind", `127.0.0.1:${port}`], { stdio: "ignore" });
process.on("exit", () => { try { proc.kill(); } catch { /* 已退出 */ } });
let failed = 0;
const check = (ok, name, detail) => {
  console.log(`  ${ok ? "✓" : "✗"} **${name}**${detail ? `（${detail}）` : ""}`);
  if (!ok) failed += 1;
};
try {
  for (let i = 0; i < 80; i += 1) { try { if ((await fetch(`${base}/health`)).ok) break; } catch {} await sleep(250); }
  const doc = "expfaildel_doc";
  const token = (await (await fetch(`${base}/api/documents`, { method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: 64, height: 64 }) })).json()).token;
  const call = async (tool, args) => (await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) })).json();
  await call("create_layer", { layer_id: "L", name: "L" });
  // ① `export_project` 叫一个不存在的文档 ⇒ 失败，哨兵必须被删。
  const targetA = join(ROOT, "stale_a.yanshi");
  writeFileSync(targetA, "old export");
  const failedA = await call("export_project", { path: targetA, doc_id: "doc_nope" });
  check(failedA.ok === false, "export_project 叫不存在的文档必须失败", JSON.stringify(failedA).slice(0, 120));
  check(!existsSync(targetA), "失败时目标路径不许留下旧文件（export_project）", `目标 ${targetA}`);
  // ② `export_png` 参数非法（只给 width 不给 height）⇒ 失败，哨兵必须被删。
  const targetB = join(ROOT, "stale_b.png");
  writeFileSync(targetB, "old png");
  const failedB = await call("export_png", { path: targetB, width: 32 });
  check(failedB.ok === false, "export_png 参数非法必须失败", JSON.stringify(failedB).slice(0, 120));
  check(!existsSync(targetB), "失败时目标路径不许留下旧文件（export_png）", `目标 ${targetB}`);
  // ③ 正例：成功导出 ⇒ 文件必须留着（守卫解除）。
  const targetC = join(ROOT, "good.yanshi");
  const exported = await call("export_project", { path: targetC, include_bitmaps: true });
  check(exported.ok === true, "成功导出必须 ok:true", JSON.stringify(exported).slice(0, 120));
  check(existsSync(targetC), "成功导出后文件必须在", `目标 ${targetC}`);
} catch (error) {
  console.error(`  ✗ 本跑没有结论：${String((error && error.message) || error).slice(0, 200)}`);
  proc.kill("SIGKILL");
  process.exit(2);
}
proc.kill("SIGKILL");
if (failed === 0) {
  rmSync(ROOT, { recursive: true, force: true });
} else {
  console.log(`  （现场保留：${ROOT}）`);
}
console.log("");
console.log(failed === 0 ? "  结论：导出失败删目标文件 ✓" : `  结论：导出失败仍留旧文件 ✗（${failed} 条）`);
process.exit(failed === 0 ? 0 : 1);
