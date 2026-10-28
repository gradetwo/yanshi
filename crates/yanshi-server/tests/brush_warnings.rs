//! **P-16：`oil-06-paint` 的"不按时间出 dab"警告，同一会话同一笔刷只报一次**
//! （paint-fix 期 7）。
//!
//! **四层齐**：实现（去重状态在 `Workspace` 上，键 = 会话 + 笔刷）／输入能触发
//! （`oil-06-paint` + 20 点四元点列，`time_ms` 递增，分诊复现同款输入）／
//! 断言（见下）／变异（删掉去重 ⇒ 5 笔出 5 条警告 ⇒ 红）。
use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

/// **测试结束即删掉临时目录** ✓（第 814 轮；`/tmp` 是 1.9 GB 的 tmpfs ✓）
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn setup(tag: &str) -> Workspace {
    let root = std::env::temp_dir().join(format!("yanshi_warn_{tag}_{}", std::process::id()));
    let _cleanup = Cleanup(root.clone());
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("建根");
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("assets");
    assert!(assets.is_dir(), "找不到资产根：{}", assets.display());
    let mut workspace = Workspace::with_file_store(root, DocumentSettings::default())
        .expect("落盘工作区")
        .with_assets_dir(Some(assets));
    workspace
        .create_document(NewDocument::new("doc_warn", 200, 200), "human:1", "s")
        .expect("create_document");
    workspace
}

/// 20 点四元点列，`time_ms` 递增（分诊复现同款输入）。
fn timed_points() -> serde_json::Value {
    let points: Vec<serde_json::Value> = (0..20)
        .map(|index| json!([10.0 + index as f64 * 8.0, 100.0, 0.8, index as f64 * 50.0]))
        .collect();
    json!(points)
}

fn stroke(
    workspace: &mut Workspace,
    session: &str,
    object_id: &str,
    brush: &str,
    brush_params: Option<serde_json::Value>,
) -> serde_json::Value {
    let registry = ToolRegistry::with_profiles(&Profile::ALL);
    let mut ctx = ToolContext::new(workspace, "doc_warn", "human:1", session)
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    let mut args = json!({
        "layer_id": "layer_default", "object_id": object_id,
        "brush": brush,
        "points": timed_points(),
        "size": 12.0,
        "color": {"r": 200, "g": 120, "b": 40, "a": 255},
    });
    if let Some(params) = brush_params {
        args["brush_params"] = params;
    }
    registry.call(&mut ctx, "brush_stroke", &args)
}

fn warned(out: &serde_json::Value) -> bool {
    out["warning"].is_string()
}

#[test]
fn time_insensitive_brush_warns_only_once_per_session_per_brush() {
    let mut workspace = setup("once");
    // 同一会话同一笔刷连画 5 笔 ⇒ 警告只出现 1 次（第 1 笔）。
    let mut warnings = 0;
    for index in 0..5 {
        let out = stroke(
            &mut workspace,
            "s",
            &format!("o_once_{index}"),
            "oil-06-paint",
            None,
        );
        assert_eq!(out["ok"], json!(true), "{out}");
        if warned(&out) {
            warnings += 1;
            assert!(
                out["warning"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("不按时间出 dab"),
                "警告文案应点名原因：{out}"
            );
        }
    }
    assert_eq!(warnings, 1, "同一会话同一笔刷连画 5 笔，警告应只出现 1 次");

    // 换会话 ⇒ 重新报一次（新调用方可能没见过首次警告）。
    let out = stroke(
        &mut workspace,
        "s2",
        "o_other_session",
        "oil-06-paint",
        None,
    );
    assert_eq!(out["ok"], json!(true), "{out}");
    assert!(warned(&out), "换会话应重新报一次：{out}");

    // `brush_params` 把 `dabs_per_second` 设成 200 ⇒ 这支笔按时间出 dab ⇒ 0 次警告。
    let out = stroke(
        &mut workspace,
        "s3",
        "o_params_override",
        "oil-06-paint",
        Some(json!({"dabs_per_second": 200})),
    );
    assert_eq!(out["ok"], json!(true), "{out}");
    assert!(
        !warned(&out),
        "dabs_per_second 设成 200 后不应再警告：{out}"
    );

    // 换一支 `dabs_per_second > 0` 的笔刷（`1pixel` 是 79.69）⇒ 0 次警告。
    let out = stroke(&mut workspace, "s4", "o_other_brush", "1pixel", None);
    assert_eq!(out["ok"], json!(true), "{out}");
    assert!(!warned(&out), "按时间出 dab 的笔刷不应警告：{out}");
}
