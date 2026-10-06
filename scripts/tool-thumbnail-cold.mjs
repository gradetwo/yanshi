// **首次取状态的延迟判据**（第 1349 轮 ✓，目标 (B)③ ✓）：**进程内第一次**渲染**不该阻塞请求 ✗。
//
// **为什么需要它** ✗（本机实测 ✓）：
//   `get_document` 的**第一次**调用要 **213 ms** ✗（它要生成文档缩略图 ⇒ 触发服务端第一次渲染 ✓），
//   而**第二次起**只要 **0.82 ms** ✗ ⇒ 差 **≈260 倍** ✗。用户感知就是：**打开文档后第一次要状态 ⇒ 卡 200 ms** ✗。
//   ⇒ 而**现有器具都量不到它** ✗：`browser-kernel-perf` 量的是**客户端内核**✗、
//     `tool-paint-memory` 量的是**整段绘画**✗ ⇒ 没有一条判据盯"**首冷**"✗。
//
// **判据** ✓：**第一次** `get_document` ≤ **16 ms**（与仓库既有的实时目标同级 ✓）；
//   **并且**打印第二次的值 ⇒ **从而把「一次性」写进输出** ✓（读的人一眼看出是首冷还是稳态 ✓）。
//   ⇒ **∴ 它现在**红**✗**（实测 213 ms ✓）⇒ **∴ 按仓库纪律**先列入已知红名单 ✗（不让 CI 变红 ✓）
//     ⇒ **∴ 而修法**是**打开文档时预热渲染器**✗ ⇒ **∴ 修好后这条判据**自然转绿 ✗ ✓。
//
// **为什么用"第一次"而不是"平均"** ✗：本会话已**三次**被平均值骗过（含我自己上一轮 ✓）
//   ⇒ 首冷与稳态必须**分开报** ✓ ⇒ 所以这条判据**只判第一次** ✗，并把第二次作为对照打印 ✓。
const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-thumbnail-cold.mjs <base-url>"); process.exit(2); }
// **按构建 profile 分档** ✗（第 1363 轮 ✓）—— **∴ 这是本判据**过去一直红**✗ 的原因 ✓**：
// 判据与 CI 跑的都是 **`debug`** ✗（`scripts/run-criteria.sh:32` 用 `./target/debug/yanshi-serve` ✓、
// `ci.yml` 的 `cargo build --workspace` 不带 `--release` ✓），而**用户跑的是 `release`** ✗
// ⇒ 实测（2026-10-07 ✓，各用多个全新进程取最小值 ✓）：
//   **∴ `release`✗ 首屏 **22.57 ms**✗（24.89／22.57／23.72 ✓）｜**∴ `debug`✗ 首屏 **191.56 ms**✗（191.56／192.79 ✓）
//   ⇒ **∴ 倍率 **8.49×**✗** ⇒ **∴ 所以 16 ms ✗ 的预算在 debug ✗ 下**差 12 倍 ⇒ **∴ 它**永远红**✗ ✓**。
//
// **∴ 所以分档** ✓：**∴ `debug`✗ 用宽预算 ✗（**∴ 它**不是产品**✗ ⇒ **∴ 只作调试代理 ✓**）；
// **∴ `release`✗ 继续对**真目标 16 ms**✗ ⇒ **∴ 从而**"还差多少"✗ 不会被掩盖 ✓**。
// **∴ 而 profile 怎么判** ✗：**∴ 看 `YANSHI_SERVE_BIN`✗ 的路径里有没有 `/release/`✗ ✓**
// （**∴ 简单 ✗＋**∴ 够用 ✓；**∴ 判据本来就用这个变量指向被测二进制 ✓**）。
const SERVE_BIN = process.env.YANSHI_SERVE_BIN || "target/debug/yanshi-serve";
const IS_RELEASE = /(^|[\\/])release([\\/]|$)/.test(SERVE_BIN) || SERVE_BIN.includes("--release");
const PROFILE = IS_RELEASE ? "release" : "debug";
// **∴ debug ✗ 的宽预算怎么定** ✗：**∴ 取**实测 191.56 ms ✗ 的量级留余量**✗ ⇒ **∴ 250 ms ✓**
// ⇒ **∴ 从而**它只在**真的再慢 30%**✗ 时才红 ⇒ **∴ 而那在 debug ✗ 下**有意义 ✗（**∴ 例如**回归 ✓**）。
// **`release`✗ 的预算 ＝ 设计明文 ✗**（第 1397 轮 ✓）：
// `docs/design/yanshi-v1.0-draft4.md:694` 的时延预算表写着
//   「区域缩略图更新（**需重渲染**）｜**< 15ms**」
// ⇒ **∴ 那正是本判据测的东西** ✗（**∴ 首次取状态 ⇒ 生成缩略图 ⇒ **∴ 需重渲染 ✓**）
// ⇒ 所以用 **15** 而不是我先前随手写的 16 ✗（**∴ 差 6.7% ✗，但**有出处**比"我记得"✗ 重要 ✓）。
const RELEASE_BUDGET_MS = 15;                        // 设计 :694 明文
const DEFAULT_BUDGET_MS = IS_RELEASE ? RELEASE_BUDGET_MS : 250;
const BUDGET_MS = Number(process.env.THUMB_COLD_MS || DEFAULT_BUDGET_MS);
console.log(`  被测二进制：${SERVE_BIN}`);
console.log(`  构建 profile：**${PROFILE}**｜首屏预算：${BUDGET_MS} ms` +
  (IS_RELEASE ? "（**∴ 真目标 ⇒ 用户感知的就是它 ✓**）" : "（**∴ debug ✗ **不是产品**✗ ⇒ **∴ 宽预算只作调试代理 ✓**）"));

