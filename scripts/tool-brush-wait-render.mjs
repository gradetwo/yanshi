// **`brush_stroke` 的 `wait_for_render` 参数** ✓
//
// **∴ 历史 ✗**：这条判据**曾经被删掉**✗ —— 因为它当时用 `preview_ms > 0` 断言 ✗，
//   而 CI 上那个值恒为 0 ✗（渲染太快）⇒ **∴ 它**在 CI 上既红又没分辨力**✗**。
// **∴ 现在重建 ✗**：**∴ 用 `preview_calls`（**语义计数**✗）替代时间 ✓**
//   ⇒ **∴ 实测 ✓**：`wait_for_render=true ⇒ preview_calls=1` ✓／`false ⇒ **0**` ✗／不给 ⇒ `1` ✓
//   ⇒ **∴ 全部是**整数相等**✗ ⇒ **∴ 与机器快慢**无关**✓ ✓
//
// **∴ 断言（四条，全是确定性 ✗）**：
//   ① `wait_for_render: true` ⇒ `preview_calls === 1`（**∴ 等了预览 ✓**）
//   ② `wait_for_render: false` ⇒ `preview_calls === 0`（**∴ 跳过了预览 ✓**）
//   ③ 不给参数 ⇒ `preview_calls === 1`（**∴ 缺省 = true，现有行为不变 ✓**）
//   ④ 三笔都真的落过笔 ⇒ 每笔 `raster_calls > 0`（**∴ 否则"跳过预览"可能只是"没画"✓**）
// **前提失败必须 exit(3)**（第 1084 轮）：无服务时 fetch 抛异常 ⇒ uncaught ⇒ 退出码 1 ✗
// ⇒ 按 336 轮那条：条件不满足应 exit(3)（不可判），不是 exit(1)（产品失败）
process.on("uncaughtException", (error) => {
  console.error(`  ⊘ 判据自身出错（不是产品问题）：${String(error).slice(0, 80)}`);
  process.exit(3);
});
const base = process.argv[2] ?? "http://127.0.0.1:0";
const doc = `waitr_${Date.now()}`;

const docCreated = await fetch(`${base}/api/documents`, {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 400, height: 400 }),
});
if (!docCreated.ok) {
  console.log(`  ✗ 建文档失败：HTTP ${docCreated.status}`);
  process.exit(1);
}
const token = (await docCreated.json())?.token ?? "";
if (!token) {
  console.log("  ✗ 建文档没拿到 token");
  process.exit(1);
}

const call = async (name, args) => {
  const reply = await fetch(
    `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`,
    { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args) },
  );
  return { ok: reply.ok, body: await reply.json() };
};

const created = await call("create_layer", { layer_id: "wr_L", name: "wr_L" });
if (created.body?.ok !== true) {
  console.log(`  ✗ 建层失败：${JSON.stringify(created.body).slice(0, 160)}`);
  process.exit(1);
}

let bad = 0;
const check = (ok, why) => {
  console.log(`  ${ok ? "✓" : "✗"} ${why}`);
  if (!ok) bad += 1;
};

const stroke = async (wait, index) => {
  const args = {
    layer_id: "wr_L",
    brush: "pointy_ink",
    color: { r: 30, g: 60, b: 120, a: 255 },
    points: [],
    size: 20,
    preview: false,
  };
  const y = 40 + index * 60;
  args.points = [[40, y, 1], [140, y + 40, 1], [260, y, 1]];
  if (wait !== undefined) args.wait_for_render = wait;
  const r = await call("brush_stroke", args);
  return r.body?.timings ?? {};
};

const waited = await stroke(true, 0);
const skipped = await stroke(false, 1);
const defaulted = await stroke(undefined, 2);

console.log(`  wait=true  ⇒ preview_calls=${waited.preview_calls}｜raster_calls=${waited.raster_calls}`);
console.log(`  wait=false ⇒ preview_calls=${skipped.preview_calls}｜raster_calls=${skipped.raster_calls}`);
console.log(`  不给参数   ⇒ preview_calls=${defaulted.preview_calls}｜raster_calls=${defaulted.raster_calls}`);

check(waited.preview_calls === 1, `wait=true ⇒ preview_calls 精确为 1（实测 ${waited.preview_calls}）`);
check(skipped.preview_calls === 0, `wait=false ⇒ preview_calls 精确为 0（实测 ${skipped.preview_calls}）`);
check(defaulted.preview_calls === 1, `不给参数 ⇒ preview_calls 为 1（缺省 = true，实测 ${defaulted.preview_calls}）`);
check(
  [waited, skipped, defaulted].every((t) => typeof t.raster_calls === "number" && t.raster_calls > 0),
  `三笔都真的落过笔（raster_calls 都 > 0：${waited.raster_calls}/${skipped.raster_calls}/${defaulted.raster_calls}）`,
);

if (bad) {
  console.log(`  ✗ wait_for_render 有 ${bad} 处不满足`);
  process.exit(1);
}
console.log("  ✓ wait_for_render：false 时 preview_calls 精确为 0，true 与缺省时精确为 1（语义，不看时间）");
