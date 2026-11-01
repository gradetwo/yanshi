//! 报告问题 #6：`smart_sample` 在报告参数下不得出规则竖条纹。
//!
//! 报告参数：1620×1920、samples=16、density=0.55、size=26、oil-01-paint、canvas。
//! 旧行为：Pass 1 是 32 条横贯长轴的直笔（整条笔只取中心一点方向），Pass 2
//! 仅 84 笔、笔长约 91 px，盖不住竖带；总笔数 149。
//! 新行为：Pass 1 沿等明度切向行走（每步重取切向），平坦区回退方向按黄金角
//! 逐笔旋转（不固定长轴）；Pass 2 笔数随面积缩放（报告参数下目标约 723）。
//!
//! 判据（均为结构量／下界，不写死系统笔数；变异：旧逻辑下本测试应红）：
//! - 全幅总笔数远大于旧 149（下界 400）；
//! - 笔数与面积成比例：全幅笔数 > 2 × 半幅笔数（旧逻辑下两档笔数基本相同）；
//! - 全幅导出图不得是规则竖条纹：强梯度像素中水平梯度占比 < 0.6
//!   （竖条纹的边缘是竖直的 ⇒ 梯度几乎全水平；斜带 + 圆 ⇒ 梯度方向分散）；
//! - 结构可见：导出图与参考图的亮度相关 > 0.4（ sanity 项，只防空白／颜色全错，
//!   不作竖条纹的判据——旧逻辑颜色也采对，故相关系数本身不判竖条纹）。

use serde_json::{json, Value};
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

/// 本测试落盘文件的清理器：Drop 时删文件。
/// 注意：测试写的是共享 /tmp 里的单个文件，不是私有临时目录，
/// 所以 Drop 只删该文件，绝不能 remove_dir_all（那会删 /tmp 本身）。
struct Cleanup(std::path::PathBuf);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn fresh_workspace(doc_w: u32, doc_h: u32) -> Workspace {
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut workspace =
        Workspace::in_memory(DocumentSettings::default()).with_assets_dir(Some(assets));
    workspace
        .create_document(
            NewDocument::new("doc_ss6", doc_w, doc_h),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn call(workspace: &mut Workspace, tool: &str, args: Value) -> Value {
    // 大画布上不等渲染：笔数不依赖渲染，导出时按需渲染即可。
    let mut ctx = ToolContext::new(workspace, "doc_ss6", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(false, 0);
    registry().call(&mut ctx, tool, &args)
}

/// 合成参考图像素：浅暖底 + 深色对角带 + 深色圆（与报告场景同构：平坦底 + 斜向结构 + 团块）。
fn reference_rgba(w: u32, h: u32) -> Vec<u8> {
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    let (bw, bh) = (w as f64, h as f64);
    let min_dim = bw.min(bh);
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f64, y as f64);
            // 对角带：到直线 y = 0.55x + 0.25h 的距离 < 0.08×min(w,h)。
            let d_band = (fy - 0.55 * fx - 0.25 * bh).abs() / (1.0f64 + 0.55f64 * 0.55).sqrt();
            // 圆：中心 (0.5w, 0.35h)，半径 0.13×min(w,h)。
            let d_circle = ((fx - 0.5 * bw).powi(2) + (fy - 0.35 * bh).powi(2)).sqrt();
            let dark = d_band < 0.08 * min_dim || d_circle < 0.13 * min_dim;
            let (r, g, b) = if dark {
                (52u8, 38, 26)
            } else {
                (205, 178, 138)
            };
            let i = ((y * w + x) * 4) as usize;
            rgba[i] = r;
            rgba[i + 1] = g;
            rgba[i + 2] = b;
            rgba[i + 3] = 255;
        }
    }
    rgba
}

/// 合成参考图 PNG 字节（与 `reference_rgba` 同像素，PNG 编解码无损）。
fn reference_png(w: u32, h: u32) -> Vec<u8> {
    let rgba = reference_rgba(w, h);
    yanshi_render::png::encode_png(w, h, &rgba).expect("合成参考图应当能编码")
}

/// 以报告参数跑一次 smart_sample，返回回执的 strokes 数。
fn run_report_case(workspace: &mut Workspace, ref_path: &str, doc_w: u32, doc_h: u32) -> u64 {
    let response = call(
        workspace,
        "smart_sample",
        json!({
            "layer_id": "layer_default",
            "ref": ref_path,
            "shape": {"type": "rect", "x": 0, "y": 0, "w": doc_w, "h": doc_h},
            "samples": 16,
            "density": 0.55,
            "size": 26,
            "brush": "oil-01-paint",
            "texture_type": "canvas",
        }),
    );
    response
        .get("strokes")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("smart_sample 应当回 strokes 数：{response}"))
}

/// 导出整幅并解码，返回 (w, h, rgba)。
fn export_rgba(workspace: &mut Workspace, path_str: &str) -> (u32, u32, Vec<u8>) {
    let response = call(workspace, "export_png", json!({"path": path_str}));
    assert_eq!(
        response.get("ok"),
        Some(&json!(true)),
        "export_png 应当成功：{response}"
    );
    let bytes = std::fs::read(path_str).expect("export_png 应当落盘");
    yanshi_render::png::decode_png(&bytes).expect("导出图应当是能解码的 PNG")
}

