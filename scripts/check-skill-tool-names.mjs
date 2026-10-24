#!/usr/bin/env node
// **skill 文档里提到的标识符必须真实存在**（第 902 轮）
//
// **⇒ 为什么** ✗：外部补丁往 `skills/` 里加文档 ✓ 而**文档里的工具名没人核对** ✗
//   ⇒ 一个不存在的工具会**教出一次失败的调用** ✓ 而 CI 全绿 ✓（因为文档不参与任何判据 ✓）
//   ⇒ 第一个实例就是 `skills/yanshi-vector-tracing/SKILL.md`（第 900 轮合入 ✓）
//
// **⇒ 判据（分母是**反引号里的 snake_case 标识符** ✓）**：
//   · 允许集 ＝ **工具注册表里的工具名**（`tools_registry.rs` 的 `name: "…"` ✓）
//     ∪ **工具参数名**（`param!("…")` ✓）—— 文档会写 `` `color_to` `` 这类参数 ✓
//   · 其余 ⇒ **红**并点名「哪个文件、哪个标识符」✓
// **⇒ 变异**：往任一 `skills/**.md` 加一个 `` `not_a_real_tool` `` ⇒ 必红 ✓。
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

const REGISTRY = "crates/yanshi-server/src/tools_registry.rs";
const SKILLS = "skills";

const registry = readFileSync(REGISTRY, "utf8");
const tools = new Set([...registry.matchAll(/name: "([a-z][a-z0-9_]+)"/g)].map((m) => m[1]));
const params = new Set([...registry.matchAll(/param!\("([a-z][a-z0-9_]+)"/g)].map((m) => m[1]));

const walk = (dir) =>
  readdirSync(dir).flatMap((entry) => {
    const path = join(dir, entry);
    return statSync(path).isDirectory() ? walk(path) : path.endsWith(".md") ? [path] : [];
  });

const files = walk(SKILLS);
const unknown = new Map();
let checked = 0;
for (const file of files) {
  const text = readFileSync(file, "utf8");
  for (const [, token] of text.matchAll(/`([a-z][a-z0-9]*(?:_[a-z0-9]+)+)`/g)) {
    checked += 1;
    if (tools.has(token) || params.has(token)) continue;
    if (!unknown.has(token)) unknown.set(token, new Set());
    unknown.get(token).add(file);
  }
}

console.log(`  ${files.length} 个 skill 文档｜反引号标识符 ${checked} 处｜工具 ${tools.size} 个｜参数 ${params.size} 个`);
if (unknown.size > 0) {
  console.error(`  ✗ ${unknown.size} 个标识符既不是工具名、也不是参数名：`);
  for (const [token, where] of [...unknown].sort()) {
    console.error(`      ${token}  ← ${[...where].join("｜")}`);
  }
  process.exit(1);
}
console.log("✓ skill 文档里引用的标识符都能在工具注册表里找到（工具名或参数名）");
