#!/usr/bin/env node
// **★ `wasm-bindgen-cli` 的版本必须**只有一个来源**，且**不许吞掉安装失败**** ★
//（**第 492 轮 ✓；**用户报的 PWA 部署失败换来 ✓）
//
// **∴ 它守什么 ✗**（**两个都在真实 CI 上炸过 ✓）：
//   **∴ ① 版本**必须来自 `Cargo.lock`**✗
//     ⇒ **∴ 反例（**真失败 ✓）**：`pwa.yml` 原来从 `crates/yanshi-wasm/Cargo.toml` 取**✗
//       ⇒ **∴ 而**那里写的是**范围** `wasm-bindgen = "0.2"`** ✓
//         ⇒ **∴ 于是**：`cargo install wasm-bindgen-cli --version "0.2"`**✗
//           ⇒ **∴ cargo 拒绝**：`invalid value '0.2' … unexpected end of input`
//             ⇒ **∴ 那一步**必失败**✗ ⇒ **∴ 绑定生成**从不执行** ✓
//               ⇒ **★ 所以 PWA 的部署**根本没跑到** ✓ ★**** ✓✓
//   **∴ ② 不许写死版本**✗
//     ⇒ **∴ 反例（**隐患 ✓）**：`ci.yml` 里 `--version 0.2.104 --locked || true`**✗
//       ⇒ **∴ 版本**过时**（**crate 是 0.2.129 ✓）＋ **∴ `|| true` **吞掉失败** ✓
//         ⇒ **∴ 于是**：**下一步**报错指向**错的地方** ✓ ★**** ✓✓
//
// **∴ 判据（**四项 ✓）★**：
//   ① **`scripts/wasm-bindgen-version.sh` 的输出必须等于 `Cargo.lock` 里的版本** ✓
//   ② **每个 workflow 里凡装 `wasm-bindgen-cli`**✗
//      ⇒ **∴ 必须**用 `$(scripts/wasm-bindgen-version.sh)`**✗
//        ＋ **∴ 不许**出现字面版本号** ✓
//   ③ **装上不许**吞掉失败**✗（**不许 `|| true`** ✓）
//   ④ **至少有一个 workflow 真的装它**✗（**∴ 否则本判据**在空转** ✓）
//
// **∴ 变异（**可验证 ✓）★**：
//   **∴ ①** 把某一处换回字面版本号 ⇒ **必须退 1**
//   **∴ ②** 给安装行加 `|| true` ⇒ **必须退 1**
//   **∴ ③** 让脚本输出一个假版本 ⇒ **必须退 1**
//
// **∴ 退出码 ✗**：0 通过｜1 产品缺陷｜**3 判据自身跑不了**（**AGENTS.md ✓）

import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

const ROOT = process.cwd();
const WF_DIR = join(ROOT, ".github/workflows");
const VER_SCRIPT = join(ROOT, "scripts/wasm-bindgen-version.sh");
const LOCK = join(ROOT, "Cargo.lock");

// **∴ 判据自身的前提 ✗**
for (const p of [WF_DIR, VER_SCRIPT, LOCK]) {
  if (!existsSync(p)) {
    console.error(`  ✗ 判据自身的输入缺失（不是产品问题）：${p}`);
    process.exit(3);
  }
}

const lockVersion = (() => {
  const lines = readFileSync(LOCK, "utf8").split("\n");
  for (let i = 0; i < lines.length; i += 1) {
    if (lines[i] === 'name = "wasm-bindgen"') {
      const m = (lines[i + 1] || "").match(/^version = "(.+)"$/);
      if (m) return m[1];
    }
  }
  return null;
})();
if (!lockVersion) {
  console.error("  ✗ Cargo.lock 里找不到 wasm-bindgen 的版本 ⇒ 不可判");
  process.exit(3);
}

let failed = 0;
const check = (ok, msg) => {
  console.log(`  ${ok ? "✓" : "✗"} ${msg}`);
  if (!ok) failed += 1;
};

console.log("  ── ① 脚本的输出必须等于 Cargo.lock ──");
let scriptOut = "";
try {
  scriptOut = execFileSync("bash", [VER_SCRIPT], { encoding: "utf8" }).trim();
} catch (error) {
  console.error(`  ✗ scripts/wasm-bindgen-version.sh 执行失败：${String(error).slice(0, 120)}`);
  process.exit(1);
}
check(
  scriptOut === lockVersion,
  `脚本输出（${scriptOut}）必须等于 Cargo.lock（${lockVersion}）`,
);

console.log("  ── ②③ 每个安装点：必须用脚本 ＋ 不许字面版本 ＋ 不许吞错 ──");
const files = readdirSync(WF_DIR).filter((f) => f.endsWith(".yml") || f.endsWith(".yaml"));
let installSites = 0;
for (const f of files) {
  const src = readFileSync(join(WF_DIR, f), "utf8");
  const lines = src.split("\n");
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i];
    if (!/cargo install wasm-bindgen-cli/.test(line)) continue;
    installSites += 1;
    const where = `${f}:${i + 1}`;
    check(
      /\$\(scripts\/wasm-bindgen-version\.sh\)|"\$VER"|'\$VER'|\$\{VER\}/.test(line),
      `${where} 必须用 $(scripts/wasm-bindgen-version.sh) / $VER（实测：${line.trim().slice(0, 80)}）`,
    );
    check(
      !/--version\s+["']?\d+\.\d+/.test(line),
      `${where} 不许写死版本号（实测：${line.trim().slice(0, 80)}）`,
    );
    check(!/\|\|\s*true/.test(line), `${where} 不许用 || true 吞掉安装失败`);
  }
  // **∴ `|| true` 也可能写在**下一行**✗（**分行写法 ✓）
  for (let i = 0; i < lines.length; i += 1) {
    if (!/cargo install wasm-bindgen-cli/.test(lines[i])) continue;
    const next = lines[i + 1] || "";
    check(
      !/^\s*\|\|\s*true/.test(next),
      `${f}:${i + 2} 安装的下一行不许是 || true（吞失败）`,
    );
  }
}

console.log("  ── ④ 至少有一个安装点（否则本判据空转）──");
check(installSites >= 1, `workflow 里至少有一处安装 wasm-bindgen-cli（实测 ${installSites} 处）`);

console.log("");
if (failed > 0) {
  console.error(`  ✗ wasm-bindgen 版本pinning 有 ${failed} 项不达标`);
  console.error("     ⇒ 这类缺陷会让**部署步骤**永远跑不到，或让**报错指向错的地方**");
  process.exit(1);
}
console.log(`  ✓ wasm-bindgen 版本：单一来源（${lockVersion}）＋ ${installSites} 个安装点都不吞错`);
process.exit(0);
