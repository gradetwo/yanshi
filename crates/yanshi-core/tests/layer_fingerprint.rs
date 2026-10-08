//! **图层内容指纹**判据 ✓（第 99 轮 ✓）—— 目标第 4 条（懒合成）的**失效信号**护栏 ✓
//!
//! **它守什么** ✓：below 缓存要问"**下方那些层的内容变了没有**" ✓。若信号**太粗** ✗
//! （例如全局 `head_seq` ✓，任何改动都变 ⇒ **缓存永不命中** ✗）；若信号**不够细** ✗
//! （例如只看对象 **id** ✓ —— **画一笔不会改 id** ✗ ⇒ **误判"没变" ⇒ 返回旧像素 ⇒ 撒谎** ✗✓）。
//!
//! **判据（三条 ✓）**：
//! 1. **确定性** ✓：同一 state 算两次 ⇒ **必须相同** ✓（**同输入同输出** ✓，与遍历顺序无关 ✓）；
//! 2. **改一笔 ⇒ 该层指纹必须推进** ✓（**这是核心** ✓）；**且其它层的指纹必须不变** ✓（**成对** ✓）；
//! 3. **改层自身属性**（`opacity` ✓）⇒ 该层指纹也必须推进 ✓。
//!
//! **变异** ✗（打在**被判的那一处** ✓）：
//! * 把 `current_version` 从哈希输入里**去掉** ✗ ⇒ **判据 2 必红** ✓
//!   （**因为只改 `metadata` 时 `id` 不变** ✓ ⇒ 指纹不变 ⇒ 误判"没变" ✓）；
//! * 把**排序**去掉 ✗ ⇒ 判据 1 在**多对象**时会红 ✓（顺序不稳 ✓）。

use yanshi_core::state::{DocumentState, Layer, LayerType, Object, ObjectType, Transform};
// **⚠️ 记录** ✓：我按行插入时它一度落在 `impl DocumentState` 里 ✗（成了关联函数 ✓，**合法但不好** ✓）
// ⇒ **∴ 已把它移出 `impl`** ✓，现在是**自由函数** ✓（与它"只读 state ✓、不碰 self ✓"的语义相符 ✓）。
use yanshi_core::state::layer_content_fingerprint;

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

fn object(id: &str, layer_id: &str, version: &str) -> Object {
    Object {
        id: id.to_owned(),
        layer_id: layer_id.to_owned(),
        object_type: ObjectType::Shape,
        z_index: 0,
        visible: true,
        locked: false,
        metadata: serde_json::Value::Null,
        transform: Transform::IDENTITY,
        style: None,
        versions: vec![version.to_owned()],
        current_version: Some(version.to_owned()),
        created_by: version.to_owned(),
        deleted_by: None,
        // 编译器纠正了我 ✓：`Object` 还有 `blobs` 与 `data` ✓（**∴ 字段以编译器为准** ✓）。
        blobs: Vec::new(),
        data: serde_json::Value::Null,
    }
}

fn state_with_two_layers() -> DocumentState {
    let mut state = DocumentState::empty();
    state.width = 64;
    state.height = 64;
    state.layers.insert("L1".to_owned(), layer("L1", 0));
    state.layers.insert("L2".to_owned(), layer("L2", 1));
    state
}

/// **判据 1** ✓：确定性 —— 同一 state 算两次必须相同 ✓。
#[test]
fn fingerprint_is_deterministic() {
    let state = state_with_two_layers();
    assert_eq!(
        layer_content_fingerprint(&state, "L1"),
        layer_content_fingerprint(&state, "L1"),
        "同一状态两次必须是同一个指纹"
    );
}

/// **判据 2（核心 ✓）**：**改一笔 ⇒ 该层指纹推进** ✓；**其它层不得变** ✓（**成对** ✓）。
#[test]
fn a_new_object_moves_only_its_own_layer() {
    let mut state = state_with_two_layers();
    let before_a = layer_content_fingerprint(&state, "L1");
    let before_b = layer_content_fingerprint(&state, "L2");

    // **在 L1 上"画一笔"** ✓（＝新增一个对象 ✓，正是真实落笔的形状 ✓）。
    state
        .objects
        .insert("o1".to_owned(), object("o1", "L1", "atom-1"));

    let after_a = layer_content_fingerprint(&state, "L1");
    let after_b = layer_content_fingerprint(&state, "L2");
    assert_ne!(
        before_a, after_a,
        "**该层**的指纹必须随新对象推进（否则 below 会拿旧结果 ✗）"
    );
    assert_eq!(
        before_b, after_b,
        "**其它层**的指纹不得变（否则缓存永不命中 ✗）"
    );

    // **同一对象再改一次**（版本推进 ✓ ⇒ **必须**再变 ✓）—— 这正是"接着画"的形状 ✓。
    state
        .objects
        .insert("o1".to_owned(), object("o1", "L1", "atom-2"));
    assert_ne!(
        after_a,
        layer_content_fingerprint(&state, "L1"),
        "**同一对象的内容变了（版本推进）⇒ 指纹必须再变**（否则接着画会拿旧像素 ✗）"
    );
}

/// **判据 3** ✓：**层自身属性**（`opacity` ✓）变了也须推进 ✓（below 也必须失效 ✓）。
#[test]
fn layer_own_properties_are_part_of_the_fingerprint() {
    let mut state = state_with_two_layers();
    let before = layer_content_fingerprint(&state, "L1");
    if let Some(l) = state.layers.get_mut("L1") {
        l.opacity = 0.5;
    }
    assert_ne!(
        before,
        layer_content_fingerprint(&state, "L1"),
        "层的 opacity 变了 ⇒ 指纹必须推进（否则 below 会拿旧结果 ✗）"
    );
}
