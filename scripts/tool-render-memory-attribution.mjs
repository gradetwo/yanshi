#!/usr/bin/env node
// **★ 整幅渲染内存的**归因扫描**（DEF-03 的下一步）★**（第 254 轮）
//
// **为什么要有它**：已知账目里有一处**对不上**的地方 ——
//   * 单层 4K 空文档整幅渲染：**+52 MB**；
//   * 4 层空层整幅渲染：**+960 MB**；
//   => 若真是"每层一个全幅缓冲"的**线性**叠加，4 层应当是 ~208 MB，**实测是 960** ✗。
//   => 所以"每层一个缓冲"这句话**不足以解释**它；要么是**非线性**（缓冲被持有到很晚 +
//      分配器的驻留），要么峰值出现在**别处**（图层自己的累积/缓存）。
//   **修法取决于这条曲线长什么样** => 本脚本就是去量它：扫 1/2/3/4 层，报**每层增量**。
//
// **口径**：只读 `/health` 的 `rss_bytes`（进程 RSS），每步取**稳定值**（连续两次相同才算），
// 不新增 API、不读 `/proc`、不改产品代码。
//
// 用法：node scripts/tool-render-memory-attribution.mjs <base-url> [--side 2048]
const base = (process.argv[2] || "").replace(/\/+$/, "");
if (!base) {
  console.error("用法: node scripts/tool-render-memory-attribution.mjs <base-url> [--side 2048]");
  process.exit(2);
}
const sideArg = process.argv.indexOf("--side");
// **默认 2048 而不是 4096**（第 254 轮实测）：本机 3.8 GB 下 4096 的四层扫描会 OOM
// （整条命令被 SIGKILL、连带带走服务进程）。**这条脚本在 CI 里也会被跑到**
// => 默认必须是"能安全跑完"的尺寸；要看 4K 就显式传 `--side 4096`，且**只做单次渲染前后**。
const SIDE = sideArg > 0 ? Number(process.argv[sideArg + 1]) : 2048;

const rss = async () => {
  for (let i = 0; i < 40; i += 1) {
    const first = await (await fetch(`${base}/health`)).json();
    await new Promise((r) => setTimeout(r, 120));
    const second = await (await fetch(`${base}/health`)).json();
    if (first.rss_bytes === second.rss_bytes && first.rss_bytes) return Number(first.rss_bytes);
  }
  return null;
};
const call = async (doc, token, tool, args) =>
  (
    await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(args || {}),
    })
  ).json();

const mb = (bytes) => (bytes === null ? "?" : (bytes / 1048576).toFixed(0));

const main = async () => {
  const rows = [];
  for (const layers of [1, 2, 3, 4]) {
    const doc = `attr_${layers}_${Date.now().toString(36)}`;
    const created = await (
      await fetch(`${base}/api/documents`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ doc_id: doc, width: SIDE, height: SIDE }),
      })
    ).json();
    if (!created || !created.token) {
      console.error("❌ 判不了：建不了文档 ⇒ 退出码 3");
      process.exit(3);
    }
    const token = created.token;
    for (let i = 0; i < layers; i += 1) {
      await call(doc, token, "create_layer", { layer_id: `L${i}`, name: `L${i}` });
    }
    const before = await rss();
    const painted = await call(doc, token, "render_region", {
      region: { x: 0, y: 0, w: SIDE, h: SIDE },
      raw: true,
    });
    if (!painted || painted.ok !== true) {
      console.error(`❌ 判不了：${layers} 层渲染失败（${JSON.stringify(painted).slice(0, 140)}）⇒ 退出码 3`);
      process.exit(3);
    }
    const after = await rss();
    rows.push({ layers, before, after, delta: after !== null && before !== null ? after - before : null });
    console.log(
      `  ${layers} 层：渲染前 ${mb(before)} MB ⇒ 渲染后 ${mb(after)} MB｜增量 ${mb(rows[rows.length - 1].delta)} MB｜每层均摊 ${rows[rows.length - 1].delta === null ? "?" : (rows[rows.length - 1].delta / 1048576 / layers).toFixed(0)} MB`,
    );
  }
  // 判断线性还是超线性：以 1 层的增量为单位，看 4 层是不是 ~4 倍
  const one = rows.find((r) => r.layers === 1)?.delta ?? null;
  const four = rows.find((r) => r.layers === 4)?.delta ?? null;
  if (one === null || four === null) {
    console.error("❌ 判不了：没量到 1 层或 4 层的增量 ⇒ 退出码 3");
    process.exit(3);
  }
  const ratio = four / Math.max(1, one);
  console.log(
    `  ⇒ 4 层 / 1 层 = ${ratio.toFixed(2)}×（线性应为 ~4×；明显更大 => 有"同时持有"或分配器驻留的放大）`,
  );
  // **这条脚本的产出是**归因**，不是门槛**：打印结论，永不因为"数字不好看"而红。
  console.log(
    ratio > 5.0
      ? "  ⇒ 结论：**超线性**（>5×）=> 修法要盯「同时在用／持有到很晚」的那一批缓冲，而不是单层大小"
      : "  ⇒ 结论：接近**线性**（≤5×）=> 修法可以从「把每层的单块变小／复用」入手",
  );
};

await main();
