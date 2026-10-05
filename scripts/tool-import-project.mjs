#!/usr/bin/env node
// **Web 端导入 `.yanshi` 工程包**判据（产品负责人报的缺口 ①）。
//
// 判据要回答的问题**不是**"路由存在吗" ✗，而是**"一份包导到另一个根上还是不是同一份文档"** ✓。
// 所以它**起两个服务端**（各自独立的根目录 / 独立的内容寻址存储 ✓）：
//
//   A（判据 runner 起的那个，`argv[2]`）
//     ① 造一份有内容的文档：建层 + 填充 + **一个真 blob**（上传 16×16 raw RGBA 再 `import_image`）；
//     ② 用**真实的** `export_project` 工具导出 `.yanshi`；
//     ③ 记下 A 上该文档**渲染**的 raw blob 哈希（逐像素相等的可证形式）。
//   B（判据自己起的，临时根目录 ⇒ **全新的 CAS** ✓）
//     ④ 把磁盘上那个包的**字节**按 Web 端的分片协议传上去导入；
//     ⑤ 走**真实 loader** 打开导入的文档（`GET /api/documents/<id>` ⇒ 落盘读 + 折叠），
//        把尺寸 / head / 原子数 / 图层数 / 对象数与 A 逐项对账；
//     ⑥ 再对一次渲染：两边的 raw 哈希必须相同。
//
// **为什么一定要另起一个 CAS** ✗：同一个根里，源文档上传时那个 blob **本来就在** ✓
// ⇒ 就算导入端一个 blob 都没写进去 ✓，渲染照样能读到 ✓ ⇒ 判据会**假绿** ✓。
// 这不是假想 ✓：我第一版就是单服务端，然后做了一次"导入时不落 blob"的变异 ✓ ⇒ **它照样全绿** ✗
// ⇒ 于是才改成两个 CAS ✓（判据自己的盲区必须先被发现 ✓）。
//
// 反例（"诚实的失败"）也要有：
//   * 目标 id 已存在 ⇒ **拒绝且原文档一个字节都不动**（绝不覆盖）；
//   * 内容不是工程包 ⇒ 明确报错并说明缺什么；
//   * 分片偏移不符 ⇒ 明确报错并告诉客户端该从哪续（少一段不能变成"悄悄缺内容"）；
//   * `GET /api/documents/import` ⇒ 405（证明这条路由**没有被 `/api/documents/` 前缀吃掉**）。
//
// 用法：node scripts/tool-import-project.mjs <base-url> [docId] [token]
import { readFileSync, existsSync, mkdtempSync, rmSync } from "node:fs";
import { spawn } from "node:child_process";
import { join, isAbsolute } from "node:path";
import { tmpdir } from "node:os";

const base = process.argv[2];
if (!base) {
  console.error("用法: node scripts/tool-import-project.mjs <base-url>");
  process.exit(2);
}

let bad = 0;
const check = (label, ok, detail) => {
  console.log((ok ? "  ✓ " : "  ✗ ") + label + (detail ? "｜" + detail : ""));
  if (!ok) bad += 1;
};
const jsonAt = async (root, path, init) => {
  const response = await fetch(root + path, init);
  const text = await response.text();
  let value = null;
  try {
    value = JSON.parse(text);
  } catch (_) {
    value = { _raw: text.slice(0, 200) };
  }
  return { status: response.status, value };
};
const json = (path, init) => jsonAt(base, path, init);
const postJsonAt = (root, path, body) =>
  jsonAt(root, path, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });

const SRC = "crit_import_src";
const DEST = "crit_import_dest";
const created = await postJsonAt(base, "/api/documents", { doc_id: SRC, width: 64, height: 64 });
const token = created.value && created.value.token;
if (!token) {
  console.error("判据无法运行：建不出源文档（不是通过）⇒ " + JSON.stringify(created.value));
  process.exit(1);
}
const callToolAt = async (root, docId, docToken, name, args) =>
  (
    await postJsonAt(
      root,
      `/api/tools/${name}?doc=${encodeURIComponent(docId)}&token=${encodeURIComponent(docToken)}`,
      args,
    )
  ).value;
const callTool = (docId, docToken, name, args) => callToolAt(base, docId, docToken, name, args);

