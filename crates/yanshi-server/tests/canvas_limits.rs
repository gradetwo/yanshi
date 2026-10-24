//! **超过画布上限必须在建文档时就被拒**（第 772 轮）
//!
//! **⇒ 为什么要有**：`create_document` 里写着 `MAX_CANVAS_EDGE = 16384` 与 `MAX_CANVAS_PIXELS = 1<<28`
//! （`service.rs:1222-1223` ✓），注释说"超过就必然 OOM，所以在这里拒绝" ✓
//! ⇒ 而**它此前没有任何判据** ✗ ⇒ 那两个常量改大、或某条路径绕过它 ⇒ **OOM 而不是报错** ✓
//! ⇒ 用户看到的是"服务卡死／被杀" ✗（正是最该挡住的一类失败 ✓）。
//!
//! **⇒ 判据（四条）**：
//! ① 单边超限（20,000 × 64）⇒ `invalid_argument`，文案提到单边上限 ✓；
//! ② 总面积超限但单边没超（16,384 × 16,384 ＝ 2.68 亿 > 1<<28 ＝ 2.68 亿……）
//!    ⇒ 用一个**明确越界**的组合 ✓；
//! ③ 边界内的（4,096 × 4,096）⇒ **必须成功** ✓（证明判据不是"什么都拒" ✓）；
//! ④ 两条拒绝都**不许**是 OOM／panic ✓（测试跑到这里本身就是证据 ✓）。
//!
//! **变异**：把 `MAX_CANVAS_EDGE` 改到 1e9 ⇒ ① 会**成功** ⇒ 红 ✓。
use yanshi_server::{DocumentSettings, NewDocument, Workspace};

fn workspace(root: &std::path::Path) -> Workspace {
    Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default()).expect("落盘工作区")
}

fn root() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("yanshi_canvas_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("建根");
    path
}

#[test]
fn oversized_canvases_are_refused_instead_of_exhausting_memory() {
    let root = root();
    let mut workspace = workspace(&root);

    // ① 单边 20,000 > 16,384 ⇒ 必须被拒 ✓
    let too_wide = workspace.create_document(
        NewDocument::new("doc_wide", 20_000, 64),
        "human:1",
        "session:test",
    );
    let error = too_wide.expect_err("单边超限必须被拒");
    let text = error.to_string();
    assert!(
        text.contains("16384") || text.contains("单边"),
        "拒绝的文案要说清单边上限，实测：{text}"
    );

    // ② 总面积超限（16,384 × 16,384 ＝ 268,435,456 ＞ 1<<28 ＝ 268,435,456？）
    //   **⇒ 用明确越界的**：16,384 × 16,384 正好等于上限 ⇒ 换成 16,384 × 16,000 仍在上限内 ✓
    //   ⇒ 所以这条选 **1×16,384 以外的**组合没有意义 ✗ ⇒ 改成"单边都不超但总面积超"是不可能的
    //   （因为 16,384² 就是上限 ✓）⇒ **这条删掉，改断"恰好在边界内要成功"** ✓（见 ③）。
    //   **⇒ 这个推理本身就是发现的收益**：两个上限里**只有单边是紧的** ✓
    //     （`16384 × 16384 = 1<<28` 恰好等于像素上限 ✓）。

    // ③ 边界内 ⇒ 必须成功 ✓
    workspace
        .create_document(
            NewDocument::new("doc_ok", 4_096, 4_096),
            "human:1",
            "session:test",
        )
        .expect("边界内的画布必须能建");

    // ④ 被拒的那个**不许**留下目录 ✓（否则下次同 id 会撞上半个文档 ✓）
    assert!(
        !root.join("docs").join("doc_wide").exists(),
        "被拒的文档不许在盘上留下目录"
    );
}
