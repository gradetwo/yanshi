#!/usr/bin/env node
// 台账的「节号」是 append-only 而**两个作者各自数自己的** ⇒ 必然撞（第 746 轮第三次踩中）
//   而这一件事一直只靠人 grep ⇒ 是「规矩要能自检」的欠账，本条把它钉成判据
// 默认读主干台账；`YANSHI_LEDGER=<路径>` 可换输入 ⇒ 变异不必弄脏主干
import { readFileSync } from "node:fs";

const LEDGER = process.env.YANSHI_LEDGER || "docs/design/brush-capability-gap.md";
// 已登记的撞号：主干本来就有的两处上游漂移（第 745 轮登记，不在我族范围）
const KNOWN_DUP = ["一百五十九", "一百六十"];

const fail = [];
let tailLine = "";
const EXPECTED = 6;
let ran = 0;
const check = (cond, m) => {
  ran++;
  if (cond) console.log(`  ✓ ${m}`);
  else { console.log(`  ✗ ${m}`); fail.push(m); }
};

let text;
try {
  text = readFileSync(LEDGER, "utf8");
} catch (e) {
  console.log(`  ⊘ 不可判：读不到台账（${LEDGER}）⇒ 判据前提不成立，不是产品失败`);
  console.log(`     原因：${e.code || e.message}`);
  process.exit(3);
}

const D = "零一二三四五六七八九";
const DIG = { 零: 0, 一: 1, 二: 2, 三: 3, 四: 4, 五: 5, 六: 6, 七: 7, 八: 8, 九: 9 };
const UNIT = { 十: 10, 百: 100, 千: 1000 };
function parseCn(s) {
  if (!/^[零一二三四五六七八九十百千]+$/.test(s)) return null;
  let total = 0, cur = 0, started = false;
  for (const ch of s) {
    if (ch in DIG) { cur += DIG[ch]; started = true; }
    else {
      const u = UNIT[ch];
      if (!started && u === 10) cur = 1;      // 「十」「十二」省略前导一
      total += cur * u;                        // **∴ 原来写的是 `(cur===0 ? 1 : cur)`**：M9 变异证明那条**永远走不到** ⇒ 删掉
      cur = 0; started = true;
    }
  }
  return total + cur;
}
function emitCn(n) {
  if (!(n > 0) || n >= 10000) return null;
  let out = "", started = false, needZero = false;
  for (const [u, base] of [["千", 1000], ["百", 100], ["十", 10]]) {
    const d = Math.floor(n / base) % 10;
    if (d) {
      if (needZero && started) out += "零";
      needZero = false;
      // 「十」「十二」不带前导一（本台账 1–19 就是这么写的）
      if (!(d === 1 && base === 10 && !started)) out += D[d];
      out += u;
      started = true;
    } else if (started) needZero = true;
  }
  const o = n % 10;
  if (o) {
    if (needZero && started) out += "零";
    out += D[o];
  }
  return out;
}

// 节头 = 以「## 」开头的行；号必须是「中文数字 + 、」
// **∴ 不用 `(\S+)、`**：那样 `\S+` 会一路吞到**最后一个**顿号，把标题本身当成号（实测过）
const allHeaders = text.split("\n").filter((l) => l.startsWith("## "));
if (allHeaders.length === 0) {
  console.log("  ⊘ 不可判：这份输入里一条「^## 」也没有 ⇒ 判据前提不成立（不是产品失败）");
  console.log("     要么路径给错了，要么台账的节头格式整体变了");
  process.exit(3);
}
const headers = [];
for (const line of allHeaders) {
  const m = /^## ([零一二三四五六七八九十百千]+)、(.+)$/.exec(line);
  if (m) headers.push({ num: m[1], title: m[2] });
}

// ① 每条节头都带「中文数字号 + 非空标题」（带不出的那条**不进**后面三条的分母 ⇒ 必须当场红）
const malformed = allHeaders.filter(
  (l) => !/^## [零一二三四五六七八九十百千]+、.+$/u.test(l)
);
check(
  malformed.length === 0,
  `每条「^## 」都是「中文数字、标题」（实测 ${allHeaders.length} 条里 ${malformed.length} 条不是${malformed.length ? "：" + malformed.slice(0, 3).map((l) => l.slice(0, 24)).join("｜") : ""}）`
);
// ①b 解析器自测：1..999 每个整数 encode→parse 必须回到原值
//    **∴ 为什么不是「收到的号都解析得出」**：那条在正则筛过之后**恒绿**（解析不出的一类已被①拦下）
//    ⇒ 恒绿的断言是安慰剂（第 261 轮），所以换成**对尺子本身的测量**
const rt = [];
for (let n = 1; n < 1000; n++) {
  const s = emitCn(n);
  if (parseCn(s) !== n) rt.push(`${n}→${s}→${parseCn(s)}`);
}
check(
  rt.length === 0,
  `号与整数互转对 1..999 全部可逆（实测 ${rt.length} 个不可逆${rt.length ? "：" + rt.slice(0, 3).join("｜") : ""}）`
);

