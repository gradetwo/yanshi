// **`speed_gain` 必须真的生效** ✓（P2-10 的判据 ✓）
//
// **∴ 关键设计 ✗**：**∴ 每次 paint 落在**独立文档**✗** ⇒ **∴ 否则**后一笔叠在前一笔上 ✗
//   ⇒ **∴ 图必然不同 ⇒ **∴ 判据恒绿（假绿）✗** ✓
//   （**∴ 教训 ✓**：我在同一文档里画三笔 ✗ ⇒ **∴ 得到 10190／10831／10339 三个不同的数 ✗
//     ⇒ **∴ 一度误以为**PNG 输出不确定**✗ ⇒ **∴ 而实测**同一文档连导五次、五个独立文档各画同笔**✗
//     md5 **全同** ✓ ⇒ **∴ 所以**渲染与编码都**完全确定**✓、错的是**我的实验设计**✗）
//
// **∴ 断言 ✓**：**∴ 空 `brush_params: {}` vs 带 `{"speed_gain": 1.0}` ✗** ⇒ **∴ 字节必须不同 ✓**
//   ⇒ **∴ 为什么用"空对象"当基线 ✗**：**∴ 传 `brush_params` 本身就改图**✗（**∴ 通道效应 ✓**）
//     ⇒ **∴ 用"不带"当基线 ✗ 会把通道效应混进信号 ✓** ⇒ **∴ 所以要扣掉它 ✓**
const fs = await import("node:fs");
const path = await import("node:path");
const os = await import("node:os");
const { trackTemp } = await import("./lib/temp-dirs.mjs");

const base = process.argv[2] ?? "http://127.0.0.1:0";
const root = trackTemp(fs.mkdtempSync(path.join(os.tmpdir(), "speedgain-")));
const stamp = Date.now();

/** 在**独立文档**里画同一笔，导出，返回字节数（**∴ 与其它 paint 互不影响 ✓**）。 */
const paintIn = async (tag, brushParams) => {
  const doc = `sg_${tag}_${stamp}`;
  const created = await fetch(`${base}/api/documents`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: 400, height: 400 }),
  });
  if (!created.ok) return { ok: false, detail: `建文档失败 HTTP ${created.status}` };
  const token = (await created.json())?.token ?? "";
  if (!token) return { ok: false, detail: "没拿到 token" };
  const call = async (name, args) => {
    const reply = await fetch(
      `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`,
      { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args) },
    );
    return reply.json();
  };
  await call("create_layer", { layer_id: "sg_L", name: "sg_L" });
  // **∴ 变速笔触 ✗**：前半密集 ⇒ 慢、后半稀疏 ⇒ 快
  const points = [
    [40, 100, 1], [50, 100, 1], [60, 100, 1], [70, 100, 1], [80, 100, 1], [90, 100, 1], [100, 100, 1],
    [140, 100, 1], [200, 100, 1], [280, 100, 1], [360, 100, 1],
  ];
  const args = { layer_id: "sg_L", brush: "oil-06-paint", color: { r: 200, g: 60, b: 40, a: 255 }, points, size: 24, preview: false };
  if (brushParams) args.brush_params = brushParams;
  const r = await call("brush_stroke", args);
  if (r?.ok !== true) return { ok: false, detail: JSON.stringify(r).slice(0, 150) };
  const out = path.join(root, `${tag}.png`);
  const e = await call("export_png", { path: out });
  if (!fs.existsSync(out)) return { ok: false, detail: `导出没落盘：${JSON.stringify(e).slice(0, 120)}` };
  return { ok: true, bytes: e.bytes ?? fs.statSync(out).size };
};

const plain = await paintIn("plain", null);
const empty = await paintIn("empty", {});
const gain = await paintIn("gain", { speed_gain: 1.0 });

const show = (label, r) => console.log(`  ${label} ⇒ ${r.ok ? `${r.bytes} 字节` : r.detail}`);
show("不带 brush_params", plain);
show("空 brush_params:{}", empty);
show("带 speed_gain:1.0", gain);

let bad = 0;
const check = (ok, why) => {
  console.log(`  ${ok ? "✓" : "✗"} ${why}`);
  if (!ok) bad += 1;
};
check(plain.ok && empty.ok && gain.ok, "三笔都成功且都导出了 PNG（各自独立文档）");
check(
  empty.ok && gain.ok && empty.bytes !== gain.bytes,
  `**扣掉通道效应后**仍必须不同（空对象 ${empty.bytes} vs speed_gain ${gain.bytes}）—— 相同即说明 speed_gain 零效果`,
);

if (bad) {
  console.log(`  ✗ speed_gain 生效性有 ${bad} 处不满足`);
  process.exit(1);
}
console.log("  ✓ speed_gain 真的生效（三个独立文档、同一笔，空对象与 speed_gain 的导图字节不同）");
