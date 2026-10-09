//! **★ 目标第 5 条：钉住的 blob 不得被误删 ✗ ★**（第 952 轮 ✓；**A① 的教训 ✓**）。
//!
//! **∴ 为什么要有这条判据 ✗**：**代码里的保护**存在很久了 ✗**
//!   （`document.rs:1200` 的 `if self.pinned_blobs.contains(old) { continue; }` ✓，
//!   **注释写明是"**第 301 轮 ✓"**，**即 A① 的教训 ✓）**，
//!   **而**在此之前**测试里**一个 `pinned_blobs` **都没用到 ✗**
//!   ⇒ **★ 与 C5 同型 ✗**：**代码有保护 ✗，**判据没有 ⇒ **∴ 将来删掉它**不会有东西变红 ✓ ★**** ✓✓
//!
//! **∴ 事故原文（**`service.rs:986-989` ✓）★**：
//!   > **调用栈实测**：`write_fill_region` → `finish_mutation` → `render_region`
//!   > → `evict_replaced_previews` → `remove` ✓
//!   > ⇒ **症状**：`set_reference` **当场成功** ✓、**下一次落笔**就从磁盘消失 ✗
//!   > ⇒ `analyze_region` 报 `reference_not_found` ✓ —— **数据丢失**一类 ✓
//!
//! **∴ 判据用**公开入口 ✗**（**∴ 不**碰私有字段 ✓）**：
//!   `Workspace::set_preferences({"reference.blob_hash": ...})` ✓
//!   （`service.rs:964` ✓ ⇒ `:992-1004` 把该 hash 发给**每个**文档 ✓）。
//!
//! **∴ 防空转 ✗ ★**：**对照组**（**不 pin ✓）**必须**真的被淘汰掉 ✗**
//!   ⇒ **∴ 否则**"**pin 住的还在**"**可能只是**"**这一轮**本来就没淘汰**" ✓**** ✓✓
//!
//! **∴ 变异 ✗**：**删掉** `document.rs` 的 `if self.pinned_blobs.contains(old) { continue; }` ✗**
//!   ⇒ **∴ `pinned_blob_survives`** 必红 ✗**（**而对照组仍绿 ✓）** ✓✓

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_pin", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

/// **整幅铺一次底色** ⇒ **∴ 于是**文档级缩略图**被替换** ✓（**并**淘汰**上一份 ✓）**。
///
/// **∴ 为什么不能用小笔画 ✗**（第 952 轮实测 ✓）：**文档级缩略图**只在
/// **整幅覆盖**时更新 ✓（`document.rs:1197` 的注释 ✓）⇒ **∴ 小笔画**只覆盖局部 ✗**
///   ⇒ **∴ 于是**：**上一份**仍在"**当前**"集合里 ✗** ⇒ **∴ 它**不进 `previous`**
///     ⇒ **∴ 淘汰**根本不会发生 ✓**** ✓✓
///   ⇒ **∴ 而**我**第一版**正是这样 ✗** ⇒ **∴ 对照组**（**不钉住 ✓）**竟然**还活着 ✓**
///     ⇒ **★ 那条防空转断言**当场抓住 ✓**（**`实测它还在` ✓）** ★**** ✓✓
fn full_canvas_fill(workspace: &mut Workspace, object_id: &str, red: u8) {
    let mut ctx = context(workspace);
    let filled = registry().call(
        &mut ctx,
        "fill",
        &json!({"layer_id": "L", "object_id": object_id,
                "data": {"color": {"r": red, "g": 120, "b": 200, "a": 255},
                         "region": {"x": 0, "y": 0, "w": 128, "h": 128}}}),
    );
    assert_eq!(filled["ok"], json!(true), "{filled}");
}

/// **★ 核心判据 ✗ ★**：被钉住的预览 blob，**在新预览取而代之之后**必须**还在** store 里 ✓。
#[test]
fn pinned_blob_survives_preview_eviction() {
    for pinned in [true, false] {
        let mut workspace = Workspace::in_memory(DocumentSettings::default());
        workspace
            .create_document(
                NewDocument::new("doc_pin", 128, 128),
                "human:1",
                "session:test",
            )
            .expect("建文档");
        // **① 先建图层** ✓（**照 `selection_clip.rs:86` 的既有写法 ✓**）：
        //   **∴ 为什么必须 ✗**：**`draw_stroke` 的 `layer_id` 不存在时会**如实报错**✗**
        //     （**实测**：`reference_not_found` ＋ `detail: 图层 L 不存在（现有图层：layer_default）` ✓）
        //     ⇒ **∴ 不**静默落到默认层 ✓（**这一点**本身**是对的 ✓）** ✓✓
        {
            let mut ctx = context(&mut workspace);
            registry().call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
        }
        // **② 落第一笔** ⇒ **∴ 于是**产生第一份预览 ✓
        full_canvas_fill(&mut workspace, "a_first", 40);
        let hash = workspace
            .document("doc_pin")
            .expect("文档在")
            .doc_thumbnail_hash_for_probe()
            .expect("第一笔之后应当有一份文档预览");
        // **③ 按需要把它钉住** ✓（**走公开的偏好入口 ✓）**
        if pinned {
            let mut values = serde_json::Map::new();
            values.insert("reference.blob_hash".to_owned(), json!(hash.to_string()));
            workspace.set_preferences(&values).expect("设偏好");
        }
        // **④ 再落一笔** ⇒ **∴ 新预览取而代之 ⇒ **∴ 触发 `evict_replaced_previews` ✓**** ✓✓
        full_canvas_fill(&mut workspace, "b_second", 220);
        let store = workspace.store();
        let alive = store.exists(&hash);
        if pinned {
            assert!(
                alive,
                "**钉住的 blob 不许被淘汰** ✗（A① 的教训 ✓）：设成参考图之后，\
                 下一次落笔就把它删掉 ⇒ `analyze_region` 会报 `reference_not_found` ✓"
            );
        } else {
            // **★ 防空转 ✗**：**不钉住**时必须**真的**被淘汰 ✓
            //   ⇒ **∴ 否则**上一条**可能只是**"**这一轮本来就没淘汰**" ✓**** ✓✓
            assert!(
                !alive,
                "对照组（**没有钉住**）**必须**被淘汰 ✗；\
                 实测它还在 ⇒ **∴ 那么**「钉住的还在」**证明不了任何事** ✓（判据空转 ✓）"
            );
        }
    }
}
