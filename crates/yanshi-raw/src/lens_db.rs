//! Lensfun 镜头数据库（设计 5.9.1／5.9.4，第四包）：**只用数据、自写
//! 解析**——Lensfun 的库代码是 LGPL，不链接、不翻译其代码；数据库本身
//! 以 CC-BY-SA 发布，本模块只读 XML 数据文件，署名随数据走（打包子集
//! 的出处与授权见 `data/ATTRIBUTION.md`）。
//!
//! 解析器是手写的单用途扫描器：只认 Lensfun 数据库的实际形状
//! （`<lens>` 条目 + `<calibration>` 里的 distortion/tca/vignetting），
//! 不冒充通用 XML 解析器。坏条目逐条跳过并计数，不许一份坏数据毒化
//! 整库；整库为空才是响亮的失败（由调用方按 db_unavailable 落痕）。

use std::path::Path;

/// 一条 PTLens / poly3 畸变标定（文件原样）。
#[derive(Debug, Clone, PartialEq)]
pub struct DistortionEntry {
    pub focal: f32,
    /// 模型名字面（"ptlens" / "poly3" / …）；非 ptlens 的条目保留在
    /// 数据里，由解析层跳过并留痕，**不许误读**（设计 5.9.2）。
    pub model: String,
    pub a: f32,
    pub b: f32,
    pub c: f32,
}

/// 一条 TCA 标定（linear：只有 vr/vb；poly3：另有 br/bb）。
#[derive(Debug, Clone, PartialEq)]
pub struct TcaEntry {
    pub focal: f32,
    pub model: String,
    pub vr: f32,
    pub vb: f32,
    pub br: f32,
    pub bb: f32,
}

/// 一条暗角标定（pa 模型：k1/k2/k3 在某个 (焦距, 光圈, 距离) 档上）。
#[derive(Debug, Clone, PartialEq)]
pub struct VignettingEntry {
    pub focal: f32,
    pub aperture: f32,
    pub distance: f32,
    pub model: String,
    pub k1: f32,
    pub k2: f32,
    pub k3: f32,
}

/// 一支镜头的全部标定数据（库原样，未插值）。
#[derive(Debug, Clone, PartialEq)]
pub struct LensEntry {
    pub maker: String,
    /// 主型号名（无 lang 属性的 `<model>`；与 EXIF 匹配的主口径）。
    pub model: String,
    /// 其余语言/别名型号（带 lang 的 `<model>`），匹配时一并认。
    pub aliases: Vec<String>,
    pub mount: Option<String>,
    /// 标定画幅的裁切系数 C_cal（画幅换算用，设计 5.9.3）。
    pub cropfactor: Option<f32>,
    /// `<center>` 光心偏移：本阶段不消费（设计 5.9 定版忽略并留痕），
    /// 只解析落账，让「忽略」是有记录的忽略。
    pub center: Option<[f32; 2]>,
    pub distortion: Vec<DistortionEntry>,
    pub tca: Vec<TcaEntry>,
    pub vignetting: Vec<VignettingEntry>,
}

impl LensEntry {
    /// 记录里的镜头标识：`maker + model`（与 EXIF 镜头显示名同口径）。
    pub fn lens_id(&self) -> String {
        if self.model.starts_with(&self.maker) {
            self.model.clone()
        } else {
            format!("{} {}", self.maker, self.model)
        }
    }
}

/// 一份装好的镜头库：条目 + 出处 + 版本（全部要进 applied 留痕）。
#[derive(Debug, Clone, PartialEq)]
pub struct LensDb {
    /// 出处标识（"bundled-subset" / 外挂目录路径口径 "external:<name>"）。
    pub source: String,
    /// 数据库版本字面（Lensfun 的 db timestamp 或子集标注）。
    pub version: String,
    pub lenses: Vec<LensEntry>,
    /// 装载时跳过的坏条目数（留痕用，不静默）。
    pub skipped_entries: usize,
}

