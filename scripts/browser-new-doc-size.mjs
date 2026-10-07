#!/usr/bin/env node
// **新建文档的尺寸 + 粗细控件** 判据（用户报告的两条 ✗ ⇒ 本判据守它们不再回来 ✓）。
//
// 用户原话：
//   ① "新建文档無尺寸選項：對話框只有名稱輸入，沒法指定尺寸（想要 1920×1080 做不到）" ✗
//   ② "調粗細的 slider 不顯眼" ✗
//
// **为什么需要它** ✗：
//   ① 服务端 `POST /api/documents` **一直**接受 `width`/`height` ✓，
//      而对话框从来没把它们暴露出来 ✗ ⇒ `ensureDocument()` 把**每个**新文档写死成 1024² ✓
//      ⇒ 这是**前端漏了一个入口**的典型：API 有、界面没有 ✓（判据之外看不出来 ✗）。
//   ② `#size` 原来是一个裸 `<label>` + 120px 滑杆 ✓，没有读数、没有分组 ✓
//      ⇒ 与旁边的复选框长得一样 ⇒ "不显眼" ✓ —— 这是**纯视觉**的事 ✓ ⇒ 源码读不出来 ✗，
//      只有真浏览器能验 ✓。
//
// **它守什么** ✓（六段，各自能红 ✓）：
//   ① 对话框里有宽度/高度输入框、常用尺寸按钮、尺寸提示行 ✓；
//   ② 点「1920 × 1080」预设 ⇒ 两个输入框被填成 1920 / 1080 ✓、提示行给出总像素 ✓；
//   ③ **真的建出 1920×1080** ✓ —— 且四方口径一致：
//      客户端 `state.docSize` ✓、画布板 `board.width/height` ✓、
//      文档列表 `GET /api/documents` ✓、`get_document` ✓；
//   ④ 不填尺寸时**仍是 1024²** ✓（守住改前的缺省行为 ✓，不许悄悄换默认 ✗）；
//   ⑤ 粗细控件"显眼且可用" ✓：上限 512 ✓、有数值读数 ✓、读数与滑杆同步 ✓、
//      有细/粗按钮 ✓、被包进独立分组 ✓、分组有边框 ✓、轨道更长 ✓；
//   ⑥ 三种改法都生效 ✓：拖滑杆 ✓、点 ± ✓、直接输入数值 ✓。
//
// **它不守什么** ✗（说清楚，别当成全覆盖 ✗）：
//   * 不量"落笔的粗细真的变了" ✗ —— 那是 `browser-ui-check.mjs` 的 F2 段（笔尖 12 vs 48 的色带宽度 ✓）；
//   * 不量像素级外观（颜色/对比度/是否"好看"✗）—— 只量结构与关键尺寸 ✓；
//   * 不管其它工具条控件的显眼程度 ✗。
//
// **不写死某次观测** ✓：文档尺寸与控件上限都从**页面与服务端当次运行**读出后比对 ✓，
// 唯一硬编码的是用户**点名要的** 1920×1080 与改前的缺省 1024 ✓（那是需求，不是观测 ✓）。
//
// 用法：CDP_PORT=9xxx node scripts/browser-new-doc-size.mjs "http://127.0.0.1:<port>/?doc=<id>&token=<token>"
// 前置条件：服务端 + 带远程调试的 Chromium（见 scripts/README.md）。

const url = process.argv[2];
const debugPort = process.env.CDP_PORT || "9333";
if (!url) {
  console.error("用法: CDP_PORT=9xxx node scripts/browser-new-doc-size.mjs <viewer-url>");
  process.exit(2);
}

