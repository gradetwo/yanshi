//! **★ 部分复用的过渡期收益（**4K，release ✓）★**（第 15 轮 ✓；**阶段一第 1 项的实测 ✓**）。
//
// **∴ 为什么要单独立这个脚本 ✗**：**部分命中**是**过渡态**✗
//   —— 缓存**从「有一些格」到「全有格」之间的那一次 ✓**
//   ⇒ **∴ 稳态（**全有命中 ✓）下它的收益是 0 ✓**（**∵ 那时**本来就绕过一切 ✓）**
//     ⇒ **★ 所以它的价值不在吞吐 ✗，**而在"**过渡期不整块作废 ✓" ★**** ✓✓
//   ⇒ **∴ 所以**：**必须**专测**那一次**✗，**而**不是**跑稳态做平均 ✓**** ✓✓
//
// **∴ 借鉴对应 ✗**：**GIMP 的 invalid region** ✗（**有效性属于 tile ✓）
//   ⇒ **∴ 于是**：**"**有几格有效 ⇒ **就用那几格 ✓"** ✗
//     ⇒ **∴ 而**原来的实现是「缺一格 ⇒ 整块作废 ✓」 ✓**** ✓✓
//
// **∴ 用法 ✗**：`node scripts/tool-partial-reuse-benefit.mjs <base-url>` ✓
//   **∴ 环境 ✗**：**服务端**默认**打开部分复用 ✓；**用**`YANSHI_NO_PARTIAL_BELOW=1`**关掉做对照 ✓**
//
// **∴ 判读 ✗**：
//   **∴ 开 ✗**：**大区域那一次**的 `below_tiles_reused > 0`** ✓
//   **∴ 关 ✗**：**同一次**是 `0`** ✓ ⇒ **∴ 两者**必须**不同**✗
//     ⇒ **∴ 若**相同 ⇒ **★ 场景没生效 ✗（**第 14 轮的教训 ✓）★**** ✓✓

