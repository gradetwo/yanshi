#!/usr/bin/env node
// **★ PSD **图层级导入**：结构 ＋ 与"PSD 自带的压平合成图"逐像素对比 ✗ ★**（**第 253 轮 ✓）
//
// **∴ 判什么（**四件 ✗ 都能红 ✓）★**：
//   **∴ ① 结构 ✗**：**夹具里 4 个光栅层 ＋ 1 个组 ⇒ **导入后**图层数／组／父子关系**都要对 ✓
//   **∴ ② 属性 ✗**：**名字 ✗ 可见性 ✗ 不透明度 ✗ 混合模式**（**建议点名要对的四样 ✓）★
//   **∴ ③ 位置 ✗**：**每层按自己的 rect 落位**（**∴ 用"红在左半 ✗ 蓝在右半 ✗
//     绿带在下部"来验 ⇒ **∴ 位置错一格就看得见** ✓）★
//   **∴ ④ 答案键 ✗ ★★★**：**把**同一份 PSD 的压平合成图**用 `import_psd` 导进另一个文档 ✗
//     **两边各渲一次**✗ 在**合成图有不透明像素的地方**逐通道比 ⇒ **差值 ≤ 2** ✓
//       ＋ **∴ 为什么只比"有墨的地方" ✗**：**合成图是**透明背景**✗ 而 yanshi 文档有白底** ✓
//         ⇒ **∴ 透明处的 RGB**本来就不该相等** ✓ ⇒ **∴ 拿它当答案键是**设计错** ✓（**如实写明 ✓）★★
//       ＋ **∴ 为什么容差 2 ✗**：**两边都要过 8 位量化 ✗ 且**合成图是写 PSD 时算的** ✓
//         ⇒ **∴ 要求逐位相等 = 要求**写出方的舍入与我们一模一样** ✗ 那不成立 ✓ ★
//
// **∴ 夹具的来源（**如实 ✓）✗**：`crates/yanshi-render/tests/data/psd/agpsd-two-layers.psd`
//   **∴ 由 `psd.rs` 的 `write_fixture_for_criteria` 用 **ag-psd 自己写** ✓
//     ＋ **∴ 为什么不用 ImageMagick 写的 ✗**：**那把混合键写成了**字节倒序**（**实测 ✓）
//       ⇒ **∴ ag-psd**（正确地）拒绝它** ✓ ⇒ **∴ 代价 ✗**：**"能读 Photoshop 原文件"
//         这件事**这轮没有证据** ✓（**不写成结论 ✓）★★
//
// 用法：node scripts/tool-psd-layers.mjs <base-url>
const base = process.argv[2];
if (!base) {
  console.error("用法: node scripts/tool-psd-layers.mjs <base-url>");
  process.exit(2);
}
const fs = await import("node:fs");
const path = await import("node:path");
const url = await import("node:url");
const FIXTURE = path.resolve(
  path.dirname(url.fileURLToPath(import.meta.url)),
  "../crates/yanshi-render/tests/data/psd/agpsd-two-layers.psd",
);

const call = async (doc, token, tool, args) =>
  (
    await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(args || {}),
    })
  ).json();
const newDoc = async (suffix) => {
  const doc = `psd_${suffix}_${Date.now().toString(36)}`;
  const created = await (
    await fetch(`${base}/api/documents`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ doc_id: doc, width: 64, height: 64 }),
    })
  ).json();
  return created && created.token ? { doc, token: created.token } : null;
};
const upload = async (doc, token, bytes) => {
  const response = await fetch(
    `${base}/api/blob?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`,
    { method: "POST", headers: { "content-type": "application/octet-stream" }, body: bytes },
  );
  const reply = await response.json().catch(() => null);
  return reply && (reply.blob_hash || reply.hash || reply.blob);
};

