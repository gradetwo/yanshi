//! 冷启动测量探针（**默认 ignore**，只在有真实 4K 工程时手动跑）。
//!
//! 用法：
//! ```text
//! YANSHI_COLD_DOC_ROOT=/tmp/cold4k \
//! YANSHI_COLD_DOC_ID=parrot-4k-bold \
//!   cargo test -p yanshi-server --test coldstart_measure -- --ignored --nocapture
//! ```
//!
//! 它**只测量、不断言**（数字随机器变化），断言在 `coldstart_reuse.rs` 里。

use std::time::{Duration, Instant};

use yanshi_server::persist::FileStore;
use yanshi_server::{DocThumbSize, DocumentSettings, Workspace};

fn root() -> String {
    std::env::var("YANSHI_COLD_DOC_ROOT").unwrap_or_else(|_| "/tmp/cold4k".to_owned())
}

fn doc_id() -> String {
    std::env::var("YANSHI_COLD_DOC_ID").unwrap_or_else(|_| "parrot-4k-bold".to_owned())
}

/// **一次"新客户端连接"**：新 `Workspace` + 打开文档 + 取文档预览（= 走 `ensure_document_thumbnail`）。
/// 返回 (打开耗时, 预览耗时, 整幅渲染次数, 全量折叠次数)。
fn connection_preview(root: &str, doc_id: &str) -> (Duration, Duration, usize, usize, usize) {
    let t = Instant::now();
    let mut workspace =
        Workspace::with_file_store(root, DocumentSettings::default()).expect("打开落盘工作区");
    workspace.open_document(doc_id).expect("打开文档");
    let open = t.elapsed();
    let t = Instant::now();
    workspace
        .ensure_document_thumbnail(doc_id, DocThumbSize::S256)
        .expect("取文档预览");
    let preview = t.elapsed();
    let document = workspace.document(doc_id).expect("文档");
    (
        open,
        preview,
        document.full_canvas_render_count(),
        document.full_fold_count(),
        document.atom_count(),
    )
}

fn disk_state(root: &str, doc_id: &str) -> String {
    let store = FileStore::open(root).expect("文件存储");
    let head = store
        .load_atoms(doc_id)
        .map(|atoms| atoms.iter().map(|a| a.seq).max().unwrap_or(0))
        .unwrap_or(0);
    let render = store
        .load_render(doc_id)
        .ok()
        .flatten()
        .map(|(seq, png)| format!("render.png seq={seq} bytes={}", png.len()))
        .unwrap_or_else(|| "render.png （无）".to_owned());
    let preview = store
        .load_preview(doc_id)
        .ok()
        .flatten()
        .map(|(seq, png)| format!("preview.png seq={seq} bytes={}", png.len()))
        .unwrap_or_else(|| "preview.png （无）".to_owned());
    format!("head={head} | {render} | {preview}")
}

#[test]
#[ignore = "需要真实 4K 工程，手动运行"]
fn coldstart_connection_costs() {
    let root = root();
    let doc_id = doc_id();
    let store = FileStore::open(&root).expect("文件存储");
    if store.load_atoms(&doc_id).expect("读原子").is_empty() {
        eprintln!("跳过：{root} 里没有文档 {doc_id}");
        return;
    }
    println!("=== 磁盘现状 ===");
    println!("  {}", disk_state(&root, &doc_id));

    println!("=== 1. 新连接（磁盘预览与 HEAD 一致 ⇒ 必须零渲染） ===");
    for round in 1..=2 {
        let (open, preview, renders, folds, atoms) = connection_preview(&root, &doc_id);
        println!(
            "  第{round}次：打开={open:?}（{atoms} atom 全量折叠 {folds} 次）文档预览={preview:?} 整幅渲染={renders}"
        );
    }

    println!("=== 2. 直接提交一笔、**不落盘预览**（模拟离线/批量路径）⇒ 预览落后一个小脏区 ===");
    {
        let mut workspace =
            Workspace::with_file_store(&root, DocumentSettings::default()).expect("工作区");
        workspace.open_document(&doc_id).expect("打开");
        let atom = yanshi_core::Atom::new(
            yanshi_core::AtomKind::DrawStroke,
            "human:1",
            "session:measure",
            serde_json::json!({
                "layer_id": "layer_default",
                "object_id": "obj_measure_probe",
                "data": {"points": [[100.0, 100.0], [400.0, 300.0]], "size": 40.0,
                         "color": {"r": 10, "g": 200, "b": 10, "a": 255}}
            }),
        );
        workspace
            .commit(&doc_id, atom, "human:1", true)
            .expect("提交");
    }
    println!("  {}", disk_state(&root, &doc_id));
    let (open, preview, renders, folds, atoms) = connection_preview(&root, &doc_id);
    println!(
        "  新连接：打开={open:?}（{atoms} atom 全量折叠 {folds} 次）文档预览={preview:?} 整幅渲染={renders}"
    );
    println!("  （整幅渲染应为 0：只重渲染脏区 ✓；全量折叠应为 1：打开那一次 ✓）");
}

/// **只测"把整幅 4K 图当预览基座"的固定成本**：解码 + 转线性 + 降采样。
///
/// 它决定了设计方向 ✓：实测 `decode_png` **153.7s** ＋ u8→线性 **26.4s**
/// ＝ **183.6s**（debug ✓）⇒ **比整幅重渲染（122s）还贵** ✗
/// ⇒ **∴ 预览基座只能用小的 256² 图** ✓（解码 ~1s ✓）。
#[test]
#[ignore = "需要真实 4K 工程，手动运行"]
fn persisted_full_frame_decode_cost() {
    let root = root();
    let doc_id = doc_id();
    let store = FileStore::open(&root).expect("打开文件存储");
    let Some((seq, png)) = store.load_render(&doc_id).expect("读取渲染缓存") else {
        eprintln!("跳过：没有 render.png");
        return;
    };
    let t = Instant::now();
    let (width, height, rgba8) = yanshi_render::png::decode_png(&png).expect("解码");
    let decode = t.elapsed();
    let t = Instant::now();
    let pixels: Vec<f32> = rgba8
        .chunks_exact(4)
        .flat_map(|p| yanshi_render::color::u8x4_to_linear_premul([p[0], p[1], p[2], p[3]]))
        .collect();
    let to_linear = t.elapsed();
    let t = Instant::now();
    let mut thumb = yanshi_render::thumb::Thumb::new(yanshi_render::thumb::ThumbKind::Doc256);
    thumb.update_full(
        &yanshi_render::Buffer::from_f32(0, 0, width, height, &pixels).expect("缓冲"),
        yanshi_core::Bbox::new(0.0, 0.0, width as f64, height as f64),
    );
    let downsample = t.elapsed();
    println!(
        "=== 整幅图的固定复用成本（seq={seq} {width}x{height} {} 字节）===",
        png.len()
    );
    println!(
        "decode_png={decode:?} u8->linear={to_linear:?} downsample={downsample:?} 合计={:?}",
        decode + to_linear + downsample
    );
}
