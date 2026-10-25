//! **P2-5：`gradient_blend` 的颜色三臂同口径**（期 2 块 2）
//!
//! **⇒ 缺陷**：`write_gradient_blend` 里的 `rgb_of` 自写了一套只认对象与 `#rrggbb` 的解析 ⇒
//!   数组色（全引擎通用写法 `[235,215,200]`）被拒（`invalid_argument`）✗，且 `#RRGGBBAA` 与
//!   数组 4 元的 alpha 被静默丢掉（`alpha_of` 只读对象 `a` 键）✗。
//!
//! **⇒ 判据**：同一对颜色用数组／对象／hex 三种写法各画一次 ⇒
//!   `first_color`／`last_color` 三臂**逐字相同** ✓；`#RRGGBBAA` 的 alpha 进子笔颜色 ✓。
//! **⇒ 变异**：把 `rgba_of` 换回只认对象+`#rrggbb` ⇒ 数组臂 `invalid_argument` 红 ✓；
//!   alpha 改回只读对象 `a` ⇒ hex-8 臂的 `first_stroke.color.a` 不是 128 红 ✓。
use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

/// **跑完即删临时目录** ✓（第 817 轮；`/tmp` 是 1.9 GB tmpfs ✓，夹具留着会写满 ✓）
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

/// 落盘工作区 + 内置资产目录指向**仓库里的 `assets/`** ✓（要画**真笔刷** ✓）。
fn workspace(root: &std::path::Path, doc_id: &str) -> Workspace {
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut workspace = Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default())
        .expect("落盘工作区")
        .with_assets_dir(Some(assets));
    workspace
        .create_document(NewDocument::new(doc_id, 300, 200), "human:1", "s")
        .expect("create_document");
    workspace
}

fn call(
    workspace: &mut Workspace,
    doc_id: &str,
    tool: &str,
    args: serde_json::Value,
) -> serde_json::Value {
    let mut ctx = ToolContext::new(workspace, doc_id, "human:1", "s").with_owner(true);
    registry().call(&mut ctx, tool, &args)
}

fn blend(
    workspace: &mut Workspace,
    doc_id: &str,
    layer_id: &str,
    from: serde_json::Value,
    to: serde_json::Value,
) -> serde_json::Value {
    assert_eq!(
        call(
            workspace,
            doc_id,
            "create_layer",
            json!({"layer_id": layer_id, "name": layer_id}),
        )["ok"],
        json!(true)
    );
    call(
        workspace,
        doc_id,
        "gradient_blend",
        json!({
            "layer_id": layer_id,
            "from": {"x": 40, "y": 100, "color": from},
            "to": {"x": 260, "y": 100, "color": to},
            "brush": "2B_pencil", "size": 24, "steps": 5,
        }),
    )
}

#[test]
fn gradient_blend_accepts_array_object_and_hex_colors() {
    let root = std::env::temp_dir().join(format!("yanshi_gb_color_{}", std::process::id()));
    let _cleanup_1 = Cleanup(root.clone());
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("建根");
    let mut workspace = workspace(&root, "doc_gb");
    let array = blend(
        &mut workspace,
        "doc_gb",
        "L_array",
        json!([235, 215, 200]),
        json!([40, 80, 160]),
    );
    assert_eq!(array["ok"], json!(true), "数组色不许再拒：{array}");
    let object = blend(
        &mut workspace,
        "doc_gb",
        "L_object",
        json!({"r": 235, "g": 215, "b": 200}),
        json!({"r": 40, "g": 80, "b": 160}),
    );
    assert_eq!(object["ok"], json!(true), "{object}");
    let hex = blend(
        &mut workspace,
        "doc_gb",
        "L_hex",
        json!("#ebd7c8"),
        json!("#2850a0"),
    );
    assert_eq!(hex["ok"], json!(true), "{hex}");
    for (name, result) in [("数组", &array), ("对象", &object), ("hex", &hex)] {
        assert_eq!(
            result["first_color"],
            json!("#ebd7c8"),
            "{name}臂首笔颜色必须是 from：{result}"
        );
        assert_eq!(
            result["last_color"],
            json!("#2850a0"),
            "{name}臂末笔颜色必须是 to：{result}"
        );
    }
    assert_eq!(
        array["first_color"], object["first_color"],
        "数组与对象两臂首笔颜色必须相同"
    );
    assert_eq!(
        array["first_color"], hex["first_color"],
        "数组与 hex 两臂首笔颜色必须相同"
    );
    assert_eq!(
        array["last_color"], hex["last_color"],
        "数组与 hex 两臂末笔颜色必须相同"
    );
}

#[test]
fn gradient_blend_keeps_hex_alpha_in_sub_strokes() {
    let root = std::env::temp_dir().join(format!("yanshi_gb_alpha_{}", std::process::id()));
    let _cleanup_1 = Cleanup(root.clone());
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("建根");
    let mut workspace = workspace(&root, "doc_gb_a");
    let result = blend(
        &mut workspace,
        "doc_gb_a",
        "L",
        json!("#ebd7c880"),
        json!("#2850a0ff"),
    );
    assert_eq!(result["ok"], json!(true), "{result}");
    assert_eq!(
        result["first_color"],
        json!("#ebd7c8"),
        "RGB 仍是 from 的色：{result}"
    );
    assert_eq!(
        result["first_stroke"]["color"]["a"].as_f64(),
        Some(128.0),
        "8 位 hex 的 alpha（0x80=128）必须进子笔颜色，不许静默丢掉：{result}"
    );
}
