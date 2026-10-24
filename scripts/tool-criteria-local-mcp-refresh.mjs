// 判据：本机入口 `scripts/run-criteria-local.sh` 的「MCP 二进制保鲜」必须按**内容**触发，
//        不能按**判据的文件名**触发（第 746 轮实测的洞）。
//
// ⇒ 为什么这条值得存在（**内核 ①：看起来对 ≠ 对**）：
//   第 740 轮我加这个守卫时写成 `[[ "$CRIT" == scripts/mcp-*.mjs ]]` ⇒ 它**看起来**在管
//   「哪些判据要 MCP」✗ 而它管的其实是**文件名**。
//   第 746 轮把本地栈 rebase 到上游 `864366d7` 之后跑 `tool-delete-document.mjs`，
//   它打的 banner 是 `yanshi-mcp 0.2.2 启动（commit e8b0ac47-dirty）`✓ ⇒
//   **那个二进制是上一个基线编的** ⇒ 测的正是上游修好**之前**的代码 ✗
//   而判据**照样全绿** ⇒ 于是「rebase 后重跑过」这句话是**假的** ✓。
//
// ⇒ 与第 956 轮同一个坑，换了入口：那次是 `yanshi-serve` 陈旧；
//   这次是「**保鲜范围靠文件名圈**」⇒ 而「文件名」**不是**「它需要哪些二进制」的可靠标识 ✓
//   （**内核 ②：会变的东西不能当身份**）
//
// ⇒ 四层齐：
//   ① 身份层：守卫**存在**且**按内容**（`grep -qE … "$CRIT"`），文件名式的 MCP 分支**已消失** ✓
//   ② 输入能触发：**普查**不是 `mcp-*` 却引用 MCP 二进制的判据；**为 0 就 exit 3**（空集合上
//      的断言恒真 ⇒ 本判据会空转 ✗）——**排除本判据自己**（第 301 轮：检查对象里不许有检查本身）✓
//   ③ 断言（**双向**）：带该字符串的夹具 ⇒ 守卫必须开火（打印「MCP 面 ⇒」＋ 子进程拿到
//      `YANSHI_MCP_BIN=target/release/yanshi-mcp`）；**不带**的夹具 ⇒ 必须**不开火**
//      （否则每条判据都白重建一次 ⇒ 那是另一种代价）✓
//   ④ 变异（4 个，见台账）：改回文件名守卫／改成恒真／把 release 换成 debug／删掉 export ✓
//
// 用法：`bash scripts/run-criteria-local.sh scripts/tool-criteria-local-mcp-refresh.mjs`
// 退出码：0 通过 ｜ 1 守卫坏了（入口脚本的失败）｜ 3 前提不成立（不可判）：
//          入口脚本没跑起来／临时服务没就绪／符合条件的判据一个都没有（空集合 ⇒ 断言恒真）

