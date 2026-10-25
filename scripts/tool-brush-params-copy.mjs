#!/usr/bin/env node
// **B5 模式撒谎三件套**：registry 文案、校验候选兜底、`try_stroke` layer_id 文本三处
// 声明与实现对不上（`dabs_per_radius` 不是规范名却三处都出现）。
//
// **判据**：
//   ① 传 `dabs_per_radius` ⇒ 报错"相近的有"候选清单里不再含 `dabs_per_radius`
//      （报错正文会提到被拒的键名本身，那不算候选——只断候选清单部分）；
//   ② 传任意未知键（如 `nosuch_setting_xyz`，无前缀匹配走兜底）⇒ 候选清单里的每个名字
//      逐个试传 `brush_params` 都不再报"没有这个规范名"（直接证明候选全真实）；
//   ③ `try_stroke` 无 `layer_id` 无 `based_on` ⇒ 报错文案与 schema 文案一致
//      （都提到 `layer_id` 与 `based_on`）；
//   ④ tools/list 里 brush_stroke 的 `brush_params` 描述不再出现 `dabs_per_radius`
//      （且仍提到真实名 `dabs_per_actual_radius`，防整段误删）。
//
// **变异**：把 `dabs_per_radius` 加回文案或候选 ⇒ ①②④ 红；把候选换成另一个假名 ⇒ ② 红。
//
// 用法：node scripts/tool-brush-params-copy.mjs <base> <doc> <token>
const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-brush-params-copy.mjs <base> <doc> <token>");
  process.exit(2);
}
const call = async (name, args) =>
  (
    await fetch(`${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(args ?? {}),
    })
  ).json();

const fails = [];
const check = (ok, msg) => {
  if (!ok) fails.push(msg);
  console.log(`  ${ok ? "✓" : "✗"} ${msg}`);
};
const detailOf = (reply) =>
  String(reply?.context?.detail ?? reply?.error ?? reply?.message ?? "");
// **只取"相近的有："之后、"；"之前的候选清单**（被拒的键名本身在冒号之前，不算候选）。
const candidatesOf = (detail) => {
  const marker = "相近的有：";
  const at = detail.indexOf(marker);
  if (at < 0) return null;
  const tail = detail.slice(at + marker.length).split("；")[0].trim();
  if (!tail) return [];
  return tail.split("、").map((s) => s.trim()).filter(Boolean);
};

const main = async () => {
  // **先建 L1**（传入文档只有缺省图层，落笔要显式图层）。
  await call("create_layer", { layer_id: "L1", name: "L1" });
  const stroke = async (brushParams) => {
    // **同一文档上每笔 object_id 必须唯一**（复用会报"对象已存在"）。
    stroke.n = (stroke.n ?? 0) + 1;
    return call("brush_stroke", {
      layer_id: "L1",
      object_id: `s${stroke.n}`,
      brush: "2B_pencil.myb",
      size: 40,
      color: { r: 0, g: 0, b: 0 },
      points: [
        [60, 120, 0.8],
        [160, 120, 0.8],
        [260, 120, 0.8],
      ],
      brush_params: brushParams,
    });
  };

  // **① 传 dabs_per_radius ⇒ 候选清单里不再有它**。
  const r1 = await stroke({ dabs_per_radius: 3 });
  const d1 = detailOf(r1);
  check(
    r1?.ok !== true && d1.includes("没有") && d1.includes("规范名"),
    `① 传 dabs_per_radius ⇒ 仍报"没有这个规范名"（实测 ${d1.slice(0, 120) || JSON.stringify(r1).slice(0, 120)}）`,
  );
  const c1 = candidatesOf(d1);
  check(
    c1 !== null && c1.length > 0,
    `① 报错里有"相近的有"候选清单（实测 ${c1 === null ? "无候选段" : `${c1.length} 条`}）`,
  );
  check(
    c1 !== null && !c1.includes("dabs_per_radius"),
    `① 候选清单不再含 dabs_per_radius（实测 ${JSON.stringify(c1)}）`,
  );

  // **② 未知键走兜底 ⇒ 候选里的每个名字逐个试传都不再报"没有这个规范名"**。
  const r2 = await stroke({ nosuch_setting_xyz: 1 });
  const d2 = detailOf(r2);
  const c2 = candidatesOf(d2);
  check(
    r2?.ok !== true && c2 !== null && c2.length > 0,
    `② 传 nosuch_setting_xyz ⇒ 报错且有候选清单（实测 ${c2 === null ? "无候选段" : JSON.stringify(c2)}）`,
  );
  if (c2 && c2.length > 0) {
    for (const name of c2) {
      const tried = await stroke({ [name]: 1.0 });
      const dt = detailOf(tried);
      check(
        tried?.ok === true,
        `② 候选 ${name} 试传 brush_params ⇒ 被接受（实测 ${tried?.ok === true ? "ok" : dt.slice(0, 100)}）`,
      );
    }
  }

  // **③ try_stroke 无 layer_id 无 based_on ⇒ 报错与 schema 文案一致**。
  const r3 = await call("try_stroke", {
    brush: "2B_pencil.myb",
    points: [
      [60, 120, 0.8],
      [160, 120, 0.8],
    ],
    size: 40,
  });
  const d3 = detailOf(r3);
  check(
    r3?.ok !== true && d3.includes("layer_id") && d3.includes("based_on"),
    `③ try_stroke 缺 layer_id ⇒ 报错同时提到 layer_id 与 based_on（实测 ${d3.slice(0, 120)}）`,
  );
  let listed;
  try {
    listed = await (await fetch(`${base}/api/tools`)).json();
  } catch (error) {
    console.error(`❌ 判不了：GET /api/tools 失败（${error}）=> 退出码 3`);
    process.exit(3);
  }
  const tools = listed?.tools ?? [];
  if (!Array.isArray(tools) || tools.length === 0) {
    console.error("❌ 判不了：GET /api/tools 没给工具清单 => 退出码 3");
    process.exit(3);
  }
  const tryStroke = tools.find((t) => t?.name === "try_stroke");
  const layerDesc = String(
    tryStroke?.inputSchema?.properties?.layer_id?.description ?? "",
  );
  check(
    layerDesc.includes("based_on"),
    `③ schema 里 try_stroke.layer_id 说明提到 based_on（实测 ${layerDesc.slice(0, 120) || "（空）"}）`,
  );

  // **④ tools/list 里 brush_params 描述不再出现 dabs_per_radius**。
  const brushStroke = tools.find((t) => t?.name === "brush_stroke");
  const bpDesc = String(
    brushStroke?.inputSchema?.properties?.brush_params?.description ?? "",
  );
  if (!bpDesc) {
    console.error("❌ 判不了：tools/list 里没找到 brush_stroke.brush_params 描述 => 退出码 3");
    process.exit(3);
  }
  check(
    !bpDesc.includes("dabs_per_radius"),
    `④ brush_params 描述不再出现 dabs_per_radius`,
  );
  check(
    bpDesc.includes("dabs_per_actual_radius"),
    `④ brush_params 描述提到真实名 dabs_per_actual_radius（防整段误删）`,
  );

  if (fails.length) {
    console.error(`❌ ${fails.length} 项没成立：${fails.join("｜")}`);
    process.exit(1);
  }
  console.log("✓ B5 撒谎三件套成立：候选全真实、文案无假名、try_stroke 报错与 schema 一致");
};

await main();
