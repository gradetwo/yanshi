#!/usr/bin/env node
// **★ 草稿层 / 正稿层 与"一键隐藏"（P1 #13）★**（第 254 轮）
//
// **需求文档 P1 #13**："草稿层/正稿层（draft 淡蓝高抖动 0.35 + ink 正式勾线 + 一键隐藏）"。
//
// **先查"仓库有没有"（本仓纪律）结论：三件都已经有，不需要新工具** ✓
//   * **图层可见性**：文档模型本来就有 `Layer.visible`（`yanshi-core/src/state.rs:109`，
//     fold 从原子 payload 读 `visible`，见 `fold.rs:819`）=> 而 `set_property` 是**通用**属性写入口
//     （`set_layer_blend` 就是它的薄封装）=> `set_property{key:"visible", value:false}` **直接可用**。
//   * **高抖动**：笔刷动力学旋钮（`offset_by_random` / `dabs_per_basic_radius` 等）经 `brush_params`
//     就能开 => "高抖动"是**参数**，不是缺功能。
//   * **淡蓝**：`color` 参数即可（本判据用 `#6f9fd8` 一类的淡蓝）。
//
// **所以本判据守的是"这套工作流真的能跑通"**（配方一旦被改坏就红）：
//   ① 草稿层 + 正稿层同时可见时，画面里**两层都有**（草稿的淡蓝与正稿的深色都能量到）；
//   ② `set_property{key:"visible", value:false}` 关掉**草稿层** => 草稿的淡蓝**消失**、
//      正稿的深色**还在**（这是"一键隐藏"的核心语义：只藏草稿、不动正稿）；
//   ③ 再设回 `true` => 草稿**回来**（可逆）；
//   ④ 确定性：同配方两次渲染**逐字节一致**。
//
// **变异（能红的根据）**：把 `visible` 的写入接错层（或让它不生效）=> ② 立刻红；
// 关掉草稿层时若把**正稿**也藏了 => ② 的后半条红。
//
// 用法：node scripts/tool-draft-ink.mjs <base-url>
const base = (process.argv[2] || "").replace(/\/+$/, "");
if (!base) {
  console.error("用法: node scripts/tool-draft-ink.mjs <base-url>");
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

// 草稿：淡蓝（#6f9fd8 => 111,159,216）；正稿：近黑深褐
const DRAFT = { r: 111, g: 159, b: 216, a: 255 };
const INK = { r: 40, g: 30, b: 26, a: 255 };

const main = async () => {
  const bad = [];
  const doc = `di_${Date.now().toString(36)}`;
  const response = await fetch(`${base}/api/documents`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: 320, height: 240 }),
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

  // 草稿层 + 正稿层
  for (const [id, name] of [["draft", "draft"], ["ink", "ink"]]) {
    const made = await call(doc, token, "create_layer", { layer_id: id, name });
    if (!made || made.ok !== true) {
      console.error(`❌ 判不了：建图层 ${id} 失败（${JSON.stringify(made).slice(0, 120)}）⇒ 退出码 3`);
      process.exit(3);
    }
  }
  // 草稿：淡蓝 + 高抖动（参数即可）；正稿：细深色线
  await call(doc, token, "brush_stroke", {
    layer_id: "draft",
    object_id: "draft_line",
    brush: "100%_Opaque",
    size: 10,
    color: DRAFT,
    points: [[40, 120, 0.8], [120, 110, 0.8], [200, 120, 0.8], [280, 112, 0.8]],
    brush_params: { offset_by_random: 1.2, dabs_per_basic_radius: 3.0 },
  });
  await call(doc, token, "brush_stroke", {
    layer_id: "ink",
    object_id: "ink_line",
    brush: "100%_Opaque",
    size: 4,
    color: INK,
    points: [[40, 140, 1.0], [280, 140, 1.0]],
  });

  const view = async () => {
    const raw = await call(doc, token, "render_region", {
      region: { x: 0, y: 0, w: 320, h: 240 },
      raw: true,
    });
    const url = raw && (raw.raw_url || raw.rawUrl);
    if (!url) return null;
    const absolute = String(url).startsWith("http") ? String(url) : `${base}${url}`;
    const bytes = new Uint8Array(await (await fetch(absolute)).arrayBuffer());
    const W = Number(raw.width || 320);
    const H = Number(raw.height || 240);
    if (W * H * 4 !== bytes.length) return null;
    let blue = 0;
    let dark = 0;
    let fingerprint = 0;
    for (let y = 0; y < H; y += 1) {
      for (let x = 0; x < W; x += 1) {
        const at = (y * W + x) * 4;
        const [r, g, b] = [bytes[at], bytes[at + 1], bytes[at + 2]];
        // 淡蓝：蓝分量明显高于红，且整体偏亮
        if (b > r + 30 && b > 150) blue += 1;
        // 深色（正稿）：整体很暗
        if (r + g + b < 260) dark += 1;
      }
    }
    for (let i = 0; i < bytes.length; i += 4) {
      fingerprint = (fingerprint * 31 + bytes[i] + bytes[i + 1] * 3 + bytes[i + 2] * 7) >>> 0;
    }
    return { blue, dark, fingerprint };
  };

  const both = await view();
  if (!both) {
    console.error("❌ 判不了：量不到渲染结果 ⇒ 退出码 3");
    process.exit(3);
  }
  console.log(`  ① 两层都可见：淡蓝 ${both.blue} 像素｜深色 ${both.dark} 像素`);

  const hide = await call(doc, token, "set_property", {
    layer_id: "draft",
    key: "visible",
    value: false,
  });
  if (!hide || hide.ok !== true) {
    bad.push(`② set_property{visible:false} 失败：${JSON.stringify(hide).slice(0, 140)}`);
  }
  const hidden = await view();
  console.log(
    `  ② 关掉草稿层：淡蓝 ${hidden ? hidden.blue : "?"} 像素（原 ${both.blue}）｜深色 ${hidden ? hidden.dark : "?"}（原 ${both.dark}）`,
  );

  const show = await call(doc, token, "set_property", {
    layer_id: "draft",
    key: "visible",
    value: true,
  });
  const back = await view();
  console.log(`  ③ 再打开草稿层：淡蓝 ${back ? back.blue : "?"} 像素`);

  // ④ 确定性：同配方另建一份
  const doc2 = `${doc}_b`;
  const created2 = await (
    await fetch(`${base}/api/documents`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ doc_id: doc2, width: 320, height: 240 }),
    })
  ).json();
  await call(doc2, created2.token, "create_layer", { layer_id: "draft", name: "draft" });
  await call(doc2, created2.token, "create_layer", { layer_id: "ink", name: "ink" });
  await call(doc2, created2.token, "brush_stroke", {
    layer_id: "draft",
    object_id: "draft_line",
    brush: "100%_Opaque",
    size: 10,
    color: DRAFT,
    points: [[40, 120, 0.8], [120, 110, 0.8], [200, 120, 0.8], [280, 112, 0.8]],
    brush_params: { offset_by_random: 1.2, dabs_per_basic_radius: 3.0 },
  });
  await call(doc2, created2.token, "brush_stroke", {
    layer_id: "ink",
    object_id: "ink_line",
    brush: "100%_Opaque",
    size: 4,
    color: INK,
    points: [[40, 140, 1.0], [280, 140, 1.0]],
  });
  const both2 = await view2(doc2, created2.token);

  // 判定
  if (both.blue <= 0) {
    bad.push("① 草稿层没画上（淡蓝像素为 0）");
  }
  if (both.dark <= 0) {
    bad.push("① 正稿层没画上（深色像素为 0）");
  }
  if (!hidden) {
    bad.push("② 关掉草稿层后量不到画面");
  } else {
    if (hidden.blue > Math.max(5, both.blue * 0.05)) {
      bad.push(
        `② 关掉草稿层之后淡蓝仍在（${both.blue} ⇒ ${hidden.blue}）⇒ "一键隐藏"没生效`,
      );
    }
    if (hidden.dark < both.dark * 0.8) {
      bad.push(
        `② 关掉草稿层把**正稿**也藏了（深色 ${both.dark} ⇒ ${hidden.dark}）⇒ 隐藏必须只作用于草稿层`,
      );
    }
  }
  if (!show || show.ok !== true) {
    bad.push(`③ 重新打开草稿层失败：${JSON.stringify(show).slice(0, 140)}`);
  } else if (!back || back.blue < both.blue * 0.8) {
    bad.push(`③ 重新打开后草稿没回来（淡蓝 ${back ? back.blue : "?"} vs 原 ${both.blue}）`);
  }
  if (!both2 || both2.fingerprint !== both.fingerprint) {
    bad.push(
      `④ 同配方两次渲染不一致（${both.fingerprint} vs ${both2 ? both2.fingerprint : "?"}）⇒ 不满足确定性`,
    );
  }

  if (bad.length) {
    console.error(`❌ ${bad.length} 项没成立：${bad.join("｜")}`);
    process.exit(1);
  }
  console.log(
    `✓ 草稿/正稿工作流可用（无需新工具）：草稿淡蓝 ${both.blue} ⇒ 隐藏后 ${hidden.blue}（正稿深色 ${both.dark} 保持不变），再打开回到 ${back.blue}，且可复现`,
  );

  async function view2(d, t) {
    const raw = await call(d, t, "render_region", {
      region: { x: 0, y: 0, w: 320, h: 240 },
      raw: true,
    });
    const url = raw && (raw.raw_url || raw.rawUrl);
    if (!url) return null;
    const absolute = String(url).startsWith("http") ? String(url) : `${base}${url}`;
    const bytes = new Uint8Array(await (await fetch(absolute)).arrayBuffer());
    let fingerprint = 0;
    for (let i = 0; i < bytes.length; i += 4) {
      fingerprint = (fingerprint * 31 + bytes[i] + bytes[i + 1] * 3 + bytes[i + 2] * 7) >>> 0;
    }
    return { fingerprint };
  }
};

await main();
