//! **`tools.rs` 的单元测试**（原本是 `tools.rs` 末尾的 `#[cfg(test)] mod tests { … }`）。
//!
//! **为什么拆出来**（色彩体系规划 **Phase 2** + 外部评审报告第 15 项）。
//! 本模块是**纯搬运**：行为**零变化**（全量测试通过即验收）。
//!
//! **为什么内层 `mod tests { }` 被去掉了** ✓（第 1279 轮实测）：
//!   保留它会让层级变成 `tools::tools_tests::tests` ⇒ 于是测试里的 `use super::ALL_TOOLS`
//!   少了一层 ⇒ 报 `unresolved import`（147 个错误同源）✗
//!   ⇒ 去掉后 `super` 直接指向 `tools` ✓
//!
//! **跨模块 test 辅助**（`backend_label` / `delta_note`）放在**父模块** `tools.rs`，
//! 因为 `tools_brush.rs` / `tools_read.rs` 的 test 也要用它们。

#[cfg(test)]
use super::*;
// **★ P0 #3 的关键一问 ✗ `dtimes` 到底有没有到达引擎 ★**（第 254 轮 ✓）
//
// **∴ 为什么不能只靠 tool-* 判据 ✗**：**实测发现**判据层看不到差别**✗ ——
//   **因为服务端的分步逻辑按**距离**决定要不要调 `stroke_to`** ✓
//     ＋ **∴ 零位移时**一次都不调** ✗ **∴ 于是** `dabs_per_second`（**停笔按时间补 dab ✓）
//       在这条集成里**根本发不出声** ✓
//   ⇒ **∴ 所以**必须**直接**问：**同一串点、只改 `dtimes` ✗ 引擎产出的像素变不变** ✓ ★★★
//
// **∴ 这条测试的价值 ✗**：**它把"接线问题"与"引擎不响应"分开** ✓
//   ＋ **∴ 若它绿 ⇒ 接线对了 ✗ 剩下的是"怎么在判据层造出可观测量"** ✓
//   ＋ **∴ 若它红 ⇒ `time_ms` 光接线**不够**✗ 还要改分步逻辑（**这才是准确结论 ✓）★★★
#[test]
fn dtimes_reach_the_engine() {
    // 用资产里的 .myb（文本格式；`read_list_brushes` 就是这么读的）
    // **★ 路径必须相对 CARGO_MANIFEST_DIR ★**（第 254 轮实测踩到）：
    //   单元测试的**当前目录是 crate 根**（crates/yanshi-server）=> 直接写
    //   "assets/brushes/…" 会 panic「读不到测试用 .myb: No such file or directory」，
    //   于是 A/B 比较**根本没跑**，白白怀疑了半天断言。用 env!("CARGO_MANIFEST_DIR")
    //   拼绝对路径才是稳的。
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/brushes/");
    let text = std::fs::read_to_string(format!("{root}Clouds.myb"))
        .or_else(|_| std::fs::read_to_string(format!("{root}Blender.myb")))
        .or_else(|_| std::fs::read_to_string(format!("{root}blending.myb")))
        .expect("读不到测试用 .myb");
    let mut brush = hokusai::myb::from_str(&text).expect("解析 .myb 失败");
    // **∴ 装一个"吃速度"的设置 ✗**：**让速度输入真的影响笔迹** ✓（否则时间再准也看不出 ✓）
    brush
        .get_mut(hokusai::BrushSetting::Speed1Slowness)
        .base_value = 1.0;
    brush.get_mut(hokusai::BrushSetting::Speed1Gamma).base_value = 1.0;
    brush
        .get_mut(hokusai::BrushSetting::DabsPerSecond)
        .base_value = 200.0;

    let points: Vec<(f64, f64, f64)> = (0..12)
        .map(|i| (40.0 + i as f64 * 18.0, 60.0 + (i % 3) as f64 * 6.0, 1.0))
        .collect();
    let stamp = |dtimes: &[f64]| {
        let mut state = hokusai::BrushState::default();
        let mut surface = hokusai::tile_mem::MemSurface::new();
        let mut previous = None;
        let steps = super::stamp_stroke_from(
            &brush,
            &mut state,
            &mut surface,
            &points,
            &mut previous,
            dtimes,
            super::tools_brush::BrushSensors::default(),
            false,
        );
        // **∴ 用仓库自己的构造器 ✗ 别手填字段** ✓（**∴ 字段是 x0/y0/x1/y1/width/height ✓）
        let region = super::PaintRegion::from_bounds((0, 0, 255, 159, 256, 160));
        let (rgba, _) = super::read_surface_region(&surface, region);
        (steps, rgba)
    };
    let fast: Vec<f64> = vec![0.005; points.len()];
    let slow: Vec<f64> = vec![0.5; points.len()];
    let (fast_steps, fast_pixels) = stamp(&fast);
    let (slow_steps, slow_pixels) = stamp(&slow);
    assert_eq!(
        fast_steps, slow_steps,
        "步数不该随 dtimes 变（真正的差别应当在像素上）"
    );
    // **★ 第二件（真正的结论）★**（第 254 轮实测）
    //   ＋ **实测**：同一串点、只把 `dtimes` 从 5ms 改成 500ms => 引擎产出的像素**不同**
    //     => **`time_ms` 的接线真的到达引擎、并且真的改变了输出**。
    //   ＋ **踩过的坑（值得记）**：这条测试第一次「失败」其实是**路径**问题 ——
    //     单元测试的当前目录是 crate 根，`"assets/brushes/…"` 找不到 =>
    //     `.expect()` 先 panic，**A/B 比较根本没跑**，我却一度以为「引擎不响应时间」。
    //     修成 `CARGO_MANIFEST_DIR` 之后才拿到真结论。
    //   ＋ **配套**：`scripts/tool-time-ms.mjs` 用的是同一套组合（`Clouds.myb` +
    //     `speed1_slowness`/`speed1_gamma`/`dabs_per_second`）；用不吃时间的笔刷
    //     （例如 `100%_Opaque` + `dabs_per_second`：那个只在**停笔**时补 dab）是看不出差别的。
    assert_ne!(
        fast_pixels, slow_pixels,
        "同一串点只改 dtimes（5ms vs 500ms）却产出**完全相同**的像素 \
         => 时间没有真正影响引擎（接线断了，或这套设置不吃时间）"
    );
}

// **★ P1 #8 生理颤动（数据层，决定性）★**（第 254 轮）
//   这条判据的**鉴别力**在于"带限"：它必须与仓库既有的**白噪声**抖动区分开 ——
//   白噪声相邻点**独立**（差分会很大），带限噪声相邻点**相关**（差分很小）。
#[test]
fn tremor_is_band_limited_and_off_by_default() {
    let line: Vec<(f64, f64, f64)> = (0..40).map(|i| (i as f64 * 8.0, 100.0, 0.5)).collect();
    // 不给 amount => 原样
    assert_eq!(super::apply_tremor(&line, 0.0, 7), line);
    let jittered = super::apply_tremor(&line, 1.0, 7);
    assert_eq!(jittered.len(), line.len());
    // **带限**：相邻点的**法向位移增量**必须小（白噪声会给到像素级跳变）
    let mut max_step: f64 = 0.0;
    for pair in jittered.windows(2) {
        let d = (pair[1].1 - pair[0].1).abs();
        max_step = max_step.max(d);
    }
    assert!(
        max_step < 1.2,
        "带限噪声的相邻位移增量应当很小（白噪声会跳几像素）：最大 {max_step}"
    );
    // 但它**确实**不是直线（位移非零）
    let moved = jittered
        .iter()
        .zip(line.iter())
        .filter(|(a, b)| (a.1 - b.1).abs() > 0.05)
        .count();
    assert!(moved > 5, "颤动能动起来的点太少：{moved}");
    // 幅度有界（<= amount × 2.5px）
    for point in &jittered {
        assert!((point.1 - 100.0).abs() <= 2.5 + 1e-9, "偏移越界：{point:?}");
    }
    // 确定性：同种子逐位相同；换种子必须不同
    assert_eq!(jittered, super::apply_tremor(&line, 1.0, 7));
    assert_ne!(
        jittered,
        super::apply_tremor(&line, 1.0, 8),
        "换种子应当换波形"
    );
    // 几何端点也不漂（首末点位移仍在界内）
    assert!((jittered[0].1 - 100.0).abs() <= 2.5 + 1e-9);
}

// **★ P1 #10 罩染（数据层，决定性）★**（第 254 轮）
#[test]
fn glaze_passes_are_interleaved_and_off_by_default() {
    let line: Vec<(f64, f64, f64)> = (0..5).map(|i| (i as f64 * 10.0, 50.0, 0.5)).collect();
    // 1 遍（或不给）=> 原样
    assert_eq!(super::apply_glaze_passes(&line, 1, 3.0), line);
    assert_eq!(super::apply_glaze_passes(&line, 3, 0.0), line);
    // 3 遍 => 点数 ×3，且**交错**（前三个点是同一个原始点的三份偏移）
    let glazed = super::apply_glaze_passes(&line, 3, 4.0);
    assert_eq!(glazed.len(), line.len() * 3, "3 遍应当把点数乘 3");
    // 水平线的法向是竖向 => 同一个原始点的三份偏移应当在 **y 上**分开、x 相同
    assert!((glazed[0].0 - glazed[1].0).abs() < 1e-9);
    assert!((glazed[1].0 - glazed[2].0).abs() < 1e-9);
    assert!((glazed[0].1 - glazed[1].1).abs() > 1.0, "三遍必须真的错开");
    // 对称扇形：中间那遍回到原位
    assert!((glazed[1].1 - 50.0).abs() < 1e-9, "中间那遍应当在原位");
    // 端点不被挪出太远（保守：偏移 <= spacing × passes/2）
    for point in &glazed {
        assert!((point.1 - 50.0).abs() <= 4.0 * 2.0 + 1e-9);
    }
    // 确定性
    assert_eq!(glazed, super::apply_glaze_passes(&line, 3, 4.0));
}

// **★ P1 #8 曲率驻留（数据层，决定性）★**（第 254 轮）
//   `dwell` 是**显式 opt-in**（缺省 0.0）=> 必须做到"不给就一个点都不加"。
#[test]
fn curvature_dwell_is_off_by_default_and_denser_at_corners() {
    let straight: Vec<(f64, f64, f64)> = (0..5).map(|i| (i as f64 * 10.0, 0.0, 0.5)).collect();
    // 缺省（0.0）⇒ 一个点都不加
    assert_eq!(
        super::apply_curvature_dwell(&straight, 0.0).len(),
        straight.len(),
        "dwell=0 时不许改点列"
    );
    // 直线（转角 0）⇒ 也不加
    assert_eq!(
        super::apply_curvature_dwell(&straight, 1.0).len(),
        straight.len(),
        "直线没有曲率，不该驻留"
    );
    // 直角拐弯 ⇒ 必须比直线多（曲率驻留生效）
    let corner = vec![(0.0, 0.0, 0.5), (10.0, 0.0, 0.5), (10.0, 10.0, 0.5)];
    let dwelled = super::apply_curvature_dwell(&corner, 1.0);
    assert!(
        dwelled.len() > corner.len(),
        "拐角处应当多插点：{} vs {}",
        dwelled.len(),
        corner.len()
    );
    // 端点必须保住（几何整体不漂）
    assert_eq!(dwelled.first().copied(), Some(corner[0]));
    assert_eq!(dwelled.last().copied(), Some(corner[2]));
    // 确定性
    assert_eq!(dwelled, super::apply_curvature_dwell(&corner, 1.0));
}

// **★ P1 #11 humanize：包络的形状与不对称（数据层，决定性）★**（第 254 轮）
//   `humanize` 在服务端是**纯数据变换**（把每个点的压力乘以包络）；
//   与其去图里猜"浓淡剖面"（实测不稳：`100%_Opaque` 的宽度不吃压感、`Clouds` 的墨斑驳），
//   不如**直接断言数据**：中段保持、两端下降、且**收笔侧的过渡长于起笔侧**（12% vs 28%）。
#[test]
fn humanize_envelope_is_asymmetric() {
    // 一条 33 点的直线，压力恒为 1.0（这样包络本身一眼可读）
    let mut points: Vec<(f64, f64, f64)> = (0..33).map(|i| (i as f64 * 10.0, 100.0, 1.0)).collect();
    let base = points.clone();
    super::apply_humanize_envelope(&mut points);
    // 中段（40%~60%）应当**保持不变**（包络为 1）
    for (i, point) in points.iter().enumerate() {
        let t = i as f64 / 32.0;
        if (0.4..=0.6).contains(&t) {
            assert!(
                (point.2 - 1.0).abs() < 1e-9,
                "中段压力不该被改：t={t} 得到 {}",
                point.2
            );
        }
    }
    // 两端必须下降
    assert!(
        points[0].2 < 0.3,
        "起笔第一点压力应明显下降，实测 {}",
        points[0].2
    );
    assert!(
        points[32].2 < 0.3,
        "收笔最后一点压力应明显下降，实测 {}",
        points[32].2
    );
    // 几何绝不能被改（humanize 只改压力）
    for (after, before) in points.iter().zip(base.iter()) {
        assert_eq!(after.0, before.0);
        assert_eq!(after.1, before.1);
    }
    // **不对称**：统计"压力 < 0.5 的点数"，收笔侧必须**多于**起笔侧
    let head_low = (0..=16).filter(|i| points[*i].2 < 0.5).count();
    let tail_low = (16..33).filter(|i| points[*i].2 < 0.5).count();
    assert!(
        tail_low > head_low,
        "收笔侧的过渡应比起笔侧长（12% vs 28%）：起笔侧 {head_low} 个低压力点 vs 收笔侧 {tail_low}"
    );
}

// 裸名 `ALL_TOOLS` 在本模块里解析不到 ⇒ 必须显式引进来（第 52 轮实测）。
use super::ALL_TOOLS;

/// **`brush_preview` 的参数面必须覆盖它实现里真正会读的名字**。
///
/// 实测报错原话：`brush_preview 不接受参数 hardness（拼写错误？）；可用参数：brush, si…`
/// ⇒ 实现里读 `opacity`/`hardness`，而参数面里没有 ⇒ 调用方只能猜。
/// 天生能红：从 `params` 里删掉任何一个名字，这个测试立刻失败。
#[test]
fn brush_preview_surface_covers_the_settings_its_implementation_reads() {
    let spec = ALL_TOOLS
        .iter()
        .find(|tool| tool.name == "brush_preview")
        .expect("笔刷预览这个工具应当在清单里");
    let declared: Vec<&str> = spec.params.iter().map(|parameter| parameter.name).collect();
    for needed in [
        "brush", "size", "color", "points", "color_to", "smooth", "opacity", "hardness",
    ] {
        assert!(
            declared.contains(&needed),
            "brush_preview 的实现会读 {needed}，而参数面里没有 ⇒ 调用方会收到「不接受参数 {needed}」并且无从知道它可以覆盖它；declared = {declared:?}"
        );
    }
}

/// **每个注册工具都必须有示例**（第 1135 轮：(B)① 的收尾）。
///
/// 既有那条只检查"示例里的参数名没有漂移"⇒ 它对**没有示例的工具**是**永久绿**的假判据 ✗。
/// 这条补上：断言 `TOOL_EXAMPLES` 覆盖**全部**注册工具 ⇒ 以后新加工具而忘了写示例，它会立刻红 ✓。
#[test]
fn every_registered_tool_has_a_documented_example() {
    let missing: Vec<&str> = super::ALL_TOOLS
        .iter()
        .map(|tool| tool.name)
        .filter(|name| {
            !super::TOOL_EXAMPLES
                .iter()
                .any(|(documented, _)| documented == name)
        })
        .collect();
    assert!(
        missing.is_empty(),
        "这些工具没有可照抄的示例 ⇒ 补进 TOOL_EXAMPLES：{missing:?}"
    );
}

/// **示例里的参数名必须都在该工具的参数面里**（两边不许漂移）。
///
/// 天生能红：示例里写错一个字（或参数面里删掉一个名字），这个测试立刻失败 ——
/// 这正是"可复制调用示例"存在的意义：**能直接抄走**，而不是抄走之后被拒。
#[test]
fn every_documented_example_only_uses_declared_parameters() {
    for (name, example) in super::TOOL_EXAMPLES {
        let spec = super::ALL_TOOLS
            .iter()
            .find(|tool| tool.name == *name)
            .unwrap_or_else(|| panic!("示例引用了不存在的工具：{name}"));
        let declared: Vec<&str> = spec.params.iter().map(|parameter| parameter.name).collect();
        let parsed: serde_json::Value = serde_json::from_str(example)
            .unwrap_or_else(|error| panic!("{name} 的示例不是合法 JSON：{error}"));
        // **多步示例**（第 258 轮加的机制 ⇒ 这里必须一起学会 ✓）：数组 ⇒
        // 逐条按"**那一步对应工具**的参数面"校验 ✓，并要求**最后一步是该工具本身** ✓。
        // 只改一半的教训已记档（第 259 轮 ✓）：机制与守它的测试必须**一起改** ✗。
        if let Some(steps) = parsed.as_array() {
            let last = steps.last().expect("多步示例不能是空数组");
            assert_eq!(
                last.get("tool").and_then(serde_json::Value::as_str),
                Some(*name),
                "{name} 的多步示例**最后一步必须是它自己**"
            );
            for step in steps {
                let step_tool = step
                    .get("tool")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_else(|| panic!("{name} 的多步示例缺少 tool 字段"));
                let step_spec = super::ALL_TOOLS
                    .iter()
                    .find(|tool| tool.name == step_tool)
                    .unwrap_or_else(|| panic!("{name} 的多步示例引用了不存在的工具：{step_tool}"));
                let step_declared: Vec<&str> = step_spec
                    .params
                    .iter()
                    .map(|parameter| parameter.name)
                    .collect();
                let arguments = step
                    .get("arguments")
                    .expect("多步示例的每一步都要有 arguments");
                for key in arguments
                    .as_object()
                    .expect("arguments 应当是 JSON 对象")
                    .keys()
                {
                    assert!(
                        step_declared.contains(&key.as_str()),
                        "{step_tool} 的这一步用了 `{key}`，但它的参数面里没有它 ⇒ 抄走会被拒；declared = {step_declared:?}"
                    );
                }
            }
            continue;
        }
        for key in parsed.as_object().expect("示例应当是一个 JSON 对象").keys() {
            assert!(
                declared.contains(&key.as_str()),
                "{name} 的示例用了 `{key}`，但该工具的参数面里没有它 ⇒ 抄走会被拒；declared = {declared:?}"
            );
        }
    }
}

