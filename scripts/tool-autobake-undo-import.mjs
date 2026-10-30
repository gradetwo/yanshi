#!/usr/bin/env node
// **auto-bake Undo 拦截与跨机导入组**（bake 二期 item 2，设计 §9 决定 12 与 origin 规则）。
//
// 覆盖（四层齐：实现／输入触发／断言／变异，变异点见文末）：
//   ① Undo 拦截（自动 Bake）：revert 一个被自动 Bake 剪枝的内容原子 ⇒ 回执 ok
//      且附 bake_folded、bake_atom_seq（等于自动 Bake 的 seq），不是错误；
//   ② Undo 不拦截（未剪枝原子）：revert 一个 bake 后新画的原子 ⇒ 回执无 bake
//      附带字段（正常 revert）；
//   ③ Undo 拦截（手动 Bake）：同一守卫对期 1 手动 Bake 同样生效（守卫与 kind 无关）；
//   ④ 跨机导入：自动 Bake 的 origin 与 trigger 随 atoms.jsonl 走，导入后状态里
//      最新 Bake 来源仍为自动；缺自动 Bake Blob 时默认拒绝、replay_and_heal 恢复。
//
// 前提失败 exit(3)（判据前提不成立 ≠ 产品失败）；产品失败 exit(1)。
// 用法：node scripts/tool-autobake-undo-import.mjs
//   （须先有新鲜的 target/release/yanshi-serve；见 run-criteria-local.sh）

process.on("uncaughtException", (error) => {
  console.error(`  ⊘ 判据自身出错（不是产品问题）：${String(error).slice(0, 200)}`);
  process.exit(3);
});
import { spawn, spawnSync, execSync } from "node:child_process";
import { mkdtempSync, existsSync, statSync, readdirSync, rmSync } from "node:fs";
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
let failed = 0;
const check = (ok, name, detail) => {
  console.log(`  ${ok ? "✓" : "✗"} **${name}**${detail ? `（${detail}）` : ""}`);
  if (!ok) failed += 1;
};
const failPre = (what, resp) => {
  console.error(`  ⊘ 判据前提不成立：${what} ⇒ ${JSON.stringify(resp).slice(0, 300)}`);
  process.exit(3);
};

async function startServer(envOn) {
  const port = 18700 + Math.floor(Math.random() * 120);
  const ROOT = trackTemp(mkdtempSync(join(tmpdir(), "ab-undo-")));
  const base = `http://127.0.0.1:${port}`;
  const env = { ...process.env };
  if (envOn) env.YANSHI_AUTO_BAKE_ENABLED = "1";
  else delete env.YANSHI_AUTO_BAKE_ENABLED;
  const proc = spawn(BIN, ["--root", ROOT, "--bind", `127.0.0.1:${port}`], { stdio: "ignore", env });
  process.on("exit", () => { try { proc.kill(); } catch { /* 已退出 */ } });
  for (let i = 0; i < 80; i += 1) { try { if ((await fetch(`${base}/health`)).ok) break; } catch {} await sleep(250); }
  return { base, proc, ROOT };
}
async function openDoc(base, doc, w = 256, h = 256) {
  const created = await (await fetch(`${base}/api/documents?role=owner`, { method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: w, height: h }) })).json();
  if (!created.token) failPre("建文档未返回 token", created);
  const call = async (tool, args) => (await fetch(base + "/api/tools/" + tool + "?doc=" + doc + "&token=" + created.token, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) })).json();
  return { call, token: created.token };
}
const strokeArg = (layer_id) => ({ name: "draw_stroke", args: { layer_id, data: { points: [[10, 10], [11, 10]], size: 2, color: [20, 20, 20, 255] } } });
async function batchStrokes(call, layer_id, n) {
  for (let done = 0; done < n; done += 500) {
    const chunk = Math.min(500, n - done);
    const calls = Array.from({ length: chunk }, () => strokeArg(layer_id));
    const resp = await call("batch", { calls });
    if (resp.ok !== true) failPre(`批量落笔失败（${done}+${chunk}）`, resp);
  }
}
const singleStroke = (call, layer_id) => call("draw_stroke", { layer_id, data: { points: [[10, 10], [11, 10]], size: 2, color: [20, 20, 20, 255] } });

