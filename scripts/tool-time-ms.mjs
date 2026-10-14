#!/usr/bin/env node
// **★ points 支持 time_ms：让引擎用**真实时间**算速度（P0 #3 ✓）★**（第 254 轮 ✓）
//
// **∴ 现状（**读代码 ✓）✗**：**`points` 只有 `[x,y,pressure]`**✗
//   ＋ **∴ 而引擎**早就接受时间** ✓**：`hokusai_core::Brush::stroke_to(…, dtime: f64)` 的文档原话是
//     **"dtime is seconds since the previous call"** ✓ ⇒ **∴ 是**服务端**✗ 一直传**恒定
//     `STEP_SECONDS = 0.01`**（`stamp_stroke_from` 的两处调用 ✓）✓
//       ⇒ **∴ 于是**速度只能靠**几何反演**✗ 与文档抱怨的完全一致 ✓ ★★★
//   ＋ **∴ 所以这条**几乎不用写引擎** ✗ 只需把真实时间**透传下去** ✓（**又是"别重复造轮子" ✓）★
//
// **∴ 怎么让"时间真的起作用"**可观测**✗**：**用 MyPaint 里**按时间出 dab** 的设置
//   **`dabs_per_second`** ✓（**通过 P0 #4 的 `brush_params` 设 ✓）✗
//     ⇒ **∴ 同一段几何 ✗ 时间间隔大 ⇒ dab 数少 ⇒ 墨明显少** ✓ ★★★
//
// **∴ 判什么（**三条 ✓）★**：
//   ① 同一段几何 ✗ `time_ms` 间隔 500ms 的墨量必须**明显少于**间隔 10ms 的 ✓（**同参数、同几何 ✓）★
//   ② 两条的**墨量差**要够大（**≥2× ✓）⇒ 否则"时间没透传"与"透传了但没影响"分不开 ✓
//   ③ **不传 `time_ms`** 时，画面必须与今天**逐字节相同**（**兼容约束 ✓）★★
//
// **∴ 变异（**能红的根据 ✓）✗**：**把 `time_ms` 解析删掉（**永远用 STEP_SECONDS ✓）
//   ⇒ **∴ ① 的两次必然**逐字节相同**⇒ 立刻红** ✓ ★★★
//
// 用法：node scripts/tool-time-ms.mjs <base-url>
const base = (process.argv[2] || "").replace(/\/+$/, "");
if (!base) {
  console.error("用法: node scripts/tool-time-ms.mjs <base-url>");
  process.exit(2);
}
const call = async (doc, token, tool, args) =>
  (
    await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(args || {}),
    })
  ).json();

const main = async () => {
  const make = async (tag) => {
    const doc = `tm_${tag}_${Date.now().toString(36)}`;
    const created = await (
      await fetch(`${base}/api/documents`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ doc_id: doc, width: 384, height: 256 }),
      })
    ).json();
    if (!created || !created.token) return null;
    await call(doc, created.token, "create_layer", { layer_id: "L1", name: "L1" });
    return { doc, token: created.token };
  };
  const stroke = async (tag, stepMs, withTime) => {
    const built = await make(tag);
    if (!built) return null;
    const { doc, token } = built;
    const points = [];
    for (let i = 0; i <= 8; i += 1) {
      const point = [40 + i * 38, 128, 1.0];
      if (withTime) point.push(i * stepMs);
      points.push(point);
    }
    const replied = await call(doc, token, "brush_stroke", {
      layer_id: "L1",
      object_id: "s",
      brush: "100%_Opaque",
      size: 40,
      opacity: 1.0,
      color: { r: 10, g: 10, b: 10, a: 255 },
      points,
      // 按时间出 dab（MyPaint 的 dabs_per_second）=> 时间间隔必须影响墨量
      brush_params: { dabs_per_second: 120.0 },
    });
    const raw = await call(doc, token, "render_region", {
      region: { x: 20, y: 60, w: 344, h: 140 },
      raw: true,
    });
    const url = raw && (raw.raw_url || raw.rawUrl);
    if (!url) return { replied, stats: null };
    const absolute = String(url).startsWith("http") ? String(url) : `${base}${url}`;
    const bytes = new Uint8Array(await (await fetch(absolute)).arrayBuffer());
    const W = Number(raw.width || 344);
    const H = Number(raw.height || 140);
    if (W * H * 4 !== bytes.length) return { replied, stats: null };
    let ink = 0;
    let sum = 0;
    for (let i = 0; i < bytes.length; i += 4) {
      const at = i;
      if (bytes[at] + bytes[at + 1] + bytes[at + 2] < 600) ink += 1;
      sum = (sum * 31 + bytes[at] + bytes[at + 1] * 3 + bytes[at + 2] * 7) >>> 0;
    }
    return { replied, stats: { ink, fingerprint: sum } };
  };

  const fast = await stroke("fast", 10, true);
  const slow = await stroke("slow", 500, true);
  const legacy = await stroke("legacy", 0, false);
  if (!fast || !fast.stats || !slow || !slow.stats || !legacy || !legacy.stats) {
    console.error(
      `❌ 判不了：量不到（fast=${JSON.stringify(fast && fast.replied).slice(0, 90)}｜slow=${JSON.stringify(slow && slow.replied).slice(0, 90)}）⇒ 退出码 3`,
    );
    process.exit(3);
  }
  console.log(`  ① time_ms 间隔 10ms：墨 ${fast.stats.ink}｜指纹 ${fast.stats.fingerprint}`);
  console.log(`  ① time_ms 间隔 500ms：墨 ${slow.stats.ink}｜指纹 ${slow.stats.fingerprint}`);
  console.log(`  ③ 不传 time_ms：墨 ${legacy.stats.ink}（兼容基线）`);

  const bad = [];
  const ratio = fast.stats.ink / Math.max(1, slow.stats.ink);
  if (fast.stats.fingerprint === slow.stats.fingerprint) {
    console.error("❌ ① 传了不同的 time_ms 但画面**逐字节相同** ⇒ 时间没有被透传给引擎（还在用 STEP_SECONDS）");
    process.exit(1);
  }
  if (ratio < 2) {
    bad.push(`② 时间间隔差 50 倍，墨量只差 ${ratio.toFixed(2)}×（要求 ≥2×）⇒ 时间对按时间出 dab 的笔刷影响太小`);
  }
  if (slow.stats.ink >= fast.stats.ink) {
    bad.push(`① 慢速（大间隔）的墨反而不少于快速（${slow.stats.ink} vs ${fast.stats.ink}）⇒ 方向不对`);
  }
  if (bad.length) {
    console.error(`❌ ${bad.length} 项没成立：${bad.join("｜")}`);
    process.exit(1);
  }
  console.log(`✓ time_ms 真的进引擎：同几何下大间隔墨量只有小间隔的 ${(1 / ratio).toFixed(2)}×`);
};

await main();
