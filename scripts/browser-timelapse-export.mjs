#!/usr/bin/env node
// criterion-target: client  ## 运行器据此注入 render=client（测本地内核）
/**
 * **★★★ 「页面能导出**可播放**的过程视频」判据 ✗ ★★★**（**第 249 轮 ✓）
 *
 * **∴ 它证明什么 ✗**（**三件 ✗ 都能红 ✓）★**：
 *   **∴ ① 入口真的在页面上 ✗**：`#timelapseExport` 存在 ⇒ 点它**真的会触发一次导出** ✓
 *     （**∴ 不靠日志文字 ✗ 靠**落盘的文件** ✓）★
 *   **∴ ② 落下的文件是**结构完整的 MP4**✗**：`ftyp` 最前 ✗ `moov` 在 `mdat` 之前 ✗
 *     `moov > trak > mdia > minf > stbl` 里的样本表**都在** ✗ 且 `stsz` 说**至少有一帧** ✓
 *     ＋ **∴ 还**自己解析 `stsz`／`stco`**✗ 按偏移把样本取出来 ✗ 断言它们**真的落在
 *       `mdat` 里** ✓ ★★
 *   **∴ ③ 它能**被解出来**✗**：**把文件喂回**同一个 Chromium 的 `<video>`**✗
 *     **∴ 等 `loadedmetadata` ✗ 断言 `videoWidth/videoHeight` 等于文档尺寸 ✗ `duration > 0`** ✓
 *     ⇒ **∴ 这才叫"可播放"✗ 而不是"文件不是空的"** ✓ ★★★
 *
 * **∴ 为什么要第 ③ 条 ✗**：**手写的 MP4（`mp4-lite.js` ✓）**最容易的错**是
 *   **长度／偏移差几个字节**✗ ⇒ **∴ 那种文件**大小正常 ✗ 甚至有 `moov`**✗
 *     但**解码器打不开** ✓ ⇒ **∴ 只有真让浏览器去解 ✗ 才算证到** ✓ ★★
 *
 * **∴ 判不了就退 3 ✗**（**与全仓约定一致 ✓）★**：
 *   ＋ **∴ 没有页面调试目标**（**Chromium 没起 ✓）；
 *   ＋ **∴ 这个浏览器没有 WebCodecs**（`typeof VideoEncoder !== "function"` ✓）
 *     —— **∴ 那是**环境不支持**✗ 不是**产品错** ✓；
 *   ＋ **∴ 判据自己的 `Runtime.evaluate` 拿不到结果**（**∴ 按 AGENTS.md ✗
 *     `returnByValue` 与 `awaitPromise` **必须成对** ✓ 否则拿到空对象 ✓）
 *       ⇒ **∴ 那种情况要报"判据自身坏了"✗ 不能栽给产品** ✓ ★★
 *
 * 用法（经 `scripts/run-criteria.sh` 时它给 `<viewer-url> <base> <token> <cdp-port>`）：
 *   node scripts/browser-timelapse-export.mjs "<viewer-url>" [base] [token] [cdpPort]
 */
const url = process.argv[2];
if (!url) {
  console.error("用法: node scripts/browser-timelapse-export.mjs <viewer-url> [base] [token] [cdpPort]");
  process.exit(2);
}
const fs = await import("node:fs");
const os = await import("node:os");
const path = await import("node:path");
// **∴ 临时目录必须用仓库统一那个模块登记 ✗**（**∴ `crates/yanshi-http/tests/
//   scripts_clean_their_temp_dirs.rs` **会红**✗ 而**它抓的正是**"跑一次泄漏一个 root"**
//   ⇒ **∴ 实测被它抓过一次 ✗ 这就是它存在的意义 ✓）★★★
const { trackTemp } = await import("./lib/temp-dirs.mjs");
const parsed = new URL(url);
// **★★★ 实测踩到的坑（**判据自己 ✗）★★★**：**`with-temp-server.sh` 会把**查看器地址
// **追加为最后一个参数**✗（**它自己已经建好了 `check` 文档 ✓）⇒ **∴ 第一版**把 `argv[3]`
// **原样当 base**✗ 而**那个串带着 `?doc=…&token=…`** ✓ ⇒ **∴ `base + "/api/documents"`
// **实际请求到的是 `/`**✗ ⇒ **∴ 服务端回**「`/` 不允许 POST」✗ ⇒ **∴ 判据**自己起不来** ✓ ★★
//   ＋ **∴ 修法 ✗**：**只取 **origin**（`new URL(x).origin` 会把 query 丢掉 ✓）★
const originOf = (value) => {
  if (!value || !String(value).startsWith("http")) return "";
  try {
    return new URL(value).origin;
  } catch {
    return "";
  }
};
const base = originOf(process.argv[3]) || originOf(url);
if (!base) cannotJudge("拿不到服务端 origin（argv 形状不对）");
/** **∴ 默认用 runner 给的那份文档与 token**✗（**它已经建好 ✓）★ */
const doc = process.env.TIMELAPSE_DOC || parsed.searchParams.get("doc") || "crit_timelapse";
const token = process.argv[4] || parsed.searchParams.get("token") || "";
const port = process.env.CDP_PORT || process.argv[5] || "9333";
const dlDir =
  process.env.TIMELAPSE_DL_DIR ||
  trackTemp(fs.mkdtempSync(path.join(os.tmpdir(), "yanshi-timelapse-")));
