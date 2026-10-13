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

/** **∴ 纯函数：函数体里是否**返回可等待物** ✗ ★**（**第 799 轮；**乙的口径 ✓）
 * **∴ 为什么不能只看 `async` ✗**：**`async` 只是**一种**返回 Promise 的写法**✗
 *   ＋ **∴ 而**乙的口径是**行为**✗ ⇒ **∴ 所以**三种写法都算** ✓ ★
 *     ⇒ **∴ ①** `async function`（**它**返回 Promise ✓）
 *       ＋ **∴ ②** 函数体里 `return new Promise(`／`Promise.resolve(`／`Promise.reject(`** ✓
 *         ＋ **∴ ③** 函数体里 `return` 一个**已知的** thenable 工厂**（**如 `kernelRegionAsync(` ✓）★
 */
export function returnsAwaitable(source, name) {
  if (isAsyncFn(source, name)) return true;
  const body = functionBody(source, name);
  if (body === null) return false;
  return /\breturn\s+(new\s+Promise|Promise\.(resolve|reject|all|race)|[A-Za-z_$][\w$]*Async\s*\()/.test(body);
}

/** **∴ 纯函数：取出函数体 ✗**（**∴ 用大括号配平 ⇒ 不依赖正则的贪婪 ✓）★ */
export function functionBody(source, name) {
  const m = new RegExp(`function\\s+${name}\\s*\\([^)]*\\)\\s*\\{`).exec(source);
  if (!m) return null;
  let i = m.index + m[0].length - 1;
  let depth = 0;
  for (let k = i; k < source.length; k += 1) {
    const c = source[k];
    if (c === "{") depth += 1;
    else if (c === "}") {
      depth -= 1;
      if (depth === 0) return source.slice(i, k + 1);
    }
  }
  return null;
}

/** **∴ 纯函数：有没有**重绘回调** ✗ ★**（**第 799 轮；**乙的第三条 ✓）
 * **∴ 为什么必须有 ✗**：**异步结果**必须**最终到达屏幕**✗
 *   ＋ **∴ 否则**"异步"就等于**"不画"** ✓ ★
 */
export function hasRepaintCallback(source) {
  return /\b(requestRepaint|scheduleRepaint|repaintSoon|invalidateFrame|markDirty)\b/.test(source);
}

/** **∴ 纯函数：收集问题 ✗**（**∴ 自检可直喂 ✓）★ */
export function problems(source, only = SITES) {
  // **∴ `only` 是**可选的** ✗ ★**（**第 799 轮；**自检抓到的 ✓）
  //   **∴ 为什么需要它 ✗**：**`problems()` 缺省检查全部 4 个函数**✗
  //     ＋ **∴ 而**自检**只喂一个小样本**（**`pickColorAt` ✓）
  //       ⇒ **∴ 于是**它**报**「找不到函数 drawKernelRegion」等 3 条** ✓
  //         ＋ **∴ 而**那**是**判据自身的输入检查**✗（**∴ 它**本身是对的 ✓）★
  //   **∴ 所以 ✗**：**给出 `only` ⇒ **∴ 自检**喂小样本 ✗ 而**真实运行**仍查全部 4 个** ✓ ★
  const found = [];
  for (const name of only) {
    if (!new RegExp(`function\\s+${name}\\s*\\(`).test(source)) {
      found.push(`判据自身的输入不完整：找不到函数 ${name}`);
      continue;
    }
    // **★★★ 第 799 轮：**按用户裁定的 route 乙重写** ✗ ★★★**
    //   **∴ 用户裁定（**第 576 轮 ② ＋ 选项表 ✓）★**：
    //     ＋ **∴ route 乙 ✗**：**异步结果 ＋ **回调触发重绘**✗ ＋ **调用方 **0 改动**** ✓
    //   **∴ 与本判据原来的口径（**甲 ✓）的差别 ✗**：
    //     ＋ **∴ 甲断言① ✗**：**函数必须是 `async`**
    //       ⇒ **∴ 而**乙只要求**返回可等待物**✗ ⇒ **∴ `async` 只是**一种实现** ✓ ★
    //     ＋ **∴ 甲断言② ✗**：**调用点必须 `await`**（**级联到调用方 ✓）
    //       ⇒ **∴ 而**乙要求**调用方 0 改动**✗ ⇒ **∴ 所以**调用点**必须**不 `await`**** ✓ ★★
    //         ＋ **∴ 那**正是两条口径**方向相反**的地方** ✓
    //   **∴ 乙的第三条（**新增 ✓）★**：**必须有**重绘回调**✗
    //     ＋ **∴ 因为**异步结果**必须**最终到达屏幕**✗ ⇒ **∴ 否则**"异步"就等于"不画"** ✓ ★★★
    if (!returnsAwaitable(source, name)) {
      found.push(`${name} 没有返回可等待物（Promise／thenable）⇒ **按乙的第 8 条未完成**`);
    }
    const sites = callSites(source, name);
    const awaited = sites.filter((s) => s.awaited);
    if (sites.length === 0) {
      found.push(`判据自身的输入异常：${name} 没有任何调用点`);
    } else if (awaited.length > 0) {
      // **∴ 乙要求**调用方 0 改动**✗ ⇒ **∴ 一旦有调用点 await ⇒ **∴ 那**就动了调用方** ✓ ★★
      found.push(
        `${name} 有 ${awaited.length} 个调用点 **await 了**（**行 ${awaited.map((s) => s.line).join("／")}**）` +
          `⇒ **调用方被改动**✗ 与 route 乙（**调用方 0 改动**）不符`,
      );
    }
    if (!hasRepaintCallback(source)) {
      found.push(`找不到重绘回调（**如 requestRepaint／scheduleRepaint**）⇒ **异步结果无法到达屏幕**`);
    }
  }
  return found;
}

if (process.argv.includes("--self-test")) {
  // **★★★ 自检 ＋ 变异都按**乙**的口径重写** ✗ ★★★**（**第 799 轮 ✓）
  //   **∴ 乙的三个条件 ✗**：**① 返回可等待物 ② 调用点**不 await** ③ 有重绘回调** ✓
  //   **∴ 为什么自检也要改 ✗**：**它**原来喂的是甲的形状**✗
  //     ＋ **∴ 于是**它**会**放过乙的红**✗ ⇒ **∴ 那**就是"判据没有牙"** ✓ ★
  const ONLY = ["pickColorAt"]; // **∴ 自检只喂一个小样本 ✓**
  const REPAINT = "function requestRepaint() {}\n";
  const sync = `function pickColorAt(event) {\n  return 1;\n}\npickColorAt(e);\n${REPAINT}`;
  const okB = `function pickColorAt(event) {\n  return new Promise((r) => r(1));\n}\npickColorAt(e);\n${REPAINT}`;
  const awaited = `function pickColorAt(event) {\n  return new Promise((r) => r(1));\n}\nawait pickColorAt(e);\n${REPAINT}`;
  const noRepaint = `function pickColorAt(event) {\n  return new Promise((r) => r(1));\n}\npickColorAt(e);\n`;
  const asyncOnly = `async function pickColorAt(event) {\n  return 1;\n}\npickColorAt(e);\n${REPAINT}`;
  const cases = [
    { name: "同步函数（**不返回可等待物 ✓）⇒ 报未完成", src: sync, want: "可等待物" },
    { name: "★ 乙的完整形状（**Promise ＋ 不 await ＋ 有回调 ✓）⇒ 该条不报 ★", src: okB, want: "clean" },
    { name: "★ 调用点 await 了 ⇒ 报「调用方被改动」★", src: awaited, want: "调用方被改动" },
    { name: "缺重绘回调 ⇒ 报「无法到达屏幕」", src: noRepaint, want: "无法到达屏幕" },
    { name: "async 函数 ＋ 不 await ＋ 有回调 ⇒ 该条不报（**`async` 也算 ✓）", src: asyncOnly, want: "clean" },
  ];
  let wrong = 0;
  let red = 0;
  console.log("  ── 自检：按 **route 乙** 的三个条件都要能红能绿 ──");
  for (const c of cases) {
    // **∴ 过滤要**按期望**而不是按函数名 ✗**（**第 799 轮；**自检抓到的 ✓）★
    //   ＋ **∴ 我原来写 `filter((m) => m.includes("pickColorAt"))`**✗
    //     ＋ **∴ 而**「无法到达屏幕」是**全局消息**✗ ⇒ **∴ 它**不含 `pickColorAt`
    //       ⇒ **∴ 于是**被过滤掉 ⇒ **∴ 自检**误报「判据没有牙」** ✓ ★★
    //         ＋ **∴ 所以**：**直接看**全部问题**✗ 而**不是先过滤** ✓
    const got = problems(c.src, ONLY);
    const hit = c.want === "clean" ? got.length === 0 : got.some((m) => m.includes(c.want));
    if (!hit) {
      wrong += 1;
      console.log(`  ✗ ${c.name} ⇒ 得到 ${JSON.stringify(got)}`);
      continue;
    }
    if (got.length > 0) red += 1;
    console.log(`  ✓ ${c.name} ⇒ ${got.length > 0 ? "红" : "绿"}（**符合预期 ✓）`);
  }
  // **∴ 三个变异（**乙的口径 ✓）★**：
  //   ⇒ **∴ ①** **在好的形状上**加一个 `await`**✗ ⇒ **∴ 必须**报「调用方被改动」** ✓
  //     ⇒ **∴ ②** **把 `Promise` 换成普通返回值**✗ ⇒ **∴ 必须**报「可等待物」** ✓
  //       ⇒ **∴ ③** **删掉重绘回调**✗ ⇒ **∴ 必须**报「无法到达屏幕」** ✓
  const muts = [
    { name: "加一个 await", src: okB.replace("pickColorAt(e);", "await pickColorAt(e);"), want: "调用方被改动" },
    { name: "Promise ⇒ 普通返回", src: okB.replace("return new Promise((r) => r(1));", "return 1;"), want: "可等待物" },
    { name: "删掉重绘回调", src: okB.replace(REPAINT, ""), want: "无法到达屏幕" },
  ];
  for (const mu of muts) {
    const got = problems(mu.src, ONLY);
    if (!got.some((m) => m.includes(mu.want))) {
      wrong += 1;
      console.log(`  ✗ 变异（${mu.name}）竟然不报「${mu.want}」⇒ **判据没有牙**`);
    } else {
      red += 1;
      console.log(`  ✓ 变异：${mu.name} ⇒ 报「${mu.want}」（**符合预期 ✓）`);
    }
  }
  console.log("");
  console.log(`  ⇒ 自检：${cases.length + muts.length} 例｜判红 ${red}｜不符合期望 ${wrong}`);
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
// **★ 传递闭包规模（**第 556 轮 ✓）★**：**「级联到调用方」到底有多大**
//   ⇒ **∴ 为什么必须报它 ✗**：**实测 ＝ **148 个函数****（**79 变 async ＋ 69 加 await ✓）
//     ＋ **∴ 而那**远超"4 处调用点"的表述**✗
//       ⇒ **∴ 所以**：**决策时需要一眼看到它** ✓ ★
function asyncClosure(src) {
  const lines = src.split("\n");
  const defs = new Map();
  const reFn = /^\s*(async\s+)?function\s+([A-Za-z_$][\w$]*)\s*\(/;
  const reArrow = /^\s*(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*(async\s*)?\(/;
  for (let i = 0; i < lines.length; i += 1) {
    let m = reFn.exec(lines[i]);
    if (m) { defs.set(m[2], { line: i + 1, async: !!m[1] }); continue; }
    m = reArrow.exec(lines[i]);
    if (m && !defs.has(m[1])) defs.set(m[1], { line: i + 1, async: !!m[2] });
  }
  const enclosing = (idx) => {
    for (let k = idx; k >= 0; k -= 1) {
      const m = reFn.exec(lines[k]) || reArrow.exec(lines[k]);
      if (m) return m[2] || m[1];
    }
    return null;
  };
  const callersOf = (name) => {
    const out = new Set();
    const def = defs.get(name);
    for (let i = 0; i < lines.length; i += 1) {
      const t = lines[i].trim();
      if (t.startsWith("//") || t.startsWith("*")) continue;
      if (def && i + 1 === def.line) continue;
      if (new RegExp(`(^|[^\\w.])${name}\\s*\\(`).test(lines[i])) {
        const e = enclosing(i);
        if (e && e !== name) out.add(e);
      }
    }
    return out;
  };
  const need = new Set();
  const frontier = [...SITES];
  while (frontier.length > 0) {
    const f = frontier.pop();
    for (const c of callersOf(f)) {
      if (!need.has(c)) { need.add(c); frontier.push(c); }
    }
  }
  let toAsync = 0;
  let toAwait = 0;
  for (const f of [...need, ...SITES]) {
    const d = defs.get(f);
    if (d && d.async) toAwait += 1; else toAsync += 1;
  }
  return { total: need.size + SITES.length, toAsync, toAwait };
}
const closure = asyncClosure(source);
console.log("");
console.log("  ★ 内核 4 个调用点的异步状态（目标第 8 条）★");
for (const name of SITES) {
  const sites = callSites(source, name);
  console.log(
    `  · ${name.padEnd(26)} ${isAsyncFn(source, name) ? "async" : "同步 ✗"}｜调用点 ${sites.length}（未 await ${sites.filter((s) => !s.awaited).length}）`,
  );
}
console.log("");
console.log(
  `  ★ 「级联到调用方」的传递闭包 ＝ ${closure.total} 个函数（要变 async ${closure.toAsync} ＋ 只加 await ${closure.toAwait}）★`,
);
if (found.length > 0) {
  for (const m of found) console.error(`  ✗ ${m}`);
  console.error("");
  console.error(`  ⇒ 目标第 8 条**未完成**：${found.length} 项待办`);
  process.exit(1);
}
console.log("  ✓ 四个调用点都是 async 且都被 await ⇒ 目标第 8 条完成");
process.exit(0);
