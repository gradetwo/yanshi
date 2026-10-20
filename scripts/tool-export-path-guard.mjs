#!/usr/bin/env node
// **`export_png` 的路径守卫不许被 `..` 绕过**（第 1342 轮；外部 **X-01 P1 安全**）
//
// **⇒ 缺陷（我实测复现）**：
//   `guarded_output_path` 的绝对路径分支只做 `Path::starts_with` 前缀匹配，**不归一化**
//   ⇒ `/tmp/x/../../home/<user>/evil.png` 的组件前缀**确实是** `/tmp` ⇒ **放行**
//   ⇒ 而 `fs::write` 会解释 `..` ⇒ **真的写到许可目录之外**（实测 `ok=true`、121 字节落盘）✗
//
// **⇒ 断什么**（5 条，覆盖「该拒的拒、该允的允、且拒绝时**没有**副作用）：
//   ① `/tmp/x/../..` 穿越到家目录 ⇒ **必须拒**
//   ② 许可外的绝对路径 ⇒ **必须拒**
//   ③ 相对路径带 `..` ⇒ **必须拒**
//   ④ 导出目录内的绝对路径 ⇒ **必须允**
//   ⑤ 纯文件名 ⇒ **必须允**
//   ⑥ ①②③ 拒绝之后，**越界位置不许有新文件**（拒绝必须是**真拒绝**，不是「写失败」）
//
// **⇒ 退出码**：0 通过｜1 不符｜**3 不可判**
import { existsSync, readdirSync } from "node:fs";
import os from "node:os";
import path from "node:path";

const [base] = process.argv.slice(2);
if (!base) {
  console.error("用法: node scripts/tool-export-path-guard.mjs <base>");
  process.exit(3);
}
const expDir = process.env.YANSHI_EXPORT_DIR
  || (process.env.YANSHI_WORKSPACE ? `${process.env.YANSHI_WORKSPACE}/exports` : null);
if (!expDir) {
  console.error("  ⊘ 不可判：找不到导出目录（YANSHI_EXPORT_DIR / YANSHI_WORKSPACE）");
  process.exit(3);
}
const stamp = Date.now();
const post = async (p, b, doc, tok) => {
  const u = `${base}${p}${doc ? `?doc=${doc}&token=${tok}` : ""}`;
  const r = await fetch(u, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(b ?? {}) });
  return r.json();
};
const doc = `guard_${stamp}`;
const r = await post("/api/documents", { doc_id: doc, width: 40, height: 40 });
if (!r?.token) { console.error("  ⊘ 不可判：建文档失败"); process.exit(3); }
const T = r.token;
const exp = async (p) => {
  const x = await post("/api/tools/export_png", { path: p, deterministic: true }, doc, T);
  return x?.ok === true;
};

// **⇒ 越界目标**：家目录下的一个唯一名（①用穿越到它，②直接用绝对路径）
const homeOut = path.join(os.homedir(), `dg_guard_${stamp}.png`);
const homeOut2 = path.join(os.homedir(), `dg_guard2_${stamp}.png`);
// **⇒ ① 的穿越写法**：`/tmp/x/../../<home 的相对部分>`（`/tmp` 下先建 `x`）
const tmpX = "/tmp/x";
try { (await import("node:fs")).mkdirSync(tmpX, { recursive: true }); } catch { /* 已有 */ }
const homeRel = homeOut.replace(/^\//, "");
const traversal = `/tmp/x/../../${homeRel}`;

const fails = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fails.push(m); console.log(`  ✗ ${m}`); };

const accepted = await exp(traversal);
if (!accepted) ok(`① 穿越路径被拒（${traversal}）`);
else bad(`① 穿越路径必须被拒 ⇒ 实测被接受（${traversal}）`);

const accepted2 = await exp(homeOut2);
if (!accepted2) ok(`② 许可外的绝对路径被拒（${homeOut2}）`);
else bad(`② 许可外的绝对路径必须被拒 ⇒ 实测被接受（${homeOut2}）`);

const accepted3 = await exp(`../dg_rel_${stamp}.png`);
if (!accepted3) ok("③ 相对路径带 .. 被拒");
else bad("③ 相对路径带 .. 必须被拒 ⇒ 实测被接受");

const accepted4 = await exp(path.join(expDir, `guard_ok_${stamp}.png`));
if (accepted4) ok("④ 导出目录内的绝对路径被接受");
else bad(`④ 导出目录内的绝对路径**必须**被接受（否则正常用法被关掉）⇒ expDir=${expDir}`);

const accepted5 = await exp(`guard_ok_rel_${stamp}.png`);
if (accepted5) ok("⑤ 纯文件名被接受");
else bad("⑤ 纯文件名**必须**被接受");

// ⑥ 拒绝必须是**真拒绝**：越界位置不许有新文件
if (!existsSync(homeOut) && !existsSync(homeOut2)) ok("⑥ 三次拒绝之后，越界位置没有新文件（是真拒绝）");
else {
  const leaked = [homeOut, homeOut2].filter((f) => existsSync(f));
  bad(`⑥ 越界位置出现了新文件 ⇒ ${leaked.join("、")}（拒绝不是真拒绝）`);
}

if (fails.length > 0) {
  console.log(`\n  ✗ ${fails.length} 处不符（路径守卫不许被 .. 绕过）`);
  process.exit(1);
}
console.log("\n  ✓ export_png 的路径守卫守住（穿越/越权被拒，合法路径照常）");
