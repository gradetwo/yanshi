//! 期 4 D3 集成测试：`auto_retouch` 走真实工具注册表 + 落盘工作区。
//!
//! 与单元测试的分工：单元测试钉住估计公式；这里钉住工具落成——
//! 只报告零原子、落成与重跑原地更新、人工层拒绝与 force、默认图层
//! 解析、组白名单与 skipped、非法参数。回测三触点（真实 RAW 照片
//! 的像素级验收）不在此文件：输入在仓库外，见 `auto_backtest.rs`
//!（`YANSHI_AUTO_BACKTEST_DIR` 门控）。

use serde_json::{json, Value};
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("yanshi_auto_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path
}

fn workspace(root: &std::path::Path, doc: &str, width: u32, height: u32) -> Workspace {
    let mut workspace = Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default())
        .expect("落盘工作区应当能建");
    workspace
        .create_document(
            NewDocument::new(doc, width, height),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn call(workspace: &mut Workspace, doc: &str, tool: &str, args: Value) -> Value {
    let mut ctx = ToolContext::new(workspace, doc, "human:1", "session:test").with_owner(true);
    registry().call(&mut ctx, tool, &args)
}

/// 合成源图（64×48，全不透明）：水平渐变 + 高频竖条纹 + 纯色块
///（与 D1 集成测试同图，便于跨工具对照）。
fn synthetic(width: u32, height: u32) -> Vec<u8> {
    let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
    for y in 0..height as usize {
        for x in 0..width as usize {
            let base = (y * width as usize + x) * 4;
            let gradient = (x * 255 / width as usize) as u8;
            let stripe = if (x / 2) % 2 == 0 { 255 } else { 0 };
            let (r, g, b) = if x >= 48 && y >= 32 {
                (200, 30, 30)
            } else if y < 8 {
                (gradient, stripe, 128)
            } else {
                (gradient, 255 - gradient, 90)
            };
            pixels[base] = r;
            pixels[base + 1] = g;
            pixels[base + 2] = b;
            pixels[base + 3] = 255;
        }
    }
    pixels
}

/// 建源图层并导入合成图，返回图层 id。
fn setup_source(workspace: &mut Workspace, doc: &str, width: u32, height: u32) -> String {
    let layer = call(
        workspace,
        doc,
        "create_layer",
        json!({"layer_id": "src", "name": "源图"}),
    );
    assert_eq!(layer["ok"], json!(true), "{layer}");
    let pixels = synthetic(width, height);
    let hash = workspace.put_blob(&pixels).expect("放源图 blob");
    let imported = call(
        workspace,
        doc,
        "import_image",
        json!({
            "layer_id": "src",
            "bitmap": {"blob_hash": hash, "size": pixels.len(), "mime_type": "image/x-yanshi-raw"},
            "region": {"x": 0, "y": 0, "w": width, "h": height},
        }),
    );
    assert_eq!(imported["ok"], json!(true), "{imported}");
    "src".to_owned()
}

fn head_seq(workspace: &Workspace, doc: &str) -> u64 {
    workspace.document(doc).unwrap().state().head_seq
}

fn auto_ids(workspace: &Workspace, doc: &str) -> Vec<String> {
    let state = workspace.document(doc).unwrap().state().clone();
    let mut ids: Vec<String> = state
        .alive_objects()
        .iter()
        .filter(|object| object.id.starts_with("auto_"))
        .map(|object| object.id.clone())
        .collect();
    ids.sort();
    ids
}

/// dry_run 与 strength=0 都只报告、零原子（head_seq 不动、无 auto 对象）。
#[test]
fn auto_retouch_zero_strength_and_dry_run_commit_nothing() {
    let root = temp_dir("dry");
    let doc = "doc_auto_dry";
    let (width, height) = (64, 48);
    let mut workspace = workspace(&root, doc, width, height);
    let src = setup_source(&mut workspace, doc, width, height);
    let before = head_seq(&workspace, doc);

    let dry = call(
        &mut workspace,
        doc,
        "auto_retouch",
        json!({"layer_id": src, "dry_run": true}),
    );
    assert_eq!(dry["ok"], json!(true), "{dry}");
    assert_eq!(dry["applied"], json!([]), "dry_run 时 applied 必须为空");
    assert!(
        dry["planned"]
            .as_array()
            .map_or(false, |items| !items.is_empty()),
        "dry_run 时 planned 必须非空：{dry}"
    );
    assert_eq!(head_seq(&workspace, doc), before, "dry_run 不许写原子");
    assert!(auto_ids(&workspace, doc).is_empty(), "dry_run 不许落对象");

    let zero = call(
        &mut workspace,
        doc,
        "auto_retouch",
        json!({"layer_id": src, "strength": 0.0}),
    );
    assert_eq!(zero["ok"], json!(true), "{zero}");
    assert_eq!(zero["applied"], json!([]), "strength=0 时 applied 必须为空");
    assert!(
        zero["planned"]
            .as_array()
            .map_or(false, |items| !items.is_empty()),
        "strength=0 时 planned 必须非空：{zero}"
    );
    assert_eq!(head_seq(&workspace, doc), before, "strength=0 不许写原子");
    assert!(
        auto_ids(&workspace, doc).is_empty(),
        "strength=0 不许落对象"
    );
}

/// 落成一次 + 重跑：第二跑全部走更新路径，auto 对象数不变、无重复。
#[test]
fn auto_retouch_rerun_updates_in_place() {
    let root = temp_dir("rerun");
    let doc = "doc_auto_rerun";
    let (width, height) = (64, 48);
    let mut workspace = workspace(&root, doc, width, height);
    let src = setup_source(&mut workspace, doc, width, height);

    let first = call(
        &mut workspace,
        doc,
        "auto_retouch",
        json!({"layer_id": src}),
    );
    assert_eq!(first["ok"], json!(true), "{first}");
    let applied_ids: Vec<String> = first["applied"]
        .as_array()
        .expect("applied 应为数组")
        .iter()
        .map(|item| {
            item["object_id"]
                .as_str()
                .expect("applied 项应有 id")
                .to_owned()
        })
        .collect();
    assert!(
        !applied_ids.is_empty(),
        "合成图上至少应落成一个对象：{first}"
    );
    // applied id 互不重复（变异：id 去确定性化 ⇒ 第二跑必多对象 ⇒ 红）。
    let mut sorted = applied_ids.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), applied_ids.len(), "applied id 不许重复");
    assert_eq!(
        auto_ids(&workspace, doc),
        sorted,
        "落成后 auto 对象集应与 applied 一致"
    );

    let second = call(
        &mut workspace,
        doc,
        "auto_retouch",
        json!({"layer_id": src}),
    );
    assert_eq!(second["ok"], json!(true), "{second}");
    let mut updated: Vec<String> = second["updated"]
        .as_array()
        .expect("updated 应为数组")
        .iter()
        .map(|item| item.as_str().expect("updated 项应为字符串").to_owned())
        .collect();
    updated.sort();
    assert_eq!(updated, sorted, "第二跑应全部走更新路径：{second}");
    assert_eq!(
        auto_ids(&workspace, doc),
        sorted,
        "重跑后 auto 对象数不许变"
    );
}

