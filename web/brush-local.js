// **★ 本地落笔（走内核）✓ ★**（第 652 轮 ✓；**部署矩阵 §14.30 的六步路线 ②③④ ✓**）
//
// **★ 为什么要单独一个文件 ✗ ★**：**它是**纯新增**✗**（**不改任何现有路径 ✓**）
//   ⇒ **∴ 风险为零 ✓** ⇒ **∴ 而**下一轮只需在 `brush_stroke` 里调它 ✓。
//
// **★ 为什么必须走内核 ✗ ★**（**第 646 轮实测 ✓**）：
//   **∴ 服务端的笔触**不是 `brush_stroke` 原子 ✗**，**而是**服务端渲染成位图后的
//   `import_image` 原子 ✗**（**`bitmap.blob_hash` ＋ `image/x-yanshi-raw` ✓**）
//   ⇒ **∴ 我上一轮自造 `brush_stroke` 原子 ⇒ **∴ 内核不认 ⇒ **∴ 渲染完全不变 ✗****。
//
// **∴ 内核契约 ✓**（`crates/yanshi-wasm/src/brush.rs:59` ✓）：
//   **`PaintRequest { myb, points, size, color, opacity, hardness, region, base }`** ✓
//   其中 **`myb` 是**笔刷文件全文**✗**（**∴ 所以要先 `fetch` 它 ✓**）
//   且 **`region` 是**要读回的文档区域**✗**（**输出 ＝ 这块的 `w*h*4` 字节 ✓**）
//   而 **`base` 是**底图**✗**（**涂抹／混合类**必需 ✓**）。
//
// **∴ 本函数只做**取笔刷 ＋ 调内核 ＋ 报错 ✗**；**∴ 存 blob ＋ 写 `import_image` 原子**留给调用方 ✓**
//   （**∴ 因为**那需要文档尺寸／层信息，属于调用方的上下文 ✓**）。

/**
 * **★ 取笔刷文件全文 ✓ ★**（**∴ 笔刷名要 URL 编码 ✗** —— **`#` ⇒ `%23` ✓**，
 * 见 `crates/yanshi-http/src/server.rs:486-500` 的说明 ✓）。
 * @param {string} brush **如 `100%_Opaque.myb` 或 `8B_Pencil#1.myb` ✓**
 */
// **★ 笔刷文本内存缓存 ✓**（PWA spray 测试 ✓）：
//   **∴ 症状 ✗**：每画一笔就 GET 一次 `/brushes/spray.myb` ✓
//   ⇒ **∴ 而**笔刷文件是静态的 ✓、**一笔内**不会变 ✓
//   ⇒ **∴ 所以**内存里留一份 ✓，**同名**直接返回 ✓（**页面级**缓存 ✓，刷新重取 ✓）。
const brushTextCache = new Map();

