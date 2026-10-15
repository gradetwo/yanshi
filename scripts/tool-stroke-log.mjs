#!/usr/bin/env node
// **★ `get_stroke_log()`：一笔一行的可审计记录（Muse 建议 3，P1）★**
//
// **它补的正是那个精确缺口** ✓：`find_atom` **按设计只返回元数据** ✓（实测字段：
// `actor/atom_id/changeset_id/heavy/kind/layer_id/object_id/seq/session/state_effect/timestamp` ✗ ——
// **没有坐标** ✓）；坐标得靠 `get_atom` **一条一条**取 ✗ ⇒ 审计"我到底画在哪了"很费劲 ✓。
//
// **本判据要证的五件事**：
//   ① 工具存在，且**只列笔触**（不把 create_layer 之类也倒出来 ✓）；
//   ② 每一行都带**坐标信息**：`points`（原样 ✓）＋ `bbox`（= 原子的 `region` ✓）＋ 首尾点 ✓；
//   ③ **压感**能报出来（min/max/首/尾 ✓）—— 这是"笔触太硬"那类问题的审计入口 ✓；
//   ④ **分页**（`limit`/`offset`/`total`/`next_offset` ✓）与 `layer_id` 过滤 ✓；
//   ⑤ **跟文档走** ✓：它不是 `/tmp` 里的旁路文件 ✓ —— 用**另一份文档**验证隔离 ✓
//      （A 文档的笔触不出现在 B 文档的日志里 ✓）。
//
// **已知限制（如实写在判据里 ✗ → ✓）**：笔触原子**不保存逐点 `time_ms`** ✓（实测：
// 传 `[[x,y,p,0],[x,y,p,120]]` 后，原子里的 `points` 只有 3 个分量 ✗）⇒ 日志只能给**原子时间戳** ✓；
// 逐点时间只在**栅格化**时起过作用 ✓。所以下面**不**断言逐点时间 ✗。
//
// 用法：node scripts/tool-stroke-log.mjs <base-url>
const base = (process.argv[2] || "").replace(/\/+$/, "");
if (!base) {
  console.error("用法: node scripts/tool-stroke-log.mjs <base-url>");
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
  const doc = `slog_${tag}_${Date.now().toString(36)}`;
  const created = await (
    await fetch(`${base}/api/documents`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ doc_id: doc, width: 320, height: 240 }),
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
  const [doc, token] = await newDoc("a");
  await call(doc, token, "create_layer", { layer_id: "L1", name: "L1" });
  await call(doc, token, "create_layer", { layer_id: "L2", name: "L2" });

  const stroke = (objectId, layer, points) =>
    call(doc, token, "brush_stroke", {
      layer_id: layer,
      object_id: objectId,
      brush: "2B_pencil.myb",
      size: 30,
      color: { r: 10, g: 20, b: 30, a: 255 },
      points,
    });
  const first = await stroke("s1", "L1", [
    [40, 50, 0.3],
    [90, 60, 0.9],
    [150, 70, 0.4],
  ]);
  const second = await stroke("s2", "L2", [
    [60, 140, 0.8],
    [200, 150, 0.2],
  ]);
  console.log(`  前置：两笔都成功=${first.ok === true && second.ok === true}`);

  const log = await call(doc, token, "get_stroke_log", {});
  if (!log || log.ok !== true) {
    console.error(
      `❌ ① 工具不存在或失败：${JSON.stringify(log).slice(0, 200)}\n` +
        "   ⇒ 这是产品缺口（Muse 建议 3 尚未实现），退出码 1",
    );
    process.exit(1);
  }
  const entries = log.entries || [];
  console.log(`  ① 条目数=${entries.length}｜total=${log.total}｜返回 ${log.returned}`);
  if (entries.length !== 2) {
    bad.push(`① 两笔笔触应当正好两条记录，实际 ${entries.length}（不许把建图层之类也列出来 ✗）`);
  }
  const kinds = [...new Set(entries.map((e) => e.kind))];
  console.log(`  ① 记录种类=${JSON.stringify(kinds)}｜笔刷=${JSON.stringify(entries.map((e) => e.brush))}`);
  for (const e of entries) {
    if (e.kind !== "brush") bad.push(`① 记录里混进了非笔触的 kind=${e.kind} ✗`);
    if (!e.brush) bad.push("① 每条记录都要报出用了哪支笔刷 ✓");
  }

  // ② 坐标信息
  const one = entries.find((e) => e.object_id === "s1");
  if (!one) {
    bad.push("② 找不回来 object_id=s1 的那一笔 ✗");
  } else {
    console.log(
      `  ② s1：points=${JSON.stringify(one.points)}｜bbox=${JSON.stringify(one.bbox)}｜` +
        `首=${JSON.stringify(one.first_point)} 尾=${JSON.stringify(one.last_point)}`,
    );
    if (!Array.isArray(one.points) || one.points.length !== 3) {
      bad.push(`② s1 的 points 应当原样有 3 个点，实际 ${JSON.stringify(one.points)}`);
    }
    const bbox = one.bbox || {};
    if (typeof bbox.x !== "number" || typeof bbox.w !== "number") {
      bad.push("② 每条记录都要带 `bbox`（原子里的 `region` ✓）");
    } else {
      // 三个点的 x 在 40..150，size 30 ⇒ bbox 应当把它包住（且比它大 ✓）
      if (!(bbox.x <= 40 && bbox.x + bbox.w >= 150)) {
        bad.push(`② bbox 没把笔触包住：bbox=${JSON.stringify(bbox)}（点 x 在 40..150）`);
      }
    }
    if (!one.first_point || one.first_point.x !== 40 || one.first_point.y !== 50) {
      bad.push(`② first_point 不对：${JSON.stringify(one.first_point)}`);
    }
    if (!one.last_point || one.last_point.x !== 150) {
      bad.push(`② last_point 不对：${JSON.stringify(one.last_point)}`);
    }
  }

  // ③ 压感可审计
  if (!one || !one.pressure) {
    bad.push("③ 每条记录都要带 pressure（min/max/first/last ✓）");
  } else {
    console.log(
      `  ③ 压感 min=${one.pressure.min} max=${one.pressure.max} ` +
        `first=${one.pressure.first} last=${one.pressure.last}`,
    );
    if (one.pressure.min !== 0.3 || one.pressure.max !== 0.9) {
      bad.push(`③ s1 的压感范围应当是 0.3..0.9，实际 ${JSON.stringify(one.pressure)}`);
    }
  }
  if (!one || typeof one.timestamp !== "number") {
    bad.push("③ 每条记录都要带原子时间戳 ✓");
  }

  // ④ 分页与过滤
  const page = await call(doc, token, "get_stroke_log", { limit: 1 });
  console.log(
    `  ④ limit=1 ⇒ 返回 ${page.returned}｜total=${page.total}｜next_offset=${page.next_offset}`,
  );
  if (page.returned !== 1) bad.push(`④ limit=1 应当只返回 1 条，实际 ${page.returned}`);
  if (page.total !== 2) bad.push(`④ total 应当是 2，实际 ${page.total}`);
  if (page.next_offset !== 1) bad.push(`④ next_offset 应当是 1，实际 ${page.next_offset}`);
  const page2 = await call(doc, token, "get_stroke_log", { limit: 1, offset: 1 });
  if ((page2.entries || [])[0]?.seq === (page.entries || [])[0]?.seq) {
    bad.push("④ offset=1 应当翻到下一笔（两页不该是同一条）✗");
  }
  const onlyL2 = await call(doc, token, "get_stroke_log", { layer_id: "L2" });
  console.log(`  ④ layer_id=L2 ⇒ ${onlyL2.returned} 条`);
  if (onlyL2.returned !== 1 || (onlyL2.entries || [])[0]?.layer_id !== "L2") {
    bad.push("④ layer_id 过滤不对 ✗");
  }

  // ⑤ 跟文档走：另一份文档看不到
  const [docB, tokenB] = await newDoc("b");
  const other = await call(docB, tokenB, "get_stroke_log", {});
  console.log(`  ⑤ 另一份文档 ⇒ ${other.returned} 条`);
  if (other.returned !== 0) bad.push(`⑤ 日志泄漏到别的文档了（应 0，实际 ${other.returned}）✗`);

  if (bad.length) {
    console.error(`❌ ${bad.length} 项没成立：${bad.join("｜")}`);
    process.exit(1);
  }
  console.log(
    "✓ 笔触日志成立：只列笔触 ✓、带坐标与 bbox ✓、压感可审计 ✓、分页与过滤 ✓、随文档不泄漏 ✓",
  );
};

await main();
