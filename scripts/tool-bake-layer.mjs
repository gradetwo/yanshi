#!/usr/bin/env node
// **bake_layer 引擎原语判据**（专家评审定版 + 第二轮 12 条）。
//
// 覆盖（四层齐：实现／输入触发／断言／变异，变异点见文末）：
//   ① 纯绘画层一次成功 + 回执必填字段 + collapsed_primitives 分项；
//   ② 再 bake 一次 ⇒ no_op_already_baked（不产生新原子与新 Blob）；
//   ③ 空层 ⇒ no_op_empty_layer；
//   ④ 含文本/矢量默认 is_error，显式 allow_rasterize_editable 后成功且回执含
//      TEXT_RASTERIZED / VECTOR_RASTERIZED（LOSSY_EDITABILITY）审计条目；
//   ⑤ 越界图元默认 is_error，显式 allow_clip_off_canvas 后成功且回执含
//      CLIPPED_OFF_CANVAS_PIXELS 区域坐标；
//   ⑥ 压平后仿射：90° 旋转与等比缩放通过，45° 旋转拒绝，非均匀默认拒绝、
//      显式 allow_non_uniform_scale 后通过；再 bake 后 transform 重置 IDENTITY
//      且水位前移（新 blob_hash ≠ 旧）；
//   ⑦ 缺 Blob 严格报错（常规折叠/渲染不自动重放）+ recover_baked_blobs
//      显式恢复成功且回执含 replayed_ms / replayed_atom_count / blob_hash；
//   ⑧ 哈希不一致 strict 拒绝；⑧b 真实哈希不一致全链路（改日志造不一致：
//      bake 后删 Blob、杀服务、改 atoms.jsonl 里一笔颜色、重启同一 root
//      再 recover——strict 拒绝且不追加原子，append_rebake 追加新 Bake
//      原子且历史原子逐字节不动）；
//   ⑨ 导出 frontier 默认不含历史 Bake Blob、all_history 含历史 Bake Blob；
//   ⑩ Undo 缺历史 Blob 被原子级拦截（MISSING_HISTORICAL_BLOB_ON_UNDO）且状态不变；
//   ⑪ 导入缺有效 Bake Blob 默认拒绝，replay_and_heal 恢复成功。
//
// 前提失败 exit(3)（判据前提不成立 ≠ 产品失败）；产品失败 exit(1)。
// 用法：node scripts/tool-bake-layer.mjs
//   （须先有新鲜的 target/release/yanshi-serve；见 run-criteria-local.sh）

process.on("uncaughtException", (error) => {
  console.error(`  ⊘ 判据自身出错（不是产品问题）：${String(error).slice(0, 200)}`);
  process.exit(3);
});
import { spawn, spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync, existsSync, statSync, rmSync, readdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const pick = () => {
  const c = [];
  for (const root of [process.env.CARGO_TARGET_DIR, "target"].filter(Boolean)) {
    for (const k of ["release", "debug"]) {
      const p = join(root, k, "yanshi-serve");
      if (existsSync(p)) c.push(p);
    }
  }
  c.sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs);
  return c[0];
};
const BIN = pick();
if (!BIN) {
  console.error("  ⊘ 判据前提不成立：找不到 target/release/yanshi-serve（先构建）");
  process.exit(3);
}
console.log(`  二进制：${BIN}（mtime ${statSync(BIN).mtime.toISOString()}）`);
const port = 18700 + Math.floor(Math.random() * 120);
const ROOT = trackTemp(mkdtempSync(join(tmpdir(), "bake-")));
const base = `http://127.0.0.1:${port}`;
const proc = spawn(BIN, ["--root", ROOT, "--bind", `127.0.0.1:${port}`], { stdio: "ignore" });
process.on("exit", () => { try { proc.kill(); } catch { /* 已退出 */ } });
let failed = 0;
const check = (ok, name, detail) => {
  console.log(`  ${ok ? "✓" : "✗"} **${name}**${detail ? `（${detail}）` : ""}`);
  if (!ok) failed += 1;
};
const failPre = (what, resp) => {
  console.error(`  ⊘ 判据前提不成立：${what} ⇒ ${JSON.stringify(resp).slice(0, 300)}`);
  process.exit(3);
};
const tarList = (path) => {
  const out = spawnSync("tar", ["-tf", path], { encoding: "utf8", timeout: 90_000 });
  if (out.status !== 0) failPre(`tar -tf ${path} 失败`, out.stderr);
  return out.stdout.split("\n").filter(Boolean);
};
const blobTarPath = (hash) => {
  const hex = hash.replace(/^sha256:/, "");
  return `blobs/sha256/${hex.slice(0, 2)}/${hex.slice(2, 4)}/${hex}`;
};

