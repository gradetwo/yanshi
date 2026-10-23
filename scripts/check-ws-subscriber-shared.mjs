#!/usr/bin/env node
// **WebSocket 的 `subscriber` 必须是两个线程共享的那一个**（第 736 轮；外部审计 S-05）
//
// **⇒ 缺陷**：它是 `u64`（`Copy`）✓ ⇒ `move` 闭包拿到的是**副本** ✗
//   ⇒ `--watch` 重载后 pusher 重新订阅的新 id **只写进副本** ✗
//   ⇒ ① 客户端之后的 `subscribe{viewport}` 打在旧 id 上（`applied: false`）✓；
//     ② 断开时 `unsubscribe(旧 id)` ⇒ **新 id 永久留在 Broadcaster** ✓
//     ⇒ `active_subscribers() > 0` ⇒ **`delete_document` 永久报「文档正在使用中」** ✗。
//
// **⇒ 判据（结构检查）**：pusher 线程的 `drain(...)`、主读线程的 `handle_ws_message(..., subscriber, ...)`
//   与收尾的 `unsubscribe(...)` 三处**都必须从共享格子读**（`push_subscriber.load` / `subscriber_cell.load`）✓，
//   且**不许**再出现裸 `subscriber`（那说明有人又用回线程副本 ✗）。
// **⇒ 变异**：把任一处改回裸 `subscriber` ⇒ 红 ✓。
//
// **⇒ 为什么是结构检查** ✗：跑真 WS ＋ `--watch` 重载 ＋ 断开后 `delete_document` 需要
//   长连接的集成夹具 ✓，本机这批判据没有它 ✓ ⇒ 退一步用源码比对，并在文案里说清它只证明"读的是共享格子" ✓。
import { readFileSync } from "node:fs";

const src = readFileSync("crates/yanshi-http/src/server.rs", "utf8");
const at = src.indexOf("let subscriber = {");
const end = src.indexOf("Ok(())", src.indexOf("unsubscribe(", at));
if (at < 0 || end < 0) {
  console.error("  ⊘ 不可判：找不到 WebSocket 的 subscriber 区块（函数改名了？）");
  process.exit(3);
}
const block = src.slice(at, end);

const checks = [
  { what: "有共享格子（`Arc<AtomicU64>`）", ok: /Arc::new\(std::sync::atomic::AtomicU64::new\(subscriber\)\)/.test(block) },
  { what: "pusher 的 `drain` 读共享格子", ok: /drain\(push_subscriber\.load\(/.test(block) },
  // **`cargo fmt` 会把 `push_subscriber.store(` 折成两行** ✓ ⇒ 正则要容忍换行 ✓
  { what: "重载后写共享格子", ok: /push_subscriber\s*\.store\(/.test(block) },
  { what: "主读线程读共享格子", ok: /subscriber_cell\.load\(std::sync::atomic::Ordering::SeqCst\)/.test(block) },
  { what: "退订读共享格子", ok: /unsubscribe\(\s*subscriber_cell\.load\(/.test(block) },
];
for (const check of checks) console.log(`  ${check.ok ? "✓" : "✗"} ${check.what}`);
const missing = checks.filter((check) => !check.ok);
if (missing.length > 0) {
  console.error(`  ✗ ${missing.length} 处缺失 ⇒ 重载后的新订阅者不会同步到读线程（外部审计 S-05）`);
  process.exit(1);
}
console.log("✓ WebSocket 的 subscriber 在读写两线程间共享（结构检查：只证明读的是共享格子）");
