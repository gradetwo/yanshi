#!/usr/bin/env node
// **auto-bake 软线与门禁跳过组**（bake 二期 item 2：软线 1433，门禁预授权缺省 false）。
//
// 覆盖（四层齐：实现／输入触发／断言／变异，变异点见文末）：
//   ① 软线：1432 笔无节、第 1433 笔带 soft_line 节与警告码、再画一笔无节（去重），
//      状态里软线已发为真；
//   ② 门禁跳过（栅格化）：含文本层过硬阈值 ⇒ skipped 节与警告码、计数不重置，
//      同原因下一次内容调用无节（去重），预授权后下一次内容调用 bake 成功且
//      回执含文本栅格化审计条目；
//   ③ 门禁跳过（越界）：含越界笔触层过硬阈值 ⇒ skipped 节，预授权后 bake 成功且
//      回执含越界裁剪审计条目。注：越界载体必须用笔触而不用形状——形状是矢量
//      图元，先触栅格化门禁（实测：形状层预授权越界后仍被栅格化原因跳过）；笔触
//      是 Stroke 对象，越界只触越界门禁，门禁隔离才测得干净。
//
// 前提失败 exit(3)（判据前提不成立 ≠ 产品失败）；产品失败 exit(1)。
// 用法：node scripts/tool-autobake-softline-gate.mjs
//   （须先有新鲜的 target/release/yanshi-serve；见 run-criteria-local.sh）

process.on("uncaughtException", (error) => {
  console.error(`  ⊘ 判据自身出错（不是产品问题）：${String(error).slice(0, 200)}`);
  process.exit(3);
});
import { spawn } from "node:child_process";
import { mkdtempSync, existsSync, statSync } from "node:fs";
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

const port = 18700 + Math.floor(Math.random() * 120);
const ROOT = trackTemp(mkdtempSync(join(tmpdir(), "ab-soft-")));
const base = `http://127.0.0.1:${port}`;
const proc = spawn(BIN, ["--root", ROOT, "--bind", `127.0.0.1:${port}`], {
  stdio: "ignore",
  env: { ...process.env, YANSHI_AUTO_BAKE_ENABLED: "1" },
});
process.on("exit", () => { try { proc.kill(); } catch { /* 已退出 */ } });

const strokeArg = (layer_id) => ({ name: "draw_stroke", args: { layer_id, data: { points: [[10, 10], [11, 10]], size: 2, color: [20, 20, 20, 255] } } });
const singleStroke = (call, layer_id) => call("draw_stroke", { layer_id, data: { points: [[10, 10], [11, 10]], size: 2, color: [20, 20, 20, 255] } });
async function batchStrokes(call, layer_id, n) {
  const resps = [];
  for (let done = 0; done < n; done += 500) {
    const chunk = Math.min(500, n - done);
    const calls = Array.from({ length: chunk }, () => strokeArg(layer_id));
    const resp = await call("batch", { calls });
    if (resp.ok !== true) failPre(`批量落笔失败（${done}+${chunk}）`, resp);
    resps.push(resp);
  }
  return resps;
}
const hasBaked = (resp) => resp.auto_bake && resp.auto_bake.status === "baked";

