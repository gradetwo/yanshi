//! **`.yanshi` 工程包：浏览器能下载** ✓ —— 判据 (①) 服务端那一半 ✓。
//!
//! **缺口** ✓：`export_project` 从前只把包写到**服务器上的路径** ✓（`path` 必填 ✓）
//! ⇒ 浏览器**拿不到**那台机器上的文件 ✗。PNG 导出与诊断包早就有"可直接下载的地址" ✓
//! （`yanshi://blob/<hash>` ⇒ HTTP 层改写成 `/api/blob/<hash>?doc=..&token=..` ✓），
//! 工程包没有 ✓ ⇒ 这条判据守的就是"**返回的那个地址解出来的就是那个包**" ✓。
//!
//! **为什么断言"字节与哈希"，而不是"URL 字段存在"** ✗：
//! 一个只回 `"url": "yanshi://blob/…"` 却指向**别的 blob**（或截断的字节）的实现 ✓
//! 会让"字段存在"这条断言**照样绿** ✗ —— 而那正是"看起来能下载、下载下来打不开" ✗。
//! ⇒ 这里把三份字节钉在一起：**磁盘上的包** ✓、**CAS 里按 URL 哈希取回的字节** ✓、
//! **内容哈希**（`BlobHash::from_bytes` ✓，即 CAS 的寻址方式 ✓）；再用
//! **导入路径真正用的那个读包器**（`yanshi_server::archive::read_tar` ✓，`service.rs` 的
//! `import_project` 就是它 ✓）解一遍 ✓ —— 三处任何一处被改坏都会红 ✓。
//!
//! **变异（判据自己会不会红）** ✓：把 `write_export_project` 里的
//! `store.put(&tar)` 换成 `store.put(&tar[..tar.len() - 1])`（或 `b"not a tar"` ✓）
//! ⇒ "CAS 字节 = 磁盘字节" 与 `read_tar` 两条**当场红** ✓。

use serde_json::json;
use yanshi_server::{
    archive, DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace,
};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

/// 造一个干净的临时目录（它先删同名目录 ⇒ 一个测试里只调一次）。
fn temp_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("yanshi_dl_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path
}

fn file_workspace(root: &std::path::Path) -> Workspace {
    Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default())
        .expect("落盘工作区应当能建")
}

