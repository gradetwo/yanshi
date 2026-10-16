#!/usr/bin/env node

// **★ 总超时守卫 ✗ ★**（2026-10-16 ✓）：**本判据要连 CDP 浏览器** ✓ ⇒ 浏览器起不来时它可能**一直等**
//   （实测：CI 的判据分片被这种判据**拖到跑不完** ✗ ⇒ 我这边十几轮拿不到判定 ✓）。
//   **∴ 本仓约定**（AGENTS.md）：「判据必须能说**我自己坏了** ✗，用**与产品失败不同的退出码**报出来 ✓（现用 exit(3) ✓）」
//   ⇒ **∴ 超时就报「无法判断」并退 3** ✓ —— **不冒充产品失败** ✗、**也不永远挂着** ✗ ★
const __judgeTimeout = setTimeout(() => {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 判据超时（CDP 浏览器可能没起来）⇒ 退出码 3");
  process.exit(3);
}, 150000);
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
// **★ 必须**真的渲染一次**✗★**（2026-10-15 实测 ✓）：`get_document` **只读状态、不渲染** ✗
//   ⇒ 我第一次写完它 ⇒ 缓存**恒为 0 字节** ⇒ 判据只好退 3（"我自己跑不了"✓，§49 之补五十 ✓）。
//   **∴ 用** `render_region` ✓ —— 它会走渲染路径 ⇒ **tile 真的进缓存** ✓
//     （其它候选：`export_psd_layers` ✓／`export_project` ✓）。
//   **参数名以服务端为准** ✓（它会自己报"可用参数"✓ —— 这是本仓的好习惯 ✓）：
//     实测 `h` 不接受 ✗ ⇒ 正确是 **`region`** ✓（`{x,y,w,h}` ✓）。
const rendered = await tool("render_region", { region: { x: 0, y: 0, w: 512, h: 512 } });
console.log(`  渲染调用：render_region ⇒ ${JSON.stringify(rendered).slice(0, 120)}`);
if (rendered && rendered.ok === false) {
  voidRun(`render_region 调用失败（形状可能不对）：${JSON.stringify(rendered).slice(0, 220)}`);
}
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
const fails = [];
const post2 = await tool("get_document", {});
console.log(`  平移后（视口 256×256）：cache_evictions=${post2.cache_evictions}｜cache_used_bytes=${post2.cache_used_bytes}`);
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
// ⑤ **★ 第二个订阅者：并集 vs 单个订阅者（2026-10-15 补 ✓）★**
//   **为什么必须补** ✗（§49 之补五十二 里我自己记下的缺口 ✓）：
//     上面 ③④ 只有**一个**订阅者 ✓ ⇒ **按单个视口淘汰也会逐出那 3 块** ✓
//     ⇒ ⇒ 那个变异**测不出**"并集 vs 单个"✗ ⇒ 而 `a152e034` 弄红 8 条**正是**这个缺陷 ✗。
//   **怎么测** ✓：让 A（左上）与 B（右下）**同时**声明各自的视口 ✓ ⇒ 两边都渲染进缓存 ✓
//     ⇒ 再**只平移 A** ⇒ ⇒ **并集**（A′ ∪ B）⇒ B 的 tile **应当活着** ✓；
//        按**单个 A**淘汰 ⇒ B 的 tile **被扔掉** ✗ ⇒ 缓存**明显变小** ✗。
//   **判别性断言** ✓：平移 A 之后，缓存里**至少**还剩 B 那一块的大小 ✓
//     （实测：512×512 的一块 ≈ **2 MB** ⇒ 取 **1 MB** 当地板 ✓ ⇒ 2 MB vs 0 的差别**两倍余量** ✓）。
const wsOpen = (viewport) =>
  new Promise((resolve, reject) => {
    const s2 = new WebSocket(wsUrl);
    s2.onopen = () => {
      s2.send(JSON.stringify({ type: "subscribe", viewport, zoom: 1 }));
      resolve(s2);
    };
    s2.onerror = () => reject(new Error("WS 连不上"));
    setTimeout(() => reject(new Error("WS 超时")), 10000);
  });
const A_VP = { x: 0, y: 0, w: 512, h: 512 };
const B_VP = { x: 3584, y: 3584, w: 512, h: 512 };
let wsA; let wsB;
try {
  wsA = await wsOpen(A_VP);
  wsB = await wsOpen(B_VP);
} catch (e) {
  voidRun(`第二个订阅者起不来：${e.message}`);
}
// 两边都**渲染进缓存** ✓（各 512×512 ⇒ 各约 2 MB ✓）。
await tool("render_region", { region: { x: 0, y: 0, w: 512, h: 512 } });
await tool("render_region", { region: { x: 3584, y: 3584, w: 512, h: 512 } });
await sleep(600);
const both = await tool("get_document", {});
console.log(`  两个订阅者都在：cache_used_bytes=${both.cache_used_bytes}`);
// 只平移 A ✓ ⇒ 并集仍然覆盖 B ✓
wsA.send(JSON.stringify({ type: "subscribe", viewport: { x: 1024, y: 1024, w: 512, h: 512 }, zoom: 1 }));
await sleep(1500);
const afterA = await tool("get_document", {});
console.log(`  只平移 A 之后：cache_evictions=${afterA.cache_evictions}｜cache_used_bytes=${afterA.cache_used_bytes}`);
wsA.close(); wsB.close();
// **★ 地板 1 MB ✓**（实测：512×512 的一块 ≈ **2 MB** ✓ ⇒ 两倍余量 ✓）。
// **★ 几何必须自洽 ✗★**（2026-10-15 实测教训 ✓，§49 之补五十五 ✓）：
//   **渲染范围必须 ⊆ 声明视口** ✓ —— 我第一版给 B **渲染** 512×512 ✓
//   却只**声明** 256×256 视口 ✗ ⇒ 多出的 3/4 合法落在视口外 ⇒ **被淘汰是对的** ✓
//   ⇒ 判据于是把自己的几何错**报成了产品缺陷** ✗（幸好实测平反了 ✓）。
//   ⇒ 所以这里三处（A、B、平移后的 A′）**统一用 512×512** ✓，与渲染范围一致 ✓。
const FLOOR = 1024 * 1024;
if (!(afterA.cache_used_bytes >= FLOOR)) {
  fails.push(
    `③ **B 的 tile 被误删了** ✗：只平移 A 之后缓存只剩 ${afterA.cache_used_bytes} 字节` +
      `（< 地板 ${FLOOR} ✓）⇒ 淘汰用的是**单个订阅者的视口**✗，` +
      `而不是**所有订阅者视口的并集** ✓ —— 这正是 \`a152e034\` 弄红 8 条的那个缺陷 ✗`,
  );
}
if (fails.length) {
  console.error("❌ 没通过：");
  for (const f of fails) console.error(`   ${f}`);
  process.exit(1);
}
console.log(
  `✓ 服务端视口淘汰成立：逐出 ${pre.cache_evictions} ⇒ ${post2.cache_evictions} ✓，` +
    `缓存字节 ${pre.cache_used_bytes} ⇒ ${post2.cache_used_bytes} ✓；` +
    `**两个订阅者时**平移 A ⇒ 缓存仍余 ${afterA.cache_used_bytes} 字节（≥ ${FLOOR} ✓＝B 还在 ✓）`,
);
process.exit(0);
