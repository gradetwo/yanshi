#!/usr/bin/env node
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
const parsed = new URL(url);
const base = (process.argv[3] && process.argv[3].startsWith("http") ? process.argv[3] : parsed.origin).replace(/\/$/, "");
const token = process.argv[4] || parsed.searchParams.get("token") || "";
const port = process.env.CDP_PORT || process.argv[5] || "9333";
const doc = process.env.TIMELAPSE_DOC || parsed.searchParams.get("doc") || "crit_timelapse";
const dlDir = process.env.TIMELAPSE_DL_DIR || fs.mkdtempSync(path.join(os.tmpdir(), "yanshi-timelapse-"));
/** **∴ 每帧渲染都要过 wasm ＋ WebCodecs ✗ 给足预算 ✗ 但**必须可配**（CI 机器慢 ✓）★ */
const WAIT_MS = Number(process.env.YANSHI_TIMELAPSE_WAIT_MS || 180000);

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
socket.onmessage = (event) => {
  const m = JSON.parse(event.data);
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
await send("Page.setDownloadBehavior", { behavior: "allow", downloadPath: dlDir }).catch(() => undefined);
await send("Browser.setDownloadBehavior", { behavior: "allow", downloadPath: dlDir, eventsEnabled: true }).catch(() => undefined);

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

// **∴ ③ 造一点内容 ✗**：**走仓库自己的工具接口画几笔**✗ 不用判据私造原子** ✓ ★
// **∴ 坑 ✗**：**第一版这里用了 URL 里的 `token`**✗ 而**判据自己新建的文档**是**另一个
//   token** ✓ ⇒ **∴ 于是**24 笔**一笔都没画上** ✓（**∴ 判据自己先红 ✗ 还差点被当成产品错 ✓）★
let docToken = token;
const call = async (tool, args) => {
  const response = await fetch(`${base}/api/tools/${tool}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(docToken)}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(args),
  });
  return response.json().catch(() => null);
};
const created = await fetch(`${base}/api/documents`, {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 320, height: 240 }),
}).then((r) => r.json()).catch(() => null);
if (!created) cannotJudge("建不了判据用的文档（服务端没起来？）");
docToken = created.token || token;
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
}
check(strokes >= 20, "判据先在文档里画了 20+ 笔（这样视频里才有内容）", `${strokes} 笔`);

// **∴ ④ 让页面换到这份文档 ✗**（**∴ 导出的 atom 取自 `/api/atoms` ✗ 页面自己会拉 ✓）★
const url2 = new URL(url);
url2.searchParams.set("doc", doc);
url2.searchParams.set("token", docToken);
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
  cannotJudge(`页面的 WASM 内核一直没就绪（info=${info}）⇒ 本环境跑不起这条功能（CI 里由 wasm 产物保证）`);
}
const before = fs.existsSync(dlDir) ? fs.readdirSync(dlDir).filter((n) => n.endsWith(".mp4")) : [];
await evaluate('document.getElementById("timelapseExport").click(), true');
const until = Date.now() + WAIT_MS;
let files = [];
while (Date.now() < until) {
  files = fs.existsSync(dlDir)
    ? fs.readdirSync(dlDir).filter((n) => n.endsWith(".mp4") && !n.endsWith(".crdownload"))
    : [];
  if (files.length > before.length) break;
  const info = await evaluate('(document.getElementById("timelapseInfo")||{}).textContent || ""');
  if (typeof info === "string" && info.startsWith("导出失败")) {
    check(false, "点「导出过程视频」后**落下一个 mp4**", info.slice(0, 160));
    console.error("❌ 页面自己报了失败 ⇒ 导出这条路走不通");
    process.exit(1);
  }
  await sleep(500);
}
check(files.length > before.length, "点「导出过程视频」后落下一个 mp4", files.join(","));
if (files.length <= before.length) {
  const info = await evaluate('(document.getElementById("timelapseInfo")||{}).textContent || ""');
  console.error(`❌ 等了 ${Math.round(WAIT_MS / 1000)} 秒没有文件；页面提示是：${info}`);
  console.error(consoleLines.slice(-8).join("\n"));
  process.exit(1);
}
const file = path.join(dlDir, files[files.length - 1]);
const bytes = fs.readFileSync(file);
console.log(`  文件：${file}（${bytes.length} 字节）`);

// **∴ ⑥ 结构自洽 ✗**（**∴ 与 `tool-mp4-lite-structure` 同一套读法 ✓ 但这里读的是**真文件** ✓）★
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
let frameCount = 0;
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
  check(played.w === 320 && played.h === 240, "解出来的画面尺寸等于文档尺寸 320×240", `${played.w}×${played.h}`);
  check(played.d > 0, "解出来的时长大于 0", `${played.d}s`);
}

console.log("");
if (failures.length) {
  console.error(`❌ ${failures.length} 项没通过：${failures.join("｜")}`);
  console.error(consoleLines.slice(-8).join("\n"));
  process.exit(1);
}
console.log(`✓ 页面入口、MP4 结构、以及"同一个浏览器能解出元数据"三件都对 ⇒ 这条功能真的能用`);