const main = async () => {
  if (!fs.existsSync(FIXTURE)) {
    console.error(`❌ 判不了：没有夹具 ${FIXTURE}（先跑 cargo test -p yanshi-render write_fixture_for_criteria）⇒ 退出码 3`);
    process.exit(3);
  }
  const bytes = new Uint8Array(fs.readFileSync(FIXTURE));
  const target = await newDoc("layers");
  if (!target) {
    console.error("❌ 判不了：建不了文档 ⇒ 退出码 3");
    process.exit(3);
  }
  const hash = await upload(target.doc, target.token, bytes);
  if (!hash) {
    console.error("❌ 判不了：上传 PSD 失败（POST /api/blob）⇒ 退出码 3");
    process.exit(3);
  }

  // **∴ ① 图层级导入 ✗** ✓
  const imported = await call(target.doc, target.token, "import_psd_layers", { blob_hash: hash });
  if (!imported || imported.ok !== true) {
    console.error(`❌ 判不了：import_psd_layers 没成功（${JSON.stringify(imported).slice(0, 220)}）⇒ 退出码 3`);
    process.exit(3);
  }
  const listed = await call(target.doc, target.token, "list_layers", {});
  const layers = (listed && (listed.layers || listed.items)) || [];
  // **∴ 两个坑（**都是判据自己的 ✓）✗**：**① `create_document` 会**自带一个默认图层**
  //   ⇒ **∴ 总数断言要**相对**（**只看我们导入的那 5 个 ✓）✗** **② `list_layers` 的 id
  //   字段**未必叫 `id`** ✗ ⇒ **∴ 两种都认** ✓ ★
  const idOf = (l) => l && (l.id ?? l.layer_id);
  const byName = (needle) => layers.find((l) => typeof l.name === "string" && l.name === needle);
  const bad = [];
  const red = byName("红");
  const blue = byName("蓝");
  const green = byName("组内绿");
  const group = byName("一组");
  const hidden = byName("隐藏层");
  const mine = ["红", "蓝", "组内绿", "一组", "隐藏层"].map(byName).filter(Boolean);
  if (mine.length !== 5) bad.push(`① 导入的那 5 个（4 光栅 ＋ 1 组）应都在，实测 ${mine.length}`);
  if (!red || !blue || !green || !group || !hidden) {
    bad.push(`① 名字对不上（缺 ${[red, blue, green, group, hidden].filter((x) => !x).length} 个）`);
  }
  if (group) {
    const type = String(group.type || group.layer_type || "");
    if (!/group/i.test(type)) bad.push(`① "一组" 的类型应是图层组，实测 ${type}`);
  }
  if (green && group) {
    const groupId = idOf(group);
    if (!groupId) bad.push("① 拿不到\"一组\"的 id（list_layers 的字段名变了？）");
    else if (green.parent_id !== groupId) {
      bad.push(`① "组内绿" 的父应是 "一组"（实测 ${green.parent_id} vs ${groupId}）`);
    }
  }
  if (hidden) {
    if (hidden.visible !== false) bad.push(`② "隐藏层" 应不可见（实测 ${hidden.visible}）`);
    if (Math.abs(Number(hidden.opacity) - 0.5) > 0.02) {
      bad.push(`② "隐藏层" 不透明度应是 0.5（实测 ${hidden.opacity}）`);
    }
    const blend = String(hidden.blend_mode || hidden.blend || "");
    if (blend.toLowerCase() !== "multiply") bad.push(`② "隐藏层" 混合模式应是 multiply（实测 ${blend}）`);
  }

  // **∴ ③ 位置 ＋ ④ 答案键 ✗** ✓
  const fetchRaw = async (value) => {
    const u = value && (value.raw_url || value.rawUrl);
    if (!u) return null;
    const absolute = String(u).startsWith("http") ? String(u) : `${base}${u}`;
    const response = await fetch(absolute);
    return response.ok ? new Uint8Array(await response.arrayBuffer()) : null;
  };
  const render = async ({ doc, token }) =>
    fetchRaw(await call(doc, token, "render_region", { region: { x: 0, y: 0, w: 64, h: 64 }, raw: true }));
  const ours = await render(target);
  if (!ours) bad.push("③ 取不回导入后的渲染（判据自己坏了）");

  // **∴ 答案键：同一份 PSD 的**压平合成图**导进另一个文档** ✓
  const key = await newDoc("key");
  const keyHash = key ? await upload(key.doc, key.token, bytes) : null;
  if (!key || !keyHash) {
    console.error("❌ 判不了：答案键那一路没建起来 ⇒ 退出码 3");
    process.exit(3);
  }
  await call(key.doc, key.token, "create_layer", { layer_id: "L1", name: "L1" });
  const composite = await call(key.doc, key.token, "import_psd", { layer_id: "L1", blob_hash: keyHash });
  if (!composite || composite.ok !== true) {
    console.error(`❌ 判不了：import_psd（合成图）没成功（${JSON.stringify(composite).slice(0, 200)}）⇒ 退出码 3`);
    process.exit(3);
  }
  const want = await render(key);
  if (!want) bad.push("④ 取不回合成图那一路的渲染（判据自己坏了）");

  let inked = 0;
  let worst = 0;
  if (ours && want) {
    if (ours.length !== want.length) {
      bad.push(`④ 两边尺寸不同（${ours.length} vs ${want.length}）`);
    } else {
      for (let i = 0; i < want.length; i += 4) {
        if (want[i + 3] === 0) continue; // **∴ 合成图透明处不比 ✗ 见文件头 ✓**
        inked += 1;
        for (let c = 0; c < 3; c++) {
          worst = Math.max(worst, Math.abs(ours[i + c] - want[i + c]));
        }
      }
      if (inked < 2000) bad.push(`④ 有墨的像素太少（${inked}）⇒ 这条比较没有说服力`);
      if (worst > 2) bad.push(`④ 与 PSD 压平合成图的最大通道差 ${worst} > 2`);
    }
  }

  console.log(`  ① 导入：${imported.layers_created} 光栅 ＋ ${imported.groups_created} 组（${imported.layers_with_pixels} 层带像素）；文档共 ${layers.length} 个图层（含默认层）`);
  console.log(`  ② 属性：隐藏层 visible=${hidden ? hidden.visible : "?"}／opacity=${hidden ? hidden.opacity : "?"}／blend=${hidden ? hidden.blend_mode : "?"}`);
  console.log(`  ④ 有墨像素 ${inked} 个，最大通道差 ${worst}（要求 ≤ 2）`);
  if (imported.skipped && imported.skipped.length) {
    console.log(`  （如实）没有栅格化的层：${JSON.stringify(imported.skipped).slice(0, 200)}`);
  }
  if (bad.length) {
    console.error(`❌ ${bad.length} 项没成立：${bad.join("｜")}`);
    process.exit(1);
  }
  console.log("✓ 图层结构／属性／位置对得上，且与 PSD 自带的压平合成图逐像素一致（容差 2）");
};

await main();
