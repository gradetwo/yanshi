#!/usr/bin/env node
// **WebSocket 跨站防护**判据（第三方代码审计 #9：升级处理从不看 `Origin` ⇒ CSWSH）。
//
// 三情形（正反都能红 ✓）：
//   ① `Origin: http://evil.example` ⇒ **必须拒绝**（不是 101）✗；
//   ② `Origin: http://127.0.0.1:<port>` ⇒ **必须 101**（同源/本机页面要能用 ✓）；
//   ③ **不带 `Origin`** ⇒ **必须 101**（探针 / curl / MCP 这类非浏览器客户端 ✓）。
// 用法：node scripts/server-ws-origin.mjs <host> <port>
import { createConnection } from "node:net";
import { createHash, randomBytes } from "node:crypto";
const [host, port] = process.argv.slice(2);
if (!host || !port) { console.error("用法: node scripts/server-ws-origin.mjs <host> <port>"); process.exit(2); }
const base = `http://${host}:${port}`;
const made = await fetch(base + "/api/documents", { method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: "wsorigin", width: 120, height: 90 }) }).then((r) => r.json());
if (!made.token) { console.error("拿不到 token ⇒ 判据无法运行（不是通过）"); process.exit(1); }
const handshake = (origin) => new Promise((resolve) => {
  const key = randomBytes(16).toString("base64");
  const socket = createConnection({ host, port: Number(port) }, () => {
    const lines = [
      "GET /ws?doc=wsorigin&token=" + made.token + " HTTP/1.1",
      `Host: ${host}:${port}`,
      "Upgrade: websocket",
      "Connection: Upgrade",
      `Sec-WebSocket-Key: ${key}`,
      "Sec-WebSocket-Version: 13",
    ];
    if (origin) lines.push("Origin: " + origin);
    socket.write(lines.join("\r\n") + "\r\n\r\n");
  });
  let data = "";
  socket.on("data", (chunk) => {
    data += chunk.toString("latin1");
    if (data.includes("\r\n\r\n")) { socket.destroy(); resolve(data.split("\r\n")[0]); }
  });
  socket.on("error", () => resolve("(连接错误)"));
  setTimeout(() => { socket.destroy(); resolve(data ? data.split("\r\n")[0] : "(超时)"); }, 2500);
});
let bad = 0;
const check = (label, ok, detail) => { console.log((ok ? "  ✓ " : "  ✗ ") + label + (detail ? "｜" + detail : "")); if (!ok) bad += 1; };
const evil = await handshake("http://evil.example");
check("跨站 Origin 必须被拒", !evil.includes("101"), evil);
const same = await handshake(`http://127.0.0.1:${port}`);
check("本机 Origin 必须放行（101）", same.includes("101"), same);
const none = await handshake(null);
check("不带 Origin 必须放行（101）", none.includes("101"), none);
console.log(bad ? `  结论：${bad} 条不成立 ✗` : "  结论：WebSocket 跨站防护成立 ✓");
process.exit(bad ? 1 : 0);