const list = await fetch(`http://127.0.0.1:${debugPort}/json/list`).then((r) => r.json());
const target = list.find((t) => t.type === "page");
if (!target) {
  console.error(`未找到调试目标（Chromium 是否以 --remote-debugging-port=${debugPort} 启动？）`);
  process.exit(2);
}
const ws = new WebSocket(target.webSocketDebuggerUrl);
let id = 1;
const pending = new Map();
ws.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);
  if (message.id && pending.has(message.id)) {
    pending.get(message.id)(message);
    pending.delete(message.id);
  }
});
await new Promise((resolve) => ws.addEventListener("open", resolve));
function send(method, params) {
  const messageId = id++;
  return new Promise((resolve) => {
    pending.set(messageId, resolve);
    ws.send(JSON.stringify({ id: messageId, method, params: params || {} }));
  });
}
async function evaluate(expression) {
  const response = await send("Runtime.evaluate", {
    expression,
    returnByValue: true,
    awaitPromise: true,
  });
  const result = response.result || {};
  if (result.exceptionDetails) {
    throw new Error("页面里抛异常：" + JSON.stringify(result.exceptionDetails).slice(0, 300));
  }
  return result.result ? result.result.value : undefined;
}
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const failures = [];
function check(ok, label, detail) {
  const suffix = detail ? "（" + detail + "）" : "";
  console.log("  " + (ok ? "✓" : "✗") + " " + label + suffix);
  if (!ok) failures.push(label + suffix);
}

// 页面脚本就绪：`window.yanshiCallTool` 是**无条件**提供的入口 ✓（见 viewer-app.js ✓）。
let ready = false;
for (let i = 0; i < 80; i += 1) {
  if (await evaluate("typeof window.yanshiCallTool === 'function'")) { ready = true; break; }
  await sleep(250);
}
if (!ready) {
  console.error("✗ 前置不成立：页面脚本没就绪（`window.yanshiCallTool` 一直不是函数）⇒ 判据无法运行（不是通过）");
  process.exit(2);
}

console.log("① 新建对话框里有尺寸入口");
const shape = await evaluate(`(() => {
  const w = document.getElementById("newWidth");
  const h = document.getElementById("newHeight");
  const presets = document.getElementById("newPresets");
  const hint = document.getElementById("newSizeHint");
  return {
    hasWidth: !!w, hasHeight: !!h,
    widthType: w ? w.type : null,
    presetCount: presets ? presets.querySelectorAll("button[data-size]").length : -1,
    hasHint: !!hint,
  };
})()`);
check(shape.hasWidth && shape.hasHeight, "有宽度与高度输入框");
check(shape.widthType === "number", "宽度是数字输入", String(shape.widthType));
check(shape.presetCount >= 5, "有常用尺寸按钮", shape.presetCount + " 个");
check(shape.hasHint, "有尺寸提示行");

console.log("② 点「1920 × 1080」预设会填进输入框");
const preset = await evaluate(`(() => {
  const presets = document.getElementById("newPresets");
  const button = [...presets.querySelectorAll("button[data-size]")].find((b) => b.getAttribute("data-size") === "1920x1080");
  if (!button) return { found: false };
  button.click();
  return {
    found: true,
    width: document.getElementById("newWidth").value,
    height: document.getElementById("newHeight").value,
    hint: document.getElementById("newSizeHint").textContent,
  };
})()`);
check(preset.found, "找得到 1920×1080 按钮");
check(preset.width === "1920" && preset.height === "1080",
  "两个输入框被填成 1920 / 1080", preset.width + " × " + preset.height);
// 1920 × 1080 = 2073600 px = 2.07M px ✓（提示行要给出这个量级 ✓）。
check(/2\.07M px/.test(String(preset.hint)), "提示行给出总像素", String(preset.hint));

