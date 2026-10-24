//! 期 4 D3 回测（D3-4）：`auto_retouch` 对 3 份金参考精修的对照。
//!
//! **门控**：输入（3 张 RAW + 3 份金 recipe）在仓库外，CI 没有它们，
//! 故本文件不是常规集成测试——环境变量 `YANSHI_AUTO_BACKTEST_DIR`
//! 未设置/指向不存在的目录时**跳过**（打印原因后直接返回，不算失败）；
//! 指向存在的目录但文件缺失时**响亮失败**（承诺了输入却没有）。
//! 变量值 = showcase 根目录（含 `downloads/` 与 `work/retouch/`）。
//!
//! 运行（盒上，raw feature 必须开，否则 import_image 拒 raw_options）：
//! `cargo test -p yanshi-server --features raw --test auto_backtest -- --nocapture`
//! 并设置 `YANSHI_AUTO_BACKTEST_DIR=~/workspace/yanshi-showcase`。
//!
//! 断言（设计 §5 D3 三触点，硬门）与报告（偏差表，只打印、不判）：
//! - 触点 1（underexposed）：auto 后显示 p50 ≥ 0.20（相对基线 0.141
//!   的实质提亮）且与金成图 p50 之差 ≤ 0.06（金锚定），且新 clip_hi
//!   ≤ 3%。**原设计带 [0.30, 0.55] 已废止**：实测金成图（工作区归档
//!   的 `underexposed__final.jpg`，本 harness 用 `decode_jpeg` 现量）
//!   p50 = 0.255——金参考自己都达不到 0.30；一个金都失败的带子不能
//!   拿来卡 auto（那等于要求 auto 比人工金参考更亮，即过曝化）。
//!   触点改为金锚定（见 phase4-notes 回测节）；
//! - 触点 2（overexposed）：highlights 组 ≤ −0.15（或有有效理由的
//!   跳过），且 clip_hi 不比基线增加；
//! - 触点 3（colorcast）：钠灯路 R−B 残差降 ≥ 40%（同 recipe 探区测量法），
//!   且 facade 中性区 R−B 变化 ≤ 0.02；
//! - 偏差表：每参数 |auto − gold|（3 张取 median/max；n=3 故 p90 ≈ max，
//!   如实写 max 不写 p90）+ 非可比项点名（curves、locals、levels 对
//!   curves、sharpen/denoise-detail 超出 auto 甜点钳制域等）；
//!   超甜点带半宽的参数由 phase4-notes 人工归因，不在本测试里硬判
//!   （归因是判断，不是数字能自动下的结论）。
//!
//! 导入口径：recipe 的 `raw_options` 原样透传（金参考的显影条件）；
//! 画布 = 展开尺寸（`decode_bytes` 帧尺寸 + EXIF orientation 换轴）；
//! auto 调用缺省全部组、strength=1、默认图层解析。

use serde_json::{json, Value};
use yanshi_core::Bbox;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("yanshi_backtest_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path
}

fn call(workspace: &mut Workspace, doc: &str, tool: &str, args: Value) -> Value {
    let mut ctx = ToolContext::new(workspace, doc, "human:1", "session:test").with_owner(true);
    registry().call(&mut ctx, tool, &args)
}

/// 显示亮度桶（整数 Rec.601，与统计层同口径）⇒ p50 与 clip_hi。
/// 全幅渲染上直接算（不降采样：clip_hi 经降采样会失真）。
fn luma_p50_and_clip(pixels: &[u8]) -> (f32, f32) {
    let mut buckets = [0u64; 256];
    let mut count = 0u64;
    for pixel in pixels.chunks_exact(4) {
        if pixel[3] == 0 {
            continue;
        }
        let luma = (299 * u32::from(pixel[0])
            + 587 * u32::from(pixel[1])
            + 114 * u32::from(pixel[2])
            + 500)
            / 1000;
        buckets[luma as usize] += 1;
        count += 1;
    }
    if count == 0 {
        return (0.0, 0.0);
    }
    let rank = count.div_ceil(2);
    let mut cumulative = 0u64;
    let mut p50 = 0u8;
    for (value, bucket) in buckets.iter().enumerate() {
        cumulative += bucket;
        if cumulative >= rank {
            p50 = value as u8;
            break;
        }
    }
    let clip = (buckets[253] + buckets[254] + buckets[255]) as f32 / count as f32;
    (p50 as f32 / 255.0, clip)
}