// ① 源文档要有**真实内容**。
await callTool(SRC, token, "create_layer", { layer_id: "L", name: "底" });
const filled = await callTool(SRC, token, "fill", {
  layer_id: "L",
  data: { color: { r: 200, g: 30, b: 90, a: 255 }, region: { x: 0, y: 0, w: 64, h: 64 } },
});
check("源文档已落笔（填充 64×64）", filled && filled.ok === true, JSON.stringify(filled).slice(0, 120));

// ①b **必须有一个真 blob** ✗：过程性原子（填充）不引用任何 blob ✓
// ⇒ 只测它们的话，"包里的 blob 能不能读回来"这条**根本没被覆盖** ✓
// （而那正是"能导出、读不回来"那个已知不对称所在 ✓）。
const pixels = Buffer.alloc(16 * 16 * 4);
for (let i = 0; i < 16 * 16; i += 1) {
  pixels[i * 4] = i % 256;
  pixels[i * 4 + 1] = 60;
  pixels[i * 4 + 2] = 200;
  pixels[i * 4 + 3] = 255;
}
const uploadedBlob = await jsonAt(
  base,
  `/api/blob?doc=${encodeURIComponent(SRC)}&token=${encodeURIComponent(token)}`,
  { method: "POST", headers: { "content-type": "image/x-yanshi-raw" }, body: pixels },
);
check(
  "源文档的位图 blob 已入库",
  uploadedBlob.value && uploadedBlob.value.ok === true,
  JSON.stringify(uploadedBlob.value).slice(0, 140),
);
const importedImage = await callTool(SRC, token, "import_image", {
  layer_id: "L",
  bitmap: {
    blob_hash: uploadedBlob.value.blob_hash,
    size: uploadedBlob.value.size,
    mime_type: uploadedBlob.value.mime_type,
  },
  region: { x: 0, y: 0, w: 16, h: 16 },
});
check(
  "源文档里有一个引用 blob 的对象",
  importedImage && importedImage.ok === true,
  JSON.stringify(importedImage).slice(0, 140),
);

// ② 真实导出。
const packageName = "crit-import-roundtrip.yanshi";
const exported = await callTool(SRC, token, "export_project", { path: packageName });
check("export_project 成功", exported && exported.ok === true, JSON.stringify(exported).slice(0, 160));

// 包到底落在哪：工具回 `path`，另外把两个常见根也试一遍（不知道服务端 CWD 时也要能找到）。
const candidates = [
  exported && exported.path,
  process.env.YANSHI_EXPORT_DIR ? join(process.env.YANSHI_EXPORT_DIR, packageName) : null,
  join(process.cwd(), "exports", packageName),
].filter((candidate) => typeof candidate === "string" && candidate.length > 0);
const packagePath = candidates
  .map((candidate) => (isAbsolute(candidate) ? candidate : join(process.cwd(), candidate)))
  .find((candidate) => existsSync(candidate));
check("导出文件真的在磁盘上", Boolean(packagePath), candidates.join(" ｜ "));
if (!packagePath) process.exit(1);
const packageBytes = readFileSync(packagePath);
check("导出文件不是空包", packageBytes.length > 0, "字节数 " + packageBytes.length);

// ③ A 上的渲染基准（同一区域 ⇒ raw RGBA 的内容哈希）。
const renderHashAt = async (root, docId, docToken) => {
  const value = await callToolAt(root, docId, docToken, "render_region", {
    region: { x: 0, y: 0, w: 64, h: 64 },
    raw: true,
  });
  const url = (value && value.raw_url) || "";
  const match = /\/(?:api\/)?blob\/([^?]+)/.exec(url) || /yanshi:\/\/blob\/([^?]+)/.exec(url);
  return {
    hash: match ? match[1] : "",
    ok: Boolean(value && value.ok),
    detail: JSON.stringify(value).slice(0, 120),
  };
};
const sourceRender = await renderHashAt(base, SRC, token);
check("源文档能渲染出像素", sourceRender.ok && sourceRender.hash.length > 0, sourceRender.detail);

// ④ **另起一个服务端 ⇒ 全新的 CAS** ✓（这是这条判据能不能看见"blob 没落盘"的关键 ✗）。
const serveBin =
  process.env.YANSHI_SERVE_BIN ||
  [
    join("target", "debug", "yanshi-serve"),
    // **共享 target 目录**（`CARGO_TARGET_DIR` 指到别处时）也要能找到 ✓ —— 否则本机验证只能靠环境变量 ✓。
    process.env.CARGO_TARGET_DIR
      ? join(process.env.CARGO_TARGET_DIR, "debug", "yanshi-serve")
      : null,
  ]
    .filter(Boolean)
    .find((candidate) => existsSync(candidate)) ||
  join("target", "debug", "yanshi-serve");
