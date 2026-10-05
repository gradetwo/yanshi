// **外壳清单** ✓ —— 第 213 轮补上**共享内核**两件 ✓：
// 退休前是**查看器自己**用 `cache.put("/brush-module.wasm", …)` 把门面塞进来的 ✓，
// 而那行随门面一起被删 ✗ ⇒ 于是**没人**再把内核放进 SW 缓存 ✗ ⇒
// **离线时内核拿不到** ✗（`cachedUrls` 里没有它 ✓ —— 判据当场把这件事量了出来 ✓）。
const SHELL = [
  "/",
  "/favicon.svg",
  "/brand/svg/icon-light.svg",
  "/brush-previews/index.json",
  "/wasm/yanshi_wasm.js",
  "/wasm/yanshi_wasm_bg.wasm",
  // **(A)①：拆出来的静态资产也要预缓存** ✓ —— 否则离线打开时**样式与脚本拿不到** ✗
  //（此前它们内嵌在页面里 ⇒ 只需缓存 `/` ✓；拆出后必须显式列出 ✓）。
  "/viewer.css",
  "/viewer-app.js",
  // **⚠️ 不把 `/service-worker.js` 放进 SHELL** ✗ —— 浏览器按自己的节奏**重新抓取 SW 来检查更新** ✓，
  // 缓存它反而可能**把旧版本钉住** ✗（那正是 (A)⑥「SW 升级不脏读」要防的 ✓）。
];
// **缓存名里带上构建标识** ✓（(A)⑥「SW 升级不脏读」的正主 ✓）：
// 名字一变 ⇒ 下面那句"删掉所有名字不同的缓存"✓ 就自动作废**整份旧外壳** ✓
// ⇒ 这正是第 210 轮查到的真因 ✓（旧 js + 新 wasm ⇒ 内核预览失败 ✓）。
const CACHE = "yanshi-shell-__BUILD_ID__";
self.addEventListener("install", (event) => {
  event.waitUntil((async () => {
    const cache = await caches.open(CACHE);
    await Promise.all(SHELL.map((url) => cache.add(url).catch(() => undefined)));
    await self.skipWaiting();
  })());
});
self.addEventListener("activate", (event) => {
  event.waitUntil((async () => {
    const names = await caches.keys();
    await Promise.all(names.filter((name) => name !== CACHE).map((name) => caches.delete(name)));
    await self.clients.claim();
  })());
});
self.addEventListener("fetch", (event) => {
  const request = event.request;
  if (request.method !== "GET") return;
  const url = new URL(request.url);
  if (url.origin !== self.location.origin) return;
  // **blob 是"按内容哈希命名"的不可变资源** ✓ ⇒ **cache-first** ✓（行业口径 ✓，与 (A)⑥ 同一条原则 ✓）。
  // **为什么放在这里** ✓：查看器里所有 blob 都出自 `const blobUrl = (hash) => api("/api/blob/" + hash)` ✓
  // ⇒ 有的是 `fetch` ✓、有的是 **`<img src>`** ✗（后者**根本不经过我的 `fetchOrLocal`** ✓
  // ⇒ 这就是"离线时那条 blob 一直失败 ✓、而且我加的写失败警告一条都不打"✓ 的原因 ✓）。
  // ⇒ 交给 SW 做，**一处覆盖全部** ✓，不必去追十几个 `<img>` 赋值点 ✗。
  if (url.pathname.startsWith("/api/blob/")) {
    event.respondWith((async () => {
      const cache = await caches.open(CACHE);
      const hit = await cache.match(request, { ignoreSearch: true });
      if (hit) return hit;
      try {
        const response = await fetch(request);
        if (response && response.ok) cache.put(request, response.clone()).catch(() => undefined);
        return response;
      } catch (error) {
        const fallback = await cache.match(request, { ignoreSearch: true });
        if (fallback) return fallback;
        throw error;
      }
    })());
    return;
  }
  if (url.pathname.startsWith("/api/") || url.pathname.startsWith("/ws")) return;
  event.respondWith((async () => {
    try {
      const response = await fetch(request);
      if (response && response.ok && request.mode === "navigate") {
        const cache = await caches.open(CACHE);
        cache.put("/", response.clone()).catch(() => undefined);
      }
      return response;
    } catch (error) {
      const cached = await caches.match(request, { ignoreSearch: true });
      if (cached) return cached;
      const shell = await caches.match("/");
      if (shell) return shell;
      throw error;
    }
  })());
});
