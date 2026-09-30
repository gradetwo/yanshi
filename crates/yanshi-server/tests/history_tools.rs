//! history 组的**只读**工具（设计 776 只列了名字 ⇒ 语义由本仓库定义并记录 ✓）。
//!
//! 五个工具的分工 ✓（互不重复 ✓）：
//! * `get_object_history` ✓：一个对象**当前生效的原子版本链**（它是怎么变成现在这样的 ✓）；
//! * `find_atom` ✓：**日志检索**（比 `get_log` 多了 `object_id`/`layer_id` 过滤 ✓ + **匹配总数** ✓）；
//! * `get_diff` ✓：**日志层**的两个序号之差（原子 ✓、按种类汇总 ✓、涉及的对象与图层 ✓）；
//! * `get_ancestors` / `get_descendants` ✓：**引用图**的上下两个方向 ✓
//!   （对象自身的原子链归 `get_object_history` ✓ ⇒ 两者不重叠 ✓）。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_hist", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

/// 画一笔并再 `close` 一次 ⇒ 这个对象应当有**两个**生效版本 ✓。
fn build_stroke(workspace: &mut Workspace, registry: &ToolRegistry) {
    let mut ctx = context(workspace);
    registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    registry.call(
        &mut ctx,
        "draw_stroke",
        &json!({"layer_id": "L", "object_id": "s1",
                "data": {"points": [[4.0, 4.0], [40.0, 4.0], [40.0, 30.0]], "size": 5.0,
                         "color": {"r": 10, "g": 10, "b": 10, "a": 255}}}),
    );
    let closed = registry.call(
        &mut ctx,
        "path_edit",
        &json!({"op": "close", "object_id": "s1"}),
    );
    assert_eq!(closed["ok"], json!(true), "{closed}");
}

/// **版本链**：两条原子 ✓（原始落笔 ✓ + `close` 的 supersede ✓），且给出种类与是否被撤销 ✓。
#[test]
fn object_history_lists_the_effective_version_chain() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_hist", 128, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    build_stroke(&mut workspace, &registry);

    let history = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "get_object_history", &json!({"object_id": "s1"}))
    };
    assert_eq!(history["ok"], json!(true), "{history}");
    assert_eq!(history["object_id"], json!("s1"), "{history}");
    assert_eq!(history["deleted"], json!(false), "{history}");
    let versions = history["versions"].as_array().cloned().unwrap_or_default();
    assert_eq!(
        versions.len(),
        2,
        "落笔 + close 应有两条生效版本：{history}"
    );
    assert!(
        // 种类名取自 `AtomKind::as_str()` ✓ —— 是 `draw_stroke` ✓（我第一版写 `stroke` ✗，
        // 测试当场指出 ✓；工具本身是对的 ✓）。
        versions
            .iter()
            .any(|atom| atom["kind"] == json!("draw_stroke")),
        "应含原始落笔：{history}"
    );
    assert!(
        versions
            .iter()
            .any(|atom| atom["kind"] == json!("supersede")),
        "应含 close 的 supersede：{history}"
    );
    assert!(
        versions.iter().all(|atom| atom["reverted"] == json!(false)),
        "都没有被撤销：{history}"
    );
    // 序号应递增 ✓（链路顺序 = 日志顺序 ✓）。
    let seqs: Vec<u64> = versions
        .iter()
        .filter_map(|atom| atom["seq"].as_u64())
        .collect();
    assert!(
        seqs.windows(2).all(|pair| pair[0] < pair[1]),
        "序号应递增：{seqs:?}"
    );
}

/// **`find_atom` 比 `get_log` 多两件事** ✓：按对象过滤 ✓ + 报告匹配总数 ✓。
#[test]
fn find_atom_filters_by_object_and_reports_the_total() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_hist", 128, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    build_stroke(&mut workspace, &registry);
    {
        let mut ctx = context(&mut workspace);
        // 另一个对象 ✓（不该出现在 s1 的检索结果里 ✓）。
        registry.call(
            &mut ctx,
            "draw_stroke",
            &json!({"layer_id": "L", "object_id": "s2",
                    "data": {"points": [[4.0, 40.0], [60.0, 40.0]], "size": 5.0,
                             "color": {"r": 10, "g": 10, "b": 10, "a": 255}}}),
        );
    }
    let found = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "find_atom", &json!({"object_id": "s1"}))
    };
    assert_eq!(found["ok"], json!(true), "{found}");
    let atoms = found["atoms"].as_array().cloned().unwrap_or_default();
    assert_eq!(atoms.len(), 2, "s1 有两条原子：{found}");
    assert!(
        atoms.iter().all(|atom| atom["object_id"] == json!("s1")),
        "不该混入别的对象：{found}"
    );
    assert_eq!(found["total_matched"], json!(2), "{found}");
    assert_eq!(found["truncated"], json!(false), "{found}");

    // **截断要如实报告** ✓：limit 1 ⇒ 只返回 1 条 ✓，但总数仍是 2 ✓。
    let limited = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "find_atom",
            &json!({"object_id": "s1", "limit": 1}),
        )
    };
    assert_eq!(limited["returned"], json!(1), "{limited}");
    assert_eq!(limited["total_matched"], json!(2), "{limited}");
    assert_eq!(
        limited["truncated"],
        json!(true),
        "截断必须如实报告：{limited}"
    );

    // 按种类过滤 ✓。
    let only_supersede = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "find_atom",
            &json!({"object_id": "s1", "kind": "supersede"}),
        )
    };
    assert_eq!(
        only_supersede["total_matched"],
        json!(1),
        "{only_supersede}"
    );
}

