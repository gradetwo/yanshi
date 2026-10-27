//! 期 4 D1 集成测试：频率分离 / 减淡加深（走真实工具注册表 + 落盘工作区）。
//!
//! 与单元测试的分工：单元测试钉住烘焙公式与自检界；这里钉住工具落成
//! ——组结构、渲染重建、空层拒绝零原子、蒙版全黑行为、灰层填充、幂等。

use serde_json::{json, Value};
use yanshi_core::Bbox;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

/// **⇒ 临时目录必须在测试结束时删掉** ✓（第 930 轮；`check-tests-clean-temp` 抓到的 ✓）
/// **⇒ 为什么** ✗：`/tmp` 是 **1.9 GB tmpfs** ✓ ⇒ 漏删会把 `bash` 都撑死 ✗
pub struct TempCleanup(pub std::path::PathBuf);
impl Drop for TempCleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("yanshi_retouch_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path
}

fn workspace(root: &std::path::Path, doc: &str, width: u32, height: u32) -> Workspace {
    let mut workspace = Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default())
        .expect("落盘工作区应当能建");
    workspace
        .create_document(
            NewDocument::new(doc, width, height),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn call(workspace: &mut Workspace, doc: &str, tool: &str, args: Value) -> Value {
    let mut ctx = ToolContext::new(workspace, doc, "human:1", "session:test").with_owner(true);
    registry().call(&mut ctx, tool, &args)
}

/// 合成源图（64×48，全不透明）：水平渐变 + 高频竖条纹 + 纯色块。
fn synthetic(width: u32, height: u32) -> Vec<u8> {
    let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
    for y in 0..height as usize {
        for x in 0..width as usize {
            let base = (y * width as usize + x) * 4;
            let gradient = (x * 255 / width as usize) as u8;
            let stripe = if (x / 2) % 2 == 0 { 255 } else { 0 };
            let (r, g, b) = if x >= 48 && y >= 32 {
                (200, 30, 30)
            } else if y < 8 {
                (gradient, stripe, 128)
            } else {
                (gradient, 255 - gradient, 90)
            };
            pixels[base] = r;
            pixels[base + 1] = g;
            pixels[base + 2] = b;
            pixels[base + 3] = 255;
        }
    }
    pixels
}

/// 建源图层并导入合成图，返回图层 id。
fn setup_source(workspace: &mut Workspace, doc: &str, width: u32, height: u32) -> String {
    let layer = call(
        workspace,
        doc,
        "create_layer",
        json!({"layer_id": "src", "name": "源图"}),
    );
    assert_eq!(layer["ok"], json!(true), "{layer}");
    let pixels = synthetic(width, height);
    let hash = workspace.put_blob(&pixels).expect("放源图 blob");
    let imported = call(
        workspace,
        doc,
        "import_image",
        json!({
            "layer_id": "src",
            "bitmap": {"blob_hash": hash, "size": pixels.len(), "mime_type": "image/x-yanshi-raw"},
            "region": {"x": 0, "y": 0, "w": width, "h": height},
        }),
    );
    assert_eq!(imported["ok"], json!(true), "{imported}");
    "src".to_owned()
}

fn full_bbox(width: u32, height: u32) -> Bbox {
    Bbox::new(0.0, 0.0, width as f64, height as f64)
}

/// 两段像素**不等**时给一条能看的摘要（通道差个数、单通道最大差、第一处不同）✓。
/// **为什么不用 `assert_eq!(Vec<u8>, Vec<u8>)`** ✗：整幅十进制数组会打进日志 ✓
/// 实测一次失败就喷出 **36 KB** ✗ CI 日志里没人能从中看出"差在哪、差多少"✓。
fn pixel_diff(before: &[u8], after: &[u8]) -> Option<String> {
    if before == after {
        return None;
    }
    let channels = before
        .iter()
        .zip(after.iter())
        .filter(|(a, b)| a != b)
        .count();
    let max_delta = before
        .iter()
        .zip(after.iter())
        .map(|(a, b)| (*a as i32 - *b as i32).abs())
        .max()
        .unwrap_or(0);
    let first = before
        .iter()
        .zip(after.iter())
        .position(|(a, b)| a != b)
        .unwrap_or(0);
    Some(format!(
        "{} 个通道不同｜单通道最大差 {max_delta}｜首处偏移 {first}（像素 {}）：前 {:?} 后 {:?}",
        channels,
        first / 4,
        &before[first..(first + 4).min(before.len())],
        &after[first..(first + 4).min(after.len())]
    ))
}

/// 两段像素在**给定 LSB 界**内是否一致：超界时给一条能看的摘要 ✓（口径同 [`pixel_diff`] ✗）。
///
/// **为什么要有一条"界"版本**（第五轮 ✓ `soft_light` 修好之后实测出来的）：
/// 中性字节 188 的残差在**两条渲染路上不一样** ✗ 拿"逐字节相同"去问其中一条就会假红 ✓：
/// * `sample_colors`（HTTP 那条）在 256 点 × 红色阶底色的扫描上 **187／188 都是 0 残差** ✓
///   而 186／189 各偏 1 ⇒ 188 确实是中性的最近整数 ✓；
/// * `render_region_raw`（引擎渲染那条 ✓ 多走一层瓦片 f16 ✓ 与频率分离那条测试同一现象 ✓）
///   在合成渐变图上 **4096 个通道（32×32×4）里有 912 个偏 1**（例：底色 63 → 64 ✓ 实测 ✓）
///   ⇒ 最大残差仍是 1 LSB ⇒ 界就取 1 ✓ 不取 0 ✓ 也不放大到"看起来差不多"✗。
fn pixel_diff_bounded(before: &[u8], after: &[u8], bound: i32) -> Option<String> {
    if before.len() != after.len() {
        return Some(format!("长度不同：{} vs {}", before.len(), after.len()));
    }
    let over = before
        .iter()
        .zip(after.iter())
        .filter(|(a, b)| (**a as i32 - **b as i32).abs() > bound)
        .count();
    if over == 0 {
        return None;
    }
    let max_delta = before
        .iter()
        .zip(after.iter())
        .map(|(a, b)| (*a as i32 - *b as i32).abs())
        .max()
        .unwrap_or(0);
    let first = before
        .iter()
        .zip(after.iter())
        .position(|(a, b)| (*a as i32 - *b as i32).abs() > bound)
        .unwrap_or(0);
    Some(format!(
        "{over} 个通道超出 {bound} LSB｜单通道最大差 {max_delta}｜首处偏移 {first}（像素 {}）：前 {:?} 后 {:?}",
        first / 4,
        &before[first..(first + 4).min(before.len())],
        &after[first..(first + 4).min(after.len())]
    ))
}

/// 频率分离端到端：组渲染（源图层隐藏后）与源图逐像素比对，
/// 界与单元测试的实测刻画界一致（引擎渲染多走一层瓦片 f16，
/// 故亮区界放宽到 4 LSB——若实测 tighter 再收紧，见注释）。
#[test]
fn frequency_separation_reconstructs_source() {
    let root = temp_dir("fs");
    let _cleanup = TempCleanup(root.clone());
    let doc = "doc_fs";
    let (width, height) = (64, 48);
    let mut workspace = workspace(&root, doc, width, height);
    let src = setup_source(&mut workspace, doc, width, height);
    let (_, _, reference) = workspace
        .render_region_raw_layer(doc, full_bbox(width, height), &src)
        .expect("源图层应当能渲染");
    let result = call(
        &mut workspace,
        doc,
        "frequency_separation",
        json!({"layer_id": src, "radius": 8.0}),
    );
    assert_eq!(result["ok"], json!(true), "{result}");
    let group_id = result["group_id"].as_str().expect("回组 id").to_owned();
    let low_id = result["low_layer_id"].as_str().expect("回低频层 id");
    let high_id = result["high_layer_id"].as_str().expect("回高频层 id");
    // 组结构：组内恰好 4 层（自下而上：低频、低频绘画、高频、高频绘画）。
    let state = workspace.document(doc).unwrap().state().clone();
    let children: Vec<_> = state
        .layers
        .values()
        .filter(|layer| layer.parent_id.as_deref() == Some(group_id.as_str()))
        .collect();
    assert_eq!(children.len(), 4, "组内应恰好 4 层");
    let high_layer = state.layers.get(high_id).expect("高频层存在");
    assert_eq!(
        high_layer.blend_mode, "linear_light",
        "高频层必须是 linear_light"
    );
    assert!(state.layers.contains_key(low_id), "低频层存在");
    // 源图层隐藏后整幅渲染 = 组单独渲染，与源图比对。
    let hidden = call(
        &mut workspace,
        doc,
        "update_layer",
        json!({"layer_id": src, "patch": {"visible": false}}),
    );
    assert_eq!(hidden["ok"], json!(true), "{hidden}");
    let (_, _, rendered) = workspace
        .render_region_raw(doc, full_bbox(width, height))
        .expect("组应当能渲染");
    assert_eq!(rendered.len(), reference.len());
    let mut max_error = 0i32;
    let mut bright_max_error = 0i32;
    let mut sum_error = 0i64;
    for (index, (a, b)) in rendered.iter().zip(reference.iter()).enumerate() {
        // alpha 通道不参与重建比较（组 alpha 规则见工具说明）。
        if index % 4 == 3 {
            continue;
        }
        let error = (*a as i32 - *b as i32).unsigned_abs() as i32;
        max_error = max_error.max(error);
        if *b >= 128 {
            bright_max_error = bright_max_error.max(error);
        }
        sum_error += error as i64;
    }
    let counted = (rendered.len() / 4 * 3) as i64;
    assert!(max_error <= 20, "组渲染最大误差超刻画界：{max_error}");
    assert!(
        bright_max_error <= 4,
        "组渲染中高调最大误差超引擎界：{bright_max_error}"
    );
    assert!(
        sum_error as f64 / counted as f64 <= 3.0,
        "组渲染均值误差超刻画界：{}",
        sum_error as f64 / counted as f64
    );
}

/// 空层调用 ⇒ precondition_failed，且 head_seq 不变（零原子落账）。
/// 变异：错误被吞（返回 ok）⇒ 本条红；原子偷偷落账 ⇒ head_seq 变 ⇒ 红。
#[test]
fn frequency_separation_rejects_empty_layer() {
    let root = temp_dir("fs_empty");
    let _cleanup = TempCleanup(root.clone());
    let doc = "doc_fs_empty";
    let mut workspace = workspace(&root, doc, 32, 32);
    let layer = call(
        &mut workspace,
        doc,
        "create_layer",
        json!({"layer_id": "empty", "name": "空层"}),
    );
    assert_eq!(layer["ok"], json!(true), "{layer}");
    let before = workspace.document(doc).unwrap().state().head_seq;
    let result = call(
        &mut workspace,
        doc,
        "frequency_separation",
        json!({"layer_id": "empty", "radius": 8.0}),
    );
    assert_eq!(result["ok"], json!(false), "空层必须拒绝：{result}");
    assert_eq!(
        result["error_code"],
        json!("precondition_failed"),
        "空层拒绝必须是 precondition_failed：{result}"
    );
    assert!(
        result.to_string().contains("没有可分离的像素"),
        "错误要说清缺什么：{result}"
    );
    let after = workspace.document(doc).unwrap().state().head_seq;
    assert_eq!(
        before, after,
        "拒绝时零原子落账：head_seq {before} → {after}"
    );
}

/// D&B curves 变体：两层、两 curves 对象（EOTF 转换后的中点）、两蒙版全黑
/// （黑蒙版 ⇒ 曲线零生效 ⇒ 整幅渲染 = 源图 ✓ 蒙版确实是闸门）✓
/// **涂开蒙版 ⇒ 画面真的动，而且方向对**（dodge 变亮 ✓ 再开 burn 往回压 ✓ 第六轮起 ✓）。
#[test]
fn dodge_burn_curves_mode_creates_masked_pair() {
    let root = temp_dir("db_curves");
    let _cleanup = TempCleanup(root.clone());
    let doc = "doc_db_curves";
    let (width, height) = (64, 48);
    let mut workspace = workspace(&root, doc, width, height);
    let src = setup_source(&mut workspace, doc, width, height);
    let (_, _, reference) = workspace
        .render_region_raw(doc, full_bbox(width, height))
        .expect("源图应当能渲染");
    let result = call(
        &mut workspace,
        doc,
        "dodge_burn_setup",
        json!({"layer_id": src, "mode": "curves"}),
    );
    assert_eq!(result["ok"], json!(true), "{result}");
    assert_eq!(result["already_exists"], json!(false));
    let layer_ids = result["layer_ids"].as_array().expect("回层 id").len();
    let object_ids = result["object_ids"].as_array().expect("回对象 id").len();
    let mask_ids = result["mask_ids"].as_array().expect("回蒙版 id").len();
    assert_eq!(layer_ids, 2, "curves 变体应建 2 层");
    assert_eq!(object_ids, 2, "curves 变体应建 2 个 curves 对象");
    assert_eq!(mask_ids, 2, "curves 变体应建 2 张蒙版");
    // 对象参数：中点是 EOTF 转换后的线性点（不是显示值原样）。
    let state = workspace.document(doc).unwrap().state().clone();
    let mut mids = Vec::new();
    for object in state.alive_objects() {
        if object.data["adjustment_type"] == json!("curves") {
            let points = object.data["params"]["points"].as_array().unwrap();
            mids.push((
                points[1][0].as_f64().unwrap(),
                points[1][1].as_f64().unwrap(),
            ));
        }
    }
    assert_eq!(mids.len(), 2, "应恰好 2 条 curves 曲线");
    // 显示 0.5 → 线性 0.2140；减淡 0.62 → 0.3423；加深 0.38 → 0.1192（±0.002）。
    for (x, y) in &mids {
        assert!(
            (x - 0.2140).abs() < 0.002,
            "曲线中点 x 应为 decode(0.5)：{x}"
        );
        assert!(
            (y - 0.3423).abs() < 0.002 || (y - 0.1192).abs() < 0.002,
            "曲线中点 y 应为 decode(0.62) 或 decode(0.38)：{y}"
        );
    }
    // 蒙版结构：全画布 rect + invert ⇒ 覆盖率处处为 0，且无涂抹补丁。
    for mask in state.masks.values() {
        if mask.is_deleted() {
            continue;
        }
        assert!(mask.invert, "D&B 蒙版必须 invert（全黑起点）");
        assert!(mask.patches.is_empty(), "新蒙版不应有涂抹补丁");
    }
    // 行为证明：蒙版全黑 ⇒ 曲线零生效 ⇒ 整幅渲染与源图逐字节相同。
    let (_, _, rendered) = workspace
        .render_region_raw(doc, full_bbox(width, height))
        .expect("D&B 组应当能渲染");
    assert_eq!(rendered, reference, "全黑蒙版下曲线必须零生效");
    // **第五轮写的那条"涂白仍零生效"断言，第六轮被上游推翻并翻了过来** ✓：
    //   第五轮实测确实如此（128 的底色涂满白仍 128 ✓）⇒ 那时把"改不到画面"钉成断言 +
    //   回执 `effective: false` ✓ 两条注释里都写了"**谁把跨层调整修好了 ⇒ 这两条会红 ⇒
    //   那时该做的是同时翻断言与撤 effective**"✗ 而不是留一个"成功而画面没动"的静默 ✓。
    //   上游 `302d414d`（纯调整层渲染时用下方内容播种）正是那个人 ✓ ⇒ 下面照约定翻断言 ✓
    //   服务端两条臂同时改回 `effective: true` ✓ 界面那条 ⚠ 也一起撤 ✓（三处缺一处就有一处在说谎 ✓）。
    assert_eq!(
        result["effective"],
        json!(true),
        "curves 路现在改得到画面 ⇒ 回执不许再报 effective=false（报了就是反向谎报）"
    );
    assert_eq!(
        result["ineffective_reason"],
        Value::Null,
        "没有「改不到画面」这回事 ⇒ 不许留一句理由"
    );
    assert_eq!(
        result["neutral_gray"],
        Value::Null,
        "curves 组没有灰层 ⇒ 不该回中性字节"
    );
    let dodge_mask = result["masks"]["dodge"]
        .as_str()
        .expect("dodge 蒙版 id")
        .to_owned();
    let painted = call(
        &mut workspace,
        doc,
        "paint_mask",
        json!({
            "mask_id": dodge_mask,
            "points": [[32.0, 24.0, 1.0]],
            "size": 200.0,
            "opacity": 1.0,
            "softness": 0.0,
        }),
    );
    assert_eq!(painted["ok"], json!(true), "{painted}");
    let (_, _, after_white) = workspace
        .render_region_raw(doc, full_bbox(width, height))
        .expect("涂白后应当能渲染");
    assert!(
        pixel_diff(&reference, &after_white).is_some(),
        "curves 变体把 dodge 蒙版涂满白后整幅合成仍与源图逐字节相同 ⇒ 这条路又改不到画面了 ✗"
    );
    // **只"变了"不够** ✓ 方向必须单独问（第五轮那条教训：中性/不变的断言对方向天然瞎 ✗）。
    // 取 (32, 24)：`synthetic` 在该点是 (127, 128, 90) ⇒ 中调 ⇒ 减淡必须抬得动 ✓。
    let at = |buf: &[u8], x: usize, y: usize| -> i32 {
        let i = (y * width as usize + x) * 4;
        (buf[i] as i32 + buf[i + 1] as i32 + buf[i + 2] as i32) / 3
    };
    let (cx, cy) = (32usize, 24usize);
    let src_lum = at(&reference, cx, cy);
    let dodge_lum = at(&after_white, cx, cy);
    assert!(
        dodge_lum > src_lum,
        "dodge 蒙版涂白 ⇒ 该点必须**变亮**（减淡的方向）：实测 {src_lum} → {dodge_lum}"
    );
    // burn 涂在同一处 ⇒ 必须往回压（"两条曲线方向相反"这件事得在像素上看得见 ✓
    // 否则把两条曲线写成同一条也照样绿 ✓）。
    let burn_mask = result["masks"]["burn"]
        .as_str()
        .expect("burn 蒙版 id")
        .to_owned();
    let burned = call(
        &mut workspace,
        doc,
        "paint_mask",
        json!({
            "mask_id": burn_mask,
            "points": [[32.0, 24.0, 1.0]],
            "size": 200.0,
            "opacity": 1.0,
            "softness": 0.0,
        }),
    );
    assert_eq!(burned["ok"], json!(true), "{burned}");
    let (_, _, after_burn) = workspace
        .render_region_raw(doc, full_bbox(width, height))
        .expect("两张蒙版都涂白后应当能渲染");
    let both_lum = at(&after_burn, cx, cy);
    assert!(
        both_lum < dodge_lum,
        "burn 蒙版也涂白 ⇒ 必须比只涂 dodge 时**更暗**（两条曲线方向相反）：dodge {dodge_lum} → 两条都开 {both_lum}"
    );
}

/// D&B gray 变体：灰层填**本引擎柔光的中性字节 188**、blend_mode=soft_light，
/// 并且**建组前后整幅合成逐字节相同**（这才是"不改变画面"那句话的验收口 ✓）。
#[test]
fn dodge_burn_gray_mode_creates_gray_layer() {
    let root = temp_dir("db_gray");
    let _cleanup = TempCleanup(root.clone());
    let doc = "doc_db_gray";
    let (width, height) = (32, 32);
    let mut workspace = workspace(&root, doc, width, height);
    let src = setup_source(&mut workspace, doc, width, height);
    // 源图是**全值域**的合成图（水平渐变 + 条纹 ✓ `synthetic`）⇒ 中性与否在每个亮度上都要成立 ✓。
    let (_, _, reference) = workspace
        .render_region_raw(doc, full_bbox(width, height))
        .expect("源图应当能渲染");
    let result = call(
        &mut workspace,
        doc,
        "dodge_burn_setup",
        json!({"layer_id": src, "mode": "gray"}),
    );
    assert_eq!(result["ok"], json!(true), "{result}");
    assert_eq!(
        result["effective"],
        json!(true),
        "灰层法必须是有效的减淡加深路"
    );
    assert_eq!(
        result["neutral_gray"],
        json!(188),
        "回执必须念出中性字节：soft_light 的中性是 Cs=0.5（线性）⇒ 编码回显示是 188 ✓ 不是 128 ✗"
    );
    let layer_id = result["layer_ids"][0]
        .as_str()
        .expect("回灰层 id")
        .to_owned();
    let state = workspace.document(doc).unwrap().state().clone();
    let gray_layer = state.layers.get(&layer_id).expect("灰层存在");
    assert_eq!(gray_layer.blend_mode, "soft_light", "灰层必须是 soft_light");
    // **行为断言（第五轮补）**：灰层既然是"中性"的 ⇒ 盖上它以后画面**最多动 1 LSB** ✓。
    // 旧版本在这里铺的是 50% 显示灰（128）✓ 而 128 在本引擎的 soft_light 下是**压暗** ✗
    // 实测 128 的底色被压到 97（**整幅暗 31 级 ✗**）⇒ 这条断言当时没有任何地方守 ✓
    // 而工具自己的 note 明说"soft_light 下不改变画面"✓ 那句话在当时是假的 ✗。
    // **界取 1 而不是 0** 的实测来由见 [`pixel_diff_bounded`] ✓（`sample_colors` 那条路 0 残差 ✓
    // 而引擎渲染那条路（本图 4096 个通道）有 912 个偏 1 ⇒ 最大仍是 1 ✗ 没有更大的 ✓）。
    let (_, _, composed) = workspace
        .render_region_raw(doc, full_bbox(width, height))
        .expect("灰层盖上后应当能渲染");
    assert_eq!(
        pixel_diff_bounded(&reference, &composed, 1),
        None,
        "中性灰层盖上前后整幅合成必须落在 1 LSB 内（超出就说明铺的不是中性字节）"
    );
    // **方向断言（本轮 `soft_light` 修复的守卫）**：`blend.rs` 的提亮支原先把 W3C 三次式
    // `((16Cb−12)Cb+4)Cb` 写成 `((16Cb−12)·√Cb+4)Cb` ✗ ⇒ 在 `Cb ≤ 0.25`（编码 ≤188 ⇒ 几乎整个中间调）
    // 上**白顶把画面压暗**：实测底色 128 铺白给 **19**（应是 ~182 ✗ 差 163 级 ✓）
    // ⇒ "涂白提亮"这句功能承诺在中暗调上是反的 ✗ 而上面那条"中性盖上不变画面"**看不见这件事** ✓
    // （中性走 `Cs ≤ 0.5` 那条支 ✗ 那条支本来就对 ✓）⇒ 必须另问"白顶往哪边走"✓。
    // 底色取合成图的渐变红道：x=2（gradient≈15 ✓ 走多项式支 ⇒ 旧 bug 在这里反号 ✓）
    // 与 x=26（gradient≈207 ✓ 走平方根支 ✓）⇒ 两支都被问到 ✓。
    for (label, x) in [("暗端", 2u32), ("亮端", 26u32)] {
        let white = call(
            &mut workspace,
            doc,
            "fill_region",
            json!({
                "layer_id": layer_id,
                "shape": { "type": "rect", "x": x, "y": 1.0, "w": 2.0, "h": 2.0 },
                "color": { "r": 255, "g": 255, "b": 255, "a": 255 },
            }),
        );
        assert_eq!(white["ok"], json!(true), "{label} 铺白失败：{white}");
    }
    let dark_fill = call(
        &mut workspace,
        doc,
        "fill_region",
        json!({
            "layer_id": layer_id,
            "shape": { "type": "rect", "x": 14.0, "y": 1.0, "w": 2.0, "h": 2.0 },
            "color": { "r": 0, "g": 0, "b": 0, "a": 255 },
        }),
    );
    assert_eq!(dark_fill["ok"], json!(true), "铺黑失败：{dark_fill}");
    let (_, _, after_paint) = workspace
        .render_region_raw(doc, full_bbox(width, height))
        .expect("涂抹后应当能渲染");
    let red = |buf: &[u8], x: u32, y: u32| buf[((y * width + x) as usize) * 4] as i32;
    for x in [2u32, 26u32] {
        let before_px = red(&composed, x, 2);
        let after_px = red(&after_paint, x, 2);
        assert!(
            after_px > before_px,
            "灰层上铺白必须**提亮**（x={x} 底色 {before_px} ⇒ 实测 {after_px}；\
             若变暗 ⇒ `blend.rs` 的 soft_light 提亮支又写错了 ✗ 本轮实测的旧值是 128→19）"
        );
    }
    let dark_before = red(&composed, 14, 2);
    let dark_after = red(&after_paint, 14, 2);
    assert!(
        dark_after < dark_before,
        "灰层上铺黑必须**压暗**（底色 {dark_before} ⇒ 实测 {dark_after}）"
    );
    // 填充值断言不能直接单层渲染：单层渲染会把 soft_light 合成到底色上
    // （188 是中性 ⇒ 结果 = 底色 ✓ 看不出铺的是多少 ✓）。改走行为断言：
    // 先把 blend 切回 normal 再单层渲染 ⇒ 此时除刚涂的两处笔触外都应是 188 灰
    //（**这一条放在涂抹之后** ✓ 断言只查**没被碰过**的像素 ✗ 不假装整层都是 188）。
    let unblended = call(
        &mut workspace,
        doc,
        "update_layer",
        json!({"layer_id": layer_id, "patch": {"blend_mode": "normal"}}),
    );
    assert_eq!(unblended["ok"], json!(true), "{unblended}");
    let (_, _, pixels) = workspace
        .render_region_raw_layer(doc, full_bbox(width, height), &layer_id)
        .expect("灰层应当能渲染");
    let painted = |x: u32, y: u32| {
        ((2..4).contains(&x) || (14..16).contains(&x) || (26..28).contains(&x))
            && (1..3).contains(&y)
    };
    let untouched_bad = pixels
        .chunks_exact(4)
        .enumerate()
        .filter(|(i, _)| !painted((i % width as usize) as u32, (i / width as usize) as u32))
        .find(|(_, p)| *p != [188, 188, 188, 255]);
    assert!(
        untouched_bad.is_none(),
        "灰层未被涂抹的像素都应是中性字节 188 不透明，实测 {:?}",
        untouched_bad.map(|(i, p)| (i / width as usize, i % width as usize, p))
    );
}

/// D&B 幂等：二次调用返回 already_exists 且 head_seq 不变、id 相同。
#[test]
fn dodge_burn_setup_is_idempotent() {
    let root = temp_dir("db_idem");
    let _cleanup = TempCleanup(root.clone());
    let doc = "doc_db_idem";
    let mut workspace = workspace(&root, doc, 32, 32);
    let src = setup_source(&mut workspace, doc, 32, 32);
    let first = call(
        &mut workspace,
        doc,
        "dodge_burn_setup",
        json!({"layer_id": src, "mode": "curves"}),
    );
    assert_eq!(first["ok"], json!(true), "{first}");
    let before = workspace.document(doc).unwrap().state().head_seq;
    let second = call(
        &mut workspace,
        doc,
        "dodge_burn_setup",
        json!({"layer_id": src, "mode": "curves"}),
    );
    assert_eq!(second["ok"], json!(true), "{second}");
    assert_eq!(
        second["already_exists"],
        json!(true),
        "二次调用必须 already_exists"
    );
    assert_eq!(
        second["group_id"], first["group_id"],
        "二次调用必须回同一组"
    );
    assert_eq!(
        second["layer_ids"], first["layer_ids"],
        "二次调用必须回同一层"
    );
    // **复用路径也得给 `mode`** ✓（第五轮实测：这一格整格漏掉 ✗ 于是界面上念出
    // `方法 undefined`／英文 `method undefined` ✓ 而 `tool-dodge-burn-masks` 那种"看字段在不在"
    // 的判据只查 `masks` ✓ 抓不到 ✗ ⇒ 补在**行为断言**上而不是形状上 ✓）。
    assert_eq!(
        second["mode"],
        json!("curves"),
        "复用回执必须带上模式（本轮实测漏过 ⇒ 界面念 undefined）"
    );
    let after = workspace.document(doc).unwrap().state().head_seq;
    assert_eq!(before, after, "幂等复用时零原子落账");
}

/// **复用回执念的是「这个组实际是哪条路建的」**，不是本次请求想要的那条（第五轮）
///
/// **⇒ 缺陷**：旧实现在复用分支上把**请求的** `mode` 抄进 `effective` / `neutral_gray` ✗
///   于是「先 `gray` 建组、再拿同名请求 `curves`」这一步会回
///   `effective:false / neutral_gray:null` ✗ **而磁盘上那张 `soft_light` 灰层明明改得到画面** ✓
///   ⇒ 回执与事实相反（说它不动、其实动 ✓）＝ 本项目禁止的那种静默 ✗
///   而且这条**只有拿两个不同模式打同一个组名才看得见** ✓ 既有的幂等测试两次都打 `curves` ✗
///   ⇒ 看不见 ✓（`tool-dodge-burn-effective.mjs` 的 C 臂同样两次同模式 ✗ 也看不见 ✓）。
/// **⇒ 判据**：① 灰组复用 ⇒ `mode == "gray"` ∧ `effective == true` ∧ `neutral_gray == 188` ✓
///   即使本次请求 `curves` ✓；`note` 必须**明说**这次请求与既有组不一致 ✓；
///   ② 分类来自图层本身（那个孩子的 `blend_mode` 确实是 `soft_light` ✓ 不是照抄请求 ✓）；
///   ③ 把灰层删掉、组名还活着 ⇒ 说不出是哪条路 ⇒ `mode: null` ∧ `effective == false`
///   ∧ `ineffective_reason` 说清"组里没有减淡加深的层了"✗ 不许假装成 `curves` ✓；
///   ④ 全程零原子落账 ✓ 组 id 不变 ✓。
/// **⇒ 变异**：把 `mode: reused_mode` 改回 `mode: mode` ⇒ ① 红 ✓；
///   把 `effective` 写成 `mode != "curves"`（照请求算）⇒ ① 红 ✓；
///   删掉 ③ 那档（`else { None }` 写成 `Some("curves")`）⇒ ③ 红 ✓。
#[test]
fn dodge_burn_setup_reuse_reports_the_existing_group_mode() {
    let root = temp_dir("db_reuse_mode");
    let _cleanup = TempCleanup(root.clone());
    let doc = "doc_db_reuse_mode";
    let mut workspace = workspace(&root, doc, 32, 32);
    let src = setup_source(&mut workspace, doc, 32, 32);
    let built = call(
        &mut workspace,
        doc,
        "dodge_burn_setup",
        json!({"layer_id": src, "mode": "gray", "name": "mix"}),
    );
    assert_eq!(built["ok"], json!(true), "{built}");
    assert_eq!(built["mode"], json!("gray"), "新建那次就该回 gray");
    let group_id = built["group_id"].as_str().unwrap().to_owned();
    let gray_id = built["layer_ids"][0].as_str().unwrap().to_owned();
    let head_before = workspace.document(doc).unwrap().state().head_seq;
    // ① 同名再请求 **另一条路** ⇒ 必须老实说"用的是既有那个 gray 组"✓。
    let reused = call(
        &mut workspace,
        doc,
        "dodge_burn_setup",
        json!({"layer_id": src, "mode": "curves", "name": "mix"}),
    );
    assert_eq!(reused["already_exists"], json!(true), "{reused}");
    assert_eq!(
        reused["mode"],
        json!("gray"),
        "既有组是灰层法建的 ⇒ 回执必须念 gray，不能照抄本次请求的 curves（旧写法在这里回 curves ✗）"
    );
    assert_eq!(
        reused["effective"],
        json!(true),
        "灰层法改得到画面 ⇒ 复用那次不许因为它被请求成 curves 就说它不动"
    );
    assert_eq!(
        reused["neutral_gray"],
        json!(188),
        "复用回执的中性字节要跟着既有组走"
    );
    let note = reused["note"].as_str().unwrap_or("");
    assert!(
        note.contains("本次请求的是 mode=curves") && note.contains("gray"),
        "note 必须明说这次请求与既有组不一致，实测：{note}"
    );
    // ② 分类依据是图层本身 ✓（不是请求参数 ✓）。
    let state = workspace.document(doc).unwrap().state().clone();
    assert_eq!(
        state.layers.get(&gray_id).unwrap().blend_mode,
        "soft_light",
        "那个孩子本来就该是 soft_light（回执念 gray 的事实依据）"
    );
    assert_eq!(reused["group_id"], json!(group_id), "复用必须回同一个组");
    assert_eq!(
        workspace.document(doc).unwrap().state().head_seq,
        head_before,
        "复用路径零原子落账"
    );
    // ③ 组名还在、里面的灰层被删了 ⇒ **说不出是哪条路** ⇒ 老实回 null + 改不到画面。
    let removed = call(
        &mut workspace,
        doc,
        "delete_layer",
        json!({"layer_id": gray_id}),
    );
    assert_eq!(removed["ok"], json!(true), "{removed}");
    let hollow = call(
        &mut workspace,
        doc,
        "dodge_burn_setup",
        json!({"layer_id": src, "mode": "gray", "name": "mix"}),
    );
    assert_eq!(hollow["already_exists"], json!(true), "{hollow}");
    assert_eq!(
        hollow["mode"],
        json!(null),
        "组里既没有 soft_light 灰层也没有那两条曲线层 ⇒ 不能假装知道是哪条路建的"
    );
    assert_eq!(
        hollow["effective"],
        json!(false),
        "空组不会改变画面 ⇒ 必须说 false（旧写法会照请求念 gray 并说 true ✗）"
    );
    let reason = hollow["ineffective_reason"].as_str().unwrap_or("");
    assert!(
        reason.contains("没有减淡加深") || reason.contains("已经没有"),
        "要给出为什么说不出来，实测：{reason}"
    );
    assert_eq!(
        hollow["neutral_gray"],
        json!(null),
        "说不出模式就不许编一个中性字节出来"
    );
}

/// **P2-10：`dodge_burn_setup` 的 `mask_ids` 标注 dodge/burn 角色**（期 2 块 2）
///
/// **⇒ 缺陷**：`mask_ids` 只有数组、无角色——调用方只能按 `[dodge, burn]` 顺序猜 ✗；
///   复用路径上数组还是按 id 排过的集合，顺序更不可靠 ✗。
///
/// **⇒ 判据**：新建 curves 组 ⇒ `masks.dodge == mask_ids[0]`、`masks.burn == mask_ids[1]` ✓，
///   且按 `mask_id` 找到的图层名字对得上（dodge ⇒「减淡 · Dodge」）✓；
///   二次调用（already_exists）⇒ 同一张映射 ✓；gray 组 ⇒ 两格都是 null ✓。
/// **⇒ 变异**：`masks` 的两格对调 ⇒ 名字断言红 ✓；复用路径不给 `masks` ⇒ 二次调用断言红 ✓。
#[test]
fn dodge_burn_setup_masks_label_roles() {
    let root = temp_dir("db_masks");
    let _cleanup = TempCleanup(root.clone());
    let doc = "doc_db_masks";
    let mut ws = workspace(&root, doc, 64, 48);
    let src = setup_source(&mut ws, doc, 64, 48);
    let first = call(
        &mut ws,
        doc,
        "dodge_burn_setup",
        json!({"layer_id": src, "mode": "curves"}),
    );
    assert_eq!(first["ok"], json!(true), "{first}");
    let mask_ids = first["mask_ids"].as_array().expect("回蒙版 id");
    assert_eq!(mask_ids.len(), 2, "curves 变体应建 2 张蒙版：{first}");
    assert_eq!(
        first["masks"]["dodge"], mask_ids[0],
        "masks.dodge 必须等于 mask_ids[0]：{first}"
    );
    assert_eq!(
        first["masks"]["burn"], mask_ids[1],
        "masks.burn 必须等于 mask_ids[1]：{first}"
    );
    // 角色必须名副其实：按 mask_id 找到图层，名字对得上。
    let state = ws.document(doc).unwrap().state().clone();
    let layer_named = |mask: &str| {
        state
            .layers
            .values()
            .find(|layer| layer.mask_id.as_deref() == Some(mask))
            .map(|layer| layer.name.clone())
    };
    assert_eq!(
        layer_named(first["masks"]["dodge"].as_str().unwrap()),
        Some("减淡 · Dodge".to_owned()),
        "masks.dodge 指的图层必须是减淡层"
    );
    assert_eq!(
        layer_named(first["masks"]["burn"].as_str().unwrap()),
        Some("加深 · Burn".to_owned()),
        "masks.burn 指的图层必须是加深层"
    );
    // 复用路径给出同一张映射（按名认角色，不靠数组顺序）。
    let second = call(
        &mut ws,
        doc,
        "dodge_burn_setup",
        json!({"layer_id": src, "mode": "curves"}),
    );
    assert_eq!(second["already_exists"], json!(true), "{second}");
    assert_eq!(
        second["masks"], first["masks"],
        "复用路径的 masks 必须与新建时一致：{second}"
    );
    // gray 变体没有蒙版 ⇒ 两格 null（另起根目录，免得两个落盘工作区共用一处）。
    let gray_root = temp_dir("db_masks_gray");
    let _cleanup_gray = TempCleanup(gray_root.clone());
    let gray_doc = "doc_db_masks_gray";
    let mut gray_workspace = workspace(&gray_root, gray_doc, 32, 32);
    let gray_src = setup_source(&mut gray_workspace, gray_doc, 32, 32);
    let gray = call(
        &mut gray_workspace,
        gray_doc,
        "dodge_burn_setup",
        json!({"layer_id": gray_src, "mode": "gray"}),
    );
    assert_eq!(gray["ok"], json!(true), "{gray}");
    assert_eq!(
        gray["masks"]["dodge"],
        json!(null),
        "gray 组没有 dodge 蒙版：{gray}"
    );
    assert_eq!(
        gray["masks"]["burn"],
        json!(null),
        "gray 组没有 burn 蒙版：{gray}"
    );
}
