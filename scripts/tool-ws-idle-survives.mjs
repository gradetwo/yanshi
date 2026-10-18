// **WS 空闲**不许**被 HTTP 的空闲读超时掐断 ✗**（外部复测 QY-D05 的判据 ✓）
//
// **∴ 缺陷 ✓**：**∴ `server.rs:511` 给 TCP 流设了 30 秒**读超时**✗**（**∴ 为 HTTP keep-alive ✓**）
//   ⇒ **∴ 而**升级到 WS 时**把同一条流**交出去**✗** ⇒ **∴ 超时**没清**✗**
//   ⇒ **∴ 他们实测 ✓**：**∴ 空闲连接 3/3 在 **30.016～30.026 秒**✗ 收到 **TCP EOF**✗**（**∴ 无 close 帧 ✓**）
//     ⇒ **∴ 页面显示「已断开」✗ ＋ 重连 ✓**
// **∴ 修法 ✓**：**∴ 升级前 `set_read_timeout(None)` ✗** ⇒ **∴ WS 自己负责心跳／失活 ✓**
//
// **∴ 断言（一条，但它是**行为**✗）**：**∴ 连上 WS ✗ ⇒ **∴ 空闲 **35 秒**✗ ⇒ **∴ 不许断开 ✓**
//   ⇒ **∴ 35 > 30 ＋ 余量**✗ ⇒ **∴ 修前必红（30.0 秒）✗、修后必绿 ✓**
const base = process.argv[2] ?? "http://127.0.0.1:0";
const IDLE_MS = Number(process.argv[3] ?? 35000);
const stamp = Date.now();
const doc = `wsidle_${stamp}`;

const created = await fetch(`${base}/api/documents`, {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 400, height: 400 }),
}).then((r) => r.json()).catch(() => null);
if (!created?.token) {
  console.log("  ✗ 建文档失败（判据前提不成立）");
  process.exit(3);
}
const wsUrl = `${base.replace(/^http/, "ws")}/ws?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(created.token)}`;
console.log(`  ${doc} ⇒ 连上后空闲 ${(IDLE_MS / 1000).toFixed(0)} 秒（HTTP 的空闲读超时是 30 秒）`);

const started = Date.now();
const outcome = await new Promise((resolve) => {
  let done = false;
  const ws = new WebSocket(wsUrl);
  const finish = (value) => {
    if (done) return;
    done = true;
    try { ws.close(); } catch {}
    resolve(value);
  };
  ws.onopen = () => {};
  ws.onclose = () => finish({ closed: true, at: (Date.now() - started) / 1000 });
  ws.onerror = () => {};
  setTimeout(() => finish({ closed: false, at: (Date.now() - started) / 1000 }), IDLE_MS);
});

if (outcome.closed) {
  console.log(`  ✗ 第 ${outcome.at.toFixed(1)} 秒断开 ⇒ WS 继承了 HTTP 的空闲读超时`);
  console.log("  ✗ WS 空闲保活不合格");
  process.exit(1);
}
console.log(`  ✓ 空闲 ${outcome.at.toFixed(1)} 秒未断 ⇒ WS 不再继承 HTTP 的空闲读超时`);
