//! 05 复审 NEW-C1：**未认证的鉴权请求不许触发全量日志读取与折叠**。
//!
//! 旧实现 `Workspace::authorize` 在查令牌**之前**就 `load_atoms`（读盘解析整份日志）
//! ＋ `open_document`（全量折叠 ＋ 渲染缓存分配）⇒ 无效令牌的匿名请求也能逼服务端做全量 IO。
//! 现在远端请求先走 meta.json 浅探针，通过后才付打开的代价。
//!
//! **变异可红**：把令牌检查挪回 `load_atoms` 之后（旧顺序）⇒
//! 测试「无效令牌」「缺令牌」当场红（计数 ≠ 0）；「有效令牌仍要打开」保证输入
//! 真的触发了重活路径（断言不是恒绿的安慰剂）。

use std::path::PathBuf;
use yanshi_server::token::TransportKind;
use yanshi_server::{CapabilityToken, DocumentSettings, NewDocument, Workspace};

/// **跑完即删临时目录**（`/tmp` 是小 tmpfs，夹具留着会写满）。
struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp_root(tag: &str) -> PathBuf {
    let mut root = std::env::temp_dir();
    root.push(format!("yanshi-auth-probe-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    root
}

fn load_atoms_calls(workspace: &Workspace) -> usize {
    workspace.persist().expect("落盘工作区").load_atoms_calls()
}

#[test]
fn authorize_rejects_without_touching_the_atom_log() {
    let root = temp_root("doc");
    let _cleanup = Cleanup(root.clone());
    let doc_id = "auth_probe_doc";
    // 第一个进程：建文档（创建原子落盘）＋ 签发 editor 令牌。
    let token = {
        let mut ws = Workspace::with_file_store(&root, DocumentSettings::default()).unwrap();
        ws.create_document(NewDocument::new(doc_id, 32, 32), "human:1", "s")
            .unwrap();
        let (token, _evicted) = ws
            .issue_token(doc_id, "human:editor", yanshi_server::Role::Editor)
            .unwrap();
        token
    };

    // 第二个进程（模拟服务重启）：文档**未打开** ⇒ 走浅探针这条路。
    let mut ws = Workspace::with_file_store(&root, DocumentSettings::default()).unwrap();
    assert_eq!(load_atoms_calls(&ws), 0, "刚建的工作区不该已经读过日志");

    // ① 无效令牌 ⇒ PermissionDenied，且**一次 `load_atoms` 都不许发生**。
    let bogus = CapabilityToken::parse(&"f".repeat(64)).unwrap();
    let err = ws
        .authorize(doc_id, Some(&bogus), TransportKind::Http, "anon")
        .expect_err("无效令牌必须被拒");
    assert_eq!(err.code, yanshi_core::ErrorCode::PermissionDenied);
    assert_eq!(
        load_atoms_calls(&ws),
        0,
        "无效令牌不得触发全量日志读取（NEW-C1 的旧顺序会在这里红）"
    );

    // ② 缺令牌 ⇒ 同样拒，同样零 IO。
    let err = ws
        .authorize(doc_id, None, TransportKind::Http, "anon")
        .expect_err("缺令牌必须被拒");
    assert_eq!(err.code, yanshi_core::ErrorCode::PermissionDenied);
    assert_eq!(load_atoms_calls(&ws), 0, "缺令牌不得触发全量日志读取");

    // ③ 有效令牌 ⇒ 通过，**这时才**付打开的代价（计数 ≥ 1 ⇒ 输入真的造出了那个情况）。
    let principal = ws
        .authorize(doc_id, Some(&token), TransportKind::Http, "anon")
        .expect("有效令牌必须通过");
    assert_eq!(principal.actor, "human:editor");
    assert!(
        load_atoms_calls(&ws) >= 1,
        "有效令牌通过后应当真正打开文档（否则上面两条断言是空口的）"
    );

    // ④ 吊销过的令牌 ⇒ 浅探针必须跟着说不认识（盘上记录同步删除）。
    drop(ws);
    {
        let mut ws = Workspace::with_file_store(&root, DocumentSettings::default()).unwrap();
        ws.open_document(doc_id).unwrap(); // `revoke_token` 作用于**已打开**的文档
        ws.revoke_token(doc_id, &token).unwrap();
    }
    let mut ws = Workspace::with_file_store(&root, DocumentSettings::default()).unwrap();
    let err = ws
        .authorize(doc_id, Some(&token), TransportKind::Http, "anon")
        .expect_err("已吊销令牌必须被拒");
    assert_eq!(err.code, yanshi_core::ErrorCode::PermissionDenied);
    assert_eq!(load_atoms_calls(&ws), 0, "已吊销令牌不得触发全量日志读取");

    drop(_cleanup);
}

#[test]
fn local_transport_behavior_unchanged() {
    // 本地传输（stdio／进程内，受信）维持旧行为：无令牌也放行 ⇒ Owner。
    let root = temp_root("local");
    let _cleanup = Cleanup(root.clone());
    let doc_id = "auth_probe_local";
    {
        let mut ws = Workspace::with_file_store(&root, DocumentSettings::default()).unwrap();
        ws.create_document(NewDocument::new(doc_id, 32, 32), "human:1", "s")
            .unwrap();
    }
    let mut ws = Workspace::with_file_store(&root, DocumentSettings::default()).unwrap();
    let principal = ws
        .authorize(doc_id, None, TransportKind::Stdio, "mcp")
        .expect("本地传输豁免令牌");
    assert_eq!(principal.role, yanshi_server::Role::Owner);
}
