#!/usr/bin/env node
// **部署脚本不许再用 `npx --yes` 现取工具**（上轮 E-12 ／ 05 复审 L-4 ✓）
//
// **⇒ 背景**：`deploy-pwa.sh` 曾经用 `npx --yes wrangler` ✗ ⇒ **绕过 lockfile** ✓
//   每次部署现取最新（供应链面 ✓ 版本不可复现 ✗），而 `npm run pwa:deploy` 走
//   仓库锁定版本 ⇒ 两条部署路**各有各的 wrangler** ✗。现在统一到 `pwa-wrangler.mjs` ✓。
//
// **⇒ 判什么**：**活代码行**（行首不是 `#` ✗ 注释不算 ✓）里不许出现 `npx --yes` ✓
//   —— 注释里提它是**正当的**（说明为什么不用 ✓）。
// **⇒ 为什么守 `scripts/` 全体**：同类写法会**漂回**别的脚本 ✗（本轮就是 deploy-pwa.sh ✗）。
//   **⇒ 已知残余** ✗：`.github/workflows/pwa.yml` 的三处 `npx --yes` 受 **L-6「暂不改工作流」冻结** ✓
//     ⇒ 本判据**只扫 scripts/**（不碰工作流，避免自己变成"红在工作流"的噪音 ✓）；解冻后扩到 workflows/ ✓。
//
// **⇒ 变异**：把 `deploy-pwa.sh` 任一 `node scripts/pwa-wrangler.mjs deploy` 换回
//   `npx --yes wrangler deploy` ⇒ 本判据红（exit 1）✓；复原 ⇒ 绿 ✓。
import { readdirSync, readFileSync } from "node:fs";

const offenders = [];
let scanned = 0;
for (const f of readdirSync("scripts").sort()) {
  if (!/\.(sh|mjs|js|bash)$/.test(f)) continue;
  scanned++;
  const text = readFileSync(`scripts/${f}`, "utf8");
  text.split("\n").forEach((line, i) => {
    if (/^\s*#/.test(line)) return;            // 注释行不算 ✗→✓
    if (/^\s*\/\//.test(line)) return;          // JS 单行注释也不算
    if (line.includes("npx " + "--yes")) offenders.push(`${f}:${i + 1}`);  // **拆成两段**：否则本判据会**抓到自己的这一行**（第 301 轮「检查对象里不得有该检查本身的文字」✓）
  });
}
if (offenders.length) {
  console.log(`  ✗ ${offenders.length} 处活代码用 ${"npx " + "--yes"} 现取工具：${offenders.join("、")}`);
  process.exit(1);
}
console.log(`✓ deploy-tool-pin：扫了 ${scanned} 个脚本，活代码里没有 ${"npx " + "--yes"}（版本随 lockfile ✓）`);
