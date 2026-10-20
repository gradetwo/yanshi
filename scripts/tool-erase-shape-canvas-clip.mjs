#!/usr/bin/env node
// **`smart_erase_shape` 的擦除区域必须裁到画布** ✓（第三轮复审 **N3D-03**，P1）
//
// **∴ 问题**：`invert=true` 用整幅画布（有界 ✓），而 `invert=false` 用
//   `shape.bbox 外扩 feather`，**完全不裁画布** ✗
//   ⇒ shape 放在画布外（如 x=1e7）⇒ 蛇形扫描线会走 1e7 × 1e7
//   ⇒ `path` 无上界 ⇒ **一次调用持锁挂死服务端** ✗
//
// **∴ 这条判据守什么**（两条走不同路径）：
//   ① **正向**：shape 在画布内 ⇒ 必须**成功**（防误拒）
//   ② **反向**：shape 在画布外（x=1e7）⇒ 必须**在 20s 内返回**且**明确说**
//      （`clipped_out: true` 或报错 —— 不是挂死、不是静默 ok）
//
// **∴ 变异**：把非反相分支的画布裁剪去掉（回到 `bx0 - feather` 等原样）⇒ ② 必红 ✓
const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-erase-shape-canvas-clip.mjs <base> <doc> <token>");
  process.exit(3); // ⊘ 不可判：用法/前提不成立（不是产品失败）
}

const call = async (name, args, timeoutMs = 20_000) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
  try {
    const reply = await fetch(url, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(args),
      signal: AbortSignal.timeout(timeoutMs),
    });
    return await reply.json();
  } catch (e) {
    return { ok: false, timedOut: true, context: { detail: `请求失败或超时：${e.message}` } };
  }
};

const fails = [];
const check = (ok, msg) => { if (!ok) fails.push(msg); console.log(`  ${ok ? "✓" : "✗"} ${msg}`); };

const mk = (x, y, w, h) => ({
  layer_id: "L1",
  shape: { kind: "rect", x, y, w, h },
  size: 40,
});

// ① 正向：画布内 ⇒ 成功
const inside = await call("smart_erase_shape", mk(20, 20, 100, 80));
check(inside.ok === true, `① 画布内的 shape 必须成功 ⇒ ok=${inside.ok}｜${JSON.stringify(inside.context?.detail ?? "").slice(0, 80)}`);

// ② 反向：画布外（1e7）⇒ 必须有限时间内返回且明确说
const t0 = Date.now();
const outside = await call("smart_erase_shape", mk(1e7, 1e7, 100, 80));
const ms = Date.now() - t0;
check(!outside.timedOut, `②a 画布外 shape 必须在 20s 内返回（挂死也算红）⇒ 实际 ${ms} ms`);
const why = outside.clipped_out === true || outside.ok === false || /无交集|画布/.test(String(outside.context?.detail ?? ""));
check(why, `②b 必须**明确说**（clipped_out 或报错或提「画布」）⇒ ${JSON.stringify({ ok: outside.ok, clipped_out: outside.clipped_out, detail: String(outside.context?.detail ?? "").slice(0, 80) })}`);

if (fails.length) {
  console.log(`\n  ✗ ${fails.length} 处不符（擦除区域的画布裁剪）`);
  process.exit(1);
}
console.log("\n  ✓ 擦除区域裁到画布，且不误拒画布内的用法");
