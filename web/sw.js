// **★ Service Worker ✓ ★**（第 608 轮 ✓；**部署矩阵 §7.4 验收 ②③ ✓**）
// **∴ 目标 ✗**：**断网后仍能打开并绘制 ✓** ⇒ **∴ 采用**缓存优先 ＋ 后台更新**✗**（**stale-while-revalidate ✓**）。
// **∴ 注意 ✗**：**内核 wasm 与 JS 胶水**必须被缓存 ✓（**否则离线打不开 ✓**）。
const CACHE = "yanshi-online-v1";
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
        if (res && res.ok) caches.open(CACHE).then((c) => c.put(request, res.clone()));
        return res;
      }).catch(() => hit);
      return hit || live;
    })
  );
});
