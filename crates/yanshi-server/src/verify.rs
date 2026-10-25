//! **服务端工程包体检 `verify_package`**（期 2 块 3，D2）。
//!
//! **为什么需要它**（D2 的缺口）：`export_project` 只报单栏 `missing`，
//! 而一个包能不能重放，取决于**缺的是本体还是派生** —— 缺本体 ⇒ 那些笔触
//! 渲染成空白（D172 的空影片）；只缺派生 tile ⇒ 实测能重放（tile 由本体
//! 重建）。调用方拿到包之后、重放之前，必须能**机读**地问一句「这包齐不齐」。
//!
//! **canonical tile 规则**（期 2 开工即定，收敛两条旧口径）：
//! - 上游 `ca58be7`：JSON 里**键名恰为 `tiles`** 的哈希 ⇒ 派生（豁免）；
//! - 期 1 B2：**路径子串**（`bitmap`/`refs`/`mask`/`patch` ⇒ 本体，
//!   `tiles` ⇒ 派生，**本体胜出**、未知 ⇒ 本体 fail-safe）；
//! - 本模块：**两条的并集** —— 同一个哈希在日志里所有出现位置的类别取并集，
//!   本体胜出；一次都没被认出来 ⇒ 本体（fail-safe）。
//!   与 `scripts/check_package.py`（独立口径）的 `classify` 逐字等价：
//!   路径含 `tiles` ⇒ 派生，否则含 `bitmap`/`refs`/`mask`/`patch` ⇒ 本体，
//!   否则其它 ⇒ 本模块把「其它」也收进本体（fail-safe），于是
//!   `missing_body + missing_derived == missing_total` **恒成立**。
//!
//! **`omitted ≠ missing`**：导出时有意省掉的位图（`include_bitmaps: false`
//! 且配方可重放 ⇒ `BUILD-INFO` 里 `blobs_omitted_replayable` 记数）**不是**
//! 缺失 —— 包里没有它，但导入端能从日志重跑出来。本模块的判定：
//! 缺本体 ⇒ `missing_body`；可重放省掉 ⇒ `omitted`（单独一栏，
//! **不进** `missing_total`）；于是恒等式是
//! `referenced == present_and_referenced + missing_total + omitted`，
//! 而任务书那条 `referenced == present + missing_total` 在「包里没有孤儿
//! blob 且 `omitted == 0`」时与之一致（样本包满足这两条）。
//! 省掉的判定要**可决定**，不能只看计数：`BUILD-INFO` 里新增两行
//! （`service.rs` 的 `export_project` 写）：`include_bitmaps: true|false`
//! 与 `missing_blobs: <hash>`（每缺一个一行）。缺席本体 blob 的判定顺序：
//! ① 在 `missing_blobs` 清单里 ⇒ 存储里就没有 ⇒ `missing_body`；
//! ② `include_bitmaps: false` 且引用它的每一条净荷都带可重放笔刷配方
//! ⇒ `omitted`；③ 其余 ⇒ `missing_body`。老包没有这两行时 ①② 不适用，
//! 一律按 ③（保守：宁可报缺，不可放过）。

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde_json::Value;
use yanshi_core::{ErrorCode, ErrorContext, Result, YanshiError};

/// 一条引用在日志里某次出现时的类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RefClass {
    /// 内容本体：位图 / 蒙版 / 补丁（缺了 ⇒ 画面真的少一块）。
    Body,
    /// 派生：tile 分块索引（缺了 ⇒ 由本体重建，仍可重放）。
    Derived,
    /// 未知上下文（既不含 `tiles` 也不含本体子串）。
    Other,
}

/// 引用类别的**最终裁决**：本体胜出；未知 ⇒ 本体（fail-safe）。
///
/// **为什么未知也要算本体**：分类器没认出来的引用，默认它重要 ——
///
/// 误报成「缺本体」只会让人多查一次，误报成「只缺派生」会把一个
/// 坏包放过去。fail-safe 的方向只能是前者。
pub fn resolve_class(classes: &BTreeSet<RefClass>) -> RefClass {
    if classes.contains(&RefClass::Body) {
        RefClass::Body
    } else if classes.contains(&RefClass::Derived) {
        RefClass::Derived
    } else {
        // 空集与只有 Other 都走这里：没认出来 ⇒ 本体。
        RefClass::Body
    }
}

