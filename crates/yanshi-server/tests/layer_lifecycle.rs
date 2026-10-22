//! 图层的**生命周期**：删除之后，它的 id 应当能**重新使用** ✓。
//!
//! 子 agent 实测（本轮问题清单 ✓）：删掉 `L_sky` 之后
//! `create_layer {layer_id: "L_sky"}` 报 **"图层 L_sky 已存在"** ✗，
//! 而对同一个 id 落笔又报 **"图层 L_sky 已删除，…被拒绝"** ✗ ——
//! 同一个 id 同时"存在"又"已删除" ✓，两句提示自相矛盾 ✓，用户无法把名字收回来用 ✓。
//!
//! 根因：折叠层保留**墓碑**（`deleted_by` ✓ —— 删除可撤销的基础 ✓），
//! 而 `CreateLayer` 用 `contains_key` 判冲突 ✗ ⇒ 墓碑也算"已存在" ✓。
//! 正确谓词 `layer_alive` 本来就写在**紧邻**的 `parent_id` 检查里 ✓。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_life", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn square(_object_id: &str) -> serde_json::Value {
    json!({
        "geometry": {"kind": "rect", "bbox": {"x": 4.0, "y": 4.0, "w": 12.0, "h": 12.0}},
        "color": {"r": 10, "g": 20, "b": 30, "a": 255},
    })
}

/// 删除之后 id 可以复用 ✓；但**未重建**之前引用它仍然必须被拒 ✓（两件事都要成立 ✓）。
#[test]
fn a_deleted_layer_id_can_be_reused_but_stays_unusable_until_recreated() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_life", 64, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();

    {
        let mut ctx = context(&mut workspace);
        let created = registry.call(
            &mut ctx,
            "create_layer",
            &json!({"layer_id": "L_sky", "name": "sky"}),
        );
        assert_eq!(created["ok"], json!(true), "{created}");
        let painted = registry.call(
            &mut ctx,
            "draw_shape",
            &json!({"layer_id": "L_sky", "object_id": "a", "data": square("a")}),
        );
        assert_eq!(painted["ok"], json!(true), "{painted}");
        let deleted = registry.call(&mut ctx, "delete_layer", &json!({"layer_id": "L_sky"}));
        assert_eq!(deleted["ok"], json!(true), "{deleted}");
    }

    // ① 删除之后**尚未重建**：引用它必须继续被拒 ✓（这是**正确**行为 ✓，不能被上面的修复带走 ✗）。
    {
        let mut ctx = context(&mut workspace);
        let rejected = registry.call(
            &mut ctx,
            "draw_shape",
            &json!({"layer_id": "L_sky", "object_id": "b", "data": square("b")}),
        );
        assert_eq!(
            rejected["ok"],
            json!(false),
            "已删除的图层不应能直接落笔：{rejected}"
        );
        let code = rejected["error_code"].as_str().unwrap_or_default();
        assert_eq!(
            code, "precondition_failed",
            "应给出「已删除」这类前置条件错误：{rejected}"
        );
    }

    // ② **重新使用同一个 id** ✓ —— 修复前这里会报"图层 L_sky 已存在" ✗。
    {
        let mut ctx = context(&mut workspace);
        let recreated = registry.call(
            &mut ctx,
            "create_layer",
            &json!({"layer_id": "L_sky", "name": "sky again"}),
        );
        assert_eq!(recreated["ok"], json!(true), "墓碑不应占用 id：{recreated}");
    }

    // ③ 重建之后就能正常落笔 ✓（修复前这里报"已删除" ✗）。
    {
        let mut ctx = context(&mut workspace);
        let painted = registry.call(
            &mut ctx,
            "draw_shape",
            &json!({"layer_id": "L_sky", "object_id": "c", "data": square("c")}),
        );
        assert_eq!(
            painted["ok"],
            json!(true),
            "重建后的图层应可落笔：{painted}"
        );
    }

    // ④ **存活的**图层仍然不能被重复创建 ✓（这条约束不能被放松 ✗）。
    {
        let mut ctx = context(&mut workspace);
        let duplicated = registry.call(
            &mut ctx,
            "create_layer",
            &json!({"layer_id": "L_sky", "name": "again"}),
        );
        assert_eq!(
            duplicated["ok"],
            json!(false),
            "存活图层不得重复创建：{duplicated}"
        );
    }
}

/// **`if_exists="reuse"` 不得把墓碑说成「复用成功」** ✓。
///
/// 实测（同一份日志、同一个 id ✓）：删掉 `detail` 之后再
/// `create_layer {if_exists: "reuse"}` 回的是 `ok: true, reused: true` ✓，
/// 但 `list_layers` **不列**它 ✗ —— 于是随后**每一笔**都撞
/// 「图层 detail 已删除」✗，而调用方手里拿着的是一句**成功**。
#[test]
fn if_exists_reuse_recreates_a_tombstone_instead_of_reporting_a_lie() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_life", 64, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();

    {
        let mut ctx = context(&mut workspace);
        assert_eq!(
            registry.call(
                &mut ctx,
                "create_layer",
                &json!({"layer_id": "L_reuse", "name": "x", "if_exists": "reuse"})
            )["ok"],
            json!(true)
        );
        assert_eq!(
            registry.call(&mut ctx, "delete_layer", &json!({"layer_id": "L_reuse"}))["ok"],
            json!(true)
        );
    }

    let mut ctx = context(&mut workspace);
    let again = registry.call(
        &mut ctx,
        "create_layer",
        &json!({"layer_id": "L_reuse", "name": "x", "if_exists": "reuse"}),
    );
    assert_eq!(
        again["ok"],
        json!(true),
        "墓碑应当被重建而不是挡住 reuse：{again}"
    );
    assert_ne!(
        again["reused"].as_bool(),
        Some(true),
        "这一条**真的新建了图层**，回 `reused: true` 就是假信息：{again}"
    );

    let listed = registry.call(&mut ctx, "list_layers", &json!({}));
    assert!(
        listed["layers"]
            .as_array()
            .map(|rows| rows
                .iter()
                .any(|row| row["layer_id"] == json!("L_reuse") && row["visible"] == json!(true)))
            .unwrap_or(false),
        "重建之后必须出现在 `list_layers` 里：{listed}"
    );

    let painted = registry.call(
        &mut ctx,
        "draw_shape",
        &json!({"layer_id": "L_reuse", "object_id": "d", "data": square("d")}),
    );
    assert_eq!(
        painted["ok"],
        json!(true),
        "重建后的图层必须能落笔：{painted}"
    );

    // **存活**图层走 reuse ⇒ 仍然复用、且**不多一条原子** ✓（原语义不能被带走 ✗）。
    let before = registry.call(&mut ctx, "list_atoms", &json!({}))["count"].clone();
    let reused = registry.call(
        &mut ctx,
        "create_layer",
        &json!({"layer_id": "L_reuse", "name": "x", "if_exists": "reuse"}),
    );
    assert_eq!(
        reused["reused"].as_bool(),
        Some(true),
        "存活图层该复用：{reused}"
    );
    let after = registry.call(&mut ctx, "list_atoms", &json!({}))["count"].clone();
    assert_eq!(before, after, "复用不得写进日志：{before} → {after}");
}
