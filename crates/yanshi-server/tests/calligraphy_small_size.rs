//! **P2-13：`calligraphy` 小 size 必须落墨**（AA 钳制）。
//!
//! **根因**：`vendor/hokusai-core/src/stroke.rs` 的抗锯齿分支在
//! `current_fadeout < aa_min` 时算 `hardness_new`，小半径下为负 ⇒
//! `draw_dab_default` 在 `hardness <= 0` 时整 dab 拒画 ⇒
//! `calligraphy` 在 size 1/2/3 全不落墨、size 4 起正常（复测/定位已纠正：
//! 阈值是 size<4，不是 size<8）。
//!
//! **修法**：AA 分支守卫由 `if hardness_new < 1.0` 改为
//! `if hardness_new > 0.0 && hardness_new < 1.0`——会变负时跳过 AA 调整，
//! 保留原半径/硬度（小 dab 不做 AA 即可，淡出带比 dab 本身还宽时 AA 无意义；
//! 不把 dab 吹大成 aa_min 半径的软斑，避免改 stroke 宽度）。
//!
//! **判据**：size 1–8 × 短路径（约 20px，3 点，跨 dab 间距）/长路径（约 200px，
//! 3 点）矩阵，每格都必须 `ok:true` 且 `painted_pixels > 0`。
//! 每格用**独立图层**（同一图层上重叠落墨会让后一笔的 painted_pixels 缩水）。
//!
//! **变异**：撤掉守卫（`hardness_new > 0.0 &&` 删掉）⇒ size 1–3 的格红。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

/// **跑完即删临时目录** ✓（照 `tests/brush_stroke.rs` 的 `temp_dir`/`Cleanup` 写法）。
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("yanshi_callig_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path
}

/// 落盘工作区 + 内置资产目录指向**仓库里的 `assets/`** ✓（测的就是**真会发出去的那批笔刷** ✓）。
fn workspace(root: &std::path::Path) -> Workspace {
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut workspace = Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default())
        .expect("落盘工作区应当能建")
        .with_assets_dir(Some(assets));
    workspace
        .create_document(
            NewDocument::new("doc_callig", 400, 200),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn call(workspace: &mut Workspace, tool: &str, args: serde_json::Value) -> serde_json::Value {
    let mut ctx = ToolContext::new(workspace, "doc_callig", "human:1", "session:test");
    registry().call(&mut ctx, tool, &args)
}

#[test]
fn calligraphy_paints_at_sizes_1_to_8_on_short_and_long_paths() {
    let root = temp_dir("matrix");
    let _cleanup = Cleanup(root.clone());
    let mut workspace = workspace(&root);
    // 短路径约 20px、长路径约 200px（3 点；都远跨 dab 间距 ⇒ 0 墨只能是 AA 的锅，
    // 不是“路径太短”——那是另一条分支，与 AA 无关）。
    let paths: [(&str, serde_json::Value); 2] = [
        (
            "short",
            json!([[40.0, 60.0, 0.85], [50.0, 65.0, 0.85], [60.0, 60.0, 0.85]]),
        ),
        (
            "long",
            json!([
                [40.0, 150.0, 0.85],
                [140.0, 155.0, 0.85],
                [240.0, 150.0, 0.85]
            ]),
        ),
    ];
    for size in 1..=8u32 {
        for (name, points) in &paths {
            let layer = format!("L{size}_{name}");
            assert_eq!(
                call(&mut workspace, "create_layer", json!({ "layer_id": layer }),)["ok"],
                json!(true),
                "建层应当成功：{layer}"
            );
            let made = call(
                &mut workspace,
                "brush_stroke",
                json!({
                    "layer_id": layer,
                    "brush": "calligraphy",
                    "size": size,
                    "color": [0.9, 0.2, 0.2, 1.0],
                    "points": points,
                }),
            );
            assert_eq!(
                made["ok"],
                json!(true),
                "calligraphy size={size} × {name} 路径应当成功（P2-13 前 size 1–3 在此拒画）：{made}"
            );
            let painted = made["painted_pixels"].as_u64().unwrap_or(0);
            assert!(
                painted > 0,
                "calligraphy size={size} × {name} 路径必须落墨：{made}"
            );
        }
    }
}
