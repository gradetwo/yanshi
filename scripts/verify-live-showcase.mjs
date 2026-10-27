#!/usr/bin/env node
// **★ 部署后核对：线上**首页显示的那几张**作品展示图必须真的能取到 ✗ ★**（2026-10-16 ✓；2026-10-28 两次纠正 ✓）
//
// **∴ 为什么单独一支 ✗**：`tool-showcase-assets.mjs` 已经会在**本地服务**上逐张抓这些 URL ✓
//   （200 ＋ 字节数与 `assets/showcase/` 逐值相同 ✓）；但**部署到线上**之后 **没有任何东西核对过线上那份** ✗
//   ⇒ 本脚本补它 ✓。
//
// **★ 2026-10-28：它自己坏了，两处都已在下面修掉 ✗ ★**
//   ① **名单曾是手抄的副本** ✗：`4f9de7b0`（10-17）删了两张展示图、同步改了 `tool-showcase-assets.mjs`，
//      但**漏改本文件的四张清单** ⇒ 本地缺 `degas-rehearsal-morning.png` ⇒ 每次 `exit 3`（判不了）✗。
//      ⇒ **∴ 现在**名单**从 `web/index.html` 的 `<img src="showcase/…">` 提取** ✓ ——
//        它是"线上到底该显示哪几张"的**唯一来源** ✓，不再有第二份清单可以漂移 ✓。
//   ② **origin 曾是 `yanshi.wangda.today`** ✗ —— 那是**官网**（首页 HTML 完全不同、图在 `/assets/showcase/`）；
//      PWA 部署在 **`yanshi-online.wangda.today`**（`wrangler.toml` 的 `routes` ＋ 部署步骤的摘要都这么写 ✓）。
//      ⇒ **∴ 现在**缺省 origin 改成 PWA 那个 ✓。
//   **★ 顺带钉住一个真陷阱 ✗**：PWA 的静态托管用 `not_found_handling = "single-page-application"`
//     ⇒ **∴ 缺文件也返回 HTTP 200**（`content-type: text/html`，正文是 index.html）✗
//     ⇒ **∴ 所以**只查状态码会**假绿** ✗ ⇒ **∴ 必须**同时查 PNG magic ＋ 字节数 ✓（见 `isPng`）。
//
// **∴ 退出码（按本仓约定 ✓）**：
//   * **连不上／DNS 失败** ⇒ **3** ✓（"**我判不了**" ✗ —— 网络问题不该冒充"线上内容错" ✗）；
//   * **HTTP 非 200 ／ 不是 PNG ／ 字节数与本地不一致** ⇒ **1** ✓（**这就是 CD 失败** ✓，要响亮 ✓）；
//   * 全对 ⇒ 0 ✓。
//
// **∴ 重试** ✓（2026-10-28）：本 job 现在**跟着部署跑**（`pwa.yml` 的 `verify-live`）⇒
//   刚部署完的几秒内，边缘可能还在发旧版本（旧版本里根本没有这些图 ⇒ SPA 回落到 index.html ✗）
//   ⇒ **∴ 每张图最多试 5 次、间隔 3 秒** ✓；**全部尝试都连不上**才退 3（网络问题）✓。
//
// **用法**：node scripts/verify-live-showcase.mjs [origin]      （缺省 https://yanshi-online.wangda.today）
import { readFileSync, existsSync } from "node:fs";

const origin = (process.argv[2] || "https://yanshi-online.wangda.today").replace(/\/+$/, "");
const SOURCE_DIR = "assets/showcase";
const PAGE = "web/index.html";
const ATTEMPTS = 5;
const RETRY_MS = 3000;

const html = readFileSync(PAGE, "utf8");
const NAMES = [...new Set([...html.matchAll(/src="showcase\/([^"?#]+)"/g)].map((m) => m[1]))];
if (!NAMES.length) {
  console.error(`❌ 判不了：${PAGE} 里找不到 src="showcase/…" ⇒ 名单提取坏了（不是线上内容错）⇒ 退出码 3`);
  process.exit(3);
}

const isPng = (bytes) =>
  bytes.length > 8 &&
  bytes[0] === 0x89 && bytes[1] === 0x50 && bytes[2] === 0x4e && bytes[3] === 0x47;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const failures = [];
for (const name of NAMES) {
  const local = `${SOURCE_DIR}/${name}`;
  if (!existsSync(local)) {
    console.error(`❌ 判不了：本地缺 ${local} ⇒ 退出码 3`);
    process.exit(3);
  }
  const localBytes = readFileSync(local).length;
  const url = `${origin}/showcase/${name}`;
  let last = null;
  let networkOnly = true;
  for (let attempt = 1; attempt <= ATTEMPTS; attempt += 1) {
    try {
      const response = await fetch(url, { redirect: "follow" });
      const bytes = new Uint8Array(await response.arrayBuffer());
      last = { status: response.status, bytes };
      networkOnly = false;
      if (response.status === 200 && isPng(bytes) && bytes.length === localBytes) break;
    } catch (error) {
      last = { error };
    }
    if (attempt < ATTEMPTS) await sleep(RETRY_MS);
  }
  if (networkOnly) {
    console.error(
      `❌ 判据自身的求值失败（不是内容问题）⇒ 连不上 ${url}（试了 ${ATTEMPTS} 次）：` +
        `${last?.error?.message} ⇒ 退出码 3`,
    );
    process.exit(3);
  }
  const bytes = last.bytes;
  const ok = last.status === 200 && isPng(bytes) && bytes.length === localBytes;
  console.log(
    `  ${ok ? "✓" : "✗"} ${url} ⇒ HTTP ${last.status}｜${bytes.length} 字节（本地 ${localBytes} ✓）` +
      (isPng(bytes) ? "" : "｜**不是 PNG**（SPA 回落成 index.html ⇒ 这个文件没部署上去）✗"),
  );
  if (!ok) failures.push(`${name}：HTTP ${last.status}｜线上 ${bytes.length} 字节 vs 本地 ${localBytes} ✓`);
}

if (failures.length) {
  console.error(`❌ 线上作品展示图 ${failures.length} 处不对（这是**CD 失败** ✓）：`);
  for (const item of failures) console.error(`   · ${item}`);
  process.exit(1);
}
console.log(
  `✓ 线上 ${NAMES.length} 张展示图都取到了：HTTP 200 ＋ 合法 PNG ＋ 字节数与 assets/showcase/ 逐值相同 ✓（${origin} ✓）`,
);
