#!/usr/bin/env node
// **★ 远端 CI 的**失败分类器** ✗ ★**（**第 246 轮接手时造 ✓；**用户指令「反复做的事要造工具」✓）
//
// **∴ 它解决什么 ✗**（**我接手时**手工做过一遍**✗ ⇒ **∴ 那次花了**十几条 `gh` 命令 ＋ 肉眼 grep** ✓）★：
//   **∴ 症状 ✗**：**`gh run view --log-failed` 的输出**按 shard 混杂**✗
//     ⇒ **∴ 于是**：**要判断「这条红是不是**本轮引入**✗」**✗
//       **∴ 得**逐 shard 拉日志 ＋ 再去翻更早的 run** ✗ ⇒ **∴ 而**那**正是
//       handover §五 待办 1 的核心动作** ✓ ★★
//   **∴ 现在 ✗**：**一条命令**给出**四件事** ✓ ——
//     **∴ ①** 每个失败判据的**原文失败行**（**不是只看名字 ✓）
//     **∴ ②** 它**在不在** `scripts/criteria-known-red.txt` 里（**已登记的已知红 ✓）
//     **∴ ③** 它**是不是**既有红**✗（**∴ 去**更早的 run** 里查同名失败 ⇒ **∴ 隔离证据 ✓）★
//     **∴ ④** 它**是不是** GPU 门禁判据**✗（**∴ 即**它**挡不挡**目标第 4–11 项 ✓）★
//
// **∴ 判定口径（**用户已裁定，**不得自行更改 ✓）✗**（**handover §七 第 6 条 ✓）：
//   **∴「CI 全绿」✗ ＝ **守住第 4–11 项的判据**没有意外失败** ✓
//     ⇒ **∴ 其余**一律进 `scripts/criteria-known-red.txt`（**每条写四要素 ✓）** ✓★
//
// **∴ 退出码（**与仓库纪律一致 ✓）✗**：
//   **∴ 0** ⇒ **没有任何意外失败** ✓（**真全绿 ✓）
//   **∴ 4** ⇒ **有意外失败**✗ **而**没有一条落在 GPU 门禁清单里**✗ ⇒ **∴ 按裁定**记账即可** ✓★
//   **∴ 1** ⇒ **有 GPU 门禁判据意外红** ⇒ **∴ 必须修** ✓★
//   **∴ 3** ⇒ **本工具自己判不了** ✗（**没有 `gh`／run 还在跑／解析不到 ✗）
//     ⇒ **∴ 与产品失败**分开** ✓（**∴ 不许**把"我跑不了"写成"产品红" ✓）★
//
// **∴ 用法 ✗**：
//   node scripts/ci-failures.mjs                     # 当前 HEAD 对应的最近一次 run（**含既有红回溯 2 次** ✓）
//   node scripts/ci-failures.mjs --history 4         # 回溯 4 次更早的 run（**隔离证据更硬 ✓）
//   node scripts/ci-failures.mjs --history 0         # 不回溯（**快 ✓）
//   node scripts/ci-failures.mjs --run 38352082607   # 指定 run
//   node scripts/ci-failures.mjs --json              # 机器可读
//
// **∴ 两面（**AGENTS.md 第 3 条 ✓）★**：
//   **∴ 收益 ✗**：**把**十几条手工命令**缩成一条**✗ ＋ **∴ 且**它**自己会**给"既有红"的证据** ✓
//     ＋ **∴ 而**那**正是**本仓**最花时间**的一步**（**逐 run 翻日志 ✓）★
//   **∴ 代价 ✗**：**它**下载**每个失败 job 的**全量日志**✗（**含 `--history` 的每一次 ✓）
//     ⇒ **∴ 所以** `--history` 每加 1 **多几 MB 流量 ＋ 十几秒** ✓
//       ＋ **∴ 且**：**它解析的是**运行器日志的文本格式**✗
//         ⇒ **∴ 若** `run-criteria.sh` 改了输出形状**✗ ⇒ **∴ 它**要先改** ✓（**∴ 且**它**解析不到时会**退 3**✗
//           而**不是**默默报"全绿" ✓）★

import { execFileSync } from "node:child_process";
import { readFileSync, existsSync } from "node:fs";

const argv = process.argv.slice(2);
const option = (name, fallback = null) => {
  const i = argv.indexOf(name);
  return i >= 0 && argv[i + 1] ? argv[i + 1] : fallback;
};
const asJson = argv.includes("--json");
const branchArg = option("--branch");
const runArg = option("--run");
const history = Number(option("--history", "2"));

