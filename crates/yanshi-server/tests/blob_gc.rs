//! Blob 三级生命周期与孤儿回收（设计 §6.3 ✓）。
//!
//! **设计原话** ✓：GC 根集 = **全日志原子引用闭包** ✓ —— "GC 永不删除被任何日志原子引用的 blob" ✓
//!（"删 blob 等于部分删除原子" ✓）。三级：活跃 ✓ / 历史 ✓（**保留** ✓）/ 孤儿 ✓（TTL 7 天后清理 ✓）。
//!
//! **本文件钉的就是那条不变量** ✓：blob 只要被日志里任何原子引用过 ✓，就绝不进"可回收" ✓ ——
//! 哪怕当前状态里已经看不到它（被 revert ✓）✓。这是"时间旅行 / Stash 重放还能取回像素"的前提 ✓。
//!
//! **断言一律针对具体 blob（看哈希清单 ✓），不看全局计数** ✗ —— 本轮实测：**一次提交会顺带产生预览 blob**
//! ✓，它当时无人引用 ⇒ 进孤儿级 ✓ ⇒ 全局计数会被它搅乱 ✗。这条也写下来 ✓：**计数是给人看的，断言要看身份** ✓。
//!
//! **两条记录在案的取舍** ✓：① **Stash 里的原子算根** ✓（否则离线编辑会在重连前被清掉 ✗）；
//! ② **暂不做 zstd 冷归档** ✓ —— 设计里"历史级 = 冷归档 + zstd" ✓，但那是**依赖决策** ✓，
//! 不擅自引入 ✗（本版本只做分级与孤儿回收 ✓，即正确性最关键的那半 ✓）。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_gc", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn setup(workspace: &mut Workspace) {
    workspace
        .create_document(
            NewDocument::new("doc_gc", 64, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    let mut ctx = context(workspace);
    registry().call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
}

fn report(workspace: &mut Workspace, ttl_days: u64) -> serde_json::Value {
    let mut ctx = context(workspace);
    let value = registry().call(&mut ctx, "blob_gc", &json!({"ttl_days": ttl_days}));
    assert_eq!(value["ok"], json!(true), "{value}");
    value
}

/// 某个哈希是否在某一级的清单里 ✓（断言看**身份** ✓，不看计数 ✗）。
fn listed(value: &serde_json::Value, level: &str, hash: &str) -> bool {
    value["hashes"][level]
        .as_array()
        .map(|items| items.iter().any(|item| item.as_str() == Some(hash)))
        .unwrap_or(false)
}

fn put_blob(workspace: &mut Workspace, seed: u8) -> String {
    let bytes: Vec<u8> = (0..16 * 16)
        .flat_map(|index| [index as u8 ^ seed, seed, 90, 255])
        .collect();
    workspace
        .store()
        .put(&bytes)
        .expect("blob 应能入库")
        .to_string()
}

fn import(workspace: &mut Workspace, blob: &str, object_id: &str) -> serde_json::Value {
    let mut ctx = context(workspace);
    registry().call(
        &mut ctx,
        "import_image",
        &json!({"layer_id": "L", "object_id": object_id,
                "bitmap": {"blob_hash": blob, "size": 1024, "mime_type": "image/x-yanshi-raw"},
                "region": {"x": 0.0, "y": 0.0, "w": 16.0, "h": 16.0}}),
    )
}

fn hash_of(text: &str) -> yanshi_core::BlobHash {
    text.parse().expect("哈希可解析")
}

/// **被日志引用的 blob ⇒ 活跃** ✓（不是孤儿 ✓）。
#[test]
fn a_referenced_blob_is_active_not_orphan() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let blob = put_blob(&mut workspace, 1);
    let imported = import(&mut workspace, &blob, "obj_a");
    assert_eq!(imported["ok"], json!(true), "{imported}");
    let listed_report = report(&mut workspace, 7);
    assert!(
        listed(&listed_report, "active", &blob),
        "应记成活跃：{listed_report}"
    );
    assert!(
        !listed(&listed_report, "orphan", &blob),
        "不该算孤儿：{listed_report}"
    );
}

/// **被 revert 掉、只剩日志引用 ⇒ 历史（保留、绝不可回收）** ✓ —— 设计里最要紧的那条 ✓。
#[test]
fn a_blob_referenced_only_by_the_log_is_history_and_never_collectible() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let blob = put_blob(&mut workspace, 2);
    let imported = import(&mut workspace, &blob, "obj_b");
    let atom = imported["atom_id"].as_str().unwrap_or_default().to_owned();
    assert!(!atom.is_empty(), "{imported}");
    {
        let mut ctx = context(&mut workspace);
        let undone = registry().call(&mut ctx, "revert", &json!({"atom_id": atom}));
        assert_eq!(undone["ok"], json!(true), "{undone}");
    }
    // TTL 给 0 ✓：若它被当成孤儿 ✓，就会"立即可回收" ✓ ⇒ 那样断言会立刻失败 ✓。
    let listed_report = report(&mut workspace, 0);
    assert!(
        !listed(&listed_report, "active", &blob),
        "当前状态里已看不到它：{listed_report}"
    );
    assert!(
        listed(&listed_report, "history", &blob),
        "应记成历史：{listed_report}"
    );
    assert!(
        !listed(&listed_report, "collectible", &blob),
        "**历史级永远不可回收**（时间旅行/重放要靠它）：{listed_report}"
    );
    assert!(
        workspace.store().exists(&hash_of(&blob)),
        "历史级 blob 必须还在 CAS 里 ✓"
    );
}

