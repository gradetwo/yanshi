#!/usr/bin/env node
// **`browser-*` 判据连 CDP 的端口，不许取自「argv[3] 那个 URL 槽」**（外§117⑨ 的守卫 ✓）
//
// **⇒ 背景（**CI 实测换来的 ✗**）**：`run-criteria.sh:389` 给每条 `browser-*` 传**四个**参数：
//   · `argv[2]` ＝ viewer／带 doc 的 URL ✓
//   · **`argv[3]` ＝ 服务端 base ⇒ 它是个 URL** ✗ 不是端口 ✗
//   · `argv[4]` ＝ 令牌 ✓ ＋ **`argv[5]` ＝ CDP 端口** ✓（并且 `CDP_PORT` 已 `export` ✓ 第 966 轮）
//   ⇒ 曾有一条写成 `process.argv[3] || "9222"` ⇒ 拼出
//   `http://127.0.0.1:http://127.0.0.1:13990/json/list` ⇒ `Failed to parse URL` ⇒ **`exit 3`** ✓
//
// **⇒ 为什么必须机检**：那条判据的失败表现是**「跳过」**✓ 而**板面不红** ✗
//   ⇒ **「做了 0 次」冒充「不需要做」**（静默降级**形态⑤** ✓）⇒ 全绿的 CI 上一条判据其实**什么都没验** ✓
//   ⇒ 同类写法在这一族里**反复出现过**（`browser-blend-modes`／`browser-fill-region`／
//     `browser-boot-panels`／`browser-drag-perf` 的注释各自记了一次 ✓ ⇒ **grep 得到、次数随修随变**，
//     所以不写死数字 ✓ 内核 ②）⇒ **所以**别再靠人记得 ✓
//
// **⇒ 判什么**（两条都是**开集**断言 ⇒ 新加的 `browser-*` 用错槽位会当场红 ✓）：
//   ① 文件连 CDP ⇒ 端口来源**三者其一**：读 `process.env.CDP_PORT` ✓
//     ／ 自起浏览器（`--remote-debugging-port` ✓）／ 用**带 `/^\d+$/` 校验的** `argv[5]` ✓
//   ② **端口变量**（`port`／`cdpPort`／`debugPort`／`rawPort`／`portArg`）的赋值里出现
//     `process.argv[3]` ⇒ 该行**或紧邻两行内**必须有 `/^\d+$/` 校验 ✓
//     （**只盯端口变量**，不是「文件里任何 `argv[3]`」⇒ 因为绝大多数判据**正当地**拿它当服务端 base ✓
//      **我第一版就是没做这个限定，16 条误报**✗ ⇒ 见下面的变异 ④ ✓）
// **⇒ 变异**（**五条都真跑过 ✓，红在哪一条按实测写 ✓**，改完 `cmp` 逐字节还原 ✓）：
//   ① 端口行改回 `process.argv[3] || "9222"`（**CI 上真实存在过的那版**✗）⇒ **两条一起红** ✓
//   ② 端口**写死** `const port = "9222"`（来源不随编排 ✓）⇒ **红在 ①**、②不红 ✓（**① 单独能红 ✓**）
//   ③ 保留 env 兜底、把 `argv[3]` 裸用（`process.env.CDP_PORT || process.argv[3]`）⇒ **红在 ②** ✓
//      （**∴ 所以** ② 管的是「**编排换槽位**」✓ 而**不是**「连不上」✓ 两件事分开 ✓）
//   ④ **只删** `process.env.CDP_PORT` ⇒ **不红** ✓（**按设计**：`argv[5]` 带 `/^\d+$/` 校验是**同样合法**的来源 ✓
//      ⇒ **∴ 我原本写「② 删 env ⇒ 红在 ①」是错的**✗ **实测 EXIT=0** ⇒ 已按实测改正 ✓）
//   ⑤ 加一行 `const base = process.argv[3];`（**非端口变量** ✓）⇒ **不红** ✓
//      （**这就是「不误报」的对照**：现仓里 **13 个**文件**正是**这样拿 `argv[3]` 当服务端 base ✓）
// **⇒ 造这条守卫时我自己错了两次**（**「测量工具要先被测量」**✓ 同一族）：
//   · 第一版把**文件里任何** `argv[3]` 都当端口 ⇒ **16 条误报** ✗ ⇒ 限定成**端口变量名**才成立 ✓（见 ⑤）
//   · 第二版用**正则套正则**判 `/^\d+$/` 是否存在 ⇒ `\d` 被当字符类 ✗ ⇒ **12 条误报** ✗
//     ⇒ 改成**字符串包含** ✓（**教训**：判「源码里有没有某个字面写法」用 `includes`，不要转义套娃 ✗）
import { readdirSync, readFileSync } from "node:fs";

