#!/usr/bin/env node
// **auto-bake 触发组**（bake 二期 item 2：阈值 2048／软线 1433，标定锁定 2026-10-30）。
//
// 覆盖（四层齐：实现／输入触发／断言／变异，变异点见文末）：
//   ① 缺省关：无环境变量时画到阈值也不 bake，状态里全局开关为关；
//   ② 全局开时 T−1 不触发（2047 笔无 baked 节）、T 触发（第 2048 笔带 baked 节，
//      回执 head 已是 bake 后 head，状态里未 bake 数归零、来源为自动、触发为水位）；
//   ③ 批量内不评估、整批后一次、一次最多一层（两层同超阈值的一批只 bake 一层，
//      持平取层 id 最小；造数批次里不出现 baked 节）；
//   ④ 层开关关时全局开也不 bake；层开关开时全局关也 bake（层显式值优先）。
//
// 前提失败 exit(3)（判据前提不成立 ≠ 产品失败）；产品失败 exit(1)。
// 用法：node scripts/tool-autobake-trigger.mjs
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

async function startServer(envOn) {
  const port = 18700 + Math.floor(Math.random() * 120);
  const ROOT = trackTemp(mkdtempSync(join(tmpdir(), "ab-trig-")));
  const base = `http://127.0.0.1:${port}`;
  const env = { ...process.env };
  if (envOn) env.YANSHI_AUTO_BAKE_ENABLED = "1";
  else delete env.YANSHI_AUTO_BAKE_ENABLED;
  const proc = spawn(BIN, ["--root", ROOT, "--bind", `127.0.0.1:${port}`], { stdio: "ignore", env });
  process.on("exit", () => { try { proc.kill(); } catch { /* 已退出 */ } });
  for (let i = 0; i < 80; i += 1) { try { if ((await fetch(`${base}/health`)).ok) break; } catch {} await sleep(250); }
  return { base, proc };
}

async function openDoc(base, doc) {
  const created = await (await fetch(`${base}/api/documents?role=owner`, { method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: 256, height: 256 }) })).json();
  if (!created.token) failPre("建文档未返回 token", created);
  const call = async (tool, args) => (await fetch(base + "/api/tools/" + tool + "?doc=" + doc + "&token=" + created.token, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) })).json();
  return call;
}

