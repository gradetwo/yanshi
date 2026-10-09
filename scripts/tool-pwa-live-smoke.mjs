#!/usr/bin/env node
// **★ 线上 PWA 的冒烟判据 ✗ ★**（第 849 轮 ✓；**用户报"改了没生效"＋"画不出来" ✓**）
//
// **∴ 为什么需要它 ✗**：**本地判据证明不了**线上到底是哪一份 ✗**
//   ⇒ **∴ 而**我**上一轮**写的一次性验证脚本**有一个**假通过缺陷 ✗**：
//      **∴ `ok = True` ＋ 遍历空集合 ⇒ **∴ 于是**"什么都没检查"也报绿 ✓**
//   ⇒ **∴ 本判据**硬性断言**检查项数 ≥ 6 ✗**（**∴ 否则**退出 1 ✓）—— **∴ 这正是那条教训的固化 ✓**。
//
// **∴ 它检查什么 ✗**（**六项 ✓，**全部针对**用户实际遇到的故障**✓）**：
//   **① 哨兵在页面**很靠前**✗**（**≤ 30 行 ✓）—— **∴ 防"405 时序"回归 ✓**；
//   **② 诊断哨兵存在 ✗**（**`__pwaDiag` ✓）**；
//   **③ 哨兵的引用次数 ＝ 4 ✗**（**定义／入队／flush／调用 ✓）**；
//   **④ `/api-local.js` 有 `/api/blob` 分支 ✗**；
//   **⑤ 二进制 body 不再被当 JSON ✗**（**`__binary` ✓）**；
//   **⑥ 内核不可用时如实 501 ✗**（**`kernel_unavailable` ✓）**。
//   **＋** 关键路径的 HTTP 200 ✗**（**`/`／品牌／我的图标／示例图／内核／本地层 ✓）**。
//
// **∴ 为什么**不接进 `run-criteria.sh` ✗**：**它需要**外网 ✗**（**CI 里**网络不可靠 ✓）
//   ⇒ **∴ 所以**在 `tool-criteria-coverage.mjs` 的 `NOT_WIRED` 里**写明这个原因 ✓**，
//     **∴ 而**它**仍然可以**手工一条命令跑到 ✗**（`node scripts/tool-pwa-live-smoke.mjs` ✓）**。
const BASE = process.env.YANSHI_PWA_BASE ?? "https://yanshi-online.wangda.today";
const bust = "?t=" + Date.now();
const fails = [];
let checked = 0;

// **★ 无网 ⇒ 跳过而不是失败 ✗ ★**（第 851 轮 ✓）：
//   **∴ 为什么 ✗**：**本判据**被 `run-criteria.sh` 的 glob **自动枚举** ✗**
//     （**第 850 轮实测 ✓）⇒ **∴ 于是**它**会在 CI 里跑 ✓**
//     ⇒ **∴ 而**它**需要**外网 ✗** ⇒ **∴ 断网时**"**判据红**"与"**环境无网**"**无法区分 ✓**
//   **∴ 现在**：**只把**明确的网络类错误**当作**跳过 ✗**（**退出 0 ＋ 显式打印 ✓）**
//     ⇒ **∴ 而**任何**断言不符**仍然**退出 1 ✓****（**∴ 不许**用"跳过"掩盖真失败 ✓）** ✓✓
//   **∴ 且**`YANSHI_LIVE_REQUIRE=1` 可以**强制**要求外网 ✗**（**∴ 那时**无网**就是**失败 ✓）** ✓✓
const REQUIRE = process.env.YANSHI_LIVE_REQUIRE === "1";
const isNetworkError = (err) => {
  const m = String((err && err.message) || err) + " " + String((err && err.cause && err.cause.code) || "");
  return /ENOTFOUND|EAI_AGAIN|ECONNREFUSED|ECONNRESET|ETIMEDOUT|fetch failed|network/i.test(m);
};
try {
  await fetch(BASE + "/health", { cache: "no-store" });
} catch (err) {
  if (isNetworkError(err) && !REQUIRE) {
    console.log("  SKIP 无法访问 " + BASE + "（" + String((err && err.message) || err) + "）");
    console.log("\n⏭  线上 PWA 冒烟跳过 ✓（无网；设 YANSHI_LIVE_REQUIRE=1 可强制要求网络）");
    process.exit(0);
  }
  console.error("  FAIL 网络不可用且要求联网：" + String((err && err.message) || err));
  process.exit(1);
}

const get = async (path) => {
  const res = await fetch(BASE + path, { cache: "no-store" });
  return { status: res.status, text: res.ok || res.status === 200 ? await res.text() : "" };
};
const check = (ok, msg) => { checked += 1; if (!ok) fails.push(msg); console.log(`  ${ok ? "OK  " : "FAIL"} ${msg}`); };

try {
  const page = await get("/" + bust);
  const api = await get("/api-local.js" + bust);
  if (page.status !== 200) check(false, `首页应 200（实测 ${page.status}）`);
  else {
    const lines = page.text.split("\n");
    const sentinel = lines.findIndex((l) => l.includes("__pwaSentinel"));
    const diag = lines.findIndex((l) => l.includes("__pwaDiag"));
    check(sentinel >= 0 && sentinel <= 30, `哨兵应在页面最前（第 ${sentinel + 1} 行）`);
    check(diag >= 0, `诊断哨兵应存在（第 ${diag + 1} 行）`);
    check((page.text.match(/__pwaSentinel/g) || []).length === 4, "哨兵引用应为 4 次");
  }
  if (api.status !== 200) check(false, `本地层应 200（实测 ${api.status}）`);
  else {
    check(api.text.includes('"/api/blob"'), "本地层应有 /api/blob 分支");
    check(api.text.includes("__binary"), "二进制 body 不应被当 JSON 解析");
    check(api.text.includes("kernel_unavailable"), "内核不可用应如实 501");
  }
  for (const p of ["/brand/svg/icon-light.svg", "/icons/icon.svg", "/samples/sample-yanshi.png",
                   "/wasm/yanshi_wasm.js"]) {
    const r = await get(p);
    check(r.status === 200, `${p} 应 200（实测 ${r.status}）`);
  }
} catch (err) {
  check(false, `请求失败：${String((err && err.message) || err)}`);
}

// **★ 防假通过 ✗ ★**：**空集合 ＋ `every` 会静默通过 ✗** ⇒ **∴ 硬性要求检查项数 ✓**
if (checked < 6) {
  fails.push(`只检查了 ${checked} 项 ⇒ 少于 6 ⇒ 判据可能被削弱或网络异常`);
  console.log(`  FAIL 检查项数 ${checked} < 6`);
}
console.log(fails.length === 0
  ? `\n✅ 线上 PWA 冒烟通过 ✓（检查 ${checked} 项）`
  : `\n❌ 线上 PWA 冒烟失败 ✗（检查 ${checked} 项，${fails.length} 项不符）`);
process.exit(fails.length === 0 ? 0 : 1);
