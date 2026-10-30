#!/usr/bin/env node
// **bake_document 引擎原语判据**（bake 二期：文档级逐层压平，单原子、多层条目）。
//
// 覆盖（四层齐：实现／输入触发／断言／变异，变异点见文末；设计 §7.3 八门）：
//   门一 压平前后全文档渲染一致（容差口径见 pngNear 说明：字面逐字节对含抗锯齿
//         内容在 8 位基底机制下不可达，属设计 §7.3 措辞与现实的差距，已报父 agent
//         定夺；本判据不断言做不到的逐字节，只断言最大差 ≤1）、导出 PNG 同口径；
//   门二 撤销文档压平原子后渲染与压平前逐字节一致、层内对象恢复旧集合、基底消失；
//   门三 隐藏层与纯调整层历史未被剪枝、撤销后仍可编辑；
//   门四 门禁触发时整单为工具错误、日志零修改（回执链 doc_seq 证明）、Blob 零新增
//         （门禁在渲染与 Blob 写之前，见实现顺序注释）；
//   门五 缺当前文档基底时常规渲染严格报错、recover_baked_blobs 可恢复文档基底、
//         哈希不一致缺省 strict（与一期同代码路径，单测覆盖；本判据不造不一致——
//         重放位图由同一日志渲染，哈希恒一致，与一期判据 ⑧ 同口径）；
//   门六 撤销守卫在缺历史文档基底时拦截并给出自愈指引；
//   门七 frontier 包不含历史文档基底且计数正确、all_history 包含历史文档基底；
//   门八 对象形态统一：文档压平基底可被后续单层 bake_layer 再压平、
//         transform 重置为 IDENTITY（以再 bake 后 bake_layer 走 no_op 证明，
//         不用变换后全幅渲染像素断言——几何带入语义检查已证实该渲染路径丢变换）；
//   D11 flatten_mask=true 端到端合成（bake_layer 与 bake_document 两工具）
//         + 压平后掩膜编辑拒绝、revert 后解除；
//   D13 组层入围：父组不可见而子层 visible=true 的子层跳过（HIDDEN_LAYER）；
//         组层有自有对象按普通层入围；组层无自有对象不是候选；
//         组不透明度继承在合成时重算（压平前后渲染一致证明）。
//
// 前提失败 exit(3)（判据前提不成立 ≠ 产品失败）；产品失败 exit(1)。
// 用法：node scripts/tool-bake-document.mjs
//   （须先有新鲜的 target/release/yanshi-serve；见 run-criteria-local.sh）

process.on("uncaughtException", (error) => {
  console.error(`  ⊘ 判据自身出错（不是产品问题）：${String(error).slice(0, 200)}`);
  process.exit(3);
});
import { spawn, spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, existsSync, statSync, rmSync, readdirSync } from "node:fs";
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
const port = 18720 + Math.floor(Math.random() * 120);
const ROOT = trackTemp(mkdtempSync(join(tmpdir(), "bakedoc-")));
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
const pngEq = (a, b) => {
  const ba = readFileSync(a);
  const bb = readFileSync(b);
  return ba.equals(bb);
};
// **渲染一致的容差口径（设计 §7.3 门一字面“逐字节一致”的实测修正，见文件头说明）**：
// 解码两张 RGBA8 PNG，断言每通道最大差 ≤1 且差异像素 ≤ 画布 10%。±1 的机制已实测：
// 笔触抗锯齿边缘像素经“孤立渲染存 8 位基底 + 再合成”两次 8 位舍入，相对直接合成的
// 一次舍入差 1；同一笔触用一期 bake_layer 对照复现出完全相同的 148 像素/最大差 1，
// 故这是 8 位基底机制的通用属性，不是本支回归。字面逐字节只在无 bake 参与的比较中
// 成立（撤销恢复、no-op 哈希稳定），本判据对那些仍用 pngEq 逐字节断言。
import { inflateSync } from "node:zlib";
const decodePngRgba = (path) => {
  const d = readFileSync(path);
  let pos = 8; let idat = Buffer.alloc(0); let w = 0; let h = 0;
  while (pos < d.length) {
    const len = d.readUInt32BE(pos); const typ = d.toString("ascii", pos + 4, pos + 8);
    const data = d.subarray(pos + 8, pos + 8 + len);
    if (typ === "IHDR") { w = data.readUInt32BE(0); h = data.readUInt32BE(4); }
    else if (typ === "IDAT") idat = Buffer.concat([idat, data]);
    pos += 12 + len;
  }
  const raw = inflateSync(idat); const stride = w * 4; const out = Buffer.alloc(w * h * 4);
  let p = 0; let prev = Buffer.alloc(stride);
  for (let y = 0; y < h; y += 1) {
    const f = raw[p]; const line = Buffer.from(raw.subarray(p + 1, p + 1 + stride)); p += 1 + stride;
    for (let i = 0; i < stride; i += 1) {
      const a = i >= 4 ? line[i - 4] : 0; const b = prev[i]; const c = i >= 4 ? prev[i - 4] : 0;
      let pr = 0;
      if (f === 1) pr = a; else if (f === 2) pr = b; else if (f === 3) pr = (a + b) >> 1;
      else if (f === 4) { const pp = a + b - c; const pa = Math.abs(pp - a); const pb = Math.abs(pp - b); const pc = Math.abs(pp - c); pr = (pa <= pb && pa <= pc) ? a : (pb <= pc ? b : c); }
      line[i] = (line[i] + pr) & 0xff;
    }
    line.copy(out, y * stride); prev = line;
  }
  return { w, h, px: out };
};
// maxDelta 缺省 1（两次舍入：孤立渲染 + 合成）；掩膜情形传 2（多一次掩膜
// 透明度 8 位量化：梯度→基底 alpha→合成，三步累积可达 2，实测好构建即 2）。
const pngNear = (a, b, maxDelta = 1) => {
  const A = decodePngRgba(a); const B = decodePngRgba(b);
  if (A.w !== B.w || A.h !== B.h) return { ok: false, detail: `尺寸不同 ${A.w}x${A.h} vs ${B.w}x${B.h}` };
  let diffPx = 0; let maxd = 0;
  for (let i = 0; i < A.px.length; i += 4) {
    let pd = 0;
    for (let k = 0; k < 4; k += 1) { const dd = Math.abs(A.px[i + k] - B.px[i + k]); if (dd > pd) pd = dd; }
    if (pd > 0) { diffPx += 1; if (pd > maxd) maxd = pd; }
  }
  const ok = maxd <= maxDelta && diffPx <= Math.ceil(A.w * A.h / 10);
  return { ok, detail: `差异像素 ${diffPx} 最大差 ${maxd}` };
};

