#!/usr/bin/env node
// **判据覆盖率守卫**（第 788 轮）：`scripts/*.mjs` 里的每个文件，要么**被 `run-criteria.sh` 枚举到** ✓，
// 要么出现在下面**两个显式清单**之一 ✓；否则本条**红** ✓。
//
// **为什么需要它** ✗：`run-criteria.sh` 是**按前缀**枚举的（`tool-*` / `browser-*` / 两条 parity ✓）⇒
// 任何不匹配前缀的判据会**静默地永不运行** ✗ —— 而「CI 全绿」看起来毫无异常 ✓。
// 第 787 轮实测：**74 个里 14 个从未运行过** ✗，其中含 `(A)⑥` 的核心判据 `wasm-brush-parity` ✗
//（它正是"同一条笔触在两种渲染模式下逐字节相同"那条 ✓）。
// ⇒ 本条把这件事变成**能被红出来的** ✓：**新增一个判据却忘了接线 ⇒ 立刻红** ✓。
//
// 判据（能红 ✓）：
//   ① 从 `run-criteria.sh` 的枚举行里解析出模式 ✓ ⇒ 用它判断每个文件名是否会被跑到 ✓；
//   ② 不在枚举里、也不在两个清单里 ⇒ **红并点名** ✓；
//   ③ **清单腐化也要红** ✓（清单里写了已不存在的文件 ⇒ 红 ✓）—— 否则清单会慢慢变成谎话 ✗。
//
// 用法：node scripts/tool-criteria-coverage.mjs [base-url]
//      （base-url 不使用 ✓，只为与 `tool-*.mjs` 的调用约定一致 ✓）

import { readFileSync, readdirSync } from "node:fs";

const RUNNER = "scripts/run-criteria.sh";

const runner = readFileSync(RUNNER, "utf8");
const enumLine = runner.split("\n").find((line) => line.includes("for script in $(ls"));
if (!enumLine) {
  console.error("❌ 覆盖率守卫无法运行：在 " + RUNNER + " 里找不到枚举行（它改过？）");
  process.exit(1);
}
const globs = [...enumLine.matchAll(/scripts\/([A-Za-z0-9*._-]+\.mjs)/g)].map((m) => m[1]);
if (globs.length < 2) {
  console.error("❌ 覆盖率守卫无法运行：只解析出 " + globs.length + " 个模式（疑解析失败）");
  process.exit(1);
}
const toRegExp = (glob) => new RegExp("^" + glob.replace(/[.+]/g, "\\$&").replace(/\*/g, ".*") + "$");
const patterns = globs.map(toRegExp);
const enumerated = (name) => patterns.some((re) => re.test(name));

