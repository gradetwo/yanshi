//! **服务端 `verify_package` 的判据**（期 2 块 3，D2 + P2-12）。
//!
//! 五条判据，每条都能**红**（变异写在测试头上）：
//!
//! * `two_imports_verify_as_missing_body_0_missing_derived_2` —— 样本包
//!   （2 笔 import）：`missing_body==0` + `missing_derived==2` + 两条恒等式；
//!   朴素口径（只认 `blob_hash`/`blobs`）对照必须报缺 0（证明它漏检）。
//! * `dropping_a_body_blob_flips_missing_body_and_completeness` —— 剔一个
//!   真在用本体 ⇒ `missing_body` +1 且 `complete` 翻转。
//! * `replayable_brush_bitmaps_are_omitted_not_missing` —— `omitted≠missing`：
//!   可重放笔刷位图省掉不算缺，且算出的省掉数与 `BUILD-INFO` 记数一致。
//! * `export_failure_deletes_the_target` —— P2-12：导出失败删目标文件
//!   （`export_project` 与 `export_png` 各一例），成功时文件留着。
//! * `verify_package_rejects_a_non_package` —— 非包 / 缺 `atoms.jsonl` 响亮报错。

use std::collections::BTreeSet;

use serde_json::{json, Value};
use yanshi_server::archive::{read_tar, write_tar, TarEntry};
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

/// **跑完即删临时目录** ✓（`/tmp` 是 1.9 GB tmpfs ✓，夹具留着会写满 ✓）
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

/// 造一个**干净的**临时目录 ✓ —— **它会先把同名目录删掉** ✗ ⇒ 一个测试里**只能调一次** ✓。
fn temp_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("yanshi_verify_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path
}

/// 落盘工作区 + 指向仓库 `assets/`（判据测的就是真会发出去的那批笔刷 ✓）。
fn workspace(root: &std::path::Path, doc: &str, width: u32, height: u32) -> Workspace {
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut workspace = Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default())
        .expect("落盘工作区应当能建")
        .with_assets_dir(Some(assets));
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

/// 导一张 64×64 的纯色 raw 位图（≥64 ⇒ 建 tile 索引 ⇒ 原子里有 `payload.tiles`）。
fn import_raw(
    workspace: &mut Workspace,
    doc: &str,
    id: &str,
    x: f64,
    y: f64,
    color: [u8; 4],
) -> Value {
    let mut rgba = vec![0u8; 64 * 64 * 4];
    for px in rgba.chunks_exact_mut(4) {
        px.copy_from_slice(&color);
    }
    let hash = workspace.store().put(&rgba).expect("入库应成功");
    call(
        workspace,
        doc,
        "import_image",
        json!({
            "layer_id": "L", "object_id": id,
            "bitmap": {"blob_hash": hash.to_string(), "size": rgba.len(), "mime_type": "image/x-yanshi-raw"},
            "region": {"x": x, "y": y, "w": 64.0, "h": 64.0},
        }),
    )
}

/// 画一条**可重放**的笔触 ✓（`2B_pencil` 的 `smudge = 0` ⇒ 不读画布 ✓）。
fn pencil_stroke(workspace: &mut Workspace, doc: &str, id: &str, points: &[[f64; 3]]) -> Value {
    let points: Vec<Value> = points.iter().map(|p| json!([p[0], p[1], p[2]])).collect();
    call(
        workspace,
        doc,
        "brush_stroke",
        json!({
            "layer_id": "L", "object_id": id, "brush": "2B_pencil.myb", "size": 12.0,
            "color": {"r": 180, "g": 140, "b": 100, "a": 220},
            "points": points
        }),
    )
}

fn export_to(workspace: &mut Workspace, doc: &str, path: &std::path::Path, include: bool) -> Value {
    call(
        workspace,
        doc,
        "export_project",
        json!({ "path": path.to_string_lossy(), "doc_id": doc, "include_bitmaps": include }),
    )
}

fn verify(workspace: &mut Workspace, doc: &str, path: &std::path::Path) -> Value {
    call(
        workspace,
        doc,
        "verify_package",
        json!({ "path": path.to_string_lossy() }),
    )
}

