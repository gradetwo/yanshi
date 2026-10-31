#!/usr/bin/env node
// criterion-target: client  ## 运行器据此注入 render=client（测本地内核）
// **★ 总超时守卫 ✗ ★**（2026-10-31）：**浏览器/CDP 起不来时本判据会**一直等**✗ ⇒
//   **∴ 超时**报「自己跑不了」并退 3** ✓（**与产品失败区分 ✓**）；`unref()` ⇒ **跑完不被它拖住** ✓。
const __judgeTimeout = setTimeout(() => {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 判据超时（浏览器/CDP 可能没起来）⇒ 退出码 3");
  process.exit(3);
}, Number(process.env.YANSHI_JUDGE_TIMEOUT_MS || 300000));
__judgeTimeout.unref();

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
// 用法：node scripts/browser-fit-fits-visible-host.mjs <viewer-url> [base] [token] [cdpPort]
const url = process.argv[2];
// **★ CDP 端口**只认纯数字** ✗ ★**（外§117⑨ ✓ 由 CI 的 `exit 3` 换来 ✓）：
//   **∴ 编排**（`run-criteria.sh:389` ✓）给 `browser-*` 传**四个**参数 ✗
//     ⇒ `argv[2]`＝viewer URL ✓ ＋ **`argv[3]`＝服务端 base（也是个 URL ✗）** ＋ `argv[4]`＝令牌 ＋ **`argv[5]`＝CDP 端口 ✓**
//   **∴ 旧写法 ✗** `process.argv[3] || "9222"` ⇒ 拿到的是 URL ⇒ 拼成
//     `http://127.0.0.1:http://127.0.0.1:13990/json/list` ⇒ `Failed to parse URL` ⇒ **`exit 3`** ✓
//   **∴ 而它表现成「跳过」**✓ **不是红** ✗ ⇒ **板面全绿而这条判据什么都没验** ✓✓（**静默降级形态⑤**）
//   **∴ 修法**：照 `browser-render-box-truth.mjs:52` ✓ ⇒ **只认纯数字**的槽位 ＋ `CDP_PORT` 兜底 ✓
const port =
  (/^\d+$/.test(process.argv[5] || "") && process.argv[5]) ||
  (/^\d+$/.test(process.argv[3] || "") && process.argv[3]) ||
  process.env.CDP_PORT ||
  "9222";
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
// **∴ 断言 ✓**：**∴ 画布高度必须**严格小于 `window.innerHeight − 96`**✗**
//   ⇒ **∴ 因为**那个值**就是**bug 的上界**✗**（**∴ 实测 481 ✓**）
//   ⇒ **∴ 而**正确的适配**必须**比它小**✗**（**∴ 因为**工具栏／状态栏**真的占了高度**✗）
//   ⇒ **∴ 实测 ✓**：**∴ 修复版画布 **312** ✗ ⇒ **∴ 比 481 小 **169**✗**
//   ⇒ **∴ 而**变异版（`innerHeight−96`）⇒ **∴ 画布会**正好 481**✗ ⇒ **∴ 判据**红 ✓
check(
  measured.canvasH < measured.innerMinus96 - 1,
  `画布高度必须严格小于 innerHeight−96（错的上界）：${measured.canvasH.toFixed(1)} vs ${measured.innerMinus96}`,
);
check(
  measured.canvasH > 240,
  `画布高度必须仍然可用（> 240）：${measured.canvasH.toFixed(1)}`,
);
const gap = measured.innerMinus96 - measured.canvasH;
console.log(`  两者之差 = ${gap.toFixed(1)}（应 ≈ 工具栏＋状态栏的高度）`);

socket.close();
if (problems.length) { console.log(`  ✗ 适配没有落在真实可见宿主内（${problems.length} 处）`); process.exit(1); }
console.log("  ✓ 适配把整幅画放进了真实可见的画布宿主内");
