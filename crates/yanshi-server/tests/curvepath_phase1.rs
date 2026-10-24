//! **曲线/路径一期端到端判据**（详细设计 §7）。
//!
//! - Path 端到端：含孔复合路径（evenodd 挖孔 / nonzero 叠加）渲染与手工展平参照
//!   逐像素比对（AA 带外零差）；开放描边路径与多边形参照只许在闭合边邻域有差。
//! - export_svg：原生 C 命令直出、fill-rule 透传、计数归 shapes_vector、
//!   objects_unsupported 不出现 Path；export_project 往返后重导出逐字节一致。
//! - 分块渲染一致性：整幅 render 与 4 分块 render 逐像素相等。
//! - stroke_path：引用对象 / 内联 / 多子路径成笔、压感剖面在墨迹宽度上可见、
//!   五种错误各带明确信息。

use serde_json::{json, Value};
use yanshi_core::Bbox;
use yanshi_render::path::{PathSeg, Subpath};
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

/// 落盘工作区 + 仓库 assets 目录：brush_stroke 要读真 .myb、export_project 要落盘
/// 原子日志，纯内存工作区两者都做不了（与 brush_stroke.rs / export_project.rs 同款脚手架）。
fn assets_workspace(name: &str) -> Workspace {
    let root = std::env::temp_dir().join(format!(
        "yanshi_curvepath_ws_{}_{}",
        name,
        std::process::id()
    ));
    let _cleanup_1 = Cleanup(root.clone());
    let _ = std::fs::remove_dir_all(&root);
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    Workspace::with_file_store(root, DocumentSettings::default())
        .expect("落盘工作区应当能建")
        .with_assets_dir(Some(assets))
}

fn call(workspace: &mut Workspace, doc: &str, tool: &str, args: &Value) -> Value {
    let mut ctx = ToolContext::new(workspace, doc, "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    registry().call(&mut ctx, tool, args)
}

fn make_doc(workspace: &mut Workspace, doc: &str, width: u32, height: u32) {
    workspace
        .create_document(
            NewDocument::new(doc, width, height),
            "human:1",
            "session:test",
        )
        .unwrap();
    let made = call(workspace, doc, "create_layer", &json!({ "layer_id": "L" }));
    assert_eq!(made["ok"], json!(true), "{made}");
}

/// 先铺一层显式白色背景矩形：参照对比不依赖文档缺省背景是什么。
fn paint_white_background(workspace: &mut Workspace, doc: &str) {
    let made = call(
        workspace,
        doc,
        "draw_shape",
        &json!({
            "layer_id": "L",
            "object_id": "bg",
            "data": {
                "geometry": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 400, "h": 300}},
                "color": {"r": 255, "g": 255, "b": 255, "a": 255}
            }
        }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
}

fn temp_path(name: &str, ext: &str) -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "yanshi_curvepath_{}_{}.{}",
        name,
        std::process::id(),
        ext
    ));
    path
}

fn export_png_pixels(workspace: &mut Workspace, doc: &str, name: &str) -> (u32, u32, Vec<u8>) {
    let path = temp_path(name, "png");
    let _ = std::fs::remove_file(&path);
    let exported = call(
        workspace,
        doc,
        "export_png",
        &json!({ "path": path.to_string_lossy() }),
    );
    assert_eq!(exported["ok"], json!(true), "{exported}");
    let bytes = std::fs::read(&path).expect("PNG 应当落盘");
    if std::env::var_os("YANSHI_CURVEPATH_KEEP_EXPORTS").is_none() {
        let _ = std::fs::remove_file(&path);
    }
    yanshi_render::png::decode_png(&bytes).expect("应当是合法 PNG")
}