/** **∴ 工具自身判不了 ⇒ 3** ✓（**∴ 与产品红分开 ✓）★ */
const unusable = (why) => {
  console.error("✗ 本工具判不了：" + why);
  console.error("  ⇒ **∴ 这**不是产品失败**✗ ⇒ **∴ 退出码 3** ✓（**∴ 仓库纪律 ✓）");
  process.exit(3);
};

function gh(args) {
  try {
    return execFileSync("gh", args, { encoding: "utf8", maxBuffer: 256 * 1024 * 1024, timeout: 600_000 });
  } catch (e) {
    unusable("gh 调用失败：" + (e.stderr || e.message).split("\n")[0]);
  }
}

// **★ GPU 门禁清单 ✗ ★**（**∴ 即**挡着目标第 4–11 项的那些判据 ✓）
//   **∴ 出处 ✗**：`docs/design/HANDOVER-gpu-245.md` §二 的表格（**每条都记了它的判据 ✓）★
//   **∴ 注意 ✗**：**这里只放**判据名**✗ ⇒ **∴ cargo 测试**（**如 `gpu_composite_wiring` ✓）
//     走的是 `GPU feature` job**✗ ⇒ **∴ 那个 job 红了**本工具**也报**（**见 `jobFailure` 分支 ✓）** ✓★
const GPU_GATES = new Set([
  "tool-gpu-modes.mjs", // ① 后端如实报（实际／请求分开）
  "tool-gpu-fallback-parity.mjs", // ① fallback 逐字节 ＋ ② off 逐字节 ＋ ③ 如实报
  "tool-gpu-stage-accounting.mjs", // ⑨ 时间戳的分段账
  "tool-gpu-fixed-cost.mjs", // ⑨ 时间戳的固定开销
  "tool-kernel-async-sites.mjs", // ⑧ 内核 4 调用点走异步（route 乙）
  "tool-render-cost-accounts.mjs", // 两本账（时间 ＋ CPU 占用）
  "tool-break-even.mjs", // ③ 重定阈值用的平衡点扫描
  "tool-composite-scaling.mjs", // ⑪ 合成上 GPU 的规模账（**已知红：规格先行 ✓）
  "tool-below-reuse.mjs", // ⑪ below 缓存（**已知红：规格先行 ✓）
  "tool-macos-gpu-benefit.mjs", // ⑭ macOS（**不可判时它自己会说明 ✓）
  "tool-pwa-assets.mjs", // ⑫ 发布出去的 PWA 必须带内核
]);

/** **∴ 已登记的已知红 ✗**（**四要素**在台账里 ✓ ⇒ **∴ 这里只取**名字** ✓）★ */
function ledgerNames() {
  const path = "scripts/criteria-known-red.txt";
  // **∴ 返回 Map（**名字 → 那一行的注释 ✓）✗ ★**（**第 247 轮 ✓）：
  //   ＋ **∴ 为什么需要注释 ✗**：**审计要区分"**稳定通过 ⇒ 可移出**"与
  //     "**抖动类（**通过属预期 ✓）**"** ✓
  //     ＋ **∴ 而**后者**本来就该时红时绿**✗ ⇒ **∴ 不能**当成"台账过期"** ✓ ★★
  if (!existsSync(path)) return new Map();
  const map = new Map();
  for (const line of readFileSync(path, "utf8").split("\n")) {
    const m = line.match(/^([A-Za-z0-9_.-]+\.(?:mjs|sh))\s+(.*)$/);
    if (m) map.set(m[1], m[2]);
  }
  return map;
}

/** **∴ 这一条是不是**抖动类**✗**（**∴ 通过属预期 ⇒ 不移出 ✓）★ */
function isFlakyEntry(comment) {
  return /抖动|flaky|时红时绿/.test(comment ?? "");
}

// **∴ 运行器日志的行形状 ✗**：`<job 名>\t<step>\t<时间戳>Z <内容>` ✓
//   ⇒ **∴ 去前缀**之后才看得见 `  ✗ name｜原因` 这类行 ✓（**∴ 否则**grep 会命中
//     **job 名里的 `criteria shard`** ✗ ⇒ **∴ 误判 ✓）★
const strip = (line) => {
  const m = line.match(/^[^\t]*\t[^\t]*\t\d{4}-\d{2}-\d{2}T[\d:.]+Z (.*)$/);
  return m ? m[1] : null;
};

