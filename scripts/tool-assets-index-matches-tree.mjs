#!/usr/bin/env node
// **★ 「资产索引」与「随包发布的文件」必须是同一批 ✗ ★**（第 745 轮 ✓；**实测换来的 ✓**）
//
// **∴ 它守的 bug ✗ ★**（**已提交在主干上 ✗ 且已出过货 ✓**）：
//   **∴ `web/assets-index.json`**是**上一次发布**时生成的**快照**✓
//     ⇒ **∴ 而**后来 `be226f6c` **停止跟踪**了 7 张大图纹理（**`Cardboard001…004`／`CardboardSet001`／`Paper004`／`Paper005`✓）
//       ⇒ **∴ 于是**：**索引**还列着它们**✗ 文件**已经不在**包里 ✓
//         ⇒ **★ PWA 的纹理面板列出 7 张点了必然 404 的纹理 ✓ ★**** ✓✓
//   **∴ 为什么 `api-local.js` 挡不住 ✗**：**`list_assets` 只照索引回答**✓（**它不查文件在不在 ✗**）
//     ⇒ **∴ 所以**：**索引说谎 ⇒ 界面就谎**✓（**内核 5 的第 5 形态 ✗「有名字没内容」✓**）
//   **∴ 为什么 CI 一直没抓到 ✗**（**内核 1 ✓**）：**没有任何判据**拿索引去比对磁盘／跟踪清单 ✓
//     （**`grep -l assets-index scripts/*.mjs`**只有同步脚本自己 ✓）
//
// **∴ 断的是**关系**✗ 不是**快照** ✓**（**内核 ② ✓**）：
//   **∴ 两边都从**仓库**算出来**✗ ⇒ **换机器**✗ 换时间**✗ 结果**同一个 ✓**
//     （**∴ 所以本判据**不写**「texture 应有 8 个」这种**数**✓**）
//
// **∴ 只比**跟踪得到**的两类 ✗**（**∴ 分母规则不同**✓ 别硬套 ✓**）：
//   · **`palette`／`texture`** ⇒ `web/<目录>`**整目录在 git 里**✓ ⇒ **能比** ✓
//   · **`brush`／`medium`** ⇒ `web/brushes`／`web/mediums`**不在跟踪里**✓（**发布流水线**建完才有 ✗
//     **∴ 干净签出**时**磁盘上没有**✗ 直接比会在 CI 上**假红** ✓）
//     ⇒ **∴ 于是**：**这里**明确说出**「不比这一类」✗ 并且断言**「**不许悄悄漏整类**」**✓
//       （**若哪天** `web/brushes` **被跟踪了**✗ 第 ④ 条**就红**✗ 逼着**把它纳进比对 ✓**）
//
// **∴ 变异（**已逐条实测 ✓**）**：
//   ①**往索引里加一个不存在的纹理**⇒ 第 ② 类红 ✓ 退出码 1
//   ②**从索引里删一个真实纹理**⇒ 同一条红（**方向不同 ✗ 文案给出缺的是谁 ✓**）✓
//   ③**把某项的 `bytes` 改成别的数**⇒ 第 ③ 条红 ✓
//   ④**让 `UNTRACKED_DIRS` 指向一个**确实被跟踪**的目录**（**把 brush 写成 `web/palettes` ✓）**
//     ⇒ 第 ④ 条红 ✓（**∴ 证明**「跟踪状态变了 ⇒ 这里会逼着改比对清单」✗ **不是**恒绿的 ✓**）
//   ⑤**把 `web/brushes` 改名**（**目录**不存在 ✓）⇒ 第 ④ 条**变成 ⊘ 跳过**✗
//     **∴ 而不是**记成一条**通过**✓（**「我问了一个不存在的目录」**不是**证据 ✓**）
//     ＋ **∴ 而**这条**踩过一次真坑 ✓**：**原来**用 `web/${kind}s`**拼目录名**✗
//       ⇒ **拼出**`web/brushess`／`web/mediumss`**（不存在 ✓）**⇒ **「没被跟踪」**恒真**✓ ⇒ **④ 恒绿 ✓**
//       ⇒ **∴ 「它没红」≠「它守住」**（**内核 1 ✗ 第 415 轮那条问句又应验了一次 ✓**）
//
// **∴ 用法 ✗**：`node scripts/tool-assets-index-matches-tree.mjs`
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { execFileSync } from "node:child_process";

