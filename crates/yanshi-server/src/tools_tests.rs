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
fn smart_shape_no_holes_backward_compat() {
    // 无 holes 时行为与旧解析一致。
    let v = json_shape(r#"{"type":"rect","x":10,"y":20,"w":30,"h":40}"#);
    let p = parse_smart_shape(&v, "test").unwrap();
    assert!(p.holes.is_empty());
    assert_eq!(p.bbox, (10.0, 20.0, 40.0, 60.0));
    assert!(p.contains(25.0, 30.0));
    assert!(!p.contains(0.0, 0.0));
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
