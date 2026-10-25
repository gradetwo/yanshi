#!/usr/bin/env node
// **缺本体 blob 时 `export_project` 必须失败、不产包；只缺派生 tile 时仍可导出**（B2）
//
// **⇒ 问题**：原子引用的本体 blob 不在存储里时，导出照样 `ok:true`、只在
//   `missing` 清单里记一笔 ⇒ 调用方拿到一个"看起来正常"的缺本体包；
//   而派生 tile（净荷 `tiles` 里藏的 tile 索引）缺失时导出根本不计数，
//   与 `check_package.py`（本体/派生两栏）的口径对不上。
//
// **判据**（同一份含真位图的文档，三臂）：
//   ① 健康导出 ⇒ `ok:true` + `missing_body == 0` + `missing_derived == 0`
//      + 恒等式 `missing_body == missing.length`；
//   ② 从包里剔掉**唯一**的本体 blob、重导入、再导出 ⇒ `ok:false` +
//      `error_code == "reference_not_found"` + 报错里含 `missing_body=1`，
//      且**不产出**可用包文件（给定的 path 上没有文件）；
//   ③ 健康包重导入、再导出 ⇒ `ok:true` + `missing_body == 0` +
//      `missing_derived > 0`（导入不恢复 tile 索引，本体全在）。
//
// **构造**（两个坑，都是实测换来的）：
//   · 本体必须是**不可重放**的位图：直接 `import_image` 一张上传的 raw 图
//     （无 `source` 配方）⇒ 打开时 `materialize_pending_replayable_blobs`
//     补不回来；笔触位图有配方，缺了会被重放补回来，测不到失败。
//     区域 64×64 ⇒ 建 tile 索引（`payload.tiles` 在，派生臂的前提）。
//   · 重导入必须走**全新 CAS 的第二个服务端**：同一个根里源文档的 blob
//     本来就在共享存储里 ⇒ 剔掉也读得到 ⇒ 判据假绿（`tool-import-project.mjs`
//     的同一教训）。
//
// **变异**：把本体缺改成 tile 缺（分类翻转）⇒ ③ 臂导出失败 ⇒ 红；
//   把失败改成成功（照常产包）⇒ ② 臂 `ok` 与"不产包"两条全红。
//
// 用法：node scripts/tool-export-missing-body.mjs <base> <doc> <token>
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
  console.error("用法: node scripts/tool-export-missing-body.mjs <base> <doc> <token>");
  process.exit(2);
}
const tag = `b2exp-${process.pid}-${Date.now() % 100000}`;
// **导出目录必须是本判据自己造的临时目录** ✓（第 1132 轮整合时修）：
//   ⇒ 原来用 `YANSHI_EXPORT_DIR`（`run-criteria.sh` 给判据导出的是 `$ROOT_DIR`）
//     而**服务端**的导出沙箱是 `$ROOT_DIR/work/exports` ✗ ⇒ 两侧不是同一个目录
//     ⇒ 产品如实拒绝（`precondition_failed: 只能写进导出目录…`）✗ ⇒ 判据假红
//     （CI 实测 7 条断言全红；本机 `run-criteria-local.sh` 恰好两边同一目录 ⇒ 假绿 ✗）
//   ⇒ 改用**系统临时目录**（产品允许："…或系统临时目录" ✓）⇒ 两侧口径一致 ✓
//   ⇒ 且用 `trackTemp` 登记，结束自动清 ✓
const exportDir = trackTemp(mkdtempSync(join(tmpdir(), "b2exp-out-")));

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
  const root = trackTemp(mkdtempSync(join(tmpdir(), "b2exp-cas-")));
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

// **② 健康导出：两栏都是 0，且 missing_body 与 missing 清单长度一致。**
const healthyName = `${tag}-healthy.yanshi`;
const healthy = await callAt(base, src, srcToken, "export_project", {
  path: healthyName,
  include_bitmaps: true,
});
if (healthy.ok !== true) unjudgeable(`健康导出失败 ⇒ ${JSON.stringify(healthy).slice(0, 200)}`);
const healthyPath = resolveExported(healthy, healthyName);
if (!healthyPath) unjudgeable(`健康包没落在磁盘上（path=${healthy.path}，exportDir=${exportDir}）`);
check(
  healthy.missing_body === 0 && healthy.missing_derived === 0,
  `① 健康导出 ⇒ missing_body=${healthy.missing_body} missing_derived=${healthy.missing_derived}（都要 0）`,
);
check(
  healthy.missing_body === (healthy.missing ?? []).length,
  `① 恒等式 missing_body == missing.length（${healthy.missing_body} == ${(healthy.missing ?? []).length}）`,
);

// **③ 从包里读出本体哈希与 tile 引用（包是地面真值），再剔掉本体 blob。**
const workDir = trackTemp(mkdtempSync(join(tmpdir(), "b2exp-")));
execFileSync("tar", ["-xf", healthyPath, "-C", workDir], { stdio: "pipe" });
const atoms = readFileSync(join(workDir, "atoms.jsonl"), "utf8")
  .split("\n")
  .filter((line) => line.trim().length > 0)
  .map((line) => JSON.parse(line));
