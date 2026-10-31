#!/usr/bin/env node
// **`web/index.html` 的字节预算：只许降、不许悄悄涨**（05 复审 NEW-H1 的防回归半边）
//
// **⇒ 背景**：审计点出查看器单文件 1.04 MB ✗（NEW-H1）。瘦身是大手术（面板拆分/构建管线），
//   排期在后 ⇒ 但**排期之前先止损**：不许每次合并补丁顺手再塞进去几十 KB 而没人看见 ✓。
//
// **⇒ 为什么可以写死这个数**（判据两分类 ✓）：CEILING 是**判据自己造的期望值**
//   （棘轮：越改只许越小），不是被测系统的数量 ⇒ 不属「上游一加就失效」那一类。
//   当前实测值用命令现算（`wc -c web/index.html`），注释里带时间戳（内核 ② ✓）。
//
// **⇒ 判什么**：① `git show HEAD:web/index.html` 取得到；② 字节数 ≤ CEILING；③ 若已逼近（≥95%）打印警示但不红
//   （棘轮的含义是**拦住增长**，不是拦住逼近）。
// **⇒ 为什么读 git 而不是工作区文件**（CI run `40646976572` 的实测教训 ✓）：CI 跑判据前
//   有 `pwa-sync` 步骤**重写工作区**的 index.html ⇒ 同一提交本机 1,038,359、CI 1,044,372 ✗
//   ⇒ 读工作区就是**跨环境不同源**（第 336 轮「它读的两个量是不是同源」的假红形态 ✗）；
//   读 HEAD 的 blob ⇒ 同一提交在两台机器上**必然同一个数** ✓。
// **⇒ 变异**：把 index.html 追加 12 KB 提交成临时 commit ⇒ ② 红 ✓（reset --soft 还原再验绿 ✓）。
// **⇒ 红之后怎么办**：若增长是有意的（合并补丁带进新功能），把 CEILING 改成新值、
//   在提交信息里写清「谁涨了多少、为什么」——让膨胀**必须被看见**，这就是本判据的全部目的。
import { execFileSync } from "node:child_process";

// 棘轮上界（NEW-H1 瘦身第一步：三张双语词表挪成外链 `/i18n.js` ⇒
// 实测 1,038,359 → 968,345 字节（−69,984，截至 2026-10-29）；
// 第二步：样式表挪成外链 `/viewer.css` ⇒ 实测 968,345 → 930,816 字节
// （−37,529，截至 2026-10-29）⇒ 上界取 931,000 ⇒ 本提交落地前对旧 HEAD 会红，属预期）。
// 第三步（2026-10-31）：入库的 index.html 落后于规范前端（`web/viewer-app.js` 不入库、
// 只在 pwa-sync 时生成）⇒ 重新导出后 930,815 → 931,329 字节（+514）：
// 参考图上传改成带 try/catch 的压缩上传、面板接线只做一次、`LOCAL_READ_TOOLS`
// 补上 `get_bake_status`／`stroke_heatmap` ⇒ 上界抬到 932,000（留 671 字节）。
// 数它用 `git show HEAD:web/index.html | wc -c`；改这个数必须附理由。
const CEILING = 932_000;

const failures = [];
const check = (ok, label, detail) => {
  console.log(`  ${ok ? "✓" : "✗"} ${label}${detail ? `（${detail}）` : ""}`);
  if (!ok) failures.push(label + (detail ? `：${detail}` : ""));
};

let size;
try {
  size = execFileSync("git", ["show", "HEAD:web/index.html"], { maxBuffer: 32 * 1024 * 1024 }).length;
} catch {
  console.error("  ⊘ 判据前提不成立：git show HEAD:web/index.html 取不到（不在 git 仓库？不是产品问题）");
  process.exit(3);
}

const ratio = size / CEILING;
check(ratio <= 1, `index.html ${size.toLocaleString("en")} 字节 ≤ 预算 ${CEILING.toLocaleString("en")}`,
  ratio > 1 ? `超出 ${(size - CEILING).toLocaleString("en")} 字节 ⇒ 要么瘦身，要么带着理由抬棘轮` : `用了 ${(ratio * 100).toFixed(1)}%`);
if (ratio <= 1 && ratio >= 0.95) {
  console.log(`  ⚠ 已用 ${(ratio * 100).toFixed(1)}% 预算 ⇒ 下次合并前先考虑瘦身（NEW-H1）`);
}

if (failures.length > 0) {
  console.error(`✗ 字节预算判据红 ${failures.length} 条`);
  process.exit(1);
}
console.log("✓ web/index.html 在字节预算内");
