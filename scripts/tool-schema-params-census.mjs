#!/usr/bin/env node
// **B5-④ 普查判据**：schema 文案里出现的参数名，必须在校验的接受集里。
//
// 背景：`dabs_per_radius` 三重不一致（文案列了、校验拒了、报错候选还建议它）
// 是"模式撒谎"——文案提到的名字，调用方照着传就会被拒。第 5 块修了三处，
// 本判据是长效普查：全表扫一遍，文案里任何一个参数名都必须在校验的接受集里。
//
// **判据**：
//   ① 全表普查：每个工具的 `description` 与每个参数的 `description` 里反引号括起
//      的名字，必须落进接受集（本工具参数 ∪ 他工具参数 ∪ 框架参数
//      doc_id/actor/session ∪ 笔刷规范名（list_brushes 的 canonical_param_names，
//      动态取）∪ 四组语义别名 hand_jitter/speed_gain/color_variance/smudge_amount
//      ∪ 工具名 ∪ 判据内显式 allowlist）；任一名字无处可落 ⇒ 红，并报工具名与位置。
//   ② brush_params 全规范名试传：66 个规范名（动态取）+ 4 个语义别名，逐个经
//      brush_stroke 的 brush_params 试传 ⇒ 都不再报"没有这个规范名"（直接证明
//      校验接受集与文案口径一致；ok:true 为接受，零墨等其它失败也算接受——
//      名字已过校验，画不画得出是另一回事）。
//   ③ 期 1 动过的工具全在普查里：brush_stroke / medium_stroke / batch /
//      swatch_brush_strokes / set_reference / try_stroke / brush_preview /
//      list_brushes / export_project / export_timelapse / new_document /
//      import_project 每只都存在、描述非空（证明文案被实际读到）、0 个未分类；
//      且 brush_stroke.brush_params 描述里的反引号名 ≥ 10 个（防空集恒真）。
//      注意：其中 5 只的文案里本来就没有反引号名（0 个是实测事实，不是漏扫），
//      所以本臂不要求"≥1 个名"。
//   ④ `dabs_per_radius` 在全表文案里 0 次出现（已知已修的例子，回归 guard）。
//   ⑤ 恒等式：已分类 + 未分类 == 抽到的反引号名总数，且总数 > 0、未分类 == 0。
//
// **变异**：把 `dabs_per_radius` 加回 brush_params 文案 ⇒ ①④ 红；往任意文案里
// 加一个假名（如 nosuch_param_xyz）⇒ ① 红；把候选名改成假名 ⇒ ② 红。
//
// 用法：node scripts/tool-schema-params-census.mjs <base> <doc> <token>
const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-schema-params-census.mjs <base> <doc> <token>");
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