try {
  const srv = await startServer(true);
  const { call } = await openDoc(srv.base, "ab_undo");

  // ---- ① 自动 Bake + revert 被剪枝原子 ----
  const cu = await call("create_layer", { layer_id: "U" });
  if (cu.ok !== true) failPre("建层 U 失败", cu);
  await batchStrokes(call, "U", 2048);
  const stU = await call("get_bake_status", { layer_id: "U" });
  if (!stU.layers || stU.layers[0].unbaked !== 0) failPre("自动 bake 未发生", stU);
  const bakeSeqU = stU.layers[0].last_bake.bake_atom_seq;
  const waterlineU = stU.layers[0].waterline;
  const listed = await call("list_atoms", { limit: 5000, include_non_stroke: true });
  if (listed.ok !== true) failPre("列原子失败", listed);
  const pruned = (listed.atoms || []).find((a) => a.kind === "draw_stroke" && a.seq <= waterlineU);
  if (!pruned) failPre("找不到被剪枝的笔触原子", { waterline: waterlineU, n: (listed.atoms || []).length });
  const rev1 = await call("revert", { atom_id: pruned.atom_id });
  check(rev1.ok === true, "① revert 被剪枝原子仍成功（附说明，不是错误）", JSON.stringify(rev1).slice(0, 200));
  check(rev1.bake_folded === true, "① 回执附 bake_folded", JSON.stringify(rev1).slice(0, 200));
  check(rev1.bake_atom_seq === bakeSeqU, "① 回执 Bake seq 等于自动 Bake 的 seq", `bake_atom_seq=${rev1.bake_atom_seq}／${bakeSeqU}`);

  // ---- ② bake 后新原子 revert 无附带字段 ----
  const fresh = await singleStroke(call, "U");
  if (fresh.ok !== true) failPre("新落笔失败", fresh);
  const listed2 = await call("list_atoms", { limit: 5000, include_non_stroke: true });
  const freshAtom = (listed2.atoms || []).find((a) => a.kind === "draw_stroke" && a.seq > waterlineU);
  if (!freshAtom) failPre("找不到 bake 后新原子", { waterline: waterlineU });
  const rev2 = await call("revert", { atom_id: freshAtom.atom_id });
  check(rev2.ok === true, "② revert 未剪枝原子成功", JSON.stringify(rev2).slice(0, 160));
  check(rev2.bake_folded === undefined, "② 未剪枝原子 revert 无 bake 附带字段", JSON.stringify(rev2).slice(0, 160));

  // ---- ③ 手动 Bake 同守卫 ----
  const cm = await call("create_layer", { layer_id: "M" });
  if (cm.ok !== true) failPre("建层 M 失败", cm);
  await batchStrokes(call, "M", 10);
  const mb = await call("bake_layer", { layer_id: "M" });
  if (!mb || mb.status !== "applied") failPre("手动 bake 未 applied", mb);
  const stM = await call("get_bake_status", { layer_id: "M" });
  const waterlineM = stM.layers[0].waterline;
  const listedM = await call("list_atoms", { limit: 5000, include_non_stroke: true });
  const prunedM = (listedM.atoms || []).find((a) => a.kind === "draw_stroke" && a.layer_id === "M" && a.seq <= waterlineM);
  if (!prunedM) failPre("找不到手动 bake 剪枝的原子", { waterline: waterlineM });
  const rev3 = await call("revert", { atom_id: prunedM.atom_id });
  check(rev3.ok === true && rev3.bake_folded === true, "③ 手动 Bake 剪枝原子 revert 同样附 bake 说明", JSON.stringify(rev3).slice(0, 200));
  check(rev3.bake_atom_seq === mb.bake_atom_seq, "③ 手动 Bake 的 seq 对得上", `bake_atom_seq=${rev3.bake_atom_seq}／${mb.bake_atom_seq}`);

  // ---- ④ 跨机导入：origin 与 trigger 随行 ----
  const ci = await call("create_layer", { layer_id: "I" });
  if (ci.ok !== true) failPre("建层 I 失败", ci);
  await batchStrokes(call, "I", 2048);
  const stI = await call("get_bake_status", { layer_id: "I" });
  if (!stI.layers || stI.layers[0].unbaked !== 0) failPre("层 I 自动 bake 未发生", stI);
  const expDir = trackTemp(mkdtempSync(join(tmpdir(), "ab-exp-")));
  const expPath = join(expDir, "ab.yanshi");
  const exp = await call("export_project", { path: expPath, blob_bundle_mode: "frontier" });
  if (exp.ok !== true) failPre("导出失败", exp);
  // 注：照 bake 判据 ⑪ 的形——第二服务器上先开一个会话文档拿 token，import_project
  // 用该会话调用、目标 doc_id 不得预建；导入后再开目标文档拿它的 token 查状态。
  const srv2 = await startServer(false);
  const sess2 = await openDoc(srv2.base, "imp_session");
  const impResp = await sess2.call("import_project", { path: expPath, doc_id: "ab_imp" });
  if (impResp.ok !== true) failPre("导入失败", impResp);
  const opened2 = await openDoc(srv2.base, "ab_imp");
  const stImp = await opened2.call("get_bake_status", {});
  const rowI = (stImp.layers || []).find((l) => l.layer_id === "I");
  check(rowI && rowI.last_bake && rowI.last_bake.origin === "auto", "④ 导入后最新 Bake 来源仍为自动", JSON.stringify(rowI ? rowI.last_bake : null).slice(0, 200));
  check(rowI && rowI.last_bake && rowI.last_bake.trigger === "layer_waterline", "④ 导入后最新 Bake 触发仍为水位", JSON.stringify(rowI ? rowI.last_bake : null).slice(0, 200));

  // ---- ④ 缺自动 Bake Blob：默认拒绝、replay_and_heal 恢复 ----
  const expDir2 = trackTemp(mkdtempSync(join(tmpdir(), "ab-exp2-")));
  const expPath2 = join(expDir2, "ab2.yanshi");
  const exp2 = await call("export_project", { path: expPath2, blob_bundle_mode: "all_history" });
  if (exp2.ok !== true) failPre("第二次导出失败", exp2);
  const bakeHash = stI.layers[0].last_bake.blob_hash;
  // 注：blob_hash 带 "sha256:" 前缀，须先剥掉再按 hex 分段（照 bake 判据 blobTarPath）。
  const bakeHex = bakeHash.replace(/^sha256:/, "");
  const blobRel = `blobs/sha256/${bakeHex.slice(0, 2)}/${bakeHex.slice(2, 4)}/${bakeHex}`;
  const unpackDir = trackTemp(mkdtempSync(join(tmpdir(), "ab-unpack-")));
  execSync(`tar -xf ${expPath2} -C ${unpackDir}`, { stdio: "ignore" });
  const entries = execSync(`tar -tf ${expPath2}`, { encoding: "utf8" }).split("\n").filter(Boolean);
  if (!entries.some((e) => e === blobRel || e.endsWith(blobRel))) failPre("导出包里找不到自动 Bake blob 条目", { blobRel, n: entries.length });
  rmSync(join(unpackDir, blobRel), { force: true });
  const strippedPath = join(expDir2, "ab2-stripped.yanshi");
  // 注：必须按原包条目名重打包；用 "-C impDir ." 会给全部条目加 ./ 前缀，
  // 导入器按裸名找 atoms.jsonl 就会误报“不是工程包”（bake 判据 ⑪ 同款坑）。
  spawnSync("tar", ["-cf", strippedPath, "-C", unpackDir, ...entries.filter((e) => e !== blobRel)], { stdio: "ignore", timeout: 90_000 });
  const srv3 = await startServer(false);
  const sess3 = await openDoc(srv3.base, "imp_session2");
  const impDeny = await sess3.call("import_project", { path: strippedPath, doc_id: "ab_imp2" });
  check(impDeny.ok === false && /缺有效压平基底|missing_baked_blob|MissingBakedBlob/.test(JSON.stringify(impDeny)), "④ 缺自动 Bake Blob 默认拒绝", JSON.stringify(impDeny).slice(0, 240));
  const impHeal = await sess3.call("import_project", { path: strippedPath, doc_id: "ab_imp2", missing_blob_policy: "replay_and_heal" });
  check(impHeal.ok === true, "④ replay_and_heal 恢复成功", JSON.stringify(impHeal).slice(0, 240));
  try { srv.proc.kill(); } catch {}
  try { srv2.proc.kill(); } catch {}
  try { srv3.proc.kill(); } catch {}

  console.log(failed === 0 ? "  ✓ tool-autobake-undo-import 全绿" : `  ✗ tool-autobake-undo-import ${failed} 条红`);
  process.exit(failed === 0 ? 0 : 1);
} catch (error) {
  console.error(`  ⊘ 判据自身出错（不是产品问题）：${String(error).slice(0, 300)}`);
  process.exit(3);
}

// **变异点**（改坏实现必须红）：
//   · write_history_atom 不附 bake 注脚 ⇒ ①③ 的 bake_folded 断言红；
//   · 守卫只认手动 Bake（按 origin 过滤）⇒ ① 的自动 Bake 注脚断言红；
//   · 导入时丢弃 origin／trigger ⇒ ④ 的来源／触发断言红；
//   · 缺自动 Bake Blob 时豁免 ⇒ ④ 默认拒绝断言红。