/// 4 倍块均值降采样后的亮度图。
fn downsampled_luma(rgba: &[u8], w: u32, h: u32) -> (usize, usize, Vec<f64>) {
    let (sw, sh) = ((w / 4) as usize, (h / 4) as usize);
    let mut lum = vec![0f64; sw * sh];
    for by in 0..sh {
        for bx in 0..sw {
            let mut sum = 0f64;
            let mut n = 0f64;
            for dy in 0..4 {
                for dx in 0..4 {
                    let (x, y) = (bx * 4 + dx, by * 4 + dy);
                    if x < w as usize && y < h as usize {
                        let i = (y * w as usize + x) * 4;
                        sum += 0.299 * rgba[i] as f64
                            + 0.587 * rgba[i + 1] as f64
                            + 0.114 * rgba[i + 2] as f64;
                        n += 1.0;
                    }
                }
            }
            lum[by * sw + bx] = sum / n.max(1.0);
        }
    }
    (sw, sh, lum)
}

/// 渲染分析：Sobel 强梯度像素数，及其中的水平梯度占比。
/// 竖条纹 ⇒ 边缘竖直 ⇒ 梯度几乎全水平（占比近 1）；斜带 + 圆 ⇒ 方向分散。
fn stripe_metrics(lum: &[f64], sw: usize, sh: usize) -> (usize, f64) {
    let mut strong = 0usize;
    let mut horizontal = 0usize;
    for y in 1..sh - 1 {
        for x in 1..sw - 1 {
            let gx =
                -lum[(y - 1) * sw + x - 1] - 2.0 * lum[y * sw + x - 1] - lum[(y + 1) * sw + x - 1]
                    + lum[(y - 1) * sw + x + 1]
                    + 2.0 * lum[y * sw + x + 1]
                    + lum[(y + 1) * sw + x + 1];
            let gy = -lum[(y - 1) * sw + x - 1]
                - 2.0 * lum[(y - 1) * sw + x]
                - lum[(y - 1) * sw + x + 1]
                + lum[(y + 1) * sw + x - 1]
                + 2.0 * lum[(y + 1) * sw + x]
                + lum[(y + 1) * sw + x + 1];
            let mag = (gx * gx + gy * gy).sqrt();
            if mag > 40.0 {
                strong += 1;
                // 水平梯度：梯度方向与水平轴夹角 < 30°。
                if gx.abs() > 1.732 * gy.abs() {
                    horizontal += 1;
                }
            }
        }
    }
    let frac = if strong > 0 {
        horizontal as f64 / strong as f64
    } else {
        1.0
    };
    (strong, frac)
}

/// 两张亮度图（同尺寸）的 Pearson 相关系数。
fn luma_correlation(a: &[f64], b: &[f64]) -> f64 {
    assert_eq!(a.len(), b.len());
    let n = a.len() as f64;
    let (ma, mb) = (a.iter().sum::<f64>() / n, b.iter().sum::<f64>() / n);
    let (mut num, mut da, mut db) = (0.0, 0.0, 0.0);
    for i in 0..a.len() {
        let (x, y) = (a[i] - ma, b[i] - mb);
        num += x * y;
        da += x * x;
        db += y * y;
    }
    if da <= 0.0 || db <= 0.0 {
        return 0.0;
    }
    num / (da * db).sqrt()
}

#[test]
fn smart_sample_report_case_no_vertical_stripes() {
    let dir = std::env::temp_dir();
    let ref_full = dir.join("yanshi-smartsample-report6-full-ref.png");
    let ref_small = dir.join("yanshi-smartsample-report6-small-ref.png");
    let export_full = dir.join("yanshi-smartsample-report6-full-export.png");
    // 显式绑定守卫：判据 check-tests-clean-temp 只认 `x = Cleanup(` 的绑定形态。
    let _cleanup_ref_full = Cleanup(ref_full.clone());
    let _cleanup_ref_small = Cleanup(ref_small.clone());
    let _cleanup_export_full = Cleanup(export_full.clone());
    std::fs::write(&ref_full, reference_png(1620, 1920)).expect("参考图应当能落盘");
    std::fs::write(&ref_small, reference_png(810, 960)).expect("参考图应当能落盘");

    // 全幅：报告参数原样。
    let mut ws_full = fresh_workspace(1620, 1920);
    let strokes_full = run_report_case(&mut ws_full, &ref_full.display().to_string(), 1620, 1920);

    // 半幅：同参数、同比例内容，笔数应随面积成比例下降。
    let mut ws_small = fresh_workspace(810, 960);
    let strokes_small = run_report_case(&mut ws_small, &ref_small.display().to_string(), 810, 960);

    // 全幅导出图：不得是规则竖条纹，结构可见。
    let (w, h, rgba) = export_rgba(&mut ws_full, &export_full.display().to_string());
    assert_eq!((w, h), (1620, 1920), "导出尺寸应当与文档一致");
    let (sw, sh, lum) = downsampled_luma(&rgba, w, h);
    let (strong, hfrac) = stripe_metrics(&lum, sw, sh);
    let ref_rgba = reference_rgba(1620, 1920);
    let (_, _, ref_lum) = downsampled_luma(&ref_rgba, 1620, 1920);
    let corr = luma_correlation(&lum, &ref_lum);
    // 先算全部度量、后统一断言——变异验证（旧逻辑）时计数断言先红，
    // 也要能看到旧逻辑的 hfrac，否则无法判断判据本身是否有效。
    assert!(
        strokes_full > 400,
        "报告参数下总笔数应远大于旧 149，实测 {strokes_full}"
    );
    assert!(
        strokes_full > 2 * strokes_small,
        "笔数应随面积成比例：全幅 {strokes_full} vs 半幅 {strokes_small}"
    );
    assert!(
        strong > 500,
        "全幅导出图应有足够强梯度像素（结构边缘），实测 {strong}"
    );
    assert!(
        hfrac < 0.6,
        "全幅导出图不得是规则竖条纹：水平梯度占比 {hfrac:.3}（竖条纹近 1.0）"
    );
    assert!(
        corr > 0.4,
        "全幅导出图结构应与参考图可见地对应，亮度相关 {corr:.3}"
    );
}