export async function fetchBrushText(brush) {
  const name = String(brush || "100%_Opaque.myb");
  // **★ 内存缓存命中 ⇒ 不发请求 ✓**
  if (brushTextCache.has(name)) return brushTextCache.get(name);
  // **★★ 必须用 `encodeURIComponent` ✗ ★★**（第 408 轮 ✓；**决定性实测 ✓）：
  //   **∴ 原来 ✗**：**`name.replace(/%/g, "_pct_").replace(/#/g, "_n_")`** ✓
  //     ＋ **∴ 注释说**「**与服务端用**路径版解码器一致**」**✗
  //       ＋ **∴ 那段英文注释还说 `%`／`#` **「**could not be addressed reliably through
  //         the static host**」 ✓
  //   **★★★ 而实测（**同一台服务端 ✓）★★★**：
  //     ```
  //     ★ 原始名(未编码)      : HTTP 200 ★
  //     ★ %25 编码           : HTTP 200 ★   ← 静态托管**完全能**处理 %25
  //     ★ _pct_ 替换(现在的)  : HTTP 404 ★   ← 唯一失败的那个
  //     ```
  //     **⇒ ★ 所以 ✗ ★**：**那段注释**是错的／过时的** ✓
  //       ＋ **∴ 而**「**路径版解码器**」**不存在**（**∴ 请求 404 证明 ✓）** ✓
  //         ⇒ **∴ 于是**：**磁盘上是 `100%_Opaque.myb`**✗ 而**请求 `/brushes/100_pct__Opaque.myb`** ✓
  //           ⇒ **∴ 服务端**404**✗ ⇒ **∴ 落笔失败 ＋ **笔被丢** ✓ ★**** ✓✓
  //   **∴ 铁证（**离线的另一半 ✓）★**：**`service-worker.js` 的 `SHELL` 里写的是
  //     `"/brushes/100%25_Opaque.myb"`**✗（**用 `%25` 编码 ✓）
  //     ⇒ **∴ 所以**：**原来**两处名字**不同**✗
  //       ⇒ **∴ 于是**离线**也**命中不了缓存** ✓
  //         ⇒ **★ 改用 `encodeURIComponent` ✗** ⇒ **∴ 一次修好**在线 ＋ 离线** ✓ ★**** ✓✓
  //   **∴ 两面 ✗**：**收益**：**在线 200 ＋ 离线命中** ✓
  //     ＋ **∴ 代价 ✗**：**无**✗（**∴ `encodeURIComponent` **是标准做法** ＋
  //       **∴ 且**它**与** SW 清单**已经一致** ✓）** ★**** ✓✓
  const encoded = encodeURIComponent(name);
  const res = await fetch("/brushes/" + encoded);
  if (!res.ok) {
    // Include the exact URL that failed: the reported name is the caller's input, while
    // the request uses the encoded form, and without the url we cannot tell them apart.
    throw new Error("笔刷取不到：" + name + " ⇒ HTTP " + res.status +
                    "｜请求 URL ＝ " + String(res.url || "(未知)") +
                    "｜base ＝ " + String(location.href));
  }
  const text = await res.text();
  // **★ 防御：SPA fallback 会返回 200 + HTML**（本地测试验证 ✓）：
  //   `/brushes/*.myb` 缺失时，静态托管的 SPA fallback 返回 `index.html`
  //   （HTTP 200，`res.ok` 为真）⇒ 若不检查，HTML 会被当成笔刷文本
  //   ⇒ `hokusai::myb::from_str` 解析失败 ⇒ "内核未产出像素"。
  //   笔刷文件是 JSON（`{"comment": "MyPaint brush file", ...}`），首字符不是 `{` 就一定是拿错了。
  const trimmed = text.trimStart();
  if (!trimmed.startsWith("{")) {
    throw new Error("笔刷取到错误内容：" + name + " ⇒ 首字符為 " +
                    JSON.stringify(trimmed.slice(0, 60)) +
                    "（期望 JSON 笔刷定义，可能是 SPA fallback 的 HTML）" +
                    "｜请求 URL ＝ " + String(res.url || "(未知)"));
  }
  // **★ 写内存缓存 ✓**
  brushTextCache.set(name, text);
  return text;
}

/**
 * **★ 用内核渲染一笔 ⇒ 返回 RGBA8 ✗ ★**（**∴ 失败时**抛出可读原因 ✗，**绝不静默 ✓**）。
 * @param {object} kernel **已初始化的 `WasmKernel` ✓**（**∴ 调用方负责 `await mod.default()` ✓**）
 * @param {{myb:string, points:Array, size?:number, color?:object, region:{x:number,y:number,w:number,h:number}, base?:object}} req
 * @returns {Uint8Array} **`region.w * region.h * 4` 字节 ✓**
 */
export function paintWithKernel(kernel, req) {
  if (!kernel) throw new Error("内核未就绪 ⇒ 无法落笔");
  const request = {
    myb: req.myb,
    points: req.points || [],
    size: req.size ?? null,
    color: req.color ?? null,
    opacity: null,
    hardness: null,
    region: { x: req.region.x, y: req.region.y, w: req.region.w, h: req.region.h },
    base: req.base ?? null,
  };
  const rgba = kernel.paint_brush(JSON.stringify(request));
  if (!rgba || rgba.length === 0) {
    // **★ 失败原因走内核自己的通道 ✗ ★**（**第 647 轮 ✓**：`paint_brush_error()` ✓）
    // ⇒ **∴ 比猜 payload 强 ✓**（**∴ 与"读内核自己的报错"同一条纪律 ✓**）。
    let why = "";
    try { why = kernel.paint_brush_error(); } catch (e) { why = "取错误也失败：" + e; }
    throw new Error("内核未产出像素：" + (why || "（没有给出原因）"));
  }
  const want = req.region.w * req.region.h * 4;
  if (rgba.length !== want) {
    throw new Error("内核产出字节数不符：期望 " + want + "，实得 " + rgba.length);
  }
  return rgba;
}

/** **★ 取区域底图（**涂抹类必需 ✓）✗ ★** ⇒ 从内核渲染该区域的 RGBA ✓。 */
export function baseFromKernel(kernel, region) {
  const rgba = kernel.render_region_rgba(region.x, region.y, region.w, region.h);
  if (!rgba || rgba.length === 0) return null;
  return { width: region.w, height: region.h, rgba: Array.from(rgba) };
}