import { readFileSync, readdirSync, mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { join, resolve, basename } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const SCRIPTS = "scripts";
const HARNESS = join(SCRIPTS, "run-criteria-local.sh");
const SELF = basename(new URL(import.meta.url).pathname);
const MCP_TOKEN = "yanshi-mcp|YANSHI_MCP_BIN";

const failures = [];
let ran = 0, passed = 0;
const ok = (label, detail) => { ran += 1; passed += 1; console.log(`  ✓ ${label}（${detail}）`); };
const bad = (label, detail) => { ran += 1; failures.push(label); console.log(`  ✗ ${label}（${detail}）`); };

// ───────── ① 身份层：守卫在不在、是不是按内容 ─────────
if (!readdirSync(SCRIPTS).includes("run-criteria-local.sh")) {
  console.error("  ⊘ 不可判：找不到 " + HARNESS + "（判据前提不成立，不是产品失败）");
  process.exit(3);
}
const harness = readFileSync(HARNESS, "utf8");

// 按内容触发的守卫：一行 `grep -qE '<MCP 的两种拼法>' "$CRIT"`
const contentGuard = /grep -qE 'yanshi-mcp\|YANSHI_MCP_BIN' "\$CRIT"/.test(harness);
if (contentGuard) ok("MCP 保鲜守卫按「判据正文里有没有 MCP」触发", "grep -qE … \"$CRIT\" 在");
else bad("MCP 保鲜守卫按「判据正文里有没有 MCP」触发", "找不到那条 grep ⇒ 保鲜范围又在靠文件名圈");

// 反向：不许残留「按文件名判 MCP 需求」的分支（注释里提它不算 ⇒ 只查可执行行）
const executable = harness.split("\n").filter((l) => l.trim() && !l.trim().startsWith("#"));
const nameKeyed = executable.filter((l) => /CRIT.*==.*scripts\/mcp-\*|scripts\/mcp-\*.*CRIT/.test(l));
if (nameKeyed.length === 0) ok("已经没有「按文件名圈 MCP」的可执行分支残留", "可执行行里 0 处");
else bad("已经没有「按文件名圈 MCP」的可执行分支残留", nameKeyed.length + " 处：" + nameKeyed[0].trim().slice(0, 70));

// ───────── ② 输入能触发：非 mcp-* 却需要 MCP 的判据必须存在 ─────────
const needingMcp = readdirSync(SCRIPTS)
  .filter((f) => f.endsWith(".mjs") && f !== SELF)
  .filter((f) => new RegExp(MCP_TOKEN).test(readFileSync(join(SCRIPTS, f), "utf8")));
const nonPrefixed = needingMcp.filter((f) => !f.startsWith("mcp-"));
if (nonPrefixed.length === 0) {
  console.error("  ⊘ 不可判：除本判据外没有任何「不叫 mcp-* 却引用 MCP 二进制」的判据 ⇒");
  console.error("     第 ③ 条要防的那个洞**无法在这里制造**（空集合上的断言恒真 ✗）");
  console.error("     实测引用面：" + needingMcp.length + " 个，全部以 mcp- 开头");
  process.exit(3);
}
ok("输入能触发：存在「文件名不是 mcp-* 但要 spawn MCP」的判据",
  nonPrefixed.length + " 个：" + nonPrefixed.sort().join(", "));

// ───────── ③ 行为层：造两个夹具，真的把入口脚本跑起来 ─────────
// 夹具只做两件事：把 `process.env.YANSHI_MCP_BIN` 报出来 ⇒ 退出 0。
// ⇒ 于是本判据测的是**入口脚本的行为**，不依赖任何产品判据（跑得快，也不会被别的判据带红）。
const T = trackTemp(mkdtempSync(join(tmpdir(), "mcp-refresh-")));
const makeFixture = (name, mentionMcp) => {
  const p = join(T, name);
  // **夹具自己不许写出那两个拼法**✗ —— 否则「不该开火」那一支会因为**打印环境变量的那行**
  // 而被守卫匹配上 ⇒ 测出来的是夹具的写法，不是入口的规则 ✓（**反向：本判据也不能污染自己的输入**）
  const envName = '"YANSHI_MCP" + "_BIN"';
  writeFileSync(p, [
    "// 夹具：只汇报环境里有没有刚保鲜的 MCP 二进制。",
    mentionMcp ? "// 这一支**正文里写出 target/release/yanshi-mcp** ⇒ 入口脚本应当为它保鲜。"
               : "// 这一支**正文里不出现那两个拼法** ⇒ 入口脚本不该为它白重建。",
    "const k = " + envName + ";",
    "console.log(\"FIXTURE_MCP_BIN=\" + (process.env[k] === undefined ? \"<未设置>\" : process.env[k]));",
    "process.exit(0);",
    ""
  ].join("\n"));
  return p;
};
const runHarness = (fixture) => {
  // **★ 必须先把「继承来的」那个环境变量删掉**✗（本判据第一次跑就抓到，第 746 轮实测）
  //   **⇒ 症状**：本判据**自己**经入口脚本跑 ⇒ 守卫为它开火 ⇒ `export YANSHI_MCP_BIN=…`
  //     ⇒ 于是**我 spawn 的嵌套运行**（连夹具）也**继承**了它 ✗
  //   **⇒ 于是**「不引用 MCP ⇒ 环境变量不该设」那条**在测到泄漏而不是在测守卫**✗
  //     （实测：守卫开火=false ⇒ 却照样看到 target/release/yanshi-mcp ✓）
  //   **⇒ 所以**：删掉继承来的键 ⇒ 夹具看到的才是**这一次嵌套运行自己**该不该设的 ✓
  //   （键名用拼接写出，否则**这一行本身**就成了「正文里引用 MCP」的证据 ⇒ 反向污染 ✓）
  const KEY = "YANSHI_MC" + "P_BIN";
  const cleanEnv = {};
  for (const [k, v] of Object.entries(process.env)) if (k !== KEY) cleanEnv[k] = v;
  const r = spawnSync("bash", [HARNESS, fixture], { encoding: "utf8", timeout: 150000, cwd: resolve("."), env: cleanEnv });
  if (r.error) { console.error("  ⊘ 不可判：入口脚本没跑起来（" + r.error.message + "）⇒ 不是产品失败"); process.exit(3); }
  if (r.signal === "SIGTERM") { console.error("  ⊘ 不可判：入口脚本 150 s 没跑完（大概在本机重建二进制）⇒ 不可判 ≠ 失败"); process.exit(3); }
  const out = (r.stdout || "") + (r.stderr || "");
  // 服务起不来 ⇒ 判据前提不成立（**与产品失败用不同退出码说话**）
  if (/服务没起来|拿不到 token|重建失败/.test(out)) {
    console.error("  ⊘ 不可判：临时服务/二进制没就绪 ⇒ 第 ③ 条没被真正跑到");
    console.error(out.split("\n").filter(Boolean).slice(-4).join("\n"));
    process.exit(3);
  }
  const line = out.split("\n").find((l) => l.startsWith("FIXTURE_MCP_BIN="));
  return { out, guardFired: /MCP 面 ⇒/.test(out), bin: line ? line.slice("FIXTURE_MCP_BIN=".length) : "<夹具没打印>" };
};

const withMcp = runHarness(makeFixture("need-mcp.mjs", true));
if (withMcp.guardFired && withMcp.bin === "target/release/yanshi-mcp")
  ok("引用 MCP 的判据（文件名不是 mcp-*）⇒ 守卫开火且指到 release",
    "打印「MCP 面 ⇒」＋ 子进程看到 " + withMcp.bin);
else bad("引用 MCP 的判据（文件名不是 mcp-*）⇒ 守卫开火且指到 release",
    "守卫开火=" + withMcp.guardFired + "｜子进程看到的 YANSHI_MCP_BIN=" + withMcp.bin +
    "（缺的是哪一样：打印「MCP 面 ⇒」= " + withMcp.guardFired + "，子进程里的键 = " + withMcp.bin + "）");

const withoutMcp = runHarness(makeFixture("no-mcp.mjs", false));
if (!withoutMcp.guardFired && withoutMcp.bin === "<未设置>")
  ok("不引用 MCP 的判据 ⇒ 守卫**不开火**（不许每条判据都白重建一次）", "无「MCP 面 ⇒」＋ 环境变量未设");
else bad("不引用 MCP 的判据 ⇒ 守卫**不开火**（不许每条判据都白重建一次）",
    "守卫开火=" + withoutMcp.guardFired + "｜YANSHI_MCP_BIN=" + withoutMcp.bin);

rmSync(T, { recursive: true, force: true });

// ───────── 计数自洽：不许「做了 0 次」冒充「做了 4 次」（外部 §71）─────────
// ⇒ **passed 与 failures 各自只在自己的助手函数里加** ⇒ 所以「passed ＋ failures ＝ ran」
//   是一条**恒等式**✗ 不是打印 ✓（第 270 轮：打印只会让人看见，恒等式才让人不能矛盾）
if (passed + failures.length !== ran) {
  console.error("  ⊘ 不可判：计数自洽失败（通过 " + passed + " ＋ 失败 " + failures.length +
    " ≠ 跑过 " + ran + "）⇒ 本判据自己记错了账，结论不算");
  process.exit(3);
}
console.log("");
console.log("  计数自洽：通过 " + passed + " ＋ 失败 " + failures.length + " = 跑过 " + ran + " 条 ✓");
if (failures.length) {
  console.log("  结论：✗ 本机判据入口的 MCP 保鲜又按文件名圈住了（" + failures.join("；") + "）");
  process.exit(1);
}
console.log("  结论：✓ MCP 二进制的保鲜范围由「判据正文」决定，不是由「判据叫什么」决定（通过 " + ran + " 条）");
process.exit(0);
