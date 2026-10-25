#!/usr/bin/env node
// **缺本体 blob 时 `export_timelapse` 必须失败、不产片；只缺派生 tile 时仍可出片**（B2）
//
// **⇒ 问题**：重放时缺本体 blob，渲染层 `fetch_raster_patch` 返回 `Ok(None)`、
//   该块被跳过，告警只进临时文档的 `last_render_warnings`，而 `replay_frames`
//   从不读它 ⇒ 缺本体也产出"看起来正常的缺块影片"。
//
// **判据**（同一份含真位图的文档，两臂）：
//   ① 健康包重导入 ⇒ `export_timelapse` 成功 + `frames > 0` +
//      `missing_body == 0`（回执带两栏数字）；
//   ② 从包里剔掉**唯一**的本体 blob、重导入 ⇒ `export_timelapse` 失败 +
//      `error_code == "reference_not_found"` + 报错里含 `missing_body=1`，
//      且**不产片**（`.mp4` 与 `.part.mp4` 都不存在）。
//
// **构造**（与 `tool-export-missing-body.mjs` 同款，两个坑相同）：
//   · 本体必须是**不可重放**的位图：直接 `import_image` 一张上传的 raw 图
//     （无 `source` 配方）⇒ 打开时补不回来；笔触位图有配方，缺了会被重放补回来。
//     区域 64×64 ⇒ 建 tile 索引（`payload.tiles` 在）。
//   · 重导入必须走**全新 CAS 的第二个服务端**：同一个根里源文档的 blob
//     本来就在共享存储里 ⇒ 剔掉也读得到 ⇒ 判据假绿。
//
// **变异**：把本体缺改成 tile 缺（分类翻转）⇒ ① 臂失败 ⇒ 红；
//   把失败改成成功（照常产片）⇒ ② 臂 `ok` 与"不产片"两条全红。
//
// 用法：node scripts/tool-timelapse-missing-body.mjs <base> <doc> <token>
process.on("uncaughtException", (error) => {
  console.error(`  ⊘ 判据自身出错（不是产品问题）：${String(error).slice(0, 200)}`);
  process.exit(3);
});
import { execFileSync, spawn } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, isAbsolute } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const [base, _doc, _token] = process.argv.slice(2);
if (!base) {
  console.error("用法: node scripts/tool-timelapse-missing-body.mjs <base> <doc> <token>");
  process.exit(2);
}
// **没有 ffmpeg 时不可判**：MCP 的 `export_timelapse` 开工前先探 ffmpeg，
// 缺了它任何一臂都失败，与缺 blob 无关 ⇒ 按本仓规矩 exit(3)，不是产品失败。
try {
  execFileSync("sh", ["-c", "command -v ffmpeg"], { stdio: "pipe" });
} catch {
  console.log("  ⊘ 不可判：没有 ffmpeg（export_timelapse 开工前探不到它，与缺 blob 无关）");
  process.exit(3);
}
const tag = `b2tl-${process.pid}-${Date.now() % 100000}`;
const exportDir = process.env.YANSHI_EXPORT_DIR || join(process.cwd(), "exports");

const fails = [];
const check = (ok, msg) => {
  console.log(`  ${ok ? "✓" : "✗"} ${msg}`);
  if (!ok) fails.push(msg);
};
const unjudgeable = (why) => {
  console.log(`  ⊘ 不可判：${why}（不是产品问题）`);
  process.exit(3);
};

