#!/usr/bin/env node
// 判据：发布包必须**真的把笔刷装进去**，而且装得**完整**。
//
// **为什么需要它**（外部回归报告 ✓）：静态发布包的 `share/yanshi/` 里**没有 `brushes/`** ✗
// ⇒ 随包的 `brush_stroke` 报 `reference_not_found` ✗、用户手里**一支可用的笔刷都没有** ✗；
// 而打包脚本**照样报成功** ✓ ⇒ 这种"产物少了东西却没人知道"的包，正是本条要拦的 ✗。
//
// **运行时到底去哪儿找笔刷**（证据 ✓，位置变了这条判据就得跟着改 ✓）：
//   * `crates/yanshi-server/src/service.rs` 的 `resolve_assets_dir`：候选里有
//     `<exe>/../share/yanshi` ✓ —— 正是发布包 `bin/` + `share/yanshi/` 的布局 ✓；
//   * `crates/yanshi-http/src/server.rs`：`root.join("brushes").join(file)` ✓；
//   * 打包脚本给服务端的资产根目录：`--assets-dir "$here/share/yanshi"` ✓。
//   ⇒ **笔刷必须落在 `<stage>/share/yanshi/brushes/`** ✓；落在别处等于没装 ✗。
//
// **判据（两条都能红 ✓）**：
//   ① 在**真仓库**上跑**真正的组装步骤** ✓（`scripts/stage-asset-kinds.sh` ——
//      打包脚本 `scripts/package-release.sh` 调的就是它 ✓，所以这不是复制品 ✗）
//      ⇒ `share/yanshi/brushes/` 必须存在 ✓、`.myb` 名字集合必须与
//      `assets/brushes/*.myb` **逐个相同** ✓、数量必须 > 0 ✓；
//   ② **反向控制** ✓：源目录 `assets/brushes` 不存在或为空时，组装必须**明确失败** ✗ ——
//      绝不能"静默跳过、打出一个没有任何笔刷的包、还退 0 说成功" ✗。
//
// **变异证明（做过 ✓）**：
//   * 把 `stage-asset-kinds.sh` 默认清单里的 `brushes` 去掉 ⇒ ① 红（目录不存在 ✓）；
//   * 把"源目录不存在"的处理改回静默 `continue` ⇒ ② 红（退出码 0 ✓）。
//
// 用法：`node scripts/tool-package-brushes.mjs`（与 `tool-*.mjs` 的调用约定一致 ✓，
// 本判据**不需要**服务端 / 浏览器 ⇒ 多余的位置参数忽略 ✓）。

import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// **用判据自身的位置解析仓库根** ✓（不许用相对路径 ✗ —— runner 可能在别处跑 ✓，
// 那样"变异后仍绿"就可能是没读到我变异的那个文件 ✓，而不是判据真的能红 ✗）。
const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const STAGER = join(REPO_ROOT, "scripts", "stage-asset-kinds.sh");
const SOURCE_BRUSHES = join(REPO_ROOT, "assets", "brushes");
const BRUSH_RELATIVE = join("share", "yanshi", "brushes");

const failures = [];
const skip = (name) => name === "._" || name.startsWith("._") || name === ".DS_Store";

/** 源目录里"应当进包"的文件名 ✓（跳过 macOS 平台垃圾 ✓，与打包脚本同一条规矩 ✓）。 */
function sourceNames(dir) {
  return readdirSync(dir).filter((name) => !skip(name)).sort();
}

/** 组装出来的文件名 ✓。 */
function stagedNames(dir) {
  return readdirSync(dir).sort();
}

function runStager(repo, stage) {
  const result = spawnSync("bash", [STAGER, repo, stage], { encoding: "utf8", timeout: 120_000 });
  return {
    code: result.status,
    out: `${result.stdout || ""}${result.stderr || ""}`,
    error: result.error,
  };
}

function assert(condition, message) {
  if (!condition) failures.push(message);
}