/** **∴ 每帧渲染都要过 wasm ＋ WebCodecs ✗ 给足预算 ✗ 但**必须可配**（CI 机器慢 ✓）★ */
// **∴ 默认收紧到 90 秒 ✗**（**∴ `run-criteria.sh` 给 `browser-*` 的整段超时是
//   `BROWSER_TIMEOUT:-420` 秒 ✓ ⇒ **∴ 两段各等 180 秒**可能把整段拖爆** ✓；
//   ＋ **∴ 而**这里等待的东西（**内核就绪 ✗ 文件落盘 ✓）**都不是产品指标** ✓
//     ⇒ **∴ 需要更宽的机器**用环境变量调** ✓）★
const WAIT_MS = Number(process.env.YANSHI_TIMELAPSE_WAIT_MS || 90000);

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const failures = [];
const check = (ok, label, detail) => {
  console.log(`  ${ok ? "✓" : "✗"} ${label}${detail ? `（${detail}）` : ""}`);
  if (!ok) failures.push(label);
};
/** **判不了 ✗**（**∴ 与"产品错"分开 ✗ 退出码 3 ✓）★ */
const cannotJudge = (message) => {
  console.error(`❌ 判不了（不是产品问题）：${message} ⇒ 退出码 3`);
  process.exit(3);
};

// **∴ ① 先连 CDP ✗**（**页面由 runner 打开 ✓）★
let targets;
try {
  targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
} catch (error) {
  cannotJudge(`连不上 CDP（127.0.0.1:${port}）：${error.message}`);
}
const page = targets.find((t) => t.type === "page" && t.url.startsWith(parsed.origin)) || targets.find((t) => t.type === "page");
if (!page) cannotJudge(`没有页面调试目标（Chromium 是否以 --remote-debugging-port=${port} 起的？）`);

const socket = new WebSocket(page.webSocketDebuggerUrl);
let nextId = 1;
const pending = new Map();
const consoleLines = [];
const networkFailures = [];
socket.onmessage = (event) => {
  const m = JSON.parse(event.data);
  if (m.method === "Network.loadingFailed") {
    networkFailures.push(`${m.params.type} ${m.params.errorText}${m.params.blockedReason ? " (" + m.params.blockedReason + ")" : ""}`);
  }
  if (m.method === "Network.responseReceived" && m.params.response && m.params.response.status >= 400) {
    networkFailures.push(`HTTP ${m.params.response.status} ${m.params.response.url}`);
  }
  if (m.method === "Runtime.consoleAPICalled") {
    consoleLines.push((m.params.args || []).map((a) => String(a.value ?? a.description ?? "")).join(" "));
  }
  if (m.method === "Runtime.exceptionThrown") {
    consoleLines.push("exception: " + (m.params.exceptionDetails?.exception?.description || m.params.exceptionDetails?.text || ""));
  }
  if (m.id && pending.has(m.id)) {
    pending.get(m.id)(m);
    pending.delete(m.id);
  }
};
await new Promise((open) => {
  socket.onopen = open;
});
const send = (method, params) =>
  new Promise((resolve) => {
    const id = nextId++;
    pending.set(id, resolve);
    socket.send(JSON.stringify({ id, method, params: params || {} }));
  });
/** **∴ AGENTS.md 的硬规矩 ✗**：**两个开关**必须成对**✗ 否则 async 表达式**静默变空对象 ✓ ★★
 */
const evaluateRaw = (expression) =>
  send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
const evaluate = async (expression) => {
  const reply = await evaluateRaw(expression);
  const detail = reply?.result?.exceptionDetails;
  if (detail) {
    cannotJudge(`判据自身的求值失败：${detail.text || JSON.stringify(detail).slice(0, 200)}`);
  }
  return reply?.result?.result?.value;
};
await send("Runtime.enable");
await send("Network.enable");
await send("Page.setDownloadBehavior", { behavior: "allow", downloadPath: dlDir }).catch(() => undefined);
await send("Browser.setDownloadBehavior", { behavior: "allow", downloadPath: dlDir, eventsEnabled: true }).catch(() => undefined);

