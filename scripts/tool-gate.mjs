#!/usr/bin/env node
// **★★★ 白名单化门禁 ✗ ★★★**（**第 644 轮 ✓）
//
// **∴ 它解决什么 ✗**：**既有红会被当成**我的失败**✗
//   ＋ **∴ 于是**每次跑门禁都要**人工判断**"是不是我弄的"** ✓
//     ＋ **∴ 而**那**容易**漏掉真正的回归** ✓
//       ⇒ **★ 本工具 ✗**：**把失败分成**已知红（**记录 ✓）与**新红（**致命 ✓）** ★
//
// **∴ 退出码 ✗**：
//   **∴ 0** ⇒ **没有新红**（**已知红会打印，但不致命 ✓）
//   **∴ 1** ⇒ **有新红**（**或** fmt／clippy 失败** ✓）
//   **∴ 3** ⇒ **工具自己坏了**（**∴ 与产品失败区分 ✓ —— 仓库纪律 ✓）
//
// **∴ 用法 ✗**：
//   `node scripts/tool-gate.mjs`                # 全量（**fmt ＋ clippy ＋ plain ＋ gpu ✓）
//   `node scripts/tool-gate.mjs --quick`        # 只跑 fmt ＋ clippy ＋ plain（**快 ✓）
//   `node scripts/tool-gate.mjs --update-known` # 把当前全部失败写进白名单
import { execSync } from "node:child_process";
import { readFileSync, writeFileSync, existsSync, readdirSync } from "node:fs";

const KNOWN = "scripts/known-reds.txt";
const args = process.argv.slice(2);
const quick = args.includes("--quick");
// **★★★ `--scoped`：只跑**受影响的 crate** ✗ ★★★**（**第 729 轮 ✓）
//
// **∴ 为什么需要它 ✗**（**用户问：门禁为什么这么慢 ✓）：
//   **∴ 本机实测 ✗**：**4 核 i5-4250U @ **1.30 GHz**（**2013 年超低压 ✓）＋ **内存 3 GB** ✓
//     ＋ **∴ 而**门禁的 `clippy --workspace --all-targets` ＋ `test --workspace`**✗
//       ⇒ **∴ 它们**编译**全部 crate 的全部目标**✗ ⇒ **∴ 在**这台机器上**要**数分钟** ✓ ★
//         ＋ **∴ 而**瓶颈**主要是**内存换页**✗（**∴ 3 GB ✓）**不是**并行度** ✓
//           ＋ **∴ 所以**：**本地只跑**受影响的 crate**✗ ＋ **∴ 全量交给** GitHub CI** ✓ ★
//             ＋ **∴ 而**那**正好**就是**目标第 13 条**（**CI 真跑 ✓）的用法** ✓ ★
//
// **∴ 用法 ✗**：`node scripts/tool-gate.mjs --quick --scoped` ★
const scoped = args.includes("--scoped");
const update = args.includes("--update-known");

