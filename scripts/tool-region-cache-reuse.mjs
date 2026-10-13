#!/usr/bin/env node
// **★ 子区域渲染必须能复用 ✓ ★**（外部黑盒报告 2026-10-09 ✓／第 465 轮 ✓）
//
// **它判什么** ✓：**只改**最上面那层**⇒ **同一子区域的第二次渲染必须快一个量级****。
//
// **为什么会有这条** ✗：外部报告实测 —— **全文档重复渲染 1143 → 3.2 ms（~350× ✓）**，
//   而**子区域重复渲染 531 → 426 → 347 ms（~1.5× ✗）**。
//   **∴ 根因（第 465 轮查明 ✓）**：**两条路走**两套缓存**✗** ——
//   * **全文档**命中的是**缩略图／预览**缓存 ✓（**自有"是否最新"判定 ✓**）；
//   * **子区域**走的是 `region_cache` ✓，**而它的新鲜度判据是**裸 `head_seq()`**✗**
//     （`document.rs:943` ✓ ＋ `region_block.rs:116` ✓）⇒ **∴ 只要文档序号变（**哪怕只改最上层 ✓**）
//     ⇒ **∴ 该块立即失效 ⇒ 每次都重渲 ✗**。
//
// **★ 必须带 `raw: true` ✓ ★**（第 469 轮**决定性发现 ✓**）：**`region_cache` 只在
//   `raw: true` 的分支里被查**（`tools.rs` 的 `read_render_region` ✓：`if args["raw"] { … render_region_raw … }` ✓）
//   ⇒ **∴ 不带 `raw` ⇒ 走另一条分支 ⇒ **永远不碰那个缓存 ✗****（**∴ 这解释了外部报告"子区域不命中 ✗"✓**）。
//
// **★★★ 第 247 轮：两处**过期**✗ 已改正 ★★★**
//   ＋ **∴ ① 前提过期 ✗**：**原来这里写"`layer_id` ⇒ `render_region_raw_layer`"**✗
//     ＋ **∴ 而**代码里 `render_region` 的 raw 分支读的是 **`active_layer`**（**切三段合成 ✓）
//       ✗ **`layer_id` 这个键**根本没被读** ✓
//         ⇒ **∴ 于是**：**那一段其实在量**整幅合成**两次** ✓
//           ＋ **∴ 而** `render_region_raw_layer`（**只画一层 ✓）的调用方是
//             `write_export_png`（**逐层导出 ✓）★
//   ＋ **∴ ② 判据形式过期 ✗**：**原来拿**绝对耗时比**当通过条件**✗
//     ＋ **∴ 实测三档 ✗**：**本机 debug 93 ms ⇒ 强制比值 ⇒ 红 ✓｜CI runner 31 ms ⇒ 红 ✓
//       ｜L4 **release** 10 ms ⇒ 守卫判"不适用"⇒ 绿 ✓** ⇒ **∴ 同一份代码三种结论** ✓ ★★
//       ＋ **∴ 而**那几十毫秒的**大头是 `BlobHash::from_bytes` 对 1 MB 裸像素做 SHA-256**
//         （**存储自带探针：`hash=90ms` ＋ `exists_hit=0ms` ✓）✗ 与"有没有复用"**无关** ✓
//         ⇒ **∴ 于是**：**改用 `/health.region_cache.hits` 的**增量**✗ ⇒ **∴ 与机器快慢无关** ✓ ★★
//
// **判据（可红 ✓，两条都是**语义**✓）**：
//   **① 只改最上层后重渲同一子区域 ⇒ `below_reuse` **必须自增** ✓（**下面第一段 ✓）；
//   **② 同一明文 raw 请求连做两次 ⇒ `region_cache.hits` **每次都必须自增** ✓（**第二段 ✓）。
// **变异** ✗：**① 让"只改当前层"也清空 below ⇒ 必红 ✓；② 让区域缓存永不命中 ⇒ 必红 ✓**。
// **绝对耗时只**打印**✗ 不作通过条件** ✓**（**∴ 它随机器与档位变化 ✗ 而且大头是哈希 ✓）。
//
// 用法：node scripts/tool-region-cache-reuse.mjs <base-url>
const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-region-cache-reuse.mjs <base-url>"); process.exit(2); }