// ② 撞号必须在已登记名单里（本轮抓到的正是这一类：上游用了 458/459，我的族同号）
const counts = new Map();
for (const h of headers) counts.set(h.num, (counts.get(h.num) || 0) + 1);
const dups = [...counts.entries()].filter(([, c]) => c > 1).map(([n]) => n);
const fresh = dups.filter((n) => !KNOWN_DUP.includes(n));
check(
  fresh.length === 0,
  `未登记的撞号为 0（总撞号 ${dups.length}｜已登记 ${dups.length - fresh.length}｜新增 ${fresh.length}${fresh.length ? "：" + fresh.join("｜") + " ⇒ 整体顺移或显式登记理由" : ""}）`
);

// ②b 登记的撞号必须**仍然**撞着（「登记」也是一次快照 ⇒ 上游修好后这条名单会变陈旧 ✓）
const staleReg = KNOWN_DUP.filter((n) => (counts.get(n) || 0) < 2);
check(
  staleReg.length === 0,
  `已登记的撞号仍然撞着（名单 ${KNOWN_DUP.length} 条｜已过期 ${staleReg.length}${staleReg.length ? "：" + staleReg.join("｜") + " ⇒ 它们已经不撞了，从名单里删掉" : ""}）`
);
// ③ 交叉引用指的号必须真存在（重编号时漏改一处 ⇒ 指向不存在的东西）
//    **∴ 要带两侧边界**：台账里嵌着 `grep '^## [零一二三四五六七八九十百千]+、'` 这种**命令原文**，
//    不加边界会把它读成一条引用（实测过）
const known = new Set(counts.keys());
const NUM = "[零一二三四五六七八九十百千]";
const refs = [];
for (const m of text.matchAll(new RegExp(`§\\s*(${NUM}{2,6})(?!${NUM})`, "g"))) refs.push(m[1]);
for (const m of text.matchAll(new RegExp(`\\[\\s*(${NUM}{2,6})(?!${NUM})\\s*\\]`, "g"))) refs.push(m[1]);
const dangling = refs.filter((n) => !known.has(n));
check(
  dangling.length === 0,
  `每个「§号」与「[号]」都指向真存在的节（引用 ${refs.length} 处｜悬空 ${dangling.length}${dangling.length ? "：" + [...new Set(dangling)].slice(0, 5).join("｜") : ""}）`
);

// ④ 号与整数互转一致（单向读法会把「四百七十」与「四百七」混成一类 ⇒ 测量本身先被测量）
const roundBad = headers
  .filter((h) => {
    const n = parseCn(h.num);
    return n !== null && emitCn(n) !== h.num;
  })
  .map((h) => `${h.num}→${parseCn(h.num)}→${emitCn(parseCn(h.num))}`);
check(
  roundBad.length === 0,
  `节号双向互转一致（实测 ${roundBad.length} 个不 round-trip${roundBad.length ? "：" + roundBad.slice(0, 3).join("｜") : ""}）`
);

const maxN = Math.max(...headers.map((h) => parseCn(h.num) || 0));
// 恒等式：跑到的断言数必须等于设计数 —— 少跑一条就是「绿了却什么也没验」✓
if (ran !== EXPECTED) {
  console.log(`  ⊘ 不可判：应当跑 ${EXPECTED} 条断言，实际跑到 ${ran} 条 ⇒ 判据自己坏了，不是产品失败`);
  process.exit(3);
}
console.log(`  ✓ 断言条数恒等式成立（跑到 ${ran}／应 ${EXPECTED}）`);
tailLine =
  `节 ${allHeaders.length}（格式对 ${headers.length}）｜号 ${counts.size} 种｜` +
  `撞号 ${dups.length}（登记内 ${dups.length - fresh.length}，新增 ${fresh.length}，登记过期 ${staleReg.length}）｜` +
  `引用 ${refs.length}（悬空 ${dangling.length}）｜最大号 ${maxN}｜断言 ${ran}/${EXPECTED}`;
console.log(`↳ ${tailLine}`);
process.exit(fail.length ? 1 : 0);
