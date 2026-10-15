//! **★ `--watch`：跟随 `atoms.jsonl` 的尾部（2026-10-15 ✓）★**
//!
//! **解决什么** ✗：`yanshi-mcp`（stdio ✓）与 `yanshi-serve`（HTTP ✓）是两个进程 ✓，
//! 各自只有**自己写入时**才更新内存 ✓ ⇒ **互相看不见对方新落的 atoms** ✗
//! （实测：MCP 落笔 5 秒后 serve 的 HTTP 仍读不到 ✓）⇒ 人与 AI **没法协作实时画** ✗。
//!
//! **设计（用户 2026-10-15 裁定 ✓：先做轮询，接口按可替换设计 ✓）**：
//! * [TailFollower] 只做一件事 ✓：**"自上次以来，文件尾部长出了哪些新原子"** ✓；
//! * 本实现是**轮询**（`stat` 看 `(len)` ✓ ＋ 读增量 ✓）⇒ ⇒ **一套代码三平台通用** ✓、
//!   **零新依赖** ✓、而且**恰好覆盖** inotify 会漏的情形 ✓（原子改名 ✓、网络文件系统 ✓、编辑器整体重写 ✓）；
//! * **将来若要亚毫秒** ✓：本模块只暴露 [`TailFollower::poll`] ✓ ⇒
//!   换一个内部用 inotify／FSEvents／RDCW 的实现即可 ✓，**上层一行不动** ✓
//!   （用户裁定："之后需要再加平台监听" ✓）。
//!
//! **四条必须一起守的纪律** ✓（少一条就会出玄学 bug ✗）：
//! 1. **半行不消费** ✓：只读到**最后一个 `\n`** —— 对方可能正写到一半 ✓；
//! 2. **按 `seq` 门控** ✓：只收 `seq > last_seq` 的原子 ⇒ **幂等** ✓
//!    （文件被截断/重写也不出错 ✓），并且**天然挡住"自己写的那条"** ✓（已在内存里 ✓）；
//! 3. **文件变短 ⇒ 重新从头跟** ✓（截断／换文件 ✓），不假设只增不减 ✓；
//! 4. **坏行不致命** ✓：解析不了的行**跳过并计数** ✓（另一个进程可能写坏／版本不同 ✓），
//!    **不让 watcher 把整个进程带崩** ✗。
//!
//! **★ 空转开销：已实测（2026-10-15 ✓，`examples/watch_overhead.rs` ✓）★**
//!
//! 用户担心"轮询浪费 CPU" ✓ ⇒ **先量再决定** ✓（不许凭感觉 ✗）。实测（本机，release ✓）：
//! ```text
//! 1 个文档，200 ms（5 Hz）× 5 s ⇒  25 次 poll ⇒ CPU user 0.00 s ＋ sys 0.00 s（低于计时器分辨率）
//! 50 个文档，5 Hz × 5 s        ⇒ 1250 次 poll ⇒ CPU user 0.01 s ＋ sys 0.01 s（≈ 16 µs／次 poll）
//! strace -c（约 10 次 poll）    ⇒ 全进程 129 次系统调用（含启动）⇒ 约 5 次／poll
//! ```
//! ⇒ **折算**：1 个文档 @5 Hz ≈ **0.008 %** 一个核 ✓；**50 个文档**@5 Hz ≈ **0.4 %** ✓
//! ⇒ ⇒ **"浪费 CPU"这条担心实测不成立** ✓；节奏还能调（1 Hz 再小 5 倍 ✓，仍远快于 1 秒的验收线 ✓）。
//!
//! **于是选择的实质** ✓（用户 2026-10-15："更倾向平台监听，但先测开销再决定" ✓）：
//! 数据把 **CPU 这条理由排除了** ✓ ⇒ 剩下的是**"优雅"** ✓ 对**"代价"** ✗ ——
//! 平台监听要**三套实现** ✓＋依赖或 FFI ✓，且 **inotify 会漏**原子改名／网络文件系统／整体重写 ✗。
//! ⇒ 本模块**暂以轮询为准** ✓，并**保持可替换** ✓：真的要亚毫秒时换内部实现即可 ✓（上层不动 ✓）。
//!
//! **已知边界（如实标注 ✗）**：两个进程**并发追加**同一个 JSONL ✓ ⇒ 依赖"单行 `O_APPEND` 的
//! 原子性" ✓ 与用户裁定的"**最后写入为准、不做合并**" ✓ —— 本模块**不**做锁 ✗、**不**做冲突合并 ✗。

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::atom::Atom;
use crate::error::Result;
use crate::fold;
use crate::ids::Seq;
use crate::state::DocumentState;