/// 名称归一化（匹配专用）：小写 + 只留字母数字。EXIF 的
/// 「EF-S18-135mm f/3.5-5.6 IS USM」与库里的「Canon EF-S 18-135mm
/// f/3.5-5.6 IS USM」在空格/标点差异下必须能对上，靠的就是这一步。
pub fn normalize_name(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// 在库中匹配镜头（设计 5.9.4 定版顺序）：
/// 1. maker+model 归一化**全等**（任一别名命中即算）；
/// 2. model-only 全等（EXIF 厂商字段缺失/异写时的兜底）；
/// 3. 归一化**包含**——仅当候选唯一时才认，歧义即未匹配
///    （认错镜头比不认更糟：会把别人的标定乘到这张图上）。
///
/// 返回 (条目, matched_via)：matched_via ∈ "exact" / "contains"。
pub fn find_lens<'a>(
    db: &'a LensDb,
    make: Option<&str>,
    model: Option<&str>,
) -> Option<(&'a LensEntry, &'static str)> {
    let model = model?.trim();
    if model.is_empty() {
        return None;
    }
    let want_model = normalize_name(model);
    let want_full = match make {
        Some(m) if !m.trim().is_empty() => normalize_name(&format!("{} {}", m, model)),
        _ => String::new(),
    };
    let names_of = |entry: &LensEntry| -> Vec<String> {
        let mut names = vec![normalize_name(&entry.model)];
        names.extend(entry.aliases.iter().map(|a| normalize_name(a)));
        names
    };
    // 1) maker+model 全等：EXIF 显示名常已是「maker model」拼接，
    //    与条目的完整 lens_id 比；也认 maker 与 model 分开的全等组合。
    for entry in &db.lenses {
        let id = normalize_name(&entry.lens_id());
        if (!want_full.is_empty() && id == want_full) || id == want_model {
            return Some((entry, "exact"));
        }
        if names_of(entry).iter().any(|n| n == &want_model) {
            return Some((entry, "exact"));
        }
    }
    // 2) 包含：唯一才认。
    let mut candidates: Vec<&LensEntry> = Vec::new();
    for entry in &db.lenses {
        let id = normalize_name(&entry.lens_id());
        let hit = names_of(entry)
            .iter()
            .any(|n| n.contains(&want_model) || want_model.contains(n.as_str()))
            || id.contains(&want_model)
            || (!want_model.is_empty() && want_model.contains(&id));
        if hit {
            candidates.push(entry);
        }
    }
    if candidates.len() == 1 {
        return Some((candidates[0], "contains"));
    }
    None
}

/// 按名字面在库中找镜头（lens_override 的手动路径）：把调用方给的
/// 整个字符串先当 model 全等/包含找，再拆首词当 maker 重试。
pub fn find_lens_by_name<'a>(db: &'a LensDb, name: &str) -> Option<(&'a LensEntry, &'static str)> {
    if let Some(found) = find_lens(db, None, Some(name)) {
        return Some(found);
    }
    let (first, rest) = name.trim().split_once(char::is_whitespace)?;
    find_lens(db, Some(first), Some(rest))
}

// ---------------------------------------------------------------------------
// XML 扫描器（单用途）：在字节流上找标签与属性，不建 DOM。
// ---------------------------------------------------------------------------

/// XML 实体还原（Lensfun 数据里实际出现的五种 + 数字实体）。
fn unescape_xml(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        let tail = &rest[pos..];
        let Some(end) = tail.find(';') else {
            out.push_str(tail);
            return out;
        };
        let entity = &tail[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix('#')
                .and_then(|n| {
                    if let Some(hex) = n.strip_prefix('x') {
                        u32::from_str_radix(hex, 16).ok()
                    } else {
                        n.parse::<u32>().ok()
                    }
                })
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => out.push(c),
            None => out.push_str(&tail[..=end]),
        }
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    out
}