/// 人工调过的层：无 force 拒绝（点名人工对象），有 force 放行。
#[test]
fn auto_retouch_refuses_manual_layer_without_force() {
    let root = temp_dir("manual");
    let doc = "doc_auto_manual";
    let (width, height) = (64, 48);
    let mut workspace = workspace(&root, doc, width, height);
    let src = setup_source(&mut workspace, doc, width, height);
    let manual = call(
        &mut workspace,
        doc,
        "add_adjustment",
        json!({"layer_id": src, "adjustment_type": "exposure", "params": {"ev": 0.2}, "object_id": "manual_1"}),
    );
    assert_eq!(manual["ok"], json!(true), "{manual}");

    let refused = call(
        &mut workspace,
        doc,
        "auto_retouch",
        json!({"layer_id": src}),
    );
    assert_eq!(
        refused["ok"],
        json!(false),
        "人工层无 force 应拒绝：{refused}"
    );
    assert_eq!(
        refused["error_code"],
        json!("precondition_failed"),
        "拒绝应为 precondition_failed：{refused}"
    );

    let forced = call(
        &mut workspace,
        doc,
        "auto_retouch",
        json!({"layer_id": src, "force": true}),
    );
    assert_eq!(forced["ok"], json!(true), "{forced}");
}

/// 默认图层解析：缺省 layer_id 时取按 z 序首个含位图对象的图层；
/// 全文档无位图层时明确拒绝。
#[test]
fn auto_retouch_default_layer_resolution() {
    let root = temp_dir("default");
    let doc = "doc_auto_default";
    let (width, height) = (64, 48);
    let mut workspace = workspace(&root, doc, width, height);
    let empty = call(
        &mut workspace,
        doc,
        "create_layer",
        json!({"layer_id": "empty", "name": "空层"}),
    );
    assert_eq!(empty["ok"], json!(true), "{empty}");
    let src = setup_source(&mut workspace, doc, width, height);

    let result = call(&mut workspace, doc, "auto_retouch", json!({}));
    assert_eq!(result["ok"], json!(true), "{result}");
    assert_eq!(
        result["layer_id"],
        json!(src),
        "缺省应选中含位图的图层而非空层"
    );

    // 全文档无位图层：新建空文档调用 ⇒ 明确拒绝（此处不用 workspace()
    // helper：本函数内已有同名局部变量遮蔽 helper 函数名）。
    let root2 = temp_dir("default_empty");
    let doc2 = "doc_auto_no_bitmap";
    let mut workspace2 =
        Workspace::with_file_store(root2.to_path_buf(), DocumentSettings::default())
            .expect("落盘工作区应当能建");
    workspace2
        .create_document(
            NewDocument::new(doc2, width, height),
            "human:1",
            "session:test",
        )
        .unwrap();
    let refused = call(&mut workspace2, doc2, "auto_retouch", json!({}));
    assert_eq!(refused["ok"], json!(false), "无位图层应拒绝：{refused}");
    assert_eq!(
        refused["error_code"],
        json!("precondition_failed"),
        "无位图层拒绝应为 precondition_failed：{refused}"
    );
}

