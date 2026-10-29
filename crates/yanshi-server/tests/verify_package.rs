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

/// **第 4 轮第 3 期口径簇 · 油画 P-5 回归**：同一批笔触原子，三处口径必须
/// 各说各的、且核包只认净荷不认包络 —— `atoms.jsonl` 与 `list_atoms` 的 kind
/// 全为 `import_image`，`get_stroke_log` 的 kind 全为 `brush`，而 `verify_package`
/// 按 `payload.source.kind + origin` 判：缺省包 ⇒ `complete` 为真、
/// `missing_body` 为 0、`omitted` 恰好等于包内缺失的不同本体哈希数
/// （去重计数，不是笔数）；导出回执 `package_self_contained` 为假（包依赖
/// import 重放，不是逐字节自含），而 `include_bitmaps: true` 的包为真。
///
/// 变异：核包改按包络 kind 名判 ⇒ 笔触全被当成照片导入 ⇒ `missing_body` > 0、
/// `complete` 翻转 ⇒ 红；`omitted` 按笔数计 ⇒ 与去重后的缺失哈希集对不上 ⇒ 红。
#[test]
fn p5_three_brush_strokes_judged_by_payload_not_envelope() {
    let root = temp_dir("p5");
    let _cleanup = Cleanup(root.clone());
    let mut workspace = workspace(&root, "doc_p5", 200, 160);
    assert_eq!(
        call(
            &mut workspace,
            "doc_p5",
            "create_layer",
            json!({ "layer_id": "L" })
        )["ok"],
        json!(true)
    );
    let strokes = [
        ("s1", [[20.0, 40.0, 0.5], [100.0, 60.0, 0.6]]),
        ("s2", [[20.0, 100.0, 0.5], [150.0, 120.0, 0.6]]),
        ("s3", [[30.0, 140.0, 0.5], [120.0, 150.0, 0.6]]),
    ];
    for (id, points) in &strokes {
        let drawn = pencil_stroke(&mut workspace, "doc_p5", id, points);
        assert_eq!(drawn["ok"], json!(true), "{drawn}");
    }
    // 一笔矢量线：它不带位图引用（`draw_stroke` 保持自有 kind，不走 import_image 包络）。
    let vector = call(
        &mut workspace,
        "doc_p5",
        "draw_stroke",
        json!({
            "layer_id": "L", "object_id": "v1",
            "data": {"points": [[10.0, 10.0], [60.0, 10.0]], "size": 3.0,
                     "hardness": 1.0, "color": {"r": 10, "g": 20, "b": 30, "a": 255}}
        }),
    );
    assert_eq!(vector["ok"], json!(true), "{vector}");
    // **三处口径**：包络 kind 全为 import_image（笔触 3 笔），语义 kind 全为 brush。
    let pack = root.join("p5.yanshi");
    let exported = export_to(&mut workspace, "doc_p5", &pack, false);
    assert_eq!(exported["ok"], json!(true), "{exported}");
    assert_eq!(
        exported["package_self_contained"],
        json!(false),
        "缺省包依赖 import 重放，不是逐字节自含：{exported}"
    );
    assert!(
        exported["omitted"].as_u64().unwrap() >= 1,
        "至少一笔可重放位图应当被省掉：{exported}"
    );
    let atoms = atoms_of(&pack);
    let brush_atoms: Vec<&Value> = atoms
        .iter()
        .filter(|atom| {
            atom.get("payload")
                .and_then(|payload| payload.get("source"))
                .and_then(|source| source.get("kind"))
                .and_then(Value::as_str)
                == Some("brush")
        })
        .collect();
    assert_eq!(brush_atoms.len(), 3, "3 笔笔触原子：{atoms:?}");
    assert!(
        brush_atoms
            .iter()
            .all(|atom| atom.get("kind").and_then(Value::as_str) == Some("import_image")),
        "笔触包络 kind 全为 import_image：{brush_atoms:?}"
    );
    assert!(
        brush_atoms.iter().all(|atom| {
            atom.get("payload")
                .and_then(|payload| payload.get("origin"))
                .and_then(Value::as_str)
                == Some("drawn")
        }),
        "笔触 origin 全为 drawn：{brush_atoms:?}"
    );
    // 矢量笔不带位图引用：它的净荷里没有任何 64 位哈希。
    let vector_atom = atoms
        .iter()
        .find(|atom| atom.get("kind").and_then(Value::as_str) == Some("draw_stroke"))
        .expect("矢量笔原子应当在包里");
    let vector_text = serde_json::to_string(vector_atom).unwrap();
    assert!(
        !vector_text.contains("sha256:"),
        "矢量笔不应当引用任何 blob：{vector_text}"
    );
    let listed = call(
        &mut workspace,
        "doc_p5",
        "list_atoms",
        json!({ "limit": 5000 }),
    );
    assert_eq!(listed["ok"], json!(true), "{listed}");
    let listed_brush: Vec<&Value> = listed["atoms"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|atom| {
            atom.get("origin").and_then(Value::as_str) == Some("drawn")
                && atom.get("brush").and_then(Value::as_str).is_some()
        })
        .collect();
    assert_eq!(
        listed_brush.len(),
        3,
        "list_atoms 里 3 笔 drawn 笔触：{listed}"
    );
    assert!(
        listed_brush
            .iter()
            .all(|atom| atom.get("kind").and_then(Value::as_str) == Some("import_image")),
        "list_atoms kind 全为 import_image：{listed_brush:?}"
    );
    let log = call(
        &mut workspace,
        "doc_p5",
        "get_stroke_log",
        json!({ "limit": 5000 }),
    );
    assert_eq!(log["ok"], json!(true), "{log}");
    let log_brush: Vec<&Value> = log["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry.get("brush").and_then(Value::as_str).is_some())
        .collect();
    assert_eq!(log_brush.len(), 3, "get_stroke_log 里 3 笔笔刷：{log}");
    assert!(
        log_brush
            .iter()
            .all(|entry| entry.get("kind").and_then(Value::as_str) == Some("brush")),
        "get_stroke_log kind 全为 brush：{log_brush:?}"
    );
    // **核包**：omitted 恰好等于包内缺失的不同本体哈希集（去重，不是笔数）。
    let entries = read_tar(&std::fs::read(&pack).unwrap()).unwrap();
    let present: BTreeSet<String> = entries
        .iter()
        .filter(|entry| entry.path.starts_with("blobs/sha256/"))
        .filter_map(|entry| entry.path.rsplit('/').next())
        .map(str::to_owned)
        .collect();
    let mut body_hashes = BTreeSet::new();
    for atom in &brush_atoms {
        if let Some(hash) = atom
            .get("payload")
            .and_then(|payload| payload.get("bitmap"))
            .and_then(|bitmap| bitmap.get("blob_hash"))
            .and_then(Value::as_str)
        {
            body_hashes.insert(hash.strip_prefix("sha256:").unwrap_or(hash).to_owned());
        }
    }
    let absent: BTreeSet<String> = body_hashes.difference(&present).cloned().collect();
    let report = verify(&mut workspace, "doc_p5", &pack);
    assert_eq!(report["ok"], json!(true), "{report}");
    assert_eq!(report["complete"], json!(true), "{report}");
    assert_eq!(report["missing_body"], json!(0), "{report}");
    let omitted: BTreeSet<String> = report["omitted_hashes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    assert_eq!(
        omitted, absent,
        "omitted 必须恰好是包内缺失的不同本体哈希集（去重）：{report}"
    );
    // **真包对照**：同一文档 `include_bitmaps: true` ⇒ 逐字节自含。
    let full_pack = root.join("p5_full.yanshi");
    let full_exported = export_to(&mut workspace, "doc_p5", &full_pack, true);
    assert_eq!(full_exported["ok"], json!(true), "{full_exported}");
    assert_eq!(
        full_exported["package_self_contained"],
        json!(true),
        "全量包应当逐字节自含：{full_exported}"
    );
    let full_report = verify(&mut workspace, "doc_p5", &full_pack);
    assert_eq!(full_report["omitted"], json!(0), "{full_report}");
    let _ = std::fs::remove_dir_all(&root);
}

/// **第 4 轮第 3 期口径簇 · 水彩 P2 反证边界**：真正的 `import_image` 工具导入
/// （用户照片，无生成参数）本体缺失是**真实丢失** —— 全量包里抠掉照片本体 ⇒
/// `missing_body == 1`、`complete` 为假；且那条原子的 `origin` 为 `imported`。
///
/// 变异：核包把照片也当可重放 ⇒ `missing_body` 归 0、`complete` 翻回真 ⇒ 红。
#[test]
fn photo_body_loss_is_real_not_replayable() {
    let root = temp_dir("photo");
    let _cleanup = Cleanup(root.clone());
    let (mut workspace, pack) = sample_pack(&root, "doc_photo");
    let atoms = atoms_of(&pack);
    let photo_atom = atoms
        .iter()
        .find(|atom| {
            atom.get("payload")
                .and_then(|payload| payload.get("bitmap"))
                .is_some()
        })
        .expect("样本包应当有照片原子");
    assert_eq!(
        photo_atom
            .get("payload")
            .and_then(|payload| payload.get("origin"))
            .and_then(Value::as_str),
        Some("imported"),
        "照片原子的 origin 应当是 imported：{photo_atom:?}"
    );
    let bytes = std::fs::read(&pack).expect("读回");
    let mut entries = read_tar(&bytes).expect("读回");
    let dropped = entries
        .iter()
        .find(|entry| entry.path.starts_with("blobs/sha256/"))
        .map(|entry| entry.path.clone())
        .expect("样本包应当有本体 blob");
    entries.retain(|entry| entry.path != dropped);
    let doctored = write_tar(&entries);
    let doctored_path = root.join("photo_doctored.yanshi");
    std::fs::write(&doctored_path, &doctored).expect("写回");
    let after = verify(&mut workspace, "doc_photo", &doctored_path);
    assert_eq!(after["ok"], json!(true), "{after}");
    assert_eq!(
        after["complete"],
        json!(false),
        "照片本体缺失必须判不合格：{after}"
    );
    assert_eq!(after["missing_body"], json!(1), "{after}");
    assert_eq!(after["omitted"], json!(0), "{after}");
    let _ = std::fs::remove_dir_all(&root);
}

/// **第 4 轮第 3 期口径簇 · 水彩 P2 反证边界（导入侧）**：照片本体缺失在
/// 导入时也必须出声 —— 全新工作区（CAS 为空）导入抠掉照片本体的包 ⇒
/// `blobs_unreplayable == 1`（按引用原子数），而不是 `ok` 背后的静默 0。
/// 根因：`materialize_pending_replayable_blobs` 从前只对有 `source` 的原子
/// 计数，无 `source` 的照片 / 媒体原子直接跳过。
///
/// 变异：把无配方分支改回 `continue` ⇒ `blobs_unreplayable` 变 0 ⇒ 红；
/// 在同一工作区重导（CAS 早有本体）⇒ 计数同样变 0 ⇒ 本测试特意用全新
/// 工作区，免得旧 blob 命中掩盖缺失。
#[test]
fn photo_body_loss_surfaces_as_unreplayable_on_import() {
    let root = temp_dir("photoloss");
    let _cleanup = Cleanup(root.clone());
    let (_workspace, pack) = sample_pack(&root, "doc_pl");
    let bytes = std::fs::read(&pack).expect("读回");
    let mut entries = read_tar(&bytes).expect("读回");
    let dropped = entries
        .iter()
        .find(|entry| entry.path.starts_with("blobs/sha256/"))
        .map(|entry| entry.path.clone())
        .expect("样本包应当有本体 blob");
    entries.retain(|entry| entry.path != dropped);
    let doctored = write_tar(&entries);
    let fresh_root = temp_dir("photoloss_fresh");
    let _cleanup_fresh = Cleanup(fresh_root.clone());
    let mut fresh = workspace(&fresh_root, "doc_holder", 64, 64);
    let imported = fresh
        .import_project(&doctored, Some("doc_pl_re"))
        .expect("导入应当成功（只记账，不失败）");
    assert_eq!(
        imported["blobs_unreplayable"].as_u64().unwrap_or(999),
        1,
        "照片本体缺失必须在导入回执里出声：{imported}"
    );
    assert_eq!(
        imported["blobs_replayed"].as_u64().unwrap_or(999),
        0,
        "照片没有重放路径，不应有重放数：{imported}"
    );
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&fresh_root);
}

