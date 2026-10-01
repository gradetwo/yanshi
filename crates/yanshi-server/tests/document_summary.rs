//! `get_document` 的计数必须与 `list_objects` / `list_layers` **同一口径** ✓。
//!
//! 子 agent 实测（本轮的问题清单 ✓）：删掉 6 个图层之后**同一瞬间**，
//! `get_document` 报 501 个对象 / 14 个图层 ✗，而 `list_objects`/`list_layers` 报 259 / 6 ✓。
//! 根因：折叠层**保留墓碑**（`deleted_by` ✓ —— 这是"删除可撤销、日志可重放"的基础 ✓），
//! 而 `summary_json` 直接数 `state.objects.len()` ✗ ⇒ 把墓碑也算了进去 ✓。
//! 同一个界面里两个数不一致 ✓，用户会以为数据坏了 ✓。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_summary", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

/// **删除之后两个口径必须一致** ✓ —— 这正是子 agent 的复现路径 ✓：
/// 建若干图层与对象 ⇒ 删掉一部分 ⇒ 同时读 `get_document` 与 `list_*` ✓。
#[test]
fn the_document_summary_counts_live_entities_only() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_summary", 128, 128),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    let mut expected_layers = 0usize;
    let mut expected_objects = 0usize;
    {
        let mut ctx = context(&mut workspace);
        for index in 0..4 {
            let layer_id = format!("L{index}");
            let created = registry.call(
                &mut ctx,
                "create_layer",
                &json!({"layer_id": layer_id, "name": layer_id}),
            );
            assert_eq!(created["ok"], json!(true), "{created}");
            expected_layers += 1;
            for slot in 0..3 {
                let drawn = registry.call(
                    &mut ctx,
                    "draw_shape",
                    &json!({
                        "layer_id": layer_id,
                        "object_id": format!("{layer_id}_box{slot}"),
                        "data": {
                            "geometry": {"kind": "rect",
                                         "bbox": {"x": 8.0 + slot as f64 * 12.0, "y": 8.0, "w": 10.0, "h": 10.0}},
                            "color": {"r": 0, "g": 0, "b": 0, "a": 255}
                        }
                    }),
                );
                assert_eq!(drawn["ok"], json!(true), "{drawn}");
                expected_objects += 1;
            }
        }
    }

    let summary_before = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "get_document", &json!({"preview_size": false}))
    };
    assert_eq!(
        (
            summary_before["layers"].as_u64(),
            summary_before["objects"].as_u64()
        ),
        (Some(expected_layers as u64), Some(expected_objects as u64)),
        "删除之前两个口径本来就该一致：{summary_before}"
    );

    // 删掉**一半**图层（连同其中的对象 ✓）。
    {
        let mut ctx = context(&mut workspace);
        for index in 0..2 {
            let removed = registry.call(
                &mut ctx,
                "delete_layer",
                &json!({"layer_id": format!("L{index}")}),
            );
            assert_eq!(removed["ok"], json!(true), "{removed}");
            expected_layers -= 1;
            expected_objects -= 3;
        }
    }

    let listed_layers = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "list_layers", &json!({}))
    };
    let listed_objects = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "list_objects", &json!({}))
    };
    let summary_after = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "get_document", &json!({"preview_size": false}))
    };

    let live_layers = listed_layers["layers"].as_array().map_or(0, Vec::len) as u64;
    let live_objects = listed_objects["objects"].as_array().map_or(0, Vec::len) as u64;
    assert_eq!(
        (live_layers, live_objects),
        (expected_layers as u64, expected_objects as u64),
        "`list_*` 的口径：{listed_layers} / {listed_objects}"
    );

    // **核心断言** ✓：摘要必须与 `list_*` 一致 ✓（此前会多算墓碑 ✗）。
    assert_eq!(
        (
            summary_after["layers"].as_u64(),
            summary_after["objects"].as_u64()
        ),
        (Some(live_layers), Some(live_objects)),
        "`get_document` 必须只数存活实体：{summary_after}"
    );
    // 墓碑数要**单独**可见 ✓（想审计删了多少的人仍看得到 ✓，且不会被误读成当前内容 ✓）。
    assert!(
        summary_after["tombstoned_layers"].as_u64().unwrap_or(0) >= 2,
        "墓碑计数应能反映被删除的图层：{summary_after}"
    );
}

