//! 期 1 B 项回归（方向拾取）：真文件在场跑、不在场跳过并出声。
//!
//! 背景：rawler 0.7.2 的 RawImage 构造器把 orientation 写死 Normal、
//! 只回填 DNG/QTK，真值在 metadata.exif.orientation 里。修复前
//! ojigi RW2 与竖拍 ARW 的 frame/meta orientation 恒为 1（红）；
//! 修复后须与 EXIF 一致（绿）。样本目录候选：环境变量
//! YANSHI_RAW_SAMPLES_DIR > 盒上探针目录 > 本地 showcase 下载目录。

use std::path::PathBuf;

fn find_sample(file: &str, extra_dirs: &[&str]) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(d) = std::env::var("YANSHI_RAW_SAMPLES_DIR") {
        dirs.push(PathBuf::from(d));
    }
    for d in extra_dirs {
        dirs.push(PathBuf::from(d));
    }
    // 相对候选以 crate 目录为锚（cargo test 的 CWD = 包根）。
    dirs.push(PathBuf::from("../../../yanshi-showcase/downloads"));
    dirs.iter().map(|d| d.join(file)).find(|p| p.exists())
}

#[test]
fn rw2_portrait_orientation_comes_from_exif() {
    let Some(path) = find_sample("ojigi_temple.RW2", &["/data/workspace/probe-raws"]) else {
        println!("跳过：ojigi_temple.RW2 样本不在场（方向真文件回归未跑）");
        return;
    };
    let bytes = std::fs::read(&path).expect("样本可读");
    let decoded = yanshi_raw::decode_bytes(&bytes).expect("ojigi RW2 必须可解码");
    // ojigi 是竖拍，EXIF Orientation = 8（探针 E1 实测）。
    assert_eq!(
        decoded.frame.orientation, 8,
        "帧方向须取 EXIF 真值 8（不是解码器写死的 Normal）"
    );
    assert_eq!(
        decoded.file_meta.orientation, decoded.frame.orientation,
        "frame 与 file_meta 方向必须同源同值"
    );
    // 同目录横拍样本不許被误伤：verona EXIF = 1。
    if let Some(v) = find_sample("verona_sunset.RW2", &["/data/workspace/probe-raws"]) {
        let bytes = std::fs::read(&v).expect("样本可读");
        let decoded = yanshi_raw::decode_bytes(&bytes).expect("verona RW2 必须可解码");
        assert_eq!(decoded.frame.orientation, 1, "横拍 RW2 方向须为 1");
        assert_eq!(decoded.file_meta.orientation, 1);
    }
}

#[test]
fn arw_portrait_orientation_comes_from_exif() {
    let Some(path) = find_sample("DSC01242.ARW", &["/data/home/test"]) else {
        println!("跳过：DSC01242.ARW 样本不在场（ARW 方向回归未跑）");
        return;
    };
    let bytes = std::fs::read(&path).expect("样本可读");
    let decoded = yanshi_raw::decode_bytes(&bytes).expect("DSC01242.ARW 必须可解码");
    // Sony ILCE-7RM2 竖拍，EXIF Orientation = 8（探针 E3 实测）。
    assert_eq!(decoded.frame.orientation, 8, "ARW 帧方向须取 EXIF 真值 8");
    assert_eq!(decoded.file_meta.orientation, 8);
}

#[test]
fn orientation_fallback_leaves_a_note() {
    // 回退不许静默：EXIF 缺失的夹具，notes 必须交代方向来自解码器
    // 字段；EXIF 在场的夹具不许出现这条回退注记。
    use yanshi_raw::dng_fixture::{write_synthetic_dng, SyntheticDng, SyntheticExif};
    let plain = yanshi_raw::decode_bytes(&write_synthetic_dng(&SyntheticDng::default()))
        .expect("无 EXIF 夹具必须可解码");
    assert!(
        plain.notes.iter().any(|n| n.contains("方向按解码器字段取")),
        "EXIF 缺失时回退必须留痕，实得 notes={:?}",
        plain.notes
    );
    let spec = SyntheticDng {
        exif: Some(SyntheticExif {
            orientation: Some(8),
            ..Default::default()
        }),
        ..Default::default()
    };
    let with_exif =
        yanshi_raw::decode_bytes(&write_synthetic_dng(&spec)).expect("带 EXIF 夹具必须可解码");
    assert!(
        !with_exif
            .notes
            .iter()
            .any(|n| n.contains("方向按解码器字段取")),
        "EXIF 在场时不许出现回退注记，实得 notes={:?}",
        with_exif.notes
    );
}

#[test]
fn dng_fixture_orientation_eight_stays_eight() {
    // E2 对照组：DNG 路（解码器回填）在修复前后都必须得 8，不许改坏。
    use yanshi_raw::dng_fixture::{write_synthetic_dng, SyntheticDng, SyntheticExif};
    let spec = SyntheticDng {
        exif: Some(SyntheticExif {
            orientation: Some(8),
            ..Default::default()
        }),
        ..Default::default()
    };
    let bytes = write_synthetic_dng(&spec);
    let decoded = yanshi_raw::decode_bytes(&bytes).expect("合成 DNG 必须可解码");
    assert_eq!(decoded.frame.orientation, 8);
    assert_eq!(decoded.file_meta.orientation, 8);
}