// **∴ ③ 造一点内容 ✗**：**走仓库自己的工具接口画几笔**✗ 不用判据私造原子** ✓ ★
// **∴ 坑 ✗**：**第一版这里用了 URL 里的 `token`**✗ 而**判据自己新建的文档**是**另一个
//   token** ✓ ⇒ **∴ 于是**24 笔**一笔都没画上** ✓（**∴ 判据自己先红 ✗ 还差点被当成产品错 ✓）★
let docToken = token;
/** **∴ 用哪份文档 ✗**：**自己建的那份（**或 runner 给的那份 ✓）★
 *  ＋ **∴ 必须和 token 成对 ✗**（**∴ 第一版拿 runner 的 doc ＋ 我的 token ⇒ 内核起不来 ✓）★
 */
let docId = doc;
const call = async (tool, args) => {
  const response = await fetch(`${base}/api/tools/${tool}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(docToken)}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(args),
  });
  return response.json().catch(() => null);
};
let created = { ok: true, token };
if (!docToken) {
  created = await fetch(`${base}/api/documents`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: 320, height: 240 }),
  }).then((r) => r.json()).catch(() => null);
  if (!created) cannotJudge("建不了判据用的文档（服务端没起来？）");
}
// **∴ 诊断（**判据自己坏了要能说清 ✓）★**：**实测踩到过 token 为空 ⇒ 页面 bootstrap 400** ✓
console.log(`  建文档回执：ok=${created.ok} 有 token=${Boolean(created.token)} 键=${Object.keys(created).join(",")}`);
if (!created.ok || !created.token) {
  console.error("  回执全文：" + JSON.stringify(created).slice(0, 300));
  cannotJudge("建文档没有拿到 token ⇒ 判据自己起不来（CI 里由 runner 传的 token 兜底）");
}
docToken = created.token || token;
docId = created.doc_id || doc;
await call("create_layer", { layer_id: "L1", name: "L1" });
let strokes = 0;
for (let i = 0; i < 24; i++) {
  const x = 30 + ((i * 11) % 240);
  const y = 30 + ((i * 17) % 170);
  const done = await call("brush_stroke", {
    layer_id: "L1",
    object_id: `tl${i}`,
    brush: "100%_Opaque",
    size: 12,
    points: [[x, y, 1.0], [x + 24, y + 14, 1.0]],
    color: { r: 30 + ((i * 9) % 200), g: 60 + ((i * 13) % 180), b: 200 - ((i * 7) % 160), a: 255 },
    preview: false,
  });
  if (done && done.ok) strokes += 1;
  if (i === 0) console.log("  第一笔回执：" + JSON.stringify(done).slice(0, 200));
}
check(strokes >= 20, "判据先在文档里画了 20+ 笔（这样视频里才有内容）", `${strokes} 笔`);

// **∴ 画面尺寸要**问文档**✗ 不能写死** ✓（**∴ 实测踩到 ✗**：**我请求 320×240 ✗
//   而那份 id 已存在 ⇒ **服务端给回**原来的 1024×1024** ⇒ **∴ 写死 320×240 的断言
//     **把一条**完全正常**的导出判成了红** ✓）★★★
let docW = 0;
let docH = 0;
{
  const info = await fetch(`${base}/api/tools/get_document?doc=${encodeURIComponent(docId)}&token=${encodeURIComponent(docToken)}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({}),
  }).then((r) => r.json()).catch(() => null);
  const value = info && (info.value || info);
  docW = Number(value && (value.width || (value.document && value.document.width))) || 0;
  docH = Number(value && (value.height || (value.document && value.document.height))) || 0;
  check(docW > 0 && docH > 0, "能问出文档真实尺寸（视频要按它解出来）", `${docW}×${docH}`);
}

// **∴ ④ 让页面换到这份文档 ✗**（**∴ 导出的 atom 取自 `/api/atoms` ✗ 页面自己会拉 ✓）★
// **∴ 必须用**我建的那份文档**✗**（**∴ 第一版导航去了 runner 的 `check`**
//   ＋ **∴ 却带着**我那份文档的 token**✗ ⇒ **∴ 页面 bootstrap 用错 token** ✓
//     ⇒ **∴ 内核**根本没起来**✗ 而**表现**只是"一直没就绪" ✓）★★
const url2 = new URL(url);
url2.searchParams.set("doc", docId);
url2.searchParams.set("token", docToken);
// **∴ 必须显式要**客户端内核**✗**（**∴ 默认那一档页面**根本不用本地 wasm** ✓
//   ⇒ **∴ `window.yanshiKernelReady` **永远不会被设**✗ 而**表现**只是"一直没就绪" ✓）
//     ＋ **∴ 这条功能**只能用本地内核 ✗ 所以要 `render=client`** ✓ ★★
url2.searchParams.set("render", "client");
console.log(`  导航到：${url2.origin}${url2.pathname}?doc=${url2.searchParams.get("doc")}（我建的那份）&token=${url2.searchParams.get("token") ? "有" : "空"}`);
await send("Page.navigate", { url: url2.toString() });
const ready = await (async () => {
  const until = Date.now() + WAIT_MS;
  while (Date.now() < until) {
    if (await evaluate('Boolean(document.getElementById("timelapseExport"))')) return true;
    await sleep(250);
  }
  return false;
})();
if (!ready) {
  // **∴ 按钮都没有 ⇒ 要么页面没加载完 ✗ 要么产品真的没接线 ✓** ⇒ **∴ 用**产品口径**报红 ✓
  check(false, "页面上有「导出过程视频」按钮（`#timelapseExport`）");
  console.error("❌ 页面上找不到那个按钮 ⇒ 接线没生效");
  process.exit(1);
}
check(true, "页面上有「导出过程视频」按钮（`#timelapseExport`）");