// **显式 allowlist**：文案里出现、但不是参数名的反引号名。每一项必须属于下面某一类；
// 新加一项要写清理由；参数名一律不许进这里（参数名走上面的动态接受集）。
//   response：某工具响应里的字段名（如 painted_pixels、landed、all_ok）；
//   value：某参数接受的枚举值/模式名（如 color_space 的 linear/srgb、
//     pressure_curve 的 auto/flat、filter_name 的 dehaze、order 的 desc、
//     anchor_mode 的 dual、color_mixing 的 pigment、MCP 的 image 内容块类型）；
//   field：对象式参数或点列里的分量名（color 的 a、points 四元组的 time_ms）；
//   internal：引擎内部变量名、环境变量、shell 命令、设计概念、操作动词泛指
//     （paint_brush 内部的 feed_base、YANSHI_EXPORT_DIR、cp、canvas 尺寸、
//      close/open 文档、设计 5.2 的 supersede）。
const ALLOWLIST = {
  response: new Set([
    "all_ok", "failed_calls", "error", "ok", "landed", "landed_reason",
    "painted_pixels", "painted_pixels_clipped", "painted_pixels_raw",
    "base_value", "bytes_per_atom_avg", "server_rss_mb", "has_more",
    "next_offset", "next_since", "truncated", "thumb_url", "clamped_pixels",
    "effective_margin", "effective_sample_stride", "field_range",
    "field_uniform", "filled_pixels", "mask_bbox", "placed", "residual_ratio",
    "residual_rms", "sample_std", "origin", "timestamp", "first_point",
    "last_point", "expected", "dx", "dy", "actual", "ink_pixels",
    "image_skipped", "anchors", "clipped", "rendered_region",
    "requested_region", "tag_source", "tags", "warnings", "pressure_sensitive",
    "pressure_settings", "image_file",
    // **P2-10（期 2）**：dodge_burn_setup 的回执字段（`masks` 里按角色的蒙版 id 数组）✓
    "mask_ids",
    // **D1（期 2b）**：`batch` 的 `stroke_evidence` 摘要字段 + 它组装的取证字段
    //   （`ink_mass` / `max_channel_delta` 是 `brush_stroke` 回执本来的字段 ✓；
    //    `paint_delta` 是 `medium_stroke` 回执的同义字段 ✓；此前都没进过文案）。
    "stroke_evidence", "ink_mass", "max_channel_delta", "paint_delta",
  ]),
  value: new Set([
    "auto", "flat", "hex", "linear", "srgb", "srgb_bytes", "dual",
    "mix_colors", "pigment", "rgb", "cross_hatch", "relief", "azimuth",
    "elevation", "dehaze", "brightness", "contrast", "brightness_contrast",
    "height_gain", "newest", "oldest", "desc", "false", "true", "image",
  ]),
  // **`fill_region` 的 `texture` 对象子键** ✓（vector-tracing v5.2 文案补了
  //   「region 类型会继承 `mode`/`patch`/`overlap`/`candidates`/`seam`/
  //   `brightness_match`/`jitter`/`min_distance`」✓，逐条对过实现：
  //   `tools.rs:4387-4464` 与 `tools_fill.rs` 真的读这些键 ✓ ⇒ 不是编出来的名字 ✗⇒✓）：
  field: new Set(["a", "time_ms", "overlap", "candidates", "seam", "brightness_match", "jitter", "min_distance"]),
  // **滤镜/调整的参数键** ✓（第 1132 轮整合时补）：
  //   `amount` 是 `add_filter`/`add_adjustment` 的 `params.amount` ✓
  //   ⇒ 它**不是**任何工具的顶层 inputSchema 属性（在 `params` 对象里）✗
  //   ⇒ 旧分类器无处置 ⇒ 未分类 1 个 ⇒ 红 ✓
  //   **⇒ 只放这一个** ✓：其余键（`black`/`white`/`gamma`/`ranges`…）若以后进文案，
  //     也应按同一问句"它是引擎真有的键吗"补进来，不要放成通配 ✓。
  effectParam: new Set(["amount"]),
  internal: new Set([
    "feed_base", "cp", "YANSHI_EXPORT_DIR", "canvas", "open", "close",
    "supersede",
  ]),
};
const allowClassOf = (name) => {
  for (const [cls, set] of Object.entries(ALLOWLIST)) if (set.has(name)) return cls;
  return null;
};

// **四组语义别名**（tools.rs BRUSH_ALIASES；组名本身就是 brush_params 接受的键）。
const BRUSH_ALIASES = ["hand_jitter", "speed_gain", "color_variance", "smudge_amount"];
const FRAMEWORK_PARAMS = ["doc_id", "actor", "session"];
// **期 1 动过的工具**（B5-④ 要求至少覆盖 brush/medium/batch/set_reference）。
const PHASE1_TOOLS = [
  "brush_stroke", "medium_stroke", "batch", "swatch_brush_strokes",
  "set_reference", "try_stroke", "brush_preview", "list_brushes",
  "export_project", "export_timelapse", "new_document", "import_project",
];

// **反引号括起的标识符**（文件路径/刷名以数字开头，不进；`a` 这种单字母子字段要进）。
const TOKEN_RE = /`([A-Za-z][A-Za-z0-9_]*)`/g;
const scanTokens = (text) => {
  const out = [];
  let m;
  while ((m = TOKEN_RE.exec(text))) out.push(m[1]);
  return out;
};
// **递归收 description**（inputSchema 目前是平铺的，递归是防以后嵌套）。
const collectDescriptions = (node, where, out) => {
  if (!node || typeof node !== "object") return;
  if (typeof node.description === "string") out.push({ where, text: node.description });
  for (const [key, value] of Object.entries(node)) {
    if (key === "description" || key === "example") continue;
    collectDescriptions(value, where, out);
  }
};