/// **★ 尾部跟随器（可替换 ✓）★**
///
/// 用法 ✓：`new(path)` ⇒ 周期性 `poll()`（收新原子 ✓／或 `apply_new()` 直接套进状态 ✓）。
/// **不持有文件句柄** ✓（每次 `open` 一次 ✓）—— 换来的是"**换文件/截断**"也能正确跟上 ✓，
/// 而代价只是一次 `open` ＋ 一次 `stat` ✓（轮询周期级别 ✓ 可忽略 ✓）。
#[derive(Debug, Clone)]
pub struct TailFollower {
    path: PathBuf,
    /// 已经消费到的**字节**位置 ✓（一定落在某个 `\n` 之后 ✓）。
    offset: u64,
    /// 已经应用过的**最大 `seq`** ✓（幂等门控 ✓）。
    last_seq: Seq,
    /// 解析不了的行数 ✓（**不致命** ✓，但要能被看见 ✓）。
    pub skipped_lines: u64,
}

/// 一次轮询的结果 ✓。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Polled {
    /// 新收到的原子 ✓（已按文件顺序 ✓，且 `seq` 严格递增 ✓）。
    pub atoms: Vec<Atom>,
    /// 跳过的坏行数 ✓（本次 ✓）。
    pub skipped: u64,
}

