#!/usr/bin/env node
// criterion-target: client  ## 运行器据此注入 render=client（测本地内核）
// **★ 缩放读数必须**等于真实显示比例**（UI-1 ✓）★**（**第 253 轮 ✓）
//
// **∴ 外部独立测试报的 UI-1 ✗**：**点「适配」后 `#zoomInput` 读数不动（**一直 100 ✗）** ✓
//   ＋ **∴ 本机实测**比它更严重 ✗**：**初始 `displayScale=0.7002`（**70% ✓）✗
//     **而读数与标签都写 `100%`**（**∴ HTML 里的静态默认值 ✓）⇒ **∴ 点「适配」后
//     视图确实归位 ✗ 读数**仍然**是旧值** ✓ ★★
//   ＋ **∴ 根因 ✗**：**回填那两行在 `renderViewport()` 里 ✗ 而那条函数在
//     **服务端渲染模式（`kernelReady()=false` ✓）下**提前 return**（**要排队服务端补画 ✓）
//     ⇒ **∴ 于是**：**只有**内核模式**会更新读数** ✓ ★★★
//
// **∴ 本判据断的是**不变量**✗ 而不是某个入口** ✓ ★**：
//   `#zoomInput.value === round(displayScale × 100)`（**旁边 `#zoom` 标签同口径 ✓）
//   ＋ **∴ 为什么这样最好 ✗**：**入口有四个**（适配／1:1／数值框／± 快捷键 ✓）✗
//     ⇒ **∴ 逐个入口写断言 ⇒ **将来加第五个入口就会漏** ✓
//       ＋ **∴ 而不变量**与入口无关** ⇒ **∴ 谁把 `displayScale` 改了而没回填 ⇒ **立刻红** ✓ ★★
//   ＋ **∴ 两种模式都测 ✗**：**服务端渲染 ✓ 与**内核（`render=client` ✓）**各开一个标签页** ✓
//     （**∴ 因为**这条 bug **只在**服务端渲染模式**下出现 ✓）★★
//
// **∴ 变异（**能红的根据 ✓）✗**：**把 `syncZoomReadout()` 的调用从
//   `clampViewport()`／`renderViewport()` 里删掉** ⇒ **∴ 读数停在 100% ⇒ **本判据红** ✓
//
// 用法：node scripts/browser-zoom-readout.mjs <viewer-url>   （CDP_PORT 走环境变量 ✓）
const viewerUrl = process.argv[2];
if (!viewerUrl) {
  console.error("用法: node scripts/browser-zoom-readout.mjs <viewer-url>");
  process.exit(2);
}
const cdpPort = process.env.CDP_PORT || "9333";

const evaluateAll = async (target) => {
  const ws = new WebSocket(target.webSocketDebuggerUrl);
  let seq = 0;
  const pending = new Map();
  // **★ 每次 CDP 调用都要**有界** ✗ ★**（**第 253 轮实测修 ✓）
  //   ＋ **∴ 症状 ✗**：**本判据头一次跑就出现过 `Detected unsettled top-level await`
  //     （**exit 13 ✓）✗ 一次 `Runtime.evaluate` **没有返回**✗
  //     ⇒ **∴ 于是**：**整个判据**挂死** ⇒ **∴ 而 CI 里看起来像"超时/产品坏了"** ✓
  //   ＋ **∴ 本仓约定 ✗**：**判据跑不了 ⇒ **打印 ＋ `exit 3`**✗ **不许挂死也不许谎报** ✓
  //   ＋ **∴ 所以 ✗**：**每个请求挂一个**超时**（缺省 15s ✓）⇒ **超时就抛 ⇒ 上层退 3** ✓ ★★
  const CALL_TIMEOUT_MS = Number(process.env.CDP_CALL_TIMEOUT_MS || "15000");
  const send = (method, params) =>
    new Promise((resolve, reject) => {
      const id = ++seq;
      const timer = setTimeout(() => {
        pending.delete(id);
        reject(new Error(`CDP ${method} 超时（${CALL_TIMEOUT_MS}ms 未返回）`));
      }, CALL_TIMEOUT_MS);
      pending.set(id, {
        resolve: (value) => {
          clearTimeout(timer);
          resolve(value);
        },
        reject: (error) => {
          clearTimeout(timer);
          reject(error);
        },
      });
      ws.send(JSON.stringify({ id, method, params }));
    });
  ws.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    if (message.id && pending.has(message.id)) {
      const entry = pending.get(message.id);
      pending.delete(message.id);
      message.error ? entry.reject(new Error(JSON.stringify(message.error))) : entry.resolve(message.result);
    }
  });
  await new Promise((resolve) => ws.addEventListener("open", resolve));
  await send("Runtime.enable", {});
  // **★ AGENTS.md 的铁律 ✗**：`returnByValue` 与 `awaitPromise` **必须成对** ✓
  //（**∴ 单独用 `returnByValue` ⇒ async 表达式**静默返回 `{}`** ⇒ **判据会误判成产品 bug** ✓）
  const evaluate = async (expression) => {
    const reply = await send("Runtime.evaluate", {
      expression,
      returnByValue: true,
      awaitPromise: true,
    });
    if (reply.exceptionDetails) {
      return { error: JSON.stringify(reply.exceptionDetails).slice(0, 200) };
    }
    return reply.result && reply.result.value;
  };
  return { ws, evaluate };
};

const openTab = async (url) => {
  const target = await (
    await fetch(`http://127.0.0.1:${cdpPort}/json/new?${encodeURIComponent(url)}`, { method: "PUT" })
  ).json();
  return target;
};

