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
// **★★ 第 1500+ 轮修：判定口径从「总 ignored 数」改成「本条测试在不在 ignored 名单里」★★**
//   **∴ 旧口径的缺陷 ✗**：`yanshi-render` 有 **7 条永久诊断基准**被 `#[ignore]`（永不解禁 ✓）⇒
//     **∴ 总 `ignored` 恒 ≥ 7** ✗ ⇒ **∴ 于是**每次都打印「红线暂时失效」✗ ⇒ **∴ 变成**恒定噪音**✗**
//     ⇒ **∴ 更糟**：**「真的被关掉的红线」**（如 `partial_region_…`）**混在 6 条基准里**✗ ⇒
//       **∴ 哪天真有人把本条也 `#[ignore]` 掉 ✗ ⇒ **∴ 输出**与现在一模一样**✗**（**∴ 没有信号 ✓**）。
//   **∴ 新口径 ✓**：**只解析逐条 `test <全名> ... ignored` 行**✓ ⇒ **∴ 只看**本条测试名**在不在里面 ✓
//     ⇒ **∴ 在** ⇒ 报「红线失效」并点名 ✓；**∴ 不在** ⇒ 报「红线有效」＋**把永久基准的条数与名单打出来**✓。
//   **∴ 拿不到逐条名单时**（输出形态不支持／被截断）**✗ ⇒ **∴ 不猜**✗ ⇒ `⊘ 不可判` ＋ `exit(3)` ✓
//     —— **∴ 因为**「不知道红线有没有被关掉」**本身就是判据跑不了**✗（**∴ 宁可 3，也不报假绿／假警告 ✓**）。
//
// 用法：node scripts/tool-stamp-parity.mjs [次数，默认 10]
//       node scripts/tool-stamp-parity.mjs --self-test   （**离线验解析逻辑 ✓；不跑 cargo ✓**）
// **∴ 默认 10 ✓**（**∵ 实测触发率约 1／10 ✓ ⇒ 跑 3 次**测不出**✗**）。
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
// **★ 数字参数必须**校验** ✗ ★**（第 88 轮 ✓；**CI 的崩溃换来的 ✓）：
//   **∴ 为什么 ✗**：编排（`run-criteria.sh:228` ✓）给 `tool-*` 的 `argv[3]` 是**文档名**✗
//     ⇒ **∴ `Number("crit_…")` ＝ NaN**✗
//       ⇒ **∴ 于是**：**循环**不跑／**等待**异常**✗ ⇒ **∴ 判据**崩溃或**误报 ✓**** ✓✓
//   **∴ 修法**：**不是有限正整数就**用默认值 ✓**** ✓✓
const __rawRuns = Number(process.argv[2]);
const runs = Number.isFinite(__rawRuns) && __rawRuns > 0 ? Math.floor(__rawRuns) : 10;

// **★ 本条红线要守的测试名 ✓ ★**（**∴ 判定只认它 ✓；别的一律视为永久基准 ✓**）
const TARGET_TEST = "incremental_stamp_matches_full_tile_re_render";
const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));

/// 取出**被 ignore 的测试全名**（`test a::b::c ... ignored`，**可能带 `, 理由`** ✓）。
/// **∴ 拿不到就返回空数组 ✓** —— **∴ 空数组是否算「不可判」由调用方**按 `ignored` 总数判断 ✓。
function parseIgnoredTestNames(out) {
  const re = /^\s*test\s+(\S+)\s+\.\.\.\s+ignored\b/gm;
  const names = [];
  let m;
  while ((m = re.exec(out)) !== null) names.push(m[1]);
  return names;
}

/// 单次 `cargo test` 输出的**分类**（**纯函数 ⇒ `--self-test` 可离线跑 ✓**）。
/// 返回 `kind` ∈ `no-test-result`／`failed`／`ignored-target`／`unparsed-ignored`／`ok`。
function classifyOutput(out) {
  const line = (out.split("\n").find((l) => l.startsWith("test result")) ?? "").trim();
  if (!line) return { kind: "no-test-result", line, ignored: 0, names: [], failedCount: 0 };
  const ignored = Number((line.match(/(\d+) ignored/) ?? [0, "0"])[1]);
  const failedCount = Number((line.match(/(\d+) failed/) ?? [0, "0"])[1]);
  const names = parseIgnoredTestNames(out);
  // **∴ 名字可能带模块前缀（`render::tests::…` ✓），也可能不带 ✓** ⇒ **∴ 两种都认 ✓**
  const targetIgnored = names.some((n) => n === TARGET_TEST || n.endsWith("::" + TARGET_TEST));
  if (failedCount > 0 || /test result: FAILED/.test(line)) {
    return { kind: "failed", line, ignored, names, failedCount };
  }
  // **∴ 先判「本条被关掉」✓** —— **∴ 那是**确定**的坏消息，比「名单不全」更该报 ✓**
  if (targetIgnored) return { kind: "ignored-target", line, ignored, names, failedCount };
  // **∴ 名单比 `ignored` 总数少 ⇒ **解析不完整**✗（被截断／形态不认识）⇒ **不可判** ✓
  if (ignored > 0 && names.length < ignored) {
    return { kind: "unparsed-ignored", line, ignored, names, failedCount };
  }
  return { kind: "ok", line, ignored, names, failedCount };
}

