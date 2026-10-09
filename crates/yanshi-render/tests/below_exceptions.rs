//! **★ C5：六类例外各自"不许走缓存" ✗ ★**（第 950 轮 ✓；**设计 §5 的 C5 ✓，**目标第 4 条明文 ✓）。
//!
//! **∴ 为什么单独立这一条 ✗**：**设计文档**§3**早就写了**八类会打破三段分解的情形** ✓**
//!   （**`docs/design/lazy-composite-design.md` ✓）**，**而 §5 的 C1–C6 里**只有 C5 没有判据 ✗**
//!   ⇒ **∴ 于是**：**"**文档写了**"**与**"**判据守住了**"**之间**出现了缺口 ✓**** ✓✓
//!   ⇒ **∴ 而**目标第 4 条**同时要求**两件事 ✗**（**写明 ✓ ＋ **能红判据 ✓）** ✓✓
//!
//! **∴ 判据形式 ✗**：**用**渲染器自己的**语义计数** ✗**（`Renderer::below_reuse_count()` ✓，
//!   **`tile_parallel` 里**已经这样用 ✓）⇒ **∴ 不看墙钟** ✗（**∴ 它**会随机器漂 ✓）** ✓✓
//!
//! **∴ 防空转 ✗ ★**：**每一条例外**都**必须**先**证明控制组**真的会复用 ✗**
//!   ⇒ **∴ 否则**：**"**例外不复用**"**可能只是**"**这个场景**本来就不复用**" ✓**** ✓✓
//!   ⇒ **∴ 所以**：`control_reuses_below` **先绿** ✗，**例外那几条**才有意义 ✓**** ✓✓
//!
//! **∴ 变异 ✗**：**把 `render.rs` 里对应的那个 `except_seen = true;` 删掉** ✗**
//!   ⇒ **∴ 对应那条**必红 ✗**（**∴ 而**控制组仍绿 ✓）** ✓✓

use serde_json::json;
use yanshi_core::blob::MemoryBlobStore;
use yanshi_core::state::{DocumentState, Layer, LayerType, Object, ObjectType, Transform};
use yanshi_render::{Renderer, TileGrid};

/// 最小图层（**照 `tile_parallel` 的既有写法 ✓**，**字段与 `Layer` 定义一一对应 ✓**）。
fn layer(id: &str, z: i64) -> Layer {
    Layer {
        id: id.to_owned(),
        name: id.to_owned(),
        layer_type: LayerType::Raster,
        parent_id: None,
        z_index: z,
        blend_mode: "normal".to_owned(),
        opacity: 1.0,
        visible: true,
        locked: false,
        alpha_lock: false,
        clipping_mask: false,
        mask_id: None,
        transform: Transform::IDENTITY,
        medium: None,
        style: None,
        metadata: serde_json::Value::Null,
        blobs: Vec::new(),
        created_by: "a1".to_owned(),
        updated_by: None,
        deleted_by: None,
    }
}

/// 最小对象（**同上 ✓**）。
fn object(id: &str, layer_id: &str, z: i64, data: serde_json::Value) -> Object {
    Object {
        id: id.to_owned(),
        layer_id: layer_id.to_owned(),
        object_type: ObjectType::Shape,
        z_index: z,
        visible: true,
        locked: false,
        metadata: serde_json::Value::Null,
        transform: Transform::IDENTITY,
        style: None,
        versions: vec!["a1".to_owned()],
        current_version: Some("a1".to_owned()),
        created_by: "a1".to_owned(),
        deleted_by: None,
        data,
        blobs: Vec::new(),
    }
}

