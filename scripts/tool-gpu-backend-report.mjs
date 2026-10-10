
// **★ 临时目录必须**自己收拾 ✗ ★**（第 53 轮 ✓；**∴ 用户报告 /tmp 被塞满 ✓）：
//   **∴ 我**的浏览器脚本**每个**都建**chromium profile ＋ 临时 root**✗
//     ⇒ **∴ 而**以前**从不删除**✗ ⇒ **∴ 跑几十次就**把 /tmp 塞满 ✓**** ✓✓
//   **∴ 现在**：**注册 ＋ 退出时递归删除**✗ ⇒ **∴ 于是**：**跑多少次都**不积累 ✓**** ✓✓
import { rmSync } from "node:fs";

const __tempPaths = [];
function trackTemp(path) { __tempPaths.push(path); return path; }
// **∴ 删除要**尽力而为 ＋ 重试 ✗ ★**（第 54 轮 ✓；**∴ 用户重启那次换来的 ✓）：
//   **∴ 我**第一版**只删一次**✗ ⇒ **∴ 实测**仍有遗留 ✓
//     （**∴ 因为**chromium**可能**还在写**那个 profile ⇒ **∴ rmSync**失败 ⇒ **∴ 被 catch 吞掉 ✓）**
//   **∴ 所以**：**删三遍**✗（**每遍之间**同步等一会儿 ✓）
//     ＋ **同步信号**（**`SIGINT`／`SIGTERM` ✓）也走**同一条清理 ✓**** ✓✓
function __cleanupTemp() {
  for (let attempt = 0; attempt < 3; attempt += 1) {
    let left = 0;
    for (const path of __tempPaths) {
      try { rmSync(path, { recursive: true, force: true }); }
      catch { left += 1; }
    }
    if (left === 0) return;
    // **∴ 同步等待 ✗**：**∴ 在 exit 钩子里不能用 await ✓** ✓✓
    try { Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 200); } catch { /* 忽略 */ }
  }
}
process.on("exit", __cleanupTemp);
for (const __signal of ["SIGINT", "SIGTERM", "SIGHUP"]) {
  process.on(__signal, () => { __cleanupTemp(); process.exit(1); });
}
//! **★ §6.3 的 ②③④：后端如实上报 ＋ 关掉 GPU 与基线逐字节相同 ✗ ★**（第 35 轮 ✓）。
//!
//! **∴ 依据 ✗**：`docs/design/gpu-webgpu-discussion.md` §6.3 的四条前置判据里，
//!   **②** GPU 不可用 ⇒ fallback ⇒ **与 CPU 基线逐字节相同** ✓
//!   **③** 参数关 GPU ⇒ **与基线逐字节相同** ✓
//!   **④** 响应／诊断**必须报出**实际后端 ＋ **最大通道差** ✓
//!     ⇒ **∴ 而**「**不许**先返回恒 0 冒充 ✓」是明文要求 ✓**** ✓✓
//!
//! **∴ 用法 ✗**：`node scripts/tool-gpu-backend-report.mjs <baseA> [baseB]`
//!   **∴ baseA** ✗：**默认（`--gpu auto` ✓）
//!   **∴ baseB** ✗：**请求 GPU（`--gpu on` ✓）⇒ **∴ 若**本机无 GPU ⇒ **∴ 它**会**如实降级 ✓
//!     ⇒ **∴ 此时**两者的**渲染结果必须**逐字节相同 ✓**** ✓✓
//!
//! **∴ 判读 ✗**：
//!   **∴ ④**：`render_backend` **必须有**✗，**且**当**没有比对**时
//!     `max_channel_delta` **必须是 `null`**✗（**不是 0 ✓）＋ **有说明字段 ✓**
//!   **∴ ②③**：**同一区域**的 `raw` 渲染**逐字节相同** ✓**** ✓✓

// **★ `--spawn` ✗ ★**（第 46 轮 ✓）：**自己起两个服务**✗
//   ⇒ **∴ 于是**：**CI 里**一条命令就能跑 ✓**（**∴ 与 `tool-bitmap-decode-scope` 同款 ✓）** ✓✓
// **★ 判据必须**自足** ✗ ★**（第 77 轮 ✓）：**没有**任何参数时**默认 `--spawn`**✗
//   **∴ 理由**：`run-criteria.sh` **无参枚举**本文件 ⇒ **∴ 否则**它会 `exit(2)`** ✓**
// **★ 我第一版写错了 ✗ ★**（第 77 轮 ✓）：**`process.argv.length` 永远 ≥ 2**✗
//   （**Node 总有 `argv[0]`＝node ＋ `argv[1]`＝脚本 ✓）
//   ⇒ **∴ 所以** `=== 0` **永远为假**✗ ⇒ **∴ 修法**没生效 ✓
//   **∴ 正确写法**：**先切掉那两个**✗ ⇒ **∴ 用** `slice(2).length === 0` ✓**** ✓✓
//   **∴ 参照**：`tool-render-cost-accounts.mjs` **本来就**切过了**✗
//     ⇒ **∴ 所以**它**一次就改对了 ✓**** ✓✓
const looksLikeUrl = (value) =>
  typeof value === "string" && /^https?:\/\//.test(value);
