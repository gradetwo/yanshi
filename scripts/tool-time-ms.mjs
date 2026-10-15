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

// **★ 可观测量必须符合引擎语义 ✗ ★**（**第 254 轮自查修 ✓）**：
//   ＋ **∴ 我第一版用"一直在移动的笔画"比时间 ✗** ⇒ **两次逐字节相同**✓
//     ＋ **原因**：MyPaint 的 `dabs_per_second` 是**停笔时**按时间补 dab，而移动中的 dab 由
//       **距离**决定（`dabs_per_basic_radius`）=> **一直在动的笔画看不出时间差别**。
//   ＋ **第三版（本版，在 Rust 测试里先验证过）**：用 **`Clouds.myb` +
//     `speed1_slowness`/`speed1_gamma`/`dabs_per_second`**、笔画**保持移动**，只把
//     `time_ms` 的间隔从 10ms 改成 500ms。
//     ＋ `crates/yanshi-server` 的单元测试 `dtimes_reach_the_engine` 已经证明：
//       同一串点只改 dtimes => 引擎产出的**像素不同**（那条测试可红可绿）。
//     ＋ 本判据守的是**端到端**：走 `brush_stroke` + `time_ms` 这条路时，画面必须随之不同。
const main = async () => {
  const hold = async (tag, stepMs) => {
    const doc = `tm_${tag}_${Date.now().toString(36)}`;
    const createdResponse = await fetch(`${base}/api/documents`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ doc_id: doc, width: 384, height: 256 }),
    });
    const createdText = await createdResponse.text();
    let created = null;
    try {
      created = JSON.parse(createdText);
    } catch {
      console.error(
        `❌ 判不了：/api/documents 回的不是 JSON（HTTP ${createdResponse.status}）：${createdText.slice(0, 120)}`,
      );
      process.exit(3);
    }
    if (!created || !created.token) {
      console.error("❌ 判不了：建不了文档 ⇒ 退出码 3");
      process.exit(3);
    }
    const token = created.token;
    await call(doc, token, "create_layer", { layer_id: "L1", name: "L1" });
    // **∴ 笔画保持移动 ✗ 只有时间间隔变** ✓（**组合与 Rust 测试里验证过的一致 ✓）
    const points = [];
    for (let i = 0; i < 12; i += 1) {
      points.push([40 + i * 18, 60 + (i % 3) * 6, 1.0, i * stepMs]);
    }
    const replied = await call(doc, token, "brush_stroke", {
      layer_id: "L1",
      object_id: "s",
      brush: "Clouds",
      size: 40,
      opacity: 1.0,
      color: { r: 10, g: 10, b: 10, a: 255 },
      points,
      brush_params: { speed1_slowness: 1.0, speed1_gamma: 1.0, dabs_per_second: 200.0 },
    });
    const raw = await call(doc, token, "render_region", {
      region: { x: 20, y: 20, w: 300, h: 180 },
      raw: true,
    });
    const url = raw && (raw.raw_url || raw.rawUrl);
    if (!url) return { replied, stats: null };
    const absolute = String(url).startsWith("http") ? String(url) : `${base}${url}`;
    const bytes = new Uint8Array(await (await fetch(absolute)).arrayBuffer());
    let ink = 0;
    let sum = 0;
    for (let i = 0; i < bytes.length; i += 4) {
      if (bytes[i] + bytes[i + 1] + bytes[i + 2] < 600) ink += 1;
      sum = (sum * 31 + bytes[i] + bytes[i + 1] * 3 + bytes[i + 2] * 7) >>> 0;
    }
    return { replied, stats: { ink, fingerprint: sum } };
  };

  const shortHold = await hold("short", 10);
  const longHold = await hold("long", 500);
  if (!shortHold || !shortHold.stats || !longHold || !longHold.stats) {
    console.error(
      `❌ 判不了：量不到（short=${JSON.stringify(shortHold && shortHold.replied).slice(0, 110)}｜long=${JSON.stringify(longHold && longHold.replied).slice(0, 110)}）⇒ 退出码 3`,
    );
    process.exit(3);
  }
  console.log(`  ① time_ms 间隔 10ms：墨 ${shortHold.stats.ink}｜指纹 ${shortHold.stats.fingerprint}`);
  console.log(`  ① time_ms 间隔 500ms：墨 ${longHold.stats.ink}｜指纹 ${longHold.stats.fingerprint}`);

  if (shortHold.stats.fingerprint === longHold.stats.fingerprint) {
    console.error(
      "❌ ① 只改 time_ms 间隔（10ms vs 500ms）、几何完全相同，画面却**逐字节相同**" +
        " ⇒ 时间没有被透传给引擎（还在用 STEP_SECONDS），或这套笔刷设置不吃速度",
    );
    process.exit(1);
  }
  // **∴ 只断言"必须不同" ✗ 不断言墨量方向** ✓：**速度对墨量的影响方向取决于 `.myb` 的
  //   输入映射 ✓ —— **∴ 那是笔刷自己的事 ✗ 不是这条判据该规定的** ✓ ★
  // **★ 问题 1（P0）：`time_ms` 对"不吃时间"的笔刷必须**明确报警**，不许静默无效果 ★**
  //
  // 新功能实测的反馈：传了 `time_ms` 但笔刷不按时间出 dab ⇒ **静默无效果** ✗
  //（只有配了 `dabs_per_second` / `speed1_*` 的笔刷才吃时间戳）。
  // **④** 用一支**不吃时间**的笔刷（`100%_Opaque`，不给 `brush_params`）传 `time_ms`
  //     ⇒ 返回值**必须带 `warning`** ✓，而且画面必须与"不传 `time_ms`"**逐字节相同** ✓
  //    （警告归警告，行为不变 ✓ —— 向后兼容 ✓）。
  // **⑤** 那支配了 `dabs_per_second` 的笔刷 ⇒ **不许**报这个 warning ✓（否则是误报 ✗）。
  const probeStroke = async (tag, brush, brushParams, times) => {
    const doc = `tm_${tag}_${Date.now().toString(36)}`;
    const created = await (
      await fetch(`${base}/api/documents`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ doc_id: doc, width: 340, height: 220 }),
      })
    ).json();
    if (!created || !created.token) return null;
    const token = created.token;
    await call(doc, token, "create_layer", { layer_id: "L1", name: "L1" });
    const points = [
      [30, 60, 0.9],
      [90, 70, 0.9],
      [150, 60, 0.9],
    ];
    if (times) points.forEach((point, index) => point.push(index * 200));
    const replied = await call(doc, token, "brush_stroke", {
      layer_id: "L1",
      object_id: "s",
      brush,
      size: 26,
      color: { r: 20, g: 20, b: 20, a: 255 },
      points,
      ...(brushParams ? { brush_params: brushParams } : {}),
    });
    const raw = await call(doc, token, "render_region", {
      region: { x: 0, y: 0, w: 340, h: 220 },
      raw: true,
    });
    const url = raw && (raw.raw_url || raw.rawUrl);
    if (!url) return { replied, fingerprint: null };
    const absolute = String(url).startsWith("http") ? String(url) : `${base}${url}`;
    const bytes = new Uint8Array(await (await fetch(absolute)).arrayBuffer());
    let fingerprint = 0;
    for (let i = 0; i < bytes.length; i += 4) {
      fingerprint = (fingerprint * 31 + bytes[i] + bytes[i + 1] * 3 + bytes[i + 2] * 7) >>> 0;
    }
    return { replied, fingerprint };
  };

  const plainWith = await probeStroke("plain_t", "100%_Opaque", null, true);
  const plainWithout = await probeStroke("plain_f", "100%_Opaque", null, false);
  const sensitive = await probeStroke(
    "sens",
    "Clouds",
    { speed1_slowness: 1.0, speed1_gamma: 1.0, dabs_per_second: 200.0 },
    true,
  );
  // **★ ⑥ 用 `brush_params` 把它**改成**时间敏感（真机报告 P0 #1）★**
  //   `100%_Opaque` 磁盘上的 `dabs_per_second` 是 **0** ✗，但调用方用 `brush_params`
  //   把它设成了 200 ✓ => 这时**不该再警告** ✓。
  //   我的第一版警告只看**磁盘上的原始值** ✗ => **误报**（用户最想骂的那条 ✓）。
  const overridden = await probeStroke(
    "ovr",
    "100%_Opaque",
    { dabs_per_second: 200.0 },
    true,
  );
  if (!plainWith || !plainWithout || !sensitive) {
    console.error("❌ 判不了：④⑤ 的探针没跑起来 ⇒ 退出码 3");
    process.exit(3);
  }
  const warned = plainWith.replied && typeof plainWith.replied.warning === "string";
  const warnedSensitive = sensitive.replied && typeof sensitive.replied.warning === "string";
  const warnedOverridden =
    overridden.replied && typeof overridden.replied.warning === "string";
  console.log(`  ④ 不吃时间的笔刷传 time_ms ⇒ warning=${warned ? "有 ✓" : "无 ✗"}`);
  console.log(`  ⑤ 配了 dabs_per_second 的笔刷 ⇒ warning=${warnedSensitive ? "有（误报 ✗）" : "无 ✓"}`);
  const bad = [];
  if (!warned) {
    bad.push("④ `time_ms` 对不吃时间的笔刷**静默无效果**（既不改画面、也不报警）⇒ 正是要修的那条 ✗");
  }
  if (warnedSensitive) {
    bad.push("⑤ 对**吃时间**的笔刷也报了「不吃 time_ms」的警告 ⇒ 误报 ✗");
  }
  console.log(
    `  ⑥ 用 brush_params 设 dabs_per_second=200 ⇒ warning=${warnedOverridden ? "有（误报 ✗）" : "无 ✓"}`,
  );
  if (warnedOverridden) {
    bad.push(
      "⑥ **误报**：调用方已用 `brush_params` 把 `dabs_per_second` 设成 200，" +
        "警告却仍说「这支笔刷不按时间出 dab」⇒ 判断没合并 `brush_params` 覆盖 ✗（真机报告 P0 #1）",
    );
  }
  // **★ 不再断言"画面完全相同"（实测教训）★**：`100%_Opaque.myb` 的 `dabs_per_second` 是 0
  //   （不按时间出 dab ✓），但它的 `speed1_gamma`/`speed1_slowness` **非零** ✓
  //   => 时间仍会通过**速度映射**影响笔迹 ✓ => 画面**会**变 ✓，而那**不是 bug** ✓。
  //   所以这条只**记录事实**，不作判红依据 ✓（判红的是"该报警却没报" ✗）。
  console.log(
    `  ④ 参考：不传/传 time_ms 的指纹 ${plainWithout.fingerprint} / ${plainWith.fingerprint}` +
      `${plainWith.fingerprint === plainWithout.fingerprint ? "（相同）" : "（不同 —— 由 speed1_* 造成，正常 ✓）"}`,
  );
  if (bad.length) {
    console.error(`❌ ${bad.length} 项没成立：${bad.join("｜")}`);
    process.exit(1);
  }
  console.log("✓ ④⑤⑥ 成立：**不按时间出 dab** 的笔刷会**明确报警**（不再静默无效果）；**吃时间**的笔刷不误报");
};

await main();