// **生成器** ✓：它们改源码/产物，本就不该当判据 ✓。
const NOT_CRITERIA = [
  // 「pwa-check-wrangler.mjs」（第 1000+ 轮 ✓）：它是**部署前的环境探测** ✗，不是产品判据 ✓ ——
  //   它只回答「本机装没装 wrangler、没装该装哪一个」✓（见文件头注释 ✓），
  //   不产生任何跨平台／确定性结论 ✗ ⇒ 归本清单 ✓（之前它「尚未接线」⇒ 永不运行 ✓）。
  "pwa-check-wrangler.mjs",
  // ****（第 1000+ 轮 ✓）：**它是生成器 ＋ 校验库** ✗，不是判据 ✓ ——
  //   **∴ 它有 write/check 两种模式** ✓，真正的**判据入口**是  ✓
  //   （**∴ 那个名字符合 tool-* glob ⇒ 会被枚举 ✓**）⇒ **∴ 它属于本清单 ✓**。
  "brush-params-doc.mjs",
  // **∴ `verify-live-showcase.mjs` ✓**（第 1000+ 轮 ✓）：**它**在**部署之后**核对线上 ✓
  //   ＋ **做什么 ✗**：对 `yanshi.wangda.today` 的**四个展示 URL** 发请求 ⇒ **断言 200
  //     ＋ 字节与 `assets/showcase/` 里的一致** ✓ ⇒ **∴ 判不了（连不上）退 3** ✓，**字节不对退 1** ✓。
  //   ＋ **为什么不该进分片 ✗**：**它要的是**已经部署好的站点**✗ —— 分片跑在**构建期** ✓，
  //     **∴ 那时线上还是旧产物** ✗ ⇒ **∴ 放进分片只会得到假红或假绿** ✗
  //       ⇒ **∴ 它由 `.github/workflows/pwa.yml` 的 `verify-live` job 调用** ✓（**部署之后** ✓）。
  //   ＋ **∴ 它**有红有绿**✓**（0／1／3 三档 ✓）⇒ **∴ 只是**接线位置**不同**✗（**部署后**而非构建期 ✓）。
  "verify-live-showcase.mjs",
  // **∴ `tool-psd-cost-accounts.mjs` ✓**（**第 253 轮 ✓）：**它**只做**测量**✗
  //   ＋ **∴ 做什么 ✗**：**按目标里"**同机两本账（**墙钟 ＋ CPU ✗ 多次取中位 ＋ 极差 ✓）**"
  //     量 **PSD 图层级导入／导出** ✓ ⇒ **∴ 它要 `--pid` 才能读服务进程的 CPU 账** ✓
  //   ＋ **∴ 为什么不该当判据 ✗**：**它**不产红／绿**✗（**∴ 没有门槛 ✓ 只有数字 ✓）
  //     ＋ **∴ 而**"导入对不对"由 `tool-psd-layers.mjs` 判 ✗
  //       **"导出对不对"由 `tool-psd-export.mjs` 判** ✓ ⇒ **∴ 红绿那两条出 ✓** ★★
  "tool-psd-cost-accounts.mjs",
  "generate-brush-previews.mjs",
  "generate-tool-examples.mjs",
  "make-samples.mjs",
  // **`pwa-stamp.mjs` ✓**（第 881 轮；外部补丁包带进来的 ✓）：**它是**生成器**✗，不是判据 ✓
  //   ＋ **∴ 它做什么 ✗**：往构建产物里**写构建戳**（build id／commit／时间 ✓）
  //     而**判它**的是 `tool-pwa-stamp-shape.mjs`（断言戳的形状 ✓）
  //   ＋ **∴ 为什么没有 `tool-`／`check-` 前缀 ✗**：它是**被调用的工具**✓（`pwa-sync-*` 之后跑 ✓），
  //     **不自己产出红／绿** ✗ ⇒ 枚举它只会让 CI 多跑一条"永远 exit 0"的假判据 ✗
  //   ＋ **∴ 代价**：补丁包合并后**漏登记了**它 ⇒ `tool-criteria-coverage` 立刻红并点名 ✓
  //     （**⇒ 这正是那条判据存在的意义 ✓** —— 而不是我自己想起来 ✓）。
  "pwa-stamp.mjs",
  // **∴ `pwa-sync-wasm.mjs` ✓**（第 610 轮 ✓）：**它**改产物**✗（**把 `crates/yanshi-wasm/pkg/`
  // 拷到 `web/wasm/` ✓**）⇒ **∴ 本就不该当判据 ✓**；**∴ 而"产物是否完整"由
  // `tool-pwa-assets.mjs` 判 ✓**（**它断言 wasm 存在且非空 ✓**）** ✓✓。
  "pwa-sync-wasm.mjs",
  // **∴ `pwa-sync-viewer.mjs` ✓**（第 632 轮 ✓）：**它**改产物**✗（**把现有 viewer 同步到 `web/` ✓**）
  // ⇒ **∴ 本就不该当判据 ✓**；**而"两侧是否一致"由 `tool-pwa-assets.mjs` 判 ✓**（**逐字节比对 ✓**）**。
  "pwa-sync-viewer.mjs",
  // **∴ `mediums-sync.mjs` ✓**（第 666 轮 ✓；**用户报告 ✓**）：**它**改产物**✗**
  //（**把 6 个介质插件的编译产物从 `target/…/release` 拷进 `assets/mediums/` ✓**）
  // ⇒ **∴ 本就不该当判据 ✓**；**而"产物是否齐"由**下面那条新判据守 ✗****。
  "mediums-sync.mjs",
  // **∴ `cdp-seed-client-render.mjs` ✓**（第 502 轮 ✓）：**它**是浏览器判据的**前置工具**✗**
  //   ⇒ **∴ 做什么 ✗**：**通过 CDP 给每个新文档**注入一小段脚本**✗
  //     ⇒ **∴ 只在**键不存在时**设 `yanshi.serverRender="0"`** ✓
  //       ⇒ **∴ 于是**：**无偏好 ⇒ 客户端渲染 ⇒ 内核加载** ✓
  //         （**∴ 否则**：**产品的默认值是服务端渲染 ⇒ **内核根本不加载**✗
  //           ⇒ **∴ 所有**需要内核的判据**必然红** ✓ —— 第 358 轮实测 ✓）★**** ✓✓
  //   **∴ 为什么不该当判据 ✗**：**它**不产出**红／绿**✗（**∴ 它**只**预置状态** ✓）
  //     ＋ **∴ 而**它**已经被接线**✗**：**`scripts/run-criteria.sh:69` 会调它** ✓
  //       ⇒ **∴ 所以**：**它属于本清单** ✓（**∴ 不是**漏接的判据 ✓）★**** ✓✓
  "cdp-seed-client-render.mjs",
  // **∴ `pwa-wrangler.mjs` ✓**（第 806 轮 ✓）：**它**执行部署 ✗**（**调 `wrangler` 上传 ✓）
  //   ⇒ **∴ 它**改的是**外部状态 ✗**（Cloudflare 上的站点 ✓），**不是本仓库的源码／产物 ✗**
  //   ⇒ **∴ 本就不该当判据 ✓**；**而"**能不能部署**"由 `pwa-check-wrangler.mjs` 那条守 ✗****。
  "pwa-wrangler.mjs",
    // **∴ `serve-web.mjs` ✓**（第 54 轮 ✓；**用户重启那次换来的 ✓）**：
    //   **∴ 它**是**一个**本地静态服务**✗（**给浏览器判据喂 `web/` ✓）
    //     ⇒ **∴ 它**自己**不判任何事 ✓ ⇒ **∴ 本就不该当判据 ✓**
    //   **∴ 为什么进仓 ✗**：**原来这份服务**写在 `/tmp/serve_web.mjs`**✗
    //     ⇒ **∴ 而 `/tmp` 是 tmpfs**✗ ⇒ **∴ 一重启**就没了 ✓
    //       ⇒ **∴ 判据**加载不到页面 ⇒ **∴ 报出**看不懂的失败 ✓**** ✓✓
    //   **∴ 它守的两点在实现里 ✗**：**`.wasm` 的 MIME 是 `application/wasm`**
    //     ＋ **只发布 `web/` 下的文件** ✓
    "serve-web.mjs",
  // **∴ `safe-edit.mjs` ✓**（第 819 轮；**用户指令「**经常犯的错误要造工具**」✓）：
  //   **∴ 它**是**编辑工具**✗（**字面替换 ＋ 四道闸 ＋ 变异检验 ✓）⇒ **∴ 本就不该当判据** ✓
  //     ＋ **∴ 而**它**自己**不判产品任何性质**✗ ⇒ **∴ 于是**永不接线**是对的** ✓ ★
  //       ＋ **∴ 用户指令的落实 ✗**：**它**消灭了本会话 **≥8 次**的「**模板串里编辑**」类错误 ✓ ★★
  "safe-edit.mjs",
  // **∴ `report-accounts.mjs` ✓**（第 804 轮；**同上 ✓）：
  //   **∴ 它**是**报告器**✗（**墙钟第一列 ＋ 自动标出假收益 ✓）⇒ **∴ 本就不该当判据** ✓
  //     ＋ **∴ 而**它**读的是别人的日志**✗ ⇒ **∴ 它**判不了产品** ✓ ★
  //       ＋ **∴ 它**消灭了 **2 次**「**只看分段不看墙钟**」的错判（**第 4／5 项 ✓）✓ ★★
  "report-accounts.mjs",
  // **∴ `ci-failures.mjs` ✓**（第 246 轮，**接手时造 ✓；**用户指令「反复做的事要造工具」✓）：
  //   **∴ 它**是**远端 CI 的失败分类器**✗（**拉 `gh` 日志 ⇒ 按 shard 列出意外失败 ✗
  //     ＋ 标出**台账已登记／GPU 门禁／更早 run 里是否也红** ✓）⇒ **∴ 它**不判产品**✗
  //     ⇒ **∴ 本就不该当判据** ✓ ★
  //       ＋ **∴ 而**"CI 到底绿没绿"由**运行器自己**（`run-criteria.sh` 的退出码 ✓）判 ✓
  //         ⇒ **∴ 本工具**只是**把人工那一遍**自动化** ✗
  //           ＋ **∴ 代价**：**它**依赖 `gh` 与运行器的日志文本形状**✗
  //             ⇒ **∴ 解析不到时会**退 3**✗ 而**不是**报"全绿" ✓ ★★
  "ci-failures.mjs",
];
// **尚未接线** ✗：已知缺口 ✓ —— **每一条都必须写清"为什么还没接"** ✓（不许留空话 ✗）。
const NOT_WIRED = [
  // （当前为空 ✓）
  // **∴ `pwa-check-wrangler.mjs` ✓**（第 806 轮 ✓；**第 224 轮加的 ✓**）：
  //   **∴ 它**是一条**真判据 ✗**（**断言"部署前 wrangler 可见"✗；**不可见就**点名三种装法 ＋ 退出 1 ✓）**
  //   ⇒ **∴ 所以**它**应当**接进 `run-criteria.sh` ✗**
  //   **∴ 还没接的具体原因（**不是空话 ✓）✗**：**它需要**一个**可能不存在的工具**✗（`wrangler` ✓）
  //     ⇒ **∴ 在**没装的 CI 上**它**必然失败 ✗** ⇒ **∴ 那时**"判据红"与"环境缺工具"**无法区分 ✓
  //     ⇒ **∴ 接线的正确做法 ✗**：**先在 CI 里装它 ✗**（**或**加一个**跳过标记 ✓），**再**枚举 ✓**
  //     ⇒ **∴ 那是**独立的一步 ✓**（**∴ 本轮**记在这里，**不许沉默跳过 ✓）** ✓✓
  "pwa-check-wrangler.mjs",
];
// ⚠️ **已退休两条** ✓，两条的理由是**同一件事**：被测的那份产物已经不存在 ✓。
//   ① `wasm-brush-preview-parity.mjs`（第 797 轮 ✓）：读裸 `.wasm` 后**空 imports** instantiate ✗，
//      而 `default_preview_points` 今天**只在服务端** ✓、共享内核暴露的是另一套预览接口 ✓
//      ⇒ 它要守的"本地预览与服务端一致"**没有实现可测** ✓（与 `browser-brush-preview-local`
//      是同一个产品缺口：产品没做"本地出图" ✓）。
//   ② `wasm-brush-parity.mjs`（第 799 轮 ✓）：同样读裸 `.wasm` + 空 imports ✗，调
//      `yanshi_brush_alloc/paint/out_ptr` ✓ —— 那些符号**只存在于脚本自己里** ✓，
//      `crates/yanshi-brush-wasm` 已不存在 ✓。
//   ⇒ ⇒ **但要注意：它守的**性质**并没有失去守卫** ✓ —— 继任者 `kernel-brush-parity.mjs`
//      自己的头部就写着"与 `wasm-brush-parity.mjs` 的差别只有一个：那边用裸 wasm + C-ABI（门面），
//      这边用 bindgen 包（浏览器真正加载的那一份）" ✓，且它是**已接线、CI 绿**的 ✓。
//   ⇒ ⚠️ **我在这件事上错了两次** ✗，两次都因为**没读"另一个判据"的自述**：
//      先是**乐观**（把 `wasm-brush-parity` 接上线就说"该性质有守卫了" ✗ —— 它根本没跑 ✓），
//      后是**悲观**（发现它测退休门面就说"该性质今天没有判据在守" ✗ —— 继任者一直在跑 ✓）。
//      ⇒ **换判据/换实现时，"谁接手了这条性质"必须去**读那边的头部**** ✓，而不是从我手上的文件推断 ✓。

