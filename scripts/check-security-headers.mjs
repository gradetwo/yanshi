#!/usr/bin/env node
// **`web/_headers` 的安全头不许悄悄掉**（05 复审 C-1）
//
// **⇒ 背景**：外部审计连续两轮点 `/*` 段零安全头 ✗；第 8a01a448 轮补了
// nosniff／Referrer-Policy／X-Frame-Options／frame-ancestors ✓，而 **CSP 只剩
// frame-ancestors 一条** ✗ ⇒ 资源面（外链脚本／外传 connect／object）仍然全开 ✓。
// 现在补成完整白名单，口径全部对着 `web/index.html` 实测用法收（见 `_headers` 注释 ✓）。
//
// **⇒ 判什么**：`/*` 段里五类头必须齐 ✓，且 CSP 必须含七条关键 directive ✓；
// 顺带断「`script-src` 里有 `'unsafe-inline'`」——**这是产物结构决定的硬依赖** ✗
// （9 段内联 `<script>` ✓），哪天有人删了它 ⇒ 线上查看器当场白屏 ✗ 要在这里红 ✓。
// **⇒ 变异**：删掉 CSP 行 ⇒ ①② 红 ✓；把 `'unsafe-inline'` 从 script-src 去掉 ⇒ ③ 红 ✓。
import { readFileSync } from "node:fs";

const failures = [];
const check = (ok, label, detail) => {
  console.log(`  ${ok ? "✓" : "✗"} ${label}${detail ? `（${detail}）` : ""}`);
  if (!ok) failures.push(label + (detail ? `：${detail}` : ""));
};

let text;
try {
  text = readFileSync("web/_headers", "utf8");
} catch {
  console.error("  ⊘ 判据前提不成立：读不到 web/_headers（不是产品问题）");
  process.exit(3);
}

// 取 `/*` 段（到下一个非缩进行为止），**注释行不参与**（注释里写 directive 不算数 ✗）。
const lines = text.split("\n");
const start = lines.findIndex((l) => l.trim() === "/*");
if (start === -1) {
  console.error("  ⊘ 判据前提不成立：_headers 里没有 /* 段");
  process.exit(3);
}
const block = [];
for (const line of lines.slice(start + 1)) {
  if (line.trim() === "" || /^\s/.test(line)) {
    if (line.trim() && !line.trim().startsWith("#")) block.push(line.trim());
    continue;
  }
  break;
}

const headerNames = block.map((l) => l.split(":")[0].trim().toLowerCase());
const cspLine = block.find((l) => l.trim().toLowerCase().startsWith("content-security-policy:"));
const csp = cspLine ? cspLine.slice(cspLine.indexOf(":") + 1) : "";
const directive = (name) => {
  const m = csp.split(";").map((d) => d.trim()).find((d) => d.startsWith(name + " "));
  return m || "";
};

check(
  ["x-content-type-options", "referrer-policy", "x-frame-options", "content-security-policy"]
    .every((h) => headerNames.includes(h)),
  "/* 段五类安全头齐（nosniff／referrer／frame-options／CSP）",
  headerNames.join("、"),
);
check(csp.length > 0, "有 Content-Security-Policy（不是只有注释）", cspLine || "缺失");
for (const [name, must] of [
  ["default-src", "'self'"],
  ["script-src", "'self'"],
  ["img-src", "blob:"],
  ["connect-src", "'self'"],
  ["object-src", "'none'"],
  ["base-uri", "'self'"],
  ["frame-ancestors", "'none'"],
]) {
  check(directive(name).includes(must), `CSP 的 ${name} 含 ${must}`, directive(name) || "（没有这条 directive）");
}
check(directive("script-src").includes("'unsafe-inline'"),
  "script-src 保留 'unsafe-inline'（9 段内联脚本是产物结构，删了线上就白屏）",
  directive("script-src"));
check(directive("script-src").includes("'wasm-unsafe-eval'"),
  "script-src 保留 'wasm-unsafe-eval'（内核走 WebAssembly.compile(bytes)，删了内核加载不了）",
  directive("script-src"));

if (failures.length > 0) {
  console.error(`✗ 安全头判据红 ${failures.length} 条`);
  process.exit(1);
}
console.log("✓ web/_headers 安全头齐");
