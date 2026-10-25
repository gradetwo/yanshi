#!/usr/bin/env node
// **C3：`set_reference` 落点明示**（方案 ①，只写明，不改落点）：
//   参考图只记进偏好、不写进文档，换进程即失效 —— 同一事实三处同步：
//   registry summary、实现注释、响应 `note` 都写「换进程需重设」。
//
// **判据**：
//   ① 调 `set_reference` ⇒ `ok:true` + `note` 含「换进程需重设」；
//   ② `GET /api/tools` 清单里 `set_reference` 的 `description`（即 registry summary）
//      含「换进程需重设」；
//   ③ 设参考图前后文档渲染逐字节不变（落点仍是偏好、文档未被改动；
//      与 `tool-reference-overlay.mjs` 的 ② 同一事实的另一臂）。
//
// **变异**：回执 `note` 里的提示被删 ⇒ ① 红；summary 里的提示被删 ⇒ ② 红。
const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-set-reference-note.mjs <base-url>"); process.exit(2); }

const failures = [];
const check = (ok, msg) => {
  if (!ok) failures.push(msg);
  console.log(`  ${ok ? "✓" : "✗"} ${msg}`);
};

const doc = `refnote-${process.pid}`;
let created;
try {
  created = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: 200, height: 150 }),
  })).json();
} catch (error) {
  console.error(`  ⊘ 不可判：连不上服务（判据前提不成立，不是产品问题）：${String(error).slice(0, 80)}`);
  process.exit(3);
}
if (!created?.token) { console.error("  ⊘ 不可判：建文档失败（判据前提不成立）"); process.exit(3); }
const token = created.token;
const call = (tool, args) => fetch(`${base}/api/tools?doc=${doc}&token=${token}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ tool, arguments: args }),
}).then((r) => r.json());

await call("create_layer", { layer_id: "layer_default", name: "l" });
const painted = await call("brush_stroke", {
  layer_id: "layer_default", brush: "classic-brush",
  points: [[40, 60, 0.5], [160, 60, 0.5]], size: 16, color: { r: 40, g: 80, b: 160, a: 255 },
});
if (painted.ok !== true) { console.log("  ✗ 先画一笔都失败 ⇒ 判据无效"); process.exit(1); }
const shot = async () => {
  const raw = await call("render_region", { region: { x: 0, y: 0, w: 200, h: 150 }, raw: true, max_px: 4_000_000 });
  if (!raw.ok || !raw.raw_url) return null;
  return Buffer.from(await (await fetch(base + raw.raw_url)).arrayBuffer());
};
const before = await shot();
if (!before) { console.log("  ⊘ 不可判：拿不到直通 RGBA（判据前提不成立）"); process.exit(3); }

// **一张最小的参考图**：先真的放进存储（`set_reference` 拒绝不存在的哈希，见 tool-reference-overlay.mjs）
const tiny = Buffer.alloc(16, 200).toString("base64");
const put = await call("put_blob", { data_base64: tiny, mime_type: "image/x-yanshi-raw" });
if (put?.ok !== true) { console.log("  ⊘ 不可判：造不出参考图 blob（判据前提不成立）"); process.exit(3); }

// ① 回执 note 含落点提示
const set = await call("set_reference", {
  blob_hash: put.blob_hash, opacity: 0.5, position: { x: 10, y: 10, w: 180, h: 130 },
});
if (set.ok !== true) { console.log(`  ✗ set_reference ⇒ ${JSON.stringify(set).slice(0, 170)}`); process.exit(1); }
check(
  typeof set.note === "string" && set.note.includes("换进程需重设"),
  `① set_reference 回执 note 含「换进程需重设」（实测 ${JSON.stringify(set.note)?.slice(0, 90)}）`,
);

// ② 清单 summary 含落点提示
let list;
try {
  list = await (await fetch(`${base}/api/tools`)).json();
} catch (error) {
  console.error(`  ⊘ 不可判：读不到工具清单（判据前提不成立）：${String(error).slice(0, 80)}`);
  process.exit(3);
}
const entry = (list.tools ?? []).find((t) => t.name === "set_reference");
if (!entry) { console.log("  ✗ 工具清单里没有 set_reference"); process.exit(1); }
check(
  typeof entry.description === "string" && entry.description.includes("换进程需重设"),
  `② 工具清单里 set_reference 的 summary 含「换进程需重设」（实测 ${JSON.stringify(entry.description)?.slice(0, 90)}）`,
);

// ③ 文档渲染逐字节不变
const during = await shot();
if (!during) { console.log("  ⊘ 不可判：设参考图后拿不到直通 RGBA（判据前提不成立）"); process.exit(3); }
check(before.equals(during), `③ 设参考图前后渲染差异字节=${before.reduce((n, v, i) => (v !== during[i] ? n + 1 : n), 0)}（必须为 0：落点仍是偏好，文档未被改动）`);

if (failures.length) {
  console.log(`\n  ✗ ${failures.length} 处不符（C3 set_reference 落点明示）`);
  process.exit(1);
}
console.log("\n  ✓ C3 set_reference：回执 note + 清单 summary 都有「换进程需重设」，文档逐字节不受影响");
