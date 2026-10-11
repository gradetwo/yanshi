#!/usr/bin/env node
// **★ 内核的 4 个调用点必须走**异步**路 ✗ ★**（**第 548 轮 ✓；目标第 8 条 ✓）
//
// **∴ 目标第 8 条原文 ✗**：「把内核其余 **4 处**调用点（`drawKernelRegion`／
//   `drawKernelBoxDirect`／`pickColorAt`／`composeFullDocumentCanvas`）迁移到异步 GPU 路
//   （**需先把外层函数变 `async`**，级联到调用方）」
//
// **∴ 实测（**本仓 ✓）✗**：**这 4 个都是**JS 函数**✗，在
//   `crates/yanshi-http/assets/viewer-app.js` ✓（**另有 `web/index.html` 旧副本 ✓）
//     ⇒ **∴ 而**它们现在**都是同步的** ✓
//       ⇒ **∴ 所以**：**本判据**现在必然红** ✓（**∴ 那**正是"第 8 条未完成"的机器可读形式 ✓）★
//
// **∴ 为什么要先立它 ✗**：**第 8 条**在本机不可编译验证**✗
//   （**`/usr/lib/rustlib` 里**没有 `wasm32` 的 std**✗ ＋ **没有 `rustup`** ✓）
//     ⇒ **∴ 而** CI **已经在构建 wasm**（`ci.yml:143／147`，**含 `--features gpu` ✓）
//       ⇒ **∴ 所以**：**把"完成条件"写成本判据 ⇒ **∴ 改完就能红→绿 ＋ CI 兜编译** ✓ ★
//
// **∴ 它断言什么 ✗**：
//   ⇒ **∴ ① 四个函数**都必须是 `async`** ✓
//   ⇒ **∴ ② 它们的**调用点**必须 `await`**（**∴ 且**不在注释里 ✓）
//   ⇒ **∴ ③ 三个自我检查 ✗**：**文件存在／四个函数都能找到／至少有一个调用点** ✓
//     （**∴ 否则**判据自己坏了 ⇒ **∴ exit 3**✗，**与产品缺陷区分** ✓）★
//
// **∴ 用法 ✗**：`node scripts/tool-kernel-async-sites.mjs`｜`--self-test`
// **∴ 退出码 ✗**：0 通过（**第 8 条已完成 ✓）｜1 产品未完成（**红 ✓）
//   ｜2 用法错｜3 判据自身跑不了

import { existsSync, readFileSync } from "node:fs";

const KERNEL = "crates/yanshi-http/assets/viewer-app.js";
const SITES = [
  "drawKernelRegion",
  "drawKernelBoxDirect",
  "pickColorAt",
  "composeFullDocumentCanvas",
];

/** **∴ 纯函数：判断一个函数是否 async ✗**（**∴ 自检能直接喂假源码 ✓）★ */
export function isAsyncFn(source, name) {
  // **∴ 允许 `async function name(`／`async function name (` 两种写法 ✓
  const re = new RegExp(`(^|[^\\w.])async\\s+function\\s+${name}\\s*\\(`, "m");
  return re.test(source);
}

/** **∴ 纯函数：找调用点（**排除定义与注释 ✓）✗** */
export function callSites(source, name) {
  const out = [];
  const lines = source.split("\n");
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i];
    const trimmed = line.trim();
    if (trimmed.startsWith("//") || trimmed.startsWith("*")) continue; // 注释不算
    const re = new RegExp(`(^|[^\\w.])${name}\\s*\\(`, "g");
    let m;
    while ((m = re.exec(line)) !== null) {
      // **∴ 定义行不算调用点 ✓
      if (/function\s+$/.test(line.slice(0, m.index + 1))) continue;
      out.push({ line: i + 1, text: trimmed, awaited: /await\s*$/.test(line.slice(0, m.index + m[0].length - name.length - 1).trimEnd()) || /\bawait\s/.test(line.slice(0, m.index)) });
    }
  }
  return out;
}

