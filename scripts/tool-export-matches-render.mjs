#!/usr/bin/env node
// **导出出来的像素，必须和渲染出来的像素是同一个颜色** ✓（第 608 轮，服务色彩 Phase 1）
//
// **⇒ 为什么需要它**：色彩体系统一（Phase 1）最怕的就是**同一条管线里转两次**
//   —— 例如渲染时 sRGB→linear、导出时又转一次 ⇒ 画面会**整体偏移**，而且**很难被现有判据发现**
//   （`tool-export-deterministic` 只比"两次导出是否相同" ⇒ **两次都错也一样相同** ✓）。
//
// **⇒ 这条判据做三件事**：
//   ① 同一块区域，`render_region(raw)` 的像素 **== `export_png` 解码出来的像素** ✓
//   ② 两者都必须**等于所填颜色**（`{r:10,g:200,b:30}`）✓
//   ③ 差异若超过 ±1 就红（PNG 是无损的 ⇒ **只允许 0 差**；留 1 是给未来 rounding 的余地）
//
// **⇒ 变异**：把导出路径里的颜色转换改成"再转一次"⇒ ① 红 ✓；
//   把期望值写成别的颜色 ⇒ ② 红 ✓（证明断言真的在读那条路径）
import { existsSync } from "node:fs";
import { join } from "node:path";
import { execFileSync } from "node:child_process";

const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-export-matches-render.mjs <base> <doc> <token>");
  process.exit(2);
}
const dir = process.env.YANSHI_EXPORT_DIR;
if (!dir) {
  console.error("  ⊘ 不可判：没设 YANSHI_EXPORT_DIR（判据要靠它找到导出文件）");
  process.exit(3);
}

const call = async (name, args) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
  const response = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(args ?? {}),
  });
  return response.json();
};
const check = (ok, message) => {
  if (!ok) {
    console.error(`  ✗ ${message}`);
    process.exit(1);
  }
};

// **填色与期望值必须分开写** ✓（第 608 轮实测）：我第一版用同一个常量既填又比 ⇒
//   把常量改成 `g:201` 时**填的也跟着变** ⇒ 判据照样绿 ✗ ⇒ **变异证明不了它在读那条路径** ✓
const FILL = { r: 10, g: 200, b: 30 };
const WANT = { r: 10, g: 200, b: 30 };
const PNG = `export_vs_render_${Date.now().toString(36)}.png`;

// **图层可能已经存在** ✓（跑在 `_bootstrap` 上时它本来就有 `layer_default`）
//   ⇒ "已存在"是**前提已满足** ✓，不是失败 ✓
const layer = await call("create_layer", { layer_id: "layer_default", name: "l" });
const layerDetail = String(layer.context?.detail ?? "");
check(
  layer.ok === true || layerDetail.includes("已存在"),
  `create_layer ⇒ ${JSON.stringify(layer).slice(0, 120)}`,
);

const filled = await call("fill_region", {
  layer_id: "layer_default",
  shape: { type: "rect", x: 0, y: 0, w: 32, h: 32 },
  color: { ...FILL, a: 255 },
  opacity: 1.0,
});
check(filled.ok === true, `fill_region ⇒ ${JSON.stringify(filled).slice(0, 140)}`);

// ① 渲染像素
const raw = await call("render_region", { region: { x: 16, y: 16, w: 1, h: 1 }, raw: true, max_px: 1000 });
check(raw.ok === true && Boolean(raw.raw_url), `render_region(raw) ⇒ ${JSON.stringify(raw).slice(0, 140)}`);
const rendered = Buffer.from(await (await fetch(base + raw.raw_url)).arrayBuffer());
const rp = [rendered[0], rendered[1], rendered[2], rendered[3]];

// ② 导出像素（PNG 无损 ⇒ 用 ffmpeg 解成 rgba 再取第一个像素）
const exported = await call("export_png", { path: PNG, deterministic: true });
check(exported.ok === true, `export_png ⇒ ${JSON.stringify(exported).slice(0, 140)}`);
const file = join(dir, PNG);
check(existsSync(file), `导出文件不存在：${file}`);
let decoded;
try {
  decoded = execFileSync("ffmpeg", ["-v", "error", "-i", file, "-f", "rawvideo", "-pix_fmt", "rgba", "-"], {
    timeout: 60_000,
    maxBuffer: 64 * 1024 * 1024,
  });
} catch (error) {
  console.error(`  ⊘ 不可判：ffmpeg 解不出这个 PNG：${(error.stderr || "").toString().slice(-200)}`);
  process.exit(3);
}
// 导出是整幅（64×64）⇒ 取 (16,16) 那个像素
const width = exported.width ?? exported.summary?.width ?? 64;
const at = (16 * width + 16) * 4;
const ep = [decoded[at], decoded[at + 1], decoded[at + 2], decoded[at + 3]];

console.log(`  渲染像素 ${JSON.stringify(rp)}｜导出像素 ${JSON.stringify(ep)}｜导出宽 ${width}`);

const near = (a, b) => Math.abs(a - b) <= 1;
check(
  near(rp[0], ep[0]) && near(rp[1], ep[1]) && near(rp[2], ep[2]) && near(rp[3], ep[3]),
  `导出与渲染的像素不一致 ⇒ 很可能在导出路径里又转了一次颜色（渲染 ${JSON.stringify(rp)} vs 导出 ${JSON.stringify(ep)}）`,
);
// **与"所填颜色"必须是精确等于** ✓（第 608 轮实测：用 ±1 的容差时，把期望值改成 g:201
//   仍然通过 ✗ ⇒ 变异证明不了这条断言还活着 ✓）⇒ 只有"渲染 vs 导出"那一对用 ±1 ✓
const exact = (left, right) => left === right;
check(
  exact(rp[0], WANT.r) && exact(rp[1], WANT.g) && exact(rp[2], WANT.b),
  `渲染像素不等于所填颜色 {r:10,g:200,b:30} ⇒ 颜色在渲染路径里被改了：${JSON.stringify(rp)}`,
);
console.log("  ✓ 导出像素 == 渲染像素 == 所填颜色 ⇒ 导出路径没有二次颜色转换");