// **★★★ 实测踩到的坑（**本判据自己的 ✗）★★★**：**WebCodecs 必须先**进到页面**再问** ✓
//   ＋ **∴ 第一版**把这个检查放在 `Page.navigate` **之前**✗ ⇒ **∴ 量到的其实是
//     `about:blank`**✗ ⇒ **∴ 那里**不是安全上下文**✗ ⇒ **`VideoEncoder` 必然 undefined** ✓
//       ⇒ **∴ 于是**判据**白白退 3**✗ 而**这台机器的 Chromium 其实**支持 avc 编码** ✓
//         （**∴ 另起一个**真·localhost 页面**复测 ⇒ `secureContext: true` ✗
//          `typeof VideoEncoder === "function"` ✗ `avc1.640028` **supported: true** ✓）★★
// **∴ ② 环境支不支持 WebCodecs ✗**（**∴ 不支持 ⇒ 判不了 ✗ 不是产品错 ✓）★
const hasWebCodecs = await evaluate('typeof VideoEncoder === "function"');
if (hasWebCodecs === undefined) cannotJudge("取不到 `typeof VideoEncoder`（求值没回来）");
// **★★★ 没有 WebCodecs 时怎么办 ✗ ★★★**（**第 249 轮实测：**HeadlessChrome 153
// 就是 `VideoEncoder === undefined`** ✓）
//   ＋ **∴ 直接退 3 太浪费 ✗**：**那等于**整条页面路一点都没验** ✓
//   ＋ **∴ 于是 ✗**：**装一个**假编码器**✗ 让**除了真 H.264 之外**的每一段都跑起来** ✓
//     **∴ 能验到 ✗**：按钮 ⇒ 取 atom ⇒ 建独立内核 ⇒ 逐帧渲染 ⇒ **时长/关键帧算得对不对**
//       ⇒ **`mp4-lite` 装出来的结构** ⇒ 落盘 ✓ ★★
//     **∴ 验不到 ✗**：**真 H.264 能不能解**（**∴ 那部分**照实退 3 ✗ 不写成绿** ✓）★★★
//   ＋ **∴ 假编码器只替换 `VideoEncoder`**✗ 页面逻辑与封装器**仍然是真的** ✓
//     ⇒ **∴ 所以**它**不会**把产品错掩盖成绿**✓（**∴ 唯一放宽的是"码流来自真编码器" ✓）★
let stubbedEncoder = false;
if (!hasWebCodecs) {
  const installed = await evaluate(`(() => {
    class StubVideoEncoder {
      constructor(init) { this.__out = init.output; this.__n = 0; }
      configure(config) { this.__config = config; }
      encode(frame, opts) {
        // **∴ 造一段**长度前缀**的假 NAL ✗ 顺便带一个假 avcC** ✓（**∴ 结构检查只认盒子 ✓）★
        const bytes = new Uint8Array(64);
        bytes[0] = 0; bytes[1] = 0; bytes[2] = 0; bytes[3] = 8;
        bytes[4] = (opts && opts.keyFrame) ? 0x65 : 0x41;
        for (let i = 5; i < bytes.length; i++) bytes[i] = (this.__n + i) & 0xff;
        if (frame && frame.close) frame.close();
        const meta = this.__n === 0
          ? { decoderConfig: { codec: "avc1.640028", description: new Uint8Array([1, 100, 0, 40, 255, 225, 0, 4, 103, 100, 0, 40]) } }
          : undefined;
        this.__out({ byteLength: bytes.length, copyTo: (dst) => dst.set(bytes), timestamp: frame && frame.timestamp ? frame.timestamp : this.__n * 33333, type: (opts && opts.keyFrame) ? "key" : "delta" }, meta);
        this.__n += 1;
      }
      async flush() {}
      close() {}
    }
    globalThis.VideoEncoder = StubVideoEncoder;
    return typeof VideoEncoder === "function";
  })()`);
  if (installed !== true) cannotJudge("装不进假编码器 ⇒ 判据自身坏了");
  stubbedEncoder = true;
  console.log("  ⚠️ 这个浏览器没有 WebCodecs ⇒ 已装**假编码器**：验「接线＋结构」，**不验**真 H.264 能不能解");
}


