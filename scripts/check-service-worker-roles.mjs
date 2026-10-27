#!/usr/bin/env node
// **Service Worker 只有一份，且两份角色各不相同**（第 745 轮；外部审计 S-10）
//
// **⇒ 缺陷**：`web/` 下原来有**两份**静态 worker ✓：`web/service-worker.js`（437 行 ✓，应用注册的那份）
//   与 `web/sw.js`（**48 行、更简单、从来没被注册** ✗）⇒ **平行实现** ✓，
//   而**两条判据在护着没人用的那份** ✓（`tool-pwa-assets` 与 `tool-sw-stamp-idempotent` ✓）。
//
// **⇒ 两种角色（第 745 轮查清，别再混）**：
//   · **内嵌那份** `crates/yanshi-http/assets/service-worker.js` ⇒ 服务端在**请求时**替换
//     `__BUILD_ID__` ✓ ⇒ **它必须保留占位符** ✓（Rust 测试 `the_service_worker_is_a_separate_packable_asset` ✓）；
//   · **静态托管那份** `web/service-worker.js` ⇒ `pwa-sync-viewer.mjs` **打上具体戳** ✓
//     ⇒ **它不该再有占位符** ✓（否则静态托管下缓存名永不变化 ✓）。
//
// **⇒ 判据（四条）**：
//  ① `web/sw.js*`（含 `.bak` 等变体）**一个都不许存在** ✓；② 内嵌那份**有** `__BUILD_ID__` ✓；
//  ③ 静态托管那份**没有**占位符、且有 `yanshi-shell-<数字>` 戳 ✓；④ 两份都预缓存内核 wasm ✓。
// **⇒ 变异**：把 `web/sw.js` 或 `web/sw.js.bak` 加回来 ⇒ ① 红 ✓；让同步不给副本打戳 ⇒ ③ 红 ✓。
import { readFileSync, readdirSync } from "node:fs";

const failures = [];
const check = (ok, label, detail) => {
  console.log(`  ${ok ? "✓" : "✗"} ${label}${detail ? `（${detail}）` : ""}`);
  if (!ok) failures.push(label + (detail ? `：${detail}` : ""));
};

const EMBEDDED = "crates/yanshi-http/assets/service-worker.js";
const STATIC = "web/service-worker.js";

// **① 的口径（xiaomi 审计 H-2 补强）**：不只 `web/sw.js` 本名 ——
// 任何 `web/sw.js*`（`.bak`、`.old`…）都算平行实现的残留 ✗；
// 实测（本场）：`web/sw.js` 第 745 轮删掉后**又以 `.bak` 形态留在磁盘**，旧判据抓不到 ✓。
let swResidue = [];
try {
  swResidue = readdirSync("web").filter((n) => n === "sw.js" || n.startsWith("sw.js."));
} catch {
  console.error("  ⊘ 判据前提不成立：读不到 web/（不是产品问题）");
  process.exit(3);
}
check(swResidue.length === 0, "没有第二份静态 worker（`web/sw.js*` 全部不许存在）",
  swResidue.length ? `残留：${swResidue.join("、")}` : "");
const embedded = readFileSync(EMBEDDED, "utf8");
const statik = readFileSync(STATIC, "utf8");
check(embedded.includes("__BUILD_ID__"), "内嵌那份保留 `__BUILD_ID__`（服务端请求时替换）", "丢了 ⇒ 缓存永不失效");
check(!statik.includes("__BUILD_ID__"), "静态托管那份已打上具体戳（不再有占位符）", "还有占位符 ⇒ 静态托管下缓存永不失效");
const stamp = statik.match(/yanshi-shell-(\d+)/);
check(Boolean(stamp), "静态托管那份带 `yanshi-shell-<数字>` 戳", stamp ? stamp[0] : "没找到");
for (const [name, text] of [["内嵌", embedded], ["静态托管", statik]]) {
  check(
    text.includes("/wasm/yanshi_wasm_bg.wasm"),
    `${name}那份预缓存内核 wasm`,
    "没有 ⇒ 离线打不开",
  );
}

if (failures.length) {
  console.error(`  ✗ ${failures.length} 项没成立（外部审计 S-10）`);
  process.exit(1);
}
console.log("✓ 只有一份 Service Worker，内嵌／静态托管两种角色各自正确");