/// 探区平均 R−B（显示字节域归一化，recipe 探区测量法同口径）。
fn probe_r_minus_b(pixels: &[u8]) -> f32 {
    let mut sum_r = 0f64;
    let mut sum_b = 0f64;
    let mut count = 0u64;
    for pixel in pixels.chunks_exact(4) {
        if pixel[3] == 0 {
            continue;
        }
        sum_r += pixel[0] as f64;
        sum_b += pixel[2] as f64;
        count += 1;
    }
    if count == 0 {
        return 0.0;
    }
    ((sum_r - sum_b) / count as f64 / 255.0) as f32
}

struct Photo {
    /// 短名（underexposed/overexposed/colorcast）。
    name: &'static str,
    /// downloads/ 下文件名。
    raw_file: &'static str,
    /// work/retouch/ 下 recipe 文件名。
    recipe_file: &'static str,
}

const PHOTOS: [Photo; 3] = [
    Photo {
        name: "underexposed",
        raw_file: "ricketts_glen_0006.CR2",
        recipe_file: "underexposed__final.recipe.json",
    },
    Photo {
        name: "overexposed",
        raw_file: "blown_sky_a6000.ARW",
        recipe_file: "overexposed__final.recipe.json",
    },
    Photo {
        name: "colorcast",
        raw_file: "20221118_0018.ORF",
        recipe_file: "colorcast__final.recipe.json",
    },
];

/// 金 recipe 的 globals（type → params），供偏差表用。
fn gold_globals(recipe: &Value) -> Vec<(String, Value)> {
    recipe["globals"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|global| {
            (
                global["type"].as_str().unwrap_or("?").to_owned(),
                global["params"].clone(),
            )
        })
        .collect()
}

/// auto 响应里按 object_id 取 applied 参数（跳过时返回 None）。
fn auto_param(response: &Value, object_id: &str) -> Option<Value> {
    response["applied"]
        .as_array()?
        .iter()
        .find(|item| item["object_id"] == json!(object_id))
        .map(|item| item["params"].clone())
}

/// auto 响应里按 object_id 取 skipped 原因（未跳过时返回 None）。
fn auto_skip_reason(response: &Value, object_id: &str) -> Option<String> {
    response["skipped"]
        .as_array()?
        .iter()
        .find(|item| item["object_id"] == json!(object_id))
        .and_then(|item| item["reason"].as_str().map(str::to_owned))
}

/// 偏差行打印（gold 与 auto 皆为数字时给 |差|，否则点名状态）。
fn deviation_row(photo: &str, param: &str, gold: f64, auto: Option<f64>) {
    match auto {
        Some(value) => eprintln!(
            "DEVIATION {photo} {param} gold={gold:.4} auto={value:.4} absdiff={:.4}",
            (value - gold).abs()
        ),
        None => eprintln!("DEVIATION {photo} {param} gold={gold:.4} auto=SKIP"),
    }
}

fn num(value: &Value) -> Option<f64> {
    value.as_f64()
}