// **★★★ 先问一句"这台机器有没有**任何一种**能编的编码器 ✗ ★★★**（**第 250 轮修 ✓）
//   ＋ **∴ 为什么不能只问 H.264 ✗**（**实测 ✓）**：**CI 的浏览器是
//     `setup-chrome` 装的 **Chrome for Testing**✗ ⇒ **∴ 它**不带专有编解码器**
//     ⇒ `avc1.640028` **unsupported** ✓ ⇒ **∴ 而**页面这时会**自动退到 VP9／VP8
//       ⇒ 产出**WebM** ✗ ⇒ **∴ 若判据只认 MP4 ✗ 就会把一条**正常**的导出判红** ✓
//         ⇒ **∴ 所以**：**问"有没有能编的"✗ 不问"有没有 H.264"** ✓ ★★
//   ＋ **∴ 三条都试不出来 ⇒ **判不了**（**环境能力 ✗ 不是产品缺陷 ✓）★
const codecSupport = await evaluate(`(async () => {
  const want = [
    ["avc", { codec: "avc1.640028", avc: { format: "avc" } }],
    ["vp9", { codec: "vp09.00.10.08" }],
    ["vp8", { codec: "vp8" }],
  ];
  if (typeof VideoEncoder !== "function") return { any: false, tried: ["no-encoder"] };
  const tried = [];
  for (const [key, extra] of want) {
    try {
      if (typeof VideoEncoder.isConfigSupported !== "function") return { any: true, tried: ["no-probe"] };
      const res = await VideoEncoder.isConfigSupported({ width: 320, height: 240, bitrate: 800000, framerate: 30, ...extra });
      if (res && res.supported) return { any: true, which: key, tried };
      tried.push(key);
    } catch (error) {
      tried.push(key + ":" + (error && error.message ? error.message : error));
    }
  }
  return { any: false, tried };
})()`);
if (!codecSupport || codecSupport.any !== true) {
  cannotJudge(`这台机器没有任何可用的视频编码器（试过 ${JSON.stringify(codecSupport && codecSupport.tried)}）⇒ 属环境能力`);
}
console.log(`  这台机器能编：${codecSupport.which || "（探测不了，直接试）"}`);

// **∴ 本机验证用 ✗ 强制某条编码器**（**∴ CI 不设它 ⇒ 自动选 ✓）★
//   ＋ **∴ 为什么需要它 ✗**：**本机 AVC 可用 ⇒ **自动**只会走 MP4** ✓
//     ⇒ **∴ 而** WebM 那一路（**CI 实际走的那条 ✓）就**永远没在本机跑过** ✓
//       ⇒ **∴ 所以**给一个开关 ✗ 两条路都验一遍** ✓ ★★
const forceCodec = process.env.TIMELAPSE_FORCE_CODEC || "";
if (forceCodec) {
  await evaluate(`globalThis.__YANSHI_TIMELAPSE_CODEC = ${JSON.stringify(forceCodec)}, true`);
  console.log(`  （本机验证：强制 codec = ${forceCodec}）`);
}

// **∴ ⑤ 点它 ✗ 等文件落盘 ✓** ★
// **∴ 必须等**页面自己的内核就绪**✗**（**∴ 实测踩到 ✓）**：**`state.wasm` 还是 null 时点按钮**✗
//   ⇒ **∴ 页面只会报"内核没加载"**✗ 而**那**验不到这条功能** ✓
//     ＋ **∴ 内核起不来（**本机 wasm 产物缺失 ✓）⇒ **∴ 判**不了**✗ 退 3 ✓ 不栽给产品** ✓ ★★
const kernelReady = await (async () => {
  const until = Date.now() + WAIT_MS;
  while (Date.now() < until) {
    if (await evaluate("window.yanshiKernelReady === true")) return true;
    await sleep(500);
  }
  return false;
})();
if (!kernelReady) {
  const info = await evaluate('(document.getElementById("timelapseInfo")||{}).textContent || ""');
  console.error("  网络失败：" + (networkFailures.slice(-6).join("｜") || "（没有失败记录）"));
  const interesting = consoleLines.filter((l) => /wasm|内核|exception|error|失败|Error/i.test(l));
  console.error("  相关日志：" + (interesting.slice(-8).join("｜") || "（没有与内核/错误相关的日志）"));
  console.error("  页面日志尾：" + (consoleLines.slice(-6).join("｜") || "（空）"));
  cannotJudge(`页面的 WASM 内核一直没就绪（info=${info}）⇒ 本环境跑不起这条功能（CI 里由 wasm 产物保证）`);
}
// **∴ 产物可能是 `.mp4` 也可能是 `.webm` ✗**（**∴ 页面按这台机器有什么编码器选 ✓）
//   ＋ **∴ 我第一版**只找 `.mp4`**✗ ⇒ **∴ 强制 vp9 那一次**明明落了盘 ✗ 判据却说
//     "没有文件"** ✓ ⇒ **∴ 那是**判据自己瞎** ✗ 不是产品错 ✓）★★
const isVideoFile = (name) =>
  (name.endsWith(".mp4") || name.endsWith(".webm")) && !name.endsWith(".crdownload");
