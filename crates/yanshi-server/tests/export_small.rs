//! **工程包体积专题的判据** ✓（目标原话："工程文件体积小，打开速度快" ✓）。
//!
//! 四条判据，每条都能**红** ✓（各自"变异了什么就会红"写在测试头上 ✓）：
//!
//! * (a) `every_package_blob_round_trips...` —— 包里每一个 blob 压了再解**逐字节相同** ✓；
//! * (b) `a_small_export_omits_bitmaps...` —— 缺省导出**不装**可重放位图、包在界内 ✓，
//!   而且**导入重放出来的画面逐像素等于原图** ✓（把位图加回来 ⇒ 体积断言当场红 ✓）；
//! * (c) `a_canvas_reading_brush_blob_is_never_omitted` —— 读画布的笔刷（涂抹类 ✓）
//!   **必须照装** ✓（把它也省掉 ⇒ 这条红 ✓）；
//! * (d) `a_package_with_a_missing_blob_still_opens_and_warns` —— 真缺一个 blob 时
//!   **文档照开、渲染不整体失败、告警说清缺了谁** ✓（把容忍改回 `?` ⇒ 这条红 ✓）。
//!
//! **为什么单独一个文件** ✓：体积专题的取舍（省什么、绝不省什么 ✓）是**一组**判据 ✓，
//! 分散进别的测试会看不出它们是**同一条约束的四个面** ✗。

use serde_json::{json, Value};
use yanshi_core::{Bbox, BlobHash};
use yanshi_server::archive::{read_tar, write_tar, TarEntry};
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

/// **缺省导出的体积上界** ✓ —— 站在"日志只有几十 KB"这一边 ✓。
///
/// 真实 4K 工程（365 MB）的实测结论**不是**这个数 ✗：那份文档里 **19 条介质笔触
/// （142.1 MiB）根本没有 `source`** ✓、**369/376 条带 `source` 的笔触用的是读画布的笔刷**
/// （`oil-03-paint.myb` 的 `smudge = 0.9` ✓）⇒ **重放不出来** ✓ ⇒ 两者都必须照装 ✓。
/// 这条界**只对"笔触全部可重放"的文档**成立 ✓（下面造的正是这种 ✓）。
const SMALL_PACKAGE_BOUND: usize = 256 * 1024;

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("yanshi_small_{}_{}", name, std::process::id()));
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

