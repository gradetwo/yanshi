#!/usr/bin/env node
//! **★ 把服务端的预设库内联进 PWA 的 `api-local.js` ✓ ★**（第 2 批 ✓）
//!
//! **为什么需要它 ✗**：`web/api-local.js` 里那份 `P2_PRESET_RAW` 是**第二份拷贝** ✗
//! （**源文件是 `crates/yanshi-server/data/style-presets.json` ✓**）⇒
//! **∴ 「同一事实只写一处」被打破 ✗ ⇒ **∴ 至少要有一个**机器**看守它 ✓**：
//!   · `--write`：从源文件重新生成内联块（**唯一允许的改法 ✓ 不要手抄 ✗**）；
//!   · `--check`：只比对 ⇒ 落后 ⇒ `exit 1`（**判据 `tool-pwa-preset-library-parity.mjs` 调它 ✓**）。
//!
//! **∴ 为什么内联而不是多一个资产文件 ✗**：`api-local.js` 已被 Service Worker 预缓存 ✓
//!   ⇒ 内联的部分离线必定可用 ✓；新增 `style-presets.json` 资产要同时改预缓存清单 ✗
//!   （**漏改 ⇒ 离线取不到 ⇒ 而库里读不到预设会被当成「没有预设」⇒ 正是本仓最怕的静默降级 ✓**）。
//!
//! **自检**（`String.raw` 的三个禁忌 ✗ 必须当场挡住 ✓）：内联文本里**不许有**
//!   反引号、`${`、`\r` ⇒ 有就报错退出（**否则模板串会被截断 ✗ 而症状是「解析失败」✓**）。
process.on("uncaughtException", (error) => {
  console.error(`  ⊘ 脚本自身出错（不是产品问题）：${String(error).slice(0, 300)}`);
  process.exit(3);
});
import { readFileSync, writeFileSync } from "node:fs";

const SOURCE = "crates/yanshi-server/data/style-presets.json";
// **只改 `web/`**✓（**`crates/yanshi-http/assets/api-local.js` 是它的**逐字副本**✗ ⇒ 靠 `cp` ✓；
//   **∴ 而「两份是不是同一份」由 `tool-pwa-preset-library-parity.mjs` 逐字比 ✓**
//   （**本仓的 `tool-pwa-assets` 在 known-red 名单里 ⇒ **∴ 不能只靠它看守 ✓**）。
const TARGETS = ["web/api-local.js"];
const BEGIN = "// <<P2_PRESET_RAW:begin>>";
const END = "// <<P2_PRESET_RAW:end>>";

const mode = process.argv.includes("--check") ? "check" : "write";
const json = readFileSync(SOURCE, "utf8");

// —— 内联安全性（三条禁忌 ✗ 一条都不能放行 ✓）——
const forbidden = [["反引号 `", /`/], ["模板插值 ${", /\$\{/], ["回车 \r", /\r/]];
for (const [label, pattern] of forbidden) {
  if (pattern.test(json)) {
    console.error(`✗ ${SOURCE} 含有 ${label} ⇒ 不能用 String.raw 内联（模板串会被截断）`);
    process.exit(1);
  }
}
// 解析一遍只为「源文件本身是合法 JSON」✓（内联后 JS 侧 JSON.parse 才可能成功 ✓）。
const parsed = JSON.parse(json);
const jsonBytes = Buffer.byteLength(json, "utf8");
const block = `${BEGIN}\nconst P2_PRESET_RAW = String.raw\`${json}\`;\n${END}\n`;

let bad = 0;
for (const target of TARGETS) {
  const text = readFileSync(target, "utf8");
  const begin = text.indexOf(BEGIN);
  const end = text.indexOf(END);
  if (begin < 0 || end < 0 || end < begin) {
    if (mode === "check") {
      console.error(`✗ ${target} 里没有内联块标记（${BEGIN}）⇒ 预设库尚未嵌入`);
      bad += 1;
      continue;
    }
    writeFileSync(target, text.endsWith("\n") ? text + "\n" + block : text + "\n\n" + block);
    console.log(`✓ ${target}：追加内联块（${jsonBytes} 字节）`);
    continue;
  }
  const current = text.slice(begin, end + END.length + 1);
  if (mode === "check") {
    if (current === block) { console.log(`✓ ${target}：内联库与源文件逐字一致`); continue; }
    console.error(`✗ ${target}：内联库**落后于** ${SOURCE} ⇒ 跑 \`node scripts/gen-pwa-preset-library.mjs\` 再生成`);
    bad += 1;
    continue;
  }
  writeFileSync(target, text.slice(0, begin) + block + text.slice(end + END.length + 1));
  console.log(`✓ ${target}：内联库已刷新（${jsonBytes} 字节、${(parsed.presets || []).length} 套预设）`);
}
process.exit(bad ? 1 : 0);