// ---------------------------------------------------------------- ① 真仓库上组装
if (!existsSync(STAGER)) {
  failures.push(`找不到组装脚本：${STAGER}（判据与打包脚本共用它；缺了就没法验证）`);
} else if (!existsSync(SOURCE_BRUSHES) || !statSync(SOURCE_BRUSHES).isDirectory()) {
  failures.push(`仓库里没有笔刷源目录：${SOURCE_BRUSHES}（判据的前置不成立）`);
} else {
  const root = mkdtempSync(join(tmpdir(), "yanshi_pkgbrush_"));
  const stage = join(root, "stage");
  try {
    const expected = sourceNames(SOURCE_BRUSHES);
    const expectedMyb = expected.filter((name) => name.endsWith(".myb"));
    assert(expectedMyb.length > 0, `仓库的资产/笔刷目录里一个 .myb 都没有（${SOURCE_BRUSHES}）`);

    const run = runStager(REPO_ROOT, stage);
    assert(run.error === undefined, `跑组装脚本失败：${run.error}`);
    assert(run.code === 0, `组装脚本退出码应为 0，实际 ${run.code}；输出：${run.out.trim()}`);

    const stagedDir = join(stage, BRUSH_RELATIVE);
    if (!existsSync(stagedDir)) {
      failures.push(
        `包内没有 ${BRUSH_RELATIVE}/ ⇒ 运行期 root.join("brushes") 会 404/找不到笔刷（用户会看到 reference_not_found）`,
      );
    } else {
      const got = stagedNames(stagedDir);
      const gotMyb = got.filter((name) => name.endsWith(".myb"));
      assert(
        gotMyb.length > 0,
        `包内 ${BRUSH_RELATIVE}/ 存在但一个 .myb 都没有（用户依然没有可用笔刷）`,
      );
      const missing = expected.filter((name) => !got.includes(name));
      const extra = got.filter((name) => !expected.includes(name));
      if (missing.length || extra.length) {
        failures.push(
          `包内 ${BRUSH_RELATIVE}/ 与 ${SOURCE_BRUSHES} 不一致：缺 ${missing.length} 个${missing.length ? `（${missing.slice(0, 5).join("、")}${missing.length > 5 ? "…" : ""}）` : ""}、多 ${extra.length} 个${extra.length ? `（${extra.slice(0, 5).join("、")}${extra.length > 5 ? "…" : ""}）` : ""}`,
        );
      }
      console.log(
        `   ① 真仓库组装：${BRUSH_RELATIVE}/ 有 ${gotMyb.length} 支 .myb（源目录 ${expectedMyb.length} 支）`,
      );
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

// ------------------------------------------------- ② 反向控制：源目录缺失必须失败
function negativeControl(label, prepare) {
  const root = mkdtempSync(join(tmpdir(), "yanshi_pkgbrush_neg_"));
  try {
    const repo = join(root, "repo");
    mkdirSync(join(repo, "assets", "textures"), { recursive: true });
    mkdirSync(join(repo, "assets", "palettes"), { recursive: true });
    writeFileSync(join(repo, "assets", "textures", "t.txt"), "x\n");
    writeFileSync(join(repo, "assets", "palettes", "p.txt"), "y\n");
    prepare(repo);

    const stage = join(root, "stage");
    const run = runStager(repo, stage);
    assert(run.error === undefined, `${label}：跑组装脚本失败：${run.error}`);
    assert(
      run.code !== 0,
      `${label}：组装**静默成功**了（退出码 0）✗ —— 它会打出一个没有任何笔刷的包还说成功；输出：${run.out.trim()}`,
    );
    assert(
      /brushes/.test(run.out),
      `${label}：退出码非 0，但报错里**没有点名 brushes** ✗ —— 用户看不出缺的是笔刷；输出：${run.out.trim()}`,
    );
    if (run.code !== 0 && /brushes/.test(run.out)) {
      console.log(`   ② ${label}：组装明确失败并点名 brushes ✓（退出码 ${run.code}）`);
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

negativeControl("缺 assets/brushes", () => {});
negativeControl("assets/brushes 为空目录", (repo) => {
  mkdirSync(join(repo, "assets", "brushes"), { recursive: true });
});

// ------------------------------------------------------------------- 结论
if (failures.length) {
  console.log("");
  for (const failure of failures) console.log(`   ✗ ${failure}`);
  console.log(`结论：笔刷装包判据失败（${failures.length} 条）`);
  process.exit(1);
}
console.log("结论：发布包会带上完整的笔刷目录，且缺源目录时会明确失败 ✓");
