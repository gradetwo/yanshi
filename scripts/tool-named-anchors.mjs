#!/usr/bin/env node
// **★ 命名锚点（真机报告 2026-10-15：解决跨会话"错位"）★**
//
// **与 `tool-anchors.mjs` 的区别** ✗：那一条测的是 **P1 ⑫ 视觉锚点**（`brush_stroke{anchor_mode:"data"}`
// 在缩略图旁边回结构化坐标 ✓，每次落笔一次性 ✓）；**这一条**测的是**文档级、命名、持久**的锚点 ✓
// （Muse 的方案 ✓：起稿时把关键点定死 ✓，之后所有会话引用同一组坐标 ✓）。**两个都要在** ✓。
//
// **要证的四件事**：
//   ① `set_anchor` 之后 `get_anchors` 能读回**同一坐标** ✓（含可选 `note` ✓）；
//   ② 同名再设 = **覆盖** ✓（不是新增一条 ✗）；
//   ③ **不跨文档泄漏** ✗（A 文档的锚点不该出现在 B 文档 ✓）；
//   ④ **删不存在的锚点必须报错** ✓ —— 这一条是**实测抓出来的 bug** ✓：
//      折叠是**容错**的 ✓（原子 `precondition` 失败只产生 `FoldWarning` ✗，工具调用照样 `ok=true` ✗）
//      => 第一版只靠 `fold::precondition` 里的检查 ⇒ "删不存在的锚点"返回 **`ok=true`** ✗✗
//      ⇒ 改成在**工具层先查**（且错误里**列出已有锚点** ✓）。
//
// **没在这里证、但人工端到端证过的一条** ✓：**重启服务后坐标不变** ✓
//   （实测：设 left_eye/right_eye/nose_tip ⇒ 杀进程重启 ⇒ `get_anchors` 返回
//    `{"left_eye":{"x":268.0,"y":286.0},"nose_tip":{"x":333.0,"y":352.0},"right_eye":{"x":402.0,"y":281.0}}` ✓）——
//   判据运行器只有**一个**服务进程 ✓，重启不在它能力范围内 ✗ ⇒ 如实写在这里 ✓，不冒充 ✓。
//
// 用法：node scripts/tool-named-anchors.mjs <base-url>
const base = (process.argv[2] || "").replace(/\/+$/, "");
if (!base) {
  console.error("用法: node scripts/tool-named-anchors.mjs <base-url>");
  process.exit(2);
}

const call = async (doc, token, tool, args) =>
  (
    await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(args || {}),
    })
  ).json();

const newDoc = async (tag) => {
  const doc = `nanch_${tag}_${Date.now().toString(36)}`;
  const created = await (
    await fetch(`${base}/api/documents`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ doc_id: doc, width: 800, height: 600 }),
    })
  ).json();
  if (!created || !created.token) {
    console.error("❌ 判不了：建不出文档 ⇒ 退出码 3");
    process.exit(3);
  }
  return [doc, created.token];
};

const main = async () => {
  const bad = [];
  const [docA, tokenA] = await newDoc("a");
  const [docB, tokenB] = await newDoc("b");

  // ① 写三个（其中两个带 note ✓）
  const wanted = [
    ["left_eye", 268.0, 286.0, "外眼角偏 2px"],
    ["right_eye", 402.0, 281.0, null],
    ["nose_tip", 333.0, 352.0, null],
  ];
  for (const [name, x, y, note] of wanted) {
    const args = note === null ? { name, x, y } : { name, x, y, note };
    const r = await call(docA, tokenA, "set_anchor", args);
    if (r.ok !== true) {
      bad.push(`① set_anchor(${name}) 失败：${JSON.stringify(r).slice(0, 140)}`);
    }
  }
  const got = await call(docA, tokenA, "get_anchors", {});
  const anchors = got.anchors || {};
  console.log(`  ① get_anchors ⇒ count=${got.count}｜${JSON.stringify(anchors).slice(0, 130)}`);
  if (got.count !== 3) bad.push(`① 应当有 3 个锚点，实际 ${got.count}`);
  for (const [name, x, y, note] of wanted) {
    const hit = anchors[name];
    if (!hit) {
      bad.push(`① 读不回锚点 ${name}`);
      continue;
    }
    if (hit.x !== x || hit.y !== y) {
      bad.push(`① ${name} 坐标漂了：给了 (${x}, ${y})，读回 (${hit.x}, ${hit.y})`);
    }
    if (note !== null && hit.note !== note) {
      bad.push(`① ${name} 的 note 没保住：${JSON.stringify(hit.note)}`);
    }
  }

  // ② 同名覆盖
  const over = await call(docA, tokenA, "set_anchor", { name: "left_eye", x: 270.5, y: 288.5 });
  const after = await call(docA, tokenA, "get_anchors", {});
  const left = (after.anchors || {}).left_eye || {};
  console.log(`  ② 同名覆盖后 count=${after.count}｜left_eye=${JSON.stringify(left)}`);
  if (over.ok !== true) bad.push("② 同名覆盖应当成功（是覆盖，不是冲突 ✗）");
  if (after.count !== 3) bad.push(`② 覆盖不该改变数量（应仍为 3，实际 ${after.count}）`);
  if (left.x !== 270.5 || left.y !== 288.5) bad.push("② 覆盖没生效（读回的还是旧坐标 ✗）");

  // ③ 不跨文档
  const other = await call(docB, tokenB, "get_anchors", {});
  console.log(`  ③ 另一份文档 count=${other.count}`);
  if (other.count !== 0) bad.push(`③ 锚点泄漏到别的文档了（应 0，实际 ${other.count}）✗`);

  // ④ 删不存在的必须报错（实测抓出来的 bug ✓）
  const gone = await call(docA, tokenA, "delete_anchor", { name: "left_eye" });
  const missing = await call(docA, tokenA, "delete_anchor", { name: "left_eye" });
  console.log(
    `  ④ 删存在的 ok=${gone.ok}｜再删 ok=${missing.ok}（应 false ✓）｜error_code=${missing.error_code}`,
  );
  if (gone.ok !== true) bad.push("④ 删存在的锚点应当成功");
  if (missing.ok !== false) {
    bad.push("④ **删不存在的锚点没有报错** ⇒ 折叠是容错的，工具层必须先查 ✗（实测 bug ✓）");
  } else if (!missing.error_code) {
    bad.push("④ 报错必须带 error_code ✓");
  }
  const tail = await call(docA, tokenA, "get_anchors", {});
  if (tail.count !== 2) bad.push(`④ 删掉一个之后应当剩 2 个，实际 ${tail.count}`);

  if (bad.length) {
    console.error(`❌ ${bad.length} 项没成立：${bad.join("｜")}`);
    process.exit(1);
  }
  console.log(
    "✓ 命名锚点成立：写入后可读回（含 note）✓、同名覆盖 ✓、不跨文档泄漏 ✓、删不存在的会报错 ✓",
  );
  console.log("  （重启后坐标不变由人工端到端证过 ✓，判据运行器无法重启服务 ⇒ 不在此冒充 ✗）");
};

await main();