try {
  for (let i = 0; i < 80; i += 1) { try { if ((await fetch(`${base}/health`)).ok) break; } catch {} await sleep(250); }
  const mkDoc = async (doc, w, h) => {
    const created = await (await fetch(`${base}/api/documents?role=owner`, { method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ doc_id: doc, width: w, height: h }) })).json();
    if (!created.token) failPre(`建文档 ${doc} 未返回 token`, created);
    const call = async (tool, args) => (await fetch(base + "/api/tools/" + tool + "?doc=" + doc + "&token=" + created.token, {
      method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) })).json();
    const must = async (tool, args, what) => {
      const r = await call(tool, args);
      if (r.ok !== true) failPre(`${what}（${tool}）`, r);
      return r;
    };
    return { call, must };
  };

  // ================= 门一 + 门一b：文档 A（两绘画层 + 空层 + 隐藏层 + 纯调整层）=====================
  const A = await mkDoc("bakedoc_a", 256, 256);
  await A.must("create_layer", { layer_id: "L1", name: "paint1", z_index: 1 }, "A 建 L1");
  await A.must("create_layer", { layer_id: "L2", name: "paint2", z_index: 2 }, "A 建 L2");
  await A.must("create_layer", { layer_id: "L3", name: "empty", z_index: 3 }, "A 建 L3");
  await A.must("create_layer", { layer_id: "L4", name: "hidden", z_index: 4 }, "A 建 L4");
  await A.must("create_layer", { layer_id: "L5", name: "adj", z_index: 5 }, "A 建 L5");
  await A.must("draw_stroke", { layer_id: "L1", data: { points: [[20, 20], [120, 20], [220, 20]], size: 12, color: [200, 60, 40, 255] } }, "A L1 落笔 1");
  await A.must("draw_stroke", { layer_id: "L1", data: { points: [[20, 60], [120, 60], [220, 60]], size: 12, color: [40, 90, 200, 255] } }, "A L1 落笔 2");
  await A.must("draw_stroke", { layer_id: "L2", data: { points: [[20, 100], [120, 100], [220, 100]], size: 12, color: [30, 160, 80, 255] } }, "A L2 落笔");
  await A.must("draw_stroke", { layer_id: "L4", data: { points: [[20, 140], [120, 140], [220, 140]], size: 12, color: [90, 90, 90, 255] } }, "A L4 落笔");
  await A.must("update_layer", { layer_id: "L4", patch: { visible: false } }, "A 藏 L4");
  await A.must("add_adjustment", { layer_id: "L5", adjustment_type: "levels", params: {} }, "A L5 调整对象");
  const preA = join(ROOT, "preA.png");
  await A.must("export_png", { path: preA }, "A 压平前导出");
  const d1 = await A.must("bake_document", {}, "A 文档压平");
  check(d1.status === "applied" && d1.scope === "document", "门一 文档压平 status=applied/scope=document", `${d1.status}/${d1.scope}`);
  check(typeof d1.bake_atom_seq === "number" && d1.document_up_to_seq === d1.bake_atom_seq - 1, "门一 document_up_to_seq == bake_atom_seq-1（调用开始快照）", `doc_seq=${d1.document_up_to_seq} bake_seq=${d1.bake_atom_seq}`);
  // 建文档自带 layer_default 空层（隐式，不占日志原子），故 no_op 与 skipped 各多 1。
  check(d1.layer_count_applied === 2 && d1.layer_count_no_op === 2 && d1.layer_count_skipped === 4, "门一 三计数 applied=2/no_op=2/skipped=4（含 layer_default 空层）", `${d1.layer_count_applied}/${d1.layer_count_no_op}/${d1.layer_count_skipped}`);
  const layerIds = (d1.layers || []).map((l) => l.layer_id);
  check(JSON.stringify(layerIds) === JSON.stringify(["layer_default", "L1", "L2", "L3"]), "门一 layers[] 按 Z 序排列（layer_default/L1/L2/L3）", layerIds.join(","));
  const l1 = (d1.layers || []).find((l) => l.layer_id === "L1");
  const l2 = (d1.layers || []).find((l) => l.layer_id === "L2");
  const l3 = (d1.layers || []).find((l) => l.layer_id === "L3");
  check(l1 && l1.status === "applied" && l1.bake_atom_seq === d1.bake_atom_seq && l1.superseded_atom_count >= 2, "门一 L1 applied 且取代计数 ≥2", JSON.stringify(l1 && l1.superseded_atom_count));
  check(l2 && l2.status === "applied" && l2.superseded_atom_count >= 1, "门一 L2 applied 且取代计数 ≥1", JSON.stringify(l2 && l2.superseded_atom_count));
  check(l1 && l1.superseded_seq_range[1] === d1.document_up_to_seq, "门一 applied 层取代上界 == document_up_to_seq", JSON.stringify(l1 && l1.superseded_seq_range));
  check(l3 && l3.status === "no_op_empty_layer" && l3.blob_hash === null, "门一 L3 no_op_empty_layer 且 blob 为空", l3 && l3.status);
  const skipReasons = Object.fromEntries((d1.skipped_layers || []).map((s) => [s.layer_id, s.reason]));
  check(skipReasons.layer_default === "EMPTY_LAYER" && skipReasons.L3 === "EMPTY_LAYER" && skipReasons.L4 === "HIDDEN_LAYER" && skipReasons.L5 === "ADJUSTMENT_ONLY_LAYER", "门一 跳过清单原因码（空/隐藏/纯调整）", JSON.stringify(skipReasons));
  check((d1.warnings || []).every((w) => typeof w.layer_id === "string"), "门一 聚合警告每条含层 ID", "");
  check(["HIDDEN_LAYER_SKIPPED", "ADJUSTMENT_ONLY_LAYER_SKIPPED", "NO_OP_EMPTY_LAYER"].every((c) => (d1.warnings || []).some((w) => w.code === c)), "门一 聚合警告含三条跳过/no-op 码", (d1.warnings || []).map((w) => w.code).join(","));
  check(d1.total_blob_byte_len === (d1.layers || []).filter((l) => l.status === "applied").reduce((s, l) => s + (l.blob_byte_len || 0), 0), "门一 total_blob_byte_len == applied 层字节和", `${d1.total_blob_byte_len}`);
  const postA = join(ROOT, "postA.png");
  await A.must("export_png", { path: postA }, "A 压平后导出");
  const nearA = pngNear(preA, postA);
  check(nearA.ok, "门一 压平前后导出 PNG 一致（容差 ±1，见 pngNear 说明）", nearA.detail);
  const objsA1 = await A.must("list_objects", { layer_id: "L1" }, "A 列 L1 对象");
  check((objsA1.objects || []).length === 1 && (objsA1.objects || [])[0].object_id === "baked:L1", "门一 压平后 L1 层内只剩 baked:L1", JSON.stringify(objsA1.objects).slice(0, 200));

  // ---- 门一b：再压平 ⇒ 全 no-op；新笔后只 L1 重压平 ----
  const d1again = await A.call("bake_document", {});
  check(d1again.status === "no_op_all_already_baked" && d1again.bake_atom_seq === null, "门一b 再压平 ⇒ no_op_all_already_baked 且无新原子", `${d1again.status}`);
  check((d1again.layers || []).find((l) => l.layer_id === "L1").blob_hash === l1.blob_hash, "门一b no-op 不产生新 Blob（L1 哈希不变）", "");
  await A.must("draw_stroke", { layer_id: "L1", data: { points: [[20, 180], [120, 180]], size: 10, color: [20, 20, 20, 255] } }, "A L1 新笔");
  const d1b = await A.must("bake_document", {}, "A L1 新笔后重压平");
  check(d1b.status === "applied", "门一b 新笔后重压平 status=applied", d1b.status);
  const d1bL1 = (d1b.layers || []).find((l) => l.layer_id === "L1");
  const d1bL2 = (d1b.layers || []).find((l) => l.layer_id === "L2");
  check(d1bL1 && d1bL1.status === "applied" && d1bL1.blob_hash !== l1.blob_hash, "门一b 仅 L1 重压平且新哈希", "");
  check(d1bL2 && d1bL2.status === "no_op_already_baked", "门一b L2 仍 no_op（覆盖表只含 applied 层，D12）", d1bL2 && d1bL2.status);
  check(!(d1b.skipped_layers || []).some((s) => s.layer_id === "L1" || s.layer_id === "L2"), "门一b applied/no-op 层不在跳过清单", "");

  // ================= 门二：文档 B，撤销后渲染与对象双恢复 =================
  const B = await mkDoc("bakedoc_b", 256, 256);
  await B.must("create_layer", { layer_id: "L1", name: "paint", z_index: 1 }, "B 建 L1");
  await B.must("create_layer", { layer_id: "L2", name: "paint2", z_index: 2 }, "B 建 L2");
  await B.must("draw_stroke", { layer_id: "L1", data: { points: [[20, 20], [120, 20]], size: 12, color: [200, 60, 40, 255] } }, "B L1 落笔 1");
  await B.must("draw_stroke", { layer_id: "L1", data: { points: [[20, 60], [120, 60]], size: 12, color: [40, 90, 200, 255] } }, "B L1 落笔 2");
  await B.must("draw_stroke", { layer_id: "L2", data: { points: [[20, 100], [120, 100]], size: 12, color: [30, 160, 80, 255] } }, "B L2 落笔");
  const preB = join(ROOT, "preB.png");
  await B.must("export_png", { path: preB }, "B 压平前导出");
  const dB = await B.must("bake_document", {}, "B 文档压平");
  const postB = join(ROOT, "postB.png");
  await B.must("export_png", { path: postB }, "B 压平后导出");
  const nearB = pngNear(preB, postB);
  check(nearB.ok, "门二 压平后渲染与压平前一致（容差 ±1，门一复核）", nearB.detail);
  const revB = await B.call("revert", { atom_id: dB.bake_atom_id });
  check(revB.ok === true, "门二 撤销文档压平原子成功（单原子整体撤销）", JSON.stringify(revB).slice(0, 160));
  const postRevB = join(ROOT, "postRevB.png");
  await B.must("export_png", { path: postRevB }, "B 撤销后导出");
  check(pngEq(preB, postRevB), "门二 撤销后渲染与压平前逐字节一致", "");
  const objsB = await B.must("list_objects", { layer_id: "L1" }, "B 撤销后列 L1");
  check((objsB.objects || []).length === 2 && !(objsB.objects || []).some((o) => o.object_id === "baked:L1"), "门二 撤销后层内对象恢复旧集合、基底消失", JSON.stringify(objsB.objects).slice(0, 200));
  await B.must("draw_stroke", { layer_id: "L1", data: { points: [[20, 180], [120, 180]], size: 10, color: [20, 20, 20, 255] } }, "B 撤销后 L1 仍可编辑");

  // ================= 门三：文档 A 隐藏层与调整层历史未被剪枝、撤销后可编辑 =================
  const objsA4 = await A.must("list_objects", { layer_id: "L4" }, "A 列隐藏层 L4");
  check((objsA4.objects || []).length === 1, "门三 隐藏层历史未被剪枝（压平后对象仍在）", JSON.stringify(objsA4.objects).slice(0, 200));
  await A.must("add_adjustment", { layer_id: "L5", adjustment_type: "levels", params: {} }, "A 调整层压平后仍可加调整");
  const revA2 = await A.call("revert", { atom_id: d1b.bake_atom_id });
  check(revA2.ok === true, "门三 撤销 A 第二次文档压平", JSON.stringify(revA2).slice(0, 160));
  const revA1 = await A.call("revert", { atom_id: d1.bake_atom_id });
  check(revA1.ok === true, "门三 撤销 A 第一次文档压平", JSON.stringify(revA1).slice(0, 160));
  await A.must("update_layer", { layer_id: "L4", patch: { visible: true } }, "A 恢复 L4 可见");
  const preA3 = join(ROOT, "preA3.png");
  await A.must("export_png", { path: preA3 }, "A 撤销后新笔前导出");
  await A.must("draw_stroke", { layer_id: "L4", data: { points: [[20, 200], [120, 200]], size: 10, color: [200, 200, 20, 255] } }, "A 撤销后隐藏层可再落笔");
  const postA3 = join(ROOT, "postA3.png");
  await A.must("export_png", { path: postA3 }, "A 撤销后新笔后导出");
  check(!pngEq(preA3, postA3), "门三 撤销后隐藏层再落笔画面变化（仍可编辑）", "");

  // ================= 门四：文档 C，门禁整单错误 + 日志零修改（回执链）=====================
  // 门禁前不设锚点 bake：锚点会把文本先行压平，使门禁调用无文本候选、恒放行
  // （上一版即因此假放行）。零修改证明改用门禁前 head：失败调用后放行重压平的
  // document_up_to_seq 必须等于门禁前 head（失败调用未落原子）。
  const C = await mkDoc("bakedoc_c", 256, 256);
  await C.must("create_layer", { layer_id: "L1", name: "paint", z_index: 1 }, "C 建 L1");
  await C.must("create_layer", { layer_id: "L2", name: "text", z_index: 2 }, "C 建 L2");
  await C.must("draw_stroke", { layer_id: "L1", data: { points: [[20, 20], [120, 20]], size: 12, color: [200, 60, 40, 255] } }, "C L1 落笔");
  const textC = await C.must("draw_text", { layer_id: "L2", data: { text: "Hi", font: "builtin", size: 24, color: [0, 0, 0, 255], position: [20, 20] } }, "C L2 文本");
  const gateHead = textC.head ?? textC.head_seq;
  if (typeof gateHead !== "number") failPre("C 文本回执无 head（门禁前 head 前提不成立）", textC);
  const denyC = await C.call("bake_document", {});
  check(denyC.ok === false && /allow_rasterize_editable/.test(JSON.stringify(denyC)), "门四 含文本默认整单 is_error 且点名 allow_rasterize_editable", JSON.stringify(denyC).slice(0, 200));
  const dC2 = await C.must("bake_document", { allow_rasterize_editable: true }, "C 门禁失败后放行重压平");
  check(dC2.document_up_to_seq === gateHead, "门四 失败调用日志零修改（放行快照 == 门禁前 head）", `doc_seq=${dC2.document_up_to_seq} gate_head=${gateHead}`);
  const dC2L2 = (dC2.layers || []).find((l) => l.layer_id === "L2");
  check(dC2L2 && (dC2L2.warnings || []).some((w) => w.code === "TEXT_RASTERIZED"), "门四 放行后 L2 回执含 TEXT_RASTERIZED 审计条目", "");
  // 越界门禁：L4 越界形状（先放行栅格化，仍应被越界门禁拦住）。
  await C.must("create_layer", { layer_id: "L4", name: "clip", z_index: 4 }, "C 建 L4");
  await C.must("draw_shape", { layer_id: "L4", data: { geometry: { kind: "rect", x: -20, y: 20, w: 100, h: 60 }, color: [200, 30, 30, 255] } }, "C L4 越界形状");
  const denyClip = await C.call("bake_document", { allow_rasterize_editable: true });
  check(denyClip.ok === false && /allow_clip_off_canvas/.test(JSON.stringify(denyClip)), "门四 越界图元（已放行栅格化）仍整单 is_error 且点名 allow_clip_off_canvas", JSON.stringify(denyClip).slice(0, 200));
  const dC3 = await C.must("bake_document", { allow_rasterize_editable: true, allow_clip_off_canvas: true }, "C 双放行压平");
  const dC3L4 = (dC3.layers || []).find((l) => l.layer_id === "L4");
  check(dC3L4 && (dC3L4.warnings || []).some((w) => w.code === "CLIPPED_OFF_CANVAS_PIXELS"), "门四 双放行后 L4 回执含 CLIPPED 区域坐标", "");

  // ================= 门五：文档 D，缺当前文档基底严格报错 + 显式恢复 =================
  const D = await mkDoc("bakedoc_d", 256, 256);
  await D.must("create_layer", { layer_id: "L1", name: "paint", z_index: 1 }, "D 建 L1");
  await D.must("create_layer", { layer_id: "L2", name: "paint2", z_index: 2 }, "D 建 L2");
  await D.must("draw_stroke", { layer_id: "L1", data: { points: [[20, 20], [120, 20]], size: 12, color: [200, 60, 40, 255] } }, "D L1 落笔");
  await D.must("draw_stroke", { layer_id: "L2", data: { points: [[20, 60], [120, 60]], size: 12, color: [40, 90, 200, 255] } }, "D L2 落笔");
  const dD = await D.must("bake_document", {}, "D 文档压平");
  const dDL1 = (dD.layers || []).find((l) => l.layer_id === "L1");
  const dDL2 = (dD.layers || []).find((l) => l.layer_id === "L2");
  const dFiles = [...findBlobFile(ROOT, dDL1.blob_hash), ...findBlobFile(ROOT, dDL2.blob_hash)];
  check(dFiles.length >= 2, "门五 能在 CAS 里找到两个当前文档基底文件", dFiles.join(","));
  for (const f of dFiles) rmSync(f, { force: true });
  const renderMiss = await D.call("render_region", { region: { x: 0, y: 0, w: 256, h: 256 } });
  check(renderMiss.ok === false && /missing_baked_blob|MissingBakedBlob/.test(JSON.stringify(renderMiss)), "门五 缺当前文档基底时渲染严格报错（不静默空白）", JSON.stringify(renderMiss).slice(0, 240));
  const recD = await D.must("recover_baked_blobs", {}, "D 显式恢复");
  check((recD.healed_blobs || []).some((h) => h.blob_hash === dDL1.blob_hash) && (recD.healed_blobs || []).some((h) => h.blob_hash === dDL2.blob_hash), "门五 recover 恢复两个文档基底", JSON.stringify(recD.healed_blobs).slice(0, 300));
  const renderOkD = await D.call("render_region", { region: { x: 0, y: 0, w: 256, h: 256 } });
  check(renderOkD.ok === true, "门五 恢复后渲染恢复成功", JSON.stringify(renderOkD).slice(0, 160));

  // ================= 门六：文档 E，缺历史文档基底时撤销守卫拦截 =================
  const E = await mkDoc("bakedoc_e", 256, 256);
  await E.must("create_layer", { layer_id: "L1", name: "paint", z_index: 1 }, "E 建 L1");
  await E.must("draw_stroke", { layer_id: "L1", data: { points: [[20, 20], [120, 20]], size: 12, color: [200, 60, 40, 255] } }, "E L1 落笔 1");
  const dE1 = await E.must("bake_document", {}, "E 第一次文档压平");
  const hEA = (dE1.layers || []).find((l) => l.layer_id === "L1").blob_hash;
  await E.must("draw_stroke", { layer_id: "L1", data: { points: [[20, 60], [120, 60]], size: 12, color: [40, 90, 200, 255] } }, "E L1 落笔 2");
  const preE2 = join(ROOT, "preE2.png");
  await E.must("export_png", { path: preE2 }, "E 第二次压平前导出（第一次基底 + 其间新笔）");
  const dE2 = await E.must("bake_document", {}, "E 第二次文档压平");
  const hEB = (dE2.layers || []).find((l) => l.layer_id === "L1").blob_hash;
  check(hEB !== hEA, "门六 两次压平哈希不同（历史基底与当前基底区分）", "");
  const histFiles = findBlobFile(ROOT, hEA);
  check(histFiles.length >= 1, "门六 能在 CAS 里找到历史文档基底文件", histFiles.join(","));
  for (const f of histFiles) rmSync(f, { force: true });
  const undoGuard = await E.call("revert", { atom_id: dE2.bake_atom_id });
  check(undoGuard.ok === false && /MISSING_HISTORICAL_BLOB_ON_UNDO/.test(JSON.stringify(undoGuard)), "门六 缺历史文档基底时撤销被原子级拦截", JSON.stringify(undoGuard).slice(0, 240));
  const objsGuard = await E.must("list_objects", { layer_id: "L1" }, "E 拦截后列对象");
  check((objsGuard.objects || []).some((o) => o.object_id === "baked:L1"), "门六 拦截后状态不变（基底仍在）", "");
  await E.must("recover_baked_blobs", { layer_id: "L1" }, "E 显式恢复历史基底");
  const undoOkE = await E.call("revert", { atom_id: dE2.bake_atom_id });
  check(undoOkE.ok === true, "门六 恢复后撤销放行", JSON.stringify(undoOkE).slice(0, 200));
  const objsAfterE = await E.must("list_objects", { layer_id: "L1" }, "E 撤销后列对象");
  // 正确语义：撤销第二次压平后，第一次基底重新生效，且其间新笔仍在世
  // （其间新笔从未被第一次压平取代；上一版“只剩基底”的期望本身错了）。
  const idsE = (objsAfterE.objects || []).map((o) => o.object_id);
  check(idsE.length === 2 && idsE.includes("baked:L1"), "门六 撤销后历史基底重新生效 + 其间新笔仍在世", idsE.join(","));
  const postRevE = join(ROOT, "postRevE.png");
  await E.must("export_png", { path: postRevE }, "E 撤销后导出");
  check(pngEq(preE2, postRevE), "门六 撤销后渲染与第二次压平前逐字节一致（无 bake 参与的比较，逐字节成立）", "");

  // ================= 门七：文档 E 继续，frontier / all_history + 跨机导入 =================
  await E.must("draw_stroke", { layer_id: "L1", data: { points: [[20, 100], [120, 100]], size: 10, color: [20, 20, 20, 255] } }, "E L1 落笔 3");
  const dE3 = await E.must("bake_document", {}, "E 第三次文档压平");
  const hEC = (dE3.layers || []).find((l) => l.layer_id === "L1").blob_hash;
  const pathF = join(ROOT, "bakedoc_frontier.yanshi");
  const exF = await E.must("export_project", { path: pathF, blob_bundle_mode: "frontier" }, "E 导出 frontier");
  const listF = tarList(pathF);
  check(!listF.some((l) => l.includes(blobTarPath(hEA))), "门七 frontier 不含历史文档基底（hEA）", `frontier_omitted=${exF.frontier_omitted_historical_blobs}`);
  check(exF.frontier_omitted_historical_blobs >= 1, "门七 frontier_omitted_historical_blobs ≥ 1", `${exF.frontier_omitted_historical_blobs}`);
  check(listF.some((l) => l.includes(blobTarPath(hEC))), "门七 frontier 含当前文档基底（hEC）", "");
  const pathA7 = join(ROOT, "bakedoc_allhistory.yanshi");
  await E.must("export_project", { path: pathA7, blob_bundle_mode: "all_history" }, "E 导出 all_history");
  const listA7 = tarList(pathA7);
  check(listA7.some((l) => l.includes(blobTarPath(hEA))), "门七 all_history 含历史文档基底（hEA）", "");
  // 跨机导入 frontier 包：直接打开当前态；再撤销当前压平 ⇒ 历史基底缺失 ⇒ 守卫拦截。
  const port2 = port + 1000;
  const ROOT2 = trackTemp(mkdtempSync(join(tmpdir(), "bakedoc-imp-")));
  const base2 = `http://127.0.0.1:${port2}`;
  const proc2 = spawn(BIN, ["--root", ROOT2, "--bind", `127.0.0.1:${port2}`], { stdio: "ignore" });
  process.on("exit", () => { try { proc2.kill(); } catch { /* 已退出 */ } });
  for (let i = 0; i < 80; i += 1) { try { if ((await fetch(`${base2}/health`)).ok) break; } catch {} await sleep(250); }
  const sess2 = await (await fetch(`${base2}/api/documents?role=owner`, { method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: "imp_session", width: 256, height: 256 }) })).json();
  if (!sess2.token) failPre("第二服务器建会话文档未返回 token", sess2);
  const callX = async (tool, args, docId) => (await fetch(base2 + "/api/tools/" + tool + "?doc=" + (docId || "imp_session") + "&token=" + sess2.token, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) })).json();
  const impF = await callX("import_project", { path: pathF, doc_id: "bakedoc_import_f" });
  check(impF.ok === true, "门七 frontier 包跨机导入成功（直接打开当前态）", JSON.stringify(impF).slice(0, 240));
  // 导入文档须用自己的 token：会话 token 按文档绑定，imp_session 的 token 调
  // bakedoc_import_f 会报 capability token 无效（上一版即因此假红）；对已存在
  // 文档再 POST /api/documents?role=owner 即返回该文档的 token。
  const sessImp = await (await fetch(`${base2}/api/documents?role=owner`, { method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: "bakedoc_import_f", width: 256, height: 256 }) })).json();
  if (!sessImp.token) failPre("导入文档取 token 失败", sessImp);
  const callImp = async (tool, args) => (await fetch(base2 + "/api/tools/" + tool + "?doc=bakedoc_import_f&token=" + sessImp.token, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) })).json();
  const renderImp = await callImp("render_region", { region: { x: 0, y: 0, w: 256, h: 256 } });
  check(renderImp.ok === true, "门七 导入后当前态可渲染", JSON.stringify(renderImp).slice(0, 160));
  const undoImp = await callImp("revert", { atom_id: dE3.bake_atom_id });
  check(undoImp.ok === false && /MISSING_HISTORICAL_BLOB_ON_UNDO/.test(JSON.stringify(undoImp)), "门七 导入后撤销当前压平 ⇒ 历史基底已被 frontier 剔除 ⇒ 守卫拦截并指引自愈", JSON.stringify(undoImp).slice(0, 240));

  // ================= 门八：文档 F，文档压平基底可被单层 bake 再压平 =================
  const F = await mkDoc("bakedoc_f", 256, 256);
  await F.must("create_layer", { layer_id: "L1", name: "paint", z_index: 1 }, "F 建 L1");
  await F.must("draw_stroke", { layer_id: "L1", data: { points: [[20, 20], [120, 20]], size: 12, color: [200, 60, 40, 255] } }, "F L1 落笔");
  const dF = await F.must("bake_document", {}, "F 文档压平");
  const hFD = (dF.layers || []).find((l) => l.layer_id === "L1").blob_hash;
  const objsF = await F.must("list_objects", { layer_id: "L1" }, "F 列对象");
  check((objsF.objects || []).length === 1 && (objsF.objects || [])[0].object_id === "baked:L1", "门八 文档压平后对象形态为 baked:L1（与单层同形态）", "");
  await F.must("draw_stroke", { layer_id: "L1", data: { points: [[20, 60], [120, 60]], size: 12, color: [40, 90, 200, 255] } }, "F L1 新笔");
  const bF = await F.must("bake_layer", { layer_id: "L1" }, "F 单层再压平文档基底");
  check(bF.status === "applied" && bF.blob_hash !== hFD, "门八 单层 bake 可再压平文档基底且新哈希", `${bF.blob_hash} vs ${hFD}`);
  const bFagain = await F.call("bake_layer", { layer_id: "L1" });
  check(bFagain.status === "no_op_already_baked", "门八 再压平后单层 bake 走 no_op（transform 已重置 IDENTITY）", bFagain.status);

  // ================= D11：flatten_mask 端到端（两工具）+ 掩膜编辑守卫 =================
  const mkMaskDoc = async (doc, bakeCall) => {
    const M = await mkDoc(doc, 256, 256);
    await M.must("create_layer", { layer_id: "L1", name: "paint", z_index: 1 }, `${doc} 建 L1`);
    await M.must("draw_stroke", { layer_id: "L1", data: { points: [[20, 20], [220, 20]], size: 12, color: [200, 60, 40, 255] } }, `${doc} L1 落笔`);
    await M.must("draw_stroke", { layer_id: "L1", data: { points: [[20, 60], [220, 60]], size: 12, color: [40, 90, 200, 255] } }, `${doc} L1 落笔 2`);
    const unmasked = join(ROOT, `${doc}_unmasked.png`);
    await M.must("export_png", { path: unmasked }, `${doc} 未挂掩膜导出`);
    // 掩膜必须用线性渐变（全幅过渡带）：二值矩形掩膜对二次作用是幂等的
    // （乘两次与一次同像），去消费变异在其上恒绿、测不出 bug；渐变掩膜的
    // 中间透明度二次作用后平方（0.5→0.25），差值数十，变异必红。
    await M.must("create_mask", { mask_id: "m1", shape: { kind: "linear_gradient", start: [0, 0], end: [256, 0] } }, `${doc} 建渐变掩膜`);
    await M.must("update_layer", { layer_id: "L1", patch: { mask_id: "m1" } }, `${doc} 挂掩膜`);
    const masked = join(ROOT, `${doc}_masked.png`);
    await M.must("export_png", { path: masked }, `${doc} 挂掩膜后导出`);
    if (pngEq(unmasked, masked)) failPre(`${doc} 渐变掩膜未改变画面（掩膜语义前提不成立）`, {});
    const baked = await bakeCall(M);
    const postFlat = join(ROOT, `${doc}_flat.png`);
    await M.must("export_png", { path: postFlat }, `${doc} flatten 后导出`);
    const nearFlat = pngNear(masked, postFlat, 2);
    check(nearFlat.ok, `D11 ${doc} flatten_mask=true 压平前后渲染一致（容差 ±2，掩膜三步量化，无二次作用）`, nearFlat.detail);
    const relink = await M.call("update_layer", { layer_id: "L1", patch: { mask_id: "m1" } });
    check(relink.ok === false && /revert/.test(JSON.stringify(relink)), `D11 ${doc} flatten 后掩膜编辑被拒绝并明示 revert`, JSON.stringify(relink).slice(0, 200));
    const revM = await M.call("revert", { atom_id: baked.bake_atom_id });
    check(revM.ok === true, `D11 ${doc} revert 压平原子成功`, JSON.stringify(revM).slice(0, 160));
    const relinkOk = await M.call("update_layer", { layer_id: "L1", patch: { mask_id: "m1" } });
    check(relinkOk.ok === true, `D11 ${doc} revert 后掩膜编辑解除`, JSON.stringify(relinkOk).slice(0, 200));
  };
  await mkMaskDoc("bakedoc_g", async (M) => {
    const b = await M.must("bake_layer", { layer_id: "L1", flatten_mask: true }, "G 单层 flatten");
    check((b.warnings || []).some((w) => w.code === "MASK_FLATTENED"), "D11 单层 flatten 回执含 MASK_FLATTENED", "");
    return b;
  });
  await mkMaskDoc("bakedoc_h", async (M) => {
    const b = await M.must("bake_document", { flatten_mask: true }, "H 文档 flatten");
    const l1 = (b.layers || []).find((l) => l.layer_id === "L1");
    check(l1 && (l1.warnings || []).some((w) => w.code === "MASK_FLATTENED"), "D11 文档 flatten 逐层回执含 MASK_FLATTENED", "");
    return b;
  });

  // ================= D13：组层入围 =================
  const I = await mkDoc("bakedoc_i", 256, 256);
  await I.must("create_layer", { layer_id: "G1", name: "group_hidden", type: "layer_group", z_index: 1 }, "I 建隐藏组 G1");
  await I.must("create_layer", { layer_id: "C1", name: "child_of_hidden", parent_id: "G1", z_index: 2 }, "I 建 C1（父组 G1）");
  await I.must("create_layer", { layer_id: "G2", name: "group_with_own", type: "layer_group", z_index: 3 }, "I 建有自有对象组 G2");
  await I.must("create_layer", { layer_id: "G3", name: "group_empty", type: "layer_group", z_index: 4 }, "I 建无自有对象组 G3");
  await I.must("create_layer", { layer_id: "C3", name: "child_of_g3", parent_id: "G3", z_index: 5 }, "I 建 C3（父组 G3）");
  await I.must("update_layer", { layer_id: "G1", patch: { visible: false } }, "I 藏 G1");
  await I.must("update_layer", { layer_id: "G2", patch: { opacity: 0.5 } }, "I G2 半透明（组不透明度继承用）");
  await I.must("draw_stroke", { layer_id: "C1", data: { points: [[20, 20], [120, 20]], size: 12, color: [200, 60, 40, 255] } }, "I C1 落笔");
  await I.must("draw_stroke", { layer_id: "G2", data: { points: [[20, 60], [120, 60]], size: 12, color: [40, 90, 200, 255] } }, "I G2 自有落笔");
  await I.must("draw_stroke", { layer_id: "C3", data: { points: [[20, 100], [120, 100]], size: 12, color: [30, 160, 80, 255] } }, "I C3 落笔");
  const preI = join(ROOT, "preI.png");
  await I.must("export_png", { path: preI }, "I 压平前导出");
  const dI = await I.must("bake_document", {}, "I 文档压平");
  const skipI = Object.fromEntries((dI.skipped_layers || []).map((s) => [s.layer_id, s.reason]));
  check(skipI.C1 === "HIDDEN_LAYER", "D13 父组不可见而子层 visible=true 的子层跳过（HIDDEN_LAYER）", JSON.stringify(skipI));
  const dIG2 = (dI.layers || []).find((l) => l.layer_id === "G2");
  check(dIG2 && dIG2.status === "applied", "D13 组层有自有对象按普通层入围（G2 applied）", dIG2 && dIG2.status);
  check(!(dI.layers || []).some((l) => l.layer_id === "G3") && !(dI.skipped_layers || []).some((s) => s.layer_id === "G3"), "D13 组层无自有对象不是候选（G3 两表均无）", "");
  const dIC3 = (dI.layers || []).find((l) => l.layer_id === "C3");
  check(dIC3 && dIC3.status === "applied", "D13 可见组子层正常入围（C3 applied）", dIC3 && dIC3.status);
  const postI = join(ROOT, "postI.png");
  await I.must("export_png", { path: postI }, "I 压平后导出");
  const nearI = pngNear(preI, postI);
  check(nearI.ok, "D13 组不透明度继承下压平前后渲染一致（容差 ±1，bake 不 bake 层属性）", nearI.detail);

  console.log(failed === 0 ? "  ✓ tool-bake-document 全绿" : `  ✗ tool-bake-document ${failed} 条红`);
  process.exit(failed === 0 ? 0 : 1);
} catch (error) {
  console.error(`  ⊘ 判据自身出错（不是产品问题）：${String(error).slice(0, 300)}`);
  process.exit(3);
}

