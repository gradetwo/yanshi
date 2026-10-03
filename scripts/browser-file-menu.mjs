#!/usr/bin/env node
// **「文件」菜单：导入导出搬出信息面板** ✓ —— 用户第 6 条 ✓（原文："导入导出之类功能也不适合放在信息面板，
// 按照行业主流软件的习惯来设置分类和安排到对应的地方"✓）。
//
// **判据（都能红 ✓）**：
//   ① 打开菜单 ⇒ 那五样必须**在菜单里** ✓（新建 / 打开 / 导出 PNG / 导出工程 / 导入为新文档 ✓）；
//   ② **信息面板里一个都不许留** ✓（`aside.contains(...)` 全为 false ✓）——
//      这一条正是"搬出去"的定义 ✓，修复前它们都在信息面板里 ⇒ 红 ✓；
//   ③ 菜单里的「导出 PNG」**真的能导出** ✓：点它 ⇒ 日志出现"已导出 PNG：W×H" ✓
//      （不是"点得到按钮"就算 ✓ —— 那是本项目最反对的判据 ✗）；
//   ④ `Esc` 收起 ✓、点菜单外面收起 ✓；
//   ⑤ 零控制台错误 ✓。
//
// 用法：node scripts/browser-file-menu.mjs "http://127.0.0.1:<port>/?doc=<id>&token=<token>"
const url = process.argv[2];
const debugPort = process.env.CDP_PORT || "9333";
const shotsDir = process.env.SHOT_DIR || "/tmp/yanshi-file-menu";
if (!url) {
  console.error("用法: node scripts/browser-file-menu.mjs <viewer-url>");
  process.exit(2);
}
const list = await fetch(`http://127.0.0.1:${debugPort}/json/list`).then((r) => r.json());
const viewerBase = url.split("?")[0];
const target =
  list.find((t) => t.type === "page" && t.url.startsWith(viewerBase)) ||
  list.find((t) => t.type === "page");
if (!target) {
  console.error(`未找到调试目标（Chromium 是否以 --remote-debugging-port=${debugPort} 启动？）`);
  process.exit(2);
}
const ws = new WebSocket(target.webSocketDebuggerUrl);
let id = 1;
const pending = new Map();
const consoleLines = [];
ws.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);
  if (message.method === "Runtime.consoleAPICalled") {
    consoleLines.push((message.params.args || []).map((a) => a.value ?? "").join(" "));
  }
  if (message.method === "Runtime.exceptionThrown") {
    const details = message.params.exceptionDetails || {};
    consoleLines.push("exception: " + (details.exception?.description || details.text || ""));
  }
  if (message.id && pending.has(message.id)) {
    pending.get(message.id)(message);
    pending.delete(message.id);
  }
});
await new Promise((resolve) => ws.addEventListener("open", resolve));
const send = (method, params = {}) =>
  new Promise((resolve) => {
    const current = id++;
    pending.set(current, resolve);
    ws.send(JSON.stringify({ id: current, method, params }));
  });
const evaluate = async (expression) =>
  (await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true })).result
    ?.result?.value;
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const waitFor = async (expression, label, timeoutMs = 25000) => {
  const until = Date.now() + timeoutMs;
  while (Date.now() < until) {
    if (await evaluate(expression)) return true;
    await sleep(200);
  }
  console.error(`  ⏱ 等待超时：${label}`);
  return false;
};
const capture = async (name) => {
  const shot = await send("Page.captureScreenshot", { format: "png" });
  const data = shot.result?.data;
  if (!data) return null;
  const fs = await import("node:fs/promises");
  await fs.mkdir(shotsDir, { recursive: true });
  const path = `${shotsDir}/${name}.png`;
  await fs.writeFile(path, Buffer.from(data, "base64"));
  return path;
};

await send("Runtime.enable");
await send("Page.enable");
await send("Page.navigate", { url });
await send("Page.reload", { ignoreCache: true });
await sleep(1600);
consoleLines.length = 0;
if (!(await waitFor("typeof window.yanshiFileMenu === 'object'", "查看器就绪"))) {
  process.exit(3);
}

