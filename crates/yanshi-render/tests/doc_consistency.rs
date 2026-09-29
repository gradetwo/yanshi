//! 文档与代码一致性：效果算子清单不得与内核常量静默分叉。
//!
//! 背景：此前多轮手工同步文档时，有一次"同步"其实是**空操作**（目标文本并不存在），
//! 但结论被当成已完成。用测试固定这类断言，比依赖人工仔细更可靠。

use std::path::PathBuf;

use yanshi_render::{ADJUSTMENT_NAMES, FILTER_NAMES};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("crate 应位于 <repo>/crates/yanshi-render")
        .to_path_buf()
}

fn read(name: &str) -> String {
    let path = repo_root().join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("读取 {path:?} 失败：{error}"))
}

/// 每个调整/滤镜名字都要出现在两份 README 与设计实现说明里。
#[test]
fn every_effect_name_is_documented() {
    // 效果清单放在 docs/tools.md（README 保持精简，只留命令与安装/编译步骤）。
    let docs = [
        ("docs/tools.md", read("docs/tools.md")),
        (
            "docs/design/implementation-notes.md",
            read("docs/design/implementation-notes.md"),
        ),
    ];
    let mut missing = Vec::new();
    for name in ADJUSTMENT_NAMES.iter().chain(FILTER_NAMES.iter()) {
        for (label, text) in &docs {
            if !text.contains(name) {
                missing.push(format!("{label} 缺少 {name}"));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "文档与内核常量不一致（共 {} 处）：\n{}",
        missing.len(),
        missing.join("\n")
    );
}

/// 实现说明里的总数行必须与常量实际长度一致。
#[test]
fn documented_effect_counts_match_the_kernel() {
    let notes = read("docs/design/implementation-notes.md");
    let line = notes
        .lines()
        .find(|line| line.contains("调整 **") && line.contains("种** / 滤镜 **"))
        .unwrap_or_else(|| {
            panic!(
                "未找到总数行（应为 `调整 **N 种** / 滤镜 **M 种**` 形式）：\
                 数字若不再被文档声明，本测试就失去了意义"
            )
        });
    let numbers: Vec<usize> = line
        .split(|character: char| !character.is_ascii_digit())
        .filter(|chunk| !chunk.is_empty())
        .filter_map(|chunk| chunk.parse().ok())
        .collect();
    assert_eq!(
        numbers.len(),
        2,
        "总数行应恰有两个数字（调整、滤镜）：{line}"
    );
    assert_eq!(
        numbers[0],
        ADJUSTMENT_NAMES.len(),
        "文档声明的调整数（{}）与内核不符（{}）：{line}",
        numbers[0],
        ADJUSTMENT_NAMES.len()
    );
    assert_eq!(
        numbers[1],
        FILTER_NAMES.len(),
        "文档声明的滤镜数（{}）与内核不符（{}）：{line}",
        numbers[1],
        FILTER_NAMES.len()
    );
}

/// 两份 README 的算子清单应逐项对应：README 里出现的每个内核名字都要在中文版里也出现。
#[test]
fn readmes_agree_on_the_effect_inventory() {
    let english = read("README.md");
    let chinese = read("README.zh-CN.md");
    let mut missing = Vec::new();
    for name in ADJUSTMENT_NAMES.iter().chain(FILTER_NAMES.iter()) {
        if english.contains(name) && !chinese.contains(name) {
            missing.push(format!("README.zh-CN.md 缺少 {name}"));
        }
        if chinese.contains(name) && !english.contains(name) {
            missing.push(format!("README.md 缺少 {name}"));
        }
    }
    assert!(
        missing.is_empty(),
        "两份 README 不一致：\n{}",
        missing.join("\n")
    );
}

/// 单效果成本预算：把「哪个效果变慢」变成**点名式**回归（而不是整档超预算后无从下手）。
///
/// 512² 缓冲、各效果**默认参数**、release 下取 3 次最小值；预算留足噪声余量
/// （本机 >4ms 量级单次计时波动可达 3×，见 implementation-notes 的多次警示）。
///
/// 实测表见 implementation-notes「单效果成本（512²，默认参数）」；超预算时本测试会**报出名字**。
#[test]
#[ignore = "性能预算：CI 用 --ignored 执行（单效果成本，512²）"]
fn single_effect_cost_budget() {
    use std::time::{Duration, Instant};
    use yanshi_render::{apply_adjustment, apply_filter, Buffer};

    fn sample() -> Buffer {
        let mut buffer = Buffer::new(0, 0, 512, 512);
        for (index, pixel) in buffer.pixels_mut().chunks_exact_mut(4).enumerate() {
            let value = (index % 251) as f32 / 251.0;
            pixel.copy_from_slice(&[value, value * 0.8, value * 0.6, 1.0]);
        }
        buffer
    }

    let mut rows: Vec<(String, Duration)> = Vec::new();

    // 滤镜：默认参数（与工具层一致）。
    for name in yanshi_render::FILTER_NAMES {
        let params = match name {
            "dehaze" => serde_json::json!({"air": [0.85, 0.85, 0.85]}),
            _ => serde_json::json!({}),
        };
        let mut best = Duration::MAX;
        for _ in 0..3 {
            let mut buffer = sample();
            let started = Instant::now();
            apply_filter(&mut buffer, name, &params, 1.0, (512.0, 512.0));
            best = best.min(started.elapsed());
        }
        rows.push((format!("filter:{name}"), best));
    }
    for name in yanshi_render::ADJUSTMENT_NAMES {
        let params = match name {
            "curves" => serde_json::json!({"points": [[0.0, 0.0], [1.0, 1.0]]}),
            _ => serde_json::json!({}),
        };
        let mut best = Duration::MAX;
        for _ in 0..3 {
            let mut buffer = sample();
            let started = Instant::now();
            apply_adjustment(&mut buffer, name, &params, 1.0);
            best = best.min(started.elapsed());
        }
        rows.push((format!("adjustment:{name}"), best));
    }

    rows.sort_by_key(|row| std::cmp::Reverse(row.1));
    println!("512² 单效果成本（默认参数，3 次最小）：");
    for (name, elapsed) in &rows {
        println!("  {name}: {elapsed:?}");
    }

    // 预算：调整类应 < 60ms；滤镜类应 < 200ms（模糊类默认半径下）。
    // 超预算时报出**具体名字**，让回归一眼可定位。
    for (name, elapsed) in &rows {
        let budget = if name.starts_with("adjustment:") {
            Duration::from_millis(60)
        } else {
            Duration::from_millis(200)
        };
        assert!(
            *elapsed < budget,
            "效果 {name} 在 512² 默认参数下耗时 {elapsed:?}，超出预算 {budget:?} —— \
             这属于单效果回归，请对照 implementation-notes 的「单效果成本」表"
        );
    }
}

/// 方框模糊的**半径伸缩**诊断（滑动窗口应呈 O(1)/像素，即耗时基本不随半径增长）。
///
/// 背景：设计方已批准模糊族放宽到 D1，朴素窗口求和（O(radius)/像素）被换成滑动窗口。
/// 改前同批次实测：`clarity` 半径 4/16/64 → 33 / 81 / **288ms**（近似线性增长）。
#[test]
#[ignore = "性能：方框模糊半径伸缩（滑动窗口后应近似平坦）"]
fn box_blur_cost_is_flat_in_radius() {
    use std::time::{Duration, Instant};
    use yanshi_render::{apply_filter, Buffer};

    let mut report = Vec::new();
    for radius in [4u64, 16, 64] {
        let mut best = Duration::MAX;
        for _ in 0..3 {
            let mut buffer = Buffer::new(0, 0, 512, 512);
            for (index, pixel) in buffer.pixels_mut().chunks_exact_mut(4).enumerate() {
                let value = (index % 251) as f32 / 251.0;
                pixel.copy_from_slice(&[value, value * 0.8, value * 0.6, 1.0]);
            }
            let started = Instant::now();
            apply_filter(
                &mut buffer,
                "clarity",
                &serde_json::json!({"amount": 0.9, "radius": radius}),
                1.0,
                (512.0, 512.0),
            );
            best = best.min(started.elapsed());
        }
        println!("  clarity radius {radius}: {best:?}");
        report.push((radius, best));
    }
    // 滑动窗口下，半径 64 不应比半径 4 慢出一个数量级（改前是 288ms vs 33ms ≈ 8.7×）。
    let slowest = report.iter().map(|(_, elapsed)| *elapsed).max().unwrap();
    let fastest = report.iter().map(|(_, elapsed)| *elapsed).min().unwrap();
    assert!(
        slowest.as_secs_f64() < fastest.as_secs_f64() * 3.0,
        "半径伸缩仍近似线性（最慢 {slowest:?} vs 最快 {fastest:?}）—— 滑动窗口可能没生效"
    );
}
