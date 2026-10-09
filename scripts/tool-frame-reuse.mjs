// **★ 整幅缓存复用的可红判据 ✗ ★**（第 805 轮 ✓；**目标第 4 条 ✓**）：
//   **∴ 判据 ✗**：**同一区域**连续渲两次 ⇒ **第二次 `frame_reused` 必须为真 ✗**
//     ＋ **第一次必须为假 ✗**（**∴ 否则**这个字段就是常量，**没有信息量 ✓）
//     ＋ **两次的 `below_reused` 都为假 ✗**（**∴ 记录**分层事实：**整幅命中的路上**不进 tile 层 ✓）
//   **∴ 变异（**手工 ✓）**：**去掉 `self.full_frame_hits += 1;` ✗**
//     ⇒ **∴ 第二次**会是假 ⇒ **∴ 本脚本**必红 ✓**（**已验证 ✓）**。
import { spawn } from "node:child_process";
import { setTimeout as sleep } from "node:timers/promises";

const PORT = 26290;
const BIN = process.env.YANSHI_SERVE ?? "/tmp/yt4b/release/yanshi-serve";
const BASE = `http://127.0.0.1:${PORT}`;
let fails = 0;
const check = (ok, msg) => { console.log(`  ${ok ? "OK  " : "FAIL"} ${msg}`); if (!ok) fails += 1; };

const srv = spawn(BIN, ["--bind", `127.0.0.1:${PORT}`, "--root", "/tmp/yt-frame-reuse",
  "--assets-dir", `${process.cwd()}/assets`, "--profile", "all"], { stdio: "ignore" });
const post = async (path, body) => {
  const r = await fetch(BASE + path, { method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify(body) });
  return r.json();
};
try {
  for (let i = 0; i < 120; i += 1) {
    try { if ((await fetch(BASE + "/health")).ok) break; } catch { /* not up yet */ }
    await sleep(250);
  }
  const doc = `fr_${Date.now()}`;
  const { token } = await post("/api/documents", { doc_id: doc, width: 256, height: 256 });
  const q = `?doc=${doc}&token=${token}`;
  const tool = async (name, args) => post(`/api/tools/${name}${q}`, args);
  for (const id of ["L0", "L1", "L2"]) await tool("create_layer", { layer_id: id, name: id });
  await tool("brush_stroke", { layer_id: "L2", brush: "100%_Opaque", size: 40,
    color: { r: 0, g: 0, b: 255, a: 255 }, points: [[100, 100, 1.0]], preview: false });
  const region = { region: { x: 0, y: 0, w: 256, h: 256 } };
  const a = await tool("render_region", region);
  const b = await tool("render_region", region);
  check(a.frame_reused === false, "第一次 frame_reused 为假（未复用）");
  check(b.frame_reused === true, "第二次 frame_reused 为真（整幅缓存命中）");
  check(a.below_reused === false && b.below_reused === false,
    "两次 below_reused 都为假（分层记录：整幅命中的路上不进 tile 层）");
} finally {
  srv.kill("SIGKILL");
}
console.log(fails === 0 ? "\n✅ 整幅缓存复用判据通过 ✓" : `\n❌ ${fails} 项失败 ✗`);
process.exit(fails === 0 ? 0 : 1);