/// 去 `criteria-known-red.txt` 里找**本条测试的记录**（**∴ 查到就打印 ✓；查不到不报错 ✓**）。
function knownRedRecords(name) {
  try {
    const text = readFileSync(join(SCRIPT_DIR, "criteria-known-red.txt"), "utf8");
    return text
      .split("\n")
      .map((l, i) => [i + 1, l.trim()])
      .filter(([, l]) => l.includes(name))
      .slice(0, 3)
      .map(([n, l]) => `criteria-known-red.txt:${n}  ${l.length > 160 ? l.slice(0, 160) + "…" : l}`);
  } catch {
    return [];
  }
}

// **★ 自检（**离线 ✓，不跑 cargo ✓**）★**：**∴ 四段人造输出 ⇒ 四种判定 ✓**
//   **=> 变异**：把 `classifyOutput` 的 `targetIgnored` 恒设为 `false` ⇒ 第 2 条红 ✓；
//     把 `unparsed-ignored` 分支删掉 ⇒ 第 3 条红 ✓（**∴ 所以自检能证明它自己会红 ✓**）。
if (process.argv.includes("--self-test")) {
  const head = "     Running unittests src/lib.rs (target/debug/deps/yanshi_render-x)\n\nrunning 298 tests\n";
  const baselines = [
    "render::tests::layer_clone_cost_probe",
    "render::tests::buffer_accessor_cost_probe",
    "render::tests::liquify_inner_math_probe",
    "render::tests::liquify_cost_probe",
    "render::tests::partial_region_render_preserves_untouched_tile_pixels",
    "brush::tests::dab_cache_ab",
    "png::tests::level_time_and_size",
  ];
  const baselineLines = baselines.map((n) => `test ${n} ... ignored`).join("\n") + "\n";
  const okTail = "test result: ok. 291 passed; 0 failed; 7 ignored; 0 measured; 0 filtered out; finished in 3.2s\n";
  const cases = [
    [
      "只有永久基准被 ignore ⇒ 红线有效",
      head + baselineLines + "test render::tests::incremental_stamp_matches_full_tile_re_render ... ok\n\n" + okTail,
      "ok",
    ],
    [
      "目标测试在 ignored 名单里 ⇒ 红线失效",
      head + baselineLines + `test render::tests::${TARGET_TEST} ... ignored, 已知红\n\n` + okTail,
      "ignored-target",
    ],
    [
      "没有逐条 ... ignored 行 ⇒ 不可判",
      head + "test render::tests::incremental_stamp_matches_full_tile_re_render ... ok\n\n" + okTail,
      "unparsed-ignored",
    ],
    [
      "test result: FAILED ⇒ 仍按失败（exit 1），不被吞",
      head + "test render::tests::incremental_stamp_matches_full_tile_re_render ... FAILED\n\n" +
        "test result: FAILED. 290 passed; 1 failed; 7 ignored; 0 measured; 0 filtered out; finished in 3.2s\n",
      "failed",
    ],
  ];
  let bad = 0;
  for (const [name, out, want] of cases) {
    const got = classifyOutput(out);
    const pass = got.kind === want;
    if (!pass) {
      bad += 1;
      console.log(`  ✗ 自测：${name}｜期望 ${want}｜实测 ${got.kind}（ignored=${got.ignored} names=${got.names.length}）`);
    } else {
      console.log(`  ✓ 自测：${name}（${got.kind}｜ignored=${got.ignored}｜名单 ${got.names.length} 条）`);
    }
  }
  // **∴ 附带验一件小事 ✓**：第 2 例必须**点名**目标测试（**∴ 只说"有 ignored"不算 ✓**）
  const named = classifyOutput(cases[1][1]).names.some(
    (n) => n === TARGET_TEST || n.endsWith("::" + TARGET_TEST)
  );
  console.log(`  ${named ? "✓" : "✗"} 自测：红线失效时必须**点名** ${TARGET_TEST}`);
  if (!named) bad += 1;
  console.log(`自测：${cases.length + 1 - bad}/${cases.length + 1} 通过`);
  process.exit(bad === 0 ? 0 : 1);
}