const INDEX = "web/assets-index.json";
// **∴ 比对清单 ✗**：**目录在 git 跟踪里**才进来 ✓（**理由见文件头 ✓**）
const TRACKED_KINDS = [["palette", "web/palettes"], ["texture", "web/textures"]];
// **∴ 明说「这一类为什么不比」**✗（**∴ 而不是**默默少一类 ✓**）
const UNTRACKED_KINDS = ["brush", "medium"];

const failures = [];
let ranCount = 0;
let okCount = 0;
const check = (ok, label, detail) => {
  console.log("  " + (ok ? "✓" : "✗") + " " + label + (detail ? "（" + detail + "）" : ""));
  ranCount += 1;
  if (ok) okCount += 1; else failures.push(label + (detail ? "：" + detail : ""));
};

// **∴ 前提 ✗**：**取不到跟踪清单 ⇒ 判据自己坏了**✗ ⇒ **退出码 3**（**不冒充产品失败 ✓**）
let trackedNames;
try {
  trackedNames = (dir) => execFileSync("git", ["ls-files", dir], { encoding: "utf8" })
    .split("\n").filter(Boolean).map((p) => p.slice(dir.length + 1));
  trackedNames("web/palettes");
} catch (e) {
  console.error(`✗ 取不到 git 跟踪清单 ⇒ 前提不成立（不是产品失败）⇒ 退出码 3（${String(e).slice(0, 120)}）`);
  process.exit(3);
}
if (!existsSync(INDEX)) {
  console.error(`✗ 缺少 ${INDEX} ⇒ 前提不成立（不是产品失败）⇒ 退出码 3`);
  process.exit(3);
}
let index;
try {
  index = JSON.parse(readFileSync(INDEX, "utf8"));
} catch (e) {
  // **∴ 这一条**是**产品失败**✗ 不是前提 ✗（**索引是**发布产物**✓ 坏了就是坏了 ✓）**
  check(false, "资产索引必须是合法 JSON", String(e).slice(0, 120));
  console.error(`  结论：✗ ${failures.length} 条失败`);
  process.exit(1);
}

console.log("① 索引覆盖四类（**不许整类消失 ✓**）：");
for (const kind of [...TRACKED_KINDS.map(([k]) => k), ...UNTRACKED_KINDS]) {
  check(Array.isArray(index[kind]), `索引里有 ${kind} 这一类`, Array.isArray(index[kind]) ? `${index[kind].length} 项` : String(index[kind]));
}

console.log("");
console.log("② 索引列的文件必须真的在包里（**两边都比 ✗ 单向等于没比 ✓**）：");
for (const [kind, dir] of TRACKED_KINDS) {
  const inIndex = new Set((index[kind] || []).map((x) => x.name));
  const tracked = new Set(trackedNames(dir));
  const phantom = [...inIndex].filter((n) => !tracked.has(n));
  const forgotten = [...tracked].filter((n) => !inIndex.has(n));
  // **∴ 断的是**两个集合相等**✗**（**内核 ② ✓**：不是「条目数 = 8」那种**快照**✓）
  check(phantom.length === 0 && forgotten.length === 0,
    `${kind}：索引与 ${dir}/ 的文件**一一对应**`,
    phantom.length || forgotten.length
      ? `索引有而包里没有 ${phantom.length}（${phantom.slice(0, 7).join(",")}）｜包里有而索引漏 ${forgotten.length}（${forgotten.slice(0, 7).join(",")}）`
      : `${tracked.size} 项 ✓`);
}