const snap = `JSON.stringify({
  input: (document.getElementById("zoomInput")||{}).value,
  label: (document.getElementById("zoom")||{}).textContent,
  displayScale: window.yanshi.state().displayScale
})`;

const main = async () => {
  const bad = [];
  for (const [name, mode] of [
    ["服务端渲染", ""],
    ["内核", "&render=client"],
  ]) {
    const target = await openTab(`${viewerUrl}${mode}`);
    if (!target || !target.webSocketDebuggerUrl) {
      console.error("❌ 判不了：开不了标签页（CDP 不可达）⇒ 退出码 3");
      process.exit(3);
    }
    const { ws, evaluate } = await evaluateAll(target);
    // **∴ 等页面真的把状态建起来 ✗**（`__appMarks` 是应用自己的启动留痕 ✓）★
    let ready = false;
    for (let i = 0; i < 80; i++) {
      const marks = await evaluate(`JSON.stringify((window.__appMarks||[]).slice(-1))`);
      if (typeof marks === "string" && /fetch-ok|refreshLayers-ok/.test(marks)) {
        ready = true;
        break;
      }
      await new Promise((r) => setTimeout(r, 250));
    }
    if (!ready) {
      console.error(`❌ 判不了：${name} 模式页面没就绪（启动留痕未见）⇒ 退出码 3`);
      ws.close();
      process.exit(3);
    }
    const check = async (when) => {
      const raw = await evaluate(snap);
      if (typeof raw !== "string") {
        bad.push(`${name}/${when}：求值失败（${JSON.stringify(raw).slice(0, 120)}）`);
        return null;
      }
      const state = JSON.parse(raw);
      const expect = Math.round((state.displayScale || 1) * 100);
      const got = Number(state.input);
      console.log(
        `  ${name}／${when}：displayScale=${(state.displayScale || 0).toFixed(4)} ⇒ 应显示 ${expect}%｜输入框 ${state.input}｜标签 ${state.label}`,
      );
      if (got !== expect) bad.push(`${name}/${when}：输入框显示 ${state.input}，真实显示比例是 ${expect}%`);
      if (String(state.label) !== `${expect}%`) {
        bad.push(`${name}/${when}：标签显示 ${state.label}，真实显示比例是 ${expect}%`);
      }
      return state;
    };
    await check("初始");
    // **∴ 点「适配」再断一次 ✗**（**对方报的正是这个入口 ✓）★
    await evaluate(`document.getElementById("zoomFit").click()`);
    await new Promise((r) => setTimeout(r, 600));
    await check("点适配后");
    // **∴ 再用 ± 快捷键把缩放**改掉**✗ 然后断不变量** ✓ ★
    await evaluate(
      `window.dispatchEvent(new KeyboardEvent("keydown", { key: "+", bubbles: true }))`,
    );
    await new Promise((r) => setTimeout(r, 600));
    const plus = await check("按 + 后");
    // **★ 数值框这条路**也必须真的改得动缩放 ✗ ★**（**第 253 轮 ✓）
    //
    // **∴ 我一开始把这条只当"信息"印出来 ✗ 因为它**看起来**改不动** ✓
    //   ＋ **∴ 后来用**决定性探针**查清 ✗**：**在页面里自挂一个监听器 ⇒ **事件确实送达**（seen=1 ✓）
    //     ＋ **∴ 派发后 `state.zoom` 与 `displayScale` **立刻**变成 1.5 ✓ 读数与标签变 150%** ✓
    //       ⇒ **∴ 所以**：**数值框是好的 ✗ 我先前两次"没生效"是**探针写法**造成的** ✓
    //         ＋ **∴ 教训（**本仓纪律 ✓）**：**"没查清"就只印不当门槛 ✓ ⇒ **而一旦查清 ⇒ **就该变成断言** ✓
    //           （**∴ 否则**那条"信息"会**长期误导读的人** ✓）★★
    if (plus && plus.displayScale) {
      const before = plus.displayScale;
      await evaluate(
        `(() => { const b = document.getElementById("zoomInput");
           b.focus(); b.value = "150";
           b.dispatchEvent(new Event("change", { bubbles: true })); b.blur(); return "ok"; })()`,
      );
      await new Promise((r) => setTimeout(r, 700));
      const after = await evaluate(`JSON.stringify(window.yanshi.state().displayScale)`);
      const moved = typeof after === "string" ? Number(after) : NaN;
      console.log(`  数值框（150%）：displayScale ${before.toFixed(4)} → ${Number.isFinite(moved) ? moved.toFixed(4) : after}`);
      if (!Number.isFinite(moved) || Math.abs(moved - before) < 1e-6) {
        bad.push(`${name}/数值框：改成 150% 后 displayScale 没动（${before} → ${after}）`);
      }
      await check("数值框改后");
    }
    ws.close();
  }
  if (bad.length) {
    console.error(`❌ ${bad.length} 项没成立：${bad.join("｜")}`);
    process.exit(1);
  }
  console.log("✓ 两种渲染模式下，缩放读数与真实显示比例始终一致（不变量成立）");
};

// **∴ 任何没接住的异常都**不许**变成"未决的 top-level await"（exit 13 ✓）✗
//   ＋ **∴ 那是**最难查的失败形态**✗（**看不到是哪一步 ✓）⇒ **∴ 统一**打印 ＋ 退 3** ✓ ★
try {
  await main();
} catch (error) {
  console.error(`❌ 判不了：判据自身出错（${error && error.message ? error.message : String(error)}）⇒ 退出码 3`);
  process.exit(3);
}
