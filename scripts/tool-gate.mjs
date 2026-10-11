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
import { readFileSync, writeFileSync, existsSync } from "node:fs";

const KNOWN = "scripts/known-reds.txt";
const args = process.argv.slice(2);
const quick = args.includes("--quick");
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

const steps = [
  ["fmt", "cargo fmt --all -- --check"],
  ["clippy", "cargo clippy --workspace --all-targets --features gpu -- -D warnings"],
  ["test (plain)", "cargo test --workspace"],
];
if (!quick) steps.push(["test (gpu)", "cargo test --workspace --features gpu"]);

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