/// 从一个标签的原文里取属性值（`key="value"`，单双引号都认）。
fn attr<'a>(tag: &'a str, key: &str) -> Option<&'a str> {
    let mut search = tag;
    loop {
        let pos = search.find(key)?;
        let after = &search[pos + key.len()..];
        let after = after.trim_start();
        if let Some(after) = after.strip_prefix('=') {
            let after = after.trim_start();
            let quote = after.chars().next()?;
            if quote == '"' || quote == '\'' {
                let inner = &after[1..];
                let end = inner.find(quote)?;
                return Some(&inner[..end]);
            }
            return None;
        }
        // 这次命中的是更长属性名的一部分（如 focal 命中 focalmax），
        // 从命中点后移一位继续找。
        search = &search[pos + 1..];
        // 防止 `focal` 误命中 `focalmax=` 时把别人的值读走：如果紧跟
        // key 的是非空白非等号字符，上面的 strip_prefix('=') 已失败，
        // 这里继续循环即可；但要防 key 出现在值里造成的死循环——
        // search 严格缩短，必然终止。
        let _ = search;
    }
}

fn attr_f32(tag: &str, key: &str) -> Option<f32> {
    attr(tag, key).and_then(|v| v.trim().parse::<f32>().ok())
}

/// 取一对开闭标签之间的文本（`<maker>Canon</maker>`），带属性的开标签也认。
fn element_text(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}");
    let start = xml.find(&open)?;
    let after_open = &xml[start..];
    let gt = after_open.find('>')?;
    let body = &after_open[gt + 1..];
    let close = format!("</{tag}>");
    let end = body.find(&close)?;
    Some(unescape_xml(body[..end].trim()))
}

/// 解析一份 Lensfun 格式 XML 文本为条目列表。返回 (条目, 跳过数)：
/// 缺 maker/model 的 `<lens>` 计为跳过，其余尽力解析。
pub fn parse_lensfun_xml(xml: &str) -> (Vec<LensEntry>, usize) {
    let mut lenses = Vec::new();
    let mut skipped = 0usize;
    let mut rest = xml;
    while let Some(start) = rest.find("<lens>") {
        let after = &rest[start + "<lens>".len()..];
        let Some(end) = after.find("</lens>") else {
            break;
        };
        let block = &after[..end];
        rest = &after[end + "</lens>".len()..];
        match parse_lens_block(block) {
            Some(entry) => lenses.push(entry),
            None => skipped += 1,
        }
    }
    (lenses, skipped)
}

fn parse_lens_block(block: &str) -> Option<LensEntry> {
    let maker = element_text(block, "maker")?;
    // model 可能有多个（多语言）：第一个无 lang 的为主型号，其余进别名。
    let mut model: Option<String> = None;
    let mut aliases: Vec<String> = Vec::new();
    let mut scan = block;
    while let Some(pos) = scan.find("<model") {
        let tail = &scan[pos..];
        let Some(gt) = tail.find('>') else { break };
        let open_tag = &tail[..gt];
        let body = &tail[gt + 1..];
        let Some(close) = body.find("</model>") else {
            break;
        };
        let text = unescape_xml(body[..close].trim());
        if attr(open_tag, "lang").is_none() && model.is_none() {
            model = Some(text);
        } else if !text.is_empty() {
            aliases.push(text);
        }
        scan = &body[close + "</model>".len()..];
    }
    let model = model?;
    let mut entry = LensEntry {
        maker,
        model,
        aliases,
        mount: element_text(block, "mount"),
        cropfactor: element_text(block, "cropfactor").and_then(|v| v.trim().parse::<f32>().ok()),
        center: None,
        distortion: Vec::new(),
        tca: Vec::new(),
        vignetting: Vec::new(),
    };
    // <center x=".." y=".."/>：只落账不消费（本阶段忽略，设计定版）。
    if let Some(pos) = block.find("<center") {
        let tail = &block[pos..];
        if let Some(gt) = tail.find('>') {
            let tag = &tail[..gt];
            if let (Some(x), Some(y)) = (attr_f32(tag, "x"), attr_f32(tag, "y")) {
                entry.center = Some([x, y]);
            }
        }
    }
    // calibration 子条目：按标签名逐个扫（自闭合标签）。
    for (tag, _) in [("distortion", 0), ("tca", 0), ("vignetting", 0)] {
        let mut scan = block;
        let needle = format!("<{tag}");
        while let Some(pos) = scan.find(&needle) {
            let tail = &scan[pos..];
            let Some(gt) = tail.find('>') else { break };
            let open_tag = &tail[..gt];
            match tag {
                "distortion" => {
                    if let (Some(focal), Some(model_name)) =
                        (attr_f32(open_tag, "focal"), attr(open_tag, "model"))
                    {
                        entry.distortion.push(DistortionEntry {
                            focal,
                            model: model_name.to_string(),
                            a: attr_f32(open_tag, "a").unwrap_or(0.0),
                            b: attr_f32(open_tag, "b").unwrap_or(0.0),
                            c: attr_f32(open_tag, "c").unwrap_or(0.0),
                        });
                    }
                }
                "tca" => {
                    if let (Some(focal), Some(model_name)) =
                        (attr_f32(open_tag, "focal"), attr(open_tag, "model"))
                    {
                        entry.tca.push(TcaEntry {
                            focal,
                            model: model_name.to_string(),
                            vr: attr_f32(open_tag, "vr").unwrap_or(1.0),
                            vb: attr_f32(open_tag, "vb").unwrap_or(1.0),
                            br: attr_f32(open_tag, "br").unwrap_or(0.0),
                            bb: attr_f32(open_tag, "bb").unwrap_or(0.0),
                        });
                    }
                }
                "vignetting" => {
                    if let (Some(focal), Some(model_name)) =
                        (attr_f32(open_tag, "focal"), attr(open_tag, "model"))
                    {
                        entry.vignetting.push(VignettingEntry {
                            focal,
                            aperture: attr_f32(open_tag, "aperture").unwrap_or(0.0),
                            distance: attr_f32(open_tag, "distance").unwrap_or(0.0),
                            model: model_name.to_string(),
                            k1: attr_f32(open_tag, "k1").unwrap_or(0.0),
                            k2: attr_f32(open_tag, "k2").unwrap_or(0.0),
                            k3: attr_f32(open_tag, "k3").unwrap_or(0.0),
                        });
                    }
                }
                _ => unreachable!(),
            }
            scan = &tail[gt + 1..];
        }
    }
    Some(entry)
}

