//! **`invert:true` 的选区拿来做裁剪**（外部评测包 B2）—— 覆盖度不许恒 0。
//!
//! **他们报的**（`yanshi-painting-eval-v10/reports/问题与BUG清单.md` B2）：
//! 「选区 `invert:true` 用于裁剪时覆盖度恒为 0 —— 静默不落墨」。
//!
//! **本测试钉的是契约本身**（与实现无关）：
//! 选区是 `rect(40,30,40,36)` ＋ `invert:true` ⇒ **矩形外可以落墨**、**矩形内不许落墨**。
//! 若覆盖度恒 0（B2 成立）⇒ 外侧那一笔一点墨都没有 ⇒ ① 立刻红。
//! 若 invert 被忽略（只当普通选区）⇒ 内侧被画上墨 ⇒ ② 立刻红。
//!
//! **两条一起才有意义**：只断「外侧有墨」的话，"invert 被完全忽略"也能过 ✗。

use serde_json::{json, Value};
use yanshi_server::{DocumentSettings, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    // **内置笔刷资产**：与 `tests/selection_clip.rs` 同口径（否则「共 0 支」⇒ reference_not_found）。
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    Workspace::in_memory(DocumentSettings::default()).with_assets_dir(Some(assets))
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_invert_clip", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 8_000)
}

fn call(ctx: &mut ToolContext<'_>, name: &str, args: &Value) -> Value {
    registry().call(ctx, name, args)
}

#[test]
fn an_inverted_selection_clips_the_inside_out_not_everything() {
    let mut workspace = workspace();
    {
        let mut ctx = context(&mut workspace);
        call(
            &mut ctx,
            "new_document",
            &json!({"doc_id": "doc_invert_clip", "width": 128, "height": 96}),
        );
        let layers = call(&mut ctx, "list_layers", &json!({}));
        let layer = layers
            .get("layers")
            .and_then(Value::as_array)
            .and_then(|layers| layers.first())
            .and_then(|layer| layer.get("layer_id").or_else(|| layer.get("id")))
            .and_then(Value::as_str)
            .expect("新文档应有图层")
            .to_string();
        // 选区 = 矩形 (40,30)-(80,66)，**反选**。
        let selection = call(
            &mut ctx,
            "create_selection",
            &json!({
                "selection_id": "sel_inv",
                "shape": {"kind": "rect", "bbox": {"x": 40, "y": 30, "w": 40, "h": 36}},
                "invert": true
            }),
        );
        assert_eq!(
            selection.get("ok").and_then(Value::as_bool),
            Some(true),
            "前置：反选创建失败：{selection}"
        );
        // 一笔横穿整个画布、**落在矩形上方（y=10）**：反选语义下这一笔**应当能落墨**。
        let drawn = call(
            &mut ctx,
            "brush_stroke",
            &json!({
                "layer_id": layer,
                "object_id": "inv_line",
                "clip_to_selection": "sel_inv",
                "brush": "2B_pencil",
                "size": 12,
                "points": [[10.0, 10.0, 1.0], [118.0, 10.0, 1.0]]
            }),
        );
        assert_eq!(
            drawn.get("ok").and_then(Value::as_bool),
            Some(true),
            "brush_stroke 失败：{drawn}"
        );
        // 另一笔落在矩形**内部**（y=48）：反选语义下**不许有墨**。
        //   注意：笔触的落点要离矩形边留足余量 —— `2B_pencil` 的印章比点坐标
        //   向外伸约 10px（上一版点从 x=46 起 ⇒ 印章伸到 x=36，出了矩形左缘 ⇒
        //   反选语义下那 5 个边缘像素是**真落在矩形外**的墨，`clipped` 报 5 才对；
        //   旧计数把透明像素也算进 `dropped` ⇒ 649−880 饱和成 0，才显得"全裁掉" ✗）。
        //   点取 [54,48]→[66,48] ⇒ 印章约 x=44..76，整笔都在矩形 (40..80) 内 ✓。
        let inside = call(
            &mut ctx,
            "brush_stroke",
            &json!({
                "layer_id": layer,
                "object_id": "inv_inside",
                "clip_to_selection": "sel_inv",
                "brush": "2B_pencil",
                "size": 12,
                "points": [[54.0, 48.0, 1.0], [66.0, 48.0, 1.0]]
            }),
        );
        assert_eq!(
            inside.get("ok").and_then(Value::as_bool),
            Some(true),
            "内测那一笔调用本身不该失败：{inside}"
        );
        // **断结构量，不断像素数** ✓（第 167 轮规矩）：
        // 画布是白底 ⇒ `sample_colors` 的 alpha 恒 255 ⇒ **不能用 alpha 判断有没有墨** ✗
        // （我第一次就是这么写的，红得莫名其妙 ✓）。用响应里的**裁剪计数**才是这条契约的中间量 ✓：
        //   `painted_pixels`         = 印章实际盖到的像素（裁剪前 ✓）
        //   `painted_pixels_clipped` = 裁剪后**真实留下的**墨 ✓
        let painted_of = |value: &Value| -> u64 {
            value
                .get("painted_pixels")
                .and_then(Value::as_u64)
                .unwrap_or_else(|| panic!("响应里没有 painted_pixels：{value}"))
        };
        let kept_of = |value: &Value| -> u64 {
            value
                .get("painted_pixels_clipped")
                .and_then(Value::as_u64)
                .unwrap_or_else(|| panic!("响应里没有 painted_pixels_clipped：{value}"))
        };
        // ① 反选**之外**：墨必须留下（B2 成立时这里是 0）。
        assert!(
            painted_of(&drawn) > 0,
            "外侧那一笔本身就该盖到像素（前置）：{drawn}"
        );
        assert!(
            kept_of(&drawn) > 0,
            "反选选区之外必须能落墨（外部 B2：覆盖度恒 0 ⇒ 一点墨都没有）⇒ {drawn}"
        );
        // ② 反选**之内**：一个像素都不许留，而且**不是因为没盖到**（`painted_pixels` 要 > 0）。
        assert!(
            painted_of(&inside) > 0,
            "内侧那一笔必须真的盖到了像素（否则②只是「什么都没做」✗）：{inside}"
        );
        assert_eq!(kept_of(&inside), 0, "反选选区**之内**不许落墨 ⇒ {inside}");
    }
}
