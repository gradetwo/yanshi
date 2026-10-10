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
// 用法：node scripts/tool-stamp-parity.mjs [次数，默认 10]
// **∴ 默认 10 ✓**（**∵ 实测触发率约 1／10 ✓ ⇒ 跑 3 次**测不出**✗**）。
import { execFileSync } from "node:child_process";
// **★ 数字参数必须**校验** ✗ ★**（第 88 轮 ✓；**CI 的崩溃换来的 ✓）：
//   **∴ 为什么 ✗**：编排（`run-criteria.sh:228` ✓）给 `tool-*` 的 `argv[3]` 是**文档名**✗
//     ⇒ **∴ `Number("crit_…")` ＝ NaN**✗
//       ⇒ **∴ 于是**：**循环**不跑／**等待**异常**✗ ⇒ **∴ 判据**崩溃或**误报 ✓**** ✓✓
//   **∴ 修法**：**不是有限正整数就**用默认值 ✓**** ✓✓
const __rawRuns = Number(process.argv[2]);
const runs = Number.isFinite(__rawRuns) && __rawRuns > 0 ? Math.floor(__rawRuns) : 10;
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
  // **★ 必须同时看 `ignored` ✓ ★**（第 566 轮 ✓）：**∴ 一个"数失败数"的判据**分不清
  // "通过 ✓"与"没跑 ✗"** ⇒ **∴ 若不看它 ⇒ **∴ 就会把"红线被关闭"说成"红线守住" ✗****。
  const ignored = Number((line.match(/(\d+) ignored/) ?? [0, "0"])[1]);
  results.push({ i, ok, line, ignored });
  if (!ok) failed++;
}
for (const r of results) console.log(`  第 ${r.i} 次: ${r.line}${r.ok ? " ✓" : " ✗"}`);
const skipped = results.reduce((a, r) => a + (r.ignored || 0), 0);
if (failed > 0) {
  console.error(`❌ ${runs} 次里有 ${failed} 次失败 ⇒ **增量与整段不一致 ⇒ 缓存**撒谎了** ✗**` +
    `（常见根因：参照组只清了 tile 缓存 ⇒ 请改用 \`clear_all_caches()\` ✓）`);
  process.exit(1);
}
if (skipped > 0) {
  // **∴ 红线**暂时失效**✗ —— **∴ 必须**明说**✓，**绝不许说"守住" ✗****。
  console.log(`  ⚠️ ${runs} 次里 0 失败 ✓，**但有 ${skipped} 处被 ignore 跳过 ✗**`);
  console.log("  ⚠️ **红线暂时失效**：被测路径因**已知缺陷**被 `ignore`（见 `scripts/criteria-known-red.txt`）");
  console.log("  ⚠️ **∴ 本条判据现在只能证明「没崩」，不能证明「增量与整段相同」✗**");
  console.log("  ⚠️ **∴ 恢复**：修好「增量盖章丢 tile 内容」后删掉该 `ignore` ⇒ **∴ 本条会自动变回真红线 ✓**");
} else {
  console.log(`  ✓ ${runs} 次全绿 ⇒ **增量与整段逐字节相同 ✓**（红线守住 ✓）`);
}
