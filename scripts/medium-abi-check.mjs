// 宿主侧 ABI 契约测试：把 `assets/mediums/*.wasm` 当作**笔刷介质插件**加载并验收 ✓。
//
// 宿主今天用 Node 代替浏览器 ✓ —— 二者都是标准 wasm 宿主 ✓，因此契约对浏览器同样成立 ✓。
// 检查的是设计 11.1 的**插件边界**：
//   ① 插件 **imports 为空** ⇒ 拿不到任何宿主能力（无网络、无时钟 ✓）；
//   ② ABI 版本必须自洽 ✓；③ 输出尺寸受 `yanshi_max_dab` **配额**限制 ✓；
//   ④ **同 seed 逐字节一致、不同 seed 不同** ⇒ 随机性完全由注入的 seed 决定 ✓；
//   ⑤ ABI v2 起，宿主用**输入缓冲**注入上下文（笔尖色 / 目标色 / 载墨 / 湿度 ✓）：
//      载墨为 0 必须完全不落墨 ✓；湿度为 1 时颜色必须被目标色主导 ✓（混色 ✓）。
//
// 多版本并存正是设计"插件 id + version 随对象记录、升级不改写历史"所要求的 ✓，
// 因此本脚本一次验收**全部**插件 ✓，而不是只测其中一个 ✓。

import { readFile } from "node:fs/promises";

const paths = process.argv.slice(2).filter((name) => name.endsWith(".wasm"));
if (paths.length === 0) paths.push("assets/mediums/example-dab.wasm", "assets/mediums/oil.wasm");

let failed = 0;
for (const path of paths) {
  if ((await checkOne(path)) !== 0) failed += 1;
}
if (failed > 0) {
  console.log(`  ❌ ${failed}/${paths.length} 个介质插件不合格`);
  process.exit(1);
}
console.log(`  ✅ ${paths.length} 个介质插件 ABI 契约全部通过`);

