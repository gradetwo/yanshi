//! 宿主侧 ABI 契约测试：把 `assets/mediums/*.wasm` 当作**笔刷介质插件**加载并验收 ✓。
//
// 宿主今天用 Node 代替浏览器 ✓ —— 二者都是标准 wasm 宿主 ✓，因此这条契约对浏览器同样成立 ✓。
// 检查的是设计 11.1 的**插件边界**：
//   ① 插件 **imports 为空** ⇒ 拿不到任何宿主能力（无网络、无时钟 ✓）；
//   ② ABI 版本必须匹配 ✓；③ 输出尺寸受 `yanshi_max_dab` **配额**限制 ✓；
//   ④ **同 seed 逐字节一致、不同 seed 不同** ⇒ 随机性完全由注入的 seed 决定 ✓（确定性 ✓）。

import { readFile } from "node:fs/promises";

const path = process.argv[2] || "assets/mediums/example-dab.wasm";
const bytes = await readFile(path);
const module = await WebAssembly.compile(bytes);
const imports = WebAssembly.Module.imports(module);

const problems = [];
if (imports.length !== 0) {
  problems.push(`插件不应导入任何宿主函数（实际 ${imports.length} 个：${imports.map((i) => i.name).join(", ")}）`);
}

const instance = await WebAssembly.instantiate(module, {});
const api = instance.exports;
for (const name of ["yanshi_abi_version", "yanshi_max_dab", "yanshi_dab_ptr", "yanshi_dab"]) {
  if (typeof api[name] !== "function") problems.push(`缺少导出函数 ${name}`);
}

let abi = 0;
let maxDab = 0;
let sameSeed = 0;
let crossSeed = 0;
let capped = 0;
if (problems.length === 0) {
  abi = api.yanshi_abi_version();
  if (abi !== 1) problems.push(`ABI 版本应为 1，实际 ${abi}`);

  maxDab = api.yanshi_max_dab();
  const size = Math.min(16, maxDab);
  const buffer = () => new Uint8Array(api.memory.buffer, api.yanshi_dab_ptr(), size * size * 4);

  api.yanshi_dab(1234, size, 1000);
  const first = Uint8Array.from(buffer());
  api.yanshi_dab(1234, size, 1000);
  const second = buffer();
  let differing = 0;
  for (let i = 0; i < first.length; i += 1) if (first[i] !== second[i]) differing += 1;
  sameSeed = differing;
  if (differing !== 0) problems.push(`同 seed 必须逐字节一致（差异字节 ${differing}）`);

  api.yanshi_dab(4321, size, 1000);
  const third = buffer();
  let cross = 0;
  for (let i = 0; i < first.length; i += 1) if (first[i] !== third[i]) cross += 1;
  crossSeed = cross;
  if (cross === 0) problems.push("不同 seed 应产生不同结果（随机性必须由 seed 驱动）");

  // 配额：请求超过 max_dab 的尺寸时，写入量必须被插件自己收紧到上限 ✓。
  const over = api.yanshi_dab(1, maxDab + 32, 1000);
  capped = over;
  if (over > maxDab * maxDab * 4) {
    problems.push(`请求超大尺寸时写入量应被限制在 ${maxDab * maxDab * 4} 字节内，实际 ${over}`);
  }
}

console.log(`  插件：${path}（${(bytes.length / 1024).toFixed(1)}KB）｜imports ${imports.length}｜ABI ${abi}｜maxDab ${maxDab}`);
console.log(`  确定性：同 seed 差异 ${sameSeed} 字节（须 0）｜不同 seed 差异 ${crossSeed} 字节（须 >0）｜超限请求写入 ${capped} 字节`);
if (problems.length > 0) {
  console.log(`  ❌ ${problems.length} 项不合格：`);
  for (const problem of problems) console.log(`     - ${problem}`);
  process.exit(1);
}
console.log("  ✅ 介质插件 ABI 契约通过（无 imports、版本匹配、按 seed 确定、有配额）");
