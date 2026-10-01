// **通用驱动** ✓：页面侧代码放在单独的 .js 文件里 ✓ ⇒ 外层再也不用模板字符串 ✓
//（本会话我被"模板字符串里的反引号"绊倒四次 ✗ —— 从结构上根除 ✓，不靠记性 ✓）。
import { readFile } from "node:fs/promises";
const pageFile = process.argv[2];
const port = process.argv[3] || "8110";
const code = await readFile(pageFile, "utf8");
const cdp = process.env.YANSHI_CDP || "9333";
const list = await fetch("http://127.0.0.1:" + cdp + "/json/list").then((r) => r.json());
const ws = new WebSocket(list.filter((t) => t.type === "page")[0].webSocketDebuggerUrl);
let id = 1; const pending = new Map();
ws.addEventListener("message", (e) => { const m = JSON.parse(e.data); if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); } });
await new Promise((r) => ws.addEventListener("open", r));
const send = (method, params = {}) => new Promise((res) => { const c = id++; pending.set(c, res); ws.send(JSON.stringify({ id: c, method, params })); });
const evaluate = async (expression) => {
  const r = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (r && r.result && r.result.exceptionDetails) return { __error: String(r.result.exceptionDetails.text || "err") };
  return r && r.result && r.result.result ? r.result.result.value : undefined;
};
await send("Runtime.enable"); await send("Page.enable"); await send("Network.enable", {});
await send("Network.setCacheDisabled", { cacheDisabled: true });
const path = process.env.YANSHI_PATH || "/";
await send("Page.navigate", { url: "http://127.0.0.1:" + port + path });
await send("Page.bringToFront", {});
for (let i = 0; i < 120; i++) {
  const ready = await evaluate('document.querySelectorAll("#tools button").length > 0');
  if (ready) break;
  await new Promise((r) => setTimeout(r, 250));
}
await new Promise((r) => setTimeout(r, 800));
const out = await evaluate(code);
console.log(JSON.stringify(out, null, 2));
const shot = process.env.YANSHI_SHOT;
if (shot) {
  const image = await send("Page.captureScreenshot", { format: "png" });
  const { writeFile } = await import("node:fs/promises");
  await writeFile(shot, Buffer.from(image.result.data, "base64"));
  console.log("screenshot: " + shot);
}
ws.close(); process.exit(0);