async function checkOne(path) {
  const bytes = await readFile(path);
  const module = await WebAssembly.compile(bytes);
  const imports = WebAssembly.Module.imports(module);
  const problems = [];
  if (imports.length !== 0) {
    problems.push(`插件不应导入任何宿主函数（实际 ${imports.length} 个：${imports.map((i) => i.name).join(", ")}）`);
  }

  const api = (await WebAssembly.instantiate(module, {})).exports;
  for (const name of ["yanshi_abi_version", "yanshi_max_dab", "yanshi_dab_ptr", "yanshi_dab"]) {
    if (typeof api[name] !== "function") problems.push(`缺少导出函数 ${name}`);
  }
  if (problems.length > 0) return report(path, bytes, imports, 0, 0, {}, problems);

  const abi = api.yanshi_abi_version();
  const maxDab = api.yanshi_max_dab();
  const size = Math.min(16, maxDab);
  const view = () => new Uint8Array(api.memory.buffer, api.yanshi_dab_ptr(), size * size * 4);
  const stats = {};

  // v2 插件要先有**合法上下文**才画得出东西 ✓ —— 否则载墨为 0 ⇒ 什么都没落笔 ⇒
  // "不同 seed 应不同"会误报 ✗（本脚本第一版就踩了：确定性测试跑在写上下文之前 ✓）。
  if (typeof api.yanshi_input_ptr === "function") {
    const floats = api.yanshi_input_len() / 4;
    new Float32Array(api.memory.buffer, api.yanshi_input_ptr(), Math.max(floats, 10))
      .set([1, 0, 0, 1, 0, 0, 0, 1, 1, 0], 0);
  }

  // ④ 确定性：同 seed 逐字节一致 ✓、不同 seed 不同 ✓。
  api.yanshi_dab(1234, size, 1000);
  const first = Uint8Array.from(view());
  api.yanshi_dab(1234, size, 1000);
  const second = view();
  stats.sameSeed = first.reduce((n, byte, i) => n + (byte === second[i] ? 0 : 1), 0);
  if (stats.sameSeed !== 0) problems.push(`同 seed 必须逐字节一致（差异字节 ${stats.sameSeed}）`);
  api.yanshi_dab(4321, size, 1000);
  const third = view();
  stats.crossSeed = first.reduce((n, byte, i) => n + (byte === third[i] ? 0 : 1), 0);
  if (stats.crossSeed === 0) problems.push("不同 seed 应产生不同结果（随机性必须由 seed 驱动）");

  // ③ 配额：请求超过 max_dab 时写入量必须被插件自己收紧 ✓。
  stats.capped = api.yanshi_dab(1, maxDab + 32, 1000);
  if (stats.capped > maxDab * maxDab * 4) {
    problems.push(`超限请求应被限制在 ${maxDab * maxDab * 4} 字节内，实际 ${stats.capped}`);
  }

  // ⑤ v2：输入缓冲注入上下文 ✓。
  if (abi >= 2) {
    stats.v2 = {};
    for (const name of ["yanshi_input_ptr", "yanshi_input_len"]) {
      if (typeof api[name] !== "function") problems.push(`ABI v2 需要导出 ${name}`);
    }
    if (typeof api.yanshi_input_ptr === "function") {
      const floats = api.yanshi_input_len() / 4;
      if (floats < 10) problems.push(`输入缓冲应至少 10 个 f32（实得 ${floats}）`);
      const input = new Float32Array(api.memory.buffer, api.yanshi_input_ptr(), Math.max(floats, 10));

      // 载墨 0 ⇒ 完全不落墨 ✓（"没颜料了" ✓）。
      // 判据要**以返回值为准** ✓：插件此时提前返回 0 字节 ✓，而输出缓冲里还留着**上一次**的点 ✓
      //（本脚本第一版就是去数旧缓冲 ✗，于是把"没落墨"误判成 86 个不透明像素 ✗）。
      input.set([1, 0, 0, 1, 0, 0, 0, 1, 0, 1], 0);
      const emptyWritten = api.yanshi_dab(7, size, 1000);
      stats.v2.emptyLoadWritten = emptyWritten;
      if (emptyWritten !== 0) problems.push(`载墨为 0 时应写入 0 字节（实际 ${emptyWritten}）`);

      // 载墨足 + 湿度 1 ⇒ 颜色被目标色（绿）主导 ✓（混色 ✓）。
      input.set([1, 0, 0, 1, 0, 1, 0, 1, 1, 1], 0);
      api.yanshi_dab(7, size, 1000);
      const mixed = Uint8Array.from(view());
      let greenish = 0;
      let reddish = 0;
      let inked = 0;
      for (let i = 0; i < mixed.length; i += 4) {
        if (mixed[i + 3] === 0) continue;
        inked += 1;
        if (mixed[i + 1] > mixed[i]) greenish += 1;
        if (mixed[i] > mixed[i + 1]) reddish += 1;
      }
      stats.v2.mixedGreen = greenish;
      stats.v2.mixedRed = reddish;
      if (inked === 0) problems.push("载墨充足时应落墨（实测没有）");
      if (greenish <= reddish) {
        problems.push(`湿度 1 时应被目标色（绿）主导（绿 ${greenish} vs 红 ${reddish}）`);
      }
    }
  }

  return report(path, bytes, imports, abi, maxDab, stats, problems);
}

function report(path, bytes, imports, abi, maxDab, stats, problems) {
  console.log(`  插件：${path}（${(bytes.length / 1024).toFixed(1)}KB）｜imports ${imports.length}｜ABI ${abi}｜maxDab ${maxDab}`);
  if (stats.sameSeed !== undefined) {
    console.log(`  确定性：同 seed 差异 ${stats.sameSeed} 字节（须 0）｜不同 seed 差异 ${stats.crossSeed} 字节（须 >0）｜超限写入 ${stats.capped} 字节`);
  }
  if (stats.v2) {
    console.log(`  v2 上下文：载墨 0 时写入 ${stats.v2.emptyLoadWritten} 字节（须 0）｜湿度 1 时绿 ${stats.v2.mixedGreen} / 红 ${stats.v2.mixedRed}（须绿多）`);
  }
  if (problems.length > 0) {
    console.log(`  ❌ ${path} 有 ${problems.length} 项不合格：`);
    for (const problem of problems) console.log(`     - ${problem}`);
    return 1;
  }
  console.log(`  ✅ ${path} ABI 契约通过（无 imports、按 seed 确定、有配额${stats.v2 ? "、v2 上下文正确" : ""}）`);
  return 0;
}
