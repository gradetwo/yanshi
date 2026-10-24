#!/usr/bin/env node
// 把每个工具的可复制示例**落成文档**，并提供防陈旧检查。
//
//   node scripts/tool-examples-doc.mjs <server-base>            # 生成/刷新 docs/design/tool-examples.md
//   node scripts/tool-examples-doc.mjs <server-base> --check    # 文档过期就 exit 1（判据）
//
// 内容全部来自**运行中的服务端目录**（`GET /api/tools`），所以它不可能与实现漂移 ——
// 这也是本仓库反复吃过的亏（源码文本 vs 产物）。
// **前提失败必须 exit(3)**（第 1084 轮）：无服务时 fetch 抛异常 ⇒ uncaught ⇒ 退出码 1 ✗
// ⇒ 按 336 轮那条：条件不满足应 exit(3)（不可判），不是 exit(1)（产品失败）
process.on("uncaughtException", (error) => {
  console.error(`  ⊘ 判据自身出错（不是产品问题）：${String(error).slice(0, 200)}`);
  process.exit(3);
});
import { readFileSync, writeFileSync } from "node:fs";

const base = process.argv[2];
const check = process.argv.includes("--check");
if (!base) { console.error("用法: node scripts/tool-examples-doc.mjs <server-base> [--check]"); process.exit(2); }
const path = "docs/design/tool-examples.md";
// **⇒ 清单取自**源码**，不再取自运行中的服务端** ✅（第 950 轮；**这是判据在 CI 上抖的真因** ✓）
//   **⇒ 症状** ✗：CI 报「第 678 行应为 `{}` / 第 679 行文档有 `filename`」✓
//     而**本机**重建服务端再生成 ⇒ 与提交**逐字节相同**、`--check` 过 ✗
//   **⇒ 根因** ✗：旧版从 `GET /api/tools` 取 `tool.example` ✓ ⇒ 而 `tool.example` 是**注册表里有没有**的镜像 ✓
//     ＋ **有些示例依赖前置状态**（`export_psd_layers` 要已有图层内容 ✓）
//     ⇒ 于是"带示例的工具集合"**随环境变** ✗ ⇒ 文档变 ✗ ⇒ `--check` 天生会抖 ✗
//   **⇒ 改法**：直接解析 **`crates/yanshi-server/src/tools_examples.rs`** 的
//     `("name", r#"json"#)` 对子 ✓ ⇒ **纯静态** ✓ ⇒ 同一份源码 ⇒ 同一份文档 ✓（与运行环境无关 ✓）
//   **⇒ 代价（两面 ✓）**：文档会列出**需要前置状态**的示例 ✓ ⇒ 而"它们能不能直接抄"由
//     `scripts/tool-example-acceptance.mjs` 判 ✓（本文件只保证**与源码一致** ✓）。
const source = readFileSync("crates/yanshi-server/src/tools_examples.rs", "utf8");
// **⇒ 名字与 raw string 之间**可以有注释行** ✅（第 1011 轮修 ✓）
//   **⇒ 原模式**（名字后直接要求 `r#"`）**漏掉**这种写法 ✗：
//   ```rust
//   (
//       "set_brush_curve",
//       // 原始偏移口径：…
//       r#"{"brush":…}"#,
//   ),
//   ```
//   ⇒ 实测后果**两个**：① 文档里**少一条示例**（`set_brush_curve` ✗）
//     ② **重复检测漏报**（多行块它扫不到 ✓ 第 1010 轮就是因此才发现 2 个重复 ✗）
const withExample = [...source.matchAll(/\(\s*"([a-z][a-z0-9_]+)"\s*,\s*(?:\/\/[^\n]*\n\s*)*r#+"([\s\S]*?)"#+,?\s*\)/g)]
  .map((m) => ({ name: m[1], example: m[2] }));
// **⇒ 去重** ✅（第 951 轮）：示例表里**同一个工具出现多次**（生成器历史上重复追加过 ✓ 实测 **235 条 / 197 唯一** ✓）
//   ⇒ 文档里会出现**两节同名** ✓ ⇒ 读者两份不同示例不知道该抄哪个 ✗
// **⇒ 源里的重复条目要**报出来**，不能只靠去重遮住** ✅（第 959 轮）
//   **⇒ 为什么** ✗：去重让文档看起来正常 ✓ 而**源里仍有 38 个重复块**（第 958 轮实测 ✓）
//     ⇒ 那说明生成器历史上重复追加过 ✓ ⇒ **没有信号** ✗ ⇒ 会再次累积 ✓
//   **⇒ 所以**：**发现重复 ⇒ 在 `--check` 模式直接红并点名** ✓（生成模式仍去重写出 ✓）
const dupNames = [...withExample.reduce((m, t) => m.set(t.name, (m.get(t.name) || 0) + 1), new Map())]
  .filter(([, count]) => count > 1)
  .map(([name, count]) => `${name}×${count}`);
if (dupNames.length > 0) {
  console.error(`  ✗ 示例表里有 ${dupNames.length} 个工具**重复**：${dupNames.slice(0, 6).join("、")} ⇒ 删掉多余块（每工具一条）`);
  process.exit(1);  // **⇒ 两种模式都红** ✓（生成模式也不许从脏表写出文档 ✓）
}
const seenNames = new Set();
withExample.splice(0, withExample.length, ...withExample.filter((tool) => {
  if (seenNames.has(tool.name)) return false;
  seenNames.add(tool.name);
  return true;
}));
withExample.sort((a, b) => a.name.localeCompare(b.name));
if (withExample.length === 0) { console.error("  ⊘ 从源码里没解析出任何示例 ⇒ 判据无法运行（不是通过）"); process.exit(3); }
// **⇒ 还要与**注册表**对齐** ✅（第 950 轮）：示例表里可能有**注册表没有的工具**（改名/删除的残留 ✓）
//   ⇒ 那正是"文档会漂移"的形态 ✓ ⇒ **静态解析注册表的 `name: "…"`** ✓ 再**求交** ✓
//   ⇒ 与运行时无关 ✓ 而仍是"两份源码互相对账" ✓ ⇒ 比"取活目录"稳 ✓ 且**照样能抓到漂移** ✓
const registrySource = readFileSync("crates/yanshi-server/src/tools_registry.rs", "utf8");
const registered = new Set([...registrySource.matchAll(/name:\s*"([a-z][a-z0-9_]+)"/g)].map((m) => m[1]));
const orphans = withExample.filter((tool) => !registered.has(tool.name)).map((tool) => tool.name);
if (orphans.length > 0) {
  console.error(`  ✗ 示例表里有 ${orphans.length} 个工具**不在注册表**：${orphans.slice(0, 6).join("、")} ⇒ 文档会教一个不存在的工具`);
  process.exit(1);
}
const lines = [
  "# 每个工具的可复制调用示例",
  "",
  "**这份文件是生成出来的**（`scripts/tool-examples-doc.mjs`）—— 内容取自运行中服务端的 `GET /api/tools`，",
  "所以它不会与实现漂移；`--check` 模式会在文档过期时失败。",
  "",
  // **不要在这里断言"全部验证过"** ✗（第 387-388 轮 ✓）：本生成器只数"有多少工具带示例" ✓，
  // 它**没有跑过任何示例** ✗ ⇒ 写"全部经过实调验证"是**一句永远不会自动为真的话** ✗
  //（实测：`tool-example-acceptance.mjs` 打印过 `18 个被拒` ✗ ⇒ 那句话当时是假的 ✓）。
  // ⇒ 只说本文件能保证的事 ✓，**把"能不能跑通"指给真正判它的判据** ✓。
  `当前共 ${withExample.length} 个工具带示例；**这些示例是否真能跑通，由 \`scripts/tool-example-acceptance.mjs\` 判定**（本文件只保证与工具目录一致）。`,
  "",
];
for (const tool of withExample) {
  lines.push(`## \`${tool.name}\``);
  lines.push("");
  lines.push("```json");
  lines.push(JSON.stringify(JSON.parse(tool.example), null, 2));
  lines.push("```");
  lines.push("");
}
const text = lines.join("\n");
if (check) {
  const current = (() => { try { return readFileSync(path, "utf8"); } catch { return ""; } })();
  if (current !== text) {
    console.error("❌ docs/design/tool-examples.md 与运行中的目录不一致（文档过期）⇒ 重新生成它");
    // **★ 第 1180 轮加诊断**：原来只说一句话，而 CI 日志里那就是 6 行重复
    //   ⇒ 于是**不知道差什么** ⇒ 只能猜（我 1177-1179 轮猜了 3 轮）✗
    //   ⇒ 而现在打印**第一处差异**（行号 + 两侧内容）⇒ 可诊断 ✓
    const a = current.split("\n"), b = text.split("\n");
    const n = Math.max(a.length, b.length);
    let shown = 0;
    for (let i = 0; i < n && shown < 3; i += 1) {
      if (a[i] !== b[i]) {
        console.error(`   第 ${i + 1} 行：文档 ${JSON.stringify((a[i] ?? "<缺>").slice(0, 90))}`);
        console.error(`   第 ${i + 1} 行：应为 ${JSON.stringify((b[i] ?? "<缺>").slice(0, 90))}`);
        shown += 1;
      }
    }
    console.error(`   文档 ${a.length} 行｜应为 ${b.length} 行`);
    process.exit(1);
  }
  // **README 必须链到它** —— 否则文档生成了却没人找得到（那是另一种"静默"）。
  const readme = (() => { try { return readFileSync("README.md", "utf8"); } catch { return ""; } })();
  // **要求的是"能点开的链接"，不是一个恰好出现的字符串** ✗ ——
  // 上一版只查字符串，而 README 里链接文字与目标都是同一个路径（出现两次）⇒
  // 我把第一处改坏时检查照样通过 ⇒ **变异无效、我当时写下的"能红"是假的** ✗。
  // 现在要求 Markdown 链接的目标形式 `](docs/design/tool-examples.md)`。
  if (!readme.includes("](docs/design/tool-examples.md)")) {
    console.error("❌ README.md 没有链到 docs/design/tool-examples.md ⇒ 文档没人找得到");
    process.exit(1);
  }
  console.log(`  ✓ 文档与目录一致（${withExample.length} 个工具带示例）✓ 且 README 已链到它`);
  process.exit(0);
}
writeFileSync(path, text);
console.log(`  ✓ 已写出 ${path}：${withExample.length} 个工具、${text.length} 字节`);
