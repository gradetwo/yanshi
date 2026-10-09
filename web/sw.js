// **★ Service Worker ✓ ★**（第 608 轮 ✓；**部署矩阵 §7.4 验收 ②③ ✓**）
// **∴ 目标 ✗**：**断网后仍能打开并绘制 ✓** ⇒ **∴ 采用**缓存优先 ＋ 后台更新**✗**（**stale-while-revalidate ✓**）。
// **∴ 注意 ✗**：**内核 wasm 与 JS 胶水**必须被缓存 ✓（**否则离线打不开 ✓**）。
// The cache name must change whenever the assets change, otherwise entries survive
// every deployment (the name was a fixed string, so stale files were served forever).
// It is derived from a build stamp that the sync step rewrites, which also means the
// old cache is dropped by the cleanup below instead of lingering next to the new one.
const CACHE = "yanshi-online-1791559526350";
const CORE = ["/", "/index.html", "/manifest.webmanifest",
              "/wasm/yanshi_wasm.js", "/wasm/yanshi_wasm_bg.wasm"];

self.addEventListener("install", (event) => {
  event.waitUntil(caches.open(CACHE).then((c) => c.addAll(CORE)).then(() => self.skipWaiting()));
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches.keys().then((keys) =>
      Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k)))
    ).then(() => self.clients.claim())
  );
});

self.addEventListener("fetch", (event) => {
  const { request } = event;
  if (request.method !== "GET") return;
  event.respondWith(
    caches.match(request).then((hit) => {
      const live = fetch(request).then((res) => {
        // **★ `clone()` 必须**同步**做 ✗ ★**（用户报告：`sw.js:30 Uncaught (in promise)
        // TypeError: Failed to execute 'clone' on 'Response': Response body is already used` ✓）：
        //   **∴ 原来的错 ✗**：**`caches.open(...).then(() => res.clone())`** 是**异步**的 ✓
        //     ⇒ **∴ 那个回调**要等 `caches.open` 的 promise ✓ ⇒ **∴ 而 `res`** 早已
        //       被**返回给页面**、**body 被读掉** ✗ ⇒ **∴ `clone()` 抛 "**body is already used**" ✓**** ✓✓
        //   **∴ 修法 ✗**：**在**返回 `res` **之前同步克隆一份 ✗** ⇒ **∴ 之后**用**那份副本**写缓存 ✓
        //     ⇒ **∴ 两者**各自**消费自己的 body ✓**** ✓✓
        //   **∴ 且**把写缓存的拒绝**吃掉 ✗**（**`catch` ✓）—— **∴ 否则**它**又是一个
        //     **未处理的 promise 拒绝 ✗** ⇒ **∴ 又会在控制台报错 ✓**** ✓✓
        if (res && res.ok) {
          const copy = res.clone();
          caches.open(CACHE).then((c) => c.put(request, copy)).catch(() => {});
        }
        return res;
      }).catch(() => hit);
      return hit || live;
    })
  );
});