// ================= ②③④①⑤⑥ 六新功能单元测试（2026-10-19）=================
// 断言设计原则：每条都能变红（见各测试末尾的"变异说明"注释）。
// （父模块函数直接可用，无需 use super::，与既有测试一致。）

fn json_shape(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap()
}

// ---- ② 带孔形状 ----
#[test]
fn smart_shape_hole_center_excluded() {
    // rect 100×100，中央 20×20 方孔：孔心必须被排除，外层点保留。
    let v = json_shape(
        r#"{"type":"rect","x":0,"y":0,"w":100,"h":100,
            "holes":[[[40,40],[60,40],[60,60],[40,60]]]}"#,
    );
    let p = parse_smart_shape(&v, "test").unwrap();
    assert!(p.contains(10.0, 10.0), "外层点应在内");
    assert!(!p.contains(50.0, 50.0), "孔洞中心必须被排除");
    assert!(!p.contains(200.0, 200.0), "外层外应在外");
    // 变异说明：把 contains 改成只判 outer → 第二条变红。
}

#[test]
fn smart_shape_hole_object_forms() {
    // 孔洞支持完整子形状对象（ellipse 用 x/y/w/h 写法）。
    let v = json_shape(
        r#"{"type":"polygon","points":[[0,0],[100,0],[100,100],[0,100]],
            "holes":[{"type":"ellipse","x":40,"y":40,"w":20,"h":20}]}"#,
    );
    let p = parse_smart_shape(&v, "test").unwrap();
    assert_eq!(p.holes.len(), 1);
    assert!(!p.contains(50.0, 50.0), "椭圆孔心应被排除");
    assert!(p.contains(10.0, 10.0));
    // 变异说明：ellipse 解析错成 rect → 孔心 (50,50) 仍在 40..60 方框内，
    // 本测试不红；但 smart_shape_hole_center_excluded 的 rect 孔不受影响。
    // 真正的 ellipse 形状断言由下一条覆盖。
}

#[test]
fn smart_shape_ellipse_hole_is_round_not_square() {
    // 椭圆孔是圆的不是方的：角点 (41,41) 距圆心 (50,50) 约 12.7 > r=10，应在内。
    let v = json_shape(
        r#"{"type":"rect","x":0,"y":0,"w":100,"h":100,
            "holes":[{"type":"ellipse","x":40,"y":40,"w":20,"h":20}]}"#,
    );
    let p = parse_smart_shape(&v, "test").unwrap();
    assert!(!p.contains(50.0, 50.0), "圆心在孔内");
    assert!(p.contains(41.0, 41.0), "圆角外应在内（若误判为方孔则变红）");
    // 变异说明：把 Ellipse::contains 改成矩形判定 → 第二条变红。
}

#[test]
fn smart_shape_polygon_with_holes_alias() {
    let v = json_shape(
        r#"{"type":"polygon_with_holes","points":[[0,0],[100,0],[100,100],[0,100]],
            "holes":[[[40,40],[60,40],[60,60],[40,60]]]}"#,
    );
    let p = parse_smart_shape(&v, "test").unwrap();
    assert!(matches!(p.outer, PrimShape::Polygon { .. }));
    assert!(!p.contains(50.0, 50.0));
    // 变异说明：去掉 "polygon_with_holes" 分支 → parse 报错，测试变红。
}

#[test]
fn smart_shape_area_subtracts_holes() {
    let v = json_shape(
        r#"{"type":"rect","x":0,"y":0,"w":100,"h":100,
            "holes":[[[40,40],[60,40],[60,60],[40,60]]]}"#,
    );
    let p = parse_smart_shape(&v, "test").unwrap();
    let area = p.area();
    assert!(
        (area - 9600.0).abs() < 1.0,
        "面积应为 10000-400=9600，得到 {area}"
    );
    // 变异说明：area 不减孔洞 → 得到 10000，变红。
}

