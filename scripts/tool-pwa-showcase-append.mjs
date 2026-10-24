#!/usr/bin/env node
// **★ `web/showcase/` 是**混血目录**✗ 所以它的形状**不许**照字体段改成「镜像」✓ ★**（第 745 轮 ✓；**实测换来的 ✓**）
//
// **∴ 为什么这一段与字体段**相反 ✗ ★**（**内核 ② ✗「同一事实只写一处」的另一种用法：这里只写**关系**✓**）：
//   **∴ 字体段**：`assets/fonts` ⇒ `web/fonts` 是**纯导出产物**✓ ⇒ **必须镜像**（**残留就是 bug ✓**）
//   **∴ 展示图**：`web/showcase/` 里**还住着仓库自带的那一张**✓
//     （**`ballet-reference-study-4k.png`✗ 6.0 M ✓ 两版 README 用 ✗ 而 `assets/showcase/` 里没有它 ✓**）
//     ⇒ **∴ 于是**「把它改成先 `rmSync` 再拷」**会删掉一张 README 在用的图**✓
//       ⇒ **∴ 症状**不是报错 ✗ 而是**GitHub 上裂图**✓（**与 `.gitignore:68` 记的那次同一形状 ✓**）
//   ⇒ **∴ 所以**这段**只追加自己拷的**✗ 但**计数必须取自源**✓（**否则「包里 3 个」冒充「本次同步 3 张」✓**）
//
// **∴ 它守的三条 ✗**：
//   **① 目录里每个文件**都要有身份**✓：要么**在源里**（同步来的 ✓），要么**被某一处引用**（故意入库的 ✓）
//     ⇒ **两者都不是 ⇒ 那是没人引用的残留**✓（**这才是「不许堆积」的正确说法 ✗ 不是「一律清空」**✓）
//   **② 计数取自源**✓：空源时**必须报 0**（**原来报的是目的地数 ⇒ 说「3 张 ✓」而本次拷 0 张 ✓**）
//   **③ 空源／缺目录都不许崩**✓（**`WARN` 后继续 ✗ 打戳那步必须做到 ✓**）
//
// **∴ 变异（**逐条实测 ✓**）**：
//   ①**往 `web/showcase` 塞一张谁也不引用的图** ⇒ 第 ① 条红 ✓
//   ②**把同步块的计数退回 `readdirSync(DST).length`** ⇒ B 场景「报 3 张而本次 0 张」红 ✓
//   ③**给同步块加一行 `rmSync(DST)`（**看似"更干净"的镜像化 ✗ 本场我自己差点就这么做了 ✓**）**
//     ⇒ B/C 里那张 README 图消失 ⇒ 红 ✓
//   ④**把守卫改成恒假**（`if (false)`✓ **语法仍合法 ⇒ 红只可能来自运行时 ✓**）
//     ⇒ **C 场景退出码 1**＋**`Error: ENOENT: no such file or directory, scandir 'assets/showcase'`**✓
//     （**∴ 注意 ✗**：第一次我改成「删掉整行 `if`」⇒ 语法先炸 ✗ ⇒ **那是判据自己坏了**✓
//       **不是**产品失败 ⇒ **变异必须保持语法合法**才能把「红」归因到行为上 ✓）
//
// **∴ 用法 ✗**：`node scripts/tool-pwa-showcase-append.mjs`
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, symlinkSync, writeFileSync, realpathSync, lstatSync, unlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";

const SRC = "assets/showcase";
const DST = "web/showcase";
const SYNC = "scripts/pwa-sync-viewer.mjs";
const READERS = ["README.md", "README.zh-CN.md", "crates/yanshi-http/src/viewer.rs", "web/index.html"];

const failures = [];
let ranCount = 0;
let okCount = 0;
const check = (ok, label, detail) => {
  console.log("  " + (ok ? "✓" : "✗") + " " + label + (detail ? "（" + detail + "）" : ""));
  ranCount += 1;
  if (ok) okCount += 1; else failures.push(label + (detail ? "：" + detail : ""));
};