const before = fs.existsSync(dlDir) ? fs.readdirSync(dlDir).filter(isVideoFile) : [];
await evaluate('document.getElementById("timelapseExport").click(), true');
const until = Date.now() + WAIT_MS;
let files = [];
while (Date.now() < until) {
  files = fs.existsSync(dlDir) ? fs.readdirSync(dlDir).filter(isVideoFile) : [];
  if (files.length > before.length) break;
  const info = await evaluate('(document.getElementById("timelapseInfo")||{}).textContent || ""');
  if (typeof info === "string" && info.startsWith("导出失败")) {
    check(false, "点「导出过程视频」后**落下一个视频文件**", info.slice(0, 160));
    console.error("❌ 页面自己报了失败 ⇒ 导出这条路走不通");
    process.exit(1);
  }
  await sleep(500);
}
check(files.length > before.length, "点「导出过程视频」后落下一个视频文件（mp4 或 webm）", files.join(","));
if (files.length <= before.length) {
  const info = await evaluate('(document.getElementById("timelapseInfo")||{}).textContent || ""');
  console.error(`❌ 等了 ${Math.round(WAIT_MS / 1000)} 秒没有文件；页面提示是：${info}`);
  console.error(consoleLines.slice(-8).join("\n"));
  process.exit(1);
}
const file = path.join(dlDir, files[files.length - 1]);
const bytes = fs.readFileSync(file);
console.log(`  文件：${file}（${bytes.length} 字节）`);

// **∴ ⑥ 结构自洽 ✗**（**∴ 与 `tool-mp4-lite-structure` ／ `tool-webm-lite-structure`
//   同一套读法 ✓ 但这里读的是**真文件** ✓）★
//
// **★★★ 第 250 轮修 ✗：**必须按**实际容器**分派** ✓ ★★★**
//   ＋ **∴ 为什么 ✗**：**页面会**按这台机器有什么编码器**自动选容器** ✓
//     （**∴ 而**CI 的 Chrome for Testing 没有 H.264 ⇒ **∴ 它**产出 WebM** ✓）
//       ⇒ **∴ 若还写死按 MP4 解 ✗ 就会把一条**完全正常**的导出判红** ✓
//         ⇒ **∴ 那是**判据自己造出来的假红** ✗ 比漏判更坏 ✓ ★★
const isMp4 = bytes.length > 12 && bytes.toString("ascii", 4, 8) === "ftyp";
const isWebm =
  bytes.length > 4 && bytes[0] === 0x1a && bytes[1] === 0x45 && bytes[2] === 0xdf && bytes[3] === 0xa3;
