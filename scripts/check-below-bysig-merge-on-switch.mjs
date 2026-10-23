#!/usr/bin/env node
// **切回 `by_sig` 里的签名时，必须把它的旧块拿来合并、并且写回**（第 708 轮；外部审计 R-03）
//
// **⇒ 缺陷**：`c.sig != sig` 时原实现**直接丢弃**当前块并把 `keep` 置空 ✗
//   ⇒ 若 `by_sig` 里还留着该签名的旧子集，这一轮**只算新区域**；而 `needs_write` 又因
//   「条目已存在且非空」算出 `false` ⇒ **不更新 `by_sig`** ✗
//   ⇒ 读侧 `tiles_for_sig` **优先返回 `by_sig`** ⇒ **旧子集永久生效**（后续永远 miss）✗
//
// **⇒ 判据（结构检查，两条都要在）**：
// ① 那个 `Some(c)` 分支里出现 `c.by_sig.remove(&sig)`（把旧块取出来当 `keep` ✓）；
// ② `needs_write` 里含 `merged_from_by_sig`（强制写回 ✓）。
// **⇒ 变异**：去掉 ① 或 ② ⇒ 红 ✓。
//
// **⇒ 为什么是结构检查** ✗：要跑出「切回旧签名 + 局部未命中」得先让 `above`／`below`
//   的签名真的交替（`below_partial_reuse.rs` 那个夹具只覆盖同一签名内的部分命中 ✓）
//   ⇒ 本轮预算内做不出**有鉴别力**的运行时判据 ✓ ⇒ 退回结构检查，
//   并**说清它只证明这两行代码在**（不谎称验证了运行时 ✓）。
import { readFileSync } from "node:fs";

const path = "crates/yanshi-render/src/render.rs";
const src = readFileSync(path, "utf8");

const checks = [
  {
    what: "① 切签名时从 `by_sig` 取回旧块（`c.by_sig.remove(&sig)`）",
    ok: /c\.by_sig\.remove\(&sig\)/.test(src),
  },
  {
    what: "② 取回之后强制写回（`needs_write` 里含 `merged_from_by_sig`）",
    ok: /let needs_write = merged_from_by_sig\s*\n?\s*\|\|/.test(src),
  },
];

for (const check of checks) console.log(`  ${check.ok ? "✓" : "✗"} ${check.what}`);
const missing = checks.filter((check) => !check.ok);
if (missing.length > 0) {
  console.error(
    `  ✗ ${missing.length} 处缺失 ⇒ 切回旧签名时会读到「by_sig 的旧子集」（外部审计 R-03）`,
  );
  process.exit(1);
}
console.log("✓ 切回旧签名会合并 `by_sig` 的旧块并写回（结构检查：只证明这两行代码在）");
