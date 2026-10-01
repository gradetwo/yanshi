//! 预览/缩略图是**缓存**，不是被原子引用的内容 ⇒ 换新的必须**淘汰旧的** ✓（设计 §6.4/§7.3 ✓）。
//!
//! **为什么值得一条专门的测试** ✓：这是**一处小疏忽在每笔提交上复利**的典型 ✓ ——
//! 实测真实工作区 **2161 个 blob 里 1912 个是孤儿、共 1.07 GB（约 95%）** ✗，
//! 全部来自"每次提交渲染一份预览、写了新的却没删旧的" ✓。单个体积中位只有 55 KB ✓ ⇒
//! 谁都想不到它会长成最大的一块占用 ✗。**所以这条不变量要有测试守着** ✓。
//!
//! **断言的是"有界"，不是"只有一份"** ✓ —— 这一条是我先写错、被实测纠正的 ✗：
//! 文档里**同时**存在**两级**预览缓存 ✓（区域预览 `last_render_blob` ✓ 与文档级缩略图 ✓），
//! 而局部区域的渲染**不会**更新缩略图 ✓ ⇒ 旧的那份**必须留着** ✓（否则缩略图会 404 ✗）。
//! 所以正确的不变量是：**CAS 里的预览数不随渲染次数增长** ✓（被级数封顶 ✓）。

use serde_json::json;
use yanshi_core::Bbox;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_pv", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn blob_count(workspace: &mut Workspace) -> usize {
    workspace.store().list().expect("应能列出 blob").len()
}

/// 文档里存在的**预览缓存槽** ✓ —— **三个** ✓，我第一版只数了两个 ✗（于是"第 2 轮 3 份"被我误判成泄漏 ✗）：
/// ① 区域预览 `last_render_blob` ✓；② 文档级缩略图 `document_thumbnail` ✓；③ 最近缩略图槽 `last_thumb_blob` ✓。
/// 局部渲染只更新 ① ✓ ⇒ ①②可以**同时**存在 ✓（缩略图那份必须留着 ✓，否则客户端 404 ✗）。
const PREVIEW_LEVELS: usize = 3;

/// **存储契约** ✓：`remove` 之后 `list` 里不许再有它 ✓。
///
/// 单列成一条 ✓，是因为"淘汰没生效"有两种可能 ✓：**我的淘汰逻辑** ✗ 还是**存储的 remove** ✗。
/// 分清责任比猜快 ✓（实测：这条契约是好的 ✓，问题在我的测试期望 ✓）。
#[test]
fn removing_a_blob_takes_it_out_of_the_listing() {
    let workspace = workspace();
    let bytes = vec![7u8; 128];
    let hash = workspace.store().put(&bytes).expect("入库应成功");
    assert!(workspace.store().exists(&hash), "刚放进去应在 ✓");
    assert!(
        workspace.store().remove(&hash).expect("删除应成功"),
        "删除应报告删掉了 ✓"
    );
    assert!(!workspace.store().exists(&hash), "删掉之后不该还在 ✓");
    assert!(
        !workspace
            .store()
            .list()
            .expect("应能列出")
            .iter()
            .any(|entry| entry.blob_hash == hash),
        "清单里也不该再有它 ✓"
    );
}

/// **连渲染多次 ⇒ 预览数被级数封顶** ✓（这才是"缓存"该有的性质 ✓）。
#[test]
fn repeated_rendering_keeps_the_preview_count_bounded() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_pv", 64, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    {
        let mut ctx = context(&mut workspace);
        registry().call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    }
    // 画一笔 ✓，然后**反复渲染** ✓（每次内容都不同 ✓ ⇒ 每次都产生新的 PNG ✓）。
    for round in 0..5 {
        {
            let mut ctx = context(&mut workspace);
            let drawn = registry().call(&mut ctx, "draw_stroke", &json!({
                "layer_id": "L",
                "data": {"points": [[6.0, 6.0 + round as f64 * 6.0], [50.0, 8.0 + round as f64 * 6.0]],
                         "size": 5.0, "color": {"r": 200, "g": 30 + round as u8 * 20, "b": 40, "a": 255}}}));
            assert_eq!(drawn["ok"], json!(true), "{drawn}");
        }
        let rendered = workspace
            .render_region("doc_pv", Bbox::new(0.0, 0.0, 64.0, 64.0))
            .expect("渲染应成功");
        assert_eq!(rendered.width, 64, "渲染尺寸应对 ✓");
        // 每次渲染之后都不许超过级数封顶 ✓（旧的那份被淘汰 ✓）。
        let count = blob_count(&mut workspace);
        assert!(
            count <= PREVIEW_LEVELS,
            "第 {} 轮之后 CAS 里有 {} 份 blob，超过预览缓存级数 {} ⇒ 淘汰没生效（旧预览没删 ✗）：{}",
            round + 1,
            count,
            PREVIEW_LEVELS,
            "这正是真实工作区里 1912 个孤儿、1.07 GB 的来源 ✓"
        );
    }
    // 而且**当前指针指向的那份必须还在** ✓（淘汰只淘汰被替换掉的 ✓）。
    let latest = workspace
        .document("doc_pv")
        .and_then(|document| document.latest_preview_url())
        .expect("应有最新预览指针 ✓");
    let hash = latest.rsplit('/').next().unwrap_or_default().to_owned();
    assert!(
        workspace
            .store()
            .list()
            .expect("应能列出")
            .iter()
            .any(|entry| entry.blob_hash.to_string() == hash),
        "最新预览必须在 CAS 里 ✓（否则客户端会 404 ✗）：{latest}"
    );
}
