#!/usr/bin/env node
// **★ 红线：增量盖章与整段盖章必须**逐字节相同** ✓ ★**（第 502／503 轮 ✓）
//
// **它判什么** ✓：**`render::tests::incremental_stamp_matches_full_tile_re_render` **连跑 N 次**✓**
//   —— **∴ 因为它是**偶发**✗**（**修复前 1／10 ✓；修复后 10／10 ✓**）⇒ **∴ 跑一次不够 ✓**。
//
// **为什么这是红线** ✗：**它测的是**"缓存不许撒谎"**✓** ——
//   **∴ 若 below 缓存被复用在**盖章之后**✗ ⇒ **∴ 参照组与增量组**逐字节不同**✗ ⇒ **∴ 用户看到错像素 ✓**。
//   **∴ 实测根因** ✓（**第 487／502 轮 ✓**）：**测试的参照组用 `cache_mut().clear()` ✗
//     （**只清 tile 缓存 ⇒ 清不到 below ✓**）⇒ **∴ 它继承了**盖章前**的下方合成 ✓** ⇒ **∴ 差 18.75% ✓**。
//
// **变异** ✗：**把测试改回 `cache_mut().clear()`** ⇒ **∴ N 次里应有失败 ⇒ 必红 ✓**。
//
// 用法：node scripts/tool-stamp-parity.mjs [次数，默认 3]
import { execFileSync } from "node:child_process";
const runs = Number(process.argv[2] ?? 3);
const env = { ...process.env, CARGO_TARGET_DIR: process.env.CARGO_TARGET_DIR ?? "/tmp/yt4b" };
let failed = 0;
const results = [];
for (let i = 1; i <= runs; i++) {
  let out = "";
  let ok = true;
  try {
    out = execFileSync("cargo", ["test", "-p", "yanshi-render", "--lib"], {
      env, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"], timeout: 600_000,
    });
  } catch (e) {
    ok = false;
    out = String((e && e.stdout) || "") + String((e && e.stderr) || "");
  }
  const line = (out.split("\n").find((l) => l.startsWith("test result")) ?? "").trim();
  results.push({ i, ok, line });
  if (!ok) failed++;
}
for (const r of results) console.log(`  第 ${r.i} 次: ${r.line}${r.ok ? " ✓" : " ✗"}`);
if (failed > 0) {
  console.error(`❌ ${runs} 次里有 ${failed} 次失败 ⇒ **增量与整段不一致 ⇒ 缓存**撒谎了** ✗**` +
    `（常见根因：参照组只清了 tile 缓存 ⇒ 请改用 \`clear_all_caches()\` ✓）`);
  process.exit(1);
}
console.log(`  ✓ ${runs} 次全绿 ⇒ **增量与整段逐字节相同 ✓**（红线守住 ✓）`);