// **∴ 前提 ✗**：缺任何一个 ⇒ **退出码 3**（**判据前提不成立 ≠ 产品失败 ✓**）
for (const need of [SRC, DST, SYNC, "web", "crates", "assets"]) {
  if (!existsSync(need)) {
    console.error(`✗ 缺少 ${need} ⇒ 前提不成立（不是产品失败）⇒ 退出码 3`);
    process.exit(3);
  }
}
const srcNames = readdirSync(SRC).filter((f) => f.endsWith(".png")).sort();
const dstNames = readdirSync(DST).sort();
check(srcNames.length > 0, "源里**有**展示图（**空集合 ⇒ 下面几条是安慰剂 ✓**）", `${srcNames.length} 张：${srcNames.join(",")}`);

// ─────────────────────────────────────────────────────────────────────────────
// **① 身份 ✗**：**包里每个文件**要么来自源**，要么被某处引用**（**两者都不是 ⇒ 残留 ✓**）
// ─────────────────────────────────────────────────────────────────────────────
console.log("① `web/showcase` 里每个文件都要有身份（**同步来的 ✗ 或故意入库的 ✓**）：");
const readerText = READERS.filter(existsSync).map((f) => readFileSync(f, "utf8"));
check(READERS.filter(existsSync).length === READERS.length,
  "四份引用方都在（**少一份 ⇒ 「谁引用了」这个问题就答不全 ✓**）",
  READERS.map((f) => `${f}${existsSync(f) ? "✓" : "✗"}`).join(" ｜ "));
const referenced = (name) => readerText.some((t) => t.includes(`showcase/${name}`));
const orphans = dstNames.filter((n) => !srcNames.includes(n) && !referenced(n));
check(orphans.length === 0,
  "**不许有**「既不在源里、又没人引用」的文件（**那才是真残留 ✗ 而它的解法是查引用、不是一律清空 ✓**）",
  orphans.length ? `无身份：${orphans.join(",")}` :
    `包里 ${dstNames.length} 个 = 源 ${srcNames.length} 张 ＋ 自带 ${dstNames.length - srcNames.length} 张（${dstNames.filter((n) => !srcNames.includes(n)).join(",") || "无"} ✓ 都被 README 引用 ✓）`);
const lost = srcNames.filter((n) => !dstNames.includes(n));
check(lost.length === 0, "源里的展示图**都**得在包里（**同步漏了 ⇒ 静态站裂图 ✓**）",
  lost.length ? `缺 ${lost.join(",")}` : `${srcNames.length} 张都在 ✓`);

// ─────────────────────────────────────────────────────────────────────────────
// **②③ 行为 ✗**：**临时镜像里跑真同步**（**四层齐的第 ② 层：自己造那两个输入 ✓**）
//   **A** 正常源 ⇒ 报「本次同步 N 张」且 N = 源里的张数 ✓ ＋ 自带那张**还在**✓
//   **B** 源里**没有** png ⇒ 必须**跑完**✓ 报 **0**✓ 且**不许**把包里的图当成「本次同步」✓
//     ⇒ **∴ 这一条就是断「计数取自源而不是目的地」✓**（**原实测：空源时它说「3 张 ✓」**✓）
//   **C** `assets/showcase` **整个不存在** ⇒ 必须 `WARN` ＋ 跑完✓（**原来靠守卫 ✓ 去掉就 ENOENT ✓**）
// ─────────────────────────────────────────────────────────────────────────────
console.log("");
console.log("②③ 行为（临时镜像里跑真同步，三个场景）：");
const T = mkdtempSync(join(tmpdir(), "pwa-showcase-"));
let verdict = 0;

