#!/usr/bin/env node
// **★ PWA 静态资源完整性 ✓ ★**（第 609 轮 ✓；**部署矩阵 §7.4／§9 ✓**）
//
// **它守什么** ✓（**∴ 无需网络、无需 wrangler、无需浏览器 ✓**）：
//   **① 内核产物必须在 PWA 目录里且非空 ✓** —— **∴ 否则 `wrangler deploy` 会发布一个
//     **没有内核**的 PWA ✗ ⇒ **∴ 用户看到的是"假装能画"✗** ⇒ **∴ 这条是**防撒谎**判据 ✓**；
//   **② `wrangler.toml` 必须存在 ＋ 含用户指定的域名 ✗**（`yanshi-online.wangda.today` ✓）；
//   **③ `wrangler.toml` 的静态目录必须指向 `web/` ✗**（**否则托管的不是这套 PWA ✓**）；
//   **④ `web/index.html` ＋ `manifest.webmanifest` ＋ `sw.js` 必须存在 ✓**；
//   **⑤ service worker 必须把 **wasm 与 JS 胶水**列入预缓存 ✗**（**否则离线打不开 ✓**）；
//   **⑥ `manifest` 必须是合法 JSON ＋ 含 `start_url` ✓**。
//
// **变异** ✗：**删掉 `web/wasm/yanshi_wasm_bg.wasm`** ⇒ **∴ ① 报红 ✓**；
//   **把域名从 `wrangler.toml` 去掉** ⇒ **∴ ② 报红 ✓**。
import { existsSync, readFileSync, statSync } from "node:fs";

const bad = [];
const read = (p) => (existsSync(p) ? readFileSync(p, "utf8") : null);
const check = (cond, msg) => { if (!cond) bad.push(msg); };

// **① 内核产物 ✓**
const wasm = "web/wasm/yanshi_wasm_bg.wasm";
const glue = "web/wasm/yanshi_wasm.js";
check(existsSync(wasm), `内核 wasm 缺失：${wasm} ✗（跑 node scripts/pwa-sync-wasm.mjs ✓）`);
check(existsSync(glue), `内核 JS 胶水缺失：${glue} ✗`);
if (existsSync(wasm)) {
  const size = statSync(wasm).size;
  check(size > 0, `${wasm} 为空 ✗`);
  console.log(`  内核 wasm：${(size / 1048576).toFixed(2)} MiB ✓`);
}

// **②③ wrangler.toml ✓**
const wrangler = read("wrangler.toml");
check(!!wrangler, "wrangler.toml 缺失 ✗");
if (wrangler) {
  check(/yanshi-online\.wangda\.today/.test(wrangler), "wrangler.toml 未含域名 yanshi-online.wangda.today ✗");
  check(/directory\s*=\s*"\.\/web"/.test(wrangler), "wrangler.toml 的静态目录未指向 ./web ✗");
  check(/main\s*=\s*"/.test(wrangler), "wrangler.toml 未指定 main ✗");
  console.log("  wrangler.toml：域名 ✓｜静态目录 ✓｜main ✓");
}

// **④ PWA 三件套 ✓**
for (const f of ["web/index.html", "web/manifest.webmanifest", "web/sw.js"]) {
  check(existsSync(f), `PWA 文件缺失：${f} ✗`);
}

// **⑤ service worker 预缓存必须含 wasm 与胶水 ✓**
const sw = read("web/sw.js");
if (sw) {
  check(/yanshi_wasm_bg\.wasm/.test(sw), "sw.js 未把内核 wasm 列入预缓存 ⇒ 离线打不开 ✗");
  check(/yanshi_wasm\.js/.test(sw), "sw.js 未把内核 JS 胶水列入预缓存 ⇒ 离线打不开 ✗");
  console.log("  sw.js：wasm ＋ 胶水 已入预缓存 ✓");
}

// **⑥ manifest 合法性 ✓**
const mf = read("web/manifest.webmanifest");
if (mf) {
  try {
    const m = JSON.parse(mf);
    check(typeof m.start_url === "string" && m.start_url.length > 0, "manifest 缺 start_url ✗");
    console.log(`  manifest：name=${m.name} ｜ display=${m.display} ✓`);
  } catch (e) {
    bad.push(`manifest 不是合法 JSON ✗：${String(e).slice(0, 60)}`);
  }
}

// **∴ 另外确认"没有服务端依赖"这条设计意图 ✓**（**用户澄清 ✓**）：
const html = read("web/index.html");
if (html) {
  check(!/\/api\/tools\//.test(html), "index.html 里出现服务端 API 调用 ⇒ 与该部署「无服务器」的设计不符 ✗");
  console.log("  index.html：无服务端 API 调用 ✓（纯离线客户端 ✓）");
}

if (bad.length) {
  console.error("❌ " + bad.join("｜"));
  process.exit(1);
}
console.log("  ✓ PWA 资源完整：内核在、域名对、预缓存含内核、无服务端依赖 ✓");
