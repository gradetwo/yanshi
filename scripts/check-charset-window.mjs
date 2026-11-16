#!/usr/bin/env node
// **`<meta charset>` 必须落在 `web/index.html` 的前 1024 字节内**（2026-11-17 修复配套）
//
// **⇒ 背景**：在线 demo（yanshi-online，Cloudflare Workers 静态托管）的响应头
//   不带 charset（托管方按扩展名推断 Content-Type，只有 `text/html`），
//   而 index.html 的 `<meta charset>` 原在第 101 行、第 6505 字节——被 `<head>`
//   之后的大段中文注释与内联哨兵脚本挤出了浏览器编码预扫描的
//   「前 1024 字节」窗口 ⇒ 编码嗅探失败 ⇒ 回退区域默认遗留编码
//   （zh-CN ⇒ GBK、ja ⇒ Shift_JIS、en ⇒ windows-1252）⇒ UTF-8 中文整页乱码
//   （用户报障；Chromium 已无手动切换编码的菜单，访客无法自救）。
//   本地 `yanshi-serve` 无此问题（http.rs 显式发 `text/html; charset=utf-8`）。
//
// **⇒ 修复**：meta 挪到 `<head>` 之后的第一个元素（治本，与托管方无关）；
//   `web/_headers` 再给 `/index.html` 补 `Content-Type: text/html; charset=utf-8`
//   （传输层加固）。本判据守治本的那一半。
//
// **⇒ 判什么**（结构量：存在性＋字节偏移，不断像素不断快照）：
//   ① `web/index.html` 里找得到 `<meta charset`；
//   ② 它的 UTF-8 字节偏移 < 1024（以 Buffer 字节数计，与浏览器预扫描同口径）。
//
// **⇒ 变异**：把 meta 挪回大段注释之后（偏移 > 1024）⇒ ② 红 ✓；
//   整行删掉 meta ⇒ ① 红 ✓（两条都实测红过再回绿）。
import { readFileSync } from "node:fs";

const failures = [];
const check = (ok, label, detail) => {
  console.log(`  ${ok ? "✓" : "✗"} ${label}${detail ? `（${detail}）` : ""}`);
  if (!ok) failures.push(label + (detail ? `：${detail}` : ""));
};

let buf;
try {
  buf = readFileSync("web/index.html");
} catch {
  console.error("  ⊘ 判据前提不成立：读不到 web/index.html（不是产品问题）");
  process.exit(3);
}

const offset = buf.indexOf("<meta charset");
check(offset !== -1, "index.html 含 <meta charset 声明", offset === -1 ? "未找到" : `字节偏移 ${offset}`);
if (offset !== -1) {
  check(offset < 1024, "<meta charset> 落在前 1024 字节内", `偏移 ${offset} / 窗口 1024`);
}

if (failures.length) {
  console.error(`✗ check-charset-window：${failures.length} 条不过`);
  process.exit(1);
}
console.log("✓ check-charset-window");