if (!existsSync(serveBin)) {
  console.error(`判据无法运行：找不到服务端可执行文件 ${serveBin}（先 cargo build --workspace --bins）`);
  process.exit(1);
}
const freshRoot = mkdtempSync(join(tmpdir(), "yanshi-import-cas-"));
const serverB = spawn(serveBin, ["--bind", "127.0.0.1:0", "--root", freshRoot], {
  stdio: ["ignore", "pipe", "pipe"],
});
let baseB = "";
let logB = "";
serverB.stdout.on("data", (chunk) => {
  logB += chunk.toString();
  const match = /(http:\/\/127\.0\.0\.1:\d+)/.exec(logB);
  if (match && !baseB) baseB = match[1];
});
serverB.stderr.on("data", (chunk) => {
  logB += chunk.toString();
});
const stopB = () => {
  try {
    serverB.kill("SIGKILL");
  } catch (_) {
    /* 已经没了 */
  }
  try {
    rmSync(freshRoot, { recursive: true, force: true });
  } catch (_) {
    /* 删不掉就算了 */
  }
};
process.on("exit", stopB);
for (let i = 0; i < 80 && !baseB; i += 1) {
  await new Promise((resolve) => setTimeout(resolve, 100));
}
check("另起的服务端（全新根目录）可用", baseB.length > 0, baseB || logB.slice(0, 200));
if (!baseB) {
  stopB();
  process.exit(1);
}

// ⑤ 分片上传（故意用小片 ⇒ 真的走多片，而不是只有一片的"伪分片"）。
const CHUNK = 4 * 1024;
const uploadAll = async (root, bytes, chunkSize = CHUNK) => {
  const begun = await jsonAt(root, "/api/documents/import?begin=1", { method: "POST" });
  const uploadId = begun.value && begun.value.upload_id;
  if (!uploadId) return { error: "begin 失败：" + JSON.stringify(begun.value).slice(0, 140) };
  let offset = 0;
  let chunks = 0;
  while (offset < bytes.length) {
    const slice = bytes.subarray(offset, offset + chunkSize);
    const step = await jsonAt(
      root,
      `/api/documents/import?upload=${encodeURIComponent(uploadId)}&offset=${offset}`,
      { method: "POST", body: slice },
    );
    if (!step.value || step.value.ok !== true) {
      return { error: JSON.stringify(step.value).slice(0, 160), uploadId, received: offset };
    }
    offset = Number(step.value.received);
    chunks += 1;
  }
  return { uploadId, chunks, received: offset };
};

const uploaded = await uploadAll(baseB, packageBytes);
check(
  "分片上传把整包送上去",
  !uploaded.error && uploaded.received === packageBytes.length,
  uploaded.error || uploaded.chunks + " 片 / " + uploaded.received + " 字节",
);
if (uploaded.error) {
  stopB();
  process.exit(1);
}

const finished = await jsonAt(
  baseB,
  `/api/documents/import?upload=${encodeURIComponent(uploaded.uploadId)}&finish=1&doc_id=${encodeURIComponent(DEST)}`,
  { method: "POST" },
);
check("finish 导入成功", finished.value && finished.value.ok === true, JSON.stringify(finished.value).slice(0, 200));
const destToken = finished.value && finished.value.token;
check("导入回执里带**新文档的令牌**（否则用户打不开它）", typeof destToken === "string" && destToken.length > 0);
check(
  "包里的 blob 真的被还原（数 ≥1 ⇒ 内容寻址核对通过）",
  finished.value && Number(finished.value.blobs) >= 1,
  "blobs=" + (finished.value && finished.value.blobs),
);

// ⑥ 走**真实 loader** 对账（另一条令牌、另一个根 ⇒ 服务端必须真的认识这份新文档）。
const summaryOf = async (root, docId, docToken) =>
  (await jsonAt(root, `/api/documents/${encodeURIComponent(docId)}?token=${encodeURIComponent(docToken)}`)).value;
