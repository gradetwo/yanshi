//! Blob 孤儿回收（设计 6.3）的入口与**安全保证**。
//!
//! 背景：`Document::collect_garbage` 早已实现，却**没有任何入口** —— 渲染产生的 blob
//! （预览、导出、逐像素自检）不被任何原子引用，只能等 TTL 到期，而没有东西触发 GC。
//! 实测一个工作区因此累积到 1.3GB（其中 4K 剖面约 738MB、整幅 1024² 约 277MB）。
//!
//! 工具 `collect_garbage` **默认干跑**，`confirm: true` 才删除。本文件固定两件事：
//! 1. 超过 TTL 的孤儿会被回收；
//! 2. **日志引用闭包（根集）里的 blob 永远不被回收**（revert / 时间旅行 / Stash 不丢数据）。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

const TTL_DAYS_MS: i64 = 8 * 24 * 60 * 60 * 1000;

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace, now: i64) -> ToolContext<'a> {
    let mut ctx = ToolContext::new(workspace, "doc_gc", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    ctx.now = now;
    ctx
}

fn hash_of(url: &str) -> yanshi_core::atom::BlobHash {
    url.trim_start_matches("yanshi://blob/")
        .parse()
        .unwrap_or_else(|error| panic!("blob 地址 {url} 非法：{error}"))
}

/// 建一个含**原子引用 blob** 的文档（patch 把源区域写入 CAS），返回根集。
fn document_with_roots(workspace: &mut Workspace) -> Vec<yanshi_core::atom::BlobHash> {
    workspace
        .create_document(
            NewDocument::new("doc_gc", 256, 256),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    let mut ctx = context(workspace, yanshi_core::now_ms());
    registry.call(&mut ctx, "create_layer", &json!({"layer_id": "layer_1"}));
    registry.call(
        &mut ctx,
        "draw_shape",
        &json!({
            "layer_id": "layer_1",
            "object_id": "obj_src",
            "data": {"geometry": {"kind": "rect", "bbox": {"x": 20, "y": 20, "w": 80, "h": 80}},
                     "color": {"r": 200, "g": 60, "b": 60, "a": 255}}
        }),
    );
    let response = registry.call(
        &mut ctx,
        "patch",
        &json!({
            "layer_id": "layer_1",
            "object_id": "obj_patch",
            "target": [120, 120],
            "source_region": {"x": 20, "y": 20, "w": 80, "h": 80}
        }),
    );
    assert_eq!(response["ok"], json!(true), "patch 应成功：{response}");

    let plan = workspace
        .document_mut("doc_gc")
        .unwrap()
        .plan_garbage(yanshi_core::now_ms())
        .unwrap();
    assert!(!plan.roots.is_empty(), "patch 之后根集不应为空");
    plan.roots.into_iter().collect()
}

/// 干跑只报告；确认后回收超 TTL 的孤儿，且**引用中的 blob 一个都不能少**。
#[test]
fn collect_garbage_reclaims_orphans_but_never_referenced_blobs() {
    let mut workspace = workspace();
    let roots = document_with_roots(&mut workspace);

    // 造孤儿：显式渲染会把 PNG 写进 CAS，但没有任何原子引用它。
    let render = {
        let mut ctx = context(&mut workspace, yanshi_core::now_ms());
        registry().call(
            &mut ctx,
            "render_region",
            &json!({"region": {"x": 0, "y": 0, "w": 256, "h": 256}}),
        )
    };
    let orphan = hash_of(render["blob_hash"].as_str().expect("渲染应返回 blob"));
    assert!(workspace.store().exists(&orphan));

    // 干跑：只报告，不删除。
    let dry = {
        let mut ctx = context(&mut workspace, yanshi_core::now_ms());
        registry().call(&mut ctx, "collect_garbage", &json!({}))
    };
    assert_eq!(dry["dry_run"], json!(true), "{dry}");
    assert_eq!(dry["reclaimed_blobs"], json!(0), "干跑不应删除：{dry}");
    assert!(dry["orphans"].as_u64().unwrap_or(0) >= 1, "{dry}");
    assert!(dry["orphan_bytes"].as_u64().unwrap_or(0) > 0, "{dry}");
    assert!(workspace.store().exists(&orphan), "干跑后孤儿仍在");

    // 确认回收：时钟推到 TTL 之后，孤儿才过期。
    let confirmed = {
        let mut ctx = context(&mut workspace, yanshi_core::now_ms() + TTL_DAYS_MS);
        registry().call(&mut ctx, "collect_garbage", &json!({"confirm": true}))
    };
    assert_eq!(confirmed["dry_run"], json!(false), "{confirmed}");
    assert!(
        confirmed["reclaimed_blobs"].as_u64().unwrap_or(0) >= 1,
        "超过 TTL 的孤儿应被回收：{confirmed}"
    );
    assert!(
        confirmed["reclaimed_bytes"].as_u64().unwrap_or(0) > 0,
        "应报告回收字节数：{confirmed}"
    );
    assert!(
        !workspace.store().exists(&orphan),
        "孤儿应在确认回收后被删除"
    );

    // 关键安全保证：日志引用闭包（根集）里的每个 blob 都必须仍在。
    for hash in &roots {
        assert!(
            workspace.store().exists(hash),
            "被日志引用的 blob {hash} 绝不能因 GC 丢失（设计 6.3）"
        );
    }
}

/// 未超过 TTL 的孤儿保留（避免删掉正在用、尚未提交引用的临时 blob）。
#[test]
fn orphans_within_ttl_are_retained() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_gc", 128, 128),
            "human:1",
            "session:test",
        )
        .unwrap();
    let render = {
        let mut ctx = context(&mut workspace, yanshi_core::now_ms());
        registry().call(
            &mut ctx,
            "render_region",
            &json!({"region": {"x": 0, "y": 0, "w": 128, "h": 128}}),
        )
    };
    let orphan = hash_of(render["blob_hash"].as_str().unwrap());
    let confirmed = {
        let mut ctx = context(&mut workspace, yanshi_core::now_ms());
        registry().call(&mut ctx, "collect_garbage", &json!({"confirm": true}))
    };
    assert_eq!(confirmed["reclaimed_blobs"], json!(0), "{confirmed}");
    assert!(
        workspace.store().exists(&orphan),
        "未超过 TTL 的孤儿应保留（缺省 TTL 7 天）"
    );
}