// **★ 两个地址必须**都给、**且都像 URL ✗ ★**（第 82 轮 ✓）：
//   **∴ 因为**编排**永远给三个参数**✗（`"$BASE" "$doc" "$tok"` ✓）
//     ⇒ **∴ 于是**：**"**只给一个 URL ✓"**这个情形**不存在** ✓**** ✓✓
//     ⇒ **∴ 所以**：**要么**两个都是 URL**✗（**显式指定 ✓）
//       ⇒ **∴ 要么**自己起两个** ✓**** ✓✓
//   **∴ 我**第一版只判了 `argv[2]`**✗
//     ⇒ **∴ 于是**：**`argv[2]` 真 URL ＋ **`argv[3]` 文档名**✗
//       ⇒ **∴ `spawnMode` 为 false ＋ **`b` 为 null**✗ ⇒ **∴ 判据**仍然**不成立 ✓**** ✓✓
const bothUrls = looksLikeUrl(process.argv[2]) && looksLikeUrl(process.argv[3]);
let a = bothUrls ? process.argv[2] : "http://127.0.0.1:8471";
let b = bothUrls ? process.argv[3] : null;
const spawnMode = process.argv.includes("--spawn")
  || process.argv.slice(2).length === 0
  || !bothUrls;
// **★ 参数**看起来像 URL**才采用 ✗ ★**（第 82 轮 ✓；**CI 的 `ERR_INVALID_URL` 换来的 ✓）：
//   **∴ 症状（**CI 实测 ✓）✗**：`ERR_INVALID_URL`，`input: 'crit_toolgpubackendreportmjs/health'`
//     ⇒ **∴ 因为** `run-criteria.sh:228` **给通用 `tool-*` 传三个参数**✗：
//       `"$BASE" "$doc" "$tok"` ✓**** ✓✓
//     ⇒ **∴ 于是** `argv[3]`**是**文档名**✗，**不是** base URL**✗
//       ⇒ **∴ 而**本判据**把 `argv[3]` 当成**第二个服务**的地址**✗
//         ⇒ **∴ 于是**：**B 的 base** ＝ **文档名** ⇒ **∴ URL**解析失败 ✓**** ✓✓
//   **∴ 修法**：**只接受** `http://` 或 `https://` 开头的参数**✗
//     ⇒ **∴ 于是**：**编排传文档名时**✗ ⇒ **∴ 两者都是 null ⇒ **∴ 走 spawn ✓**** ✓✓
let spawned = [];
if (spawnMode) {
  const { spawn } = await import("node:child_process");
  const { mkdtempSync, existsSync } = await import("node:fs");
  const { tmpdir } = await import("node:os");
  const { join } = await import("node:path");
  const binary = ["target/release/yanshi-serve", "target/debug/yanshi-serve"]
    .find((path) => existsSync(path));
  if (!binary) {
    console.error("✗ --spawn 需要先构建一个 yanshi-serve（release 或 debug）");
    process.exit(2);
  }
  const start = async (port, extra) => {
    const root = trackTemp(mkdtempSync(join(tmpdir(), "gpu-report-")));
    const child = spawn(binary, ["--root", root, "--bind", `127.0.0.1:${port}`, ...extra], {
      stdio: "ignore",
    });
    for (let i = 0; i < 80; i += 1) {
      try {
        if ((await fetch(`http://127.0.0.1:${port}/health`)).ok) return child;
      } catch { /* 还没起来 */ }
      await new Promise((r) => setTimeout(r, 250));
    }
    child.kill();
    throw new Error(`服务未在 127.0.0.1:${port} 起来`);
  };
  // **∴ 一默认（**auto ✓）**一请求 GPU（**on ✓）✗ ⇒ **∴ 正是**§6.3 的 ②③ ✓**** ✓✓
  spawned.push(await start(8791, []));
  spawned.push(await start(8792, ["--gpu", "on"]));
  a = "http://127.0.0.1:8791";
  b = "http://127.0.0.1:8792";
  process.on("exit", () => {
    for (const child of spawned) {
      try { child.kill(); } catch { /* 已经没了 */ }
    }
  });
  for (const sig of ["SIGINT", "SIGTERM"]) process.on(sig, () => process.exit(1));
  console.log(`  · --spawn：默认 ${a}｜请求 GPU ${b}｜二进制 ${binary}`);
}

const bad = [];

async function health(base) {
  const res = await fetch(`${base}/health`);
  return res.json();
}

