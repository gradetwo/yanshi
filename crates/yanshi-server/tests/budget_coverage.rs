//! 设计 14.10 预算表的**覆盖情况**必须可核对。
//!
//! 设计原文称「本表关键行纳入 CI benchmark 套件，自动回归」—— 这句话若无校验，
//! 就只是愿望。本测试解析 `docs/design/implementation-notes.md` 里的覆盖表并检查：
//!
//! 1. 预算项**集合**与设计 14.10 的条目一一对应（删行即失败）；
//! 2. 状态为「已覆盖」时，证据中的 `文件::测试函数`（或脚本路径）**必须真实存在**；
//! 3. 非「已覆盖」的行必须写明**理由** —— 允许未覆盖，不允许含糊。

use std::path::{Path, PathBuf};

/// 设计 14.10 的预算条目（作为权威清单；缺行/多行都会失败）。
const BUDGET_ITEMS: [&str; 16] = [
    "打开文档 view 模式",
    "打开文档 edit 模式（可交互）",
    "首笔呈现延迟（本地）",
    "持续笔迹帧预算",
    "原子提交（单原子，服务端处理）",
    "原子提交（批量，服务端处理）",
    "区域渲染（缓存命中）",
    "区域渲染（未命中·简单）",
    "区域渲染（未命中·复杂）",
    "时间旅行（近期历史 / checkpoint）",
    "时间旅行（老历史，含归档取回）",
    "内存（4K/10 图层）",
    "内存（8K/5 图层）",
    "显存（4K/10 图层，合成后端）",
    "网络延迟（公网）",
    "8h 会话性能衰减",
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate 应位于 <repo>/crates/yanshi-server")
        .to_path_buf()
}

struct Row {
    item: String,
    status: String,
    evidence: String,
}

fn coverage_rows() -> Vec<Row> {
    let notes = std::fs::read_to_string(repo_root().join("docs/design/implementation-notes.md"))
        .expect("实现说明应可读");
    let start = notes
        .find("<!-- budget-coverage:start -->")
        .expect("覆盖表缺少起始标记");
    let end = notes
        .find("<!-- budget-coverage:end -->")
        .expect("覆盖表缺少结束标记");
    let mut rows = Vec::new();
    for line in notes[start..end].lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') || trimmed.contains("---") {
            continue;
        }
        let cells: Vec<&str> = trimmed
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect();
        if cells.len() < 4 || cells[0] == "预算项" {
            continue;
        }
        rows.push(Row {
            item: cells[0].to_owned(),
            status: cells[2].to_owned(),
            evidence: cells[3].to_owned(),
        });
    }
    rows
}

/// `path::function` 或脚本路径是否存在。
fn evidence_exists(root: &Path, evidence: &str) -> bool {
    // 证据里可能同时列多个（用「；」或空白分隔）；逐个检查第一个可解析的。
    let token = evidence
        .split(['；', ';', ' '])
        .find(|token| token.contains("::") || token.contains('/'))
        .unwrap_or(evidence);
    if let Some((file, function)) = token.split_once("::") {
        let path = find_file(root, file);
        let Some(path) = path else {
            return false;
        };
        return std::fs::read_to_string(path)
            .map(|text| text.contains(&format!("fn {function}")))
            .unwrap_or(false);
    }
    root.join(token).exists()
}

/// 在 crates/*/tests、crates/*/src、scripts 里按文件名查找。
fn find_file(root: &Path, name: &str) -> Option<PathBuf> {
    let candidates = [
        root.join("scripts").join(name),
        root.join("crates/yanshi-server/tests").join(name),
        root.join("crates/yanshi-render/tests").join(name),
        root.join("crates/yanshi-core/tests").join(name),
        root.join("crates/yanshi-wasm/tests").join(name),
        root.join("crates/yanshi-http/tests").join(name),
    ];
    candidates.into_iter().find(|path| path.exists())
}

#[test]
fn every_budget_item_has_a_row() {
    let rows = coverage_rows();
    let items: Vec<String> = rows.iter().map(|row| row.item.clone()).collect();
    for expected in BUDGET_ITEMS {
        assert!(
            items.iter().any(|item| item == expected),
            "覆盖表缺少预算项「{expected}」—— 设计 14.10 的条目必须逐一在册"
        );
    }
    assert_eq!(
        rows.len(),
        BUDGET_ITEMS.len(),
        "覆盖表行数与设计条目数不一致：{items:?}"
    );
}

#[test]
fn covered_rows_point_at_real_evidence() {
    let root = repo_root();
    for row in coverage_rows() {
        if row.status == "已覆盖" {
            assert!(
                evidence_exists(&root, &row.evidence),
                "「{}」标为已覆盖，但证据不存在或找不到对应测试：{}",
                row.item,
                row.evidence
            );
        } else {
            assert!(
                row.evidence.contains("理由："),
                "「{}」的状态是「{}」，必须写明「理由：…」而不是留空",
                row.item,
                row.status
            );
            assert!(
                ["部分覆盖", "未覆盖", "不适用"].contains(&row.status.as_str()),
                "「{}」的状态「{}」不在允许集合内",
                row.item,
                row.status
            );
        }
    }
}
