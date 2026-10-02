//! **纹理缓存** ✓（用户裁定：下载到**本地缓存** ✓，不入 git ✗）。
//!
//! **背景** ✓：CC0 纹理最小的 `1K-PNG` 就有 **13.6MB** ✓（实测自 ambientCG 的 API ✓）
//! ⇒ 几十张就是几百 MB ✗ ⇒ **签进 git 会把仓库撑爆** ✓ ⇒ 权威副本放**工作区** ✓、
//! 由 **`scripts/fetch-textures.sh`** 填充 ✓、由**工具**列给 **MCP 与 Web** 用 ✓
//!（能力在工具层 ⇒ 两边同时可用 ✓，这是用户新加的硬要求 ✓）。
//!
//! **本测试不联网** ✓ —— 它只往缓存目录里摆文件 ✓，然后断言"列得对不对" ✓。
//! **联网那一段是脚本的事** ✓，不该让单元测试去依赖网络 ✗（否则它会随机变红 ✓）。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

/// 干净的临时目录 ✓（**会先删同名目录** ✗ ⇒ 一个测试里只调一次 ✓）。
fn temp_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("yanshi_tex_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path
}

/// 落盘工作区 + 一份文档 ✓（工具调用需要一个文档上下文 ✓）。
fn workspace(root: &std::path::Path) -> Workspace {
    let mut workspace = Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default())
        .expect("落盘工作区应当能建");
    workspace
        .create_document(
            NewDocument::new("doc_tex", 64, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

/// **造一个头部合法的 PNG** ✓（33 字节就够 ✓ —— 可用性探测只读头 ✓）。
///
/// **为什么必须有它** ✓：我第一版往 fixture 里写 `vec![0u8; 1234]` ✗
/// ⇒ 新加的"**真读 PNG 头**"判定说它不能用 ✓ ⇒ 测试红了 ✓ —— **红得对** ✓：
/// 产品说的是实话 ✓，是**测试的 fixture 在说谎** ✗。
fn png_header(depth: u8, color: u8, interlace: u8) -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.extend_from_slice(&13u32.to_be_bytes());
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&8u32.to_be_bytes());
    bytes.extend_from_slice(&8u32.to_be_bytes());
    bytes.push(depth);
    bytes.push(color);
    bytes.push(0);
    bytes.push(0);
    bytes.push(interlace);
    bytes.extend_from_slice(&0u32.to_be_bytes());
    bytes
}

fn call(workspace: &mut Workspace, args: serde_json::Value) -> serde_json::Value {
    let mut ctx = ToolContext::new(workspace, "doc_tex", "human:1", "session:test");
    registry().call(&mut ctx, "list_textures", &args)
}

/// **缓存为空是正常状态** ✓，不是错误 ✗ —— 还没下载过而已 ✓（而且要说清楚去哪儿补 ✓）。
#[test]
fn an_empty_cache_is_an_empty_list_with_a_hint_not_an_error() {
    let root = temp_dir("empty");
    let mut workspace = workspace(&root);
    let got = call(&mut workspace, json!({}));
    assert_eq!(got["ok"], json!(true), "空缓存不该报错：{got}");
    assert_eq!(got["count"], json!(0), "{got}");
    let hint = got["hint"].as_str().unwrap_or_default();
    assert!(
        hint.contains("fetch-textures"),
        "提示里要写清怎么补：{hint}"
    );
}