impl TailFollower {
    /// 从**头**开始跟 ✓（第一次轮询会把已有原子全收进来 ✓ ⇒
    /// 调用方若已经有内存状态 ✓，应当**先** `new_at(path, last_seq)` ✗ 或自行忽略 ✓）。
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into(), offset: 0, last_seq: 0, skipped_lines: 0 }
    }

    /// 从**已知的 `seq` 之后**开始跟 ✓（两个进程启动时最常用 ✓：
    /// 自己已经加载到 `last_seq` ✓ ⇒ 只关心**之后**的新原子 ✓）。
    pub fn new_after(path: impl Into<PathBuf>, last_seq: Seq) -> Self {
        Self { path: path.into(), offset: 0, last_seq, skipped_lines: 0 }
    }

    /// 文件路径 ✓。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 已经应用到的最大 `seq` ✓。
    pub fn last_seq(&self) -> Seq {
        self.last_seq
    }

    /// **把新原子直接套进文档状态** ✓（走 `fold::apply` ✓ —— 与"从头加载"**同一条**实现 ✓）。
    ///
    /// 返回**新应用的条数** ✓（0 表示尾巴没动 ✓ ⇒ 调用方据此决定要不要推 WS ✓）。
    pub fn apply_new(&mut self, state: &mut DocumentState) -> Result<usize> {
        let polled = self.poll()?;
        let mut applied = 0usize;
        for atom in &polled.atoms {
            fold::apply(state, atom)?;
            applied += 1;
        }
        Ok(applied)
    }

    /// **轮询一次** ✓：文件尾巴长了就解析新行 ✓，没长就返回空 ✓。
    pub fn poll(&mut self) -> Result<Polled> {
        let mut out = Polled::default();
        let Ok(mut file) = File::open(&self.path) else {
            // 文件还没出现（新文档 ✓）⇒ **不是错误** ✓：下次再看 ✓。
            return Ok(out);
        };
        let len = file.metadata().map(|m| m.len()).unwrap_or(0);
        // **纪律 3** ✓：变短 ⇒ 截断/换文件 ⇒ 从头跟 ✓（并把 `last_seq` 的门交给下面按 seq 判定 ✓）。
        if len < self.offset {
            self.offset = 0;
        }
        if len == self.offset {
            return Ok(out);
        }
        if file.seek(SeekFrom::Start(self.offset)).is_err() {
            self.offset = 0;
            return Ok(out);
        }
        let mut buf = Vec::new();
        if file.read_to_end(&mut buf).is_err() {
            return Ok(out);
        }
        // **纪律 1** ✓：只消费到**最后一个 `\n`** —— 后面那截可能是半行 ✓。
        let Some(last_nl) = buf.iter().rposition(|b| *b == b'\n') else {
            return Ok(out);
        };
        let complete = &buf[..=last_nl];
        for raw in complete.split(|b| *b == b'\n') {
            if raw.iter().all(|b| b.is_ascii_whitespace()) {
                continue;
            }
            let line = String::from_utf8_lossy(raw);
            match serde_json::from_str::<Atom>(&line) {
                Ok(atom) => {
                    // **纪律 2** ✓：只收"没见过的" ⇒ 幂等 ✓ ＋ 挡住自己的写入 ✓。
                    if atom.seq > self.last_seq {
                        self.last_seq = atom.seq;
                        out.atoms.push(atom);
                    }
                }
                Err(_) => {
                    // **纪律 4** ✓：坏行跳过并计数 ✓，不致命 ✓。
                    out.skipped += 1;
                }
            }
        }
        self.skipped_lines += out.skipped;
        self.offset += complete.len() as u64;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn atom_line(seq: Seq, id: &str) -> String {
        format!(
            r#"{{"id":"{id}","seq":{seq},"kind":"create_document","actor":"human:web","session":"s","timestamp":1,"schema_version":1,"payload":{{"background":{{"a":255,"b":255,"g":255,"r":255}},"color_space":"srgb","doc_id":"d","height":10,"width":10}}}}"#
        )
    }

    /// ① 追加一行 ⇒ 下次 `poll` 收到它 ✓（这正是"另一个进程落了笔" ✓）。
    #[test]
    fn appended_line_is_picked_up() {
        let dir = std::env::temp_dir().join(format!("yanshi-watch-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("atoms.jsonl");
        let _ = std::fs::remove_file(&path);
        std::fs::write(&path, format!("{}\n", atom_line(1, "a1"))).unwrap();
        let mut f = TailFollower::new(&path);
        let first = f.poll().unwrap();
        assert_eq!(first.atoms.len(), 1, "第一行应当被收到");
        assert_eq!(f.last_seq(), 1);
        // 第二次没有新东西 ✓（幂等 ✓）
        assert!(f.poll().unwrap().atoms.is_empty(), "没有新行时不该重复收");
        // 对方追加 ✓
        let mut fh = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(fh, "{}", atom_line(2, "a2")).unwrap();
        drop(fh);
        let second = f.poll().unwrap();
        assert_eq!(second.atoms.len(), 1, "追加的那行应当被收到");
        assert_eq!(second.atoms[0].seq, 2);
        let _ = std::fs::remove_file(&path);
    }

    /// ② **半行不消费** ✓ —— 对方正写到一半时，绝不能当成完整原子 ✗。
    #[test]
    fn half_written_line_is_not_consumed() {
        let dir = std::env::temp_dir().join(format!("yanshi-watch-half-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("atoms.jsonl");
        let _ = std::fs::remove_file(&path);
        let full = atom_line(1, "a1");
        let half = &full[..full.len() / 2];
        std::fs::write(&path, half).unwrap(); // 没有换行 ⇒ 半行
        let mut f = TailFollower::new(&path);
        assert!(f.poll().unwrap().atoms.is_empty(), "半行不该被消费");
        // 对方写完了 ✓
        std::fs::write(&path, format!("{full}\n")).unwrap();
        let done = f.poll().unwrap();
        assert_eq!(done.atoms.len(), 1, "写完之后应当收到");
    }

    /// ③ **`seq` 门控** ✓：启动时已经加载到 5 ⇒ 只关心 6 以后 ✓（不重复套用自己的写入 ✓）。
    #[test]
    fn seq_gate_skips_what_we_already_have() {
        let dir = std::env::temp_dir().join(format!("yanshi-watch-seq-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("atoms.jsonl");
        let _ = std::fs::remove_file(&path);
        std::fs::write(&path, format!("{}\n{}\n", atom_line(1, "a1"), atom_line(2, "a2"))).unwrap();
        let mut f = TailFollower::new_after(&path, 2);
        assert!(f.poll().unwrap().atoms.is_empty(), "seq ≤ 2 的都不该再收");
        let mut fh = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(fh, "{}", atom_line(3, "a3")).unwrap();
        drop(fh);
        assert_eq!(f.poll().unwrap().atoms.len(), 1, "seq 3 应当收到");
    }

    /// ④ **文件变短 ⇒ 重新从头跟** ✓（截断／换文件 ✓），且坏行只计数不致命 ✓。
    #[test]
    fn truncation_restarts_and_bad_lines_are_counted() {
        let dir = std::env::temp_dir().join(format!("yanshi-watch-trunc-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("atoms.jsonl");
        let _ = std::fs::remove_file(&path);
        std::fs::write(&path, format!("{}\n{}\n", atom_line(1, "a1"), atom_line(2, "a2"))).unwrap();
        let mut f = TailFollower::new(&path);
        assert_eq!(f.poll().unwrap().atoms.len(), 2);
        // 整体重写（更短 ✓）＋ 一行坏的 ✓
        std::fs::write(&path, format!("not json\n{}\n", atom_line(7, "a7"))).unwrap();
        let again = f.poll().unwrap();
        assert_eq!(again.atoms.len(), 1, "重写之后应当重新收到 seq 7");
        assert_eq!(again.atoms[0].seq, 7);
        assert_eq!(again.skipped, 1, "坏行应当被计数");
        assert_eq!(f.skipped_lines, 1);
        let _ = std::fs::remove_file(&path);
    }

    /// ⑤ **文件还不存在** ⇒ **不是错误** ✓（新文档 ✓）。
    #[test]
    fn missing_file_is_not_an_error() {
        let path = std::env::temp_dir().join("yanshi-watch-does-not-exist.jsonl");
        let _ = std::fs::remove_file(&path);
        let mut f = TailFollower::new(&path);
        assert!(f.poll().unwrap().atoms.is_empty());
    }
}
