#!/usr/bin/env node
// **★ 结构变更点普查 ✗ ★**（**第 744 轮 ✓）
//
// **∴ 它解决什么问题 ✗**（**本会话两次真实收获换来的 ✓）★**：
//   **∴ 第 741 轮 ✗**：**外部补丁给 `Tile` 加了 `maybe_opaque` 标志**✗
//     ＋ **∴ 而** `write_row_f32` **漏更新它**✗ ⇒ **∴ 静默**误报透明 ⇒ **∴ 丢内容** ✓ ★
//   **∴ 第 742 轮 ✗**：**同一套方法查 `DabCache`**✗ ⇒ **∴ 结论是**没有缺陷** ✓（**∴ 否定结果也有价值 ✓）★
//   ⇒ **∴ 所以**：**「**列出全部状态字段 ＋ 全部变更点 ＋ 找出**某条路径没碰某个字段**」**✗
//     ＋ **∴ 那**是本会话**最有效的**审查手法**✗ ⇒ **∴ 值得**固化** ✓ ★
//
// **∴ 它做什么 ✗**：**对指定 Rust 文件里的每个 `struct`**✗**：
//   **∴ ① 列出字段**✗ ／ **∴ ② 找出所有 `self.<字段>` 的**写入点**✗ ／ **∴ ③ 找出所有**变更该结构的函数** ✓
//     ＋ **∴ ④ 报告**可疑组合**✗**：**某个字段**只在**少数函数**里被写**✗
//       ＋ **∴ 而**那些函数**看起来**也改像素／改状态** ✓ ★
//
// **∴ 用法 ✗**：`node scripts/tool-struct-mutation-census.mjs <文件> [结构名]` ★
// **∴ 退出码 ✗**：**0 ⇒ 无可疑**｜**1 ⇒ 有可疑（**需要人看 ✓）｜**2 ⇒ 用法错** ✓ ★
import { readFileSync } from "node:fs";

const [, , file, onlyStruct] = process.argv;
if (!file) {
  console.error("用法：node scripts/tool-struct-mutation-census.mjs <rust文件> [结构名]");
  process.exit(2);
}
const lines = readFileSync(file, "utf8").split("\n");

// **∴ 找结构体：`struct X {` ⇒ 跳到匹配的 `}` ✓
const structs = [];
for (let i = 0; i < lines.length; i += 1) {
  const m = lines[i].match(/^\s*(?:pub(?:\([^)]*\))?\s+)?struct\s+([A-Za-z_][A-Za-z0-9_]*)\s*\{/);
  if (!m) continue;
  if (onlyStruct && m[1] !== onlyStruct) continue;
  let depth = 0;
  const fields = [];
  for (let j = i; j < lines.length; j += 1) {
    depth += (lines[j].match(/\{/g) || []).length;
    depth -= (lines[j].match(/\}/g) || []).length;
    if (j > i) {
      const f = lines[j].match(/^\s{4,}(?:pub(?:\([^)]*\))?\s+)?([a-z_][a-z0-9_]*)\s*:/);
      if (f) fields.push(f[1]);
    }
    if (depth <= 0 && j > i) {
      structs.push({ name: m[1], start: i, end: j, fields });
      break;
    }
  }
}

if (structs.length === 0) {
  console.error(`找不到结构体${onlyStruct ? " " + onlyStruct : ""}（文件：${file}）`);
  process.exit(2);
}

// **∴ 找所有函数边界 ✓（**`fn name(` ⇒ 下一个同／更低缩进的 `}` ✓）★
const fns = [];
for (let i = 0; i < lines.length; i += 1) {
  const m = lines[i].match(/^\s*(?:pub(?:\([^)]*\))?\s+)?(?:const\s+)?fn\s+([a-z_][a-z0-9_]*)\s*\(/);
  if (!m) continue;
  const indent = lines[i].match(/^\s*/)[0].length;
  let j = i + 1;
  for (; j < lines.length; j += 1) {
    if (/^\s*\}/.test(lines[j]) && lines[j].match(/^\s*/)[0].length === indent) break;
  }
  fns.push({ name: m[1], start: i, end: j });
}

let suspicious = 0;
for (const st of structs) {
  console.log(`\n== ${st.name}（${file}）｜字段 ${st.fields.length} 个 ==`);
  const writes = new Map(st.fields.map((f) => [f, []]));
  for (const fn of fns) {
    for (let i = fn.start; i <= fn.end && i < lines.length; i += 1) {
      for (const f of st.fields) {
        // **∴ 必须同时支持**索引赋值**（`self.pixels[i] = …`）**✗ ★**（**第 744 轮实测：**不加它会把
        //   `pixels` 误报成"从不写入" ✓）★
        const re = new RegExp(
          `self\\.${f}\\b(?:\\s*\\[[^\\]]*\\])?\\s*(?:=[^=]|\\+=|-=|\\*=|/=|\\.push|\\.insert|\\.remove|\\.clear|\\.extend|\\.copy_from_slice|\\.chunks_exact_mut|\\.iter_mut)`
        );
        if (re.test(lines[i])) writes.get(f).push(fn.name);
      }
    }
  }
  const mutating = fns.filter((fn) =>
    st.fields.some((f) => writes.get(f).includes(fn.name))
  );
  console.log(`   变更该结构的函数：${mutating.length} 个 ⇒ ${mutating.map((f) => f.name).join(", ") || "（无）"}`);
  for (const f of st.fields) {
    const w = [...new Set(writes.get(f))];
    console.log(`   · ${f.padEnd(20)} 写入于 ${w.length} 个函数${w.length ? "：" + w.join(", ") : "（★ 从不写入 ⇒ 只读或漏更新）"}`);
    if (w.length === 0) suspicious += 1;
  }
  // **∴ 可疑：某个字段只被**极少数**函数写 ⇒ **∴ 而**那些函数**也改别的字段 ✓
  const counts = st.fields.map((f) => new Set(writes.get(f)).size).filter((n) => n > 0);
  if (counts.length > 1) {
    const max = Math.max(...counts);
    const min = Math.min(...counts);
    if (max - min >= 2) {
      console.log(`   ★ 可疑：字段的**写入函数数**差异 ${min}…${max} ⇒ 看**少数派**是否漏了同步`);
      suspicious += 1;
    }
  }
}
console.log(`\n⇒ 可疑项：${suspicious}`);
process.exit(suspicious > 0 ? 1 : 0);
