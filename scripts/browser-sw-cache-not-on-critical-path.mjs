#!/usr/bin/env node
// criterion-target: client  ## 运行器据此注入 render=client（测本地内核）
// **★ 缓存记账不许站在"已经拿到答案"与"把答案交给页面"中间 ★**（第八轮 ✓）
//
// **⇒ 缺陷怎么发现的** ✓：第七轮我把"SW 写缓存失败会不会让导航挂住"记成**未证明的假设** ✗
//   这一轮先用 sabotage 探针问了一遍（实测记录见 `docs/surface-alignment.md` 登记 ㊹）✓：
//   · `Cache.prototype.put` 永不 settle ⇒ 导航 **122 ms 照样 commit** ✓
//     ⇒ 我原先猜的"写 tee 反压住导航"**被证伪** ✗；
//   · `CacheStorage.prototype.open` 永不 settle ⇒ 导航**停在半路 ✓ 21 s 都没有 commit** ✗
//     根因就是旧代码里那句 `const cache = await caches.open(CACHE)` ✓ 它夹在
//     `const response = await fetch(request)` 与 `return response` **中间** ✓
//     ⇒ 网络响应已经拿到了 ✓ 却因为"**开一个缓存**"而不给页面 ✓ 而页面上一句责任都没有 ✗；
//   · `open` 直接抛 `QuotaExceededError` ⇒ 导航 commit ✓（外层 `catch` 走 `caches.match` 兜住了 ✓）
//     ⇒ **抛错不致命 ✓ 挂起才致命** ✓ —— 而 ㊵ 那个现场（配额／Cache Storage 目录不可用）
//       两种都可能 ✓ 所以这条**只能靠"别把它放在关键路径上"来守** ✗ try/catch 守不住挂起 ✓。
//
// **⇒ 本判据问什么** ✓（只问产品行为 ✗ 不问"我有没有写新 API"✓）：
//   ① `open` 挂起时导航仍然 commit ✓ 且 commit 的是**真外壳**（不是错误页／空白页）✓；
//   ② `open` 挂起时一次 blob 请求仍在有界时间内返回 ✓（blob 那一支是 cache-first ✓
//      "读"本来就在回答之前 ⇒ 没法挪后 ✓ 修法是有界等待 ✓ 所以这里守的是"**不永远挂着**"✓）；
//   ③ `put` 挂起时导航照样 commit ✓；
//   ④ **不 sabotage 时写入照旧落地** ✓ ＋ 没有静默写失败 ✓
//      （防止有人把修法做成"干脆不写缓存"✗ 那会让 ①②③ 全部假绿 ✓）。
//
// **⇒ 反假绿设计** ✓（本判据最容易骗自己的地方 ✗）：
//   sabotage 打在 **SW 自己的全局**上 ✓ 只活在**那个 worker 实例**里 ✓ ⇒
//   worker 若中途被回收 ✓ `open` 就**悄悄变回原生** ✓ 导航当然 commit ✓
//   ⇒ 那是**仪器坏了**不是产品好 ✓ 所以**打完读回一次 ✓ 测完再读回一次** ✓
//   任何一次不是"被换的实现"就判"无法判断"退 3 ✗ 并且还原后再测一次导航 ✓
//   不 commit 说明影响跨轮残留 ✗ 同样退 3（不冒充产品失败 ✓）。
//   退出码约定（AGENTS.md）：0 绿 ✓ 1 产品红 ✗ **3 判据自己的前提不成立** ✗
//
// **⇒ 变异（自我证明）** ✓：第八轮**在修复前的那一份构建上跑 ⇒ ①②③ 就是红的** ✓
//   把 `open`/`put` 挪出响应路径后转绿 ✓（修复前的红 = 现成的变异证据 ✓ 不必再造）。
//
// 用法：node scripts/browser-sw-cache-not-on-critical-path.mjs <viewer-url> [base] [token] [cdpPort]
const guard = setTimeout(() => {
  console.error("❌ 判据自身超时（连不上 CDP／浏览器没起来 ⇒ 不是产品问题）⇒ 退出码 3");
  process.exit(3);
}, 200000);