async function post(base, path, body) {
  const res = await fetch(`${base}${path}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  return res.json();
}

/** **∴ 在某个服务上建同构的文档并渲同一区域**✗ ⇒ **∴ 返回原始像素字节 ✓** */
async function renderSame(base, tag) {
  const docId = `gpurep_${tag}`;
  const created = await post(base, "/api/documents", {
    doc_id: docId,
    width: 512,
    height: 512,
  });
  const token = created.token;
  const q = `doc=${docId}&token=${token}`;
  for (const id of ["L0", "L1", "L2"]) {
    await post(base, `/api/tools/create_layer?${q}`, { layer_id: id });
  }
  await post(base, `/api/tools/fill?${q}`, {
    layer_id: "L1",
    object_id: "o1",
    data: {
      color: { r: 40, g: 90, b: 160, a: 210 },
      region: { x: 8, y: 8, width: 300, height: 200 },
    },
  });
  const out = await post(base, `/api/tools/render_region?${q}`, {
    region: { x: 0, y: 0, w: 256, h: 256 },
    raw: true,
  });
  // **★ 比的是**像素** ✗ ★**（第 35 轮 ✓；**∴ 我**第一版比**整个响应 JSON** ✗
  //   ⇒ **∴ 其中含 `renders_done`／`below_tiles_*` 等**会变的字段**✗
  //     ⇒ **∴ 于是**必然不同 ⇒ **★ 那是**我脚本的错 ✓ ★**** ✓✓
  //   **∴ raw 出口**给的是 `raw_url`**✗（**不内联像素 ✓）
  //     ⇒ **∴ 要**再 GET 它 ✓ ⇒ **∴ 比**字节 ✓**** ✓✓
  if (!out.raw_url) {
    return { bytes: `NO_RAW_URL:${JSON.stringify(Object.keys(out))}`, out };
  }
  const url = out.raw_url.startsWith("http")
    ? out.raw_url
    : `${base}${out.raw_url}`;
  const res = await fetch(url);
  const buf = new Uint8Array(await res.arrayBuffer());
  // **∴ 用长度 ＋ FNV-1a 做指纹 ✗**（**∴ 比**每个字节**也行 ✓，**而**指纹**便于打印 ✓）** ✓✓
  let hash = 0x811c9dc5;
  for (const byte of buf) {
    hash ^= byte;
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return { bytes: `${buf.length}:${hash.toString(16)}`, len: buf.length, hash, out };
}

const ha = await health(a);
console.log("  A 的 /health：", JSON.stringify({
  render_backend: ha.render_backend,
  gpu_mode: ha.gpu_mode,
  gpu_adapter_note: ha.gpu_adapter_note,
  max_channel_delta: ha.max_channel_delta,
}));

// **④ 后端必须如实上报。**
if (ha.render_backend !== "cpu" && ha.render_backend !== "gpu") {
  bad.push(`render_backend 必须是 cpu 或 gpu ✗（实测 ${ha.render_backend}）`);
}
if (ha.render_backend === "cpu" && !ha.gpu_adapter_note) {
  bad.push("走了 CPU ⇒ 必须给出 gpu_adapter_note ✗（不许静默降级）");
}
if (!("max_channel_delta" in ha)) {
  bad.push("诊断里必须报 max_channel_delta ✗（§6.3 的 ④）");
} else if (ha.max_channel_delta === 0) {
  bad.push("max_channel_delta 是 0 ⇒ 疑似**恒 0 冒充** ✗（没有比对时必须报 null）");
}
if (ha.render_backend === "cpu" && ha.max_channel_delta !== null) {
  bad.push(`CPU 路径没有比对 ⇒ max_channel_delta 必须是 null ✗（实测 ${ha.max_channel_delta}）`);
}

// **②③ 逐字节相同。**
if (b) {
  const hb = await health(b);
  console.log("  B 的 /health：", JSON.stringify({
    render_backend: hb.render_backend,
    gpu_mode: hb.gpu_mode,
    gpu_adapter_note: hb.gpu_adapter_note,
    max_channel_delta: hb.max_channel_delta,
  }));
  const ra = await renderSame(a, "a");
  const rb = await renderSame(b, "b");
  const same = ra.bytes === rb.bytes;
  console.log(`  ②③ 像素指纹：A=${ra.bytes}｜B=${rb.bytes}`);
  console.log(`  ②③ 两个服务同区域渲染是否逐字节相同：${same}`);
  if (!same) {
    bad.push("请求 GPU 的服务与默认服务的渲染结果**不同** ⇒ 违反 §6.3 的 ②③");
  }
}

if (bad.length) {
  console.error("  ❌ §6.3 的 ②③④ 未达标：");
  for (const line of bad) console.error("     - " + line);
  process.exit(1);
}
console.log("  ✓ §6.3 的 ②③④ 达标：后端如实上报 ＋ 无 GPU 时报 null ＋ 两条路径逐字节相同");
// **★ 必须**显式退出 ✗ ★**（第 46 轮 ✓）：**∴ `--spawn` 起过子进程**✗
//   ⇒ **∴ 父进程**的事件循环**不会被自动清空**✗ ⇒ **∴ 实测**退出码 **124**（**超时 ✓）**
//   ⇒ **∴ 所以**：**结束处**显式 `exit`**✗ ⇒ **∴ 于是**：**exit 钩子**立刻杀子进程 ✓**** ✓✓
process.exit(0);
