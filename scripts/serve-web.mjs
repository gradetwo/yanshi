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
import { basename, extname, join, normalize, resolve } from "node:path";

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
  // **★ 静态服务也要有 `/health` ✗ ★**（第 83 轮 ✓；**CI 的真根因换来的 ✓）：
  //   **∴ 为什么 ✗**：**`browser-*` 判据**会**先查 `/health`**✗
  //     ⇒ **∴ 而**静态服务**原来**回 **404** ✗
  //       ⇒ **∴ 于是**：**判据**判"**内核未就绪 ✓" ⇒ **∴ 红 ✓**** ✓✓
  //     **∴ 实测（**CI 日志 ✓）✗**：`browser-kernel-perf` 报
  //       "**内核未就绪（**服务端 `/health` 的 wasm 是否为 true？✓"** ✓**** ✓✓
  //   **∴ 如实的内容 ✗**：**静态部署**确实**有 wasm 产物**（**`/wasm/*.js` ⇒ 200 ✓）
  //     ⇒ **∴ `wasm: true`**✗；**而**它**没有服务端**✗ ⇒ **∴ `server: false`** ✓**** ✓✓
  //     ⇒ **∴ 于是**：**判据**看到的**和线上一致 ✓**** ✓✓
  // **∴ 参数名是 `response`**✗，**不是 `res`** ✗**（第 83 轮 ✓；**∴ 我**踩到了 ✓）：
  //   **∴ 症状 ✗**：**我**写 `res.writeHead`**✗ ⇒ **∴ 抛** ReferenceError**✗
  //     ⇒ **∴ 被**外层 catch**吃掉**✗ ⇒ **∴ 于是**回 **"not found"** ✓**** ✓✓
  if (pathname === "/health") {
    response.writeHead(200, { "content-type": "application/json; charset=utf-8",
      "cache-control": "no-store" });
    response.end(JSON.stringify({ ok: true, server: false, wasm: true,
      backend: "cpu", render_backend: "cpu",
      gpu_mode: "off", gpu_unavailable_reason: "static_deployment_has_no_server",
      max_channel_delta: null,
      max_channel_delta_note: "静态部署没有服务端 ⇒ 未做 GPU／CPU 比对（§6.3）" }));
    return;
  }
  // **★ favicon 三条路由 ✗ ★**（第 65 轮 ✓；**CI 的 404 换来的 ✓）：
  //   **∴ `web/index.html`**引用了** `/favicon.svg` 与 `/favicon.png`**✗
  //     ⇒ **∴ 而 `web/` 里**没有**它们 ✓（**∴ 线上**由**服务端路由**提供 ✓）
  //       ⇒ **∴ 于是**：**本地静态服务**会 **404** ✗
  //         ⇒ **∴ 判据**报"**有 4xx／5xx ✓" ⇒ **∴ 失败 ✓**** ✓✓
  //   **∴ 映射**照抄 `server.rs:710-713`**✗ ⇒ **∴ 于是**本地与线上**行为一致 ✓**** ✓✓
  const FAVICON = {
    "/favicon.svg": "brand/svg/icon-light.svg",
    "/favicon.png": "brand/png/favicon-32.png",
    "/favicon.ico": "brand/png/favicon.ico",
  };
  if (FAVICON[pathname]) pathname = "/" + FAVICON[pathname];
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
// **★ 静态部署必须**替换 SW 的构建占位符** ✗ ★**（第 165 轮 ✓；**离线旧页面换来的 ✓）：
    //   **∴ 症状 ✗**：**静态部署**里**缓存名**恒为 `yanshi-shell-__BUILD_ID__`**✗
    //     ⇒ **∴ 因为**本服务器**从不**替换它** ✓（**实测：**出现 0 次 ✓）** ✓✓
    //     ⇒ **★ 于是**：**SW 缓存**永不失效** ✗**
    //       ⇒ **∴ 用户**离线时**永远**拿到**第一次那份** ✗**** ✓✓
    //       ⇒ **∴ 老用户**要**手工清缓存**才能更新** ✓ ★**** ✓✓
    //   **∴ 而**真实的服务端**早就**替换它**✗（**`BUILD_ID_TEXT` ✓）
    //     ⇒ **∴ 所以**本修法**只是**让**静态部署与它对齐** ✓**** ✓✓
    //   **∴ 做法**：**用** `viewer-app.js` **的 mtime** 当构建戳**✗
    //     ⇒ **∴ 于是**：**每次部署**（**文件变了 ✓）**缓存名**都变** ✓
    //       ＋ **∴ 同一个构建内**恒定**（**∴ 不**反复失效 ✓）** ✓✓
    const body = readFileSync(file);
    if (basename(file) === "service-worker.js") {
      let stamp = "0";
      try { stamp = String(Math.floor(statSync(join(root, "viewer-app.js")).mtimeMs)); } catch { /* 用默认 */ }
      response.end(Buffer.from(body.toString("utf8").replaceAll("__BUILD_ID__", stamp), "utf8"));
      return;
    }
    response.end(body);
});

server.listen(port, "127.0.0.1", () => {
  console.log(`  web 服务：http://127.0.0.1:${port}（**根目录 ${root} ✓）`);
});
