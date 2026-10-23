#!/usr/bin/env node
// **★ PWA 的字体必须是**镜像**✗ 不是**追加**✗ 也不是**旧文件堆积** ✓ ★**（第 745 轮 ✓；**实测换来的 ✓**）
//
// **∴ 它守的三件事 ✗**（**同一处的**同步代码 ✗ **两条**是当场**复现出来的**✓）：
//   **① `@font-face` 引用的文件必须在包里**✓（**`url("/fonts/Inter.ttf")` ✓**）
//     ⇒ **∴ 否则**离线时**字体 404 ⇒ 静默回退**✓ ⇒ **症状是**「字体没生效」✗ 而**不是**「缺文件」✓
//       （**内核 5 ✗ 失败没有信号 ✓**）
//   **② `web/fonts` 必须与 `assets/fonts` 逐字节相同 ✗ 且**不许有**源里没有的文件**✓**
//     ⇒ **∴ 原来 ✗**：**只拷**不删**⇒ 上游删掉的字体**在 `web/` 里赖着不走**✓（**镜像变追加 ✓**）
//   **③ 同步**源里没有字体时**必须**跑完**✗ 并**说清是 0 个**✓**（**不许**炸在中途 ✗ 也不许**冒充**✓**）
//     **∴ 实测原状 ✗**（**两条都是临时镜像里跑出来的 ✓**）：
//       · 源空 ＋ `web/fonts` 有旧文件 ⇒ 打印 **`OK: 预置字体 2 个`**✓ ——**本次一个都没拷**✗
//         那 2 个是**上一次留下的**（**外部 §71 ✗「做了 0 次」冒充「做了 2 次」✓**）
//       · `web/fonts` 不存在 ⇒ **`ENOENT … scandir 'web/fonts'`**✓ **退出码 1**✓
//         ⇒ **∴ 而**它**已经跑过**大半（**拷贝 ＋ 索引 ✓**）⇒ **打戳那步没做**✓ ⇒ **产物半新半旧**✓
//   **∴ 为什么 CI 一直没抓到 ✗**（**内核 1 ✓**）：**没有任何判据**碰字体 ✓
//     （**`grep -ln 'web/fonts' scripts/*.mjs`** 只有**同步脚本自己**✓）
//
// **∴ 断的是**关系**✗ 不是**快照** ✓**（**内核 ② ✓**）：**不写**「字体应有 2 个」**✓
//   ⇒ **两边都从**仓库**算出来**✗ ⇒ **换机器**✗ **换时间**✗ 结果**同一个 ✓**
//
// **∴ 变异（**已逐条实测 ✓**）**：
//   ①**把 `web/fonts/Inter.ttf` 改名** ⇒ 「CSS 引用的都在包里」＋「源里的都在包里」
//     ＋「不许有源里没有的」**三条一起红**✓ 退出码 1
//   ②**往 `web/fonts` 里塞一个源里没有的文件** ⇒ 「镜像不是追加」红 ✓
//   ③**改一个字体的内容**（**截掉尾巴 ✓）** ⇒ 「逐字节相同」红 ✓（**名字对而内容旧 ✗ 等于没部署 ✓**）
//   ④**把同步脚本的字体段退回原来那五行**（**逐字退回 ✗ 实测 ✓**）⇒ **5 条红**✓ 退出码 1
//     · **A 两条**：**退出码 1**（**`ENOENT … scandir 'web/fonts'`✓**）＋ 目录根本没建 ✓
//     · **B 两条**：那句**原样**是 **`OK: 预置字体 2 个`**✓（**而本次源里 0 个 ✓**）＋ 残留两个文件 ✓
//     · **C 一条**：**没有**「本次同步 N 个」那一句 ✓（**旧文案不报本次 ✗ 所以正向对照也该红 ✓**）
//   ⑤**去掉同步脚本里「先 `rmSync` 清空目标」那一行**（**其余保持修好 ✓）**
//     ⇒ **A/C 全绿 ✗ 只有 B 的「残留清空」红**✓
//     ⇒ **∴ 所以**：**「跑成功」与「报真话」都不足以证明**它是**镜像**✗
//       **必须**有一条专门问「上一次的残留去哪了**」✓**
//
// **∴ 用法 ✗**：`node scripts/tool-pwa-fonts-mirror.mjs`
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, symlinkSync } from "node:fs";
import { createHash } from "node:crypto";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";

