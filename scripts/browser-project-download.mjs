#!/usr/bin/env node
// **浏览器能把 `.yanshi` 工程包下载到本机**判据 ✓（判据 ② 与 ③ 的浏览器那一半 ✓）。
//
// **它要回答的问题** ✓（不是"按钮存在吗" ✗）：
//   ① 在**真实查看器**里点「导出工程」⇒ **下载目录里真的出现一个文件** ✓；
//   ② 那个文件的字节 = **服务端 URL 上的那份包**（逐字节 SHA-256 相同 ✓）；
//   ③ 那个文件 = **导入路径读得懂的工程包**（用查看器同一条分片协议把它导成一份新文档 ✓，
//      原子/blob 都得真的还原出来 ✓）。
//
// **为什么必须落到"字节"** ✗：只断言"下载目录里有个文件"会被"下载了一张错误页"骗过 ✓；
// 只断言"回执里有 url"会被"url 指向别的 blob"骗过 ✓。（后者的服务端那一半在
// `crates/yanshi-server/tests/export_project_download.rs` ✓。）
//
// **变异（判据自己会不会红）** ✓：
//   * 查看器不再创建那个 `<a download>` ⇒ 下载目录空 ⇒ 红 ✓；
//   * `write_export_project` 把 `store.put(&tar)` 换成 `store.put(b"not a tar")`
//     ⇒ 文件照样下载 ✓，但 ② 的哈希对不上、③ 的导入被拒 ⇒ 红 ✓。
//
// 用法：node scripts/browser-project-download.mjs <viewer-url> [base] [token] [cdpPort] [downloadDir]
//   `run-criteria.sh` 给 browser-* 传的是 `<viewer-url> <base> <token> <cdp-port>` ✓。
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, readdirSync, rmSync, statSync } from "node:fs";

const url = process.argv[2];
const baseArg = process.argv[3] || "";
const tokenArg = process.argv[4] || "";
// ⚠️ **端口不能从 argv[4] 取** ✗：那一格是 token ✓（`run-criteria.sh` 的约定 ✓）。
const port = process.argv[5] || process.env.CDP_PORT || "9333";
const dir = process.argv[6] || process.env.YANSHI_DL_DIR || "/var/tmp/yanshi-project-dl";
if (!url) {
  console.error(
    "用法: node scripts/browser-project-download.mjs <viewer-url> [base] [token] [cdpPort] [downloadDir]",
  );
  process.exit(2);
}
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const failures = [];
const check = (label, ok, detail) => {
  console.log((ok ? "  ✓ " : "  ✗ ") + label + (detail ? "｜" + detail : ""));
  if (!ok) failures.push(label);
};

const parsed = new URL(url);
const base = baseArg || parsed.origin;
const doc = parsed.searchParams.get("doc") || "";
const token = tokenArg || parsed.searchParams.get("token") || "";
if (!doc || !token) {
  console.error("判据无法运行：viewer URL 里没有 doc / token（不是通过）");
  process.exit(1);
}
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

// 下载目录**在断言之前**清干净 ✓ —— 否则上次跑剩的文件会让"有没有下载"这条假绿 ✓。
rmSync(dir, { recursive: true, force: true });
mkdirSync(dir, { recursive: true });

