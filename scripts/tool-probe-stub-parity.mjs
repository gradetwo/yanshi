#!/usr/bin/env node
// **两个 `mod stage_probe` 必须"模块级接口一致"**（第 871 轮）：`render.rs` 里有**两份**探针模块 ——
// 真实现（native ✓）与**空实现**（`#[cfg(target_arch = "wasm32")]` ✓）。
//
// **为什么需要它** ✗：`#[cfg]` 让**本地编译看不到 wasm 那份** ✗ ⇒ 我因此**连续两次**把回归送上 CI ✓：
//   ① `enter` 写进了 `impl ObjectTimings`（**模块级没有它** ✗）⇒ `wasm smoke` 红 ✓；
//   ② stub 的 `report(_scope, …)` 把 `_scope` 放在**第一位** ✗，而真实现与调用点放在**最后** ✓
//      ⇒ **五个 criteria shard 全红** ✗（全是编译错 ✓）。
//
// **判据（两条真条件 ✓，都能红 ✓）**：
//   ① **同名模块级函数 ⇒ 参数类型序列必须一致** ✓（只比类型 ✓，参数名允许 `_` 前缀差异 ✓）；
//      ⚠️ 只解析**模块级**函数 ✓（**跳过 `impl` 体内** ✓ —— 那里的缩进与接口无关 ✓）；
//   ② 文件里**每个 `stage_probe::X` 的用法** ✓ ⇒ 在**两份**模块里都必须存在**模块级**的 `X` ✗
//      （**这条正是抓 `enter` 那次的 ✓**）。
//   ③ 任一侧解析到 0 个函数 ⇒ **报错**（不许静默通过 ✗）；④ 打印覆盖面 ✓。
import { readFileSync } from "node:fs";

const SRC = "crates/yanshi-render/src/render.rs";
const text = readFileSync(SRC, "utf8");
const lines = text.split("\n");

const starts = lines.map((l, i) => (l.match(/^mod stage_probe \{/) ? i : -1)).filter((i) => i >= 0);
if (starts.length !== 2) {
  console.error("❌ 应有**两份** mod stage_probe，实际 " + starts.length + " 份 ⇒ 判据无法运行（不是通过 ✗）");
  process.exit(1);
}
const blocks = starts.map((s) => {
  const end = lines.findIndex((l, i) => i > s && l === "}");
  return lines.slice(s, end + 1);
});

// **模块级函数** ✓：4 空格缩进、且不在 `impl … {` 体内 ✓（impl 体到它的 `    }` 结束 ✓）
const moduleLevel = (body) => {
  const out = new Map();
  let inImpl = false;
  for (let i = 0; i < body.length; i += 1) {
    const line = body[i];
    if (/^    impl\b.*\{\s*$/.test(line)) { inImpl = true; continue; }
    if (inImpl && line === "    }") { inImpl = false; continue; }
    if (inImpl) continue;
    const m = line.match(/^    pub (?:const )?fn ([a-z_0-9]+)\s*\(/);
    if (!m) continue;
    let buf = line.slice(line.indexOf("(") + 1);
    for (let j = i + 1; !buf.includes(")") && j < body.length; j += 1) buf += " " + body[j];
    buf = buf.slice(0, buf.indexOf(")"));
    const types = buf.split(",").map((p) => p.trim()).filter(Boolean)
      .map((p) => { const k = p.indexOf(":"); return (k >= 0 ? p.slice(k + 1) : p).trim(); })
      .join(" | ");
    out.set(m[1], types);
  }
  return out;
};
const [real, stub] = [moduleLevel(blocks[0]), moduleLevel(blocks[1])];

// 文件里用到的 `stage_probe::X`（**含跨行** ✓；只取顶层名字 ✓，`Stage::start` 形式取其类型名 ✓）
const used = new Set();
for (const m of text.matchAll(/stage_probe::([A-Za-z_0-9]+)/g)) used.add(m[1]);

let failed = false;
const say = (m) => { console.error("  ✗ " + m); failed = true; };
for (const [fn, types] of real) {
  if (stub.has(fn) && stub.get(fn) !== types) {
    say("`" + fn + "` 的参数**类型序列不同** ✗：\n      真实现：" + types + "\n      wasm 空实现：" + stub.get(fn));
  }
}
for (const name of used) {
  const isType = /^[A-Z]/.test(name);
  if (isType) continue;                      // `Stage` 这类类型另有其检查（impl 体会被跳过 ✓）
  if (!real.has(name) || !stub.has(name)) {
    say("文件里用了 `stage_probe::" + name + "` ✓，但它在**模块级**"
        + (real.has(name) ? "" : "【真实现】") + (stub.has(name) ? "" : "【wasm 空实现】") + " 不存在 ✗ ⇒ 该目标编不过 ✓");
  }
}
if (real.size === 0 || stub.size === 0) say("某一侧解析到 0 个模块级函数 ⇒ 判据成了空话（不是通过 ✗）");

console.log("  扫描：" + SRC + " ⇒ 真实现模块级 " + real.size + " 个 ✓｜wasm 空实现 " + stub.size + " 个 ✓"
  + "｜文件里用到 stage_probe:: 名字 " + used.size + " 个 ✓");
if (!failed) console.log("  ✓ 两份 stage_probe 的**模块级接口一致**、且**调用点用到的名字两侧都在** ⇒ 两个目标都能编");
process.exit(failed ? 1 : 0);
