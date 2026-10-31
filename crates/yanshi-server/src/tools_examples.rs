//! **每个工具一句「能直接抄走」的调用示例**（`TOOL_EXAMPLES`）——**纯数据**。
//!
//! **为什么拆出来**（色彩体系规划 **Phase 2** + 外部评审报告第 15 项）。
//! 本模块是**纯搬运**：行为**零变化**（全量测试通过即验收）。
//!
//! **可见性**：保持 `pub` ⇒ 父模块 `pub use tools_examples::TOOL_EXAMPLES;` 即可用 ✓
//! （`tools.rs:696` 的 `TOOL_EXAMPLES.iter().find(...)` 依赖它）

/// **每个工具一句"能直接抄走"的调用示例**（目标第 6 条）。
///
/// **生成方式**：参数名取自工具**自己声明的参数面**（`inputSchema.properties`，必填取 `required`），
/// 所以**不可能**写出一个参数面里没有的名字（仍由本文件末尾的测试逐个把关）。
/// **取值**：必填项给有意义的样例值；部分可选项是**占位值**（`x`/`1`）⇒ 抄走前按自己的场景改一下。
///
/// **为什么要有它**：实测踩到过 —— `brush_preview` 的实现读 `opacity`/`hardness`，而参数面里没有，
/// 调用方收到的是「不接受参数 hardness（拼写错误？）」，**只能靠猜**。示例把"该怎么写"直接摆出来，
/// 并由下面的测试保证**示例里的每个参数名都在该工具的参数面里**（示例与参数面不许漂移）。
///
/// 形状：`(工具名, 一段可直接粘贴的 JSON 参数)`。**先给日常最常用的三个**，
/// 其余工具按同一模式补齐（每加一个，测试自动替它把关）。
pub const TOOL_EXAMPLES: &[(&str, &str)] = &[
    ("bake_layer", r#"{"layer_id":"L1"}"#),
    ("bake_document", r#"{}"#),
    ("recover_baked_blobs", r#"{"layer_id":"L1"}"#),
    ("get_bake_status", r#"{}"#),
    ("smart_glaze", r#"{"shape":{},"color":[],"layer_id":"L1"}"#),
    ("smart_erase_shape", r#"{"layer_id":"L1","shape":{}}"#),
    (
        "smart_blend",
        r#"{"shape":{},"color_a":[],"color_b":[],"layer_id":"L1"}"#,
    ),
    ("set_anchor", r#"{"name":"x","x":1,"y":1}"#),
    ("select_opaque_pixels", r#"{"layer_id":"L1"}"#),
    ("get_histogram", r#"{}"#),
    ("fit_background_field", r#"{"layer_id":"L1","mask":{}}"#),
    ("export_timelapse", r#"{"path":"x"}"#),
    // **原生矢量（路线甲 Phase 2）**：先建矢量层、在矢量层上落笔，
    // 之后才能用脊线橡皮截断、用命中测试按脊线选笔 ✓。
    (
        "vector_erase",
        r#"[{"tool":"create_layer","arguments":{"layer_id":"v1","kind":"vector"}},{"tool":"draw_stroke","arguments":{"layer_id":"v1","data":{"points":[[20,60],[140,60]],"size":20,"color":[200,60,40,255]}}},{"tool":"vector_erase","arguments":{"layer_id":"v1","points":[[80,40],[80,80]],"radius":6}}]"#,
    ),
    (
        "vector_hit_test",
        r#"{"layer_id":"v1","x":80,"y":60,"tolerance":2}"#,
    ),
    // **选区变换 + 不透明像素选区**（yanshi-gaps 补丁，第 1187 轮补示例）：
    //   **补丁加了这两个工具却漏了 TOOL_EXAMPLES** ⇒ 被
    //   `every_registered_tool_has_a_documented_example` 抓到 ✗
    (
        "transform_selection",
        r#"{"selection_id":"s1","translate":[10,0],"scale":[1.5,1.5],"rotate_deg":30,"pivot":[100,100]}"#,
    ),
    // **蒙版画笔/擦除 + 智能肌理**（mask-texture 补丁遗留的三个缺口，第 765 轮补齐；
    // smart-select 补丁的说明里主动指出『paint_mask/delete_mask/smart_texture 缺示例，
    // 剩余 3 个需 gradetwo 补』）。
    // **蒙版要先 `create_mask` 建**：`create_selection` 建的 selection 不被 `paint_mask` 认。
    (
        "create_mask",
        r#"{"mask_id":"m1","shape":{"kind":"rect","x":100,"y":0,"w":100,"h":300}}"#,
    ),
    (
        "paint_mask",
        r#"{"mask_id":"m1","mode":"paint","size":60,"points":[[150,20,1],[150,280,1]]}"#,
    ),
    ("delete_mask", r#"{"mask_id":"m1"}"#),
    // **D1-a 范围遮罩**：亮度带示例（暗部 0–0.4 段）；颜色带见工具注册表（hue 必填）。
    (
        "create_range_mask",
        r#"{"mask_id":"m1","layer_id":"L1","range":"luminance","low":0.0,"high":0.4}"#,
    ),
    // `texture_type` 不是 `texture`；`density` 严格单调（第 750 轮实测 10/21/28/37/41）。
    (
        "smart_texture",
        r#"{"layer_id":"L1","shape":{"kind":"rect","x":20,"y":20,"w":360,"h":360},"texture_type":"stone","density":0.5}"#,
    ),
    // **四个 smart 工具**（2026-10-19 补丁）：颜色一律是 **array** `[r,g,b,a]`（0..1 或 0..255），
    // 不是 `{r,g,b,a}` 对象 —— 传对象会报「类型应为 array」。
    (
        "smart_fill",
        r#"{"layer_id":"L1","shape":{"type":"rect","x":40,"y":40,"w":180,"h":120},"brush":"2B_pencil","color":[0.86,0.24,0.16,1.0]}"#,
    ),
    (
        "smart_edge_soften",
        r#"{"layer_id":"L1","shape":{"type":"rect","x":30,"y":30,"w":120,"h":100}}"#,
    ),
    (
        "smart_select",
        r#"{"seed":[320,240],"tolerance":0.2,"mode":"new"}"#,
    ),
    (
        "smart_sample",
        r#"{"ref":"L_ref","layer_id":"L1","shape":{"type":"rect","x":40,"y":40,"w":180,"h":120},"samples":8}"#,
    ),
    (
        "smart_hatch_follow",
        r#"{"layer_id":"L1","guides":[[[40,200],[120,160],[200,200]]],"color":[0.2,0.15,0.1,1.0],"width":12,"size":22,"density":0.6}"#,
    ),
    ("sample_colors", r#"{"points": [[120, 64], [200, 80]]}"#),
    // **`get_histogram`**（包 B）：两个参数都可选 ⇒ 示例给一个区域作用域的典型用法 ✓。
    ("describe_canvas", r#"{}"#),
    ("undo_last", r#"{}"#),
    // 只读查询 ⇒ 不需要任何参数 ✓（落笔之后问一次，界面据此刷新按钮 ✓）。
    ("get_undo_status", r#"{}"#),
    // **字体方案 Phase 3**：只读查询 ⇒ 无参数 ✓（先问有哪些字体，再把族名交给 draw_text ✓）。
    ("list_fonts", r#"{}"#),
    // **附录五（P0）**：试笔触 —— 批量但只给一支也能跑（示例拿最小可用形状）。
    (
        "swatch_brush_strokes",
        // **`Blender` 是涂抹类** ⇒ 空画布上没有颜色可抹 ⇒ A6 之后这个示例会 `ok=false` ✗
        //   （第 1132 轮整合实测：`tool-example-acceptance` 唯一被拒的示例就是它）
        //   ⇒ 换成能沉积的 `Classic_Paint` ✓（示例要演示"试笔触"，不是"试涂抹"）。
        r#"{"brushes":["Classic_Paint"],"size":24}"#,
    ),
    // **示例必须是"语法合法"的哈希** ✗（第 41 轮实测 ✓）：原来是占位符 `"x"` ✗
    // ⇒ 被 `write_set_reference` 的**形状校验**拒掉（"blob_hash 必须是 sha256:<64 位十六进制>" ✓）
    // ⇒ 而 `scripts/tool-example-acceptance.mjs` 把"**形状／类型不对**"判成**红** ✗
    //（它的判据是"**抄走会被拒**"✓）⇒ **CI 的 criteria shard 2/6 因此失败** ✗。
    // 换成合法形状后 ✓，错误变成"**这个 blob 不在存储里**"（＝**缺前置状态** ✓）
    // ⇒ 判据记作"参数被接受 ✓，另有原因" ✓（93 个跑通 ＋ 1 个"需要前置状态" ✓）。
    // **∴ 不要把它改回 `x`** ✗ —— 那会让 CI 再红一次 ✓。
    (
        "set_reference",
        r#"{"blob_hash":"sha256:0000000000000000000000000000000000000000000000000000000000000000"}"#,
    ),
    ("set_preferences", r#"{"values":{}}"#),
    ("redo_last", r#"{}"#),
    ("new_document", r#"{"width":1,"height":1}"#),
    ("gradient_fill", r#"{"layer_id":"L1","from":{},"to":{}}"#),
    ("get_resolved_state", r#"{"object_id":"o1"}"#),
    ("get_object_history", r#"{"object_id":"o1"}"#),
    ("get_object", r#"{"object_id":"o1"}"#),
    // **A1 路径数据面**：读回 path 几何 / 节点级编辑（先 draw_shape 建 kind=path 对象）。
    ("get_path", r#"{"object_id":"p1"}"#),
    (
        "edit_path",
        r#"{"object_id":"p1","op":"move_node","subpath":0,"node":1,"to":[120,80]}"#,
    ),
    ("get_descendants", r#"{"object_id":"o1"}"#),
    ("get_dependency_graph", r#"{"object_id":"o1"}"#),
    ("get_ancestors", r#"{"object_id":"o1"}"#),
    ("fill", r#"{"layer_id":"L1","data":{}}"#),
    ("export_png", r#"{"path":"x"}"#),
    // **查一个工具怎么调** ✓（第 1561 轮；八处清单里我漏了这一处 ⇒ CI 的
    // `every_registered_tool_has_a_documented_example` 当场抓住 ✗）
    //   ⇒ **这条判据的价值正在此**：新工具忘了写示例，它立刻红 ✓
    ("explain_tool", r#"{"name":"stroke_path"}"#),
    ("export_svg", r#"{"path":"x"}"#),
    // **两点之间生成过渡笔触**（AI 画家需求 P0-1.2）：左暗蓝 ⇒ 右暖白，10 笔。
    // 它逐笔走 `brush_stroke` 的落笔实现 ⇒ 与手画一致；`steps` 含首末两点。
    (
        "gradient_blend",
        r##"{"layer_id": "layer_default", "from": {"x": 40, "y": 100, "color": "#2040a0"}, "to": {"x": 260, "y": 100, "color": "#f0e0c0"}, "brush": "classic-brush", "size": 24, "steps": 10}"##,
    ),
    // **渐变填充**（回归报告踩过：它和我们一开始都猜了个不存在的参数名 ✗）——
    // 真参数是 `kind`（不是 `geometry`）；颜色用**对象写法**而不是 `"#rrggbb"`，
    // 因为 Rust 的 `r#"…"#` 裸字符串不能包含 `"#`（会提前结束、把源码弄坏）。
    ("estimate_dehaze", r#"{}"#),
    // **期 4 D1 频率分离 / D&B**：参数面见工具注册表（layer_id 必填，
    // radius 域 [1,64]，mode 仅 curves/gray）。
    ("frequency_separation", r#"{"layer_id":"L1","radius":8}"#),
    // **mode 的历史（写在这里而不是文档里 ⇒ 文档是生成出来的，手写注释会被重生成抹掉）**：
    // 示例原先用 `curves`，而当时 curves 那条路改不到像素（纯调整层落在没有像素的层里），
    // 所以第五轮实测把它改成 `gray`；上游 `302d414d`「纯调整层渲染时用下方内容播种」之后
    // curves 真的改得到画面了（截至该轮实测：dodge 蒙版涂白 128→159、burn 涂白 128→98、
    // 端点 0 与 255 不动；权威断言在 `scripts/tool-dodge-burn-effective.mjs`）。
    // 示例仍写 `gray` 只因为「涂就见效」这一条更短，**不再**是因为 curves 无效。
    ("dodge_burn_setup", r#"{"layer_id":"L1","mode":"gray"}"#),
    // **期 4 D3 自动模式**：参数面见工具注册表（layer_id 可缺省，
    // strength 域 [0, 1]，groups 为 8 组白名单子集）。
    ("auto_retouch", r#"{"layer_id":"L1"}"#),
    (
        "erase",
        r#"{"layer_id":"L1","data":{"region":{"x":40,"y":40,"w":120,"h":80},"size":32.0,"hardness":1.0,"opacity":1.0}}"#,
    ),
    ("duplicate_layer", r#"{"layer_id":"L1"}"#),
    ("draw_text", r#"{"layer_id":"L1","data":{}}"#),
    ("draw_stroke", r#"{"layer_id":"L1","data":{}}"#),
    // **事务 / 历史这一族的多步示例** ✓（第 227 轮 ✓）—— 目标 (B)① 点名的"需事务/历史/冲突等前置状态" ✓。
    // 契约（`every_documented_example_only_uses_declared_parameters` ✓，我为此栽过三次 ✗）：
    // **合法 JSON** ✓、**每步都是 `{"tool":…,"arguments":{…}}`** ✓（参数**不**平铺 ✗）、
    // **末步必须是这个工具自己** ✓、**无参数的工具可给普通对象** ✓。
    // 撤销/重做**必须"真的画过"** ✓ ⇒ 示例里就画一笔 ✓（参数抄 `draw_stroke` 自己的示例 ✓，**不猜** ✗）。
    ("begin_transaction", r#"{}"#),
    // **冲突 / 历史族** ✓（第 229 轮 ✓）：目标 (B)① 点名的"事务/历史/冲突" ✓。
    // 参数都按原文抄 ✓（**不猜** ✗）：`resolve_conflict` 必填 `resolution` ✓（四个枚举值之一 ✓）；
    // `get_object_history` 必填 `object_id` ✓。前置状态由**先画一笔**给出 ✓（末步仍是它自己 ✓）。
    (
        "resolve_conflict",
        r#"[{"tool":"draw_stroke","arguments":{"layer_id":"L1","data":{}}},{"tool":"resolve_conflict","arguments":{"resolution":"keep_ours"}}]"#,
    ),
    (
        "commit_transaction",
        r#"[{"tool":"begin_transaction","arguments":{}},{"tool":"draw_stroke","arguments":{"layer_id":"L1","data":{}}},{"tool":"commit_transaction","arguments":{}}]"#,
    ),
    // **变更集 / 祖先族**（第 278 轮）：与事务/历史相邻的一族，原先一条示例都没有。
    // 参数照原文抄：`abort_changeset` 无参数；`revert_changeset` 必填 `changeset_id`；
    // `get_ancestors` / `get_descendants` 必填 `object_id`。契约同前：末步是自己、参数在 arguments 里。
    ("abort_changeset", r#"{}"#),
    (
        "revert_changeset",
        r#"[{"tool":"begin_changeset","arguments":{}},{"tool":"commit_changeset","arguments":{}},{"tool":"revert_changeset","arguments":{"changeset_id":"cs1"}}]"#,
    ),
    // **标注 / 参考图**（第 280 轮）：这一族原先一条示例都没有；
    // `delete_annotation` 与 `get_annotation` 必填 `annotation_id`；`clear_reference` 无参数。
    ("delete_annotation", r#"{"annotation_id":"ann1"}"#),
    ("get_annotation", r#"{"annotation_id":"ann1"}"#),
    ("clear_reference", r#"{}"#),
    // **选区 / 填充 / 变更集**（第 279 轮）：这一族原先一条示例都没有；
    // 取值按各自参数说明里的形状照写（`shape` 的字段名就是从说明里抄的），不编造。
    (
        "fill_region",
        r#"{"layer_id":"L1","shape":{"type":"rect","x":10,"y":10,"w":60,"h":60},"color":{"r":200,"g":30,"b":60,"a":255}}"#,
    ),
    (
        "create_selection",
        r#"{"selection_id":"sel1","shape":{"kind":"rect","bbox":{"x":10,"y":10,"w":60,"h":60}}}"#,
    ),
    ("delete_selection", r#"{"selection_id":"sel1"}"#),
    ("commit_changeset", r#"{}"#),
    ("create_group", r#"{"group_id":"x","layer_id":"L1"}"#),
    ("comment", r#"{"text": "x"}"#),
    ("collect_garbage", r#"{}"#),
    ("checkpoint", r#"{}"#),
    ("blob_gc", r#"{}"#),
    ("begin_changeset", r#"{}"#),
    ("put_blob", r#"{"path":"/tmp/red8.png"}"#),
    (
        "import_image",
        r#"{"layer_id": "L1", "bitmap": {}, "region": {"x": 0, "y": 0, "w": 32, "h": 32}}"#,
    ),
    (
        "medium_stroke",
        r#"{"layer_id": "L1", "medium": "oil", "points": [[60, 220, 1], [140, 220, 1]]}"#,
    ),
    ("list_palette_colors", r#"{"palette": "open-color.json"}"#),
    ("delete_layer", r#"{"layer_id": "L1"}"#),
    ("list_textures", r#"{}"#),
    ("list_documents", r#"{}"#),
    // **补的示例都要"本会话亲手调通过"** ✓（目标第 (5) 条：继续扩大覆盖面 ✓）。
    // 新文档现在**自带 `layer_default`** ✓（第 (3) 项）⇒ 示例可以放心引用它 ✓（不必先建层 ✓）。
    ("get_state", r#"{"include_objects": true}"#),
    ("list_objects", r#"{}"#),
    (
        "render_region",
        r#"{"region": [0, 0, 64, 64], "include_image": true}"#,
    ),
    (
        "update_layer",
        r#"{"layer_id": "layer_default", "patch": {"blend_mode": "multiply"}}"#,
    ),
    ("get_brush_settings", r#"{"brush": "Clouds"}"#),
    (
        "set_brush_setting",
        r#"{"brush": "Clouds", "setting": "smudge", "value": 0.5}"#,
    ),
    ("list_engine_capabilities", r#"{}"#),
    ("list_assets", r#"{"kind": "brush", "tag": "fur"}"#),
    // 第 (5) 条继续扩 ✓：形状照**仓库里现成的调用**写 ✓（`browser-ui-check.mjs` 的 fill/draw_shape ✓），
    // 不凭记忆编参数 ✗（示例会被 `tool-example-acceptance.mjs` **真的执行** ✓，编错就红 ✓）。
    // **必须带颜色** ✓（外部第 10 轮实测 ✓）：`draw_shape` 没有 `fill` 键 ✓，而原来的示例里
    // **也没有颜色** ✗ ⇒ 对方查不出来"怎么画一个有色矩形"，只好改用 `import_image` 导入自己生成的 PNG ✗
    // ⇒ 示例就是接口的一部分 ✓：绘制类示例要**画出东西**（`data.color` 是填充色 ✓，与 `fill_region` 同口径 ✓）。
    (
        "draw_shape",
        r#"{"layer_id": "layer_default", "object_id": "box", "data": {"geometry": {"kind": "rect", "bbox": {"x": 40, "y": 40, "w": 30, "h": 30}}, "color": [200, 60, 60, 255]}}"#,
    ),
    ("list_selections", r#"{}"#),
    // 第 (5) 条继续扩 ✓：形状照**仓库里现成的调用**抄 ✓（第 236 轮普查 ✓）——
    // `get_diff` 见 `registry.call(&mut ctx, "get_diff", &json!({"from_seq": 0}))` ✓；
    // `find_atom` / `get_object_history` 见各自测试里的 `{"object_id": "s1"}` ✓。
    ("get_diff", r#"{"from_seq": 0}"#),
    ("find_atom", r#"{"object_id": "s1"}"#),
    // 第 (5) 条继续扩 ✓：这一批用**机器普查**挑 ✓（第 238 轮 ✓）——
    // 条件：**没有任何必填参数** ✓（`inputSchema.required` 为空 ✓）且**还没有示例** ✓。
    // 这类是纯读型 ✓ ⇒ 在全新文档上跑 `{}` 不会因"前置状态缺失"而红 ✓。
    // （同一批里的 `redo_last`/`undo_last`/事务类**故意不选** ✗ —— 它们**有副作用或需要前置状态** ✓，
    //  单条示例结构上不适用 ✓，见第 237 轮记下的机制性限制 ✓。）
    ("get_log", r#"{}"#),
    ("get_preferences", r#"{}"#),
    ("get_checkpoints", r#"{}"#),
    ("get_changesets", r#"{}"#),
    ("list_stashes", r#"{}"#),
    ("list_effects", r#"{}"#),
    ("list_annotations", r#"{}"#),
    ("list_comments", r#"{}"#),
    // 第 (5) 条继续扩 ✓：必填只有 `layer_id` ✓ ⇒ 用新建文档自带的 `layer_default` ✓（机器普查选出 ✓）。
    ("lock_layer", r#"{"layer_id": "layer_default"}"#),
    // 第 (5) 条继续扩 ✓：必填只有 `layer_id` ✓ ⇒ 用新建文档自带的 `layer_default` ✓（机器普查选出 ✓）。
    ("unlock_layer", r#"{"layer_id": "layer_default"}"#),
    // **多步示例** ✓（第 258 轮机制 ✓）：先画出它的对象 ✓，再对该对象操作 ✓（本批由普查+实调选出 ✓）。
    // **多步示例** ✓（第 258 轮机制 ✓）：先画出它的对象 ✓，再对该对象操作 ✓（本批由普查+实调选出 ✓）。
    // **多步示例** ✓（第 258 轮机制 ✓）：先画出它的对象 ✓，再对该对象操作 ✓（本批由普查+实调选出 ✓）。
    // **多步示例** ✓（第 258 轮机制 ✓）：先画出它的对象 ✓，再对该对象操作 ✓（本批由普查+实调选出 ✓）。
    // **多步示例** ✓（第 258 轮机制 ✓ + 本轮的测试与脚本 ✓）：数组 ⇒ 先画一个 `s1`、再查它的历史 ✓。
    // 它要求对象**先存在** ✓，而示例机制是"**在全新文档上单跑一条**" ✓ ⇒ 结构上不适用 ✗
    //（要给这类工具做示例，得先支持**多步示例** ✓ —— 那是示例机制的扩展 ✓，不是这一条的事 ✗）。
    ("list_layers", r#"{}"#),
    ("sample_color", r#"{"x": 120, "y": 64}"#),
    ("get_document", r#"{}"#),
    // **★ 命名锚点（真机报告 2026-10-15）★**：起稿定死关键点 ⇒ 跨会话不再重估 ✓。
    ("get_anchors", r#"{}"#),
    ("delete_anchor", r#"{"name": "left_eye"}"#),
    ("calibrate", r#"{}"#),
    ("get_stroke_log", r#"{"limit": 10}"#),
    ("create_layer", r#"{"layer_id": "L1", "name": "Layer 1"}"#),
    (
        "brush_stroke",
        r#"{"brush": "100%_Opaque", "layer_id": "L1", "points": [[100, 100, 1], [180, 140, 1], [260, 100, 1]], "color": {"r": 255, "g": 0, "b": 0, "a": 255}, "size": 40}"#,
    ),
    (
        "stroke_path",
        r#"{"layer_id": "L1", "subpaths": [{"start": [60, 200], "segments": [{"op": "C", "c1": [160, 60], "c2": [300, 60], "to": [400, 200]}]}], "brush": "2B_pencil", "size": 24, "color": {"r": 30, "g": 30, "b": 30, "a": 255}}"#,
    ),
    (
        "combine_shapes",
        r#"{"object_ids": ["shape_a", "shape_b"], "mode": "union", "result_id": "merged"}"#,
    ),
    (
        "brush_preview",
        r#"{"brush": "spray", "size": 24, "hardness": 0.6, "opacity": 0.8, "color": {"r": 0, "g": 64, "b": 255, "a": 255}}"#,
    ),
    (
        "export_book_pngs",
        r#"{"doc_ids": ["page-01", "page-02"], "dir": "book-pages"}"#,
    ),
    ("delete_object", r#"{"object_id": "o1"}"#),
    // **分组 / 悬空变更集 / 路径转换**（第 287 轮）：这几族原先都没有示例；
    // 参数照原文抄：`add_to_group` 必填 `group_id` 与 `object_id`；
    // `apply_stash` / `discard_stash` 必填 `stash_id`；`convert_to_shape` 必填 `object_id`（`shape_id` 可选）。
    ("add_to_group", r#"{"group_id":"g1","object_id":"L1"}"#),
    ("apply_stash", r#"{"stash_id":"st1"}"#),
    ("discard_stash", r#"{"stash_id":"st1"}"#),
    ("convert_to_shape", r#"{"object_id":"L1"}"#),
    // **作业 / 渲染状态 / 路径转换 / 蒙版**（第 288 轮）：这几族原先都没有示例；
    // 参数照原文抄：`cancel_job` 与 `get_job` 必填 `job_id`；`get_render_status` 必填 `atom_id`；
    // `convert_to_path` 必填 `object_id`；`create_mask` 必填 `mask_id` 与 `shape`（说明里给了 kind/bbox 的形状）。
    ("cancel_job", r#"{"job_id":"job1"}"#),
    ("get_job", r#"{"job_id":"job1"}"#),
    ("get_render_status", r#"{"atom_id":"atom1"}"#),
    // **在飞观察/取消**（外部测试报告 P1 ✓）：两个都**无必填参数** ✓
    //（`doc_id` 可选 ✓ ⇒ 缺省就是当前文档 ✓）—— 客户端超时后先查、再决定重试还是中止 ✓。
    ("get_inflight", r#"{}"#),
    ("cancel_operation", r#"{}"#),
    ("convert_to_path", r#"{"object_id":"L1"}"#),
    // **区域分析 / 实例**（第 290 轮）：`analyze_region` 必填 `region`（`{x,y,w,h}`，与 `render_region` 同一形状）；
    // `create_instance` 必填 `instance_id` / `layer_id` / `master_id`（`local_transform` 可选）。
    (
        "analyze_region",
        r#"{"region":{"x":0,"y":0,"w":100,"h":100}}"#,
    ),
    (
        "create_instance",
        r#"{"instance_id":"inst1","layer_id":"L1","master_id":"L1"}"#,
    ),
    // **标注创建 / 批处理**（第 291 轮）：取值照参数说明抄 ——
    // `create_annotation`：`type` ∈ region|object|arrow|text|doodle|highlight、
    // `intent` ∈ modify|add|remove|replace|style|move|resize|color、`target` = {target:"region",bbox}；
    // `batch`：`calls` = [{tool, arguments}]（**参数**里的嵌套形状，不是多步示例的写法）。
    (
        "create_annotation",
        r#"{"type":"region","intent":"add","target":{"target":"region","bbox":{"x":10,"y":10,"w":60,"h":60}}}"#,
    ),
    (
        "batch",
        r#"{"calls":[{"tool":"draw_stroke","arguments":{"layer_id":"L1","data":{}}}]}"#,
    ),
    // **建议 / 图章 / 基线 / 实例脱离 / 原子**（第 292 轮）：这一批的取值全部来自参数说明 ——
    // `clone_stamp` 与 `heal_stamp` 必填 `layer_id`/`points`/`source_offset`（`points` = [[x,y],…]）；
    // `declare_head` 必填 `base_type`（atom|checkpoint）与 `base_id`；`detach_instance` 必填 `instance_id`；
    // `get_atom` / `accept_suggestion` 各必填一个 id。（`add_adjustment`/`add_filter` 的取值说明写"见工具说明"，仍旧不写。）
    ("accept_suggestion", r#"{"suggestion_id":"sug1"}"#),
    (
        "clone_stamp",
        r#"{"layer_id":"L1","points":[[10,10],[50,50]],"source_offset":[0,-30]}"#,
    ),
    (
        "heal_stamp",
        r#"{"layer_id":"L1","points":[[10,10],[50,50]],"source_offset":[0,-30]}"#,
    ),
    ("declare_head", r#"{"base_type":"atom","base_id":"atom1"}"#),
    ("detach_instance", r#"{"instance_id":"inst1"}"#),
    (
        "try_stroke",
        r#"{"layer_id":"L1","brush":"2B_pencil","points":[[10,10],[40,40]],"size":8}"#,
    ),
    ("revert_group", r#"{"group_id":"g-2026-10-19-1"}"#),
    ("revert_to_seq", r#"{"seq":100}"#),
    ("revert_atoms", r#"{"atom_ids":["a1","a2"]}"#),
    (
        "try_batch",
        r#"{"layer_id":"L1","strokes":[{"brush":"2B_pencil","points":[[10,10],[40,40]]}]}"#,
    ),
    ("commit_batch", r#"{"attempt_ids":["a1","a2"]}"#),
    ("discard_stroke", r#"{"attempt_id":"abc123"}"#),
    (
        "revoke_token",
        r#"{"token":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"}"#,
    ),
    ("list_attempts", r#"{"limit":10,"status":"pending"}"#),
    ("commit_stroke", r#"{"attempt_id":"abc123"}"#),
    ("list_atoms", r#"{"limit":10,"label":"nose"}"#),
    ("get_atom", r#"{"atom_id":"atom1"}"#),
    // **一批"纯 id / 无参数"的工具**（第 293 轮）：这些的必填参数都是 id 或什么都不需要，
    // 取值不需要猜 ⇒ 一次补齐。（`add_adjustment`/`add_filter`/`set_property` 等仍不写：取值说明不足。）
    ("export_project", r#"{"path":"exports/demo.yanshi"}"#),
    ("import_project", r#"{"path":"exports/demo.yanshi"}"#),
    // **期 2 块 3（D2）漏了示例**：`verify_package` 注册时没进 TOOL_EXAMPLES，
    // pre-commit 的注册表清单检查当场红 ⇒ 补上（与 import_project 同一路徑写法）。
    ("verify_package", r#"{"path":"exports/demo.yanshi"}"#),
    ("update_stroke", r#"{"object_id":"L1"}"#),
    ("transform_object", r#"{"object_id":"L1"}"#),
    ("restore_object", r#"{"object_id":"L1"}"#),
    ("move_object", r#"{"object_id":"L1"}"#),
    ("resize_canvas_bounds", r#"{"width":200,"height":100}"#),
    ("scale_canvas", r#"{"scale":2,"anchor_name":"center"}"#),
    ("rotate_canvas", r#"{"degrees":90,"anchor_name":"center"}"#),
    (
        "flip_canvas",
        r#"{"direction":"horizontal","anchor_name":"center"}"#,
    ),
    (
        "crop_canvas",
        r#"{"region":{"x":10,"y":10,"w":100,"h":50}}"#,
    ),
    (
        "extend_canvas",
        r#"{"margins":{"left":10,"top":10,"right":10,"bottom":10}}"#,
    ),
    ("revert", r#"{"atom_id":"atom1"}"#),
    ("reapply", r#"{"atom_id":"atom1"}"#),
    ("revert_to", r#"{"atom_id":"atom1"}"#),
    ("restore_checkpoint", r#"{"checkpoint_id":"cp1"}"#),
    ("update_annotation", r#"{"annotation_id":"ann1"}"#),
    ("resolve_annotation", r#"{"annotation_id":"ann1"}"#),
    ("reject_annotation", r#"{"annotation_id":"ann1"}"#),
    ("remove_from_group", r#"{"group_id":"g1","object_id":"L1"}"#),
    (
        "link_to_master",
        r#"{"instance_id":"inst1","master_id":"L1"}"#,
    ),
    ("update_override", r#"{"instance_id":"inst1"}"#),
    ("reject_suggestion", r#"{"suggestion_id":"sug1"}"#),
    ("accept_suggestions", r#"{"suggestion_ids":["sug1"]}"#),
    ("reject_suggestions", r#"{"suggestion_ids":["sug1"]}"#),
    ("list_suggestions", r#"{}"#),
    ("preview_suggestion", r#"{}"#),
    ("list_brushes", r#"{}"#),
    // **取值写在参数说明里的那一批**（第 294 轮）：`op` ∈ reverse|close|join|merge|split、
    // `policy` ∈ all|none、`mode` ∈ normal|multiply|screen|overlay|darken|lighten|add、
    // `delta` = {dx,dy}、`source_region` / `area` = {x,y,w,h}、`points` = [[x,y],…]。
    // **颜色一律用对象** {r,g,b,a} —— **不能用 "#rrggbb"**：`"#` 会**提前结束** `r#"…"#` 原始字符串。
    // 仍不写的 7 个：`add_adjustment`/`add_filter`（"见工具说明"）、`set_reference`（占位哈希）、
    // `set_property`（key 未知）、`import_psd`（要先有 blob）、`suggest`（说明被截断）、`set_preferences`（键未知）。
    ("reorder_layers", r#"{"order":["L1"]}"#),
    ("reorder_objects", r#"{"layer_id":"L1","order":["o1"]}"#),
    ("update_object", r#"{"object_id":"L1","patch":{}}"#),
    (
        "submit_offline",
        r#"{"atoms":[{"kind":"draw_stroke","payload":{}}]}"#,
    ),
    ("resample", r#"{"object_id":"L1"}"#),
    ("path_edit", r#"{"op":"reverse","object_id":"L1"}"#),
    (
        "update_sync_policy",
        r#"{"instance_id":"inst1","policy":"all"}"#,
    ),
    (
        "set_group_transform",
        r#"{"group_id":"g1","delta":{"dx":10,"dy":0}}"#,
    ),
    ("replace_object_data", r#"{"object_id":"L1","data":{}}"#),
    ("update_adjustment", r#"{"object_id":"L1","params":{}}"#),
    ("update_filter", r#"{"object_id":"L1","params":{}}"#),
    (
        "liquify_push",
        r#"{"layer_id":"L1","points":[[10,10]],"direction":[10,0]}"#,
    ),
    ("liquify_twirl", r#"{"layer_id":"L1","points":[[10,10]]}"#),
    ("liquify_pinch", r#"{"layer_id":"L1","points":[[10,10]]}"#),
    ("smudge", r#"{"layer_id":"L1","points":[[10,10]]}"#),
    (
        "patch",
        r#"{"layer_id":"L1","source_region":{"x":0,"y":0,"w":20,"h":20},"target":[40,40]}"#,
    ),
    (
        "scatter_strokes",
        r#"{"layer_id":"L1","seed":1,"area":{"x":0,"y":0,"w":100,"h":100},"palette":[{"r":255,"g":0,"b":0,"a":255}],"brush":"100%_Opaque"}"#,
    ),
    (
        "save_palette",
        r#"{"name":"my.gpl","colors":[{"r":255,"g":0,"b":0,"a":255}]}"#,
    ),
    ("import_asset", r#"{"kind":"brush","name":"my.myb"}"#),
    (
        "set_brush_dynamics",
        // **倍率必须 > 0** ✓（第 515 轮 ✓）：原示例写的是 `[[0,0],[1,1]]` ✗ —— "起点倍率 0" 被产品拒掉 ✓
        // （实测报文：「size_pressure 的倍率必须 > 0（收到 0）」✓）⇒ 改成 **0.2** ✓：
        // **轻按时出细线、按满时到 1** ✓ —— 这是一条真实可照抄的压力曲线 ✓（spec：[[输入值, 倍率], …] ✓）。
        r#"{"brush":"100%_Opaque","curve":{"size_pressure":[[0,0.2],[1,1]]}}"#,
    ),
    ("get_brush_dynamics", r#"{"brush":"100%_Opaque"}"#),
    (
        "set_brush_curve",
        // 原始偏移口径：hardness 随 pressure 从 -0.2 到 +0.2（.myb 里的原值，不是倍率）。
        r#"{"brush":"100%_Opaque","setting":"hardness","input":"pressure","points":[[0,-0.2],[1,0.2]]}"#,
    ),
    (
        "apply_curve_preset",
        r#"{"brush":"100%_Opaque","shape":"ease-in"}"#,
    ),
    // **无参数工具**（✓，第 404 轮批量补 ✓）：这些 spec 的 `params` 是空数组 ✓
    // ⇒ **示例只可能是"成功"或"需要前置状态"** ✓，**不可能因缺参被拒** ✗。
    //（`commit_changeset` / `commit_transaction` 之类确实需要先有开启的变更集 ✓ ⇒ 归"需要前置状态"是**合理**的 ✓）
    // **`set_property`**（✓，第 441 轮 ✓）：它的"必填"有一部分**只写在实现里** ✗ ——
    // `param!` 把 `layer_id`/`object_id` 都标成可选 ✓，而写入器要求**二者必居其一** ✓
    //（`tools.rs:7907` ✓）⇒ **照 spec 写会被拒** ✗ ⇒ 这里按实现写 ✓。
    // `layer_default` 是默认图层 id ✓（第 349 轮那条羽化判据实际用过 ✓，有证据 ✓）。
    (
        "set_property",
        r#"{"layer_id":"layer_default","key":"visible","value":true}"#,
    ),
    // **`texture_background`**（✓，第 456 轮 ✓）：唯一必填参数是纹理名 ✓，
    // 取值域来自仓库里的真实文件 `assets/textures/`（Paper001.png ✓ / Cardboard001.png ✓ …）⇒
    // **不是编的** ✓。`mode` 缺省是 tile ✓ ⇒ 只给必填项就成立 ✓。
    ("texture_background", r#"{"texture":"Paper001.png"}"#),
    // **`rebackground`**：唯一必填参数是目标底色 ✓（与 new_document background 同形 ✓）⇒ 只给必填项就成立 ✓。
    (
        "rebackground",
        r#"{"color":{"r":30,"g":60,"b":120,"a":255}}"#,
    ),
    // **`toned_ground`**（反馈 §5.3）：唯一必填参数是颜色 ✓（与 new_document background 同形 ✓）⇒ 只给必填项就成立 ✓。
    (
        "toned_ground",
        r#"{"color":{"r":210,"g":190,"b":160,"a":255}}"#,
    ),
    // **`set_preferences`**（✓，第 459 轮 ✓）：唯一必填参数是 values（一个键值对对象 ✓，
    // 键是自由的 ✓）；spec 明说「值为 null 表示删除该键」✓、且是**合并**（没提到的键不动 ✓）⇒
    // 这里用一个**有据可查的真键** `reference.blob_hash` ✓（ΔE 写入器读的就是它 ✓，见 tools.rs:3121 ✓），
    // 传 null 表示"确保它不存在" ✓ ⇒ **对任何环境都成立、且不留副作用** ✓。
    // **`add_filter`**（✓，第 460 轮 ✓）：必填两个 ✓ —— `layer_id`（用默认图层 ✓，第 349 轮那条羽化判据
    // 实际用过 ✓）与 `filter_name`（spec 里写"见工具说明" ✗ ⇒ **取值域在渲染层** ✓：
    // `crates/yanshi-render/src/filter.rs` 的解析分支列出了 invert / posterize / blur / sharpen / noise … ✓）。
    // 这里选 **invert**：它**不需要额外参数** ✓ ⇒ 只给两个必填项就成立 ✓；`params` 与 `opacity` 都有缺省 ✓。
    (
        "add_filter",
        r#"{"layer_id":"layer_default","filter_name":"invert"}"#,
    ),
    // **`add_adjustment`**（✓，第 461 轮 ✓）：必填两个 ✓ —— `layer_id`（默认图层 ✓）与 `adjustment_type`
    // （**取值域 spec 自己就列了** ✓：brightness_contrast / saturation / vibrance / invert / levels /
    // exposure / white_balance / curves / hsl / posterize … ✓，且与 `filter.rs:48-54` 的解析分支一致 ✓）。
    // 选 **invert**：与 add_filter 同一个理由 —— **不需要额外参数** ✓ ⇒ 只给两个必填项就成立 ✓。
    (
        "add_adjustment",
        r#"{"layer_id":"layer_default","adjustment_type":"invert"}"#,
    ),
    // **`apply_adjustments`**（包 B）：必填只有 `adjustments` 数组；示例两项都走顶层缺省图层 ✓。
    (
        "apply_adjustments",
        r#"{"layer_id":"layer_default","adjustments":[{"adjustment_type":"exposure","params":{"ev":0.3}},{"adjustment_type":"vibrance","params":{"amount":0.6}}]}"#,
    ),
    // **风格预设系统**：先 list 查 id、再 get/preview 看展开、最后 apply 落成（二次 apply 同参走更新不叠加）。
    ("list_presets", r#"{"tag":"夜景"}"#),
    ("get_preset", r#"{"preset_id":"hong_kong"}"#),
    (
        "preview_preset",
        r#"{"preset_id":"hong_kong","strength":0.6,"knobs":{"teal":0.3}}"#,
    ),
    (
        "apply_preset",
        r#"{"preset_id":"hong_kong","layer_id":"layer_default","strength":0.8}"#,
    ),
    // **`suggest`**（✓）：唯一必填参数 `patch` 是**非空的工具步骤数组** ✓ ⇒
    // 这里填一步 `add_filter` ✓，**参数就是上面那条示例里验证过的那组** ✓（`invert` 不需要额外参数 ✓）。
    // ⇒ 这条示例**自足** ✓：照抄它能真的提交一条建议 ✓。
    (
        "suggest",
        r#"{"patch":[{"tool":"add_filter","arguments":{"layer_id":"layer_default","filter_name":"invert"}}],"summary":"把画面反相"}"#,
    ),
    ("set_layer_blend", r#"{"layer_id":"L1","mode":"multiply"}"#),
    // **`collect_diagnostics`** ✓：无必填参数 ✓；示例里关掉缩略图 ✓ ⇒
    // 照抄它得到的是**小而定**的包 ✓（不把一张可能的 1 MiB PNG 塞进结果 ✓）。
    ("collect_diagnostics", r#"{"include_thumbnail":false}"#),
    // **补三条示例**（第 1135 轮：(B)① 的收尾 ⇒ 让覆盖达到 141/141）。
    // 三个工具此前没有示例：删除文档、导入 PSD、设置参考图。
    // 它们都是"用户会问怎么用"的工具，而且各自的坑都写在下面对应的注释里。
    (
        "delete_document",
        // ⚠️ `document_id` 与 `doc_id` **不是一回事**：`doc_id` 是本次调用的会话文档（框架按需创建），
        // `document_id` 才是要被删掉的那一份。所以这条示例**必须显式写 document_id**。
        r##"{"document_id": "sample-oil"}"##,
    ),
    // **release/reopen 薄包装**：参数面只有一个必填 document_id，示例照抄即可；
    // 前置状态是「磁盘上已有这份文档」（先 new_document 建一份，再 release 掉）。
    ("release_document", r#"{"document_id":"sample-oil"}"#),
    ("reopen_document", r#"{"document_id":"sample-oil"}"#),
    (
        "export_psd_layers",
        // **★ 分层导出 ✗ ★**（**第 253 轮 ✓）：**把当前文档写成**带图层结构的 PSD**
        //   （**图层／组／不透明度／可见性／混合模式 ＋ 压平合成图 ✓）★
        r##"{"filename": "yanshi-export.psd"}"##,
    ),
    (
        "import_psd_layers",
        // **★ 图层级导入 ✗ ★**（**第 253 轮 ✓）：**同一份 PSD ✗ 它按**图层树**建 yanshi 的
        //   图层与图层组**（**图层名／不透明度／可见性／混合模式／位置都对着搬 ✓）★
        //   ＋ **∴ 与 `import_psd` 的分工 ✗**：**那条**只要合成图**（**一个图层 ✓）★
        r##"{"blob_hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000", "name_prefix": "PSD·"}"##,
    ),
    (
        "import_psd",
        // **blob 先行**：先把 PSD 字节传上去（用 `stage_blob` 得到 `blob_hash`），再让它落到某个图层 ✓。
        // 这里给的是**形状合法**的示例值（`sha256:` ＋ 64 位十六进制 ✓，全 0 ⇒ 一眼看出是示例 ✓）：
        // 原先写的是占位文本 `sha256:<先上传 PSD 得到的哈希>` ⇒ **形状不合法** ⇒ 抄走必然失败 ✗
        //（2026-10-06 判据实测：`blob_hash` 不是合法哈希 ✓）⇒ 违反「示例要**真的可照抄**」 ✓。
        r##"{"layer_id": "layer_default", "blob_hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000"}"##,
    ),
    // **stroke_heatmap**（反馈 5.6）：唯一必填参数是 region（与 render_region 同形）⇒ 只给必填项就成立 ✓。
    (
        "stroke_heatmap",
        r#"{"region":{"x":0,"y":0,"w":400,"h":300}}"#,
    ),
];