const srcSummary = await summaryOf(base, SRC, token);
const dstSummary = await summaryOf(baseB, DEST, destToken || "");
const fieldOf = (value, key) => (value && value.document ? value.document[key] : undefined);
const same = ["width", "height", "head_seq", "atoms", "layers", "objects"].filter(
  (key) => JSON.stringify(fieldOf(srcSummary, key)) !== JSON.stringify(fieldOf(dstSummary, key)),
);
check(
  "导入的文档与源文档**状态一致**（尺寸/head/原子/图层/对象）",
  same.length === 0,
  same.length
    ? same.map((key) => key + "：" + fieldOf(srcSummary, key) + " ⇒ " + fieldOf(dstSummary, key)).join("，")
    : "head=" + fieldOf(dstSummary, "head_seq") + " atoms=" + fieldOf(dstSummary, "atoms"),
);

// ⑦ 渲染也要一致（**在全新 CAS 上** ⇒ "blob 没落盘"会在这里现形 ✗）。
const destRender = await renderHashAt(baseB, DEST, destToken || "");
check(
  "两边的渲染**逐像素相同**（raw blob 哈希相等，且在全新 CAS 上）",
  destRender.ok && sourceRender.hash.length > 0 && sourceRender.hash === destRender.hash,
  "源 " + (sourceRender.hash || "(空)") + " ｜ 导入 " + (destRender.hash || "(空)") + " " + destRender.detail,
);

// 反例 (a)：目标 id 已存在 ⇒ 拒绝，且**已有文档一个字节都不动**。
const beforeOverwrite = fieldOf(await summaryOf(baseB, DEST, destToken || ""), "atoms");
const clash = await uploadAll(baseB, packageBytes);
const clashFinish = await jsonAt(
  baseB,
  `/api/documents/import?upload=${encodeURIComponent(clash.uploadId)}&finish=1&doc_id=${encodeURIComponent(DEST)}`,
  { method: "POST" },
);
check(
  "导入到已存在的 id ⇒ **拒绝**（绝不覆盖）",
  clashFinish.value && clashFinish.value.ok === false,
  "status=" + clashFinish.status + " " + JSON.stringify(clashFinish.value).slice(0, 180),
);
const afterOverwrite = fieldOf(await summaryOf(baseB, DEST, destToken || ""), "atoms");
check("被拒绝的导入没有动已有的文档", beforeOverwrite === afterOverwrite, beforeOverwrite + " ⇒ " + afterOverwrite);

// 反例 (b)：内容不是工程包 ⇒ 明确报错，并说出包里有什么。
const junkUpload = await uploadAll(baseB, Buffer.from("hello, this is definitely not a tar archive"));
const junkFinish = await jsonAt(
  baseB,
  `/api/documents/import?upload=${encodeURIComponent(junkUpload.uploadId)}&finish=1&doc_id=crit_import_junk`,
  { method: "POST" },
);
const junkDetail =
  (junkFinish.value && junkFinish.value.context && junkFinish.value.context.detail) || "";
check(
  "不是工程包 ⇒ 明确报错（且说明缺什么）",
  junkFinish.value && junkFinish.value.ok === false && junkDetail.length > 0 && junkFinish.status >= 400,
  "status=" + junkFinish.status + " " + junkDetail.slice(0, 140),
);

// 反例 (c)：分片偏移不符 ⇒ 拒绝并告诉客户端该从哪续（不能"悄悄少一段"）。
const skew = await jsonAt(baseB, "/api/documents/import?begin=1", { method: "POST" });
const skewId = skew.value && skew.value.upload_id;
const skewed = await jsonAt(baseB, `/api/documents/import?upload=${encodeURIComponent(skewId)}&offset=7`, {
  method: "POST",
  body: Buffer.from("abc"),
});
const skewDetail = (skewed.value && skewed.value.context && skewed.value.context.detail) || "";
check(
  "分片偏移不符 ⇒ 拒绝并说清该从哪续",
  skewed.value && skewed.value.ok === false && skewed.status === 409 && skewDetail.includes("续传"),
  "status=" + skewed.status + " " + skewDetail.slice(0, 160),
);

// 反例 (d)：这条路由不能被 `/api/documents/` 前缀吃掉（那会表现为"路由不存在"）。
const shadow = await json("/api/documents/import", { method: "GET" });
check(
  "GET /api/documents/import ⇒ 405（没被文档前缀路由遮蔽）",
  shadow.status === 405,
  "status=" + shadow.status + " " + JSON.stringify(shadow.value).slice(0, 120),
);

stopB();
console.log(
  bad
    ? `  结论：${bad} 条不成立 ✗（导入没有真正闭环）`
    : "  结论：导出的包导到另一个根仍是同一份文档 ✓（状态与渲染都对上）",
);
process.exit(bad ? 1 : 0);