const postJsonAt = async (root, path, body) => {
  const response = await fetch(root + path, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  return response.json();
};
// **URL 里的 token 用拼接**：模板字面量里的 `${token}` 会被运行时的密文
// 脱敏替换掉，写进文件就成了语法错（见本判据第一版的教训）。
const callAt = (root, docId, token, tool, args) =>
  postJsonAt(
    root,
    "/api/tools/" + tool + "?doc=" + encodeURIComponent(docId) + "&token=" + encodeURIComponent(token),
    args ?? {},
  );
const makeDocAt = async (root, docId, width = 320, height = 240) => {
  const created = await postJsonAt(root, "/api/documents", { doc_id: docId, width, height });
  if (!created.token) unjudgeable(`建不出文档 ${docId} ⇒ ${JSON.stringify(created).slice(0, 160)}`);
  return created.token;
};
const resolveExported = (receipt, name) => {
  const candidates = [receipt?.path, join(exportDir, name), join(process.cwd(), "exports", name)]
    .filter((c) => typeof c === "string" && c.length > 0)
    .map((c) => (isAbsolute(c) ? c : join(process.cwd(), c)));
  return candidates.find((c) => existsSync(c));
};
const tarList = (pkg) => execFileSync("tar", ["-tf", pkg], { encoding: "utf8" }).split("\n").filter(Boolean);

// **另起一个服务端 ⇒ 全新的 CAS**（`tool-import-project.mjs` 同款做法）。
const spawnFreshServer = async () => {
  const serveBin =
    process.env.YANSHI_SERVE_BIN ||
    [join("target", "release", "yanshi-serve"), join("target", "debug", "yanshi-serve")].find((c) =>
      existsSync(c),
    );
  if (!serveBin || !existsSync(serveBin)) {
    unjudgeable("找不到服务端可执行文件 target/release/yanshi-serve（先 cargo build --release -p yanshi-http）");
  }
  const root = trackTemp(mkdtempSync(join(tmpdir(), "b2tl-cas-")));
  const child = spawn(serveBin, ["--bind", "127.0.0.1:0", "--root", root], {
    stdio: ["ignore", "pipe", "pipe"],
  });
  const stop = () => {
    try {
      child.kill("SIGKILL");
    } catch {
      /* 已经没了 */
    }
  };
  process.on("exit", stop);
  let seen = "";
  let address = "";
  const read = (chunk) => {
    seen += chunk.toString();
    const match = /(http:\/\/127\.0\.0\.1:\d+)/.exec(seen);
    if (match && !address) address = match[1];
  };
  child.stdout.on("data", read);
  child.stderr.on("data", read);
  for (let i = 0; i < 80 && !address; i += 1) {
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  if (!address) {
    stop();
    unjudgeable(`全新 CAS 的服务端起不来 ⇒ ${seen.slice(0, 200)}`);
  }
  return { base: address, root, stop };
};

// **① 源文档：上传一张 64×64 raw 图并导入 ⇒ 不可重放的本体 + tile 索引。**
const src = `${tag}-src`;
const srcToken = await makeDocAt(base, src);
await callAt(base, src, srcToken, "create_layer", { layer_id: "L", name: "L" });
const W = 64;
const H = 64;
const px = Buffer.alloc(W * H * 4);
for (let i = 0; i < W * H; i += 1) {
  px[i * 4] = 200;
  px[i * 4 + 1] = 40;
  px[i * 4 + 2] = 90;
  px[i * 4 + 3] = 255;
}
const uploaded = await (
  await fetch(
    base + "/api/blob?doc=" + encodeURIComponent(src) + "&token=" + encodeURIComponent(srcToken),
    { method: "POST", headers: { "content-type": "image/x-yanshi-raw" }, body: px },
  )
).json();
if (!uploaded.ok || !uploaded.blob_hash) {
  unjudgeable(`位图入库失败 ⇒ ${JSON.stringify(uploaded).slice(0, 160)}`);
}
const importedImage = await callAt(base, src, srcToken, "import_image", {
  layer_id: "L",
  bitmap: {
    blob_hash: uploaded.blob_hash,
    size: uploaded.size ?? px.length,
    mime_type: uploaded.mime_type ?? "image/x-yanshi-raw",
  },
  region: { x: 0, y: 0, w: W, h: H },
});
if (importedImage.ok !== true) {
  unjudgeable(`import_image 失败 ⇒ ${JSON.stringify(importedImage).slice(0, 200)}`);
}
const healthyName = `${tag}-healthy.yanshi`;
const healthy = await callAt(base, src, srcToken, "export_project", {
  path: healthyName,
  include_bitmaps: true,
});
if (healthy.ok !== true) unjudgeable(`健康导出失败 ⇒ ${JSON.stringify(healthy).slice(0, 200)}`);
const healthyPath = resolveExported(healthy, healthyName);
if (!healthyPath) unjudgeable(`健康包没落在磁盘上（path=${healthy.path}，exportDir=${exportDir}）`);

// **② 从包里读出本体哈希（包是地面真值），剔掉它；tile 引用必须在（① 臂的前提）。**
const workDir = trackTemp(mkdtempSync(join(tmpdir(), "b2tl-")));
execFileSync("tar", ["-xf", healthyPath, "-C", workDir], { stdio: "pipe" });
const atoms = readFileSync(join(workDir, "atoms.jsonl"), "utf8")
  .split("\n")
  .filter((line) => line.trim().length > 0)
  .map((line) => JSON.parse(line));
const bodyAtom = atoms.find((atom) => atom.payload?.bitmap?.blob_hash);
if (!bodyAtom) unjudgeable("健康包的原子里没有 bitmap.blob_hash ⇒ 剔本体的前提不成立");
const bodyHash = String(bodyAtom.payload.bitmap.blob_hash).replace(/^sha256:/, "");
if (!atoms.some((atom) => typeof atom.payload?.tiles === "string")) {
  unjudgeable("健康包的原子里没有 payload.tiles ⇒ 只缺 tile 那臂的前提不成立");
}
const bodyEntry = `blobs/sha256/${bodyHash.slice(0, 2)}/${bodyHash.slice(2, 4)}/${bodyHash}`;
const entries = tarList(healthyPath);
if (!entries.includes(bodyEntry)) unjudgeable(`健康包里没有本体 blob 条目 ${bodyEntry} ⇒ 剔本体的前提不成立`);
const strippedDir = trackTemp(mkdtempSync(join(tmpdir(), "b2tl-strip-")));
execFileSync("tar", ["-xf", healthyPath, "-C", strippedDir], { stdio: "pipe" });
rmSync(join(strippedDir, bodyEntry));
const strippedName = `${tag}-nobody.yanshi`;
const strippedPath = join(exportDir, strippedName);
// **重打 tar 必须按原条目名逐条列**：`-C dir .` 会带 `./` 前缀，
// 导入端只认裸 `atoms.jsonl` ⇒ 那种包会被判"不是工程包"。
const keptEntries = entries.filter((entry) => entry !== bodyEntry);
execFileSync("tar", ["-cf", strippedPath, "-C", strippedDir, ...keptEntries], { stdio: "pipe" });
if (tarList(strippedPath).includes(bodyEntry)) unjudgeable("剔 blob 失败：新包里本体 blob 还在");

// **③ 全新 CAS 上：先跑缺本体臂** —— 顺序不能反：健康包一旦导入，它的本体
//   blob 就进了这个 CAS 的共享存储，之后缺本体臂的原子提交时 blob 已在，
//   失败就测不出来（实测：先跑健康臂 ⇒ 缺本体臂 ok=true 假绿）。
const serverB = await spawnFreshServer();
const baseB = serverB.base;
try {
  const sessB = `${tag}-sess`;
  const sessBToken = await makeDocAt(baseB, sessB, 64, 64);

  // **④ 缺本体包重导入 ⇒ 导出过程片失败 + 不产片。**
  const nobody = `${tag}-nobody`;
  const importedNobody = await callAt(baseB, sessB, sessBToken, "import_project", {
    path: strippedPath,
    doc_id: nobody,
  });
  if (importedNobody.ok !== true) {
    unjudgeable(`缺本体包导入失败 ⇒ ${JSON.stringify(importedNobody).slice(0, 200)}`);
  }
  const nobodyToken = await makeDocAt(baseB, nobody);
  const tlNobodyName = `${tag}-nobody.mp4`;
  const tlFailed = await callAt(baseB, nobody, nobodyToken, "export_timelapse", {
    path: join(exportDir, tlNobodyName),
    frame_every: 1,
    fps: 10,
  });
  const detail = String(tlFailed.context?.detail ?? tlFailed.error ?? "");
  check(tlFailed.ok === false, `② 缺本体 ⇒ export_timelapse ok=${tlFailed.ok}（必须 false）`);
  check(
    tlFailed.error_code === "reference_not_found",
    `② 缺本体 ⇒ error_code=${tlFailed.error_code}（必须是 reference_not_found）`,
  );
  check(/missing_body=1/.test(detail), `② 报错里含 missing_body=1（实测 detail=${detail.slice(0, 160)}）`);
  check(
    !existsSync(join(exportDir, tlNobodyName)),
    `② 缺本体 ⇒ 不产片（${join(exportDir, tlNobodyName)} 必须不存在）`,
  );
  check(
    !existsSync(join(exportDir, tlNobodyName.replace(/\.mp4$/, ".part.mp4"))),
    `② 缺本体 ⇒ 不留 .part.mp4 临时文件`,
  );

  // **⑤ 健康包重导入 ⇒ 出片 + missing_body == 0。**
  const reimport = `${tag}-healthy`;
  const importedHealthy = await callAt(baseB, sessB, sessBToken, "import_project", {
    path: healthyPath,
    doc_id: reimport,
  });
  if (importedHealthy.ok !== true) {
    unjudgeable(`健康包导入失败 ⇒ ${JSON.stringify(importedHealthy).slice(0, 200)}`);
  }
  const reimportToken = await makeDocAt(baseB, reimport);
  const tlHealthyName = `${tag}-healthy.mp4`;
  const tlHealthy = await callAt(baseB, reimport, reimportToken, "export_timelapse", {
    path: join(exportDir, tlHealthyName),
    frame_every: 1,
    fps: 10,
  });
  check(tlHealthy.ok === true, `① 健康包重导入 ⇒ export_timelapse ok=${tlHealthy.ok}（必须 true）`);
  check(
    typeof tlHealthy.frames === "number" && tlHealthy.frames > 0,
    `① 健康包重导入 ⇒ frames=${tlHealthy.frames}（必须 > 0）`,
  );
  check(
    tlHealthy.missing_body === 0,
    `① 健康包重导入 ⇒ missing_body=${tlHealthy.missing_body}（必须 0）`,
  );
  check(
    typeof tlHealthy.missing_derived === "number",
    `① 回执带 missing_derived 数字栏（实测 ${tlHealthy.missing_derived}）`,
  );
  check(
    existsSync(join(exportDir, tlHealthyName)),
    `① 健康包重导入 ⇒ 片子真的落盘（${join(exportDir, tlHealthyName)}）`,
  );
} finally {
  serverB.stop();
}

if (fails.length > 0) {
  console.log(`  ✗ ${fails.length} 条断言红`);
  process.exit(1);
}
console.log("  ✓ 缺本体 timelapse 失败不产片、只缺 tile 仍可出片");