const SRC = "assets/fonts";
const DST = "web/fonts";
const CSS = "web/viewer.css";
const SYNC = "scripts/pwa-sync-viewer.mjs";
const FONT_EXT = [".ttf", ".woff2"];

const failures = [];
let ranCount = 0;
let okCount = 0;
const check = (ok, label, detail) => {
  console.log("  " + (ok ? "✓" : "✗") + " " + label + (detail ? "（" + detail + "）" : ""));
  ranCount += 1;
  if (ok) okCount += 1; else failures.push(label + (detail ? "：" + detail : ""));
};
const digest = (f) => createHash("sha256").update(readFileSync(f)).digest("hex").slice(0, 12);

// **∴ 前提 ✗**：**缺任何一个 ⇒ 判据自己坏了**✗ ⇒ **退出码 3**（**不冒充产品失败 ✓**）
for (const need of [SRC, DST, CSS, SYNC]) {
  if (!existsSync(need)) {
    console.error(`✗ 缺少 ${need} ⇒ 前提不成立（不是产品失败）⇒ 退出码 3`);
    process.exit(3);
  }
}
const srcFonts = readdirSync(SRC).filter((n) => FONT_EXT.some((e) => n.endsWith(e))).sort();
const dstFonts = readdirSync(DST).sort();

