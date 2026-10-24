// **⇒ 示例表的解析**只有一个来源** ✅（第 1013 轮；内核②「同一事实只写一处」✓）
//
// **⇒ 为什么抽出来** ✗：解析规则在**两个地方**各写过一份 ⇒ **两份漂移**过三次 ✓
//   ① `scripts/tool-examples-doc.mjs`（生成文档 ＋ 重复检测）
//   ② 我**手写在命令行里**的临时统计（第 1008／1009 轮）
//   ⇒ 而两份用的模式**不一样**（严格要求名字后紧跟 `r#"` vs 宽松 ✓）
//   ⇒ 结果：**同一份源码数出 197 与 203 两个数** ✗ ⇒ 我追了两轮才明白 ✗
//
// **⇒ 规则（唯一的）**：条目形如 `( "name", <任意行注释> r#"…"# ),` ✓
//   **⇒ 名字与载荷之间可以有注释行** ✓ —— 这是第 1011 轮修掉的盲区 ✓
//   **⇒ 载荷可以是 `r#"…"#` 或 `r##"…"##`** ✓（含 `"` 的 JSON 用双井号 ✓）
const ENTRY = /\(\s*"([a-z][a-z0-9_]+)"\s*,\s*(?:\/\/[^\n]*\n\s*)*r#+"([\s\S]*?)"#+,?\s*\)/g;

/**
 * 解析 `crates/yanshi-server/src/tools_examples.rs` 的示例表。
 * @param {string} source 文件内容
 * @returns {{name: string, payload: string}[]} 按出现顺序
 */
export function parseExampleEntries(source) {
  return [...source.matchAll(ENTRY)].map((m) => ({ name: m[1], payload: m[2] }));
}

/**
 * 每个工具出现几次（用于重复检测 ✓）。
 * @param {{name: string}[]} entries
 * @returns {Map<string, number>}
 */
export function countByName(entries) {
  const counts = new Map();
  for (const { name } of entries) counts.set(name, (counts.get(name) ?? 0) + 1);
  return counts;
}
