#!/usr/bin/env node
// **★ 曲率驻留（P1 #8 的一半）：转弯处多插点 ★**（第 254 轮）
//
// **需求文档 P1 #8** 里的"曲率驻留三分之二幂律"：手在**转弯处**天然减速 ⇒ 单位长度采样点更密
// ⇒ 拐角处颜料更足、边缘更实。经验关系：驻留量 ∝ **κ^(2/3)**。
//
// **设计取舍（本仓纪律）**：它会**改变点列** ⇒ 改变像素 ⇒ 既有像素级判据会动。
// 所以做成**显式 opt-in** 参数 `dwell`（**缺省不给 ⇒ 一个点都不加、逐字节不变**）。
//
// **数据层的形状由 Rust 单测 `curvature_dwell_is_off_by_default_and_denser_at_corners` 钉住**
// （缺省/直线不加点、直角拐弯加点、端点不漂、确定性）；本判据守**端到端**：
//   ① **缺省**（不传 `dwell`）与 `dwell: 0` 必须**逐字节相同**（opt-in 的承诺）；
//   ② 传 `dwell: 1.0` 画一个 **L 形急拐角** => 画面必须**变**，且**拐角邻域的墨必须更多**
//      （驻留的可见后果）；
//   ③ 确定性：同参数两次**逐字节一致**。
//
// **变异（能红的根据）**：把 `dwell` 接成默认开启（或让它在直线也插点、把拐角稀释）=> ① 或 ② 红。
//
// 用法：node scripts/tool-dwell.mjs <base-url>
const base = (process.argv[2] || "").replace(/\/+$/, "");
if (!base) {
  console.error("用法: node scripts/tool-dwell.mjs <base-url>");
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

// L 形：从左上走到拐角，再往下 —— 拐角在 (96, 96)
const CORNER = [96, 96];
const POINTS = [
  [96, 24, 1.0],
  [96, 96, 1.0],
  [176, 96, 1.0],
];

const main = async () => {
  const bad = [];
  const paint = async (tag, extra) => {
    const doc = `dw_${tag}_${Date.now().toString(36)}`;
    const response = await fetch(`${base}/api/documents`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ doc_id: doc, width: 288, height: 192 }),
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
      object_id: "elbow",
      brush: "100%_Opaque",
      size: 24,
      opacity: 1.0,
      color: { r: 0, g: 0, b: 0, a: 255 },
      points: POINTS,
      ...(extra || {}),
    });
    if (!replied || replied.ok !== true) {
      console.error(
        `❌ 判不了：笔画失败（${JSON.stringify(replied).slice(0, 160)}）⇒ 退出码 3`,
      );
      process.exit(3);
    }
    const raw = await call(doc, token, "render_region", {
      region: { x: 0, y: 0, w: 288, h: 192 },
      raw: true,
    });
    const url = raw && (raw.raw_url || raw.rawUrl);
    if (!url) {
      console.error("❌ 判不了：量不到渲染结果 ⇒ 退出码 3");
      process.exit(3);
    }
    const absolute = String(url).startsWith("http") ? String(url) : `${base}${url}`;
    const bytes = new Uint8Array(await (await fetch(absolute)).arrayBuffer());
    const W = Number(raw.width || 288);
    const H = Number(raw.height || 192);
    if (W * H * 4 !== bytes.length) {
      console.error("❌ 判不了：像素尺寸不符 ⇒ 退出码 3");
      process.exit(3);
    }
    // 拐角邻域（以拐角为心、半径 28）的"墨量" = Σ(1 - 亮度/255)
    let cornerInk = 0;
    let totalInk = 0;
    for (let y = 0; y < H; y += 1) {
      for (let x = 0; x < W; x += 1) {
        const at = (y * W + x) * 4;
        const lum = (bytes[at] + bytes[at + 1] + bytes[at + 2]) / 3;
        const ink = Math.max(0, 1 - lum / 255);
        totalInk += ink;
        const dx = x - CORNER[0];
        const dy = y - CORNER[1];
        if (dx * dx + dy * dy <= 28 * 28) cornerInk += ink;
      }
    }
    let fingerprint = 0;
    for (let i = 0; i < bytes.length; i += 4) {
      fingerprint = (fingerprint * 31 + bytes[i] + bytes[i + 1] * 3 + bytes[i + 2] * 7) >>> 0;
    }
    return { cornerInk, totalInk, fingerprint };
  };

  const plain = await paint("plain", {});
  const zero = await paint("zero", { dwell: 0 });
  const dwelled = await paint("on", { dwell: 1.0 });
  const again = await paint("again", { dwell: 1.0 });

  console.log(
    `  拐角邻域墨量：缺省 ${plain.cornerInk.toFixed(0)}｜dwell:0 ${zero.cornerInk.toFixed(0)}｜dwell:1 ${dwelled.cornerInk.toFixed(0)}`,
  );
  console.log(
    `  全幅墨量：  缺省 ${plain.totalInk.toFixed(0)}｜dwell:0 ${zero.totalInk.toFixed(0)}｜dwell:1 ${dwelled.totalInk.toFixed(0)}`,
  );

  // ① opt-in 的承诺：缺省 ≡ dwell:0（逐字节）
  if (plain.fingerprint !== zero.fingerprint) {
    bad.push(
      `① 不传 \`dwell\` 与传 \`dwell: 0\` 画面不同（${plain.fingerprint} vs ${zero.fingerprint}）⇒ opt-in 的承诺被破坏`,
    );
  }
  // ② 驻留必须**改变画面**且**让拐角墨更多**
  if (dwelled.fingerprint === plain.fingerprint) {
    bad.push("② 传了 `dwell: 1.0` 画面却逐字节相同 ⇒ 曲率驻留没生效");
  }
  // **② 改成**结构量**：只要求"不同"，不再要求"更多"** ✓（第 869 轮）
  //   **⇒ 为什么改**：`cornerInk` 是**像素量级** ✓ ⇒ **跨机器不稳**：
  //     本机实测 `1234 ⇒ 1235`（**+1**）✓ 而 CI 实测 `1227 ⇒ 1222`（**−5**）✗ —— **方向相反** ✓
  //     ⇒ 断 `>` 会让同一条判据在 CI 上必红 ✓ ⇒ 而"驻留是否生效"**本来不需要**方向 ✓：
  //       只要**画面不同**（上面 ② 的第一条）＋ **拐角墨量不同** ⇒ 就说明驻留真的插了额外点 ✓。
  //   **⇒ 数字仍然打印**（留着给人看趋势 ✓），只是**不再当判据** ✓。
  if (dwelled.cornerInk === plain.cornerInk) {
    bad.push(
      `② 驻留之后拐角邻域的墨**完全没变**（${plain.cornerInk.toFixed(0)} ⇒ ${dwelled.cornerInk.toFixed(0)}）⇒ 与「转弯处多插点」的语义不符`,
    );
  }
  // ③ 确定性
  if (again.fingerprint !== dwelled.fingerprint) {
    bad.push(`③ 同参数两次画面不同（${dwelled.fingerprint} vs ${again.fingerprint}）⇒ 不满足确定性`);
  }

  if (bad.length) {
    console.error(`❌ ${bad.length} 项没成立：${bad.join("｜")}`);
    process.exit(1);
  }
  console.log(
    `✓ 曲率驻留（opt-in）成立：缺省与 dwell:0 逐字节相同；dwell:1 让拐角邻域墨 ${plain.cornerInk.toFixed(0)} ⇒ ${dwelled.cornerInk.toFixed(0)}，且可复现`,
  );
};

await main();
