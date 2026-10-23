#!/usr/bin/env node
// **`get_diff` 的两台宿主必须回同一组字段名**（第 735 轮；外部审计 S-07 第 ④ 项）
//
// **⇒ 缺陷**：本地层（`web/api-local.js`）返回 `count`，服务端（`read_get_diff`）返回 `atom_count` ✗
//   ⇒ 还少了 `head_seq` ✗ ⇒ **同一份代码在两台宿主上字段不同** ✓
//   ⇒ 调用方按服务端写、在浏览器里读不到 ✓（PWA 与 server 的"两条路漂移"✓，本仓反复吃过的亏 ✓）。
//
// **⇒ 判据（结构检查）**：从**两处源码**里各抽 `get_diff` 的返回字段名 ✓，断言**服务端那组是浏览器那组的前缀集**
//   （浏览器**可以多**给 `doc_id`／`server`／`note` 这类宿主信息 ✓，但**不许少** ✓）。
// **⇒ 变异**：把 `atom_count` 改回 `count` ⇒ 红 ✓。
//
// **⇒ 为什么是结构检查** ✗：跑真浏览器要 chromium ✓（CI 上有、本机这批判据不跑 ✓）
//   ⇒ 退一步：**字段名漂移**用源码比对就能抓 ✓，而在文案里**说清它只证明字段名一致** ✓。
import { readFileSync } from "node:fs";

const server = readFileSync("crates/yanshi-server/src/tools.rs", "utf8");
const local = readFileSync("web/api-local.js", "utf8");

const serverBody = (() => {
  const at = server.indexOf("fn read_get_diff");
  const end = server.indexOf("\n}", at);
  return server.slice(at, end);
})();
const localBody = (() => {
  const at = local.indexOf('if (path === "/api/tools/get_diff")');
  const end = local.indexOf('if (path === "/api/tools/get_dependency_graph")', at);
  return local.slice(at, end);
})();

const keys = (text) => {
  const found = new Set();
  for (const match of text.matchAll(/"([a-z_]+)"\s*:/g)) found.add(match[1]);
  for (const match of text.matchAll(/\b([a-z_]+):\s/g)) found.add(match[1]);
  return found;
};

/** 返回值字段（服务端是 `json!({...})`，浏览器是 `json({...})`） */
const returnKeys = (text) => {
  const at = Math.max(text.lastIndexOf("json!({"), text.lastIndexOf("json({"));
  if (at < 0) return new Set();
  const body = text.slice(at);
  const found = new Set();
  for (const match of body.matchAll(/(?<![\w.])"?([a-z_]+)"?\s*:/g)) found.add(match[1]);
  return found;
};

const serverKeys = returnKeys(serverBody);
const localKeys = returnKeys(localBody);
const missing = [...serverKeys].filter((key) => !localKeys.has(key));

for (const key of [...serverKeys].sort()) {
  const mark = localKeys.has(key) ? "✓" : "✗";
  console.log(`  ${mark} ${key}`);
}
if (missing.length > 0) {
  console.error(
    `  ✗ 本地层少给了服务端的 ${missing.length} 个字段：${missing.join("、")} ⇒ 两台宿主漂移（外部审计 S-07 第 ④ 项）`,
  );
  process.exit(1);
}
console.log(
  `✓ \`get_diff\` 的字段名一致（服务端 ${serverKeys.size} 个都在本地层；本地层另多 ${localKeys.size - serverKeys.size} 个宿主字段）`,
);
