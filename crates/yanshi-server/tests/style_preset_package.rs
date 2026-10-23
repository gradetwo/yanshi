//! 风格预设系统集成测试（设计 `docs/design/style-preset-system.md`）。
//!
//! 断言口径与 `photo_adjust_package.rs` 一致：精确数值/恒等与非恒等关系，
//! 不许只断「不报错」。像素断言只断**方向/非恒等**，不断绝对像素值（跨机器稳定）。

use serde_json::{json, Map, Value};
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

/// 内置首批预设 id 全表（与 `crates/yanshi-server/data/style-presets.json` 对账；
/// 数据文件少一套或改名，这里先红）。
const PRESET_IDS: [&str; 22] = [
    "japanese_airy",
    "ccd",
    "japanese_film",
    "retro_film",
    "hong_kong",
    "korean",
    "western_cool",
    "cinema",
    "french",
    "forest",
    "morandi",
    "dark_moody",
    "black_white",
    "cyberpunk",
    "y2k",
    "guofeng",
    "ins_minimal",
    "sweet",
    "commercial",
    "hdr_landscape",
    "lomo",
    "pastoral_cream",
];

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn make_registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace, doc: &str) -> ToolContext<'a> {
    ToolContext::new(workspace, doc, "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn call(workspace: &mut Workspace, doc: &str, tool: &str, args: Value) -> Value {
    let registry = make_registry();
    let mut ctx = context(workspace, doc);
    registry.call(&mut ctx, tool, &args)
}

/// 建 doc（黑底）+ 图层 L + 整幅填充 [160,150,140]（中等明度、带色，多数风格对它非恒等）。
fn doc_with_fill(workspace: &mut Workspace, doc: &str) {
    workspace
        .create_document(
            NewDocument::new(doc, 16, 16)
                .with_background(json!({"r": 0, "g": 0, "b": 0, "a": 255})),
            "human:1",
            "session:test",
        )
        .unwrap();
    let created = call(workspace, doc, "create_layer", json!({"layer_id": "L"}));
    assert_eq!(created["ok"], json!(true), "{created}");
    let filled = call(
        workspace,
        doc,
        "fill_region",
        json!({
            "layer_id": "L",
            "shape": {"type": "rect", "x": 0, "y": 0, "w": 16, "h": 16},
            "color": [160, 150, 140],
            "opacity": 1.0
        }),
    );
    assert_eq!(filled["ok"], json!(true), "{filled}");
}

fn center_pixel(workspace: &mut Workspace, doc: &str) -> Vec<i64> {
    let sample = call(workspace, doc, "sample_color", json!({"x": 8, "y": 8}));
    assert_eq!(sample["ok"], json!(true), "{sample}");
    sample["rgba"]
        .as_array()
        .expect("sample_color 应回 rgba")
        .iter()
        .map(|v| v.as_i64().expect("rgba 分量应为整数"))
        .collect()
}

fn effect_count(workspace: &mut Workspace, doc: &str) -> usize {
    let effects = call(workspace, doc, "list_effects", json!({"layer_id": "L"}));
    assert_eq!(effects["ok"], json!(true), "{effects}");
    effects["count"].as_u64().expect("list_effects 应回 count") as usize
}