#[test]
fn auto_backtest_three_touchstones() {
    let Ok(root) = std::env::var("YANSHI_AUTO_BACKTEST_DIR") else {
        eprintln!("BACKTEST SKIP：未设置 YANSHI_AUTO_BACKTEST_DIR（输入在仓库外，CI 无此数据）");
        return;
    };
    let root = std::path::PathBuf::from(root);
    if !root.is_dir() {
        eprintln!(
            "BACKTEST SKIP：YANSHI_AUTO_BACKTEST_DIR 指向不存在的目录：{}",
            root.display()
        );
        return;
    }

    // 单张失败不中断：三张的 stats/偏差表都要看到，最后汇总断言
    // （任一触点失败 ⇒ 退出码仍非零，硬门语义不变）。
    let mut failures: Vec<String> = Vec::new();
    for photo in PHOTOS {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let raw_path = root.join("downloads").join(photo.raw_file);
            let recipe_path = root.join("work").join("retouch").join(photo.recipe_file);
            assert!(
                raw_path.is_file(),
                "回测输入缺失（承诺了目录却没有文件）：{}",
                raw_path.display()
            );
            assert!(
                recipe_path.is_file(),
                "回测输入缺失（承诺了目录却没有文件）：{}",
                recipe_path.display()
            );
            let raw_bytes = std::fs::read(&raw_path).expect("读 RAW");
            let recipe: Value =
                serde_json::from_str(&std::fs::read_to_string(&recipe_path).expect("读 recipe"))
                    .expect("解析 recipe");
            let raw_options = recipe["raw_options"].clone();
            // 金成图 p50（触点 1 的锚）：归档的最终 JPG 现量，不写死数字。
            let gold_jpg_path = root
                .join("work")
                .join("retouch")
                .join(format!("{}__final.jpg", photo.name));
            assert!(
                gold_jpg_path.is_file(),
                "回测输入缺失（金成图）：{}",
                gold_jpg_path.display()
            );
            let gold_jpg_bytes = std::fs::read(&gold_jpg_path).expect("读金成图");
            let (_, _, gold_pixels) =
                yanshi_render::decode_jpeg(&gold_jpg_bytes).expect("解码金成图");
            let (gold_p50, _) = luma_p50_and_clip(&gold_pixels);
            drop(gold_pixels);
            eprintln!("BACKTEST {}：金成图 p50={gold_p50:.3}", photo.name);

            // 展开尺寸：帧尺寸 + EXIF orientation 换轴（5–8 为 90°/270° 系）。
            let decoded = yanshi_raw::decode_bytes(&raw_bytes).expect("解码 RAW 取尺寸");
            let (mut dev_w, mut dev_h) = (decoded.frame.width as u32, decoded.frame.height as u32);
            if (5..=8).contains(&decoded.file_meta.orientation) {
                std::mem::swap(&mut dev_w, &mut dev_h);
            }
            eprintln!(
                "BACKTEST {}：{} 帧 {}×{} orientation={} ⇒ 画布 {}×{} raw_options={}",
                photo.name,
                photo.raw_file,
                decoded.frame.width,
                decoded.frame.height,
                decoded.file_meta.orientation,
                dev_w,
                dev_h,
                raw_options
            );

            let dir = temp_dir(photo.name);
            // **⇒ 本仓规矩：用了 `temp_dir()` 就必须有 Drop 守卫** ✓（第 930 轮；`check-tests-clean-temp` 抓到的 ✓）
            struct Cleanup(std::path::PathBuf);
            impl Drop for Cleanup {
                fn drop(&mut self) {
                    let _ = std::fs::remove_dir_all(&self.0);
                }
            }
            let _cleanup = Cleanup(dir.clone());
            let doc = format!("doc_backtest_{}", photo.name);
            let mut workspace =
                Workspace::with_file_store(dir.clone(), DocumentSettings::default())
                    .expect("落盘工作区应当能建");
            workspace
                .create_document(
                    NewDocument::new(&doc, dev_w, dev_h),
                    "human:1",
                    "session:test",
                )
                .unwrap();
            let layer = call(
                &mut workspace,
                &doc,
                "create_layer",
                json!({"layer_id": "src", "name": "源图"}),
            );
            assert_eq!(layer["ok"], json!(true), "{layer}");
            let hash = workspace.put_blob(&raw_bytes).expect("放 RAW blob");
            let imported = call(
                &mut workspace,
                &doc,
                "import_image",
                json!({
                    "layer_id": "src",
                    "bitmap": {"blob_hash": hash, "size": raw_bytes.len(), "mime_type": "image/x-yanshi-rawsrc"},
                    "region": {"x": 0, "y": 0, "w": dev_w, "h": dev_h},
                    "raw_options": raw_options,
                }),
            );
            assert_eq!(imported["ok"], json!(true), "RAW 导入应成功：{imported}");
            eprintln!(
                "BACKTEST {}：import raw_meta iso={}（对象落账后 auto 应读到同一值）",
                photo.name, imported["raw_meta"]["iso"]
            );

            // 基线：全幅渲染（未 auto）+ 探区 R−B。
            let bbox = Bbox::new(0.0, 0.0, dev_w as f64, dev_h as f64);
            let (_, _, base_pixels) = workspace
                .render_region_raw(&doc, bbox)
                .expect("基线全幅渲染");
            let (base_p50, base_clip) = luma_p50_and_clip(&base_pixels);
            drop(base_pixels);
            let mut base_probes: Vec<(String, f32)> = Vec::new();
            for probe in recipe["probes"].as_array().cloned().unwrap_or_default() {
                let region = &probe["region"];
                let (x, y, w, h) = (
                    region["x"].as_f64().unwrap_or(0.0),
                    region["y"].as_f64().unwrap_or(0.0),
                    region["w"].as_f64().unwrap_or(0.0),
                    region["h"].as_f64().unwrap_or(0.0),
                );
                let (_, _, pixels) = workspace
                    .render_region_raw(&doc, Bbox::new(x, y, w, h))
                    .expect("探区渲染");
                let value = probe_r_minus_b(&pixels);
                base_probes.push((probe["name"].as_str().unwrap_or("?").to_owned(), value));
            }
            eprintln!(
                "BACKTEST {}：基线 p50={base_p50:.3} clip_hi={base_clip:.4}",
                photo.name
            );

            // auto（缺省：全部组、strength=1、默认图层）。
            let auto = call(&mut workspace, &doc, "auto_retouch", json!({}));
            assert_eq!(auto["ok"], json!(true), "auto 应成功：{auto}");
            eprintln!("BACKTEST {}：auto stats={}", photo.name, auto["stats"]);
            for item in auto["applied"].as_array().cloned().unwrap_or_default() {
                eprintln!(
                    "BACKTEST {}：applied {} {} params={} confidence={} rationale={}",
                    photo.name,
                    item["group"],
                    item["object_id"],
                    item["params"],
                    item["confidence"],
                    item["rationale"]
                );
            }
            eprintln!(
                "BACKTEST {}：scene_flags={}",
                photo.name, auto["scene_flags"]
            );
            for item in auto["skipped"].as_array().cloned().unwrap_or_default() {
                eprintln!(
                    "BACKTEST {}：skipped {} 原因={}",
                    photo.name, item["object_id"], item["reason"]
                );
            }

            // auto 后：全幅渲染 + 探区 R−B。
            let (_, _, auto_pixels) = workspace
                .render_region_raw(&doc, bbox)
                .expect("auto 后全幅渲染");
            let (auto_p50, auto_clip) = luma_p50_and_clip(&auto_pixels);
            drop(auto_pixels);
            let mut auto_probes: Vec<(String, f32)> = Vec::new();
            for probe in recipe["probes"].as_array().cloned().unwrap_or_default() {
                let region = &probe["region"];
                let (x, y, w, h) = (
                    region["x"].as_f64().unwrap_or(0.0),
                    region["y"].as_f64().unwrap_or(0.0),
                    region["w"].as_f64().unwrap_or(0.0),
                    region["h"].as_f64().unwrap_or(0.0),
                );
                let (_, _, pixels) = workspace
                    .render_region_raw(&doc, Bbox::new(x, y, w, h))
                    .expect("探区渲染");
                auto_probes.push((
                    probe["name"].as_str().unwrap_or("?").to_owned(),
                    probe_r_minus_b(&pixels),
                ));
            }
            let probe_value = |probes: &[(String, f32)], name: &str| -> Option<f32> {
                probes
                    .iter()
                    .find(|(probe_name, _)| probe_name == name)
                    .map(|(_, value)| *value)
            };

            // ---- 三触点（硬门）----
            match photo.name {
                "underexposed" => {
                    assert!(
                    auto_p50 >= 0.20,
                    "触点 1：underexposed auto 后 p50 应 ≥ 0.20（相对基线 {base_p50:.3} 的实质提亮），实得 {auto_p50:.3}"
                );
                    assert!(
                    (auto_p50 - gold_p50).abs() <= 0.06,
                    "触点 1：underexposed auto 后 p50 应贴近金成图 {gold_p50:.3}（|差| ≤ 0.06），实得 {auto_p50:.3}"
                );
                    assert!(
                        auto_clip <= 0.03,
                        "触点 1：underexposed auto 后 clip_hi 应 ≤ 3%，实得 {auto_clip:.4}"
                    );
                }
                "overexposed" => {
                    let highlights = auto_param(&auto, "auto_tone")
                        .and_then(|params| num(&params["highlights"]));
                    match highlights {
                        Some(value) => assert!(
                            value <= -0.15,
                            "触点 2：overexposed highlights 组应 ≤ −0.15，实得 {value:.3}"
                        ),
                        None => {
                            let reason = auto_skip_reason(&auto, "auto_tone")
                                .expect("highlights 未落成时应有 skipped 原因");
                            assert!(!reason.is_empty(), "highlights 跳过原因不许为空");
                            eprintln!(
                                "BACKTEST overexposed：highlights 跳过（有效理由）：{reason}"
                            );
                        }
                    }
                    assert!(
                    auto_clip <= base_clip,
                    "触点 2：overexposed auto 后 clip_hi 不许比基线增加（基线 {base_clip:.4}，auto 后 {auto_clip:.4}）"
                );
                }
                "colorcast" => {
                    let base_road = probe_value(&base_probes, "road_lit")
                        .expect("colorcast 应有 road_lit 探区");
                    let auto_road = probe_value(&auto_probes, "road_lit")
                        .expect("colorcast 应有 road_lit 探区");
                    assert!(
                        base_road > 0.02,
                        "触点 3 前提：基线 road_lit R−B 应为正（钠灯偏色），实得 {base_road:.4}"
                    );
                    let reduction = (base_road - auto_road) / base_road;
                    let reduction_pct = reduction * 100.0;
                    eprintln!(
                        "BACKTEST colorcast：触点 3 实测 road_lit 基线 {base_road:.4} → auto 后 {auto_road:.4}（降 {reduction_pct:.1}%）"
                    );
                    assert!(
                    reduction >= 0.40,
                    "触点 3：road_lit R−B 残差应降 ≥ 40%，基线 {base_road:.4} → auto 后 {auto_road:.4}（降 {reduction_pct:.1}%）"
                );
                    let base_facade =
                        probe_value(&base_probes, "facade").expect("colorcast 应有 facade 探区");
                    let auto_facade =
                        probe_value(&auto_probes, "facade").expect("colorcast 应有 facade 探区");
                    let facade_shift = (auto_facade - base_facade).abs();
                    eprintln!(
                        "BACKTEST colorcast：触点 3 实测 facade 基线 {base_facade:.4} → auto 后 {auto_facade:.4}（变化 {facade_shift:.4}）"
                    );
                    assert!(
                    (auto_facade - base_facade).abs() <= 0.02,
                    "触点 3：facade 中性区 R−B 变化应 ≤ 0.02，基线 {base_facade:.4} → auto 后 {auto_facade:.4}"
                );
                }
                _ => unreachable!(),
            }
            eprintln!(
                "BACKTEST {}：触点通过（auto 后 p50={auto_p50:.3} clip_hi={auto_clip:.4}）",
                photo.name
            );

            // ---- 偏差表（只打印，不硬判；归因见 phase4-notes）----
            let gold = gold_globals(&recipe);
            let gold_of = |type_name: &str| -> Option<&Value> {
                gold.iter()
                    .find(|(name, _)| name == type_name)
                    .map(|(_, params)| params)
            };
            if let Some(params) = gold_of("exposure") {
                deviation_row(
                    photo.name,
                    "exposure.ev",
                    num(&params["ev"]).unwrap_or(0.0),
                    auto_param(&auto, "auto_exposure").and_then(|p| num(&p["ev"])),
                );
            }
            if let Some(params) = gold_of("shadows_highlights") {
                deviation_row(
                    photo.name,
                    "tone.shadows",
                    num(&params["shadows"]).unwrap_or(0.0),
                    auto_param(&auto, "auto_tone").and_then(|p| num(&p["shadows"])),
                );
                deviation_row(
                    photo.name,
                    "tone.highlights",
                    num(&params["highlights"]).unwrap_or(0.0),
                    auto_param(&auto, "auto_tone").and_then(|p| num(&p["highlights"])),
                );
            }
            if let Some(params) = gold_of("white_balance") {
                deviation_row(
                    photo.name,
                    "wb.temperature",
                    num(&params["temperature"]).unwrap_or(0.0),
                    auto_param(&auto, "auto_white_balance").and_then(|p| num(&p["temperature"])),
                );
                deviation_row(
                    photo.name,
                    "wb.tint",
                    num(&params["tint"]).unwrap_or(0.0),
                    auto_param(&auto, "auto_white_balance").and_then(|p| num(&p["tint"])),
                );
            }
            if let Some(params) = gold_of("vibrance") {
                deviation_row(
                    photo.name,
                    "vibrance.amount",
                    num(&params["amount"]).unwrap_or(0.0),
                    auto_param(&auto, "auto_vibrance").and_then(|p| num(&p["amount"])),
                );
            }
            if let Some(params) = gold_of("saturation") {
                deviation_row(
                    photo.name,
                    "saturation.amount",
                    num(&params["amount"]).unwrap_or(0.0),
                    auto_param(&auto, "auto_saturation").and_then(|p| num(&p["amount"])),
                );
            }
            for (gold_type, auto_id, label) in [
                ("dehaze", "auto_dehaze", "dehaze.amount"),
                ("clarity", "auto_clarity", "clarity.amount"),
                ("texture", "auto_texture", "texture.amount"),
            ] {
                if let Some(params) = gold_of(gold_type) {
                    deviation_row(
                        photo.name,
                        label,
                        num(&params["amount"]).unwrap_or(0.0),
                        auto_param(&auto, auto_id).and_then(|p| num(&p["amount"])),
                    );
                }
            }
            if let Some(params) = gold_of("denoise") {
                deviation_row(
                    photo.name,
                    "denoise.luminance",
                    num(&params["luminance"]).unwrap_or(0.0),
                    auto_param(&auto, "auto_denoise").and_then(|p| num(&p["luminance"])),
                );
                deviation_row(
                    photo.name,
                    "denoise.color",
                    num(&params["color"]).unwrap_or(0.0),
                    auto_param(&auto, "auto_denoise").and_then(|p| num(&p["color"])),
                );
                eprintln!(
                    "DEVIATION {} denoise.detail gold={:.4} auto=0.6（模式常数，非可比）",
                    photo.name,
                    num(&params["detail"]).unwrap_or(0.0)
                );
            }
            if let Some(params) = gold_of("sharpen") {
                deviation_row(
                    photo.name,
                    "sharpen.amount",
                    num(&params["amount"]).unwrap_or(0.0),
                    auto_param(&auto, "auto_sharpen").and_then(|p| num(&p["amount"])),
                );
                eprintln!(
                    "DEVIATION {} sharpen.radius gold={:.4} auto=1（模式常数，非可比）",
                    photo.name,
                    num(&params["radius"]).unwrap_or(0.0)
                );
            }
            if let Some(params) = gold_of("levels") {
                deviation_row(
                    photo.name,
                    "levels.black",
                    num(&params["black"]).unwrap_or(0.0),
                    auto_param(&auto, "auto_levels").and_then(|p| num(&p["black"])),
                );
                deviation_row(
                    photo.name,
                    "levels.white",
                    num(&params["white"]).unwrap_or(0.0),
                    auto_param(&auto, "auto_levels").and_then(|p| num(&p["white"])),
                );
            } else {
                eprintln!(
                    "DEVIATION {} levels 金用 curves 承当色阶（非可比，不强制）",
                    photo.name
                );
            }
            if gold.iter().any(|(name, _)| name == "curves") {
                eprintln!(
                    "DEVIATION {} curves 金有 curves 组，auto 无（非可比）",
                    photo.name
                );
            }
            if !recipe["locals"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .is_empty()
            {
                eprintln!(
                    "DEVIATION {} locals 金含 {} 处局部调整，auto 为全局 only（非可比）",
                    photo.name,
                    recipe["locals"].as_array().map_or(0, Vec::len)
                );
            }
        }));
        if let Err(error) = result {
            let message = error
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| error.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "未知 panic".to_owned());
            failures.push(format!("{}：{message}", photo.name));
        }
    }
    assert!(
        failures.is_empty(),
        "回测触点失败（{} 张）：\n{}",
        failures.len(),
        failures.join("\n")
    );
}
