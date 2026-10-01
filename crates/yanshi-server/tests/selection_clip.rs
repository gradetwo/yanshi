//! 选区**约束落笔**（设计 4.4；语义由用户确认为路线 A）。
//!
//! **当前状态（本轮核实 ✓）**：**接线已在** ✓，本文件 **6 条用例全部通过** ✓、
//! **没有任何 `#[ignore]`** ✓（我按文首那句"接线撤回中"去找被忽略的用例 ✓ ⇒ 一个也没找到 ✗，
//! 才发现**注释过期了** ✗ —— 这正是本项目反复出现的**文档漂移** ✓）。
//! 下面那段撤回记录**保留** ✓（诊断价值高 ✓），但**不要再按它判断现状** ✗。
//!
//! ---
//!
//! （以下为历史记录：接线撤回期间写的分析）
//!
//! 现状：内核侧的裁剪做法（把覆盖度折进印章）已在渲染器层面验证正确 ✓，
//! 并有独立单测 ✓；但在**服务端分次（脏区）渲染**下走裁剪分支会丢掉笔画 ✗
//! （实测：内核确实画了 9 个印章 ✓，但服务到的画面里看不到笔画 ✗ ⇒ 用户会丢失笔画 ✗）。
//! 下一轮先比对 `stamp_stroke` 与裁剪路径在**增量盖章/脏区分块**上的差异 ✓，再恢复接线 ✓。
//!
//! 现状（本轮实测，如实记录）：
//! * 选区几何模块（`yanshi-render::selection`）**正确** ✓：直测 `coverage(64,64)=1`、`coverage(20,64)=0` ✓；
//! * 选区工具层（create/delete/list）**正确** ✓，有独立测试 ✓；
//! * 渲染期接线**未通过** ✗：在对象循环里按"对象晚于选区创建"做裁剪后，
//!   实测选区**内**的落笔也消失 ✗（调试输出显示选区外 1580 个像素被按预期还原 ✓、
//!   最大覆盖度 1 ✓ ⇒ 几何与还原逻辑都对 ✓，问题出在"裁剪与绘制/合成的先后"上 ✓）；
//! * 由于错误的表现形式是**静默吞掉用户笔画** ✗（不可接受），本轮**撤回接线** ✓，
//!   把意图与证据留在本文件与 implementation-notes ✓，下一轮重新设计接线方式 ✓。
//!
//! 下方两个用例在接线上线后应当恢复为普通测试（去掉 `#[ignore]`）✓。
//!
//! 要求（用户决策）：
//! 1. 选区只约束**之后新落笔**的像素 ✓；
//! 2. **不改写已有内容** ✓；
//! 3. **可撤销**：删掉选区原子后，已画内容恢复"未被裁剪"的样子 ✓
//!    （因为裁剪在渲染期按日志重算 ✓，不烘焙进像素 ✓）。
//!
//! 测试判据刻意**不依赖具体颜色** ✓：只比较"绘制前后哪些像素被改动" ✓，
//! 并检查这些像素是否落在选区内 ✓（颜色数值随色彩管理变化，改动位置不会 ✓）。

use serde_json::json;
use yanshi_core::Bbox;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_clip", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn render(workspace: &mut Workspace) -> Vec<u8> {
    workspace
        .render_region_raw("doc_clip", Bbox::new(0.0, 0.0, 128.0, 128.0))
        .expect("渲染应成功")
        .2
}

/// 返回与基准相比**被改动**的像素坐标集合。
fn changed(base: &[u8], now: &[u8]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for index in 0..now.len() / 4 {
        if base[index * 4..index * 4 + 4] != now[index * 4..index * 4 + 4] {
            out.push((index % 128, index / 128));
        }
    }
    out
}

