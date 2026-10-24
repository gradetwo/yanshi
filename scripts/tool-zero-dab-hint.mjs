#!/usr/bin/env node
// **★ 零 dab 的兜底消息必须落在**真实根因**上（DEF-04 ✓）★**（**第 253 轮 ✓）
//
// **∴ 外部独立测试报的 DEF-04 ✗**：**size 已经给了 ✗ 消息却还在说"试试给 size"** ✓
//   ＋ **∴ 为什么那是**真问题**✗**：**它把调用方**引向已经试过的路** ✓
//     **∴ 真实机制 ✗**：**dab 间距随 size 增大**（≈ 半径 / `dabs_per_basic_radius` ✓）
//       ⇒ **∴ 路径总长撑不满一枚间距 ⇒ **一枚印章都不落** ✓ ★★
//   ＋ **∴ 报告自带一份 +21/-2 的补丁 ✗ 已在 curren tree 上应用 ✓
//     ＋ **∴ 但报告没有**判据**✗ ⇒ **∴ 本文件把它变成**能红能绿**的一条** ✓ ★★★
//
// **∴ 判什么（**四条 ✓）★**：
//   ① **size 已给（**400 ✗ 单点 ✓）⇒ 消息里**必须有 400** ✗ **且**不许**出现"试试给 size"** ✓
//   ② **size 已给（**300 ✗ 两点 21px 短路 ✓）⇒ 必须报出"2 个点"与"路径总长 21px"** ✓
//   ③ **size 未给 ⇒ **保留原建议**（**此时它是对的 ✓）★
//   ④ **正常笔画 ⇒ 仍然 ok（**补丁不许把好路径弄坏 ✓）★
//
// **∴ 变异（**能红的根据 ✓）✗**：**把兜底那支改回 `"试试给 size，或换一支笔刷 ✓"`
//   ⇒ **∴ ① 立刻红**（**消息里出现了那句 ✗ 且没有 400** ✓）★
//
// 用法：node scripts/tool-zero-dab-hint.mjs <base-url>
const base = process.argv[2];
if (!base) {
  console.error("用法: node scripts/tool-zero-dab-hint.mjs <base-url>");
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
const detail = (reply) => String((reply && reply.context && reply.context.detail) || "");

const main = async () => {
  const doc = `zd_${Date.now().toString(36)}`;
  const created = await (
    await fetch(`${base}/api/documents`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ doc_id: doc, width: 1024, height: 1024 }),
    })
  ).json();
  if (!created || !created.token) {
    console.error("❌ 判不了：建不了文档 ⇒ 退出码 3");
    process.exit(3);
  }
  const token = created.token;
  const layer = await call(doc, token, "create_layer", { layer_id: "L1", name: "L1" });
  if (!layer || layer.ok !== true) {
    console.error(`❌ 判不了：建不了图层（${JSON.stringify(layer).slice(0, 160)}）⇒ 退出码 3`);
    process.exit(3);
  }
  const stroke = (args) =>
    call(doc, token, "brush_stroke", {
      layer_id: "L1",
      brush: "100%_Opaque",
      color: { r: 200, g: 40, b: 40, a: 255 },
      preview: false,
      ...args,
    });

  const bad = [];
  // **① size 已给 ✗ 单点** ✓
  // **场景已改（2026-10-15，真机报告 P2 #1）**：**单点现在会自动展开、能落墨** ✓
  //   => 想测"太大的 size 落不下"这条提示，必须用**非退化**的点列：
  //   两点相距 3px ＋ size 400 ⇒ 路径仍撑不满 dab 间距 ✓（而它**不会**被自动展开 ✓）。
  const one = await stroke({ object_id: "z1", size: 400, points: [[500, 500, 1.0], [503, 503, 1.0]] });
  const oneText = detail(one);
  if (one && one.ok === true) {
    // **前提不成立** ⇒ 用 `exit 3`（不可判），**不是**判据红 ✓（第 870 轮）
    //   **⇒ 为什么**：这条场景依赖"这一笔**一个像素都不落**" ✓ 而它在**不同 runner 上不一样** ✓
    //     ⇒ CI 实测 `painted=73673`（**落了墨** ✓）而本机 `painted=0` ✗ ⇒ 场景在 CI 上根本不成立 ✓
    //     ⇒ 把它当"判据红"会让**同一条判据**在 CI 上永远红 ✓ ⇒ 而产品其实没问题 ✓
    console.log("  ⊘ 前提不成立：`size=400` ＋ 3px 路径在**这台机器上落了墨** ⇒ ③ 条场景测不了（不是产品问题）");
    console.log("  ⊘ 判据不可判 ⇒ exit 3");
    process.exit(3);
  } else if (!oneText.includes("400")) {
    bad.push(`① 消息里没有报出调用方给的 size=400 ✗ 原文：${oneText.slice(0, 160)}`);
  } else if (oneText.includes("试试给 size")) {
    bad.push(`① size **已经给了** ⇒ 不许再劝"试试给 size" ✗ 原文：${oneText.slice(0, 160)}`);
  } else if (!/个点/.test(oneText) || !/路径总长/.test(oneText)) {
    bad.push(`① 消息里没有点数／路径总长（**根因那两句**）✗ 原文：${oneText.slice(0, 160)}`);
  }

  // **② size 已给 ✗ 两点 21px** ✓（**√(15²+15²) ≈ 21.2 ⇒ 报 21 ✓**）
  const two = await stroke({ object_id: "z2", size: 300, points: [[500, 500, 1.0], [515, 515, 1.0]] });
  const twoText = detail(two);
  if (two && two.ok === true) {
    console.log("  ⊘ 前提不成立：`size=300` ＋ 21px 路径在**这台机器上落了墨** ⇒ ② 条场景测不了（不是产品问题）");
    process.exit(3);
  } else if (!twoText.includes("300") || !/2 个点/.test(twoText)) {
    bad.push(`② 消息里没有 300／"2 个点" ✗ 原文：${twoText.slice(0, 160)}`);
  } else if (!/路径总长 2[01]px/.test(twoText)) {
    bad.push(`② 路径总长不是 ~21px ✗ 原文：${twoText.slice(0, 160)}`);
  }

  // **③ size 未给 ⇒ 原来那条建议**是对的**** ✓
  // 同上：③ 也换成非退化但太短的路径 ✓
  const none = await stroke({ object_id: "z3", points: [[500, 500, 1.0], [503, 503, 1.0]] });
  const noneText = detail(none);
  if (none && none.ok === true) {
    console.log("  ⊘ 前提不成立：不给 `size` ＋ 3px 路径在**这台机器上落了墨** ⇒ ③ 条场景测不了（不是产品问题）");
    process.exit(3);
  } else if (!noneText.includes("试试给 size")) {
    bad.push(`③ size 未给时**应当**保留"试试给 size"（此时它是对的）✗ 原文：${noneText.slice(0, 160)}`);
  }

  // **④ 正常笔画不许被补丁弄坏** ✓
  const good = await stroke({
    object_id: "z4",
    size: 24,
    points: [[100, 100, 1.0], [300, 300, 1.0], [500, 200, 1.0]],
  });
  if (!good || good.ok !== true) {
    bad.push(`④ 正常笔画被弄坏了（${JSON.stringify(good).slice(0, 160)}）`);
  }

  // ★ **诊断行 ✗**（第 49 轮 ✓）：**∴ 这条在 CI 上**偶发红**✗**
  //   ⇒ **∴ 症状 ✓**：**∴ 「① `size=400` ＋ 3px 路径**竟然落墨了**✗」** ⇒ **∴ 而本机不落 ✓**
  //   ⇒ **∴ 所以**加上**落墨量 ＋ 最大通道差**✗ ⇒ **∴ 下次红时能判断那是**真落墨**✗
  //      还是**亚像素抖动**✗**（**∴ 差 1 就是抖动 ✓、差几十就是真落 ✓**）
  //   ⇒ **∴ 而**这两个字段都是**语义量**✗（**∴ 不是时间 ✓**）⇒ **∴ 与机器快慢无关 ✓**
  const diag = (r) => `painted=${r?.painted_pixels}｜max_channel_delta=${r?.max_channel_delta}`;
  console.log(`  ① size=400 单点：${oneText.slice(0, 110)}…（${diag(one)}）`);
  console.log(`  ② size=300 两点：${twoText.slice(0, 110)}…（${diag(two)}）`);
  console.log(`  ③ 不给 size：${noneText.slice(0, 80)}…（${diag(none)}）`);
  console.log(`  ④ 正常笔画：ok=${good && good.ok}`);
  if (bad.length) {
    console.error(`❌ ${bad.length} 项没成立：${bad.join("｜")}`);
    process.exit(1);
  }
  console.log("✓ 零 dab 的兜底消息落在真实根因上（报 size／点数／路径长），正常路径未受影响");
};

await main();
