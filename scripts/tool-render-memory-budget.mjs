#!/usr/bin/env node
// **★ 4K 整幅渲染的内存预算（第 ④ 项 ✓）★**（**第 254 轮 ✓）
//
// **∴ 为什么盯这条账目 ✗**（**实测归因 ✓ 见下 ✓）**：
//   ＋ **∴ 本机实测（**4K ＋ 4 层真实素材，`web/samples/*.png` ✓）**：
//     **∴ 空载 4 MB｜4K 空文档 5 MB（**懒分配 ✓）｜导入 4 层小素材后 151 MB
//     **∴ 整幅 4K 渲染之后 **1052 MB**✗ —— **∴ 而**像素只有 67 MB** ⇒ **~13×** ✓ ★★
//   ＋ **∴ 出处（**读代码 ✓）✗**：**渲染的工作缓冲是 **f32**（`buffer.rs`：`vec![0.0; w*h*4]` ✓）
//     ⇒ **∴ 4096² 一个缓冲就是 **268 MB**✗ ⇒ **∴ 三四个就是那 ~900 MB** ✓ ★★
//   ＋ **∴ 为什么这条最值钱 ✗**：**8K 是 4K 的 4 倍像素 ⇒ 光这条路就要 ~3.6 GB**✗
//     ⇒ **∴ 这正是记录在案、从未修的 DEF-03（8K OOM ×3）** ✓ ★★★
//
// **∴ 判什么 ✗**：**4K ＋ 4 层真实素材，整幅渲染**那一步的 **RSS 增量**必须 ≤ 预算** ✓
//   ＋ **∴ 用**增量**而不是绝对值 ✗**：**∴ 基线（空文档＋导入 ✓）本来就有几十 MB ✓
//     ＋ **∴ 增量才是"这一步自己的账" ✓**（**∴ 与 CPU 那边同一个口径 ✓）★★
//   ＋ **∴ 数据来源 ✗**：**`GET /health` 的 `rss_bytes`** ✓（**实测它在 ✓ —— 所以这条
//     **不必**新增 API ✓，也不必读 `/proc` ✓ 能当 `tool-*` 判据跑 ✓）★★
//
// **∴ 变异（**能红的根据 ✓）✗**：**把工作缓冲改回**每帧新分配**（或者预算调小 ✓）⇒ 立刻红** ✓
//
// 用法：node scripts/tool-render-memory-budget.mjs <base-url> [预算 MB]
const base = (process.argv[2] || "").replace(/\/+$/, "");
if (!base) {
  console.error("用法: node scripts/tool-render-memory-budget.mjs <base-url> [预算 MB]");
  process.exit(2);
}
// **∴ 预算怎么定的 ✗**：**像素 67 MB ✓ ⇒ 工作缓冲合理上限按**4 倍像素**算 ≈ 268 MB** ✓
//   ＋ **∴ 再加基线几十 MB ＋ 编码器一份 ⇒ 取 **800 MB** 留足余量** ✓
//     ＋ **∴ 现状 1561 MB 峰值 ⇒ **红得很远**✓（**∴ 优化后再收紧这个数 ✓）★★
const BUDGET_MB = Number(process.argv[3] || "800");
const SAMPLES = ["sample-oil", "sample-watercolor", "sample-lake", "sample-yanshi"];

const call = async (doc, token, tool, args) =>
  (
    await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(args || {}),
    })
  ).json();

const rssMb = async () => {
  const health = await (await fetch(`${base}/health`)).json();
  return typeof health.rss_bytes === "number" ? health.rss_bytes / 1048576 : null;
};

const main = async () => {
  const doc = `memb_${Date.now().toString(36)}`;
  const created = await (
    await fetch(`${base}/api/documents`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ doc_id: doc, width: 4096, height: 4096 }),
    })
  ).json();
  if (!created || !created.token) {
    console.error("❌ 判不了：建不了 4K 文档 ⇒ 退出码 3");
    process.exit(3);
  }
  const token = created.token;
  const { readFileSync } = await import("node:fs");
  let i = 0;
  for (const name of SAMPLES) {
    i += 1;
    let png;
    try {
      png = readFileSync(`web/samples/${name}.png`);
    } catch (error) {
      console.error(`❌ 判不了：夹具 web/samples/${name}.png 读不到（${error.message}）⇒ 退出码 3`);
      process.exit(3);
    }
    const layer = `R${i}`;
    await call(doc, token, "create_layer", { layer_id: layer, name: layer });
    // **∴ 必须用上传响应里的 `{blob_hash,size,mime_type}` ✗**（**服务端已把 PNG 解码成
    // 原始像素 ✓ ⇒ **上传 PNG 的字节数会被判成"PNG 解码失败" ✓）—— 第 189 轮的坑 ✓ ★★
    const up = await (
      await fetch(`${base}/api/blob?doc=${doc}&token=${token}`, {
        method: "POST",
        headers: { "content-type": "image/png" },
        body: png,
      })
    ).json();
    if (!up || !up.blob_hash) {
      console.error(`❌ 判不了：上传 ${name} 失败（${JSON.stringify(up).slice(0, 140)}）⇒ 退出码 3`);
      process.exit(3);
    }
    const w = up.decoded_from ? up.decoded_from.width : 900;
    const h = up.decoded_from ? up.decoded_from.height : 640;
    const imported = await call(doc, token, "import_image", {
      layer_id: layer,
      bitmap: { blob_hash: up.blob_hash, size: up.size, mime_type: up.mime_type },
      region: { x: ((i - 1) % 2) * 2000, y: Math.floor((i - 1) / 2) * 2000, w, h },
    });
    if (!imported || imported.ok !== true) {
      console.error(`❌ 判不了：import_image 失败（${JSON.stringify(imported).slice(0, 140)}）⇒ 退出码 3`);
      process.exit(3);
    }
  }
  const before = await rssMb();
  if (before === null) {
    console.error("❌ 判不了：/health 没有 rss_bytes ⇒ 这条判据量不到账 ⇒ 退出码 3");
    process.exit(3);
  }
  await call(doc, token, "render_region", { region: { x: 0, y: 0, w: 4096, h: 4096 } });
  const after = await rssMb();
  const delta = after - before;
  console.log(
    `  4K ＋ ${SAMPLES.length} 层真实素材：渲染前 ${before.toFixed(0)} MB ⇒ 渲染后 ${after.toFixed(0)} MB` +
      `｜增量 ${delta.toFixed(0)} MB（预算 ${BUDGET_MB} MB）｜像素本身只有 ${(4096 * 4096 * 4 / 1048576).toFixed(0)} MB/层`,
  );
  if (delta > BUDGET_MB) {
    console.error(
      `❌ 整幅 4K 渲染这一步吃掉了 ${delta.toFixed(0)} MB（预算 ${BUDGET_MB} MB）` +
        `⇒ 工作缓冲太大（f32 全幅 = 268 MB/个）⇒ 8K 必 OOM（DEF-03）`,
    );
    process.exit(1);
  }
  console.log(`✓ 整幅 4K 渲染的内存增量 ${delta.toFixed(0)} MB ≤ 预算 ${BUDGET_MB} MB`);
};

await main();