/// **关掉的文档也必须报出真实计数** ✓（回归测试 ✓ —— 修复前这里恒为 0 ✓）。
///
/// **这条测试的来历值得写下来** ✓：列表接口对**未打开的文档**曾经**写死 `layers: 0, objects: 0`** ✗
///（因为计数原本只从内存里的态读 ✓）。后果很实在 ✗：我按列表里的"对象数 = 0"挑选要清理的空文档 ✓
/// ⇒ 把有 **56** 个对象的水彩示例与有 **260** 个对象的笔刷示例当成空文档清掉了 ✗
///（我当时的另一个测量 `get_document` 明明说有 56 ✓ —— **两个来源矛盾时我没有先对账就动手** ✗；
/// 幸好用的是**移动到备份**而不是删除 ✓，已全部恢复 ✓）。
/// 现在两条路径用**同一套折叠** ✓ ⇒ 口径一致 ✓，不会再出现"一个说有、一个说 0" ✓。
#[test]
fn closed_documents_report_their_real_counts() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "yanshi-doc-summary-{}-{}",
        std::process::id(),
        yanshi_core::now_ms()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let mut workspace = Workspace::with_file_store(&root, DocumentSettings::default()).unwrap();
    workspace
        .create_document(
            NewDocument::new("closed_doc", 128, 128),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = ToolContext::new(&mut workspace, "closed_doc", "human:1", "session:test")
            .with_owner(true)
            .with_wait_for_render(true, 4_000);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
        for slot in 0..2 {
            let drawn = registry.call(
                &mut ctx,
                "draw_shape",
                &json!({"layer_id": "L", "object_id": format!("box{slot}"), "data": {
                    "geometry": {"kind": "rect", "bbox": {"x": 4.0, "y": 4.0, "w": 8.0, "h": 8.0}},
                    "color": {"r": 30, "g": 30, "b": 30, "a": 255}}}),
            );
            assert_eq!(drawn["ok"], json!(true), "{drawn}");
        }
    }
    // **先看"打开着"的口径** ✓（这是基准 ✓）。
    let while_open = workspace
        .list_documents()
        .expect("列表应成功")
        .into_iter()
        .find(|summary| summary.doc_id == "closed_doc")
        .expect("应能找到该文档");
    assert_eq!(while_open.layers, 1, "打开着：图层数");
    assert_eq!(while_open.objects, 2, "打开着：对象数");
    // **关掉它** ✓ —— 修复前这里会变成 0 ✗（而磁盘上的原子一条没少 ✓）。
    workspace.close_document("closed_doc");
    let while_closed = workspace
        .list_documents()
        .expect("列表应成功")
        .into_iter()
        .find(|summary| summary.doc_id == "closed_doc")
        .expect("关闭后**仍然**应该列出来 ✓（文档没被删 ✓）");
    assert_eq!(
        (while_closed.layers, while_closed.objects),
        (1, 2),
        "关闭后计数应与打开时**一致** ✓（修复前恒为 0 ✗）"
    );
    assert!(while_closed.persisted, "磁盘上仍然保留 ✓");
    let _ = std::fs::remove_dir_all(&root);
}

/// **三条路的计数必须是同一个数** ✓（打开中 / 关闭后 / `get_document` ✓）。
///
/// **这条用例的来历** ✓：用户报告"示例打开都是空白" ✓，我去量 `default` 文档 ✓，
/// 发现**同一瞬间**列表说"241 个对象" ✗、而 `list_objects` 说"0 个" ✓ ——
/// 根因是列表对**已打开**的文档直接数 `state.objects.len()` ✓（**含墓碑** ✗），
/// 而我上一轮只修了**未打开**那一路 ✓ ⇒ 两条路又不一致 ✓。
/// **教训** ✓：同一个事实只应有一处算法 ✓；"两处各算一遍"迟早会分叉 ✓。
#[test]
fn list_and_summary_agree_on_live_counts_for_open_and_closed_documents() {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "yanshi-live-counts-{}-{}",
        std::process::id(),
        yanshi_core::now_ms()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let mut workspace = Workspace::with_file_store(&root, DocumentSettings::default()).unwrap();
    workspace
        .create_document(
            NewDocument::new("counts", 128, 128),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = ToolContext::new(&mut workspace, "counts", "human:1", "session:test")
            .with_owner(true)
            .with_wait_for_render(true, 4_000);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
        for slot in 0..3 {
            registry.call(
                &mut ctx,
                "draw_shape",
                &json!({"layer_id": "L", "object_id": format!("b{slot}"), "data": {
                    "geometry": {"kind": "rect", "bbox": {"x": 4.0, "y": 4.0, "w": 6.0, "h": 6.0}},
                    "color": {"r": 30, "g": 30, "b": 30, "a": 255}}}),
            );
        }
        // **删掉两个** ✓ ⇒ 存活 1 个 ✓、墓碑 2 个 ✓（这就是分叉的土壤 ✓）。
        for slot in 0..2 {
            registry.call(
                &mut ctx,
                "delete_object",
                &json!({"object_id": format!("b{slot}")}),
            );
        }
    }
    let listed = |workspace: &mut Workspace| -> (usize, usize) {
        let summary = workspace
            .list_documents()
            .expect("列表应成功")
            .into_iter()
            .find(|item| item.doc_id == "counts")
            .expect("应能找到该文档");
        (summary.layers, summary.objects)
    };
    let live_objects = |workspace: &mut Workspace| -> usize {
        let listed = {
            let mut ctx =
                ToolContext::new(workspace, "counts", "human:1", "session:test").with_owner(true);
            registry.call(&mut ctx, "list_objects", &json!({}))
        };
        listed["objects"].as_array().map(Vec::len).unwrap_or(0)
    };
    // **打开中** ✓：列表与 `list_objects` 必须一致 ✓（修复前是 (1, 3) vs 1 ✗）。
    let open_side = listed(&mut workspace);
    assert_eq!(
        open_side.1,
        live_objects(&mut workspace),
        "打开中：列表对象数应等于 list_objects"
    );
    assert_eq!(open_side, (1, 1), "打开中：1 层 1 个存活对象（墓碑不算 ✓）");
    // **`get_document` 那一侧由本文件第一条用例覆盖** ✓（它比较 `get_document` 与 `list_*` ✓）；
    // 这里只钉"列表（打开中）↔列表（关闭后）↔list_objects"三者 ✓。
    // 注：我第一版在这里写了 `workspace.get_document_summary(...)` ✗ —— 那个方法不存在 ✓，
    // 编译当场报错 ✓（幸好不是靠肉眼发现 ✓）。
    // **关闭后** ✓：同一个数 ✓（走的是磁盘折叠那一路 ✓）。
    workspace.close_document("counts");
    assert_eq!(
        listed(&mut workspace),
        open_side,
        "关闭后与打开中必须一致 ✓"
    );
    let _ = std::fs::remove_dir_all(&root);
}
