#!/usr/bin/env node
// **GPU 读回的 `map_async` 失败不许被忽略**（第 747 轮；外部审计 R-06）
//
// **⇒ 缺陷**（第 747 轮逐处核对，行号是当时的）：
//   · `lib.rs:1070` `move |_r|` ⇒ **回调结果被丢掉** ✗ ⇒ 映射失败时后面的
//     `get_mapped_range()` 会 **panic** ✓（`wgpu` 在未映射上 `get_mapped_range` 是 panic，不是 Result ✓）；
//   · `lib.rs:1139` 把 `result` 发回来，但只 `if rx.recv().is_ok()` ⇒ **只看了"通道还在"，没看 `Err`** ✗；
//   · `lib.rs:1962` ✅ **正确**（`rx.recv().ok()?.is_err()` ⇒ 见 `Err` 就返回 `None`）；
//   · `lib.rs:2107` 只 `if rx.recv().is_err()` ⇒ **通道关闭**才算错，`Err(结果)` **被忽略** ✗。
//
// **⇒ 判据（规格先行 ✓）**：每个 `map_async(..., |r| …)` 的**回调结果**必须**在某处被判定** ✓：
//   要么回调里 `match/if` 了它 ✓，要么接收端对 `Err` 做了分支 ✓。
//   **⇒ 当前状态**：4 处里 **3 处不合格** ⇒ 本条**预期红** ✓（登记在 `criteria-known-red.txt` ✓）。
// **⇒ 为什么只定位不修** ✗：GPU／wgpu 是**明确的边界**（"渲染层问题只复现与定位，不修" ✓）⇒
//   这里只把缺陷**钉成可复核的规格** ✓，等那侧的人来修 ✓（红转绿就是修好的证明 ✓）。
import { readFileSync } from "node:fs";

const src = readFileSync("crates/yanshi-gpu/src/lib.rs", "utf8");
const lines = src.split("\n");
const sites = [];
for (let index = 0; index < lines.length; index += 1) {
  const line = lines[index];
  if (!/map_async\(wgpu::MapMode::Read/.test(line)) continue;
  // 收集到回调结束（粗略：下 12 行内找 `});`）
  const window = lines.slice(index, index + 12).join("\n");
  const callback = /map_async\(wgpu::MapMode::Read,\s*(?:move\s*)?\|([A-Za-z_][A-Za-z0-9_]*)\|([\s\S]*?)\}\)/.exec(window);
  const name = callback ? callback[1] : "?";
  const body = callback ? callback[2] : "";
  // ① 回调里直接判定（`if name.is_err()` / `match name`）⇒ 合格
  const judgedInline = new RegExp(`(?:if|match)\\s+(?:${name}\\s*)?\\.?is_err|match\\s+${name}\\b`).test(body);
  // ② 忽略（参数名以 `_` 开头）⇒ 不合格
  const ignored = name.startsWith("_");
  // ③ 接收端判定**必须判到"回调结果"本身** ✓：
  //   **⇒ 两种形态**：把 `result` 本身 `send` 出去 ✓ 或 只发信号 ✓
  //     · 发 `result` ⇒ 接收端要 `recv().ok()?.is_err()`（**判到值上** ✓）；
  //       `recv().is_err()` **只说明通道关了** ✗ —— 我第一次就把它当成合格 ✓
  //       （于是 2107 那处漏判 ✓，与审计不一致 ✓）。
  //     · 只发信号（没带结果）⇒ **回调里必须自己判** ✓，否则结果丢了 ✗。
  const sendsResult = new RegExp(`send\\(\\s*${name}\\s*\\)`).test(body);
  const receiverJudged = sendsResult && /recv\(\)\s*\.ok\(\)\?\.is_err\(\)|recv\(\)\s*\.ok\(\)\?\.map\(.*is_err|recv\(\)\s*\.ok\(\)\?\.flatten/.test(window);
  sites.push({
    line: index + 1,
    name,
    ok: !ignored && (judgedInline || receiverJudged) || (!ignored && /recv\(\)\s*\.ok\(\)\?\s*\.is_err\(\)/.test(window)),
    why: ignored
      ? "回调参数以下划线开头（结果被丢掉）"
      : judgedInline
        ? "回调里判定了"
        : receiverJudged
          ? "接收端判到了结果值上"
          : sendsResult
            ? "接收端只看通道（`recv().is_err()`），没判回调的 Err"
            : "结果既没在回调里判、也没发给接收端",
  });
}
if (sites.length === 0) {
  console.error("  ⊘ 不可判：`yanshi-gpu` 里找不到 `map_async` 读回（实现改名了？）");
  process.exit(3);
}
let bad = 0;
for (const site of sites) {
  if (!site.ok) bad += 1;
  console.log(`  ${site.ok ? "✓" : "✗"} lib.rs:${site.line} \`|${site.name}|\` ⇒ ${site.why}`);
}
if (bad > 0) {
  console.error(
    `  ✗ ${bad}/${sites.length} 处忽略了 \`map_async\` 的结果 ⇒ 映射失败时 \`get_mapped_range()\` 会 panic（外部审计 R-06）`,
  );
  process.exit(1);
}
console.log(`✓ ${sites.length} 处读回都判定了 \`map_async\` 的结果（外部审计 R-06 已修）`);
