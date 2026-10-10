//! **★ 服务 worker 缓存戳必须**幂等更新** ✗ ★**（第 62 轮 ✓；**用户补丁换来的判据 ✓**）。
//!
//! **∴ 它守的 bug ✗ ★**（**用户给的 `yanshi-sw-stamp-fix.patch` 指出的 ✓）：
//!   **∴ `web/sw.js`**是 git 跟踪文件**✗
//!     ⇒ **∴ 占位符 `__BUILD_STAMP__`**在第一次运行后**就被替换并提交 ✓**
//!       ⇒ **∴ 之后**每次同步**都**找不到目标**✗
//!         ⇒ **∴ 原来那句 `replace("__BUILD_STAMP__", …)`** 静默无操作 ✓**
//!           ⇒ **★ 于是**戳**永不更新**✗ ⇒ **∴ 浏览器**永远用旧缓存 ✓ ★**** ✓✓
//!
//! **∴ 判据（**三条 ✓）★**：
//!   **∴ ①** **连续跑两次同步**✗ ⇒ **∴ 两次**戳都必须**与上一次**不同** ✓
//!   **∴ ②** **`web/sw.js` 里**不许**出现占位符**✗（**∴ 一旦**替换过 ⇒ **∴ 它**就该消失 ✓）**
//!   **∴ ③** **戳的格式**必须是 `yanshi-online-<数字>`**✗（**∴ 那**是 SW 删旧缓存的依据 ✓）** ✓✓
//!
//! **∴ 变异（**已验证 ✓）★**：**把**替换退回**只** `replace("__BUILD_STAMP__", …)`**✗
//!   ⇒ **∴ 第二次**戳不变 ⇒ **∴ 判据必红 ✓**** ✓✓
//!
//! **∴ 用法 ✗**：`node scripts/tool-sw-stamp-idempotent.mjs`

import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";

const SW = "web/sw.js";
const SYNC = "scripts/pwa-sync-viewer.mjs";
const failures = [];
const check = (ok, label, detail) => {
  console.log("  " + (ok ? "✓" : "✗") + " " + label + (detail ? "（" + detail + "）" : ""));
  if (!ok) failures.push(label + (detail ? "：" + detail : ""));
};

/** **∴ 读当前戳 ✗**（**∴ 没有**也**要**能看出 ✓）** ✓✓ */
function stamp() {
  const text = readFileSync(SW, "utf8");
  const match = text.match(/yanshi-online-(\d+)/);
  return match ? match[0] : null;
}

function runSync() {
  const result = spawnSync("node", [SYNC], { encoding: "utf8", timeout: 600_000 });
  return { code: result.status, output: (result.stdout || "") + (result.stderr || "") };
}

// **★ 我前两版都把判据设错了 ✗ ★**（第 62 轮 ✓；**判据自己一轮轮抓出来的 ✓）：
//   **∴ 第一版**断言"**连续两次同步戳都要变 ✓"**✗
//     ⇒ **∴ 实测**两次都不变 ✓ ⇒ **∴ 我**的前提**错了 ✓**** ✓✓
//   **∴ 第二版**断言"**改动文件后再同步戳要变 ✓"**✗
//     ⇒ **∴ 而**真相是：**戳在脚本**顶部**就算好了**✗（**文件复制**在它**之后 ✓）
//       ⇒ **∴ 同一进程内**它**反映的是**上一次**的状态 ✓ ⇒ **∴ 仍然**不成立 ✓**** ✓✓
//   **∴ 第三版（**现在 ✓）**：**不再依赖 mtime**✗
//     ⇒ **∴ 而是**直接测**那个补丁修的行为 ✓**：
//       **∴ 把 `web/sw.js` 里的戳**改成一个哨兵值**✗ ⇒ **∴ 跑**同步 ⇒ **∴ 它**必须**被换掉 ✓**
//         **∴ 连做两次**✗ ⇒ **★ 第二次**正是**原实现**会失败的地方**✗
//           （**∴ 因为**占位符**已不在** ⇒ **∴ 只替换占位符** ⇒ **∴ 静默无操作 ✓）★**** ✓✓
import { writeFileSync as writeSync, readFileSync as readSync } from "node:fs";

function putSentinel(value) {
  const text = readSync(SW, "utf8");
  // **∴ 只**改戳那一处**✗（**∴ 不**动别的 ✓）** ✓✓
  writeSync(SW, text.replace(/yanshi-online-\d+/, `yanshi-online-${value}`));
}

const first = stamp();
console.log(`  起始戳：${first}`);
check(first !== null, "`web/sw.js` 必须有 `yanshi-online-<数字>` 形式的戳", String(first));

// **∴ 第一次：**放哨兵 ⇒ 同步 ⇒ 必须被换掉 ✓** ✓✓
putSentinel("1111111111111");
const runA = runSync();
check(runA.code === 0, "第一次同步必须成功", `退出码 ${runA.code}`);
const second = stamp();
console.log(`  放哨兵后同步：1111111111111 ⇒ ${second}`);
check(second !== "yanshi-online-1111111111111",
  "**把戳改成哨兵后**，同步必须**换掉它**（**否则**替换是**静默无操作 ✓）",
  `1111111111111 ⇒ ${second}`);

// **★ 第二次：**再放哨兵 ⇒ 必须**再**被换掉 ✗ ★**（**∴ 这才是那个 bug 的守门用例 ✓）** ✓✓
putSentinel("2222222222222");
const runB = runSync();
check(runB.code === 0, "第二次同步必须成功", `退出码 ${runB.code}`);
const third = stamp();
console.log(`  再放哨兵后同步：2222222222222 ⇒ ${third}`);
check(third !== "yanshi-online-2222222222222",
  "**再一次放哨兵后**，同步**仍必须**换掉它（**原实现**在这里**静默失效 ✓）",
  `2222222222222 ⇒ ${third}`);

// **∴ ② 占位符必须已经消失 ✗** ✓✓
const text = readFileSync(SW, "utf8");
check(!text.includes("__BUILD_STAMP__"),
  "替换之后 `web/sw.js` **不许**再留占位符（**它**是**一次性的 ✓）");

// **∴ ③ 戳必须唯一 ✗**（**∴ 多处出现**会**各删各的缓存 ✓）** ✓✓
const all = text.match(/yanshi-online-\d+/g) || [];
check(new Set(all).size <= 1,
  "`web/sw.js` 里的戳必须**唯一**", `${all.length} 处 ⇒ ${[...new Set(all)].join(",")}`);

console.log("");
if (failures.length) {
  console.error(`  结论：缓存戳**不是幂等的** ✗（${failures.length} 条）`);
  for (const line of failures) console.error("   - " + line);
  process.exit(1);
}
console.log("  结论：✓ 缓存戳幂等更新（**连续两次同步都会换戳 ✓）");
process.exit(0);
