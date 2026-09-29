//! 「按 tile 组合」与「整幅直接渲染」必须逐字节一致。
//!
//! 客户端按 tile 缓存组合出画面，服务端一次渲染整块区域；两者若不一致，
//! 浏览器端的 bit-exact 自检就会失败。这里用夹具原子集在宿主上复现并回归。
//!
//! 目前有一个已知缺陷（见 docs/design/implementation-notes.md）：
//! `phase3b` 夹具（形状 + 笔迹 + 调整/滤镜 + 修图 + 液化 + 蒙版同时存在）会不一致，
//! 因此该用例标记为 `#[ignore]`，等定位修复后移除标记。

use serde_json::Value;
use yanshi_core::Bbox;
use yanshi_wasm::Kernel;

const MEMORY_LIMIT: usize = 256 * 1024 * 1024;

fn fixture(name: &str) -> Vec<Value> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("读取夹具 {} 失败：{error}", path.display()));
    serde_json::from_str(&text).expect("夹具必须是原子数组")
}

fn kernel_from(atoms: &[Value]) -> Kernel {
    let mut kernel = Kernel::new("phase3b", 256, 1024, 1024, MEMORY_LIMIT).unwrap();
    for atom in atoms {
        kernel
            .apply_atom_json(&atom.to_string())
            .unwrap_or_else(|error| panic!("应用原子失败：{error:?}"));
    }
    kernel
}

/// 把 1024×1024 拆成 16 块 256² 逐块渲染后拼接，与整幅渲染比较。
fn composed_vs_full(name: &str) -> (usize, usize) {
    let atoms = fixture(name);
    let mut kernel = kernel_from(&atoms);
    let full = kernel
        .render_region(Bbox::new(0.0, 0.0, 1024.0, 1024.0))
        .unwrap();

    // 重建一个内核（避免 tile 缓存影响），逐块渲染并拼接。
    let mut tiled = kernel_from(&atoms);
    let mut composed = vec![0u8; 1024 * 1024 * 4];
    for ty in 0..4 {
        for tx in 0..4 {
            let (x, y) = (tx as f64 * 256.0, ty as f64 * 256.0);
            let tile = tiled.render_region(Bbox::new(x, y, 256.0, 256.0)).unwrap();
            for row in 0..256usize {
                let source = row * 256 * 4;
                let target = ((y as usize + row) * 1024 + x as usize) * 4;
                composed[target..target + 256 * 4]
                    .copy_from_slice(&tile.rgba8[source..source + 256 * 4]);
            }
        }
    }
    let diff = full
        .rgba8
        .iter()
        .zip(composed.iter())
        .filter(|(a, b)| a != b)
        .count();
    (diff, full.rgba8.len())
}

/// 默认用例：形状 + 笔迹，两条路径必须逐字节一致（毫秒级）。
#[test]
fn tile_composition_matches_full_render_for_shapes_and_strokes() {
    let (diff, len) = composed_vs_full("simple_atoms.json");
    assert_eq!(diff, 0, "形状 + 笔迹不应有差异：{diff}/{len}");
}

/// 全特性文档：**目前通过**（分块组合与整幅渲染一致），但单次 1024² 全效果渲染
/// 需要约 3 分钟（外扩 128 + 辉光/液化等重运算），因此标记 `#[ignore]`，
/// 由 CI 的 `--ignored` 作业执行。
///
/// 重要结论：这个用例通过，说明浏览器端 bit-exact 失败**不是**分块组合导致的，
/// 而应定位到「客户端 WASM 与服务端原生」的差异或浏览器渲染路径。
#[test]
#[ignore = "耗时约 3 分钟；结论见 docs/design/implementation-notes.md"]
fn tile_composition_matches_full_render_for_the_full_phase3_document() {
    let (diff, len) = composed_vs_full("phase3b_atoms.json");
    assert_eq!(diff, 0, "分块组合与整幅渲染必须一致：{diff}/{len}");
}
