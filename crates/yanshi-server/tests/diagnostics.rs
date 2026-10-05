//! **诊断包的判据** ✓（P0 事故复盘：事发时**没有任何东西可收** ✗）。
//!
//! 这些测试守的是根要求：**出事了能把必要信息打成一个 zip 交出来** ✓，
//! 而且其中的 stderr、去密、被跳过的补丁告警**都真的在里面** ✓。
//!
//! 每条断言都能红 ✓（怎么红写在各自的注释里 ✓）；它们都断言**语义**（条目清单 / 计数 /
//! 真实字节里有没有某个串 ✓），不赌时序 ✓。

use std::io::Read;
use std::sync::Mutex;

use serde_json::{json, Value};
use yanshi_server::archive::{read_tar, write_tar};
use yanshi_server::diagnostics::{self, DiagnosticsRequest, Limits, SurfaceFacts};
use yanshi_server::{DocumentSettings, Profile, ToolContext, ToolRegistry, Workspace};

/// **碰全局 stderr 环的测试必须串行** ✓：环是**进程级**的 ✓（那正是它能救命的性质 ✓），
/// 而 cargo 默认并发跑测试 ✓ ⇒ 不串行的话，一个测试的 `clear_logs` 会把另一个的哨兵洗掉 ✗。
static RING_TESTS: Mutex<()> = Mutex::new(());

/// **这条判据自己抄一份必需条目清单** ✓（**不**引用 `ENTRY_NAMES` ✓）。
///
/// **为什么故意抄** ✗：若拿实现里的常量当期望值 ✓，那"把实现里的条目删掉"会**同时**改期望 ✓
/// ⇒ 判据永远绿 ✓ —— 那是自欺 ✓。抄一份 ⇒ **实现与规格分叉就红** ✓。
const REQUIRED_ENTRIES: &[&str] = &[
    "README.txt",
    "build.json",
    "config.json",
    "surface.json",
    "document.json",
    "atoms.jsonl",
    "atoms.meta.json",
    "warnings.json",
    "timings.json",
    "stderr.log",
    "stderr.meta.json",
    "thumbnail.json",
    "thumbnail.bin",
    "privacy.json",
];

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("yanshi_diag_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path
}

fn workspace(root: &std::path::Path, doc: &str, width: u32, height: u32) -> Workspace {
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut workspace = Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default())
        .expect("落盘工作区应当能建")
        .with_assets_dir(Some(assets));
    workspace
        .create_document(
            yanshi_server::NewDocument::new(doc, width, height),
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

fn request(doc: &str, facts: SurfaceFacts) -> DiagnosticsRequest {
    DiagnosticsRequest {
        doc_id: doc.to_owned(),
        facts,
        limits: Limits::default(),
    }
}

/// **把 zip 读成一个 map** ✓ —— 用**真的 zip 读取器**（`zip` crate ✓），
/// 而不是我们自己写的解析 ✓ ⇒ "能被真实读取器打开"这件事是**被证明**的 ✓。
fn unzip(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec()))
        .expect("zip 必须能被真实读取器打开");
    let mut entries = Vec::new();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).expect("条目可读");
        let name = file.name().to_owned();
        let mut content = Vec::new();
        file.read_to_end(&mut content).expect("条目内容可读");
        entries.push((name, content));
    }
    entries
}

fn entry<'a>(entries: &'a [(String, Vec<u8>)], name: &str) -> &'a [u8] {
    entries
        .iter()
        .find(|(entry, _)| entry == name)
        .map(|(_, bytes)| bytes.as_slice())
        .unwrap_or_else(|| panic!("缺少条目 {name}；实际条目：{:?}", names_of(entries)))
}

fn names_of(entries: &[(String, Vec<u8>)]) -> Vec<String> {
    entries.iter().map(|(name, _)| name.clone()).collect()
}