/// **按值扫 64 位十六进制哈希**（与 `check_package.py` 的 `HEX64` 同口径）。
///
/// 手写扫描，不引 `regex`：整段文本里最长的 `[0-9a-f]` 连续段恰好 64 位
/// 才算（65 位及更长的段一个都不算 —— 与正则的两侧否定前瞻同一语义）。
/// 大写十六进制不算（blob 哈希恒小写；`check_package.py` 亦然）。
pub fn scan_hex64(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let bytes = text.as_bytes();
    let mut start = 0usize;
    while start < bytes.len() {
        if !bytes[start].is_ascii_hexdigit() || bytes[start].is_ascii_uppercase() {
            start += 1;
            continue;
        }
        let mut end = start;
        while end < bytes.len()
            && bytes[end].is_ascii_hexdigit()
            && !bytes[end].is_ascii_uppercase()
        {
            end += 1;
        }
        if end - start == 64 {
            out.insert(text[start..end].to_owned());
        }
        start = end;
    }
    out
}

/// **一个 JSON 字符串值是不是 blob 引用**：去 `sha256:` 前缀后恰好 64 位小写十六进制。
pub fn hash_from_str(raw: &str) -> Option<String> {
    let stripped = raw.strip_prefix("sha256:").unwrap_or(raw);
    if stripped.len() == 64
        && stripped
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Some(stripped.to_owned())
    } else {
        None
    }
}

/// **一条引用路径的类别**（与 `check_package.py` 的 `classify` 逐字等价）。
///
/// 路径是点分键路径（数组记 `[]`，如 `.payload.tiles`、`.refs.blobs[]`）：
/// 含 `tiles` ⇒ 派生；否则含 `bitmap`/`refs`/`mask`/`patch` ⇒ 本体；
/// 否则其它。**注意顺序**：`tiles` 优先于本体子串（与 py 同序）——
/// 同一哈希在别处还有本体出现时，由 `resolve_class` 的本体胜出兜底。
fn classify_path(path: &str) -> RefClass {
    let lowered = path.to_lowercase();
    if lowered.contains("tiles") {
        RefClass::Derived
    } else if lowered.contains("bitmap")
        || lowered.contains("refs")
        || lowered.contains("mask")
        || lowered.contains("patch")
    {
        RefClass::Body
    } else {
        RefClass::Other
    }
}

/// **递归扫一个 JSON 值**，把每个 blob 引用的出现类别记进 `out`。
///
/// `path` 是当前值的点分路径（根调用传 `""`，对象键续 `.key`，数组续 `[]`）。
/// 只认**字符串值**里的哈希（与 py 的 `blob_refs` 同口径：数字、布尔里的
/// 64 位串不算引用）。
pub fn collect_classes(value: &Value, path: &str, out: &mut BTreeMap<String, BTreeSet<RefClass>>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                collect_classes(child, &format!("{path}.{key}"), out);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_classes(item, &format!("{path}[]"), out);
            }
        }
        Value::String(text) => {
            if let Some(hash) = hash_from_str(text) {
                out.entry(hash).or_default().insert(classify_path(path));
            }
        }
        _ => {}
    }
}

/// **给一条内存里的原子做分类**（`export_png` 的缺本体检查用，与日志行同一口径）。
///
/// `payload` 走 `payload` 前缀的 JSON  walk（路径形如
/// `payload.bitmap.blob_hash`）；`refs.blobs` 里的哈希是结构化引用，
/// 直接记本体（`collect_refs` 只收 `blob_hash`/`blob` 键 ⇒ 里面没有 tile）。
pub fn collect_atom_classes(
    payload: &Value,
    refs_blobs: &[String],
    out: &mut BTreeMap<String, BTreeSet<RefClass>>,
) {
    collect_classes(payload, "payload", out);
    for blob in refs_blobs {
        if let Some(hash) = hash_from_str(blob) {
            out.entry(hash).or_default().insert(RefClass::Body);
        }
    }
}

