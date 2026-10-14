#!/usr/bin/env node
// **★ 生理颤动 tremor（P1 #8 的最后一块）：带限 1D value noise ★**（第 254 轮）
//
// **需求文档 P1 #8**："2–5Hz 手腕 + 8–12Hz 震颤的 1D value noise"。
//
// **与仓库既有的抖动有什么区别**（这是本条的**要点**）：`apply_brush_style` 里**已经有**
// 逐点白噪声抖动（`scatter_next`，相邻点**独立**）=> 视觉上像"砂纸"；人的手是**带限**的
// （手腕低频摆动 ＋ 生理性高频震颤）=> 位移必须沿笔迹**缓慢变化**（相邻点**相关**）。
// 新增的 `tremor` 就是这一层：沿**弧长**采样 1D value noise（格点用确定性哈希、
// 格点间余弦插值），两个八度（手腕 λ≈90px、震颤 λ≈28px）沿**局部法向**偏移，
// 再叠一个 ±3% 的压力调制。
//
// **"带限"由 Rust 单测强断言**（`tremor_is_band_limited_and_off_by_default`：
// 相邻点法向位移增量 < 1.2px —— 白噪声会给到像素级跳变）。本判据守端到端：
//   ① **opt-in 的承诺**：不传 `tremor` 与传 `tremor: 0` 必须**逐字节相同**；
//   ② `tremor: 1` 的画面必须**变**，但**墨量不能被毁**（总墨量仍在 ±25% 内 —— 抖的是位置，不是把线擦掉）；
//   ③ **确定性**：同种子两次**逐字节一致**，且**换种子必须换波形**。
//
// **变异（能红的根据）**：把 `tremor` 接成默认开启（① 红）；把噪声换成白噪声并让幅度失控
// （② 的"墨量不毁"红）；读时钟/随机源（③ 红）。
//
// 用法：node scripts/tool-tremor.mjs <base-url>
const base = (process.argv[2] || "").replace(/\/+$/, "");
if (!base) {
  console.error("用法: node scripts/tool-tremor.mjs <base-url>");
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

const POINTS = [
  [40, 100, 1.0],
  [120, 100, 1.0],
  [200, 100, 1.0],
  [280, 100, 1.0],
];

const main = async () => {
  const bad = [];
  const paint = async (tag, extra) => {
    const doc = `tr_${tag}_${Date.now().toString(36)}`;
    const response = await fetch(`${base}/api/documents`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ doc_id: doc, width: 320, height: 200 }),
    });
    const text = await response.text();
    let created = null;
    try {
      created = JSON.parse(text);
    } catch {
      console.error(`❌ 判不了：/api/documents 回的不是 JSON（HTTP ${response.status}）：${text.slice(0, 120)}`);
      process.exit(3);
    }
    if (!created || !created.token) {
      console.error("❌ 判不了：建不了文档 ⇒ 退出码 3");
      process.exit(3);
    }
    const token = created.token;
    await call(doc, token, "create_layer", { layer_id: "L1", name: "L1" });
    const replied = await call(doc, token, "brush_stroke", {
      layer_id: "L1",
      object_id: "line",
      brush: "100%_Opaque",
      size: 8,
      opacity: 1.0,
      color: { r: 0, g: 0, b: 0, a: 255 },
      points: POINTS,
      ...(extra || {}),
    });
    if (!replied || replied.ok !== true) {
      console.error(`❌ 判不了：笔画失败（${JSON.stringify(replied).slice(0, 150)}）⇒ 退出码 3`);
      process.exit(3);
    }
    const raw = await call(doc, token, "render_region", {
      region: { x: 0, y: 0, w: 320, h: 200 },
      raw: true,
    });
    const url = raw && (raw.raw_url || raw.rawUrl);
    if (!url) {
      console.error("❌ 判不了：量不到渲染结果 ⇒ 退出码 3");
      process.exit(3);
    }
    const absolute = String(url).startsWith("http") ? String(url) : `${base}${url}`;
    const bytes = new Uint8Array(await (await fetch(absolute)).arrayBuffer());
    let ink = 0;
    let fingerprint = 0;
    for (let i = 0; i < bytes.length; i += 4) {
      const lum = (bytes[i] + bytes[i + 1] + bytes[i + 2]) / 3;
      ink += Math.max(0, 1 - lum / 255);
      fingerprint = (fingerprint * 31 + bytes[i] + bytes[i + 1] * 3 + bytes[i + 2] * 7) >>> 0;
    }
    return { ink, fingerprint };
  };

  const plain = await paint("plain", {});
  const zero = await paint("zero", { tremor: 0 });
  const shaken = await paint("shaken", { tremor: 1.0, tremor_seed: 7 });
  const again = await paint("again", { tremor: 1.0, tremor_seed: 7 });
  const otherSeed = await paint("other", { tremor: 1.0, tremor_seed: 8 });

  console.log(
    `  总墨量：不传 ${plain.ink.toFixed(0)}｜tremor:0 ${zero.ink.toFixed(0)}｜tremor:1 ${shaken.ink.toFixed(0)}`,
  );

  if (plain.fingerprint !== zero.fingerprint) {
    bad.push(
      `① 不传 \`tremor\` 与传 \`tremor: 0\` 画面不同（${plain.fingerprint} vs ${zero.fingerprint}）⇒ opt-in 的承诺被破坏`,
    );
  }
  if (shaken.fingerprint === plain.fingerprint) {
    bad.push("② 传了 `tremor: 1` 画面却逐字节相同 ⇒ 颤动没生效");
  }
  const ratio = shaken.ink / Math.max(1e-6, plain.ink);
  if (ratio < 0.75 || ratio > 1.25) {
    bad.push(
      `② 颤动把墨量改了太多（${plain.ink.toFixed(0)} ⇒ ${shaken.ink.toFixed(0)}，比值 ${ratio.toFixed(2)}）⇒ 抖的该是**位置**，不是把线擦掉`,
    );
  }
  if (again.fingerprint !== shaken.fingerprint) {
    bad.push(`③ 同种子两次画面不同（${shaken.fingerprint} vs ${again.fingerprint}）⇒ 不满足确定性`);
  }
  if (otherSeed.fingerprint === shaken.fingerprint) {
    bad.push("③ 换了 `tremor_seed` 波形却没变 ⇒ 种子没起作用");
  }

  if (bad.length) {
    console.error(`❌ ${bad.length} 项没成立：${bad.join("｜")}`);
    process.exit(1);
  }
  console.log(
    `✓ 颤动成立（opt-in）：缺省与 tremor:0 逐字节相同；tremor:1 画面变而墨量守恒（比值 ${ratio.toFixed(2)}）；同种子可复现、换种子换波形`,
  );
};

await main();
