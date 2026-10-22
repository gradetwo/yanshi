//! **位图选区（`create_mask{kind:bitmap}` → `create_selection{from_mask}`）必须能裁剪**。
//!
//! **实测的缺陷**（第 2665 轮）：`clip_to_selection` 指向位图选区时
//! `ok:true` 却 **`painted_pixels_clipped=0`、`clip_coverage=0.0`** ⇒ **一个像素都不落** ✗
//! （他们 `E1742` 一族是同一处）。
//!
//! **两处根因（同一族，第三次）**：
//! ① 选区的 `bbox` 是**数组** `[x,y,w,h]`，而裁剪那段只按**对象**读 ⇒ 读出 `(0,0,0,0)` ⇒ 裁剪框空 ✗；
//! ② 真正查表那一步（`SelectionShape::parse_geom`）**只认 `blob`**，而选区存的是 **`blob_hash`**
//!    ⇒ `mask=None` ⇒ 覆盖率恒 0 ✗（注册那边早就两种键都收了 ✓）。
//!
//! **判据断结构量**（两个方向成对 ✓）：
//! ① 蒙版**不透明**的那一半必须有墨（`painted_pixels_clipped > 0` ＋ 采样点非白）；
//! ② 蒙版**透明**的那一半必须一点墨都没有（否则就是「按 bbox 裁剪」✗）。

use serde_json::{json, Value};
use yanshi_server::{DocumentSettings, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    Workspace::in_memory(DocumentSettings::default()).with_assets_dir(Some(assets))
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_bitmap_clip", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 8_000)
}

fn call(ctx: &mut ToolContext<'_>, name: &str, args: &Value) -> Value {
    registry().call(ctx, name, args)
}

/// 64×64 的蒙版位图：**左半不透明**（alpha 255）、**右半全透明**（alpha 0）。
fn mask_bytes() -> Vec<u8> {
    let mut bytes = vec![0u8; 64 * 64 * 4];
    for y in 0..64 {
        for x in 0..32 {
            let index = (y * 64 + x) * 4;
            bytes[index] = 255;
            bytes[index + 1] = 255;
            bytes[index + 2] = 255;
            bytes[index + 3] = 255;
        }
    }
    bytes
}

fn rgba_at(ctx: &mut ToolContext<'_>, x: i32, y: i32) -> [u64; 4] {
    let reply = call(ctx, "sample_colors", &json!({"points": [[x, y]]}));
    let values = reply
        .get("samples")
        .and_then(Value::as_array)
        .and_then(|samples| samples.first())
        .and_then(|sample| sample.get("rgba"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut out = [0u64; 4];
    for (index, slot) in out.iter_mut().enumerate() {
        *slot = values.get(index).and_then(Value::as_u64).unwrap_or(0);
    }
    out
}

#[test]
fn a_bitmap_selection_clips_to_its_opaque_pixels() {
    let mut workspace = workspace();
    let hash = workspace
        .store()
        .put(&mask_bytes())
        .expect("蒙版位图应能入库")
        .to_string();
    {
        let mut ctx = context(&mut workspace);
        call(
            &mut ctx,
            "new_document",
            &json!({"doc_id": "doc_bitmap_clip", "width": 64, "height": 64}),
        );
        let made = call(
            &mut ctx,
            "create_mask",
            &json!({
                "mask_id": "mk",
                "shape": {"kind": "bitmap", "blob_hash": hash, "width": 64, "height": 64}
            }),
        );
        assert_eq!(
            made.get("ok").and_then(Value::as_bool),
            Some(true),
            "前置：位图蒙版没建出来：{made}"
        );
        let selected = call(
            &mut ctx,
            "create_selection",
            &json!({"selection_id": "sel_bitmap", "from_mask": "mk"}),
        );
        assert_eq!(
            selected.get("ok").and_then(Value::as_bool),
            Some(true),
            "前置：从蒙版转选区失败：{selected}"
        );
    }
    let mut ctx = context(&mut workspace);

    let drawn = call(
        &mut ctx,
        "brush_stroke",
        &json!({
            "layer_id": "layer_default",
            "object_id": "bitmap_clip_line",
            "clip_to_selection": "sel_bitmap",
            "brush": "100%_Opaque",
            "size": 20,
            "color": [10, 10, 10, 255],
            "points": [[8.0, 32.0, 1.0], [56.0, 32.0, 1.0]]
        }),
    );
    assert_eq!(
        drawn.get("ok").and_then(Value::as_bool),
        Some(true),
        "笔触本身应当成功：{drawn}"
    );
    let kept = drawn
        .get("painted_pixels_clipped")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    assert!(
        kept > 0,
        "位图选区的不透明部分必须能落墨（修之前 `painted_pixels_clipped=0`）⇒ {drawn}"
    );
    let left = rgba_at(&mut ctx, 16, 32);
    assert_ne!(
        left,
        [255, 255, 255, 255],
        "不透明那一半（x=16）必须有墨 ⇒ 实测 {left:?}"
    );
    let right = rgba_at(&mut ctx, 48, 32);
    assert_eq!(
        right,
        [255, 255, 255, 255],
        "透明那一半（x=48）必须一点墨都没有 ⇒ 实测 {right:?}"
    );
}