/// **包里 `BUILD-INFO` 的机读部分**（导出端写，见 `service.rs`）。
#[derive(Debug, Default, Clone)]
pub struct BuildInfo {
    /// 导出时 `include_bitmaps` 的取值（老包没有这一行 ⇒ `None`）。
    pub include_bitmaps: Option<bool>,
    /// `blobs_omitted_replayable` 的记数（老包没有这一行 ⇒ `None`）。
    pub omitted_declared: Option<usize>,
    /// 导出时存储里就没有的本体 blob（`missing_blobs: <hash>`，每缺一个一行）。
    pub missing_declared: BTreeSet<String>,
}

/// 解析 `BUILD-INFO` 文本里的机读行（不认识的行直接跳过）。
pub fn parse_build_info(text: &str) -> BuildInfo {
    let mut info = BuildInfo::default();
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("include_bitmaps:") {
            info.include_bitmaps = match rest.trim() {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            };
        } else if let Some(rest) = line.strip_prefix("blobs_omitted_replayable:") {
            info.omitted_declared = rest.trim().parse::<usize>().ok();
        } else if let Some(rest) = line.strip_prefix("missing_blobs:") {
            if let Some(hash) = hash_from_str(rest.trim()) {
                info.missing_declared.insert(hash);
            }
        }
    }
    info
}

/// **一次包体检的报告**（`verify_package` 的回执本体，可直接序列化）。
#[derive(Debug, Clone, Serialize)]
pub struct PackageReport {
    /// 日志里引用的不同 blob 数（文本口径：`atoms.jsonl` 原文扫 64 位十六进制）。
    pub referenced: usize,
    /// 包里 `blobs/` 下的 blob 数（含没被任何原子引用的孤儿）。
    pub present: usize,
    /// 既被引用、包里也有的 blob 数。
    pub present_and_referenced: usize,
    /// 真缺的总数（`== missing_body + missing_derived`，不含 `omitted`）。
    pub missing_total: usize,
    /// 缺的本体 blob 数。
    pub missing_body: usize,
    /// 缺的派生 tile 数。
    pub missing_derived: usize,
    /// 有意省掉的可重放位图数（不是缺失，不进 `missing_total`）。
    pub omitted: usize,
    /// `BUILD-INFO` 记的省掉数（老包没有这一行 ⇒ `None`）。
    pub omitted_declared: Option<usize>,
    /// 解析失败的原子行数（它们的哈希按本体 fail-safe 计入 `missing_body`）。
    pub unparsable_lines: usize,
    /// **包是否完整**：`missing_body == 0`（只缺派生 ⇒ 完整，可重放）。
    pub complete: bool,
    /// 缺的本体哈希（排序后，`sha256:` 前缀已去掉）。
    pub missing_body_hashes: Vec<String>,
    /// 缺的派生哈希（排序后）。
    pub missing_derived_hashes: Vec<String>,
    /// 省掉的可重放位图哈希（排序后）。
    pub omitted_hashes: Vec<String>,
    /// 人读的判定说明（机读字段在上面；这里只解释，不承载判定）。
    pub warnings: Vec<String>,
}