#[test]
fn a_selection_clips_new_painting_and_is_reversible() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_clip", 128, 128),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    }
    // 已有内容：铺满底色（**等它渲染完成** ✓，否则会与后面的笔画渲染竞态 ✗）。
    let filled = {
        let mut ctx = context(&mut workspace);
        // **对象 id 决定叠放次序**（`objects_in_layer` 按 id 的 BTreeMap 序 ✓）：
        // 底色取一个排在前面的 id ✓，笔画取 `z_` 前缀 ✓ —— 否则后画的底色会把笔画盖住 ✗
        // （本用例曾经因此报"笔画完全没有痕迹" ✗，而我一度误以为是自己的选区裁剪 ✗）。
        registry.call(
            &mut ctx,
            "fill",
            &json!({"layer_id": "L", "object_id": "a_background",
                    "data": {"color": {"r": 40, "g": 120, "b": 200, "a": 255},
                             "region": {"x": 0, "y": 0, "w": 128, "h": 128}}}),
        )
    };
    assert_eq!(filled["ok"], json!(true), "{filled}");
    let baseline = render(&mut workspace);

    // 选区：中间 40..88 的竖直带。
    {
        let mut ctx = context(&mut workspace);
        let created = registry.call(
            &mut ctx,
            "create_selection",
            &json!({"selection_id": "sel1",
                    "shape": {"kind": "rect", "bbox": {"x": 40.0, "y": 0.0, "w": 48.0, "h": 128.0}},
                    "feather": 0.0, "invert": false, "mode": "new"}),
        );
        assert_eq!(created["ok"], json!(true), "{created}");
    }

    // 横贯整幅画一条粗线：应**只**在选区内留下改动。
    {
        let mut ctx = context(&mut workspace);
        let drawn = registry.call(
            &mut ctx,
            "draw_stroke",
            &json!({"layer_id": "L", "object_id": "z_line1",
                    "data": {"points": [[4.0, 64.0], [124.0, 64.0]], "size": 20.0,
                             "hardness": 1.0,
                             "color": {"r": 250, "g": 240, "b": 20, "a": 255}}}),
        );
        assert_eq!(drawn["ok"], json!(true), "{drawn}");
    }
    let clipped = render(&mut workspace);
    let clipped_changes = changed(&baseline, &clipped);
    assert!(
        !clipped_changes.is_empty(),
        "选区内应当有落笔（否则用例前置条件不成立）"
    );
    let outside: Vec<(usize, usize)> = clipped_changes
        .iter()
        .copied()
        .filter(|(x, _)| *x < 40 || *x >= 88)
        .collect();
    assert!(
        outside.is_empty(),
        "选区外不应有新落笔（越界改动 {} 个像素，例如 {:?}）",
        outside.len(),
        outside.iter().take(5).collect::<Vec<_>>()
    );

    // 删除选区 ⇒ 同一笔画应当"铺满"，即选区外也出现改动 ⇒ 证明裁剪**可逆**且未烘焙进像素。
    {
        let mut ctx = context(&mut workspace);
        let deleted = registry.call(
            &mut ctx,
            "delete_selection",
            &json!({"selection_id": "sel1"}),
        );
        assert_eq!(deleted["ok"], json!(true), "{deleted}");
    }
    let unclipped = render(&mut workspace);
    let unclipped_changes = changed(&baseline, &unclipped);
    let outside_after: Vec<(usize, usize)> = unclipped_changes
        .iter()
        .copied()
        .filter(|(x, _)| *x < 40 || *x >= 88)
        .collect();
    assert!(
        !outside_after.is_empty(),
        "删除选区后，同一笔画应恢复为不受约束（选区外仍无改动 ⇒ 裁剪被错误地烘焙进了像素）"
    );
}

#[test]
fn without_a_selection_rendering_is_unchanged() {
    // 向后兼容的硬性要求：**没有选区时渲染结果与接线前逐字节一致**。
    // 这里用"同一文档渲染两次"与"有无选区机制不影响无选区文档"来守住这一点：
    // 无选区文档渲染两次必须完全相同（确定性 ✓），且改动集合包含整条笔画。
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_clip", 128, 128),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    }
    let first = render(&mut workspace);
    let second = render(&mut workspace);
    assert_eq!(first, second, "无选区时渲染必须确定");

    {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "draw_stroke",
            &json!({"layer_id": "L", "object_id": "z_line2",
                    "data": {"points": [[4.0, 64.0], [124.0, 64.0]], "size": 20.0,
                             "hardness": 1.0,
                             "color": {"r": 250, "g": 240, "b": 20, "a": 255}}}),
        );
    }
    let after = render(&mut workspace);
    let changes = changed(&first, &after);
    assert!(
        changes.iter().any(|(x, _)| *x < 40),
        "没有选区时笔画应铺满（左侧也应有改动）"
    );
}

