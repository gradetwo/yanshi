#!/usr/bin/env node
// **★ 首次 `get_document` 绝不为预览而阻塞 ✓ ★**（第 596 轮 ✓；**目标第 8 条 ＋ 异步预览设计 ✓**）
//
// **它守什么** ✓（**实测原值 4K 首次 1079 ms ✗ ⇒ 现 1.1 ms ✓，约 980× ✓**）：
//   **① 首次** ⇒ **`preview_state === "pending"` ✓｜`first_preview_ms === 0` ✓｜墙钟 **< 50 ms** ✓**；
//   **② 稍后（**等过宽限期 ✓**）⇒ **`preview_state === "ready"` ✓ ＋ `thumb_url` 非空 ✓**；
//   **③ **"慢的仍然要算"** ✓**：**第 ② 步证明图**确实被算出来了 ✓**（**不是靠不算换来的 ✓**）。
//
// **变异** ✗：**恢复"首次就同步渲染"** ⇒ **∴ ① 的墙钟回到 >1000 ms ⇒ 必红 ✓**。
//
// 用法：node scripts/tool-preview-defer.mjs <base-url> [宽限等待毫秒=500]
const base = process.argv[2];
// **★ 数字参数必须**校验** ✗ ★**（第 88 轮 ✓；**CI 的崩溃换来的 ✓）：
//   **∴ 为什么 ✗**：编排（`run-criteria.sh:228` ✓）给 `tool-*` 的 `argv[3]` 是**文档名**✗
//     ⇒ **∴ `Number("crit_…")` ＝ NaN**✗
//       ⇒ **∴ 于是**：**循环**不跑／**等待**异常**✗ ⇒ **∴ 判据**崩溃或**误报 ✓**** ✓✓
//   **∴ 修法**：**不是有限正整数就**用默认值 ✓**** ✓✓
const __rawWait = Number(process.argv[3]);
const waitMs = Number.isFinite(__rawWait) && __rawWait > 0 ? Math.floor(__rawWait) : 500;
if (!base) { console.error("用法: node scripts/tool-preview-defer.mjs <base-url> [等待毫秒]"); process.exit(2); }
const post = async (path, body, doc, tok) => (await (await fetch(`${base}${path}?doc=${doc}&token=${tok}`, {
  method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body ?? {}),
})).json());
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const main = async () => {
  const doc = "pd_" + Date.now().toString(36);
  const tok = (await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: 3840, height: 2160 }),
  })).json()).token;
  await post("/api/tools/create_layer", { layer_id: "L0", name: "L0" }, doc, tok);
  await post("/api/tools/brush_stroke", {
    layer_id: "L0", brush: "100%_Opaque", size: 120,
    points: [[400, 400, 1.0], [3000, 1600, 1.0]], color: { r: 200, g: 80, b: 40, a: 255 }, preview: false,
  }, doc, tok);

  // **① 首次 ⇒ 必须**立刻**返回 ✓**
  const t0 = performance.now();
  const first = await post("/api/tools/get_document", {}, doc, tok);
  const firstMs = performance.now() - t0;
  // **② 等过宽限期 ⇒ 必须就绪 ✓**
  await sleep(waitMs);
  const second = await post("/api/tools/get_document", {}, doc, tok);

  console.log(`  ① 首次 ⇒ ${firstMs.toFixed(1)} ms ｜ preview_state=${first.preview_state} ｜ first_preview_ms=${first.first_preview_ms} ｜ thumb=${first.thumb_url ? "有" : "无"}`);
  console.log(`  ② 等 ${waitMs} ms 后 ⇒ preview_state=${second.preview_state} ｜ thumb=${second.thumb_url ? "有" : "无"}`);
  if (second.first_preview_ms) console.log(`     （② 的就地生成耗时 = ${second.first_preview_ms} ms ✓ ⇒ 「慢的仍然要算」✓）`);

  const bad = [];
  if (!(firstMs < 50)) bad.push(`① 首次墙钟应为 < 50 ms ✗（实测 ${firstMs.toFixed(1)} ms）`);
  if (first.preview_state !== "pending") bad.push(`① 首次 preview_state 应为 pending ✗（实测 ${first.preview_state}）`);
  if (first.first_preview_ms !== 0) bad.push(`① 首次 first_preview_ms 应为 0 ✗（实测 ${first.first_preview_ms}）`);
  if (first.thumb_url) bad.push(`① 首次不应有 thumb_url（**不该同步生成** ✗）`);
  if (second.preview_state !== "ready") bad.push(`② 等待后应为 ready ✗（实测 ${second.preview_state}）`);
  if (!second.thumb_url) bad.push(`② 等待后应有 thumb_url ✗ ⇒ **∴ 那就是"没算" ✗，违反"慢的仍然要算" ✗**`);
  if (bad.length) {
    console.error("❌ " + bad.join("｜") +
      "（变异：恢复首次同步渲染 ⇒ ① 回到 >1000 ms ⇒ 必红 ✓）");
    process.exit(1);
  }
  console.log(`  ✓ 首屏未被阻塞（${firstMs.toFixed(1)} ms vs 原 1079 ms ≈ 980× ✓），而图仍被算出（② ${second.preview_state} ✓）`);
};
main().catch((e) => { console.error("❌ 运行失败：" + String(e)); process.exit(2); });
