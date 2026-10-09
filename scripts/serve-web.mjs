//! **★ 给判据用的**本地静态服务** ✗ ★**（第 54 轮 ✓）。
//!
//! **∴ 为什么它必须在**仓内** ✗ ★**（**∴ 用户重启机器那次事故换来的教训 ✓）：
//!   **∴ 我**原来把这份服务**写在 `/tmp/serve_web.mjs`**✗
//!     ⇒ **∴ 而 `/tmp` 是 **tmpfs**✗ ⇒ **∴ 一重启**就没了 ✓**
//!       ⇒ **∴ 于是**：**判据**加载不到页面**✗ ⇒ **∴ 报出**看不懂的
//!         "**`yanshiGpuQuantize=false`**"**✗（**∴ 真原因**是服务不在 ✓）** ✓✓
//!   ⇒ **∴ 所以**：**辅助脚本**进仓 ✓ ＋ **临时资源**自己收拾 ✓**** ✓✓
//!
//! **∴ 用法 ✗**：`node scripts/serve-web.mjs [端口] [web 目录]`
//!   **∴ 默认 ✗**：**端口 8899｜目录 `web/`** ✓
//!   **∴ MIME 要点 ✗**：**`.wasm` 必须是 `application/wasm`**✗
//!     （**∴ 否则**浏览器**拒绝**流式编译 ✓）** ✓✓
//!
//! **∴ 安全 ✗**：**只发布 `web/` 下的文件**✗ ⇒ **∴ 拒绝**任何**越界路径** ✓**** ✓✓

import { createServer } from "node:http";
import { readFileSync, existsSync, statSync } from "node:fs";
import { extname, join, normalize, resolve } from "node:path";

const port = Number(process.argv[2] || 8899);
const root = resolve(process.argv[3] || "web");

const TYPES = {
  ".js": "text/javascript",
  ".mjs": "text/javascript",
  ".css": "text/css",
  ".html": "text/html; charset=utf-8",
  ".json": "application/json",
  ".wasm": "application/wasm",
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".webp": "image/webp",
  ".svg": "image/svg+xml",
  ".ico": "image/x-icon",
  ".woff2": "font/woff2",
  ".myb": "application/octet-stream",
};

const server = createServer((request, response) => {
  let pathname = "/";
  try {
    pathname = decodeURIComponent(String(request.url || "/").split("?")[0]);
  } catch {
    response.writeHead(400).end("bad path");
    return;
  }
  if (pathname.endsWith("/")) pathname += "index.html";
  const file = resolve(join(root, normalize(pathname)));
  // **∴ 越界检查 ✗**：**解析后的绝对路径**必须**仍在 root 下** ✓**** ✓✓
  if (!file.startsWith(root) || !existsSync(file) || !statSync(file).isFile()) {
    response.writeHead(404, { "content-type": "text/plain; charset=utf-8" });
    response.end("not found");
    return;
  }
  response.writeHead(200, {
    "content-type": TYPES[extname(file)] || "application/octet-stream",
    // **∴ 判据**不希望被缓存**✗（**∴ 我**们自己**管 SW 的戳 ✓）** ✓✓
    "cache-control": "no-store",
  });
  response.end(readFileSync(file));
});

server.listen(port, "127.0.0.1", () => {
  console.log(`  web 服务：http://127.0.0.1:${port}（**根目录 ${root} ✓）`);
});
