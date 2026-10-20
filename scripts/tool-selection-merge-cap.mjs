#!/usr/bin/env node
// **选区布尔合并必须有分配上界** ✓（第三轮复审 **N3D-01**，P1）
//
// **∴ 问题**：`merge_selection_shapes` 的 `w`/`h` 是**两个选区包围盒的并**，
//   **不裁画布、无上界** ⇒ 构造两个相距遥远的选区即可让 `w*h` 达天文数字
//   ⇒ `vec![0u8; w*h]` 直接内存 abort、或持锁 O(w·h) 循环挂死服务端 ✗
//
// **∴ 这条判据守什么**：
//   ① **正向**：两个**靠近**的选区合并 ⇒ 必须**成功**（防误拒 —— 上限不能把正常用法挡掉）
//   ② **反向**：两个**远隔**的选区合并 ⇒ 必须**明确报错**（说「区域过大」），
//      **不是** abort、**不是**静默返回 ok:true（静默会让调用方以为「合并没有变化」✗）
//
// **∴ 变异**：把 `check_merge_pixels` 的 `>` 改成恒假（去掉上限）⇒ ② 必红 ✓
//   （① 仍绿 —— 这正是「一条判据两条断言要走不同路径」的例子）
const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-selection-merge-cap.mjs <base> <doc> <token>");
  process.exit(2);
}

const call = async (name, args) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
  const reply = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(args),
  });
  return await reply.json();
};

const fails = [];
const check = (ok, msg) => { if (!ok) fails.push(msg); console.log(`  ${ok ? "✓" : "✗"} ${msg}`); };

// ① 正向：两个靠近的选区（同一 id，mode=add）⇒ 必须成功
const near1 = await call("create_selection", { selection_id: "cap_near", shape: { kind: "rect", x: 10, y: 10, w: 20, h: 20 } });
const near2 = await call("create_selection", { selection_id: "cap_near", mode: "add", shape: { kind: "rect", x: 40, y: 40, w: 20, h: 20 } });
check(near1.ok === true, `①a 首个选区建立 ⇒ ok=${near1.ok}`);
check(near2.ok === true, `①b 靠近的两个选区能合并（防误拒）⇒ ok=${near2.ok}｜${JSON.stringify(near2.context?.detail ?? "").slice(0, 80)}`);

// ② 反向：两个远隔的选区 ⇒ 必须明确报「区域过大」
const far1 = await call("create_selection", { selection_id: "cap_far", shape: { kind: "rect", x: 10, y: 10, w: 20, h: 20 } });
check(far1.ok === true, `②a 远隔组首个选区建立 ⇒ ok=${far1.ok}`);
const far2 = await call("create_selection", { selection_id: "cap_far", mode: "add", shape: { kind: "rect", x: 200000, y: 200000, w: 20, h: 20 } });
const detail = String(far2.context?.detail ?? "");
check(far2.ok === false, `②b 远隔的两个选区必须**拒绝**（不是 abort、不是静默）⇒ ok=${far2.ok}`);
check(/区域过大/.test(detail), `②c 错误信息必须说「区域过大」（可诊断）⇒ ${JSON.stringify(detail).slice(0, 100)}`);

if (fails.length) {
  console.log(`\n  ✗ ${fails.length} 处不符（选区合并的分配上界）`);
  process.exit(1);
}
console.log("\n  ✓ 选区布尔合并有分配上界，且不误拒正常用法");