/// **第 4 轮第 3 期口径簇 · 老日志派生**：缺省包的原子行去掉 `payload.origin`
/// （模拟 `origin` 落账之前的老日志）⇒ 核包仍按 `source` 带笔刷名派生为
/// `drawn` ⇒ `omitted` 与 `complete` 不变。
///
/// 变异：派生分支丢掉 ⇒ 老日志的笔触全翻成 `missing_body` ⇒ 红。
#[test]
fn stripped_origin_on_old_log_still_derives_drawn() {
    let root = temp_dir("oldlog");
    let _cleanup = Cleanup(root.clone());
    let mut workspace = workspace(&root, "doc_old", 200, 160);
    assert_eq!(
        call(
            &mut workspace,
            "doc_old",
            "create_layer",
            json!({ "layer_id": "L" })
        )["ok"],
        json!(true)
    );
    let drawn = pencil_stroke(
        &mut workspace,
        "doc_old",
        "p1",
        &[[20.0, 40.0, 0.5], [100.0, 60.0, 0.6]],
    );
    assert_eq!(drawn["ok"], json!(true), "{drawn}");
    let pack = root.join("old.yanshi");
    let exported = export_to(&mut workspace, "doc_old", &pack, false);
    assert_eq!(exported["ok"], json!(true), "{exported}");
    let before = verify(&mut workspace, "doc_old", &pack);
    assert_eq!(before["complete"], json!(true), "{before}");
    let before_omitted = before["omitted"].as_u64().unwrap();
    assert!(before_omitted >= 1, "缺省包应当有 omitted：{before}");
    // **去掉每条原子行的 `payload.origin`**，其余条目原样重打。
    let bytes = std::fs::read(&pack).expect("读回");
    let mut entries = read_tar(&bytes).expect("读回");
    for entry in entries.iter_mut() {
        if entry.path == "atoms.jsonl" {
            let text = String::from_utf8_lossy(&entry.bytes).into_owned();
            let stripped: Vec<String> = text
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(|line| {
                    let mut atom: Value = serde_json::from_str(line).expect("原子行合法");
                    if let Some(payload) = atom.get_mut("payload") {
                        if let Some(object) = payload.as_object_mut() {
                            object.remove("origin");
                        }
                    }
                    serde_json::to_string(&atom).unwrap()
                })
                .collect();
            entry.bytes = stripped.join("\n").into_bytes();
        }
    }
    let stripped_path = root.join("old_stripped.yanshi");
    std::fs::write(&stripped_path, write_tar(&entries)).expect("写回");
    let after = verify(&mut workspace, "doc_old", &stripped_path);
    assert_eq!(after["ok"], json!(true), "{after}");
    assert_eq!(
        after["complete"],
        json!(true),
        "老日志派生后仍应完整：{after}"
    );
    assert_eq!(
        after["omitted"].as_u64().unwrap(),
        before_omitted,
        "老日志派生后 omitted 应当不变：{after} vs {before}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// **第 4 轮第 3 期口径簇 · `get_stroke_log` 计数**：缺省只返 50 条，
/// 计数必须读 `total`（或传 `limit` 翻页），不能数 `entries` 条数。
///
/// 变异：缺省 limit 被改大 ⇒ `returned` 变 60 ⇒ 红；`total` 被改成页内数 ⇒
/// `total` 变 50 ⇒ 红。
#[test]
fn stroke_log_default_page_is_50_and_total_counts_all() {
    let root = temp_dir("slog");
    let _cleanup = Cleanup(root.clone());
    let mut workspace = workspace(&root, "doc_slog", 400, 300);
    assert_eq!(
        call(
            &mut workspace,
            "doc_slog",
            "create_layer",
            json!({ "layer_id": "L" })
        )["ok"],
        json!(true)
    );
    for index in 0..60 {
        let y = 10.0 + index as f64 * 4.0;
        let drawn = pencil_stroke(
            &mut workspace,
            "doc_slog",
            &format!("p{index}"),
            &[[10.0, y, 0.5], [60.0, y, 0.6]],
        );
        assert_eq!(drawn["ok"], json!(true), "{drawn}");
    }
    let page = call(&mut workspace, "doc_slog", "get_stroke_log", json!({}));
    assert_eq!(page["ok"], json!(true), "{page}");
    assert_eq!(page["returned"], json!(50), "缺省只返 50 条：{page}");
    assert_eq!(page["total"], json!(60), "total 必须计全部 60 笔：{page}");
    assert_eq!(page["truncated"], json!(true), "{page}");
    let all = call(
        &mut workspace,
        "doc_slog",
        "get_stroke_log",
        json!({ "limit": 500 }),
    );
    assert_eq!(all["ok"], json!(true), "{all}");
    assert_eq!(all["returned"], json!(60), "{all}");
    assert_eq!(all["total"], json!(60), "{all}");
    let _ = std::fs::remove_dir_all(&root);
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
