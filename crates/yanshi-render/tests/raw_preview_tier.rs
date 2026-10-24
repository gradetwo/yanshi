//! C1-b 判据（render 取图分派）：预览档入口强制档位、记录档位
//! 重放保真。feature `raw` 开时编译。
#![cfg(feature = "raw")]

use yanshi_raw::dng_fixture::{write_synthetic_dng, SyntheticDng};
use yanshi_render::{develop_raw_source, develop_raw_source_preview, develop_raw_source_recorded};

fn fixture_bytes() -> Vec<u8> {
    let mosaic: Vec<u16> = (0..240 * 160)
        .map(|i| 600 + ((i * 37) % 14000) as u16)
        .collect();
    write_synthetic_dng(&SyntheticDng {
        width: 240,
        height: 160,
        mosaic,
        ..SyntheticDng::default()
    })
}

#[test]
fn preview_fetch_forces_tier_and_records_it() {
    let bytes = fixture_bytes();
    // 全档记录（导入期落账的形态）。
    let full =
        develop_raw_source(&bytes, &yanshi_raw::DevelopOptions::default()).expect("全档展开");
    assert_eq!((full.width, full.height), (240, 160));
    assert_eq!(full.record.developed_max_edge, None);
    let record_json = serde_json::to_value(&full.record).expect("record 可序列化");

    // 凭记录重展开 = 全档（重放保真）。
    let mut warnings = Vec::new();
    let replayed =
        develop_raw_source_recorded(&bytes, Some(&record_json), &mut warnings).expect("重放");
    assert_eq!((replayed.width, replayed.height), (240, 160));
    assert_eq!(replayed.record.developed_max_edge, None);

    // 预览取图：同一份记录、强制 64 档 ⇒ 64×42（160×64÷240 = 42），
    // 产物 record 记本次档位。
    let mut warnings = Vec::new();
    let preview = develop_raw_source_preview(&bytes, Some(&record_json), 64, &mut warnings)
        .expect("预览取图");
    assert_eq!((preview.width, preview.height), (64, 42));
    assert_eq!(preview.record.developed_max_edge, Some(64));

    // 预览记录再凭记录重放 ⇒ 仍是预览档（档位凭记录复原，
    // 不需要调用方再点名）。
    let preview_json = serde_json::to_value(&preview.record).expect("预览 record 可序列化");
    let mut warnings = Vec::new();
    let replayed_preview = develop_raw_source_recorded(&bytes, Some(&preview_json), &mut warnings)
        .expect("预览记录重放");
    assert_eq!((replayed_preview.width, replayed_preview.height), (64, 42));
    assert_eq!(replayed_preview.record.developed_max_edge, Some(64));
}

#[test]
fn preview_fetch_rejects_out_of_range_edge_loudly() {
    let bytes = fixture_bytes();
    let mut warnings = Vec::new();
    let error = develop_raw_source_preview(&bytes, None, 32, &mut warnings)
        .expect_err("越界 edge 必须经展开校验响亮报错");
    assert!(
        error.to_string().contains("max_output_edge"),
        "错误必须点名参数：{error}"
    );
}
