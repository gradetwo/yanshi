#!/usr/bin/env node
// **`get_stroke_log` 必须把**旧 atom** 的数组 `color` 归一成对象** ✓
//
// **∴ 为什么它自己起 server ✗**：数组 `color` 只出现在**旧 atom**✗ 的
//   `payload.source.color: [176,152,103]` ✓（**∴ 不是 `payload.color` ✓**）
//   ⇒ **∴ 要造它**必须先在同一个 workspace 里生成 blob（`import_image` 要 `bitmap.blob_hash` ✓）
//   ⇒ **∴ 且必须在**起 server 之前**✗ 写进磁盘 ✓
//   ⇒ **∴ 而 `run-criteria.sh` 是先起 server 再跑判据 ✗ ⇒ **∴ 那条路做不到 ✓**
//   ⇒ **∴ 所以这一条**自己起一个 server**✗（**∴ 只为这一条 ✓，不改共享脚本 ✓**）
//
// **∴ 判据 ✓**：
//   ① 起 server（临时 root）⇒ 建文档 ⇒ 画一笔（**∴ 于是有 blob ✓**）
//   ② **改** `atoms.jsonl` 里那条 atom 的 `payload.source.color` ⇒ 数组 ✗
//   ③ **重启** server ⇒ 读 `get_stroke_log` ⇒ **∴ 每一项 `color` 必须是对象 ✓**
//   ⇒ **∴ 若归一化失效 ⇒ 那条是数组 ⇒ 红 ✓**
import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync, existsSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { setTimeout as sleep } from "node:timers/promises";
// **∴ 仓库约定 ✗**：**造临时目录的脚本**必须 import 清理模块**✗**（`scripts_clean_their_temp_dirs.rs` 守着 ✓）
import { trackTemp } from "./lib/temp-dirs.mjs";

const [bin] = process.argv.slice(2);
if (!bin) { console.error("用法: node scripts/tool-stroke-log-color-legacy.mjs <path-to-yanshi-serve>"); process.exit(3); }
if (!existsSync(bin)) { console.error(`✗ 找不到 ${bin}（先 cargo build --release -p yanshi-http）`); process.exit(3); }

const root = trackTemp(mkdtempSync(join(tmpdir(), "legacycolor-")));
const port = 18000 + Math.floor(Math.random() * 1000);
const base = `http://127.0.0.1:${port}`;
let server = null;
const startServer = async () => {
  server = spawn(bin, ["--root", root, "--doc", "boot", "--width", "200", "--height", "200", "--bind", `127.0.0.1:${port}`], { stdio: "ignore" });
  for (let i = 0; i < 120; i += 1) {
    try { const r = await fetch(`${base}/api/documents`); if (r.ok) return; } catch { /* 还没起 */ }
    await sleep(250);
  }
  throw new Error("server 没起来");
};
const stopServer = async () => { if (server) { server.kill("SIGKILL"); server = null; await sleep(400); } };
// **∴ 兜底 ✗**：**∴ 任何退出路径都要收掉那个 serve ✓**
//   ⇒ **∴ 因为**`stopServer` 只在**正常路径**被调 ✗ ⇒ **∴ 判据中途抛错时会漏进程 ✓**
//   ⇒ **∴ 实测 ✓**：**∴ 卫生检查发现一个 **3 小时 56 分**的残留 ✗**（**∴ `--root /tmp/legacycolor-*` ✓**）
//     ⇒ **∴ 而**它正是这个判据起的 ✓ ⇒ **∴ 所以**加这一条 ✓
process.on("exit", () => { try { server?.kill("SIGKILL"); } catch { /* 已经没了 */ } });

try {
  await startServer();
  const boot = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: "legacycolor", width: 200, height: 200 }),
  })).json();
  const token = boot.token;
  if (!token) { console.log(`  ✗ 建文档失败：${JSON.stringify(boot).slice(0, 140)}`); process.exit(3); }
  const call = async (name, args) => {
    const url = `${base}/api/tools/${name}?doc=legacycolor&token=${encodeURIComponent(token)}`;
    return await (await fetch(url, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args ?? {}) })).json();
  };
  await call("create_layer", { layer_id: "L", name: "L" });
  const painted = await call("brush_stroke", {
    layer_id: "L", brush: "100%_Opaque", color: { r: 120, g: 60, b: 30, a: 255 },
    points: [[40, 40, 1], [160, 40, 1]], size: 24, preview: false,
  });
  if (painted.ok !== true) { console.log(`  ✗ 画不上去：${JSON.stringify(painted).slice(0, 160)}`); process.exit(1); }
  await stopServer();

  // ② 把那条 atom 的 payload.source.color 改成数组
  const file = join(root, "docs", "legacycolor", "atoms.jsonl");
  if (!existsSync(file)) { console.log(`  ✗ 找不到 ${file}`); process.exit(3); }
  const lines = readFileSync(file, "utf8").split("\n").filter((l) => l.trim());
  let changed = 0;
  const out = lines.map((line) => {
    const atom = JSON.parse(line);
    const source = atom?.payload?.source;
    if (source && Array.isArray(source.color) === false && source.color && typeof source.color === "object") {
      const c = source.color;
      source.color = [c.r ?? 0, c.g ?? 0, c.b ?? 0];
      changed += 1;
      return JSON.stringify(atom);
    }
    return line;
  });
  console.log(`  ${changed > 0 ? "✓" : "✗"} 把 ${changed} 条 atom 的 source.color 改成数组（旧格式）`);
  if (changed === 0) { console.log("  ✗ 没有可改的 atom ⇒ 判据无意义"); process.exit(3); }
  writeFileSync(file, out.join("\n") + "\n", "utf8");

  // ③ 重启 ⇒ 读
  await startServer();
  const log = await call("get_stroke_log", { layer_id: "L", limit: 50 });
  const entries = log.entries ?? [];
  if (entries.length === 0) { console.log("  ✗ 重启后一条 stroke 都没有 ⇒ 判据无意义"); process.exit(3); }
  const shapes = entries.map((e) => (Array.isArray(e.color) ? "array" : typeof e.color));
  console.log(`  读到 ${entries.length} 条｜color 形状 ⇒ ${JSON.stringify(shapes)}`);
  const allObjects = shapes.every((k) => k === "object");
  const first = entries[0]?.color;
  const keysOk = first && typeof first === "object" && !Array.isArray(first)
    && ["r", "g", "b", "a"].every((k) => typeof first[k] === "number");
  console.log(`  ${allObjects ? "✓" : "✗"} 旧 atom 的数组 color 被归一成对象`);
  console.log(`  ${keysOk ? "✓" : "✗"} 归一后的对象含 r/g/b/a ⇒ ${JSON.stringify(first)}`);
  if (!allObjects || !keysOk) { console.log("  ✗ normalize_color 没生效（旧 atom 仍吐数组）"); process.exit(1); }
  console.log("  ✓ 旧 atom 的数组 color 也被归一成对象");
} finally {
  await stopServer();
  rmSync(root, { recursive: true, force: true });
}