/** **★ 从一个 job 的日志里取出**它红了哪些判据 ＋ 原文原因 ✗ ★**
 *  **∴ 两种行都要收 ✗**：
 *    **∴ ①** `  ✗ <name> (EXIT=n)` ✓（**逐条执行时打的 ✓）
 *    **∴ ②** 末尾汇总 `  ✗ <name>｜<原因>` ✓（**那条**带原因**✗ ⇒ **∴ 它更有用** ✓）★
 *  **∴ 而** `⚠ <name>（**已知红…` ⇒ **∴ 已知红**不算失败** ✓（**运行器已经不阻塞 ✓）★
 */
function parseJobLog(text) {
  const failed = new Map(); // name → { exit, reason, knownRed }
  // **★★★ 第 247 轮：**每一条判据的**最终标记**✗ ★★★**（**`✓`／`⚠`／`✗` ✓）
  //   ＋ **∴ 为什么需要它 ✗**：**台账里的条目**即使**失败**也**不阻塞 CI**✗
  //     ⇒ **∴ 于是**：**那一分片**整片是**成功**✗ ⇒ **∴ 只解析"失败的 job"**看不到它们** ✓
  //       ＋ **∴ 而**那**正是"台账腐化"**（**功能早实现了 ✗ 条目还写着"未实现" ✓）**藏身之处** ✓ ★★
  //         ⇒ **∴ 所以**：**无论成败**都把标记收下来**✗ ⇒ **∴ 于是**能问
  //           "**台账里哪几条**其实已经绿了**" ✓ ★★
  const marks = new Map(); // name → "pass" | "knownRed" | "fail"
  const summary = [];
  let counts = null;
  // **★ 判据自己的失败行 ✗ ★**（**∴ 比汇总行更可信 ✓）★
  //   **∴ 为什么需要它 ✗**（**∴ 实测 ✓）：**汇总行有时**不是**判据的失败行** ✓
  //     ＋ **∴ 例 ✗**：`browser-offline-brush` 的汇总行抓到的是一条
  //       `· 负对照（SW 桩必须让未缓存请求失败）= {"failed":true,…}`
  //       ⇒ **∴ 那**是**诊断**✗ 而**不是失败**（**`failed:true` ＝ 负对照**通过**✓）★
  //         ⇒ **∴ 于是**：**只看汇总行 ⇒ **会把人**引到错的地方** ✓ ★★
  //   **∴ 所以 ✗**：**同时收** `项│/头│/后│…` 那组行里**以 ✗／❌ 开头**的最后一条** ✓
  //     ＋ **∴ 而**汇总行**不以 `·` 开头时**优先用它** ✓（**∴ 那是运行器**自己提炼的 ✓）★
  //   **⚠️ 而那个回退**必须按判据分桶**✗（**∴ 我第一版用一个全局变量 ⇒ **抓到了别人的行** ✓）★
  //     **∴ 实测（**第 246 轮 ✓）✗**：**全局写法下 `browser-offline-brush` 的原因变成了
  //       「参考图功能不存在」✗（**∵ 那是**它后面那条判据**的诊断 ✓）⇒ **∴ 于是**：
  //       **台账里会写下**张冠李戴的原因** ✗ ⇒ **∴ 比没有原因**更坏** ✓ ★★
  //     **∴ 正确做法 ✗**：**诊断行**归属于**最近一条** `✗ <name> (EXIT=n)`** ✓
  //       （**∴ 运行器**逐条跑**✗ ⇒ **∴ 顺序天然可用 ✓）★
  const diagByName = new Map();
  let current = null;
  for (const raw of text.split("\n")) {
    const line = strip(raw);
    if (line === null) continue;
    const d = line.match(/^\s+(?:项|体|头|前|后)│\s*(?:✗|❌)\s*(.+)$/);
    if (d && current) diagByName.set(current, d[1].trim());
    // **∴ 顺序要紧 ✗ ★**（**∴ 实测教训 ✓）：**汇总行**也以 `  ✗ <name>` 开头**✗
    //   ⇒ **∴ 若**先匹配"逐条执行行"**✗ 而**它的 `(EXIT=n)` 是可选的**✗
    //     ⇒ **∴ 汇总行**会被吃掉 ⇒ **∴ 于是**原因**永远为空** ✓（**本工具第一版就这样 ✓）★
    //   **∴ 所以 ✗**：**汇总行（**带 `｜` ✓）先判 ✗** ＋ **逐条行**必须**以 `(EXIT=n)` 结尾** ✓★
    const sumLine = line.match(/^\s+✗\s+([A-Za-z0-9_.-]+\.(?:mjs|sh))｜(.*)$/);
    if (sumLine) {
      const [, name, reason] = sumLine;
      const entry = failed.get(name) || { exit: null, reason: "", knownRed: false };
      const trimmed = reason.trim();
      // **∴ 以 `·` 开头的**是诊断行**✗ ⇒ **∴ 换成**该判据自己的失败行** ✓★
      entry.reason = trimmed.startsWith("·") && diagByName.has(name) ? diagByName.get(name) : trimmed;
      failed.set(name, entry);
      summary.push(name);
      continue;
    }
    // **∴ 通过那行**没有 `(EXIT=n)`**✗ ⇒ **∴ 单独一条** ✓（**∴ 否则**会与 `✗ … (EXIT=n)` 混 ✓）★
    const passLine = line.match(/^\s+✓\s+([A-Za-z0-9_.-]+\.(?:mjs|sh))\s*$/);
    if (passLine) {
      marks.set(passLine[1], "pass");
      current = passLine[1];
      continue;
    }
    const runLine = line.match(/^\s+([✓✗])\s+([A-Za-z0-9_.-]+\.(?:mjs|sh))\s*\(EXIT=(-?\d+)\)\s*$/);
    if (runLine) {
      const [, mark, name, exit] = runLine;
      // **∴ 诊断归属**跟着它走**✗ ⇒ **∴ 下一条 `项│ ✗` 就算**这一条**的 ✓★
      current = name;
      const entry = failed.get(name) || { exit: null, reason: "", knownRed: false };
      entry.exit = Number(exit);
      if (mark === "✗") {
        failed.set(name, entry);
        marks.set(name, "fail");
      }
      continue;
    }
    const knownLine = line.match(/^\s+⚠\s+([A-Za-z0-9_.-]+\.(?:mjs|sh))/);
    if (knownLine) {
      const name = knownLine[1];
      // **∴ 已知红那条**也要**接手"当前判据"** ✗ ★**（**∴ 实测教训 ✓）**：
      //   **∴ 若**不接**✗ ⇒ **∴ 它自己的 `项│ ✗` 行**会被算进**前一条**判据的名字里** ✓
      //     ＋ **∴ 实测（**第 246 轮 ✓）✗**：`browser-offline-brush` 于是拿到了
      //       「参考图功能不存在」✗（**那**是**它后面那条**（**已知红 ✓）的诊断 ✓）★★
      current = name;
      const entry = failed.get(name) || { exit: null, reason: "", knownRed: false };
      entry.knownRed = true;
      failed.set(name, entry);
      marks.set(name, "knownRed");
      continue;
    }
    const c = line.match(/^\s+——\s*通过\s*(\d+)｜意外失败\s*(\d+)｜已知红\s*(\d+)｜跳过\s*(\d+)/);
    if (c) counts = { pass: +c[1], unexpected: +c[2], knownRed: +c[3], skipped: +c[4] };
  }
  return { failed, counts, marks };
}