fn error_detail(result: &Value) -> String {
    result["context"]["detail"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

/// ① 22 套内置预设全部能展开、落成对象，且对测试填充非恒等（逐套核）。
/// 例外说明：本测试的填充是**平坦单色**，纯空间类效果（暗角中心、颗粒的块效应）
/// 在中心像素可能不动，但每套都至少含一笔全局影调/色彩操作 ⇒ 中心像素必须变；
/// 若某套未来改成纯空间配方，这里会红，届时应按设计文档口径单独说明而不是放宽断言。
#[test]
fn all_22_presets_apply_land_objects_and_change_pixels() {
    let mut ws = workspace();
    doc_with_fill(&mut ws, "doc_base");
    let base = center_pixel(&mut ws, "doc_base");
    assert_eq!(base, vec![160, 150, 140, 255], "填充色渲染应逐字节还原");

    // 先对账清单：list_presets 必须恰好给出这 22 套。
    let listed = call(&mut ws, "doc_base", "list_presets", json!({}));
    assert_eq!(listed["ok"], json!(true), "{listed}");
    assert_eq!(listed["count"], json!(22), "内置预设必须是 22 套：{listed}");
    let listed_ids: Vec<&str> = listed["presets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["preset_id"].as_str().unwrap())
        .collect();
    for id in PRESET_IDS {
        assert!(listed_ids.contains(&id), "list_presets 缺少 {id}");
    }
    // 按标签筛选真的生效（夜景子集非空且每项都带该标签）。
    let night = call(&mut ws, "doc_base", "list_presets", json!({"tag": "夜景"}));
    let night_presets = night["presets"].as_array().unwrap();
    assert!(!night_presets.is_empty(), "夜景标签不该为空集");
    assert!(
        night_presets
            .iter()
            .all(|p| p["tags"].as_array().unwrap().iter().any(|t| t == "夜景")),
        "标签筛选结果里混入了不带夜景标签的预设：{night}"
    );

    for (i, id) in PRESET_IDS.iter().enumerate() {
        let doc = format!("doc_p{i}");
        doc_with_fill(&mut ws, &doc);
        let preview = call(&mut ws, &doc, "preview_preset", json!({"preset_id": id}));
        assert_eq!(preview["ok"], json!(true), "预设 {id} 展开失败：{preview}");
        let op_count = preview["op_count"].as_u64().unwrap() as usize;
        assert!(op_count > 0, "预设 {id} 展开为空");

        let applied = call(
            &mut ws,
            &doc,
            "apply_preset",
            json!({"preset_id": id, "layer_id": "L"}),
        );
        assert_eq!(applied["ok"], json!(true), "预设 {id} 落成失败：{applied}");
        assert_eq!(
            applied["created"].as_u64().unwrap() as usize,
            op_count,
            "预设 {id} 首次应用必须新建全部展开对象"
        );
        assert_eq!(
            applied["updated"].as_u64().unwrap(),
            0,
            "预设 {id} 首次应用不该更新任何对象"
        );
        assert_eq!(
            effect_count(&mut ws, &doc),
            op_count,
            "预设 {id} 落成对象数必须等于展开笔数"
        );
        assert_ne!(
            center_pixel(&mut ws, &doc),
            base,
            "预设 {id} 在 strength=1 下不应对测试填充恒等（配方被偷换/未生效？）"
        );
    }
}

/// ② strength=0：风格段恒等 ⇒ 渲染与「只施修正段」逐像素一致；
/// 且与原图不同（证明修正段确实落了，不是整组空转）。
#[test]
fn strength_zero_style_segment_is_identity_over_correction() {
    // 实验组：整套预设 strength=0（含修正段）。
    let mut ws_a = workspace();
    doc_with_fill(&mut ws_a, "doc_a");
    let applied = call(
        &mut ws_a,
        "doc_a",
        "apply_preset",
        json!({"preset_id": "japanese_airy", "layer_id": "L", "strength": 0.0}),
    );
    assert_eq!(applied["ok"], json!(true), "{applied}");
    let pixel_full = center_pixel(&mut ws_a, "doc_a");

    // 对照组：按 preview 的展开清单，只把修正段逐笔用既有工具落成。
    let mut ws_b = workspace();
    doc_with_fill(&mut ws_b, "doc_b");
    let base = center_pixel(&mut ws_b, "doc_b");
    let preview = call(
        &mut ws_b,
        "doc_b",
        "preview_preset",
        json!({"preset_id": "japanese_airy", "strength": 0.0}),
    );
    assert_eq!(preview["ok"], json!(true), "{preview}");
    let ops = preview["ops"].as_array().unwrap();
    let correction_ops: Vec<&Value> = ops
        .iter()
        .filter(|op| op["segment"] == json!("correction"))
        .collect();
    assert!(!correction_ops.is_empty(), "小清新必须有修正段");
    for op in &correction_ops {
        let (tool, key) = if op["kind"] == json!("adjustment") {
            ("add_adjustment", "adjustment_type")
        } else {
            ("add_filter", "filter_name")
        };
        let added = call(
            &mut ws_b,
            "doc_b",
            tool,
            json!({"layer_id": "L", key: op["type"], "params": op["params"]}),
        );
        assert_eq!(added["ok"], json!(true), "修正段 {op} 落成失败：{added}");
    }
    let pixel_correction_only = center_pixel(&mut ws_b, "doc_b");

    assert_eq!(
        pixel_full, pixel_correction_only,
        "strength=0 时风格段必须恒等：整套渲染应与只施修正段逐像素一致"
    );
    assert_ne!(
        pixel_correction_only, base,
        "修正段本身必须非恒等（否则上面的恒等式是在空转上成立的）"
    );
}

/// ③ 二次应用（改 strength）：object_id 集合不变、对象总数不增、像素与第一次不同。
#[test]
fn second_apply_updates_in_place_not_stacks() {
    let mut ws = workspace();
    doc_with_fill(&mut ws, "doc_re");
    let first = call(
        &mut ws,
        "doc_re",
        "apply_preset",
        json!({"preset_id": "hong_kong", "layer_id": "L", "strength": 0.5}),
    );
    assert_eq!(first["ok"], json!(true), "{first}");
    let first_ids: Vec<String> = first["created_object_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    let count_first = effect_count(&mut ws, "doc_re");
    assert_eq!(count_first, first_ids.len());
    let pixel_first = center_pixel(&mut ws, "doc_re");

    let second = call(
        &mut ws,
        "doc_re",
        "apply_preset",
        json!({"preset_id": "hong_kong", "layer_id": "L", "strength": 1.0}),
    );
    assert_eq!(second["ok"], json!(true), "{second}");
    assert_eq!(
        second["created"].as_u64().unwrap(),
        0,
        "二次应用不许新建对象（叠加）：{second}"
    );
    assert_eq!(
        second["updated"].as_u64().unwrap() as usize,
        first_ids.len(),
        "二次应用必须把全部对象走更新路径"
    );
    let second_ids: Vec<String> = second["updated_object_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(second_ids, first_ids, "二次应用的 object_id 集合必须不变");
    assert_eq!(
        effect_count(&mut ws, "doc_re"),
        count_first,
        "二次应用后对象总数不许增加"
    );
    assert_ne!(
        center_pixel(&mut ws, "doc_re"),
        pixel_first,
        "strength 0.5 → 1.0 的更新必须真的改参数（像素要变）"
    );
}

/// ④ 未知预设 / 未知旋钮 / strength 越界 / 旋钮越界：都报错，且错误文本含具体名字/值；
/// 且失败后文档里不许留下任何预设对象（全量校验、整组不落）。
#[test]
fn invalid_inputs_error_with_names_and_commit_nothing() {
    let mut ws = workspace();
    doc_with_fill(&mut ws, "doc_err");

    let unknown = call(
        &mut ws,
        "doc_err",
        "apply_preset",
        json!({"preset_id": "no_such_preset", "layer_id": "L"}),
    );
    assert_eq!(unknown["ok"], json!(false), "{unknown}");
    assert!(
        error_detail(&unknown).contains("no_such_preset"),
        "未知预设的报错必须写出收到的 id：{unknown}"
    );

    let bad_knob = call(
        &mut ws,
        "doc_err",
        "apply_preset",
        json!({"preset_id": "hong_kong", "layer_id": "L", "knobs": {"no_such_knob": 0.5}}),
    );
    assert_eq!(bad_knob["ok"], json!(false), "{bad_knob}");
    assert!(
        error_detail(&bad_knob).contains("no_such_knob"),
        "未知旋钮的报错必须写出旋钮名：{bad_knob}"
    );

    let bad_strength = call(
        &mut ws,
        "doc_err",
        "apply_preset",
        json!({"preset_id": "hong_kong", "layer_id": "L", "strength": 1.5}),
    );
    assert_eq!(bad_strength["ok"], json!(false), "{bad_strength}");
    assert!(
        error_detail(&bad_strength).contains("1.5"),
        "strength 越界的报错必须写出收到的值：{bad_strength}"
    );

    let knob_range = call(
        &mut ws,
        "doc_err",
        "apply_preset",
        json!({"preset_id": "hong_kong", "layer_id": "L", "knobs": {"teal": 7.0}}),
    );
    assert_eq!(knob_range["ok"], json!(false), "{knob_range}");
    let detail = error_detail(&knob_range);
    assert!(
        detail.contains("teal") && detail.contains('7'),
        "旋钮越界的报错必须写出旋钮名与值：{knob_range}"
    );

    assert_eq!(
        effect_count(&mut ws, "doc_err"),
        0,
        "四次失败调用不许在文档里留下任何对象"
    );
}

/// ⑤ knob 覆盖只改变其目标参数：preview 对比——除目标操作的目标参数外，
/// 全部操作的参数逐项相等；且目标参数确实变了（不是空转）。
#[test]
fn knob_override_changes_only_its_target_param() {
    let mut ws = workspace();
    doc_with_fill(&mut ws, "doc_knob");

    // 从 get_preset 读旋钮的真实映射（不抄写数据，避免数据改名后测试还绿着骗人）。
    let definition = call(
        &mut ws,
        "doc_knob",
        "get_preset",
        json!({"preset_id": "hong_kong"}),
    );
    assert_eq!(definition["ok"], json!(true), "{definition}");
    let knob = &definition["preset"]["knobs"][0];
    let knob_id = knob["id"].as_str().unwrap().to_owned();
    let target_op = knob["targets"][0]["op"].as_u64().unwrap() as usize;
    let target_param = knob["targets"][0]["param"].as_str().unwrap().to_owned();
    let knob_max = knob["max"].as_f64().unwrap();

    let base = call(
        &mut ws,
        "doc_knob",
        "preview_preset",
        json!({"preset_id": "hong_kong"}),
    );
    let mut knob_args = Map::new();
    knob_args.insert(knob_id.clone(), json!(knob_max));
    let moved = call(
        &mut ws,
        "doc_knob",
        "preview_preset",
        json!({"preset_id": "hong_kong", "knobs": Value::Object(knob_args)}),
    );
    assert_eq!(base["ok"], json!(true), "{base}");
    assert_eq!(moved["ok"], json!(true), "{moved}");
    let base_ops = base["ops"].as_array().unwrap();
    let moved_ops = moved["ops"].as_array().unwrap();
    assert_eq!(base_ops.len(), moved_ops.len());
    let mut target_changed = false;
    for (b, m) in base_ops.iter().zip(moved_ops.iter()) {
        assert_eq!(b["object_id"], m["object_id"]);
        let is_target =
            m["segment"] == json!("style") && m["index"].as_u64().unwrap() as usize == target_op;
        let bp = b["params"].as_object().unwrap();
        let mp = m["params"].as_object().unwrap();
        assert_eq!(bp.len(), mp.len(), "旋钮不许改变参数个数");
        for (key, bv) in bp {
            if is_target && key == &target_param {
                if bv != &mp[key] {
                    target_changed = true;
                }
            } else {
                assert_eq!(
                    bv, &mp[key],
                    "旋钮 {knob_id} 只许改风格段第 {target_op} 笔的 {target_param}，\
                     但 {} 的 {key} 也变了",
                    m["object_id"]
                );
            }
        }
    }
    assert!(
        target_changed,
        "旋钮 {knob_id} 拉满后目标参数 {target_param} 必须真的变化"
    );
}