/// **列出来的东西** ✓：按名字排序 ✓、带上字节数 ✓、**并标明能不能直接用** ✓。
///
/// **为什么 `usable` 重要** ✓：内核**只解 PNG** ✗（JPEG/WebP 会被**明确拒绝** ✓）
/// ⇒ 与其让调用方试一次才知道 ✓，不如**列表里就说清** ✓
///（与"错误里带可用选项"是同一规矩 ✓）。
#[test]
fn the_list_reports_name_size_and_usability() {
    let root = temp_dir("list");
    let cache = root.join("textures");
    std::fs::create_dir_all(&cache).unwrap();
    // 8 位、非隔行、RGB ⇒ **真的能用** ✓（与解码器判据一致 ✓）。
    std::fs::write(cache.join("paper.png"), png_header(8, 2, 0)).unwrap();
    // **灰度 PNG ⇒ 不能用** ✓ —— 钉住刚修的那个**真事故** ✗：
    // `assets/textures/Paper003.png` 原本是灰度 ✓，却被按扩展名报成 `usable: true` ✗，
    // 导入时被解码器拒绝 ✓（"只支持 8 位、非隔行的 RGB/RGBA PNG" ✓）。
    std::fs::write(cache.join("grey.png"), png_header(8, 0, 0)).unwrap();
    std::fs::write(cache.join("canvas.jpg"), vec![0u8; 99]).unwrap();
    std::fs::write(cache.join("NOTICE.md"), b"source: ambientCG, CC0").unwrap();
    std::fs::write(cache.join(".hidden.png"), vec![0u8; 5]).unwrap();
    std::fs::create_dir_all(cache.join("sub")).unwrap();
    let mut workspace = workspace(&root);

    let got = call(&mut workspace, json!({}));
    assert_eq!(got["ok"], json!(true), "{got}");
    let names: Vec<String> = got["textures"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["name"].as_str().unwrap_or_default().to_owned())
        .collect();
    // **隐藏文件与子目录不算纹理** ✓（`NOTICE.md` 要留下 ✓ —— 它是许可凭据 ✓）。
    assert_eq!(
        names,
        vec!["NOTICE.md", "canvas.jpg", "grey.png", "paper.png"],
        "排序或过滤不对：{names:?}"
    );
    let by_name = |name: &str| {
        got["textures"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["name"] == json!(name))
            .cloned()
            .unwrap_or(json!(null))
    };
    assert_eq!(by_name("paper.png")["bytes"], json!(33), "{got}");
    assert_eq!(
        by_name("paper.png")["usable"],
        json!(true),
        "RGB PNG 应当可用：{got}"
    );
    // **灰度 PNG 必须报不可用** ✓ —— 只看扩展名就会漏掉它 ✗。
    assert_eq!(
        by_name("grey.png")["usable"],
        json!(false),
        "灰度 PNG 解码器吃不了，必须说清：{got}"
    );
    assert_eq!(
        by_name("canvas.jpg")["usable"],
        json!(false),
        "JPEG 内核解不了，要说清：{got}"
    );
    assert_eq!(by_name("NOTICE.md")["usable"], json!(false), "{got}");
}

