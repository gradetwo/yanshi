//! **`dry_brush` 在小笔宽下必须真的飞白**（报告 1.md #12；映射层绝对像素下限）。
//!
//! **⇒ bug**：映射全是相对量（`offset_by_random = 0.45×v`、`radius_by_random = 0.55×v`），
//!   引擎侧偏移 σ = 相对量 × 半径 ⇒ 3–6 px 笔的抖动只有 0.24–0.6 px，
//!   亚像素被光栅化洗掉 ⇒ `dry_brush` 0.05–0.6 几乎无差别（报告现象）。
//! **⇒ 修法**：映射层加绝对像素下限（偏移 σ ≥ 1.5 px、半径抖动 σ ≥ 1.0 px），
//!   只影响 `dry_brush`，不改引擎通用语义与大笔宽相对量。
//! **⇒ 本测试量什么**：水平一笔的导出光栅里，可见足迹宽（中间列段里
//!   任一像素 ≠ 底色的行跨度）＋核心带露底率＋回执 `ink_mass`。
//!   量测口径是探针实测后定的，有一处反直觉必须写明：固定核心带的
//!   露底率**不能**当飞白信号——抖动越大，dab 越向带外散布，带内反而
//!   被填得更满（1.5 px 下限下 size 4/v0.6 的带露底率只有 0.002，
//!   比 1.0 px 下限下的 0.029 还低）。真正随下限变大的，是足迹宽
//!   （dab 散布把线画宽）与边缘毛糙；眼睛在 4 倍放大下判：1.0 px 下限
//!   的 size 4/v0.6 仍是实心线（判 FAIL），1.5 px 下限下 3 px 干、
//!   4 px 干中带糙、6 px 糙（判过）；size 30 仍实心。
//! **⇒ 判据**（实测值见各断言后注释；变异见末尾）：
//!   ① 足迹宽：size 3/4/6 的 v0.6 足迹都必须明显宽于名义直径
//!   （散布把线画宽——下限的直接后果）；② size 30、0.35 的核心带
//!   必须全盖住（大笔宽保持实心，下限没有把大笔打散；大笔宽设定值
//!   与修前逐字相同由映射层测试钉住）；③ 同尺寸 0.6 的 `ink_mass`
//!   必须明显低于 0.05（取值可区分的轻淡侧；它主要来自不透明度与
//!   dab 密度这两项既有 v 依赖，下限的守门员是 ①，不是它）。
//! **⇒ 变异**：去掉映射层下限 ⇒ ① 的足迹宽塌回名义直径附近 ⇒ 红 ✓
//!   （变异实测值见提交时的盒上记录，不写进断言当靶子）。
//! **⇒ 各 case 用独立工作区与独立文档**（第 916 轮教训：同文档多 case
//!   会互相污染采样；此处五笔若共文档，足迹量的是叠加）。
//! **⇒ 确定性**：引擎 RNG 每支新笔都从种子 1000、相位 0 开始，
//!   同输入同输出（盒上两轮实测数逐字相同）⇒ 阈值可带余量。
use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

/// **跑完即删临时目录**（与 `ink_pixels`／`brush_receipt_export_caliber` 同款）。
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const BG: (u8, u8, u8) = (215, 198, 168);
const CANVAS_W: u32 = 128;
const CANVAS_H: u32 = 64;

/// 一笔水平线（y=32，x 8→120）的量测：足迹宽、核心带露底率、`ink_mass`。
struct Measure {
    footprint_width: u32,
    band_gap: f64,
    ink_mass: f64,
}

fn setup(tag: &str) -> (Workspace, Cleanup) {
    let root = std::env::temp_dir().join(format!("yanshi_drybrush_{tag}_{}", std::process::id()));
    let cleanup = Cleanup(root.clone());
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("建根");
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("assets");
    assert!(assets.is_dir(), "找不到资产根：{}", assets.display());
    let mut workspace = Workspace::with_file_store(root, DocumentSettings::default())
        .expect("落盘工作区")
        .with_assets_dir(Some(assets));
    workspace
        .create_document(
            NewDocument::new("doc_drybrush", CANVAS_W, CANVAS_H)
                .with_background(json!({"r": BG.0, "g": BG.1, "b": BG.2, "a": 255})),
            "human:1",
            "s",
        )
        .expect("create_document");
    (workspace, cleanup)
}

fn call(workspace: &mut Workspace, tool: &str, args: serde_json::Value) -> serde_json::Value {
    let mut ctx = ToolContext::new(workspace, "doc_drybrush", "human:1", "s")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    ToolRegistry::with_profiles(&Profile::ALL).call(&mut ctx, tool, &args)
}

