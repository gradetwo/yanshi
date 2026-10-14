#!/usr/bin/env node
// **★ humanize：默认拟人，但显式压力者一位像素不动（P1 #11）★**（第 254 轮 ✓）
//
// **文档 P1 #11**：`brush_stroke` 加 `humanize`（**默认 true**）；硬约束
// **`humanize:false` 时老行为逐字节不变**。**附录二**给了非对称起收笔包络（起 12%／行 60%／收 28%）。
//
// **爆炸半径怎么控住**（设计决定）：**显式给了每个点的压力 => humanize 什么都不做**；
// 只有"只给坐标"（压力缺省 0.5）时才套包络。
//
// **本判据只断言**可证明**的东西 ✗**（前两版栽在这里，写下来）：
//   ＋ 第一版量**宽度**：`100%_Opaque` 的宽度由 size 决定、不吃压感 => ①④ 剖面完全一样，
//     我误判"包络没生效"（那 0.75×/0.71× 只是**圆头几何**）。
//   ＋ 第二版量**墨浓均值**：换 `Clouds` 后中段浓 0.008 而头 0.14，数值反而倒挂
//     （斑驳笔刷的"有墨像素均值"没有意义）。
//   => **∴ 结论 ✗**：**"包络的形状与不对称"交给 Rust 单测**（
//      `tools::tests::humanize_envelope_is_asymmetric` —— 纯数据变换，决定性 ✓）；
//      **本判据只守两条**端到端性质：**默认与关闭必须不同**、**显式压力时两者必须逐字节相同** ✓。
//
// 用法：node scripts/tool-humanize.mjs <base-url>
const base = (process.argv[2] || "").replace(/\/+$/, "");
if (!base) {
  console.error("用法: node scripts/tool-humanize.mjs <base-url>");
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
  const bad = [];
  const paint = async (tag, points, extra) => {
    const doc = `hu_${tag}_${Date.now().toString(36)}`;
    const response = await fetch(`${base}/api/documents`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ doc_id: doc, width: 512, height: 320 }),
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
      object_id: "stroke",
      // **∴ 用 `Clouds` 而不是 `100%_Opaque` ✗**（第 254 轮实测 ✓）：
      //   **`100%_Opaque` 的落墨**不吃压感**（宽度由 size 决定 ✓）⇒ **∴ 包络改了压力却看不出** ✓
      //   **∴ 而 `Clouds` 吃**（实测默认 vs 关闭的画面**不同** ✓）★★
      brush: "Clouds",
      size: 48,
      opacity: 1.0,
      color: { r: 10, g: 10, b: 10, a: 255 },
      points,
      ...(extra || {}),
    });
    if (!replied || replied.ok !== true) return { replied, fingerprint: null };
    const raw = await call(doc, token, "render_region", {
      region: { x: 0, y: 90, w: 512, h: 140 },
      raw: true,
    });
    const url = raw && (raw.raw_url || raw.rawUrl);
    if (!url) return { replied, fingerprint: null };
    const absolute = String(url).startsWith("http") ? String(url) : `${base}${url}`;
    const bytes = new Uint8Array(await (await fetch(absolute)).arrayBuffer());
    let sum = 0;
    for (let i = 0; i < bytes.length; i += 4) {
      sum = (sum * 31 + bytes[i] + bytes[i + 1] * 3 + bytes[i + 2] * 7) >>> 0;
    }
    return { replied, fingerprint: sum };
  };

  const line = [];
  for (let i = 0; i <= 16; i += 1) line.push([40 + i * 26, 160]);

  // ① 点里没压力 => 默认（humanize on）与 humanize:false **必须不同**
  const autoOn = await paint("auto_on", line, {});
  const autoOff = await paint("auto_off", line, { humanize: false });
  if (!autoOn.fingerprint || !autoOff.fingerprint) {
    console.error(
      `❌ 判不了：笔画失败（on=${JSON.stringify(autoOn.replied).slice(0, 110)}｜off=${JSON.stringify(autoOff.replied).slice(0, 110)}）⇒ 退出码 3`,
    );
    process.exit(3);
  }
  console.log(`  ① 无压力：默认指纹 ${autoOn.fingerprint}｜humanize:false 指纹 ${autoOff.fingerprint}`);
  if (autoOn.fingerprint === autoOff.fingerprint) {
    bad.push("① 点里没有压力时，默认与 humanize:false 的画面**逐字节相同** ⇒ 拟人包络没生效");
  }

  // ② 显式压力 => 默认与 false 必须**逐字节相同**（爆炸半径的保证）
  const explicit = line.map(([x, y], i) => [x, y, 0.35 + (i % 5) * 0.1]);
  const expOn = await paint("exp_on", explicit, {});
  const expOff = await paint("exp_off", explicit, { humanize: false });
  console.log(`  ② 显式压力：默认指纹 ${expOn.fingerprint}｜humanize:false 指纹 ${expOff.fingerprint}`);
  if (!expOn.fingerprint || !expOff.fingerprint) {
    bad.push("② 显式压力那两笔画不上");
  } else if (expOn.fingerprint !== expOff.fingerprint) {
    bad.push("② 显式给了压力时，默认（humanize=true）仍改了画面 ⇒ 应当一位像素都不动");
  }

  // ③ 确定性：同样参数两次，默认行为逐字节一致
  const again = await paint("auto_again", line, {});
  if (again.fingerprint !== autoOn.fingerprint) {
    bad.push(`③ 同参数两次调用画面不同（${autoOn.fingerprint} vs ${again.fingerprint}）⇒ 不满足确定性`);
  } else {
    console.log(`  ③ 确定性：默认行为两次一致（${again.fingerprint}）`);
  }

  if (bad.length) {
    console.error(`❌ ${bad.length} 项没成立：${bad.join("｜")}`);
    process.exit(1);
  }
  console.log(
    "✓ humanize 端到端性质成立：无压力时默认与关闭**不同**（包络生效），显式压力时两者**逐字节相同**（不动老行为），且可复现",
  );
};

await main();