fn call(
    workspace: &mut Workspace,
    doc: &str,
    tool: &str,
    args: serde_json::Value,
) -> serde_json::Value {
    let mut ctx = ToolContext::new(workspace, doc, "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    registry().call(&mut ctx, tool, &args)
}

/// **判据 ①** ✓：`export_project` 给出的地址，取回来就是**同一个包**（逐字节 + 哈希 + 能解包）。
///
/// 一次导出同时要 `path` 与 `url` ✓ ⇒ 磁盘字节是"那个包"的独立副本 ✓ ——
/// 两份字节来自**同一次** `export_project` ✓（不是打两次再比 ✓：那样两边一起错也测不出来 ✗）。
#[test]
fn the_returned_url_resolves_to_the_exact_package_bytes() {
    let root = temp_dir("download");
    let mut workspace = file_workspace(&root);
    workspace
        .create_document(
            NewDocument::new("doc_dl", 200, 160),
            "human:1",
            "session:test",
        )
        .unwrap();
    {
        let mut ctx = ToolContext::new(&mut workspace, "doc_dl", "human:1", "session:test")
            .with_owner(true)
            .with_wait_for_render(true, 4_000);
        registry().call(&mut ctx, "create_layer", &json!({ "layer_id": "L" }));
        let shape = registry().call(
            &mut ctx,
            "draw_shape",
            &json!({"layer_id": "L", "object_id": "s1",
                    "data": {"geometry": {"kind": "rect", "bbox": {"x": 10, "y": 10, "w": 80, "h": 60}},
                             "color": {"r": 20, "g": 20, "b": 20, "a": 255}}}),
        );
        assert_eq!(shape["ok"], json!(true), "{shape}");
        // 介质笔触 ⇒ 包里**真的有 blob** ✓（否则"取回来的字节是不是包"这件事测得太浅 ✓）。
        let stroke = registry().call(
            &mut ctx,
            "medium_stroke",
            &json!({"layer_id": "L", "object_id": "m1", "medium": "oil",
                    "points": [[20.0, 120.0, 1.0], [100.0, 130.0, 0.8], [170.0, 120.0, 0.6]],
                    "size": 20, "color": {"r": 200, "g": 60, "b": 40, "a": 255}}),
        );
        assert_eq!(stroke["ok"], json!(true), "{stroke}");
    }

    let pack = root.join("dl.yanshi");
    let exported = call(
        &mut workspace,
        "doc_dl",
        "export_project",
        json!({ "path": pack.to_string_lossy() }),
    );
    assert_eq!(exported["ok"], json!(true), "{exported}");

    // ① **回执里必须有可直接下载的地址** ✓（而不是只有一个服务器路径 ✗）。
    let url = exported["url"]
        .as_str()
        .unwrap_or_else(|| panic!("export_project 必须回可直接下载的 url：{exported}"));
    let blob_hash = exported["blob_hash"]
        .as_str()
        .unwrap_or_else(|| panic!("export_project 必须回 blob_hash：{exported}"));
    assert_eq!(
        url,
        format!("yanshi://blob/{blob_hash}"),
        "url 与 blob_hash 必须互相印证：{exported}"
    );
    assert!(
        exported["filename"]
            .as_str()
            .is_some_and(|name| name.ends_with(".yanshi")),
        "回执要给下载文件名：{exported}"
    );

    // ② **磁盘上的那个包**（独立副本）。
    let on_disk = std::fs::read(&pack).expect("export_project 写了 path ⇒ 文件应当在");
    assert!(!on_disk.is_empty(), "包不该是空的");
    assert_eq!(
        exported["bytes"].as_u64(),
        Some(on_disk.len() as u64),
        "回执里的 bytes 必须等于磁盘上的包长：{exported}"
    );

    // ③ **按 url 里的哈希从 CAS 取回**（= 浏览器 GET /api/blob/<hash> 会拿到的东西 ✓）。
    let hash: yanshi_core::BlobHash = blob_hash.parse().expect("blob_hash 应当是合法哈希");
    let from_cas = workspace
        .store()
        .get(&hash)
        .expect("url 指向的 blob 必须真的在 CAS 里（否则浏览器 404 ✗）");

    // ④ **逐字节比**（长度 + 内容）—— "地址存在"不等于"地址对" ✗。
    assert_eq!(
        from_cas.len(),
        on_disk.len(),
        "URL 取回的字节数与磁盘上的包不同 ⇒ 下载下来的不是那个包 ✗"
    );
    assert!(
        from_cas == on_disk,
        "URL 取回的字节与磁盘上的包不同 ⇒ 下载下来的不是那个包 ✗"
    );

    // ⑤ **内容寻址自证** ✓：CAS 是按 SHA-256 寻址的 ✓ ⇒ 哈希必须等于字节的哈希 ✓。
    //（这一条挡住"把同一个哈希写成两个不同 blob"这类不可能的事 ✓，也把 ④ 与寻址绑在一起 ✓。）
    assert_eq!(
        yanshi_core::BlobHash::from_bytes(&from_cas),
        hash,
        "CAS 里那个哈希下的字节，其 SHA-256 必须是该哈希本身"
    );

    // ⑥ **导入路径真正用的那个读包器**能读 ✓（`service.rs` 的 `import_project` 就是 `read_tar` ✓）。
    let entries = archive::read_tar(&from_cas).expect("下载路径的字节必须能被导入用的读包器解开");
    let names: Vec<&str> = entries.iter().map(|entry| entry.path.as_str()).collect();
    for expected in ["atoms.jsonl", "meta.json", "BUILD-INFO"] {
        assert!(
            names.contains(&expected),
            "包里应当有 {expected}：{names:?}"
        );
    }
    assert!(
        names.iter().any(|name| name.starts_with("blobs/sha256/")),
        "包里应当有 blob（否则这条判据没测到 blob 那条路）：{names:?}"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// **判据 ③ 的服务端一半** ✓：**用下载路径产出的字节**导入到一个全新工作区 ⇒ 成功。
///
/// **为什么"下载路径产出的字节"要说清楚** ✗：如果直接从 `path` 读磁盘文件再导入 ✓，
/// 那测的是"导出到文件"这条老路 ✓ —— 而这一轮的新东西是 **CAS 里那份**（浏览器拿到的就是它 ✓）。
/// ⇒ 这里只从 `url` 的哈希取字节 ✓，一个字节都不碰 `path` ✓。
#[test]
fn the_bytes_behind_the_url_import_into_a_fresh_workspace() {
    let root = temp_dir("download_import");
    let mut source = file_workspace(&root);
    source
        .create_document(
            NewDocument::new("doc_src", 120, 96),
            "human:1",
            "session:test",
        )
        .unwrap();
    {
        let mut ctx = ToolContext::new(&mut source, "doc_src", "human:1", "session:test")
            .with_owner(true)
            .with_wait_for_render(true, 4_000);
        registry().call(&mut ctx, "create_layer", &json!({ "layer_id": "L" }));
        let filled = registry().call(
            &mut ctx,
            "fill",
            &json!({"layer_id": "L",
                    "data": {"color": {"r": 200, "g": 30, "b": 90, "a": 255},
                             "region": {"x": 0, "y": 0, "w": 120, "h": 96}}}),
        );
        assert_eq!(filled["ok"], json!(true), "{filled}");
    }
    // **不给 path** ✓ —— 纯下载用法（浏览器就是这条 ✓）：路径必须是**可选的** ✓。
    let exported = call(&mut source, "doc_src", "export_project", json!({}));
    assert_eq!(
        exported["ok"],
        json!(true),
        "不给 path 时也应当能导出（只给可下载的地址）：{exported}"
    );
    assert!(
        exported.get("path").is_none() || exported["path"].is_null(),
        "没给 path 就不该凭空回一个 path：{exported}"
    );
    let url = exported["url"].as_str().expect("必须回 url");
    let hash: yanshi_core::BlobHash = url
        .strip_prefix("yanshi://blob/")
        .expect("url 应当是 yanshi://blob/<hash>")
        .parse()
        .expect("url 里应当是合法哈希");
    let bytes = source.store().get(&hash).expect("blob 在 CAS 里");

    // 导入到一个**全新工作区**（另一台机器的等价物 ✓）。
    let root_b = temp_dir("download_import_b");
    let mut target = file_workspace(&root_b);
    let imported = target
        .import_project(&bytes, Some("doc_copy"))
        .expect("下载路径的字节必须能导入");
    // 注意：直接调 `Workspace::import_project` 不回 `ok` 字段 ✓（那是**工具注册表**包上的 ✓）；
    // 这里看的是它真正的回执（原子数 / 文档 id ✓）。
    assert_eq!(imported["doc_id"], json!("doc_copy"), "{imported}");
    assert!(
        imported["atoms"].as_u64().unwrap_or(0) >= 2,
        "至少要有建图层/填充两条原子：{imported}"
    );

    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&root_b);
}