const bodyAtom = atoms.find((atom) => atom.payload?.bitmap?.blob_hash);
if (!bodyAtom) unjudgeable("健康包的原子里没有 bitmap.blob_hash ⇒ 剔本体的前提不成立");
const bodyHash = String(bodyAtom.payload.bitmap.blob_hash).replace(/^sha256:/, "");
const tileRefs = atoms.filter((atom) => typeof atom.payload?.tiles === "string").length;
if (tileRefs === 0) unjudgeable("健康包的原子里没有 payload.tiles ⇒ 只缺 tile 那臂的前提不成立");
const bodyEntry = `blobs/sha256/${bodyHash.slice(0, 2)}/${bodyHash.slice(2, 4)}/${bodyHash}`;
if (!tarList(healthyPath).includes(bodyEntry)) {
  unjudgeable(`健康包里没有本体 blob 条目 ${bodyEntry} ⇒ 剔本体的前提不成立`);
}
const strippedDir = trackTemp(mkdtempSync(join(tmpdir(), "b2exp-strip-")));
execFileSync("tar", ["-xf", healthyPath, "-C", strippedDir], { stdio: "pipe" });
rmSync(join(strippedDir, bodyEntry));
const strippedName = `${tag}-nobody.yanshi`;
const strippedPath = join(exportDir, strippedName);
// **重打 tar 必须按原条目名逐条列**：`-C dir .` 会带 `./` 前缀，
// 导入端只认裸 `atoms.jsonl` ⇒ 那种包会被判"不是工程包"。
const keptEntries = tarList(healthyPath).filter((entry) => entry !== bodyEntry);
execFileSync("tar", ["-cf", strippedPath, "-C", strippedDir, ...keptEntries], { stdio: "pipe" });
if (tarList(strippedPath).includes(bodyEntry)) unjudgeable("剔 blob 失败：新包里本体 blob 还在");
check(true, `② 已剔本体 blob（包里 ${bodyEntry} 不在，原子仍引用它）`);

// **④ 全新 CAS 上：缺本体包重导入、再导出 ⇒ 失败 + 不产包。**
const serverB = await spawnFreshServer();
const baseB = serverB.base;
try {
  const sessB = `${tag}-sess`;
  const sessBToken = await makeDocAt(baseB, sessB, 64, 64);
  const nobody = `${tag}-nobody`;
  const importedNobody = await callAt(baseB, sessB, sessBToken, "import_project", {
    path: strippedPath,
    doc_id: nobody,
  });
  if (importedNobody.ok !== true) {
    unjudgeable(`缺本体包导入失败 ⇒ ${JSON.stringify(importedNobody).slice(0, 200)}`);
  }
  const nobodyToken = await makeDocAt(baseB, nobody);
  const nobodyOutName = `${tag}-nobody-out.yanshi`;
  const failed = await callAt(baseB, nobody, nobodyToken, "export_project", {
    path: join(exportDir, nobodyOutName),
    include_bitmaps: true,
  });
  const detail = String(failed.context?.detail ?? failed.error ?? "");
  check(failed.ok === false, `② 缺本体再导出 ⇒ ok=${failed.ok}（必须 false）`);
  check(
    failed.error_code === "reference_not_found",
    `② 缺本体再导出 ⇒ error_code=${failed.error_code}（必须是 reference_not_found）`,
  );
  check(/missing_body=1/.test(detail), `② 报错里含 missing_body=1（实测 detail=${detail.slice(0, 160)}）`);
  check(/missing_derived=\d+/.test(detail), `② 报错里含 missing_derived 计数（实测 detail=${detail.slice(0, 160)}）`);
  check(
    !existsSync(join(exportDir, nobodyOutName)),
    `② 缺本体导出不产包（${join(exportDir, nobodyOutName)} 必须不存在）`,
  );

  // **⑤ 全新 CAS 上：健康包重导入、再导出 ⇒ 成功 + 只缺派生。**
  const reimport = `${tag}-healthy`;
  const importedHealthy = await callAt(baseB, sessB, sessBToken, "import_project", {
    path: healthyPath,
    doc_id: reimport,
  });
  if (importedHealthy.ok !== true) {
    unjudgeable(`健康包导入失败 ⇒ ${JSON.stringify(importedHealthy).slice(0, 200)}`);
  }
  const reimportToken = await makeDocAt(baseB, reimport);
  const reexport = await callAt(baseB, reimport, reimportToken, "export_project", {
    path: join(exportDir, `${tag}-healthy-reexport.yanshi`),
    include_bitmaps: true,
  });
  check(reexport.ok === true, `③ 健康包重导入再导出 ⇒ ok=${reexport.ok}（必须 true）`);
  check(
    reexport.missing_body === 0,
    `③ 健康包重导入再导出 ⇒ missing_body=${reexport.missing_body}（必须 0）`,
  );
  check(
    typeof reexport.missing_derived === "number" && reexport.missing_derived > 0,
    `③ 健康包重导入再导出 ⇒ missing_derived=${reexport.missing_derived}（必须 > 0，tile 索引不随包恢复）`,
  );
  check(
    reexport.missing_body === (reexport.missing ?? []).length,
    `③ 恒等式 missing_body == missing.length（${reexport.missing_body} == ${(reexport.missing ?? []).length}）`,
  );
} finally {
  serverB.stop();
}

if (fails.length > 0) {
  console.log(`  ✗ ${fails.length} 条断言红`);
  process.exit(1);
}
console.log("  ✓ 缺本体导出失败、只缺 tile 仍可导出（两栏口径与 check_package.py 一致）");
