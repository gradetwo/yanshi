//! **导出重放证明：配方必须记全** ✓（P-2 / X-14）。
//!
//! **为什么有这条测试** ✓：`export_project` 逐 blob 重放证明时，
//! `source` 里缺 `brush_params` / 点列时间 / `color_mixing` 任一项，
//! 重放就画不出同一笔 ⇒ `kept_mismatch` ⇒ 整张位图进包 ✓
//!（实测：油画 `kept_mismatch` 19,910 / 19,943 个 blob，占比 99.8% ✓）。
//!
//! **判据** ✓：① 带 `brush_params` 3/3 的笔 ⇒ 导出 `omitted == 1`、
//! `kept_mismatch == 0`；② 带 4 元点时间 `t=0,100,200` 的笔 ⇒ 同上；
//! ③ 两者组合 ⇒ 同上；④ `update_stroke` 换色后 ⇒ 新 `source` 仍带
//! `brush_params` 与 `times`、旧红与新绿两张位图导出都 `omitted`；⑤ 老笔触（无新字段）⇒
//! 仍按老行为导出（`omitted == 1`，与修复前一致 ✓）。
//!
//! **每条测试的笔都不同** ✓（点位与颜色各不相同）：证明缓存是进程内的，
//! 键含目标哈希 —— 两条测试若画出同一张位图，后一条会命中前一条的证明，
//! 断言就测不到重放本身 ✗（`update_stroke` 那条换色后位图不同，不在此列 ✓）。
//!
//! **恒等式** ✓（跨输入成立）：`kept_mismatch + kept_no_recipe == blob_count`
//!（`blob_count` 只计装进包的；`omitted` 的不装、缺失的也不装 ✓）。
//!
//! **变异**（每条断言各一次，见本期说明）：
//! ① `source` 不记 `brush_params` ⇒ ①③ 红；② 重放不取 `times` ⇒ ②③ 红；
//! ③ `restyle` 重建旧字段清单 ⇒ ④ 红；④ 共享重放改回 `brush_params=None` ⇒ ①②③④ 全红；
//! ⑤ 导出只认净荷顶层 `source`（不认 `data` 下）⇒ ④ 红（新绿判 `no_source`）。

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

/// **落盘工作区 + 仓库里的 `assets/`** ✓ —— `brush_stroke` 要从那儿读 `.myb` 笔刷 ✓。
/// 返回工作区与根目录（包落根目录下 ✓）。
fn workspace(name: &str) -> (Workspace, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("yanshi_p2_proof_{name}_{}", std::process::id()));
    let _cleanup = Cleanup(root.clone());
    let _ = std::fs::remove_dir_all(&root);
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut workspace = Workspace::with_file_store(root.clone(), DocumentSettings::default())
        .expect("落盘工作区应当能建")
        .with_assets_dir(Some(assets));
    workspace
        .create_document(
            NewDocument::new("doc_p2", 320, 240),
            "human:1",
            "session:test",
        )
        .unwrap();
    call(&mut workspace, "create_layer", json!({ "layer_id": "L" }));
    (workspace, root)
}

fn call(workspace: &mut Workspace, tool: &str, args: serde_json::Value) -> serde_json::Value {
    let mut ctx = ToolContext::new(workspace, "doc_p2", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    registry().call(&mut ctx, tool, &args)
}

/// **画一笔 `brush_stroke`** ✓（`100%_Opaque` 不读画布 ⇒ 可重放 ✓）。
fn paint(workspace: &mut Workspace, args: serde_json::Value) -> serde_json::Value {
    let made = call(workspace, "brush_stroke", args);
    assert_eq!(made["ok"], json!(true), "{made}");
    made
}

/// **导出并返回回执** ✓。
fn export(workspace: &mut Workspace, root: &std::path::Path, name: &str) -> serde_json::Value {
    let pack = root.join(format!("{name}.yanshi"));
    let exported = call(
        workspace,
        "export_project",
        json!({ "path": pack.to_string_lossy() }),
    );
    assert_eq!(exported["ok"], json!(true), "{exported}");
    exported
}

/// **从包里读 `atoms.jsonl`** ✓（用系统 `tar` 解，不用自己的解析器读自己的包 ✗）。
fn atoms_of(root: &std::path::Path, name: &str) -> Vec<serde_json::Value> {
    let pack = root.join(format!("{name}.yanshi"));
    let out = std::process::Command::new("tar")
        .args(["-xOf", pack.to_str().unwrap(), "atoms.jsonl"])
        .output()
        .expect("tar 应当能解出 atoms.jsonl");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::str::from_utf8(&out.stdout)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("每条原子都应当是合法 JSON"))
        .collect()
}