/// **判据 1：归档被产出、能被真实 zip 读取器读、且包含每一条必需条目** ✓。
///
/// **能红**：把 `ENTRY_NAMES` 里任一名字（或 `collect` 里对应的 `push`）去掉 ⇒ 红 ✓；
/// 把 `write_zip` 的 `CompressionMethod::Deflated` 改坏 ⇒ 读取器打不开 ⇒ 红 ✓。
#[test]
fn archive_is_a_real_zip_with_every_required_entry() {
    let root = temp_dir("c1");
    let doc = "doc_diag";
    let mut ws = workspace(&root, doc, 320, 240);
    let made = call(
        &mut ws,
        doc,
        "create_layer",
        json!({"layer_id": "L", "name": "底"}),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    let made = call(&mut ws, doc, "create_layer", json!({"layer_id": "L2"}));
    assert_eq!(made["ok"], json!(true), "{made}");
    let made = call(
        &mut ws,
        doc,
        "draw_shape",
        json!({
            "layer_id": "L",
            "object_id": "shape1",
            "data": {"geometry": {"kind": "rect", "bbox": {"x": 10, "y": 10, "w": 60, "h": 40}},
                     "color": {"r": 200, "g": 30, "b": 60, "a": 255}}
        }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");

    let facts = SurfaceFacts {
        surface: "test".to_owned(),
        build: json!({"version": "0.1.0", "commit": "test"}),
        config: json!({"profiles": ["core"], "width": 320, "height": 240}),
        extra: json!({"requests": 3}),
        secrets: Vec::new(),
    };
    let bundle = diagnostics::collect(&ws, &request(doc, facts)).expect("采集应当成功");
    let zip = bundle.zip().expect("打包应当成功");

    // ① 体积有上限 ✓（硬要求：截断而不是无界增长 ✓）。
    assert!(
        zip.len() <= diagnostics::MAX_ARCHIVE_BYTES,
        "zip {} 字节超过上限 {}",
        zip.len(),
        diagnostics::MAX_ARCHIVE_BYTES
    );
    // ② 真实读取器能打开，且条目清单**逐字**等于**规格里抄来的**清单 ✓。
    let entries = unzip(&zip);
    let expected: Vec<String> = REQUIRED_ENTRIES
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    assert_eq!(
        names_of(&entries),
        expected,
        "包内条目必须与规格清单逐字一致（两面的条目由此不漂移）"
    );
    // 规格清单与实现里的公开常量也必须一致 ✓（两处都写下来 ⇒ 任何一处被改都会红 ✓）。
    assert_eq!(
        diagnostics::ENTRY_NAMES,
        REQUIRED_ENTRIES,
        "公开的 ENTRY_NAMES 必须等于规格清单"
    );
    // ③ 文档元数据真的有这次的内容 ✓。
    let document: Value = serde_json::from_slice(entry(&entries, "document.json")).expect("JSON");
    assert_eq!(document["doc_id"], json!(doc));
    assert_eq!(document["width"], json!(320));
    assert_eq!(document["height"], json!(240));
    assert!(
        document["head_seq"].as_u64().unwrap_or(0) >= 2,
        "head_seq 应已推进：{document}"
    );
    assert!(
        document["atoms"].as_u64().unwrap_or(0) >= 2,
        "原子数应 ≥2：{document}"
    );
    // ④ 原子日志是 JSONL，且每行都是合法 JSON ✓。
    let atoms = String::from_utf8_lossy(entry(&entries, "atoms.jsonl")).into_owned();
    assert!(!atoms.trim().is_empty(), "原子日志不应为空");
    for line in atoms.lines() {
        serde_json::from_str::<Value>(line)
            .unwrap_or_else(|error| panic!("原子行不是 JSON：{error}"));
    }
    // ⑤ 包内 README 要**逐条说明**每个条目 ✓（"不在场的人靠它就能看懂" ✓）。
    let readme = String::from_utf8_lossy(entry(&entries, "README.txt")).into_owned();
    for name in REQUIRED_ENTRIES {
        assert!(readme.contains(name), "README 没说明条目 {name}");
    }
    assert!(readme.contains("没采到什么") || readme.contains("没**采到"));

    let _ = std::fs::remove_dir_all(&root);
}

/// **判据 2：采集之前的 stderr 行出现在包里；缓冲不会无界增长** ✓。
///
/// **能红**：把 `LogRing::push` 里的容量裁剪去掉 ⇒ `log_stats().len` 会超过容量 ⇒ 红 ✓；
/// 把 `collect` 里 `stderr.log` 那个条目去掉 ⇒ 找不到哨兵行 ⇒ 红 ✓。
#[test]
fn stderr_ring_is_bounded_and_pre_collection_lines_are_in_the_archive() {
    let _guard = RING_TESTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    diagnostics::clear_logs();
    // 先灌爆环：容量之外的行**必须**被丢掉 ✓。
    for index in 0..(diagnostics::LOG_RING_CAPACITY + 50) {
        diagnostics::log_line(format!("flood-{index}"));
    }
    // 再写一条**唯一**哨兵（它在采集之前写出 ✓ ⇒ 必须出现在包里 ✓）。
    let sentinel = "SENTINEL-BEFORE-COLLECT-9f3a";
    diagnostics::log_line(format!("这是采集前写下的一行：{sentinel}"));

    let stats = diagnostics::log_stats();
    assert_eq!(stats.capacity, diagnostics::LOG_RING_CAPACITY);
    assert!(
        stats.len <= stats.capacity,
        "环形缓冲 {} 行 > 容量 {} 行 ⇒ 无界增长",
        stats.len,
        stats.capacity
    );
    assert!(stats.dropped >= 50, "被覆盖的行数应当 ≥50：{stats:?}");
    assert!(
        diagnostics::log_lines()
            .iter()
            .any(|line| line.contains(sentinel)),
        "哨兵行应当在环里（否则本判据测的不是它）"
    );

    let root = temp_dir("c2");
    let doc = "doc_ring";
    let ws = workspace(&root, doc, 64, 64);
    let bundle = diagnostics::collect(
        &ws,
        &request(
            doc,
            SurfaceFacts {
                surface: "test".to_owned(),
                ..SurfaceFacts::default()
            },
        ),
    )
    .expect("采集应当成功");
    let zip = bundle.zip().expect("打包应当成功");
    let entries = unzip(&zip);
    let stderr = String::from_utf8_lossy(entry(&entries, "stderr.log")).into_owned();
    assert!(
        stderr.contains(sentinel),
        "采集之前写下的 stderr 行必须出现在包里；实际尾部：{}",
        stderr.lines().rev().take(3).collect::<Vec<_>>().join(" | ")
    );
    let meta: Value = serde_json::from_slice(entry(&entries, "stderr.meta.json")).expect("JSON");
    assert_eq!(meta["ring_capacity"], json!(diagnostics::LOG_RING_CAPACITY));
    assert!(
        meta["dropped_lines"].as_u64().unwrap_or(0) >= 50,
        "meta 要如实报出被覆盖的行数：{meta}"
    );
    assert!(meta["buffered_lines"].as_u64().unwrap_or(0) <= diagnostics::LOG_RING_CAPACITY as u64);

    let _ = std::fs::remove_dir_all(&root);
}

/// **判据 3：归档里不出现令牌 / 机密 / 家目录路径 —— 断言在真实字节上** ✓。
///
/// **能红**：把 `collect` 里那段 `for entry in &mut entries { … redactor.text … }` 去掉 ⇒
/// 哨兵会留在条目里 ⇒ 红 ✓；把 `Redactor::replace_home` 去掉 ⇒ `/home/alice` 出现 ⇒ 红 ✓。
#[test]
fn archive_contains_no_tokens_secrets_or_home_paths() {
    let _guard = RING_TESTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    diagnostics::clear_logs();
    let secret = "SUPER-SECRET-a1b2c3d4e5";
    let home = "/home/alice/private/yanshi";
    // **负向对照**：先证明"这段输入里确实有机密" ✓ —— 否则"包里没有"可能只是因为压根没放进去 ✓。
    let logged = format!("Authorization: Bearer {secret} ｜ root={home}");
    diagnostics::log_line(&logged);
    assert!(
        diagnostics::log_lines()
            .iter()
            .any(|line| line.contains(secret)),
        "哨兵必须先存在于输入里，这条判据才有意义"
    );

    let root = temp_dir("c3");
    let doc = "doc_secret";
    let ws = workspace(&root, doc, 64, 64);
    let facts = SurfaceFacts {
        surface: "test".to_owned(),
        build: json!({"api_key": secret}),
        config: json!({
            "root": home,
            "note": format!("token={secret}"),
            "nested": {"password": secret, "safe": "this is fine"},
        }),
        extra: json!({"authorization": format!("Bearer {secret}")}),
        secrets: vec![secret.to_owned()],
    };
    let bundle = diagnostics::collect(&ws, &request(doc, facts)).expect("采集应当成功");
    let zip = bundle.zip().expect("打包应当成功");

    // ① **先扫真实 zip 字节** ✓（压缩后的字节里也不许出现 ✓）。
    assert!(
        !contains_bytes(&zip, secret.as_bytes()),
        "机密出现在 zip 的原始字节里"
    );
    assert!(
        !contains_bytes(&zip, home.as_bytes()),
        "绝对家目录路径出现在 zip 的原始字节里"
    );
    assert!(
        !contains_bytes(&zip, b"/home/alice"),
        "家目录的用户名出现在 zip 的原始字节里"
    );
    // ② 再逐条扫**解压后**的内容 ✓（压缩可能把串拆开，所以两条都要 ✓）。
    let entries = unzip(&zip);
    for (name, bytes) in &entries {
        let text = String::from_utf8_lossy(bytes);
        assert!(!text.contains(secret), "条目 {name} 里有机密");
        assert!(!text.contains("/home/alice"), "条目 {name} 里有家目录路径");
        assert!(
            !text.contains("Bearer SUPER-SECRET"),
            "条目 {name} 里有 Bearer 令牌"
        );
    }
    // ③ 去密报告要如实 ✓，且无害内容不被误伤 ✓。
    let privacy: Value = serde_json::from_slice(entry(&entries, "privacy.json")).expect("JSON");
    assert!(
        privacy["secrets_replaced"].as_u64().unwrap_or(0) >= 1,
        "去密报告应当报告至少一处：{privacy}"
    );
    assert!(
        privacy["home_paths_replaced"].as_u64().unwrap_or(0) >= 1,
        "家目录替换应当至少一处：{privacy}"
    );
    let config = String::from_utf8_lossy(entry(&entries, "config.json")).into_owned();
    assert!(
        config.contains("this is fine"),
        "去密不该把正常内容一起抹掉：{config}"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// **判据 4：补丁被跳过时，告警出现在包里** ✓。
///
/// 情形照 `preview_warnings.rs` 造 ✓（仓库已经知道怎么造"不可读的 blob" ✓）：
/// 导出工程包 ⇒ 抠掉一条 blob ⇒ 导入 ⇒ 渲染 ⇒ `warnings.json` 必须报出缺哪一个 blob ✓。
///
/// **能红**：把 `collect` 里 `warnings.json` 的 `last_render_warnings` 去掉 ⇒ 红 ✓。
#[test]
fn skipped_patch_warnings_are_in_the_archive() {
    let root = temp_dir("c4src");
    let doc = "doc_skipped";
    let mut source = workspace(&root, doc, 400, 300);
    assert_eq!(
        call(&mut source, doc, "create_layer", json!({"layer_id": "L"}))["ok"],
        json!(true)
    );
    // **故意用"读画布"的笔刷** ✓（`oil-01-paint` 的 `smudge > 0` ✓）：导入端重放不出它 ✓
    // ⇒ 那条 blob 被抠掉之后就是**真缺** ✓。
    let made = call(
        &mut source,
        doc,
        "brush_stroke",
        json!({"layer_id": "L", "object_id": "s1", "brush": "oil-01-paint.myb", "size": 40,
               "color": {"r": 20, "g": 120, "b": 220, "a": 255},
               "points": [[60.0, 100.0, 1.0], [340.0, 180.0, 1.0]]}),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    let pack = root.join("pack.yanshi");
    let exported = call(
        &mut source,
        doc,
        "export_project",
        json!({"path": pack.to_string_lossy(), "doc_id": doc, "include_bitmaps": false}),
    );
    assert_eq!(exported["ok"], json!(true), "{exported}");

    // 抠掉一条 blob ⇒ 真缺。
    let bytes = std::fs::read(&pack).expect("读回包");
    let mut tar = read_tar(&bytes).expect("解包");
    let dropped = tar
        .iter()
        .find(|entry| entry.path.starts_with("blobs/sha256/"))
        .map(|entry| entry.path.clone())
        .expect("这个文档应当有 blob");
    let dropped_hash = format!("sha256:{}", dropped.rsplit('/').next().unwrap());
    tar.retain(|entry| entry.path != dropped);
    let doctored = root.join("doctored.yanshi");
    std::fs::write(&doctored, write_tar(&tar)).expect("写回包");

    let restore_root = temp_dir("c4restore");
    let mut restored =
        Workspace::with_file_store(restore_root.clone(), DocumentSettings::default())
            .expect("落盘工作区应当能建")
            .with_assets_dir(Some(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
            ));
    let imported = call(
        &mut restored,
        doc,
        "import_project",
        json!({"path": doctored.to_string_lossy(), "doc_id": doc}),
    );
    assert_eq!(imported["ok"], json!(true), "{imported}");

    // 渲染一次 ⇒ 那条"画不出来"的补丁进入 `last_render_warnings` ✓。
    let preview = restored
        .document_mut(doc)
        .expect("文档已打开")
        .render_document_preview()
        .expect("预览渲染");
    assert!(
        !preview.warnings.is_empty(),
        "前置不成立：这次渲染没有产生告警 ⇒ 判据测不到它"
    );

    let bundle = diagnostics::collect(
        &restored,
        &request(
            doc,
            SurfaceFacts {
                surface: "test".to_owned(),
                ..SurfaceFacts::default()
            },
        ),
    )
    .expect("采集应当成功");
    let zip = bundle.zip().expect("打包应当成功");
    let entries = unzip(&zip);
    let warnings = String::from_utf8_lossy(entry(&entries, "warnings.json")).into_owned();
    assert!(
        warnings.contains("缺少 blob") && warnings.contains(&dropped_hash),
        "被跳过的补丁必须出现在诊断包里（要找 {dropped_hash}）；实际：{warnings}"
    );
    let count = serde_json::from_str::<Value>(&warnings)
        .ok()
        .and_then(|value| value["skipped_patch_count"].as_u64())
        .unwrap_or(0);
    assert!(count >= 1, "skipped_patch_count 应当 ≥1：{warnings}");
    // 包内 README 也要把这件事说出来 ✓（"不在场的人"先看 README ✓）。
    let readme = String::from_utf8_lossy(entry(&entries, "README.txt")).into_owned();
    assert!(
        readme.contains("渲染告警") || readme.contains("被跳过"),
        "README 应当点出渲染告警：{readme}"
    );

    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&restore_root);
}

/// **判据 5（结构那一半）：条目清单与"哪个面"无关** ✓。
///
/// 两个面各填各的 `SurfaceFacts` ✓，条目名必须**逐字相同** ✓ ——
/// 这是"MCP 与 Web 不会漂移"在 Rust 侧的守卫 ✓；
/// **跨真实两面的那一半**在 `scripts/tool-collect-diagnostics.mjs` ✓（它真的起 MCP 进程与 HTTP 路由 ✓）。
///
/// **能红**：让 `collect` 按 `surface` 增删条目 ⇒ 红 ✓。
#[test]
fn entry_list_does_not_depend_on_the_surface() {
    let root = temp_dir("c5");
    let doc = "doc_parity";
    let ws = workspace(&root, doc, 64, 64);
    let mcp = diagnostics::collect(
        &ws,
        &request(
            doc,
            SurfaceFacts {
                surface: "mcp".to_owned(),
                build: json!({"transport": "stdio"}),
                config: json!({"transport": "stdio"}),
                ..SurfaceFacts::default()
            },
        ),
    )
    .expect("采集应当成功");
    let web = diagnostics::collect(
        &ws,
        &request(
            doc,
            SurfaceFacts {
                surface: "web".to_owned(),
                build: json!({"transport": "http"}),
                config: json!({"transport": "http"}),
                ..SurfaceFacts::default()
            },
        ),
    )
    .expect("采集应当成功");
    assert_eq!(
        mcp.entry_names(),
        web.entry_names(),
        "两个面的条目清单必须相同"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// **体积上限真的会裁；裁完"自述与实物"必须一致** ✓。
///
/// 这条守两件事：① 归档**有界**（硬要求 ✓）；② `atoms.meta.json` 不许在裁剪之后
/// 还写着裁剪前的条数 ✓ —— 一个自称"包含 1000 条"而实际只有 400 条的包，
/// **比没有元数据更坏** ✗（它会让复盘的结论建立在错数字上 ✓）。
///
/// **能红**：把 `trim_to_budget` 里更新 `atoms.meta.json` 那段去掉 ⇒ `included` 与
/// `atoms.jsonl` 的行数不等 ⇒ 红 ✓；把裁剪循环去掉 ⇒ `content_bytes` 超上限 ⇒ 红 ✓。
#[test]
fn oversized_content_is_trimmed_and_metadata_stays_consistent() {
    let root = temp_dir("c6");
    let doc = "doc_budget";
    let mut ws = workspace(&root, doc, 64, 64);
    for index in 0..200 {
        let made = call(
            &mut ws,
            doc,
            "create_layer",
            json!({"layer_id": format!("L{index}")}),
        );
        assert_eq!(made["ok"], json!(true), "{made}");
    }
    let limits = Limits {
        max_content_bytes: 60_000,
        max_archive_bytes: 200_000,
        max_atoms: 200,
        ..Limits::default()
    };
    let bundle = diagnostics::collect(
        &ws,
        &DiagnosticsRequest {
            doc_id: doc.to_owned(),
            facts: SurfaceFacts {
                surface: "test".to_owned(),
                ..SurfaceFacts::default()
            },
            limits,
        },
    )
    .expect("采集应当成功");
    assert!(
        bundle.content_bytes <= limits.max_content_bytes,
        "裁剪后内容 {} 字节仍超上限 {}",
        bundle.content_bytes,
        limits.max_content_bytes
    );
    let zip = bundle.zip().expect("打包应当成功");
    assert!(zip.len() <= limits.max_archive_bytes);
    let entries = unzip(&zip);
    let atoms = String::from_utf8_lossy(entry(&entries, "atoms.jsonl")).into_owned();
    let lines = if atoms.trim().is_empty() {
        0
    } else {
        atoms.lines().count()
    };
    let meta: Value = serde_json::from_slice(entry(&entries, "atoms.meta.json")).expect("JSON");
    assert_eq!(
        meta["included"].as_u64().unwrap_or(u64::MAX),
        lines as u64,
        "atoms.meta.json 的 included 必须等于 atoms.jsonl 的行数：{meta}"
    );
    assert_eq!(
        meta["trimmed_by_budget"],
        json!(true),
        "被体积上限裁过就必须自己承认：{meta}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// 字节串包含（不依赖 UTF-8 边界 ✓）。
fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}
