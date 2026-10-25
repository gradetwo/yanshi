#!/usr/bin/env node
// **`list_atoms` 的 `total` 必须是过滤后的匹配总数，且上限 5000 要装得下 600 笔**
// （期 2 块 1：D3 提上限 + P2-4 `total` 虚高修）
//
// **⇒ P2-4 的缺陷**：`read_list_atoms` 里"收到 limit+1 条就不再收"的判断放在**全部过滤器之前** ⇒
//   页满之后所有原子都被无过滤计入 `total`：无 offset 时 `limit=20` 很快填满，随后记账原子
//   （建文档／建图层）也被数进 `total`，实测虚高 +3/+4 ✗。
// **⇒ D3**：`limit` 上限 500 ⇒ `limit=2000` 仍只回 500，600 笔文档会被读成"只有 500 笔" ✗。
//
// **⇒ 判据四件事**（600 笔实笔画，分 25 笔一批落下——25 笔是已验证的安全批大小）：
//   ① `limit=2000` ⇒ `count=600`、`total=600`、`truncated=false`、`has_more=false` ✓；
//   ② 小页 `limit=1`（缺省只列笔迹）⇒ `total` 仍是 600（页满之后记账原子不许混进来）✓；
//   ③ `offset` 翻页（每页 200）⇒ 各页条数之和 `== total`，`atom_id` 无重无漏 ✓；
//   ④ 每页回执的 `total` 都等于 600（不是"本页条数"）✓。
//
// **变异**：上限改回 500 ⇒ ① `count=500` 红 ✓；`total` 改成恒等于返回条数 ⇒ ②④ 红 ✓；
//   "不再收"的判断移回过滤器之前 ⇒ ② `total>600` 红 ✓。
// **⇒ 退出码**：0 通过｜1 不符｜**3 不可判**
const [base] = process.argv.slice(2);
if (!base) {
  console.error("用法: node scripts/tool-list-atoms-total-600.mjs <base>");
  process.exit(3);
}
const doc = `atoms_total600_${Date.now()}`;
const N = 600;
const BATCH = 25;
const post = async (tool, args) => {
  const r = await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${TOK}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args ?? {}),
  });
  return r.json();
};
let TOK = "";
{
  const made = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: 400, height: 400 }),
  })).json();
  TOK = made.token;
  if (!TOK) { console.error("  ⊘ 不可判：建不出文档"); process.exit(3); }
}
const fails = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fails.push(m); console.log(`  ✗ ${m}`); };

await post("create_layer", { layer_id: "L", name: "L" });
// 600 笔：位置沿画布网格走，每笔短横线（都落墨，免得 0 墨笔不进账）
for (let start = 0; start < N; start += BATCH) {
  const calls = [];
  for (let k = start; k < Math.min(start + BATCH, N); k += 1) {
    const x = 20 + (k % 20) * 18;
    const y = 20 + Math.floor(k / 20) * 12;
    calls.push({
      tool: "brush_stroke",
      arguments: {
        layer_id: "L", brush: "calligraphy", size: 8,
        color: [0.2 + (k % 5) * 0.1, 0.3, 0.5],
        points: [[x, y, 0.8], [x + 10, y, 0.8]],
        preview: false, wait_for_render: false,
      },
    });
  }
  const rb = await post("batch", { calls, silent: true });
  if (rb?.ok !== true && rb?.all_ok !== true) {
    // batch 顶层 ok 在期 1 之前是"请求跑完了"；看 fail_count 更准
    if ((rb?.fail_count ?? 0) > 0) bad(`batch(${start}..) 有 ${rb.fail_count} 笔失败 ⇒ ${JSON.stringify(rb).slice(0, 200)}`);
  }
}

const all = await post("list_atoms", { limit: 2000, want_thumb: false });
if (all?.ok !== true) { console.error(`  ⊘ 不可判：list_atoms(limit=2000) 失败 ⇒ ${JSON.stringify(all).slice(0, 200)}`); process.exit(3); }
if (all.count === N && all.total === N) ok(`① limit=2000 ⇒ count=${all.count}、total=${all.total}（600 笔一次取完）`);
else bad(`① limit=2000 应返回 600 条且 total=600，实测 count=${all.count}、total=${all.total}（上限没提／total 不对）`);
if (all.truncated === false && all.has_more === false) ok("① truncated=false、has_more=false（装得下就不许报截断）");
else bad(`① 装得下时 truncated/has_more 必须为 false，实测 ${all.truncated}/${all.has_more}`);

const small = await post("list_atoms", { limit: 1, want_thumb: false });
if (small?.ok !== true) { console.error(`  ⊘ 不可判：list_atoms(limit=1) 失败`); process.exit(3); }
if (small.total === N) ok(`② 小页 limit=1 ⇒ total 仍是 ${small.total}（页满之后记账原子没混进来）`);
else bad(`② 小页 total 必须是 ${N}（过滤后的匹配总数），实测 ${small.total} ⇒ P2-4 虚高还在`);
if (small.count === 1) ok("② 小页 count=1");
else bad(`② 小页 count 应为 1，实测 ${small.count}`);

// ③④ 翻页：每页 200，共 3 页
const ids = new Set();
let sum = 0;
let pageTotalsOk = true;
for (let offset = 0; offset < N; offset += 200) {
  const page = await post("list_atoms", { limit: 200, offset, want_thumb: false });
  if (page?.ok !== true) { bad(`③ offset=${offset} 翻页失败`); break; }
  sum += page.count ?? 0;
  if (page.total !== N) pageTotalsOk = false;
  for (const a of page.atoms ?? []) {
    if (ids.has(a.atom_id)) bad(`③ 翻页取到重复原子 ${a.atom_id}`);
    ids.add(a.atom_id);
  }
}
if (sum === N && ids.size === N) ok(`③ 翻页合计 ${sum} 条，无重无漏（== total ${N}）`);
else bad(`③ 翻页合计 ${sum} 条、去重后 ${ids.size}，与 total ${N} 不一致 ⇒ 分页漏了／重了原子`);
if (pageTotalsOk) ok("④ 每页回执的 total 都是 600（不是本页条数）");
else bad("④ 某页回执的 total 不是 600 ⇒ total 被写成了本页条数");

if (fails.length > 0) {
  console.log(`\n  ✗ ${fails.length} 处不符（list_atoms total／上限）`);
  process.exit(1);
}
console.log(`\n  ✓ list_atoms：600 笔一次取完，total 诚实，翻页无重无漏`);
