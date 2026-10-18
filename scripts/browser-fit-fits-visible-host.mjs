#!/usr/bin/env node
// **「适配」必须把整幅画放进**真实可见的画布宿主**✗**（外部复测 QY-D04 的判据 ✓）
//
// **∴ 缺陷 ✓**：**∴ `availableArea` 的高度原本是**`window.innerHeight - 96`**✗**（**∴ 96 是猜的 ✓**）
//   ⇒ **∴ 而**工具栏换行／面板／状态栏出现时**✗ ⇒ **∴ 真实可用高度**更小**✗**
//   ⇒ **∴ 于是**适配把画面缩进一个**溢出的容器**✗ ⇒ **∴ 底部色条**看不见**✗**
//     （**∴ 他们实测 ✓**：**∴ 约 13% ✗、顶见底不见 ✓；全屏模式同 13% 两个都见 ✓**）
//
// **∴ 判据 ✓**：**∴ 点「适配」之后 ✗ ⇒ **∴ 画布元素（`main canvas`）的 rect**✗
//   ⇒ **∴ 必须**完全落在**它的宿主容器 rect 内**✗**（**∴ 容差 2px ✓**）
//   ⇒ **∴ 且**宿主的可见高度**必须**被用满到 90% 以上**✗**（**∴ 防止把画面缩得过小 ✓**）
//
// 用法：node scripts/browser-fit-fits-visible-host.mjs <viewer-url> [cdpPort]
const url = process.argv[2];
const port = process.argv[3] || "9222";
if (!url) { console.error("用法: node scripts/browser-fit-fits-visible-host.mjs <viewer-url> [cdpPort]"); process.exit(2); }

const sleep = (ms) => new Promise((done) => setTimeout(done, ms));
let targets;
try {
  targets = await fetch(`http://127.0.0.1:${port}/json/list`).then((r) => r.json());
} catch (error) {
  console.error(`❌ 判据自身的求值失败（不是产品问题）⇒ 连不上 CDP（127.0.0.1:${port}）：${error.message} ⇒ 退出码 3`);
  process.exit(3);
}
const page = targets.find((t) => t.type === "page");
if (!page) { console.error("没有可用的页面目标 ⇒ 判据无法运行（不是通过）⇒ 退出码 3"); process.exit(3); }

const socket = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((done, fail) => { socket.onopen = done; socket.onerror = fail; });
let nextId = 0;
const waiting = new Map();
socket.onmessage = (event) => {
  const message = JSON.parse(event.data);
  if (message.id && waiting.has(message.id)) { waiting.get(message.id)(message); waiting.delete(message.id); }
};
const send = (method, params = {}) =>
  new Promise((done) => { const id = ++nextId; waiting.set(id, done); socket.send(JSON.stringify({ id, method, params })); });

// **∴ 两个开关必须成对 ✗**（**∴ AGENTS.md 第 402 轮 ✓**：缺 `awaitPromise` 会**静默返回 {}** ✓）
const evaluate = async (expression) => {
  const reply = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  return reply.result?.result?.value;
};

await send("Page.enable");
await send("Runtime.enable");
await send("Page.navigate", { url });
await sleep(4000);

const problems = [];
const check = (ok, why) => { console.log(`  ${ok ? "✓" : "✗"} ${why}`); if (!ok) problems.push(why); };

// ★ **不需要画内容 ✗** ★**（第 141 轮 ✓）：**∴ 那个 bug **与画什么无关**✗** ⇒ **∴ 它是**容器高度算错**✗**
//   ⇒ **∴ 所以**判据**只**打开页面 ✗ ⇒ **∴ 点适配 ✗ ⇒ **∴ 比画布 rect 与宿主 rect**✗** ✓
//   ⇒ **∴ 前提 ✓**：**∴ 调用方给一个**已存在的 doc + token**✗**（**∴ 本判据**不自己建 ✓**）
const tokenForPage = new URL(url).searchParams.get("token") || "";
if (!tokenForPage) {
  console.log("  ❌ 判据自身前提不成立（URL 里没有 token）⇒ 退出码 3");
  process.exit(3);
}

// **∴ 点「适配」✗**（**∴ 找那个按钮 ✓**）
const clicked = await evaluate(`(() => {
  const el = [...document.querySelectorAll("button")].find(b => /适配|fit/i.test(b.textContent || "") || b.id === "fit");
  if (!el) return "no-button";
  el.click();
  return "clicked";
})()`);
if (clicked !== "clicked") { console.log(`  ❌ 判据自身前提不成立（找不到「适配」按钮：${clicked}）⇒ 退出码 3`); process.exit(3); }
await sleep(1200);

// **∴ 独立基准 ✗**：**∴ 不再比**画布 vs 它爹**✗**（**∴ 那是循环 ✓**，第 142 节实测差恒为 2px ✓）
//   ⇒ **∴ 而是**用 **`main.clientHeight` 减去**其它子元素**的实际高度**✗** ⇒ **∴ 那**就是**真实可用高度**✗**
//   ⇒ **∴ 关键 ✗**：**∴ `window.innerHeight - 96` ✗ 会**大于**这个数**✗**（**∴ 工具栏／状态栏占掉了 ✓**）
const measured = await evaluate(`(() => {
  const canvas = document.querySelector("main canvas");
  if (!canvas) return null;
  const host = canvas.parentElement;
  const main = document.querySelector("main");
  const c = canvas.getBoundingClientRect();
  // 独立基准：main 的可见高度 − 除画布宿主之外的所有子元素高度
  let other = 0;
  if (main) {
    for (const child of main.children) {
      if (child === host) continue;
      other += child.getBoundingClientRect().height;
    }
  }
  const mainH = main ? main.clientHeight : window.innerHeight;
  return {
    canvasH: c.height,
    hostH: host.getBoundingClientRect().height,
    independentBudget: Math.max(0, mainH - other),
    mainH,
    otherH: other,
    innerMinus96: window.innerHeight - 96,
  };
})()`);
if (!measured) { console.log("  ❌ 判据自身前提不成立（找不到 main canvas）⇒ 退出码 3"); process.exit(3); }

console.log(`  画布 h=${measured.canvasH.toFixed(1)}｜宿主 h=${measured.hostH.toFixed(1)}`);
console.log(`  独立基准 = main.clientHeight(${measured.mainH}) − 其它子元素(${measured.otherH.toFixed(1)}) = ${measured.independentBudget.toFixed(1)}`);
console.log(`  旧公式 innerHeight−96 = ${measured.innerMinus96}`);
// 断言：画布高度不得超过独立基准（超了就是适配算大了 → 底部会被裁）
check(
  measured.canvasH <= measured.independentBudget + 2,
  `画布高度不得超过独立基准（${measured.canvasH.toFixed(1)} vs ${measured.independentBudget.toFixed(1)}）`,
);
check(
  measured.canvasH >= measured.independentBudget - 2,
  `画布高度应基本用满独立基准（${measured.canvasH.toFixed(1)} vs ${measured.independentBudget.toFixed(1)}）`,
);

socket.close();
if (problems.length) { console.log(`  ✗ 适配没有落在真实可见宿主内（${problems.length} 处）`); process.exit(1); }
console.log("  ✓ 适配把整幅画放进了真实可见的画布宿主内");