/// **Stash 里的原子也算根** ✓（否则离线编辑会在重连前被清掉 ✗）。
#[test]
fn a_stashed_atoms_blob_counts_as_a_root() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let blob = put_blob(&mut workspace, 3);
    let atoms = vec![yanshi_core::Atom::new(
        yanshi_core::AtomKind::ImportImage,
        "human:1",
        "session:offline",
        json!({"object_id": "obj_c", "layer_id": "L", "type": "raster_patch",
               "bitmap": {"blob_hash": blob, "size": 1024, "mime_type": "image/x-yanshi-raw"},
               "region": {"x": 0.0, "y": 0.0, "w": 16.0, "h": 16.0},
               "width": 16, "height": 16}),
    )];
    let stash_id = workspace
        .stash("doc_gc", "human:1", "session:offline", "测试 ✓", atoms)
        .expect("搁置应成功");
    assert!(!stash_id.is_empty());
    let listed_report = report(&mut workspace, 0);
    assert!(
        !listed(&listed_report, "orphan", &blob) && !listed(&listed_report, "collectible", &blob),
        "**Stash 引用的 blob 必须算根**（离线编辑不能在重连前被清掉）：{listed_report}"
    );
}

/// **刚上传、还没被引用 ⇒ 孤儿；TTL 内不可回收，过期才可回收** ✓。
#[test]
fn an_unreferenced_blob_is_collectible_only_after_the_ttl() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let blob = put_blob(&mut workspace, 4);
    let fresh = report(&mut workspace, 7);
    assert!(listed(&fresh, "orphan", &blob), "未被引用 ⇒ 孤儿：{fresh}");
    assert!(
        !listed(&fresh, "collectible", &blob),
        "7 天内不许回收：{fresh}"
    );
    let expired = report(&mut workspace, 0);
    assert!(
        listed(&expired, "collectible", &blob),
        "过了 TTL ⇒ 可回收：{expired}"
    );
    assert!(
        workspace.store().exists(&hash_of(&blob)),
        "默认 dry-run 不许删任何东西 ✓"
    );
}

/// **删除必须显式确认** ✓；确认后**只删可回收的孤儿** ✓，活跃与历史一个都不动 ✓。
#[test]
fn collection_requires_confirmation_and_never_touches_active_or_history() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let active_blob = put_blob(&mut workspace, 5);
    let history_blob = put_blob(&mut workspace, 6);
    let orphan_blob = put_blob(&mut workspace, 7);
    let active = import(&mut workspace, &active_blob, "obj_active");
    assert_eq!(active["ok"], json!(true), "{active}");
    let history = import(&mut workspace, &history_blob, "obj_history");
    {
        let mut ctx = context(&mut workspace);
        registry().call(
            &mut ctx,
            "revert",
            &json!({"atom_id": history["atom_id"].as_str().unwrap_or_default()}),
        );
    }
    // ① 想删但没确认 ⇒ 明确报错 ✓（不是静默什么都不做 ✗）。
    let refused = {
        let mut ctx = context(&mut workspace);
        registry().call(
            &mut ctx,
            "blob_gc",
            &json!({"dry_run": false, "ttl_days": 0}),
        )
    };
    assert_eq!(refused["ok"], json!(false), "{refused}");
    assert!(
        refused["context"]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("confirm"),
        "错误应说明缺 confirm：{refused}"
    );
    // ② 确认 ⇒ 回收 ✓。
    let collected = {
        let mut ctx = context(&mut workspace);
        registry().call(
            &mut ctx,
            "blob_gc",
            &json!({"dry_run": false, "confirm": true, "ttl_days": 0}),
        )
    };
    assert_eq!(collected["ok"], json!(true), "{collected}");
    assert!(
        collected["removed"].as_u64().unwrap_or(0) >= 1,
        "至少该删掉我造的那个孤儿（提交产生的预览 blob 也会一并过期 ✓）：{collected}"
    );
    assert!(
        workspace.store().exists(&hash_of(&active_blob)),
        "**活跃不许删** ✓"
    );
    assert!(
        workspace.store().exists(&hash_of(&history_blob)),
        "**历史不许删** ✓"
    );
    assert!(
        !workspace.store().exists(&hash_of(&orphan_blob)),
        "孤儿应被删掉 ✓"
    );
}