/// **`get_diff` 是日志层差分** ✓：区间内的原子 ✓、按种类汇总 ✓、涉及的对象与图层 ✓。
#[test]
fn get_diff_reports_the_atoms_and_what_they_touch() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_hist", 128, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    build_stroke(&mut workspace, &registry);
    let head = {
        let mut ctx = context(&mut workspace);
        let history = registry.call(&mut ctx, "get_object_history", &json!({"object_id": "s1"}));
        history["head_seq"].as_u64().unwrap_or(0)
    };
    let diff = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "get_diff", &json!({"from_seq": 0}))
    };
    assert_eq!(diff["ok"], json!(true), "{diff}");
    assert_eq!(diff["from_seq"], json!(0), "{diff}");
    assert_eq!(diff["to_seq"], json!(head), "缺省应到 HEAD：{diff}");
    assert!(diff["atom_count"].as_u64().unwrap_or(0) > 0, "{diff}");
    assert!(
        diff["by_kind"]["create_document"].as_u64().unwrap_or(0) >= 1,
        "建文档也算在差分里：{diff}"
    );
    assert!(
        diff["objects"]
            .as_array()
            .map(|items| items.iter().any(|item| item == &json!("s1")))
            .unwrap_or(false),
        "s1 应出现在涉及对象里：{diff}"
    );
    assert!(
        diff["layers"]
            .as_array()
            .map(|items| items.iter().any(|item| item == &json!("L")))
            .unwrap_or(false),
        "L 应出现在涉及图层里：{diff}"
    );
    // **空区间**：from == head ⇒ 没有原子 ✓（这也是"什么都没发生"的正确表达 ✓）。
    let empty = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "get_diff", &json!({"from_seq": head}))
    };
    assert_eq!(empty["atom_count"], json!(0), "{empty}");
    // **区间反向要报错** ✓，而不是给一个空结果 ✓。
    let bad = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "get_diff",
            &json!({"from_seq": head, "to_seq": 0}),
        )
    };
    assert_eq!(bad["ok"], json!(false), "{bad}");
}

/// **引用图两个方向** ✓：实例的 `ancestors` 是 master ✓；master 的 `descendants` 是实例 ✓。
#[test]
fn ancestors_and_descendants_walk_the_reference_graph() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_hist", 256, 256),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
        registry.call(
            &mut ctx,
            "draw_shape",
            &json!({"layer_id": "L", "object_id": "master", "data": {
                "geometry": {"kind": "rect", "bbox": {"x": 4.0, "y": 4.0, "w": 10.0, "h": 10.0}},
                "color": {"r": 200, "g": 30, "b": 30, "a": 255}}}),
        );
        registry.call(
            &mut ctx,
            "create_instance",
            &json!({"instance_id": "mirror", "layer_id": "L", "master_id": "master"}),
        );
        // 再套一层 ✓（传递闭包 ✓）。
        registry.call(
            &mut ctx,
            "create_instance",
            &json!({"instance_id": "mirror2", "layer_id": "L", "master_id": "mirror"}),
        );
    }
    let ancestors = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "get_ancestors", &json!({"object_id": "mirror2"}))
    };
    let chain: Vec<String> = ancestors["ancestors"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item["object_id"].as_str())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        chain,
        vec!["mirror".to_owned(), "master".to_owned()],
        "{ancestors}"
    );

    let descendants = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "get_descendants", &json!({"object_id": "master"}))
    };
    let mut found: Vec<String> = descendants["descendants"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    assert_eq!(
        found,
        vec!["mirror".to_owned(), "mirror2".to_owned()],
        "{descendants}"
    );
    assert_eq!(descendants["count"], json!(2), "{descendants}");

    // 不存在的对象 ✓ ⇒ 明确报错 ✓。
    let ghost = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "get_descendants", &json!({"object_id": "nope"}))
    };
    assert_eq!(ghost["ok"], json!(false), "{ghost}");
}
