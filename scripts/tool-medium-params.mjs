#!/usr/bin/env node
// **★ medium_stroke 的介质参数（P0 #5）★**（第 254 轮 ✓）
//
// **∴ 文档怎么说 ✗**：**"`medium_stroke` 只能传介质名和颜色 ✗ `load`、`mixing`、`wetness`
//   等暴露不出来"** ⇒ **∴ 要加一个 `medium_params` 参数** ✓ ★
//
// **∴ 读代码的实际结果（**先查库／先读代码 ✓）✗**：**三个都已经在** ✗ 只差一个** ✓
//   ＋ **∴ `medium_stroke` 的 handler **早就从 args 读** `load`／`wetness`／`texture`** ✓
//     （tools.rs:12117/12118/12121，默认 1.0／0.4／0.0 ✓ —— 与 `StrokeSettings::new` 一致 ✓）
//   ＋ **∴ 引擎侧也有**：`yanshi_medium_host::StrokeSettings { load, wetness, texture }` ✓
//     直接喂进 `dab()`（`floats_of` 把它们传进插件 ABI ✓）
//   ＋ **∴ spec 也**已经声明**了 `load` 与 `wetness`** ✓
//     ＋ **∴ **唯一缺口：`texture` 没在 spec 里声明**** ✗
//       ⇒ **∴ 而白名单是**照 spec 生成**的 ⇒ **handler 里那行读 `texture` 是**死代码**✗
//         **调用方一传就被拒** ✓ ★★★
//
// **∴ 判什么（**四条 ✓）★**：
//   ① `load`／`wetness`／`texture` **三个都要被接受**（**∴ 不被"不接受参数"拒掉 ✓）★
//   ② `load` 0.1 vs 1.0 ⇒ **画面必须不同**（**∴ 证明它真的生效 ✗ 不是收了不用** ✓）★★
//   ③ `wetness` 0.0 vs 0.9 ⇒ **画面必须不同** ✓ ★★
//   ④ `texture` 0.0 vs 1.0 ⇒ **画面必须不同** ✓ ★★
//
// **∴ 变异（**能红的根据 ✓）✗**：**把 spec 里 `texture` 那行删掉 ⇒ **① 立刻红**（被白名单拒 ✓）★
//
// 用法：node scripts/tool-medium-params.mjs <base-url>
const base = (process.argv[2] || "").replace(/\/+$/, "");
if (!base) {
  console.error("用法: node scripts/tool-medium-params.mjs <base-url>");
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
  const paint = async (tag, extra) => {
    const doc = `md_${tag}_${Date.now().toString(36)}`;
    // **★ 建文档要**稳健**✗ 不能假定回执一定是 JSON** ✓（**第 254 轮实测 ✓）**：
    //   ＋ **∴ 踩到的坑 ✗**：**我把服务端起在 8080 ✗ 那上面**另有服务**✗ 回的是纯文本
    //     `unauthorized`** ⇒ **`.json()` 直接抛 ⇒ 判据**崩掉**✗ 而不是"判不了" ✓
    //       ⇒ **∴ 现在**先读文本 ✗ 解析失败就**打印并退 3** ✓ ★★
    const createdResponse = await fetch(`${base}/api/documents`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ doc_id: doc, width: 384, height: 200 }),
    });
    const createdText = await createdResponse.text();
    let created = null;
    try {
      created = JSON.parse(createdText);
    } catch {
      console.error(
        `❌ 判不了：${base}/api/documents 回的不是 JSON（HTTP ${createdResponse.status}）：${createdText.slice(0, 120)}`,
      );
      process.exit(3);
    }
    if (!created || !created.token) return null;
    const token = created.token;
    await call(doc, token, "create_layer", { layer_id: "L1", name: "L1" });
    const args = {
      layer_id: "L1",
      object_id: "s",
      medium: "oil",
      size: 48,
      color: { r: 30, g: 60, b: 140, a: 255 },
      points: [[60, 100, 1.0], [160, 96, 1.0], [260, 104, 1.0], [340, 100, 1.0]],
    };
    Object.assign(args, extra || {});
    const replied = await call(doc, token, "medium_stroke", args);
    if (!replied || replied.ok !== true) return { replied, stats: null };
    const raw = await call(doc, token, "render_region", {
      region: { x: 20, y: 40, w: 344, h: 120 },
      raw: true,
    });
    const url = raw && (raw.raw_url || raw.rawUrl);
    if (!url) return { replied, stats: null };
    const absolute = String(url).startsWith("http") ? String(url) : `${base}${url}`;
    const bytes = new Uint8Array(await (await fetch(absolute)).arrayBuffer());
    let ink = 0;
    let sum = 0;
    for (let i = 0; i < bytes.length; i += 4) {
      if (bytes[i] + bytes[i + 1] + bytes[i + 2] < 720) ink += 1;
      sum = (sum * 31 + bytes[i] + bytes[i + 1] * 3 + bytes[i + 2] * 7) >>> 0;
    }
    return { replied, stats: { ink, fingerprint: sum } };
  };

  const bad = [];
  const cases = [
    ["① 三个参数一起传", { load: 1.0, wetness: 0.4, texture: 0.5 }, true],
    ["② load 0.1", { load: 0.1 }, false],
    ["② load 1.0", { load: 1.0 }, false],
    ["③ wetness 0.0", { wetness: 0.0 }, false],
    ["③ wetness 0.9", { wetness: 0.9 }, false],
    ["④ texture 0.0", { texture: 0.0 }, false],
    ["④ texture 1.0", { texture: 1.0 }, false],
  ];
  const results = new Map();
  for (const [label, extra] of cases) {
    const r = await paint(label.replace(/[^\w]/g, ""), extra);
    if (!r) {
      console.error("❌ 判不了：建不了文档 ⇒ 退出码 3");
      process.exit(3);
    }
    const detail = (r.replied && r.replied.context && r.replied.context.detail) || "";
    const rejected = r.replied && r.replied.ok === false && /不接受参数/.test(String(detail));
    console.log(
      `  ${label}：ok=${r.replied && r.replied.ok}${rejected ? "（白名单拒了这个参数）" : ""}｜墨 ${r.stats ? r.stats.ink : "-"}｜指纹 ${r.stats ? r.stats.fingerprint : "-"}`,
    );
    results.set(label, { replied: r.replied, stats: r.stats, rejected, detail: String(detail) });
  }

  const first = results.get("① 三个参数一起传");
  if (first.rejected) {
    console.error(`❌ ① 介质参数还没接上：${first.detail.slice(0, 160)}`);
    process.exit(1);
  }
  if (!first.stats) {
    bad.push(`① 同时传三个参数时笔画失败：${first.detail.slice(0, 140)}`);
  }
  const differs = (a, b, name) => {
    const x = results.get(a);
    const y = results.get(b);
    if (!x.stats || !y.stats) {
      bad.push(`${name}：有一边画不上（${x.detail.slice(0, 80)} / ${y.detail.slice(0, 80)}）`);
      return;
    }
    if (x.stats.fingerprint === y.stats.fingerprint) {
      bad.push(`${name}：两次画面**逐字节相同** ⇒ 这个参数被收了但没生效`);
    }
  };
  differs("② load 0.1", "② load 1.0", "② load");
  differs("③ wetness 0.0", "③ wetness 0.9", "③ wetness");
  differs("④ texture 0.0", "④ texture 1.0", "④ texture");

  if (bad.length) {
    console.error(`❌ ${bad.length} 项没成立：${bad.join("｜")}`);
    process.exit(1);
  }
  console.log("✓ 介质参数可用且都真的生效：load／wetness／texture（文档那句「暴露不出来」已过时）");
};

await main();
