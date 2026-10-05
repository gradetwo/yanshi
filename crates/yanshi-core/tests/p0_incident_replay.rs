//! **事故日志判据**：对真实工程日志（含检查点与撤销交错）验证
//! "编辑路径 == 冷重放"，并核对有效集与存活补丁数。
//!
//! 用法（事故日志不在仓库里，用环境变量指过去）：
//!
//! ```text
//! YANSHI_P0_LOG=/tmp/p0real/bug_pkg/doc/atoms.jsonl \
//!   cargo test -p yanshi-core --test p0_incident_replay -- --nocapture
//! ```
//!
//! 日志（667 个原子）的结构：`create_document` 1 ＋ `create_checkpoint` 33 ＋
//! `draw_shape` 2 ＋ `import_image` 529 ＋ `revert` 102，**没有 `declare_head`**。
//! 102 个撤销**各指向一个不同的 `import_image`**（没有撤销-撤销、没有重复撤销），
//! 因此日志自身的语义是唯一确定的：**102 个补丁失效、427 个存活**，
//! 其中 seq 652–657（末尾六个"隐形釉面"笔触）失效、651 存活。
//!
//! 判据（三条，全部由日志自身的语义导出，不依赖任何金标准图片）：
//! 1. **编辑路径 == 冷重放**：逐 seq 比对整个 `StateAt`（状态 ＋ 有效集 ＋ 警告）；
//! 2. **有效集计数**：`suppressed` 恰为 102，且每个被撤销的目标都是 `import_image`；
//! 3. **存活补丁计数**：`import_image` 总数 − `suppressed` == 427，
//!    且 seq 652–657 失效、651 存活。
//!
//! 未设 `YANSHI_P0_LOG` 时**跳过**（与 `history_fold_bench` 同一约定）——
//! 这条日志是用户作品的原子流，不适合签进仓库。

use yanshi_core::seq::{state_at, IncrementalFolder, StateAtCache};
use yanshi_core::{Atom, AtomKind, AtomLog};

fn load(path: &str) -> AtomLog {
    let text = std::fs::read_to_string(path).expect("读日志");
    let mut atoms: Vec<Atom> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        atoms.push(serde_json::from_str(line).unwrap_or_else(|e| panic!("原子 JSON 坏了: {e}")));
    }
    AtomLog::with_atoms(atoms).expect("日志")
}

#[test]
fn incident_log_editor_and_cold_replay_agree_field_for_field() {
    let Ok(path) = std::env::var("YANSHI_P0_LOG") else {
        eprintln!("跳过：未设置 YANSHI_P0_LOG（事故日志不在仓库里）");
        return;
    };
    let log = load(&path);
    let head = log.head_seq();

    // ① 编辑路径（逐条前推）== 冷重放（每步一个全新缓存），逐字段。
    let mut folder = IncrementalFolder::new();
    let mut divergences = 0usize;
    for n in 1..=head {
        let incremental = folder.fold(&log, n).expect("增量折叠");
        let cold = state_at(&log, n, &mut StateAtCache::new()).expect("冷重放");
        if incremental != cold {
            divergences += 1;
            if divergences <= 5 {
                eprintln!(
                    "分歧 seq={n}: state={} suppressed={} warnings={} origin={}",
                    incremental.state == cold.state,
                    incremental.suppressed == cold.suppressed,
                    incremental.warnings == cold.warnings,
                    incremental.origin_seq == cold.origin_seq
                );
            }
        }
    }
    assert_eq!(
        divergences, 0,
        "增量折叠与冷重放在 {divergences} 个 seq 上分歧"
    );

    let cold = state_at(&log, head, &mut StateAtCache::new()).expect("冷重放 head");

    // ② 有效集计数：每个撤销各指一个不同的 `import_image`。
    let images: Vec<&Atom> = log
        .iter()
        .filter(|a| a.kind == AtomKind::ImportImage)
        .collect();
    let reverts: Vec<&Atom> = log.iter().filter(|a| a.kind == AtomKind::Revert).collect();
    let declared = reverts
        .iter()
        .filter_map(|revert| {
            revert
                .payload
                .get("target")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    assert_eq!(
        reverts.len(),
        declared,
        "有撤销重复指向同一个目标 ⇒ 日志语义不再是「每个目标失效一次」"
    );
    for revert in &reverts {
        let target = revert
            .payload
            .get("target")
            .and_then(serde_json::Value::as_str)
            .expect("撤销必须带 target");
        let target = log.get(target).expect("撤销目标必须存在");
        assert_eq!(
            target.kind,
            AtomKind::ImportImage,
            "撤销目标 {} 不是 import_image（本判据的前提被推翻）",
            target.id
        );
    }
    assert_eq!(
        cold.suppressed.len(),
        declared,
        "有效集大小必须等于被撤销的目标数"
    );

    // ③ 存活补丁计数（事故日志：529 − 102 == 427）。
    if head == 667 && images.len() == 529 {
        assert_eq!(
            cold.suppressed.len(),
            102,
            "事故日志应当恰有 102 个补丁失效"
        );
        assert_eq!(
            images.len() - cold.suppressed.len(),
            427,
            "存活补丁数必须是 427（529 说明撤销被忽略，<427 说明过度回退）"
        );
        for seq in 652..=657u64 {
            let id = &log.by_seq(seq).expect("原子存在").id;
            assert!(
                cold.suppressed.contains(id),
                "seq {seq} 的尾部釉面笔触必须失效"
            );
        }
        let id_651 = &log.by_seq(651).expect("原子存在").id;
        assert!(
            !cold.suppressed.contains(id_651),
            "seq 651 必须存活（它不是六个尾部撤销的目标）"
        );
    } else {
        eprintln!(
            "注意：这份日志不是事故日志（head={head}, import_image={}）⇒ 只跑了通用判据",
            images.len()
        );
    }
}
