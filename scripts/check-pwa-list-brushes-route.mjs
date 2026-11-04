#!/usr/bin/env node
// **PWA 离线列笔刷路由判据**（三方对齐 · (C) 一处最小真实修复）
//
// **⇒ 背景**：服务端把「列笔刷」收在 `/api/tools/` 工具分发里（`tools.rs:2560`），
//   而本地层只实现了无前缀的 `/list_brushes`（`web/api-local.js` 约 `:3952`，回执主字段是 `brushes`）。
//   前端按服务端前缀调 `/api/tools/list_brushes` 会落到 `endpoint_not_local` ⇒ 501 ✗ ⇒ 离线与服务端不一致 ✗。
//   修法：在 `localDispatch` 的路径归一化里把该前缀归一到 `/list_brushes`（复用同一处处理，不复制回执）。
//
// **⇒ 判据（静态结构检查）**：
//   ① `web/api-local.js` 与 `crates/yanshi-http/assets/api-local.js` 逐字节一致（两份镜像）；
//   ② 两份都含「把 `/api/tools/list_brushes` 归一到 `/list_brushes`」的唯一锚点（count === 1）。
// **⇒ 变异**：临时删掉该归一化（改回坏形态）⇒ ② 的锚点 count 变 0 ⇒ 红（exit 1）；还原 ⇒ 绿（exit 0）。
// **⇒ 前提不成立**（读不到文件）⇒ exit(3)，不报成产品红。
//
// 退出码：0 = 两份镜像一致且归一锚点唯一｜1 = 断言红｜3 = 读不到源码（不可判）
import { readFileSync } from "node:fs";

const FILES = ["web/api-local.js", "crates/yanshi-http/assets/api-local.js"];
const ANCHOR = 'p2Norm === "/api/tools/list_brushes" ? "/list_brushes"';

let bufs;
try {
  bufs = FILES.map((f) => readFileSync(f));
} catch (e) {
  console.error("  ⊘ 判据前提不成立：读不到两份 api-local.js（不是产品问题）：" + e.message);
  process.exit(3);
}
const texts = bufs.map((b) => b.toString("utf8"));

const failures = [];
const check = (ok, label, detail) => {
  console.log(`  ${ok ? "✓" : "✗"} ${label}${detail ? `（${detail}）` : ""}`);
  if (!ok) failures.push(label + (detail ? `：${detail}` : ""));
};

// ① 逐字节一致（Buffer 比，避免编码归一化掩盖差异）
check(Buffer.compare(bufs[0], bufs[1]) === 0, "两份 api-local.js 逐字节一致");

// ② 归一锚点在两份里都恰好出现一次
for (let i = 0; i < FILES.length; i++) {
  const count = texts[i].split(ANCHOR).length - 1;
  check(count === 1, `${FILES[i]} 含归一锚点且唯一（count === 1）`, `实测 ${count}`);
}

if (failures.length > 0) {
  console.error(`✗ PWA 列笔刷路由判据红 ${failures.length} 条`);
  process.exit(1);
}
console.log("✓ PWA 离线列笔刷路由对齐（两份镜像一致 ＋ 归一锚点唯一）");
