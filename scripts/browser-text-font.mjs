#!/usr/bin/env node
// criterion-target: client  ## 运行器据此注入 render=client（测本地内核）
// **★ 总超时守卫 ✗ ★**（2026-10-31）：**浏览器/CDP 起不来时本判据会**一直等**✗ ⇒
//   **∴ 超时**报「自己跑不了」并退 3** ✓（**与产品失败区分 ✓**）；`unref()` ⇒ **跑完不被它拖住** ✓。
const __judgeTimeout = setTimeout(() => {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 判据超时（浏览器/CDP 可能没起来）⇒ 退出码 3");
  process.exit(3);
}, Number(process.env.YANSHI_JUDGE_TIMEOUT_MS || 300000));
__judgeTimeout.unref();

// **★ 字体选择必须**真的影响画面**（Phase 1 ✓）★**（**第 254 轮 ✓）
//
// **∴ 为什么要这条判据 ✗**：**现状是**只有一套内嵌 16×16 位图图集**✗（`font_atlas.rs` ✓）
//   ＋ **∴ 客户端**已经在传 `font: "builtin"`**✗（`commitText` ✓）⇒ **∴ 于是**：
//     **∴ 用户**选不了字体**✗ **大字号**只能是位图✗ **emoji 画不出** ✓ ★★
//   ＋ **∴ 方案（`~/patch/yanshi-字体方案-20261014.md` ✓）的 Phase 1 ✗**：
//     **Web 侧自己用 Canvas `fillText` 渲染系统字体 ✗ 再把位图按**笔触那条路**上传** ✓
//     ＋ **∴ 所以**判据要断的核心是：**字体参数**真的改变了像素** ✓ ★★★
//
// **∴ 判什么（**三条 ✓）★**：
//   ① **字体选择入口存在**（`#textFont` ✓ 且至少有 2 个可选字体 ✓）
//   ② **同一段字 ✗ 用两种**明显不同**的字体画 ⇒ **墨迹指纹必须不同** ✓ ★★★
//      （**∴ 还是 `builtin` 位图 ⇒ 两次必然**逐字节相同** ⇒ **红** ✓）
//   ③ **两次都真的画上了**（**∴ 否则"都画不上"也会让指纹相同 ⇒ 假绿** ✓）★
//
// **∴ 变异（**能红的根据 ✓）✗**：**把 `#textFont` 的选择读掉／忽略（**永远用 builtin ✓）
//   ⇒ **∴ ② 立刻红** ✓
//
// **∴ 这条判据**不做**什么（**如实说 ✓）✗**：**不判**字形好不好看**✗
//   **不判**具体是哪个字体文件** ✓（**∴ 只判"参数真的起作用" ✓）★
//
// 用法：node scripts/browser-text-font.mjs <viewer-url> [base-url] [token]
const viewerUrl = process.argv[2];
const baseArg = process.argv[3];
const tokenArg = process.argv[4];
if (!viewerUrl) {
  console.error("用法: node scripts/browser-text-font.mjs <viewer-url> [base-url] [token]");
  process.exit(2);
}
const cdpPort = process.env.CDP_PORT || "9333";
const parsed = new URL(viewerUrl);
const base = baseArg || parsed.origin;
const token = tokenArg || parsed.searchParams.get("token") || "";
const doc = parsed.searchParams.get("doc") || "";