/// **包里最后一笔笔触的 `source`** ✓（`update_stroke` 后是合并后的那份 ✓）。
///
/// **原子结构** ✓：笔触原子是 `kind == "import_image"`、`source` 在 `payload` 下；
/// `update_stroke` 的 `supersede` 原子、`source` 在 `payload.data` 下 ✓。
fn stroke_source(root: &std::path::Path, name: &str) -> serde_json::Value {
    let atoms = atoms_of(root, name);
    let found = atoms.iter().rev().find(|a| {
        a["payload"]["source"]["kind"] == json!("brush")
            || a["payload"]["data"]["source"]["kind"] == json!("brush")
    });
    match found {
        Some(atom) => {
            if atom["payload"]["source"]["kind"] == json!("brush") {
                atom["payload"]["source"].clone()
            } else {
                atom["payload"]["data"]["source"].clone()
            }
        }
        None => panic!(
            "包里应当有一笔带 source 的笔触：{}",
            serde_json::to_string(&atoms).unwrap()
        ),
    }
}

/// **回执恒等式** ✓：`blob_count` 只计**装进包的** blob ✓（`omitted` 的不装、
/// 缺失的也不装 ✓）⇒ 装进包的 = 以 `kept_mismatch` / `kept_no_recipe` 留下的 ✓。
fn assert_receipt_identity(receipt: &serde_json::Value) {
    let mismatch = receipt["kept_mismatch"].as_u64().unwrap();
    let no_recipe = receipt["kept_no_recipe"].as_u64().unwrap();
    let total = receipt["blob_count"].as_u64().unwrap();
    assert_eq!(
        mismatch + no_recipe,
        total,
        "kept_mismatch + kept_no_recipe == blob_count：{receipt}"
    );
}

/// **被证明的笔 ⇒ `omitted` 涨到期望数、`kept_*` 全为 0** ✓。
fn assert_proved(receipt: &serde_json::Value, omitted: u64) {
    assert_eq!(receipt["omitted"], json!(omitted), "{receipt}");
    assert_eq!(receipt["kept_mismatch"], json!(0), "{receipt}");
    assert_eq!(receipt["kept_no_recipe"], json!(0), "{receipt}");
    assert_eq!(receipt["kept_missing"], json!(0), "{receipt}");
    assert_receipt_identity(receipt);
}

/// **① 带 `brush_params` 3/3 的笔 ⇒ 导出省略** ✓（X-14 验收）。
#[test]
fn stroke_with_brush_params_exports_as_omitted() {
    let (mut workspace, root) = workspace("params");
    paint(
        &mut workspace,
        json!({"layer_id": "L", "brush": "100%_Opaque",
               "points": [[40.0, 60.0, 1.0], [160.0, 60.0, 1.0], [280.0, 60.0, 0.8]],
               "size": 30, "color": [0.45, 0.38, 0.30],
               "brush_params": {"dabs_per_actual_radius": 3, "dabs_per_basic_radius": 3}}),
    );
    let exported = export(&mut workspace, &root, "params");
    // **配方真的记下来了** ✓（不只是导出碰巧对了 ✓）。
    let source = stroke_source(&root, "params");
    assert_eq!(
        source["brush_params"],
        json!({"dabs_per_actual_radius": 3, "dabs_per_basic_radius": 3}),
        "{source}"
    );
    assert_proved(&exported, 1);
}

