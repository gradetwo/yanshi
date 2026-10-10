//! **★ 回归判据：`new_document` 必须只产生**一个**默认图层 ✗ ★**（第 305 轮 ✓）。
//!
//! **∴ 为什么需要它（**第 303 轮的发现 ✓）★**：
//!   **∴ 我**应用了用户的 `yanshi-duplicate-layer.patch`**✗ ⇒ **∴ 而**当我去找守护**✗
//!     ⇒ **∴ 发现** `crates/yanshi-server/tests/duplicate_layer.rs` **名字像**✗
//!       ⇒ **★ 而**它测的是 `duplicating_a_layer_*`**✗（**∴ 「**复制图层**」功能 ✓）
//!         ⇒ **∴ 与**「**新建文档重复默认层**」**是两回事** ✓ ★**** ✓✓
//!   **∴ 变异检验（**第 303 轮 ✓）✗**：**把 `layer_default` 改回 `layer_1`**✗
//!     ⇒ **∴ 那条判据**仍然绿**✗ ⇒ **∴ 没牙** ✓**** ✓✓
//!   **⇒ ★ 所以 ✗ ★**：**本判据**专测那个 bug** ✓
//!
//! **∴ 判据（**两条 ✓）★**：
//!   **∴ ①** **`new_document` 之后**图层数必须 == 1** ✓
//!   **∴ ②** **那一个图层的 id 必须是 `layer_default`** ✓
//!
//! **∴ 变异点 ✗**：**把 `let default_layer = "layer_default";` 改回 `"layer_1"`**
//!   ⇒ **∴ 本判据**必红** ✓（**∴ 而**旧判据**不会** ✓）** ✓✓
//!
//! **∴ 借鉴来源（**用户第 593 轮 ✓）★**：
//!   **∴ GIMP 的**初始化投影**✗**：**新图像**只**建立一个默认图层** ✓
//!     ⇒ **∴ 而**我们与它的差别**✗：**我们**既有** fold 的自动插入**✗ ＋ **又有**工具的显式建层** ✓
//!       ⇒ **∴ 那**正是**双份来源**✗ ⇒ **∴ 所以**要用判据钉住**恰好一个** ✓**** ✓✓
//!
//! **∴ 两面（**AGENTS.md 第 3 条 ✓）★**：
//!   **∴ 收益**：**「**两个同名图层**」这个症状**不会再回来** ✓**** ✓✓
//!   **∴ 代价**：**把「**默认层 id 叫 `layer_default`**」写成了契约**✗
//!     ⇒ **∴ 若**将来**要改那个 id**✗ ⇒ **∴ 必须**同步改本判据** ✓**** ✓✓
//!
//! `cargo test -p yanshi-server --test new_document_single_layer`

use serde_json::json;
use yanshi_server::{DocumentSettings, Profile, ToolContext, ToolRegistry, Workspace};

const DOC: &str = "doc_new";

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, DOC, "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

#[test]
fn new_document_leaves_exactly_one_default_layer() {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    let registry = registry();

    // **★ 不要预建文档 ✗ ★**（第 305 轮的更正 ✓）：
    //   **∴ 我**第一版**先调了 `create_document`**✗
    //     ⇒ **∴ 而** `new_document` **对**已存在的 doc_id** 只**「打开」**✗
    //       （**∴ 见** `tools.rs`：「**该 doc_id 已存在 ⇒ 本次是**打开**」 ✓）
    //         ⇒ **★ 所以**它**根本不走创建路径**✗ ⇒ **∴ 那个 bug**不会出现** ✓ ★**** ✓✓
    //   **∴ 现在**：**直接**调 `new_document` 工具**✗ ⇒ **∴ 于是**它**必须**自己建文档** ✓
    //
    // **∴ 变异点（**本轮实测 ✓）★**：
    //   **∴ `patch -p1 -R < yanshi-duplicate-layer.patch`**✗（**∴ 恢复显式建层 ✓）
    //     ⇒ **∴ 本判据**必红** ✓（**∴ 而**加 `create_document` 时**它不会红** ✓）** ✓✓
    //
    // **★ 被测：直接调 `new_document` 工具 ✗ ★**（**∴ 不预建 ⇒ 走**创建**路径 ✓）
    let created = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "new_document",
            &json!({"doc_id": DOC, "width": 64, "height": 64}),
        )
    };
    assert_eq!(
        created["ok"],
        json!(true),
        "new_document 必须成功：{created}"
    );

    // **∴ 列图层 ✗**（**∴ 与**线上同一个工具 ✓）
    let listed = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "list_layers", &json!({}))
    };
    let layers = listed["layers"].as_array().cloned().unwrap_or_default();

    // **★ ① 恰好一个 ✗ ★**（**∴ bug 时是**两个都叫「图层 1」** ✓）
    assert_eq!(
        layers.len(),
        1,
        "**∴ 新建文档必须只有 1 个图层** ✗（**∴ bug 时是 2 个 ✓）：{listed}"
    );
    // **★ ② id 必须是 `layer_default` ✗ ★**（**∴ bug 时是 `layer_1`** ✓）
    //   ⇒ **∴ 这一条**才是**变异检验**能抓住的** ✓（**∴ 第 303 轮的教训 ✓）**
    assert_eq!(
        layers[0]["layer_id"],
        json!("layer_default"),
        "**∴ 默认层的 id 必须是 `layer_default`** ✗（**∴ 改回 `layer_1` 时本判据必红 ✓）：{listed}"
    );

    println!(
        "  ★ new_document 后图层数 = {}｜id = {} ★",
        layers.len(),
        layers[0]["layer_id"]
    );
}