/// **朴素口径**（`check_package.py` 的 C3）：只认 `blob_hash`/`hash`/`blobs` 键。
/// 它必须漏掉 `payload.tiles` —— 这正是它对照的意义。
fn naive_hashes(value: &Value, out: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if key == "blob_hash" || key == "hash" {
                    if let Some(text) = child.as_str() {
                        let stripped = text.strip_prefix("sha256:").unwrap_or(text);
                        if stripped.len() == 64 {
                            out.insert(stripped.to_owned());
                        }
                    }
                } else if key == "blobs" {
                    if let Some(items) = child.as_array() {
                        for item in items {
                            if let Some(text) = item.as_str() {
                                let stripped = text.strip_prefix("sha256:").unwrap_or(text);
                                if stripped.len() == 64 {
                                    out.insert(stripped.to_owned());
                                }
                            }
                        }
                    } else {
                        naive_hashes(child, out);
                    }
                } else {
                    naive_hashes(child, out);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                naive_hashes(item, out);
            }
        }
        _ => {}
    }
}

fn atoms_of(pack: &std::path::Path) -> Vec<Value> {
    let bytes = std::fs::read(pack).expect("读回导出的包");
    let entries = read_tar(&bytes).expect("自己的包必须读得回来");
    let atoms = entries
        .iter()
        .find(|entry| entry.path == "atoms.jsonl")
        .expect("包里应当有 atoms.jsonl");
    let text = String::from_utf8_lossy(&atoms.bytes);
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str::<Value>(line).expect("原子行应当是合法 JSON"))
        .collect()
}

/// 样本包：2 笔 import（各 64×64），`include_bitmaps: true`。
fn sample_pack(root: &std::path::Path, doc: &str) -> (Workspace, std::path::PathBuf) {
    let mut workspace = workspace(root, doc, 128, 128);
    assert_eq!(
        call(
            &mut workspace,
            doc,
            "create_layer",
            json!({ "layer_id": "L" })
        )["ok"],
        json!(true)
    );
    let first = import_raw(&mut workspace, doc, "img_a", 0.0, 0.0, [200, 40, 90, 255]);
    assert_eq!(first["ok"], json!(true), "{first}");
    let second = import_raw(&mut workspace, doc, "img_b", 64.0, 0.0, [40, 120, 200, 255]);
    assert_eq!(second["ok"], json!(true), "{second}");
    let pack = root.join("sample.yanshi");
    let exported = export_to(&mut workspace, doc, &pack, true);
    assert_eq!(exported["ok"], json!(true), "{exported}");
    assert!(pack.exists(), "成功导出后文件必须在");
    (workspace, pack)
}

