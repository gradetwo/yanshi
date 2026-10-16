#!/usr/bin/env node
// **★ 部署后核对：线上四张作品展示图必须真的能取到 ✗ ★**（2026-10-16 ✓）
//
// **∴ 为什么单独一支 ✗**：`tool-showcase-assets.mjs` 已经会在**本地服务**上逐张抓这四个 URL ✓
//   （200 ＋ 字节数与 `assets/showcase/` 逐值相同 ✓）；但**部署到线上**之后 ✓
//   （`yanshi.wangda.today` ✓，PWA 走 `web/` ✓）**没有任何东西核对过线上那份** ✗ ⇒ 本脚本补它 ✓。
//
// **∴ 退出码（按本仓约定 ✓）**：
//   * **连不上／DNS 失败** ⇒ **3** ✓（"**我判不了**" ✗ —— 网络问题不该冒充"线上内容错" ✗）；
//   * **HTTP 非 200 ／ 不是 PNG ／ 字节数与本地不一致** ⇒ **1** ✓（**这就是 CD 失败** ✓，要响亮 ✓）；
//   * 全对 ⇒ 0 ✓。
//
// **用法**：node scripts/verify-live-showcase.mjs [origin]      （缺省 https://yanshi.wangda.today）
import { readFileSync, existsSync } from "node:fs";

const origin = (process.argv[2] || "https://yanshi.wangda.today").replace(/\/+$/, "");
const SOURCE_DIR = "assets/showcase";
const NAMES = [
  "degas-rehearsal-morning.png",
  "degas-blue-dancers.png",
  "degas-backstage-oil.png",
  "degas-four-dancers-oil.png",
];

const isPng = (bytes) =>
  bytes.length > 8 &&
  bytes[0] === 0x89 && bytes[1] === 0x50 && bytes[2] === 0x4e && bytes[3] === 0x47;

const failures = [];
let judged = 0;
for (const name of NAMES) {
  const local = `${SOURCE_DIR}/${name}`;
  if (!existsSync(local)) {
    console.error(`❌ 判不了：本地缺 ${local} ⇒ 退出码 3`);
    process.exit(3);
  }
  const localBytes = readFileSync(local).length;
  const url = `${origin}/showcase/${name}`;
  let response;
  try {
    response = await fetch(url, { redirect: "follow" });
  } catch (error) {
    console.error(`❌ 判据自身的求值失败（不是内容问题）⇒ 连不上 ${url}：${error.message} ⇒ 退出码 3`);
    process.exit(3);
  }
  const bytes = new Uint8Array(await response.arrayBuffer());
  judged += 1;
  const ok = response.status === 200 && isPng(bytes) && bytes.length === localBytes;
  console.log(
    `  ${ok ? "✓" : "✗"} ${url} ⇒ HTTP ${response.status}｜${bytes.length} 字节（本地 ${localBytes} ✓）` +
      (isPng(bytes) ? "" : "｜**不是 PNG** ✗"),
  );
  if (!ok) failures.push(`${name}：HTTP ${response.status}｜线上 ${bytes.length} 字节 vs 本地 ${localBytes} ✓`);
}

if (judged === 0) {
  console.error("❌ 判不了：一张都没抓到 ⇒ 退出码 3");
  process.exit(3);
}
if (failures.length) {
  console.error(`❌ 线上作品展示图 ${failures.length} 处不对（这是**CD 失败** ✓）：`);
  for (const item of failures) console.error(`   · ${item}`);
  process.exit(1);
}
console.log(`✓ 线上四张展示图都取到了：HTTP 200 ＋ 合法 PNG ＋ 字节数与 assets/showcase/ 逐值相同 ✓（${origin} ✓）`);