/** **★ 一个 job 红了、但**一条判据都没解析出来** ✗ ★**（**∴ 最危险的一种 ✓）★
 *
 *  **★★★ 它为什么必须存在 ✗ ★★★**（**∴ 我自己踩的 ✓）**：
 *    **∴ 实测（**run 38355714859 ✓）✗**：**`test (stable)`／`test (beta)` 因为
 *      `cargo test -p yanshi-server --test script_portability` 失败 ⇒ **∴ 整个 run**红** ✓
 *      ＋ **∴ 而**本工具**第一版**只找 `criteria shard` 的失败行**✗
 *        ⇒ **∴ 于是**它**打印了「★ 没有任何意外失败 ⇒ 绿 ★」**✗ —— **∴ 对一次**真红的 run** ✓ ★★
 *    **∴ 所以 ✗**：**失败的 job 里**解析不到判据**✗ ⇒ **∴ 必须**当成"无法归类"**✗
 *      ＋ **∴ 而**"无法归类"**不能当成"不挡第 4–11 项"**✗（**∴ 我们**没证据 ✓）
 *        ⇒ **∴ 所以**：**它也**走"必须修"那条（**退出码 1 ✓）★**** ✓✓
 */
function tailLines(text, n) {
  const lines = text
    .split("\n")
    .map((l) => strip(l) ?? l)
    .filter((l) => l.trim());
  // **∴ 优先取**真失败行**✗ ★**（**∴ 日志尾部通常是**收尾命令**✗ ⇒ **∴ 它**帮不上判 ✓）★
  //   ＋ **∴ 而** `##[error]Process completed` 也不算证据**✗（**∴ 那只说"这步红了" ✓）★
  //   ＋ **⚠️ 别用大小写不敏感的 `failed`** ✗（**∴ 那会命中**"0 failed"这种**通过行** ✓
  //     ⇒ **∴ 于是**证据里全是"test result: ok"✗ —— **∴ 我第一版就这样 ✓）★★
  const evidence = lines.filter(
    (l) =>
      !/Process completed with exit code/.test(l) &&
      !/^test result: ok\./.test(l) &&
      (/\bFAILED\b/.test(l) ||
        /panicked at/.test(l) ||
        /^error(\[|:)/.test(l) ||
        /##\[error\]/.test(l) ||
        /^\s*failures:\s*$/.test(l) ||
        /error: test failed/.test(l)),
  );
  return (evidence.length ? evidence : lines).slice(-n);
}

/** **★ 枚举一个 run 里**所有 job 红了的判据**（**∴ 供 --history 用 ✓）★
 *
 *  **★★★ 第 247 轮：`allCriteriaJobs` ✗ ★★★**（**∴ `--ledger-audit` 要用 ✓）**：
 *    ＋ **∴ 为什么 ✗**：**台账里的条目**即使失败也**不阻塞**✗
 *      ⇒ **∴ 于是**：**那种分片整片是**成功**✗ ⇒ **∴ 只看失败的 job**什么也看不到** ✓（**∴ 实测 ✓）★★
 *        ⇒ **∴ 所以**：**审计时**把**所有 `criteria shard *` 的 job**都解析一遍** ✓ ★★
 */
function runFailures(runId, allCriteriaJobs = false) {
  const view = JSON.parse(gh(["run", "view", String(runId), "--json", "status,conclusion,headSha,displayTitle,jobs"]));
  if (view.status !== "completed") {
    return { status: view.status, names: null, jobs: view.jobs, view };
  }
  const names = new Set();
  const marks = new Map();
  const perJob = [];
  for (const job of view.jobs) {
    const isCriteriaShard = job.name.startsWith("criteria shard");
    if (job.conclusion !== "failure" && !(allCriteriaJobs && isCriteriaShard)) continue;
    const log = gh(["run", "view", "--job", String(job.databaseId), "--log"]);
    const parsed = parseJobLog(log);
    const list = [...parsed.failed.entries()].map(([name, info]) => ({ name, ...info }));
    for (const item of list) names.add(item.name);
    for (const [n, m] of parsed.marks) {
      // **∴ 一条判据只跑一次 ⇒ **∴ 直接覆盖**也安全** ✓；**∴ 先来的**更可信（**失败**优先 ✓）★
      if (!marks.has(n) || m !== "pass") marks.set(n, m);
    }
    perJob.push({
      job: job.name,
      counts: parsed.counts,
      failures: list,
      // **∴ 红了却没有判据行 ⇒ **无法归类** ✓（**∴ 见上面那段说明 ✓）
      unparsed: list.length === 0,
      tail: list.length === 0 ? tailLines(log, 12) : [],
    });
  }
  return { status: view.status, names, marks, jobs: view.jobs, view, perJob };
}

// —— 解析本地 HEAD 与分支 ——（**∴ 只读 git，不联网 ✓）
const branch = branchArg || execFileSync("git", ["rev-parse", "--abbrev-ref", "HEAD"], { encoding: "utf8", timeout: 600_000 }).trim();
const head = execFileSync("git", ["rev-parse", "HEAD"], { encoding: "utf8", timeout: 600_000 }).trim();

let runId = runArg;
if (!runId) {
  const runs = JSON.parse(
    gh(["run", "list", "--branch", branch, "--limit", "40", "--json", "databaseId,headSha,status,conclusion,displayTitle,createdAt"]),
  );
  if (!runs.length) unusable(`分支 ${branch} 上没有任何 CI run`);
  // **∴ 优先取**当前 HEAD 的那一次**✗ ⇒ **∴ 没有**（**如 docs 提交不触发 ✓）⇒ **∴ 退回**最近一次** ✓★
  const forHead = runs.find((r) => r.headSha === head);
  const chosen = forHead || runs.find((r) => r.status === "completed") || runs[0];
  runId = chosen.databaseId;
  if (!asJson) {
    console.error(
      `∴ 分支 ${branch}｜HEAD ${head.slice(0, 8)}${forHead ? "" : "（**没有**对应 HEAD 的 run ⇒ **退回最近一次** ✓）"}` +
        `｜run ${runId}｜${chosen.status}/${chosen.conclusion ?? "-"}｜${chosen.displayTitle}`,
    );
  }
}

const current = runFailures(runId);
if (current.status !== "completed") {
  unusable(`run ${runId} 还在跑（status=${current.status}）⇒ **∴ 日志还拿不到** ⇒ **∴ 等它结束再跑本工具**`);
}

const known = ledgerNames();
const rows = [];
for (const job of current.perJob) {
  for (const f of job.failures) {
    rows.push({ ...f, job: job.job, inLedger: known.has(f.name), gpuGate: GPU_GATES.has(f.name) });
  }
}

// —— 既有红回溯 ——（**∴ 这就是"隔离证据" ✓）★
const historyRuns = [];
if (history > 0) {
  const runs = JSON.parse(
    gh(["run", "list", "--branch", branch, "--limit", "60", "--json", "databaseId,headSha,status,createdAt"]),
  )
    .filter((r) => r.status === "completed" && String(r.databaseId) !== String(runId))
    .slice(0, history);
  for (const r of runs) {
    const res = runFailures(r.databaseId);
    if (!res.names) continue;
    historyRuns.push({ run: r.databaseId, headSha: r.headSha, names: res.names });
  }
  for (const row of rows) {
    row.seenIn = historyRuns.filter((h) => h.names.has(row.name)).map((h) => h.run);
  }
}

// **★★★ `--ledger-audit`：**台账腐化普查**✗ ★★★**（**第 247 轮 ✓）
//   ＋ **∴ 它回答 ✗**：**`scripts/criteria-known-red.txt` 里的每一条 ✗
//     今天**到底**是"还红着"还是"早就绿了"** ✓
//   ＋ **∴ 为什么要它 ✗**（**∴ 第 247 轮实测 ✓）**：
//     **∴ `tool-below-reuse` 与 `tool-composite-scaling` 已经**通过**✗
//       而**台账还写着"below 缓存未实现"** ✓
//       ＋ **∴ 而**运行器对已知红**不阻塞**✗ ⇒ **∴ 于是**：**谁把这两条**改坏**✗
//         **CI 也**不会红** ✓ ★★
//   ＋ **∴ 退出码 ✗**：0 ＝ 台账里没有"已绿却还挂着"的条目｜**4 ＝ 有**（**∴ 该移出 ✓）｜3 判不了 ✓
if (argv.includes("--ledger-audit")) {
  // **★★★ 一轮的"通过"**不足以判"台账过期"**✗ ★★★**（**第 247 轮实测 ✓）
  //   ＋ **∴ 反例（**同一份台账 ✓）**：
  //     **∴ `browser-cs-render-switch` 在本轮是 ✓**✗ 而**它在更早 3 个 run 里红过** ✓
  //       ＋ **∴ `kernel-brush-parity` 在分片里是 ✓**✗ 而** CI 的 `all`（**199 支 ✓）那条是红的** ✓
  //         ⇒ **★ 所以 ✗ ★**：**只看本轮 ⇒ **会把"抖动"或"另一配置下红"当成"过期"**✗
  //           ⇒ **∴ 于是一刀切移出 ⇒ **把真回归放走** ✓ ★★
  //   ＋ **∴ 做法 ✗**：**本轮 ✓ ＋ **更早 N 个 run 的标记**一起看** ✓：
  //     **∴ 全绿 ⇒ "稳定通过 ⇒ 可移出"**✗
  //     **∴ 更早红过 ⇒ "抖动／范围不同 ⇒ 先别移出"** ✓ ★★
  const now = runFailures(runId, true);
  const seen = now.marks ?? new Map();
  // **∴ 回溯必须**跳过 cancelled**✗ ★**（**第 247 轮实测 ✓）：
  //   ＋ **∴ 症状 ✗**：**我前一版取了"最近 8 个 completed"**✗
  //     ＋ **∴ 而**本仓**连续推送**会产生一串 **cancelled**（**并发组替换 ✓）
  //       ⇒ **∴ 那些 run**没有判据标记**✗（**日志被取消 ✓）
  //         ⇒ **∴ 于是**：**B 类恒为 0**✗ ⇒ **∴ 抖动**看不出来** ✓（**实测：8 个窗口里 B=0 ✓）★★
  //   ＋ **∴ 修法 ✗**：**只回溯**成功或失败**的 run** ✓（**∴ cancelled 没有信息 ✓）★
  const histIds = JSON.parse(
    gh(["run", "list", "--branch", branch, "--limit", "80", "--json", "databaseId,status,conclusion"]),
  )
    .filter(
      (r) =>
        r.status === "completed" &&
        (r.conclusion === "success" || r.conclusion === "failure") &&
        String(r.databaseId) !== String(runId),
    )
    .slice(0, Math.max(1, history));
  const hist = [];
  for (const r of histIds) {
    const res = runFailures(r.databaseId, true);
    if (res.marks) hist.push({ run: r.databaseId, marks: res.marks });
  }
  const ledger = ledgerNames();
  const names = [...ledger.keys()].sort();
  const stale = [];
  const flaky = [];
  const flakyDeclared = [];
  const stillRed = [];
  const unseen = [];
  for (const n of names) {
    const m = seen.get(n);
    if (m !== "pass") {
      if (m === "knownRed" || m === "fail") stillRed.push(n);
      else unseen.push(n);
      continue;
    }
    const redBefore = hist.filter((h) => {
      const hm = h.marks.get(n);
      return hm === "knownRed" || hm === "fail";
    });
    // **∴ 台账自己写明是"抖动"的 ⇒ **单看本轮的 ✓ 不作数** ✓（**∴ 通过**正是它该有的样子 ✓）★
    if (isFlakyEntry(ledger.get(n))) flakyDeclared.push(n);
    else if (redBefore.length) flaky.push({ name: n, runs: redBefore.map((h) => h.run) });
    else stale.push(n);
  }
  console.log("");
  console.log(`∴ 台账共 ${names.length} 条｜本轮跑到 ${names.length - unseen.length} 条｜回溯 ${hist.length} 个 run`);
  console.log(`  ★ A. 稳定通过（**本轮 ✓ ＋ 回溯里没红过 ⇒ **可移出台账**）${stale.length} 条 ★`);
  for (const n of stale) console.log(`     ✓ ${n}`);
  console.log(`  ★ B. 本轮通过但**更早红过**（**抖动／另一配置 ⇒ 先别移出**）${flaky.length} 条 ★`);
  for (const f of flaky) console.log(`     ~ ${f.name}（红过 ${f.runs.length}/${hist.length}：${f.runs.join(",")}）`);
  console.log(`  ★ A'. 台账自标"抖动"（**本轮 ✓ 属预期 ⇒ **不移出**）${flakyDeclared.length} 条 ★`);
  for (const n of flakyDeclared) console.log(`     ~ ${n}`);
  console.log(`  · C. 仍然红（**台账有效**）${stillRed.length} 条`);
  for (const n of stillRed) console.log(`     ⚠ ${n}`);
  console.log(`  · D. 本次没看到（**没跑到／已改名**）${unseen.length} 条`);
  for (const n of unseen) console.log(`     ? ${n}`);
  if (stale.length) {
    console.error(
      `\n⚠ **A 类 ${stale.length} 条**稳定通过却还挂在台账里** ⇒ **∴ 它们**不再守回归** ✓（**按纪律移出 ✓）★`,
    );
    process.exit(4);
  }
  console.log("\n✓ 没有**稳定通过却还挂着**的条目（**B 类要人判：抖动／范围 ✓）★");
  process.exit(0);
}

if (asJson) {
  console.log(
    JSON.stringify(
      { run: Number(runId), head, branch, history: historyRuns.map((h) => ({ run: h.run, headSha: h.headSha })), rows },
      null,
      2,
    ),
  );
} else {
  console.log("");
  // **★ 先报"无法归类"✗ ★**（**∴ 它**比任何判据红都**优先** ✓ —— **∴ 因为**它说明
  //   **这个 run 的红**我根本没看懂**✗ ⇒ **∴ 那时**说"绿"就是撒谎** ✓）★★
  const unparsedJobs = current.perJob.filter((j) => j.unparsed);
  if (unparsedJobs.length) {
    console.log(`★★ 有 ${unparsedJobs.length} 个 job 红了、但**解析不到任何判据行**✗ ⇒ **∴ 本工具**无法归类** ★★`);
    for (const j of unparsedJobs) {
      console.log(`  ✗ job「${j.job}」的日志尾部：`);
      for (const l of j.tail) console.log(`      ${l}`);
    }
    console.log("");
  }
  if (!rows.length && !unparsedJobs.length) {
    console.log("★ 没有任何意外失败 ⇒ **∴ 就「CI 全绿」的定义（**第 4–11 项的判据无意外失败 ✓）而言：绿 ★");
  } else if (rows.length) {
    for (const job of [...new Set(rows.map((r) => r.job))]) {
      const list = rows.filter((r) => r.job === job);
      console.log(`== ${job}（${list.length} 条）==`);
      for (const r of list) {
        const tags = [];
        if (r.inLedger) tags.push("台账已登记（**∴ 即**既有红 ✓）");
        if (r.gpuGate) tags.push("★ GPU 门禁 ★");
        // **⚠️ 台账里的**不做历史判定** ✗ ★**（**∴ 实测教训 ✓）**：
        //   **∴ 运行器对台账里的**不置红**✗ ⇒ **∴ 那些 shard**整片是"成功"** ✓
        //     ＋ **∴ 而**本工具的历史回溯**只看**失败的 job**✗
        //       ⇒ **∴ 于是**它**在更早的 run 里找不到这些名字**✗
        //         ⇒ **∴ 标签写成**「更早的 run 里没有 ⇒ 疑似本轮引入」**✗
        //           ⇒ **∴ 那**是**假的**警报** ✓（**∴ 第 246 轮实测：14 条里 11 条被误标 ✓）★★
        //   **∴ 所以 ✗**：**台账已登记的**历史判定**跳过** ✓（**∴ 它本来就是"既有" ✓）★
        if (!r.inLedger) {
          if (r.seenIn && r.seenIn.length) tags.push(`既有红（${r.seenIn.length}/${historyRuns.length} 次更早的 run 同名）`);
          else if (historyRuns.length) tags.push("★ 更早的 run 里没有 ⇒ 疑似本轮引入（**也可能只是**抖动** ✓）★");
        }
        console.log(`  ✗ ${r.name}${r.exit !== null ? ` (EXIT=${r.exit})` : ""}｜${tags.join("｜")}`);
        if (r.reason) console.log(`      ↳ ${r.reason}`);
      }
    }
    const gates = rows.filter((r) => r.gpuGate && !r.inLedger);
    const fresh = rows.filter((r) => (!r.seenIn || !r.seenIn.length) && !r.inLedger);
    const registered = rows.filter((r) => r.inLedger);
    console.log("");
    console.log(
      // **∴ 三个数都要报 ✗**（**∴ 不许**把"台账里那些"混进"意外失败" ✗ —— 那会让**红被稀释** ✓）★
      `∴ 合计判据红 ${rows.length} 条 ＝ 台账已登记 ${registered.length} ＋ **意外失败 ${rows.length - registered.length}**` +
        `｜其中 GPU 门禁 ${gates.length} 条｜更早 run 里没有的 ${fresh.length} 条`,
    );
  }
}

// **∴ 判定 ✗**（**∴ 已知红台账里的**不算意外** ✓ —— 运行器也已经不阻塞它们 ✓）★
const unexpected = rows.filter((r) => !r.inLedger);
const gateRed = unexpected.filter((r) => r.gpuGate);
// **★★ 无法归类的 job ⇒ **一律按"必须修"处理** ✗ ★★**（**∴ 我**没有证据说它不挡第 4–11 项** ✓）
const unparsed = current.perJob.filter((j) => j.unparsed);
if (unparsed.length) {
  console.error(
    `✗ **有 ${unparsed.length} 个 job 红了而本工具**无法归类** ⇒ **∴ 不能声称"绿"** ✗（**∴ 退出码 1 ✓）★：` +
      unparsed.map((j) => j.job).join("、"),
  );
  process.exit(1);
}
if (unexpected.length === 0) process.exit(0);
if (gateRed.length) {
  console.error(`✗ **有 ${gateRed.length} 条 GPU 门禁判据意外红 ⇒ **按裁定必须修** ✗：${gateRed.map((r) => r.name).join("、")}`);
  process.exit(1);
}
console.error(
  `⚠ **有 ${unexpected.length} 条意外失败，但**没有一条**落在 GPU 门禁清单里** ⇒ **∴ 按裁定**：登记进 scripts/criteria-known-red.txt（四要素）即可** ✓`,
);
process.exit(4);