try {
  for (let i = 0; i < 80; i += 1) { try { if ((await fetch(`${base}/health`)).ok) break; } catch {} await sleep(250); }
  const doc = "bake_doc";
  // 注：会话文档用 ?role=owner 建（tool-import-project-layers.mjs 的既定做法）：
  // ⑪ 的跨文档导入（目标为新 doc_id）须过 ensure_doc_scope 闸，editor 令牌会被拒。
  const created = await (await fetch(`${base}/api/documents?role=owner`, { method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: 256, height: 256 }) })).json();
  const token = created.token;
  if (!token) failPre("建文档未返回 token", created);
  const call = async (tool, args) => (await fetch(base + "/api/tools/" + tool + "?doc=" + doc + "&token=" + token, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) })).json();
  const must = async (tool, args, what) => {
    const r = await call(tool, args);
    if (r.ok !== true) failPre(`${what}（${tool}）`, r);
    return r;
  };

  // ---- 前置：四层 ----
  await must("create_layer", { layer_id: "L1", name: "paint" }, "建 L1");
  await must("create_layer", { layer_id: "L2", name: "textvec" }, "建 L2");
  await must("create_layer", { layer_id: "L3", name: "empty" }, "建 L3");
  await must("create_layer", { layer_id: "L4", name: "clip" }, "建 L4");
  await must("draw_stroke", { layer_id: "L1", data: { points: [[20, 20], [120, 20], [220, 20]], size: 12, color: [200, 60, 40, 255] } }, "L1 落笔 1");
  await must("draw_stroke", { layer_id: "L1", data: { points: [[20, 60], [120, 60], [220, 60]], size: 12, color: [40, 90, 200, 255] } }, "L1 落笔 2");
  await must("draw_text", { layer_id: "L2", data: { text: "Hi", font: "builtin", size: 24, color: [0, 0, 0, 255], position: [20, 20] } }, "L2 文本");
  await must("draw_shape", { layer_id: "L2", data: { geometry: { kind: "rect", x: 20, y: 80, w: 80, h: 60 }, color: [30, 160, 80, 255] } }, "L2 形状");
  await must("draw_shape", { layer_id: "L4", data: { geometry: { kind: "rect", x: -20, y: 20, w: 100, h: 60 }, color: [200, 30, 30, 255] } }, "L4 越界形状");

  // ---- ① 纯绘画层一次成功 ----
  const b1 = await must("bake_layer", { layer_id: "L1" }, "bake L1");
  check(b1.status === "applied", "① bake 纯绘画层 status=applied", `status=${b1.status}`);
  check(b1.scope === "layer" && b1.layer_id === "L1", "① 回执 scope/layer_id", `${b1.scope}/${b1.layer_id}`);
  check(typeof b1.bake_atom_seq === "number" && b1.bake_atom_seq > 0, "① 回执 bake_atom_seq 为正数", `seq=${b1.bake_atom_seq}`);
  check(typeof b1.blob_hash === "string" && b1.blob_hash.startsWith("sha256:"), "① 回执 blob_hash 为 sha256", b1.blob_hash);
  check(b1.blob_byte_len > 0 && b1.dimensions && b1.dimensions.width === 256 && b1.dimensions.height === 256, "① 回执 blob 尺寸与画布 1:1", `${b1.blob_byte_len}B ${JSON.stringify(b1.dimensions)}`);
  check(b1.collapsed_primitives && b1.collapsed_primitives.strokes === 2, "① collapsed_primitives.strokes=2", JSON.stringify(b1.collapsed_primitives));
  check(b1.superseded_atom_count >= 2 && b1.retained_atom_count >= 0, "① superseded/retained 计数", `sup=${b1.superseded_atom_count} ret=${b1.retained_atom_count}`);
  // 注：up_to_seq = bake 提交前的 head；bake_atom_seq = head+1 ⇒ 上界应为 bake_atom_seq-1。
  check(b1.superseded_seq_range[1] === b1.bake_atom_seq - 1, "① superseded_seq_range[1] == bake_atom_seq-1", JSON.stringify(b1.superseded_seq_range));
  check(Array.isArray(b1.warnings) && b1.warnings.length === 0, "① 纯绘画层 warnings 为空", JSON.stringify(b1.warnings));

  // ---- ② 再 bake ⇒ no_op ----
  const b1again = await call("bake_layer", { layer_id: "L1" });
  check(b1again.status === "no_op_already_baked", "② 再 bake ⇒ no_op_already_baked", `status=${b1again.status}`);
  check(b1again.blob_hash === b1.blob_hash, "② no_op 不产生新 Blob（哈希不变）", `${b1again.blob_hash} vs ${b1.blob_hash}`);

  // ---- ③ 空层 ----
  const b3 = await call("bake_layer", { layer_id: "L3" });
  check(b3.status === "no_op_empty_layer", "③ 空层 ⇒ no_op_empty_layer", `status=${b3.status}`);

  // ---- ④ 文本/矢量门禁 ----
  const b2deny = await call("bake_layer", { layer_id: "L2" });
  check(b2deny.ok === false, "④ 含文本/矢量默认 is_error", JSON.stringify(b2deny).slice(0, 160));
  check(/allow_rasterize_editable/.test(JSON.stringify(b2deny)), "④ 拒绝文案点名 allow_rasterize_editable", "");
  const b2 = await must("bake_layer", { layer_id: "L2", allow_rasterize_editable: true }, "bake L2（放行）");
  const codes2 = (b2.warnings || []).map((w) => w.code);
  check(codes2.includes("TEXT_RASTERIZED") && codes2.includes("VECTOR_RASTERIZED"), "④ 放行后回执含 TEXT/VECTOR_RASTERIZED 审计条目", codes2.join(","));
  check((b2.warnings || []).every((w) => w.level === "LOSSY_EDITABILITY"), "④ 该组警告级别均为 LOSSY_EDITABILITY", "");

  // ---- ⑤ 越界门禁 ----
  // 注：L4 的越界矩形本身是矢量（Shape），默认调用会先撞栅格化门禁；
  // 越界门禁的隔离测法是：先放行栅格化，仍应被越界门禁拦住。
  const b4deny = await call("bake_layer", { layer_id: "L4", allow_rasterize_editable: true });
  check(b4deny.ok === false && /allow_clip_off_canvas/.test(JSON.stringify(b4deny)), "⑤ 越界图元（已放行栅格化）仍 is_error 且点名 allow_clip_off_canvas", JSON.stringify(b4deny).slice(0, 200));
  const b4 = await must("bake_layer", { layer_id: "L4", allow_rasterize_editable: true, allow_clip_off_canvas: true }, "bake L4（双放行）");
  const clipWarn = (b4.warnings || []).find((w) => w.code === "CLIPPED_OFF_CANVAS_PIXELS");
  check(!!clipWarn && clipWarn.level === "LOSSY_PIXELS" && Array.isArray(clipWarn.clipped_regions) && clipWarn.clipped_regions.length >= 1, "⑤ 放行后回执含 CLIPPED 区域坐标", JSON.stringify(clipWarn).slice(0, 200));

  // ---- ⑥ 压平后仿射 ----
  const objs = await must("list_objects", { layer_id: "L1" }, "列 L1 对象");
  // 注：list_objects 的对象键是 object_id，不是 id。
  const bakedList = (objs.objects || []).filter((o) => o.object_id === "baked:L1");
  check((objs.objects || []).length === 1 && bakedList.length === 1, "⑥ 压平后层内只剩 baked:L1 基底对象", JSON.stringify(objs.objects).slice(0, 300));
  const rot90 = await call("transform_object", { object_id: "baked:L1", rotate: { degrees: 90 }, anchor: { x: 128, y: 128 } });
  check(rot90.ok === true, "⑥ 压平后 90° 旋转通过", JSON.stringify(rot90).slice(0, 160));
  const rot45 = await call("transform_object", { object_id: "baked:L1", rotate: { degrees: 45 }, anchor: { x: 128, y: 128 } });
  check(rot45.ok === false, "⑥ 45° 旋转被拒绝", JSON.stringify(rot45).slice(0, 160));
  const scaleUni = await call("transform_object", { object_id: "baked:L1", scale: { x: 2, y: 2 }, anchor: { x: 0, y: 0 } });
  check(scaleUni.ok === true, "⑥ 等比缩放通过", JSON.stringify(scaleUni).slice(0, 160));
  const scaleNonUni = await call("transform_object", { object_id: "baked:L1", scale: { x: 2, y: 1 }, anchor: { x: 0, y: 0 } });
  check(scaleNonUni.ok === false, "⑥ 非均匀缩放默认被拒绝", JSON.stringify(scaleNonUni).slice(0, 160));
  const scaleNonUniOk = await call("transform_object", { object_id: "baked:L1", scale: { x: 2, y: 1 }, anchor: { x: 0, y: 0 }, allow_non_uniform_scale: true });
  check(scaleNonUniOk.ok === true, "⑥ 非均匀缩放显式放行后通过", JSON.stringify(scaleNonUniOk).slice(0, 160));
  // 再 bake：新 Blob、新水位；且渲染不再因旧 transform 叠加而失败。
  await must("draw_stroke", { layer_id: "L1", data: { points: [[30, 200], [200, 200]], size: 10, color: [20, 20, 20, 255] } }, "L1 压平后新笔");
  // 注：基底经 2 倍缩放后包围盒超出画布，再 bake 须放行越界裁剪（超出部分按
  // ClampToTransparent 裁剪，属预期行为，不是静默丢弃：警告里有区域坐标）。
  const b1b = await must("bake_layer", { layer_id: "L1", allow_clip_off_canvas: true }, "L1 再 bake");
  check(b1b.status === "applied" && b1b.blob_hash !== b1.blob_hash, "⑥ 再 bake 产生新 Blob（水位前移）", `${b1b.blob_hash} vs ${b1.blob_hash}`);
  check(b1b.superseded_seq_range[1] > b1.superseded_seq_range[1], "⑥ 再 bake 水位高于旧水位", JSON.stringify(b1b.superseded_seq_range));
  const objsAfter = await must("list_objects", { layer_id: "L1" }, "列再 bake 后 L1 对象");
  const bakedAfter = (objsAfter.objects || []).filter((o) => o.object_id === "baked:L1");
  check((objsAfter.objects || []).length === 1 && bakedAfter.length === 1, "⑥ 再 bake 后仍为单个 baked:L1 基底", JSON.stringify(objsAfter.objects).slice(0, 200));

  // ---- ⑦⑩ 缺 Blob：删 CAS 里的历史 Bake Blob，测严格报错 + Undo 拦截 + 显式恢复 ----
  // 历史 Bake = b1.blob_hash（已被 b1b 取代）；当前 Bake = b1b.blob_hash。
  // CAS 落盘位置：--root 下 blobs/（FsBlobStore）；按哈希十六进制找文件并删掉。
  const findBlobFile = (root, hash) => {
    const hex = hash.replace(/^sha256:/, "");
    const cands = [];
    const walk = (dir) => {
      for (const e of readdirSync(dir, { withFileTypes: true })) {
        const p = join(dir, e.name);
        if (e.isDirectory()) walk(p);
        else if (p.includes(hex) || e.name === hex) cands.push(p);
      }
    };
    try { walk(root); } catch {}
    return cands;
  };
  const histFiles = findBlobFile(ROOT, b1.blob_hash);
  check(histFiles.length >= 1, "⑦ 能在 CAS 里找到历史 Bake Blob 文件", histFiles.join(","));
  for (const f of histFiles) rmSync(f, { force: true });
  // Undo 当前 Bake（b1b）⇒ 恢复后生效的是历史 Bake b1，其 Blob 已删 ⇒ 必须被拦截。
  const undoHist = await call("revert", { atom_id: b1b.bake_atom_id });
  check(undoHist.ok === false && /MISSING_HISTORICAL_BLOB_ON_UNDO/.test(JSON.stringify(undoHist)), "⑩ Undo 缺历史 Blob 被原子级拦截", JSON.stringify(undoHist).slice(0, 240));
  // 状态不变：再列对象仍为再 bake 后的基底。
  const objsGuard = await must("list_objects", { layer_id: "L1" }, "拦截后列对象");
  check((objsGuard.objects || []).some((o) => o.object_id === "baked:L1"), "⑩ 拦截后状态不变（基底仍在）", "");
  // 显式恢复：recover_baked_blobs 应把历史 Blob 重建回存储。
  const rec = await must("recover_baked_blobs", { layer_id: "L1" }, "显式恢复");
  check(typeof rec.replayed_ms === "number" && typeof rec.replayed_atom_count === "number", "⑦ 恢复回执含 replayed_ms / replayed_atom_count", JSON.stringify(rec).slice(0, 200));
  check(JSON.stringify(rec.healed_blobs || rec).includes(b1.blob_hash.replace(/^sha256:/, "").slice(0, 16)) || (rec.healed_blobs || []).some((h) => h.blob_hash === b1.blob_hash), "⑦ 恢复回执含历史 blob_hash", JSON.stringify(rec.healed_blobs).slice(0, 200));
  // 恢复后 Undo 应放行。
  const undoOk = await call("revert", { atom_id: b1b.bake_atom_id });
  check(undoOk.ok === true, "⑦ 恢复后 Undo 放行", JSON.stringify(undoOk).slice(0, 200));

  // ---- ⑦b 当前 Bake 缺 Blob 时整幅渲染严格报错（不画空白层）----
  const doc2 = "bake_doc2";
  const created2 = await (await fetch(`${base}/api/documents`, { method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc2, width: 128, height: 128 }) })).json();
  if (!created2.token) failPre("建 doc2 未返回 token", created2);
  const call2 = async (tool, args) => (await fetch(`${base}/api/tools/${tool}?doc=${doc2}&token=${created2.token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) })).json();
  const must2 = async (tool, args, what) => {
    const r = await call2(tool, args);
    if (r.ok !== true) failPre(`${what}（${tool}）`, r);
    return r;
  };
  await must2("create_layer", { layer_id: "M1", name: "m" }, "建 M1");
  await must2("draw_stroke", { layer_id: "M1", data: { points: [[10, 10], [100, 10]], size: 10, color: [200, 30, 30, 255] } }, "M1 落笔");
  const bm = await must2("bake_layer", { layer_id: "M1" }, "bake M1");
  const mFiles = findBlobFile(ROOT, bm.blob_hash);
  check(mFiles.length >= 1, "⑦b 能在 CAS 里找到当前 Bake Blob 文件", mFiles.join(","));
  for (const f of mFiles) rmSync(f, { force: true });
  const renderMiss = await call2("render_region", { region: { x: 0, y: 0, w: 128, h: 128 } });
  check(renderMiss.ok === false && /missing_baked_blob|MissingBakedBlob/.test(JSON.stringify(renderMiss)), "⑦b 当前 Bake 缺 Blob 时渲染严格报错（不静默空白）", JSON.stringify(renderMiss).slice(0, 240));
  // 二期第 3 项口径：报错里的 bake_seq 必须是 Bake 原子 seq（bm.bake_atom_seq），
  // 不能是水位（doc2 里水位 = bake_atom_seq - 1；旧代码用 up_to_seq 作代理值）。
  check(new RegExp(`bake_seq=${bm.bake_atom_seq}\\b`).test(JSON.stringify(renderMiss)), "⑦b 缺 Blob 报错里的 bake_seq 是 Bake 原子 seq（非水位）", `bake_atom_seq=${bm.bake_atom_seq} resp=${JSON.stringify(renderMiss).slice(0, 240)}`);
  const rec2 = await must2("recover_baked_blobs", { layer_id: "M1" }, "doc2 显式恢复");
  check((rec2.healed_blobs || []).some((h) => h.blob_hash === bm.blob_hash), "⑦b 恢复后当前 Bake Blob 回到存储", JSON.stringify(rec2.healed_blobs).slice(0, 200));
  const renderOk = await call2("render_region", { region: { x: 0, y: 0, w: 128, h: 128 } });
  check(renderOk.ok === true, "⑦b 恢复后渲染恢复成功", JSON.stringify(renderOk).slice(0, 160));

  // ---- ⑨ 导出 frontier / all_history ----
  // 此时有效 Bake 为 b1（Undo b1b 后），历史 Bake 为 b1b；再 bake 一次造新的当前 Bake，
  // 使 b1b 成为“被新 Bake 遮蔽的历史 Blob”（frontier 应剔除它）。
  await must("draw_stroke", { layer_id: "L1", data: { points: [[40, 230], [210, 230]], size: 8, color: [90, 90, 90, 255] } }, "L1 再落笔");
  // 注：Undo 回到 b1 基底后它仍带 2 倍缩放变换（包围盒超画布），
  // 第三次 bake 须放行越界裁剪；否则门禁正确拦截。
  const b1c = await must("bake_layer", { layer_id: "L1", allow_clip_off_canvas: true }, "L1 第三次 bake");
  const pathF = join(ROOT, "bake_frontier.yanshi");
  const exF = await must("export_project", { path: pathF, blob_bundle_mode: "frontier" }, "导出 frontier");
  check(exF.blob_bundle_mode === "frontier", "⑨ frontier 回执模式字段", exF.blob_bundle_mode);
  const listF = tarList(pathF);
  check(!listF.some((l) => l.includes(blobTarPath(b1b.blob_hash))), "⑨ frontier 不含历史 Bake Blob（b1b）", `frontier_omitted=${exF.frontier_omitted_historical_blobs}`);
  check(exF.frontier_omitted_historical_blobs >= 1, "⑨ frontier_omitted_historical_blobs ≥ 1", `${exF.frontier_omitted_historical_blobs}`);
  check(listF.some((l) => l.includes(blobTarPath(b1c.blob_hash))), "⑨ frontier 含当前 Bake Blob（b1c）", "");
  const pathA = join(ROOT, "bake_allhistory.yanshi");
  await must("export_project", { path: pathA, blob_bundle_mode: "all_history" }, "导出 all_history");
  const listA = tarList(pathA);
  check(listA.some((l) => l.includes(blobTarPath(b1b.blob_hash))), "⑨ all_history 含历史 Bake Blob（b1b）", "");

  // ---- ⑪ 导入策略：从 all_history 包里剔掉当前 Bake Blob，再导入 ----
  const impDir = trackTemp(mkdtempSync(join(tmpdir(), "bake-imp-")));
  spawnSync("tar", ["-xf", pathA, "-C", impDir], { timeout: 90_000 });
  rmSync(join(impDir, blobTarPath(b1c.blob_hash)), { force: true });
  const stripped = join(ROOT, "bake_stripped.yanshi");
  // 注：必须按原包条目名重打包；用 "-C impDir ." 会给全部条目加 ./ 前缀，
  // 导入器按裸名找 atoms.jsonl 就会误报“不是工程包”。
  const entriesA = tarList(pathA);
  spawnSync("tar", ["-cf", stripped, "-C", impDir, ...entriesA], { timeout: 90_000 });
  // 注：必须跨服务器导入（第二台 --root 全新的服务器）：工作区 CAS 跨文档
  // 共享，同一服务器内源文档 bake_doc 已有 b1c 的 Blob，store.exists 恒真，
  // 导入检查永远不会触发——那不是产品放行，是判据测错了地方。跨机（新 root）
  // 才是第二轮第 10 条“跨机导入”的真实场景。
  const port2 = port + 1000;
  const ROOT2 = trackTemp(mkdtempSync(join(tmpdir(), "bake-imp-")));
  const base2 = `http://127.0.0.1:${port2}`;
  const proc2 = spawn(BIN, ["--root", ROOT2, "--bind", `127.0.0.1:${port2}`], { stdio: "ignore" });
  process.on("exit", () => { try { proc2.kill(); } catch { /* 已退出 */ } });
  for (let i = 0; i < 80; i += 1) { try { if ((await fetch(`${base2}/health`)).ok) break; } catch {} await sleep(250); }
  const sess2 = await (await fetch(`${base2}/api/documents?role=owner`, { method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: "imp_session", width: 256, height: 256 }) })).json();
  if (!sess2.token) failPre("第二服务器建会话文档未返回 token", sess2);
  const callX = async (tool, args) => (await fetch(base2 + "/api/tools/" + tool + "?doc=imp_session&token=" + sess2.token, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) })).json();
  // 注：目标文档不得预建（import_project 不覆盖已存在文档）。
  const impDeny = await callX("import_project", { path: stripped, doc_id: "bake_import_deny" });
  check(impDeny.ok === false && /缺有效压平基底|missing_baked_blob|MissingBakedBlob/.test(JSON.stringify(impDeny)), "⑪ 导入缺有效 Bake Blob 默认拒绝（缺 Blob 原因，非权限原因）", JSON.stringify(impDeny).slice(0, 240));
  const impHeal = await callX("import_project", { path: stripped, doc_id: "bake_import_heal", missing_blob_policy: "replay_and_heal" });
  check(impHeal.ok === true, "⑪ replay_and_heal 导入成功", JSON.stringify(impHeal).slice(0, 240));
  check((impHeal.baked_blobs_healed || []).some((h) => h.blob_hash === b1c.blob_hash), "⑪ 自愈回执含当前 Bake blob_hash", JSON.stringify(impHeal.baked_blobs_healed || []).slice(0, 200));

  // ---- ⑧b 真实哈希不一致：改日志造不一致，再走 strict / append_rebake ----
  // 独立文档（916 轮教训：多 case 共用文档会互相污染）。主服务先杀掉再
  // 改盘上日志——服务活着时改会被内存态覆盖；重启后沿用同一 token（token
  // 哈希落盘，重启仍认旧 token；若不认，属判据前提失败 exit(3)，不是产品红）。
  const doc8 = "bake_doc8";
  const created8 = await (await fetch(`${base}/api/documents?role=owner`, { method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc8, width: 128, height: 128 }) })).json();
  if (!created8.token) failPre("建 doc8 未返回 token", created8);
  const call8 = async (tool, args) => (await fetch(base + "/api/tools/" + tool + "?doc=" + doc8 + "&token=" + created8.token, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) })).json();
  const must8 = async (tool, args, what) => {
    const r = await call8(tool, args);
    if (r.ok !== true) failPre(`${what}（${tool}）`, r);
    return r;
  };
  await must8("create_layer", { layer_id: "L1", name: "m" }, "建 doc8 L1");
  await must8("draw_shape", { layer_id: "L1", data: { geometry: { kind: "rect", x: 20, y: 20, w: 60, h: 40 }, color: [200, 40, 30, 255] } }, "doc8 矩形");
  const b8 = await must8("bake_layer", { layer_id: "L1", allow_rasterize_editable: true }, "bake doc8");
  const b8Files = findBlobFile(ROOT, b8.blob_hash);
  check(b8Files.length >= 1, "⑧b 能在 CAS 里找到 doc8 Bake Blob 文件", b8Files.join(","));
  for (const f of b8Files) rmSync(f, { force: true });
  // 杀主服务（等健康检查彻底 down 才改盘，否则写锁/内存态会干扰）。
  try { proc.kill("SIGTERM"); } catch {}
  for (let i = 0; i < 40; i += 1) { try { await fetch(`${base}/health`); } catch { break; } await sleep(250); }
  await sleep(500);
  const atoms8Path = join(ROOT, "docs", doc8, "atoms.jsonl");
  if (!existsSync(atoms8Path)) failPre("doc8 atoms.jsonl 不在盘上", { path: atoms8Path });
  const lines8 = readFileSync(atoms8Path, "utf8").trim().split("\n").map((l) => JSON.parse(l));
  const shape8 = lines8.find((a) => a.kind === "draw_shape");
  if (!shape8 || !Array.isArray(shape8.payload?.data?.color)) failPre("doc8 日志里找不到 draw_shape 颜色", lines8.map((a) => a.kind));
  shape8.payload.data.color[0] = 201;
  writeFileSync(atoms8Path, lines8.map((l) => JSON.stringify(l)).join("\n") + "\n");
  for (const f of ["render.png", "render.seq", "preview.png", "preview.seq"]) { try { rmSync(join(ROOT, "docs", doc8, f), { force: true }); } catch {} }
  // 同一 root、新端口重启（旧端口可能仍在 TIME_WAIT；port2 已占 port+1000，故用 +2000）。
  const port8 = port + 2000;
  const base8 = `http://127.0.0.1:${port8}`;
  const proc8 = spawn(BIN, ["--root", ROOT, "--bind", `127.0.0.1:${port8}`], { stdio: "ignore" });
  process.on("exit", () => { try { proc8.kill(); } catch { /* 已退出 */ } });
  for (let i = 0; i < 80; i += 1) { try { if ((await fetch(`${base8}/health`)).ok) break; } catch {} await sleep(250); }
  const call8b = async (tool, args) => (await fetch(base8 + "/api/tools/" + tool + "?doc=" + doc8 + "&token=" + created8.token, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) })).json();
  const probe8 = await call8b("list_objects", { layer_id: "L1" });
  if (probe8.ok !== true) failPre("重启后旧 token 不被接受（判据前提不成立）", probe8);
  // 注意：不能用 export_project 给日志拍照——删 Blob 后导出本身会因“缺本
  // 体 blob”拒绝（产品行为正确）。直接读盘上 atoms.jsonl：重启已完成、
  // 服务已打开文档，此后文件只在提交时追加，是日志的权威形态。
  // 重启后先读一次（atomsA），strict/append 的前后对比都以它为准。
  const atomsA = readFileSync(atoms8Path, "utf8");
  // strict：拒绝，且错误里 bake_seq 必须是 Bake 原子 seq（不是水位）。
  const strict8 = await call8b("recover_baked_blobs", { layer_id: "L1" });
  check(strict8.ok === false && /哈希不一致|HashMismatch/.test(JSON.stringify(strict8)), "⑧b strict 下哈希不一致被拒绝", JSON.stringify(strict8).slice(0, 240));
  check(new RegExp(`bake_seq=${b8.bake_atom_seq}\\b`).test(JSON.stringify(strict8)), "⑧b strict 错误里的 bake_seq 是 Bake 原子 seq", `bake_atom_seq=${b8.bake_atom_seq} resp=${JSON.stringify(strict8).slice(0, 240)}`);
  check(strict8.context?.extra?.bake_seq === b8.bake_atom_seq && strict8.context?.extra?.expected_hash === b8.blob_hash, "⑧b strict 错误 extra 含 bake_seq 与 expected_hash", JSON.stringify(strict8.context?.extra || {}).slice(0, 200));
  // strict 不追加原子：再读盘上日志，逐字节不变。
  check(readFileSync(atoms8Path, "utf8") === atomsA, "⑧b strict 后日志逐字节不变（未追加原子）", "");
  // append_rebake：追加新 Bake 原子，历史原子不动。
  const app8 = await call8b("recover_baked_blobs", { layer_id: "L1", on_hash_mismatch: "append_rebake" });
  const healed8 = (app8.healed_blobs || [])[0] || {};
  check(app8.ok === true, "⑧b append_rebake 成功", JSON.stringify(app8).slice(0, 240));
  check(healed8.mode === "append_rebake" && healed8.bake_seq === b8.bake_atom_seq && healed8.expected_hash === b8.blob_hash && healed8.actual_hash !== b8.blob_hash && healed8.new_bake_atom_seq > b8.bake_atom_seq, "⑧b append 回执 mode/bake_seq/expected/actual/new_seq 全对", JSON.stringify(healed8).slice(0, 240));
  const hex8 = (healed8.actual_hash || "").replace(/^sha256:/, "");
  check(hex8.length === 64 && existsSync(join(ROOT, "blobs/sha256", hex8.slice(0, 2), hex8.slice(2, 4), hex8)), "⑧b 新哈希 Blob 已入库", healed8.actual_hash);
  // append 的提交落盘可能有极短延迟：轮询盘上日志至多 2 秒等新行出现。
  let atomsC = readFileSync(atoms8Path, "utf8");
  for (let i = 0; i < 8 && atomsC.trim().split("\n").length !== atomsA.trim().split("\n").length + 1; i += 1) {
    await sleep(250);
    atomsC = readFileSync(atoms8Path, "utf8");
  }
  const linesC = atomsC.trim().split("\n");
  check(atomsC.startsWith(atomsA) && linesC.length === atomsA.trim().split("\n").length + 1, "⑧b append 后历史行全保留、只多一行", `a=${atomsA.trim().split("\n").length} c=${linesC.length}`);
  const newLine = JSON.parse(linesC[linesC.length - 1]);
  check(newLine.kind === "bake_layer" && newLine.payload?.replaces_bake_seq === b8.bake_atom_seq && newLine.payload?.blob_hash === healed8.actual_hash, "⑧b 新行是 Bake 原子且 replaces_bake_seq 指旧 Bake", JSON.stringify(newLine.payload || {}).slice(0, 200));
  const render8 = await call8b("render_region", { region: { x: 0, y: 0, w: 128, h: 128 } });
  check(render8.ok === true, "⑧b append 后渲染恢复成功", JSON.stringify(render8).slice(0, 160));

  console.log(failed === 0 ? "  ✓ tool-bake-layer 全绿" : `  ✗ tool-bake-layer ${failed} 条红`);
  process.exit(failed === 0 ? 0 : 1);
} catch (error) {
  console.error(`  ⊘ 判据自身出错（不是产品问题）：${String(error).slice(0, 300)}`);
  process.exit(3);
}

// **变异点**（改坏实现必须红）：
//   · bake_layer 门禁改成只写 warnings 不报错 ⇒ ④⑤ 红；
//   · fold 的 horizon 预扫描去掉 ⇒ ② 再 bake 后层内对象数 ≠ 1（⑥ 对象检查）红；
//   · BakedBase 渲染缺 Blob 时跳过画空白 ⇒ ⑦ 恢复前渲染不报错（本判据经 Undo 拦截间接覆盖；
//     直接渲染缺 Blob 由 render.rs 分支的 Err 覆盖，单测 baked_base 分支断言）；
//   · frontier 不过滤历史 Bake ⇒ ⑨ frontier 含 b1b 红；
//   · Undo 拦截去掉 ⇒ ⑩ revert 成功红；
//   · 导入 error 策略改成放行 ⇒ ⑪ 默认导入成功红。
//   · append_rebake 分支改成 strict 拒绝 ⇒ ⑧b append 调用红；
//     append 时改写历史 Bake 原子（而非追加）⇒ ⑧b“历史行全保留、只多
//     一行”红；strict 改成静默追加 ⇒ ⑧b“strict 后日志逐字节不变”红。
