#!/usr/bin/env node
// **★ PSD 图层级导入／导出的**两本账**（墙钟 ＋ CPU）✗ ★**（**第 253 轮 ✓）
//
// **∴ 为什么要有它 ✗**：**目标明文要求"**同机两本账（**墙钟 ＋ CPU ✗ 多次取中位 ＋ 极差 ✓）**" ✓
//   ＋ **∴ 而没有实测的数字 ⇒ **不许写成结论** ✓（**本仓库的纪律 ✓）★★
//
// **∴ 怎么测（**每一条都为了"量的是它自己" ✓）✗**：
//   ＋ **∴ ① 两个方向用**同一量级**✗**：**先把 1024² 场景**导出一次**✗
//     **拿那份字节当导入的输入** ✓ ⇒ **∴ 导入与导出面对的是**同规模的东西** ✓ ★
//   ＋ **∴ ② 每一轮都**新建文档**✗**：**∴ 因为**导入会**累加图层**✗
//     ⇒ **∴ 若在同一文档里连测 ⇒ **后几轮越来越慢**✗ 量到的是"文档变大"不是"导入" ✓ ★★
//   ＋ **∴ ③ 建文档**不计入**✗（**∴ 它**两件事都不做 ✓）** ⇒ **∴ 只在**被量的调用**前后取时间 ✓ ★
//   ＋ **∴ ④ CPU 取**服务进程**的 `utime+stime`**（**`/proc/<pid>/stat` ✓ 与
//     `tool-render-cost-accounts.mjs` 同一口径 ✓）⇒ **∴ 两本账**同源可比** ✓ ★★
//
// **∴ 如实写明的边界（**未覆盖 ✓）✗**：
//   ＋ **∴ 这是**单机单次会话**的测量 ✗ 不是"所有机器都这样" ✓ ★
//   ＋ **∴ 文档是**工具造出来的**（**不是真用户工程 ✓）✗ 且图层数偏少（5 层 ✓）★
//   ＋ **∴ 导入的 PSD 是**我们自己导出的**（**同规模 ✓ 但**不是 Photoshop 产的 ✓）★
//
// 用法：node scripts/tool-psd-cost-accounts.mjs <base-url> --pid <服务进程 pid> [--rounds 7]
const base = process.argv[2];
if (!base) {
  console.error("用法: node scripts/tool-psd-cost-accounts.mjs <base-url> --pid <pid> [--rounds 7]");
  process.exit(2);
}
const argv = process.argv.slice(3);
const option = (name, fallback) => {
  const at = argv.indexOf(name);
  return at >= 0 && argv[at + 1] ? argv[at + 1] : fallback;
};
const pid = option("--pid", null);
const rounds = Number(option("--rounds", "7"));
if (!pid) {
  console.error("❌ 判不了：需要 --pid <服务进程 pid>（CPU 那本账要读它的 utime+stime）⇒ 退出码 3");
  process.exit(3);
}
const fs = await import("node:fs");
const os = await import("node:os");
const path = await import("node:path");
const { trackTemp } = await import("./lib/temp-dirs.mjs");

const W = 1024;
const H = 1024;
const call = async (doc, token, tool, args) =>
  (
    await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(args || {}),
    })
  ).json();
const newDoc = async (suffix) => {
  const doc = `psdacct_${suffix}_${Date.now().toString(36)}${Math.floor(Math.random() * 1e4)}`;
  const created = await (
    await fetch(`${base}/api/documents`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ doc_id: doc, width: W, height: H }),
    })
  ).json();
  return created && created.token ? { doc, token: created.token } : null;
};
const upload = async (doc, token, bytes) => {
  const reply = await (
    await fetch(`${base}/api/blob?doc=${doc}&token=${token}`, {
      method: "POST",
      headers: { "content-type": "application/octet-stream" },
      body: bytes,
    })
  ).json();
  return reply && (reply.blob_hash || reply.hash);
};
const blobUrl = (u0) => {
  const s = String(u0 || "");
  if (s.startsWith("yanshi://blob/")) return `${base}/api/blob/${s.slice("yanshi://blob/".length)}`;
  return s.startsWith("http") ? s : `${base}${s}`;
};
/** **∴ CPU 那本账 ✗**：**与 `tool-render-cost-accounts.mjs` 同一口径 ✓** ★ */
const readCpuTicks = () => {
  let stat;
  try {
    stat = fs.readFileSync(`/proc/${pid}/stat`, "utf8");
  } catch (error) {
    console.error(`❌ 判不了：读不到 /proc/${pid}/stat（${error && error.code}）⇒ 服务进程不在了 ⇒ 退出码 3`);
    process.exit(3);
  }
  const tail = stat.slice(stat.lastIndexOf(")") + 2).split(" ");
  return Number(tail[11]) + Number(tail[12]);
};
const CLK_TCK = 100; // **∴ Linux 上恒为 100 ✓（`getconf CLK_TCK` ✓）★