const viewer = process.argv[2];
const base = process.argv[3] || (viewer ? new URL(viewer).origin : "http://127.0.0.1:15900");
const port = process.argv[5] || process.env.CDP_PORT || "9333";
const NAV_COMMIT_MS = 10000; // 产品该在几百毫秒内 commit ✓ 给 10 s 是**判据的耐心** ✗ 不是标准
const BLOB_RETURN_MS = 8000; // blob 那一支的有界等待实测 1.5 s ⇒ 给 8 s ✓ 再慢就算"挂住"✗
const PROBE_ASSET = "/viewer.css"; // 外壳真的会拉它 ✓（不是我以为的 URL ✗ 下面还会核对状态码）

let failed = 0;
let voided = null;
const say = (ok, label, detail) => {
  console.log(`  ${ok ? "✓" : "✗"} ${label}${detail ? `（${detail}）` : ""}`);
  if (!ok) failed += 1;
  return ok;
};
const premise = (ok, label, detail) => {
  console.log(`  ${ok ? "✓" : "✗"} [前提] ${label}${detail ? `（${detail}）` : ""}`);
  if (!ok && !voided) voided = label;
  return ok;
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function main() {
  const version = await fetch(`http://127.0.0.1:${port}/json/version`).then((r) => r.json()).catch(() => null);
  if (!version || !version.webSocketDebuggerUrl) {
    console.error(`无法判断：CDP 没起来（${port}）⇒ 退出码 3`);
    process.exit(3);
  }
  const ws = new WebSocket(version.webSocketDebuggerUrl);
  let msgId = 0;
  const pending = new Map();
  const eventWaiters = [];
  ws.addEventListener("message", (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.id && pending.has(msg.id)) { pending.get(msg.id)(msg); pending.delete(msg.id); }
    for (let i = eventWaiters.length - 1; i >= 0; i -= 1) {
      const w = eventWaiters[i];
      if (w.sessionId && msg.sessionId !== w.sessionId) continue;
      if (w.method === msg.method) { w.resolve(msg); eventWaiters.splice(i, 1); }
    }
  });
  await new Promise((res, rej) => { ws.addEventListener("open", res); ws.addEventListener("error", rej); });
  const send = (method, params = {}, sessionId) => new Promise((res) => {
    const id = ++msgId;
    pending.set(id, res);
    ws.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }));
  });
  // **CDP 的 `Runtime.evaluate` 是双层 result** ✗（取错层 ⇒ 永远 `undefined` ✓
  // 第八轮的探针就是这么把"sabotage 到底有没有生效"看成"没生效"的 ✓）
  const value = (r) => {
    const inner = (r && r.result) || {};
    if (inner.exceptionDetails) return { __throw: JSON.stringify(inner.exceptionDetails).slice(0, 200) };
    return (inner.result || {}).value;
  };
  const openTabs = [];
  // **先声明再赋值** ✓：`finish()` 在前两条前提失败时就会被调用 ✓ 那时 `swEval` 还没定义 ✗
  // ⇒ 直接 `const swEval = …` 会让 `finish` 踩到 TDZ ✓ 把"前提不成立"报成"判据抛异常"✗
  let swEval = null;

  // **每个场景都用新标签** ✓：同一标签重复 `Page.navigate` 会被"同文档导航"短路 ✓
  // 而新标签才真的走 SW 的 `fetch` 分支 ✗（旧标签可能命中浏览器的文档缓存 ⇒ 假绿 ✓）
  async function navigateFresh(label) {
    const t = await send("Target.createTarget", { url: "about:blank" });
    const targetId = t.result.targetId;
    openTabs.push(targetId);
    const s = (await send("Target.attachToTarget", { targetId, flatten: true })).result.sessionId;
    await send("Runtime.enable", {}, s);
    await send("Page.enable", {}, s);
    const t0 = Date.now();
    const done = new Promise((res) => {
      eventWaiters.push({ method: "Page.frameStoppedLoading", sessionId: s, resolve: () => res("event") });
      setTimeout(() => res(null), NAV_COMMIT_MS);
    });
    // **`Page.navigate` 自己也要 race** ✓：SW 不返回响应时这个调用**不 resolve** ✗
    //（第八轮的探针就挂在这里 ✓ 于是整体超时把 sabotage 留在了运行中的浏览器里 ✗）
    const nav = send("Page.navigate", { url: base + "/" }, s);
    const how = await Promise.race([done, sleep(NAV_COMMIT_MS + 3000).then(() => null)]);
    await Promise.race([nav, sleep(2000).then(() => undefined)]);
    const elapsed = Date.now() - t0;
    const ev = async (expression) => value(await send("Runtime.evaluate",
      { expression, returnByValue: true, awaitPromise: true }, s));
    const out = { how, elapsed, ready: null, shell: null, blob: null, assetStatus: null };
    if (how) {
      out.ready = await ev("document.readyState");
      out.shell = await ev("(!!document.getElementById('effectsList') && document.title.length > 0)");
      // **blob 与静态资产都在这个标签里问** ✓：blob 走 SW 的另一支（cache-first ✓）
      out.blob = await ev("(async()=>{const t0=Date.now();try{const r=await fetch('/api/blob/0000000000000000000000000000000000000000');" +
        "return {settled:true,status:r.status,ms:Date.now()-t0};}catch(e){return {settled:'threw',ms:Date.now()-t0,error:String(e&&e.message)};}})()");
      out.assetStatus = await ev("(async()=>{try{const r=await fetch(" + JSON.stringify(PROBE_ASSET) +
        ");return r.status;}catch(e){return 'threw';}})()");
    }
    await send("Target.closeTarget", { targetId }).catch(() => undefined);
    openTabs.splice(openTabs.indexOf(targetId), 1);
    console.log(`  ${label}：commit=${how ? "✓" : "✗（未 commit）"} ${elapsed}ms readyState=${out.ready} ` +
      `外壳在=${out.shell} blob=${JSON.stringify(out.blob)} 资产=${out.assetStatus}`);
    return out;
  }

  const SAB = {
    OPEN_HANG: "(()=>{if(!('__o' in CacheStorage.prototype)){CacheStorage.prototype.__o=CacheStorage.prototype.open;}" +
      "CacheStorage.prototype.open=function(){return new Promise(()=>{});};return 'patched';})()",
    PUT_HANG: "(()=>{if(!('__p' in Cache.prototype)){Cache.prototype.__p=Cache.prototype.put;}" +
      "Cache.prototype.put=function(){return new Promise(()=>{});};return 'patched';})()",
    RESTORE: "(()=>{if(Cache.prototype.__p){Cache.prototype.put=Cache.prototype.__p;delete Cache.prototype.__p;}" +
      "if(CacheStorage.prototype.__o){CacheStorage.prototype.open=CacheStorage.prototype.__o;delete CacheStorage.prototype.__o;}" +
      "return 'restored';})()",
  };
  const READBACK = {
    OPEN_HANG: "String(CacheStorage.prototype.open).slice(0,60)",
    PUT_HANG: "String(Cache.prototype.put).slice(0,60)",
  };
  const isSabotaged = (text) => typeof text === "string" && text.indexOf("[native code]") < 0;

  console.log("── 前提：这台浏览器上真的有已激活的 SW ──");
  const ctrl = await navigateFresh("对照导航（不 sabotage）");
  if (!premise(Boolean(ctrl.how && ctrl.ready === "complete"), "不 sabotage 时导航能 commit",
    ctrl.how ? `${ctrl.elapsed}ms` : "连对照都没 commit ⇒ 环境有问题，先修仪器")) { await finish(); return; }
  say(ctrl.shell === true, "⓪对照拿到的是真外壳（有 `#effectsList` ✓ 有 title ✓）", `shell=${ctrl.shell}`);
  await sleep(1200);
  const swTargets = (await send("Target.getTargets")).result.targetInfos.filter((x) => x.type === "service_worker");
  if (!premise(swTargets.length > 0, "拿得到 SW 调试目标", "导航已 commit ✓ 还拿不到 ⇒ 仪器问题")) { await finish(); return; }
  const swSid = (await send("Target.attachToTarget", { targetId: swTargets[0].targetId, flatten: true })).result.sessionId;
  swEval = (expression) => send("Runtime.evaluate",
    { expression, returnByValue: true, awaitPromise: true }, swSid).then(value);

  console.log("\n── `CacheStorage.open` 挂起（旧代码就是把导航扣在这里 ✗）──");
  console.log("  patch ⇒", JSON.stringify(await swEval(SAB.OPEN_HANG)));
  const appliedText = await swEval(READBACK.OPEN_HANG);
  if (!premise(isSabotaged(appliedText), "sabotage 确实打上了（读回不是原生实现）", JSON.stringify(appliedText))) {
    await swEval(SAB.RESTORE); await finish(); return;
  }
  const hung = await navigateFresh("open 挂起时");
  const stillText = await swEval(READBACK.OPEN_HANG);
  if (!premise(isSabotaged(stillText), "测量结束时 sabotage 仍在（worker 没被回收 ⇒ 那一格不是假绿）",
    JSON.stringify(stillText))) { await swEval(SAB.RESTORE); await finish(); return; }
  await swEval(SAB.RESTORE);
  const restoredText = String(await swEval(READBACK.OPEN_HANG));
  premise(restoredText.indexOf("[native code]") >= 0, "已还原成原生实现", JSON.stringify(restoredText.slice(0, 44)));
  const recheck = await navigateFresh("还原后复测");
  if (!premise(Boolean(recheck.how), "还原后导航恢复 commit（影响不残留 ⇒ 上面那格可信）",
    "仍不 commit ⇒ 环境已污染，结论作废")) { await finish(); return; }

  console.log("\n── 产品断言 ──");
  say(Boolean(hung.how), "①`open` 挂起时导航仍然 commit（缓存不许挡在响应前面）",
    hung.how ? `${hung.elapsed}ms ✓` : `✗ ${NAV_COMMIT_MS}ms 内没有 commit（修复前实测：21s 无 commit）`);
  say(Boolean(hung.how) && hung.ready === "complete" && hung.shell === true,
    "②挂起时 commit 的是真外壳（不是错误页／空白页）", `readyState=${hung.ready} 外壳在=${hung.shell}`);
  const blobMs = hung.blob && typeof hung.blob.ms === "number" ? hung.blob.ms : null;
  say(Boolean(hung.how) && hung.blob && hung.blob.settled === true && blobMs !== null && blobMs < BLOB_RETURN_MS,
    "③`open` 挂起时一次 blob 请求仍在有界时间内返回（有界等待生效）",
    hung.blob ? `settled=${hung.blob.settled} status=${hung.blob.status} ${blobMs}ms` : "✗ 压根没拿到");

  console.log("\n── `Cache.put` 挂起（对照：写缓存更不许扣住响应）──");
  console.log("  patch ⇒", JSON.stringify(await swEval(SAB.PUT_HANG)));
  const putApplied = isSabotaged(await swEval(READBACK.PUT_HANG));
  const putHung = await navigateFresh("put 挂起时");
  const putStill = isSabotaged(await swEval(READBACK.PUT_HANG));
  await swEval(SAB.RESTORE);
  if (premise(putApplied && putStill, "sabotage 全程都在（打前测后各读回一次）",
    `打前=${putApplied} 测后=${putStill}`)) {
    say(Boolean(putHung.how), "④`put` 挂起时导航照样 commit", putHung.how ? `${putHung.elapsed}ms ✓` : "✗ 没 commit");
  }

  console.log("\n── 反向断言：不 sabotage 时写入照旧落地（防止修法变成「干脆不缓存」）──");
  let cached = null;
  for (let i = 0; i < 6; i += 1) {
    cached = await swEval("(async()=>{const c=await caches.open(CACHE);" +
      `const r=await c.match(${JSON.stringify(base + PROBE_ASSET)},{ignoreSearch:true});` +
      "return r?{hit:true,status:r.status}:{hit:false};})()");
    if (cached && cached.hit) break;
    await sleep(600);
  }
  const writeFailures = await swEval("(self.__swWriteFailures||[]).slice(0,4)");
  say(Boolean(cached && cached.hit), "⑤用过的资产真的进了缓存（挪到 `waitUntil` 没把写入弄丢）", JSON.stringify(cached));
  say(Array.isArray(writeFailures) && writeFailures.length === 0,
    "⑥没有静默的写失败（`__swWriteFailures` 为空）", JSON.stringify(writeFailures).slice(0, 160));

  async function finish() {
    if (swEval) await swEval(SAB.RESTORE).catch(() => undefined);
    for (const id of openTabs.slice()) { await send("Target.closeTarget", { targetId: id }).catch(() => undefined); }
    clearTimeout(guard);
    try { ws.close(); } catch (e) {}
    if (voided) {
      console.log(`\n✗ 无法判断：判据自己的前提不成立（${voided}）⇒ 退出码 3（**不是产品失败**）`);
      process.exit(3);
    }
    if (failed) {
      console.log(`\n✗ ${failed} 条没成立 ⇒ 退出码 1（产品红）`);
      process.exit(1);
    }
    console.log("✓ 缓存记账不在响应路径上：`open`/`put` 挂起都不扣住导航与 blob ✓ 而写入照旧落地");
    process.exit(0);
  }
  await finish();
}

main().catch((error) => {
  console.error("无法判断：判据自身抛异常 ⇒ " + String((error && error.stack) || error).slice(0, 300) + " ⇒ 退出码 3");
  process.exit(3);
});
