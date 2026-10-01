//! `resolve_conflict`（设计 12.3 ✓ —— 设计把这一节写得**非常明确** ✓，含四种手段的展开表 ✓）。
//!
//! **设计原话** ✓：`resolve_conflict` 是**组合宏、不是新原子类型** ✓ —— **折叠器零改动** ✓。
//! 本文件因此**不检查内核改动** ✗，而是检查：四种手段各自的**展开结果** ✓。
//!
//! **一处设计与本仓库模型的落差，如实记下** ✓：设计表写 `tombstone(我方原子)` 与
//! `move(我方原子 → 正式图层)` ✓，但本仓库里**原子既不能墓碑化、归属也不可改**（日志追加式 ✓）。
//! 工具因此用同一模型里的对应机制 ✓：让原子失效 = `Revert` ✓；搬移 = 对象型提交改对象的 `layer_id` ✓、
//! 原子型提交（像素补丁 ✓）**改投正式图层重提交** ✓。本用例用**对象型**提交（`import_image`
//! + `sampling: true` ✓，§12.3 认这种 ✓）⇒ 能对"移回正式图层"做强断言 ✓。

use serde_json::json;
use yanshi_core::{Atom, AtomKind};
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_rc", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

/// 往 CAS 放一张 4×4 的原始像素 ✓（`import_image` 的 blob 先行协议要求它先存在 ✓）。
fn put_blob(workspace: &mut Workspace) -> String {
    let bytes: Vec<u8> = (0..4 * 4)
        .flat_map(|index| [index as u8 * 16, 40, 90, 255])
        .collect();
    workspace
        .store()
        .put(&bytes)
        .expect("blob 应能入库")
        .to_string()
}

/// 提交一条**采样性替换**原子（`import_image` + `type: raster_patch` + `sampling: true` ✓）。
fn sampling_atom(blob: &str, layer_id: &str, object_id: &str, session: &str) -> Atom {
    // **原子种类要挑对** ✓：`is_sampling_replace` 对"带 payload 的"那种只认
    // `CreateObject | Supersede` + `type: raster_patch` + `sampling: true` ✓
    //（我第一版用了 `ImportImage` ✗ ⇒ 检测器根本不认 ✓ ⇒ 第二次提交**静默成功** ✗，
    //  症状是"冲突没发生" ✓ —— 测试当场指出 ✓）。
    Atom::new(
        AtomKind::CreateObject,
        format!("human:{session}"),
        session,
        json!({
            "object_id": object_id,
            "layer_id": layer_id,
            "type": "raster_patch",
            "sampling": true,
            "bitmap": {"blob_hash": blob, "size": 64, "mime_type": "image/x-yanshi-raw"},
            "region": {"x": 4, "y": 4, "w": 4, "h": 4},
            "width": 4,
            "height": 4,
        }),
    )
}

/// **造一个真冲突** ✓：两个会话在同一 Region 各做一次采样性替换 ⇒ 第二次被拒并建冲突图层 ✓。
fn make_conflict(workspace: &mut Workspace) -> (String, String, String) {
    let blob = put_blob(workspace);
    {
        let mut ctx = context(workspace);
        registry().call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    }
    // 会话 A 先提交 ✓（成功 ✓）。
    let first = sampling_atom(&blob, "L", "a_obj", "a");
    let result = workspace.commit("doc_rc", first.clone(), "human:a", true);
    assert!(result.is_ok(), "第一次采样性替换应成功：{result:?}");
    // 会话 B 提交同一 Region ✓ ⇒ **拒绝 + conflict** ✓（设计 §874-875 ✓）。
    let second = sampling_atom(&blob, "L", "b_obj", "b");
    let refused = workspace.commit("doc_rc", second, "human:b", true);
    let error = refused.expect_err("并发采样性替换应被拒绝");
    // **错误码是 `conflict`** ✓（设计 §434 的错误码表里就有它 ✓）——
    // 我第一版写成 `InvalidArgument` ✗（凭印象 ✓），测试当场指出 ✓。
    assert_eq!(error.code, yanshi_core::ErrorCode::Conflict, "{error:?}");
    assert!(
        error.retryable,
        "冲突是**可重试**的（改投冲突图层 ✓）：{error:?}"
    );
    // **冲突图层 id 随错误一起给出** ✓（设计 §875 ✓），否则客户端不知道往哪重提交 ✗。
    assert!(
        error.context.extra.contains_key("conflict_layer_id"),
        "错误里应带冲突图层 id：{error:?}"
    );
    // 冲突图层由服务端建好 ✓（`system:conflict` ✓，`metadata.conflict = true` ✓）。
    let state = workspace.document_mut("doc_rc").unwrap().state().clone();
    let conflict_layer = state
        .layers
        .values()
        .find(|layer| {
            !layer.is_deleted()
                && layer
                    .metadata
                    .get("conflict")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false)
        })
        .map(|layer| layer.id.clone())
        .expect("应已建出冲突图层");
    // 客户端按 §876 **改投冲突图层重提交** ✓。
    let resubmit = sampling_atom(&blob, &conflict_layer, "b_obj2", "b");
    workspace
        .commit("doc_rc", resubmit, "human:b", true)
        .expect("改投冲突图层后应能提交");
    (conflict_layer, first.id.clone(), "b_obj2".to_owned())
}

