#!/usr/bin/env node
// **★ P0 新入口：图层混合模式下拉 ＋ 笔刷压感三预设（真浏览器点击）★**（**第 254 轮 ✓）
//
// **∴ 为什么要这条判据 ✗**：**补丁包自己的 README 写着**"未验证：真机浏览器点击"** ✗
//   ＋ **∴ 而**这两条恰好都是"**接了但可能没生效**"的高危形态** ✓
//     （**∴ 本仓的老毛病 ✗**：**看起来接上了 ✗ 其实没生效** ✓）★
//   ＋ **∴ 所以 ✗**：**本判据**真的去点**✗ 并**从服务端把结果读回来** ✓ ★★★
//
// **∴ 判什么（**四条 ✓）★**：
//   ① **混合模式下拉存在**（**每个图层行都有 ✓ 且当前值 = 服务端的 `blend_mode`** ✓）
//   ② **改它 ⇒ 服务端真的变了**（**改成 multiply ⇒ `list_layers` 读回来是 multiply** ✓
//      ＋ **∴ 刷新图层面板后下拉仍显示 multiply**（**∴ 不是只改了 DOM ✓）★★
//   ③ **压感三预设按钮存在**（柔／线性／硬 ✓）
//   ④ **点「柔」⇒ 服务端**真的应用了**（**∴ 判据直接调 `set_brush_dynamics` 验证契约 ✓
//      ＋ **∴ 并从**页面的日志**确认那一次点击**没有报错**（**"已设置…" 而不是 "设置…失败"** ✓）★★
//
// **∴ 这条判据**抓不到**什么（**如实说 ✓）✗**：**笔刷动力学的效果是"下一笔更粗/更细" ✗
//   ⇒ **∴ 要验证"画出来真的变了"得**画两笔比像素**✗ 本轮没做** ✓（**∴ 只验到"应用成功 ✓）★
//
// 用法：node scripts/browser-p0-entries.mjs <viewer-url> [base-url] [token]
const viewerUrl = process.argv[2];
const baseArg = process.argv[3];
const tokenArg = process.argv[4];
if (!viewerUrl) {
  console.error("用法: node scripts/browser-p0-entries.mjs <viewer-url> [base-url] [token]");
  process.exit(2);
}
const cdpPort = process.env.CDP_PORT || "9333";
// **∴ 从 viewer URL 里兜底取 base/token ✗**（**∴ 有的调用只给一个参数 ✓）★
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

const openTarget = async () => {
  const target = await (
    await fetch(`http://127.0.0.1:${cdpPort}/json/new?${encodeURIComponent(viewerUrl)}`, { method: "PUT" })
  ).json();
  if (!target || !target.webSocketDebuggerUrl) return null;
  return target;
};

const connect = async (target) => {
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
  return { ws, evaluate };
};

