// **帮助里写的语法，解析器必须真的接受 ✗**（外部复测 QY-D06 的判据 ✓）
//
// **∴ 缺陷 ✓**：**∴ 帮助写**`--cache-bust [<token>]`✗（**∴ 空格形式 ✓**）
//   ⇒ **∴ 而**解析器**只接受**裸开关**✗ 或 `--cache-bust=token`** ✓
//   ⇒ **∴ 于是**《清游》**按帮助传空格形式**✗ ⇒ **∴ `exit 2` 未知参数 ✗ ⇒ **∴ 一次启动失败 ✓**
//
// **∴ 判据 ✓**：**∴ 从 `--help` 输出里**抽出 `--cache-bust` 的写法**✗**
//   ⇒ **∴ **它自己展示的每种形式 ✗ 都必须**不被当成未知参数**✗** ✓
//   ⇒ **∴ 而**空格形式**若被明确标成**不支持**✗（**∴ 文案里写 ✗ 会被当成未知参数 ✓**）⇒ **∴ 那就**不要求 ✓**
//
// **∴ 用法 ✓**：**∴ `node tool-help-syntax-matches-parser.mjs <yanshi-serve 路径>` ✓**
const bin = process.argv[2];
if (!bin) {
  console.log("  用法: node scripts/tool-help-syntax-matches-parser.mjs <path-to-yanshi-serve>");
  process.exit(3);
}
const { execFileSync, spawnSync } = await import("node:child_process");
let help = "";
try {
  help = execFileSync(bin, ["--help"], { encoding: "utf8" });
} catch (error) {
  console.log(`  ✗ 跑 ${bin} --help 失败（判据前提不成立）`);
  process.exit(3);
}

// **∴ 抽 cache-bust 那一行 ✗ ＋ 它后面缩进的续行 ✓**
const lines = help.split("\n");
const start = lines.findIndex((l) => l.includes("--cache-bust"));
if (start < 0) {
  console.log("  ✗ 帮助里没有 --cache-bust（判据前提不成立）");
  process.exit(3);
}
const block = [lines[start]];
for (let i = start + 1; i < lines.length && /^\s{6,}\S/.test(lines[i]); i += 1) block.push(lines[i]);
const text = block.join(" ");

// **∴ 帮助里声明的形式 ✓**：`--cache-bust[=<token>]` 表示两种 ✓
const declaresBare = /--cache-bust(\[|$|\s)/.test(text);
const declaresEquals = /--cache-bust\[=</.test(text) || /--cache-bust=</.test(text);
// **∴ 明确说不支持空格形式 ✓**（**∴ 那就**不要求**✗**）
const disclaimsSpace = /--cache-bust abc.*(未知|不支持|拒)/.test(text);

let bad = 0;
const check = (ok, why) => {
  console.log(`  ${ok ? "✓" : "✗"} ${why}`);
  if (!ok) bad += 1;
};

const run = (args) => {
  const r = spawnSync(bin, [...args, "--bind", "127.0.0.1:0", "--root", "/tmp/help-syntax-probe"], { encoding: "utf8", timeout: 5000 });
  const out = `${r.stdout ?? ""}${r.stderr ?? ""}`;
  return { rejectedAsUnknown: /未知|unknown parameter|Unknown/i.test(out), out: out.slice(0, 80) };
};

console.log(`  帮助原文：${text.replace(/\s+/g, " ").slice(0, 150)}`);
console.log(`  声明：裸开关=${declaresBare}｜等号形式=${declaresEquals}｜明确不支持空格=${disclaimsSpace}`);

if (declaresBare) {
  const r = run(["--cache-bust"]);
  check(!r.rejectedAsUnknown, `帮助展示的裸开关必须被接受（${r.out}）`);
}
if (declaresEquals) {
  const r = run(["--cache-bust=probe-token"]);
  check(!r.rejectedAsUnknown, `帮助展示的等号形式必须被接受（${r.out}）`);
}
if (!disclaimsSpace) {
  const r = run(["--cache-bust", "probe-token"]);
  check(!r.rejectedAsUnknown, `帮助若没明说"空格形式不支持"，空格形式就该被接受（${r.out}）`);
} else {
  console.log("  ✓ 帮助已明说空格形式不支持 ⇒ 不要求解析器接受它");
}

if (bad) {
  console.log(`  ✗ 帮助语法与解析器有 ${bad} 处不一致`);
  process.exit(1);
}
console.log("  ✓ 帮助里展示的每种 --cache-bust 写法，解析器都真的接受");
