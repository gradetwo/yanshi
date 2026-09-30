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
if (paths.length === 0) {
  paths.push(
    "assets/mediums/example-dab.wasm",
    "assets/mediums/oil.wasm",
    "assets/mediums/watercolor.wasm",
    // 设计 11.1 的其余介质 ✓（马克笔 / 铅笔 ✓）—— 与前三者走**同一套**边界检查 ✓：
    // 零 imports ✓、ABI 版本 ✓、同 seed 一致 / 不同 seed 有别 ✓、配额 ✓、v2 上下文 ✓。
    "assets/mediums/marker.wasm",
    "assets/mediums/pencil.wasm",
  );
}

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
      // **湿混是"湿介质"的特性，不是 ABI 的通则** ✗ —— 加入铅笔时这里报了不合格 ✓，
      // 而铅笔**本来就该**忽略湿度 ✓（干介质不会把下面的颜色拉上来 ✓，见插件文档 ✓）。
      // 干介质不但**允许**不混色 ✓，还必须**断言它确实不混** ✓，否则"有没有实现"无从验证 ✓。
      const wetMedium = path.includes("oil") || path.includes("watercolor");
      if (wetMedium && greenish <= reddish) {
        problems.push(`湿介质在湿度 1 时应被目标色（绿）主导（绿 ${greenish} vs 红 ${reddish}）`);
      }
      if (!wetMedium && reddish <= greenish) {
        problems.push(`干介质不该把目标色拉上来（红 ${reddish} 应多于绿 ${greenish}）`);
      }

  // --- marker specific --------------------------------------------------------
  // 马克笔的标志性行为有两条 ✓，都能量 ✓：
  //   ① **平头笔尖是各向异性的** ✓（沿长轴比沿短轴宽得多 ✓）；
  //   ② **叠色会变深** ✓ —— 但注意**插件是无状态的** ✓：同一输入永远同一输出 ✓
  //      （上面已断言 ✓）。所以这里比较"目标处**已有墨**"与"目标处**干净**"两种输入的输出 ✓，
  //      前者的颜色应更暗 ✓。（我第一版写成"同一输入调用两次再比" ✗ ⇒ 两次必然相同 ✓，
  //      因为叠色发生在**宿主的合成**里 ✓ 而不是插件的内部状态 ✓。）
  if (path.includes("marker")) {
    const size = 48;
    input.set([1, 0, 0, 1, 1, 1, 1, 1, 1, 0], 0);
    api.yanshi_dab(11, size, 1000);
    const nib = Uint8Array.from(view());
    let minX = size, maxX = -1, minY = size, maxY = -1;
    for (let y = 0; y < size; y++) {
      for (let x = 0; x < size; x++) {
        if (nib[(y * size + x) * 4 + 3] > 8) {
          if (x < minX) minX = x;
          if (x > maxX) maxX = x;
          if (y < minY) minY = y;
          if (y > maxY) maxY = y;
        }
      }
    }
    const spanX = maxX - minX + 1;
    const spanY = maxY - minY + 1;
    const anisotropy = Math.max(spanX, spanY) / Math.max(1, Math.min(spanX, spanY));
    const dabWith = (dstAlpha) => {
      input.set([1, 0, 0, 1, 0.5, 0.5, 0.5, dstAlpha, 1, 0], 0);
      api.yanshi_dab(11, size, 1000);
      const dab = Uint8Array.from(view());
      let colour = 0;
      let count = 0;
      for (let i = 0; i < dab.length; i += 4) {
        if (dab[i + 3] === 0) continue;
        colour += dab[i] + dab[i + 1] + dab[i + 2];
        count += 1;
      }
      return { colour, count };
    };
    const onClean = dabWith(0);
    const onInked = dabWith(1);
    stats.marker = { anisotropy, spanX, spanY, onClean: onClean.colour, onInked: onInked.colour };
    if (anisotropy < 1.35) {
      problems.push(`平头笔尖应各向异性（实测 ${spanX}×${spanY}，比值 ${anisotropy.toFixed(2)}，须 ≥1.35）`);
    }
    if (onClean.count === 0) {
      problems.push("马克笔在载墨充足时没有落墨");
    } else if (!(onInked.colour < onClean.colour)) {
      problems.push(`马克笔在已有墨的目标上应更深（干净 ${onClean.colour} vs 已有墨 ${onInked.colour}）`);
    }
  }

  // --- pencil specific --------------------------------------------------------
  // 铅笔的标志性行为 ✓：**压力决定深浅** ✓（轻压只有稀疏石墨 ✓、重压才实 ✓）。
  if (path.includes("pencil")) {
    const size = 48;
    const coverageAt = (pressure) => {
      input.set([0, 0, 0, 1, 1, 1, 1, 1, 1, 0], 0);
      api.yanshi_dab(5, size, pressure);
      const dab = Uint8Array.from(view());
      let sum = 0;
      for (let i = 3; i < dab.length; i += 4) sum += dab[i];
      return sum;
    };
    const light = coverageAt(120);
    const medium = coverageAt(500);
    const heavy = coverageAt(1000);
    stats.pencil = { light, medium, heavy };
    if (!(heavy > medium && medium > light)) {
      problems.push(`铅笔的深浅必须随压力单调增加（轻 ${light} / 中 ${medium} / 重 ${heavy}）`);
    }
    if (light >= heavy * 0.75) {
      problems.push(`轻压应明显比重压淡（轻 ${light} vs 重 ${heavy}）`);
    }
  }

      // --- watercolour specific -------------------------------------------------
      // 水彩与油画的差别不在参数而在**行为** ✓（见插件文档 ✓）。这里量三条标志性特征 ✓：
      //   ① 半透明（留白 ✓）；② 边缘沉积（外沿比中心深 ✓）；③ 边界不规则（水痕 ✓）。
      if (path.includes("watercolor")) {
        const centre = Uint8Array.from(view());
        const n = size;
        let sumAll = 0;
        let sumInner = 0;
        let innerCount = 0;
        let sumOuter = 0;
        let outerCount = 0;
        for (let y = 0; y < n; y++) {
          for (let x = 0; x < n; x++) {
            const a = centre[(y * n + x) * 4 + 3];
            sumAll += a;
            const dx = x + 0.5 - n / 2;
            const dy = y + 0.5 - n / 2;
            const r = Math.hypot(dx, dy) / (n / 2);
            if (r < 0.45) { sumInner += a; innerCount += 1; }
            else if (r > 0.6 && r < 0.95) { sumOuter += a; outerCount += 1; }
          }
        }
        const meanAll = sumAll / (n * n);
        const meanInner = innerCount ? sumInner / innerCount : 0;
        const meanOuter = outerCount ? sumOuter / outerCount : 0;
        stats.watercolor = {
          meanAll: meanAll.toFixed(1),
          meanInner: meanInner.toFixed(1),
          meanOuter: meanOuter.toFixed(1),
        };
        if (meanAll > 180) {
          problems.push(`水彩应半透明（整体 alpha 均值 ${meanAll.toFixed(1)} 过高）`);
        }
        if (meanOuter <= meanInner) {
          problems.push(`水彩应有边缘沉积（外沿 ${meanOuter.toFixed(1)} 应深于中心 ${meanInner.toFixed(1)}）`);
        }
        // 边界不规则：同一半径上不同角度的 alpha 应有明显起伏 ✓（圆形笔尖则几乎为 0 ✗）。
        let minRing = 255;
        let maxRing = 0;
        for (let k = 0; k < 72; k++) {
          const angle = (k / 72) * Math.PI * 2;
          const x = Math.round(n / 2 + Math.cos(angle) * n * 0.45);
          const y = Math.round(n / 2 + Math.sin(angle) * n * 0.45);
          if (x < 0 || y < 0 || x >= n || y >= n) continue;
          const a = centre[(y * n + x) * 4 + 3];
          minRing = Math.min(minRing, a);
          maxRing = Math.max(maxRing, a);
        }
        stats.watercolor.ringSpread = maxRing - minRing;
        if (maxRing - minRing < 20) {
          problems.push(`水彩边界应不规则（同半径上 alpha 起伏仅 ${maxRing - minRing}）`);
        }
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
  if (stats.marker) {
    console.log(`  马克笔特征：各向异性 ${stats.marker.anisotropy.toFixed(2)}（${stats.marker.spanX}×${stats.marker.spanY}）｜已有墨处更深 ${stats.marker.onInked < stats.marker.onClean ? "✓" : "✗"}（干净 ${stats.marker.onClean} vs 已有墨 ${stats.marker.onInked}）`);
  }
  if (stats.pencil) {
    console.log(`  铅笔特征：压力 120/500/1000 的墨量 ${stats.pencil.light}/${stats.pencil.medium}/${stats.pencil.heavy}（须单调递增）`);
  }
  if (stats.watercolor) {
    console.log(`  水彩特征：整体 alpha ${stats.watercolor.meanAll}｜中心 ${stats.watercolor.meanInner} / 外沿 ${stats.watercolor.meanOuter}（须外沿更深）｜同半径起伏 ${stats.watercolor.ringSpread}（须 ≥20）`);
  }
  if (problems.length > 0) {
    console.log(`  ❌ ${path} 有 ${problems.length} 项不合格：`);
    for (const problem of problems) console.log(`     - ${problem}`);
    return 1;
  }
  console.log(`  ✅ ${path} ABI 契约通过（无 imports、按 seed 确定、有配额${stats.v2 ? "、v2 上下文正确" : ""}）`);
  return 0;
}