#[test]
fn smart_shape_path_d_form_is_a_real_path() {
    // type=path + d：三角形 (20,100) (100,20) (180,100)。contains/bbox/area
    // 都按路径形状本身，不是控制点包围盒、更不是缺省矩形。
    let v = json_shape(r#"{"type":"path","d":"M20 100 L100 20 L180 100 Z"}"#);
    let p = parse_smart_shape(&v, "test").unwrap();
    assert!(matches!(p.outer, PrimShape::Path { .. }));
    assert_eq!(p.outer.kind_str(), "path");
    assert_eq!(p.bbox, (20.0, 20.0, 180.0, 100.0));
    assert!(p.contains(100.0, 73.0), "三角形质心应在内");
    assert!(!p.contains(30.0, 30.0), "左上在三角形外（斜边 x+y=120）");
    // 面积 = 底 160 × 高 80 / 2 = 6400（覆盖率口径，容差按边界长度给）。
    let area = p.area();
    assert!(
        (area - 6400.0).abs() < 48.0,
        "三角形面积应为 6400，得到 {area}"
    );
    // 变异说明：path 臂回到「返回空/矩形」⇒ contains 与 bbox 断言变红。
}

#[test]
fn smart_shape_bare_d_without_type_is_path_not_default_rect() {
    // 事故回归：{"d": …} 没有 type ⇒ 按 path 解析，绝不吞成 (0,0,100,100)。
    let v = json_shape(r#"{"d":"M20 100 L100 20 L180 100 Z"}"#);
    let p = parse_smart_shape(&v, "test").unwrap();
    assert!(matches!(p.outer, PrimShape::Path { .. }));
    assert_ne!(p.bbox, (0.0, 0.0, 100.0, 100.0), "bbox 不该是缺省矩形");
    assert!(!p.contains(10.0, 10.0), "缺省矩形内、三角形外 ⇒ 必须在外");
    assert!(p.contains(100.0, 73.0), "三角形质心应在内");
    // 变异说明：去掉 d/subpaths 特征键推断 ⇒ 落回 Rect(0,0,100,100)，三条全红。
}

#[test]
fn smart_shape_no_type_with_only_rect_keys_stays_rect() {
    // 存量兼容：无 type 且只有 rect 键 ⇒ 维持缺省 rect。
    let v = json_shape(r#"{"x":5,"y":5,"w":10,"h":10}"#);
    let p = parse_smart_shape(&v, "test").unwrap();
    assert!(matches!(p.outer, PrimShape::Rect { .. }));
    assert!(p.contains(10.0, 10.0));
    assert!(!p.contains(50.0, 50.0));
}

#[test]
fn smart_shape_curve_d_area_is_path_area_not_default_rect() {
    // 外部复现样本（gradetwo 判据同款 d）：贝塞尔闭合路径。
    // 真实几何（解析值）：y 极值在 t≈0.211/0.789 处 ⇒ bbox ≈
    // (150, 105.6)-(300, 134.5)；边界与闭合弦相交成双叶，填充面积是
    // 两叶之和 ≈1400（带符号鞋带只会给出两叶之差 ≈75）；被吞成缺省
    // 矩形时面积是 10000、bbox 是 (0,0,100,100)——三头都对不上。
    let v = json_shape(r#"{"d":"M 150,120 C 200,70 260,170 300,120 Z"}"#);
    let p = parse_smart_shape(&v, "fit_background_field").unwrap();
    let (x0, y0, x1, y1) = p.bbox;
    assert!(
        (x0 - 150.0).abs() < 0.5 && (x1 - 300.0).abs() < 0.5,
        "bbox x 应为 150..300，实测 {p:?}"
    );
    assert!(
        (104.0..=107.0).contains(&y0),
        "bbox y0 应 ≈105.6，实测 {y0}"
    );
    assert!(
        (133.0..=136.0).contains(&y1),
        "bbox y1 应 ≈134.5，实测 {y1}"
    );
    let area = p.area();
    assert!(
        (1250.0..=1600.0).contains(&area),
        "曲线路径面积应 ≈1400（双叶之和，解析积分 ≈1.42e3），实测 {area}（缺省矩形会给 10000）"
    );
    // 变异说明：d 被吞回缺省 rect ⇒ bbox=(0,0,100,100)、area=10000，四条全红。
}

#[test]
fn smart_shape_path_subpaths_fill_rules() {
    // subpaths 给法 + fill_rule 语义：外环 0..100、内环 40..60（同为逆时针
    // 书写时针序不论）：evenodd ⇒ 内环是孔；nonzero 同向嵌套 ⇒ 孔被填回。
    let subs = r#"[
        {"start":[0,0],"closed":true,"segments":[{"op":"L","to":[100,0]},{"op":"L","to":[100,100]},{"op":"L","to":[0,100]}]},
        {"start":[40,40],"closed":true,"segments":[{"op":"L","to":[60,40]},{"op":"L","to":[60,60]},{"op":"L","to":[40,60]}]}
    ]"#;
    let even = json_shape(&format!(
        r#"{{"type":"path","fill_rule":"evenodd","subpaths":{subs}}}"#
    ));
    let p = parse_smart_shape(&even, "test").unwrap();
    assert!(!p.contains(50.0, 50.0), "evenodd：内环中心是孔");
    assert!(p.contains(10.0, 10.0), "evenodd：外环内、内环外应在内");
    let area = p.area();
    assert!(
        (area - 9600.0).abs() < 64.0,
        "evenodd 面积应为 10000-400=9600，得到 {area}"
    );
    let non = json_shape(&format!(
        r#"{{"type":"path","fill_rule":"nonzero","subpaths":{subs}}}"#
    ));
    let p = parse_smart_shape(&non, "test").unwrap();
    assert!(p.contains(50.0, 50.0), "nonzero 同向嵌套：内环中心被填回");
    // 变异说明：contains 不分 fill_rule（恒 evenodd）⇒ nonzero 断言变红；
    // area 不走 fill_rule 覆盖率 ⇒ 9600 断言变红。
}

#[test]
fn smart_shape_path_errors_point_the_way() {
    // path 什么都不给 ⇒ 明确报错并指路（d / subpaths 两种给法都写）。
    let v = json_shape(r#"{"type":"path"}"#);
    let err = parse_smart_shape(&v, "smart_fill").unwrap_err();
    let text = format!("{err:?}");
    assert!(text.contains("必须给"), "实测：{text}");
    assert!(text.contains("subpaths"), "实测：{text}");
    // d 与 subpaths 同给 ⇒ 二选一报错。
    let v = json_shape(
        r#"{"type":"path","d":"M0 0 L1 1 Z","subpaths":[{"start":[0,0],"segments":[{"op":"L","to":[1,1]}]}]}"#,
    );
    let err = parse_smart_shape(&v, "smart_fill").unwrap_err();
    assert!(format!("{err:?}").contains("只能给一个"), "实测：{err:?}");
    // d 语法坏 ⇒ 报错带解析原因（字符位置）。
    let v = json_shape(r#"{"type":"path","d":"M0 0 X9 9"}"#);
    let err = parse_smart_shape(&v, "smart_fill").unwrap_err();
    assert!(format!("{err:?}").contains("d 非法"), "实测：{err:?}");
    // 未知 type ⇒ 支持列表必须含 path（旧文案把二期主打功能写成不支持）。
    let v = json_shape(r#"{"type":"blob","x":0,"y":0}"#);
    let err = parse_smart_shape(&v, "smart_fill").unwrap_err();
    let text = format!("{err:?}");
    assert!(text.contains("不支持的 shape type"), "实测：{text}");
    assert!(text.contains("path"), "支持列表应含 path，实测：{text}");
    // 变异说明：错误文案退回旧版（无 path）⇒ 最后一条变红。
}

#[test]
fn smart_shape_no_holes_backward_compat() {
    // 无 holes 时行为与旧解析一致。
    let v = json_shape(r#"{"type":"rect","x":10,"y":20,"w":30,"h":40}"#);
    let p = parse_smart_shape(&v, "test").unwrap();
    assert!(p.holes.is_empty());
    assert_eq!(p.bbox, (10.0, 20.0, 40.0, 60.0));
    assert!(p.contains(25.0, 30.0));
    assert!(!p.contains(0.0, 0.0));
}

// ---- smart_glaze 真椭圆（A3：rx≠ry 静默无效果修复）----

/// 端到端跑一次 `smart_glaze`（内存工作区 + 真实笔刷落笔），返回其 JSON
/// 结果；工具报错时把错误文本带出来（基线下真椭圆正是走这条路）。
fn glaze_e2e(
    doc_id: &str,
    shape: serde_json::Value,
) -> std::result::Result<serde_json::Value, String> {
    // 内存工作区缺省 assets_dir=None ⇒ 笔刷一只都找不到（落笔全失败）；
    // 显式挂上仓库 assets（与 dtimes 测试拼 .myb 路径同口径）。
    let mut workspace =
        Workspace::in_memory(crate::document::DocumentSettings::default()).with_assets_dir(Some(
            std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets")),
        ));
    workspace
        .create_document(
            crate::document::NewDocument::new(doc_id, 300, 300),
            "test:1",
            "s",
        )
        .map_err(|e| format!("{e:?}"))?;
    let mut ctx = ToolContext::new(&mut workspace, doc_id, "test:1", "s");
    write_create_layer(&mut ctx, &json!({"layer_id": "p", "name": "p"}))
        .map_err(|e| format!("{e:?}"))?;
    write_smart_glaze(
        &mut ctx,
        &json!({
            "layer_id": "p",
            "shape": shape,
            "color": [0.86, 0.24, 0.16, 1.0],
            "layers": 3,
            "size": 15.0,
        }),
    )
    .map_err(|e| format!("{e:?}"))
}

#[test]
fn smart_glaze_true_ellipse_produces_strokes() {
    // 真椭圆 rx=100、ry=40：基线下 kind_str 给 "ellipse"，而 glaze 的内缩与
    // 点内判定只认 "circle"，每层落进缺省无效臂被跳过 ⇒ ok_count=0 报错、
    // 作用量（成功笔触数）恒为 0。修后必须显著 >0 且每层都有排线。
    let r = glaze_e2e(
        "glaze_ellipse_doc",
        json!({"type": "ellipse", "cx": 150.0, "cy": 150.0, "rx": 100.0, "ry": 40.0}),
    )
    .expect("真椭圆 glaze 不应报错");
    let strokes = r["strokes"].as_u64().unwrap_or(0);
    assert!(
        strokes >= 20,
        "真椭圆作用量（成功笔触数）应显著 >0，实测 {strokes}"
    );
    let lines: Vec<u64> = r["layer_lines"]
        .as_array()
        .map(|arr| arr.iter().map(|v| v.as_u64().unwrap_or(0)).collect())
        .unwrap_or_default();
    assert_eq!(lines.len(), 3, "应有 3 层记录，实测 {lines:?}");
    assert!(
        lines.iter().all(|&n| n > 0),
        "每一层都应有排线（基线下全为 0），实测 {lines:?}"
    );
    assert_eq!(r["failed"], json!(0), "不应有失败笔触：{r}");
    // 变异说明：把 ellipse 臂删掉 ⇒ 每层又落进无效臂 ⇒ 本测试回到基线红。
}

#[test]
fn smart_glaze_circle_unchanged_identity() {
    // 回归锁：rx==ry（圆）时与基线行为逐项一致 —— 同一几何分别用
    // type=circle 与 type=ellipse(rx==ry) 两种写法给入，结果必须逐项相同；
    // 再与基线（29e248bd）实测的作用量常数对照锁死。
    let circle = glaze_e2e(
        "glaze_circle_doc",
        json!({"type": "circle", "cx": 150.0, "cy": 150.0, "r": 60.0}),
    )
    .expect("圆 glaze 应成功");
    let as_ellipse = glaze_e2e(
        "glaze_circle_as_ellipse_doc",
        json!({"type": "ellipse", "cx": 150.0, "cy": 150.0, "rx": 60.0, "ry": 60.0}),
    )
    .expect("rx==ry 的 ellipse 应与圆一致");
    assert_eq!(circle["layer_lines"], as_ellipse["layer_lines"]);
    assert_eq!(circle["strokes"], as_ellipse["strokes"]);
    assert_eq!(circle["layer_dab_alphas"], as_ellipse["layer_dab_alphas"]);
    // 基线对照常数（29e248bd 未改代码实测，见 A3 包 README）。
    assert_eq!(circle["strokes"], json!(37), "圆作用量与基线对照");
    assert_eq!(
        circle["layer_lines"],
        json!([13, 13, 11]),
        "圆逐层排线与基线对照"
    );
}

/// 某点被多少层内缩椭圆覆盖 = 罩染在该点的「作用量」（层数越多罩得越浓）。
/// 与生产路径同口径：逐层 inset = i*size*0.18，半轴走
/// `glaze_ellipse_layer_radii`，点内判定走 `glaze_ellipse_contains`。
fn glaze_cover_count(rx: f64, ry: f64, layers: usize, size: f64, px: f64, py: f64) -> usize {
    (0..layers)
        .filter(|&i| {
            let inset_px = i as f64 * size * 0.18;
            match glaze_ellipse_layer_radii(rx, ry, inset_px) {
                Some((lrx, lry)) => glaze_ellipse_contains(0.0, 0.0, lrx, lry, px, py),
                None => false,
            }
        })
        .count()
}

#[test]
fn smart_glaze_ellipse_q_profile() {
    // 归一化距离：长轴上 90/100 与短轴上 36/40 是同一个 q=0.9。
    let q_long = glaze_ellipse_q(0.0, 0.0, 100.0, 40.0, 90.0, 0.0);
    let q_short = glaze_ellipse_q(0.0, 0.0, 100.0, 40.0, 0.0, 36.0);
    assert!(
        (q_long - 0.9).abs() < 1e-12,
        "长轴 q 应为 0.9，实测 {q_long}"
    );
    assert!(
        (q_long - q_short).abs() < 1e-12,
        "等 q 点的 q 必须相等：{q_long} vs {q_short}"
    );
    // 同一欧氏距离 90：短轴方向 q=2.25，远大于长轴方向的 0.9。
    let q_short_far = glaze_ellipse_q(0.0, 0.0, 100.0, 40.0, 0.0, 90.0);
    assert!(
        (q_short_far - 2.25).abs() < 1e-12,
        "短轴 90px 处 q 应为 2.25，实测 {q_short_far}"
    );
    // 变异说明：q 误用欧氏距离（不除半轴）⇒ q_short 变 36、q_short_far 与
    // q_long 相等 ⇒ 上面三条全红。
}

#[test]
fn smart_glaze_ellipse_layer_radii_rules() {
    // 圆：与旧实现同一算术（r − inset），逐位相等，不是近似。
    assert_eq!(
        glaze_ellipse_layer_radii(60.0, 60.0, 2.7),
        Some((60.0 - 2.7, 60.0 - 2.7)),
        "圆的内缩半轴必须与基线算术逐位一致"
    );
    // 第 0 层（inset=0）：真椭圆半轴原样返回，逐位相等。
    assert_eq!(
        glaze_ellipse_layer_radii(100.0, 40.0, 0.0),
        Some((100.0, 40.0))
    );
    // 真椭圆内缩后仍是相似椭圆（同比例缩放）：半轴比保持 2.5。
    let (lrx, lry) = glaze_ellipse_layer_radii(100.0, 40.0, 2.7).expect("第 1 层应存在");
    assert!(
        (lrx / lry - 2.5).abs() < 1e-12,
        "内缩后半轴比应保持 2.5，实测 {} / {}",
        lrx,
        lry
    );
    // 短轴方向恰好退 inset_px：40 − lry = 2.7（归一化内缩的定义）。
    assert!(
        (40.0 - lry - 2.7).abs() < 1e-9,
        "短轴应恰退 2.7px，实测退 {}",
        40.0 - lry
    );
    // inset 吃掉短轴（>=ry）⇒ 该层不存在。
    assert_eq!(glaze_ellipse_layer_radii(100.0, 40.0, 40.0), None);
    assert_eq!(glaze_ellipse_layer_radii(100.0, 40.0, 41.0), None);
    // 变异说明：两轴同减 inset（圆口径套椭圆）⇒ 半轴比变 97.3/37.3≈2.609
    // 且短轴退量仍是 2.7 但等 q 覆盖（下一条）先红；本条半轴比断言亦红。
}

#[test]
fn smart_glaze_ellipse_cover_count_decays_by_q() {
    // rx=100、ry=40、3 层、size=15（逐层 inset 0 / 2.7 / 5.4）。
    let (rx, ry, layers, size) = (100.0, 40.0, 3usize, 15.0f64);
    // 中心被全部 3 层覆盖。
    assert_eq!(glaze_cover_count(rx, ry, layers, size, 0.0, 0.0), 3);
    // 等 q（0.9）点：长轴 (90,0) 与短轴 (0,36) 覆盖层数必须相等（同为 2：
    // 第 2 层 k=0.865 < 0.9）。这是「衰减只依赖椭圆距离场」的核心恒等式。
    let c_long = glaze_cover_count(rx, ry, layers, size, 90.0, 0.0);
    let c_short = glaze_cover_count(rx, ry, layers, size, 0.0, 36.0);
    assert_eq!(c_long, 2, "长轴 q=0.9 处应覆盖 2 层，实测 {c_long}");
    assert_eq!(
        c_long, c_short,
        "等 q 点覆盖层数必须相等：长轴 {c_long} vs 短轴 {c_short}"
    );
    // 等 q（0.5）点同理：(50,0) 与 (0,20) 都被 3 层覆盖。
    assert_eq!(glaze_cover_count(rx, ry, layers, size, 50.0, 0.0), 3);
    assert_eq!(glaze_cover_count(rx, ry, layers, size, 0.0, 20.0), 3);
    // 同一欧氏距离 90：长轴方向 2 层、短轴方向 0 层 ⇒ 长短轴衰减剖面不同。
    let c_short_far = glaze_cover_count(rx, ry, layers, size, 0.0, 90.0);
    assert_eq!(c_short_far, 0, "短轴 90px 处应在椭圆外，实测 {c_short_far}");
    assert!(
        c_long > c_short_far,
        "同欧氏距离下长轴作用量应大于短轴：{c_long} vs {c_short_far}"
    );
    // 沿长轴单调衰减：0 → 50 → 90 → 99 覆盖层数不增。
    let prof: Vec<usize> = [0.0, 50.0, 90.0, 99.0]
        .iter()
        .map(|&x| glaze_cover_count(rx, ry, layers, size, x, 0.0))
        .collect();
    assert!(
        prof.windows(2).all(|w| w[0] >= w[1]),
        "长轴剖面应单调不增，实测 {prof:?}"
    );
    assert_eq!(prof, vec![3, 3, 2, 1], "长轴剖面数值，实测 {prof:?}");
    // 变异说明：内缩改回两轴同减 inset ⇒ (0,36) 第 1 层半轴 (97.3,37.3)
    // 时 36/37.3<1 仍在、第 2 层 (94.6,34.6) 时 36/34.6>1 出局 ⇒ 也是 2，
    // 但 (90,0) 第 2 层 90/94.6<1 ⇒ 3 层 ⇒ 等 q 恒等式（2==2）变红。
}

// ---- ③ 方向场 ----
#[test]
fn direction_field_guides_angle_wrap() {
    // 355° 与 5° 的插值应 ≈0°，绝不能是 180°（角度数值线性插值的经典错）。
    let v = json_shape(r#"[[0,0,355],[100,0,5]]"#);
    let f = DirectionField::from_guides(&v, (0.0, 0.0, 100.0, 100.0)).unwrap();
    let a = f.angle_at(50.0, 0.0);
    // 归一化到 [-180,180] 再断言接近 0。
    let norm = ((a + 180.0).rem_euclid(360.0)) - 180.0;
    assert!(
        norm.abs() < 10.0,
        "355°/5° 中点应≈0°，得到 {a}（若用角度线性插值会得 180°）"
    );
    // 变异说明：把单位向量插值改成角度直接平均 → 得 180°，变红。
}

#[test]
fn direction_field_guides_single_point() {
    let v = json_shape(r#"[[30,40,90]]"#);
    let f = DirectionField::from_guides(&v, (0.0, 0.0, 100.0, 100.0)).unwrap();
    let a = f.angle_at(10.0, 10.0);
    assert!((a - 90.0).abs() < 5.0, "单引导点附近应≈90°，得到 {a}");
}

#[test]
fn direction_field_constant() {
    let f = DirectionField::constant(-30.0);
    assert_eq!(f.angle_at(0.0, 0.0), -30.0);
    assert_eq!(f.angle_at(999.0, 999.0), -30.0);
}

#[test]
fn direction_field_from_guides_empty_is_none() {
    let v = json_shape(r#"[]"#);
    assert!(DirectionField::from_guides(&v, (0.0, 0.0, 100.0, 100.0)).is_none());
    let v2 = json_shape(r#"[[0,0]]"#); // 缺角度
    assert!(DirectionField::from_guides(&v2, (0.0, 0.0, 100.0, 100.0)).is_none());
}

// ---- ④ fold ----
#[test]
fn fold_profile_values() {
    let p = get_texture_profile("fold");
    assert!((p.pair_offset_mult - 0.85).abs() < 1e-9);
    assert!((p.dark_l_shift - (-0.16)).abs() < 1e-9);
    assert!((p.light_l_shift - 0.13).abs() < 1e-9);
    assert!((p.len_mult - 7.5).abs() < 1e-9);
    assert!((p.scatter - 0.22).abs() < 1e-9);
    // 其余材质的偶极子字段必须为 0。
    for t in ["canvas", "fabric", "skin", "stone", "wood"] {
        let q = get_texture_profile(t);
        assert_eq!(q.pair_offset_mult, 0.0, "{t} 不应有偶极子");
    }
    // 变异说明：改错任一标定值 → 对应断言变红。
}

#[test]
fn fold_dipole_colors_light_above_dark() {
    let p = get_texture_profile("fold");
    let base = [0.5, 0.4, 0.3];
    let (_, _, base_l) = rgb_to_hsl(base[0], base[1], base[2]);
    let (dark, light) = fold_dipole_colors(base, p, false);
    let (_, _, ld) = rgb_to_hsl(dark[0], dark[1], dark[2]);
    let (_, _, ll) = rgb_to_hsl(light[0], light[1], light[2]);
    assert!(
        (ld - (base_l - 0.16)).abs() < 0.02,
        "暗笔 L 应为底色-0.16，底色 L={base_l:.2}，得到 {ld:.2}"
    );
    assert!(
        (ll - (base_l + 0.13)).abs() < 0.02,
        "亮笔 L 应为底色+0.13，底色 L={base_l:.2}，得到 {ll:.2}"
    );
    assert!(
        (light[3] - p.base_alpha * 0.85).abs() < 1e-9,
        "亮笔 Alpha 应为 base_alpha×0.85"
    );
    // 变异说明：把 dark/light 配色写反 → 第一条变红。
}

#[test]
fn fold_emits_paired_strokes() {
    // fold 每次迭代发射暗+亮两笔：总数为偶数，且亮笔比暗笔宽 1.35×。
    let p = get_texture_profile("fold");
    let mut rng = TextureXorShift(12345);
    let inside = |_x: f64, _y: f64| true;
    let eroded = |_x: f64, _y: f64, _e: f64| true;
    let strokes = plan_texture_strokes(
        &mut rng,
        "fold",
        p,
        0.0,
        0.0,
        400.0,
        400.0,
        160000.0,
        8.0,
        20,
        [0.5, 0.4, 0.3],
        &inside,
        &eroded,
        None,
    );
    assert!(!strokes.is_empty(), "fold 应产出笔触");
    assert_eq!(
        strokes.len() % 2,
        0,
        "fold 应成对发射，得到 {} 笔",
        strokes.len()
    );
    // 相邻两笔为一组：第二笔（亮）比第一笔（暗）宽 1.35×。
    let ratio = strokes[1].size / strokes[0].size;
    assert!(
        (ratio - 1.35).abs() < 1e-9,
        "亮笔应为暗笔 1.35× 宽，得到 {ratio}"
    );
    // 变异说明：去掉成对发射（只推暗笔）→ 总数为奇数或 ratio 断言变红。
}

// ---- ① smart_sample ----
#[test]
fn bilinear_field_samples_center_correctly() {
    // **★ 4×4 像素、S=4** ✓（第 1443 轮修：原来写 2×2 + `s=2` ✗）
    //   **⇒ 为什么必须改** ✗：`build()` 里 `s = s.clamp(4, 24)` ⇒ 传 2 实际得 **4** ⇒
    //     而 2×2 的图像只能填到 4×4 场里**隔行隔列**的格点 ⇒ 中间那些格用**全局均值**填 ⇒
    //     场是**锯齿状**的 ✗ ⇒ Catmull-Rom 有**负瓣**会**放大**这种交替 ⇒ 中心算出近黑（0.1838）✗
    //     ⇒ 而旧断言（0.2..0.9）在 bilinear 下过关只是因为矩形窗把它**糊平**了 ✓
    //   **⇒ ⇒ 所以**改成 4×4 图像 + `s=4`** ⇒ 每个格点都有真实数据 ⇒ 无混叠 ⇒
    //     bicubic 与 bilinear 的**中心都该是中灰** ✓（这才是这条测试本来想测的东西 ✓）
    let px: Vec<u8> = vec![
        0, 0, 0, 255, 255, 255, 255, 255, 0, 0, 0, 255, 255, 255, 255,
        255, // y=0: 黑 白 黑 白
        255, 255, 255, 255, 0, 0, 0, 255, 255, 255, 255, 255, 0, 0, 0,
        255, // y=1: 白 黑 白 黑
        0, 0, 0, 255, 255, 255, 255, 255, 0, 0, 0, 255, 255, 255, 255, 255, // y=2
        255, 255, 255, 255, 0, 0, 0, 255, 255, 255, 255, 255, 0, 0, 0, 255, // y=3
    ];
    let f = BilinearOklabField::build(&px, 4, 4, 4, (0.0, 0.0, 4.0, 4.0));
    let (rgb, _, _) = f.sample_at(2.0, 2.0); // 区域是 0..4 ⇒ 中心是 (2,2) ✓
                                             // 应为灰色（r≈g≈b），明度在黑白之间。
    let mx = rgb.iter().cloned().fold(0f32, f32::max);
    let mn = rgb.iter().cloned().fold(1f32, f32::min);
    let spread = mx - mn;
    assert!(spread < 0.05, "中心应为灰色（r≈g≈b），得到 {rgb:?}");
    assert!(
        rgb[0] > 0.2 && rgb[0] < 0.9,
        "中心明度应在黑白之间，得到 {rgb:?}（双线性插值错则变红）"
    );
    // 角点采样应接近原色。
    let (corner, _, _) = f.sample_at(0.2, 0.2);
    assert!(corner[0] < 0.2, "左上角应接近黑，得到 {corner:?}");
    // 变异说明：把双线性改成最近邻 → 中心取到某格纯色（spread 仍小但值偏），
    // 或角点采样错位 → 变红。
}

/// **格点边界处**不许有折痕**（第 1443 轮；外部 `E1408` 的根治判据）
///
/// **⇒ 为什么** ✗：双线性是**分段线性** ⇒ 格点边界处**一阶导跳变** ⇒ 画出来是**折痕/竖纹** ✗
/// **⇒ ⇒ 怎么量**（**第一版是安慰剂，这是第二版** ✗）：
///   第一版量的是**割线斜率之差** ⇒ 而它被**二次曲率**主导（`Δslope ≈ f'' · eps`）
///   ⇒ **bilinear 与 bicubic 都能过** ⇒ 变异不变红 ⇒ **换掉** ✓
/// **⇒ ⇒ ⇒ 正确量法**：**二阶差分**（`Δ²`）在**结点**与**格内**的**比值**
///   ⇒ 分段线性在**格内** `Δ² ≈ 0`、在**结点** `Δ²` 突跳 ⇒ **比值很大** ✗
///   ⇒ C¹ 插值两处都小 ⇒ **比值 ≈ 1** ✓
#[test]
fn field_has_no_crease_at_cell_boundary() {
    // 平滑**二次**灰阶 ⇒ 线性插值无法精确表示 ⇒ 结点处必然折
    let (w, h, s) = (24usize, 4usize, 4usize);
    let mut px: Vec<u8> = Vec::with_capacity(w * h * 4);
    for _y in 0..h {
        for x in 0..w {
            let t = x as f32 / (w - 1) as f32;
            let v = (t * t * 255.0).round().clamp(0.0, 255.0) as u8;
            px.extend_from_slice(&[v, v, v, 255]);
        }
    }
    let f = BilinearOklabField::build(&px, w, h, s, (0.0, 0.0, w as f64, h as f64));
    let lum = |x: f64| {
        let (rgb, _, _) = f.sample_at(x, h as f64 / 2.0);
        0.2126 * f64::from(rgb[0]) + 0.7152 * f64::from(rgb[1]) + 0.0722 * f64::from(rgb[2])
    };
    // **二阶差分**（步长 d）
    let d2 = |x: f64, d: f64| (lum(x + d) - 2.0 * lum(x) + lum(x - d)).abs() / (d * d);
    let d = 0.25;
    // 结点：`s=4` ⇒ 内部结点在 u = 1/3、2/3 ⇒ 文档 x = w/3、2w/3
    let knot = (w as f64) / 3.0;
    // 格内：结点与中点之间
    let mid = (w as f64) / 3.0 + (w as f64) / 6.0;
    let at_knot = d2(knot, d);
    let away = d2(mid, d);
    let ratio = at_knot / away.max(1e-6);
    assert!(
        ratio < 4.0,
        "结点处的二阶差分不应远大于格内（折痕判据）：结点 {at_knot:.4}、格内 {away:.4}、\
         比值 {ratio:.2}（bilinear 会让这个比值到几十 ⇒ 红 ✓）"
    );
}

#[test]
fn oklab_roundtrip_approx() {
    for (r, g, b) in [(255, 0, 0), (0, 255, 0), (0, 0, 255), (128, 128, 128)] {
        let lab = srgb_u8_to_oklab(r, g, b);
        let rgb = oklab_to_srgb_f32(lab[0], lab[1], lab[2]);
        let back = [
            (rgb[0] * 255.0).round() as i32,
            (rgb[1] * 255.0).round() as i32,
            (rgb[2] * 255.0).round() as i32,
        ];
        let orig = [r as i32, g as i32, b as i32];
        for k in 0..3 {
            assert!(
                (back[k] - orig[k]).abs() <= 3,
                "OKLab 往返误差应 ≤3，{orig:?} → {back:?}"
            );
        }
    }
    // 变异说明：改错任一矩阵系数 → 往返误差超标变红。
}

#[test]
fn bilinear_field_transparent_pixels_skipped() {
    // 全透明像素不参与平均：左半透明、右半白 → 场应为白色而非灰。
    let mut px = vec![0u8; 2 * 2 * 4];
    px[4] = 255;
    px[5] = 255;
    px[6] = 255;
    px[7] = 255; // (1,0) 白不透明
    px[12] = 255;
    px[13] = 255;
    px[14] = 255;
    px[15] = 255; // (1,1) 白不透明
                  // (0,0),(0,1) 透明（a=0）
    let f = BilinearOklabField::build(&px, 2, 2, 4, (0.0, 0.0, 2.0, 2.0));
    let (rgb, _, _) = f.sample_at(1.5, 0.5);
    assert!(
        rgb[0] > 0.9 && rgb[1] > 0.9 && rgb[2] > 0.9,
        "透明像素不应拉低平均，得到 {rgb:?}"
    );
    // 变异说明：去掉 a<16 跳过 → 平均被透明黑拉低，变红。
}

// ---- ⑤ smart_erase_shape ----
#[test]
fn erase_runs_skip_hole() {
    // rect 100×100 带中央孔：每行的游程应断开，孔洞处无游程。
    let v = json_shape(
        r#"{"type":"rect","x":0,"y":0,"w":100,"h":100,
            "holes":[[[40,40],[60,40],[60,60],[40,60]]]}"#,
    );
    let p = parse_smart_shape(&v, "test").unwrap();
    let rows = erase_shape_runs(&p, false, 0.0, 0.0, 100.0, 100.0, 10.0, 2.0);
    // y=50 行（穿过孔洞）：应有左右两段游程，中间 40..60 无覆盖。
    let mid = &rows[5];
    assert_eq!(mid.len(), 2, "孔洞行应断成两段，得到 {mid:?}");
    assert!(mid[0].1 < 40.0, "左段应在孔左，得到 {mid:?}");
    assert!(mid[1].0 >= 60.0, "右段应在孔右，得到 {mid:?}");
    // y=10 行（无孔）：一段。
    assert_eq!(rows[1].len(), 1);
    // 变异说明：游程不扣孔洞 → mid.len()==1，变红。
}

#[test]
fn erase_runs_invert_covers_outside() {
    let v = json_shape(r#"{"type":"rect","x":40,"y":40,"w":20,"h":20}"#);
    let p = parse_smart_shape(&v, "test").unwrap();
    let rows = erase_shape_runs(&p, true, 0.0, 0.0, 100.0, 100.0, 10.0, 2.0);
    // y=50 行：形状内 40..60 不擦，两侧擦。
    let mid = &rows[5];
    assert_eq!(mid.len(), 2, "反相应擦形状两侧，得到 {mid:?}");
    assert!(mid[0].1 < 40.0);
    assert!(mid[1].0 > 60.0);
    // 变异说明：invert 逻辑写反 → mid.len()==1（只擦形状内），变红。
}

/// **两套形状词汇不得互相兜底** ✓（实测事故 ✗）。
///
/// `draw_shape` 用 `{"kind":"rect","bbox":{x,y,w,h}}` ✓，smart_* 用**平铺**的
/// `{"type":"rect","x":…,"w":…}` ✓。把前者喂给 `smart_erase_shape` 时它**静默**
/// 兜底成 `(0,0,100,100)` ⇒ 擦掉左上角一小块、还回 `ok:true` ✗
///（本仓 4K 临摹时真的踩了一次 ✓：`erased_runs:9`、`dirty_bbox:[-12,-12,124,120]`✓）
#[test]
fn shape_from_the_other_vocabulary_is_rejected_not_defaulted() {
    for bad in [
        r#"{"kind":"rect","bbox":{"x":0,"y":1488,"w":3840,"h":1488}}"#,
        r#"{"type":"rect","bbox":{"x":0,"y":0,"w":10,"h":10}}"#,
        // **`kind` 与 `type` 互为别名** ⇒ `{kind:"circle",x,y,r}` 属于**平铺**词汇、是合法写法 ✓
        // ⇒ 真正的"另一套"是带 `bbox`／`geometry` 的结构化形态 ✓（下面这条就是）。
        r#"{"kind":"circle","bbox":{"x":10,"y":10,"w":10,"h":10}}"#,
    ] {
        let v = json_shape(bad);
        let err = parse_smart_shape(&v, "smart_erase_shape").unwrap_err();
        assert!(
            format!("{err:?}").contains("平铺"),
            "错误信息要写出可用形式，实测：{err:?}"
        );
    }
    // **`kind` 当别名的平铺写法也必须通过** ✓（本轮 CI 上两条判据与外评测都用它 ✓）
    let alias = json_shape(r#"{"kind":"rect","x":0,"y":1488,"w":3840,"h":1488}"#);
    assert_eq!(
        parse_smart_shape(&alias, "smart_erase_shape").unwrap().bbox,
        (0.0, 1488.0, 3840.0, 2976.0),
        "`kind` 与 `type` 互为别名 ⇒ 平铺 + kind 必须照旧通过"
    );
    // 而**平铺**形式必须照旧通过 ✓（不能把修 bug 变成新限制 ✗）
    let ok = json_shape(r#"{"type":"rect","x":0,"y":1488,"w":3840,"h":1488}"#);
    assert_eq!(
        parse_smart_shape(&ok, "smart_erase_shape").unwrap().bbox,
        (0.0, 1488.0, 3840.0, 2976.0)
    );
}

// ---- ⑥ smart_hatch_follow ----
#[test]
fn guide_table_straight_line_tangent_zero() {
    // 直线引导：切向 ≈0°，曲率 ≈0。
    let t = GuideTable::build(&[(0.0, 0.0), (100.0, 0.0)]).unwrap();
    let (_, _, tan, kap) = t.at(50.0);
    assert!(tan.abs() < 0.05, "直线切向应≈0，得到 {tan}");
    assert!(kap.abs() < 1e-6, "直线曲率应≈0，得到 {kap}");
    assert!(t.total > 99.0 && t.total < 101.0);
    // 变异说明：切向算错 → tan 断言变红。
}

#[test]
fn guide_table_arc_length_monotonic() {
    let t = GuideTable::build(&[(0.0, 0.0), (50.0, 30.0), (100.0, 0.0)]).unwrap();
    let mut prev = -1.0;
    for r in &t.rows {
        assert!(r.0 >= prev, "弧长表必须单调递增");
        prev = r.0;
    }
    assert!((t.total - prev).abs() < 1.0, "total 应等于末行 s");
}

#[test]
fn guide_table_curved_has_curvature() {
    // 折线引导：转弯处曲率显著 >0。
    let t = GuideTable::build(&[(0.0, 0.0), (50.0, 0.0), (50.0, 50.0)]).unwrap();
    let max_k = t.rows.iter().map(|r| r.4).fold(0f64, f64::max);
    assert!(max_k > 0.01, "转弯处曲率应显著，得到 {max_k}");
    // 变异说明：曲率恒返回 0 → 变红。
}

#[test]
fn guide_table_needs_two_points() {
    assert!(GuideTable::build(&[]).is_none());
    assert!(GuideTable::build(&[(1.0, 1.0)]).is_none());
}

#[test]
fn stroke_path_plan_line_spacing_and_count() {
    // 直线 300px、size 20 ⇒ Δs = max(1, 0.2×20) = 4：点数 = floor(300/4)+1 = 76，间距恒 4。
    let sub = yanshi_render::path::Subpath {
        start: (0.0, 0.0),
        segments: vec![yanshi_render::path::PathSeg::Line { to: (300.0, 0.0) }],
        closed: false,
    };
    let plan = plan_stroke_path(
        &[sub],
        20.0,
        StrokePressureProfile::Constant,
        1.0,
        0.25,
        0.5,
    );
    assert_eq!(plan.skipped_subpaths, 0);
    assert_eq!(plan.strokes.len(), 1);
    let pts = &plan.strokes[0];
    assert_eq!(pts.len(), 76, "点数应为 76，实测 {}", pts.len());
    for pair in pts.windows(2) {
        let d = (pair[1][0] - pair[0][0]).abs();
        assert!(
            (4.0 * 0.99..=4.0 * 1.01).contains(&d),
            "采样间距 {d} 超出 Δs ±1%"
        );
        assert_eq!(pair[0][1], 0.0);
    }
    assert!((plan.length_total - 300.0).abs() < 1e-9);
}

#[test]
fn stroke_path_plan_profiles_and_time() {
    let sub = yanshi_render::path::Subpath {
        start: (0.0, 0.0),
        segments: vec![yanshi_render::path::PathSeg::Line { to: (300.0, 0.0) }],
        closed: false,
    };
    let plan = plan_stroke_path(
        std::slice::from_ref(&sub),
        20.0,
        StrokePressureProfile::Simulate,
        0.8,
        0.25,
        0.5,
    );
    let pts = &plan.strokes[0];
    assert_eq!(pts.first().unwrap()[2], 0.0, "simulate 首点压感应为 0");
    assert_eq!(pts.last().unwrap()[2], 0.0, "simulate 尾点压感应为 0");
    let mid = pts[pts.len() / 2];
    assert!(
        (mid[2] - 0.8).abs() < 1e-6,
        "simulate 中段应满压 0.8，实测 {}",
        mid[2]
    );
    for pair in pts.windows(2) {
        assert!(pair[1][3] > pair[0][3], "time_ms 必须严格单调递增");
    }
    assert_eq!(pts.last().unwrap()[3], 600.0, "末点 time = round(300/0.5)");
    // constant：全点压感相等。
    let plan = plan_stroke_path(
        &[sub],
        20.0,
        StrokePressureProfile::Constant,
        0.8,
        0.25,
        0.5,
    );
    assert!(plan.strokes[0].iter().all(|p| (p[2] - 0.8).abs() < 1e-12));
}

#[test]
fn stroke_path_plan_skips_zero_length_and_counts_closed_loop() {
    let zero = yanshi_render::path::Subpath {
        start: (5.0, 5.0),
        segments: vec![yanshi_render::path::PathSeg::Line { to: (5.0, 5.0) }],
        closed: false,
    };
    let line = yanshi_render::path::Subpath {
        start: (0.0, 0.0),
        segments: vec![yanshi_render::path::PathSeg::Line { to: (30.0, 0.0) }],
        closed: false,
    };
    let plan = plan_stroke_path(
        &[zero, line],
        10.0,
        StrokePressureProfile::Constant,
        1.0,
        0.25,
        0.5,
    );
    assert_eq!(plan.skipped_subpaths, 1, "零长子路径应跳过并计数");
    assert_eq!(plan.strokes.len(), 1);
    // 闭合子路径：回环段（50 的斜边）计入总长：30 + 40 + 50 = 120。
    let closed = yanshi_render::path::Subpath {
        start: (0.0, 0.0),
        segments: vec![
            yanshi_render::path::PathSeg::Line { to: (30.0, 0.0) },
            yanshi_render::path::PathSeg::Line { to: (30.0, 40.0) },
        ],
        closed: true,
    };
    let plan = plan_stroke_path(
        &[closed],
        10.0,
        StrokePressureProfile::Constant,
        1.0,
        0.25,
        0.5,
    );
    assert!(
        (plan.length_total - 120.0).abs() < 1e-9,
        "闭合回环总长应为 120，实测 {}",
        plan.length_total
    );
    // 闭合笔的末点回到起点。
    let last = plan.strokes[0].last().unwrap();
    assert_eq!((last[0], last[1]), (0.0, 0.0));
}

/// **第 1440 轮审计**：`color_lut` 的 `path` 曾直接 `read_to_string` ✗ ⇒
/// root 之外的任意绝对路径都能读。现在与 `put_blob`/`import_asset` 同一沙箱
/// （`file_source::resolve_local_path` 唯一实现 ✓）。
/// 变异（退回 `read_to_string`）⇒ root 外文件被读进来 ⇒ 本测试红 ✓。
#[test]
fn color_lut_path_must_stay_inside_the_workspace_root() {
    let pid = std::process::id();
    let root = std::env::temp_dir().join(format!("yanshi_lut_root_{pid}"));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let cube = "LUT_3D_SIZE 2\n0 0 0\n0 0 1\n0 1 0\n0 1 1\n1 0 0\n1 0 1\n1 1 0\n1 1 1\n";

    // root **内**的 .cube ⇒ 正常解析出 size+table ✓。
    let inside = root.join("good.cube");
    std::fs::write(&inside, cube).unwrap();
    let mut params = serde_json::Map::new();
    params.insert(
        "path".to_owned(),
        Value::String(inside.display().to_string()),
    );
    resolve_lut_path("color_lut", &mut params, Some(root.as_path())).unwrap();
    assert_eq!(
        params.get("size"),
        Some(&json!(2)),
        "root 内的合法 LUT 应当照常导入"
    );

    // root **外**的 .cube ⇒ 必须拒绝，且说清沙箱理由 ✓。
    let outside = std::env::temp_dir().join(format!("yanshi_lut_outside_{pid}.cube"));
    std::fs::write(&outside, cube).unwrap();
    let mut params = serde_json::Map::new();
    params.insert(
        "path".to_owned(),
        Value::String(outside.display().to_string()),
    );
    let err = resolve_lut_path("color_lut", &mut params, Some(root.as_path()))
        .expect_err("root 之外的 path 必须被拒");
    assert!(
        format!("{err}").contains("工作区根目录"),
        "要像 put_blob 一样说清沙箱理由：{err}"
    );

    // `file://` 形态不许绕道本地路径 ✓（与 tools_bitmap 同口径）。
    let mut params = serde_json::Map::new();
    params.insert(
        "path".to_owned(),
        Value::String(format!("file://{}", inside.display())),
    );
    let err = resolve_lut_path("color_lut", &mut params, Some(root.as_path()))
        .expect_err("file:// 必须被拒");
    assert!(
        format!("{err}").contains("scheme"),
        "要指名 scheme 问题：{err}"
    );

    std::fs::remove_dir_all(&root).ok();
    std::fs::remove_file(&outside).ok();
}

/// **第 1440 轮审计**：`edge` 曾 `_ => {}` 把拼错的/非字符串的值**静默当 "hard"** ✗
///（静默降级形态四）。现在只认 hard/soft/lost，其余**报错并列出可用值** ✓。
/// 变异（退回 `_ => {}` 吞掉）⇒ 前两段的 `unwrap_err` 拿不到错 ⇒ 红 ✓。
#[test]
fn edge_intent_rejects_unknown_values_with_allowed_list() {
    // 合法三档照常工作 ✓（soft/lost 确实改了 hardness ✓）。
    assert!(merge_intent_brush_params(&json!({ "edge": "hard" })).is_ok());
    let soft = merge_intent_brush_params(&json!({ "edge": "soft" }))
        .unwrap()
        .expect("soft 应产出 hardness");
    assert_eq!(soft["hardness"], json!(0.48), "{soft}");
    let lost = merge_intent_brush_params(&json!({ "edge": "lost" }))
        .unwrap()
        .expect("lost 应产出 hardness");
    assert_eq!(lost["hardness"], json!(0.1), "{lost}");

    // 拼错 ⇒ 报错，且把**给的坏值**与**可用值**都写出来 ✓。
    let err = format!(
        "{}",
        merge_intent_brush_params(&json!({ "edge": "fuzzy" })).unwrap_err()
    );
    assert!(err.contains("fuzzy"), "要点名坏值：{err}");
    for allowed in ["hard", "soft", "lost"] {
        assert!(err.contains(allowed), "要列出可用值 {allowed}：{err}");
    }

    // 非字符串 ⇒ 说明类型，不静默忽略 ✓。
    let err = format!(
        "{}",
        merge_intent_brush_params(&json!({ "edge": 1 })).unwrap_err()
    );
    assert!(err.contains("必须是字符串"), "要说明类型要求：{err}");
}

// ---- 跨文档的在飞操作：每文档令牌不许越界（第 1440 轮审计）----

/// `get_inflight`／`cancel_operation` 的 `doc_id` 是**入参** ✓ ⇒ 不设限时，
/// 一支文档 A 的 editor 令牌就能**枚举／打断任意文档**的在飞操作 ✗
///（12.x 的按文档授权被入参绕开 ✓）。规矩与 12.5 同形：跨边界只有 **Owner** ✓。
#[test]
fn inflight_tools_refuse_cross_document_targets_for_non_owner_tokens() {
    let mut workspace = Workspace::in_memory(crate::document::DocumentSettings::default());
    for id in ["doc_a", "doc_b"] {
        workspace
            .create_document(crate::document::NewDocument::new(id, 32, 32), "test:1", "s")
            .unwrap_or_else(|error| panic!("建不出 {id}：{error:?}"));
    }
    let mut ctx = ToolContext::new(&mut workspace, "doc_a", "test:1", "s");
    ctx.role = crate::token::Role::Editor;
    // **editor 拿不动跨文档** ✓。
    let err = read_get_inflight(&mut ctx, &json!({"doc_id": "*"}))
        .expect_err("editor 令牌不许枚举全部文档的在飞操作");
    let text = format!("{err}");
    assert!(text.contains("owner"), "错误要给出办法：{text}");
    assert!(text.contains("doc_a"), "错误要点名已授权文档：{text}");
    assert!(text.contains('*'), "错误要点名被请求的目标：{text}");
    assert!(
        read_get_inflight(&mut ctx, &json!({"doc_id": "doc_b"})).is_err(),
        "枚举别人的文档同样不许"
    );
    // 取消也一样 ✓ —— 原先的 `can_edit` 只看**角色** ✗，拦不住"取消**别人的文档**" ✗。
    let err = write_cancel_operation(&mut ctx, &json!({"doc_id": "doc_b"}))
        .expect_err("editor 不许打断其他文档的操作");
    assert!(
        format!("{err}").contains("doc_b"),
        "错误要点名被请求文档：{err}"
    );
    // **授权范围内的都照旧** ✓：守卫不许变成墙 ✗。
    assert!(read_get_inflight(&mut ctx, &json!({"doc_id": "doc_a"})).is_ok());
    assert!(read_get_inflight(&mut ctx, &json!({})).is_ok());
    assert!(write_cancel_operation(&mut ctx, &json!({"doc_id": "doc_a"})).is_ok());
    // **owner ⇒ 跨文档照旧可用** ✓（MCP／嵌入式／本地缺省就是 owner ⇒ 行为不变 ✓）。
    ctx.role = crate::token::Role::Owner;
    assert!(read_get_inflight(&mut ctx, &json!({"doc_id": "*"})).is_ok());
    assert!(write_cancel_operation(&mut ctx, &json!({"doc_id": "doc_b"})).is_ok());
}

// ---- 期 1 E-b：对象槽位三态 ----

/// `ensure_object_slot` 必须分清三态（期 1 E-b）：Absent 走新建、
/// Alive 走更新、Tombstoned 走复活——三态混同正是「同 id 删后不能
/// 重建」的根源。变异：把 Tombstoned 判回 Alive/Absent ⇒ 本条红。
#[test]
fn ensure_object_slot_distinguishes_three_states() {
    let mut workspace = Workspace::in_memory(crate::document::DocumentSettings::default());
    workspace
        .create_document(
            crate::document::NewDocument::new("doc_slot", 32, 32),
            "test:1",
            "s",
        )
        .unwrap();
    let mut ctx = ToolContext::new(&mut workspace, "doc_slot", "test:1", "s");
    write_create_layer(&mut ctx, &json!({"layer_id": "L"})).unwrap();
    assert_eq!(
        ensure_object_slot(&mut ctx, "obj_x").unwrap(),
        ObjectSlot::Absent,
        "从未存在的 id 必须是 Absent"
    );
    write_add_effect(
        &mut ctx,
        &json!({
            "layer_id": "L",
            "adjustment_type": "exposure",
            "object_id": "obj_x",
            "params": {"ev": 0.5}
        }),
        EffectKind::Adjustment,
    )
    .unwrap();
    assert_eq!(
        ensure_object_slot(&mut ctx, "obj_x").unwrap(),
        ObjectSlot::Alive,
        "建出后必须是 Alive"
    );
    write_tombstone(&mut ctx, &json!({"object_id": "obj_x"}), "object_id").unwrap();
    assert_eq!(
        ensure_object_slot(&mut ctx, "obj_x").unwrap(),
        ObjectSlot::Tombstoned,
        "删除后对象表留墓碑条目，必须是 Tombstoned（不是 Absent）"
    );
}

/// 纵深防御：`apply_allowlist` 求交为空必须响亮 panic 并点名冲突双方，
/// 绝不静默生成空注册表（设计 §7「求交为空响亮失败」）。
/// 正常组成下此路结构性不可达（白名单 ⊆ 自身基底展开，防漂移测试已钉死），
/// 故直接对 helper 喂一个人造空交集，验证守卫本身能红。
#[test]
fn allowlist_empty_intersection_panics_naming_parties() {
    let requested: std::collections::BTreeSet<Profile> =
        [Profile::Photo, Profile::Svg].into_iter().collect();
    let allowlist: std::collections::BTreeSet<&'static str> =
        ["definitely_not_a_tool"].into_iter().collect();
    let tools = ToolRegistry::full().tools().to_vec();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        apply_allowlist(tools, &allowlist, &requested)
    }));
    let payload = result.expect_err("求交为空必须 panic，不许静默返回空注册表");
    let message = payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_owned()))
        .unwrap_or_default();
    assert!(
        message.contains("求交为空"),
        "panic 须点明求交为空：{message}"
    );
    assert!(
        message.contains("photo"),
        "panic 须点名请求方 photo：{message}"
    );
    assert!(message.contains("svg"), "panic 须点名请求方 svg：{message}");
}

/// 触发判据钉死（Wang 2026-10-24 裁定）：三层具名组成（ALL / IMPLEMENTED /
/// DEFAULT）不收窄；只有最终用户显式点名的子集才触发白名单收窄。
#[test]
fn whitelist_trigger_only_for_explicit_subsets() {
    let as_set = |ps: &[Profile]| {
        ps.iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert!(
        whitelist_for(&as_set(&Profile::ALL)).is_none(),
        "ALL 不得收窄"
    );
    assert!(
        whitelist_for(&as_set(&Profile::IMPLEMENTED)).is_none(),
        "IMPLEMENTED（--profile all 的展开）不得收窄"
    );
    assert!(
        whitelist_for(&as_set(&Profile::DEFAULT)).is_none(),
        "DEFAULT 缺省常用集不得收窄（Photo 在此只贡献成员资格）"
    );
    assert!(
        whitelist_for(&as_set(&[Profile::Core, Profile::Collab])).is_none(),
        "不含 Photo/Svg 的请求不收窄"
    );
    let photo_list = whitelist_for(&as_set(&[Profile::Photo])).expect("[Photo] 必须收窄");
    assert_eq!(photo_list.len(), PHOTO_TOOLS.len());
    let union_list =
        whitelist_for(&as_set(&[Profile::Photo, Profile::Svg])).expect("[Photo, Svg] 必须收窄");
    assert!(union_list.len() > photo_list.len());
}

/// **落笔顶层结论的判定顺序**（A1/B1）：四臂全覆盖 + 边界。
///
/// 判定顺序钉死：`clipped == 0` ⇒ `clipped_empty`；`ink_footprint == 0` ⇒ `zero_ink`；
/// `max_delta == 0` ⇒ `invisible_on_background`；其余 ⇒ `painted`。
/// `landed` 为真当且仅当 reason 是 `painted`。
#[test]
fn landed_reason_four_arms() {
    assert_eq!(
        landed_reason(0, 5, 10),
        ("clipped_empty", false),
        "clipped 为 0 ⇒ 裁空，与足迹／delta 无关"
    );
    assert_eq!(
        landed_reason(10, 0, 10),
        ("zero_ink", false),
        "落了面积但足迹为 0 ⇒ 零墨"
    );
    assert_eq!(
        landed_reason(10, 5, 0),
        ("invisible_on_background", false),
        "落了墨但 delta 为 0 ⇒ 白上画白"
    );
    assert_eq!(
        landed_reason(10, 5, 10),
        ("painted", true),
        "三者皆非零 ⇒ 落上了"
    );
}

#[test]
fn landed_reason_precedence_and_boundaries() {
    // **顺序即优先级**：三个零同时出现时，先判定的赢。
    assert_eq!(
        landed_reason(0, 0, 0),
        ("clipped_empty", false),
        "三者全零 ⇒ 裁空先赢"
    );
    assert_eq!(
        landed_reason(5, 0, 0),
        ("zero_ink", false),
        "足迹与 delta 同零 ⇒ 零墨先于白上画白"
    );
    // **边界**：最小非零输入即落上。
    assert_eq!(landed_reason(1, 1, 1), ("painted", true), "1/1/1 ⇒ 落上了");
    // **`landed` 与 reason 恒一致**：为真当且仅当 reason 是 `painted`。
    for (clipped, footprint, delta) in [
        (0, 0, 0),
        (0, 1, 1),
        (1, 0, 0),
        (1, 1, 0),
        (1, 1, 1),
        (u64::MAX, u64::MAX, u64::MAX),
    ] {
        let (reason, landed) = landed_reason(clipped, footprint, delta);
        assert_eq!(
            landed,
            reason == "painted",
            "landed 须与 reason 一致：({clipped}, {footprint}, {delta}) ⇒ {reason}/{landed}"
        );
    }
}

/// `color_space_of` 三臂（A5）：与 `parse_spec_color` 的量级切换**同一条规则** ✓。
///
/// 浮点数组（任一分量都不 > 1）⇒ `linear`；字节数组（任一分量 > 1）／对象式 ⇒ `srgb_bytes`；
/// hex 字符串 ⇒ `hex`。
#[test]
fn color_space_of_three_arms() {
    assert_eq!(
        color_space_of(&json!([0.2, 0.4, 0.9, 1.0])),
        "linear",
        "浮点数组 ⇒ linear（alpha 恰为 1.0 也不 reroute，与 parse_spec_color 同口径）"
    );
    assert_eq!(
        color_space_of(&json!([115, 97, 77])),
        "srgb_bytes",
        "字节数组 ⇒ srgb_bytes（任一分量 > 1 即字节分支）"
    );
    assert_eq!(
        color_space_of(&json!([200, 200, 200, 1])),
        "srgb_bytes",
        "混合域 [200,200,200,1] ⇒ srgb_bytes（RGB 是字节域，alpha 的 1.0 不改分支）"
    );
    assert_eq!(
        color_space_of(&json!({"r": 115, "g": 97, "b": 77})),
        "srgb_bytes",
        "对象式 ⇒ srgb_bytes（恒为字节）"
    );
    assert_eq!(color_space_of(&json!("#73391F")), "hex", "hex 字符串 ⇒ hex");
}

/// `applied_echo_values` 回显的是**传入的值**，不是写死的缺省（A5）。
///
/// **变异**：把回显恒写缺省（`wait_for_render` 恒 `true`、`wait_budget_ms` 恒 500）
/// ⇒ 下面这组非缺省输入立刻红 ✓。
#[test]
fn applied_echo_values_echoes_given_values_not_defaults() {
    let applied = applied_echo_values(
        false,
        2000,
        false,
        true,
        "doc-x",
        Some(&json!([0.2, 0.4, 0.9, 1.0])),
        None,
    );
    assert_eq!(
        applied.get("wait_for_render").and_then(Value::as_bool),
        Some(false),
        "wait_for_render:false 必须原样回显，不能恒写缺省 true"
    );
    assert_eq!(
        applied.get("wait_budget_ms").and_then(Value::as_u64),
        Some(2000),
        "wait_budget_ms 必须回显传入的 2000，不能恒写缺省 500"
    );
    assert_eq!(
        applied.get("preview").and_then(Value::as_bool),
        Some(false),
        "preview:false 必须原样回显"
    );
    assert_eq!(
        applied.get("silent").and_then(Value::as_bool),
        Some(true),
        "silent:true 必须原样回显"
    );
    assert_eq!(
        applied.get("doc_id").and_then(Value::as_str),
        Some("doc-x"),
        "doc_id 必须原样回显"
    );
    assert_eq!(
        applied.get("color_space").and_then(Value::as_str),
        Some("linear"),
        "浮点数组 ⇒ color_space linear"
    );
}

/// 没给 `color` 时不编 `color_space`（A5）：缺省色是笔刷／介质内部的事实，回显不认领。
#[test]
fn applied_echo_values_omits_color_space_without_color() {
    let applied = applied_echo_values(true, 500, true, false, "doc-x", None, None);
    assert!(
        applied.get("color_space").is_none(),
        "没给 color ⇒ applied 里不该出现 color_space（实测 {:?}）",
        applied.get("color_space")
    );
    assert_eq!(
        applied.get("wait_for_render").and_then(Value::as_bool),
        Some(true),
        "缺省值仍要回显：wait_for_render true"
    );
    assert_eq!(
        applied.get("wait_budget_ms").and_then(Value::as_u64),
        Some(500),
        "缺省值仍要回显：wait_budget_ms 500"
    );
}

/// `parse_color_space_arg` 只认两个值（C1）：非法值报错并列出允许值；不传 / null ⇒ None。
///
/// **变异**：非法值回落到 `None`（不报错）⇒ 下面 `unwrap_err` 拿不到错 ⇒ 红 ✓。
#[test]
fn parse_color_space_arg_accepts_only_srgb_and_linear() {
    use yanshi_render::color::ColorSpace;
    assert_eq!(
        parse_color_space_arg(&json!({})).unwrap(),
        None,
        "不传 ⇒ None（缺省量级切换）"
    );
    assert_eq!(
        parse_color_space_arg(&json!({"color_space": null})).unwrap(),
        None,
        "null ⇒ None"
    );
    assert_eq!(
        parse_color_space_arg(&json!({"color_space": "srgb"})).unwrap(),
        Some(ColorSpace::Srgb),
        "srgb ⇒ Srgb"
    );
    assert_eq!(
        parse_color_space_arg(&json!({"color_space": "linear"})).unwrap(),
        Some(ColorSpace::Linear),
        "linear ⇒ Linear"
    );
    let err = parse_color_space_arg(&json!({"color_space": "lab"})).unwrap_err();
    let text = format!("{err:?}");
    assert!(text.contains("srgb"), "非法值报错必须列出 srgb：{text}");
    assert!(text.contains("linear"), "非法值报错必须列出 linear：{text}");
    assert!(
        parse_color_space_arg(&json!({"color_space": 3})).is_err(),
        "非字符串也必须拒绝"
    );
    // **重放侧共用同一解析**：source 里记的原话（"srgb" / "linear" / 缺省 null）都能读回。
    assert_eq!(
        color_space_from_value(Some(&json!("srgb"))).unwrap(),
        Some(ColorSpace::Srgb),
        "source 里的 \"srgb\" 读回 Srgb"
    );
    assert_eq!(
        color_space_from_value(None).unwrap(),
        None,
        "source 里没记 ⇒ None（老笔触）"
    );
    assert_eq!(
        color_space_from_value(Some(&json!(null))).unwrap(),
        None,
        "source 里记 null ⇒ None"
    );
}

/// 报告 5/6 第 6 项：`mixed_color_space_warning` 只在「显式 linear＋渐变两端异族」时报。
///
/// 钉死定义：数组端（显式 linear 下线性直通）与对象式／hex 端（恒为 sRGB 字节）同现一笔。
/// 与报告 4 P4 分工：P4 要求空间缺省，本判据要求空间显式 linear，两谓词空间不交，合入后
/// 同一笔永不因这两项得两条 warning。
///
/// **变异**：判据恒报 ⇒ 显式 srgb 臂、同族臂、单色臂红；判据恒不报 ⇒ 两触发臂红；
/// 文案删「混写」或「linear」任一词 ⇒ 文案臂红。
#[test]
fn mixed_color_space_warning_only_for_explicit_linear_gradient_mix() {
    use yanshi_render::color::ColorSpace;
    let linear = Some(ColorSpace::Linear);
    let srgb = Some(ColorSpace::Srgb);
    // 触发：数组端＋对象式端，两个方向都算混写。
    let note = mixed_color_space_warning(
        Some(&json!([0.47, 0.353, 0.235])),
        Some(&json!({"r": 120, "g": 90, "b": 60})),
        linear,
    )
    .expect("显式 linear＋数组／对象式两端必须报 warning");
    assert!(
        note.contains("混写") && note.contains("linear"),
        "文案必须同时说明混写与显式 linear：{note}"
    );
    assert!(
        note.contains("同一写法"),
        "文案必须给出统一写法的出路：{note}"
    );
    assert!(
        mixed_color_space_warning(
            Some(&json!({"r": 120, "g": 90, "b": 60})),
            Some(&json!([0.47, 0.353, 0.235])),
            linear,
        )
        .is_some(),
        "反向（对象式起点＋数组末端）同样是混写，也应报"
    );
    assert!(
        mixed_color_space_warning(
            Some(&json!([0.47, 0.353, 0.235])),
            Some(&json!("#785A3C")),
            linear,
        )
        .is_some(),
        "数组端＋hex 端同样是混写，也应报"
    );
    // 不触发：不传空间——两端口径不同时属 P4（缺省线性）覆盖，本判据不抢报。
    assert!(
        mixed_color_space_warning(
            Some(&json!([120, 90, 60])),
            Some(&json!([0.5, 0.2, 0.2])),
            None,
        )
        .is_none(),
        "不传空间时本判据不报（P4 的地盘，合入后不重复）"
    );
    // 不触发：显式 srgb 时两端全是 sRGB 字节族，不存在混写。
    assert!(
        mixed_color_space_warning(
            Some(&json!([0.47, 0.353, 0.235])),
            Some(&json!({"r": 120, "g": 90, "b": 60})),
            srgb,
        )
        .is_none(),
        "显式 srgb 下数组也被解释为 sRGB 字节，不应触发"
    );
    // 不触发：两端同族——数组＋数组（同为线性）、对象式＋hex（同为 sRGB 字节）。
    assert!(
        mixed_color_space_warning(
            Some(&json!([0.47, 0.353, 0.235])),
            Some(&json!([0.5, 0.2, 0.2])),
            linear,
        )
        .is_none(),
        "数组＋数组同为线性，不应触发"
    );
    assert!(
        mixed_color_space_warning(
            Some(&json!({"r": 120, "g": 90, "b": 60})),
            Some(&json!("#785A3C")),
            linear,
        )
        .is_none(),
        "对象式＋hex 同为 sRGB 字节族（回显词不同而已），不应触发"
    );
    // 不触发：只给一端颜色——单色无“两端混写”（相邻的显式空间被忽略项不在本项范围）。
    assert!(
        mixed_color_space_warning(Some(&json!({"r": 120, "g": 90, "b": 60})), None, linear)
            .is_none(),
        "单色对象式＋显式 linear 不属本项（回执 applied 已照实报 srgb_bytes）"
    );
    assert!(
        mixed_color_space_warning(
            Some(&json!([0.47, 0.353, 0.235])),
            Some(&json!(null)),
            linear,
        )
        .is_none(),
        "color_to 为 null 视同没给，不应触发"
    );
}

/// 显式 `color_space` 下的回显（C1）：回显的是**生效值**，不是原话 ——
/// 对象式 / hex 传了显式空间也不认领它。
///
/// **变异**：回显恒写 `color_space_of` 的推导（无视显式参数）⇒ srgb 臂红；
/// 回显恒写 `"srgb_bytes"` ⇒ linear 臂与 hex 臂红。
#[test]
fn applied_echo_values_effective_space_with_explicit_param() {
    use yanshi_render::color::ColorSpace;
    let echo = |color: Value, space: Option<ColorSpace>| {
        applied_echo_values(true, 500, true, false, "doc-x", Some(&color), space)
            .get("color_space")
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    assert_eq!(
        echo(json!([0.45, 0.38, 0.30]), Some(ColorSpace::Srgb)).as_deref(),
        Some("srgb_bytes"),
        "浮点数组 + 显式 srgb ⇒ srgb_bytes"
    );
    assert_eq!(
        echo(json!([0.45, 0.38, 0.30]), Some(ColorSpace::Linear)).as_deref(),
        Some("linear"),
        "浮点数组 + 显式 linear ⇒ linear"
    );
    assert_eq!(
        echo(json!([115, 97, 77]), Some(ColorSpace::Linear)).as_deref(),
        Some("linear"),
        "字节数组 + 显式 linear ⇒ linear（直通口径，分量 > 1 也不 reroute）"
    );
    assert_eq!(
        echo(json!("#73391F"), Some(ColorSpace::Srgb)).as_deref(),
        Some("hex"),
        "hex + 显式 srgb ⇒ 仍是 hex（hex 不受参数影响）"
    );
    assert_eq!(
        echo(
            json!({"r": 115, "g": 97, "b": 77}),
            Some(ColorSpace::Linear)
        )
        .as_deref(),
        Some("srgb_bytes"),
        "对象式 + 显式 linear ⇒ 仍是 srgb_bytes（对象式恒为字节）"
    );
    assert_eq!(
        echo(json!([0.45, 0.38, 0.30]), None).as_deref(),
        Some("linear"),
        "不传 ⇒ 量级切换推导（浮点 ⇒ linear）"
    );
}

/// **P2-7**：batch 水位谓词（纯函数）—— 每条分支都要能红 ✓。
///   变异：把 `>` 改成 `>=`（边界）或把阈值改大 ⇒ 对应断言红 ✓。
#[test]
fn batch_watermark_fires_on_high_rss() {
    let reason = batch_watermark_reason_for(Some(4 * 1024 + 1), Some(8 * 1024));
    assert!(
        reason.as_deref().unwrap_or("").contains("RSS"),
        "RSS 超 4 GiB 应当触发：{reason:?}"
    );
}

#[test]
fn batch_watermark_fires_on_low_mem_available() {
    let reason = batch_watermark_reason_for(Some(100), Some(511));
    assert!(
        reason.as_deref().unwrap_or("").contains("可用内存"),
        "MemAvailable 低于 512MB 应当触发：{reason:?}"
    );
}

#[test]
fn batch_watermark_stays_quiet_within_limits() {
    assert_eq!(
        batch_watermark_reason_for(Some(100), Some(8 * 1024)),
        None,
        "两项都在限内时不应当触发"
    );
    // 边界值本身不触发（`>` / `<`，不是 `>=` / `<=` ✓）。
    assert_eq!(
        batch_watermark_reason_for(Some(4 * 1024), Some(512)),
        None,
        "恰好等于阈值时不应当触发"
    );
}

#[test]
fn batch_watermark_fails_open_when_unreadable() {
    assert_eq!(
        batch_watermark_reason_for(None, None),
        None,
        "两项都读不到时检查跳过（fail-open），不拦批次"
    );
    assert_eq!(
        batch_watermark_reason_for(None, Some(8 * 1024)),
        None,
        "RSS 读不到、内存充足时不应当触发"
    );
    assert!(
        batch_watermark_reason_for(None, Some(100)).is_some(),
        "RSS 读不到、内存不足时仍应当由另一项触发"
    );
}

/// **A 期（paint-r2-A）**：水位缺省 4096 不变 —— 解析口径本身可单测，不碰进程环境变量 ✓。
///   变异：把缺省改大 ⇒ 缺省断言红；把 `filter(>= 1)` 删掉 ⇒ 0 值断言红 ✓。
#[test]
fn batch_rss_limit_defaults_to_4096_and_rejects_bad_values() {
    assert_eq!(
        parse_batch_rss_limit_mb(None),
        4 * 1024,
        "未设环境变量时水位缺省必须是 4096（旧硬编码值）"
    );
    assert_eq!(
        parse_batch_rss_limit_mb(Some("4096")),
        4 * 1024,
        "显式 4096 与缺省同值"
    );
    assert_eq!(
        parse_batch_rss_limit_mb(Some(" 8192 ")),
        8192,
        "首尾空白应当容忍"
    );
    assert_eq!(
        parse_batch_rss_limit_mb(Some("not-a-number")),
        4 * 1024,
        "解析失败应当回退缺省，而不是编一个会立刻触发的上限"
    );
    assert_eq!(
        parse_batch_rss_limit_mb(Some("0")),
        4 * 1024,
        "0 不是合法水位（会立刻触发），应当回退缺省"
    );
}

/// **A 期**：环境变量覆盖生效 —— 显式水位版谓词：同一 RSS 在旧水位下不触发、在覆盖水位下触发 ✓。
///   变异：谓词忽略传入水位、仍用 4096 ⇒ 第二个断言红 ✓。
#[test]
fn batch_watermark_override_limit_takes_effect() {
    assert_eq!(
        batch_watermark_reason_for_limit(Some(2048), Some(8 * 1024), 4 * 1024),
        None,
        "RSS 2048 在缺省水位 4096 下不应当触发"
    );
    assert!(
        batch_watermark_reason_for_limit(Some(2048), Some(8 * 1024), 1024)
            .as_deref()
            .unwrap_or("")
            .contains("1024"),
        "同一 RSS 在覆盖水位 1024 下应当触发，且文案报的是覆盖后的水位：{:?}",
        batch_watermark_reason_for_limit(Some(2048), Some(8 * 1024), 1024)
    );
    assert_eq!(
        batch_watermark_reason_for_limit(Some(1024), Some(512), 1024),
        None,
        "恰好等于覆盖水位时不应当触发（`>`，不是 `>=`）"
    );
}

/// **D1（期 2b）：`batch_stroke_evidence` 的字段归一** ✓。
///
/// 断的是**组装逻辑本身**（不经过 `batch` 的循环 ✓）：brush 回执原样取四字段、
/// medium 回执由同义字段归一、非笔触不编数字、失败带完整 `error`、缺失字段给 `null`。
#[test]
fn batch_stroke_evidence_maps_brush_and_medium_receipts() {
    // **brush 回执** ✓：四字段原样取（`painted_pixels_clipped` 本来就有 ✓）。
    let brush = json!({
        "ok": true, "landed": true, "painted_pixels": 1000,
        "painted_pixels_clipped": 800, "ink_mass": 123456, "max_channel_delta": 178,
    });
    let entry = batch_stroke_evidence(0, "brush_stroke", &brush, false);
    assert_eq!(
        entry,
        json!({
            "index": 0, "tool": "brush_stroke", "ok": true, "landed": true,
            "painted_pixels_clipped": 800, "ink_mass": 123456, "max_channel_delta": 178,
        }),
        "brush 摘要应原样取四字段"
    );
    // **medium 回执** ✓：`painted_pixels` 即 clipped、`paint_delta` 即 delta（只在 batch 路径归一 ✓）。
    let medium = json!({
        "ok": true, "landed": true, "painted_pixels": 900,
        "ink_mass": 9999, "paint_delta": 106,
    });
    let entry = batch_stroke_evidence(1, "medium_stroke", &medium, false);
    assert_eq!(
        entry["painted_pixels_clipped"],
        json!(900),
        "medium 没有 painted_pixels_clipped ⇒ 回落到 painted_pixels"
    );
    assert_eq!(
        entry["max_channel_delta"],
        json!(106),
        "medium 没有 max_channel_delta ⇒ 回落到 paint_delta"
    );
    // **非笔触** ✓：不编数字（只有 index/tool/ok ✓）。
    let shape = json!({"ok": true, "object_id": "s1"});
    assert_eq!(
        batch_stroke_evidence(2, "draw_shape", &shape, false),
        json!({"index": 2, "tool": "draw_shape", "ok": true}),
        "非笔触摘要不应编取证数字"
    );
    // **失败** ✓：带完整 `error`（调用方传进来的渲染错误原样带走 ✓）。
    let failed = json!({
        "ok": false, "error_code": "reference_not_found",
        "retryable": false, "context": {"detail": "找不到 brush「x.myb」"},
    });
    let entry = batch_stroke_evidence(3, "brush_stroke", &failed, true);
    assert_eq!(entry["ok"], json!(false));
    assert_eq!(entry["error"], failed, "失败摘要的 error 应是完整渲染错误");
    // **缺失字段给 `null`** ✓（可观测：断言断数值时 `null` 会红，0 会谎报 ✓）。
    let sparse = json!({"ok": true, "painted_pixels": 5});
    let entry = batch_stroke_evidence(4, "brush_stroke", &sparse, false);
    assert!(entry["landed"].is_null(), "缺 landed ⇒ null，不编 false");
    assert_eq!(entry["painted_pixels_clipped"], json!(5));
    assert!(entry["ink_mass"].is_null(), "缺 ink_mass ⇒ null，不编 0");
    assert!(
        entry["max_channel_delta"].is_null(),
        "缺 delta ⇒ null，不编 0"
    );
}

#[test]
fn draw_text_position_accepts_both_array_and_dict() {
    // **gt3 冷启动探针（2026-10-27）**：字典 `{"x":..,"y":..}` 曾被真字体路径
    // 静默忽略成 `(0, 0)`（回执照样 `ok:true` ✓）⇒ 现在两种形态都认 ✓。
    // **变异**：把 `parse_position` 的字典分支删掉 ⇒ 字典用例红 ✓。
    assert_eq!(parse_position(&json!([100.0, 200.0])), (100.0, 200.0));
    assert_eq!(
        parse_position(&json!({"x": 100.0, "y": 200.0})),
        (100.0, 200.0)
    );
    assert_eq!(parse_position(&json!({"x": 100.0})), (100.0, 0.0));
    assert_eq!(parse_position(&json!([100.0])), (100.0, 0.0));
    assert_eq!(parse_position(&json!("nope")), (0.0, 0.0));
    assert_eq!(parse_position(&json!(42)), (0.0, 0.0));
}

// **审计 Batch 2 · 第 9 项**：试画表必须有上限、必须能淘汰、磁盘也要跟着清 ✓。
// 三层各自一条（内存淘汰／日志截断／过期目录清理），变异记录见 HANDOFF §74。

fn attempt_record(doc_id: &str, created: u64) -> AttemptRecord {
    AttemptRecord {
        doc_id: doc_id.to_owned(),
        layer_id: "layer_1".to_owned(),
        brush: "round".to_owned(),
        x0: 0,
        y0: 0,
        width: 1,
        height: 1,
        rgba: Vec::new(),
        painted: 0,
        label: None,
        status: "pending",
        params: json!({}),
        created,
    }
}

#[test]
fn attempt_eviction_removes_the_oldest_of_the_same_document_only() {
    let mut map = std::collections::HashMap::new();
    for (id, created) in [("a1", 1u64), ("a2", 2), ("a3", 3), ("a4", 4)] {
        map.insert(id.to_owned(), attempt_record("doc_a", created));
    }
    // 别的文档里有一条**更旧**的：不许被这个文档的淘汰挤掉 ✓。
    map.insert("b0".to_owned(), attempt_record("doc_b", 0));
    let evicted = evict_attempts_over_cap(&mut map, "doc_a", 2);
    assert_eq!(
        evicted,
        vec!["a1".to_owned(), "a2".to_owned()],
        "淘汰最旧的两条"
    );
    assert!(
        map.contains_key("a3") && map.contains_key("a4"),
        "新记录必须留下"
    );
    assert!(map.contains_key("b0"), "别的文档不许被挤掉");
    // 恒等式：淘汰数 + 留下的本档记录 = 原有本档记录 ⇒ 「一条记录要么留下要么进 evicted」✓。
    let left = map
        .values()
        .filter(|record| record.doc_id == "doc_a")
        .count();
    assert_eq!(evicted.len() + left, 4, "淘汰与留存必须覆盖全部记录");
    // 边界：恰好等于上限 ⇒ 不淘汰（`<=` 不是 `<`）。
    let mut exact = std::collections::HashMap::new();
    for (id, created) in [("a1", 1u64), ("a2", 2)] {
        exact.insert(id.to_owned(), attempt_record("doc_a", created));
    }
    assert!(
        evict_attempts_over_cap(&mut exact, "doc_a", 2).is_empty(),
        "等于上限时不该淘汰"
    );
    assert_eq!(exact.len(), 2);
}

#[test]
fn attempt_log_keeps_only_the_newest_lines() {
    let dir = std::env::temp_dir().join(format!("yanshi-attempt-log-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let log = dir.join("log.jsonl");
    let body: String = (1..=6).map(|n| format!("line-{n}\n")).collect();
    std::fs::write(&log, &body).unwrap();
    prune_attempt_log(&dir, 4);
    assert_eq!(
        std::fs::read_to_string(&log).unwrap(),
        "line-3\nline-4\nline-5\nline-6\n",
        "截断必须保留最新的行，而不是最旧的"
    );
    // 未超限 ⇒ 原样不动（不重写 ⇒ 不改变字节）。
    std::fs::write(&log, "line-1\nline-2\n").unwrap();
    prune_attempt_log(&dir, 4);
    assert_eq!(std::fs::read_to_string(&log).unwrap(), "line-1\nline-2\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn attempt_cleanup_removes_only_directories_older_than_the_retention() {
    let root = std::env::temp_dir().join(format!("yanshi-attempt-cleanup-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let stale = root.join("doc_stale");
    let fresh = root.join("doc_fresh");
    std::fs::create_dir_all(&stale).unwrap();
    std::fs::create_dir_all(&fresh).unwrap();
    let stale_file = stale.join("log.jsonl");
    std::fs::write(&stale_file, "old\n").unwrap();
    let old =
        std::time::SystemTime::now() - std::time::Duration::from_secs(ATTEMPT_RETENTION_SECS + 60);
    std::fs::File::options()
        .write(true)
        .open(&stale_file)
        .unwrap()
        .set_modified(old)
        .unwrap();
    std::fs::write(fresh.join("log.jsonl"), "new\n").unwrap();
    let removed = cleanup_attempt_dirs_older_than(
        &root,
        std::time::Duration::from_secs(ATTEMPT_RETENTION_SECS),
    );
    assert_eq!(removed, 1, "只删过期的那个目录");
    assert!(!stale.exists(), "过期目录必须删掉");
    assert!(
        fresh.exists(),
        "新鲜目录必须留着——删光会打破 list_attempts 的跨重启承诺"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// 审计 H-2：吊销只许 owner；找不到如实报错；吊销后立刻失效。
#[test]
fn revoke_token_is_owner_only_and_reports_unknown_tokens() {
    let mut workspace = Workspace::in_memory(crate::document::DocumentSettings::default());
    workspace
        .create_document(
            crate::document::NewDocument::new("doc_revoke", 16, 16),
            "test:1",
            "s",
        )
        .unwrap();
    let token = workspace
        .issue_token("doc_revoke", "test:2", crate::token::Role::Viewer)
        .unwrap()
        .0;
    let mut ctx = ToolContext::new(&mut workspace, "doc_revoke", "test:1", "s");
    // editor ⇒ permission_denied（handler 自己守 owner，不依赖路由）。
    ctx.role = crate::token::Role::Editor;
    let err = write_revoke_token(&mut ctx, &json!({"token": token.as_str()})).unwrap_err();
    assert_eq!(err.code, ErrorCode::PermissionDenied);
    // owner ⇒ 真吊销，并报告剩余数。
    ctx.role = crate::token::Role::Owner;
    let value = write_revoke_token(&mut ctx, &json!({"token": token.as_str()})).unwrap();
    assert_eq!(value["ok"], true);
    assert_eq!(value["revoked"], true);
    assert_eq!(value["remaining_tokens"], 0);
    // 再吊销同一串 ⇒ reference_not_found（不静默成功）。
    let err = write_revoke_token(&mut ctx, &json!({"token": token.as_str()})).unwrap_err();
    assert_eq!(err.code, ErrorCode::ReferenceNotFound);
    // 形态不对 ⇒ invalid_argument。
    let err = write_revoke_token(&mut ctx, &json!({"token": "short"})).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidArgument);
}

/// 审计 M-6：单文档存活图层数有上限；复用不占额度；墓碑不占额度（删了能再建）。
#[test]
fn create_layer_stops_at_the_per_document_cap() {
    let mut workspace = Workspace::in_memory(crate::document::DocumentSettings::default());
    workspace
        .create_document(
            crate::document::NewDocument::new("doc_cap", 16, 16),
            "test:1",
            "s",
        )
        .unwrap();
    let mut ctx = ToolContext::new(&mut workspace, "doc_cap", "test:1", "s");
    // **新建文档自带一个图层** ✓ ⇒ 可新建数 = 上限 − 已有数（不把「初始几层」写成判据输入 ✗）。
    let initial = ctx
        .workspace
        .document("doc_cap")
        .map(|document| document.state().alive_layers().len())
        .unwrap();
    assert!(initial >= 1, "新建文档应至少有一个初始图层，实测 {initial}");
    // **上限是允许数，不是「少一个」** ✓：最后一层必须建得出来。
    for index in 0..(MAX_LAYERS_PER_DOC - initial) {
        write_create_layer(&mut ctx, &json!({ "layer_id": format!("L{index}") }))
            .unwrap_or_else(|err| panic!("第 {index} 个图层应允许：{err}"));
    }
    // 到顶 ⇒ 明确拒绝（precondition_failed，错误里带上限与出路）。
    let err = write_create_layer(&mut ctx, &json!({ "layer_id": "L_overflow" })).unwrap_err();
    assert_eq!(err.code, ErrorCode::PreconditionFailed);
    assert!(format!("{err}").contains("上限"), "错误要说清上限：{err}");
    // 复用已存活图层 ⇒ 不新建 ⇒ 到顶仍可用。
    let reused =
        write_create_layer(&mut ctx, &json!({ "layer_id": "L0", "if_exists": "reuse" })).unwrap();
    assert_eq!(reused["reused"], true);
    // 删掉一个（留墓碑）⇒ 存活数 −1 ⇒ 又能建一个；建回上限后再次到顶。
    write_delete_layer(&mut ctx, &json!({ "layer_id": "L0" })).unwrap();
    let replaced = write_create_layer(&mut ctx, &json!({ "layer_id": "L_replacement" })).unwrap();
    assert_eq!(replaced["ok"], true);
    let err = write_create_layer(&mut ctx, &json!({ "layer_id": "L_overflow_2" })).unwrap_err();
    assert_eq!(err.code, ErrorCode::PreconditionFailed);
}

/// **P-4（涂抹预算）**：预测模型在标定锚点上分得清「预算内」与「预算外」。
/// 变异：把 `SMUDGE_COST_PER_PX3_MS` 改小一个量级 ⇒ 650 那笔不再超预算，本测试红；
/// 把 `SMUDGE_BUDGET_MS` 改大十倍 ⇒ 同样红。
#[test]
fn smudge_budget_model_splits_the_calibration_anchors() {
    use super::tools_brush::smudge_stroke_cost_ms;
    // watercolor-02 size 300、路径 2800 px：修后实测 16.1 秒 ⇒ 预算内。
    let inside = smudge_stroke_cost_ms(2800.0, 150.0);
    assert!(
        inside < super::tools_brush::SMUDGE_BUDGET_MS,
        "size 300 应当在预算内（预测 {inside:.0} ms）"
    );
    // 分诊的 size 650、路径 4240 px：修前 7 分钟以上未完成 ⇒ 预算外。
    let outside = smudge_stroke_cost_ms(4240.0, 325.0);
    assert!(
        outside > super::tools_brush::SMUDGE_BUDGET_MS,
        "size 650 应当超预算（预测 {outside:.0} ms）"
    );
    // 模型是单调的：路径与半径任一增大，预测只增不减。
    assert!(smudge_stroke_cost_ms(2800.0, 150.0) < smudge_stroke_cost_ms(2800.0, 151.0));
    assert!(smudge_stroke_cost_ms(2800.0, 150.0) < smudge_stroke_cost_ms(2801.0, 150.0));
}

/// **反馈期 4（5.2 涂抹自动拆分）**：拆分阈值按期 2 新上限 512 校准（不是旧 128），
/// 每段成本皆在预算内；退化情形不拆分（保留原 Err 路径）。
/// 变异：把 `SMUDGE_SPLIT_SAFETY` 改成 1.0 ⇒ 报告量级段成本断言红；
/// 把 `SMUDGE_SPLIT_MAX_SEGMENTS` 改成 100 ⇒ 超长路径断言红；
/// 把上限常量改回 128 ⇒ 首行断言红（期 2 已放宽到 512）。
#[test]
fn smudge_split_plan_calibrated_to_new_reach_cap_512() {
    use super::tools_brush::{
        brush_points_path_len_px, smudge_split_plan, smudge_split_segment_limit_px,
        smudge_stroke_cost_ms, split_dtimes_by_arclength, split_points_by_arclength,
        SMUDGE_BUDGET_MS, SMUDGE_COST_PER_PX3_MS, SMUDGE_SPLIT_SAFETY,
    };
    // 期 2 新上限就是 512：直接读渲染常量（不另写数 ⇒ 上限再改时此处自动跟上，
    // 但本测试名与注释写明 512，旧 128 在此红）。
    assert_eq!(
        yanshi_render::MAX_EFFECT_PADDING,
        512,
        "期 2 已把区域外扩上限从 128 放宽到 512"
    );
    // 段长上限（半径 130px 时约 4565px；安全系数 0.9 已计入）。
    let limit = smudge_split_segment_limit_px(130.0);
    assert!(
        (limit - 4565.0).abs() < 2.0,
        "段长上限应当约 4565px（实测 {limit:.1}）"
    );
    // 报告量级：路径 9382px、半径 130px ⇒ 预测约 55.5 秒，超 30 秒预算 ⇒ 拆 3 段。
    let plan = smudge_split_plan(9382.0, 130.0).expect("报告量级必须拆分");
    assert_eq!(plan.segments, 3, "9382 ÷ 4565 向上取整是 3 段");
    // 均分后每段成本 ≤ 预算 × 安全系数（每段约 18.5 秒）。
    let seg_cost = smudge_stroke_cost_ms(9382.0 / 3.0, 130.0);
    assert!(
        seg_cost <= SMUDGE_BUDGET_MS * SMUDGE_SPLIT_SAFETY + 1.0,
        "每段成本应当 ≤ 预算 × 0.9（实测 {seg_cost:.0} ms）"
    );
    // 预算内不拆分。
    assert!(smudge_split_plan(2800.0, 150.0).is_none());
    // 刚在预算内不拆分、刚超预算拆 2 段（边界两侧各留 0.1% 余量，避浮点抖动）。
    let at_budget = SMUDGE_BUDGET_MS / (130.0 * 130.0 * SMUDGE_COST_PER_PX3_MS);
    assert!(smudge_split_plan(at_budget * 0.999, 130.0).is_none());
    assert_eq!(
        smudge_split_plan(at_budget * 1.01, 130.0).map(|p| p.segments),
        Some(2)
    );
    // 段数超上限不拆分：64 段在半径 130 时只覆盖约 29.2 万 px，40 万 px 要 88 段。
    assert!(smudge_split_plan(400_000.0, 130.0).is_none());
    // 段长上限低于 100px 下限不拆分：半径 1000 时上限约 77px。
    assert!(smudge_split_plan(10_000.0, 1000.0).is_none());
    // 弧长切分：接点共享、段内原点保留、段长之和等于总长（不拉直折线）。
    let points = vec![
        (0.0, 0.0, 0.5),
        (100.0, 0.0, 0.5),
        (100.0, 100.0, 1.0),
        (200.0, 100.0, 0.5),
    ];
    let segs = split_points_by_arclength(&points, 3);
    assert_eq!(segs.len(), 3);
    assert_eq!(segs[0].last(), segs[1].first(), "相邻段必须共享接点");
    assert_eq!(segs[1].last(), segs[2].first(), "相邻段必须共享接点");
    assert_eq!(segs[0], vec![(0.0, 0.0, 0.5), (100.0, 0.0, 0.5)]);
    assert_eq!(segs[1], vec![(100.0, 0.0, 0.5), (100.0, 100.0, 1.0)]);
    assert_eq!(segs[2], vec![(100.0, 100.0, 1.0), (200.0, 100.0, 0.5)]);
    let total: f64 = segs.iter().map(|s| brush_points_path_len_px(s)).sum();
    assert!(
        (total - 300.0).abs() < 1e-9,
        "各段弧长之和应当等于总长（{total}）"
    );
    // 插值接点：直线两点切 2 段，接点在中点且压力线性插值。
    let line = vec![(0.0, 0.0, 0.0), (200.0, 0.0, 1.0)];
    let halves = split_points_by_arclength(&line, 2);
    assert_eq!(halves[0].last(), Some(&(100.0, 0.0, 0.5)));
    assert_eq!(halves[1].first(), Some(&(100.0, 0.0, 0.5)));
    // 段时间：长度不对齐 ⇒ None；对齐时接点时间插值。
    assert!(split_dtimes_by_arclength(&line, &[0.0], 2).is_none());
    let times = split_dtimes_by_arclength(&line, &[0.0, 2.0], 2).unwrap();
    assert_eq!(times[0].last(), Some(&1.0));
    assert_eq!(times[1].first(), Some(&1.0));
}

/// **反馈期 4（5.2）**：可达量 = 拖曳 + 半径 + 2，与期 2 的 `enforce_reach` 同口径；
/// 超 512 不拆分，`smudge = 0` 的笔不进拆分决策。
#[test]
fn smudge_reach_uses_drag_plus_radius_plus_two_against_cap_512() {
    use super::tools_brush::smudge_reach_px;
    use hokusai::BrushSetting;
    let mut brush = hokusai::Brush::default();
    brush.set(BrushSetting::Smudge, hokusai::SettingValue::constant(1.0));
    brush.set(
        BrushSetting::SmudgeLength,
        hokusai::SettingValue::constant(0.5),
    );
    brush.set(
        BrushSetting::Radius,
        hokusai::SettingValue::constant(libm::log(100.0) as f32),
    );
    let reach = smudge_reach_px(&brush).expect("涂抹笔必有可达量");
    // drag = 0.5 × 100 = 50；reach = 50 + 100 + 2 = 152 ≤ 512（f32 半径有约 1e-5 误差）。
    assert!(
        (reach - 152.0).abs() < 1e-3,
        "可达量应当是 152px（实测 {reach}）"
    );
    assert!(reach <= yanshi_render::MAX_EFFECT_PADDING as f64);
    // smudge = 0 ⇒ 闸不生效 ⇒ None（拆分决策不调用它，单笔路原样走）。
    brush.set(BrushSetting::Smudge, hokusai::SettingValue::constant(0.0));
    assert!(smudge_reach_px(&brush).is_none());
    // 超上限量级：半径 500、length 0.5 ⇒ drag 250，reach 752 > 512。
    let mut big = hokusai::Brush::default();
    big.set(BrushSetting::Smudge, hokusai::SettingValue::constant(1.0));
    big.set(
        BrushSetting::SmudgeLength,
        hokusai::SettingValue::constant(0.5),
    );
    big.set(
        BrushSetting::Radius,
        hokusai::SettingValue::constant(libm::log(500.0) as f32),
    );
    assert!(
        smudge_reach_px(&big).unwrap() > 512.0,
        "半径 500 的涂抹笔可达量必须超 512"
    );
}

/// **D 期（项 14）**：每步涂抹量归一到每 dab 间距一名义量。
/// 水彩 size 300（半径 150，dpar 2 + dpbr 6 ⇒ 间距 18.75px、每间距 9.375 步）：
/// 名义 0.3 ⇒ 每步约 0.0372，且 `(1-步量)^(间距/2)` 恰回 0.7（累计一名义量）。
/// 变异：把指数写成 `spacing / 2`（倒置）⇒ 第二组断言红；把 helper 改回返回原量
/// ⇒ 第一组断言红。
#[test]
fn smudge_step_amount_normalises_per_dab_interval() {
    use super::tools_brush::smudge_step_amount;
    use hokusai::BrushSetting;
    let mut brush = hokusai::Brush::default();
    brush.set(
        BrushSetting::DabsPerActualRadius,
        hokusai::SettingValue::constant(2.0),
    );
    brush.set(
        BrushSetting::DabsPerBasicRadius,
        hokusai::SettingValue::constant(6.0),
    );
    let step = smudge_step_amount(0.3, 150.0, &brush);
    assert!(
        (step - 0.0372).abs() < 0.001,
        "水彩 size 300 的每步量应当约 0.0372（实测 {step}）"
    );
    let cumulative = 1.0 - (1.0 - step).powf(18.75 / 2.0);
    assert!(
        (cumulative - 0.3).abs() < 1e-9,
        "每间距累计必须恰为一名义量 0.3（实测 {cumulative}）"
    );
    // 边界恒等：0 与 1 不变；两率皆 0（引擎不盖 dab）时返回原量。
    assert_eq!(smudge_step_amount(0.0, 150.0, &brush), 0.0);
    assert_eq!(smudge_step_amount(1.0, 150.0, &brush), 1.0);
    let mut no_dabs = hokusai::Brush::default();
    no_dabs.set(
        BrushSetting::DabsPerActualRadius,
        hokusai::SettingValue::constant(0.0),
    );
    no_dabs.set(
        BrushSetting::DabsPerBasicRadius,
        hokusai::SettingValue::constant(0.0),
    );
    assert_eq!(smudge_step_amount(0.3, 150.0, &no_dabs), 0.3);
}

/// **P-4 跟进（水彩 s36 洗白）**：净室的涂抹量与拖曳长度都读求值后值，
/// 只计 `brush_radius` 那条映射。小尺寸（半径 18，ln≈2.89，正好落在
/// 曲线的 2.89 拐点上）时：smudge 0.3+0.65 ⇒ 0.95，长度 0.03+0.07 ⇒ 0.10；
/// 大尺寸（半径 150，ln≈5.01，落在 3.91～6.0 的 0 平段上）时曲线给 0
/// ⇒ 0.3／0.03 不变。
/// 变异：把 helper 改回只读 `base_value` ⇒ 第一、二组断言红；把任一曲线的
/// 增量改成 0 ⇒ 对应那组红；把 `Radius` 的输入值换成别处 ⇒ 两组都红。
#[test]
fn radius_evaluated_value_reads_the_brush_radius_curves() {
    use hokusai::BrushSetting;
    let mut brush = hokusai::Brush::default();
    let mut smudge = hokusai::SettingValue::constant(0.3);
    let mut smudge_map = hokusai::InputMapping::new(hokusai::BrushInput::BrushRadius);
    smudge_map.points = vec![(1.0, 0.65), (2.89, 0.65), (3.91, 0.0), (6.0, 0.0)];
    smudge.inputs.push(smudge_map);
    brush.set(BrushSetting::Smudge, smudge);
    let mut length = hokusai::SettingValue::constant(0.03);
    let mut length_map = hokusai::InputMapping::new(hokusai::BrushInput::BrushRadius);
    length_map.points = vec![(1.0, 0.07), (2.89, 0.07), (3.91, 0.0), (6.0, 0.0)];
    length.inputs.push(length_map);
    brush.set(BrushSetting::SmudgeLength, length);
    // 小尺寸：半径 18 px，ln(18)≈2.8904——拐点取 2.89（与 .myb 同值），
    // 这样期望值是精确的 0.95／0.10，不受插值斜率影响。
    brush.set(BrushSetting::Radius, hokusai::SettingValue::constant(2.89));
    let small_amount = super::radius_evaluated_value(&brush, BrushSetting::Smudge);
    assert!(
        (small_amount - 0.95).abs() < 1e-6,
        "size 36 的涂抹量应当是 0.95（实测 {small_amount}）"
    );
    let small_length = super::radius_evaluated_value(&brush, BrushSetting::SmudgeLength);
    assert!(
        (small_length - 0.10).abs() < 1e-6,
        "size 36 的拖曳长度应当是 0.10（实测 {small_length}）"
    );
    // size 300：半径 150 px。
    brush.set(
        BrushSetting::Radius,
        hokusai::SettingValue::constant(libm::log(150.0) as f32),
    );
    let large_amount = super::radius_evaluated_value(&brush, BrushSetting::Smudge);
    assert!(
        (large_amount - 0.30).abs() < 1e-6,
        "size 300 的涂抹量应当是 0.30（实测 {large_amount}）"
    );
    let large_length = super::radius_evaluated_value(&brush, BrushSetting::SmudgeLength);
    assert!(
        (large_length - 0.03).abs() < 1e-6,
        "size 300 的拖曳长度应当是 0.03（实测 {large_length}）"
    );
    // 没有曲线的笔刷：与今天只读 base 的行为完全一致。
    let plain = hokusai::Brush::default();
    let plain_base = f64::from(plain.get(BrushSetting::SmudgeLength).base_value);
    assert_eq!(
        super::radius_evaluated_value(&plain, BrushSetting::SmudgeLength),
        plain_base
    );
}

/// **P-15：`delta_distribution` 的分位数规则** ✓（纯函数，直接测）。
///
/// 口径：直方图只统计改了的像素（`hist[0]` 不进分位数）；分位数是“累计 ≥ total×pct
/// 的最小 d”；一个像素都没改时三值全 0。
/// **变异** ✓：把分位数改成恒返 255 ⇒ 第一、第二条红；把 `skip(1)` 改成全算 ⇒
/// 第一条红（median 变 0，因为未改的像素占多数）。
#[test]
fn delta_distribution_quantiles_follow_the_documented_rule() {
    // 10 个像素，差值恰好是 1..=10 各一个 ⇒ 中位 5、p90 9、p99 10。
    let mut hist = [0u32; 256];
    for slot in hist.iter_mut().skip(1).take(10) {
        *slot = 1;
    }
    let dist = delta_distribution(&hist);
    assert_eq!(dist.median, 5, "累计 ≥ 5 的最小 d 是 5");
    assert_eq!(dist.p90, 9, "累计 ≥ 9 的最小 d 是 9");
    assert_eq!(dist.p99, 10, "累计 ≥ 9.9 的最小 d 是 10");
    assert_eq!(dist.changed_pixels, 10);
    // 5 个像素全是差 1 ⇒ 三值全是 1（纯函数例：改了的像素差值全相等时分布与最大值一致）。
    let mut hist = [0u32; 256];
    hist[1] = 5;
    let dist = delta_distribution(&hist);
    assert_eq!((dist.median, dist.p90, dist.p99), (1, 1, 1));
    assert_eq!(dist.changed_pixels, 5);
    // 只有 1 个像素改了，差 255 ⇒ 中位值就是 255（一个像素的分布没有“多数”可言；
    // 判单像素 spike 靠 `changed_pixels == 1`，不靠 median）。
    let mut hist = [0u32; 256];
    hist[255] = 1;
    let dist = delta_distribution(&hist);
    assert_eq!((dist.median, dist.p90, dist.p99), (255, 255, 255));
    assert_eq!(dist.changed_pixels, 1);
    // 一个像素都没改 ⇒ 三值全 0（不报空）。
    let dist = delta_distribution(&[0u32; 256]);
    assert_eq!((dist.median, dist.p90, dist.p99), (0, 0, 0));
    assert_eq!(dist.changed_pixels, 0);
}

/// **D-4b 回归：只靠曲线驱动涂抹的笔刷也读画布**。
///
/// `wet_round` 的 `smudge`：`base_value = 0`，但 `pressure` 曲线把它推到约 1.0；
/// 引擎的涂抹门（`stroke.rs` 的 `smudge_mapping_active`）认“`base != 0` 或有输入”，
/// 服务端旧判定只认 `base_value` ⇒ `fed_base=false`，引擎涂抹只能采本笔自己的墨，
/// 叠盖的笔触各成扁平胶囊，胶囊轮廓在密集重叠下织成环形网格（干笔软边加抖动不成囊，故无环）。
#[test]
fn reads_canvas_counts_curve_driven_smudge() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/brushes/");
    let wet_text =
        std::fs::read_to_string(format!("{root}wet_round.myb")).expect("读不到 wet_round.myb");
    let wet = hokusai::myb::from_str(&wet_text).expect("解析 wet_round.myb 失败");
    assert_eq!(
        wet.get(hokusai::BrushSetting::Smudge).base_value,
        0.0,
        "本测试的前提是 wet_round 的 smudge 只靠曲线驱动"
    );
    assert!(
        !wet.get(hokusai::BrushSetting::Smudge).inputs.is_empty(),
        "本测试的前提是 wet_round 的 smudge 带输入曲线"
    );
    assert!(
        super::brush_reads_the_canvas(&wet),
        "曲线驱动的涂抹也必须喂底图（修前这里是 false，即红）"
    );
    let dry_text =
        std::fs::read_to_string(format!("{root}dry_brush.myb")).expect("读不到 dry_brush.myb");
    let dry = hokusai::myb::from_str(&dry_text).expect("解析 dry_brush.myb 失败");
    assert!(
        !super::brush_reads_the_canvas(&dry),
        "不涂抹的笔刷仍不读画布"
    );
    let knife_text =
        std::fs::read_to_string(format!("{root}ramon-Knife.myb")).expect("读不到 ramon-Knife.myb");
    let knife = hokusai::myb::from_str(&knife_text).expect("解析 ramon-Knife.myb 失败");
    assert!(
        super::brush_reads_the_canvas(&knife),
        "base 驱动的涂抹笔刷保持读画布"
    );
    // **D-4b 跟进：永不为正的 smudge 曲线不读画布**。
    //
    // `Round` 的 `smudge`：`base_value = 0`，但 `pressure` 曲线恒在 [-0.4, 0]，
    // 钳后永远是 0，从不混画布色；判读画布会把纯红存成与白底合成过的粉色
    //（半透明单测红）。正值精炼下它必须判 false（旧口径这里是 true，即红）。
    let round_text = std::fs::read_to_string(format!("{root}Round.myb")).expect("读不到 Round.myb");
    let round = hokusai::myb::from_str(&round_text).expect("解析 Round.myb 失败");
    assert_eq!(
        round.get(hokusai::BrushSetting::Smudge).base_value,
        0.0,
        "本测试的前提是 Round 的 smudge 只靠曲线驱动"
    );
    assert!(
        !round.get(hokusai::BrushSetting::Smudge).inputs.is_empty(),
        "本测试的前提是 Round 的 smudge 带输入曲线"
    );
    assert!(
        !super::brush_reads_the_canvas(&round),
        "永不为正的 smudge 曲线不读画布（否则纯红存成粉色）"
    );
}
/// **反馈期 1（3.6a）：短路径延长的几何规则** ✓（纯函数，直接测）。
///
/// 口径：触发值是 dab 间距，目标值是 1.5× 间距；等于触发值不延长（正常笔画不动）；
/// 起笔处不动；单点／零长点列沿 +x 延长；末段为零长时沿最后一段非零方向延长。
/// **变异** ✓：把 `>=` 改成 `>` ⇒ “等于间距”那条红；把方向取成最后两点（可能零长）
/// ⇒ “末段零长”那条红；把目标值改成触发值 ⇒ “极短路径”那条总长断言红。
#[test]
fn extend_short_path_geometry() {
    let path_len = |points: &[(f64, f64, f64)]| -> f64 {
        points
            .windows(2)
            .map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt())
            .sum()
    };
    // 极短路径：5px（3-4-5 三角）→ 延长到 1.5× 间距，起笔不动、沿原方向。
    let short = vec![(0.0, 0.0, 1.0), (3.0, 4.0, 1.0)];
    let extended = extend_short_path(&short, 45.66, 68.49).expect("5px < 间距，必须延长");
    assert_eq!(extended[0], (0.0, 0.0, 1.0), "起笔处不动");
    assert!(
        (path_len(&extended) - 68.49).abs() < 1e-6,
        "延长后总长 = 目标值"
    );
    assert!((extended[2].0 - 41.094).abs() < 1e-6 && (extended[2].1 - 54.792).abs() < 1e-6);
    // 刚好小于间距：50px 路径、触发值 50.5 → 延长。
    let below = vec![(0.0, 0.0, 1.0), (30.0, 40.0, 1.0)];
    assert!(
        extend_short_path(&below, 50.5, 75.75).is_some(),
        "50 < 50.5，必须延长"
    );
    // 等于间距：50px 路径、触发值 50 → 不延长（正常笔画行为不变的边界）。
    assert!(
        extend_short_path(&below, 50.0, 75.0).is_none(),
        "等于间距不延长"
    );
    // 正常路径：200px → 不延长。
    let normal = vec![(0.0, 0.0, 1.0), (200.0, 0.0, 1.0)];
    assert!(
        extend_short_path(&normal, 45.66, 68.49).is_none(),
        "正常路径不动"
    );
    // 单点：沿 +x 延长到目标值，压力不变。
    let single = vec![(7.0, 9.0, 0.5)];
    let extended = extend_short_path(&single, 45.66, 68.49).expect("单点必须延长");
    assert_eq!(extended, vec![(7.0, 9.0, 0.5), (75.49, 9.0, 0.5)]);
    // 零长多点：沿 +x 延长。
    let zero = vec![(5.0, 5.0, 1.0), (5.0, 5.0, 1.0), (5.0, 5.0, 1.0)];
    let extended = extend_short_path(&zero, 45.66, 68.49).expect("零长点列必须延长");
    assert_eq!(extended.last().copied(), Some((73.49, 5.0, 1.0)));
    // 末段零长：沿最后一段非零方向（30,40）延长到 90。
    let trailing_dup = vec![(0.0, 0.0, 1.0), (30.0, 40.0, 1.0), (30.0, 40.0, 1.0)];
    let extended = extend_short_path(&trailing_dup, 60.0, 90.0).expect("末段零长仍须延长");
    assert!((path_len(&extended) - 90.0).abs() < 1e-6);
    assert!((extended.last().unwrap().0 - 54.0).abs() < 1e-6);
    assert!((extended.last().unwrap().1 - 72.0).abs() < 1e-6);
    // 空点列与非法数值：不延长（不 panic）。
    assert!(extend_short_path(&[], 45.66, 68.49).is_none());
    assert!(extend_short_path(&short, f64::NAN, 68.49).is_none());
    assert!(
        extend_short_path(&short, 45.66, 40.0).is_none(),
        "目标 < 触发是非法的"
    );
}

/// **反馈期 1（3.6a）：短路径延长后引擎至少落一枚 dab** ✓（落笔层实测）。
///
/// 口径：`100%_Opaque`、size 400（半径 200，dabs_per_basic_radius 2.18、
/// dabs_per_actual_radius 2.2、dabs_per_second 0）⇒ dab 间距 ≈ 45.7px；
/// 两点 4.2px 的原路径不延长时 0 像素，延长到 1.5× 间距后 > 0 像素。
/// 这条直接证明“轻点至少落一枚 dab”，不是只证几何延长。
/// **变异** ✓：把 `paint_brush` 的兜底删掉 ⇒ 集成测试那条红；把延长目标改回
/// 间距以下 ⇒ 本条红（延长后仍 0 像素）。
#[test]
fn short_stroke_extension_lands_at_least_one_dab() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/brushes/");
    let text =
        std::fs::read_to_string(format!("{root}100%_Opaque.myb")).expect("读不到测试用 .myb");
    let mut brush = hokusai::myb::from_str(&text).expect("解析 .myb 失败");
    brush.set(
        hokusai::BrushSetting::Radius,
        hokusai::SettingValue::constant(libm::log(200.0) as f32),
    );
    brush
        .get_mut(hokusai::BrushSetting::DabsPerSecond)
        .base_value = 0.0;
    let dpar = f64::from(
        brush
            .get(hokusai::BrushSetting::DabsPerActualRadius)
            .base_value,
    );
    let dpbr = f64::from(
        brush
            .get(hokusai::BrushSetting::DabsPerBasicRadius)
            .base_value,
    );
    let spacing = 200.0 / (dpar + dpbr);
    let points: Vec<(f64, f64, f64)> = vec![(100.0, 100.0, 1.0), (103.0, 103.0, 1.0)];
    let stamp = |points: &[(f64, f64, f64)]| -> usize {
        let mut state = hokusai::BrushState::default();
        let mut surface = hokusai::tile_mem::MemSurface::new();
        let mut previous = None;
        super::stamp_stroke_from(
            &brush,
            &mut state,
            &mut surface,
            points,
            &mut previous,
            &[],
            super::tools_brush::BrushSensors::default(),
            false,
        );
        let region = super::PaintRegion::from_bounds((0, 0, 399, 399, 400, 400));
        let (rgba, _) = super::read_surface_region(&surface, region);
        rgba.chunks_exact(4)
            .filter(|px| px.iter().any(|byte| *byte != 0))
            .count()
    };
    assert_eq!(
        stamp(&points),
        0,
        "原 4.2px 路径撑不满间距，必须 0 像素（否则本测试的前提不成立）"
    );
    let extended = extend_short_path(&points, spacing, spacing * 1.5).expect("短路径必须延长");
    let painted = stamp(&extended);
    assert!(
        painted > 0,
        "延长到 1.5× 间距后至少落一枚 dab ⇒ 像素必须 > 0，实测 {painted}"
    );
}

/// **预算字段收缩版**（feedback5-feat-budget-fields）：`ms_per_stroke_avg` 的
/// 公式与 0 笔行为。分子是同回执 `timings.total_ms`，3 位小数口径与
/// `timings` 一致；分母为 0 时字段恒为 0.0（不可判），不是缺失键。
#[test]
fn ms_per_stroke_avg_formula_and_zero() {
    assert_eq!(super::ms_per_stroke_avg(10.0, 4), 2.5);
    assert!(
        (super::ms_per_stroke_avg(10.0, 3) - 3.333).abs() < 1e-9,
        "10/3 应 3 位小数舍入为 3.333，实测 {}",
        super::ms_per_stroke_avg(10.0, 3)
    );
    assert_eq!(
        super::ms_per_stroke_avg(5.0, 0),
        0.0,
        "0 笔时字段取 0.0（不可判），不是缺失、不是 NaN"
    );
    assert_eq!(super::ms_per_stroke_avg(0.0, 5), 0.0);
}

/// 预算字段端到端共用脚手架：内存工作区 + 仓库 assets（笔刷可读）+ 空文档。
fn budget_fields_setup(doc_id: &str) -> super::Workspace {
    let mut workspace = super::Workspace::in_memory(crate::document::DocumentSettings::default())
        .with_assets_dir(Some(std::path::PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets"
        ))));
    workspace
        .create_document(
            crate::document::NewDocument::new(doc_id, 300, 300),
            "test:1",
            "s",
        )
        .expect("建测试文档");
    workspace
}

/// `describe_canvas` 顶层回执带字段，且字段值等于公式值（容差内）；
/// 其他工具（如 `get_document`）的回执不带该字段——证明“只在两处”。
#[test]
fn describe_canvas_receipt_carries_ms_per_stroke_avg() {
    let mut workspace = budget_fields_setup("budget_describe_doc");
    let registry = super::ToolRegistry::full();
    let mut ctx = super::ToolContext::new(&mut workspace, "budget_describe_doc", "test:1", "s");
    let response = registry.call(&mut ctx, "describe_canvas", &json!({}));
    assert_eq!(response["ok"], json!(true), "describe 应成功：{response}");
    let total_ms = response["timings"]["total_ms"]
        .as_f64()
        .expect("describe 回执须带 timings.total_ms");
    let atoms = response["counts"]["atoms"]
        .as_u64()
        .expect("describe 回执须带 counts.atoms");
    let avg = response["ms_per_stroke_avg"]
        .as_f64()
        .expect("describe 回执须带 ms_per_stroke_avg");
    if atoms == 0 {
        assert_eq!(avg, 0.0, "0 原子时字段取 0.0：{response}");
    } else {
        assert!(
            (avg * atoms as f64 - total_ms).abs() <= 0.001 * atoms as f64 + 1e-9,
            "字段值须等于 total_ms/atoms（3 位小数舍入容差内）：avg={avg} atoms={atoms} total_ms={total_ms}"
        );
    }
    let other = registry.call(&mut ctx, "get_document", &json!({}));
    assert_eq!(other["ok"], json!(true), "get_document 应成功：{other}");
    assert!(
        other.get("ms_per_stroke_avg").is_none(),
        "非目标工具的回执不许带该字段：{other}"
    );
}

/// `batch` 顶层回执带字段：2 笔纯笔触批次里 stroke_calls==2，
/// 字段值等于 total_ms/2（容差内）；全是非笔触子调用的批次里
/// stroke_calls==0，字段取 0.0。
#[test]
fn batch_receipt_carries_ms_per_stroke_avg() {
    let mut workspace = budget_fields_setup("budget_batch_doc");
    let registry = super::ToolRegistry::full();
    let mut ctx = super::ToolContext::new(&mut workspace, "budget_batch_doc", "test:1", "s");
    let created = registry.call(
        &mut ctx,
        "create_layer",
        &json!({"layer_id": "L1", "name": "Layer 1"}),
    );
    assert_eq!(created["ok"], json!(true), "建层应成功：{created}");
    let stroke = |x: i64| {
        json!({
            "tool": "brush_stroke",
            "arguments": {
                "brush": "100%_Opaque", "layer_id": "L1",
                "points": [[x, 100, 1], [x + 80, 140, 1], [x + 160, 100, 1]],
                "color": {"r": 255, "g": 0, "b": 0, "a": 255}, "size": 40
            }
        })
    };
    let batched = registry.call(
        &mut ctx,
        "batch",
        &json!({"calls": [stroke(20), stroke(20)]}),
    );
    assert_eq!(batched["ok"], json!(true), "纯笔触批次应全成：{batched}");
    assert_eq!(
        batched["stroke_calls"],
        json!(2),
        "两笔笔触子调用：{batched}"
    );
    let total_ms = batched["timings"]["total_ms"]
        .as_f64()
        .expect("batch 回执须带 timings.total_ms");
    let avg = batched["ms_per_stroke_avg"]
        .as_f64()
        .expect("batch 回执须带 ms_per_stroke_avg");
    assert!(
        (avg * 2.0 - total_ms).abs() <= 0.002 + 1e-9,
        "字段值须等于 total_ms/2（3 位小数舍入容差内）：avg={avg} total_ms={total_ms}"
    );
    let no_stroke = registry.call(
        &mut ctx,
        "batch",
        &json!({"calls": [
            {"tool": "create_layer", "arguments": {"layer_id": "L2", "name": "Layer 2"}},
            {"tool": "create_layer", "arguments": {"layer_id": "L3", "name": "Layer 3"}}
        ]}),
    );
    assert_eq!(
        no_stroke["ok"],
        json!(true),
        "非笔触批次应全成：{no_stroke}"
    );
    assert_eq!(
        no_stroke["stroke_calls"],
        json!(0),
        "无笔触子调用：{no_stroke}"
    );
    assert_eq!(
        no_stroke["ms_per_stroke_avg"],
        json!(0.0),
        "0 笔触时字段取 0.0（不可判）：{no_stroke}"
    );
}
