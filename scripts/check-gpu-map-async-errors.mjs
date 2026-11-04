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
//   要么回调里判了 `Err`／`Ok` ✓，要么**把结果本身**发给接收端、且接收端在**值**上分支 ✓。
//   **⇒ 只判「通道还在」不算**（`recv().is_err()`／`recv().is_ok()` 只说明通道，不说明映射结果 ✗）。
//
// **★ 本判据自己过期过一次（本轮修，「测量工具要先被测量」✓）**：
//   产品侧后来按审计 P3 #4 把每一处都修好了，但用了**好几种形态** ——
//   `if let Err(e) = result`（把 Err 存进共享格，下游据此跳过 `get_mapped_range`）、
//   `matches!(rx.recv(), Ok(Ok(())))`（含取反写法）、`rx.recv().ok()?.is_err()`；
//   而本判据的正则**只认最后一种** ⇒ 报「多处忽略结果」的**假红**，又因为登记在 known-red 里
//   一直没被看见 ✗ ⇒ **规格没过期，过期的是量它的尺子** ✓。
// **⇒ 现在认全这些形态，但不放松**：回调参数以 `_` 开头（丢结果）、接收端只看通道、
//   以及「既没在回调判、也没把结果本身送出去」三种仍不合格 ✓。
// **⇒ 变异**：把任一处改回这三种之一 ⇒ 恰好那一处红 ✓（每种形态各验一次）。
// **⇒ 转绿后已从 `criteria-known-red.txt` 移出**（红转绿就是修好的证明 ✓）。
import { readFileSync } from "node:fs";

const SRC = "crates/yanshi-gpu/src/lib.rs";
let src;
try {
  src = readFileSync(SRC, "utf8");
} catch {
  console.error(`  ⊘ 不可判：读不到 ${SRC}（不是产品问题）`);
  process.exit(3);
}
const lines = src.split("\n");
const RECV = "[A-Za-z_][A-Za-z0-9_]*\\.recv\\(\\)";
const sites = [];
for (let index = 0; index < lines.length; index += 1) {
  const line = lines[index];
  if (!/map_async\(wgpu::MapMode::Read/.test(line)) continue;
  // 收集到回调结束（粗略：下 12 行内找 `});`）
  const window = lines.slice(index, index + 12).join("\n");
  const callback = /map_async\(wgpu::MapMode::Read,\s*(?:move\s*)?\|([A-Za-z_][A-Za-z0-9_]*)\|([\s\S]*?)\}\)/.exec(window);
  if (!callback) {
    sites.push({ line: index + 1, name: "?", ok: false, why: "找不到回调体（形态变了？）" });
    continue;
  }
  const name = callback[1];
  const body = callback[2];
  // ① 回调里直接判定结果（含「把 Err 存进共享格、下游据此跳过 get_mapped_range」的形态 ✓）
  const judgedInline = new RegExp(
    `(?:if|while|match)\\s+let\\s+(?:Err|Ok)\\s*\\([^)]*\\)\\s*=\\s*${name}\\b` +
      `|(?:if|matches!)\\s*\\(?\\s*${name}\\s*\\.\\s*is_(?:err|ok)\\s*\\(` +
      `|(?:if|matches!)\\s*\\(?\\s*${name}\\s*,\\s*(?:Err|Ok)\\b` +
      `|match\\s+${name}\\b`,
  ).test(body);
  // ② 忽略（参数名以 `_` 开头）⇒ 不合格
  const ignored = name.startsWith("_");
  // ③ 接收端判定：**必须把结果本身送出来**（只送信号不算 ✓），且接收端判到值上
  const sendsResult = new RegExp(`send\\(\\s*${name}\\s*\\)`).test(body);
  const receiverJudged = sendsResult && new RegExp(
    `${RECV}\\s*\\.ok\\(\\)\\?\\s*\\.is_err\\(\\)` +
      `|${RECV}\\s*\\.ok\\(\\)\\?\\s*\\.map\\([^)]*is_err` +
      `|${RECV}\\s*\\.ok\\(\\)\\?\\s*\\.flatten` +
      `|${RECV}\\s*\\.unwrap\\(\\)\\s*\\.is_err\\(\\)` +
      `|matches!\\s*\\(\\s*${RECV}\\s*,\\s*Ok\\s*\\(\\s*Ok\\b` +
      `|(?:if|while)\\s+let\\s+Ok\\s*\\(\\s*Ok\\s*\\([^)]*\\)\\s*\\)\\s*=\\s*${RECV}`,
  ).test(window);
  sites.push({
    line: index + 1,
    name,
    ok: !ignored && (judgedInline || receiverJudged),
    why: ignored
      ? "回调参数以下划线开头（结果被丢掉）"
      : judgedInline
        ? "回调里判定了"
        : receiverJudged
          ? "接收端判到了结果值上"
          : sendsResult
            ? "接收端只看通道，没判回调的 Err"
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
  console.log(`  ${site.ok ? "✓" : "✗"} ${SRC}:${site.line} \`|${site.name}|\` ⇒ ${site.why}`);
}
if (bad > 0) {
  console.error(
    `  ✗ ${bad}/${sites.length} 处忽略了 \`map_async\` 的结果 ⇒ 映射失败时 \`get_mapped_range()\` 会 panic（外部审计 R-06）`,
  );
  process.exit(1);
}
console.log(`✓ ${sites.length} 处读回都判定了 \`map_async\` 的结果（外部审计 R-06 已修）`);