/// 画一条**可重放**的笔触 ✓（`2B_pencil` 的 `smudge = 0` ⇒ 不读画布 ✓）。
fn pencil_stroke(
    workspace: &mut Workspace,
    doc: &str,
    id: &str,
    size: f64,
    points: &[[f64; 3]],
) -> Value {
    let points: Vec<Value> = points.iter().map(|p| json!([p[0], p[1], p[2]])).collect();
    call(
        workspace,
        doc,
        "brush_stroke",
        json!({
            "layer_id": "L", "object_id": id, "brush": "2B_pencil.myb", "size": size,
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
        json!({"path": path.to_string_lossy(), "doc_id": doc, "include_bitmaps": include}),
    )
}

fn blob_entries(entries: &[TarEntry]) -> Vec<&TarEntry> {
    entries
        .iter()
        .filter(|entry| entry.path.starts_with("blobs/sha256/"))
        .collect()
}

fn build_doc_with_blobs(root: &std::path::Path, doc: &str) -> Workspace {
    let mut workspace = workspace(root, doc, 900, 600);
    assert_eq!(
        call(
            &mut workspace,
            doc,
            "create_layer",
            json!({"layer_id": "L"})
        )["ok"],
        json!(true)
    );
    // 三档尺寸各来一笔 ✓（判据 (a) 要求"每种尺寸档都覆盖到" ✓）：
    // 小（几十 px 的一条 ✓）／中（一条长横线 ✓）／大（横跨大半张画布的折线 ✓）。
    let strokes: [(&str, f64, &[[f64; 3]]); 3] = [
        ("p_small", 6.0, &[[60.0, 40.0, 0.5], [110.0, 60.0, 0.6]]),
        (
            "p_medium",
            40.0,
            &[[40.0, 200.0, 0.4], [400.0, 220.0, 0.9], [820.0, 200.0, 0.5]],
        ),
        (
            "p_large",
            90.0,
            &[
                [40.0, 300.0, 0.5],
                [860.0, 320.0, 0.9],
                [60.0, 560.0, 0.6],
                [860.0, 580.0, 0.7],
            ],
        ),
    ];
    for (id, size, points) in strokes {
        let made = pencil_stroke(&mut workspace, doc, id, size, points);
        assert_eq!(made["ok"], json!(true), "{made}");
    }
    workspace
}

/// **(a) 压了再解逐字节相同** ✓ —— 变异：把 `zlib` 换成有损/会重新解释字节的编码 ⇒ 红 ✓。
#[test]
fn every_package_blob_round_trips_through_the_package_codec() {
    let root = temp_dir("roundtrip");
    let mut workspace = build_doc_with_blobs(&root, "doc_rt");
    let pack = root.join("doc_rt.yanshi");
    // **`include_bitmaps: true`** ✓：这条判据测的是**编码**，所以要保证每个 blob 都在包里 ✓。
    let exported = export_to(&mut workspace, "doc_rt", &pack, true);
    assert_eq!(exported["ok"], json!(true), "{exported}");
    let bytes = std::fs::read(&pack).expect("读回导出的包");
    let entries = read_tar(&bytes).expect("自己的包必须读得回来");
    let encoding = entries
        .iter()
        .find(|entry| entry.path == "blobs.encoding")
        .map(|entry| String::from_utf8_lossy(&entry.bytes).trim().to_owned());
    assert_eq!(
        encoding.as_deref(),
        Some("zlib"),
        "包必须**显式**声明 blobs/ 的编码（不能靠猜）"
    );
    let blobs = blob_entries(&entries);
    assert!(
        blobs.len() >= 3,
        "三档笔触应当至少产生 3 个 blob：{}",
        blobs.len()
    );
    let mut smallest = usize::MAX;
    let mut largest = 0usize;
    for entry in &blobs {
        let hex = entry.path.rsplit('/').next().unwrap().to_owned();
        let plain = yanshi_render::png::zlib_decompress(&entry.bytes)
            .unwrap_or_else(|| panic!("{} 必须解得开", entry.path));
        assert_eq!(
            BlobHash::from_bytes(&plain).hex(),
            hex,
            "解出来的明文必须与路径上的哈希一致（{}）",
            entry.path
        );
        smallest = smallest.min(plain.len());
        largest = largest.max(plain.len());
    }
    assert!(smallest < 64 * 1024, "要有小档（实测最小 {smallest}）");
    assert!(largest > 512 * 1024, "要有大档（实测最大 {largest}）");
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_file(&pack);
}

/// **(b) 缺省导出不装可重放位图、包在界内，且导入重放后画面逐像素等于原图** ✓。
///
/// 变异（两种都会红 ✓）：
/// * 把"省略可重放位图"去掉 ⇒ `blobs/` 回来了 ⇒ 体积断言与"零 blob"断言当场红 ✓；
/// * 把重放写错（例如忘了 `smooth` / 用错颜色）⇒ **逐像素相同**那条红 ✓。
#[test]
fn a_small_export_omits_bitmaps_and_replays_pixel_identical() {
    let root = temp_dir("small");
    let mut workspace = build_doc_with_blobs(&root, "doc_small");
    // 再补 6 笔 ✓ ⇒ "把位图加回来"那一版**必然**超过界 ✓（否则变异测不出来 ✓）。
    for index in 0..6 {
        let row = 40.0 + index as f64 * 90.0;
        let made = pencil_stroke(
            &mut workspace,
            "doc_small",
            &format!("q{index}"),
            90.0,
            &[
                [40.0, row, 0.4],
                [430.0, row + 30.0, 0.9],
                [820.0, row, 0.5],
            ],
        );
        assert_eq!(made["ok"], json!(true), "{made}");
    }
    // **★ 决定性实验：**先渲一个小区域**会不会改变目标区域的输出 ✗ ★**（第 31 轮 ✓）
    //   **∴ 若**会 ⇒ **∴ 进程级缓存**（**below／bitmap ✓）**能改变输出**✗
    //     ⇒ **★ 那**就是 bug 的载体 ✓ ★**** ✓✓
    {
        let before = workspace
            .render_region_raw("doc_small", Bbox::new(0.0, 0.0, 64.0, 64.0))
            .expect("小区域渲染");
        let _ = before;
        let probe_before = workspace
            .render_region_raw("doc_small", Bbox::new(0.0, 0.0, 900.0, 600.0))
            .expect("目标区域（污染前）");
        let _ = workspace
            .render_region_raw("doc_small", Bbox::new(0.0, 0.0, 64.0, 64.0))
            .expect("再次小区域");
        let probe_after = workspace
            .render_region_raw("doc_small", Bbox::new(0.0, 0.0, 900.0, 600.0))
            .expect("目标区域（污染后）");
        eprintln!(
            "DIFF 小区域渲染是否改变目标输出：{}",
            probe_before != probe_after
        );
    }
    let original = workspace
        .render_region_raw("doc_small", Bbox::new(0.0, 0.0, 900.0, 600.0))
        .expect("原图渲染");

    let pack = root.join("small.yanshi");
    let exported = export_to(&mut workspace, "doc_small", &pack, false);
    assert_eq!(exported["ok"], json!(true), "{exported}");
    let bytes = std::fs::read(&pack).expect("读回导出的包");
    assert!(
        bytes.len() < SMALL_PACKAGE_BOUND,
        "缺省导出必须小于 {SMALL_PACKAGE_BOUND} 字节（实测 {} 字节）",
        bytes.len()
    );
    let entries = read_tar(&bytes).expect("读回");
    assert!(
        blob_entries(&entries).is_empty(),
        "可重放的位图一条都不该装（实测 {} 条）",
        blob_entries(&entries).len()
    );
    let build_info = entries
        .iter()
        .find(|entry| entry.path == "BUILD-INFO")
        .map(|entry| String::from_utf8_lossy(&entry.bytes).into_owned())
        .unwrap_or_default();
    assert!(
        !build_info.contains("blobs_omitted_replayable: 0"),
        "BUILD-INFO 必须记下省了几条：{build_info}"
    );

    // **变异对照** ✓：把位图加回来 ⇒ 包必须变大、blob 必须回来（这就是"判据会红"的那一步 ✓）。
    let with_pack = root.join("small_with.yanshi");
    let with_bitmaps = export_to(&mut workspace, "doc_small", &with_pack, true);
    assert_eq!(with_bitmaps["ok"], json!(true), "{with_bitmaps}");
    let with_bytes = std::fs::read(&with_pack).expect("读回带位图的包");
    let with_entries = read_tar(&with_bytes).expect("读回带位图的包");
    assert!(
        !blob_entries(&with_entries).is_empty(),
        "include_bitmaps: true ⇒ 位图必须回来（否则这条变异没有意义）"
    );
    assert!(
        with_bytes.len() > bytes.len(),
        "带位图的包必须更大（{} vs {}）",
        with_bytes.len(),
        bytes.len()
    );

    // **导入另一个工作区** ✓ ⇒ 打开必须能重放回同一幅画 ✓。
    let restore_root = temp_dir("small_restore");
    {
        let mut restored =
            Workspace::with_file_store(restore_root.clone(), DocumentSettings::default())
                .expect("落盘工作区应当能建")
                .with_assets_dir(Some(
                    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
                ));
        let imported = call(
            &mut restored,
            "doc_small",
            "import_project",
            json!({"path": pack.to_string_lossy()}),
        );
        assert_eq!(imported["ok"], json!(true), "{imported}");
        assert!(
            imported["blobs_replayed"].as_u64().unwrap_or(0) > 0,
            "导入必须把省掉的位图重放回来：{imported}"
        );
        // **★ 补不回来的位图数必须为 0 ✗ ★**（第 942 轮 ✓）：
        //   **∴ 为什么单列一条 ✗**：**像素比对**失败时**只说明"**不一样**"✗，
        //     **而**说不出**为什么 ✓ ⇒ **∴ 于是**：**先断言这个计数** ✗**
        //       ⇒ **∴ 失败信息**立刻变成"**有几笔没补回来 ✓" ✓**** ✓✓
        //   **∴ 它**在**导出侧**早有对应计数 ✗**（`kept_mismatch` ✓）
        //     ⇒ **∴ 而**导入侧原来**一声不响 ✓** —— **∴ 本条**就是**把那份沉默**堵上 ✓**** ✓✓
        //   **∴ 变异 ✗**：**让**补回**不比对哈希 ✗**（**直接把重放字节写进去 ✓）
        //     ⇒ **∴ 计数**变 0 ✗** ⇒ **∴ 本条**变绿 ✗** —— **∴ 而**那**是**错的**✗**
        //       （**∴ 会把**另一张图**塞进那个哈希 ✓）⇒ **∴ 所以**本条**与像素比对**必须**同时绿 ✓**** ✓✓
        assert_eq!(
            imported["blobs_unreplayable"].as_u64().unwrap_or(999),
            0,
            "有笔画的位图补不回来 ⇒ 打开后**必然缺图形**：{imported}"
        );
        // **★ 输入侧 diff ✗ ★**（第 28 轮 ✓）：**像素不同时**先问「输入是否相同」 ✓
        //   **∴ 写法上**不用下标** ✗**（**第 27 轮的教训：**`&AtomLog` 不能 `[k]` ✓）**
        //     ⇒ **∴ 用**`iter().zip()`** ✗ ⇒ **∴ 一次编过 ✓**** ✓✓
        {
            let before = workspace.document("doc_small").expect("原文档");
            let after = restored.document("doc_small").expect("重放文档");
            let la: Vec<_> = before.log().iter().collect();
            let lb: Vec<_> = after.log().iter().collect();
            eprintln!("DIFF 原子数：原 {} vs 重放 {}", la.len(), lb.len());
            for (k, (a, b)) in la.iter().zip(lb.iter()).enumerate() {
                let ja = json!({"id": a.id, "seq": a.seq, "kind": a.kind,
                                    "heavy": a.is_heavy(), "payload": a.payload});
                let jb = json!({"id": b.id, "seq": b.seq, "kind": b.kind,
                                    "heavy": b.is_heavy(), "payload": b.payload});
                if ja != jb && k < 3 {
                    eprintln!("DIFF 第 {k} 条：原 = {ja}｜重放 = {jb}");
                }
            }
        }
        // **★ 决定性问题：**同一文档渲染两次会不会不同 ✗ ★**（第 28 轮 ✓）
        //   **∴ 若**不同 ⇒ **∴ 渲染本身**非确定 ✓**（**∴ 与包无关 ✓）**
        //   **∴ 若**相同 ⇒ **∴ 差异**来自**两个工作区的状态差**✗（**且**不在原子日志里 ✓）** ✓✓
        {
            let again = workspace
                .render_region_raw("doc_small", Bbox::new(0.0, 0.0, 900.0, 600.0))
                .expect("原文档二次渲染");
            eprintln!("DIFF 同文档两次渲染是否相同：{}", again == original);
        }
        // **★ 状态级 diff ✗ ★**（第 30 轮 ✓）：**用**折叠状态**定位那个非日志字段 ✓**
        //   **∴ API（**上一轮走错过 ✓）**：**`state_at` 在 `Document` 上 ✗，**不在 `Workspace` 上 ✓**
        //     ⇒ **∴ 走**`document_mut(id)?.state_at(seq)?.state` ✓**** ✓✓
        //   **∴ 已知 ✗**：**日志逐条相同 ＋ 两侧各自确定 ✗，**而**彼此不同 ✓
        //     ⇒ **∴ 差异**必在**不进 `log()` 的状态里 ✓**** ✓✓
        {
            let last = {
                let doc = workspace.document("doc_small").expect("原文档");
                doc.log().iter().map(|a| a.seq).max().unwrap_or(0)
            };
            let sa = workspace
                .document_mut("doc_small")
                .expect("原文档可写")
                .state_at(last)
                .expect("原状态折叠")
                .state;
            let sb = restored
                .document_mut("doc_small")
                .expect("重放文档可写")
                .state_at(last)
                .expect("重放状态折叠")
                .state;
            eprintln!("DIFF 折叠状态相等：{}", sa == sb);
            if sa != sb {
                let ja = serde_json::to_value(&sa).unwrap_or(json!(null));
                let jb = serde_json::to_value(&sb).unwrap_or(json!(null));
                if let (Some(ma), Some(mb)) = (ja.as_object(), jb.as_object()) {
                    for k in ma.keys().chain(mb.keys()) {
                        if ma.get(k) != mb.get(k) {
                            let x = ma.get(k).map(|v| v.to_string()).unwrap_or_default();
                            let y = mb.get(k).map(|v| v.to_string()).unwrap_or_default();
                            eprintln!(
                                "DIFF 状态键 {k}：原 {}｜重放 {}",
                                &x[..x.len().min(220)],
                                &y[..y.len().min(220)]
                            );
                        }
                    }
                }
            }
        }
        {
            let again = restored
                .render_region_raw("doc_small", Bbox::new(0.0, 0.0, 900.0, 600.0))
                .expect("重放侧二次渲染");
            eprintln!("DIFF 重放侧与原图是否相同：{}", again == original);
        }
        let replayed = restored
            .render_region_raw("doc_small", Bbox::new(0.0, 0.0, 900.0, 600.0))
            .expect("重放渲染");
        assert_eq!(
            replayed, original,
            "重放出来的画面必须与原图**逐像素相同**（宽高 {}×{} vs {}×{}）",
            replayed.0, replayed.1, original.0, original.1
        );
    }
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&restore_root);
    let _ = std::fs::remove_file(&pack);
    let _ = std::fs::remove_file(&with_pack);
}

/// **(c) 读画布的笔刷必须照装** ✓ —— 变异：把 `brush_reads_the_canvas` 那一条判据去掉 ⇒ 红 ✓。
///
/// **为什么这条最重要** ✗：省掉它就是"打开是错的画" ✓（`oil-01-paint` 的 `smudge = 0.4` ✓
/// ⇒ 烘出来的像素取决于落笔时底下是什么 ✓ ⇒ `source` 不是重放配方 ✓）。
#[test]
fn a_canvas_reading_brush_blob_is_never_omitted() {
    let root = temp_dir("smudge");
    let mut workspace = workspace(&root, "doc_smudge", 400, 300);
    assert_eq!(
        call(
            &mut workspace,
            "doc_smudge",
            "create_layer",
            json!({"layer_id": "L"})
        )["ok"],
        json!(true)
    );
    let made = call(
        &mut workspace,
        "doc_smudge",
        "brush_stroke",
        json!({"layer_id": "L", "object_id": "s1", "brush": "oil-01-paint.myb", "size": 30,
               "color": {"r": 200, "g": 40, "b": 40, "a": 255},
               "points": [[60.0, 100.0, 1.0], [340.0, 180.0, 1.0]]}),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    let pack = root.join("smudge.yanshi");
    let exported = export_to(&mut workspace, "doc_smudge", &pack, false);
    assert_eq!(exported["ok"], json!(true), "{exported}");
    let bytes = std::fs::read(&pack).expect("读回");
    let entries = read_tar(&bytes).expect("读回");
    assert!(
        !blob_entries(&entries).is_empty(),
        "读画布的笔刷重放不出来 ⇒ 它的位图**必须**留在包里 ✓（省了它 ⇒ 打开就是错的画 ✗）"
    );
    // **而且必须"因为判据拒绝"才留下** ✓（不是碰巧重跑对不上 ✓）。
    //
    // **为什么非要这一条** ✗：我第一版只断言"blob 还在" ✓ —— 实测把
    // `brush_reads_the_canvas` 那条判据**整条删掉**，这一条**照样绿** ✗：
    // 省不掉的原因变成了"重跑哈希对不上" ✓（另一条独立防线 ✓）⇒ 判据没有钉住它该钉的东西 ✗。
    // ⇒ 让导出把**两个理由分别记下来** ✓，判据直接钉"配方不可重放"这一个 ✓。
    // **变异**：把 `brush_source_is_replayable` 里的 `!brush_reads_the_canvas(&parsed)` 去掉
    // ⇒ `blobs_kept_no_replayable_recipe` 变 0 ⇒ 本条红 ✓。
    let build_info = entries
        .iter()
        .find(|entry| entry.path == "BUILD-INFO")
        .map(|entry| String::from_utf8_lossy(&entry.bytes).into_owned())
        .unwrap_or_default();
    assert!(
        build_info.contains("blobs_kept_no_replayable_recipe: 1"),
        "读画布的那一条必须被**配方判据**拦下：{build_info}"
    );
    assert!(
        build_info.contains("blobs_omitted_replayable: 0"),
        "读画布的笔刷一条都不该被省：{build_info}"
    );
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_file(&pack);
}

/// **(f) 配方"重跑得出来吗"必须**真的比哈希** ✗ —— 不是"有配方就信" ✓。
///
/// **做法** ✓：直接提交一张**伪造**的位图 ✓，却给它配一份**看起来完全正常**的笔刷配方 ✓
/// （`2B_pencil` + 点列 + size + color ✓ —— 也就是**非**读画布、**完全可重放**的那种 ✓）。
/// 重跑一遍当然画不出那张伪造位图 ✓ ⇒ 导出必须**照装**它 ✓。
///
/// **变异**：把导出里那句 `BlobHash::from_bytes(&replayed) == hash` 改成永远为真
/// （或整条去掉 ✓）⇒ 伪造位图被省掉 ⇒ 包里没有它 ⇒ 本条红 ✓
/// —— **这正是判据 (c) 做不到的那一种单点变异** ✓（(c) 被"读画布"那条判据挡住了 ✓，走不到这里 ✓）。
#[test]
fn a_blob_whose_recorded_recipe_does_not_reproduce_it_is_kept() {
    let root = temp_dir("fake_recipe");
    let mut workspace = workspace(&root, "doc_fake", 400, 300);
    assert_eq!(
        call(
            &mut workspace,
            "doc_fake",
            "create_layer",
            json!({"layer_id": "L"})
        )["ok"],
        json!(true)
    );
    // 一张**一眼看得出不是笔画**的 8×8 位图 ✓（对角条纹 ✓）⇒ 任何笔刷都重跑不出它 ✓。
    let mut rgba = vec![0u8; 8 * 8 * 4];
    for y in 0..8usize {
        for x in 0..8usize {
            let at = (y * 8 + x) * 4;
            let on = u8::from((x + y) % 2 == 0) * 200;
            rgba[at] = on;
            rgba[at + 1] = 255 - on;
            rgba[at + 2] = 60;
            rgba[at + 3] = 255;
        }
    }
    let hash = workspace.store().put(&rgba).expect("把伪造位图放进 CAS");
    // **直接提交原子** ✓（不走 `import_image` 工具 ✓）：工具的参数表里没有 `source` ✓
    //（笔触那条路是**内部**直接调 `write_import_image` ✓，本来就不经过参数校验 ✓）——
    // 这里要造的正是"日志里有一条**配方与像素对不上**的原子" ✓，用哪条路提交不影响导出侧的判据 ✓。
    let payload = json!({
        "object_id": "fake1",
        "layer_id": "L",
        "type": "raster_patch",
        "bitmap": {
            "blob_hash": hash.to_string(),
            "size": rgba.len(),
            "mime_type": "image/x-yanshi-raw",
        },
        "region": {"x": 20.0, "y": 20.0, "w": 8.0, "h": 8.0},
        "width": 8,
        "height": 8,
        // **配方完全是可重放的样子** ✓（非读画布 ✓、参数齐全 ✓）—— 就是画不出上面的像素 ✓。
        "source": {
            "kind": "brush", "brush": "2B_pencil.myb",
            "points": [[10.0, 10.0, 0.5], [90.0, 60.0, 0.8]],
            "size": 12.0, "color": {"r": 10, "g": 20, "b": 30, "a": 255},
            "color_to": null, "smooth": false, "opacity": null, "hardness": null, "seed": 0
        }
    });
    let atom = yanshi_core::Atom::new(
        yanshi_core::AtomKind::ImportImage,
        "human:1",
        "session:test",
        payload,
    );
    workspace
        .commit("doc_fake", atom, "human:1", true)
        .expect("提交伪造位图原子");
    let pack = root.join("fake.yanshi");
    let exported = export_to(&mut workspace, "doc_fake", &pack, false);
    assert_eq!(exported["ok"], json!(true), "{exported}");
    let bytes = std::fs::read(&pack).expect("读回");
    let entries = read_tar(&bytes).expect("读回");
    let kept = blob_entries(&entries)
        .iter()
        .any(|entry| entry.path.ends_with(hash.hex()));
    assert!(
        kept,
        "重跑哈希对不上 ⇒ 伪造位图**必须**留在包里（否则打开就是另一张图 ✗）"
    );
    let build_info = entries
        .iter()
        .find(|entry| entry.path == "BUILD-INFO")
        .map(|entry| String::from_utf8_lossy(&entry.bytes).into_owned())
        .unwrap_or_default();
    assert!(
        build_info.contains("blobs_kept_replay_mismatch: 1"),
        "要记下「因为重跑对不上而留下」这一条：{build_info}"
    );
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_file(&pack);
}

/// **(d) 真缺一个 blob 时：文档照开、渲染不整体失败、告警说清缺了谁** ✓。
///
/// 变异：把渲染里那句"跳过 + 记 `unsupported`"改回 `store.get(&blob)?` ⇒ 渲染整体报错 ⇒ 红 ✓。
#[test]
fn a_package_with_a_missing_blob_still_opens_and_warns() {
    let root = temp_dir("missing");
    let mut workspace = workspace(&root, "doc_missing_blob", 400, 300);
    assert_eq!(
        call(
            &mut workspace,
            "doc_missing_blob",
            "create_layer",
            json!({"layer_id": "L"})
        )["ok"],
        json!(true)
    );
    // **故意用读画布的笔刷** ✓：导入端**不会**（也不该 ✓）替它重放 ✓ ⇒ 缺了就是真缺 ✓。
    let made = call(
        &mut workspace,
        "doc_missing_blob",
        "brush_stroke",
        json!({"layer_id": "L", "object_id": "s1", "brush": "oil-01-paint.myb", "size": 30,
               "color": {"r": 20, "g": 120, "b": 220, "a": 255},
               "points": [[60.0, 100.0, 1.0], [340.0, 180.0, 1.0]]}),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    let pack = root.join("missing.yanshi");
    let exported = export_to(&mut workspace, "doc_missing_blob", &pack, false);
    assert_eq!(exported["ok"], json!(true), "{exported}");

    // **手工把那条 blob 抠掉** ✓（模拟"包传丢了 / 被裁过" ✓）。
    let bytes = std::fs::read(&pack).expect("读回");
    let mut entries = read_tar(&bytes).expect("读回");
    let dropped = entries
        .iter()
        .find(|entry| entry.path.starts_with("blobs/sha256/"))
        .map(|entry| entry.path.clone())
        .expect("这个文档本来应当有 blob");
    // 告警里用的是 `sha256:<hex>`（不是路径 ✓）⇒ 断言要对上那一个 ✓。
    let dropped_hash = format!("sha256:{}", dropped.rsplit('/').next().unwrap());
    entries.retain(|entry| entry.path != dropped);
    let doctored = write_tar(&entries);
    let doctored_path = root.join("missing_doctored.yanshi");
    std::fs::write(&doctored_path, &doctored).expect("写回");

    let restore_root = temp_dir("missing_restore");
    let mut restored =
        Workspace::with_file_store(restore_root.clone(), DocumentSettings::default())
            .expect("落盘工作区应当能建")
            .with_assets_dir(Some(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
            ));
    let imported = call(
        &mut restored,
        "doc_missing_blob",
        "import_project",
        json!({"path": doctored_path.to_string_lossy()}),
    );
    assert_eq!(
        imported["ok"],
        json!(true),
        "缺一个 blob **不该**让整个包导不进来（文档照开 ✓）：{imported}"
    );
    // **渲染不许整体失败** ✓，而且必须**说清缺了谁** ✓（不静默 ✓）。
    let rendered = call(
        &mut restored,
        "doc_missing_blob",
        "render_region",
        json!({"region": {"x": 0, "y": 0, "w": 400, "h": 300}}),
    );
    assert_eq!(
        rendered["ok"],
        json!(true),
        "缺一个补丁不该让整幅渲染失败：{rendered}"
    );
    let warnings = rendered["warnings"].to_string();
    assert!(
        warnings.contains("缺少 blob") && warnings.contains(&dropped_hash),
        "告警必须说清缺的是哪一个 blob（要找 {dropped_hash}；实际告警：{warnings}）"
    );
    // **裸像素那条出口也必须说** ✗（`export_png` 走 `render_region_raw` ✓ —— 它的返回类型
    // 没有告警位 ✓ ⇒ 必须靠 `Document::last_render_warnings` ✓ 带出去 ✓）。
    // 变异：把 `export_png` 的 `"warnings"` 去掉 ⇒ 这条红 ✓。
    let exported_png = call(
        &mut restored,
        "doc_missing_blob",
        "export_png",
        json!({"path": restore_root.join("missing.png").to_string_lossy(), "doc_id": "doc_missing_blob"}),
    );
    assert_eq!(exported_png["ok"], json!(true), "{exported_png}");
    let png_warnings = exported_png["warnings"].to_string();
    assert!(
        png_warnings.contains("缺少 blob") && png_warnings.contains(&dropped_hash),
        "裸像素出口（export_png）也必须把缺块报出来（实际：{png_warnings}）"
    );
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&restore_root);
    let _ = std::fs::remove_file(&pack);
    let _ = std::fs::remove_file(&doctored_path);
}

/// **(e) `--root` 还原那条路也必须能读回包里的 blob** ✓ —— 这是**产品实际用的**那条路
/// （README：把包解开到 `<root>/` ✓），而它**不经过 `import_project`** ✗。
///
/// **它同时是"又写回明文"的探针** ✓：本地 CAS（`FsBlobStore` ＋ `RenderCodec` ✓）只按
/// **编解码器**读 blob ✓ ⇒ 导出若写明文 ✓，这里 `open_document` 就会因为解不开而失败 ✓
/// 或返回错字节 ⇒ 逐像素比对红 ✓。变异：把 `export_project` 的 `zlib_compress_best` 去掉 ⇒ 红 ✓。
#[test]
fn a_package_restores_through_root_and_renders_identically() {
    let root = temp_dir("root_restore");
    let mut workspace = build_doc_with_blobs(&root, "doc_root");
    // 再加几笔 ✓ ⇒ 缺省导出会省掉它们的位图 ✓ ⇒ 这条路必须**重放**出来 ✓。
    for index in 0..3 {
        let made = pencil_stroke(
            &mut workspace,
            "doc_root",
            &format!("r{index}"),
            60.0,
            &[
                [60.0, 60.0 + index as f64 * 40.0, 0.5],
                [840.0, 100.0 + index as f64 * 40.0, 0.8],
            ],
        );
        assert_eq!(made["ok"], json!(true), "{made}");
    }
    // **再补一笔"读画布"的笔触** ✓（`oil-01-paint` 的 `smudge = 0.4` ✓ ⇒ **省不掉** ✓
    // ⇒ 包里**真的会有一个 blob** ✓）。**没有它这条判据就是空转的** ✗：
    // 上面那些笔触全都会被省掉 ✓ ⇒ 包里一个 blob 都没有 ✓ ⇒ 本地 CAS 的编解码器
    // **根本没被走到** ✓ —— 实测过：把导出改回"写明文"这条判据**照样绿** ✗（那次变异就是这么暴露的 ✓）。
    let oil = call(
        &mut workspace,
        "doc_root",
        "brush_stroke",
        json!({"layer_id": "L", "object_id": "oil1", "brush": "oil-01-paint.myb", "size": 40,
               "color": {"r": 200, "g": 60, "b": 40, "a": 255},
               "points": [[80.0, 520.0, 1.0], [820.0, 560.0, 1.0]]}),
    );
    assert_eq!(oil["ok"], json!(true), "{oil}");
    let original = workspace
        .render_region_raw("doc_root", Bbox::new(0.0, 0.0, 900.0, 600.0))
        .expect("原图渲染");
    let pack = root.join("root.yanshi");
    let exported = export_to(&mut workspace, "doc_root", &pack, false);
    assert_eq!(exported["ok"], json!(true), "{exported}");
    let bytes = std::fs::read(&pack).expect("读回");
    let entries = read_tar(&bytes).expect("读回");
    assert!(
        !blob_entries(&entries).is_empty(),
        "必须至少有一个**省不掉**的 blob 留在包里 ⇒ 否则这条判据走不到本地 CAS 的编解码器 ✗"
    );

    // **按 README 的还原方式铺一个 `--root`** ✓（文档在 `docs/<doc_id>/` ✓、CAS 在 `blobs/` ✓）。
    let restore_root = temp_dir("root_restore_target");
    std::fs::create_dir_all(restore_root.join("docs/doc_root")).unwrap();
    for entry in &entries {
        if entry.path == "atoms.jsonl" || entry.path == "meta.json" {
            std::fs::write(
                restore_root.join("docs/doc_root").join(&entry.path),
                &entry.bytes,
            )
            .unwrap();
        } else if let Some(rest) = entry.path.strip_prefix("blobs/") {
            // **逐字节照抄** ✓ —— 这一条正是"包里的 blob 与本地 CAS 同格式"的检验 ✓。
            let target = restore_root.join("blobs").join(rest);
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::write(&target, &entry.bytes).unwrap();
        }
    }
    // **走真正的产品读回路径** ✓：`Workspace::with_file_store(root)`（内部是
    // `FsBlobStore` ＋ `RenderCodec` ✓）⇒ `open_document` ⇒ 渲染 ✓。
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut restored =
        Workspace::with_file_store(restore_root.clone(), DocumentSettings::default())
            .expect("落盘工作区应当能建")
            .with_assets_dir(Some(assets));
    let opened = restored.open_document("doc_root");
    assert!(
        opened.is_ok(),
        "把包解开到 --root 之后必须能打开（blob 编码必须与本地 CAS 一致）：{:?}",
        opened.err()
    );
    // **省掉的位图要在打开时补回来** ✓（否则画面是静默不完整的 ✓ —— 这条就是那个判据 ✓）。
    let replayed = restored
        .document("doc_root")
        .map(|document| document.replayed_blobs())
        .unwrap_or(0);
    assert!(
        replayed > 0,
        "缺省导出省掉了位图 ⇒ `--root` 打开时必须重放补回来（实测补了 {replayed} 条）"
    );
    let rendered = restored
        .render_region_raw("doc_root", Bbox::new(0.0, 0.0, 900.0, 600.0))
        .expect("--root 还原后必须渲染得出来");
    assert_eq!(
        rendered, original,
        "`--root` 还原后重放出来的画面必须与原图逐像素相同"
    );
    let warn = restored.last_render_warnings("doc_root");
    assert!(warn.is_empty(), "不该有缺块告警（实测 {warn:?}）");
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&restore_root);
    let _ = std::fs::remove_file(&pack);
}
