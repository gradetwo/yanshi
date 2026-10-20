#!/usr/bin/env node
// **画布尺寸必须有上限，超了就拒绝**（第 1418 轮；外部 D-12）
//
// **⇒ 原缺陷** ✗：`POST /api/documents` 接受 **100000×100000**（= **10^10 像素**）⇒
//   实测**真的建成**了（惰性分配 ⇒ 当场不炸）⇒ 但**任何一次全幅渲染/导出**都要 ≈ **40 GB**
//   ⇒ 那不是"慢"，是**一次必然的 OOM** ✗
//   **⇒ 而 `new_document` **工具**那条路早有 `clamp(1, 65536)` ✓ ⇒ **同一个参数两条路行为不同** ✗
//
// **⇒ 断什么**：
//   ① `POST /api/documents` 传 100000² ⇒ **ok=false**，且 detail 提到「上限」
//   ② 4000² ⇒ **ok=true**（正常画布不许被误拒）
//   ③ 单边超限（20000×200）⇒ **ok=false**
//
// **⇒ 退出码**：0 通过｜1 不符｜**3 不可判**
const [base] = process.argv.slice(2);
if (!base) {
  console.error("用法: node scripts/check-canvas-size-cap.mjs <base>");
  process.exit(3);
}
const stamp = Date.now();
const fails = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fails.push(m); console.log(`  ✗ ${m}`); };

const create = async (doc, width, height) => {
  const r = await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width, height }),
  });
  return r.json();
};

// ① 100000² 必须被拒
const huge = await create(`cap_huge_${stamp}`, 100000, 100000);
const detail = String((huge.context || {}).detail || huge.detail || "");
if (huge.ok === false && /上限/.test(detail)) {
  ok(`① 100000² 被拒：${detail.slice(0, 60)}…`);
} else {
  bad(`① 100000²（10^10 像素）必须被拒并说明上限 ⇒ 实测 ok=${huge.ok}｜detail=${detail.slice(0, 60)}`);
}

// ② 正常画布不许被误拒
const normal = await create(`cap_ok_${stamp}`, 4000, 4000);
if (normal.ok === true || normal.doc_id) {
  ok("② 4000² 正常创建（没被误拒）");
} else {
  bad(`② 4000² 必须能创建 ⇒ 实测 ${JSON.stringify(normal).slice(0, 120)}`);
}

// ③ 单边超限
const wide = await create(`cap_wide_${stamp}`, 20000, 200);
const wd = String((wide.context || {}).detail || wide.detail || "");
if (wide.ok === false && /上限/.test(wd)) ok(`③ 20000×200 被拒（单边）：${wd.slice(0, 50)}…`);
else bad(`③ 单边 20000 必须被拒 ⇒ 实测 ok=${wide.ok}｜detail=${wd.slice(0, 60)}`);

if (fails.length > 0) {
  console.log(`\n  ✗ ${fails.length} 处不符（画布必须有上限，超了要拒绝）`);
  process.exit(1);
}
console.log("\n  ✓ 画布上限生效，且正常尺寸不受影响");
