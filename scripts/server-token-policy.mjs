#!/usr/bin/env node
// **令牌策略**判据（第三方代码审计 P0 第 2 条：`/api/documents` 原先匿名即发 Editor 令牌）。
//
// 策略（最小且可预期）：**绑回环** ⇒ 允许匿名发写权限令牌 ✓（本机开发/探针依赖它 ✓）；
// **绑对外** ⇒ 必须带 `YANSHI_API_KEY` ✓；没配密钥 ⇒ 拒绝并给出两条出路 ✓；密钥不对 ⇒ 拒绝 ✓。
//
// 用法：node scripts/server-token-policy.mjs <server-base> <allow|refuse> [key]
import { strict as assert } from "node:assert";
const [base, mode, key] = process.argv.slice(2);
if (!base || !["allow", "refuse"].includes(mode)) {
  console.error("用法: node scripts/server-token-policy.mjs <server-base> <allow|refuse> [key]");
  process.exit(2);
}
const url = base + "/api/documents" + (key ? "?key=" + encodeURIComponent(key) : "");
const reply = await fetch(url, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: "tokenpolicy", width: 200, height: 150 }),
}).then((r) => r.json());
const gotToken = typeof reply.token === "string" && reply.token.length > 0;
const detail = String((reply.context || {}).detail || "");
if (mode === "allow") {
  assert.ok(gotToken, `应当签发令牌，实测：${JSON.stringify(reply).slice(0, 120)}`);
  console.log("  ✓ 允许签发写权限令牌（如预期）");
} else {
  assert.ok(!gotToken, `应当拒绝签发，却拿到了令牌 ✗`);
  assert.ok(/YANSHI_API_KEY|密钥/.test(detail), `拒绝理由要能照做，实测：${detail.slice(0, 100)}`);
  console.log("  ✓ 拒绝签发写权限令牌，并给出可照做的理由｜" + detail.slice(0, 60));
}
