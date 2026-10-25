//! RSS 修复 paint-rss 的回归判据（进程内）：一份 4K 全配方文档的渲染侧常驻
//! 缓存必须有界；预算闸的估算必须覆盖实测；关掉重建不得棘轮。
//!
//! 口径：直接读 `Document::renderer_retained_bytes()`（字节数，与机器无关），
//! 不断 RSS（RSS 随机器与分配器变，留给驱动臂 `tool-rss-locate.mjs` 的
//! A1/A2/B/C 四臂去量）。配方复用定位驱动的 R-5 配方（同种子、同笔、
//! 同补丁坐标，笔数减到 8——保留量主要来自导出填满的缓存，与笔数弱相关）。
//! 导出确定性（会话内逐字节相同）由 `export_png` 既有判据守，这里只量保留量。
use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

/// **单文档渲染侧常驻上限（RSS 修复 paint-rss，2026-10-26 实测标定）**：
///
/// 4K 全配方文档（8 笔 + 1 次位图补丁 + 1 次导出）落定后的
/// `renderer_retained_bytes()`。修前同配方约 260MB（定位报告 §1，
/// 驱动臂 A1 实测每文档 +132–321MB，均值 ~260MB）；修后由池大缓冲只留一张
/// （≤132.7MB）+ tile 64MB + 位图 + 区域块等组成。**本数是上限不是均值**：
/// 取实测值上浮约 25% 的余量；若实现改动让它系统性抬高，先修实现、再改数，
/// 不要直接放宽（放宽等于把判据改成安慰剂）。
const MAX_RETAINED_BYTES_PER_DOC: usize = 220 * 1024 * 1024;

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
    let path = std::env::temp_dir().join(format!("yanshi_rss_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn workspace(root: &std::path::Path) -> Workspace {
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default())
        .expect("落盘工作区应当能建")
        .with_assets_dir(Some(assets))
}

fn call(workspace: &mut Workspace, doc: &str, tool: &str, args: serde_json::Value) -> serde_json::Value {
    let mut ctx = ToolContext::new(workspace, doc, "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 30_000);
    registry().call(&mut ctx, tool, &args)
}

/// 驱动同款 LCG（`tool-rss-locate.mjs` 的 `lcg`）。
struct Lcg(u32);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(1664525)
            .wrapping_add(1013904223);
        f64::from(self.0) / 4294967296.0
    }
}

/// 驱动同款笔触几何（种子 1000，尺寸 30–70，6 点/笔，压感 0.3/0.9/0.95/0.9/0.6/0.3）。
fn make_strokes(width: f64, height: f64, count: usize, seed: u32) -> Vec<serde_json::Value> {
    let mut rnd = Lcg(seed);
    let palette = [
        [58, 72, 110],
        [122, 74, 52],
        [74, 96, 58],
        [176, 148, 96],
        [140, 52, 52],
        [52, 96, 110],
    ];
    let pressures = [0.3, 0.9, 0.95, 0.9, 0.6, 0.3];
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let x0 = rnd.next() * width * 0.9 + width * 0.05;
        let y0 = rnd.next() * height * 0.9 + height * 0.05;
        let ang = rnd.next() * std::f64::consts::PI * 2.0;
        let len = (0.25 + rnd.next() * 0.45) * width.min(height);
        let mut points = Vec::with_capacity(6);
        for k in 0..6 {
            let t = f64::from(k) / 5.0;
            let x = (x0 + ang.cos() * len * t + (t * 9.0 + i as f64).sin() * 60.0).round();
            let y = (y0 + ang.sin() * len * t + (t * 7.0 + i as f64).cos() * 60.0).round();
            points.push(json!([x, y, pressures[k]]));
        }
        let color = palette[i % palette.len()];
        out.push(json!({
            "layer_id": "L1",
            "object_id": format!("s{i}"),
            "brush": "Round_Bl.myb",
            "size": (30.0 + rnd.next() * 40.0).round() as i64,
            "color": color,
            "points": points,
            "preview": false,
            "wait_for_render": true,
        }));
    }
    out
}

/// 读一份文档的保留分账（打印用）与总量。
fn retained_breakdown(workspace: &mut Workspace, doc: &str) -> (usize, usize, usize, usize, usize) {
    let document = workspace
        .document_mut(doc)
        .unwrap_or_else(|error| panic!("文档 {doc} 应在场：{error}"));
    let pool = document.buffer_pool_stats();
    let tile = document.cache_stats().used_bytes;
    let bitmap = document.bitmap_cache_stats().bytes;
    let region = document.region_cache_bytes();
    let total = document.renderer_retained_bytes();
    (total, pool.retained_bytes, tile, bitmap, region)
}