/// **体检一个 `.yanshi` 包的字节**（`verify_package` 的实现本体）。
///
/// `is_replayable`：判断一条引用净荷是否带可重放笔刷配方
/// （调用方传 `brush_source_is_replayable` 那条判定；本模块不碰工作区）。
/// 缺席本体 blob 的判定顺序见模块头：① `BUILD-INFO` 的 `missing_blobs`
/// 清单 ⇒ `missing_body`；② `include_bitmaps: false` 且每条引用净荷都
/// 可重放 ⇒ `omitted`；③ 其余 ⇒ `missing_body`。
pub fn verify_package_bytes(
    bytes: &[u8],
    is_replayable: &dyn Fn(&Value) -> bool,
) -> Result<PackageReport> {
    let entries = crate::archive::read_tar(bytes)?;
    let atoms_entry = entries.iter().find(|entry| entry.path == "atoms.jsonl");
    let Some(atoms_entry) = atoms_entry else {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("包里根本没有 atoms.jsonl ⇒ 这不是一个可重放的工程包".to_owned()),
        ));
    };
    let atoms_text = String::from_utf8_lossy(&atoms_entry.bytes).into_owned();
    // **引用集按文本口径**（与 `check_package.py` 的 `scanned` 同口径）：
    // 原文扫 64 位十六进制 ⇒ 解析失败的行里的引用也算进来（它们没有类别，
    // 下面按本体 fail-safe 处理，而不是悄悄漏掉）。
    let referenced = scan_hex64(&atoms_text);
    // **包内集**（与 py 同口径：`blobs/` 下、末段 64 字符 —— 不校验十六进制，
    // 口径一致才能逐项对上；包内 blob 名本来就是哈希）。
    let mut present = BTreeSet::new();
    for entry in &entries {
        let parts: Vec<&str> = entry.path.split('/').collect();
        if parts.first() == Some(&"blobs") {
            if let Some(last) = parts.last() {
                if last.len() == 64 {
                    present.insert(last.to_string());
                }
            }
        }
    }
    // **`BUILD-INFO` 机读行**（老包没有 ⇒ 全 `None`/空 ⇒ 判定走保守分支）。
    let build_info = entries
        .iter()
        .find(|entry| entry.path == "BUILD-INFO")
        .map(|entry| parse_build_info(&String::from_utf8_lossy(&entry.bytes)))
        .unwrap_or_default();
    // **逐行解析**：类别 + 每条哈希的引用净荷（供 `omitted` 判定用）。
    let mut classes: BTreeMap<String, BTreeSet<RefClass>> = BTreeMap::new();
    let mut payloads_by_hash: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut unparsable_lines = 0usize;
    for line in atoms_text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(atom) = serde_json::from_str::<Value>(line) else {
            unparsable_lines += 1;
            continue;
        };
        let mut here: BTreeMap<String, BTreeSet<RefClass>> = BTreeMap::new();
        collect_classes(&atom, "", &mut here);
        let payload = atom.get("payload").cloned().unwrap_or(Value::Null);
        for (hash, kinds) in here {
            classes.entry(hash.clone()).or_default().extend(kinds);
            payloads_by_hash
                .entry(hash)
                .or_default()
                .push(payload.clone());
        }
    }
    let present_and_referenced = referenced.intersection(&present).count();
    let missing: BTreeSet<String> = referenced.difference(&present).cloned().collect();
    let mut missing_body_hashes: Vec<String> = Vec::new();
    let mut missing_derived_hashes: Vec<String> = Vec::new();
    let mut omitted_hashes: Vec<String> = Vec::new();
    for hash in &missing {
        let class = classes
            .get(hash)
            .map(resolve_class)
            .unwrap_or(RefClass::Body);
        match class {
            RefClass::Derived => missing_derived_hashes.push(hash.clone()),
            RefClass::Body | RefClass::Other => {
                // **本体缺席的三段判定**（见模块头）：清单在先，可重放在中。
                if build_info.missing_declared.contains(hash) {
                    missing_body_hashes.push(hash.clone());
                } else if build_info.include_bitmaps == Some(false) {
                    let payloads = payloads_by_hash.get(hash);
                    let replayable = payloads
                        .is_some_and(|list| !list.is_empty() && list.iter().all(is_replayable));
                    if replayable {
                        omitted_hashes.push(hash.clone());
                    } else {
                        missing_body_hashes.push(hash.clone());
                    }
                } else {
                    missing_body_hashes.push(hash.clone());
                }
            }
        }
    }
    let missing_body = missing_body_hashes.len();
    let missing_derived = missing_derived_hashes.len();
    let omitted = omitted_hashes.len();
    let mut warnings: Vec<String> = Vec::new();
    if missing_body > 0 {
        warnings.push(format!(
            "缺本体 {missing_body} 个（首个 {}）⇒ 那些笔触会渲染成空白，先补 blob 再谈重放",
            missing_body_hashes[0]
        ));
    }
    if missing_derived > 0 {
        warnings.push(format!(
            "只缺派生 tile {missing_derived} 个，本体全在 ⇒ 实测能重放（tile 由本体重建），但不要拿它主张离线无损归档"
        ));
    }
    if omitted > 0 {
        warnings.push(format!(
            "另有 {omitted} 个位图是导出时有意省掉的可重放位图（omitted），不在缺失里"
        ));
    }
    if unparsable_lines > 0 {
        warnings.push(format!(
            "原子日志里有 {unparsable_lines} 行解析失败，它们的引用按本体 fail-safe 计入 missing_body"
        ));
    }
    Ok(PackageReport {
        referenced: referenced.len(),
        present: present.len(),
        present_and_referenced,
        missing_total: missing_body + missing_derived,
        missing_body,
        missing_derived,
        omitted,
        omitted_declared: build_info.omitted_declared,
        unparsable_lines,
        complete: missing_body == 0,
        missing_body_hashes,
        missing_derived_hashes,
        omitted_hashes,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// **64 位跑的边界**：恰好 64 算、65 不算、带 `sha256:` 前缀的照算。
    #[test]
    fn hex_scan_counts_only_exact_64_runs() {
        let hash = "a".repeat(64);
        let long = "b".repeat(65);
        let text = format!("x {hash} y sha256:{hash} z {long} w");
        let found = scan_hex64(&text);
        assert_eq!(found.len(), 1, "65 位的段一个都不该算：{found:?}");
        assert!(found.contains(&hash), "恰好 64 位的段应当算：{found:?}");
    }

    /// **分类器与 `check_package.py` 同口径**：`tiles` ⇒ 派生，
    /// `refs.blobs` / `bitmap.blob_hash` ⇒ 本体，本体胜出，未知 ⇒ 本体。
    #[test]
    fn classifier_matches_the_python_rule() {
        let tile = "c".repeat(64);
        let body = "d".repeat(64);
        let both = "e".repeat(64);
        let unknown = "f".repeat(64);
        let atom = json!({
            "payload": {
                "tiles": tile,
                "bitmap": {"blob_hash": format!("sha256:{body}")},
                "other": {"deep": {"tiles": both, "bitmap": {"blob_hash": format!("sha256:{both}")}}},
                "mystery": unknown,
            },
            "refs": {"blobs": [format!("sha256:{body}")]}
        });
        let mut out = BTreeMap::new();
        collect_classes(&atom, "", &mut out);
        assert_eq!(
            resolve_class(&out[&tile]),
            RefClass::Derived,
            "只在 tiles 下出现 ⇒ 派生"
        );
        assert_eq!(
            resolve_class(&out[&body]),
            RefClass::Body,
            "bitmap + refs.blobs ⇒ 本体"
        );
        assert_eq!(
            resolve_class(&out[&both]),
            RefClass::Body,
            "同一哈希两处出现 ⇒ 本体胜出"
        );
        assert_eq!(
            resolve_class(out.get(&unknown).unwrap_or(&BTreeSet::new())),
            RefClass::Body,
            "未知上下文 ⇒ 本体 fail-safe"
        );
    }

    /// **`BUILD-INFO` 机读行**：三行都认，不认识的行跳过。
    #[test]
    fn build_info_parses_the_machine_lines() {
        let hash = "a".repeat(64);
        let info = parse_build_info(&format!(
            "name: yanshi\ninclude_bitmaps: false\nblobs_omitted_replayable: 2\nmissing_blobs: {hash}\nrestore: xxx\n"
        ));
        assert_eq!(info.include_bitmaps, Some(false));
        assert_eq!(info.omitted_declared, Some(2));
        assert!(info.missing_declared.contains(&hash));
    }
}