console.log("③ 真的建出 1920×1080，且四方口径一致");
const docId = "newsize-" + Date.now().toString(36);
const created = await evaluate(`(async () => {
  document.getElementById("newName").value = ${JSON.stringify(docId)};
  document.getElementById("newWidth").value = "1920";
  document.getElementById("newHeight").value = "1080";
  document.getElementById("newCreate").click();
  // **要等 docSize 跟上**，不能只看 docId ✗：docId 先变 ✓，
  // 而 docSize 由 refreshThumb 的 get_document 覆盖 ✓（它有 in-flight 去重 ⇒ 可能晚一拍 ✓）。
  let last = null;
  for (let i = 0; i < 80; i += 1) {
    await new Promise((r) => setTimeout(r, 250));
    const st = window.yanshi && window.yanshi.state && window.yanshi.state();
    if (!st) continue;
    const board = document.getElementById("board");
    last = {
      docId: st.docId,
      w: st.docSize ? st.docSize.w : 0,
      h: st.docSize ? st.docSize.h : 0,
      boardW: board.width, boardH: board.height,
    };
    if (st.docId === ${JSON.stringify(docId)} && last.w === 1920 && last.h === 1080 && last.boardW === 1920) return last;
  }
  return last || { docId: null, w: 0, h: 0, boardW: 0, boardH: 0 };
})()`);
check(created.docId === docId, "切到了新文档", String(created.docId));
check(created.w === 1920 && created.h === 1080, "客户端 docSize 是 1920×1080", created.w + " × " + created.h);
// **画布板必须跟着**（本轮实测过：`board` 会停在 1024×1024 ✗ —— 那是 `refreshThumb`
// 覆盖 `docSize` 与 `switchDocument` 调 `sizeBoards` 的**时序**问题 ✓）。
check(created.boardW === 1920 && created.boardH === 1080, "画布板也是 1920×1080",
  "board " + created.boardW + " × " + created.boardH);
const listed = await evaluate(`(async () => {
  const value = await fetch("/api/documents").then((r) => r.json());
  const info = (value.documents || []).find((item) => item.doc_id === ${JSON.stringify(docId)});
  return info ? { w: info.width, h: info.height } : null;
})()`);
check(listed && listed.w === 1920 && listed.h === 1080, "文档列表里尺寸一致",
  listed ? listed.w + " × " + listed.h : "列表里找不到该文档");
// `get_document` **必须带 token** ✓（不带会拿到错误体 ⇒ width 为 undefined ✓，那是判据自己的坑 ✗）。
const serverSize = await evaluate(`(async () => {
  const token = new URLSearchParams(location.search).get("token") || "";
  const response = await fetch("/api/tools/get_document?doc=" + encodeURIComponent(${JSON.stringify(docId)}) +
    "&token=" + encodeURIComponent(token), {
    method: "POST", headers: { "content-type": "application/json" }, body: "{}",
  });
  const value = await response.json();
  if (!value.ok) return { w: -1, h: -1, error: JSON.stringify(value).slice(0, 120) };
  return { w: value.width, h: value.height };
})()`);
check(serverSize.w === 1920 && serverSize.h === 1080, "get_document 也报 1920×1080",
  serverSize.w + " × " + serverSize.h + (serverSize.error ? " " + serverSize.error : ""));

console.log("④ 不填尺寸时仍是 1024²（守住改前的缺省）");
const defaultDocId = "newsize-default-" + Date.now().toString(36);
const byDefault = await evaluate(`(async () => {
  const presets = document.getElementById("newPresets");
  document.getElementById("newName").value = ${JSON.stringify(defaultDocId)};
  document.getElementById("newWidth").value = "1024";
  document.getElementById("newHeight").value = "1024";
  document.getElementById("newCreate").click();
  let last = null;
  for (let i = 0; i < 80; i += 1) {
    await new Promise((r) => setTimeout(r, 250));
    const st = window.yanshi && window.yanshi.state && window.yanshi.state();
    if (!st) continue;
    last = { docId: st.docId, w: st.docSize ? st.docSize.w : 0, h: st.docSize ? st.docSize.h : 0 };
    if (st.docId === ${JSON.stringify(defaultDocId)} && last.w === 1024 && last.h === 1024) return last;
  }
  return last || { docId: null, w: 0, h: 0 };
})()`);
check(byDefault.w === 1024 && byDefault.h === 1024, "缺省仍是 1024×1024",
  byDefault.w + " × " + byDefault.h);

