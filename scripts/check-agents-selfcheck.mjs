#!/usr/bin/env node
// **∴ 「规矩要能自检」的第一个实现**✗（`AGENTS.md` 内核 ② ✓）
//   ⇒ **∴ 它**验**三条**能被机器验的规矩**✗** ⇒ **∴ 而**其余**靠**人**✗** ✓
import { readFileSync } from "node:fs";

const fail = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const check = (cond, m) => { if (cond) ok(m); else { console.log(`  ✗ ${m}`); fail.push(m); } };

const agents = readFileSync("AGENTS.md", "utf8");
const handoff = readFileSync("docs/design/HANDOFF.md", "utf8");

// ① AGENTS.md 不许有「N 条规矩」这类快照（内核 ②·别写会过期的数字）
const badCount = agents.match(/（\d+ 条规矩/g);
check(!badCount, `AGENTS.md 不许写「N 条规矩」（实测 ${badCount ? badCount.join(",") : "无"}）`);

// ② 已知红只在一处：HANDOFF.md 不许有已知红表格行（内核 ②·同一事实只写一处）
const dup = (handoff.match(/^\| .*(tool-dwell|brush parity|browser-\*)/gm) || []).length;
check(dup === 0, `已知红名单只许在 AGENTS.md（HANDOFF.md 实测 ${dup} 行）`);

// ③ **可机检的那几段**必须有自检或命令（内核 ②·规矩要能自检）
//    **∴ 只查能机检的**✗ ⇒ **∴ 其余**靠人**✗** ⇒ **∴ 而**这一点**必须**写清**✗** ✓
const MACHINE_CHECKABLE = [
  /## 判据里的 CDP/,
  /## 中文文本里一律用/,
  /## 新增代码的插入位置/,
  /## 改代码用字符串替换时/,
];
const ruled = agents.split(/^## /m).slice(1);
const targets = ruled.filter((s) => MACHINE_CHECKABLE.some((re) => re.test("## " + s)));
check(targets.length >= 3, `可机检的规矩段至少找到 3 段（实测 ${targets.length}）`);
const noCheck = targets.filter((s) => !/自检|grep|assert|check\(|node --check|fmt/i.test(s));
check(noCheck.length === 0, `可机检的规矩段都要有自检或命令（实测 ${noCheck.length} 段没有：${noCheck.map((s) => s.split("\n")[0]).join("｜")}）`);
//    **∴ 而**下面这句**是**诚实的边界**✗**：**∴ 9 段**里**只有 4 段**能机检**✗** ✓
console.log(`  可机检 ${targets.length} 段／全部 ${ruled.length} 段 ⇒ 其余靠人（内核 ②·自检）`);

// ④ **循环引用**：A 指向 B 而 B 又指回 A（内核 ②·消除重复要连元信息一起消除）
//    **∴ 第 300 轮实测**✗：**∴ AGENTS.md** 说「与 HANDOFF 第六节必须一致」✗
//    ⇒ **∴ 而** HANDOFF 说「见 AGENTS.md」✗ ⇒ **∴ 于是**成了**循环**✗** ✓
const aPointsToH = /HANDOFF\.md.*必须一致/.test(agents);
const hPointsToA = /见.*AGENTS\.md.*已知红/.test(handoff);
check(
  !(aPointsToH && hPointsToA),
  `不许循环引用（AGENTS→HANDOFF=${aPointsToH}｜HANDOFF→AGENTS=${hPointsToA}）`,
);

// ⑤ **表格类事实只许一份**：问句表 / 已知红表不许在两份文档里各写一遍
//    **∴ 第 303 轮实测**✗：**∴ AGENTS.md** 有 6 问句**✗ 而** HANDOFF.md 有 5 个**✗** ✓
const agentsRows = (agents.match(/^\| 「/gm) || []).length;
const handoffRows = (handoff.match(/^\| 「/gm) || []).length;
check(
  handoffRows === 0,
  `问句表只许在 AGENTS.md（HANDOFF.md 实测 ${handoffRows} 行）`,
);

// ⑥ **问句 ②「它下次还是同一个吗」**：不许写**会变的位置**（分片号／shard N/M）
//    **∴ 第 285 轮**✗ 我看到 shard 4/6 变 5/6 就猜"大概新红"✗ 而实测还是 tool-dwell**✗** ✓
const shardRefs = agents.match(/shard \d+\/\d+/g) || [];
const shardRefsH = handoff.match(/shard \d+\/\d+/g) || [];
check(
  shardRefs.length === 0 && shardRefsH.length === 0,
  `不许把分片号当身份（AGENTS=${shardRefs.length} 处｜HANDOFF=${shardRefsH.length} 处；它下次会变）`,
);

// ⑦ **不许在文档里写具体 HEAD**（内核 ②·别写会变的东西；第 331 轮实测 b39c341f 已过期）
const headLiterals = (agents + handoff).match(/HEAD\s*=\s*[0-9a-f]{7,8}/g) || [];
check(
  headLiterals.length === 0,
  `不许写具体 HEAD 的哈希（实测 ${headLiterals.length} 处：${headLiterals.join("｜")}）——它每次提交都会变`,
);

console.log(`\n  ${fail.length ? "✗" : "✓"} 自检 ${fail.length ? "红了" : "绿了"}（${fail.length} 处不满足）`);
process.exit(fail.length ? 1 : 0);
