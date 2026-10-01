//! 悬空变更集 / Stash（设计 §12.4 ✓）。
//!
//! **设计给了行为、没给工具名 ⇒ 记录选择** ✓：`submit_offline` ✓（重连时提交离线原子 ✓）、
//! `list_stashes` ✓（UI 的"分支对比"素材 ✓）、`apply_stash` ✓（§898 的"强制应用到当前 HEAD" ✓）、
//! `discard_stash` ✓（§898 的"丢弃" ✓）。三者恰好对应设计写明的三种上层选择 ✓
//!（丢弃 / 强制应用 / **基于 HEAD 重新生成** ✗ —— 最后这种是**上层自己重新编辑** ✓，
//! 服务端没有可做的事 ✓，所以**不造一个假工具** ✗）。
//!
//! **两条最要紧的性质** ✓：
//! * **整批要么全进日志、要么一条都不进** ✓（§897"原子及 blob **一起**打包" ✓）——
//!   逐条提交的话第三条失败时前两条已经进日志 ✗，那不是打包而是半途而废 ✗；
//! * **丢弃不删 blob** ✓（§899：Stash 的 blob 归历史级保留、不被 GC ✓）。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_st", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn head(workspace: &mut Workspace) -> u64 {
    workspace
        .document("doc_st")
        .map(|document| document.head_seq())
        .unwrap_or(0)
}

fn stroke_atom(id: &str, layer: &str, payload_extra: serde_json::Value) -> serde_json::Value {
    let mut payload = json!({"layer_id": layer, "object_id": id,
        "data": {"points": [[10.0, 10.0], [40.0, 20.0]], "size": 6.0,
                 "color": {"r": 20, "g": 20, "b": 20, "a": 255}}});
    if let Some(extra) = payload_extra.as_object() {
        for (key, value) in extra {
            payload[key] = value.clone();
        }
    }
    json!({"kind": "draw_stroke", "payload": payload, "actor": "human:1", "session": "session:offline"})
}

/// 造一个带 blob 的离线原子 ✓（`import_image` ✓）—— 用来验"blob 与原子一起打包" ✓。
fn image_atom(id: &str, layer: &str, blob: &str) -> serde_json::Value {
    json!({"kind": "import_image", "payload": {
        "object_id": id, "layer_id": layer, "type": "raster_patch",
        "bitmap": {"blob_hash": blob, "size": 8, "mime_type": "image/x-yanshi-raw"},
        "region": {"x": 0, "y": 0, "w": 4, "h": 4}, "width": 4, "height": 4},
        "actor": "human:1", "session": "session:offline"})
}

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

/// **全部通过 ⇒ 整批进日志** ✓（一个变更集 ✓，返回各条的 seq ✓）。
#[test]
fn a_clean_offline_batch_is_applied_whole() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_st", 64, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    }
    let before = head(&mut workspace);
    let submitted = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "submit_offline",
            &json!({"atoms": [stroke_atom("s1", "L", json!({})), stroke_atom("s2", "L", json!({}))]}),
        )
    };
    assert_eq!(submitted["ok"], json!(true), "{submitted}");
    assert_eq!(
        submitted["stashed"],
        json!(false),
        "干净的一批不该被搁置：{submitted}"
    );
    assert_eq!(submitted["applied"], json!(2), "{submitted}");
    assert_eq!(
        submitted["seqs"].as_array().map(Vec::len),
        Some(2),
        "§900：重放后获得新的 seq：{submitted}"
    );
    assert!(head(&mut workspace) > before, "日志头应前进");
    let listed = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "list_stashes", &json!({}))
    };
    assert_eq!(
        listed["count"],
        json!(0),
        "成功重放不该留下悬空变更集：{listed}"
    );
}

/// **一条不过 ⇒ 整批进悬空变更集，且前一条也绝不许进日志** ✓（§897 的"一起打包" ✓）。
#[test]
fn one_bad_atom_stashes_the_whole_batch_without_touching_the_log() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_st", 64, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    let blob = put_blob(&mut workspace);
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    }
    let before = head(&mut workspace);
    let submitted = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "submit_offline",
            &json!({"atoms": [
                stroke_atom("s1", "L", json!({})),
                image_atom("blobbed", "L", &blob),
                stroke_atom("s3", "NO_SUCH_LAYER", json!({})),
            ]}),
        )
    };
    assert_eq!(submitted["ok"], json!(true), "{submitted}");
    assert_eq!(
        submitted["stashed"],
        json!(true),
        "有一条第不过就该整批搁置：{submitted}"
    );
    assert_eq!(submitted["applied"], json!(0), "{submitted}");
    assert_eq!(submitted["atoms"], json!(3), "{submitted}");
    assert!(
        !submitted["reason"].as_str().unwrap_or_default().is_empty(),
        "{submitted}"
    );
    // **日志头一动不动** ✓ —— 这是"整批"的核心断言 ✓（逐条提交会悄悄进两条 ✗）。
    assert_eq!(head(&mut workspace), before, "搁置时日志头不该前进");
    let listed = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "list_stashes", &json!({}))
    };
    assert_eq!(listed["count"], json!(1), "{listed}");
    let entry = &listed["stashes"][0];
    assert_eq!(entry["atoms"], json!(3), "{entry}");
    assert_eq!(
        entry["blob_refs"],
        json!(1),
        "§897：blob 与原子一起打包：{entry}"
    );
    // **断言要"点名"** ✓：不是在找某个英文小写词 ✓（我第一版就是这么写的 ✗，
    // 而实际原因是中文"图层 NO_SUCH_LAYER 在当前 HEAD 中不存在" ✓ ⇒ 当场失败 ✓）。
    // 直接要求原因里出现**那一条的具体对象名** ✓ —— 这才叫"说清哪一条不过" ✓。
    assert!(
        entry["reason"]
            .as_str()
            .unwrap_or_default()
            .contains("NO_SUCH_LAYER"),
        "原因要点名是哪一条不过：{entry}"
    );
}

