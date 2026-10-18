// **`brush_stroke` 的 `wait_for_render` 参数** ✓（第 8 轮 ✓）
//
// **∴ 为什么需要它 ✗**：`preview: false` 时**仍会**调 `render_region_complete_jobs` ✗
//   （**∴ 契约：返回时状态已落盘 ✓**）⇒ **∴ 实测占单笔 **66%**✗**（**∴ 500 对象 `preview_ms` 11.0 / `total` 16.8 ✓**）。
//   ⇒ **∴ 连续作画不要图时，这块是纯浪费 ✗** ⇒ **∴ 所以加了 `wait_for_render: false` ✓**。
//
// **∴ 断言（确定性 ✓ 不看时间阈值 ✗）**：
//   ① 缺省（wait=true）⇒ `preview_ms > 0`（**∴ 证明它确实在等 ✓**）
//   ② `wait_for_render: false` ⇒ **`preview_ms == 0`**（**∴ 整块被跳过 ✓ ⇒ **∴ 唯一确定的判据 ✓**）
//   ③ `wait_for_render: true` ⇒ `preview_ms > 0`（**∴ 参数真的生效 ✓**）
//   ④ `total_ms` 必须下降（**∴ 同进程内比 ✓，跨机不比 ✓**）
const base = process.argv[2] ?? "http://127.0.0.1:0";
const doc = process.argv[3] ?? `wait_${Date.now()}`;
const token = process.argv[4] ?? "";

const url = (name) => `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
const call = async (name, args) => {
  const reply = await fetch(url(name), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(args),
  });
  const body = await reply.json();
  return { ok: reply.ok, body };
};

const created = await call("create_layer", { layer_id: "wr_L", name: "wr_L" });
if (created.body?.ok !== true) {
  console.log(`  ✗ 建层失败：${JSON.stringify(created.body).slice(0, 160)}`);
  process.exit(1);
}

const stroke = (index, wait) => {
  const args = {
    layer_id: "wr_L",
    brush: "100%_Opaque",
    color: { r: 80, g: 60, b: 40, a: 255 },
    points: [],
    size: 16,
    preview: false,
  };
  args.points = [
    [10 + (index * 7) % 580, 20 + (index * 13) % 1040, 1],
    [60 + (index * 7) % 540, 20 + (index * 13) % 1040, 1],
  ];
  if (wait !== undefined) args.wait_for_render = wait;
  return call("brush_stroke", args);
};

// 铺 120 笔，让"等渲染"这块有非零成本可测
for (let i = 1; i <= 120; i += 1) await stroke(i);

const timingsOf = (reply) => reply.body?.timings ?? {};
const defaulted = timingsOf(await stroke(500));
const skipped = timingsOf(await stroke(501, false));
const forced = timingsOf(await stroke(502, true));

let bad = 0;
const check = (ok, why) => {
  console.log(`  ${ok ? "✓" : "✗"} ${why}`);
  if (!ok) bad += 1;
};

const dPreview = defaulted.preview_ms;
const sPreview = skipped.preview_ms;
const fPreview = forced.preview_ms;

check(dPreview > 0, `缺省（wait 不给）时 preview_ms > 0 ⇒ 它确实在等渲染（${dPreview}）`);
check(sPreview === 0, `wait_for_render=false ⇒ preview_ms 必须精确为 0（${sPreview}）`);
check(fPreview > 0, `wait_for_render=true ⇒ preview_ms > 0 ⇒ 参数真的生效（${fPreview}）`);
check(
  skipped.total_ms < defaulted.total_ms,
  `wait=false 的 total_ms 必须小于缺省（同进程内比：${skipped.total_ms} < ${defaulted.total_ms}）`,
);

if (bad) {
  console.log(`  ✗ wait_for_render 有 ${bad} 处不满足`);
  process.exit(1);
}
console.log("  ✓ wait_for_render：false 时跳过等渲染（preview_ms 精确为 0），true/缺省时仍等");