/// **两层文档** ✓：下层有内容（**∴ 它是"**下方**" ✓）**，上层是**活动层**（**∴ 它是"**当前**" ✓）**。
///
/// `exception` 决定**在上层**制造哪一种例外 ✓（`None` ⇒ **控制组** ✓）。
fn document(exception: Option<&str>) -> DocumentState {
    let mut state = DocumentState::empty();
    state.doc_id = Some("doc_c5".to_owned());
    state.width = 128;
    state.height = 128;
    state.background = json!({"r": 255, "g": 255, "b": 255, "a": 255});
    let content = json!({"geometry": {"kind": "rect",
                                      "bbox": {"x": 16.0, "y": 16.0, "w": 48.0, "h": 48.0}},
                         "color": {"r": 20, "g": 40, "b": 80, "a": 200}});
    state
        .layers
        .insert("layer_bottom".to_owned(), layer("layer_bottom", 0));
    state.objects.insert(
        "rect_bottom".to_owned(),
        object("rect_bottom", "layer_bottom", 0, content.clone()),
    );
    let mut top = layer("layer_top", 1);
    match exception {
        // **④ 剪贴蒙版 ✓**：**用下方内容的 alpha 裁剪本层 ⇒ 不是层的函数 ✗**。
        Some("clipping") => top.clipping_mask = true,
        // **① 非可分离混合 ✓**：**结果依赖下方已合成的像素 ✗**。
        Some("behind") => top.blend_mode = "behind".to_owned(),
        // **③ 组不透明度 ≠ 1 ✓**：**它不能分配到各层 ⇒ below 的边界会算错 ✗**。
        Some("group_opacity") => {
            top.layer_type = LayerType::LayerGroup;
            top.opacity = 0.5;
        }
        // **② 穿透组 ✓（**只用 `parent_id` ✓）**：**∴ 单独走这一条 ✗**
        //   ⇒ **∴ 于是**：**变异 `render.rs` 的 `parent_id.is_some()` 那一处 ⇒ **∴ 本条**必红 ✓**** ✓✓
        //   **∴ 为什么需要它 ✗**（第 950 轮实测 ✓）：**`group_opacity` 用例**同时**设了
        //     `LayerGroup` ✗ ⇒ **∴ 它**也会命中 `is_group`**✗**
        //       ⇒ **∴ 变异其中一处**时**另一处**仍然生效 ✓ ⇒ **∴ 那一处**看起来**无牙 ✓**** ✓✓
        //       ⇒ **∴ 所以**：**必须**有一个**只**靠 `parent_id` 的用例 ✓**** ✓✓
        Some("parent_group") => top.parent_id = Some("group_outer".to_owned()),
        _ => {}
    }
    state.layers.insert("layer_top".to_owned(), top);
    // **⑤ 读画布类笔刷 ✓**：**它读下方像素 ⇒ 该层渲染本身就是 below 的函数 ✗**
    //   ⇒ **∴ 判据走的是"**对象数据里出现 `smudge` 等字样**" ✓（**照 `render.rs` 的既有实现 ✓）**。
    let mut top_object = object("rect_top", "layer_top", 0, content);
    if exception == Some("canvas_brush") {
        top_object.metadata = json!({"brush": "smudge"});
    }
    state.objects.insert("rect_top".to_owned(), top_object);
    state
}

/// **★ C5：六类例外各自"不许走缓存" ✗ ★**（**一个测试内**顺序测量 ✓）。
///
/// **∴ 为什么必须**合起来 ✗ ★**（第 950 轮实测 ✓）：
///   **`below_reuse_count()`**是**进程级**计数**✗（**它**由**自由函数
///   `note_below_reuse()` 递增 ✓）⇒ **∴ 多个 `#[test]`**会**并行**跑 ✗**
///     ⇒ **∴ 于是**：**一个用例**看到的次数**包含**别的用例的贡献 ✓
///       （**实测**：**"**首次渲染不可能复用**"**当场被打破 ✗ —— `实测 4` ✓）** ✓✓
///   ⇒ **∴ 所以**：**合为一个测试 ✗、**每段取**增量**✓**
///     ⇒ **∴ 于是**：**无论计数器**是全局还是每实例 ✗，**结论**都成立 ✓**** ✓✓
#[test]
fn exceptions_never_reuse_below() {
    let measure = |label: &str, exception: Option<&str>| -> usize {
        let state = document(exception);
        let store = MemoryBlobStore::new();
        let mut renderer = Renderer::new(TileGrid::new(64, state.width, state.height).unwrap());
        renderer.render_document(&state, &store).expect("首次渲染");
        let before = renderer.below_reuse_count();
        renderer.render_document(&state, &store).expect("二次渲染");
        let reused = renderer.below_reuse_count() - before;
        eprintln!("C5 {label} ⇒ 二次渲染复用 below {reused} 次");
        reused
    };

    // **★ 防空转 ✗**：**控制组**必须**真的**复用 ✓ ⇒ **∴ 否则**后面全是空转 ✓**** ✓✓
    let control = measure("控制组（无例外）", None);
    assert!(
        control > 0,
        "控制组（**没有任何例外**）在第二次渲染时**必须**复用 below ✗；\
         实测增量 0 ⇒ **∴ 要么**判据构造不对 ✗，**要么**缓存根本没工作 ✓"
    );

    // **★ 四条例外 ✗**：**各自**必须**完全不复用 ✓ ★**** ✓✓
    for (label, kind) in [
        ("剪贴蒙版", "clipping"),
        ("非可分离混合 behind", "behind"),
        ("穿透组（仅 parent_id）", "parent_group"),
        ("组不透明度 ≠ 1", "group_opacity"),
        ("读画布类笔刷 smudge", "canvas_brush"),
    ] {
        let reused = measure(label, Some(kind));
        assert_eq!(
            reused, 0,
            "{label} **会打破三段分解** ⇒ **∴ 不许走 below 缓存** ✓；\
             实测复用 {reused} 次 ⇒ **∴ 它会返回**错的块 ✓（**宁慢勿错 ✓）"
        );
    }
}