/// **② 带 4 元点时间 `t=0,100,200` 的笔 ⇒ 导出省略** ✓（速度敏感的笔刷缺时间即错笔 ✓）。
#[test]
fn stroke_with_point_times_exports_as_omitted() {
    let (mut workspace, root) = workspace("times");
    paint(
        &mut workspace,
        json!({"layer_id": "L", "brush": "100%_Opaque",
               "points": [[40.0, 120.0, 0.6, 0], [160.0, 120.0, 1.0, 100], [280.0, 120.0, 0.6, 200]],
               "size": 24, "color": [0.2, 0.5, 0.7]}),
    );
    let exported = export(&mut workspace, &root, "times");
    let source = stroke_source(&root, "times");
    // **`times` 是原样克隆** ✓（第 4 元素是什么类型就存什么类型 ✓）⇒ 本测试传整数，断言用整数 ✓。
    assert_eq!(source["times"], json!([0, 100, 200]), "{source}");
    assert_proved(&exported, 1);
}

/// **③ `brush_params` + 点列时间组合 ⇒ 导出省略** ✓。
#[test]
fn stroke_with_brush_params_and_times_exports_as_omitted() {
    let (mut workspace, root) = workspace("both");
    paint(
        &mut workspace,
        json!({"layer_id": "L", "brush": "100%_Opaque",
               "points": [[40.0, 180.0, 0.6, 0], [160.0, 180.0, 1.0, 100], [280.0, 180.0, 0.6, 200]],
               "size": 20, "color": [0.7, 0.2, 0.2],
               "brush_params": {"dabs_per_actual_radius": 3, "dabs_per_basic_radius": 3}}),
    );
    let exported = export(&mut workspace, &root, "both");
    let source = stroke_source(&root, "both");
    assert_eq!(
        source["brush_params"],
        json!({"dabs_per_actual_radius": 3, "dabs_per_basic_radius": 3}),
        "{source}"
    );
    assert_eq!(source["times"], json!([0, 100, 200]), "{source}");
    assert_proved(&exported, 1);
}

/// **④ `update_stroke` 换色后，合并的 `source` 仍带配方，导出仍省略** ✓。
#[test]
fn update_stroke_keeps_the_proof() {
    let (mut workspace, root) = workspace("update");
    paint(
        &mut workspace,
        json!({"layer_id": "L", "object_id": "S1", "brush": "100%_Opaque",
               "points": [[40.0, 90.0, 0.6, 0], [160.0, 90.0, 1.0, 100], [280.0, 90.0, 0.6, 200]],
               "size": 26, "color": [1.0, 0.0, 0.0, 1.0],
               "brush_params": {"dabs_per_actual_radius": 3, "dabs_per_basic_radius": 3}}),
    );
    let updated = call(
        &mut workspace,
        "update_stroke",
        json!({ "object_id": "S1", "core": { "color": [0.0, 1.0, 0.0, 1.0] } }),
    );
    assert_eq!(updated["ok"], json!(true), "{updated}");
    let exported = export(&mut workspace, &root, "update");
    // **换色后的新 `source` 仍带配方** ✓（旧重建丢 `brush_params` / `times` ✗）。
    let source = stroke_source(&root, "update");
    assert_eq!(
        source["brush_params"],
        json!({"dabs_per_actual_radius": 3, "dabs_per_basic_radius": 3}),
        "{source}"
    );
    assert_eq!(source["times"], json!([0, 100, 200]), "{source}");
    // **换色产生两张位图**（旧红 + 新绿）⇒ 两张都该被证明（新绿那张的配方在
    // `supersede` 原子的 `data` 下，导出也要认那一处 ✓）；一张错即 `kept_* > 0` ✗。
    assert_proved(&exported, 2);
}

/// **⑤ 老笔触（无新字段）⇒ 仍按老行为导出** ✓（共享重放不得改坏旧路 ✓）。
#[test]
fn legacy_stroke_without_new_fields_still_exports() {
    let (mut workspace, root) = workspace("legacy");
    paint(
        &mut workspace,
        json!({"layer_id": "L", "brush": "100%_Opaque",
               "points": [[40.0, 200.0, 1.0], [160.0, 200.0, 0.8], [280.0, 200.0, 1.0]],
               "size": 18, "color": [0.3, 0.3, 0.3]}),
    );
    let exported = export(&mut workspace, &root, "legacy");
    let source = stroke_source(&root, "legacy");
    assert_eq!(source["brush_params"], json!(null), "{source}");
    assert_eq!(source["times"], json!([null, null, null]), "{source}");
    assert_proved(&exported, 1);
}