/// **样本包（2 笔）⇒ `missing_body==0` + `missing_derived==2` + 恒等式** ✓。
///
/// 变异：分类器把 `tiles` 当成本体 ⇒ `missing_body` 变 2、`missing_derived` 变 0 ⇒ 红；
/// 把文本口径改成只扫 `blob_hash` ⇒ `referenced` 塌掉、恒等式红 ✓。
#[test]
fn two_imports_verify_as_missing_body_0_missing_derived_2() {
    let root = temp_dir("sample");
    let _cleanup = Cleanup(root.clone());
    let (mut workspace, pack) = sample_pack(&root, "doc_vp");
    let report = verify(&mut workspace, "doc_vp", &pack);
    assert_eq!(report["ok"], json!(true), "{report}");
    assert_eq!(report["complete"], json!(true), "{report}");
    assert_eq!(report["missing_body"], json!(0), "{report}");
    assert_eq!(report["missing_derived"], json!(2), "{report}");
    assert_eq!(report["missing_total"], json!(2), "{report}");
    assert_eq!(report["omitted"], json!(0), "{report}");
    // **恒等式**（不是单点断言）：两栏之和 == 总数；引用 == 在包 + 真缺 + 省掉。
    let missing_body = report["missing_body"].as_u64().unwrap();
    let missing_derived = report["missing_derived"].as_u64().unwrap();
    let missing_total = report["missing_total"].as_u64().unwrap();
    let referenced = report["referenced"].as_u64().unwrap();
    let present_and_referenced = report["present_and_referenced"].as_u64().unwrap();
    let omitted = report["omitted"].as_u64().unwrap();
    assert_eq!(
        missing_body + missing_derived,
        missing_total,
        "missing_body + missing_derived == missing_total：{report}"
    );
    assert_eq!(
        present_and_referenced + missing_total + omitted,
        referenced,
        "referenced == present_and_referenced + missing_total + omitted：{report}"
    );
    // 样本包没有孤儿 blob ⇒ 任务书那条 `referenced == present + missing_total` 也成立。
    assert_eq!(
        report["present"], report["present_and_referenced"],
        "样本包不应有孤儿 blob：{report}"
    );
    assert_eq!(
        referenced,
        report["present"].as_u64().unwrap() + missing_total,
        "referenced == present + missing_total：{report}"
    );
    // **派生的那 2 个必须恰好是 `payload.tiles`**（不是随便哪两个缺失）。
    let tiles: BTreeSet<String> = atoms_of(&pack)
        .iter()
        .filter_map(|atom| atom.get("payload")?.get("tiles")?.as_str())
        .map(|text| text.strip_prefix("sha256:").unwrap_or(text).to_owned())
        .collect();
    assert_eq!(
        tiles.len(),
        2,
        "两笔 import 各应有一个 tile 索引：{tiles:?}"
    );
    let derived: BTreeSet<String> = report["missing_derived_hashes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    assert_eq!(
        derived, tiles,
        "missing_derived 必须恰好是那两个 tile 索引：{report}"
    );
    // **朴素口径对照**：只认 `blob_hash`/`blobs` ⇒ 必须报缺 0（它漏掉了 tiles）。
    let atoms = atoms_of(&pack);
    let entries = read_tar(&std::fs::read(&pack).unwrap()).unwrap();
    let present: BTreeSet<String> = entries
        .iter()
        .filter(|entry| entry.path.starts_with("blobs/sha256/"))
        .filter_map(|entry| entry.path.rsplit('/').next())
        .map(str::to_owned)
        .collect();
    let mut naive = BTreeSet::new();
    for atom in &atoms {
        naive_hashes(atom, &mut naive);
    }
    let naive_missing = naive.difference(&present).count();
    assert_eq!(
        naive_missing, 0,
        "朴素口径（只认 blob_hash/blobs）必须报缺 0 —— 它把 tiles 全漏掉了， naive={naive:?}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// **剔一个真在用本体 ⇒ `missing_body` +1 且 `complete` 翻转** ✓。
///
/// 变异：分类器把本体算成派生 ⇒ `missing_body` 不动、`missing_derived` +1 ⇒ 红；
/// 把 `complete` 写成恒 `true` ⇒ 翻转断言红 ✓。
#[test]
fn dropping_a_body_blob_flips_missing_body_and_completeness() {
    let root = temp_dir("tamper");
    let _cleanup = Cleanup(root.clone());
    let (mut workspace, pack) = sample_pack(&root, "doc_vp");
    let before = verify(&mut workspace, "doc_vp", &pack);
    assert_eq!(before["complete"], json!(true), "{before}");
    // **手工把一个本体 blob 抠掉** ✓（模拟"包传丢了 / 被裁过" ✓）。
    let bytes = std::fs::read(&pack).expect("读回");
    let mut entries = read_tar(&bytes).expect("读回");
    let dropped = entries
        .iter()
        .find(|entry| entry.path.starts_with("blobs/sha256/"))
        .map(|entry| entry.path.clone())
        .expect("样本包应当有本体 blob");
    let dropped_hash = dropped.rsplit('/').next().unwrap().to_owned();
    entries.retain(|entry| entry.path != dropped);
    let doctored = write_tar(&entries);
    let doctored_path = root.join("doctored.yanshi");
    std::fs::write(&doctored_path, &doctored).expect("写回");
    let after = verify(&mut workspace, "doc_vp", &doctored_path);
    assert_eq!(after["ok"], json!(true), "{after}");
    assert_eq!(
        after["complete"],
        json!(false),
        "缺本体后 complete 必须翻转：{after}"
    );
    assert_eq!(after["missing_body"], json!(1), "{after}");
    assert_eq!(after["missing_derived"], json!(2), "{after}");
    assert_eq!(after["missing_total"], json!(3), "{after}");
    let body_hashes: Vec<&str> = after["missing_body_hashes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(
        body_hashes,
        vec![dropped_hash.as_str()],
        "缺的本体必须恰好是抠掉的那一个：{after}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// **`omitted≠missing`**：可重放笔刷位图省掉不算缺，且算出的数与 `BUILD-INFO` 记数一致。
///
/// **注意**：笔触原子**也有** `payload.tiles`——`brush_stroke` 是把一笔渲染成
/// 合成位图再走 `import_image` 落账，笔触包围盒任一边 ≥ 64px 时 `import_image`
/// 就建 tile 索引（`tools_bitmap.rs:292-304`）。所以这两笔各贡献 1 个派生
/// 索引：`missing_derived == 2` 是对的（索引由可重放配方重建，不算缺本体），
/// `missing_total == 2`、`omitted == 2`、`complete == true`。
///
/// 变异：把 `omitted` 算进 `missing_total` ⇒ 恒等式红；把 `include_bitmaps`
/// 的判定去掉 ⇒ 省掉的位图被算成 `missing_body` ⇒ `complete` 翻转 ⇒ 红。
#[test]
fn replayable_brush_bitmaps_are_omitted_not_missing() {
    let root = temp_dir("omitted");
    let _cleanup = Cleanup(root.clone());
    let mut workspace = workspace(&root, "doc_om", 200, 160);
    assert_eq!(
        call(
            &mut workspace,
            "doc_om",
            "create_layer",
            json!({ "layer_id": "L" })
        )["ok"],
        json!(true)
    );
    let first = pencil_stroke(
        &mut workspace,
        "doc_om",
        "p1",
        &[[20.0, 40.0, 0.5], [100.0, 60.0, 0.6]],
    );
    assert_eq!(first["ok"], json!(true), "{first}");
    let second = pencil_stroke(
        &mut workspace,
        "doc_om",
        "p2",
        &[[20.0, 100.0, 0.5], [150.0, 120.0, 0.6]],
    );
    assert_eq!(second["ok"], json!(true), "{second}");
    let pack = root.join("omitted.yanshi");
    let exported = export_to(&mut workspace, "doc_om", &pack, false);
    assert_eq!(exported["ok"], json!(true), "{exported}");
    let report = verify(&mut workspace, "doc_om", &pack);
    assert_eq!(report["ok"], json!(true), "{report}");
    assert_eq!(report["complete"], json!(true), "{report}");
    assert_eq!(report["missing_body"], json!(0), "{report}");
    // **两笔各一个 tile 索引**（笔触包围盒 ≥ 64px ⇒ `import_image` 建索引）：
    // 派生缺 2 个是对的——索引由可重放配方重建，不算缺本体。
    assert_eq!(report["missing_derived"], json!(2), "{report}");
    assert_eq!(report["missing_total"], json!(2), "{report}");
    // **派生的那 2 个必须恰好是两笔的 `payload.tiles`**（不是随便哪两个缺失）。
    let tiles: BTreeSet<String> = atoms_of(&pack)
        .iter()
        .filter_map(|atom| atom.get("payload")?.get("tiles")?.as_str())
        .map(|text| text.strip_prefix("sha256:").unwrap_or(text).to_owned())
        .collect();
    assert_eq!(tiles.len(), 2, "两笔各应有一个 tile 索引：{tiles:?}");
    let derived: BTreeSet<String> = report["missing_derived_hashes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    assert_eq!(
        derived, tiles,
        "missing_derived 必须恰好是那两个 tile 索引：{report}"
    );
    // **恒等式**：引用 == 在包 + 真缺 + 省掉（包里 0 个 blob，全省了）。
    assert_eq!(
        report["referenced"].as_u64().unwrap(),
        report["present_and_referenced"].as_u64().unwrap()
            + report["missing_total"].as_u64().unwrap()
            + report["omitted"].as_u64().unwrap(),
        "referenced == present_and_referenced + missing_total + omitted：{report}"
    );
    let omitted = report["omitted"].as_u64().unwrap();
    assert!(
        omitted >= 1,
        "至少一个可重放位图应当被省掉（omitted≥1）：{report}"
    );
    assert_eq!(
        report["omitted_declared"].as_u64(),
        Some(omitted),
        "算出的 omitted 必须与 BUILD-INFO 记数一致：{report}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// **P2-12：导出失败删目标文件**（旧的也不留，免得冒充新产物）；成功时文件留着。
///
/// 变异：把守卫的 `Drop` 删掉 ⇒ 哨兵文件还在 ⇒ 红；把 `disarm` 删掉 ⇒
/// 成功导出也删文件 ⇒ 正例红 ✓。
#[test]
fn export_failure_deletes_the_target() {
    let root = temp_dir("p212");
    let _cleanup = Cleanup(root.clone());
    let mut workspace = workspace(&root, "doc_p212", 64, 64);
    assert_eq!(
        call(
            &mut workspace,
            "doc_p212",
            "create_layer",
            json!({ "layer_id": "L" })
        )["ok"],
        json!(true)
    );
    // ① `export_project` 叫一个不存在的文档 ⇒ 失败，哨兵必须被删。
    let target_a = root.join("stale_a.yanshi");
    std::fs::write(&target_a, b"old export").expect("哨兵");
    let failed = call(
        &mut workspace,
        "doc_p212",
        "export_project",
        json!({ "path": target_a.to_string_lossy(), "doc_id": "doc_nope" }),
    );
    assert_eq!(failed["ok"], json!(false), "{failed}");
    assert!(!target_a.exists(), "失败时目标路径不许留下旧文件：{failed}");
    // ② `export_png` 参数非法（只给 width 不给 height）⇒ 失败，哨兵必须被删。
    let target_b = root.join("stale_b.png");
    std::fs::write(&target_b, b"old png").expect("哨兵");
    let failed_png = call(
        &mut workspace,
        "doc_p212",
        "export_png",
        json!({ "path": target_b.to_string_lossy(), "width": 32 }),
    );
    assert_eq!(failed_png["ok"], json!(false), "{failed_png}");
    assert!(
        !target_b.exists(),
        "失败时目标路径不许留下旧文件：{failed_png}"
    );
    // ③ 正例：成功导出 ⇒ 文件必须留着（守卫解除）。
    let target_c = root.join("good.yanshi");
    let exported = export_to(&mut workspace, "doc_p212", &target_c, true);
    assert_eq!(exported["ok"], json!(true), "{exported}");
    assert!(target_c.exists(), "成功导出后文件必须在");
    let _ = std::fs::remove_dir_all(&root);
}

/// **导出校验回执附 live 笔数（无撤销）**：`stroke_count` 必须等于
/// `get_stroke_log` 的 `total`，`stroke_reverted_excluded` 为 0。
///
/// 变异：笔数栏恒 0 ⇒ 与 `total` 的相等断言红；把排除数算进 live ⇒
/// 无撤销时仍过、有撤销那条才红（两条互补）。
#[test]
fn verify_package_reports_live_stroke_count_without_revert() {
    let root = temp_dir("strokecount");
    let _cleanup = Cleanup(root.clone());
    let mut workspace = workspace(&root, "doc_sc", 200, 160);
    assert_eq!(
        call(
            &mut workspace,
            "doc_sc",
            "create_layer",
            json!({ "layer_id": "L" })
        )["ok"],
        json!(true)
    );
    let first = pencil_stroke(
        &mut workspace,
        "doc_sc",
        "p1",
        &[[20.0, 40.0, 0.5], [100.0, 60.0, 0.6]],
    );
    assert_eq!(first["ok"], json!(true), "{first}");
    let second = pencil_stroke(
        &mut workspace,
        "doc_sc",
        "p2",
        &[[20.0, 100.0, 0.5], [150.0, 120.0, 0.6]],
    );
    assert_eq!(second["ok"], json!(true), "{second}");
    let log = call(
        &mut workspace,
        "doc_sc",
        "get_stroke_log",
        json!({ "limit": 500 }),
    );
    assert_eq!(log["ok"], json!(true), "{log}");
    let live = log["total"].as_u64().expect("get_stroke_log 必须回 total");
    assert!(live >= 2, "两笔落下后 live 笔数至少为 2：{log}");
    let pack = root.join("strokecount.yanshi");
    let exported = export_to(&mut workspace, "doc_sc", &pack, false);
    assert_eq!(exported["ok"], json!(true), "{exported}");
    let report = verify(&mut workspace, "doc_sc", &pack);
    assert_eq!(report["ok"], json!(true), "{report}");
    assert_eq!(
        report["stroke_count"].as_u64(),
        Some(live),
        "无撤销时 stroke_count 必须等于 get_stroke_log total：{report} vs {log}"
    );
    assert_eq!(
        report["stroke_reverted_excluded"],
        json!(0),
        "无撤销时排除数必须为 0：{report}"
    );
}

/// **导出校验回执附 live 笔数（有撤销）**：撤掉一笔 ⇒ `stroke_count`
/// 减 1、`stroke_reverted_excluded` 为 1、两栏之和等于撤销前的笔数。
/// 口径说明：`stroke_count` 只数 live 笔，`stroke_reverted_excluded`
/// 另计已撤销笔；无撤销时与原子计数一致，有撤销时不同。
#[test]
fn verify_package_stroke_count_excludes_reverted_strokes() {
    let root = temp_dir("strokerevert");
    let _cleanup = Cleanup(root.clone());
    let mut workspace = workspace(&root, "doc_sr", 200, 160);
    assert_eq!(
        call(
            &mut workspace,
            "doc_sr",
            "create_layer",
            json!({ "layer_id": "L" })
        )["ok"],
        json!(true)
    );
    let first = pencil_stroke(
        &mut workspace,
        "doc_sr",
        "p1",
        &[[20.0, 40.0, 0.5], [100.0, 60.0, 0.6]],
    );
    assert_eq!(first["ok"], json!(true), "{first}");
    let second = pencil_stroke(
        &mut workspace,
        "doc_sr",
        "p2",
        &[[20.0, 100.0, 0.5], [150.0, 120.0, 0.6]],
    );
    assert_eq!(second["ok"], json!(true), "{second}");
    let log = call(
        &mut workspace,
        "doc_sr",
        "get_stroke_log",
        json!({ "limit": 500 }),
    );
    assert_eq!(log["ok"], json!(true), "{log}");
    let before = log["total"].as_u64().expect("get_stroke_log 必须回 total");
    let victim = log["entries"][0]["atom_id"]
        .as_str()
        .expect("日志条目必须带 atom_id")
        .to_owned();
    let reverted = call(
        &mut workspace,
        "doc_sr",
        "revert_atoms",
        json!({ "atom_ids": [victim] }),
    );
    assert_eq!(reverted["ok"], json!(true), "{reverted}");
    assert_eq!(reverted["reverted_count"], json!(1), "{reverted}");
    let pack = root.join("strokerevert.yanshi");
    let exported = export_to(&mut workspace, "doc_sr", &pack, false);
    assert_eq!(exported["ok"], json!(true), "{exported}");
    let report = verify(&mut workspace, "doc_sr", &pack);
    assert_eq!(report["ok"], json!(true), "{report}");
    assert_eq!(
        report["stroke_count"].as_u64(),
        Some(before - 1),
        "撤一笔后 live 笔数必须减 1：{report}（撤销前 {before}）"
    );
    assert_eq!(
        report["stroke_reverted_excluded"],
        json!(1),
        "撤一笔后排除数必须为 1：{report}"
    );
    assert_eq!(
        report["stroke_count"].as_u64().unwrap()
            + report["stroke_reverted_excluded"].as_u64().unwrap(),
        before,
        "恒等式：stroke_count + stroke_reverted_excluded == 全部匹配笔触原子数：{report}"
    );
}

/// **非包 / 缺 `atoms.jsonl` 响亮报错**（不静默、不当成"空包完整"）。
#[test]
fn verify_package_rejects_a_non_package() {
    let root = temp_dir("badpack");
    let _cleanup = Cleanup(root.clone());
    let mut workspace = workspace(&root, "doc_bad", 64, 64);
    let garbage = root.join("garbage.yanshi");
    std::fs::write(&garbage, b"not a tar at all").expect("写垃圾");
    let rejected = verify(&mut workspace, "doc_bad", &garbage);
    assert_eq!(rejected["ok"], json!(false), "{rejected}");
    let no_atoms = root.join("noatoms.yanshi");
    let bytes = write_tar(&[TarEntry {
        path: "meta.json".to_owned(),
        bytes: br#"{"doc_id":"doc_bad"}"#.to_vec(),
    }]);
    std::fs::write(&no_atoms, &bytes).expect("写包");
    let rejected = verify(&mut workspace, "doc_bad", &no_atoms);
    assert_eq!(rejected["ok"], json!(false), "{rejected}");
    assert!(
        rejected.to_string().contains("atoms.jsonl"),
        "错误必须点名缺的是 atoms.jsonl：{rejected}"
    );
    let _ = std::fs::remove_dir_all(&root);
}
