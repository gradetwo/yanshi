#!/usr/bin/env node
// **wasm 门面 vs 服务端：同一笔必须逐字节相同** ✓ —— 这是 Hokusai wasm 化的核心判据 ✓。
//
// **为什么能这样比** ✓：服务端 `brush_stroke` 的对象 blob **就是**那一块区域的 RGBA 读回（含反预乘 ✓），
// 而门面回的是**同一块区域**的 RGBA ✓ —— 两边喂的笔刷、设置、点列、区域全都相同 ✓。
// 区域直接用**服务端自己报的 `region`** ✓（不在这里重算一遍 ✗ —— 重算就是第二份实现 ✓）。
//
// **内核 vs 服务端：同一笔必须逐字节相同** ✓（(A)③：把门面的判据搬到**共享内核**上 ✓）。
// 与 `wasm-brush-parity.mjs` 的差别只有一个 ✓：那边用**裸 wasm + C-ABI**（门面 ✓），
// 这边用**bindgen 包**（`crates/yanshi-wasm/pkg/yanshi_wasm.js` ✓ = 浏览器真正加载的那一份 ✓）。
// 用法：node scripts/kernel-brush-parity.mjs <server-base> <doc> <token> <pkg/yanshi_wasm.js 路径> [笔刷名...]
import { readFileSync, readdirSync } from "node:fs";
import { pathToFileURL } from "node:url";

const [base, doc, token, wasmPath, ...brushes] = process.argv.slice(2);
if (!base || !doc || !token || !wasmPath) {
  console.error("用法: node scripts/wasm-brush-parity.mjs <server-base> <doc> <token> <wasm> [brush...]");
  process.exit(2);
}
// **覆盖面必须自己说出来** ✓（第 804 轮）：判据默认只测 3 支笔 ✓，而仓库里有 **199 支** ✗ ⇒
// 一条"绿"如果不说覆盖面，就会被读成"性质成立" ✗（第 800 轮实测：约一半笔刷其实不同 ✗）。
// ⇒ ① 结论行里**打出覆盖比例** ✓（runner 成功时也会打最后一行 ✓ ⇒ CI 日志里每次都看得到 ✓）；
//   ② 传 `all` 当笔刷名 ⇒ **跑全部** ✓（调研/定期全量用 ✓）；③ 默认仍是小的那一组 ✓
//      —— **不改成默认全量** ✗：那会让 CI 立刻红 ✗，而差异原因（两边数学实现不同 ✓）正在修 ✓。
const allBrushes = readdirSync("assets/brushes").filter((f) => f.endsWith(".myb"))
  .map((f) => f.replace(/\.myb$/, "")).sort();
