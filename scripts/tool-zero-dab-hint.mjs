#!/usr/bin/env node
// **★ 短笔触兜底的提示必须落在**真实根因**上（DEF-04 的后续 ✓）★**
//
// **∴ 语义已变（反馈期 1：3.6a）✓**：路径总长撑不满一枚 dab 间距时，
//   旧行为是 `precondition_failed` 拒绝（本判据旧版断言的就是那份拒绝文案）；
//   新行为是**延长落一枚 dab、照常成功**，诊断事实（size／点数／路径长／间距）
//   降为回执 `warnings` 里的一条提示，不再是拒绝理由。
//   ⇒ 本判据随之反转：断言**成功 + 落墨 + 提示在位**，不再断言拒绝。
//   旧版“前提不成立 ⇒ exit 3”的那套（CI 落墨、本机不落墨的环境差异）
//   已随兜底落地而消失：两边都确定落墨。
//
// **∴ 判什么（**四条 ✓）★**：
//   ① **size 已给（**400 ✗ 两点 4.2px 短路 ✓）⇒ ok:true、painted>0，
//      提示里必须有 400、"2 个点"、"路径总长"、"dab 间距"，且不许出现"试试给 size"**
//      （size 已经给了，再劝就是误导 —— DEF-04 的原要求，提示里同样守 ✓）
//   ② **size 已给（**300 ✗ 两点 21px 短路 ✓）⇒ 同上，提示里必须有 300／"2 个点"／路径总长 ~21px**
//   ③ **size 未给（两点 4.2px）⇒ 同样成功 + 提示在位**（"试试给 size"那条旧建议已退役：
//      不给 size 的短路径现在也兜底落墨，不再拿“给 size”当出路 ✓）
//   ④ **正常笔画 ⇒ 仍然 ok，且不许带兜底提示**（补丁不许把好路径弄坏 ✓，也不许乱贴提示 ✓）
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
  const warnText = (reply) => (reply.warnings || []).join(" ");

  const bad = [];
  const checkShort = (label, reply, sizeText, pathPattern) => {
    const text = warnText(reply);
    if (!reply || reply.ok !== true) {
      bad.push(`${label} 短路径现在应当成功（兜底落墨），实测 ok=${reply && reply.ok}`);
    } else if (!(Number(reply.painted_pixels ?? 0) > 0)) {
      bad.push(`${label} 短路径必须真的落墨（painted_pixels > 0）`);
    } else if (!text.includes(sizeText)) {
      bad.push(`${label} 提示里没有报出调用方给的 size=${sizeText} ✗ 提示：${text.slice(0, 160)}`);
    } else if (text.includes("试试给 size")) {
      bad.push(`${label} size **已经给了**（或兜底已生效）⇒ 不许再劝"试试给 size" ✗ 提示：${text.slice(0, 160)}`);
    } else if (!/个点/.test(text) || !/路径总长/.test(text) || !/dab 间距/.test(text)) {
      bad.push(`${label} 提示里没有点数／路径总长／dab 间距（**根因那几句**）✗ 提示：${text.slice(0, 160)}`);
    } else if (pathPattern && !pathPattern.test(text)) {
      bad.push(`${label} 提示里的路径总长不对 ✗ 提示：${text.slice(0, 160)}`);
    }
  };

  // **① size=400 ＋ 两点 4.2px** ✓
  const one = await stroke({ object_id: "z1", size: 400, points: [[500, 500, 1.0], [503, 503, 1.0]] });
  checkShort("①", one, "400", /路径总长 4\.2px/);

  // **② size=300 ＋ 两点 21px** ✓（√(15²+15²) ≈ 21.2 ⇒ 报 21.2 ✓）
  const two = await stroke({ object_id: "z2", size: 300, points: [[500, 500, 1.0], [515, 515, 1.0]] });
  checkShort("②", two, "300", /路径总长 21\.2px/);

  // **③ 不给 size ＋ 两点 4.2px** ✓（笔刷自带半径的间距约 4.6px，仍撑不满 ⇒ 同样兜底）
  const none = await stroke({ object_id: "z3", points: [[500, 500, 1.0], [503, 503, 1.0]] });
  checkShort("③", none, "", null);
  if (none && none.ok === true && warnText(none).includes("试试给 size")) {
    bad.push(`③ 兜底提示里不许再劝"试试给 size"（短路径已落墨，不再拿给 size 当出路）`);
  }

  // **④ 正常笔画不许被补丁弄坏，且不许乱贴兜底提示** ✓
  const good = await stroke({
    object_id: "z4",
    size: 24,
    points: [[100, 100, 1.0], [300, 300, 1.0], [500, 200, 1.0]],
  });
  if (!good || good.ok !== true) {
    bad.push(`④ 正常笔画被弄坏了（${JSON.stringify(good).slice(0, 160)}）`);
  } else if (warnText(good).includes("dab 间距")) {
    bad.push(`④ 正常路径不许带兜底提示（提示：${warnText(good).slice(0, 160)}）`);
  }

  console.log(`  ① size=400 短路：ok=${one && one.ok}｜painted=${one && one.painted_pixels}｜提示：${warnText(one).slice(0, 110)}…`);
  console.log(`  ② size=300 短路：ok=${two && two.ok}｜painted=${two && two.painted_pixels}｜提示：${warnText(two).slice(0, 110)}…`);
  console.log(`  ③ 不给 size 短路：ok=${none && none.ok}｜painted=${none && none.painted_pixels}｜提示：${warnText(none).slice(0, 110)}…`);
  console.log(`  ④ 正常笔画：ok=${good && good.ok}`);
  if (bad.length) {
    console.error(`❌ ${bad.length} 项没成立：${bad.join("｜")}`);
    process.exit(1);
  }
  console.log("✓ 短路径兜底落墨且提示落在真实根因上（报 size／点数／路径长／间距），正常路径未受影响");
};

await main();