const strokeArg = (layer_id) => ({ name: "draw_stroke", args: { layer_id, data: { points: [[10, 10], [11, 10]], size: 2, color: [20, 20, 20, 255] } } });
// 批量画 n 笔（每批 ≤500 子调用），返回每批回执数组。
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
  // ---- 全局开：T−1 / T ----
  const on = await startServer(true);
  const call = await openDoc(on.base, "ab_trigger");
  const l1 = await call("create_layer", { layer_id: "L" });
  if (l1.ok !== true) failPre("建层失败", l1);
  const resps = await batchStrokes(call, "L", 2047);
  check(!resps.some(hasBaked), "① T−1：2047 笔无 baked 节", `批数=${resps.length}`);
  check(resps[resps.length - 1].auto_bake === undefined, "① T−1：最后一批无 auto_bake 节", JSON.stringify(resps[resps.length - 1].auto_bake || null).slice(0, 120));
  const t = await call("draw_stroke", { layer_id: "L", data: { points: [[10, 10], [11, 10]], size: 2, color: [20, 20, 20, 255] } });
  check(t.ok === true, "② T：触发调用本身成功（bake 失败也不反噬，成功更不反噬）", JSON.stringify(t).slice(0, 120));
  check(t.auto_bake && t.auto_bake.status === "baked", "② T：第 2048 笔带 baked 节", JSON.stringify(t.auto_bake || null).slice(0, 200));
  check(t.auto_bake && t.auto_bake.trigger === "layer_waterline", "② T：触发为水位", t.auto_bake ? t.auto_bake.trigger : "无节");
  check(t.head === (t.auto_bake ? t.auto_bake.bake_atom_seq : null), "② T：回执 head 已是 bake 后 head", `head=${t.head}`);
  const st = await call("get_bake_status", { layer_id: "L" });
  check(st.ok === true && st.layers && st.layers[0].unbaked === 0, "② T：状态里未 bake 数归零", JSON.stringify(st.layers || null).slice(0, 200));
  check(st.layers && st.layers[0].last_bake && st.layers[0].last_bake.origin === "auto", "② T：最新 Bake 来源为自动", JSON.stringify(st.layers ? st.layers[0].last_bake : null).slice(0, 200));
  check(st.layers && st.layers[0].last_bake && st.layers[0].last_bake.trigger === "layer_waterline", "② T：最新 Bake 触发为水位", JSON.stringify(st.layers ? st.layers[0].last_bake : null).slice(0, 200));
  try { on.proc.kill(); } catch {}

  // ---- 批量内不评估、整批后一次、一次最多一层 ----
  const on2 = await startServer(true);
  const call2 = await openDoc(on2.base, "ab_batch");
  for (const lid of ["A", "B"]) {
    const c = await call2("create_layer", { layer_id: lid });
    if (c.ok !== true) failPre(`建层 ${lid} 失败`, c);
  }
  for (const lid of ["A", "B"]) {
    const rs = await batchStrokes(call2, lid, 2000);
    check(!rs.some(hasBaked), `③ 造数：层 ${lid} 的 2000 笔批次里无 baked 节`, "");
  }
  const mixed = [];
  for (let i = 0; i < 100; i += 1) mixed.push(strokeArg("A"));
  for (let i = 0; i < 100; i += 1) mixed.push(strokeArg("B"));
  const mb = await call2("batch", { calls: mixed });
  check(mb.ok === true, "③ 混合批本身成功", JSON.stringify(mb).slice(0, 120));
  check(mb.auto_bake && mb.auto_bake.status === "baked", "③ 混合批整批后 bake 一次", JSON.stringify(mb.auto_bake || null).slice(0, 200));
  check(mb.auto_bake && mb.auto_bake.layer_id === "A", "③ 持平取层 id 最小（A）", mb.auto_bake ? mb.auto_bake.layer_id : "无节");
  const st2 = await call2("get_bake_status", {});
  const rowA = (st2.layers || []).find((l) => l.layer_id === "A");
  const rowB = (st2.layers || []).find((l) => l.layer_id === "B");
  check(rowA && rowA.unbaked === 0, "③ 混合批后 A 已 bake", JSON.stringify(rowA || null).slice(0, 160));
  check(rowB && rowB.unbaked === 2100, "③ 混合批后 B 未 bake（一次最多一层）", JSON.stringify(rowB || null).slice(0, 160));
  try { on2.proc.kill(); } catch {}

  // ---- 层开关关时全局开也不 bake ----
  const on3 = await startServer(true);
  const call3 = await openDoc(on3.base, "ab_layeroff");
  const c3 = await call3("create_layer", { layer_id: "OFF" });
  if (c3.ok !== true) failPre("建层 OFF 失败", c3);
  const off = await call3("update_layer", { layer_id: "OFF", patch: { auto_bake: false } });
  if (off.ok !== true) failPre("关层开关失败", off);
  await batchStrokes(call3, "OFF", 2048);
  const one = await call3("draw_stroke", { layer_id: "OFF", data: { points: [[10, 10], [11, 10]], size: 2, color: [20, 20, 20, 255] } });
  check(one.auto_bake === undefined, "④ 层开关关：过阈值也无 auto_bake 节", JSON.stringify(one.auto_bake || null).slice(0, 160));
  const st3 = await call3("get_bake_status", { layer_id: "OFF" });
  check(st3.layers && st3.layers[0].enabled === false && st3.layers[0].enabled_source === "layer", "④ 层开关关：状态里开关关、来源为层", JSON.stringify(st3.layers || null).slice(0, 200));
  try { on3.proc.kill(); } catch {}

  // ---- 缺省关 + 层开关开时全局关也 bake ----
  const off2 = await startServer(false);
  const call4 = await openDoc(off2.base, "ab_defaultoff");
  const c4 = await call4("create_layer", { layer_id: "D" });
  if (c4.ok !== true) failPre("建层 D 失败", c4);
  await batchStrokes(call4, "D", 2048);
  const d1 = await call4("draw_stroke", { layer_id: "D", data: { points: [[10, 10], [11, 10]], size: 2, color: [20, 20, 20, 255] } });
  check(d1.auto_bake === undefined, "① 缺省关：过阈值也无 auto_bake 节", JSON.stringify(d1.auto_bake || null).slice(0, 160));
  const st4a = await call4("get_bake_status", { layer_id: "D" });
  check(st4a.global_enabled === false, "① 缺省关：状态里全局开关为关", JSON.stringify(st4a.global_enabled));
  const on4 = await call4("update_layer", { layer_id: "D", patch: { auto_bake: true } });
  if (on4.ok !== true) failPre("开层开关失败", on4);
  const d2 = await call4("draw_stroke", { layer_id: "D", data: { points: [[10, 10], [11, 10]], size: 2, color: [20, 20, 20, 255] } });
  check(d2.auto_bake && d2.auto_bake.status === "baked", "④ 层开关开：全局关也 bake", JSON.stringify(d2.auto_bake || null).slice(0, 200));
  try { off2.proc.kill(); } catch {}

  console.log(failed === 0 ? "  ✓ tool-autobake-trigger 全绿" : `  ✗ tool-autobake-trigger ${failed} 条红`);
  process.exit(failed === 0 ? 0 : 1);
} catch (error) {
  console.error(`  ⊘ 判据自身出错（不是产品问题）：${String(error).slice(0, 300)}`);
  process.exit(3);
}

// **变异点**（改坏实现必须红）：
//   · 阈值改成 2047 ⇒ ① T−1 的最后一批带 baked 节 ⇒ 红；
//   · 批量子调用也走评估 ⇒ ③ 造数批次里出现 baked 节（或混合批 bake 两层）⇒ 红；
//   · 层开关解析改成只读全局 ⇒ ④ 两处反向 ⇒ 红；
//   · 回执 head 不更新 ⇒ ② head ≠ bake_atom_seq ⇒ 红。