/// **`apply_stash`** ✓：世界修好之后能重放成功 ✓；**修不好则原样保留** ✓（绝不毁掉离线工作 ✓）。
#[test]
fn apply_stash_replays_and_keeps_the_stash_when_it_still_fails() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_st", 64, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    }
    let stash_id = {
        let mut ctx = context(&mut workspace);
        let submitted = registry.call(
            &mut ctx,
            "submit_offline",
            &json!({"atoms": [stroke_atom("s1", "L", json!({})), stroke_atom("s2", "MISSING", json!({}))]}),
        );
        submitted["stash_id"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    };
    assert!(!stash_id.is_empty(), "应拿到悬空变更集 id");
    // ① 世界**还没**修好 ⇒ 重放失败 ✓，但**必须原样保留** ✓。
    let still_failing = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "apply_stash", &json!({"stash_id": stash_id}))
    };
    assert_eq!(still_failing["ok"], json!(false), "{still_failing}");
    assert_eq!(
        still_failing["kept"],
        json!(true),
        "失败时必须保留：{still_failing}"
    );
    let listed = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "list_stashes", &json!({}))
    };
    assert_eq!(
        listed["count"],
        json!(1),
        "失败之后它仍应在列表里：{listed}"
    );
    // ② 把缺的图层补上 ⇒ 重放成功 ✓、悬空变更集消失 ✓。
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "MISSING"}));
    }
    let before = head(&mut workspace);
    let applied = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "apply_stash", &json!({"stash_id": stash_id}))
    };
    assert_eq!(applied["ok"], json!(true), "{applied}");
    assert_eq!(applied["applied"], json!(2), "{applied}");
    assert!(head(&mut workspace) > before, "重放成功应推进日志头");
    let listed = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "list_stashes", &json!({}))
    };
    assert_eq!(
        listed["count"],
        json!(0),
        "重放成功之后应不再挂着：{listed}"
    );
}

/// **`discard_stash` 只丢"待重放"，不删 blob** ✓（§899：Stash 的 blob 归历史级保留 ✓）。
#[test]
fn discarding_a_stash_keeps_its_blobs() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_st", 64, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    let blob = put_blob(&mut workspace);
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    }
    let stash_id = {
        let mut ctx = context(&mut workspace);
        let submitted = registry.call(
            &mut ctx,
            "submit_offline",
            &json!({"atoms": [image_atom("blobbed", "L", &blob), stroke_atom("s2", "MISSING", json!({}))]}),
        );
        submitted["stash_id"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    };
    let discarded = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "discard_stash", &json!({"stash_id": stash_id}))
    };
    assert_eq!(discarded["ok"], json!(true), "{discarded}");
    assert_eq!(discarded["discarded"], json!(true), "{discarded}");
    assert_eq!(
        discarded["blobs_kept"],
        json!(1),
        "应如实报告留下了几个 blob：{discarded}"
    );
    // **blob 仍在 CAS 里** ✓（§899 ✓）—— 这条断言是"丢弃 ≠ 抹掉历史"的硬证据 ✓。
    let hash: yanshi_core::BlobHash = blob.parse().expect("blob 哈希应可解析");
    assert!(
        workspace.store().exists(&hash),
        "§899：Stash 的 blob 属历史级保留，丢弃它不该删 blob"
    );
    let listed = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "list_stashes", &json!({}))
    };
    assert_eq!(listed["count"], json!(0), "{listed}");
}

/// **参数与找不到的情形都要明确报错** ✓（不静默 ✓）。
#[test]
fn stash_tools_refuse_bad_input_clearly() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_st", 64, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(&mut workspace);
        let empty = registry.call(&mut ctx, "submit_offline", &json!({"atoms": []}));
        assert_eq!(empty["ok"], json!(false), "{empty}");
        assert!(
            empty["context"]["detail"]
                .as_str()
                .unwrap_or_default()
                .contains("不能为空"),
            "{empty}"
        );
        let bad_kind = registry.call(
            &mut ctx,
            "submit_offline",
            &json!({"atoms": [{"kind": "wat", "payload": {}}]}),
        );
        assert_eq!(bad_kind["ok"], json!(false), "{bad_kind}");
        assert!(
            bad_kind["context"]["detail"]
                .as_str()
                .unwrap_or_default()
                .contains("kind"),
            "{bad_kind}"
        );
        for tool in ["apply_stash", "discard_stash"] {
            let missing = registry.call(&mut ctx, tool, &json!({"stash_id": "stash_nope"}));
            assert_eq!(missing["ok"], json!(false), "{tool}：{missing}");
            assert_eq!(
                missing["error_code"].as_str().unwrap_or_default(),
                "reference_not_found",
                "{tool}：{missing}"
            );
        }
    }
}
