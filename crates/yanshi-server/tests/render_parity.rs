//! 服务端原生渲染 vs 客户端内核渲染：同一份原子集必须产出相同像素。
//!
//! 浏览器端的 bit-exact 自检失败时，这个测试用来判定问题出在
//! 「服务端与内核的代码路径差异」还是「WASM 与原生」。
//! 夹具由 `tests/data/*.json` 提供（从运行中的服务端 `GET /api/atoms` 导出）。

use serde_json::Value;
use yanshi_core::{Atom, Bbox};
use yanshi_server::{DocumentSettings, NewDocument, Workspace};
use yanshi_wasm::Kernel;

const MEMORY_LIMIT: usize = 256 * 1024 * 1024;

fn fixture(name: &str) -> Vec<Value> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../yanshi-wasm/tests/data")
        .join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("读取夹具 {} 失败：{error}", path.display()));
    serde_json::from_str(&text).expect("夹具必须是原子数组")
}

/// 返回 (差异字节数, 总字节数)。
fn parity(name: &str) -> (usize, usize) {
    let atoms = fixture(name);
    let (width, height) = (1024u32, 1024u32);

    // 服务端：建文档后逐条提交同一批原子，再整幅渲染。
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    workspace
        .create_document(
            NewDocument::new("parity", width, height),
            "human:1",
            "session:1",
        )
        .unwrap();
    let server_pixels;
    {
        let document = workspace.document_mut("parity").unwrap();
        for value in &atoms {
            let atom: Atom = serde_json::from_value(value.clone()).expect("夹具原子合法");
            document.commit_as(atom, "human:1", true, None).unwrap();
        }
        let (w, h, pixels) = document
            .render_region_raw(Bbox::new(0.0, 0.0, width as f64, height as f64))
            .unwrap();
        assert_eq!((w, h), (width, height));
        server_pixels = pixels;
    }

    // 客户端内核：同序应用原子后整幅渲染（内部按 tile 组合）。
    let mut kernel = Kernel::new("parity", 256, width, height, MEMORY_LIMIT).unwrap();
    for value in &atoms {
        kernel
            .apply_atom_json(&value.to_string())
            .unwrap_or_else(|error| panic!("内核应用原子失败：{error:?}"));
    }
    let rendered = kernel
        .render_region(Bbox::new(0.0, 0.0, width as f64, height as f64))
        .unwrap();
    let diff = server_pixels
        .iter()
        .zip(rendered.rgba8.iter())
        .filter(|(a, b)| a != b)
        .count();
    (diff, server_pixels.len())
}

#[test]
fn server_and_kernel_agree_on_shapes_and_strokes() {
    let (diff, len) = parity("simple_atoms.json");
    assert_eq!(diff, 0, "服务端与内核在简单场景下必须一致：{diff}/{len}");
}

/// 全特性文档：目前**不一致**（差异见输出），这是 bit-exact 复盘的定位靶子。
#[test]
#[ignore = "耗时数分钟；用于定位服务端/内核差异，结论见 implementation-notes"]
fn server_and_kernel_agree_on_the_full_phase3_document() {
    let (diff, len) = parity("phase3b_atoms.json");
    println!("服务端 vs 内核：差异 {diff}/{len} 字节");
    assert_eq!(diff, 0, "服务端与内核必须一致：{diff}/{len}");
}