// **自带一个假介质 ✗ 不许继承真目录**✓（第 746 轮：干净 clone 上实测换来的 ✓）
//   **∴ 环境事实**：`assets/mediums/*.wasm` **是发布流水线建的**✓（干净签出里只有 `README.md`✓）
//     ⇒ **CI 的判据 job 会先跑 `node scripts/mediums-sync.mjs`**✓ **但一台没跑过的机器上不会**✓
//   **∴ 于是**：同步脚本在**镜像**里以 `ERROR: 一个介质插件都没同步` **exit 1**✓
//     ⇒ 那**不是产品失败 ✗ 是环境的缺前提**✓ ⇒ 却以**红**报出来 ✗（**违反本仓「前提不成立要用不同退出码」✓**）
//   **∴ 解法**：**判据自己造那个前提**（§四百六十五「输入自造 ✗ 不靠机器」同族 ✓）：
//     把 `assets/mediums` 换成**镜像里的真目录 ＋ 一个假 .wasm**✓（**本判据只管字体／展示图段 ✓**
//     ⇒ **假介质的内容没有任何断言依赖它 ✓**）⇒ **换机器结果不变** ✓✓
const seedMediums = (dir) => {
  const target = join(dir, "assets", "mediums");
  const trunk = realpathSync(resolve("assets", "mediums"));
  const st = lstatSync(target, { throwIfNoEntry: false });
  if (st && st.isSymbolicLink()) {
    // **叶子本身是链接**⇒ `unlinkSync` **只断开链接**✓（**实测**：`rmSync(链接, {recursive:true})`
    //   也只删链接 ✓ —— 两种写法在这里等价，用 `unlinkSync` 是因为**它按定义不跟随链接**✓）
    unlinkSync(target);
  } else {
    // **硬闸：不许经过符号链接落到仓库真的 `assets/mediums`**✗（**第 746 轮踩过 ✓**）
    //   **∴ 那次怎么坏的**：镜像里写的是 `symlinkSync(resolve("assets"), TC/assets)` ⇒
    //     链接在**父级** ⇒ `TC/assets/mediums` 解析到**真目录** ⇒ 那个 `rmSync`
    //     删的是**主干的文件**（`assets/mediums/README.md` ＋ 6 个 `.wasm` ✗ 事后 `git status` 才抓到 ✓）
    //   **∴ 而判据照样报「12 条全绿」**✗ ——**破坏发生在断言之外的路径上 ⇒ 断言看不见 ✓**
    //   **∴ 所以**：**动任何可能落到主干的路径之前，先把 realpath 算出来比一比**✓（**内核 ③ ✓**）
    const real = existsSync(target) ? realpathSync(target)
                                    : realpathSync(resolve(target, "..")) + "/mediums";
    if (real === trunk) {
      console.error("  ⊘ 不可判：镜像里的 `assets/mediums` 解析到了**仓库真目录**（" + target + " ⇒ " + real + "）✗");
      console.error("     那是**判据自己把镜像搭错了**（链接不许出现在这一条路径上）**不是产品失败** ✓");
      process.exit(3);
    }
    rmSync(target, { recursive: true, force: true });
  }
  mkdirSync(target, { recursive: true });
  writeFileSync(join(target, "criterion-fake.wasm"),
    Buffer.from([0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]));
};
const buildMirror = (mode) => {
  mkdirSync(join(T, "assets"), { recursive: true });
  for (const entry of readdirSync(resolve("assets"))) {
    if (entry === "showcase") continue;
    symlinkSync(resolve("assets", entry), join(T, "assets", entry));
  }
  if (mode !== "no-dir") {
    mkdirSync(join(T, "assets", "showcase"), { recursive: true });
    if (mode === "normal") {
      for (const n of srcNames) cpSync(join(resolve(SRC), n), join(T, "assets", "showcase", n));
    }
  }
  symlinkSync(resolve("crates"), join(T, "crates"));
  cpSync(resolve("web"), join(T, "web"), { recursive: true });
  seedMediums(T);
};
const runMirror = () => {
  const run = spawnSync("node", [resolve(SYNC), "--allow-missing-server"],
    { cwd: T, encoding: "utf8", timeout: 240_000 });
  return { code: run.status, err: run.error, out: (run.stdout || "") + (run.stderr || "") };
};
try {
  // **A ✗**：正常源
  buildMirror("normal");
  let a = runMirror();
  check(!a.err && a.code === 0, "A：正常源 ⇒ 同步必须跑完",
    a.err ? String(a.err) : `退出码 ${a.code}`);
  const aNum = (a.out.match(/本次同步作品展示图 (\d+) 张/) || [])[1];
  check(aNum !== undefined && Number(aNum) === srcNames.length,
    "A：报的数必须**等于源里的张数**（**不是目的地数 ✓**）",
    `实测 ${aNum ?? "没有那一句"}｜源 ${srcNames.length}`);
  const selfOwned = dstNames.filter((n) => !srcNames.includes(n));
  check(selfOwned.every((n) => existsSync(join(T, "web", "showcase", n))),
    "A：**仓库自带那张必须还在**（**⇒ 谁把这段改成「先清空」就会红 ✓**）",
    selfOwned.join(",") || "无自带文件");

  // **B ✗**：源里一个 png 都没有（**目的地留着上一次的 3 张 ✓**）
  rmSync(T, { recursive: true, force: true }); mkdirSync(T, { recursive: true });
  buildMirror("empty-src");
  let b = runMirror();
  const bTail = b.out.split("\n").filter((x) => x.trim()).slice(-2).join(" / ").slice(0, 200);
  check(!b.err && b.code === 0, "B：空源 ⇒ 同步**仍要跑完**（**打戳那步不许被跳过 ✓**）",
    b.err ? String(b.err) : `退出码 ${b.code}｜${bTail}`);
  const bNum = (b.out.match(/本次同步作品展示图 (\d+) 张/) || [])[1];
  check(bNum === "0",
    "B：空源时必须**如实报 0**（**原来报目的地数 ⇒ 空源也说「3 张 ✓」✗ 外部 §71 ✓**）",
    `实测「${bNum ?? "没有那一句"}」`);
  check(b.out.includes("WARN"), "B：必须**留下信号**（**不许静默 ✓ 内核 5 ✓**）",
    (b.out.match(/WARN[^\n]*/) || ["无"])[0].slice(0, 60));
  check(existsSync(join(T, "web", "showcase")) &&
    selfOwned.every((n) => existsSync(join(T, "web", "showcase", n))),
    "B：空源**不许**把包里的图一起清掉（**那张 README 在用 ✓**）",
    existsSync(join(T, "web", "showcase")) ? readdirSync(join(T, "web", "showcase")).join(",") : "目录没了 ✗");

  // **C ✗**：`assets/showcase` 整个不存在
  rmSync(T, { recursive: true, force: true }); mkdirSync(T, { recursive: true });
  buildMirror("no-dir");
  let c = runMirror();
  const cTail = c.out.split("\n").filter((x) => x.trim()).slice(-2).join(" / ").slice(0, 200);
  check(!c.err && c.code === 0, "C：源目录缺失 ⇒ 必须 `WARN` 后**跑完**（**原来靠 existsSync 守卫 ✓**）",
    c.err ? String(c.err) : `退出码 ${c.code}｜${cTail}`);
} finally {
  rmSync(T, { recursive: true, force: true });
}

console.log("");
// **∴ 恒等式 ✓**（**内核 ② ✓**）：**通过 ＋ 失败 = 跑过**✗ ⇒ 「漏计数」当场红 ✓
if (okCount + failures.length !== ranCount) {
  console.error(`  ✗ 计数不自洽：通过 ${okCount} ＋ 失败 ${failures.length} ≠ 跑过 ${ranCount} 条 ⇒ 判据自己坏了 ✗`);
  process.exit(3);
}
console.log(`  计数自洽：通过 ${okCount} ＋ 失败 ${failures.length} = 跑过 ${ranCount} 条 ✓`);
if (failures.length) {
  console.log(`  结论：✗ ${failures.length} 条失败｜通过 ${okCount} 条`);
  for (const f of failures) console.error("   - " + f);
  verdict = 1;
} else {
  console.log(`  结论：✓ 展示图目录是「追加 ＋ 身份核对」，计数取自源，空源与缺目录都跑得完（通过 ${okCount} 条）`);
}
process.exit(verdict);