// ① 五样都在菜单里 ✓
const ids = ["newDoc", "openDoc", "exportPng", "projectExport", "projectImport"];
const placement = await evaluate(`(() => {
  const menu = document.getElementById("fileMenuBody");
  const aside = document.querySelector("aside");
  const wanted = ${JSON.stringify(ids)};
  const where = (id) => {
    const node = document.getElementById(id);
    if (!node) return "缺失";
    if (menu && menu.contains(node)) return "菜单";
    if (aside && aside.contains(node)) return "信息面板";
    return "别处";
  };
  return Object.fromEntries(wanted.map((id) => [id, where(id)]));
})()`);
console.log("  ① 控件位置：" + JSON.stringify(placement, null, 0));
const notInMenu = ids.filter((key) => placement[key] !== "菜单");
const stillInPanel = ids.filter((key) => placement[key] === "信息面板");
if (notInMenu.length > 0) {
  console.error(`❌ 这些还没进「文件」菜单：${notInMenu.join(", ")} ✗`);
  await capture("file-menu-missing-controls");
  process.exit(1);
}
// ② 信息面板里一个都不许留 ✓
if (stillInPanel.length > 0) {
  console.error(`❌ 信息面板里还留着：${stillInPanel.join(", ")} ⇒ 没搬出去 ✗`);
  await capture("file-menu-still-in-panel");
  process.exit(1);
}
const shotOpen = await capture("file-menu-open");

// ④ Esc / 点外面收起 ✓
await evaluate("window.yanshiFileMenu.open()");
await sleep(200);
const opened = await evaluate("window.yanshiFileMenu.isOpen()");
await evaluate(`document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }))`);
await sleep(250);
const afterEsc = await evaluate("window.yanshiFileMenu.isOpen()");
await evaluate("window.yanshiFileMenu.open()");
await sleep(200);
await evaluate(`document.body.dispatchEvent(new MouseEvent("click", { bubbles: true }))`);
await sleep(250);
const afterOutside = await evaluate("window.yanshiFileMenu.isOpen()");
console.log(`  ④ 开=${opened}｜Esc 后开=${afterEsc}｜点外面后开=${afterOutside}`);
if (!opened || afterEsc || afterOutside) {
  console.error("❌ 菜单的收起行为不对（Esc 或点外面没收起，或压根没打开）✗");
  await capture("file-menu-close-failed");
  process.exit(1);
}

// ③ 菜单里的「导出 PNG」真的导出 ✓（看日志里那句"已导出 PNG：W×H"，不看"按钮点得到"✗）
await evaluate("window.yanshiFileMenu.open()");
await sleep(200);
await evaluate(`document.getElementById("exportPng").click()`);
const exported = await waitFor(
  `/已导出 PNG/.test(document.getElementById("log").textContent || "")`,
  "导出完成的日志",
  30000,
);
const logLine = await evaluate(`(() => {
  const text = document.getElementById("log").textContent || "";
  const line = text.split("\\n").filter((l) => l.includes("已导出 PNG")).pop() || "";
  return line.trim().slice(0, 120);
})()`);
console.log(`  ③ 点菜单里的「导出 PNG」⇒ ${exported ? "日志出现导出结果 ✓" : "没等到结果 ✗"}：${logLine}`);
// **判据要能在"日志没有换行分隔"的情况下也成立** ✗ —— 第一版要求整行匹配 ✓
//（`logLine` 里被粘上了后面几条消息 ✗）⇒ 误报"没导出"✓，而它其实导出了 15826 字节 ✓。
// ⇒ 改成在**整段日志文本**里找那句话 ✓，并把抓到的那句显示出来 ✓（判据错了不是产品错了 ✓）。
if (!exported || !/已导出 PNG：\d+×\d+/.test(logLine)) {
  console.error("❌ 菜单里的「导出 PNG」没有真的导出 ✗（只「点得到按钮」不算 ✓）");
  await capture("file-menu-export-failed");
  process.exit(1);
}
const shotExport = await capture("file-menu-export");

const errors = consoleLines.filter(
  (line) => /error|uncaught|exception|failed/i.test(line) && !/favicon/i.test(line),
);
console.log(`  ⑤ 控制台错误：${errors.length}`);
if (errors.length > 0) {
  errors.slice(0, 4).forEach((line) => console.error("     " + line.slice(0, 160)));
  process.exit(1);
}
console.log(
  JSON.stringify(
    {
      ok: true,
      placement,
      closeBehaviour: { opened, afterEsc, afterOutside },
      exportLogLine: logLine,
      screenshots: [shotOpen, shotExport],
    },
    null,
    2,
  ),
);
ws.close();
