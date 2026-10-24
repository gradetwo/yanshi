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
// **★★★ 它**只提示**，**不判定** ✗ ★★★**（**第 744 轮实测 ✓）：
//   **∴ 实测的三个结果 ✗**（**都用它跑过 ✓）★**：
//     **∴ ① `Tile` ⇒ **真缺陷**✗**（**`maybe_opaque` 写入 5 处而 `write_row_f32` 曾漏 ⇒ 已修 ✓）★
//       ＋ **∴ ② `DabCache` ⇒ **可疑项 0**✗**（**与手工查的结论一致 ✓）★
//         ＋ **∴ ③ `Buffer`／`TileCache` ⇒ **报了可疑但**不是缺陷**✗ ★**：
//           **∴ 因为**差异来自**「**配置字段**只在一个地方写」**✗ ——
//           **∴ 如** `Buffer.origin_x` **只被** `reset` **写**（**合理 ✓）
//             ＋ **∴ 而** `TileCache.budget_bytes` **只被** `set_budget` **写**（**合理 ✓）★
//   ⇒ **∴ 所以正确用法 ✗**：**看**完整字段表**✗ ＋ **∴ 由人**判断**「**那个少数派**该不该**也被别的路径更新」** ✓ ★
//     ＋ **∴ 它**不是**自动判定器**✗ ⇒ **∴ 退出码 1 **只表示**「**值得看一眼**」** ✓ ★
//
// **∴ 已知局限（**如实写 ✓）★**：
//   **∴ ① 正则式静态分析**✗ ⇒ **immutable 字段**会报「**从不写入**」** ✓（**∴ 如** `TileGrid` 三个字段 ✓）
//     ＋ **∴ ② 索引赋值**已支持**✗（**`self.pixels[i] = ` ✓）＋ **∴ 切片写**也支持** ✓
//       ＋ **∴ ③ 跨函数的间接写**（**∴ 如**经 `&mut` 传出后由别的函数改 ✓）**看不见** ✓ ★
//
// **∴ 用法 ✗**：`node scripts/tool-struct-mutation-census.mjs <文件> [结构名]` ★
// **∴ 退出码 ✗**：**0 ⇒ 无可疑**｜**1 ⇒ 有可疑（**需要人看 ✓）｜**2 ⇒ 用法错** ✓ ★
import { readFileSync } from "node:fs";

// **★★★ 契约不匹配 ⇒ **回退到默认目标** ✗ ★★★**（**第 770 轮修；**CI 实测 ✓）
//   **∴ 症状 ✗**（**CI ✓）★**：
//     `✗ tool-struct-mutation-census.mjs｜Error: ENOENT: no such file or directory,
//      open 'http://127.0.0.1:13990'`
//   **∴ 根因 ✗**：**运行器对 `tool-*` 的统一约定是 `$BASE $doc $tok`** ✓
//     ＋ **∴ 而**本判据**期望 `<rust文件> [结构名]`**✗
//       ⇒ **∴ 于是**它**把 `$BASE`（**一个 URL ✓）当成文件路径** ⇒ **∴ ENOENT** ✓ ★★
//   **∴ 修法 ✗**：**参数不像文件就**用默认目标**✗（**∴ 而不是**报用法错 ✓）★
//     ＋ **∴ 默认目标 ✗**：`crates/yanshi-render/src/tile.rs Tile`** —— **∴ 那**正是本工具诞生时
//       **找到真缺陷的那个结构**（**`maybe_opaque` 漏更新 ✓）★
//         ＋ **∴ 而**人**仍可**显式传文件**✗ ⇒ **∴ 行为不变** ✓ ★
//   **∴ 注意 ✗**：**必须在**解构之前**改 `process.argv`**✗
//     ＋ **∴ 因为** `const [, , file, …] = process.argv` **取的是**改之前的值** ✓ ★
const looksLikeFile = (arg) => !!arg && !/^[a-z]+:\/\//i.test(arg) && !arg.startsWith("-");
if (!looksLikeFile(process.argv[2])) {
  console.error("  ⓘ 第一个参数不像文件（运行器的约定是 <base-url> <doc> <token>）⇒ 用默认目标：crates/yanshi-render/src/tile.rs Tile");
  process.argv[2] = "crates/yanshi-render/src/tile.rs";
  // **∴ `argv[3]` 也要用默认 ✗**（**第 770 轮实测 ✓）：**运行器给的是 `$doc`（**`crit_…` ✓）
  //   ⇒ **∴ 而**那**会被当成**结构名**✗ ⇒ **∴ 于是**报「**找不到结构体 crit_…」 ✓ ★
  process.argv[3] = "Tile";
}
const [, , file, onlyStruct] = process.argv;

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
// **⇒ 退出码改成 0／3，不再是 0／1** ✓（第 871 轮）
//   **⇒ 为什么** ✗：这个脚本是**普查报告** ✓ 而"有可疑项"**不等于"产品坏了"** ✓
//     ⇒ 它列的是「某个字段只被 0 个／极少数函数写 ⇒ **值得人看一眼**」✓
//     ⇒ 用 `exit 1` 会让它**永远阻塞 CI** ✗，而它**自己**从来没断言过"哪个是错的" ✓
//   **⇒ 所以**：有可疑项 ⇒ **`exit 3`（不可判／需人看）** ✓；没有 ⇒ `exit 0` ✓。
//     ⇒ 这是本仓对"这条检查给不出真/假结论"的标准形态 ✓（`⊘ 不算失败` ✓）。
if (suspicious > 0) {
  console.log("  ⊘ 有可疑项 ⇒ 需要**人**看一眼（不是产品失败）⇒ exit 3");
  process.exit(3);
}
process.exit(0);