check(isMp4 || isWebm, "产物是 MP4 或 WebM（按这台机器的编码器自动选）", isMp4 ? "MP4" : isWebm ? "WebM" : "都不是");
const wantExt = isWebm ? ".webm" : ".mp4";
check(
  file.endsWith(wantExt),
  `落盘文件名的扩展名与容器一致（${wantExt}）`,
  path.basename(file),
);
let frameCount = 0;
if (isMp4) {
  const u32 = (buf, at) => ((buf[at] << 24) | (buf[at + 1] << 16) | (buf[at + 2] << 8) | buf[at + 3]) >>> 0;
  const boxes = (buf, start = 0, end = buf.length) => {
    const out = [];
    let at = start;
    while (at + 8 <= end) {
      const size = u32(buf, at);
      const type = buf.toString("ascii", at + 4, at + 8);
      if (size < 8 || at + size > end) {
        out.push({ type, size, at, bad: true });
        break;
      }
      out.push({ type, size, at, body: at + 8, end: at + size });
      at += size;
    }
    return out;
  };
  const find = (list, type) => list.find((b) => b.type === type);
  const top = boxes(bytes);
  const names = top.map((b) => b.type);
  check(names[0] === "ftyp", "第一个盒子是 ftyp", names.join(","));
  check(top.every((b) => !b.bad), "每个顶层盒子的长度自洽");
  check(
    names.includes("moov") && names.includes("mdat") && names.indexOf("moov") < names.indexOf("mdat"),
    "moov 在 mdat 之前",
  );
  const moov = find(top, "moov");
  const trak = moov ? find(boxes(bytes, moov.body, moov.end), "trak") : null;
  const mdia = trak ? find(boxes(bytes, trak.body, trak.end), "mdia") : null;
  const minf = mdia ? find(boxes(bytes, mdia.body, mdia.end), "minf") : null;
  const stbl = minf ? find(boxes(bytes, minf.body, minf.end), "stbl") : null;
  check(Boolean(stbl), "moov > trak > mdia > minf > stbl 都在");
  frameCount = 0;
  if (stbl) {
    const stblBoxes = boxes(bytes, stbl.body, stbl.end);
    for (const need of ["stsd", "stts", "stsc", "stsz", "stco"]) {
      check(Boolean(find(stblBoxes, need)), `stbl 里有 ${need}`);
    }
    const stsz = find(stblBoxes, "stsz");
    if (stsz) frameCount = u32(bytes, stsz.body + 8);
    check(frameCount >= 2, "样本表里至少有 2 帧", `${frameCount} 帧`);
    const mdat = find(top, "mdat");
    const stco = find(stblBoxes, "stco");
    if (mdat && stco && stsz) {
      const count = u32(bytes, stco.body + 4);
      let inside = true;
      for (let i = 0; i < count; i++) {
        const off = u32(bytes, stco.body + 8 + i * 4);
        const size = u32(bytes, stsz.body + 12 + i * 4);
        if (off < mdat.body || off + size > mdat.end) inside = false;
      }
      check(inside && count === frameCount, "每个样本的偏移＋长度都落在 mdat 之内");
    }
  }

} else if (isWebm) {
  // **∴ WebM 那一路 ✗**（**同一套 EBML 读法 ✓ 见 `tool-webm-lite-structure` ✓）★
  const readVint = (buf, at) => {
    const first = buf[at];
    if (first === undefined || first === 0) return null;
    let len = 1;
    while (len <= 8 && (first & (1 << (8 - len))) === 0) len++;
    if (len > 8) return null;
    let value = first & ((1 << (8 - len)) - 1);
    for (let i = 1; i < len; i++) value = value * 256 + buf[at + i];
    return { value, len };
  };
  const elements = (buf, from = 0, to = buf.length) => {
    const out = [];
    let at = from;
    while (at < to) {
      let idLen = 1;
      while (idLen <= 4 && (buf[at] & (1 << (8 - idLen))) === 0) idLen++;
      if (idLen > 4 || at + idLen > to) break;
      const id = buf.subarray(at, at + idLen);
      const size = readVint(buf, at + idLen);
      if (!size) break;
      const body = at + idLen + size.len;
      if (body + size.value > to) break;
      out.push({ id, body, end: body + size.value });
      at = body + size.value;
    }
    return out;
  };
  const hex = (id) => Array.from(id).map((b) => b.toString(16).padStart(2, "0")).join("");
  const textOf = (buf, el) => {
    let out = "";
    for (let i = el.body; i < el.end; i++) out += String.fromCharCode(buf[i]);
    return out;
  };
  const uintOf = (buf, el) => {
    let value = 0;
    for (let i = el.body; i < el.end; i++) value = value * 256 + buf[i];
    return value;
  };
  const top = elements(bytes);
  const ebml = top.find((e) => hex(e.id) === "1a45dfa3");
  const segment = top.find((e) => hex(e.id) === "18538067");
  check(Boolean(ebml), "WebM：顶层有 EBML 头");
  check(Boolean(segment), "WebM：顶层有 Segment");
  if (ebml && segment) {
    const docType = elements(bytes, ebml.body, ebml.end).find((e) => hex(e.id) === "4282");
    check(Boolean(docType) && textOf(bytes, docType) === "webm", "WebM：DocType = webm");
    const segKids = elements(bytes, segment.body, segment.end);
    const tracks = segKids.find((e) => hex(e.id) === "1654ae6b");
    const entry = tracks ? elements(bytes, tracks.body, tracks.end).find((e) => hex(e.id) === "ae") : null;
    const kids = entry ? elements(bytes, entry.body, entry.end) : [];
    const type = kids.find((e) => hex(e.id) === "83");
    const codecEl = kids.find((e) => hex(e.id) === "86");
    check(Boolean(type) && uintOf(bytes, type) === 1, "WebM：TrackType = 1（视频）");
    const codecId = codecEl ? textOf(bytes, codecEl) : "";
    check(codecId === "V_VP8" || codecId === "V_VP9", "WebM：CodecID 是 VP8／VP9", codecId);
    const clusters = segKids.filter((e) => hex(e.id) === "1f43b675");
    let blocks = 0;
    for (const cluster of clusters) {
      blocks += elements(bytes, cluster.body, cluster.end).filter((e) => hex(e.id) === "a3").length;
    }
    frameCount = blocks;
    check(blocks >= 2, "WebM：样本（SimpleBlock）至少 2 帧", `${blocks} 帧`);
    check(clusters.length >= 1, "WebM：至少一个 Cluster", `${clusters.length} 个`);
  }
}
// **∴ ⑦ 让**同一个浏览器**真的把它解出来 ✗ ★★★**（**∴ 这一步才叫"可播放" ✓）★
const base64 = bytes.toString("base64");
const played = await evaluate(`(async () => {
  try {
    const raw = atob(${JSON.stringify(base64)});
    const buf = new Uint8Array(raw.length);
    for (let i = 0; i < raw.length; i++) buf[i] = raw.charCodeAt(i);
    const blob = new Blob([buf], { type: "video/mp4" });
    const video = document.createElement("video");
    video.muted = true;
    video.src = URL.createObjectURL(blob);
    const meta = await new Promise((resolve) => {
      const timer = setTimeout(() => resolve(null), 20000);
      video.onloadedmetadata = () => { clearTimeout(timer); resolve({ w: video.videoWidth, h: video.videoHeight, d: video.duration }); };
      video.onerror = () => { clearTimeout(timer); resolve(null); };
    });
    URL.revokeObjectURL(video.src);
    return meta;
  } catch (error) {
    return { error: String(error && error.message || error) };
  }
})()`);
if (played === undefined) cannotJudge("取不到 `<video>` 的元数据（求值没回来）");
if (stubbedEncoder) {
  // **∴ 假码流当然解不出来 ✗** ⇒ **∴ 这一段**不能判 ✗ 也不许写成绿** ✓ ★★
  check(failures.length === 0, "按钮 ⇒ 取 atom ⇒ 独立内核 ⇒ 逐帧渲染 ⇒ 封装 ⇒ 落盘（**接线与结构 ✓）");
  console.log("");
  console.log("⚠️ 只验到**接线与 MP4 结构**：这个浏览器没有 WebCodecs ⇒ **真 H.264 可播放性未证**");
  console.log(`   已落盘：${file}（${bytes.length} 字节、${frameCount} 帧）`);
  console.error("❌ 判不了（不是产品问题）：本环境的 Chromium 没有 VideoEncoder ⇒ 无法证「可播放」 ⇒ 退出码 3");
  process.exit(failures.length ? 1 : 3);
}
if (played && played.error) {
  check(false, "浏览器能解出这个 mp4 的元数据", played.error);
} else if (!played) {
  check(false, "浏览器能解出这个 mp4 的元数据", "`loadedmetadata` 或 `error` 都没在 20 秒内到");
} else {
  check(
    played.w === docW && played.h === docH,
    `解出来的画面尺寸等于文档真实尺寸 ${docW}×${docH}`,
    `${played.w}×${played.h}`,
  );
  check(played.d > 0, "解出来的时长大于 0", `${played.d}s`);
}