/// 形状/填充同样受选区约束 ✓（与笔触同一"逐像素覆盖度"模式 ✓）。
#[test]
fn a_selection_clips_a_fill_and_is_reversible() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_clip", 128, 128),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    }
    let blank = render(&mut workspace);

    // 选区：左半（0..64）。
    {
        let mut ctx = context(&mut workspace);
        let created = registry.call(
            &mut ctx,
            "create_selection",
            &json!({"selection_id": "sel_fill",
                    "shape": {"kind": "rect", "bbox": {"x": 0.0, "y": 0.0, "w": 64.0, "h": 128.0}},
                    "feather": 0.0, "invert": false, "mode": "new"}),
        );
        assert_eq!(created["ok"], json!(true), "{created}");
    }

    // 铺满整幅的填充：应**只**在左半生效。
    {
        let mut ctx = context(&mut workspace);
        let filled = registry.call(
            &mut ctx,
            "fill",
            &json!({"layer_id": "L", "object_id": "z_fill",
                    "data": {"color": {"r": 250, "g": 40, "b": 40, "a": 255},
                             "region": {"x": 0, "y": 0, "w": 128, "h": 128}}}),
        );
        assert_eq!(filled["ok"], json!(true), "{filled}");
    }
    let clipped = render(&mut workspace);
    let changes = changed(&blank, &clipped);
    assert!(!changes.is_empty(), "选区内应当被填充（前置条件）");
    let outside: Vec<(usize, usize)> = changes.iter().copied().filter(|(x, _)| *x >= 64).collect();
    assert!(
        outside.is_empty(),
        "选区外不应被填充（越界 {} 个像素，例如 {:?}）",
        outside.len(),
        outside.iter().take(5).collect::<Vec<_>>()
    );

    // 删除选区 ⇒ 同一填充恢复为铺满整幅 ✓（可逆 ✓）。
    {
        let mut ctx = context(&mut workspace);
        let deleted = registry.call(
            &mut ctx,
            "delete_selection",
            &json!({"selection_id": "sel_fill"}),
        );
        assert_eq!(deleted["ok"], json!(true), "{deleted}");
    }
    let unclipped = render(&mut workspace);
    let after = changed(&blank, &unclipped);
    assert!(
        after.iter().any(|(x, _)| *x >= 64),
        "删除选区后填充应恢复为整幅（选区外仍无改动 ⇒ 裁剪被错误地烘焙进像素）"
    );
}

/// 文本对象同样受选区约束 ✓（与笔触/擦除/形状同一"逐像素覆盖度"模式 ✓）。
#[test]
fn a_selection_clips_text() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_clip", 128, 128),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    }
    let blank = render(&mut workspace);

    // 选区：左半 0..40（文本放在 10.. 处，会跨过选区边界）。
    {
        let mut ctx = context(&mut workspace);
        let created = registry.call(
            &mut ctx,
            "create_selection",
            &json!({"selection_id": "sel_text",
                    "shape": {"kind": "rect", "bbox": {"x": 0.0, "y": 0.0, "w": 40.0, "h": 128.0}},
                    "feather": 0.0, "invert": false, "mode": "new"}),
        );
        assert_eq!(created["ok"], json!(true), "{created}");
    }
    {
        let mut ctx = context(&mut workspace);
        let drawn = registry.call(
            &mut ctx,
            "draw_text",
            &json!({"layer_id": "L", "object_id": "z_text",
                    "data": {"text": "ABCDEFGH", "font": "builtin", "size": 14.0,
                             "color": {"r": 250, "g": 250, "b": 250, "a": 255},
                             "position": [4.0, 40.0], "align": "left"}}),
        );
        assert_eq!(drawn["ok"], json!(true), "{drawn}");
    }
    let clipped = render(&mut workspace);
    let changes = changed(&blank, &clipped);
    assert!(!changes.is_empty(), "选区内应当有文本（前置条件）");
    let outside: Vec<(usize, usize)> = changes.iter().copied().filter(|(x, _)| *x >= 40).collect();
    assert!(
        outside.is_empty(),
        "选区外不应有文本像素（越界 {} 个，例如 {:?}）",
        outside.len(),
        outside.iter().take(5).collect::<Vec<_>>()
    );

    // 删除选区 ⇒ 整行文字都应出现 ✓（可逆 ✓）。
    {
        let mut ctx = context(&mut workspace);
        let deleted = registry.call(
            &mut ctx,
            "delete_selection",
            &json!({"selection_id": "sel_text"}),
        );
        assert_eq!(deleted["ok"], json!(true), "{deleted}");
    }
    let unclipped = render(&mut workspace);
    let after = changed(&blank, &unclipped);
    assert!(
        after.iter().any(|(x, _)| *x >= 40),
        "删除选区后整行文字都应出现（选区外仍空白 ⇒ 裁剪被错误地烘焙进像素）"
    );
}