/// **纯内存工作区：能列内置 ✓、但必须说清"不能导入"** ✗。
///
/// **这里我改过一次行为，理由记下来** ✓：第一版是"纯内存 ⇒ 直接报错" ✓；
/// 但有了**内置资产**之后 ✓，纯内存模式**仍然能列出内置的** ✓（它们在磁盘上 ✓）
/// ⇒ 一律报错**过于严格** ✗。**可是**"静默返回空表"更坏 ✗ ——
/// 用户会以为"就是没有资产" ✓（上一版 `list_assets` 正是把错误吞掉了 ✗，测试当场抓住 ✓）
/// ⇒ 正确做法是：**列出来 ✓ + 用 `can_import` 与提示说清"要导入得给 --root"** ✓。
#[test]
fn an_in_memory_workspace_lists_bundled_assets_but_says_importing_needs_a_root() {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    workspace
        .create_document(
            NewDocument::new("doc_tex", 64, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    // **这里要调通用的那个** ✓（`list_assets` ✓）—— 它才有 `can_import` ✓；
    // `list_textures` 是纹理专用的薄包装 ✓，字段更少 ✓（两者共用同一份实现 ✓）。
    let got = {
        let mut ctx = ToolContext::new(&mut workspace, "doc_tex", "human:1", "session:test");
        registry().call(&mut ctx, "list_assets", &json!({ "kind": "texture" }))
    };
    assert_eq!(got["ok"], json!(true), "列举本身应当成功：{got}");
    // **没有配置内置目录 ⇒ 空 ✓**，但**必须说清为什么** ✗（不能让人以为"就是没有" ✓）。
    let assets = got["assets"].as_array().cloned().unwrap_or_default();
    assert!(
        assets.is_empty(),
        "纯内存且未配内置目录 ⇒ 应当是空的：{got}"
    );
    assert_eq!(
        got["can_import"],
        json!(false),
        "要明确说「不能导入」：{got}"
    );
    let hint = got["hint"].as_str().unwrap_or_default();
    assert!(hint.contains("--root"), "提示要说清怎么修：{hint}");

    // **导入必须被明确拒绝** ✓（而不是假装成功 ✗）—— 这才是纯内存模式真正的限制 ✓。
    let mut ctx = ToolContext::new(&mut workspace, "doc_tex", "human:1", "session:test");
    let imported = registry().call(
        &mut ctx,
        "import_asset",
        &json!({ "kind": "texture", "name": "x.png", "path": "/etc/hostname" }),
    );
    assert_eq!(
        imported["ok"],
        json!(false),
        "纯内存模式不该能导入：{imported}"
    );
    assert_eq!(
        imported["error_code"],
        json!("precondition_failed"),
        "{imported}"
    );
}

/// **目录名要挡住路径穿越** ✗（`..` 与绝对路径都不该能指到工作区之外 ✓）。
#[test]
fn a_directory_name_cannot_escape_the_workspace() {
    let root = temp_dir("escape");
    let mut workspace = workspace(&root);
    for bad in ["../etc", "/etc", "a/../../b"] {
        let got = call(&mut workspace, json!({ "dir": bad }));
        assert_eq!(got["ok"], json!(false), "{bad} 应当被拒：{got}");
        assert_eq!(got["error_code"], json!("invalid_argument"), "{bad}: {got}");
    }
}

/// **资产目录要能被解析成一个"真的存在"的目录** ✓（真实用户报告 ✓）。
///
/// **为什么值得一条测试** ✓：用户换了一台机器跑 `make dev` ⇒ 笔刷下拉里只有"内置画笔" ✓、
/// 调色板与纹理一片空白 ✗。我**原样复现**了原因 ✓：
/// `--assets-dir` 缺省是**相对路径** `assets` ✓ ⇒ 换个工作目录启动就找不到 ✓ ⇒
/// 三类内置资产的 `count` **全是 0** ✗（不是"功能没做" ✓，而是"没找到文件" ✗）。
/// **判据** ✓：调用方给的路径存在 ⇒ **原样用它** ✓；不存在 ⇒
/// **要么换成一个真的存在的** ✓、**要么在说明里写清找过哪些** ✓（不能悄悄返回一个空目录 ✗）。
#[test]
fn the_assets_directory_resolves_to_something_real_or_says_what_it_tried() {
    use yanshi_server::service::resolve_assets_dir;

    let repo_assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let (resolved, note) = resolve_assets_dir(Some(repo_assets.clone()));
    assert_eq!(
        resolved.as_ref(),
        Some(&repo_assets),
        "给了真实存在的目录就该原样用：{note}"
    );
    assert!(note.contains("资产目录"), "要把用的是哪个说出来：{note}");

    // **给一个不存在的** ✓：要么解析到别的真实目录 ✓，要么说明"找过哪些" ✓ —— 两者都不算失败 ✓，
    // **但"悄悄返回一个不存在又不说"就算失败** ✗。
    // **给一个不存在的** ✓ —— 契约是三条里的一条 ✓（我第一版把第三条当成失败 ✗，其实它是**有意**的 ✓）：
    // 1. 解析到另一个**真的存在**的目录 ✓（开发/发行布局 ✓）；
    // 2. 返回**原值**并注明「未找到（找过：…）」✓ —— **保留调用方的意图** ✓，同时**不让他猜** ✓；
    // 3. 返回 `None` ✓。
    // **唯一不许发生的** ✓ 是"说找到了、其实不存在" ✗。
    let bogus = std::path::PathBuf::from("/nonexistent/yanshi-assets-does-not-exist");
    let (fallback, note) = resolve_assets_dir(Some(bogus.clone()));
    match fallback {
        Some(dir) => assert!(
            dir.is_dir() || note.contains("未找到"),
            "说找到了就得真存在；否则必须说明没找到并列出找过哪些：{}（{note}）",
            dir.display()
        ),
        None => assert!(note.contains("未找到"), "没找到时必须写清找过哪些：{note}"),
    }
    assert!(
        note.contains("找过") || note.contains("资产目录"),
        "说明要能读懂：{note}"
    );
}