/// 组白名单：只请求 exposure 时 applied 只含曝光组；其余组逐对象
/// 出现在 skipped 里且原因点名「未请求」。
#[test]
fn auto_retouch_groups_whitelist_and_skipped() {
    let root = temp_dir("groups");
    let doc = "doc_auto_groups";
    let (width, height) = (64, 48);
    let mut workspace = workspace(&root, doc, width, height);
    let src = setup_source(&mut workspace, doc, width, height);

    let result = call(
        &mut workspace,
        doc,
        "auto_retouch",
        json!({"layer_id": src, "groups": ["exposure"]}),
    );
    assert_eq!(result["ok"], json!(true), "{result}");
    for item in result["applied"].as_array().expect("applied 应为数组") {
        assert_eq!(
            item["group"],
            json!("exposure"),
            "只请求曝光组时 applied 不许含他组"
        );
    }
    let skipped = result["skipped"].as_array().expect("skipped 应为数组");
    assert!(
        skipped
            .iter()
            .any(|item| item["object_id"] == json!("auto_white_balance")
                && item["reason"]
                    .as_str()
                    .map_or(false, |reason| reason.contains("未请求"))),
        "未请求的组应逐对象出现在 skipped 里：{result}"
    );
}

/// 非法参数一律报错：strength 越界、未知组、不存在的图层。
#[test]
fn auto_retouch_bad_args_rejected() {
    let root = temp_dir("badargs");
    let doc = "doc_auto_badargs";
    let (width, height) = (64, 48);
    let mut workspace = workspace(&root, doc, width, height);
    let src = setup_source(&mut workspace, doc, width, height);

    let bad_strength = call(
        &mut workspace,
        doc,
        "auto_retouch",
        json!({"layer_id": src, "strength": 2.0}),
    );
    assert_eq!(bad_strength["ok"], json!(false));
    assert_eq!(bad_strength["error_code"], json!("invalid_argument"));

    let bad_group = call(
        &mut workspace,
        doc,
        "auto_retouch",
        json!({"layer_id": src, "groups": ["bogus"]}),
    );
    assert_eq!(bad_group["ok"], json!(false));
    assert_eq!(bad_group["error_code"], json!("invalid_argument"));

    let no_layer = call(
        &mut workspace,
        doc,
        "auto_retouch",
        json!({"layer_id": "no_such_layer"}),
    );
    assert_eq!(no_layer["ok"], json!(false));
    assert_eq!(no_layer["error_code"], json!("reference_not_found"));
}
