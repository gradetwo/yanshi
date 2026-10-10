#!/usr/bin/env node
// **★ 浏览器内核必须**如实上报后端**：`render_backend` ＋ `webgpu_used_by_kernel` ＋ 原因 ✗ ★**
//   （第 444 轮 ✓；**目标第 7 条 ✓）
//
// **∴ 为什么必须有这条 ✗**：**目标第 7 条**说：
//   **「**CPU 是真值，GPU 是可选加速**；**不许假装用了 GPU**；**不许静默降级**；
//     **`render_backend` 必须报**实际**后端**」 ✓
//   ⇒ **∴ 而**第 444 轮实测**✗：**浏览器内核**此前**零处**提到
//     **`navigator`／`gpu`／`backend`** ✓
//     ⇒ **∴ 于是**：「**不许假装**」这条**在浏览器侧**无从校验** ✓
//       ⇒ **★ 所以**：**内核必须**自报**✗
//         ⇒ **∴ 而且**它**必须**把两件事分开**** ✓ ★**** ✓✓
//
// **∴ 判据（四条 ✓）★**：
//   **∴ ①** **内核导出 `backend_report_json`** ✓
//   **∴ ②** **报告里**如实**写 `"render_backend": "cpu"`** ✓
//     （**∴ 因为**内核**尚未**接 WebGPU**✗ ⇒ **∴ 报 `cpu` 才是**如实** ✓）
//   **∴ ③** **`webgpu_in_page` **与** `webgpu_used_by_kernel` **必须**分开**
//     （**∴ 前者**是**浏览器能力**✗；**∴ 后者**是**我们的行为** ✓）
//     ＋ **∴ 后者**恒 `false`**** ✓
//   **∴ ④** **必须**给出 `webgpu_unused_reason`**（**∴ 不许**含糊 ✓）★**** ✓✓
//
// **∴ 变异（**两条 ✓）★**：
//   **∴ 甲**：**把 `render_backend` 改成 `"gpu"`（**假装 ✓）⇒ **∴ ② 必红** ✓
//   **∴ 乙**：**删掉 `webgpu_unused_reason`** ⇒ **∴ ④ 必红** ✓
//
// 用法：node scripts/tool-kernel-backend-report.mjs
import { readFileSync } from "node:fs";

const SRC = "crates/yanshi-wasm/src/lib.rs";
const source = readFileSync(SRC, "utf8");

let failed = 0;
const check = (ok, name, detail) => {
  if (ok) {
    console.log(`  ✓ ${name}（${detail}）`);
  } else {
    console.error(`  ✗ ${name}（${detail}）`);
    failed += 1;
  }
};

const fnIdx = source.indexOf("pub fn backend_report_json(&self) -> String {");
const body = fnIdx === -1 ? "" : source.slice(fnIdx, fnIdx + 1600);

check(fnIdx !== -1, "内核必须导出 `backend_report_json`（**∴ 否则**无从校验「不许假装」 ✓）",
  fnIdx === -1 ? "找不到" : "已找到");

check(/"render_backend"\s*:\s*"cpu"/.test(body),
  "`render_backend` 必须**如实**写 `\"cpu\"`（**∴ 内核尚未接 WebGPU ⇒ 报 cpu 才对** ✓）",
  /"render_backend"\s*:\s*"cpu"/.test(body) ? "是 cpu" : "不是 cpu（**∴ 可能在假装** ✗）");

check(/"webgpu_in_page"/.test(body) && /"webgpu_used_by_kernel"/.test(body),
  "`webgpu_in_page`（**浏览器能力**）与 `webgpu_used_by_kernel`（**我们的行为**）必须**分开**",
  /"webgpu_in_page"/.test(body) && /"webgpu_used_by_kernel"/.test(body) ? "两者都在" : "缺一个");

check(/"webgpu_used_by_kernel"\s*:\s*false/.test(body),
  "`webgpu_used_by_kernel` 必须**恒 `false`**（**∴ 内核现在**就是 CPU** ✓）",
  /"webgpu_used_by_kernel"\s*:\s*false/.test(body) ? "是 false" : "不是 false");

check(/"webgpu_unused_reason"/.test(body),
  "必须给出 `webgpu_unused_reason`（**∴ 不许**含糊其辞 ✓）",
  /"webgpu_unused_reason"/.test(body) ? "已给出" : "缺失");

console.log("");
if (failed === 0) {
  console.log("  结论：✓ 内核如实上报后端（**render_backend ＝ cpu；**能力与行为分开** ✓）");
  process.exit(0);
} else {
  console.error(`  结论：✗ 内核后端上报不达标（${failed} 条）`);
  process.exit(1);
}