/// 建一份 4K 全配方文档并跑到导出落定，返回落定保留量（字节）。
fn build_full_recipe_doc(workspace: &mut Workspace, doc: &str, export_path: &std::path::Path) -> usize {
    workspace
        .create_document(NewDocument::new(doc, 3840, 2160), "human:1", "session:test")
        .unwrap_or_else(|error| panic!("建文档 {doc} 失败：{error}"));
    let out = call(
        workspace,
        doc,
        "create_layer",
        json!({ "layer_id": "L1", "name": "paint" }),
    );
    assert_eq!(out["ok"], json!(true), "建层失败：{out}");
    for (index, stroke) in make_strokes(3840.0, 2160.0, 8, 1000).into_iter().enumerate() {
        let out = call(workspace, doc, "brush_stroke", stroke);
        assert_eq!(out["ok"], json!(true), "第 {index} 笔失败：{out}");
    }
    let (total, pool, tile, bitmap, region) = retained_breakdown(workspace, doc);
    eprintln!(
        "RSS-RETAINED {doc} afterStrokes total={total} pool={pool} tile={tile} bitmap={bitmap} region={region}"
    );
    // 位图补丁（驱动 tex/erase/serase 三选一中的 tex：800×1120 平铺背景）。
    let out = call(
        workspace,
        doc,
        "texture_background",
        json!({
            "layer_id": "L1", "texture": "TileCanvas128.png",
            "region": { "x": 1500, "y": 500, "w": 800, "h": 1120 }, "mode": "tile"
        }),
    );
    assert_eq!(out["ok"], json!(true), "位图补丁失败：{out}");
    let out = call(
        workspace,
        doc,
        "export_png",
        json!({ "path": export_path.to_string_lossy(), "deterministic": true }),
    );
    assert_eq!(out["ok"], json!(true), "导出失败：{out}");
    let (total, pool, tile, bitmap, region) = retained_breakdown(workspace, doc);
    eprintln!(
        "RSS-RETAINED {doc} afterExport total={total} pool={pool} tile={tile} bitmap={bitmap} region={region} pool_large={}",
        workspace
            .document_mut(doc)
            .map(|d| d.buffer_pool_stats().retained_large_buffers)
            .unwrap_or(usize::MAX)
    );
    total
}

/// **4K 全配方文档的渲染侧常驻有界**：落定保留量 ≤ 上限，且预算闸的估算
/// 必须覆盖实测（否则闸对 30 文档轮次永远不触发，见定位报告 §5 第 3 条）。
#[test]
fn full_recipe_doc_retained_is_bounded_and_estimated() {
    let root = temp_dir("retained");
    let _cleanup = Cleanup(root.clone());
    let mut workspace = workspace(&root);
    let export_path = root.join("exports").join("rss-doc-0.png");
    std::fs::create_dir_all(export_path.parent().unwrap()).unwrap();
    let retained = build_full_recipe_doc(&mut workspace, "rss-doc-0", &export_path);
    assert!(
        retained <= MAX_RETAINED_BYTES_PER_DOC,
        "4K 全配方文档落定保留 {retained} 字节，超过上限 {MAX_RETAINED_BYTES_PER_DOC} 字节（修前约 260MB）"
    );
    // 大缓冲只留一张（池规则的端到端回响；单元判据在 buffer_pool.rs）。
    let large = workspace
        .document_mut("rss-doc-0")
        .map(|d| d.buffer_pool_stats().retained_large_buffers)
        .unwrap_or(usize::MAX);
    assert!(
        large <= 1,
        "池里大缓冲只能留 1 张，实测 {large} 张"
    );
    // 预算闸估算必须覆盖实测保留量（估算 = 原子×300KB + 保留量，恒 ≥ 保留量）。
    let infos = workspace.loaded_documents();
    assert_eq!(infos.len(), 1, "应只有一份已载入文档");
    let estimated = infos[0].estimated_bytes as usize;
    assert!(
        estimated >= retained,
        "预算闸估算 {estimated} 必须覆盖实测保留 {retained}，否则 30 文档轮次不触发卸载"
    );
    assert!(
        estimated <= MAX_RETAINED_BYTES_PER_DOC + 64 * 1024 * 1024,
        "估算 {estimated} 不应比上限高出太多（原子口径 + 保留量），否则闸过早卸载"
    );
}

/// **关掉重建不棘轮**：关完已载入估算归零；重建同配方文档，保留量仍在上限内。
#[test]
fn close_then_rebuild_does_not_ratchet() {
    let root = temp_dir("rebuild");
    let _cleanup = Cleanup(root.clone());
    let mut workspace = workspace(&root);
    let export_dir = root.join("exports");
    std::fs::create_dir_all(&export_dir).unwrap();
    let first = build_full_recipe_doc(&mut workspace, "rss-doc-0", &export_dir.join("a.png"));
    assert!(first <= MAX_RETAINED_BYTES_PER_DOC, "首文档保留 {first} 超上限");
    assert!(workspace.close_document("rss-doc-0"), "关闭应成功");
    assert!(
        workspace.loaded_documents().is_empty(),
        "关完不应再有已载入文档"
    );
    let second = build_full_recipe_doc(&mut workspace, "rss-doc-1", &export_dir.join("b.png"));
    assert!(
        second <= MAX_RETAINED_BYTES_PER_DOC,
        "重建文档保留 {second} 超上限（首文档 {first}）——保留量逐轮抬高即未修好"
    );
}