console.log("① CSS 引用的字体必须真的在包里（**离线时不会 404 ✓**）：");
const css = readFileSync(CSS, "utf8");
const refs = [...new Set([...css.matchAll(/url\("?\/fonts\/([^")']+)"?\)/g)].map((m) => m[1]))];
check(refs.length > 0, "CSS 里**读到了** `url(\"/fonts/…\")`（**空集合 ⇒ 下面几条是安慰剂 ✓**）",
  `${refs.length} 个：${refs.join(",") || "无"}`);
const missingFromPkg = refs.filter((n) => !existsSync(join(DST, n)));
check(missingFromPkg.length === 0, "每个被引用的字体都在 `web/fonts/` 里",
  missingFromPkg.length ? `缺 ${missingFromPkg.join(",")} ⇒ 离线必然 404 后静默回退 ✓` : `${refs.length} 个都在 ✓`);

console.log("");
console.log("② `web/fonts` 必须是 `assets/fonts` 的**镜像**（**双向 ✗ 单向等于没比 ✓**）：");
const forgotten = srcFonts.filter((n) => !dstFonts.includes(n));
const stale = dstFonts.filter((n) => !srcFonts.includes(n));
check(forgotten.length === 0, "源里的字体**都**在包里",
  forgotten.length ? `包里没有：${forgotten.join(",")}` : `源 ${srcFonts.length} 个 ✓`);
check(stale.length === 0, "包里**不许有**源里没有的文件（**镜像 ✗ 不是追加 ✓**）",
  stale.length ? `多出来：${stale.join(",")} ⇒ 上游删掉它时 ` + "`web/`" + ` 里会赖着不走 ✓` : "无残留 ✓");
const bytesDiffer = srcFonts.filter((n) => existsSync(join(DST, n)) && digest(join(SRC, n)) !== digest(join(DST, n)));
check(bytesDiffer.length === 0, "同名字体**逐字节相同**（**名字对而内容旧 ✗ 等于没部署 ✓**）",
  bytesDiffer.length ? `不同：${bytesDiffer.map((n) => `${n}(${digest(join(SRC, n))} vs ${digest(join(DST, n))})`).join(",")}` : `${srcFonts.length} 个 sha256 前 12 位相同 ✓`);

console.log("");
console.log("③ 行为：源里**没有**字体时 ⇒ 同步必须**跑完**＋**如实报 0**＋**不许留旧文件**（**内核 5 ✗ 三个形态一起压 ✓**）：");
// **∴ 自己造那两个「CI 造不出来」的输入 ✗**（**四层齐的第 ② 层 ✓**）：
//   **场景 A**：**空源 ＋ 目标目录不存在**✓
//     ⇒ **∴ 原来 ✗**：`mkdirSync` **写在循环里**⇒ **一个都没拷时目录根本没建**✓
//       ⇒ **∴ 于是**后面 `readdirSync(FONT_DST)` **抛 ENOENT**✓ ⇒ **整个同步崩在中途**✓（**退出码 1 ✓**）
//       ⇒ **∴ 而**它**已经跑过**拷贝与索引 ⇒ **只剩**打戳没做**✓ ⇒ **产物半新半旧**✓★**** ✓✓
//   **场景 B**：**空源 ＋ 目标目录里留着上一次的字体**✓（**这才是那句「冒充」的现场 ✓**）
//     ⇒ **∴ 原来 ✗**：**报的是**目的地的文件数**✓ ⇒ 打印 **`OK: 预置字体 2 个`**✓
//       而**本次一个都没拷**✗ 那 2 个是**上一次留下的**（**外部 §71 ✗「做了 0 次」冒充「做了 2 次」✓**）
//     ⇒ **∴ 而且**：旧代码**只拷不删**✓ ⇒ 源里删掉的字体**在 `web/` 里赖着不走**✓（**镜像变追加 ✓**）
//   **∴ 为什么两个场景都要 ✗**：**A 断「不崩」✗ B 断「不谎」✓** ——**它们是同一段代码的两种坏法**✗
//     **只测 A** 就放过了**「跑成功但报假数」**✓（**那比崩**更危险**，因为**它绿着 ✓**）
const T = mkdtempSync(join(tmpdir(), "pwa-fonts-"));
const buildMirror = (dir, { withFonts }) => {
  mkdirSync(join(dir, "assets", "fonts"), { recursive: true });
  for (const entry of readdirSync(resolve("assets"))) {
    if (entry === "fonts") continue;
    symlinkSync(resolve("assets", entry), join(dir, "assets", entry));
  }
  symlinkSync(resolve("crates"), join(dir, "crates"));
  cpSync(resolve("web"), join(dir, "web"), { recursive: true });
  rmSync(join(dir, "web", "fonts"), { recursive: true, force: true });
  if (withFonts) {
    mkdirSync(join(dir, "web", "fonts"), { recursive: true });
    for (const n of srcFonts) cpSync(join(resolve(SRC), n), join(dir, "web", "fonts", n));
  }
};
const runMirror = (dir) => {
  const run = spawnSync("node", [resolve(SYNC), "--allow-missing-server"],
    { cwd: dir, encoding: "utf8", timeout: 240_000 });
  return { code: run.status, err: run.error,
    out: (run.stdout || "") + (run.stderr || "") };
};
try {
  // **场景 A ✗**：空源 ＋ **目标目录不存在**（**压「崩在中途」那条 ✓**）
  buildMirror(T, { withFonts: false });
  const a = runMirror(T);
  const aTail = a.out.split("\n").filter((x) => x.trim()).slice(-2).join(" / ").slice(0, 180);
  check(!a.err && a.code === 0,
    "A：空源时同步必须**成功跑完**（**原来在这里 ENOENT 崩 ✗ 打戳那步根本没做到 ✓**）",
    a.err ? String(a.err).slice(0, 140) : `退出码 ${a.code}｜${aTail}`);
  const dirA = existsSync(join(T, "web", "fonts")) ? readdirSync(join(T, "web", "fonts")) : null;
  check(dirA !== null && dirA.length === 0,
    "A：目标目录**建出来了且为空**（**建目录不许靠「刚好拷到过一个文件**」✓**）",
    dirA === null ? "目录根本没建 ⇒ 就是那个 ENOENT 的根 ✓"
      : (dirA.length ? `里面还有 ${dirA.join(",")} ⇒ 那是上一次的残留 ✓` : "目录在、且空 ✓"));

  // **场景 B ✗**：空源 ＋ **目标目录里留着上一次的字体**（**压「报假数」与「不删旧」两条 ✓**）
  const TB = mkdtempSync(join(tmpdir(), "pwa-fonts-stale-"));
  try {
    buildMirror(TB, { withFonts: true });
    const before = readdirSync(join(TB, "web", "fonts")).length;
    const b = runMirror(TB);
    const lineB = b.out.split("\n").find((l) => l.includes("字体")) || "（输出里一句字体都没提 ✓）";
    check(!b.err && b.code === 0, "B：留着旧字体时同步也必须**跑完**",
      b.err ? String(b.err).slice(0, 140) : `退出码 ${b.code}`);
    check(/0 个/.test(lineB),
      `B：必须报**本次 0** 个（**原来报的是目的地数 ⇒ 源里 0 个时它说「${before} 个」✓**）`,
      lineB.trim().slice(0, 130));
    const after = existsSync(join(TB, "web", "fonts")) ? readdirSync(join(TB, "web", "fonts")) : [];
    check(after.length === 0,
      "B：源里删掉的字体**必须从包里消失**（**镜像 ✗ 不是只追加 ✓**）",
      after.length ? `残留：${after.join(",")} ⇒ 包里有、源里没有 ⇒ 部署出去就是**多带一份**✓` : "残留清空 ✓");
  } finally { rmSync(TB, { recursive: true, force: true }); }

  // **场景 C ✗（正向对照 ✓）**：**源正常** ⇒ 报的数**必须等于源里的名单**✗
  //   **∴ 为什么要有这条 ✗**：A/B 只压「空源」✗ **漏拷**（**源有 3 个而只拷到 2 个 ✓）**
  //     **要靠「报出的数 == 源名单长度**」**才看得见 ✓**
  const TC = mkdtempSync(join(tmpdir(), "pwa-fonts-ok-"));
  try {
    symlinkSync(resolve("assets"), join(TC, "assets"));
    symlinkSync(resolve("crates"), join(TC, "crates"));
    cpSync(resolve("web"), join(TC, "web"), { recursive: true });
    rmSync(join(TC, "web", "fonts"), { recursive: true, force: true });
    const c = runMirror(TC);
    const m = c.out.match(/本次同步预置字体 (\d+) 个/);
    const copied = existsSync(join(TC, "web", "fonts")) ? readdirSync(join(TC, "web", "fonts")).sort() : [];
    check(c.code === 0 && m && Number(m[1]) === srcFonts.length && copied.length === srcFonts.length,
      "C：有源时**报的数**与**拷到的数**都必须等于源名单长度（**读目的地就永远发现不了漏拷 ✓**）",
      `源 ${srcFonts.length}｜报 ${m ? m[1] : "无那一句"}｜拷到 ${copied.length}｜退出码 ${c.code}`);
  } finally { rmSync(TC, { recursive: true, force: true }); }
} finally {
  rmSync(T, { recursive: true, force: true });
}

console.log("");
if (okCount + failures.length !== ranCount) {
  console.error(`  ✗ 计数不自洽：通过 ${okCount} ＋ 失败 ${failures.length} ≠ 跑过 ${ranCount} ⇒ 判据自己坏了 ✗`);
  process.exit(3);
}
console.log(`  计数自洽：通过 ${okCount} ＋ 失败 ${failures.length} = 跑过 ${ranCount} 条 ✓`);
if (failures.length) {
  console.log(`  结论：✗ PWA 字体镜像不成立（${failures.length} 条）｜离线字体会 404 后静默回退 ✓`);
  for (const line of failures) console.error("   - " + line);
  process.exit(1);
}
console.log(`  结论：✓ 字体是镜像、CSS 引用的都在包里、空源时同步如实报 0（通过 ${okCount} 条）`);
process.exit(0);