const postTool = async (name, args) =>
  (
    await fetch(`${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(args || {}),
    })
  ).json();

// ① 先给文档**真内容** ✓（空文档的包也能下载 ✓，但那测不到 blob 那条路 ✓）。
await postTool("create_layer", { layer_id: "dl_layer", name: "dl" });
const filled = await postTool("fill", {
  layer_id: "dl_layer",
  data: { color: { r: 40, g: 180, b: 120, a: 255 }, region: { x: 0, y: 0, w: 96, h: 72 } },
});
if (!filled || filled.ok !== true) {
  console.error("判据无法运行：造内容失败（不是通过）⇒ " + JSON.stringify(filled).slice(0, 160));
  process.exit(1);
}
// **必须有一个真 blob** ✗：过程性原子（填充）不引用任何 blob ✓ ⇒ 只测它会漏掉"包里的 blob
// 能不能还原" ✓（而那正是导出/导入最容易不对称的一处 ✓）。上传一张 16×16 raw RGBA 再 `import_image` ✓。
const pixels = Buffer.alloc(16 * 16 * 4);
for (let i = 0; i < 16 * 16; i += 1) {
  pixels[i * 4] = i % 256;
  pixels[i * 4 + 1] = 70;
  pixels[i * 4 + 2] = 190;
  pixels[i * 4 + 3] = 255;
}
const uploaded = await (
  await fetch(`${base}/api/blob?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`, {
    method: "POST",
    headers: { "content-type": "image/x-yanshi-raw" },
    body: pixels,
  })
).json();
const image = await postTool("import_image", {
  layer_id: "dl_layer",
  bitmap: { blob_hash: uploaded.blob_hash, size: uploaded.size, mime_type: uploaded.mime_type },
  region: { x: 0, y: 0, w: 16, h: 16 },
});
if (!uploaded || uploaded.ok !== true || !image || image.ok !== true) {
  console.error(
    "判据无法运行：造 blob 失败（不是通过）⇒ " +
      JSON.stringify({ uploaded, image }).slice(0, 200),
  );
  process.exit(1);
}

// ② 连上调试浏览器 ✓（与 `browser-offline-export.mjs` 同一套 CDP 手法 ✓）。
const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const page = targets.find((target) => target.type === "page");
if (!page) {
  console.error("没有页面目标 ⇒ 判据无效");
  process.exit(1);
}
const socket = new WebSocket(page.webSocketDebuggerUrl);
let nextId = 1;
const pending = new Map();
socket.onmessage = (event) => {
  const message = JSON.parse(event.data);
  if (message.id && pending.has(message.id)) {
    pending.get(message.id)(message);
    pending.delete(message.id);
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
const evaluate = async (expression) =>
  (await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true })).result?.result?.value;

await send("Runtime.enable");
await send("Page.enable");
await send("Network.enable");
await send("Page.setDownloadBehavior", { behavior: "allow", downloadPath: dir }).catch(() => undefined);
await send("Browser.setDownloadBehavior", {
  behavior: "allow",
  downloadPath: dir,
  eventsEnabled: true,
}).catch(() => undefined);

await send("Page.navigate", { url });
// 等一个与断言**不同**的就绪信号 ✓：`#projectExport` 的处理器挂在**异步的**
// `setupAssetPanels()` 里 ✓ ⇒ 光等按钮会**抢在挂监听器之前**点 ⇒ 看起来像"点了没反应" ✗
//（实测就是这么红的 ✓）⇒ 等查看器自己写的那个就绪标志 ✓。
let ready = false;
for (let i = 0; i < 60; i += 1) {
  await sleep(300);
  try {
    if (
      await evaluate(
        'document.readyState === "complete" && !!document.getElementById("projectExport") && !!(window.yanshiStats && window.yanshiStats.projectExportReady)',
      )
    ) {
      ready = true;
      break;
    }
  } catch (_) {
    /* 还没就绪 ⇒ 继续等 ✓ */
  }
}
check("查看器就绪（#projectExport 的处理器已挂上）", ready);
if (!ready) {
  socket.close();
  process.exit(1);
}

// ③ **走真实用户路径**：点那个按钮 ✓（不是直接调工具 ✓）。
const clicked = await evaluate(
  '(() => { const b = document.getElementById("projectExport"); if (!b) return "no-button"; b.click(); return "clicked"; })()',
);
check("点到了「导出工程」按钮", clicked === "clicked", String(clicked));

// ④ **有界等待** ✓（20s ✓）：等下载目录里出现一个**大小稳定**的文件 ✓。
// ⚠️ 这里等的是"文件出现" ✓，而断言判的是**内容** ✓（哈希 / 导入 ✓）⇒ 不是自证 ✓。
let files = [];
let stable = false;
for (let i = 0; i < 100; i += 1) {
  await sleep(200);
  const seen = readdirSync(dir).filter((name) => !name.endsWith(".crdownload"));
  if (seen.length > 0) {
    const first = statSync(`${dir}/${seen[0]}`);
    await sleep(200);
    let again = null;
    try {
      again = statSync(`${dir}/${seen[0]}`);
    } catch (_) {
      /* 还在写 */
    }
    if (again && again.size === first.size && first.size > 0) {
      files = seen;
      stable = true;
      break;
    }
  }
}
check("点导出后下载目录里出现了一个文件", stable && files.length > 0, JSON.stringify(files));
if (!stable || files.length === 0) {
  console.log("  下载目录：" + JSON.stringify(readdirSync(dir)));
  // **失败要能自证为什么** ✓（否则只能看到"没下载" ✓ —— 分不清"按钮没反应"与"浏览器没落盘" ✗）。
  const info = await evaluate('(document.getElementById("projectInfo")||{}).textContent');
  const report = await evaluate("window.yanshiStats && window.yanshiStats.lastProjectExport");
  const logTail = await evaluate("(document.getElementById('log')||{}).textContent");
  console.log("  页面 projectInfo：" + JSON.stringify(info));
  console.log("  页面 lastProjectExport：" + JSON.stringify(report));
  console.log("  页面日志尾部：" + String(logTail || "").slice(0, 300));
  socket.close();
  process.exit(1);
}

const filePath = `${dir}/${files[0]}`;
const downloaded = readFileSync(filePath);
console.log(`  下载文件：${files[0]}（${downloaded.length} 字节）`);
check("下载文件名以 .yanshi 结尾（查看器给的是包名）", files[0].endsWith(".yanshi"), files[0]);
check("下载下来的不是空文件", downloaded.length > 0, String(downloaded.length));

// ⑤ **查看器回执里的 URL** ✓（这是"下载的是什么"的权威说明 ✓）。
const report = await evaluate("window.yanshiStats && window.yanshiStats.lastProjectExport");
check(
  "查看器记下了这次导出的 url / filename / bytes",
  Boolean(report) && typeof report.url === "string" && report.url.length > 0,
  JSON.stringify(report).slice(0, 160),
);
const href = report && typeof report.url === "string" ? report.url : "";
check(
  "回执里的字节数等于下载下来的字节数",
  Boolean(report) && Number(report.bytes) === downloaded.length,
  `回执 ${report && report.bytes} vs 文件 ${downloaded.length}`,
);

// ⑥ **字节级证据**：直接 GET 那个 url，比 SHA-256（下载文件 vs 服务端那份包 ✓）。
if (href) {
  const response = await fetch(new URL(href, base));
  const served = Buffer.from(await response.arrayBuffer());
  check("回执里的 url 能取到字节（HTTP 200）", response.ok, `status=${response.status}`);
  check(
    "下载文件与服务端 URL 上的包**逐字节相同**（SHA-256）",
    served.length === downloaded.length && sha256(served) === sha256(downloaded),
    `url ${served.length}B ${sha256(served).slice(0, 16)}… vs 文件 ${downloaded.length}B ${sha256(downloaded).slice(0, 16)}…`,
  );
  check(
    "服务端那份包的哈希 = 回执里的 blob_hash（内容寻址自证）",
    Boolean(report.blob_hash) && String(report.blob_hash).endsWith(sha256(served)),
    `${report && report.blob_hash} vs sha256:${sha256(served)}`,
  );
} else {
  check("回执里有 url（否则无法核对下载的是什么）", false);
}

// ⑦ **判据 ③**：把**下载下来的那些字节**按查看器同一条分片协议导入成一份新文档 ✓。
const destId = "dl_import_" + Date.now().toString(36);
const begun = await (
  await fetch(`${base}/api/documents/import?begin=1`, { method: "POST" })
).json();
let imported = null;
if (!begun || !begun.ok) {
  check("导入会话能开始（分片协议）", false, JSON.stringify(begun).slice(0, 160));
} else {
  const chunk = Math.max(1, Math.min(Number(begun.max_chunk_bytes) || 1 << 20, 1 << 20));
  let offset = Number(begun.received) || 0;
  let uploadError = "";
  while (offset < downloaded.length) {
    const slice = downloaded.subarray(offset, offset + chunk);
    const step = await (
      await fetch(
        `${base}/api/documents/import?upload=${encodeURIComponent(begun.upload_id)}&offset=${offset}`,
        { method: "POST", body: slice },
      )
    ).json();
    if (!step || step.ok !== true) {
      uploadError = JSON.stringify(step).slice(0, 160);
      break;
    }
    offset = Number(step.received);
  }
  check("下载下来的字节能整包上传", uploadError === "" && offset === downloaded.length, uploadError || `${offset} 字节`);
  if (!uploadError && offset === downloaded.length) {
    imported = await (
      await fetch(
        `${base}/api/documents/import?upload=${encodeURIComponent(begun.upload_id)}&finish=1&doc_id=${encodeURIComponent(destId)}`,
        { method: "POST" },
      )
    ).json();
    check("下载下来的包能被导入路径读懂并导入", imported && imported.ok === true, JSON.stringify(imported).slice(0, 200));
    check(
      "导入还原出了原子与 blob（不是空壳）",
      Boolean(imported) && Number(imported.atoms) >= 2 && Number(imported.blobs) >= 1,
      `atoms=${imported && imported.atoms} blobs=${imported && imported.blobs}`,
    );
  }
}

socket.close();
console.log(
  failures.length
    ? `  结论：${failures.length} 条不成立 ✗ ⇒ 浏览器下载工程包这条路没有闭环`
    : "  结论：查看器点导出 ⇒ 本机得到一个与服务端 URL 逐字节相同、且导入得回去的工程包 ✓",
);
process.exit(failures.length ? 1 : 0);