/// **计划绝不允许删除任何东西**。
///
/// 真实缺陷：`Document::plan_garbage` 曾调用会删除的 `run_gc` 并丢弃报告 ✗ ——
/// 于是"干跑"实际上删掉了过期孤儿；工具层先计划后回收的两步写法让报告与实况完全对不上
/// （表现为 `expiring > 0` 却 `reclaimed == 0`）。这条断言把"计划是纯函数"固定住。
#[test]
fn planning_never_deletes_anything() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_gc", 128, 128),
            "human:1",
            "session:test",
        )
        .unwrap();
    let render = {
        let mut ctx = context(&mut workspace, yanshi_core::now_ms());
        registry().call(
            &mut ctx,
            "render_region",
            &json!({"region": {"x": 0, "y": 0, "w": 128, "h": 128}}),
        )
    };
    let orphan = hash_of(render["blob_hash"].as_str().unwrap());

    // 把时钟推到 TTL 之后再看计划：即使已过期，计划也不得删除。
    let later = yanshi_core::now_ms() + TTL_DAYS_MS;
    let plan = workspace
        .document_mut("doc_gc")
        .unwrap()
        .plan_garbage(later)
        .unwrap();
    assert!(
        !plan.expiring.is_empty(),
        "TTL 之后应能列出可清理的孤儿（否则本测试没测到点子上）"
    );
    assert!(
        workspace.store().exists(&orphan),
        "生成计划不得删除任何 blob（plan 必须是纯函数）"
    );
}

/// **跨文档安全**：GC 从文档 B 触发，绝不能删掉文档 A 引用的 blob。
///
/// 真实缺陷（由干跑发现）：GC 曾按**当前文档**取根集 ✗ —— 某工作区 280 个 blob
/// 在单文档视角下**全部**显示为孤儿，真删就会毁掉其它文档的数据。
/// 修法：根集 = **所有文档**的引用闭包并集。
#[test]
fn gc_from_one_document_never_deletes_another_documents_blobs() {
    let mut workspace = workspace();
    // 文档 A：含 patch 引用 blob。
    let roots_a = document_with_roots(&mut workspace);
    // 文档 B：另一个文档，并且造一个真正的孤儿（显式渲染）。
    workspace
        .create_document(
            NewDocument::new("doc_b", 128, 128),
            "human:1",
            "session:test",
        )
        .unwrap();
    let render = {
        let mut ctx = ToolContext::new(&mut workspace, "doc_b", "human:1", "session:test")
            .with_owner(true)
            .with_wait_for_render(true, 4_000);
        ctx.now = yanshi_core::now_ms();
        registry().call(
            &mut ctx,
            "render_region",
            &json!({"region": {"x": 0, "y": 0, "w": 128, "h": 128}}),
        )
    };
    let orphan_b = hash_of(render["blob_hash"].as_str().unwrap());

    // 从**文档 B** 触发工作区级 GC（时钟推到 TTL 之后）。
    let confirmed = {
        let mut ctx = ToolContext::new(&mut workspace, "doc_b", "human:1", "session:test")
            .with_owner(true)
            .with_wait_for_render(true, 4_000);
        ctx.now = yanshi_core::now_ms() + TTL_DAYS_MS;
        registry().call(&mut ctx, "collect_garbage", &json!({"confirm": true}))
    };
    assert_eq!(confirmed["dry_run"], json!(false), "{confirmed}");
    assert!(
        confirmed["reclaimed_blobs"].as_u64().unwrap_or(0) >= 1,
        "文档 B 的孤儿应被回收：{confirmed}"
    );
    assert!(
        !workspace.store().exists(&orphan_b),
        "文档 B 的孤儿应被删除"
    );
    for hash in &roots_a {
        assert!(
            workspace.store().exists(hash),
            "文档 A 引用的 blob {hash} 被文档 B 触发的 GC 删掉了（跨文档数据丢失）"
        );
    }
}
