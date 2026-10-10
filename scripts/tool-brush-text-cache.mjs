#!/usr/bin/env node
// **★ 笔刷文本必须**只取一次****（PWA spray 测试 ✓；用户补丁 `yanshi-spray-2bugs.patch` 的第 ② 个 bug ✓）
//
// **∴ 它判什么 ✗**：**同一个笔刷名**调 `fetchBrushText` 两次**✗ ⇒ **∴ `fetch` **只能发一次** ✓
//   （**∴ 因为**笔刷文件是**静态**的 ✗、**一笔内**不会变 ✓ ⇒ **∴ 页面级缓存**是**正确**的 ✓）
//
// **∴ 它守的 bug ✗**：**用户报告**「**每画一笔就 GET 一次 `/brushes/spray.myb`**」 ✓
//   ⇒ **∴ 修法**：**`web/brush-local.js` 的 `brushTextCache`（Map ✓）** ✓
//
// **∴ 变异（**必红 ✓）✗**：**去掉 `brushTextCache`**
//   （**∴ 即**删掉 `if (brushTextCache.has(name)) return …` 与 `brushTextCache.set(…)` 两句 ✓）
//     ⇒ **∴ 于是** `fetch` **发两次** ⇒ **∴ 本判据**退出码 1** ✓ ****✓✓
//
// **∴ 为什么不写浏览器判据 ✗**：**本模块**不依赖 DOM**✗（**∴ 它**只调 `fetch` ＋ 读 `location.href` ✓）
//   ⇒ **∴ 在 node 里**桩掉这两个**就能**直接测** ✓ ⇒ **∴ 于是**判据**快 ＋ 稳 ＋ 无 flake** ✓ ★**** ✓✓
//
// 用法：node scripts/tool-brush-text-cache.mjs

const BRUSH = "spray.myb";
const BODY = '{"comment": "MyPaint brush file", "preset": "spray"}';

let calls = 0;
const seen = [];
globalThis.fetch = async (url) => {
  calls += 1;
  seen.push(String(url));
  return {
    ok: true,
    status: 200,
    url: "http://local.test" + String(url),
    text: async () => BODY,
  };
};
globalThis.location = { href: "http://local.test/?doc=d&token=t" };

const { fetchBrushText } = await import("../web/brush-local.js");

const a = await fetchBrushText(BRUSH);
const b = await fetchBrushText(BRUSH);

console.log(`  第一次 ⇒ ${JSON.stringify(String(a).slice(0, 40))}…`);
console.log(`  第二次 ⇒ ${JSON.stringify(String(b).slice(0, 40))}…`);
console.log(`  fetch 调用次数 = ${calls}｜URL = ${JSON.stringify(seen)}`);

const failures = [];
if (a !== BODY || b !== BODY) failures.push("返回值与桩不符 ⇒ 判据前提不成立");
if (calls !== 1) {
  failures.push(
    `同名笔刷调两次，fetch 却发了 ${calls} 次（期望 1）` +
      " ⇒ 笔刷文本缓存没生效（每画一笔都会 GET 一次笔刷文件）",
  );
}
if (seen.length && !seen.every((u) => u.startsWith("/brushes/"))) {
  failures.push(`请求 URL 不以 /brushes/ 开头：${JSON.stringify(seen)}`);
}

if (failures.length) {
  console.error("✗ 笔刷文本缓存判据未通过：");
  for (const f of failures) console.error("  - " + f);
  process.exit(1);
}
console.log("✓ 同名笔刷只取一次 ⇒ 笔刷文本内存缓存生效 ✓");