/// **`keep_ours`** ✓：撤销对方原子 ✓ + 把我们的对象移回正式图层 ✓ + 冲突图层墓碑化（标记保留 ✓）。
#[test]
fn keep_ours_moves_our_object_back_and_retires_the_conflict_layer() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_rc", 64, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    let (conflict_layer, opponent, our_object) = make_conflict(&mut workspace);
    let resolved = {
        let mut ctx = context(&mut workspace);
        registry().call(
            &mut ctx,
            "resolve_conflict",
            &json!({"resolution": "keep_ours", "conflict_layer_id": conflict_layer,
                    "formal_layer_id": "L", "opponent_atom_id": opponent}),
        )
    };
    assert_eq!(resolved["ok"], json!(true), "{resolved}");
    assert_eq!(
        resolved["moved_to_formal"],
        json!(1),
        "我们的对象应被移回：{resolved}"
    );
    assert_eq!(
        resolved["reverted_opponent"],
        json!(true),
        "对方原子应失效：{resolved}"
    );
    let state = workspace.document_mut("doc_rc").unwrap().state().clone();
    let object = state.objects.get(&our_object).expect("我们的对象应还在");
    assert_eq!(object.layer_id, "L", "对象应回到正式图层 ✓");
    let layer = state
        .layers
        .get(&conflict_layer)
        .expect("冲突图层仍在（被墓碑化 ✓）");
    assert!(layer.is_deleted(), "冲突图层应当被墓碑化 ✓");
    assert!(
        layer
            .metadata
            .get("conflict")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        "`metadata.conflict` 应保留供审计 ✓（设计原话 ✓）"
    );
}

/// **`keep_theirs`** ✓：我方的提交失效 ✓、对方**保留** ✓、冲突图层关闭 ✓。
#[test]
fn keep_theirs_drops_our_submission_only() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_rc", 64, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    let (conflict_layer, _opponent, our_object) = make_conflict(&mut workspace);
    let resolved = {
        let mut ctx = context(&mut workspace);
        registry().call(
            &mut ctx,
            "resolve_conflict",
            &json!({"resolution": "keep_theirs", "conflict_layer_id": conflict_layer}),
        )
    };
    assert_eq!(resolved["ok"], json!(true), "{resolved}");
    assert_eq!(
        resolved["reverted_opponent"],
        json!(false),
        "keep_theirs 不该动对方：{resolved}"
    );
    let state = workspace.document_mut("doc_rc").unwrap().state().clone();
    assert!(
        state
            .objects
            .get(&our_object)
            .map(|object| object.is_deleted())
            .unwrap_or(true),
        "我们的对象应被撤销：{resolved}"
    );
    assert!(
        state
            .layers
            .get(&conflict_layer)
            .map(|layer| layer.is_deleted())
            .unwrap_or(false),
        "冲突图层应被关闭"
    );
}