/// 液化同样受选区约束 ✓（按**增量**衰减 ⇒ 选区外逐字节不变 ✓、可逆 ✓）。
#[test]
fn a_selection_clips_liquify() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_clip", 128, 128),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
        // 有结构的内容：竖条纹，便于看出位移。
        for index in 0..8 {
            let x = 8.0 + index as f64 * 16.0;
            registry.call(
                &mut ctx,
                "draw_stroke",
                &json!({"layer_id": "L", "object_id": format!("a_{index:02}"),
                        "data": {"points": [[x, 0.0], [x, 128.0]], "size": 6.0, "hardness": 1.0,
                                 "color": {"r": 30, "g": 30, "b": 200, "a": 255}}}),
            );
        }
    }
    let before = render(&mut workspace);

    // 选区：左半 0..64。
    {
        let mut ctx = context(&mut workspace);
        let created = registry.call(
            &mut ctx,
            "create_selection",
            &json!({"selection_id": "sel_liquify",
                    "shape": {"kind": "rect", "bbox": {"x": 0.0, "y": 0.0, "w": 64.0, "h": 128.0}},
                    "feather": 0.0, "invert": false, "mode": "new"}),
        );
        assert_eq!(created["ok"], json!(true), "{created}");
    }
    // 液化：在选区边界附近推一把，作用圈横跨选区内外。
    {
        let mut ctx = context(&mut workspace);
        let pushed = registry.call(
            &mut ctx,
            "liquify_push",
            &json!({"layer_id": "L", "object_id": "z_push",
                    "points": [[30.0, 64.0], [90.0, 64.0]], "size": 60.0,
                    "strength": 1.5, "direction": [0.0, -1.0]}),
        );
        assert_eq!(pushed["ok"], json!(true), "{pushed}");
    }
    let after = render(&mut workspace);
    let changes = changed(&before, &after);
    assert!(!changes.is_empty(), "选区内应被液化（前置条件）");
    let outside: Vec<(usize, usize)> = changes.iter().copied().filter(|(x, _)| *x >= 64).collect();
    assert!(
        outside.is_empty(),
        "选区外不应被液化（越界 {} 个像素，例如 {:?}）",
        outside.len(),
        outside.iter().take(5).collect::<Vec<_>>()
    );

    // 删除选区 ⇒ 同一次液化恢复为不受约束 ✓（可逆 ✓）。
    {
        let mut ctx = context(&mut workspace);
        let deleted = registry.call(
            &mut ctx,
            "delete_selection",
            &json!({"selection_id": "sel_liquify"}),
        );
        assert_eq!(deleted["ok"], json!(true), "{deleted}");
    }
    let unclipped = render(&mut workspace);
    let after_delete = changed(&before, &unclipped);
    assert!(
        after_delete.iter().any(|(x, _)| *x >= 64),
        "删除选区后液化应恢复为不受约束（选区外仍无变化 ⇒ 裁剪被错误地烘焙进像素）"
    );
}

/// 修图（仿制/修复/涂抹）同样受选区约束 ✓ —— 最后一个图元 ✓。
#[test]
fn a_selection_clips_retouch() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_clip", 128, 128),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
        // 内容：左半蓝、右半红，便于看出"仿制"是否跨过选区。
        registry.call(
            &mut ctx,
            "draw_shape",
            &json!({"layer_id": "L", "object_id": "a_left",
                    "data": {"geometry": {"kind": "rect", "bbox": {"x": 0.0, "y": 0.0, "w": 64.0, "h": 128.0}},
                             "color": {"r": 20, "g": 40, "b": 220, "a": 255}}}),
        );
        registry.call(
            &mut ctx,
            "draw_shape",
            &json!({"layer_id": "L", "object_id": "a_right",
                    "data": {"geometry": {"kind": "rect", "bbox": {"x": 64.0, "y": 0.0, "w": 64.0, "h": 128.0}},
                             "color": {"r": 230, "g": 30, "b": 30, "a": 255}}}),
        );
    }
    let before = render(&mut workspace);

    // 选区：右半 64..128（仿制目标落在右半）。
    {
        let mut ctx = context(&mut workspace);
        let created = registry.call(
            &mut ctx,
            "create_selection",
            &json!({"selection_id": "sel_retouch",
                    "shape": {"kind": "rect", "bbox": {"x": 64.0, "y": 0.0, "w": 64.0, "h": 128.0}},
                    "feather": 0.0, "invert": false, "mode": "new"}),
        );
        assert_eq!(created["ok"], json!(true), "{created}");
    }
    // 仿制：从右半采样、涂到右半（选区之外也有落点）。
    {
        let mut ctx = context(&mut workspace);
        let cloned = registry.call(
            &mut ctx,
            "clone_stamp",
            &json!({"layer_id": "L", "points": [[72.0, 40.0], [120.0, 40.0]],
                    "source_offset": [-56.0, 0.0], "size": 24.0, "hardness": 1.0, "opacity": 1.0}),
        );
        if cloned["ok"] != json!(true) {
            // 工具名/参数以注册表为准：打印真实错误，便于按实际接口修正（不猜）。
            panic!("clone_stamp 调用失败：{cloned}");
        }
    }
    let after = render(&mut workspace);
    let changes = changed(&before, &after);
    assert!(!changes.is_empty(), "选区内应被仿制（前置条件）");
    let outside: Vec<(usize, usize)> = changes.iter().copied().filter(|(x, _)| *x < 64).collect();
    assert!(
        outside.is_empty(),
        "选区外不应被修图改动（越界 {} 个像素，例如 {:?}）",
        outside.len(),
        outside.iter().take(5).collect::<Vec<_>>()
    );
}