try {
  for (let i = 0; i < 80; i += 1) { try { if ((await fetch(`${base}/health`)).ok) break; } catch {} await sleep(250); }
  const doc = "ab_soft_gate";
  const created = await (await fetch(`${base}/api/documents?role=owner`, { method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: 256, height: 256 }) })).json();
  if (!created.token) failPre("建文档未返回 token", created);
  const call = async (tool, args) => (await fetch(base + "/api/tools/" + tool + "?doc=" + doc + "&token=" + created.token, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) })).json();

  // ---- ① 软线 ----
  const cs = await call("create_layer", { layer_id: "S" });
  if (cs.ok !== true) failPre("建层 S 失败", cs);
  const sResps = await batchStrokes(call, "S", 1432);
  check(!sResps.some((r) => r.auto_bake !== undefined), "① 1432 笔无 auto_bake 节", "");
  const s1433 = await singleStroke(call, "S");
  check(s1433.auto_bake && s1433.auto_bake.status === "soft_line", "① 第 1433 笔带 soft_line 节", JSON.stringify(s1433.auto_bake || null).slice(0, 200));
  check(s1433.auto_bake && (s1433.auto_bake.warnings || []).some((w) => w.code === "AUTO_BAKE_SOFT_LINE"), "① 软线警告码", JSON.stringify(s1433.auto_bake || null).slice(0, 200));
  const s1434 = await singleStroke(call, "S");
  check(s1434.auto_bake === undefined, "① 再画一笔无节（软线去重）", JSON.stringify(s1434.auto_bake || null).slice(0, 160));
  const stS = await call("get_bake_status", { layer_id: "S" });
  check(stS.layers && stS.layers[0].soft_fired === true, "① 状态里软线已发为真", JSON.stringify(stS.layers || null).slice(0, 200));

  // ---- ② 门禁跳过（栅格化）----
  const cg = await call("create_layer", { layer_id: "G" });
  if (cg.ok !== true) failPre("建层 G 失败", cg);
  const tx = await call("draw_text", { layer_id: "G", data: { text: "Hi", font: "builtin", size: 24, color: [0, 0, 0, 255], position: [20, 20] } });
  if (tx.ok !== true) failPre("文本落笔失败", tx);
  const gResps = await batchStrokes(call, "G", 2047);
  const gLast = gResps[gResps.length - 1];
  check(gLast.auto_bake && gLast.auto_bake.status === "skipped", "② 含文本层过硬阈值 ⇒ skipped 节", JSON.stringify(gLast.auto_bake || null).slice(0, 240));
  check(gLast.auto_bake && (gLast.auto_bake.warnings || []).some((w) => w.code === "AUTO_BAKE_SKIPPED_GATE"), "② 跳过警告码", JSON.stringify(gLast.auto_bake || null).slice(0, 240));
  const stG1 = await call("get_bake_status", { layer_id: "G" });
  check(stG1.layers && stG1.layers[0].unbaked === 2048, "② 跳过后计数不重置（仍 2048）", JSON.stringify(stG1.layers || null).slice(0, 200));
  const gAgain = await singleStroke(call, "G");
  check(gAgain.auto_bake === undefined, "② 同原因下一次内容调用无节（跳过去重）", JSON.stringify(gAgain.auto_bake || null).slice(0, 160));
  const pre = await call("update_layer", { layer_id: "G", patch: { allow_auto_rasterize_editable: true } });
  if (pre.ok !== true) failPre("预授权失败", pre);
  const gBaked = await singleStroke(call, "G");
  check(gBaked.auto_bake && gBaked.auto_bake.status === "baked", "② 预授权后下一次内容调用 bake 成功", JSON.stringify(gBaked.auto_bake || null).slice(0, 240));
  check(gBaked.auto_bake && (gBaked.auto_bake.warnings || []).some((w) => w.code === "TEXT_RASTERIZED"), "② bake 回执含文本栅格化审计条目", JSON.stringify(gBaked.auto_bake || null).slice(0, 240));
  const stG2 = await call("get_bake_status", { layer_id: "G" });
  check(stG2.layers && stG2.layers[0].unbaked === 0, "② bake 后未 bake 数归零", JSON.stringify(stG2.layers || null).slice(0, 200));

  // ---- ③ 门禁跳过（越界）----
  const cc = await call("create_layer", { layer_id: "C" });
  if (cc.ok !== true) failPre("建层 C 失败", cc);
  const shp = await call("draw_stroke", { layer_id: "C", data: { points: [[-10, 10], [300, 10]], size: 2, color: [200, 30, 30, 255] } });
  if (shp.ok !== true) failPre("越界笔触失败", shp);
  const cResps = await batchStrokes(call, "C", 2047);
  const cLast = cResps[cResps.length - 1];
  check(cLast.auto_bake && cLast.auto_bake.status === "skipped", "③ 含越界笔触层过硬阈值 ⇒ skipped 节", JSON.stringify(cLast.auto_bake || null).slice(0, 240));
  check(cLast.auto_bake && /越界/.test(cLast.auto_bake.reason || ""), "③ 跳过原因须为越界（不是栅格化）", JSON.stringify(cLast.auto_bake || null).slice(0, 240));
  check(!cResps.some(hasBaked), "③ 造数批次里无 baked 节", "");
  const preC = await call("update_layer", { layer_id: "C", patch: { allow_auto_clip_off_canvas: true } });
  if (preC.ok !== true) failPre("越界预授权失败", preC);
  const cBaked = await singleStroke(call, "C");
  check(cBaked.auto_bake && cBaked.auto_bake.status === "baked", "③ 越界预授权后 bake 成功", JSON.stringify(cBaked.auto_bake || null).slice(0, 240));
  check(cBaked.auto_bake && (cBaked.auto_bake.warnings || []).some((w) => w.code === "CLIPPED_OFF_CANVAS_PIXELS"), "③ bake 回执含越界裁剪审计条目", JSON.stringify(cBaked.auto_bake || null).slice(0, 240));

  console.log(failed === 0 ? "  ✓ tool-autobake-softline-gate 全绿" : `  ✗ tool-autobake-softline-gate ${failed} 条红`);
  process.exit(failed === 0 ? 0 : 1);
} catch (error) {
  console.error(`  ⊘ 判据自身出错（不是产品问题）：${String(error).slice(0, 300)}`);
  process.exit(3);
}

// **变异点**（改坏实现必须红）：
//   · 软线改成 1432 ⇒ ① 1432 批里出现 soft_line 节 ⇒ 红；
//   · 门禁失败改成直接 bake（无视预授权）⇒ ②③ 的 skipped 断言红；
//   · 跳过后把计数清零 ⇒ ② 计数断言红；
//   · 跳过去重去掉 ⇒ ② 再画一笔带 skipped 节 ⇒ 红。
