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
    parity_attempt(&atoms).unwrap_or_else(|error| panic!("对比失败：{error}"))
}

/// 用给定原子集做对比；可失败（用于二分时跳过破坏日志的原子）。
fn parity_attempt(atoms: &[Value]) -> Result<(usize, usize), String> {
    let (width, height) = (1024u32, 1024u32);

    // 服务端：建文档后逐条提交同一批原子，再整幅渲染。
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    workspace
        .create_document(
            NewDocument::new("parity", width, height),
            "human:1",
            "session:1",
        )
        .map_err(|error| format!("建文档失败：{error:?}"))?;
    let server_pixels;
    {
        let document = workspace
            .document_mut("parity")
            .map_err(|error| format!("取文档失败：{error:?}"))?;
        for value in atoms {
            let atom: Atom = serde_json::from_value(value.clone())
                .map_err(|error| format!("夹具原子非法：{error}"))?;
            document
                .commit_as(atom, "human:1", true, None)
                .map_err(|error| format!("提交失败：{error:?}"))?;
        }
        let (w, h, pixels) = document
            .render_region_raw(Bbox::new(0.0, 0.0, width as f64, height as f64))
            .map_err(|error| format!("服务端渲染失败：{error:?}"))?;
        if (w, h) != (width, height) {
            return Err(format!("尺寸不符：{w}x{h}"));
        }
        server_pixels = pixels;
    }

    // 客户端内核：同序应用原子后整幅渲染（内部按 tile 组合）。
    let mut kernel = Kernel::new("parity", 256, width, height, MEMORY_LIMIT)
        .map_err(|error| format!("建内核失败：{error:?}"))?;
    for value in atoms {
        kernel
            .apply_atom_json(&value.to_string())
            .map_err(|error| format!("内核应用原子失败：{error:?}"))?;
    }
    let rendered = kernel
        .render_region(Bbox::new(0.0, 0.0, width as f64, height as f64))
        .map_err(|error| format!("内核渲染失败：{error:?}"))?;
    let diff = server_pixels
        .iter()
        .zip(rendered.rgba8.iter())
        .filter(|(a, b)| a != b)
        .count();
    Ok((diff, server_pixels.len()))
}

/// 差异像素明细（坐标、通道、两侧取值、tile 索引），用于判断是否落在边界上。
#[test]
#[ignore = "定位用：打印差异像素明细"]
fn report_differing_pixels() {
    let atoms = fixture("phase3b_atoms.json");
    let (diff, len) = parity_attempt(&atoms).expect("对比应可完成");
    println!("差异 {diff}/{len} 字节");
    let details = differing_pixels(&atoms);
    for line in details.iter().take(20) {
        println!("  {line}");
    }
}

/// 逐原子二分：去掉某个原子后差异是否消失。
#[test]
#[ignore = "定位用：逐个删除原子找出关键因子（约 1-2 分钟）"]
fn bisect_phase3b() {
    let atoms = fixture("phase3b_atoms.json");
    for skip in 0..atoms.len() {
        let subset: Vec<Value> = atoms
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != skip)
            .map(|(_, value)| value.clone())
            .collect();
        let kind = atoms[skip]["kind"].as_str().unwrap_or("?").to_owned();
        match parity_attempt(&subset) {
            Ok((0, _)) => println!("#{skip} {kind} 删除后差异为 0 ← 关键因子"),
            Ok((diff, _)) => println!("#{skip} {kind} 删除后仍有 {diff} 字节差异"),
            Err(error) => println!("#{skip} {kind} 删除后无法比较：{error}"),
        }
    }
}

/// 收集差异像素的明细。
fn differing_pixels(atoms: &[Value]) -> Vec<String> {
    let (width, height) = (1024u32, 1024u32);
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    workspace
        .create_document(
            NewDocument::new("parity", width, height),
            "human:1",
            "session:1",
        )
        .unwrap();
    let server_pixels = {
        let document = workspace.document_mut("parity").unwrap();
        for value in atoms {
            let atom: Atom = serde_json::from_value(value.clone()).unwrap();
            document.commit_as(atom, "human:1", true, None).unwrap();
        }
        document
            .render_region_raw(Bbox::new(0.0, 0.0, width as f64, height as f64))
            .unwrap()
            .2
    };
    let mut kernel = Kernel::new("parity", 256, width, height, MEMORY_LIMIT).unwrap();
    for value in atoms {
        kernel.apply_atom_json(&value.to_string()).unwrap();
    }
    let rendered = kernel
        .render_region(Bbox::new(0.0, 0.0, width as f64, height as f64))
        .unwrap();
    let mut lines = Vec::new();
    for (index, (a, b)) in server_pixels.iter().zip(rendered.rgba8.iter()).enumerate() {
        if a == b {
            continue;
        }
        let pixel = index / 4;
        let (x, y) = (pixel as u32 % width, pixel as u32 / width);
        lines.push(format!(
            "({x},{y}) 通道 {} 服务端 {a} / 内核 {b} | tile ({},{}) | 像素内偏移 ({}, {})",
            index % 4,
            x / 256,
            y / 256,
            x % 256,
            y % 256
        ));
    }
    lines
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
