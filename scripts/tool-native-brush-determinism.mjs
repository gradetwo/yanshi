#!/usr/bin/env node
// **原生画笔字节判据**（第 288 轮 ✓）—— 把 Rust 侧的
// `crates/yanshi-wasm/tests/native_brush_bytes.rs` **接进 CI 的判据集** ✓。
//
// **为什么需要这一条** ✗（目标 C⑨ ✓）：
//   那个 Rust 测试一直在跑 ✓（`cargo test --workspace` ✓ ⇒ CI 的 `test` job 里 ✓），
//   但它**不在判据集里** ✗ ⇒ 判据集的**覆盖率守卫**（`tool-criteria-coverage.mjs` ✓）**看不见它** ✗
//   ⇒ 有人删掉它、或它被 `#[ignore]` 掉，**没人会红** ✗。
//   **∴ 本包装器让它成为判据集的一等成员** ✓：`tool-*.mjs` ⇒ 被 `run-criteria.sh` 自动枚举 ✓
//   ⇒ 分片会跑它 ✓、守卫会追踪它 ✓。
//
// **它判什么** ✓（两层 ✓）：
//   ① Rust 测试**必须过** ✓（它自证：同一次调用跑两遍**逐字节相同** ✓
//      —— 输出行 `NATIVE_BRUSH twice_same=true diff=0` ✓）；
//   ② 产物**必须真的写出来且非空** ✓ ⇒ 否则"测试过了"可能只是因为**什么都没跑** ✗。
//
// **用法**：`node scripts/tool-native-brush-determinism.mjs <base> <doc> <token>`
//   （与其它 `tool-*` 同形 ✓；三个参数**都不用** ✓ —— 它跑的是本机 Rust 测试 ✓，
//    但**保留签名**以免 `run-criteria.sh` 的派发要特判 ✗ —— 与 `tool-criteria-coverage.mjs` 同一招 ✓）。
//
// **代价两面** ✗：收益＝进判据集、被守卫追踪、分片会跑 ✓；
//   代价＝分片里要**多编译一个测试目标** ✗（本机实测几秒 ✓ —— 与分片已做的 `cargo build` 共用缓存 ✓）。

import { spawnSync } from "node:child_process";
import { statSync } from "node:fs";

const OUT = process.env.NATIVE_BRUSH_OUT || "/tmp/native_brush.bin";

function fail(message) {
  console.error("  ✗ " + message);
  console.error("结论：原生画笔字节判据不合格 ✗");
  process.exit(1);
}

const run = spawnSync(
  "cargo",
  ["test", "-p", "yanshi-wasm", "--test", "native_brush_bytes", "--", "--nocapture"],
  { encoding: "utf8", maxBuffer: 32 * 1024 * 1024 },
);

if (run.error) {
  console.error(
    "✗ 前置不成立：起不了 `cargo` ⇒ 判据无法运行（**不是通过** ✗）⇒ " + run.error.message +
    "（CI 的判据分片本来就要 `cargo build` ⇒ 那里应当有 cargo ✓）",
  );
  process.exit(2);
}
const out = (run.stdout || "") + (run.stderr || "");

// 把测试自己那两行照印 ✓（读的人一眼看到"跑过了、并且逐字节相同" ✓）。
for (const line of out.split("\n")) {
  if (line.includes("NATIVE_BRUSH")) console.log("  " + line.trim());
}

if (run.status !== 0) {
  // **把真正的报错打出来** ✗ —— 否则只看到"失败"两个字 ✓，无从下手 ✓。
  const tail = out.split("\n").filter((l) => l.trim()).slice(-20);
  console.error("  ✗ Rust 测试失败（退出码 " + run.status + "）⇒ 下面是它最后 20 行：");
  for (const line of tail) console.error("      " + line.slice(0, 160));
  fail("原生画笔字节测试没过");
}

// ① 它必须自证**确定性** ✓（同一调用跑两遍逐字节相同 ✓）。
if (!/NATIVE_BRUSH twice_same=true/.test(out)) {
  fail(
    "测试过了，但**没有**打出 `NATIVE_BRUSH twice_same=true` " +
    "⇒ 要么这次没跑到那一步、要么确定性断言被拿掉了 ⇒ 判据无法成立（不是通过）",
  );
}
// ② 产物必须**真的存在且非空** ✓（否则"过了"可能只是因为什么都没跑 ✗）。
let size = 0;
try {
  size = statSync(OUT).size;
} catch {
  fail(`测试过了，但产物 ${OUT} 不存在 ⇒ 它**什么都没写** ⇒ 判据无法成立（不是通过）`);
}
if (size === 0) fail(`产物 ${OUT} 是 0 字节 ⇒ 判据无法成立（不是通过）`);

console.log(`  产物：${OUT}（${size} 字节 ✓）`);
console.log("结论：✓ 原生画笔字节测试既跑到了、又自证了逐字节确定性 ✓");
process.exit(0);
