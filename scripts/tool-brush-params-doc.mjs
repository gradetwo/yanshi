#!/usr/bin/env node
// **CI 入口** ✓：`scripts/run-criteria.sh` 按 `tool-*.mjs` 收集判据 ✓
//   ⇒ **∴ 命名必须 `tool-` 开头 ✓、参数必须 `<base> <doc> <token>` ✓**（与其它判据一致 ✓）。
// 真正的生成/校验逻辑在 `brush-params-doc.mjs`（**∴ 它多一个 `write|check` 模式参数 ✓**）。
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-brush-params-doc.mjs <base> <doc> <token>");
  process.exit(3);
}
try {
  execFileSync(process.execPath, [fileURLToPath(new URL("./brush-params-doc.mjs", import.meta.url)), "check", base, doc, token], { stdio: "inherit" });
} catch (error) {
  // **∴ 把子进程的退出码原样带出去** ✓（**∴ 1=判据红、3=跑不了 ✓**）
  process.exit(typeof error.status === "number" ? error.status : 1);
}
