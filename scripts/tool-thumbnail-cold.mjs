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
const BUDGET_MS = Number(process.env.THUMB_COLD_MS || 16);

const stamp = Date.now().toString(36);
const doc = `thumb_cold_${stamp}`;
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
const END_TO_END_BUDGET_MS = Number(process.env.THUMB_E2E_MS || 400);
if (endToEnd > END_TO_END_BUDGET_MS) {
  console.error(`  ✗ 端到端（建文档 ＋ 首次取状态）要 ${endToEnd.toFixed(2)} ms ⇒ 超过 ${END_TO_END_BUDGET_MS} ms` +
    `（建文档 ${createMs.toFixed(2)} ＋ 首次取状态 ${first.ms.toFixed(2)}）⇒ 别把一次性成本往这儿挪`);
  process.exit(1);
}
console.log(`  ✓ 端到端 ${endToEnd.toFixed(2)} ms ≤ ${END_TO_END_BUDGET_MS} ms ⇒ 成本没有被挪到别处`);