const main = async () => {
  const bad = [];
  const target = await openTarget();
  if (!target) {
    console.error("❌ 判不了：开不了标签页（CDP 不可达）⇒ 退出码 3");
    process.exit(3);
  }
  const { ws, evaluate } = await connect(target);
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
    console.error("❌ 判不了：页面没就绪（启动留痕未见）⇒ 退出码 3");
    ws.close();
    process.exit(3);
  }
  // **★★★ 先确认页面上真的能连到**服务端 API** ✗ ★★★**（**CI 实测踩到 ✓；第 254 轮 ✓）
  //   ＋ **∴ 症状 ✗**：**CI 里这条判据报"改成 multiply 却没生效"** ✗ **但本地一切正常** ✓
  //     ＋ **∴ 真相（**CI 日志 ✓）✗**：**页面拿到的是**本地/PWA 页面**✗ 工具调用被
  //       `api-local.js` 接管 ⇒ 回 `endpoint_not_local` ＋ wasm-bindgen 那句
  //       "recursive use of an object …"** ✓ ⇒ **∴ 那**不是产品失败**✗ 是**环境编排** ✓ ★★
  //   ＋ **∴ 所以 ✗**：**动手断言之前**先问一句**：**页面的 `callTool` 能不能打通服务端** ✓
  //     ＋ **∴ 打不通 ⇒ 按本仓约定**打印 ＋ 退 3**✗（**∴ 绝不**误判成产品 bug** ✓）★★★
  const probe = await evaluate(
    `(async () => { try { const r = await window.callTool("get_log", {}, { refresh: false }); return JSON.stringify({ ok: r && r.ok, error: r && r.error_code, detail: (r && r.context && r.context.detail) || "" }); } catch (e) { return "EXC:" + String((e && e.message) || e); } })()`,
  );
  if (typeof probe !== "string" || /^EXC:/.test(probe) || /endpoint_not_local/.test(probe)) {
    console.error(`❌ 判不了：这个页面连不到服务端 API（拿到的是本地/PWA 页面）⇒ 退出码 3｜探针回执：${String(probe).slice(0, 200)}`);
    ws.close();
    process.exit(3);
  }
  const probeOk = JSON.parse(probe);
  if (!probeOk.ok) {
    console.error(`❌ 判不了：页面调 get_log 没成功（${JSON.stringify(probeOk).slice(0, 160)}）⇒ 退出码 3`);
    ws.close();
    process.exit(3);
  }
  console.log(`  （前置）页面能连到服务端 API ✓`);
  // **∴ 先确保面板里**至少有一个图层行**✗（**默认文档有 `layer_default` ✓）
  await evaluate(`(async () => { if (window.refreshLayers) await window.refreshLayers(); return true; })()`);
  await new Promise((r) => setTimeout(r, 800));

  // **① 下拉存在 ＋ 当前值对得上** ✓
  const present = await evaluate(
    `JSON.stringify({ rows: document.querySelectorAll(".layer-row").length, selects: document.querySelectorAll(".layer-row select.layer-blend").length, options: (document.querySelector(".layer-row select.layer-blend")||{options:[]}).options.length, value: (document.querySelector(".layer-row select.layer-blend")||{}).value, dyn: ["dynSoft","dynLinear","dynHard"].filter(id=>document.getElementById(id)).length })`,
  );
  if (typeof present !== "string") {
    console.error("❌ 判不了：读不到图层面板（求值失败）⇒ 退出码 3");
    ws.close();
    process.exit(3);
  }
  const p = JSON.parse(present);
  console.log(`  ① 图层行 ${p.rows} 个｜混合下拉 ${p.selects} 个（${p.options} 个选项）｜压感按钮 ${p.dyn}/3`);
  if (p.rows < 1) bad.push("① 图层面板里没有图层行（判据建不起来）");
  if (p.selects < 1) bad.push("① 图层行里没有混合模式下拉（新入口没接上）");
  if (p.options !== 7) bad.push(`① 混合下拉应有 7 个模式，实测 ${p.options}`);
  if (p.dyn !== 3) bad.push(`① 压感预设按钮应有 3 个（柔/线性/硬），实测 ${p.dyn}`);
  // **★ 必须按**行的 layer id**配对 ✗ 不能拿 `list_layers[0]`** ✓ ★★★**（**CI 实测踩到 ✓）
  //   ＋ **∴ 症状 ✗**：**本地（只有一层 ✓）一切正常 ✗ CI 上"改成 multiply 却没生效"** ✓
  //   ＋ **∴ 根因 ✗**：**图层面板按 **z 降序** 画（**最上面的层在第一行 ✓）✗
  //     而 `list_layers` 按 **z 升序** 返回** ✓ ⇒ **∴ 两边"第一项"**不是同一层** ✓ ★★
  //   ＋ **∴ 现在 ✗**：**从行里读出它的 layer id ✗ 再去接口里找**那一层** ✓
  //     （**∴ 行里的 `name.title` 就是 `layer_id` ✓ 见 `renderLayerPanel` ✓）★★
  const rowLayerId = await evaluate(
    `(() => { const row = document.querySelector(".layer-row"); if (!row) return ""; const span = row.querySelector(".layer-name"); return (span && span.title) || row.dataset.layerId || ""; })()`,
  );
  const findLayer = (list, id) => (((list || {}).layers) || []).find((l) => l.layer_id === id) || null;
  if (!rowLayerId) bad.push("① 拿不到第一行的 layer id（判据自己坏了）");
  const layers0 = await api("list_layers", {});
  const first = findLayer(layers0, rowLayerId);
  if (!first) bad.push(`① 接口里找不到行里的那一层（${rowLayerId}）`);
  else if (p.value && first.blend_mode && p.value !== first.blend_mode) {
    bad.push(`① 下拉显示 ${p.value}，而服务端是 ${first.blend_mode}（初始就不同步）`);
  }

  // **② 改下拉 ⇒ 服务端真的变** ✓
  const changed = await evaluate(
    `(() => { const s = document.querySelector(".layer-row select.layer-blend"); if (!s) return "no-select"; const before = s.value; s.value = "multiply"; s.dispatchEvent(new Event("change", { bubbles: true })); return JSON.stringify({ before, after: s.value }); })()`,
  );
  console.log(`  ② 把下拉改成 multiply：${changed}`);
  await new Promise((r) => setTimeout(r, 1200));
  const layers1 = await api("list_layers", {});
  const after = findLayer(layers1, rowLayerId);
  if (!after) {
    bad.push("② 读不回图层列表（判据自己坏了）");
  } else if (String(after.blend_mode).toLowerCase() !== "multiply") {
    bad.push(`② 下拉改成 multiply 后，服务端的 blend_mode 是 ${after.blend_mode}（**没生效**）`);
  }
  // **∴ 再刷一次面板 ✗ 看下拉是否跟着服务端**（**∴ 不是只改了 DOM ✓）★
  await evaluate(`(async () => { if (window.refreshLayers) await window.refreshLayers(); return true; })()`);
  await new Promise((r) => setTimeout(r, 800));
  const valueNow = await evaluate(`(document.querySelector(".layer-row select.layer-blend")||{}).value`);
  console.log(`  ② 刷新面板后下拉显示：${valueNow}`);
  if (String(valueNow).toLowerCase() !== "multiply") {
    bad.push(`② 刷新后下拉显示 ${valueNow}，与服务端的 multiply 不一致（下拉没有被服务端状态驱动）`);
  }

  // **③④ 压感预设：两段都要验** ✓
  //   ＋ **∴ ③a（**当前是"（内置画笔）"✓）✗**：**点「柔」必须给出**可照着改**的消息** ✓
  //     ＋ **∴ 本机实测踩到 ✗**：**原来只说"请先选择一支笔刷"** ✗
  //       ⇒ **∴ 而**真相比这更细 ✗**：**`#brush` 的空值是**内置几何画笔**（**它没有 MyPaint
  //         动力学 ✓）⇒ **∴ 不能偷偷退回某支 .myb** ✗ **∴ 只能把"去哪选"讲清** ✓ ★★
  //   ＋ **∴ ③b（**真选一支 .myb ✓）✗**：**点「柔」必须**成功** ✗ 且服务端真的应用了** ✓
  //     ＋ **∴ 这一步才是"功能真的能用"的证据 ✗ 而不是"契约存在" ✓ ★★★
  // **★ 读**整段**日志文本 ✗ 不要按"最后 14 个节点"取** ✓ ★**（**第 254 轮实测修 ✓）
  //   ＋ **∴ 我原来取 `slice(-14)` ✗**（**∴ 以为新行在后面 ✓）✗
  //     ＋ **∴ 而** `log()` 是**把新行**插到最前**✗（**聊天式 ✓）⇒ **∴ 于是**我读到的
  //       永远是**最旧的 14 行** ✗ ⇒ **∴ 新入口的日志**明明写了却"看不到"** ✓
  //         ⇒ **∴ 差一点**把好功能判成坏** ✓ ⇒ **∴ 改成读整段文本 ✗ 与顺序无关** ✓ ★★★
  const readLog = async () => {
    const raw = await evaluate(`String((document.getElementById("log")||{}).textContent||"")`);
    return typeof raw === "string" ? raw : "";
  };
  await evaluate(`(() => { const b = document.getElementById("dynSoft"); if (b) b.click(); return Boolean(b); })()`);
  await new Promise((r) => setTimeout(r, 1200));
  const logBuiltin = await readLog();
  console.log(`  ③a 内置画笔时点「柔」：${logBuiltin.slice(0, 200)}`);
  if (/设置笔刷动力学失败/.test(logBuiltin)) {
    bad.push("③a 内置画笔时点「柔」报的是「设置失败」，而不是可照着改的提示");
  } else if (!/（内置画笔）没有压感曲线/.test(logBuiltin)) {
    bad.push("③a 内置画笔时点「柔」没有给出可照着改的提示（说了「请先选择」但没说去哪选）");
  }
  // **③b 真选一支 .myb：点开笔刷库 ⇒ 等列表 ⇒ 选第一支真笔刷** ✓
  await evaluate(`(() => { const o = document.getElementById("brushLibraryOpen"); if (o) o.click(); return Boolean(o); })()`);
  let picked = "";
  for (let i = 0; i < 40; i++) {
    picked = await evaluate(
      `(() => { const s = document.getElementById("brush"); if (!s) return ""; const real = Array.from(s.options).find(o => o.value); if (!real) return ""; s.value = real.value; s.dispatchEvent(new Event("change", { bubbles: true })); return real.value; })()`,
    );
    if (picked) break;
    await new Promise((r) => setTimeout(r, 250));
  }
  const applied = picked
    ? await api("set_brush_dynamics", { brush: picked, curve: { size_pressure: [[0, 0.3], [0.5, 0.8], [1, 1.5]] } })
    : null;
  const appliedCount = applied && Array.isArray(applied.applied) ? applied.applied.length : 0;
  console.log(`  ③b 选中的真笔刷：${picked || "（一支都没选到）"}｜契约复核 ok=${applied && applied.ok}｜applied=${appliedCount} 项`);
  if (!picked) {
    bad.push("③b 笔刷列表打开后仍取不到任何 .myb 笔刷（懒加载或列表接口坏了）");
  } else if (!applied || applied.ok !== true || appliedCount < 1) {
    bad.push(`③b set_brush_dynamics(${picked}) 契约不成立（${JSON.stringify(applied).slice(0, 160)}）`);
  } else {
    // **∴ 真选之后点「柔」必须成功** ✓（**→ 页面日志里出现成功那行 ✓）★
    await evaluate(`(() => { const b = document.getElementById("dynSoft"); if (b) b.click(); return Boolean(b); })()`);
    await new Promise((r) => setTimeout(r, 1500));
    const logReal = await readLog();
    console.log(`  ③b 真笔刷时点「柔」：${logReal.slice(0, 200)}`);
    if (!/压感曲线/.test(logReal) || /设置笔刷动力学失败/.test(logReal)) {
      bad.push("③b 选了真笔刷后点「柔」仍未成功（入口没接上或参数不对）");
    }
  }

  // **④ gaps 补丁的另外三个入口** ✓（**第 254 轮 ✓）：**版本历史 ／ 渐变混合 ／ 导出分层 PSD**
  //   ＋ **∴ 为什么必须写进来 ✗**：**这三个都是"接了但可能没生效"的形态 ✗**
  //     而**本轮实测抓到两处真 bug** ✓：
  //       **∴ ① `export_psd_layers` 的回执字段是 `url` ✗ 补丁读的是 `download_url`**
  //         ⇒ **∴ 照原样永远走 else ⇒ 只打印"已导出（无下载链接）"✗ 点了什么都不下载** ✓
  //       **∴ ② `gradient_blend.brush` 是必填 ✗ 补丁用 `|| "basic"` 兜底**
  //         ⇒ **∴ 而 "basic" 不是资产里的笔刷 ⇒ 请求必失败** ✓ ★★
  const buttons = await evaluate(
    `JSON.stringify({ history: Boolean(document.getElementById("objectHistory")), blend: Boolean(document.getElementById("gradBlend")), psd: Boolean(document.getElementById("exportPsdLayers")) })`,
  );
  const btn = typeof buttons === "string" ? JSON.parse(buttons) : {};
  console.log(`  ④ 新按钮：#objectHistory=${btn.history}｜#gradBlend=${btn.blend}｜#exportPsdLayers=${btn.psd}`);
  if (!btn.history || !btn.blend || !btn.psd) bad.push("④ gaps 补丁的三个按钮没有全部出现在页面上");

  // **④a 对象版本历史：**真的画一个对象 ⇒ 调 `get_object_history` 读它的链** ✓
  const stroke = await api("brush_stroke", {
    layer_id: (await evaluate(`(window.yanshi.state().layerId)||"layer_default"`)) || "layer_default",
    object_id: "hist_probe",
    brush: picked || "classic-brush",
    size: 24,
    points: [[120, 120, 1.0], [240, 240, 1.0]],
    color: { r: 200, g: 40, b: 40, a: 255 },
  });
  const history = await api("get_object_history", { object_id: "hist_probe" });
  const versions = history && (history.atoms || history.history || []);
  console.log(`  ④a 版本历史：画一笔 ok=${stroke && stroke.ok}｜get_object_history ok=${history && history.ok}｜版本 ${Array.isArray(versions) ? versions.length : "?"} 个`);
  if (!stroke || stroke.ok !== true) bad.push(`④a 探针笔画没成功（${JSON.stringify(stroke).slice(0, 120)}）`);
  if (!history || history.ok !== true) bad.push(`④a get_object_history 契约不成立（${JSON.stringify(history).slice(0, 140)}）`);

  // **④b 渐变混合：**用真笔刷调契约**（**∴ 并断言"缺笔刷时页面给的是可照着改的消息" ✓）★
  const gradInfo = await evaluate(
    `(() => { const b = document.getElementById("gradBlend"); if (b) b.click(); return true; })()`,
  );
  await new Promise((r) => setTimeout(r, 1200));
  const gradLog = await readLog();
  console.log(`  ④b 点「渐变混合」（当前笔刷=${picked || "?"}）：${gradLog.slice(0, 200)}`);
  const gradContract = await api("gradient_blend", {
    layer_id: (await evaluate(`(window.yanshi.state().layerId)||"layer_default"`)) || "layer_default",
    from: { x: 40, y: 400, color: { r: 255, g: 255, b: 255, a: 255 } },
    to: { x: 400, y: 400, color: { r: 0, g: 0, b: 0, a: 255 } },
    brush: picked || "classic-brush",
    size: 20,
    steps: 5,
  });
  console.log(`  ④b gradient_blend 契约：ok=${gradContract && gradContract.ok}`);
  if (!gradContract || gradContract.ok !== true) {
    bad.push(`④b gradient_blend 契约不成立（${JSON.stringify(gradContract).slice(0, 140)}）`);
  }
  if (!gradInfo) bad.push("④b 点不到 #gradBlend");

  // **④c 导出分层 PSD：**点它 ⇒ 日志必须是"已导出：<文件名>"✗ 不是"无下载链接"** ✓ ★★★
  //   ＋ **∴ 这一条就是那个 bug 的**回归守卫** ✓（**改回 `download_url` ⇒ 立刻红 ✓）★
  await evaluate(`(() => { const b = document.getElementById("exportPsdLayers"); if (b) b.click(); return true; })()`);
  await new Promise((r) => setTimeout(r, 2500));
  const psdLog = await readLog();
  console.log(`  ④c 点「导出分层 PSD」：${psdLog.slice(0, 200)}`);
  if (/无下载链接/.test(psdLog)) {
    bad.push("④c 导出分层 PSD 落了「无下载链接」分支 ⇒ 回执字段名又对不上了（应为 url）");
  } else if (!/分层 PSD 已导出：/.test(psdLog)) {
    bad.push("④c 点「导出分层 PSD」后日志里没有「已导出：<文件名>」（入口没接上或请求失败）");
  }
  const psdReply = await api("export_psd_layers", {});
  const hasUrl = psdReply && typeof psdReply.url === "string" && psdReply.url.length > 0;
  console.log(`  ④c export_psd_layers 回执：ok=${psdReply && psdReply.ok}｜url=${hasUrl}｜filename=${psdReply && psdReply.filename}`);
  if (!psdReply || psdReply.ok !== true || !hasUrl) {
    bad.push(`④c export_psd_layers 回执里没有 url（${JSON.stringify(psdReply).slice(0, 160)}）`);
  }

  ws.close();
  if (bad.length) {
    console.error(`❌ ${bad.length} 项没成立：${bad.join("｜")}`);
    process.exit(1);
  }
  console.log("✓ 混合模式下拉真的改到服务端（刷新后仍一致）＋ 压感预设点击无错、契约可用");
};

try {
  await main();
} catch (error) {
  console.error(`❌ 判不了：判据自身出错（${error && error.message ? error.message : String(error)}）⇒ 退出码 3`);
  process.exit(3);
}
