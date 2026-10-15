#!/usr/bin/env node
// **★ 服务端 tile 缓存的**视口淘汰**（2026-10-15 ✓；乙(iii) 的验收判据 ✓）★**
//
// **为什么需要它** ✗（`docs/design/vermeer-acceptance.md` §49 之补四十七 ✓）：
//   唯一提到"视口淘汰"的判据是 `browser-viewport-evict.mjs` ✓，而它测的是**客户端内核** ✓
//   ⇒ **服务端**那条（`TileCache::evict_outside_viewport` ✓）**没有任何判据守着** ✗
//   ⇒ ⇒ 而乙(iii) 的账目（"4K 下每组约 256 MB"✓）指的正是**服务端**这份缓存 ✓。
//
// **判什么** ✓：把 tile **先渲染进缓存** ✓ ⇒ 再用 **WS 的 `subscribe` 消息**声明一个
//   **不同的小视口** ✓ ⇒ 断言 `get_document` 的读数：
//     `cache_evictions` **上升** ✓ 且 `cache_used_bytes` **下降** ✓。
//   **数据来源** ✓：`crates/yanshi-server/src/tools.rs` 的 `read_get_document`
//     暴露的 `cache_evictions`／`cache_used_bytes` ✓（`1499916d` ✓）——
//     **不必**新增 API ✓、**不必**读 /proc ✓ ⇒ 能当 `tool-*` 判据跑 ✓。
//
// **变异（能红的根据 ✓）**：去掉接线（或按**单个**订阅者视口淘汰 ✓）⇒ 读数不动 ⇒ **红** ✓。
//
// 用法：node scripts/tool-viewport-evict-server.mjs <base-url>
const base = process.argv[2];
if (!base) {
  console.error("用法: node scripts/tool-viewport-evict-server.mjs <base-url>");
  process.exit(2);
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const die = (code, msg) => { console.error(msg); process.exit(code); };
// **判据自身的失败通道** ✓（照本仓约定：退 3 ＝ "我自己跑不了"✗，不是产品失败 ✓）。
const voidRun = (msg) => die(3, `❌ 判不了：${msg}（这不是产品问题）`);

const post = async (path, body) => {
  const r = await fetch(base + path, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  return r.json();
};

// ① 建一份**够大**的文档 ✓（4096 ✓ ⇒ 视口与缓存都有余量 ✓）。
const made = await post("/api/documents", { doc_id: "vpev", width: 4096, height: 4096 });
const token = made && made.token;
if (!token) voidRun(`建文档失败：${JSON.stringify(made).slice(0, 200)}`);
const url = (p) => `${base}${p}?doc=vpev&token=${token}`;
const tool = async (name, args) => post(`/api/tools?doc=vpev&token=${token}`, { tool: name, arguments: args });

// ② 落一笔并**整幅渲染一次** ✓ ⇒ 让 tile **真的进缓存** ✓（否则 `used_bytes` 恒为 0 ✓，判据没意义 ✗）。
await tool("brush_stroke", { points: [[200, 200], [1200, 900]], brush: "round", color: "#204060" });
const pre = await tool("get_document", {});
if (pre && pre.ok === false) voidRun(`get_document 失败：${JSON.stringify(pre).slice(0, 200)}`);
if (pre.cache_used_bytes === undefined) {
  voidRun("响应里没有 `cache_used_bytes` ⇒ 读数没接线（或跑的不是新二进制）");
}
console.log(`  渲染前：cache_evictions=${pre.cache_evictions}｜cache_used_bytes=${pre.cache_used_bytes}`);
if (!(pre.cache_used_bytes > 0)) {
  voidRun(`渲染之后缓存仍为 0 字节 ⇒ 这一步**没让 tile 进缓存** ⇒ 判据此刻测不了`);
}

// ③ 用 **WS 的 `subscribe` 消息**换个**小视口** ✓ —— 这正是客户端声明视口的那条路 ✓
//    （`crates/yanshi-http/src/server.rs` 的 `"subscribe"` 分支 ✓）。
const wsUrl = url("/ws").replace(/^http/, "ws");
const socket = new WebSocket(wsUrl);
await new Promise((open, fail) => {
  socket.onopen = open;
  socket.onerror = () => fail(new Error("WS 连不上"));
  setTimeout(() => fail(new Error("WS 超时")), 10000);
}).catch((e) => voidRun(`WS 失败：${e.message}`));
// **声明一个与整幅不同的视口** ✓ ⇒ 视口外的 tile 应当被淘汰 ✓。
socket.send(JSON.stringify({ type: "subscribe", viewport: { x: 0, y: 0, w: 256, h: 256 }, zoom: 1 }));
await sleep(1500);
socket.close();

// ④ 断言 ✓
const post2 = await tool("get_document", {});
console.log(`  平移后（视口 256×256）：cache_evictions=${post2.cache_evictions}｜cache_used_bytes=${post2.cache_used_bytes}`);
const fails = [];
if (!(post2.cache_evictions > pre.cache_evictions)) {
  fails.push(
    `① **没有发生任何视口淘汰** ✗：cache_evictions ${pre.cache_evictions} ⇒ ${post2.cache_evictions}` +
      ` ⇒ 服务端的 \`evict_outside_viewport\` 仍然**没被调用** ✗（或读数没接上 ✗）`,
  );
}
if (!(post2.cache_used_bytes < pre.cache_used_bytes)) {
  fails.push(
    `② **缓存字节没降** ✗：cache_used_bytes ${pre.cache_used_bytes} ⇒ ${post2.cache_used_bytes}` +
      ` ⇒ 淘汰没生效 ✗（或视口外的 tile 本来就不在缓存里 ✓ —— 那要调整测试的视口与渲染范围 ✓）`,
  );
}
if (fails.length) {
  console.error("❌ 没通过：");
  for (const f of fails) console.error(`   ${f}`);
  process.exit(1);
}
console.log(
  `✓ 服务端视口淘汰成立：逐出 ${pre.cache_evictions} ⇒ ${post2.cache_evictions} ✓，` +
    `缓存字节 ${pre.cache_used_bytes} ⇒ ${post2.cache_used_bytes} ✓`,
);
process.exit(0);