fn stroke_measure(workspace: &mut Workspace, tag: &str, size: f64, dry_brush: f64) -> Measure {
    let receipt = call(
        workspace,
        "brush_stroke",
        json!({
            "layer_id": "layer_default", "object_id": "o_dry",
            "brush": "8B_Pencil#1",
            "points": [[8.0, 32.0, 1.0], [120.0, 32.0, 1.0]],
            "size": size, "dry_brush": dry_brush,
            "color": {"r": 200, "g": 20, "b": 20, "a": 255},
        }),
    );
    assert_eq!(receipt["ok"], json!(true), "{receipt}");
    assert!(
        receipt["painted_pixels"].as_u64().unwrap_or(0) > 0,
        "这笔必须真的落墨，否则量测是空洞的：{receipt}"
    );
    let ink_mass = receipt["ink_mass"].as_f64().expect("ink_mass 必须是数值");
    let path =
        std::env::temp_dir().join(format!("yanshi_drybrush_{tag}_{}.png", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let exported = call(
        workspace,
        "export_png",
        json!({ "path": path.to_string_lossy() }),
    );
    assert_eq!(exported["ok"], json!(true), "{exported}");
    let (width, height, rgba) =
        yanshi_render::png::decode_png(&std::fs::read(&path).expect("导出 PNG 必须落盘"))
            .expect("导出必须是能解码的 PNG");
    let _ = std::fs::remove_file(&path);
    assert_eq!(
        (width, height),
        (CANVAS_W, CANVAS_H),
        "导出尺寸必须是画布尺寸"
    );
    let is_bg = |i: usize| rgba[i] == BG.0 && rgba[i + 1] == BG.1 && rgba[i + 2] == BG.2;
    // 核心带（中心线 ±2 行，避开两头 8 px 笔帽）露底率。
    let mut total = 0u64;
    let mut gaps = 0u64;
    for y in 30..35 {
        for x in 16..113 {
            total += 1;
            if is_bg(((y * width + x) * 4) as usize) {
                gaps += 1;
            }
        }
    }
    // 足迹宽：中间列段里任一像素 ≠ 底色的行跨度。
    let mut min_row = CANVAS_H;
    let mut max_row = 0u32;
    for y in 0..CANVAS_H {
        for x in 16..113 {
            if !is_bg(((y * width + x) * 4) as usize) {
                min_row = min_row.min(y);
                max_row = max_row.max(y);
            }
        }
    }
    Measure {
        footprint_width: max_row + 1 - min_row,
        band_gap: gaps as f64 / total as f64,
        ink_mass,
    }
}

#[test]
fn small_width_dry_brush_leaves_visible_gaps_and_large_width_stays_solid() {
    let (mut ws3, _c3) = setup("s3");
    let m3 = stroke_measure(&mut ws3, "s3", 3.0, 0.6);
    let (mut ws4a, _c4a) = setup("s4a");
    let m4a = stroke_measure(&mut ws4a, "s4a", 4.0, 0.05);
    let (mut ws4b, _c4b) = setup("s4b");
    let m4b = stroke_measure(&mut ws4b, "s4b", 4.0, 0.6);
    let (mut ws6, _c6) = setup("s6");
    let m6 = stroke_measure(&mut ws6, "s6", 6.0, 0.6);
    let (mut ws30, _c30) = setup("s30");
    let m30 = stroke_measure(&mut ws30, "s30", 30.0, 0.35);
    println!(
        "足迹宽：s3/v0.6={}，s4/v0.05={}，s4/v0.6={}，s6/v0.6={}，s30/v0.35={}；\
         s4 ink_mass 比 0.6/0.05={:.3}；s30 带露底率={:.3}",
        m3.footprint_width,
        m4a.footprint_width,
        m4b.footprint_width,
        m6.footprint_width,
        m30.footprint_width,
        m4b.ink_mass / m4a.ink_mass,
        m30.band_gap
    );
    // ① 足迹宽必须明显大于名义直径（散布把线画宽；实测 11/11/12）。
    assert!(
        m3.footprint_width >= 8,
        "size 3、dry_brush 0.6 的足迹宽必须 ≥8，实测 {}",
        m3.footprint_width
    );
    assert!(
        m4b.footprint_width >= 9,
        "size 4、dry_brush 0.6 的足迹宽必须 ≥9，实测 {}",
        m4b.footprint_width
    );
    assert!(
        m6.footprint_width >= 10,
        "size 6、dry_brush 0.6 的足迹宽必须 ≥10，实测 {}",
        m6.footprint_width
    );
    // ② 大笔宽核心带全盖住（实心；实测 0.000）。
    assert!(
        m30.band_gap <= 0.01,
        "size 30、dry_brush 0.35 的核心带露底率必须 ≤0.01（实心），实测 {:.3}",
        m30.band_gap
    );
    // ③ 同尺寸 0.6 比 0.05 轻淡（实测比 0.69）。
    assert!(
        m4b.ink_mass < m4a.ink_mass * 0.85,
        "size 4 时 dry_brush 0.6 的 ink_mass 必须 < 0.05 的 0.85 倍，实测 {:.0} vs {:.0}",
        m4b.ink_mass,
        m4a.ink_mass
    );
}