/** **∴ 造一个 1024² 的代表性场景 ✗**：**3 个顶层 ＋ 1 个组（**内含 2 层 ✓）★ */
const buildScenario = async (target) => {
  const layers = [
    ["L1", null, 1, { r: 220, g: 40, b: 40, a: 255 }],
    ["L2", null, 2, { r: 40, g: 200, b: 60, a: 255 }],
    ["L3", null, 3, { r: 40, g: 60, b: 220, a: 255 }],
  ];
  for (const [id, parent, z, color] of layers) {
    await call(target.doc, target.token, "create_layer", { layer_id: id, name: id, z_index: z, parent_id: parent });
    await call(target.doc, target.token, "brush_stroke", {
      layer_id: id,
      object_id: `${id}_stroke`,
      brush: "100%_Opaque",
      size: 48,
      points: [[120, 120 + z * 120, 1.0], [900, 120 + z * 120, 1.0]],
      color,
      preview: false,
    });
  }
  await call(target.doc, target.token, "create_layer", { layer_id: "G1", name: "G1", type: "layer_group", z_index: 4 });
  for (const [id, z, color] of [["L4", 5, { r: 220, g: 200, b: 40, a: 255 }], ["L5", 6, { r: 200, g: 40, b: 200, a: 255 }]]) {
    await call(target.doc, target.token, "create_layer", { layer_id: id, name: id, parent_id: "G1", z_index: z });
    await call(target.doc, target.token, "brush_stroke", {
      layer_id: id,
      object_id: `${id}_stroke`,
      brush: "100%_Opaque",
      size: 48,
      points: [[200, 200 + z * 100, 1.0], [820, 200 + z * 100, 1.0]],
      color,
      preview: false,
    });
  }
  await call(target.doc, target.token, "update_layer", { layer_id: "G1", patch: { opacity: 0.5 } });
};

const stats = (xs) => {
  const sorted = [...xs].sort((a, b) => a - b);
  const mid = sorted.length >> 1;
  return {
    median: sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2,
    min: sorted[0],
    max: sorted[sorted.length - 1],
  };
};
const fmt = (s) => `${s.median.toFixed(1)}（${s.min.toFixed(1)}–${s.max.toFixed(1)}）`;

const main = async () => {
  // **∴ ① 造场景 ✗ 导出一份当导入输入 ✗ 并如实报告它多大** ✓
  const source = await newDoc("src");
  if (!source) {
    console.error("❌ 判不了：建不了文档 ⇒ 退出码 3");
    process.exit(3);
  }
  await buildScenario(source);
  const seed = await call(source.doc, source.token, "export_psd_layers", {});
  if (!seed || seed.ok !== true) {
    console.error(`❌ 判不了：种子导出失败（${JSON.stringify(seed).slice(0, 200)}）⇒ 退出码 3`);
    process.exit(3);
  }
  const seedBytes = new Uint8Array(await (await fetch(blobUrl(seed.url))).arrayBuffer());
  const seedFile = trackTemp(path.join(fs.mkdtempSync(path.join(os.tmpdir(), "yanshi-psd-accounts-")), "seed.psd"));
  fs.writeFileSync(seedFile, seedBytes);

  const wallImport = [];
  const cpuImport = [];
  const wallExport = [];
  const cpuExport = [];
  for (let i = 0; i < rounds; i++) {
    const target = await newDoc(`r${i}`);
    if (!target) {
      console.error(`❌ 判不了：第 ${i + 1} 轮建不了文档 ⇒ 退出码 3`);
      process.exit(3);
    }
    const hash = await upload(target.doc, target.token, seedBytes);
    if (!hash) {
      console.error(`❌ 判不了：第 ${i + 1} 轮上传种子失败 ⇒ 退出码 3`);
      process.exit(3);
    }
    // **∴ 导入 ✗** ✓
    let before = readCpuTicks();
    let t0 = performance.now();
    const imported = await call(target.doc, target.token, "import_psd_layers", { blob_hash: hash });
    const importWall = performance.now() - t0;
    const importCpu = (readCpuTicks() - before) * (1000 / CLK_TCK);
    if (!imported || imported.ok !== true) {
      console.error(`❌ 判不了：第 ${i + 1} 轮导入失败（${JSON.stringify(imported).slice(0, 200)}）⇒ 退出码 3`);
      process.exit(3);
    }
    wallImport.push(importWall);
    cpuImport.push(importCpu);
    // **∴ 导出 ✗** ✓
    before = readCpuTicks();
    t0 = performance.now();
    const exported = await call(target.doc, target.token, "export_psd_layers", {});
    const exportWall = performance.now() - t0;
    const exportCpu = (readCpuTicks() - before) * (1000 / CLK_TCK);
    if (!exported || exported.ok !== true) {
      console.error(`❌ 判不了：第 ${i + 1} 轮导出失败（${JSON.stringify(exported).slice(0, 200)}）⇒ 退出码 3`);
      process.exit(3);
    }
    wallExport.push(exportWall);
    cpuExport.push(exportCpu);
    console.log(
      `  第 ${i + 1}/${rounds} 轮：导入 墙钟 ${importWall.toFixed(1)} ms／CPU ${importCpu.toFixed(1)} ms ｜ 导出 墙钟 ${exportWall.toFixed(1)} ms／CPU ${exportCpu.toFixed(1)} ms`,
    );
  }
  console.log("");
  console.log(`  场景：${W}×${H}，6 个图层（3 顶层 ＋ 1 组（内含 2 层）），组不透明度 0.5`);
  console.log(`  种子 PSD：${seedBytes.length} 字节（**我们自己导出的** ⇒ 与导入同规模；不是 Photoshop 产的，如实记）`);
  console.log(`  导入　墙钟 ms（中位（极差））：${fmt(stats(wallImport))}｜CPU ms：${fmt(stats(cpuImport))}`);
  console.log(`  导出　墙钟 ms（中位（极差））：${fmt(stats(wallExport))}｜CPU ms：${fmt(stats(cpuExport))}`);
  console.log(`  轮数 ${rounds}；每轮都新建文档（避免导入累加图层把后几轮拖慢）`);
  console.log(
    "  未覆盖：单机单会话；文档是工具造的（非真工程）；种子 PSD 是自产的（不是 Photoshop 文件）",
  );
};

await main();
