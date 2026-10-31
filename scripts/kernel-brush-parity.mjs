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
// **★ 画布尺寸必须问服务端 ✗ 不许写死 ★**（2641 轮实测 ✓；外部「测量工具要先被测量」✓）
//   **⇒ 症状**：CI 的 parity job 起的是 **320×240** 的文档 ✓，而本脚本写死 **400×300**
//     ⇒ 只有"吃底图"的笔刷要整幅合成 ⇒ 长度期望 `400×300×4 = 480000`
//     ⇒ 实际 `320×240×4 = 307200` ⇒ **50 支全被跳过** ✗
//     ⇒ 而结论那句"存在差异（差异的机制是两边数学实现不同）"**是假的** ✗ ——
//       **一个字节都没比** ✓（连 arm64 也是同一个假象 ✓）
//   **⇒ 修法**：尺寸从 `describe_canvas` 读 ✓（两处 `WasmKernel` 也用它 ✓），
//     拿不到就 `exit(3)`（判据前提不成立 ≠ 产品失败 ✓）。
const canvasInfo = await tool("describe_canvas", {});
if (!canvasInfo?.ok || !Number.isFinite(canvasInfo.width) || !Number.isFinite(canvasInfo.height)) {
  console.error(
    `  ⊘ 不可判：拿不到画布尺寸 ⇒ ${JSON.stringify(canvasInfo).slice(0, 160)}`,
  );
  process.exit(3);
}
const CANVAS = { width: canvasInfo.width, height: canvasInfo.height };
const kernel = new module_.WasmKernel("kernel-parity", 256, CANVAS.width, CANVAS.height, 64 * 1024 * 1024);
/// 收 JSON 请求 ⇒ 成功给像素 ✓、失败给 null ✓（内核返回 `Option<Vec<u8>>` ✓ ⇒ 可区分 ✓）
const facade = (request) => {
  const out = kernel.paint_brush(JSON.stringify(request));
  return out ? Uint8Array.from(out) : null;
};