const files = readdirSync("scripts").filter((name) => name.endsWith(".mjs")).sort();
const isEnumerated = files.filter(enumerated);
const isNotCriteria = NOT_CRITERIA.filter((name) => files.includes(name));
const isNotWired = NOT_WIRED.filter((name) => files.includes(name));

console.log("  scripts/*.mjs：共 " + files.length + " 个 ⇒ 被枚举 " + isEnumerated.length +
  "｜生成器 " + isNotCriteria.length + "｜尚未接线 " + isNotWired.length);

const unwired = files.filter(
  (name) => !enumerated(name) && !NOT_CRITERIA.includes(name) && !NOT_WIRED.includes(name)
);
if (unwired.length > 0) {
  console.error("❌ 有 " + unwired.length + " 个判据既没被 run-criteria.sh 枚举、也不在两个清单里");
  console.error("   ⇒ **它会静默地永不运行** ✗（而「CI 全绿」看不出任何异常）：");
  for (const name of unwired) console.error("     - " + name);
  console.error("   ⇒ 修法：接进 run-criteria.sh（可能要为它写一条 case 分支 ✓），");
  console.error("           或加进本文件的清单，并**写明为什么还没接** ✓。");
  process.exit(1);
}

const stale = [...NOT_CRITERIA, ...NOT_WIRED].filter((name) => !files.includes(name));
if (stale.length > 0) {
  console.error("❌ 清单腐化：这些名字在 scripts/ 里已不存在 ⇒ 清单在说已经不成立的话 ✗：");
  for (const name of stale) console.error("     - " + name);
  process.exit(1);
}

console.log("✓ 每个 scripts/*.mjs 都「要么被枚举、要么在清单里」⇒ 判据不会被静默跳过");
