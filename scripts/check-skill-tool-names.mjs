#!/usr/bin/env node
// **skill 文档里提到的标识符必须真实存在**（第 902 轮）
//
// **⇒ 为什么** ✗：外部补丁往 `skills/` 里加文档 ✓ 而**文档里的工具名没人核对** ✗
//   ⇒ 一个不存在的工具会**教出一次失败的调用** ✓ 而 CI 全绿 ✓（因为文档不参与任何判据 ✓）
//   ⇒ 第一个实例就是 `skills/yanshi-vector-tracing/SKILL.md`（第 900 轮合入 ✓）
//
// **⇒ 判据（分母是**反引号里的 snake_case 标识符** ✓）**：
//   · 允许集 ＝ **工具名**（`tools_registry.rs` 的 `name: "…"` ✓）
//     ∪ **工具参数名**（`param!("…")` ✓）—— 文档会写 `` `color_to` `` 这类参数 ✓
//     ∪ **响应字段名**（`crates/*/src/*.rs` 里作对象键出现的 `"…":` ✓）
//     ∪ **错误码**（`=> "…"` 的 `as_str()` 臂 ＋ 内联 `"error_code": "…"` ✓）
//   · 其余 ⇒ **红**并点名「哪个文件、哪个标识符」✓
//
// **★★ 后两个集合是 §117 CI 复盘补的**（**∴ 旧允许集太窄**✗）
//
//   **∴ 症状 ✗**：`skill-paint-v5` 合入后本判据红在 3 个标识符
//     （`reads_canvas`／`reference_not_found`／`resource_exhausted`），**本机与 CI 同红**✓
//     ⇒ **∴ 所以不是环境差异**✓ ⇒ **逐条查源码**✓：
//       · `reads_canvas` ＝ **真响应字段**（`tools.rs:3363`、`3379` ✓）
//       · `reference_not_found`／`resource_exhausted` ＝ **真错误码**（`error.rs:69`、`74` ✓）
//     ⇒ **∴ 三个全是真的**✓ ⇒ **∴ 错的是尺子**✗：**文档除了工具名还要写**「**回执里有什么字段**」✗
//       与「**会收到哪个错误码**」✓ ⇒ **∴ 那两类名字**本来就在**判据的分母**里 ✓
//   **∴ 为什么不直接放行这三个词 ✗**：**写死的白名单**＝**快照** ⇒ **内核 ②**禁止 ✓
//     ⇒ **∴ 所以**每个集合都**从源码现算**✓（**同一事实只写一处 ✓**）
//   **∴ 放宽的代价**（要认）：**恰好吗另一个真实键**的**笔误**会漏过 ✓
//     （**∴ 例**：`layer_id` 写成 `layer_ids` ⇒ 若别处真有那个键就绿 ✓）
//     ⇒ **∴ 但**它**仍然指向**一个**源码里真在发的名字**✓ ⇒ **不会**教出**不存在的调用**✓
//   **∴ 变异**（**∴ 三条都真跑过 ✓**）：
//     ① 文档加 `` `not_a_real_tool` `` ⇒ 红（**守卫本身没被放宽成摆设 ✓**）
//     ② **摘掉**响应字段集 ⇒ 红在 `reads_canvas` ✓（**∴ 证明那 629 个键不是装饰 ✓**）
//     ③ **摘掉**错误码集 ⇒ 红在 `reference_not_found`／`resource_exhausted` ✓
// **⇒ 变异**：往任一 `skills/**.md` 加一个 `` `not_a_real_tool` `` ⇒ 必红 ✓。
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { execFileSync } from "node:child_process";

const REGISTRY = "crates/yanshi-server/src/tools_registry.rs";
const SKILLS = "skills";

const registry = readFileSync(REGISTRY, "utf8");
const tools = new Set([...registry.matchAll(/name: "([a-z][a-z0-9_]+)"/g)].map((m) => m[1]));
const params = new Set([...registry.matchAll(/param!\("([a-z][a-z0-9_]+)"/g)].map((m) => m[1]));

// **响应字段名与错误码**：从**全部 crate 源码**现算（§117 ⇒ 见文件头）
const SNAKE = String.raw`([a-z][a-z0-9]*(?:_[a-z0-9]+)+)`;
const srcFiles = execFileSync("find", ["crates", "-name", "*.rs", "-path", "*/src/*"])
  .toString()
  .trim()
  .split("\n");
const fields = new Set();
const codes = new Set();
for (const file of srcFiles) {
  const text = readFileSync(file, "utf8");
  // `"foo_bar":` ⇒ JSON 键（响应字段／请求字段）
  for (const [, k] of text.matchAll(new RegExp(`"${SNAKE}"\\s*:`, "g"))) fields.add(k);
  // `=> "foo_bar"` ⇒ `as_str()` 的错误码臂，以及内联 `"error_code": "foo_bar"`
  for (const [, c] of text.matchAll(new RegExp(`=>\\s*"${SNAKE}"`, "g"))) codes.add(c);
}

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
    if (tools.has(token) || params.has(token) || fields.has(token) || codes.has(token)) continue;
    if (!unknown.has(token)) unknown.set(token, new Set());
    unknown.get(token).add(file);
  }
}

console.log(
  `  ${files.length} 个 skill 文档｜反引号标识符 ${checked} 处｜允许集：工具 ${tools.size}` +
    `｜参数 ${params.size}｜响应字段 ${fields.size}｜错误码 ${codes.size}（源码 ${srcFiles.length} 个 .rs 现算）`
);
if (unknown.size > 0) {
  console.error(`  ✗ ${unknown.size} 个标识符既不是工具名、参数名、响应字段名、也不是错误码：`);
  for (const [token, where] of [...unknown].sort()) {
    console.error(`      ${token}  ← ${[...where].join("｜")}`);
  }
  process.exit(1);
}
console.log("✓ skill 文档里引用的标识符都能在源码里找到（工具名／参数名／响应字段名／错误码）");