function run(cmd) {
  try {
    const out = execSync(cmd, { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
    return { ok: true, out };
  } catch (e) {
    return { ok: false, out: `${e.stdout ?? ""}${e.stderr ?? ""}` };
  }
}

function failedTests(text) {
  const names = new Set();
  for (const m of text.matchAll(/test ([A-Za-z0-9_:]+) \.\.\. FAILED/g)) {
    names.add(m[1]);
  }
  return names;
}

const known = new Set(
  existsSync(KNOWN)
    ? readFileSync(KNOWN, "utf8")
        .split("\n")
        .map((l) => l.trim())
        .filter((l) => l && !l.startsWith("#"))
    : [],
);

// **∴ 从 git 找**受影响的 crate** ✗**（**∴ 用于 `--scoped` ✓）★
function affectedPackages() {
  if (!scoped) return "";
  try {
    // **∴ 必须同时看**工作树 ＋ 最后一次提交** ✗**（**第 730 轮的实测教训 ✓）：
    //   **∴ 原来的错 ✗**：**只看 `HEAD~1..HEAD`**✗ ⇒ **∴ 若**改动**还没提交**（**∴ 最常见 ✓）
    //     ＋ **∴ 它**就报「**没找到受影响的 crate ⇒ 跑全量**」✗ ⇒ **∴ 于是**慢** ✓ ★
    //       ＋ **∴ 现在 ✗**：**三段合并**✗（**∴ 工作树 ＋ 暂存 ＋ 最后一次提交 ✓）★
    const fromGit = (cmd) => {
      try {
        return execSync(cmd, { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] })
          .split("\n")
          .filter((l) => l.startsWith("crates/"));
      } catch {
        return [];
      }
    };
    const changed = [
      ...fromGit("git diff --name-only"),
      ...fromGit("git diff --cached --name-only"),
      ...fromGit("git diff --name-only HEAD~1 HEAD"),
    ];
    const names = new Set();
    for (const f of changed) {
      const m = f.match(/^crates\/([^/]+)\//);
      if (m) names.add(m[1]);
    }
    if (names.size === 0) return "";
    return [...names].map((n) => `-p ${n}`).join(" ");
  } catch {
    return "";
  }
}
const scope = affectedPackages();
// **★★★ `--features gpu` **只能给**有该 feature 的 crate** ✗ ★★★**（**第 755 轮 ✓）
//   **∴ 症状 ✗**（**实测 ✓）**：
//     **∴ `node scripts/tool-gate.mjs --quick --scoped`**✗ 在**改到 `yanshi-server` 时**报**
//       `error: the package 'yanshi-server' does not contain this feature: gpu`** ✓
//     ＋ **∴ 于是**：**clippy **必然硬失败**✗ ⇒ **∴ 门禁**红**而**与代码无关** ✓ ★★
//   **∴ 根因 ✗**：** `--scoped` **生成** `${scope} --features gpu`**✗
//     ＋ **∴ 而** `scope` **是**改动涉及的 crate 列表**✗（**∴ 如** `-p yanshi-server` ✓）
//       ＋ **∴ 那些 crate **未必**有 `gpu` feature** ✓ ★
//   **∴ 修法 ✗**：**读每个 crate 的 `Cargo.toml`**✗
//     ＋ **∴ 只对**确实声明了 `gpu` feature 的 crate**保留那个 flag** ✓
//       ＋ **∴ 否则**去掉它**✗（**∴ 从而**命令**合法** ✓）★
function cratesWithGpuFeature() {
  const names = new Set();
  try {
    const dir = "crates";
    for (const entry of readdirSync(dir)) {
      const manifest = `${dir}/${entry}/Cargo.toml`;
      if (!existsSync(manifest)) continue;
      const text = readFileSync(manifest, "utf8");
      if (/^gpu\s*=/m.test(text)) names.add(entry);
    }
  } catch {
    /* 读不到 ⇒ 视为都没有 ⇒ 不加 flag ✓ */
  }
  return names;
}
const gpuCrates = cratesWithGpuFeature();
function scopeHasGpu() {
  if (!scope) return true;
  const picked = scope.split(/\s+/).filter((t) => t && t !== "-p");
  return picked.every((name) => gpuCrates.has(name));
}
const scopeWithGpuFlag = scope || "";
const gpuFlag = !scope || scopeHasGpu() ? " --features gpu" : "";
if (scoped) {
  console.log(scope ? `== --scoped：只跑 ${scope} ==` : "== --scoped：没找到受影响的 crate ⇒ 跑全量 ==");
}

// **★★★ 脚本语法检查**必须**进门禁** ✗ ★★★**（**第 753 轮；**本会话两次踩坑 ✓）
//   **∴ 为什么需要 ✗**（**两次真实事故 ✓）★**：
//     **∴ ① 第一次 ✗**：**模板字符串内的注释含反引号**✗
//       ＋ **∴ 于是**：**那份脚本**语法错**✗ ⇒ **∴ 而**所有本地门禁**都**没检查脚本语法** ✗
//         ＋ **∴ 结果**：**它**带着语法错**提交并推送**✗ ⇒ **∴ 一轮 CI**全红** ✓ ★★
//     **∴ ② 第二次 ✗**：**同一个坑**✗ ⇒ **∴ 但**我**在命令里写了 `node --check … || exit 1`** ✗
//       ＋ **∴ 所以**它**当场中止**✗ ⇒ **∴ 没有**进提交** ✓ ★
//   **∴ 为什么不用**正则判据** ✗**（**∴ 我**试过 ✓）★**：
//     **∴ 我**写了一条 Rust 判据扫「模板内注释的反引号」**✗
//       ＋ **∴ 而**它**报了 **60 处**✗ 其中**绝大多数**是**假阳性** ✓（**∴ 启发式解析**不可靠 ✓）★
//         ＋ **∴ 所以**：**删掉它**✗ ＋ **∴ 改用** `node --check`**✗
//           ＋ **∴ 因为**那**是** JS **引擎自己的**语法检查** ⇒ **∴ 权威 ＋ 零假阳性** ✓ ★
const steps = [
  // **∴ 先查脚本语法**（**快 ＋ 早失败 ✓）★
  [
    "scripts syntax",
    "for f in scripts/*.mjs; do node --check \"$f\" || exit 1; done",
  ],
  ["fmt", "cargo fmt --all -- --check"],
  [
    "clippy",
    scope
      ? `cargo clippy ${scopeWithGpuFlag} --all-targets${gpuFlag} -- -D warnings`
      : "cargo clippy --workspace --all-targets --features gpu -- -D warnings",
  ],
  ["test (plain)", scope ? `cargo test ${scope}` : "cargo test --workspace"],
];
if (!quick)
  steps.push([
    "test (gpu)",
    scope ? `cargo test ${scope} --features gpu` : "cargo test --workspace --features gpu",
  ]);
// **★★★ `--quick` 必须**说出来**它跳过了什么 ✗ ★★★**（**第 760 轮 ✓）
//   **∴ 为什么 ✗**（**一条**真实的缺口**，不是预防性补丁 ✓）★**：
//     **∴ 我一直用 `--quick`**✗ ⇒ **∴ 于是** `test (gpu)` **在我本地**从未跑过** ✓
//       ＋ **∴ 而**只有 `--features gpu` 才编译的判据**✗（**∴ 如** `gpu_quantize_dispatch` ✓）
//         ＋ **∴ 它们**恰好就是**GPU 的核心判据** ✓
//           ＋ **∴ 代价 ✗**：**已经真的发生了** ——
//             **∴ 一条判据假定阈值是 30 000**✗ 而**现实是 1 000 000** ✓
//               ＋ **∴ 它**用 256×256 ＝ 65 536**✗ ⇒ **∴ 那句「本测试必须超过阈值」**当场红** ✓
//                 ＋ **∴ 而**它**逃过了**本地（`--quick` ✓）与 CI（**原来只跑 `yanshi-http` ✓）** ✓
//                   ＋ **∴ 所以**：**一次失败**能藏很久** ✓ ★★
//   **∴ 现在 ✗**：**显式警告**✗ ⇒ **∴ 用的人**知道**自己没验什么** ✓
//     ＋ **∴ 而不是**默认它验过了** ✓ ★
if (quick)
  console.warn(
    "  ⚠️ --quick：**跳过了 `test (gpu)`** ⇒ 只有 `--features gpu` 才编译的判据（量化／合成／哨兵）本次**没有验**",
  );

const allFailed = new Set();
let hardFail = [];

for (const [label, cmd] of steps) {
  const r = run(cmd);
  if (r.ok) {
    console.log(`  ✓ ${label}`);
    continue;
  }
  const fails = failedTests(r.out);
  if (fails.size === 0) {
    hardFail.push(label);
    console.log(`  ★ ${label} 失败（**且拿不到测试名 ⇒ 视为硬失败 ✓）`);
  } else {
    for (const f of fails) allFailed.add(f);
    console.log(`  ⚠ ${label}：${fails.size} 个失败`);
  }
}

if (update) {
  const header = readFileSync(KNOWN, "utf8")
    .split("\n")
    .filter((l) => l.startsWith("#"))
    .join("\n");
  const body = [...allFailed].sort().join("\n");
  writeFileSync(KNOWN, `${header}\n${body}\n`, "utf8");
  console.log(`  ✓ 已把 ${allFailed.size} 个失败写入 ${KNOWN}`);
  process.exit(0);
}

const knownHit = [...allFailed].filter((f) => known.has(f)).sort();
const newRed = [...allFailed].filter((f) => !known.has(f)).sort();

if (knownHit.length) {
  console.log(`  ── 已知红（**不致命，但**记录在案** ✓）${knownHit.length} 个：`);
  for (const f of knownHit) console.log(`      · ${f}`);
}
if (newRed.length) {
  console.log(`  ★★ 新红 ${newRed.length} 个（**致命 ✓）：`);
  for (const f of newRed) console.log(`      ✗ ${f}`);
}
if (hardFail.length) {
  console.log(`  ★★ 硬失败：${hardFail.join("｜")}`);
}

if (newRed.length || hardFail.length) {
  console.log("  ⇒ 判定：★ 失败（有新红或硬失败）");
  process.exit(1);
}
console.log(`  ⇒ 判定：✓ 通过（新红 0｜已知红 ${knownHit.length}）`);
process.exit(0);