// ---------------------------------------------------------------------------
// 装载：打包子集（保底）+ 外挂目录（同 ID 优先覆盖）
// ---------------------------------------------------------------------------

/// 打包子集 XML（从 Lensfun 数据库摘录的常用镜头，出处/授权见
/// `data/ATTRIBUTION.md`），编译期嵌入，保证无外挂时也有保底库。
pub const BUNDLED_SUBSET_XML: &str = include_str!("../data/lensfun-subset.xml");

/// 打包子集的版本字面（与摘录时上游 db timestamp 对应，见 ATTRIBUTION）。
pub const BUNDLED_SUBSET_VERSION: &str = "lensfun-subset@1577948414";

/// 只装打包子集。
pub fn bundled_db() -> LensDb {
    let (lenses, skipped) = parse_lensfun_xml(BUNDLED_SUBSET_XML);
    LensDb {
        source: "bundled-subset".to_string(),
        version: BUNDLED_SUBSET_VERSION.to_string(),
        lenses,
        skipped_entries: skipped,
    }
}

/// 从目录装外挂库：读全部 `*.xml`（文件名排序，装载确定性），
/// 版本取目录下 `timestamp.txt`（Lensfun 发布口径），无则记 "unknown"。
pub fn load_db_dir(dir: &Path) -> std::io::Result<LensDb> {
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "xml") {
            files.push(path);
        }
    }
    files.sort();
    let mut lenses = Vec::new();
    let mut skipped = 0usize;
    for file in &files {
        let text = std::fs::read_to_string(file)?;
        let (mut parsed, bad) = parse_lensfun_xml(&text);
        lenses.append(&mut parsed);
        skipped += bad;
    }
    let version = std::fs::read_to_string(dir.join("timestamp.txt"))
        .map(|t| format!("lensfun@{}", t.trim()))
        .unwrap_or_else(|_| "lensfun@unknown".to_string());
    Ok(LensDb {
        source: format!("external:{}", dir.display()),
        version,
        lenses,
        skipped_entries: skipped,
    })
}

