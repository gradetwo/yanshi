#!/usr/bin/env node
// **`smart_hatch_follow` 的引导曲线必须有弧长/点数上界** ✓（第三轮复审 **N3D-02**，P1）
//
// **∴ 问题**：`GuideTable::build` 里 `n = ceil(total / 2px) + 1` ⇒
//   `Vec::with_capacity(n)` **直接按调用方的曲线长度分配**
//   ⇒ 一条很长的 guide + 2px 步长 = 巨表；随后 `for k in 0..n` 又是**持锁长循环** ✗
//   ⇒ 一次调用即可挂死服务端（或内存 abort）
//
// **∴ 这条判据守什么**：
//   ① **正向**：正常 guide（跨度 ~200px）⇒ 必须**成功**（防误拒 —— 上限不能挡正常用法）
//   ② **反向**：超长 guide（跨度 1e7 px）⇒ 必须**明确报错**，且**在有限时间内返回**
//      （挂死也算红 ⇒ 所以请求带 20s 超时）
//
// **∴ 变异**：把 `MAX_GUIDE_ARC` 改成 `f64::INFINITY` ⇒ ② 必红（会挂死或吃光内存）✓
const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-guide-arc-bound.mjs <base> <doc> <token>");
  process.exit(3);  // ⊘ 不可判：用法/前提不成立（不是产品失败）
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
    return { ok: false, timeout: true, context: { detail: `请求失败或超时：${e.message}` } };
  }
};

const fails = [];
const check = (ok, msg) => { if (!ok) fails.push(msg); console.log(`  ${ok ? "✓" : "✗"} ${msg}`); };

const mk = (guides) => ({
  layer_id: "L1",
  guides,
  color: [0.2, 0.2, 0.2, 1.0],
  width: 40,
  size: 40,
});

// ① 正向：正常跨度的 guide ⇒ 必须成功
const near = await call("smart_hatch_follow", mk([[[10, 10], [100, 60], [180, 120]]]));
check(near.ok === true, `① 正常 guide（跨度 ~200px）必须成功 ⇒ ok=${near.ok}｜${JSON.stringify(near.context?.detail ?? "").slice(0, 80)}`);

// ② 反向：超长 guide ⇒ 必须明确报错，且在有限时间内返回
const t0 = Date.now();
const far = await call("smart_hatch_follow", mk([[[0, 0], [1e9, 1e9]]]));
const ms = Date.now() - t0;
const detail = String(far.context?.detail ?? "");
check(far.ok === false, `②a 超长 guide（跨度 1e9px）必须**拒绝** ⇒ ok=${far.ok}`);
check(!far.timeout, `②b 必须在 20s 内返回（挂死也算红）⇒ 实际 ${ms} ms`);
check(/引导线/.test(detail), `②c 错误信息必须提到「引导线」（可诊断）⇒ ${JSON.stringify(detail).slice(0, 90)}`);

if (fails.length) {
  console.log(`\n  ✗ ${fails.length} 处不符（guide 弧长上界）`);
  process.exit(1);
}
console.log("\n  ✓ 引导曲线有弧长/点数上界，且不误拒正常用法");