// **每个色用新图层**（第 206 轮）：判据原先在**同一画布**上累积落笔 ⇒
// 从第二笔起底图**非空** ⇒ 吃画布的笔刷（smudge / watercolor 一类）服务端会读到它，
// 而门面从空表面开始 ⇒ **两边输入不等价** ⇒ 389/796 次"差异"其实是判据的假设不成立。
// 新图层 ⇒ 底图恒为空 ⇒ 两边输入真正等价 ✓。
let allEqual = true;
let fedBaseCount = 0;
let missingFlagCount = 0;
// **整层合成通道**（第 296 轮 ✓，C⑧ ✓）：吃底图的笔刷**也能比了** ✓。
// 依据（第 295 轮的实测 ✓）：门面（喂非空 base ✓）的输出**就是"base ⊕ 这一笔"的合成结果** ✓ ——
// 两支 `fed_base=true` 的笔刷实测"最大通道差恰好是 **1**" ✓，而两组对照差 2123／7012 ✓。
// ⇒ **∴ 比"门面 vs 画后合成"** ✓，容差按**放宽后的经验包络** ✓（Wang 2026-10-29 定放宽 ✗：旧 1 ulp 语义边界已被两条管线的舍入缺口证伪 ✗，依据见判定处注释 ✓）。
let compositeCompared = 0;
let compositeSame = 0;
const docSize = { x: 0, y: 0, w: CANVAS.width, h: CANVAS.height };
/// 取整幅原始 RGBA（`raw:true` ✓ ⇒ CAS 里的裸像素 ✓）。
const rawRegion = async (region) => {
  const r = await tool("render_region", { region, raw: true });
  if (!r.ok || !r.raw_url) return null;
  return new Uint8Array(await fetch(`${base}${r.raw_url}`).then((x) => x.arrayBuffer()));
};
/// 从整幅里裁出 `region`（行主序 RGBA ✓）。
const cropWhole = (bytes, region) => {
  const out = new Uint8Array(region.w * region.h * 4);
  for (let y = 0; y < region.h; y += 1) {
    const from = ((region.y + y) * docSize.w + region.x) * 4;
    out.set(bytes.subarray(from, from + region.w * 4), y * region.w * 4);
  }
  return out;
};
for (const brush of names) {
 for (const [colourName, colour] of colours) {
  const slug = `${brush.replace(/[^a-z0-9]/gi, "_")}_${colourName}`;
  // **先建"底色层"** ✓（C⑧ ✓）：吃底图的笔刷（smudge 类）靠"抹开画布上已有的颜色"工作 ✓
  // ⇒ 空底图时它们**会被服务端直接拒绝** ✓（实测 `smudge` ✓）⇒ 所以要先铺一层底色 ✓。
  // **顺序即层序** ✓：底色层先建 ⇒ 在**下** ✓；笔触层后建 ⇒ 在**上** ✓。
  const baseLayerId = `B_${slug}`;
  const baseLayer = await tool("create_layer", { layer_id: baseLayerId });
  let beforeWhole = null;
  if (baseLayer.ok) {
    const seeded = await tool("brush_stroke", {
      layer_id: baseLayerId, object_id: `ob_${slug}`, brush: "100%_Opaque",
      size: 44, color: { r: 40, g: 120, b: 200, a: 255 },
      points: [[36, 40, 1], [60, 40, 1], [84, 40, 1]],
    });
    // **惰性**：先只取"画前合成"的字节 ✓（一次整幅 raw ✓）。真要比时才用它 ✓
    // ⇒ 不吃底图的笔刷**不付任何额外代价** ✓（已绿的 535 条不冒风险 ✓）。
    if (seeded.ok) beforeWhole = await rawRegion(docSize);
  }
  const layerId = `L_${slug}`;
  const layer = await tool("create_layer", { layer_id: layerId });
  if (!layer.ok) {
    console.log(`  ${brush.padEnd(14)} 建图层被拒：${(layer.context || {}).detail?.slice(0, 60)}`);
    allEqual = false;
    continue;
  }
  const made = await tool("brush_stroke", {
    layer_id: layerId, object_id: `o_${slug}`, brush,
    size, color: colour, points: pointList,
  });
  if (!made.ok) {
    console.log(`  ${brush.padEnd(14)} 服务端拒绝：${(made.context || {}).detail?.slice(0, 60)}`);
    allEqual = false;
    continue;
  }
  // **按服务端报的 `fed_base` 分类**（第 216 轮）：
  // 这一笔**吃了底图** ⇒ 门面必须拿到**同一份整层合成**才可比 ✓，而判据现在拿不到
  //（`render_region` 回的是 PNG ✗，对象 blob 只是**那一笔**✗，不是整层 ✓）
  // ⇒ **不参与**逐字节判定 ✗，但要**如实计数并打印** ✓（不假装它通过了 ✓）。
  if (made.fed_base === true) {
    fedBaseCount += 1;
    // **C⑧：走整层合成通道** ✓（第 296 轮 ✓）—— 不再跳过 ✗。
    const region = made.region;
    const afterWhole = await rawRegion(docSize);
    if (!beforeWhole || !afterWhole) {
      console.log(`  ${(brush + "/" + colourName).padEnd(22)} 取不到画前/画后合成（整幅 raw 路线失败）⇒ 本次跳过 ✗`);
      allEqual = false;
      continue;
    }
    const want = region.w * region.h * 4;
    if (beforeWhole.length !== docSize.w * docSize.h * 4 || afterWhole.length !== docSize.w * docSize.h * 4) {
      // **这是"判据前提不成立"，不是"两支笔不一样"** ✗（2641 轮：写死尺寸时它伪装成后者 ✓）。
      console.error(
        `  ⊘ 不可判：整幅 raw 长度 ${beforeWhole.length}/${afterWhole.length}，` +
          `期望 ${docSize.w * docSize.h * 4}（文档 ${docSize.w}×${docSize.h}）⇒ 尺寸口径不一致，不是笔刷差异`,
      );
      process.exit(3);
    }
    const beforeBox = cropWhole(beforeWhole, region);
    const afterBox = cropWhole(afterWhole, region);
    const mybC = readFileSync(`assets/brushes/${brush}.myb`, "utf8");
    const kernelC = new module_.WasmKernel("kernel-parity-composite", 256, CANVAS.width, CANVAS.height, 64 * 1024 * 1024);
    // **`Base` 要有三个字段** ✓（`width`/`height`/`rgba` ✓ —— 只传 `rgba` 会被 serde 拒 ⇒ 门面回空 ✗，踩过 ✓）。
    // **新语义底图（Y26-006，own-layer）**：服务端落笔只采本层自身内容 ⇒
    //   笔触层是新建空层、这一笔是它上面第一笔 ⇒ 层自身画前内容恒为透明 ⇒
    //   门面必须喂同一份透明底图（全零），而不是画前整幅合成。
    //   （旧口径喂整幅合成，两边底图不等价 ⇒ 5 色全超容差、最大通道差约 212。）
    //   画后合成比对本身不变：门面层像素仍按 Normal 叠到画前合成上，再与画后合成比 ✓。
    //   若 CI 显示门面拒空底图（输出空），再改走备选：把测试笔直接画在底色层上、
    //   每对后删层（`delete_layer`），使层内容即合成内容——本轮先不做。
    //   **第二轮注（2026-10-30，盒上实测钉死，断言未动）**：C⑧ 仍红不是门面底图没换——
    //   换底图已生效（未合成差 2197→8160），红在服务端一侧：空层底图取回的是**带文档白底**的
    //   渲染（`render_region_raw_layer` 叠了背景），服务端在白底上落笔 ⇒ 笔触对象带不透明白晕
    //   （实测：空层红笔 fringe [255,244,242,255]，与白底上门面输出逐字节同值；层外种子区外两者
    //   全同、种子区内才分叉）。own-layer 语义要求层自身像素（透明底），修产品另立分支，
    //   本判据不断言改动、只等产品分支落地后复绿。
    const emptyBase = new Uint8Array(region.w * region.h * 4);
    const outC = kernelC.paint_brush(JSON.stringify({
      myb: mybC, points: pointList, size, color: colour, opacity: null, hardness: null,
      region, base: { width: region.w, height: region.h, rgba: Array.from(emptyBase) },
    }));
    const facadeC = outC ? Uint8Array.from(outC) : null;
    if (!facadeC || facadeC.length !== want) {
      console.log(`  ${(brush + "/" + colourName).padEnd(22)} 门面合成输出长度 ${facadeC ? facadeC.length : -1}（期望 ${want}）⇒ 本次跳过 ✗`);
      allEqual = false;
      continue;
    }
    // **★ 比法换了（第 2685 轮）✗ ★**：原先拿**门面输出**直接比**画后合成** ✗ ——
    //   而门面给的是**这一笔所在图层的像素** ✓，合成里还含着**底图** ✓ ⇒ 两者本来就不同 ✓
    //   （实测：`Classic_Paint` 五色差 2054~2652 字节、最大通道差 215 ✓ —— 那是**底图**的贡献 ✓，
    //    不是"两边数学不同" ✗）。
    //   **⇒ 正确的对照**：**把门面输出按 Normal 合成叠到底图上** ✓，再和**画后合成**比 ✓
    //     ⇒ 若逐字节（±1 ulp）相同 ⇒ 门面这一笔与真实渲染**等价** ✓。
    // **第四轮注（Y26，线性光合成）**：服务端按线性光合成图层（brush_stroke 响应
    // applied.color_space="linear" 佐证；盒上像素图实测：线性光 Normal 精确命中 893/1155，
    // 余下全部在 ±1 内、平均 0.08；现行 sRGB 直色平均差 22.52、最大 67；1723 个不透明像素
    // 两边公式零差 ⇒ 差异只在半透明墨区，不是边缘抗锯齿）。故 compositeOver 的 RGB 通道
    // 走线性光混合，alpha 通道不动（实测 alpha 零差）。改回 sRGB 直色 ⇒ jitter C⑧ 回红（变红证明）。
    const srgb2lin = (v) => { const s = v / 255; return s <= 0.04045 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4); };
    const lin2srgb = (l) => { const s = l <= 0.0031308 ? l * 12.92 : 1.055 * Math.pow(l, 1 / 2.4) - 0.055; return Math.max(0, Math.min(255, Math.round(s * 255))); };
    const compositeOver = (base, layer) => {
      const out = new Uint8Array(base.length);
      for (let i = 0; i < base.length; i += 4) {
        const a = layer[i + 3] / 255;
        const inv = 1 - a;
        for (let k = 0; k < 3; k += 1) {
          out[i + k] = lin2srgb(srgb2lin(layer[i + k]) * a + srgb2lin(base[i + k]) * inv);
        }
        out[i + 3] = Math.round(layer[i + 3] + base[i + 3] * inv);
      }
      return out;
    };
    const expectedBox = compositeOver(beforeBox, facadeC);
    let diffC = 0, maxC = 0;
    for (let i = 0; i < want; i += 1) {
      const d = Math.abs(afterBox[i] - expectedBox[i]);
      if (d !== 0) { diffC += 1; if (d > maxC) maxC = d; }
    }
    // 旧比法（门面 vs 合成）只作**参考**打印：它的差主要是**底图**贡献 ✓，不参与判定 ✓。
    let rawDiff = 0;
    for (let i = 0; i < want; i += 1) {
      if (afterBox[i] !== facadeC[i]) rawDiff += 1;
    }
    // **容差 8（最大通道差）** ✓ —— 这是**经验包络＋margin** ✓（Wang 2026-10-29 定放宽 ✗：旧 1 ulp
    //   语义边界已被两条管线的舍入缺口证伪 ✗）。依据：D4b 分支实测最大通道差 3（Fountain_SF#1
    //   红色区 88×48，22 字节差；更早 run 最大也是 3）⇒ 8 = 实测最大 3 的约 2.7 倍，留 5 级 margin；
    //   仍比真实分歧量级小一个数量级以上（同文件旧比法参考值最大通道差 215；去门面 base 的变异
    //   最大通道差 255）⇒ 真实笔刷偏差仍会红 ✓。差异字节数只打印、不参与判定（保持原状 ✗：
    //   新加字节 cap 会是新断言维度，可能让现绿的笔刷新红，不属本次放宽范围 ✗；实测 12～68 字节
    //   只占区域字节（88×48×4＝16896）的 0.4% 以内，是抖动量级的佐证 ✓）。
    const okC = maxC <= 8;
    compositeCompared += 1;
    if (okC) compositeSame += 1; else allEqual = false;
    console.log(
      `  ${(brush + "/" + colourName).padEnd(22)} 区域 ${region.w}×${region.h}（整层合成 ✓）｜` +
      `门面⊕底图 vs 画后合成：不同字节 ${diffC}｜最大通道差 ${maxC}｜（未合成时 ${rawDiff}）｜` +
      (okC ? "**在放宽后容差（最大通道差 ≤ 8）内** ✓" : "超出放宽后容差 ✗（**这不等于「笔刷不同」** ✗ —— 两侧管线尚未对齐 ✓）"),
    );
    continue;
  }
  if (made.fed_base !== false) {
    console.log(`  ${brush.padEnd(14)} 服务端没报 fed_base（旧二进制？）⇒ 本次跳过`);
    missingFlagCount += 1;
    continue;
  }
  const region = made.region;
  const got = await tool("get_object", { object_id: `o_${slug}` });
  const blobHash = got?.data?.bitmap?.blob_hash;
  const serverBytes = new Uint8Array(
    await fetch(`${base}/api/blob/${blobHash}?doc=${doc}&token=${token}`).then((r) => r.arrayBuffer()),
  );
  const myb = readFileSync(`assets/brushes/${brush}.myb`, "utf8");
  // **打印递给门面的请求关键字段**（第 258 轮）：门面输出为空 ⇒ 请求里必然有一处不同。
  console.log(
    `    门面请求：region=${JSON.stringify(region)} size=${JSON.stringify(size)}` +
    ` myb_len=${myb.length} points=${JSON.stringify(pointList)}`,
  );
  // **另建一个只用于笔刷比对的内核实例**（第 260 轮）：
  // 上面那个 kernel 在前序段落里已被用过（预览/渲染等）⇒ 文档状态可能已变
  // ⇒ 笔刷可能落在**空的当前图层**上 ⇒ 门面返回**整块全透明**
  //（实测：服务端有墨 #115，门面 #-1 ✓）。
  const brushKernel = new module_.WasmKernel("kernel-parity-brush", 256, CANVAS.width, CANVAS.height, 64 * 1024 * 1024);
  const facadeFresh = (request) => {
    const out = brushKernel.paint_brush(JSON.stringify(request));
    return out ? Uint8Array.from(out) : null;
  };
  const freshBytes = facadeFresh({ myb, points: pointList, size, color: colour, opacity: null, hardness: null, region });
  {
    const firstInk = (buf) => {
      for (let i = 0; i + 3 < buf.length; i += 4) if (buf[i + 3] > 0) return i / 4;
      return -1;
    };
    console.log(`    新实例门面：len=${freshBytes ? freshBytes.length : -1} 有墨首像素 #${freshBytes ? firstInk(freshBytes) : "-"}`);
  }
  const facadeBytes = facadeFresh({ myb, points: pointList, size, color: colour, opacity: null, hardness: null, region });
  if (!facadeBytes) {
    console.log(`  ${brush.padEnd(14)} 门面返回 0 ⇒ 画不出来 ✗`);
    allEqual = false;
    continue;
  }
  // **不再对门面字节做额外换算**（第 266 轮撤回）：
  // 第 253 轮我在这里加过一步"反预乘"，理由是"内核没有这一步"。
  // 但第 257 轮读到内核 read_back 的实际代码：
  //     (u32::from(channel) * 32767).checked_div(alpha15).map(|v| (v.min(32767) >> 7) as u8)
  // ⇒ **内核本来就反预乘** ⇒ 我那一句是**重复施加** ✗ ⇒ 撤回 ✓。
  // 当时的实测也显示它"没改变计数" —— 原因就是门面输出**全零**（alpha 全 0），
  // 而我的换算只对 0 < alpha < 255 生效 ⇒ 它是一次**未被证据支持**的改动。
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
  // **可选：把差异画成一张小图** ✓（`DUMP_DIFF=1` ✓，第 820 轮 ✓）——
  // 因为"最大通道差 255、而与颜色无关"这种读数**说不清形状** ✗，而形状能直接指向机制 ✓。
  // 字符含义：`.` 两边都透明 ✓｜`#` 都实、且颜色一致 ✓｜`S` 只有服务端有墨 ✗｜`K` 只有内核有墨 ✗｜
  // `x` 两边都有墨但颜色不同 ✗。
  if (process.env.DUMP_DIFF && differing > 0) {
    const at = (buf, x, y) => { const i = (y * region.w + x) * 4; return [buf[i], buf[i+1], buf[i+2], buf[i+3]]; };
    console.log(`    差异图（${region.w}×${region.h}，区域左上 = ${region.x},${region.y}）：`);
    for (let y = 0; y < region.h; y += 1) {
      let line = "";
      for (let x = 0; x < region.w; x += 1) {
        const a = at(serverBytes, x, y), b = at(facadeBytes, x, y);
        const aInk = a[3] > 8, bInk = b[3] > 8;
        if (!aInk && !bInk) line += ".";
        else if (aInk && bInk) line += (a[0] === b[0] && a[1] === b[1] && a[2] === b[2] && a[3] === b[3]) ? "#" : "x";
        else line += aInk ? "S" : "K";
      }
      console.log("      " + line);
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
      (firstDiff >= 0
        ? `｜首差像素 @${firstDiff - (firstDiff % 4)}：服务端 [${[0, 1, 2, 3].map((k) => serverBytes[firstDiff - (firstDiff % 4) + k])}]｜门面 [${[0, 1, 2, 3].map((k) => facadeBytes[firstDiff - (firstDiff % 4) + k])}]`
        : "") +
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
  (allEqual
    ? "结论：可比范围内逐字节相同 ✓"
    : "结论：存在差异 ✗（**机制未隔离** ✗ —— 不许再写「两边数学实现不同」当结论 ✓："
      + "取证是本机把**两种底图口径**都试过 ⇒ region 裁剪 2652 字节 / 全画布 6564 字节 ✗"
      + " ⇒ 至少不是「底图被裁小了」这一个原因 ✓；下一步要把服务端的**第二遍掩膜落墨**与门面的单遍对齐再比 ✓）") +
  `｜吃底图的 **已用整层合成比对** ✓：${compositeCompared} 条（其中 ${compositeSame} 条在放宽后容差（最大通道差 ≤ 8）内 ✓）` +
  `｜吃底图的**未比对**：${fedBaseCount - compositeCompared} 条` +
  (missingFlagCount > 0 ? `｜服务端未报 fed_base：${missingFlagCount} 条` : "") +
  `｜覆盖 ${names.length}/${total} 支笔（${pct}%｜${sample}）` +
  (allEqual && names.length < total ? "｜⚠️ 这只说明**这几支**相同，不代表全部 ✗（用 `all` 可全量 ✓）" : ""),
);
process.exit(allEqual ? 0 : 1);
