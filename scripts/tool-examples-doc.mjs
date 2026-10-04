#!/usr/bin/env node
// 把每个工具的可复制示例**落成文档**，并提供防陈旧检查。
//
//   node scripts/tool-examples-doc.mjs <server-base>            # 生成/刷新 docs/design/tool-examples.md
//   node scripts/tool-examples-doc.mjs <server-base> --check    # 文档过期就 exit 1（判据）
//
// 内容全部来自**运行中的服务端目录**（`GET /api/tools`），所以它不可能与实现漂移 ——
// 这也是本仓库反复吃过的亏（源码文本 vs 产物）。
import { readFileSync, writeFileSync } from "node:fs";

const base = process.argv[2];
const check = process.argv.includes("--check");
if (!base) { console.error("用法: node scripts/tool-examples-doc.mjs <server-base> [--check]"); process.exit(2); }
const path = "docs/design/tool-examples.md";
const doc = "doc1";
const created = await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 900, height: 640 }),
}).then((r) => r.json());
if (!created.token) { console.error("建文档没拿到 token ⇒ 判据无法运行（不是通过）"); process.exit(1); }
const catalogue = await fetch(`${base}/api/tools?doc=${doc}&token=${created.token}`).then((r) => r.json());
const withExample = (catalogue.tools || []).filter((tool) => tool.example).sort((a, b) => a.name.localeCompare(b.name));
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
  lines.push(JSON.stringify(tool.example, null, 2));
  lines.push("```");
  lines.push("");
}
const text = lines.join("\n");
if (check) {
  const current = (() => { try { return readFileSync(path, "utf8"); } catch { return ""; } })();
  if (current !== text) {
    console.error("❌ docs/design/tool-examples.md 与运行中的目录不一致（文档过期）⇒ 重新生成它");
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