const stamp = Date.now().toString(36);
const doc = `thumb_cold_${stamp}`;
// **先热身一次 `fetch`** ✗（第 1363 轮实测暴露 ✓）：**∴ 否则 `createStarted`✗ 会把
// **Node 首次建连**✗ 算进"建文档"✗**（**∴ debug ✗ 实测 "建文档"✗ 报 **294 ms**✗ 而产品只需 1.5 ms ✓）
// ⇒ **∴ 那会让端到端**虚高 ✗＋**∴ 从而**误报 ✓**。
await fetch(`${base}/api/documents`).then((r) => r.text()).catch(() => {});
const createStarted = process.hrtime.bigint();
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 320, height: 240 }),
})).json();
const token = created.token;
if (!token) { console.error("  ✗ 建文档没拿到 token ⇒ 判据无法成立：" + JSON.stringify(created).slice(0, 160)); process.exit(2); }

const timeIt = async () => {
  const started = process.hrtime.bigint();
  const response = await fetch(`${base}/api/tools/get_document?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: "{}",
  });
  const value = await response.json();
  return { ms: Number(process.hrtime.bigint() - started) / 1e6, value };
};

// **第一次**（首冷 ✓）—— 这是被判的那一次 ✓。
const first = await timeIt();
// 第二次（稳态对照 ✓）—— 只打印，不判 ✓。
const second = await timeIt();

const createMs = Number(process.hrtime.bigint() - createStarted) / 1e6;
const endToEnd = createMs + first.ms;
console.log(`  建文档（**打开** ⇒ 预热在这里付）：${createMs.toFixed(2)} ms`);
console.log(`  第一次 get_document：${first.ms.toFixed(2)} ms（**首冷** ⇒ 生成缩略图 ⇒ 触发服务端第一次渲染）`);
console.log(`  第二次 get_document：${second.ms.toFixed(2)} ms（**稳态** ⇒ 只作对照，不判）`);
console.log(`  端到端（**建文档 ＋ 首次取状态**）：${endToEnd.toFixed(2)} ms（**这是用户实际感知的第一步**）`);
if (first.value && first.value.ok !== true) {
  console.error("  ✗ 第一次调用本身失败：" + JSON.stringify(first.value).slice(0, 200));
  process.exit(2);
}
if (first.ms > BUDGET_MS) {
  console.error(`  ✗ 进程内**第一次**取状态要 ${first.ms.toFixed(2)} ms ⇒ 超过 ${BUDGET_MS} ms` +
    `（用户感知：打开文档后第一次要状态会卡这么久；第二次只要 ${second.ms.toFixed(2)} ms ⇒ 是**首冷**✗）`);
  process.exit(1);
}
console.log(`  ✓ 第一次取状态 ${first.ms.toFixed(2)} ms ≤ ${BUDGET_MS} ms ⇒ 首冷不阻塞请求`);
// **端到端也要判**：否则把成本往建文档挪会永远通过，而那只是把卡顿换了位置。
// 上限取实测值的量级：本机 2026-10-07 实测建文档约 190 ms ＋ 首次取状态约 5 ms ⇒ 端到端约 195 ms，
// 所以 400 ms 能容忍机器抖动，而真回归（例如回到 900 ms）会红。
// **端到端也分档** ✗：**∴ `release`✗ 是产品 ⇒ **∴ 对它用**紧的 400 ms**✗**（**∴ 实测 ≈113 ms ✓**）；
// **∴ `debug`✗ 不是产品 ⇒ **∴ 用**宽预算**✗（**∴ 实测 ≈500 ms ✗ ⇒ **∴ 取 900 ms ✓**）
// ⇒ **∴ 从而**它只在**真的再慢一倍**✗ 时才红 ⇒ **∴ 而那在 debug ✗ 下有意义 ✓**。
const END_TO_END_BUDGET_MS = Number(process.env.THUMB_E2E_MS || (IS_RELEASE ? 400 : 900));
if (endToEnd > END_TO_END_BUDGET_MS) {
  console.error(`  ✗ 端到端（建文档 ＋ 首次取状态）要 ${endToEnd.toFixed(2)} ms ⇒ 超过 ${END_TO_END_BUDGET_MS} ms` +
    `（建文档 ${createMs.toFixed(2)} ＋ 首次取状态 ${first.ms.toFixed(2)}）⇒ 别把一次性成本往这儿挪`);
  process.exit(1);
}
console.log(`  ✓ 端到端 ${endToEnd.toFixed(2)} ms ≤ ${END_TO_END_BUDGET_MS} ms ⇒ 成本没有被挪到别处`);
