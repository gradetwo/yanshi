#!/usr/bin/env node
// **`list_atoms` 必须能翻页，且满页时要说"可能还有"** ✓（第 636 轮；外部 `report.md` 的 D7）
//
// **⇒ 症状（他们的实测）**：`limit` 上限 500、倒序、**没有 `offset`**、回执**没有 `total`/`truncated`**
//   ⇒ 一幅 39,354 笔的画被"证明"成 500 笔 ✗（`provenance.py` 真被骗过 ✓）。
//
// **⇒ 判据三件事**：
//   ① 满页（`count == limit`）⇒ `truncated` 与 `has_more` 都必须是 `true` ✓；
//   ② `offset` 真的翻页：用 `offset = 0,step,2*step…` 逐页取 `atom_id`，
//      **不许有重复**、且**取到的条数必须等于一次取大 limit 拿到的条数** ✓；
//   ③ 最后一页不满 ⇒ `truncated` 必须是 `false` ✓。
//
// **变异**：让 `read_list_atoms` 忽略 `offset`（`let offset = 0`）⇒ ② 出现重复 ⇒ 红 ✓。
const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-list-atoms-pagination.mjs <base> <doc> <token>");
  process.exit(2);
}
const call = async (name, args) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
  const response = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(args ?? {}),
  });
  return response.json();
};
const fail = (message) => {
  console.error(`  ✗ ${message}`);
  process.exit(1);
};

// 造 6 个原子（图层足够；`list_atoms` 默认只列笔迹 ⇒ 加 `include_non_stroke`）
const layer = await call("create_layer", { layer_id: "layer_default", name: "l" });
const layerDetail = String(layer.context?.detail ?? "");
if (layer.ok !== true && !layerDetail.includes("已存在")) fail(`create_layer ⇒ ${JSON.stringify(layer).slice(0, 120)}`);
for (let i = 0; i < 6; i += 1) {
  const made = await call("create_layer", { layer_id: `atom_page_${i}`, name: `p${i}` });
  if (made.ok !== true) fail(`create_layer(${i}) ⇒ ${JSON.stringify(made).slice(0, 120)}`);
}

const STEP = 2;
const page = async (offset) => {
  const answer = await call("list_atoms", { limit: STEP, offset, include_non_stroke: true });
  if (answer.ok !== true) fail(`list_atoms(offset=${offset}) ⇒ ${JSON.stringify(answer).slice(0, 160)}`);
  return answer;
};

const all = await call("list_atoms", { limit: 500, offset: 0, include_non_stroke: true });
if (all.ok !== true) fail(`list_atoms(大 limit) ⇒ ${JSON.stringify(all).slice(0, 160)}`);
const totalSeen = (all.atoms ?? []).length;
if (totalSeen < 4) {
  console.log(`  ⊘ 不可判：只看到 ${totalSeen} 条（判据自己造的原子没到 4 条）`);
  process.exit(3);
}

const first = await page(0);
if (first.count !== STEP) fail(`第一页应有 ${STEP} 条，实测 ${first.count}`);
if (first.truncated !== true || first.has_more !== true) {
  fail(`满页时 truncated/has_more 必须为 true（实测 ${first.truncated}/${first.has_more}）⇒ 调用方会以为已经拿全`);
}
if (first.offset !== 0) fail(`回执必须回显 offset（实测 ${first.offset}）`);

// 逐页翻完（用 totalSeen 作上限，防止死循环）
const ids = [];
let lastTruncated = true;
for (let offset = 0; offset < totalSeen && lastTruncated; offset += STEP) {
  const answer = await page(offset);
  if (answer.offset !== offset) fail(`回执的 offset 没回显（请求 ${offset}，回 ${answer.offset}）`);
  for (const atom of answer.atoms ?? []) ids.push(atom.atom_id);
  lastTruncated = answer.truncated === true;
  if ((answer.count ?? 0) < STEP) {
    if (answer.truncated !== false) fail(`不满页时 truncated 必须是 false（实测 ${answer.truncated}）`);
    break;
  }
}
const unique = new Set(ids);
if (unique.size !== ids.length) {
  fail(`翻页取到重复原子（共 ${ids.length} 条、去重后 ${unique.size}）⇒ offset 没生效`);
}
if (ids.length !== totalSeen) {
  fail(`翻页合计 ${ids.length} 条，与一次取大 limit 的 ${totalSeen} 条不一致 ⇒ 分页漏了原子`);
}
console.log(`✓ list_atoms 分页：${ids.length} 条（每页 ${STEP}），无重复；满页 truncated=true、末页 false`);