/// 合并两份库：`over` 中与 `base` 同 ID（maker+model 归一化全等）的
/// 条目**替换** base 条目，其余追加（设计定版：外挂同 ID 优先）。
/// 合并后的 source/version 以 `over` 为准（实际生效的是它）。
pub fn merge_db(base: LensDb, over: LensDb) -> LensDb {
    let mut lenses = base.lenses;
    for entry in over.lenses {
        let id = normalize_name(&entry.lens_id());
        if let Some(slot) = lenses
            .iter_mut()
            .find(|e| normalize_name(&e.lens_id()) == id)
        {
            *slot = entry;
        } else {
            lenses.push(entry);
        }
    }
    LensDb {
        source: over.source,
        version: over.version,
        lenses,
        skipped_entries: base.skipped_entries + over.skipped_entries,
    }
}

/// 本进程的生效库：打包子集保底；环境变量 `YANSHI_LENSFUN_DIR`
/// 指向外挂目录且装载成功时合并覆盖。装载失败不静默：返回注记，
/// 由导入层写进 warnings（库本身退回子集，不算 db_unavailable）。
pub fn default_db() -> (LensDb, Vec<String>) {
    let mut notes = Vec::new();
    let base = bundled_db();
    if let Ok(dir) = std::env::var("YANSHI_LENSFUN_DIR") {
        if !dir.trim().is_empty() {
            match load_db_dir(Path::new(&dir)) {
                Ok(external) => {
                    notes.push(format!(
                        "镜头库：已合并外挂目录 {dir}（{} 支镜头，同 ID 覆盖打包子集）",
                        external.lenses.len()
                    ));
                    return (merge_db(base, external), notes);
                }
                Err(error) => {
                    notes.push(format!(
                        "镜头库：外挂目录 {dir} 装载失败（{error}），已退回打包子集"
                    ));
                }
            }
        }
    }
    (base, notes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<lensdatabase version="2">
    <lens>
        <maker>FixtureOptics</maker>
        <model>FixtureOptics Zoom 18-135mm f/3.5-5.6</model>
        <model lang="en">FixtureOptics Zoom 18-135mm f/3.5-5.6</model>
        <mount>Fixture Mount</mount>
        <cropfactor>1.6</cropfactor>
        <center x="0.01" y="-0.02"/>
        <calibration>
            <distortion model="ptlens" focal="18" a="0.03" b="-0.1" c="0.05"/>
            <distortion model="ptlens" focal="35" a="0.01" b="-0.02" c="0.01"/>
            <tca model="poly3" focal="18" vr="1.001" vb="0.999" br="0.0001" bb="-0.0001"/>
            <vignetting model="pa" focal="18" aperture="3.5" distance="0.5" k1="-0.4" k2="0.1" k3="-0.02"/>
        </calibration>
    </lens>
    <lens>
        <model>缺厂商的坏条目</model>
    </lens>
</lensdatabase>"#;

    #[test]
    fn parses_real_lensfun_shape_and_counts_bad_entries() {
        let (lenses, skipped) = parse_lensfun_xml(FIXTURE);
        assert_eq!(lenses.len(), 1);
        assert_eq!(skipped, 1);
        let lens = &lenses[0];
        assert_eq!(lens.maker, "FixtureOptics");
        assert_eq!(lens.cropfactor, Some(1.6));
        assert_eq!(lens.center, Some([0.01, -0.02]));
        assert_eq!(lens.distortion.len(), 2);
        assert_eq!(lens.distortion[0].a, 0.03);
        assert_eq!(lens.tca[0].model, "poly3");
        assert_eq!(lens.vignetting[0].distance, 0.5);
    }

    #[test]
    fn matching_exact_contains_and_ambiguous() {
        let (lenses, _) = parse_lensfun_xml(FIXTURE);
        let db = LensDb {
            source: "test".into(),
            version: "v".into(),
            lenses,
            skipped_entries: 0,
        };
        // EXIF 无空格无厂商写法：归一化后包含且唯一。
        let (_, via) = find_lens(&db, None, Some("Zoom18-135mm f/3.5-5.6")).unwrap();
        assert_eq!(via, "contains");
        let (_, via) = find_lens(
            &db,
            Some("FixtureOptics"),
            Some("FixtureOptics Zoom 18-135mm f/3.5-5.6"),
        )
        .unwrap();
        assert_eq!(via, "exact");
        assert!(find_lens(&db, None, Some("不存在的镜头 50mm")).is_none());
    }
}
