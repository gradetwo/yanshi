//! 介质描述符随 `import_image` 一并记录（设计 11.1 ✓）。
//!
//! **设计原话** ✓："插件 id + version 随原子记录，升级不自动改变旧文档渲染" ✓。
//! 此前查看器用**两条**原子做这件事 ✗（导入 ✓ + `replace_object_data` 钉描述符 ✓）——
//! 实测每次原子提交都有实打实的成本 ✓ ⇒ 一条原子能说清的事分两条就是白花钱 ✓。
//!
//! **两条性质** ✓：
//! * **一条原子记全** ✓（描述符 + 位图 + 区域 ✓）；
//! * **与老的两次提交在渲染相关字段上逐字一致** ✓ —— 这条是**兼容性守卫** ✓：
//!   对象数据决定怎么画 ✓，它必须没变 ✓（多出来的只是描述符键 ✓，而设计要求记下它 ✓）。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_mi", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn put_blob(workspace: &mut Workspace) -> String {
    let bytes: Vec<u8> = (0..8 * 8)
        .flat_map(|index| [index as u8 * 3, 90, 40, 255])
        .collect();
    workspace
        .store()
        .put(&bytes)
        .expect("blob 应能入库")
        .to_string()
}

fn data_of(workspace: &mut Workspace, object_id: &str) -> serde_json::Value {
    let state = workspace.document_mut("doc_mi").unwrap().state().clone();
    state
        .objects
        .get(object_id)
        .map(|object| object.data.clone())
        .unwrap_or(serde_json::Value::Null)
}

/// **一条原子记全** ✓：位图 ✓、区域 ✓、尺寸 ✓、介质描述符 ✓。
#[test]
fn one_atom_records_the_bitmap_and_the_medium_descriptor() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_mi", 64, 64),
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
    let before = workspace.document("doc_mi").unwrap().log().len();
    let imported = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "import_image",
            &json!({
                "layer_id": "L", "object_id": "dab_1",
                "bitmap": {"blob_hash": blob, "size": 256, "mime_type": "image/x-yanshi-raw"},
                "region": {"x": 4.0, "y": 6.0, "w": 8.0, "h": 8.0},
                "medium": {"id": "oil", "version": 2},
            }),
        )
    };
    assert_eq!(imported["ok"], json!(true), "{imported}");
    let after = workspace.document("doc_mi").unwrap().log().len();
    assert_eq!(after - before, 1, "**只该多一条原子** ✓：{imported}");
    let data = data_of(&mut workspace, "dab_1");
    assert_eq!(
        data["medium"]["id"],
        json!("oil"),
        "描述符要随原子记下：{data}"
    );
    assert_eq!(data["medium"]["version"], json!(2), "{data}");
    assert_eq!(data["bitmap"]["blob_hash"], json!(blob), "{data}");
    assert_eq!(data["region"]["w"], json!(8.0), "{data}");
    assert_eq!(data["width"], json!(8), "{data}");
    assert_eq!(data["height"], json!(8), "{data}");
    // **blob 引用仍被发现** ✓（`all_blob_refs` 扫净荷 ✓）—— 这决定了校验与保留 ✓。
    let atom = workspace
        .document("doc_mi")
        .unwrap()
        .log()
        .iter()
        .find(|atom| {
            atom.payload
                .get("object_id")
                .and_then(serde_json::Value::as_str)
                == Some("dab_1")
        })
        .cloned()
        .expect("应能找到那条原子");
    assert!(
        atom.all_blob_refs()
            .iter()
            .any(|hash| hash.to_string() == blob),
        "导入原子的 blob 引用必须仍能被发现：{:?}",
        atom.all_blob_refs()
    );
}

/// **兼容性守卫** ✓：与老的"导入 + `replace_object_data`"两步相比，
/// **渲染相关字段逐字一致** ✓（`bitmap`/`region`/`width`/`height` ✓），只是多了描述符 ✓。
#[test]
fn the_one_atom_form_matches_the_old_two_step_form_on_every_rendering_field() {
    let mut one = workspace();
    one.create_document(
        NewDocument::new("doc_mi", 64, 64),
        "human:1",
        "session:test",
    )
    .unwrap();
    let registry = registry();
    let blob = put_blob(&mut one);
    {
        let mut ctx = context(&mut one);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    }
    // ① 新做法：一条原子 ✓。
    {
        let mut ctx = context(&mut one);
        let imported = registry.call(
            &mut ctx,
            "import_image",
            &json!({"layer_id": "L", "object_id": "obj_new",
                    "bitmap": {"blob_hash": blob, "size": 256, "mime_type": "image/x-yanshi-raw"},
                    "region": {"x": 4.0, "y": 6.0, "w": 8.0, "h": 8.0},
                    "medium": {"id": "oil", "version": 2}}),
        );
        assert_eq!(imported["ok"], json!(true), "{imported}");
    }
    // ② 老做法：两条原子 ✓（导入 + 替换 ✓）。
    {
        let mut ctx = context(&mut one);
        let imported = registry.call(
            &mut ctx,
            "import_image",
            &json!({"layer_id": "L", "object_id": "obj_old",
                    "bitmap": {"blob_hash": blob, "size": 256, "mime_type": "image/x-yanshi-raw"},
                    "region": {"x": 4.0, "y": 6.0, "w": 8.0, "h": 8.0}}),
        );
        assert_eq!(imported["ok"], json!(true), "{imported}");
        let replaced = registry.call(
            &mut ctx,
            "replace_object_data",
            &json!({"object_id": "obj_old",
                    "data": {"bitmap": {"blob_hash": blob, "size": 256, "mime_type": "image/x-yanshi-raw"},
                             "region": {"x": 4.0, "y": 6.0, "w": 8.0, "h": 8.0},
                             "width": 8, "height": 8,
                             "medium": {"id": "oil", "version": 2}}}),
        );
        assert_eq!(replaced["ok"], json!(true), "{replaced}");
    }
    let new_data = data_of(&mut one, "obj_new");
    let old_data = data_of(&mut one, "obj_old");
    for key in ["bitmap", "region", "width", "height", "medium"] {
        assert_eq!(
            new_data.get(key),
            old_data.get(key),
            "渲染相关字段 {key} 必须逐字一致：新 {new_data}｜旧 {old_data}"
        );
    }
    // 老做法的对象数据**只有**一个 `data` 子对象的内容 ✓；新做法是净荷 ✓
    // ⇒ 新做法会多出 `object_id`/`layer_id`/`type` 这几个**不影响渲染**的键 ✓ —— 明确记下来 ✓。
    assert_eq!(
        new_data["type"],
        json!("raster_patch"),
        "对象类型仍在 ✓：{new_data}"
    );
}