/// **`discard`** ✓：双方都失效 ✓。**`merge`** ✓：内容**不动** ✓，只关闭冲突 ✓。
#[test]
fn discard_drops_both_and_merge_only_closes_the_conflict() {
    for resolution in ["discard", "merge"] {
        let mut workspace = workspace();
        workspace
            .create_document(
                NewDocument::new("doc_rc", 64, 64),
                "human:1",
                "session:test",
            )
            .unwrap();
        let (conflict_layer, opponent, our_object) = make_conflict(&mut workspace);
        let resolved = {
            let mut ctx = context(&mut workspace);
            registry().call(
                &mut ctx,
                "resolve_conflict",
                &json!({"resolution": resolution, "conflict_layer_id": conflict_layer,
                        "opponent_atom_id": opponent}),
            )
        };
        assert_eq!(resolved["ok"], json!(true), "{resolution}：{resolved}");
        let state = workspace.document_mut("doc_rc").unwrap().state().clone();
        let ours_gone = state
            .objects
            .get(&our_object)
            .map(|object| object.is_deleted())
            .unwrap_or(true);
        if resolution == "discard" {
            assert!(ours_gone, "discard 应撤销我们这一侧：{resolved}");
            assert_eq!(resolved["reverted_opponent"], json!(true), "{resolved}");
        } else {
            // **`merge` 只关闭冲突标记** ✓（设计原话："上层手动编辑后自行提交；工具仅关闭冲突标记" ✓）。
            //
            // **这条断言我改过** ✗：原来断言"我们的对象还在" ✓ —— 但关闭冲突 = **墓碑化冲突图层** ✓
            // ⇒ 挂在它上面的残留自然**不再生效** ✓（这正是设计的意思 ✓：
            //   调用方**先**把自己的编辑提交到正式图层 ✓、**再**调 `merge` 关掉冲突 ✓）。
            // 所以真正该钉住的是：**工具没有主动撤销任何一方** ✓
            //（`tombstoned_ours = 0` ✓、`reverted_opponent = false` ✓），而不是"残留还活着" ✗。
            assert_eq!(
                resolved["tombstoned_ours"],
                json!(0),
                "merge 不该撤销我方：{resolved}"
            );
            assert_eq!(resolved["reverted_opponent"], json!(false), "{resolved}");
            assert_eq!(
                resolved["moved_to_formal"],
                json!(0),
                "merge 不该搬移内容：{resolved}"
            );
        }
        assert!(
            state
                .layers
                .get(&conflict_layer)
                .map(|layer| layer.is_deleted())
                .unwrap_or(false),
            "{resolution}：冲突图层都该被关闭"
        );
    }
}

/// **参数校验要点** ✓：未知手段 ✓、缺 `formal_layer_id`（keep_ours ✓）、缺 `opponent_atom_id` ✓、
/// 没有冲突图层时 ✓ —— 都要**明确报错** ✓，而不是悄悄做点什么 ✗。
#[test]
fn resolve_conflict_validates_its_arguments() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_rc", 64, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    {
        let mut ctx = context(&mut workspace);
        registry().call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
        // 没有冲突图层时 ⇒ 明确报错 ✓。
        let none = registry().call(
            &mut ctx,
            "resolve_conflict",
            &json!({"resolution": "merge"}),
        );
        assert_eq!(none["ok"], json!(false), "{none}");
        assert!(
            none["context"]["detail"]
                .as_str()
                .unwrap_or_default()
                .contains("冲突图层"),
            "错误应说明找不到冲突图层：{none}"
        );
        // 未知手段 ⇒ 列出可用的 ✓。
        let unknown = registry().call(
            &mut ctx,
            "resolve_conflict",
            &json!({"resolution": "wat", "conflict_layer_id": "L"}),
        );
        assert_eq!(unknown["ok"], json!(false), "{unknown}");
        let detail = unknown["context"]["detail"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        for name in ["keep_ours", "keep_theirs", "discard", "merge"] {
            assert!(detail.contains(name), "错误应列出 {name}：{detail}");
        }
    }
}