// **分母**：连 CDP 的判据 ⇒ 三条真实形态都算（取 `/json/*`、走 `/devtools/page` 长连接、自起浏览器）
const CDP = /127\.0\.0\.1:\$\{[^}]+\}\/json|\/devtools\/page|remote-debugging-port/;
const PORT_VAR = /^(const|let)\s+(port|cdpPort|debugPort|rawPort|portArg)\b/;
// **源码里那句「只认纯数字」的写法**（用**字符串包含**而不是正则套正则 ✓ ——
//   我第一版在这里踩了转义：`\d` 被当成字符类而非「反斜杠＋d」⇒ **12 条误报** ✗）
const hasDigitTest = (s) => s.includes(String.raw`/^\d+$/`) || s.includes("/^[0-9]+$/");

const failures = [];
const check = (ok, label, detail) => {
  if (!ok) {
    console.log(`  ✗ ${label}${detail ? `（${detail}）` : ""}`);
    failures.push(label);
  }
};

let files;
try {
  files = readdirSync("scripts").filter((f) => /^browser-.*\.mjs$/.test(f)).sort();
} catch {
  console.error("  ⊘ 判据前提不成立：读不到 scripts/（不是产品问题）");
  process.exit(3);
}
if (files.length === 0) {
  console.error("  ⊘ 判据前提不成立：scripts/ 下没有 browser-*.mjs（不是产品问题）");
  process.exit(3);
}

const noPortSource = [];
const bareArgv3 = [];
let usesCdp = 0;
for (const f of files) {
  const text = readFileSync(`scripts/${f}`, "utf8");
  if (!CDP.test(text)) continue;
  usesCdp += 1;
  // ① 端口来源合法（env ／ 自起浏览器 ／ 校验过的 argv[5]）
  const legal =
    /process\.env\.CDP_PORT/.test(text) ||
    /--remote-debugging-port/.test(text) ||
    (text.includes("process.argv[5]") && hasDigitTest(text));
  if (!legal) noPortSource.push(f);
  // ② 端口变量的赋值不许裸用 argv[3]
  const lines = text.split("\n");
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i].trim();
    if (!PORT_VAR.test(line) || !/process\.argv\[3\]/.test(line)) continue;
    if (!hasDigitTest(lines.slice(i, i + 3).join("\n"))) bareArgv3.push(`${f}：${line.slice(0, 72)}`);
  }
}
// 「分母为空 ⇒ 判据在验空气」⇒ 必须能自己说出来（四层齐之「输入能触发」✓）
if (usesCdp === 0) {
  console.error("  ⊘ 判据前提不成立：没有一条 browser-* 连 CDP（分母为空）");
  process.exit(3);
}

check(noPortSource.length === 0, "连 CDP 的判据都有合法端口来源（env CDP_PORT／自起浏览器／校验过的 argv[5]）", noPortSource.join("、"));
check(bareArgv3.length === 0, "端口变量的赋值没有裸用 argv[3]（那是服务端 base 的槽位，不是端口）", bareArgv3.join("｜"));

if (failures.length === 0) {
  console.log(`✓ browser-cdp-port-source：${files.length} 条 browser-* 里 ${usesCdp} 条连 CDP，端口来源都合法`);
} else {
  console.log(`✗ browser-cdp-port-source：${failures.length} 条不过`);
  process.exit(1);
}
