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
// 2026-11-04：(D) 回执可见性（log 增 forceNotice + stageNotice 兜底防递归 + 4 处导出成功回执）使 index.html 931,493→932,334（+841 B）⇒ 上界 932,000→933,000（留 666 B）。
// 2026-11-04：锁定层的「没有发送」长句补 I18N 模式词条 m42（`refuseLockedLayer` 的 `log(…,"#c33")`
//   会覆盖 `stageNotice` ⇒ 用户看见的是长句，缺词条即英文界面漏中文）使 932,334→933,641（+1,307 B：
//   词条 337 B + 说明注释 970 B）⇒ 上界 933,000→934,000（留 359 B）。
// 2026-11-04：补 I18N 模式词条 m43（`没有 WASM 内核，无法自检` ⇒ `No WASM kernel ⇒ the self-check cannot run`）使
//   933,641→934,026（+385 B = 句式 64 + 链上一行 71 + 说明注释 250）⇒ 上界 934,000→934,385（+385，留 359 B 不变）。
// 2026-11-04：锁定层 toast 的英文去掉 `permission_denied` 术语（创作者不该读 HTTP 词）；状态栏缩放
//   读数去重（`#zoom` 收进 hidden，仍写 textContent 供判据读）使 934,026→934,423（+397 B：
//   去术语 −44 B，zoom 说明 +约 230 B，m42 说明改写 +约 210 B）⇒ 上界 934,385→934,782（+397，留 359 B 不变）。
// 2026-11-04：`submitAtom` 把「取响应」与「解析 JSON」拆成两步（非 JSON 不再被当成离线 ⇒ 不入队）
//   ＋ catch 只在 `error.yanshiOffline === true` 时入队，使 934,423→936,218（+1,795 B：
//   两段说明注释 +约 1,600 B，拆分与状态纠正代码 +约 195 B）⇒ 上界 934,782→936,577（+1,795，留 359 B 不变）。
// 2026-11-04：手机布局修复（390×844 画布 104→244px；面板开合后重新适配；状态栏不裁切；
//   复选框 40px 命中区；顶栏图标换内联 SVG）使 936,218→940,732（+4,514 B：
//   CSS 改动只在 viewer.css，不计入本文件；index.html 的增量全部来自 viewer-app.js 的
//   `refitStage`／`ResizeObserver`／图标替换及其说明注释）⇒ 上界 936,577→941,091
//   （+4,514，留 359 B 不变）。
// 2026-11-04：320×568 短屏修复（画布 43×32→232×174；工具条两行＋露头；状态栏换行；右栏抽屉）
//   使 940,732→942,077（+1,345 B）。CSS 改动只在 viewer.css（不计入本文件）；index.html 侧的
//   增量全部来自 viewer-app.js 的 `loadPanels()` 短屏默认（+776 字符：条件常量、
//   短屏一律先收起、桌面照旧尊重存储）及其说明注释 ⇒ 上界 941,091→942,077
//   （+986，**不留余量**：这次只抬到实测值，下一次改动必须自己写出余量来）。
// 2026-11-05：**入库的 index.html 落后于源码若干提交**（CSS 手机布局／常驻提示等都没进过文件），
//   从当前二进制重新导出后 942,077→945,465（+3,388 B）⇒ 先**瘦身**：把本次新加的说明注释
//   压到两三行（viewer-app.js −约 2.4 KB、viewer.css −约 0.4 KB）⇒ 实测 **943,528 B**。
//   仍比旧上界多 1,451 B（差额来自**源码本来就有、只是没入库**的那部分）⇒ 抬到
//   **944,500**（+1,451 之后留 **972 B** 余量 ⇒ 下一次改动不必先抬棘轮）。
const CEILING = 944_500;

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