console.log("");
if (failures.length) {
  console.error(`❌ ${failures.length} 项没通过：${failures.join("｜")}`);
  console.error(consoleLines.slice(-8).join("\n"));
  process.exit(1);
}
console.log(
  `✓ 页面入口、${isWebm ? "WebM" : "MP4"} 结构、以及"同一个浏览器能解出元数据"三件都对 ⇒ 这条功能真的能用`,
);
// **★★★ 必须**主动收摊 ✗ ★★★**（**CI 实测抓到的 ✓）**：
//   ＋ **∴ 症状 ✗**：**判据**每一条都打勾 ✗ 打印了成功行**✗ 却在 CI 里算
//     **`EXIT=124`（**超时 ✓）⇒ **∴ 于是**：**整条 shard**因为一句"成功"**红** ✓
//   ＋ **∴ 根因 ✗**：**我**没关 CDP 的 `WebSocket`**✗ ⇒ **∴ 事件循环**一直有活** ⇒
//     **∴ `node`**不退出** ✓（**∴ 本地那几次的 124**也是它 ✗ 我先前后成"重建慢"** ✓）★
//   ＋ **∴ 修法 ✗**：**显式关掉 socket ✗ 再**显式退出** ✓（**∴ 退出码 0 ⇒ 与判据结论一致 ✓）
//     ＋ **∴ 而**失败/判不了那两条路**本来就 `process.exit(1|3)`** ✓ ⇒ **∴ 只有成功路径漏了** ✓ ★★
try {
  socket.close();
} catch (error) {
  void error;
}
process.exit(0);