fn export_svg_text(workspace: &mut Workspace, doc: &str, name: &str) -> (Value, String) {
    let path = temp_path(name, "svg");
    let _ = std::fs::remove_file(&path);
    let exported = call(
        workspace,
        doc,
        "export_svg",
        &json!({ "path": path.to_string_lossy(), "flatten_opacity": false }),
    );
    assert_eq!(exported["ok"], json!(true), "{exported}");
    let text = std::fs::read_to_string(&path).expect("SVG 应当落盘");
    if std::env::var_os("YANSHI_CURVEPATH_KEEP_EXPORTS").is_none() {
        let _ = std::fs::remove_file(&path);
    }
    (exported, text)
}

/// 外环：圆角矩形风的闭合 blob（4 段 C），顺时针（y 向下口径）。
fn outer_subpath() -> Subpath {
    Subpath {
        start: (60.0, 60.0),
        segments: vec![
            PathSeg::Cubic {
                c1: (140.0, 20.0),
                c2: (260.0, 20.0),
                to: (340.0, 60.0),
            },
            PathSeg::Cubic {
                c1: (380.0, 140.0),
                c2: (380.0, 160.0),
                to: (340.0, 240.0),
            },
            PathSeg::Cubic {
                c1: (260.0, 280.0),
                c2: (140.0, 280.0),
                to: (60.0, 240.0),
            },
            PathSeg::Cubic {
                c1: (20.0, 160.0),
                c2: (20.0, 140.0),
                to: (60.0, 60.0),
            },
        ],
        closed: true,
    }
}

/// 内环：与外环同向的闭合小 blob（挖孔 / 叠加的对照体）。
fn inner_subpath() -> Subpath {
    Subpath {
        start: (160.0, 110.0),
        segments: vec![
            PathSeg::Line { to: (240.0, 110.0) },
            PathSeg::Cubic {
                c1: (265.0, 110.0),
                c2: (265.0, 190.0),
                to: (240.0, 190.0),
            },
            PathSeg::Line { to: (160.0, 190.0) },
            PathSeg::Cubic {
                c1: (135.0, 190.0),
                c2: (135.0, 110.0),
                to: (160.0, 110.0),
            },
        ],
        closed: true,
    }
}

fn subpath_json(sub: &Subpath) -> Value {
    let segments: Vec<Value> = sub
        .segments
        .iter()
        .map(|seg| match seg {
            PathSeg::Line { to } => json!({"op": "L", "to": [to.0, to.1]}),
            PathSeg::Cubic { c1, c2, to } => {
                json!({"op": "C", "c1": [c1.0, c1.1], "c2": [c2.0, c2.1], "to": [to.0, to.1]})
            }
        })
        .collect();
    json!({
        "start": [sub.start.0, sub.start.1],
        "closed": sub.closed,
        "segments": segments,
    })
}