const api = async (tool, args) => {
  const reply = await fetch(
    `${base}/api/tools/${tool}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(args || {}),
    },
  );
  return reply.json();
};

const main = async () => {
  const target = await (
    await fetch(`http://127.0.0.1:${cdpPort}/json/new?${encodeURIComponent(viewerUrl)}`, { method: "PUT" })
  ).json();
  if (!target || !target.webSocketDebuggerUrl) {
    console.error("❌ 判不了：开不了标签页（CDP 不可达）⇒ 退出码 3");
    process.exit(3);
  }
  const ws = new WebSocket(target.webSocketDebuggerUrl);
  let seq = 0;
  const pending = new Map();
  const CALL_TIMEOUT_MS = Number(process.env.CDP_CALL_TIMEOUT_MS || "15000");
  const send = (method, params) =>
    new Promise((resolve, reject) => {
      const id = ++seq;
      const timer = setTimeout(() => {
        pending.delete(id);
        reject(new Error(`CDP ${method} 超时（${CALL_TIMEOUT_MS}ms）`));
      }, CALL_TIMEOUT_MS);
      pending.set(id, {
        resolve: (v) => {
          clearTimeout(timer);
          resolve(v);
        },
        reject: (e) => {
          clearTimeout(timer);
          reject(e);
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
  // **∴ AGENTS.md 的铁律 ✗**：`returnByValue` 与 `awaitPromise` **必须成对** ✓
  const evaluate = async (expression) => {
    const reply = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
    if (reply.exceptionDetails) return { error: JSON.stringify(reply.exceptionDetails).slice(0, 200) };
    return reply.result && reply.result.value;
  };
  const readLog = async () => {
    const raw = await evaluate(`String((document.getElementById("log")||{}).textContent||"")`);
    return typeof raw === "string" ? raw : "";
  };

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
    console.error("❌ 判不了：页面没就绪 ⇒ 退出码 3");
    ws.close();
    process.exit(3);
  }

  const bad = [];
  // **① 字体选择入口** ✓
  const fontUi = await evaluate(
    `JSON.stringify({ has: Boolean(document.getElementById("textFont")), options: (document.getElementById("textFont")||{options:[]}).options.length })`,
  );
  const ui = typeof fontUi === "string" ? JSON.parse(fontUi) : {};
  console.log(`  ① 字体选择：#textFont=${ui.has}｜可选 ${ui.options} 项`);
  if (!ui.has) {
    bad.push("① 没有字体选择入口（#textFont）⇒ 字体功能还没接上");
  } else if (ui.options < 2) {
    bad.push(`① #textFont 只有 ${ui.options} 个选项 ⇒ 选不出两种字体`);
  }

  // **∴ 判据自己驱动：**把同一个字符串用两种字体各画一次 ✗ 比墨迹指纹** ✓ ★
  //   ＋ **∴ `commitText` 用 `window.prompt` ✗ ⇒ 判据里先把它**换成固定返回** ✓
  //     （**∴ 这是**测试替身**✗ 不是改产品 ✓ —— 产品仍走 prompt ✓）★
  const draft = "FontAB";
  const draw = async (family) => {
    const before = await api("get_render_status", {});
    await evaluate(
      `(() => {
         window.prompt = () => ${JSON.stringify(draft)};
         const sel = document.getElementById("textFont");
         if (sel) { sel.value = ${JSON.stringify(family)}; sel.dispatchEvent(new Event("change", { bubbles: true })); }
         return true;
       })()`,
    );
    // **∴ 切到文本工具（**键盘 t ✓ 与 UI 同一条路 ✓）并点画布** ✓
    await evaluate(
      `(() => {
         window.dispatchEvent(new KeyboardEvent("keydown", { key: "t", bubbles: true }));
         return true;
       })()`,
    );
    await new Promise((r) => setTimeout(r, 400));
    const tool = await evaluate(`window.yanshi.state().tool`);
    const box = await evaluate(
      `JSON.stringify((() => { const b = document.getElementById("board"); const r = b.getBoundingClientRect(); return { x: r.left + 120, y: r.top + 120 }; })())`,
    );
    const point = JSON.parse(box);
    await evaluate(
      `(() => {
         const b = document.getElementById("board");
         const opts = { bubbles: true, clientX: ${point.x}, clientY: ${point.y}, button: 0, pointerId: 1, pointerType: "mouse", isPrimary: true };
         b.dispatchEvent(new PointerEvent("pointerdown", opts));
         b.dispatchEvent(new PointerEvent("pointerup", opts));
         return true;
       })()`,
    );
    await new Promise((r) => setTimeout(r, 2500));
    const after = await api("get_render_status", {});
    // **∴ 取"文本那一块"的像素指纹 ✗**（**画布左上角附近 ✓）★
    // **★ 按**刚画出来的那个对象的 bbox** 取样 ✗ ★**（**第 254 轮实测修 ✓）
    //   ＋ **∴ 原来固定取 (0,0,400,300) ✗** 而**文本落在点下去的位置**✗
    //     ⇒ **∴ 于是**我量到的只是整块画布的一角 ⇒ **墨只有 26** ⇒ **∴ 差点把自己的判据判红** ✓ ★★
    //   ＋ **∴ 现在 ✗**：**先问对象列表 ✗ 拿最后一个对象的 bbox ✗ 四周留 8px 边** ✓ ★
    const objects = await api("list_objects", {});
    const list = (objects && (objects.objects || objects.items)) || [];
    const mine = list.filter((o) => String(o.object_id || o.id || "").startsWith("text_"));
    const last = mine[mine.length - 1] || list[list.length - 1];
    let region = { x: 0, y: 0, w: 400, h: 300 };
    const bb = last && (last.bbox || last.bounds);
    if (Array.isArray(bb) && bb.length === 4) {
      region = {
        x: Math.max(0, Math.floor(bb[0]) - 8),
        y: Math.max(0, Math.floor(bb[1]) - 8),
        w: Math.max(1, Math.ceil(bb[2]) + 16),
        h: Math.max(1, Math.ceil(bb[3]) + 16),
      };
    }
    const raw = await api("render_region", { region, raw: true });
    if (!raw || !(raw.raw_url || raw.rawUrl)) return { ok: false, why: "取不回像素", tool, head: after && after.head_seq };
    const absolute = String(raw.raw_url || raw.rawUrl).startsWith("http")
      ? String(raw.raw_url || raw.rawUrl)
      : `${base}${raw.raw_url || raw.rawUrl}`;
    const bytes = new Uint8Array(await (await fetch(absolute)).arrayBuffer());
    let ink = 0;
    let sum = 0;
    for (let i = 0; i < bytes.length; i += 4) {
      const isWhite = bytes[i] >= 250 && bytes[i + 1] >= 250 && bytes[i + 2] >= 250;
      if (!isWhite) ink += 1;
      sum = (sum * 31 + bytes[i] + bytes[i + 1] * 3 + bytes[i + 2] * 7) >>> 0;
    }
    return { ok: true, tool, ink, fingerprint: sum, head: after && after.head_seq, before: before && before.head_seq };
  };

  const first = await draw("serif");
  const second = await draw("monospace");
  console.log(`  ③ 第一次（serif）：tool=${first.tool}｜ok=${first.ok}｜墨 ${first.ink}｜指纹 ${first.fingerprint}`);
  console.log(`  ③ 第二次（monospace）：tool=${second.tool}｜ok=${second.ok}｜墨 ${second.ink}｜指纹 ${second.fingerprint}`);
  if (!first.ok || !second.ok) {
    bad.push(`③ 有画不上的（${JSON.stringify(first.why || second.why || "")}）`);
  } else {
    if (first.tool !== "text") bad.push(`③ 没能切到文本工具（tool=${first.tool}）⇒ 判据自己坏了`);
    if (first.ink < 50 || second.ink < 50) {
      bad.push(`③ 有画面上没有墨（${first.ink} / ${second.ink}）⇒ 这条比较没有说服力`);
    } else if (first.fingerprint === second.fingerprint) {
      bad.push("② 两种字体画出来的墨迹**逐字节相同** ⇒ 字体参数没有真的生效（还是内置位图？）");
    }
  }
  // **★ ④ 预置字体（字体方案 Phase 2）★**：**自托管的 `@font-face` 必须**真的可用** ✓ ★★
  //   ＋ **∴ 为什么用 `document.fonts` 而不是"画出来比比看" ✗**：
  //     **∴ 在**默认 sans-serif 恰好就是 Inter**的机器上 ✗ "画出来不同"会**假红** ✓
  //       ⇒ **∴ 于是**：**先 `document.fonts.load` ✗ 再 `check`** ✓ —— **∴ 它**直接回答
  //         "这支字体到底装上了没有"** ✓（**∴ 不依赖系统默认字体是什么 ✓）★★
  //   ＋ **∴ 变异（**能红的根据 ✓）✗**：**把 `@font-face` 去掉（或文件 404 ✓）
  //     ⇒ **`check` 变 false ⇒ **④ 立刻红** ✓★★
  // **★ ④ 预置字体（字体方案 Phase 2）★**：**用两条**谁也冒充不了**的证据** ✓ ★★★
  //   ＋ **∴ 我第一版用 `document.fonts.check('21px "Inter"')` ✗ 当场被证伪**：
  //     **∴ 这台机器**本来就装了 Inter**✗ ⇒ **`check` 恒 true** ✗ 区分不出"自托管装上了没有" ✓ ★★
  //       （**∴ 而且**规范上 `check` 对"能不能用**某个回退**渲染"也返回 true ✓ ⇒ 它**不适合**当这件事的判据 ✓）
  //   ＋ **∴ 现在两条 ✗**：
  //     **∴ ① `document.fonts` 里**有没有我的 `@font-face` 声明** ✓**（**∴ 系统字体**不会**出现在这里 ✓✓）
  //     **∴ ② HTTP 探 `/fonts/<file>` ⇒ 必须 200** ✓（**∴ 文件**真的在、路由**真的通** ✓✓）
  //   ＋ **∴ 变异 ✗**：**去掉 `@font-face`（或路由／文件 ✓）⇒ ① 或 ② 立刻红** ✓ ★★★
  const bundled = [
    { family: "Inter", file: "Inter.ttf" },
    { family: "JetBrains Mono", file: "JetBrainsMono.ttf" },
  ];
  const declared = await evaluate(
    `JSON.stringify(Array.from(document.fonts || []).map((f) => String(f.family).replace(/["']/g, "")))`,
  );
  const families = typeof declared === "string" ? JSON.parse(declared) : [];
  const options = await evaluate(
    `JSON.stringify(Array.from((document.getElementById("textFont")||{options:[]}).options).map((o) => o.value))`,
  );
  const opts = typeof options === "string" ? JSON.parse(options) : [];
  console.log(`  ④ @font-face 声明：${JSON.stringify(families)}`);
  console.log(`  ④ #textFont 选项：${JSON.stringify(opts)}`);
  for (const { family, file } of bundled) {
    if (!families.includes(family)) {
      bad.push(`④ 没有「${family}」的 @font-face 声明（document.fonts 里看不到）`);
    }
    if (!opts.includes(family)) bad.push(`④ #textFont 里没有「${family}」这个选项`);
    const probe = await fetch(`${base}/fonts/${encodeURIComponent(file)}`, { method: "GET" });
    const bytes = probe.ok ? (await probe.arrayBuffer()).byteLength : 0;
    console.log(`  ④ GET /fonts/${file} ⇒ HTTP ${probe.status}｜${(bytes / 1048576).toFixed(2)} MB`);
    if (!probe.ok) bad.push(`④ 字体文件 /fonts/${file} 取不到（HTTP ${probe.status}）⇒ 路由或文件缺`);
    else if (bytes < 20000) bad.push(`④ /fonts/${file} 只有 ${bytes} 字节 ⇒ 像是错误页而不是字体`);
  }

  // **★ ⑤ 系统字体选择（字体方案 Phase 4）★**：**下拉里要出现**服务端枚举到的系统字体** ✓ ★★
  //   ＋ **∴ 为什么 ✗**：**只有 5 个静态族名（builtin/sans-serif/serif/monospace/cursive ✓）
  //     ＋ 2 个预置 ⇒ **∴ 用户**选不了自己机器上的字体**✗（**而 Phase 3 已经能画它们 ✓）
  //       ⇒ **∴ 于是**："服务端会画"与"用户能选"**必须对上** ✓ ★★
  //   ＋ **∴ 判据做法 ✗**：**点／聚焦那个下拉 ⇒ 触发**懒加载**（**与笔刷列表同一个取舍 ✓：
  //     **别在启动时拉没人要的东西** ✓）⇒ **∴ 然后要求选项数**明显多于静态那 7 个** ✓
  //     ＋ **∴ 并**至少有一个族名能在 `list_fonts` 的回执里找到** ✓（**∴ 证明它来自服务端 ✓）★★
  //   ＋ **∴ 变异 ✗**：**去掉懒加载 ⇒ 仍是 7 个 ⇒ ⑤ 红** ✓ ★★★
  const before = await evaluate(
    `String(Array.from((document.getElementById("textFont")||{options:[]}).options).length)`,
  );
  await evaluate(
    `(() => { const s = document.getElementById("textFont"); if (!s) return false; s.dispatchEvent(new FocusEvent("focus", { bubbles: true })); s.dispatchEvent(new Event("pointerdown", { bubbles: true })); return true; })()`,
  );
  let after = Number(before);
  for (let i = 0; i < 40; i += 1) {
    await new Promise((r) => setTimeout(r, 250));
    const now = await evaluate(
      `String(Array.from((document.getElementById("textFont")||{options:[]}).options).length)`,
    );
    after = Number(now);
    if (after > 7) break;
  }
  const systemOptions = await evaluate(
    `JSON.stringify(Array.from((document.getElementById("textFont")||{options:[]}).options).map((o) => o.value))`,
  );
  const sysOpts = typeof systemOptions === "string" ? JSON.parse(systemOptions) : [];
  const serverFonts = await api("list_fonts", {});
  const serverNames = new Set(((serverFonts && serverFonts.fonts) || []).map((f) => f.family));
  const fromServer = sysOpts.filter((name) => serverNames.has(name));
  console.log(`  ⑤ 下拉选项：懒加载前 ${before} 个 ⇒ 之后 ${after} 个｜其中来自服务端 list_fonts 的 ${fromServer.length} 个`);
  if (after <= 7) {
    bad.push(`⑤ 字体下拉里只有 ${after} 个选项 ⇒ 系统字体没被加载进来（用户选不到机器上的字体）`);
  } else if (fromServer.length === 0) {
    bad.push("⑤ 下拉里的选项没有一个来自 list_fonts ⇒ 它们不是服务端枚举出来的系统字体");
  }

  const logText = await readLog();
  if (/endpoint_not_local/.test(logText)) {
    console.error("❌ 判不了：这个页面是本地/PWA 页面（日志里出现 endpoint_not_local）⇒ 退出码 3");
    ws.close();
    process.exit(3);
  }
  ws.close();
  if (bad.length) {
    console.error(`❌ ${bad.length} 项没成立：${bad.join("｜")}`);
    process.exit(1);
  }
  console.log("✓ 字体选择真的影响画面（两种字体墨迹不同），且两次都真的画上了");
};

try {
  await main();
} catch (error) {
  console.error(`❌ 判不了：判据自身出错（${error && error.message ? error.message : String(error)}）⇒ 退出码 3`);
  process.exit(3);
}