// **变异点**（改坏实现必须红）：
//   · 折叠期 flatten 消费去掉（两臂不清 mask_id）⇒ D11 两处压平前后渲染不一致红
//     （二次作用使掩膜区像素差远超 ±1，容差不掩盖此 bug；容差改回逐字节则
//     门一/门二/D11/D13 全红——证明容差是 8 位基底机制必需，不是放水）；
//   · compute_bake_horizon 不含 BakeDocument 条目 ⇒ 门三隐藏层对象数检查红、门六拦截红；
//   · bake_document 回执 layers[] 不按 Z 序 ⇒ 门一 Z 序检查红；
//   · 门禁改成只写 warnings 不报错 ⇒ 门四两处 is_error 检查红；
//   · frontier 扫描不含 BakeDocument 条目 ⇒ 门七 frontier 含 hEA 红；
//   · 撤销守卫有效集不含 BakeDocument 条目 ⇒ 门六 revert 成功红、门七导入后撤销成功红；
//   · 有效可见性改成本层 visible ⇒ D13 C1 入围 applied、跳过清单无 C1 红；
//   · 组层无自有对象计入跳过清单 ⇒ D13 G3 两表均无检查红；
//   · 单原子改多原子（每层一原子）⇒ 门二撤销单原子后仍有基底红、门一 bake_atom_seq 链红。
