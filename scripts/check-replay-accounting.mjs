#!/usr/bin/env node
// **"全部重放失败"也要记账**（第 725 轮；外部审计 S-03）
//
// **⇒ 缺陷**：`service.rs` 原来只有 `if replayed > 0 { … }` ✗
//   ⇒ 当 `replayed == 0` 且 `unreplayable > 0`（**一个都没补回来**）时，
//     `set_unreplayed_blobs(unreplayable)` **不执行** ✗
//   ⇒ **文档状态把"全失败"报成 0** ✓ —— 最坏的一种静默（"看起来画是完整的" ✓）。
//
// **⇒ 判据（结构检查）**：那个 `if` 的条件里必须**同时**出现 `replayed` 与 `unreplayable` ✓。
// **⇒ 变异**：改回 `if replayed > 0` ⇒ 红 ✓。
//
// **⇒ 为什么不是运行时判据** ✗：要造出"有不可重放的 blob、且一个都补不回来"的包 ✓，
//   得先做出"带坏配方的 blob 引用"（`export_project` 的省位图路径 ✓）——
//   现有夹具没有这种包 ✓ ⇒ 本轮退回结构检查，并在文案里**说清它只证明这个条件在** ✓。
import { readFileSync } from "node:fs";

const src = readFileSync("crates/yanshi-server/src/service.rs", "utf8");
const at = src.indexOf("let (replayed, unreplayable) = self.materialize_pending_replayable_blobs");
if (at < 0) {
  console.error("  ⊘ 不可判：找不到 `materialize_pending_replayable_blobs` 的调用点（改名了？）");
  process.exit(3);
}
const window = src.slice(at, at + 800);
const condition = /if\s+([^{]+)\{/.exec(window.replace(/^\s*let[^;]+;\s*/, ""));
const text = condition ? condition[1] : "";
const mentionsBoth = text.includes("replayed") && text.includes("unreplayable");
console.log(
  `  ${mentionsBoth ? "✓" : "✗"} 记账条件同时覆盖 replayed 与 unreplayable：${text.trim() || "（没解析到）"}`,
);
if (!mentionsBoth) {
  console.error(
    "  ✗ 条件只看 `replayed` ⇒ 「一个都没补回来」时不可重放的计数不会写入文档状态（外部审计 S-03）",
  );
  process.exit(1);
}
console.log("✓ 全部重放失败时也会记账（结构检查：只证明这个条件在，不声称验证运行时）");