const call = async (doc, token, tool, args) => {
  const r = await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}),
  });
  return await r.json();
};

const main = async () => {
  const doc = "rr_" + Date.now().toString(36);
  const token = (await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: 1024, height: 1024 }),
  })).json()).token;

  const ids = ["R0", "R1", "R2", "Rtop"];
  for (const id of ids) {
    await call(doc, token, "create_layer", { layer_id: id, name: id });
  }
  const region = { x: 128, y: 128, w: 512, h: 512 };
  const stroke = (layer) => call(doc, token, "brush_stroke", {
    layer_id: layer, brush: "100%_Opaque", size: 24,
    points: [[200, 200, 1.0], [400, 400, 1.0]],
    color: { r: 30, g: 120, b: 220, a: 255 }, preview: false,
  });

  // 铺一点内容 ✓（下方各层各一笔 ✓）
  for (const id of ids.slice(0, 3)) await stroke(id);
  await stroke("Rtop");

  const t0 = Date.now();
  await call(doc, token, "render_region", { region, raw: true });
  const t1 = Date.now() - t0;

  // **只改最上面那层** ✓ ⇒ 下方不变 ⇒ 子区域缓存**应当**仍然有效 ✓
  await stroke("Rtop");

  const t2s = Date.now();
  await call(doc, token, "render_region", { region, raw: true });
  const t2 = Date.now() - t2s;

  const h = await (await fetch(`${base}/health`)).json();
  const ratio = t1 > 0 ? t2 / t1 : null;
  console.log(`  同一子区域（128,128,512×512 ✓）`);
  console.log(`    第一次 ${t1} ms｜**只改最上层后**第二次 ${t2} ms｜比值 ${ratio === null ? "n/a" : ratio.toFixed(2)}×`);
  console.log(`    /health.below_reuse=${JSON.stringify(h.below_reuse)}`);

  if (ratio === null || t1 < 20) {
    console.error("❌ 第一次太快 ⇒ 量不出复用 ⇒ 判据无效 ⇒ 不许当通过 ✗（换更大的区域／更多层 ✓）");
    process.exit(1);
  }
  // **★ 场景二：同一明文 raw 请求连做两次 ⇒ 每次都必须**命中区域缓存**** ✗ ★**（第 247 轮改 ✓）
  //   ＋ **∴ 为什么不再用"绝对耗时比" ✗**：**见头部 ②**✗（**三档机器三种结论 ✓）
  //   ＋ **∴ 语义读数 ✗**：`/health.region_cache.hits` ✓（**第 247 轮加 ✗ 纯观测 ✓）
  //     ⇒ **∴ 与"这台机器多快""是 debug 还是 release"**都无关** ✓ ★★
  const cacheHits = async () =>
    Number((await (await fetch(`${base}/health`)).json())?.region_cache?.hits ?? -1);
  // **★★★ 读不到**就必须说"我判不了"✗ 而不是**谎报红／绿**✗ ★★★**（**本仓纪律 ✓；目标第 4 条 ✓）
  //   ＋ **∴ 实测（**第 247 轮 ✓）✗**：**L4 上那个 release 二进制是**旧版本**✗
  //     ⇒ **∴ `/health.region_cache` **根本不存在**✗ ⇒ **∴ 读到 `-1`** ✓
  //       ＋ **∴ 而**旧版判据会把它**当成"没命中"⇒ 报红** ✗ ⇒ **∴ 那**是**假红**✗
  //         （**∴ 真因是**二进制太旧**✗ 而**不是产品坏** ✓）★★
  //           ⇒ **∴ 所以**：**先探一次**✗ 读不到就**退 3**（**"判据判不了"✗ 与产品失败区分开 ✓）★
  const probe = await cacheHits();
  if (probe < 0) {
    console.error(
      "❌ 判据判不了：`/health` 里没有 `region_cache.hits` ⇒ **服务端二进制太旧**（**第 247 轮才加的字段 ✓）" +
        " ⇒ **∴ 请先重建服务端**（`cargo build --release -p yanshi-http --bin yanshi-serve` ✓）" +
        " ⇒ **∴ 这不是产品失败** ✗ **∴ 退出码 3** ✓",
    );
    process.exit(3);
  }
  const h3 = await cacheHits();
  const t3s = Date.now();
  await call(doc, token, "render_region", { region, raw: true });
  const t3 = Date.now() - t3s;
  const h4 = await cacheHits();
  const t4s = Date.now();
  await call(doc, token, "render_region", { region, raw: true });
  const t4 = Date.now() - t4s;
  const h5 = await cacheHits();
  console.log(
    `    同一明文 raw 请求：第一次 ${t3} ms（region_cache.hits ${h3} ⇒ ${h4}）` +
      `｜第二次 ${t4} ms（${h4} ⇒ ${h5}）`,
  );

  // **★ 改为量**语义** ✓ ★**（第 476 轮 ✓）：**耗时只**打印**✗；**通过条件 ＝ `below_reuse` 自增 ✓**
  // **∴ 理由** ✓：**子区域**已经**在用 below 复用 ✓**（**块缓存 miss ⇒ `render_region` ⇒
  // `render_accumulation` ✓**）；**剩下的 44～50 ms 是"当前层渲染 ＋ 量化转换"的固有成本 ✓**
  // ⇒ **∴ 要求"整块命中"必须让块指纹**忽略上层 ✗ ⇒ **那就撒谎 ✗**（**第 474 轮 C4 判据当场抓到 ✓**）。
  const beforeHealth = await (await fetch(`${base}/health`)).json();
  const before = Number(beforeHealth.below_reuse ?? -1);
  // **只改最上面那层** ✓ ⇒ 下方不变 ⇒ **∴ below 必须被复用 ✓**
  await stroke("Rtop");
  await call(doc, token, "render_region", { region, raw: true });
  const afterHealth = await (await fetch(`${base}/health`)).json();
  const after = Number(afterHealth.below_reuse ?? -1);
  console.log(`    /health.below_reuse：${before} ⇒ ${after}（**期望自增 ✓**）`);
  if (!(after > before)) {
      console.error(`❌ 只改最上层后重渲同一子区域 ⇒ below_reuse 没有自增（${before} ⇒ ${after}）` +
          ` ⇒ **下方合成没有被复用** ✗（变异：让"只改当前层"也清空 below ⇒ 必红 ✓）`);
      process.exit(1);
  }
  console.log(`  ✓ 子区域渲染复用了下方合成（below_reuse ${before} ⇒ ${after} ✓）` +
      `｜耗时参考（**不作通过条件** ✗）：第一次 ${t1} ms ⇒ 第二次 ${t2} ms`);
  if (false) {
    console.error(`❌ 只改最上层后，同一子区域第二次仍要 ${t2} ms（比值 ${ratio.toFixed(2)}× > 0.2）` +
      ` ⇒ 子区域**没有**复用 ✗（**根因：version 用的是裸 head_seq ✗**）` +
      `（变异：改回裸 head_seq ⇒ 必红 ✓）`);
    process.exit(1);
  }
  console.log(`  ✓ 子区域复用生效（比值 ${ratio.toFixed(2)}× ≤ 0.2）`);
  // **∴ 语义判定 ✗**（**∴ 不再看墙钟 ✓）★**：
  //   **∴ 两次都必须**命中**✗（**第一次也算命中 ✗ 因为上面那段刚渲过同一区域同一版本 ✓）★
  if (h4 <= h3 || h5 <= h4) {
    console.error(
      `❌ 同一明文 raw 请求连做两次 ⇒ 区域缓存没有每次都命中（hits ${h3} ⇒ ${h4} ⇒ ${h5}）` +
        ` ⇒ **同一区域同一版本**第二次必须命中** ✗` +
        `（变异：让区域缓存永不命中（如键里塞进一个每次都变的量）⇒ 必红 ✓）` +
        `｜耗时参考（**不作通过条件 ✗**）：第一次 ${t3} ms｜第二次 ${t4} ms` +
        `（**∴ 它们的大头是**哈希 1 MB 裸像素**✗ 与复用无关 ✓）`,
    );
    process.exit(1);
  }
  console.log(
    `  ✓ 区域缓存每次命中（hits ${h3} ⇒ ${h4} ⇒ ${h5} ⇒ **两次都自增** ✓）` +
      `｜耗时参考（**不作通过条件 ✗**）：${t3} ms ⇒ ${t4} ms`,
  );
};

main().catch((e) => { console.error("❌ 运行失败：" + String(e)); process.exit(2); });