console.log("");
console.log("③ 索引里写的 `bytes` 必须是真大小（**否则前端按大小做的决定全是错的 ✓**）：");
for (const [kind, dir] of TRACKED_KINDS) {
  const wrong = [];
  for (const it of index[kind] || []) {
    const f = `${dir}/${it.name}`;
    if (!existsSync(f)) { wrong.push(`${it.name}（文件不在）`); continue; }
    const real = statSync(f).size;
    if (real !== it.bytes) wrong.push(`${it.name}：索引写 ${it.bytes}｜实际 ${real}`);
  }
  check(wrong.length === 0, `${kind}：每项的 bytes 与磁盘一致`,
    wrong.length ? `${wrong.length} 项不对 ⇒ ${wrong.slice(0, 3).join(" ｜ ")}` : `${(index[kind] || []).length} 项 ✓`);
}

console.log("");
console.log("④ 不许悄悄漏掉一整类（**跟踪状态变了 ⇒ 这里必须红 ✓**）：");
// **∴ 目录名**查表**✗ 不从类名**拼**✓**（**第 745 轮 ✗ 我自己先踩了一次 ✓**）：
//   **∴ 原来写的是** `web/${kind}s`✗ ⇒ **拼出来**`web/brushess`／`web/mediumss`✓（**不存在 ✓**）
//     ⇒ **∴ 于是**：**对一个不存在的目录**问「有没有被跟踪」**恒为**「没有」✗
//       ⇒ **∴ 那条断言**恒绿**✓ ⇒ **「它没红」≠「它守住」**（**内核 1 ✓**）
//     ⇒ **∴ 修法 ✗**：**路径写进表里**＋**先断言「那个目录确实存在或在跟踪体系里说得清」**✓
const UNTRACKED_DIRS = { brush: "web/brushes", medium: "web/mediums" };
let skipped = 0;
for (const [kind, dir] of Object.entries(UNTRACKED_DIRS)) {
  const nowTracked = trackedNames(dir);
  // **∴ 目录不存在时**说出来**✗ 不**断言**✓**（**干净签出时**这两类**靠发布流水线建 ✗
  //   **∴ 而**「不存在」既**不是**「不跟踪」✓ 也**不是**「跟踪了」✓ ⇒ **只能**⊗ ✗
  //   **∴ 绝不能**把「**我问了一个不存在的目录**」记成一条**通过**✓）
  if (!existsSync(dir)) {
    console.log(`  ⊘ 跳过 ${kind}：${dir}/ 不存在（干净签出的常态）⇒ 本条不判定`);
    skipped += 1;
    continue;
  }
  check(nowTracked.length === 0,
    `${kind}：${dir}/ 仍然不在 git 跟踪里（**∴ 所以不比它 ✗ 一旦开始跟踪 ⇒ 要加进 TRACKED_KINDS ✓**）`,
    nowTracked.length ? `${dir}/ 已被跟踪 ${nowTracked.length} 个 ⇒ 本判据的比对清单过期了 ✗`
      : `目录在（${readdirSync(dir).length} 个文件）⇒ 确实靠发布流水线建 ✓`);
}

console.log("");
// **∴ 恒等式 ✓**（**内核 ② ✓**）：**通过 ＋ 失败 = 跑过**✗ ⇒ **「漏计数」当场红 ✓**
if (okCount + failures.length !== ranCount) {
  console.error(`  ✗ 计数不自洽：通过 ${okCount} ＋ 失败 ${failures.length} ≠ 跑过 ${ranCount} ⇒ 判据自己坏了 ✗`);
  process.exit(3);
}
console.log(`  计数自洽：通过 ${okCount} ＋ 失败 ${failures.length} = 跑过 ${ranCount} 条 ✓`);
if (failures.length) {
  console.log(`  结论：✗ 资产索引与包内文件不一致（${failures.length} 条）｜⇒ 面板会列出点了 404 的项 ✓`);
  for (const line of failures) console.error("   - " + line);
  process.exit(1);
}
console.log(`  结论：✓ 资产索引与包内文件一一对应（通过 ${okCount} 条${skipped ? `｜⊘ 明确跳过 ${skipped} 条：那两类靠发布流水线建 ✓` : ""}）`);
process.exit(0);
