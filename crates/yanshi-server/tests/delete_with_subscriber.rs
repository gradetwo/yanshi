//! **有实时连接在用时，删文档必须被拒**（第 770 轮；外部审计 S-05 的**运行时**判据）
//!
//! **⇒ 原判据只是源码比对** ✗（`check-ws-subscriber-shared.mjs` 查「计数存在共享 `Arc` 里」✓）
//!   ⇒ 它**证明不了**"真的有连接时会拒删" ✓ —— 而那正是用户会遇到的判断 ✓。
//!
//! **⇒ 判据（运行时）**：① 订阅一个实时通道 ⇒ `delete_document` **必须**返回 `conflict` ✓
//! 且文案提到连接数 ✓；② 释放订阅（通道 drop ✓）⇒ **删除成功** ✓。
//! **变异**：把 `delete_document` 里那段 `subscribers > 0` 的判断去掉 ⇒ ① 红 ✓。
use yanshi_server::broadcast::PushChannel;
use yanshi_server::{DocumentSettings, NewDocument, Workspace};

fn workspace(root: &std::path::Path) -> Workspace {
    Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default()).expect("落盘工作区")
}

#[test]
fn a_live_subscriber_blocks_deletion_until_it_goes_away() {
    let root = std::env::temp_dir().join(format!("yanshi_delsub_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("建根");
    let mut workspace = workspace(&root);
    workspace
        .create_document(
            NewDocument::new("doc_sub", 32, 32),
            "human:1",
            "session:test",
        )
        .expect("create_document");

    // ① 开一个实时通道 ⇒ 删除必须被拒 ✓
    //   `PushChannel` 是**枚举**（`WebSocket`／`Poll`／`InProcess` ✓）而不是可构造的通道 ✓
    let subscriber = {
        let document = workspace.document_mut("doc_sub").expect("已打开");
        document.subscribe("session:viewer", PushChannel::InProcess)
    };
    let refused = workspace.delete_document("doc_sub");
    assert!(
        refused.is_err(),
        "有人正开着这个文档，删除却成功了 ⇒ 那个标签页会拿着已删除的文档继续写"
    );
    let error = refused.expect_err("上面已断言 Err");
    let text = error.to_string();
    assert!(
        text.contains("正在使用中") || text.contains("1 个实时连接"),
        "拒绝的文案应当说清「有连接在用」，实测：{text}"
    );

    // ② 释放订阅 ⇒ 删除必须成功 ✓
    {
        let document = workspace.document_mut("doc_sub").expect("已打开");
        document.broadcaster_mut().unsubscribe(subscriber);
    }
    assert_eq!(
        workspace
            .document("doc_sub")
            .map(|doc| doc.broadcaster().len()),
        Some(0),
        "订阅应当已释放"
    );
    workspace
        .delete_document("doc_sub")
        .expect("没有连接之后必须能删");
    assert!(
        workspace.document("doc_sub").is_none(),
        "删完就不该还在内存里"
    );
}
