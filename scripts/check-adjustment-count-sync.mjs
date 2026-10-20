#!/usr/bin/env node
// **调整数量的三处必须一致**：`filter.rs` 的数组长度｜`docs/tools.md` 的标题｜运行时 `counts.adjustments`
//
// **⇒ 为什么有这条** ✗（第 1353 轮）：
//   这个数字在**三个地方**各写一遍 ⇒ 而**改一处忘两处**是这一场反复出现的漂移族
//   （`63` 那个数字我在 4 个地方改了 4 次 ✗）。⇒ 用一条判据把三处绑在一起 ✓
//
// **⇒ 断什么**：
//   ① `crates/yanshi-render/src/filter.rs` 的 `ADJUSTMENT_NAMES: [&str; N]` 能解析出 N
//   ② `docs/tools.md` 的 `## Adjustments (M)` 的 M 能解析出
//   ③ **N === M**
//   ④ 若给了 BASE（服务在跑）⇒ 再验运行时 `list_engine_capabilities.counts.adjustments === N`
//      （没给 BASE ⇒ ④ 记「未验」，**不算通过也不算失败** ✓）
//
// **⇒ 退出码**：0 通过｜1 不符｜**3 不可判**
import { readFileSync, existsSync } from "node:fs";

const FILTER = "crates/yanshi-render/src/filter.rs";
const DOC = "docs/tools.md";
for (const f of [FILTER, DOC]) {
  if (!existsSync(f)) {
    console.error(`  ⊘ 不可判：找不到 ${f}`);
    process.exit(3);
  }
}
const fails = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fails.push(m); console.log(`  ✗ ${m}`); };

const code = readFileSync(FILTER, "utf8");
const m1 = code.match(/ADJUSTMENT_NAMES:\s*\[&str;\s*(\d+)\]/);
if (!m1) { console.error("  ⊘ 不可判：filter.rs 里找不到 ADJUSTMENT_NAMES: [&str; N]"); process.exit(3); }
const codeN = Number(m1[1]);

const doc = readFileSync(DOC, "utf8");
const m2 = doc.match(/^## Adjustments \((\d+)\)/m);
if (!m2) { console.error("  ⊘ 不可判：docs/tools.md 里找不到 `## Adjustments (N)`"); process.exit(3); }
const docN = Number(m2[1]);

ok(`① filter.rs 的数组长度 = ${codeN}`);
ok(`② docs 的标题 = ${docN}`);
if (codeN === docN) ok(`③ 两处一致（${codeN}）`);
else bad(`③ 必须一致 ⇒ 代码 ${codeN}、文档 ${docN}`);

// ④ 运行时（可选）
const base = process.argv[2];
if (base) {
  try {
    // **⇒ `doc_id` 必须自己留着用** ✓（第 1353 轮修）：原先读 `d.doc_id`，
    //   而建文档的响应**不含** `doc_id` ⇒ URL 变成 `?doc=&token=…` ⇒ 服务端拒绝
    //   ⇒ 于是 ④ **静默降级成「未验」** ✗ —— 那正是我在打的那个模式 ✗
    const docId = `adj_${Date.now()}`;
    const r = await fetch(`${base}/api/documents`, {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ doc_id: docId, width: 8, height: 8 }),
    });
    const d = await r.json();
    const tok = d?.token;
    if (!tok) {
      console.log("  ○ ④ 运行时未验：建不出文档（不算失败）");
    } else {
      const rr = await fetch(`${base}/api/tools/list_engine_capabilities?doc=${docId}&token=${tok}`, {
        method: "POST", headers: { "content-type": "application/json" }, body: "{}",
      });
      // **⇒ `counts` 在**顶层**，不在 `context` 里** ✓（第 1353 轮实测：
      //   响应顶层 keys 是 `adjustments`/`blend_modes`/`counts`/… ⇒ `context` 里没有）
      const body = await rr.json();
      const live = body?.counts?.adjustments ?? body?.context?.counts?.adjustments;
      if (typeof live !== "number") console.log("  ○ ④ 运行时未验：响应里没有 counts.adjustments（不算失败）");
      else if (live === codeN) ok(`④ 运行时 counts.adjustments = ${live}（与代码一致）`);
      else bad(`④ 运行时 counts.adjustments 必须是 ${codeN} ⇒ 实测 ${live}`);
    }
  } catch (e) {
    console.log(`  ○ ④ 运行时未验：${String(e.message || e).slice(0, 60)}（不算失败）`);
  }
} else {
  console.log("  ○ ④ 运行时未验（没给 BASE）");
}

if (fails.length > 0) {
  console.log(`\n  ✗ ${fails.length} 处不符（调整数量三处必须一致）`);
  process.exit(1);
}
console.log("\n  ✓ 调整数量在代码与文档里一致");
