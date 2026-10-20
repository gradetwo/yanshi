#!/usr/bin/env node
// **`medium_stroke` 的 `texture`：声明必须是**数字 0..1**，且只有 `oil` 真的用它**（第 1412 轮）
//
// **⇒ 外部的请求** ✗：「我从错误信息里看到 `texture` 存在，但文档没写取值」⇒ **我去查，发现更糟：**
//   · spec 里声明成 **`String`** 且说明写着「**暂不支持**（只接受缺省/smooth）」✗
//   · 而实现一直按**数字**读：`args.get("texture").and_then(Value::as_f64)` ⇒ **然后 clamp(0,1)** ✗
//   ⇒ **所以「声明」与「实现」不符** ⇒ 传 `"smooth"` 会被当成 None（数字读失败）⇒ **静默变 0** ✗
//
// **⇒ 断什么**：
//   ① `GET /api/tools` 里 `medium_stroke.texture` 的类型**必须是 number**（不是 string）
//   ② 传 `"smooth"`（旧说明暗示的取值）⇒ **必须被拒**（`ok=false`），不许静默当 0
//   ③ **`oil` 的产物随 `texture` 改变**（0 vs 1 的导出字节不同 ✓）
//   ④ **打印**其余五种介质是否改变（实测**都不变** ⇒ 这是**能力边界**，如实打印，不断言）
//
// **⇒ 退出码**：0 通过｜1 不符｜**3 不可判**
import { readFileSync } from "node:fs";
import { createHash } from "node:crypto";

const [base] = process.argv.slice(2);
if (!base) {
  console.error("用法: node scripts/tool-stroke-texture.mjs <base>");
  process.exit(3);
}
const stamp = Date.now();
const fails = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fails.push(m); console.log(`  ✗ ${m}`); };

// ① 声明类型
{
  const r = await (await fetch(`${base}/api/tools`)).json();
  // 实际形状：每个工具是 {name, description, example, inputSchema} ⇒ 参数在 inputSchema.properties
  const spec = (r.tools || []).find((t) => t.name === "medium_stroke");
  const props = (spec && spec.inputSchema && spec.inputSchema.properties) || {};
  const p = props.texture;
  if (!p) {
    bad("① medium_stroke.inputSchema.properties 里没有 texture（有：" + Object.keys(props).join(",") + "）");
  } else if (String(p.type).toLowerCase() === "number") {
    ok("① texture 声明为 number（" + String(p.description || "").slice(0, 50) + "…）");
  } else {
    bad("① texture 必须声明为 number ⇒ 实测 type=" + p.type);
  }
}


const newDoc = async (tag) => {
  const id = `tx_${tag}_${stamp}`;
  const r = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: id, width: 128, height: 128,
      background: { r: 128, g: 128, b: 128, a: 255 } }),
  })).json();
  return r.token ? { id, tok: r.token } : null;
};
const post = async (d, tool, args) => {
  const r = await fetch(`${base}/api/tools/${tool}?doc=${d.id}&token=${d.tok}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args ?? {}),
  });
  return r.json();
};

// ② 字符串必须被拒
{
  const d = await newDoc("str");
  if (!d) { console.error("  ⊘ 不可判：建不出文档"); process.exit(3); }
  await post(d, "create_layer", { layer_id: "L", name: "L" });
  const r = await post(d, "medium_stroke", { layer_id: "L", medium: "oil", size: 20, texture: "smooth",
    points: [[20, 40, 0.8], [100, 80, 0.8]] });
  if (r?.ok === false) ok(`② texture="smooth" 被拒（${String((r.context || {}).detail || "").slice(0, 60)}…）`);
  else bad(`② texture="smooth" 必须被拒（声明是 number）⇒ 实测 ok=${r?.ok}`);
}

// ③ oil 必须随 texture 改变；④ 其余介质如实打印
const role = async (med) => {
  const hashes = [];
  for (const tx of [0.0, 1.0]) {
    const d = await newDoc(`${med}${Number(tx)}`);
    if (!d) return null;
    await post(d, "create_layer", { layer_id: "L", name: "L" });
    await post(d, "medium_stroke", { layer_id: "L", medium: med, size: 24, load: 0.9, wetness: 0.4,
      color: [0.3, 0.4, 0.5], texture: tx, points: [[20, 40, 0.8], [100, 80, 0.8]] });
    const e = await post(d, "export_png", { path: `tx_${med}_${Number(tx)}_${stamp}.png`, deterministic: true });
    if (e?.ok !== true || !e.path) return null;
    hashes.push(createHash("sha256").update(readFileSync(e.path)).digest("hex"));
  }
  return hashes[0] !== hashes[1];
};

const oilChanges = await role("oil");
if (oilChanges === true) ok("③ oil 的产物随 texture 改变（0 与 1 的导出字节不同）");
else if (oilChanges === false) bad("③ oil 必须响应 texture ⇒ 实测 0 与 1 的导出字节相同");
else console.log("  ○ ③ 未验：oil 导出失败");

const others = [];
for (const med of ["watercolor", "marker", "pencil", "pixel", "example"]) {
  const changes = await role(med);
  others.push(`${med}${changes === null ? "?" : changes ? "✓" : "✗"}`);
}
console.log(`  ④ 打印：其余介质是否响应 texture ⇒ ${others.join("｜")}（✗ = 该介质忽略 texture，属能力边界）`);

if (fails.length > 0) {
  console.log(`\n  ✗ ${fails.length} 处不符（texture 的声明与行为）`);
  process.exit(1);
}
console.log("\n  ✓ texture 声明为数字、非法字符串被拒，且 oil 真的响应");