fn draw_compound_path(workspace: &mut Workspace, doc: &str, fill_rule: &str) {
    let made = call(
        workspace,
        doc,
        "draw_shape",
        &json!({
            "layer_id": "L",
            "object_id": "blob",
            "data": {
                "geometry": {
                    "kind": "path",
                    "fill_rule": fill_rule,
                    "subpaths": [subpath_json(&outer_subpath()), subpath_json(&inner_subpath())],
                },
                "color": {"r": 200, "g": 40, "b": 40, "a": 255}
            }
        }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
}

fn draw_polygon(
    workspace: &mut Workspace,
    doc: &str,
    id: &str,
    points: &[(f64, f64)],
    color: Value,
) {
    let pts: Vec<Value> = points.iter().map(|p| json!([p.0, p.1])).collect();
    let made = call(
        workspace,
        doc,
        "draw_shape",
        &json!({
            "layer_id": "L",
            "object_id": id,
            "data": {
                "geometry": {"kind": "polygon", "points": pts},
                "color": color,
            }
        }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
}

fn pixel(rgba: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * width + x) * 4) as usize;
    [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
}

#[test]
fn path_fill_evenodd_matches_flattened_reference_outside_aa_band() {
    // Doc A：path 复合形状（evenodd）。Doc B（参照）：外环展平多边形（红）+ 内环展平多边形（白）。
    // 内环覆盖率饱和（0/1）的像素必须逐字节相同；只有内环 AA 带（0<cov<1）允许不同
    //（带内两条路的合成顺序不同：先红后白 vs 覆盖率相减，是 §7 说的「AA 带外零差」）。
    let mut ws_a = assets_workspace("evenodd_a");
    make_doc(&mut ws_a, "docA", 400, 300);
    paint_white_background(&mut ws_a, "docA");
    draw_compound_path(&mut ws_a, "docA", "evenodd");
    let (w, h, rgba_a) = export_png_pixels(&mut ws_a, "docA", "evenodd_a");

    let outer_flat = yanshi_render::path::flatten_subpath(&outer_subpath(), 0.10);
    let inner_flat = yanshi_render::path::flatten_subpath(&inner_subpath(), 0.10);
    let mut ws_b = assets_workspace("evenodd_b");
    make_doc(&mut ws_b, "docB", 400, 300);
    paint_white_background(&mut ws_b, "docB");
    draw_polygon(
        &mut ws_b,
        "docB",
        "outer",
        &outer_flat,
        json!({"r": 200, "g": 40, "b": 40, "a": 255}),
    );
    draw_polygon(
        &mut ws_b,
        "docB",
        "inner",
        &inner_flat,
        json!({"r": 255, "g": 255, "b": 255, "a": 255}),
    );
    let (_, _, rgba_b) = export_png_pixels(&mut ws_b, "docB", "evenodd_b");
    assert_eq!(rgba_a.len(), rgba_b.len());

    // 内环 AA 带：用覆盖率（与 Path 渲染同一展平、经单环逐位一致性单测锁定）圈定。
    let inner_cov = yanshi_render::geometry::polygon_coverage_clipped(
        &inner_flat,
        4,
        &Bbox::new(0.0, 0.0, 400.0, 300.0),
    );
    let mut diff_outside = 0usize;
    let mut red_pixels = 0usize;
    for y in 0..h {
        for x in 0..w {
            let pa = pixel(&rgba_a, w, x, y);
            if pa[0] > 150 && pa[1] < 100 && pa[2] < 100 {
                red_pixels += 1;
            }
            if pa != pixel(&rgba_b, w, x, y) {
                let cov = inner_cov.at_doc(f64::from(x), f64::from(y));
                if cov <= 0.0 || cov >= 1.0 {
                    diff_outside += 1;
                }
            }
        }
    }
    assert!(red_pixels > 5_000, "红色填充面积应可观，实测 {red_pixels}");
    assert_eq!(
        diff_outside, 0,
        "AA 带外不许有差异像素，实测 {diff_outside} 个"
    );
    // 孔中心必须是背景白（挖孔生效的直接证据）。
    assert_eq!(pixel(&rgba_a, w, 200, 150), [255, 255, 255, 255]);
    // 环带中点是红。
    let ring = pixel(&rgba_a, w, 100, 150);
    assert!(ring[0] > 150 && ring[1] < 100, "环带应为红，实测 {ring:?}");
}

#[test]
fn path_fill_nonzero_matches_outer_only_reference() {
    // nonzero + 同向嵌套 ⇒ 绕向叠加、不挖孔：结果应与只有外环的参照全图逐字节一致。
    let mut ws_a = assets_workspace("nonzero_a");
    make_doc(&mut ws_a, "docA", 400, 300);
    paint_white_background(&mut ws_a, "docA");
    draw_compound_path(&mut ws_a, "docA", "nonzero");
    let (_, _, rgba_a) = export_png_pixels(&mut ws_a, "docA", "nonzero_a");
    // 中心被填（不挖孔）。
    let center = pixel(&rgba_a, 400, 200, 150);
    assert!(
        center[0] > 150 && center[1] < 100,
        "nonzero 中心应填红，实测 {center:?}"
    );

    let outer_flat = yanshi_render::path::flatten_subpath(&outer_subpath(), 0.10);
    let mut ws_b = assets_workspace("nonzero_b");
    make_doc(&mut ws_b, "docB", 400, 300);
    paint_white_background(&mut ws_b, "docB");
    draw_polygon(
        &mut ws_b,
        "docB",
        "outer",
        &outer_flat,
        json!({"r": 200, "g": 40, "b": 40, "a": 255}),
    );
    let (_, _, rgba_b) = export_png_pixels(&mut ws_b, "docB", "nonzero_b");
    assert_eq!(rgba_a, rgba_b, "nonzero 叠加渲染应与外环参照逐字节一致");
}

#[test]
fn path_open_stroke_only_matches_reference_outside_closing_edge() {
    // 开放路径、无 color、只有描边：与「展平多边形（白填充 + 同宽描边）」参照相比，
    // 差异只许出现在多边形多出的那条闭合边（末点→首点的直段）的盖章邻域内。
    let open = Subpath {
        start: (50.0, 200.0),
        segments: vec![PathSeg::Cubic {
            c1: (150.0, 80.0),
            c2: (250.0, 280.0),
            to: (350.0, 200.0),
        }],
        closed: false,
    };
    let mut ws_a = assets_workspace("open_a");
    make_doc(&mut ws_a, "docA", 400, 300);
    paint_white_background(&mut ws_a, "docA");
    let made = call(
        &mut ws_a,
        "docA",
        "draw_shape",
        &json!({
            "layer_id": "L",
            "object_id": "curve",
            "data": {
                "geometry": {"kind": "path", "subpaths": [subpath_json(&open)]},
                "stroke_width": 6.0,
                "stroke_color": {"r": 30, "g": 60, "b": 220, "a": 255}
            }
        }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    let (w, h, rgba_a) = export_png_pixels(&mut ws_a, "docA", "open_a");

    let flat = yanshi_render::path::flatten_subpath(&open, 0.10);
    let mut ws_b = assets_workspace("open_b");
    make_doc(&mut ws_b, "docB", 400, 300);
    paint_white_background(&mut ws_b, "docB");
    let pts: Vec<Value> = flat.iter().map(|p| json!([p.0, p.1])).collect();
    let made = call(
        &mut ws_b,
        "docB",
        "draw_shape",
        &json!({
            "layer_id": "L",
            "object_id": "poly",
            "data": {
                "geometry": {"kind": "polygon", "points": pts},
                "color": {"r": 255, "g": 255, "b": 255, "a": 255},
                "stroke_width": 6.0,
                "stroke_color": {"r": 30, "g": 60, "b": 220, "a": 255}
            }
        }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    let (_, _, rgba_b) = export_png_pixels(&mut ws_b, "docB", "open_b");

    // 蓝色墨量两边都应可观（描边真的画出来了）。
    let blue = |rgba: &[u8]| {
        rgba.chunks_exact(4)
            .filter(|p| p[2] > 150 && p[0] < 120 && p[1] < 140)
            .count()
    };
    assert!(blue(&rgba_a) > 200, "path 描边墨量过少：{}", blue(&rgba_a));
    // 差异像素必须贴着闭合边（y=200 的水平段，x∈[50,350]）6+1 px 内。
    let mut diff_total = 0usize;
    let mut diff_far = 0usize;
    for y in 0..h {
        for x in 0..w {
            if pixel(&rgba_a, w, x, y) != pixel(&rgba_b, w, x, y) {
                diff_total += 1;
                let (px, py) = (f64::from(x), f64::from(y));
                let dist = if (50.0..=350.0).contains(&px) {
                    (py - 200.0).abs()
                } else {
                    let dx = if px < 50.0 { 50.0 - px } else { px - 350.0 };
                    let dy = py - 200.0;
                    (dx * dx + dy * dy).sqrt()
                };
                if dist > 7.0 {
                    diff_far += 1;
                }
            }
        }
    }
    assert!(
        diff_total > 100,
        "闭合边应造成可观差异（参照多画了一条边），实测 {diff_total}"
    );
    assert_eq!(diff_far, 0, "闭合边邻域外不许有差异，实测 {diff_far} 个");
}

#[test]
fn path_export_svg_native_curves_and_project_roundtrip() {
    let mut ws = assets_workspace("roundtrip_a");
    make_doc(&mut ws, "docA", 400, 300);
    paint_white_background(&mut ws, "docA");
    draw_compound_path(&mut ws, "docA", "evenodd");
    let (exported, svg) = export_svg_text(&mut ws, "docA", "roundtrip_a");
    assert!(
        svg.contains(" C "),
        "SVG 必须含原生 C 命令（不展平）：{svg}"
    );
    assert!(
        svg.contains("fill-rule=\"evenodd\""),
        "evenodd 必须透传：{svg}"
    );
    assert!(
        exported["shapes_vector"].as_u64().unwrap_or(0) >= 1,
        "Path 计数应归 shapes_vector：{exported}"
    );
    assert_eq!(
        exported["objects_unsupported"],
        json!(0),
        "Path 不得进 objects_unsupported：{exported}"
    );

    // export_project 往返 ⇒ 重导出逐字节一致（atom 里只存控制点、无损）。
    let pack = temp_path("roundtrip", "yanshi");
    let _ = std::fs::remove_file(&pack);
    let exported_pack = call(
        &mut ws,
        "docA",
        "export_project",
        &json!({"path": pack.to_string_lossy(), "doc_id": "docA", "include_bitmaps": false}),
    );
    assert_eq!(exported_pack["ok"], json!(true), "{exported_pack}");
    let mut ws2 = assets_workspace("roundtrip_b");
    let imported = call(
        &mut ws2,
        "docA",
        "import_project",
        &json!({"path": pack.to_string_lossy(), "doc_id": "docA"}),
    );
    assert_eq!(imported["ok"], json!(true), "{imported}");
    let _ = std::fs::remove_file(&pack);
    let (_, svg2) = export_svg_text(&mut ws2, "docA", "roundtrip_b");
    assert_eq!(svg, svg2, "工程往返后 SVG 应逐字节一致");
    let (_, _, rgba1) = export_png_pixels(&mut ws, "docA", "roundtrip_png_a");
    let (_, _, rgba2) = export_png_pixels(&mut ws2, "docA", "roundtrip_png_b");
    assert_eq!(rgba1, rgba2, "工程往返后 PNG 应逐像素一致");
}

#[test]
fn path_chunked_rendering_matches_full_render() {
    // 整幅渲染 vs 4 分块渲染逐像素相等（覆盖率只取决于像素与几何的关系，与 clip 无关）。
    let mut ws = assets_workspace("chunked");
    make_doc(&mut ws, "docA", 400, 300);
    paint_white_background(&mut ws, "docA");
    draw_compound_path(&mut ws, "docA", "evenodd");
    let (_, _, full) = ws
        .render_region_raw("docA", Bbox::new(0.0, 0.0, 400.0, 300.0))
        .expect("整幅渲染应成功");
    let mut stitched = vec![0u8; full.len()];
    for (qx, qy) in [(0.0, 0.0), (200.0, 0.0), (0.0, 150.0), (200.0, 150.0)] {
        let (qw, qh, quad) = ws
            .render_region_raw("docA", Bbox::new(qx, qy, 200.0, 150.0))
            .expect("分块渲染应成功");
        assert_eq!((qw, qh), (200, 150));
        for row in 0..150usize {
            let src = row * 200 * 4;
            let dst = ((qy as usize + row) * 400 + qx as usize) * 4;
            stitched[dst..dst + 200 * 4].copy_from_slice(&quad[src..src + 200 * 4]);
        }
    }
    assert_eq!(full, stitched, "分块渲染与整幅渲染必须逐像素一致");
}

#[test]
fn stroke_path_end_to_end_profile_visible_in_ink() {
    let mut ws = assets_workspace("stroke_e2e");
    make_doc(&mut ws, "docA", 400, 300);
    // 被引用的 path 对象本身不落墨（无 color、stroke_width 0）。
    let arc = Subpath {
        start: (40.0, 150.0),
        segments: vec![PathSeg::Cubic {
            c1: (140.0, 30.0),
            c2: (260.0, 30.0),
            to: (360.0, 150.0),
        }],
        closed: false,
    };
    let made = call(
        &mut ws,
        "docA",
        "draw_shape",
        &json!({
            "layer_id": "L",
            "object_id": "arc1",
            "data": {"geometry": {"kind": "path", "subpaths": [subpath_json(&arc)]}}
        }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");

    let stroked = call(
        &mut ws,
        "docA",
        "stroke_path",
        &json!({
            "layer_id": "L",
            "path_object_id": "arc1",
            "brush": "2B_pencil",
            "size": 24,
            "color": {"r": 30, "g": 30, "b": 30, "a": 255}
        }),
    );
    assert_eq!(stroked["ok"], json!(true), "{stroked}");
    assert_eq!(stroked["strokes"], json!(1));
    assert_eq!(stroked["skipped_subpaths"], json!(0));
    let points_total = stroked["points_total"].as_u64().unwrap_or(0);
    assert!(points_total > 50, "烘焙点数应可观，实测 {points_total}");
    let length_total = stroked["length_total"].as_f64().unwrap_or(0.0);
    assert!(
        (300.0..400.0).contains(&length_total),
        "弧长应在 300..400，实测 {length_total}"
    );

    // §5.3-5：压感剖面在墨迹上可见——以曲线上三点为圆心数盘内墨量
    //（竖直列穿斜笔会按 1/cosθ 虚增厚度，与笔向无关的盘测才测的是笔宽本身）。
    // 三点取自同一条弧的 Bézier 求值：t=0.06 ≈ (58, 130)、t=0.5 = (200, 82.5)、t=0.94 ≈ (342, 130)。
    let (_, _, rgba) = ws
        .render_region_raw("docA", Bbox::new(0.0, 0.0, 400.0, 300.0))
        .expect("渲染应成功");
    let disk_ink = |cx: f64, cy: f64| {
        let mut mass = 0u64;
        for y in 0..300u32 {
            for x in 0..400u32 {
                let dx = f64::from(x) - cx;
                let dy = f64::from(y) - cy;
                if dx * dx + dy * dy <= 24.0 * 24.0 {
                    let p = pixel(&rgba, 400, x, y);
                    // 暗度质量：背景（白/不透明）记 0，墨越深越大——不用 alpha
                    //（文档背景不透明时 alpha 处处饱和，测不出笔墨）。
                    mass += u64::from(255 - p[0]);
                }
            }
        }
        mass
    };
    let mid = disk_ink(200.0, 82.5);
    let start = disk_ink(58.0, 130.0);
    let end = disk_ink(342.0, 130.0);
    println!("disk darkness mass: start={start} mid={mid} end={end}");
    assert!(mid >= 20_000, "中段满压盘内墨量应可观，实测 {mid}");
    assert!(
        mid * 2 >= start * 3 && mid * 2 >= end * 3,
        "中段（{mid}）应显著粗于两端（起 {start} / 收 {end}）：压感斜坡没有生效"
    );
}

#[test]
fn stroke_path_inline_multi_and_zero_length() {
    let mut ws = assets_workspace("stroke_inline");
    make_doc(&mut ws, "docA", 400, 300);
    // 内联两条子路径 ⇒ 两笔；constant 剖面。
    let stroked = call(
        &mut ws,
        "docA",
        "stroke_path",
        &json!({
            "layer_id": "L",
            "subpaths": [
                {"start": [20, 260], "segments": [{"op": "L", "to": [380, 260]}]},
                {"start": [20, 280], "segments": [{"op": "L", "to": [380, 280]}]}
            ],
            "brush": "2B_pencil",
            "size": 12,
            "color": {"r": 30, "g": 30, "b": 30, "a": 255},
            "pressure_profile": "constant"
        }),
    );
    assert_eq!(stroked["ok"], json!(true), "{stroked}");
    assert_eq!(
        stroked["strokes"],
        json!(2),
        "两个 subpath 应逐个成笔：{stroked}"
    );
    // 零长子路径：跳过并计数，不静默、不报错。
    let skipped = call(
        &mut ws,
        "docA",
        "stroke_path",
        &json!({
            "layer_id": "L",
            "subpaths": [{"start": [10, 10], "segments": [{"op": "L", "to": [10, 10]}]}],
            "brush": "2B_pencil",
            "size": 12
        }),
    );
    assert_eq!(skipped["ok"], json!(true), "{skipped}");
    assert_eq!(skipped["strokes"], json!(0));
    assert_eq!(skipped["skipped_subpaths"], json!(1));
}

#[test]
fn stroke_path_errors_are_explicit() {
    let mut ws = assets_workspace("stroke_errors");
    make_doc(&mut ws, "docA", 400, 300);
    let rect = call(
        &mut ws,
        "docA",
        "draw_shape",
        &json!({
            "layer_id": "L",
            "object_id": "rect1",
            "data": {
                "geometry": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 10, "h": 10}},
                "color": {"r": 0, "g": 0, "b": 0, "a": 255}
            }
        }),
    );
    assert_eq!(rect["ok"], json!(true), "{rect}");
    let line = json!([{"start": [0, 0], "segments": [{"op": "L", "to": [50, 0]}]}]);
    let cases: Vec<(Value, &str)> = vec![
        // 二选一同时给。
        (
            json!({"layer_id": "L", "brush": "2B_pencil", "path_object_id": "x", "subpaths": line}),
            "只能给一个",
        ),
        // 都不给。
        (json!({"layer_id": "L", "brush": "2B_pencil"}), "必须给"),
        // 引用不存在。
        (
            json!({"layer_id": "L", "brush": "2B_pencil", "path_object_id": "nope"}),
            "找不到 path_object_id 引用的对象：nope",
        ),
        // 引用了非 path 对象。
        (
            json!({"layer_id": "L", "brush": "2B_pencil", "path_object_id": "rect1"}),
            "不是 path 形状",
        ),
        // 内联非法段类型：错误带字段路径。
        (
            json!({"layer_id": "L", "brush": "2B_pencil", "subpaths": [{"start": [0, 0], "segments": [{"op": "Q", "to": [1, 1]}]}]}),
            "subpaths[0].segments[0].op",
        ),
        // 非法剖面名。
        (
            json!({"layer_id": "L", "brush": "2B_pencil", "subpaths": line, "pressure_profile": "bogus"}),
            "pressure_profile 只认",
        ),
        // ramp_ratio 越界。
        (
            json!({"layer_id": "L", "brush": "2B_pencil", "subpaths": line, "ramp_ratio": 0.9}),
            "ramp_ratio 必须在 0..0.5",
        ),
    ];
    for (args, needle) in cases {
        let failed = call(&mut ws, "docA", "stroke_path", &args);
        assert_eq!(failed["ok"], json!(false), "应报错：{failed}");
        let text = failed.to_string();
        assert!(
            text.contains(needle),
            "错误应包含「{needle}」，实测：{text}"
        );
    }
}

#[test]
fn draw_shape_path_illegal_payload_is_rejected_at_tool_layer() {
    // 工具层与渲染层同一份校验：非法 path 在 draw_shape 就被拒，不许落库再画空。
    let mut ws = assets_workspace("draw_reject");
    make_doc(&mut ws, "docA", 400, 300);
    let failed = call(
        &mut ws,
        "docA",
        "draw_shape",
        &json!({
            "layer_id": "L",
            "data": {
                "geometry": {
                    "kind": "path",
                    "subpaths": [{"start": [0, 0], "segments": [{"op": "C", "c2": [1, 1], "to": [2, 2]}]}]
                },
                "color": {"r": 0, "g": 0, "b": 0, "a": 255}
            }
        }),
    );
    assert_eq!(failed["ok"], json!(false), "{failed}");
    let text = failed.to_string();
    assert!(
        text.contains("subpaths[0].segments[0].c1"),
        "工具层错误应带字段路径，实测：{text}"
    );
}