const main = async () => {
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
  const toolNames = new Set(tools.map((t) => t?.name));
  // **全表参数名集合**（他工具参数类）。
  const allParams = new Map();
  for (const t of tools) {
    for (const p of Object.keys(t?.inputSchema?.properties ?? {})) {
      if (!allParams.has(p)) allParams.set(p, []);
      allParams.get(p).push(t.name);
    }
  }
  // **笔刷规范名动态取**（list_brushes.canonical_param_names，66 项）。
  const brushes = await call("list_brushes", {});
  const canonical = brushes?.canonical_param_names;
  if (!Array.isArray(canonical) || canonical.length === 0) {
    console.error(`❌ 判不了：list_brushes 没给 canonical_param_names（实测 ${JSON.stringify(brushes).slice(0, 120)}）=> 退出码 3`);
    process.exit(3);
  }
  const brushNames = new Set(canonical);
  // **滤镜/调整名动态取** ✓（第 1132 轮）：Y-06 的 schema 文案要写
  //   「`saturation` 的恒等值是 1」⇒ `saturation` 是**引擎效果名** ✓，
  //   不是工具名、不是参数名 ⇒ 旧分类器无处置 ✗。从能力表动态取，不写死。
  const caps = await call("list_engine_capabilities", {});
  const effectNames = new Set([...(caps?.filters ?? []), ...(caps?.adjustments ?? [])]);

  // **① 全表普查**。
  const perTool = new Map(); // tool -> {checked, unclassified: []}
  const classCount = {};
  let total = 0;
  let dabsHits = [];
  const classify = (tool, ownParams, token) => {
    if (ownParams.has(token)) return "own-param";
    if (FRAMEWORK_PARAMS.includes(token)) return "framework";
    if (toolNames.has(token)) return "tool-name";
    if (brushNames.has(token)) return "brush-canonical";
    if (effectNames.has(token)) return "effect-name";
    if (BRUSH_ALIASES.includes(token)) return "brush-alias";
    if (allParams.has(token)) return "other-tool-param";
    const allow = allowClassOf(token);
    if (allow) return `allowlist:${allow}`;
    return null;
  };
  for (const t of tools) {
    const ownParams = new Set(Object.keys(t?.inputSchema?.properties ?? {}));
    const descs = [{ where: "summary", text: String(t?.description ?? "") }];
    collectDescriptions(t?.inputSchema, "inputSchema", descs);
    const stat = { checked: 0, unclassified: [] };
    perTool.set(t.name, stat);
    for (const { where, text } of descs) {
      for (const token of scanTokens(text)) {
        total += 1;
        stat.checked += 1;
        if (token === "dabs_per_radius") dabsHits.push(`${t.name}@${where}`);
        const cls = classify(t.name, ownParams, token);
        if (cls === null) {
          stat.unclassified.push(`${token}@${where}`);
        } else {
          classCount[cls] = (classCount[cls] ?? 0) + 1;
        }
      }
    }
  }
  const unclassifiedTotal = [...perTool.values()].reduce(
    (n, s) => n + s.unclassified.length,
    0,
  );
  const classifiedTotal = total - unclassifiedTotal;
  for (const [tool, stat] of [...perTool.entries()].sort()) {
    if (stat.unclassified.length > 0) {
      check(
        false,
        `① ${tool} 文案有 ${stat.unclassified.length} 个名字无处可落：${stat.unclassified.join("、")}`,
      );
    }
  }
  check(
    unclassifiedTotal === 0,
    `① 全表 ${tools.length} 个工具、${total} 个反引号名：已分类 ${classifiedTotal}，未分类 ${unclassifiedTotal}（${Object.entries(classCount).map(([k, v]) => `${k}:${v}`).join("、")}）`,
  );

  // **③ 期 1 工具覆盖**（存在 + 文案被实际读到 + 0 未分类；防空集恒真）。
  // 注意：list_brushes / export_project / export_timelapse / new_document /
  // import_project 的文案里本来就没有反引号名（0 个是实测事实，不是漏扫），
  // 所以不要求"≥1 个名"，改要求"描述非空"——证明普查确实读到了它的文案。
  for (const name of PHASE1_TOOLS) {
    const stat = perTool.get(name);
    check(
      stat !== undefined,
      `③ 期 1 工具 ${name} 在工具清单里`,
    );
    if (stat) {
      const tool = tools.find((t) => t?.name === name);
      const descLen = String(tool?.description ?? "").length;
      check(
        stat.unclassified.length === 0 && descLen > 0,
        `③ ${name} 被普查到 ${stat.checked} 个名、未分类 ${stat.unclassified.length} 个、描述 ${descLen} 字（已读到文案）`,
      );
    }
  }
  const brushStroke = tools.find((t) => t?.name === "brush_stroke");
  const bpDesc = String(
    brushStroke?.inputSchema?.properties?.brush_params?.description ?? "",
  );
  const bpTokens = scanTokens(bpDesc);
  check(
    bpTokens.length >= 10,
    `③ brush_stroke.brush_params 描述里反引号名 ${bpTokens.length} 个（≥10，防空集恒真；实测 ${bpTokens.join("、")}）`,
  );

  // **④ dabs_per_radius 全表 0 次**。
  check(
    dabsHits.length === 0,
    `④ dabs_per_radius 在全表文案里出现 ${dabsHits.length} 次${dabsHits.length ? `（${dabsHits.join("、")}）` : ""}`,
  );

  // **⑤ 恒等式**。
  check(
    classifiedTotal + unclassifiedTotal === total && total > 0 && unclassifiedTotal === 0,
    `⑤ 恒等式：已分类 ${classifiedTotal} + 未分类 ${unclassifiedTotal} == 总数 ${total}，且总数 > 0、未分类 == 0`,
  );

  // **② brush_params 全规范名试传**（66 规范名 + 4 别名；名字被拒才算失败）。
  await call("create_layer", { layer_id: "L1", name: "L1" });
  let n = 0;
  // **object_id 带运行戳**：同一服务上重复跑判据时，旧 object_id 已存在会让整笔
  // 以"对象已存在"失败——那不是名字被拒，但会让试传臂空转（全进"其它接受"）。
  const runTag = Date.now().toString(36);
  const stroke = async (brushParams) => {
    n += 1;
    return call("brush_stroke", {
      layer_id: "L1",
      object_id: `census-${runTag}-${n}`,
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
  const trialNames = [...canonical, ...BRUSH_ALIASES];
  let okTrue = 0;
  let acceptedOther = [];
  let rejected = [];
  for (const name of trialNames) {
    const tried = await stroke({ [name]: 1.0 });
    const detail = detailOf(tried);
    if (tried?.ok === true) {
      okTrue += 1;
    } else if (detail.includes("没有") && detail.includes("规范名")) {
      rejected.push(name);
    } else {
      // **名字已过校验**（画不画得出是另一回事，如零墨短路径）：记一笔，不算失败。
      acceptedOther.push(`${name}（${detail.slice(0, 60) || JSON.stringify(tried).slice(0, 60)}）`);
    }
  }
  check(
    rejected.length === 0,
    `② 试传 ${trialNames.length} 个名（规范名 ${canonical.length} + 别名 ${BRUSH_ALIASES.length}）：ok:true ${okTrue} 个、其它接受 ${acceptedOther.length} 个、被拒 ${rejected.length} 个${rejected.length ? `（${rejected.join("、")}）` : ""}`,
  );
  if (acceptedOther.length > 0) {
    console.log(`    （其它接受明细：${acceptedOther.join("｜")}）`);
  }

  if (fails.length) {
    console.error(`❌ ${fails.length} 项没成立：${fails.join("｜")}`);
    process.exit(1);
  }
  console.log(
    `✓ B5-④ 普查成立：${tools.length} 个工具、${total} 个文案名全有落处；` +
      `${trialNames.length} 个笔刷名逐个试传全被接受；dabs_per_radius 全表 0 次`,
  );
};

await main();
