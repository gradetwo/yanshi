//! PE-002：`release_document` 不许为回答 `persisted` 而扫描无关的已关闭文档。
//!
//! 旧实现在 `close_document` 之后调 `list_documents()`，而 `list_documents` 对每个
//! 未打开的持久化文档执行 `load_atoms`＋`fold_atoms`——放下一个文档的代价是把
//! 全工作区已关闭文档的历史全读全折一遍（Pearl 包单场景实测 6837.94ms→0.14ms、
//! 5391.78MiB→5.75MiB；单场景一次前后测，不是受控重复基准）。
//!
//! 本测试用落盘存储的 `load_atoms_calls` 计数作观测口，不用计时断言（计时抖动
//! 会让回归测试变 flaky）：修前计数跳变已关闭文档数，修后一动不动。
//! 末尾的重开断言是阳性对照——计数不是死的，0 才不是安慰剂。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn call(workspace: &mut Workspace, tool: &str, args: serde_json::Value) -> serde_json::Value {
    let mut ctx = ToolContext::new(workspace, "target", "human:1", "session:test");
    registry().call(&mut ctx, tool, &args)
}

fn load_atoms_calls(workspace: &Workspace) -> usize {
    workspace.persist().expect("落盘工作区").load_atoms_calls()
}

/// **跑完即删临时目录**（`/tmp` 是小 tmpfs，夹具留着会写满）。
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn release_document_does_not_scan_unrelated_closed_documents() {
    let root = std::env::temp_dir().join(format!("yanshi-pe002-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let _cleanup = Cleanup(root.clone());

    // 夹具：目标＋3 个旁观者，全部落盘，随后全部离开内存（模拟重启后的
    // 已关闭态——这正是 Pearl 包里 42 个局部工程的状态）。
    {
        let mut ws = Workspace::with_file_store(root.clone(), DocumentSettings::default()).unwrap();
        for id in ["target", "bystander1", "bystander2", "bystander3"] {
            ws.create_document(NewDocument::new(id, 32, 32), "human:1", "session:test")
                .unwrap();
        }
    }

    let mut ws = Workspace::with_file_store(root.clone(), DocumentSettings::default()).unwrap();
    assert_eq!(load_atoms_calls(&ws), 0, "刚建的工作区不该已经读过日志");

    let got = call(
        &mut ws,
        "release_document",
        json!({ "document_id": "target" }),
    );
    assert_eq!(got["ok"], json!(true), "{got}");
    assert_eq!(
        got["persisted"],
        json!(true),
        "关掉的文档磁盘上应当仍保留：{got}"
    );
    assert_eq!(
        load_atoms_calls(&ws),
        0,
        "release_document 不许为回答 persisted 而读无关已关闭文档的日志（PE-002：旧实现调 list_documents，全量 load_atoms＋fold）"
    );

    // 阳性对照：重开目标文档应当真的读日志，否则上面的 0 是安慰剂。
    let got = call(
        &mut ws,
        "reopen_document",
        json!({ "document_id": "target" }),
    );
    assert_eq!(got["ok"], json!(true), "{got}");
    assert!(
        load_atoms_calls(&ws) >= 1,
        "重开目标文档应当真的读日志，否则上面的 0 是空口的"
    );
}
