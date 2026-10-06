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
  // **(A)②：笔刷定义随包** ✓ —— 实测 `assets/brushes` 总共约 **1.9 MB**（201 支 ✓）⇒
  // 一次性 install 预取完全可接受 ✓（**离线作画的前提：面板之外至少要有笔可用 ✓**）。
  // **名字里的特殊字符必须逐段编码**（与 `viewer-app.js` 的 `brushAssetUrl` 同一把尺子）：
  // `#` 是 URL 的片段起点 ⇒ 字面写它等于把后缀丢掉；`%` / `+` 是同类。
  // 编码后才是浏览器与查看器真正请求的那个 URL ⇒ 离线回退（按 request.url 查缓存）才命中。
  "/brushes/100%25_Opaque.myb",
  "/brushes/1pixel.myb",
  "/brushes/2B_pencil.myb",
  "/brushes/4H_pencil.myb",
  "/brushes/8B_Pencil%231.myb",
  "/brushes/Airbrush_a.myb",
  "/brushes/B-pencil.myb",
  "/brushes/Beamlight.myb",
  "/brushes/BigAirbrush.myb",
  "/brushes/Blender.myb",
  "/brushes/Blur_Fast.myb",
  "/brushes/Classic_Paint.myb",
  "/brushes/Clouds.myb",
  "/brushes/DNA_brush.myb",
  "/brushes/Delayed_.myb",
  "/brushes/Dirty_Noise.myb",
  "/brushes/Dirty_Transparent_sk.myb",
  "/brushes/Dissolver.myb",
  "/brushes/Eraser.myb",
  "/brushes/Fan%231.myb",
  "/brushes/Flat2%231.myb",
  "/brushes/Flight_Feathers.myb",
  "/brushes/Fount-offset%231.myb",
  "/brushes/Fountain_SF%231.myb",
  "/brushes/Glow_Airbrush.myb",
  "/brushes/Grain.myb",
  "/brushes/HalfTone%231.myb",
  "/brushes/HalfToneCMY%231.myb",
  "/brushes/Hard_Eraser.myb",
  "/brushes/Marker.myb",
  "/brushes/P-Shade.myb",
  "/brushes/Pastel_1.myb",
  "/brushes/PenBrush.myb",
  "/brushes/Pencil-_Left_Handed.myb",
  "/brushes/Posterizer.myb",
  "/brushes/RS_blendOP.myb",
  "/brushes/Round%231.myb",
  "/brushes/Round.myb",
  "/brushes/Round_Bl.myb",
  "/brushes/Sketch_1.myb",
  "/brushes/Sketcher2_sk.myb",
  "/brushes/Smear.myb",
  "/brushes/Smear_sm.myb",
  "/brushes/Soft_Eraser.myb",
  "/brushes/Splash.myb",
  "/brushes/Tail_Feathers.myb",
  "/brushes/Tail_Feathers2.myb",
  "/brushes/Thin_Pen.myb",
  "/brushes/WateryFlatbrush.myb",
  "/brushes/Wet_Direction.myb",
  "/brushes/acrylic-03-only-water.myb",
  "/brushes/acrylic-03-paint.myb",
  "/brushes/acrylic-03-with-water.myb",
  "/brushes/acrylic-04-only-water.myb",
  "/brushes/acrylic-04-paint.myb",
  "/brushes/acrylic-04-with-water.myb",
  "/brushes/acrylic-05-only-water.myb",
  "/brushes/acrylic-05-paint.myb",
  "/brushes/acrylic-05-with-water.myb",
  "/brushes/airbruch_press_a.myb",
  "/brushes/airbrush.myb",
  "/brushes/airsmudge_a.myb",
  "/brushes/airsmudgeultimate_sk.myb",
  "/brushes/arrow%231.myb",
  "/brushes/ballpen.myb",
  "/brushes/basic.myb",
  "/brushes/basic_digital_brush.myb",
  "/brushes/basic_digital_brush_smudging.myb",
  "/brushes/basic_digital_knife.myb",
  "/brushes/basic_digital_knife_smudging.myb",
  "/brushes/blend%2Bpaint.myb",
  "/brushes/blending.myb",
  "/brushes/blending_knife.myb",
  "/brushes/blur.myb",
  "/brushes/brush.myb",
  "/brushes/brushkit-pen.myb",
  "/brushes/bubble.myb",
  "/brushes/bulk.myb",
  "/brushes/calligraphy.myb",
  "/brushes/chalk.myb",
  "/brushes/charcoal-01.myb",
  "/brushes/charcoal-03.myb",
  "/brushes/charcoal-04.myb",
  "/brushes/charcoal-blur1.myb",
  "/brushes/charcoal.myb",
  "/brushes/classic-brush.myb",
  "/brushes/classic-knife.myb",
  "/brushes/classic-pen.myb",
  "/brushes/classic_sk.myb",
  "/brushes/classicroundblock_static_c.myb",
  "/brushes/coarse_bulk_1.myb",
  "/brushes/coarse_bulk_2.myb",
  "/brushes/coarse_bulk_3.myb",
  "/brushes/deevad-2B_pencil.myb",
  "/brushes/deevad-brush.myb",
  "/brushes/deevad-pen.myb",
  "/brushes/detail_brush_large.myb",
  "/brushes/detail_brush_large_glazing.myb",
  "/brushes/detail_brush_thin.myb",
  "/brushes/detail_brush_thin_glazing.myb",
  "/brushes/dry_brush.myb",
  "/brushes/extreme_round_l.myb",
  "/brushes/fill.myb",
  "/brushes/fill_c.myb",
  "/brushes/flat_bar_l.myb",
  "/brushes/fur.myb",
  "/brushes/glow.myb",
  "/brushes/grainy_blending.myb",
  "/brushes/hard_blot.myb",
  "/brushes/hard_sting.myb",
  "/brushes/imp_blending.myb",
  "/brushes/imp_details.myb",
  "/brushes/impressionism.myb",
  "/brushes/ink-slowline_s.myb",
  "/brushes/ink_blot.myb",
  "/brushes/ink_eraser.myb",
  "/brushes/ink_slow_s.myb",
  "/brushes/inkster_l.myb",
  "/brushes/irregular_ink.myb",
  "/brushes/kabura.myb",
  "/brushes/kneaded_eraser.myb",
  "/brushes/kneaded_eraser_large.myb",
  "/brushes/large_hard_eraser.myb",
  "/brushes/large_watercolor_fringe.myb",
  "/brushes/leaves.myb",
  "/brushes/liner.myb",
  "/brushes/long_grass.myb",
  "/brushes/marker-01.myb",
  "/brushes/marker-05.myb",
  "/brushes/marker_fat.myb",
  "/brushes/marker_small.myb",
  "/brushes/modelling.myb",
  "/brushes/modelling2.myb",
  "/brushes/oil-01-clean.myb",
  "/brushes/oil-01-paint.myb",
  "/brushes/oil-03-clean.myb",
  "/brushes/oil-03-paint.myb",
  "/brushes/oil-06-clean.myb",
  "/brushes/oil-06-paint.myb",
  "/brushes/oil-mop.myb",
  "/brushes/only_water_fringe.myb",
  "/brushes/paint_barrr_sm.myb",
  "/brushes/paint_radius_2_sm.myb",
  "/brushes/paint_sm.myb",
  "/brushes/particules_3.myb",
  "/brushes/particules_eraser.myb",
  "/brushes/pen-note.myb",
  "/brushes/pencil-2b.myb",
  "/brushes/pencil-8b.myb",
  "/brushes/pencil.myb",
  "/brushes/pick_and_drag.myb",
  "/brushes/pixel_hardink.myb",
  "/brushes/pixelblocking.myb",
  "/brushes/pointy_ink.myb",
  "/brushes/puantilism.myb",
  "/brushes/puantilism2.myb",
  "/brushes/ramon-2B_pencil.myb",
  "/brushes/ramon-Knife.myb",
  "/brushes/ramon-Pen.myb",
  "/brushes/rigger_brush.myb",
  "/brushes/rigger_brush_thin.myb",
  "/brushes/rough.myb",
  "/brushes/rounded.myb",
  "/brushes/sewing.myb",
  "/brushes/short_grass.myb",
  "/brushes/slow_ink.myb",
  "/brushes/small_blot.myb",
  "/brushes/smudge%2Bpaint.myb",
  "/brushes/smudge.myb",
  "/brushes/smudge_ink(0.7)_sm.myb",
  "/brushes/soft-dip-pen.myb",
  "/brushes/soft.myb",
  "/brushes/soft_irregular.myb",
  "/brushes/spaced-blot.myb",
  "/brushes/speed_blot.myb",
  "/brushes/splatter-02.myb",
  "/brushes/splatter-04.myb",
  "/brushes/sponge_smudging.myb",
  "/brushes/spray.myb",
  "/brushes/spray2.myb",
  "/brushes/subtle_pencil.myb",
  "/brushes/texture-03.myb",
  "/brushes/texture-06.myb",
  "/brushes/texture-12.myb",
  "/brushes/textured_ink.myb",
  "/brushes/thin_hard_eraser.myb",
  "/brushes/thin_watercolor.myb",
  "/brushes/track.myb",
  "/brushes/water-01.myb",
  "/brushes/water-02.myb",
  "/brushes/water-05.myb",
  "/brushes/water-06.myb",
  "/brushes/watercolor-02-paint.myb",
  "/brushes/watercolor-02-water.myb",
  "/brushes/watercolor_expressive.myb",
  "/brushes/watercolor_glazing.myb",
  "/brushes/wet_knife.myb",
  "/brushes/wet_paint_sm.myb",
  "/brushes/wet_round.myb",
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
  // **`/service-worker.js` 永不入缓存** ✗（与上面 SHELL 里那条注释同一条理由 ✓）：
  // 缓存它可能**把旧版本钉住** ✗ —— 而"新 SW 才作废旧缓存"正是 (A)⑥ 的机制 ✓。
  const neverCache = url.pathname === "/service-worker.js";
  event.respondWith((async () => {
    try {
      const response = await fetch(request);
      // **(A)②：同源静态资产"用到就存"** ✓（cache on first use ✓）——**为什么不全量预缓存** ✗：
      // 未预缓存的那几类可路由资产合计 **≈ 19.0 MiB**（实测：textures 12,601,760 ✓ ＋
      // samples 5,106,710 ✓ ＋ brush-previews 1,896,972 ✓ ＋ brand 159,817 ✓ ＋ mediums 159,873 ✓）
      // ⇒ 塞进 `SHELL` 等于**每一次 install** 都拖这 19 MiB ✗；而目标原文是"**纹理按需**"✓。
      // ⇒ 改成"**在线第一趟照常走网络、用过的才留下**"✓：攒够一趟之后断网才由缓存兜底 ✓。
      //
      // **为什么写进 `CACHE`（名字里带构建标识 ✓）而不是另开一个资产缓存** ✗：
      // ① `activate` 会**删掉所有名字不等于 `CACHE` 的缓存** ✓ ⇒ 另开一个必然在每次升级时被删 ✗；
      // ② 写进带标识的 `CACHE` ⇒ **换构建 ⇒ 整份旧资产一起作废** ✓
      //    ⇒ 不会拿旧贴图 / 旧笔刷预览去配新页面 ✓（这正是 (A)⑥"升级不脏读"要的那条 ✓）。
      // **同一份缓存仍不脏读** ✓：本分支**始终网络优先** ✓ ⇒ 构建标识不变时能上网就拿新的 ✓。
      // **只存 200/2xx 的完整响应** ✗：`206`（Range 部分响应 ✓）存下来会变成一个坏条目 ✓。
      if (response && response.ok && !neverCache && response.status !== 206) {
        const cache = await caches.open(CACHE);
        if (request.mode === "navigate") {
          // 导航响应按外壳的键 `/` 存 ✓（离线打开页面的先决条件 ✓，与此前行为一致 ✓）。
          cache.put("/", response.clone()).catch(() => undefined);
        } else {
          // 静态资产按**原请求**存 ✓ —— 与下面 `caches.match(request, …)` 的查法**对称** ✗
          //（存 `url.pathname` 会丢掉查询串 ⇒ 存了也命中不了 ✓）。
          cache.put(request, response.clone()).catch(() => undefined);
        }
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