console.log("⑤ 粗细控件显眼且可用");
const sizeShape = await evaluate(`(() => {
  const size = document.getElementById("size");
  const value = document.getElementById("sizeValue");
  const down = document.getElementById("sizeDown");
  const up = document.getElementById("sizeUp");
  const group = size ? size.closest(".size-control") : null;
  const style = group ? getComputedStyle(group) : null;
  return {
    hasRange: !!size,
    max: size ? size.max : null,
    hasValue: !!value,
    valueType: value ? value.type : null,
    synced: !!(size && value) && Number(size.value) === Number(value.value),
    hasButtons: !!down && !!up,
    grouped: !!group,
    borderWidth: style ? style.borderTopWidth : null,
    rangeWidth: size ? Math.round(size.getBoundingClientRect().width) : 0,
  };
})()`);
check(sizeShape.hasRange && sizeShape.max === "512", "滑杆上限是 512（应用本来就支持到 512）",
  "max=" + sizeShape.max);
check(sizeShape.hasValue && sizeShape.valueType === "number", "有数值读数", String(sizeShape.valueType));
check(sizeShape.synced, "读数与滑杆同步");
check(sizeShape.hasButtons, "有细 / 粗两颗按钮");
check(sizeShape.grouped, "滑杆被包进独立分组（不是裸 label）");
check(sizeShape.borderWidth && sizeShape.borderWidth !== "0px", "分组有边框（与旁边控件区分开）",
  String(sizeShape.borderWidth));
check(sizeShape.rangeWidth >= 180, "轨道更长（原来 120px）", sizeShape.rangeWidth + "px");

console.log("⑥ 三种改法都生效");
const flow = await evaluate(`(async () => {
  const size = document.getElementById("size");
  const value = document.getElementById("sizeValue");
  const read = () => ({ range: Number(size.value), out: Number(value.value) });
  size.value = "6";
  size.dispatchEvent(new Event("input", { bubbles: true }));
  const start = read();
  document.getElementById("sizeUp").click();
  const afterUp = read();
  document.getElementById("sizeDown").click();
  document.getElementById("sizeDown").click();
  const afterDown = read();
  value.value = "200";
  value.dispatchEvent(new Event("input", { bubbles: true }));
  await new Promise((r) => setTimeout(r, 50));
  const afterType = read();
  // 越界值要被夹住 ✓（setBrushSize 是唯一入口 ✓）。
  value.value = "9999";
  value.dispatchEvent(new Event("input", { bubbles: true }));
  await new Promise((r) => setTimeout(r, 50));
  const afterClamp = read();
  return { start, afterUp, afterDown, afterType, afterClamp };
})()`);
check(flow.start.range === 6 && flow.start.out === 6, "起点两边都是 6", JSON.stringify(flow.start));
check(flow.afterUp.range === 7 && flow.afterUp.out === 7, "「＋」让两边都变 7", JSON.stringify(flow.afterUp));
check(flow.afterDown.range === 5 && flow.afterDown.out === 5, "「−」让两边都变 5", JSON.stringify(flow.afterDown));
check(flow.afterType.range === 200 && flow.afterType.out === 200, "直接输入 200 生效", JSON.stringify(flow.afterType));
check(flow.afterClamp.range === 512 && flow.afterClamp.out === 512, "超过 512 被夹到 512",
  JSON.stringify(flow.afterClamp));

console.log("");
if (failures.length) {
  console.error(`结论：新建尺寸与粗细控件不合格 ✗（${failures.length} 条）：`);
  for (const item of failures) console.error("   - " + item);
  process.exit(1);
}
console.log("结论：✓ 新建文档可指定尺寸（含 1920×1080）、粗细控件显眼且三路可控");
process.exit(0);
