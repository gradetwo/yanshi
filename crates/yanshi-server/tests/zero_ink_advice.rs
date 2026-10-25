//! **零像素笔触的建议必须落在真实原因上**（外部评测包 `B3`）。
//!
//! **他们报的**：`size=120`、两点、路径总长 **15.2px** 的笔被拒画，而报错首推
//! 「加大 size 到 4 以上」⇒ 对 `size=120` 是**无效建议**（真实原因是路径太短）。
//!
//! **本判据钉两条**（各自对应一次真实的误诊）：
//! ① **大笔刷 ＋ 短路径不再变成 0 像素** —— 当前 HEAD 实测 **9712 像素、steps=9** ✓
//!    （他们那次是整笔拒画 ⇒ 这条会盯住回归）；
//! ② **真 0 像素**（`calligraphy` size=2、路径总长 0.4px ⇒ 撑不满一个 dab 间距）
//!    仍要报错，且**文案不重复**开头那句，并且**首推"加长路径"**（这一档的原因
//!    是路径太短；P2-13 之前这一档是 AA 把 hardness 压成负数，size=2 长路径也
//!    0 墨——修后小 size 能落墨，② 的输入改成超短路径，原因档位随之改成路径太短）。
//!
//! **变异**：把 0 像素改回 `ok:true` ⇒ ② 红（该拒绝的成了成功）；把结论句写死成
//! 「几乎都是 size 太小」⇒ ①/② 里对 `cause` 的断言仍能过，但 `size=2` 那句会同时出现两次 ⇒ 重复断言红。

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
    ToolContext::new(workspace, "doc_zero_ink", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 8_000)
}

fn call(ctx: &mut ToolContext<'_>, name: &str, args: &Value) -> Value {
    registry().call(ctx, name, args)
}

fn detail_of(body: &Value) -> String {
    body.get("context")
        .and_then(|context| context.get("detail"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

#[test]
fn a_large_brush_on_a_short_path_paints_and_a_tiny_size_still_explains_itself() {
    let mut workspace = workspace();
    {
        let mut ctx = context(&mut workspace);
        call(
            &mut ctx,
            "new_document",
            &json!({"doc_id": "doc_zero_ink", "width": 200, "height": 200}),
        );
    }
    let mut ctx = context(&mut workspace);

    // ① **他们那一笔的构型**：size=120、两点、15.2px ⇒ 现在必须**真的落墨**。
    let short = call(
        &mut ctx,
        "brush_stroke",
        &json!({
            "layer_id": "layer_default",
            "brush": "oil-06-paint",
            "size": 120,
            "points": [[40.0, 40.0, 1.0], [55.2, 40.0, 1.0]]
        }),
    );
    assert_eq!(
        short.get("ok").and_then(Value::as_bool),
        Some(true),
        "大笔刷 ＋ 短路径不该整笔拒画（外部 B3 的构型）：{short}"
    );
    let painted = short
        .get("painted_pixels")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    assert!(
        painted > 0,
        "这一笔必须落墨（他们当时是 0 像素）⇒ 实测 painted_pixels={painted}"
    );

    // ② **真 0 像素**（size=2 的书法笔、路径总长 0.4px ⇒ 撑不满一个 dab 间距；
    //    `dabs_per_basic_radius=0` 时间距钳在下限 1.0px）仍必须拒绝，而且文案要干净、排序要对。
    //    P2-13 之前这一档是 AA 把 hardness 压成负数（size=2 长路径也 0 墨）⇒
    //    修后小 size 能落墨，② 的输入改成超短路径，原因档位随之改成路径太短。
    let tiny = call(
        &mut ctx,
        "brush_stroke",
        &json!({
            "layer_id": "layer_default",
            "brush": "calligraphy",
            "size": 2,
            "points": [[40.0, 40.0, 1.0], [40.4, 40.0, 1.0]]
        }),
    );
    assert_eq!(
        tiny.get("ok").and_then(Value::as_bool),
        Some(false),
        "0 像素不许回 ok:true（那是静默降级）：{tiny}"
    );
    let detail = detail_of(&tiny);
    let opening = "这一笔没落下任何像素";
    assert_eq!(
        detail.matches(opening).count(),
        1,
        "开头那句只许出现一次（原来会重复成「…没落下任何像素 ⇒ 笔刷「calligraphy.myb」这一笔没落下任何像素…」）⇒ {detail}"
    );
    assert!(
        detail.contains("路径太短"),
        "0.4px 路径这一档的原因**确实是路径太短** ⇒ 结论句必须这么写：{detail}"
    );
    // 建议顺序：`path_len < size` 这一档首推「加长路径」（加大 size 只会让比值更小）。
    //   **量法**：从「①」之后量（“怎么改”后面的括号里“不是一律先加大 size”本身就含
    //   「加大 size」，从“怎么改”起量会被括号误导——旧测试就是这么被括号带偏的）。
    let advice = detail.split("怎么改").nth(1).unwrap_or_default();
    let numbered = advice.split('①').nth(1).unwrap_or_default();
    let path_at = numbered.find("加长路径").unwrap_or(usize::MAX);
    let size_at = numbered.find("加大 size").unwrap_or(usize::MAX);
    assert!(
        path_at < size_at,
        "`path_len < size` 这一档必须先推「加长路径」（实测顺序 路径@{path_at} vs size@{size_at}）：{advice}"
    );
}
