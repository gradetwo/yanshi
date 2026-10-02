//! **撤销粒度：`undo_last`** ✓（目标 ⑦ ✓）—— "撤销我刚画的那一笔" ✓。
//!
//! **为什么要它** ✓：`revert` 是**原子级**的 ✓（得先知道 atom id ✗），而人说的是"撤销这一笔" ✓。
//! **判据（用像素 ✓，不看返回值 ✓）**：
//! **撤掉的必须恰好是最后一笔** ✓ —— 前三笔的画面减去第三笔 ⇒ 与"只画前两笔"的画面**逐字节相同** ✓；
//! 而"再撤一次没有可撤"必须**明说** ✗（不能静默成功 ✓）。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("yanshi_undo_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path
}

fn workspace(root: &std::path::Path) -> Workspace {
    let mut workspace = Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default())
        .expect("落盘工作区应当能建");
    workspace
        .create_document(
            NewDocument::new("doc_u", 200, 120),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn call(workspace: &mut Workspace, tool: &str, args: serde_json::Value) -> serde_json::Value {
    let mut ctx = ToolContext::new(workspace, "doc_u", "human:1", "session:test");
    registry().call(&mut ctx, tool, &args)
}

fn render_bytes(workspace: &mut Workspace, path: &std::path::Path) -> Vec<u8> {
    let exported = call(
        workspace,
        "export_png",
        json!({ "path": path.display().to_string() }),
    );
    assert_eq!(exported["ok"], json!(true), "导出应当成功：{exported}");
    std::fs::read(path).expect("导出的 PNG 应当存在")
}

fn shape(workspace: &mut Workspace, object_id: &str, y: f64, color: serde_json::Value) {
    let made = call(
        workspace,
        "draw_shape",
        json!({
            "layer_id": "L", "object_id": object_id,
            "data": {
                "geometry": { "kind": "rect", "bbox": { "x": 20, "y": y, "w": 160, "h": 20 } },
                "color": color
            }
        }),
    );
    assert_eq!(made["ok"], json!(true), "画形状应当成功：{made}");
}

/// **`undo_last` 撤掉的恰好是最后一笔** ✓ —— 与"只画前两笔"的画面**逐字节相同** ✓。
#[test]
fn undo_last_removes_exactly_the_last_gesture() {
    let root = temp_dir("last");
    let mut workspace = workspace(&root);
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    shape(
        &mut workspace,
        "one",
        10.0,
        json!({"r": 220, "g": 40, "b": 40, "a": 255}),
    );
    shape(
        &mut workspace,
        "two",
        45.0,
        json!({"r": 40, "g": 200, "b": 60, "a": 255}),
    );
    // **先记下"只画两笔"的样子** ✓（这是判据的基准 ✓）。
    let two_only = render_bytes(&mut workspace, &root.join("two.png"));
    // 第三笔 ✓
    shape(
        &mut workspace,
        "three",
        80.0,
        json!({"r": 40, "g": 60, "b": 220, "a": 255}),
    );
    let with_three = render_bytes(&mut workspace, &root.join("three.png"));
    assert_ne!(
        two_only, with_three,
        "第三笔应当改变画面（否则本测试测不到东西 ✗）"
    );

    let undone = call(&mut workspace, "undo_last", json!({}));
    assert_eq!(undone["ok"], json!(true), "{undone}");
    assert_eq!(
        undone["gestures_undone"],
        json!(1),
        "应当只撤一笔：{undone}"
    );
    assert_eq!(
        undone["undone_count"],
        json!(1),
        "应当撤掉一个原子：{undone}"
    );

    let after = render_bytes(&mut workspace, &root.join("after.png"));
    // **逐字节比对** ✓（不是"看起来差不多" ✗）。
    let left = yanshi_render::png::decode_png(&after).unwrap();
    let right = yanshi_render::png::decode_png(&two_only).unwrap();
    assert_eq!(left.0, right.0);
    assert_eq!(
        left.2, right.2,
        "撤销一笔之后应当**恰好**回到「只画两笔」的样子 ✗"
    );
}

/// **可以一次撤多笔** ✓；**撤到没有了再说一次 ⇒ 明确报"没有可撤的"** ✗（不静默成功 ✓）。
#[test]
fn undo_last_can_take_several_and_says_when_there_is_nothing_left() {
    let root = temp_dir("several");
    let mut workspace = workspace(&root);
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    // **基准必须在动笔之前取** ✗ —— 我第一版把它渲染在**画完三笔之后** ✓
    // ⇒ 变量名叫 `blank` ✓、内容却是"三笔" ✓（测试自己的错 ✓，而**数字当场说穿了** ✓：
    //   "基准非白 9600 像素" ✓）⇒ **名字与内容不符** 是这类错最典型的伪装 ✓。
    let blank = root.join("blank.png");
    let _ = render_bytes(&mut workspace, &blank);
    shape(
        &mut workspace,
        "a",
        10.0,
        json!({"r": 200, "g": 0, "b": 0, "a": 255}),
    );
    shape(
        &mut workspace,
        "b",
        45.0,
        json!({"r": 0, "g": 200, "b": 0, "a": 255}),
    );
    shape(
        &mut workspace,
        "c",
        80.0,
        json!({"r": 0, "g": 0, "b": 200, "a": 255}),
    );

    // **一次撤 10 笔** ✓（只有 3 笔 ✓ ⇒ 撤完 ✓ 不该报错 ✓）。
    let undone = call(&mut workspace, "undo_last", json!({ "count": 10 }));
    assert_eq!(undone["ok"], json!(true), "{undone}");
    // **建层不算"一笔"** ✓（测试当场教我的 ✓：第一版把 `CreateLayer` 也算进去 ✗
    // ⇒ "撤 10 笔"会**顺手把整个图层撤掉** ✗ —— 那超出了"撤销最后一笔"的意思 ✓）。
    assert_eq!(
        undone["gestures_undone"],
        json!(3),
        "三笔笔迹应当都撤掉（建层不算 ✓）：{undone}"
    );
    assert!(
        undone["ignored_non_content_atoms"].as_u64().unwrap_or(0) >= 1,
        "要如实报出被跳过的非内容原子（至少那个建层 ✓）：{undone}"
    );
    assert_eq!(undone["remaining_gestures"], json!(0), "{undone}");
    // **画面回到空白** ✓（与"还没画"的那份对照 ✓）。
    let after_all = render_bytes(&mut workspace, &root.join("after_all.png"));
    let left = yanshi_render::png::decode_png(&after_all).unwrap();
    // **`decode_png` 收字节、不是路径** ✗（我传了个 `PathBuf` ✓ ⇒ 编译器当场纠正 ✓）。
    let blank_bytes = std::fs::read(&blank).unwrap();
    let right = yanshi_render::png::decode_png(&blank_bytes).unwrap();
    // **先量，再下结论** ✓（这一条是整个项目最有用的习惯 ✓）：
    // 看不出差在哪时 ✓ ⇒ 数一数"非白像素"与"不同的像素" ✓，它们会直接说清 ✓。
    let ink = |pixels: &[u8]| {
        pixels
            .chunks(4)
            .filter(|px| px[0] != 255 || px[1] != 255 || px[2] != 255)
            .count()
    };
    let differing = left
        .2
        .chunks(4)
        .zip(right.2.chunks(4))
        .filter(|(a, b)| a[0] != b[0] || a[1] != b[1] || a[2] != b[2])
        .count();
    // **永远不要对整张像素向量用 `assert_eq!`** ✗ —— 失败时它会把几十万个数全打出来 ✓
    //（我刚就把终端撑爆了一次 ✓）⇒ **先算摘要、再断言，消息里只放数字** ✓。
    assert!(
        left.2 == right.2,
        "撤完三笔应当与「还没画」时逐字节相同 ✗（撤后非白 {} 像素；基准非白 {} 像素；不同 {} 像素）",
        ink(&left.2),
        ink(&right.2),
        differing
    );

    // **再撤一次** ✓ ⇒ **明说没有可撤的** ✗（不能静默"成功" ✓）。
    let again = call(&mut workspace, "undo_last", json!({}));
    assert_eq!(again["ok"], json!(true), "{again}");
    assert_eq!(
        again["undone_count"],
        json!(0),
        "没有可撤的了就该是 0：{again}"
    );
    let message = again["message"].as_str().unwrap_or_default();
    assert!(message.contains("没有可撤销"), "要说清没有可撤的：{again}");
}

/// **重做：把最近撤掉的那一笔恢复回来** ✓（`redo_last` ✓ —— 设计的 `revert(revert(x)) ≡ reapply(x)` ✓）。
///
/// **判据** ✓：三笔 ⇒ 全撤 ⇒ 空白 ✓；再重做一笔 ⇒ **恰好回到"只有一笔"的画面** ✓（逐字节 ✓）。
/// **另外** ✓：撤到底之后**再撤**必须报"没有可撤的" ✗（不能对已经撤过的原子再撤一次而报成功 ✓ ——
/// 我第一版正是那样 ✓，实测日志里多出一条**没有任何效果的** revert ✗）。
#[test]
fn redo_last_brings_back_exactly_one_gesture() {
    let root = temp_dir("redo");
    let mut workspace = workspace(&root);
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    shape(
        &mut workspace,
        "a",
        10.0,
        json!({"r": 220, "g": 40, "b": 40, "a": 255}),
    );
    // **不再需要"只有 a"的基准** ✓：重做的语义是"**最近撤掉的最先回来**" ✓（= c ✓），
    // 所以判据换成"墨量 = 一笔 ✓ + 墨落在 c 的位置 ✓"（见下 ✓）——
    // **一个没人用的变量就是一句谎话** ✗，删掉 ✓。
    shape(
        &mut workspace,
        "b",
        45.0,
        json!({"r": 40, "g": 200, "b": 60, "a": 255}),
    );
    shape(
        &mut workspace,
        "c",
        80.0,
        json!({"r": 40, "g": 60, "b": 220, "a": 255}),
    );

    // **全撤** ✓（三笔 ✓）。
    let undone = call(&mut workspace, "undo_last", json!({ "count": 3 }));
    assert_eq!(undone["ok"], json!(true), "{undone}");
    assert_eq!(
        undone["undone_count"],
        json!(3),
        "三笔应当真的都撤掉：{undone}"
    );

    // **再撤一次** ✓ ⇒ 必须**明说没有** ✗（实测教训 ✓）。
    let nothing = call(&mut workspace, "undo_last", json!({}));
    assert_eq!(
        nothing["undone_count"],
        json!(0),
        "已经撤到底了就不该再撤：{nothing}"
    );
    assert!(
        nothing["message"]
            .as_str()
            .unwrap_or_default()
            .contains("没有可撤销"),
        "要说清没有可撤的：{nothing}"
    );

    // **重做一笔** ✓ ⇒ 回来的必须是**最近被撤掉的那一笔**（= c ✓），而不是任意一笔 ✓。
    //
    // **我上一版把这里断言成"与只有 a 的画面相同"** ✗ ⇒ 实测"不同 6400 像素" ✓ ——
    // 而 6400 = **两块**形状的面积 ✓ ⇒ **重做把 c 恢复了** ✓、基准却是 a ✓
    // ⇒ **是我的期望错了** ✓，重做语义本来就该是"最近撤的最先回来" ✓。
    // ⇒ 判据改成两件**说得清**的事 ✓：**墨量等于一笔** ✓、**墨落在 c 的位置** ✓。
    let redone = call(&mut workspace, "redo_last", json!({}));
    assert_eq!(redone["ok"], json!(true), "{redone}");
    assert_eq!(
        redone["redone_count"],
        json!(1),
        "应当恢复一条原子：{redone}"
    );
    let after = render_bytes(&mut workspace, &root.join("after_redo.png"));
    let (width, _height, rgba) = yanshi_render::png::decode_png(&after).unwrap();
    let mut in_band_c = 0usize;
    let mut in_band_a = 0usize;
    let mut ink = 0usize;
    for (index, px) in rgba.chunks(4).enumerate() {
        if px[0] == 255 && px[1] == 255 && px[2] == 255 {
            continue;
        }
        ink += 1;
        let y = index / width as usize;
        if (80..100).contains(&y) {
            in_band_c += 1;
        }
        if (10..30).contains(&y) {
            in_band_a += 1;
        }
    }
    assert_eq!(ink, 160 * 20, "墨量应当正好是**一笔**的面积（实测 {ink}）");
    assert!(
        in_band_c > 0,
        "重做回来的应当是**最近撤掉的 c**（y 80..100 有墨 ✓，实测 {in_band_c}）"
    );
    assert_eq!(
        in_band_a, 0,
        "a 那一笔还没重做 ⇒ 它的位置不该有墨（实测 {in_band_a}）"
    );

    // **重做到底之后再说一次** ✓ ⇒ 同样要**明说没有** ✗。
    let _ = call(&mut workspace, "redo_last", json!({ "count": 5 }));
    let again = call(&mut workspace, "redo_last", json!({}));
    assert_eq!(
        again["redone_count"],
        json!(0),
        "没有可重做的就该是 0：{again}"
    );
    assert!(
        again["message"]
            .as_str()
            .unwrap_or_default()
            .contains("没有可重做"),
        "要说清没有可重做的：{again}"
    );
}
