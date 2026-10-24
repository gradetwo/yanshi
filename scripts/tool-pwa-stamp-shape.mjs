#!/usr/bin/env node
// **★ 构建戳的「形状」判据 ✗ ★**（第 744 轮 ✓；**实测换来的 ✓）
//
// **∴ 它守的 bug ✗ ★**（**真实存在 ✗ 且已提交在主干上 ✓**）：
//   **∴ 同一个事实**（**「`yanshi-shell-` 后面那串允许哪些字符」**）
//     **原来在** `pwa-sync-viewer.mjs` **里写了四遍**✗ 而**字符类各不相同**✗
//       ⇒ **归一化哈希**能吃掉的戳 ✗ **打戳那步吃不掉** ✓（**内核 ② ✓**）
//   **∴ 实测后果 ✗**：未提交的工作区导出的构建号是 `e8b0ac47-dirty-dad838e8`✓
//     ⇒ 旧的 `[0-9a-f-]+` **只吃到** `e8b0ac47-d`（**`i` 不在类里 ✓）**
//       ⇒ 换出来的名字是 **`yanshi-shell-<新戳>irty-dad838e8`** ✓ ★**** ✓✓
//   **∴ 于是**：页面 `caches.open(名字A)` ✗ 而 SW 预缓存的是**名字B** ✗
//     ⇒ **∴ 离线外壳取不到** ✓（**症状像「缓存没建」**✗ 实际是**名字被截断后拼脏了** ✓）
//
// **∴ 为什么 CI 一直没抓到 ✗ ★**（**内核 1 ✓**）：**CI 的工作区是干净的**
//   ⇒ **构建号只有十六进制短 sha**✗ 不含 `-dirty-` ✓ ⇒ **旧窄类在 CI 上恰好够用** ✓
//   ⇒ **∴ 只有**「**从未提交的工作区部署**」那个人会踩 ✓
//     ⇒ **∴ 所以本判据自己造那个输入**（**四层齐的第 ② 层 ✓**）
//       **不是**依赖「跑它时工作区刚好是脏的」✓ ★**** ✓✓
//
// **∴ 三层断言 ✗**：
//   **① 单元 ✓**：直接调**产品代码** `scripts/pwa-stamp.mjs` 的 `setStamp`（**∴ 不是抄一份 ✓）**
//   **② 结构 ✓**：**形状只许有一处定义**✗ ⇒ 别处**不许**再手写 `yanshi-shell-`／`yanshi-online-` 的替换
//   **③ 端到端 ✓**：**在临时镜像里**跑真正的同步 ✗ ⇒ **产物里所有戳必须是同一个 13 位数字**
//     **∴ 镜像**故意不链接** `target/`**✓**（**第 745 轮 ✓**）：**没有**服务端二进制 ⇒ **跳过**静态页导出**
//       ⇒ **∴ 于是**注入进 `index.html` 的脏号**不会被**重新导出**盖掉**✗ **本机有二进制时也一样 ✓**
//     **∴ 产物三份** = `index.html`／`service-worker.js`／`viewer-app.js`✓（**`web/sw.js` 已被上游删除 ✓ S-10）**
//
// **∴ 变异（**已逐条验证 ✓**）**：把 `STAMP_TOKEN` 退回 `[0-9a-f-]+` ⇒ ①③ 红 ✓
//   ＋ 退回**非全局** ⇒ ① 的「两处」红 ✓ ＋ 端到端「字母式脏号」红 ✓ ＋ 类里去掉 `_` ⇒ 占位符那条红 ✓
//   ＋ 在 `pwa-sync-viewer.mjs` 里加回一行手写替换 ⇒ ② 红 ✓（**细节见台账第 744 轮 ✓**）
//
// **∴ 用法 ✗**：`node scripts/tool-pwa-stamp-shape.mjs`
import { cpSync, existsSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { setStamp, STAMP_TOKEN, STAMP_PREFIXES } from "./pwa-stamp.mjs";

const SYNC = "scripts/pwa-sync-viewer.mjs";
// **∴ 产物清单 = 同步脚本**最后那组**打 `yanshi-shell-` 戳的文件**✗（**第 745 轮核对 ✓）**
//   **⇒ 而** `web/sw.js`**已被上游删除**✓（**外部审计 S-10 ✗ 它**从来没被注册**✓）**
//     ⇒ **∴ 所以**静态托管那份**只剩** `web/service-worker.js`✓（**判据本身**不许**依赖一份**不存在**的产物 ✓）**
const PRODUCTS = ["index.html", "service-worker.js", "viewer-app.js"];
const failures = [];
let okCount = 0;
let ranCount = 0;
const check = (ok, label, detail) => {
  console.log("  " + (ok ? "✓" : "✗") + " " + label + (detail ? "（" + detail + "）" : ""));
  ranCount += 1;
  if (ok) okCount += 1; else failures.push(label + (detail ? "：" + detail : ""));
};
// **∴ 读戳的正则**必须比**写戳**的更宽 ✗ ★**（**否则「截断后拼脏的尾巴」**✗
//   **∴ 会被读成**「**两个独立 token**」✗ ⇒ **∴ 于是**判据看不见脏尾巴 ✓ ⇒ **恒绿 ✗**）
//   ⇒ **∴ 所以这里带上 `.`**（**产品类里没有它 ✓**）＋ **整串锚定**再比形状 ✓
//   **∴ 用 `+` 不用 `*` ✗**：**注入片段里有散文**`yanshi-online-<mtime 戳>`✓
//     ⇒ **`*` 会把「后面什么都没跟」也读成一个戳**✗ ⇒ **假红** ✓（**实测第 744 轮 ✓**）
const READ = /yanshi-(?:shell|online)-([A-Za-z0-9._-]+)/g;
const stampsIn = (text) => [...text.matchAll(READ)].map((m) => m[1]);
// **∴ 端到端只读 `yanshi-shell-`**✗（**第 745 轮 ✓**）：**上游删掉 `web/sw.js` 之后**✗
//   **产物里**已经**没有**真正的 `yanshi-online-` 戳**了**✗（**只剩注释里的散文 ✓**）
//   ⇒ **∴ 拿它去断「产物同戳」**✗ 是在断**一个不存在的东西**✓ ⇒ **∴ 所以**端到端**只断**外壳名 ✓
const SHELL_ONLY = /yanshi-shell-([A-Za-z0-9._-]+)/g;
const shellStampsIn = (text) => [...text.matchAll(SHELL_ONLY)].map((m) => m[1]);
const NUMERIC = /^\d{13}$/;

// ─────────────────────────────────────────────────────────────────────────────
// **① 单元 ✗**：**产品代码**对**各种真实形状**的戳怎么处理 ✓
// ─────────────────────────────────────────────────────────────────────────────
console.log("① 单元（setStamp 对真实构建号形状）：");

const DIRTY = "yanshi-shell-29a7b41e-dirty-dad838e8";
// **∴ 输入就是**导出页里那一行 ✓**（**实测的 git 式脏号 ✓）
const line = `caches.open("${DIRTY}")`;
const stamped = setStamp(line, "yanshi-shell", "3311211289735");
// **★ 断的是**整行 ✗**：换完之后**一个字符都不许多**✓**
//   （**旧版留下的尾巴 `irty-dad838e8` 就在这里现形 ✓**）
check(stamped === `caches.open("yanshi-shell-3311211289735")`,
  "git 式脏号必须被**整串**换掉（**不许留 `irty-…` 尾巴 ✓**）", stamped);

// **∴ 形状 ✓**：换出来的名字**本身**必须是数字戳 ✓（**页面与 SW 比较的就是这个串 ✓）**
check(stampsIn(stamped).length === 1 && NUMERIC.test(stampsIn(stamped)[0]),
  "换完只剩一个戳，且是 13 位数字", stampsIn(stamped).join("|"));

// **∴ 幂等 ✓**：**再换一次必须不动**（**同步会被反复跑 ✗ 不幂等就会每次换名 ✓**）
check(setStamp(stamped, "yanshi-shell", "3311211289735") === stamped,
  "对已打戳的文本再打一次 ⇒ 结果不变（幂等 ✓）");

// **∴ 自愈 ✓**：**旧版本截脏出来的名字**✗ 新版必须**吃下整串** ✓
const corrupted = `caches.open("yanshi-shell-4036615775438irty-f3ba45b2")`;
const healed = setStamp(corrupted, "yanshi-shell", "3311211289735");
check(healed === `caches.open("yanshi-shell-3311211289735")`,
  "被旧版截脏过的名字 ⇒ 再同步一次必须**自愈**（**否则主干上的脏值永远留在产物里 ✓**）", healed);

// **∴ 全局 ✓**：**同行两处**都要换 ✗（**旧版给 `sw.js` 用的是**非全局**正则 ✓）**
const two = `const A="yanshi-online-1111111111111",B="yanshi-online-abc9999d-dirty-00000000";`;
const both = setStamp(two, "yanshi-online", "3311211289735");
check(stampsIn(both).length === 2 && stampsIn(both).every((v) => NUMERIC.test(v)),
  "同一行两处 ⇒ 必须都换掉（**非全局只换第一处 ✗**）", stampsIn(both).join("|"));

// **∴ 占位符 ✓**：源头资产里是 `yanshi-shell-__BUILD_ID__`✗ 它也必须被换掉
//   （**`_` 与大写字母不在旧类里时 ✗ 这一条就是断点 ✓**）
const ph = setStamp(`const C="yanshi-shell-__BUILD_ID__";`, "yanshi-shell", "3311211289735");
check(stampsIn(ph).every((v) => NUMERIC.test(v)) && !ph.includes("__BUILD_ID__"),
  "占位符 `__BUILD_ID__` 也必须被换掉（**源头那一份靠它 ✓**）", ph);

// **∴ 字母式旧戳 ✓**：**旧版给 sw.js 的类是** `\d+`**✗ ⇒ 遇到字母开头的旧戳**整个不换** ✓
const letters = setStamp(`const CACHE = "yanshi-online-abc1234d-dirty-ef56789a";`, "yanshi-online", "3311211289735");
check(letters.includes("yanshi-online-3311211289735") && !letters.includes("abc1234d"),
  "字母式旧戳也必须被换掉（**旧版 `\\d+` 在这儿**静默不更新** ✓）**", letters);

// **∴ 形状自述 ✓**：**判据自己**也要能被验 —— 产品用的类**必须**是本文件读到的那个类
check(STAMP_TOKEN === "[A-Za-z0-9_-]+" && STAMP_PREFIXES.length === 2,
  "`pwa-stamp.mjs` 仍导出同一个形状（**改了形状 ⇒ 本判据要重新核对 ✓**）", STAMP_TOKEN);

// ─────────────────────────────────────────────────────────────────────────────
// **② 结构 ✗**：**形状只许一处定义**（**内核 ② ✓「同一事实只写一处」**）
// ─────────────────────────────────────────────────────────────────────────────
console.log("");
console.log("② 结构（替换只经由 setStamp）：");
const stripComments = (t) => t.split("\n").filter((l) => !/^\s*(\/\/|\/\/!|\*|\/\*)/.test(l)).join("\n");
const scriptsDir = "scripts";
if (!existsSync(scriptsDir)) {
  console.error("✗ 找不到 scripts/ ⇒ 结构层无法判定（不是产品失败）⇒ 退出码 3");
  process.exit(3);
}
// **∴ 分母 ✗**：**部署／同步链路**上的脚本（**判据本身允许自己读戳 ✗ 但它不改产品 ✓）**
const DEPLOY_SCRIPTS = [
  "pwa-sync-viewer.mjs", "pwa-sync-wasm.mjs", "serve-web.mjs",
  "mediums-sync.mjs", "deploy-pwa.sh",
].filter((f) => existsSync(join(scriptsDir, f)));
check(DEPLOY_SCRIPTS.length > 0, "部署链路的脚本清单非空", DEPLOY_SCRIPTS.join(","));
const offenders = [];
for (const f of DEPLOY_SCRIPTS) {
  const body = stripComments(readFileSync(join(scriptsDir, f), "utf8"));
  // **∴ 断的是 ✗**：**不许**出现「用正则替换 `yanshi-shell-`／`yanshi-online-`」的手写副本 ✓
  //   （**`setStamp(` 是唯一允许的形式 ✓**）
  // **★★ 这一条**差点**是安慰剂 ✗ ★**（**第 744 轮变异抓到的 ✓**）：
  //   **∴ 我原来写的是** `/\breplaceAll?\(/`✗ 而 **`?` 只管最后一个 `l`**✗
  //     ⇒ **∴ 它**要求的是 `replaceAl(`**✗ ⇒ **`.replace(` 一个都不匹配** ✓
  //       ⇒ **∴ 于是**变异「加回一行手写替换」**照样绿**（**退出码 0 ✓）**
  //         ⇒ **∴ 现在 ✗**：写成 **`(?:replace|replaceAll)`**（**两兄弟都列全 ✓）**
  //           ⇒ **∴ 教训 ✗**：**结构层判据也必须被变异验一次**✗
  //             **「它没红」≠「它守住」**（**内核 1 ✓**）
  const bad = body.split("\n").filter((l) =>
    /yanshi-(shell|online)/.test(l) &&
    (/\.(?:replace|replaceAll)\(/.test(l) || /new RegExp\(/.test(l)) &&
    !/setStamp\(/.test(l));
  if (bad.length) offenders.push(`${f}：${bad[0].trim().slice(0, 90)}`);
}
check(offenders.length === 0,
  "部署链路里不许再有第二份「戳的替换」写法（**四份不同字符类 ⇒ 就是这次的根因 ✓**）",
  offenders.join(" ｜ ") || `已查 ${DEPLOY_SCRIPTS.length} 个脚本`);

// ─────────────────────────────────────────────────────────────────────────────
// **③ 端到端 ✗**：**在临时镜像里**跑真同步 ✗ 产物必须「**三处同戳、且是纯数字**」✓
// ─────────────────────────────────────────────────────────────────────────────
console.log("");
console.log("③ 端到端（临时镜像跑 pwa-sync-viewer）：");
const T = mkdtempSync(join(tmpdir(), "pwa-stamp-shape-"));
let verdict = 0;
try {
  // **∴ 前提 ✗**：`web/` 与两个源目录必须在 ✗ 缺任何一个 ⇒ **退出码 3**（**不冒充产品失败 ✓**）
  for (const need of ["web", "crates", "assets"]) {
    if (!existsSync(need)) {
      console.error(`✗ 缺少 ${need}/ ⇒ 端到端前提不成立（不是产品失败）⇒ 退出码 3`);
      process.exit(3);
    }
  }
  // **∴ 只链接**输入**✗** ⇒ **`web/` 必须**拷进临时目录**✗
  //   **∴ 否则**同步会**写回仓库里那份**（**本判据不许弄脏主干 ✓**）
  // **∴ 而**这里**故意不链接 `target/`**✗（**第 745 轮改成这样 ✓）**：
  //   **⇒ 没有**服务端二进制 ⇒ **同步**跳过静态页导出**✓（**`--allow-missing-server` ✓）**
  //     ⇒ **∴ 于是** `web/index.html`**保持我注入的那一份**✗ **不被**重新导出**盖掉**✓
  //       ⇒ **∴ 输入**必然**被验**✗（**本机有二进制时也一样 ✓**）
  symlinkSync(resolve("crates"), join(T, "crates"));
  symlinkSync(resolve("assets"), join(T, "assets"));
  cpSync(resolve("web"), join(T, "web"), { recursive: true });
  // **★ 自己造那个「CI 造不出来」的输入 ✗ ★**（**四层齐的第 ② 层 ✓**）：
  //   **∴ 把 `index.html` 的戳改成**带字母**的 git 式脏号 ✗
  //     ⇒ **∴ 旧版**（`[0-9a-f-]+` ✓ 遇 `i` 就断 ✗）**只吃到**第一个非十六进制字符**✗
  //       ⇒ **∴ 于是**产物里留着**脏尾巴**`irty-…`✓ ⇒ **页面与 SW 的缓存名分岔**✓
  //   **∴ 这不需要**工作区刚好是脏的 ✗ ⇒ **∴ CI 上也必然触发 ✓**
  const indexPath = join(T, "web", "index.html");
  if (!existsSync(indexPath)) {
    console.error("✗ 临时镜像里没有 web/index.html ⇒ 前提不成立 ⇒ 退出码 3");
    process.exit(3);
  }
  const before = readFileSync(indexPath, "utf8");
  const SHELL_BEFORE = /yanshi-shell-[A-Za-z0-9._-]+/;
  if (!SHELL_BEFORE.test(before)) {
    console.error("✗ `web/index.html` 里一个 `yanshi-shell-` 戳都没有 ⇒ 无从注入 ⇒ 退出码 3");
    process.exit(3);
  }
  const sentinel = "abc1234d-dirty-ef56789a";
  writeFileSync(indexPath, before.replace(SHELL_BEFORE, `yanshi-shell-${sentinel}`)
    // **∴ 再放第二处 ✗**：**旧版那句是**非全局**的**⇒ 第二处**不会被换** ✓
    + `\n<!-- 第二处哨兵：yanshi-shell-${sentinel}-2 -->\n`);
  // **∴ 断言「输入真的造出来了」**✗（**内核 1 ✗ 不假定自己写进去了就算数 ✓**）
  check(readFileSync(indexPath, "utf8").split(sentinel).length - 1 === 2,
    "端到端的输入里必须有**两处**字母式脏号（**少了 ⇒ 这一层什么都没验 ✓**）",
    `哨兵出现 ${(readFileSync(indexPath, "utf8").split(sentinel).length - 1)} 次`);
  // **∴ `--allow-missing-server` ✓**：**没有二进制时**同步会跳过静态页导出 ✗
  //   ⇒ **∴ 于是**本判据**不依赖**「本机刚好有 `yanshi-serve`」✓（**有就用 ✓）
  const run = spawnSync("node", [resolve(SYNC), "--allow-missing-server"],
    { cwd: T, encoding: "utf8", timeout: 240_000 });
  if (run.error || run.status !== 0) {
    const why = run.error ? String(run.error)
      : `退出码 ${run.status}：${((run.stdout || "") + (run.stderr || "")).split("\n").filter((x) => x.trim()).slice(-3).join(" / ").slice(0, 260)}`;
    console.error(`✗ 同步脚本没跑成 ⇒ 前提不成立（不是产品失败）⇒ 退出码 3（${why}）`);
    process.exit(3);
  }
  const per = {};
  const missing = [];
  for (const p of PRODUCTS) {
    const f = join(T, "web", p);
    if (!existsSync(f)) { missing.push(p); continue; }
    per[p] = shellStampsIn(readFileSync(f, "utf8"));
  }
  check(missing.length === 0, "三份产物都在", missing.length ? `缺 ${missing.join(",")}` : PRODUCTS.join(","));
  const values = [...new Set(Object.values(per).flat())];
  // **∴ 集合非空 ✗**：**空集合上** `every()`**恒真**（**内核 1 ✓**）
  //   ⇒ **∴ 所以**先断「**确实读到了戳**」✗ 否则下面两条**什么都没验** ✓
  check(values.length > 0, "产物里**读到了**戳（**空集合 ⇒ 下面的断言是安慰剂 ✓**）",
    Object.entries(per).map(([k, v]) => `${k}:${v.length}`).join(" ｜ "));
  // **∴ 断的是**关系**不是**单点 ✗（**内核 ② ✓**）：
  //   **∴ 「页面用的名字」与「SW 预缓存的名字」**必须**是同一个**✗
  //     ⇒ **∴ 于是**「各文件各自合法却互相矛盾」那种状态**封不住**就不算守住 ✓
  const nonNumeric = values.filter((v) => !NUMERIC.test(v));
  check(nonNumeric.length === 0,
    "产物里每个戳都是 13 位数字（**脏尾巴 ⇒ 有字母 ⇒ 这条红 ✓**）",
    values.join("|").slice(0, 160));
  check(values.length === 1,
    "**三份产物的戳必须是同一个值**（**名字一分岔 ⇒ 离线外壳取不到 ✓）**",
    Object.entries(per).map(([k, v]) => `${k}=${[...new Set(v)].join(",") || "无"}`).join(" ｜ ").slice(0, 200));
  check(!readFileSync(indexPath, "utf8").includes(sentinel),
    "**我造的字母式脏号（两处）必须被同步全换掉**（**旧版窄类 ⇒ 留下 `irty-…` 尾巴 ✓**）",
    `哨兵 ${sentinel}`);
} finally {
  rmSync(T, { recursive: true, force: true });
}

console.log("");
// **∴ 恒等式 ✓**（**内核 ② ✓**）：**通过数 ＋ 失败数 = 跑过的条数**✗
//   ⇒ **∴ 于是**「**漏计数**」（**把失败悄悄丢掉 ✓**）**当场红**✗
//   **∴ 注意 ✗**：**跑过的条数由 `check` 自己累加**（**`ranCount` ✓）**✗
//     ⇒ **∴ 所以**它**不是**「用 `ok+fail` 定义 total 再断言相等」那种**恒真安慰剂** ✓
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
  console.log(`  结论：✓ 构建戳形状只有一处定义，且产物多处同戳、纯数字（通过 ${okCount} 条）`);
}
process.exit(verdict);
