#!/usr/bin/env node
// **"描述承诺的类型"必须被 schema 接受**（第 843 轮）：外部报告 #10 实测的一条 ✗ ——
// `fill_region` 的 `color` **描述写着支持 `#rrggbb`** ✓，而 `param!` 把类型声明成 **`Object`** ✗
// ⇒ 调用方按描述传字符串 ⇒ **在 schema 层就被拒**（`expected object, got string` ✓），**走不到实现** ✓；
// 而实现其实 `.and_then(Value::as_str)` ⇒ **它是支持字符串的** ✓ ⇒ **该改的是 schema** ✓。
//
// **为什么现有判据抓不到** ✗：`tool-param-parity.mjs` 比的是「声明的**参数名** vs 实现接受的**名字**」✓
// ⇒ 它看不见「声明的**类型**比实现更窄」✗ —— 字符串在 schema 层就被挡下 ✓。
//
// **判据（能红 ✓，判的正是那个条件 ✓）**：
//   ① 扫 `tools.rs` 里每一处 `param!("<名>", <类型>, <必填>, "<描述>")` ✓；
//   ② 描述提到 `#rrggbb` 而类型是 `Object` ⇒ **红并点名** ✓；
//   ③ 一处都扫不到 ⇒ **报错**（不许静默通过 ✗）；提到十六进制色的一处都没有 ⇒ **也报错** ✓；
//   ④ **打印覆盖面** ✓。
// 用法：node scripts/tool-color-schema.mjs（**静态检查 ✓，不需要服务端** ✓）
import { readFileSync } from "node:fs";

const SRC = "crates/yanshi-server/src/tools.rs";
const text = readFileSync(SRC, "utf8");
const re = /param!\(\s*"([a-z_0-9]+)"\s*,\s*([A-Za-z]+)\s*,\s*(true|false)\s*,\s*"((?:[^"\\]|\\.)*)"/g;
const all = [...text.matchAll(re)];
if (all.length === 0) {
  console.error("❌ 在 " + SRC + " 里一处 param! 都没扫到 ⇒ 判据无法运行（**不是通过** ✗）");
  process.exit(1);
}
const hexNamed = all.filter((m) => /#(rrggbb|RRGGBB)/.test(m[4]));
const offences = hexNamed.filter((m) => m[2] === "Object");

let failed = false;
if (offences.length > 0) {
  console.error("  ✗ " + offences.length + " 处：描述承诺接受十六进制颜色，而 schema 声明为 Object ⇒ 调用方会被拒：");
  for (const m of offences) console.error('     - param!("' + m[1] + '", ' + m[2] + ', ' + m[3] + ', "… #rrggbb …") ⇒ 应改为 Any');
  console.error("  ⇒ 修法：**先读实现再决定改哪边** ✓ —— 实现接受字符串就改 schema ✓，否则改描述 ✓。");
  failed = true;
}
if (hexNamed.length === 0) {
  console.error("  ✗ 一处「描述里提到十六进制色」的 param! 都没扫到 ⇒ 判据成了空话（**不是通过** ✗）");
  failed = true;
}
console.log("  扫描：" + SRC + " ⇒ param! 共 " + all.length + " 处 ✓｜描述提到十六进制色 " + hexNamed.length + " 处 ✓");
if (!failed) console.log("  ✓ 凡描述承诺接受十六进制颜色的参数，其类型都不是 Object ⇒ 描述与 schema 一致");
process.exit(failed ? 1 : 0);