// **★ `argv[2]` **可能是一个标志**✗**（第 91 轮 ✓；**本地复现：`input: '--spawn/api/documents'` ✓**）：
//   **∴ 症状 ✗**：**我**跑 `node scripts/tool-partial-reuse-benefit.mjs --spawn`**✗
//     ⇒ **∴ `base` ＝ `"--spawn"`**✗ ⇒ **∴ 于是**：**URL**解析失败 ⇒ **`ERR_INVALID_URL` ✓**** ✓✓
//   **∴ 修法**：**跳过**以 `--` 开头的参数**✗
//     ⇒ **∴ 取**第一个**看起来像 URL** 的参数 ✓**** ✓✓
const __urlArg = process.argv.slice(2).find((a) => /^https?:\/\//.test(a));
const base = __urlArg ?? "http://127.0.0.1:8471";
const CANVAS = 4096;
// **∴ below 的 tile 是 256 ✗**（`BELOW_TILE` ✓）⇒ **∴ 区域要**跨 256 边界 ✓** ✓✓
// **★ 大区域必须**明显更大** ✗ ★**（第 15 轮实测 ✓）：**小区域的 `want` 含**外扩
//   （filter padding ✓）⇒ **∴ 若**两者尺寸接近 ⇒ **∴ 小区域**已经把大区域要的格**都缓存了 ✓
//     ⇒ **∴ 于是** `missing = 0`**✗ ⇒ **∴ 这次测量**不成立 ✓**（**脚本会**拒绝它 ✓）** ✓✓
const SMALL = { x: 0, y: 0, w: 256, h: 256 };
const BIG = { x: 0, y: 0, w: 2048, h: 512 };
const ROUNDS = 3;

async function api(path, body) {
  const res = await fetch(`${base}${path}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  return res.json();
}

async function oneRound(i) {
  const docId = `partial_${i}_${Date.now()}`;
  const doc = await api("/api/documents", {
    doc_id: docId,
    width: CANVAS,
    height: CANVAS,
  });
  const token = doc.token ?? doc.capability ?? doc.capability_token;
  const q = `doc=${docId}&token=${token}`;
  for (const id of ["L0", "L1", "L2"]) {
    await api(`/api/tools/create_layer?${q}`, { layer_id: id });
  }
  await api(`/api/tools/fill?${q}`, {
    layer_id: "L1",
    object_id: "o1",
    data: {
      color: { r: 40, g: 90, b: 160, a: 210 },
      region: { x: 8, y: 8, w: 1200, h: 600 },
    },
  });
  // **① 先渲小区域 ⇒ 缓存里因此只有它的格。**
  await api(`/api/tools/render_region?${q}`, {
    region: SMALL,
    active_layer: "L2",
  });
  // **② 再渲大区域 ⇒ 这一次**本该**是部分命中。**
  const t0 = process.hrtime.bigint();
  const big = await api(`/api/tools/render_region?${q}`, {
    region: BIG,
    active_layer: "L2",
  });
  const t1 = process.hrtime.bigint();
  return {
    ms: Number(t1 - t0) / 1e6,
    reused: big.below_tiles_reused ?? 0,
    available: big.below_tiles_available ?? 0,
    wanted: big.below_tiles_wanted ?? 0,
    missing: big.below_tiles_missing ?? 0,
  };
}

const rows = [];
for (let i = 0; i < ROUNDS; i += 1) {
  rows.push(await oneRound(i));
}

const sum = (k) => rows.reduce((a, r) => a + r[k], 0);
const msSum = sum("ms");
console.log("  轮次明细：");
for (const [i, r] of rows.entries()) {
  console.log(
    `    ${i + 1}: ${r.ms.toFixed(1)} ms｜wanted=${r.wanted} available=${r.available} missing=${r.missing} reused=${r.reused}`,
  );
}
console.log(
  `  合计：${msSum.toFixed(1)} ms｜reused 合计 ${sum("reused")}｜available 合计 ${sum("available")}`,
);

// **∴ 判读 ✗**：**必须有**部分命中**✗，**否则**这次测量**不成立 ✓** ✓✓
// **★ 不变式：`available + missing == wanted` ✗ ★**（第 24 轮 ✓）：
//   **∴ 为什么必须有它 ✗**：**我**曾连续两轮**被"**`available < wanted` 而 `missing = 0` ✓"**误导**✗
//     ⇒ **∴ 真相**是**`missing`**根本没被暴露**✗ ⇒ **∴ 脚本**读 `?? 0` ⇒ **∴ 恒 0 ✓**
//       ⇒ **★ 所以**：**自检里**必须显式断言这条不变式 ✗**
//         ⇒ **∴ 于是**：**字段缺失**或**跨渲染混合**都会**当场暴露 ✓ ★**** ✓✓
for (const [i, r] of rows.entries()) {
  if (r.available + r.missing !== r.wanted) {
    console.error(
      `  ❌ 第 ${i + 1} 轮四数不自洽：available(${r.available}) + missing(${r.missing}) != wanted(${r.wanted})`,
    );
    console.error("     ⇒ **∴ 要么**字段缺失 ✓，**要么**它们**来自不同的渲染 ✓");
    process.exit(1);
  }
}
if (sum("missing") === 0) {
  console.error(
    "  ❌ 这次一条缺格都没有 ⇒ **∴ 场景**没生效 ✓（**∴ 不是**「部分复用没用 ✓」）",
  );
  process.exit(1);
}
if (sum("reused") === 0) {
  console.error(
    "  ❌ 有缺格而**一格都没复用** ⇒ **∴ 部分复用**没生效 ✓（**∴ 对照：**用 YANSHI_NO_PARTIAL_BELOW=1 时应当正是这个结果 ✓）",
  );
  process.exit(1);
}
console.log("  ✓ 部分复用已生效：有缺格，且**已有格**被用上了 ✓");