// **★ 判据产物必须落**仓库 `target/`✗，不许回退 `/tmp` ✗ ★**（第 1500+ 轮 ✓）
//   **∴ 为什么 ✗**：本机 `/tmp` 是 **tmpfs ＋ usrquota**（上限约 **4.6G**，其中 **2.6G** 属别的项目 ✓）
//     ⇒ **∴ 实测一次**全量回归**在 **358／447 条**被 `Disk quota exceeded (os error 122)`**打断**✗
//     ⇒ **∴ 于是** `cargo` 在**构建阶段**就退出 ⇒ 一个测试都没跑 ⇒ 判据报 `⊘ 不可判` ✓。
//   **∴ 而** `run-criteria.sh:19` 里的 `export CARGO_TARGET_DIR=…` **只对它的子进程生效** ✗
//     ⇒ **∴ 直接** `node scripts/tool-stamp-parity.mjs` 时**读不到** ✗ ⇒ **∴ 仍然**回退 `/tmp` ✗
//     ⇒ **∴ 分工缝隙**（编排层修了、判据层没修 ✓）⇒ **∴ 所以在这里**也写一份 ✓。
//   **∴ 语义保持 ✗**：**外部显式设了 `CARGO_TARGET_DIR` 就尊重**✓（`??` 不是 `||` ⇒ 空串也尊重 ✓）。
const ROOT = dirname(SCRIPT_DIR);
const env = {
  ...process.env,
  CARGO_TARGET_DIR: process.env.CARGO_TARGET_DIR ?? join(ROOT, "target", "criteria-stamp"),
};
let failed = 0;
const results = [];
for (let i = 1; i <= runs; i++) {
  let out = "";
  let ok = true;
  try {
    out = execFileSync("cargo", ["test", "-p", "yanshi-render", "--lib"], {
      env, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"], timeout: 90_000,
    });
  } catch (e) {
    ok = false;
    out = String((e && e.stdout) || "") + String((e && e.stderr) || "");
  }
  const cls = classifyOutput(out);
  const line = cls.line;
  // **★ 没有 `test result` 行 ⇒ **不可判**✗，不是「增量与整段不一致」✗ ★**
  //   **∴ 实测**：本机 `/tmp` 配额满 ⇒ `cargo` 在**构建阶段**就退出 101 ✗
  //     ⇒ **∴ 一个测试都没跑 ✗** ⇒ **∴ 而旧代码**把这 10 次都算成"10/10 不一致" ✗
  //       ⇒ **∴ 于是**一条**环境问题**被报成**产品缺陷**✗（**同 566 轮那条「数失败数」的毛病 ✓**）。
  if (!line) {
    console.error(`⊘ 不可判：第 ${i} 次没有「test result」行 ⇒ cargo test 没走到测试（多半是构建失败）`);
    console.error(`  最后几行输出：\n${out.trim().split("\n").slice(-8).join("\n")}`);
    process.exit(3);
  }
  // **∴ 文本说 FAILED 也算失败 ✓**（**∴ 不只看进程退出码 ✓** —— **∴ 退出码可能被包装层吞掉 ✗**）
  if (cls.kind === "failed") ok = false;
  // **★ 失败时把**明细**留下 ✗ ★**（第 1000+ 轮 ✓）：**∴ 原来 `out` 已经拿到完整输出 ✓，
  //   却只 `find` 了 `test result` 那一行 ✗ ⇒ **∴ 报"1 failed"却**说不出是哪条 ✗**
  //     ⇒ **∴ 于是这条缺陷**不可诊断**✓（**实测：日志里连一个测试名都没有 ✓**）
  //   ⇒ **∴ 现在截出 `failures:` 之后的段落 ✓，失败时打印 ✓** —— **∴ 只加诊断 ✓，不改判定 ✓**。
  const detail = ok
    ? ""
    : (out.split(/^failures:\s*$/m)[1] ?? "").split("\n").slice(0, 14).join("\n").trim();
  results.push({ i, ok, line, cls, detail });
  if (!ok) failed++;
}
for (const r of results) console.log(`  第 ${r.i} 次: ${r.line}${r.ok ? " ✓" : " ✗"}`);
if (failed > 0) {
  console.error(`❌ ${runs} 次里有 ${failed} 次失败 ⇒ **增量与整段不一致 ⇒ 缓存**撒谎了** ✗**` +
    `（常见根因：参照组只清了 tile 缓存 ⇒ 请改用 \`clear_all_caches()\` ✓）`);
  // **★ 陈旧提示已更正 ✗ ★**（第 1000+ 轮 ✓）：**∴ 原文案说「修复"增量盖章丢内容"后删掉该 \`ignore\`」✗，
  //   而**事实是：该缺陷**第 600 轮已修**✓（`render.rs:5623` 记了根因 ＝ \`blit_rgba8\` 用 \`buffer.width()\`
  //   读 \`size×size\` 源 ⇒ 边缘 tile 行错位 ✓），**且 \`#[ignore]\` 早已移除 ✓** ⇒ **∴ 它现在是**真红线**✓
  //   （**实测：它就在本轮 10/10 全绿的 291 条里 ✓**）。
  //   **∴ 所以这句提示会**误导后人去修一个已修好的东西**✗ ⇒ **∴ 改成事实 ✓**（**∴ 判据的文案也是判据的一部分 ✓**）★
  console.error(
    `  ∴ 附注（第 1000+ 轮更正 ✓）：\`incremental_stamp_matches_full_tile_re_render\` 的缺陷` +
      `（增量盖章丢 tile 内容 ✓）**第 600 轮已修**✓，\`#[ignore]\` 已移除 ✓ ⇒ **它是真红线 ✓**；` +
      `上述"修好后删 ignore"的旧提示**不再适用**✗。`
  );
  // **∴ 把失败测试名列出来 ✓**（**∴ 这是本判据以前缺的那一块 ✓** —— 没有它就无法定性 ✗）
  for (const r of results) {
    if (r.ok || !r.detail) continue;
    console.error(`  第 ${r.i} 次的失败明细：\n${r.detail}`);
  }
  process.exit(1);
}
// **★ 拿不到逐条 ignored 名单 ⇒ **不可判**✗（**∴ 不许猜 ✓**）★**
const unparsed = results.find((r) => r.cls.kind === "unparsed-ignored");
if (unparsed) {
  console.error(
    `⊘ 不可判：第 ${unparsed.i} 次的「test result」说 ${unparsed.cls.ignored} 处 ignored，` +
      `但只解析到 ${unparsed.cls.names.length} 条「test … ... ignored」行 ⇒ 名单不完整`
  );
  console.error(
    "  ⇒ **∴ 不知道本条红线有没有被关掉 ✗ ⇒ **∴ 所以**不报绿、也不报「永久基准」✓（**∴ 宁可 3 ✓**）"
  );
  process.exit(3);
}
// **★ 判定只看**本条测试**在不在 ignored 名单里 ✓ ★**（**∴ 永久诊断基准不算红线失效 ✓**）
const redRuns = results.filter((r) => r.cls.kind === "ignored-target");
const baselines = [...new Set(results.flatMap((r) => r.cls.names))];
if (redRuns.length > 0) {
  // **∴ 红线**真的被关掉了**✗ —— **∴ 必须**点名**✓，**绝不许说"守住" ✗****。
  console.log(`  ⚠️ **红线失效**：本条红线要守的测试被 \`#[ignore]\` 关掉了 ✗（${runs} 次里 ${redRuns.length} 次）`);
  console.log(`     被 ignore 的测试名：${TARGET_TEST}（输出里可能带模块前缀，如 render::tests::…）`);
  const found = knownRedRecords(TARGET_TEST);
  if (found.length > 0) {
    for (const rec of found) console.log(`     known-red 记录：${rec}`);
  } else {
    console.log("     known-red 记录：⊘ 未在 scripts/criteria-known-red.txt 里查到（**∴ 未登记的关闭 ✗**）");
  }
  console.log("  ⚠️ **∴ 本条判据现在只能证明「没崩」，不能证明「增量与整段相同」✗**");
  console.log("  ⚠️ **∴ 恢复**：说明为何 ignore 并修好它 ⇒ **∴ 本条会自动变回真红线 ✓**");
} else {
  const list = baselines.length > 0 ? `：${baselines.join("、")}` : "";
  console.log(
    `  ✓ 红线有效（另有 ${baselines.length} 条永久诊断基准被 ignore，不影响本条${list}）`
  );
  console.log(`  ✓ ${runs} 次全绿 ⇒ **增量与整段逐字节相同 ✓**（红线守住 ✓；${TARGET_TEST} 未在 ignored 名单里）`);
}
