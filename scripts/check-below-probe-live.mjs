#!/usr/bin/env node
// **`YANSHI_BELOW_PROBE` 的探针文件必须真的被写、且格式可解析**（第 759 轮）
//
// **⇒ 为什么要有这条** ✗：最近几轮的渲染性能工作（`below`／`above` 缓存、`split`、`tiles_ok`）
//   **全靠这个探针取证** ✓ —— 而它**一直只有"我在本机 grep 过"** ✓，**没有任何判据** ✗
//   ⇒ 探针一旦静默失效（改名／不再写／字段消失）⇒ **下一次调查会以为"缓存一次都没命中"** ✓
//   ⇒ 那正是本仓反复吃过的「我的测量比被测对象更容易错」✓。
//
// **⇒ 判据（运行时，不是结构检查）**：在**最小工程包**上跑一次 `yanshi-timelapse`（落盘模式 ✓，
//   不需要 ffmpeg ✓），`YANSHI_BELOW_PROBE` 指向一个临时文件 ✓，然后断言：
//   ① 文件**存在且非空** ✓；② 至少有一条 `split_ids` 行（切点证据 ✓）；
//   ③ 至少有一条 `READ` 行，且它的 `n_tiles=` 是**数字** ✓（格式没漂 ✓）。
// **⇒ 变异**：把探针那段 `write_all` 去掉 ⇒ ① 红 ✓。
import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const BIN = "target/release/yanshi-timelapse";
if (!existsSync(BIN)) {
  console.error(`  ⊘ 不可判：没有 ${BIN}（先 cargo build --release -p yanshi-timelapse）`);
  process.exit(3);
}

const root = trackTemp(mkdtempSync(join(tmpdir(), "yanshi-probe-")));
// **最小包**：一条 create_document ＋ 两条图层 ＋ 一笔 —— 与 `check-timelapse-produces-frames` 同形 ✓
const W = 64, H = 64;
const atoms = [
  { id: "a0", seq: 1, kind: "create_document", actor: "human:t", session: "s", timestamp: 0,
    schema_version: 1, payload: { doc_id: "probe_doc", width: W, height: H, color_space: "srgb" } },
  { id: "a1", seq: 2, kind: "create_layer", actor: "human:t", session: "s", timestamp: 0,
    schema_version: 1, payload: { layer_id: "L1", name: "L1", type: "raster" } },
  { id: "a2", seq: 3, kind: "create_layer", actor: "human:t", session: "s", timestamp: 0,
    schema_version: 1, payload: { layer_id: "L2", name: "L2", type: "raster" } },
  { id: "a3", seq: 4, kind: "draw_shape", actor: "human:t", session: "s", timestamp: 0,
    schema_version: 1,
    payload: { layer_id: "L1", object_id: "rect", type: "shape",
               data: { geometry: { kind: "rect", bbox: { x: 4, y: 4, w: 24, h: 24 } },
                       color: { r: 20, g: 120, b: 200, a: 255 } } } },
];
writeFileSync(join(root, "meta.json"), JSON.stringify({ width: W, height: H, doc_id: "probe_doc", fps: 24 }));
writeFileSync(join(root, "atoms.jsonl"), `${atoms.map((atom) => JSON.stringify(atom)).join("\n")}\n`);
const pack = join(root, "probe.yanshi");
execFileSync("tar", ["-cf", pack, "-C", root, "meta.json", "atoms.jsonl"], { stdio: "pipe" });

const probe = join(root, "below-probe.txt");
try {
  execFileSync(
    "sh",
    ["-c", `${BIN} ${pack} ${join(root, "out.mp4")} 1 2>&1 >/dev/null`],
    { encoding: "utf8", timeout: 240_000,
      env: { ...process.env, YANSHI_BELOW_PROBE: probe, YANSHI_TIMELAPSE_RAW_DIR: join(root, "frames") } },
  );
} catch (error) {
  console.error(`  ✗ 跑不起来：${(error.stdout || error.stderr || "").slice(-400)}`);
  process.exit(1);
}

const failures = [];
const check = (ok, label, detail) => {
  console.log(`  ${ok ? "✓" : "✗"} ${label}${detail ? `（${detail}）` : ""}`);
  if (!ok) failures.push(label);
};

check(existsSync(probe), "探针文件被创建", probe);
const text = existsSync(probe) ? readFileSync(probe, "utf8") : "";
check(text.length > 0, "探针文件非空", `${text.length} 字节`);
const split = text.split("\n").filter((line) => line.startsWith("split_ids "));
const reads = text.split("\n").filter((line) => line.startsWith("READ "));
check(split.length > 0, "有 `split_ids` 行（切点证据）", `${split.length} 条`);
check(reads.length > 0, "有 `READ` 行（读取证据）", `${reads.length} 条`);
const tiles = reads.map((line) => /n_tiles=(\d+)/.exec(line)).filter(Boolean).map((m) => Number(m[1]));
check(tiles.length === reads.length, "每条 `READ` 都能解析出数字 `n_tiles`", `${tiles.length}/${reads.length}`);

if (failures.length) {
  console.error(`  ✗ ${failures.length} 项没成立 ⇒ 探针静默失效，下一次调查会被它带偏`);
  process.exit(1);
}
console.log(`✓ 探针可用：${split.length} 条 split_ids、${reads.length} 条 READ（运行时判据）`);
