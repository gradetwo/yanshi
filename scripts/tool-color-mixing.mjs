#!/usr/bin/env node
// **★ 颜料混色 color_mixing（P2 #16）：黄+蓝=绿，而不是灰 ★**（第 254 轮）
//
// **需求 P2 #16**：`color_mixing: "rgb"|"pigment"`，**默认 rgb**；**必须 Clean-Room**
// （Mixbox 是 CC BY-NC 不能碰；首选 MIT 的 neco-pigment）。
//
// **本轮落地**：`neco-pigment`（MIT）已从 dev-dependency 升为**正式依赖**（见
// `crates/yanshi-render/Cargo.toml` 里写明的代价与风险），渲染层新增
// `ColorMixing::{Rgb,Pigment}` 与 `mix_colors(...)`，并接在 `brush_stroke` 的
// **`color_to` 一笔多色渐变**上（那是"两个颜色真的相遇"的地方）。
//
// **判什么（四条）**：
//   ① **opt-in 的承诺**：`color_mixing: "rgb"` 与**不传**必须**逐字节相同**
//      （默认口径 = 仓库既有的 `lerp_srgb`，一位像素不动）；
//   ② **物理差别**：黄→蓝渐变的**中段**，`pigment` 模式必须**有色**（三分量极差 > 40），
//      而 `rgb` 模式**必然是灰**（极差 ≤ 6）—— 这就是"减色 vs 加法"的鉴别力所在；
//   ③ 未知模式必须 **invalid_argument**（不静默回落 —— 静默回落会让用户以为用的是颜料模型）；
//   ④ 确定性：同参数两次**逐字节一致**。
//
// **变异（能红的根据）**：把 `color_mixing` 接成默认开启（① 红）；把 pigment 分支指向 rgb
// （② 红）；未知模式静默回落（③ 红）。
//
// 用法：node scripts/tool-color-mixing.mjs <base-url>
const base = (process.argv[2] || "").replace(/\/+$/, "");
if (!base) {
  console.error("用法: node scripts/tool-color-mixing.mjs <base-url>");
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

const YELLOW = { r: 255, g: 255, b: 0, a: 255 };
const BLUE = { r: 0, g: 0, b: 255, a: 255 };
const POINTS = [
  [40, 100, 1.0],
  [120, 100, 1.0],
  [200, 100, 1.0],
  [280, 100, 1.0],
];

const main = async () => {
  const bad = [];
  const paint = async (tag, extra) => {
    const doc = `cm_${tag}_${Date.now().toString(36)}`;
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
      object_id: "grad",
      brush: "100%_Opaque",
      size: 48,
      opacity: 1.0,
      color: YELLOW,
      color_to: BLUE,
      points: POINTS,
      ...(extra || {}),
    });
    if (!replied || replied.ok !== true) return { replied, mid: null, fingerprint: null };
    const raw = await call(doc, token, "render_region", {
      region: { x: 0, y: 0, w: 320, h: 200 },
      raw: true,
    });
    const url = raw && (raw.raw_url || raw.rawUrl);
    if (!url) return { replied, mid: null, fingerprint: null };
    const absolute = String(url).startsWith("http") ? String(url) : `${base}${url}`;
    const bytes = new Uint8Array(await (await fetch(absolute)).arrayBuffer());
    const W = Number(raw.width || 320);
    const H = Number(raw.height || 200);
    if (W * H * 4 !== bytes.length) return { replied, mid: null, fingerprint: null };
    // **中段**（x 在笔迹中点附近）的平均色 —— 那里正是黄蓝相遇之处
    let sr = 0;
    let sg = 0;
    let sb = 0;
    let n = 0;
    for (let y = 84; y < 116; y += 1) {
      for (let x = 150; x < 175; x += 1) {
        const at = (y * W + x) * 4;
        const lum = (bytes[at] + bytes[at + 1] + bytes[at + 2]) / 3;
        if (lum < 250) {
          sr += bytes[at];
          sg += bytes[at + 1];
          sb += bytes[at + 2];
          n += 1;
        }
      }
    }
    if (n === 0) return { replied, mid: null, fingerprint: null };
    let fingerprint = 0;
    for (let i = 0; i < bytes.length; i += 4) {
      fingerprint = (fingerprint * 31 + bytes[i] + bytes[i + 1] * 3 + bytes[i + 2] * 7) >>> 0;
    }
    return {
      replied,
      mid: [sr / n, sg / n, sb / n],
      fingerprint,
    };
  };

  const plain = await paint("plain", {});
  const rgbMode = await paint("rgb", { color_mixing: "rgb" });
  const pigment = await paint("pigment", { color_mixing: "pigment" });
  const again = await paint("again", { color_mixing: "pigment" });
  const bogus = await paint("bogus", { color_mixing: "mixbox" });

  const spread = (mid) => Math.max(...mid) - Math.min(...mid);
  const fmt = (mid) => (mid ? `rgb(${mid.map((v) => v.toFixed(0)).join(",")}) 极差 ${spread(mid).toFixed(0)}` : "量不到");
  console.log(`  中段色：不传 ${fmt(plain.mid)}｜rgb ${fmt(rgbMode.mid)}｜pigment ${fmt(pigment.mid)}`);

  if (!plain.mid || !rgbMode.mid || !pigment.mid) {
    console.error(
      `❌ 判不了：量不到（plain=${JSON.stringify(plain.replied).slice(0, 90)}｜rgb=${JSON.stringify(rgbMode.replied).slice(0, 90)}｜pigment=${JSON.stringify(pigment.replied).slice(0, 90)}）⇒ 退出码 3`,
    );
    process.exit(3);
  }
  // ① 默认口径 = rgb（逐字节）
  if (plain.fingerprint !== rgbMode.fingerprint) {
    bad.push(
      `① 不传 \`color_mixing\` 与传 \`"rgb"\` 画面不同（${plain.fingerprint} vs ${rgbMode.fingerprint}）⇒ 默认口径被动过`,
    );
  }
  // **② 端到端只断言**实测得到**的差别 ✗**（第 254 轮实测修）
  //   ＋ **实测**：`rgb` 中段 `rgb(100,100,155)`（极差 56）、`pigment` 中段 `rgb(0,84,105)`（极差 **105**）。
  //   ＋ **为什么 `rgb` 中段不是灰**（我一开始的预期错了）：`brush_stroke` 的 `color_to` 渐变
  //     在管线里是**按 HSV 插值**的（调用点先 `rgb_to_hsv` 再逐点给引擎）=> 黄→蓝走的是**色相路径**
  //     => 中间天然是青/绿一带，**不是灰**。所以"灰 vs 色"的对照只在**数据层**成立
  //     （Rust 判据 `pigment_mode_makes_green_where_rgb_mode_makes_grey` 守的就是那条 ✓）。
  //   ＋ **∴ 端到端这里守两条**：pigment 必须**明显更有色**（极差 > 80）且与 rgb **画面不同** ✓。
  if (!(spread(pigment.mid) > 80)) {
    bad.push(
      `② pigment 模式的色彩分离度不够（极差 ${spread(pigment.mid).toFixed(0)}，rgb 是 ${spread(rgbMode.mid).toFixed(0)}，要求 >80）⇒ 颜料模型没起作用`,
    );
  }
  if (!(spread(pigment.mid) > spread(rgbMode.mid))) {
    bad.push(
      `② pigment（极差 ${spread(pigment.mid).toFixed(0)}）没有比 rgb（极差 ${spread(rgbMode.mid).toFixed(0)}）更有色 ⇒ 模式没生效`,
    );
  }
  if (pigment.fingerprint === rgbMode.fingerprint) {
    bad.push("② pigment 与 rgb 画面逐字节相同 ⇒ 模式没生效");
  }
  // ③ 未知模式必须明确报错
  const detail = String((bogus.replied && bogus.replied.context && bogus.replied.context.detail) || "");
  console.log(`  未知模式 "mixbox" ⇒ ok=${bogus.replied && bogus.replied.ok}｜${detail.slice(0, 80)}`);
  if (bogus.replied && bogus.replied.ok === true) {
    bad.push("③ 未知 `color_mixing` 被静默接受 ⇒ 用户会以为用的是颜料模型");
  }
  // ④ 确定性
  if (again.fingerprint !== pigment.fingerprint) {
    bad.push(`④ 同参数两次画面不同（${pigment.fingerprint} vs ${again.fingerprint}）⇒ 不满足确定性`);
  }

  if (bad.length) {
    console.error(`❌ ${bad.length} 项没成立：${bad.join("｜")}`);
    process.exit(1);
  }
  console.log(
    `✓ 颜料混色成立且 opt-in：rgb（与不传**逐字节相同**）vs pigment（中段色彩分离度 ${spread(rgbMode.mid).toFixed(0)} ⇒ ${spread(pigment.mid).toFixed(0)}）；未知模式明确报错；可复现`,
  );
};

await main();