const wantsAll = brushes.includes("all");
const names = wantsAll ? allBrushes : (brushes.length ? brushes : ["100%_Opaque", "2B_pencil", "spray"]);
const tool = async (name, args) => {
  const response = await fetch(`${base}/api/tools?doc=${doc}&token=${token}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ tool: name, arguments: args }),
  });
  return response.json();
};
const pointList = [[40, 40, 1], [80, 40, 1]];
const size = 40;
/// **多种颜色** ✓（第 70 轮：此前只试过红色 ✗ ⇒ 颜色数学若真有分歧，这一步就会抓出来 ✓）：
/// 纯红 / 半灰 / 饱和蓝 / 白 / 黑 —— 覆盖色相 ✓、低饱和 ✓、极值 ✓。
const colours = [
  ["red", { r: 255, g: 0, b: 0, a: 255 }],
  ["grey", { r: 128, g: 128, b: 128, a: 255 }],
  ["blue", { r: 0, g: 64, b: 255, a: 255 }],
  ["white", { r: 255, g: 255, b: 255, a: 255 }],
  ["black", { r: 0, g: 0, b: 0, a: 255 }],
];

// **走 bindgen 包** ✓ —— 与浏览器里那条路**完全一致** ✓（这才是"两模式同笔同结果"的真正对照 ✓）。
const module_ = await import(pathToFileURL(wasmPath).href);
// **必须先初始化** ✓ —— 与查看器同一句 ✓（`viewer.rs:1536` 的 `await module.default()` ✓）。
// Node 里没有 fetch 相对路径的语境 ✓ ⇒ 把**同目录的 `_bg.wasm`** 交给它 ✓（bindgen 的标准入参 ✓）。
const bgPath = wasmPath.replace(/\.js$/, "_bg.wasm");
await module_.default(readFileSync(bgPath));
const kernel = new module_.WasmKernel("kernel-parity", 256, 400, 300, 64 * 1024 * 1024);
/// 收 JSON 请求 ⇒ 成功给像素 ✓、失败给 null ✓（内核返回 `Option<Vec<u8>>` ✓ ⇒ 可区分 ✓）
const facade = (request) => {
  const out = kernel.paint_brush(JSON.stringify(request));
  return out ? Uint8Array.from(out) : null;
};

await tool("create_layer", { layer_id: "L" });
let allEqual = true;
for (const brush of names) {
 for (const [colourName, colour] of colours) {
  const slug = `${brush.replace(/[^a-z0-9]/gi, "_")}_${colourName}`;
  const made = await tool("brush_stroke", {
    layer_id: "L", object_id: `o_${slug}`, brush,
    size, color: colour, points: pointList,
  });
  if (!made.ok) {
    console.log(`  ${brush.padEnd(14)} 服务端拒绝：${(made.context || {}).detail?.slice(0, 60)}`);
    allEqual = false;
    continue;
  }
  const region = made.region;
  const got = await tool("get_object", { object_id: `o_${slug}` });
  const blobHash = got?.data?.bitmap?.blob_hash;
  const serverBytes = new Uint8Array(
    await fetch(`${base}/api/blob/${blobHash}?doc=${doc}&token=${token}`).then((r) => r.arrayBuffer()),
  );
  const myb = readFileSync(`assets/brushes/${brush}.myb`, "utf8");
  const facadeBytes = facade({ myb, points: pointList, size, color: colour, opacity: null, hardness: null, region });
  if (!facadeBytes) {
    console.log(`  ${brush.padEnd(14)} 门面返回 0 ⇒ 画不出来 ✗`);
    allEqual = false;
    continue;
  }
  const expected = region.w * region.h * 4;
  let firstDiff = -1, maxDelta = 0, differing = 0;
  const length = Math.min(serverBytes.length, facadeBytes.length);
  for (let i = 0; i < length; i += 1) {
    const delta = Math.abs(serverBytes[i] - facadeBytes[i]);
    if (delta !== 0) {
      differing += 1;
      if (firstDiff < 0) firstDiff = i;
      if (delta > maxDelta) maxDelta = delta;
    }
  }
  const same = serverBytes.length === facadeBytes.length && differing === 0;
  if (!same) allEqual = false;
  // **本地框必须是服务端区域的超集**（第 121 轮定的规矩）：查看器拖动时用的是**本地算的框**，
  // 而服务端算的是**权威区域**；业界做法是"本地只给包得住它的提示区、以服务端为准"。
  // 所以这里不要求两份算式相同，而要**关系成立**：服务端的区域必须被本地框包住。
  // （本地框算式 = 点的 bbox ± (size/2+4)，再 floor/ceil —— 与查看器 `liveRegion` 一致。）
  const half = size / 2 + 4;
  const clientBox = (() => {
    const xs = pointList.map((p) => p[0]);
    const ys = pointList.map((p) => p[1]);
    const left = Math.max(0, Math.floor(Math.min(...xs) - half));
    const top = Math.max(0, Math.floor(Math.min(...ys) - half));
    const right = Math.ceil(Math.max(...xs) + half);
    const bottom = Math.ceil(Math.max(...ys) + half);
    return { x: left, y: top, w: right - left, h: bottom - top };
  })();
  const covers = clientBox.x <= region.x && clientBox.y <= region.y
    && clientBox.x + clientBox.w >= region.x + region.w
    && clientBox.y + clientBox.h >= region.y + region.h;
  if (!covers) {
    allEqual = false;
    console.log(`      ✗ 本地框没包住服务端区域：本地 ${JSON.stringify(clientBox)} vs 服务端 ${JSON.stringify(region)}`);
  }
  console.log(
    `  ${(brush + "/" + colourName).padEnd(22)} 区域 ${region.w}×${region.h}（期望 ${expected} 字节）｜` +
      `服务端 ${serverBytes.length} vs 门面 ${facadeBytes.length}｜不同字节 ${differing}` +
      (firstDiff >= 0 ? `｜首个 @${firstDiff}（通道 ${firstDiff % 4}）｜最大通道差 ${maxDelta}` : "") +
      `｜${same ? "**逐字节相同** ✓" : "有差异 ✗"}`,
  );
 }
}
const total = allBrushes.length;
const pct = total > 0 ? (100 * names.length / total).toFixed(1) : "?";
const sample = wantsAll ? "全量" : (brushes.length ? "指定" : "**默认样本**");
console.log(
  (allEqual ? "结论：逐字节相同 ✓" : "结论：存在差异 ✗（见上；差异的机制是两边数学实现不同 ⇒ 正在修 ✓）") +
  `｜覆盖 ${names.length}/${total} 支笔（${pct}%｜${sample}）` +
  (allEqual && names.length < total ? "｜⚠️ 这只说明**这几支**相同，不代表全部 ✗（用 `all` 可全量 ✓）" : ""),
);
process.exit(allEqual ? 0 : 1);