/** **∴ 纯函数：收集问题 ✗**（**∴ 自检可直喂 ✓）★ */
export function problems(source) {
  const found = [];
  for (const name of SITES) {
    if (!new RegExp(`function\\s+${name}\\s*\\(`).test(source)) {
      found.push(`判据自身的输入不完整：找不到函数 ${name}`);
      continue;
    }
    if (!isAsyncFn(source, name)) {
      found.push(`${name} 还不是 async ⇒ **第 8 条未完成**`);
    }
    const sites = callSites(source, name);
    const notAwaited = sites.filter((s) => !s.awaited);
    if (sites.length === 0) {
      found.push(`判据自身的输入异常：${name} 没有任何调用点`);
    } else if (notAwaited.length > 0) {
      found.push(
        `${name} 有 ${notAwaited.length} 个调用点没有 await（**行 ${notAwaited.map((s) => s.line).join("／")}**）`,
      );
    }
  }
  return found;
}

if (process.argv.includes("--self-test")) {
  const sync = "function pickColorAt(event) {\n  return 1;\n}\npickColorAt(e);\n";
  const asynced = "async function pickColorAt(event) {\n  return 1;\n}\nawait pickColorAt(e);\n";
  const cases = [
    { name: "同步函数 ⇒ 报未完成", src: sync, want: "async" },
    { name: "async ＋ await ⇒ 该条不报", src: asynced, want: "clean" },
  ];
  let wrong = 0;
  let red = 0;
  console.log("  ── 自检：识别 async 与 await 都要能红能绿 ──");
  for (const c of cases) {
    const got = problems(c.src).filter((m) => m.includes("pickColorAt"));
    const hit = c.want === "clean" ? got.length === 0 : got.some((m) => m.includes(c.want));
    if (!hit) {
      wrong += 1;
      console.log(`  ✗ ${c.name} ⇒ 得到 ${JSON.stringify(got)}`);
      continue;
    }
    if (got.length > 0) red += 1;
    console.log(`  ✓ ${c.name} ⇒ ${got.length > 0 ? "红" : "绿"}（**符合预期 ✓）`);
  }
  // **∴ 变异 ✗**：**把 `await` 去掉 ⇒ 必须报"没有 await"** ✓
  const mutated = asynced.replace("await pickColorAt(e);", "pickColorAt(e);");
  const mustRed = problems(mutated).some((m) => m.includes("没有 await"));
  if (!mustRed) {
    wrong += 1;
    console.log("  ✗ 变异：去掉 await 竟然不报 ⇒ **判据没有牙**");
  } else {
    red += 1;
    console.log("  ✓ 变异：去掉 await ⇒ 报「没有 await」（**符合预期 ✓）");
  }
  console.log("");
  console.log(`  ⇒ 自检：${cases.length + 1} 例｜判红 ${red}｜不符合期望 ${wrong}`);
  if (wrong > 0) {
    console.error("  ✗ 自检失败 ⇒ **本判据没有牙**");
    process.exit(1);
  }
  console.log("  ✓ 自检通过");
  process.exit(0);
}

if (!existsSync(KERNEL)) {
  console.error(`  ✗ 判据自身的输入缺失（不是产品问题）：找不到 ${KERNEL}`);
  process.exit(3);
}
const source = readFileSync(KERNEL, "utf8");
const missing = SITES.filter((n) => !new RegExp(`function\\s+${n}\\s*\\(`).test(source));
if (missing.length > 0) {
  console.error(`  ✗ 判据自身的输入异常：这些函数找不到 ⇒ ${missing.join("／")}`);
  process.exit(3);
}
const found = problems(source);
console.log("");
console.log("  ★ 内核 4 个调用点的异步状态（目标第 8 条）★");
for (const name of SITES) {
  const sites = callSites(source, name);
  console.log(
    `  · ${name.padEnd(26)} ${isAsyncFn(source, name) ? "async" : "同步 ✗"}｜调用点 ${sites.length}（未 await ${sites.filter((s) => !s.awaited).length}）`,
  );
}
console.log("");
if (found.length > 0) {
  for (const m of found) console.error(`  ✗ ${m}`);
  console.error("");
  console.error(`  ⇒ 目标第 8 条**未完成**：${found.length} 项待办`);
  process.exit(1);
}
console.log("  ✓ 四个调用点都是 async 且都被 await ⇒ 目标第 8 条完成");
process.exit(0);
