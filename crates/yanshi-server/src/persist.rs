//! 文件持久化（设计文档 18 章：本地文件 + Blob CAS）。
//!
//! 目录布局：
//!
//! ```text
//! <root>/
//!   blobs/sha256/<ab>/<cd>/<hex>      # CAS（FsBlobStore，hash 分桶）
//!   docs/<doc_id>/atoms.jsonl         # 原子日志（每行一个 JSON 原子，含权威 seq）
//!   docs/<doc_id>/meta.json           # 文档元数据与 capability token
//!   docs/<doc_id>/render.png          # 最近一次 HEAD 渲染缓存（14.5「打开即图片」）
//!   docs/<doc_id>/render.seq          # 渲染缓存对应的 seq
//!   docs/<doc_id>/preview.png         # 最近一次 HEAD 的**小图**文档预览（256²，增量预览基座）
//!   docs/<doc_id>/preview.seq         # 上述预览对应的 seq
//! ```
//!
//! **为什么预览与渲染分成两份文件** ✓（冷启动复用专题）：两者用途与代价都不同 ✓ ——
//! `render.png` 是**整幅**导出缓存（4K 实测 6.7MB、解码 153.7s ✓），只在"整幅区域渲染
//! 与 HEAD 一致"时用来**直接命中** ✓；`preview.png` 是**小的**文档预览（256²、解码 ~1s ✓），
//! 用来做**增量预览的像素基座** ✓。共用一份文件会互相顶掉 ✓ ⇒ 两条复用路径都失效 ✗。

use crate::token::{CapabilityToken, Role};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::BufRead;
use std::io::Write;
use std::path::{Path, PathBuf};
use yanshi_core::blob::FsBlobStore;
use yanshi_core::{Atom, ErrorCode, ErrorContext, Result, Seq, YanshiError};

/// 文件存储布局。
#[derive(Debug, Clone)]
pub struct FileStore {
    root: PathBuf,
}

/// 文档元数据（meta.json）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentMeta {
    /// 文档 id。
    pub doc_id: String,
    /// 画布宽。
    pub width: u32,
    /// 画布高。
    pub height: u32,
    /// 创建时间（Unix 毫秒）。
    pub created_at: i64,
    /// 已发放的 capability token（12.7）。
    #[serde(default)]
    pub tokens: Vec<TokenRecord>,
}

/// 持久化的令牌记录。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TokenRecord {
    /// 令牌。
    pub token: String,
    /// 绑定 actor。
    pub actor: String,
    /// 角色。
    pub role: Role,
}

impl FileStore {
    /// 打开（或创建）根目录。
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(root.join("docs")).map_err(|error| io_error(&root, error))?;
        fs::create_dir_all(root.join("blobs")).map_err(|error| io_error(&root, error))?;
        // **悬空变更集**（设计 §12.4 ✓）：离线窗口可能跨服务重启 ✓ ⇒ 它必须落盘 ✓，
        // 否则"用户离线期间的编辑"会在重启时凭空消失 ✗。
        fs::create_dir_all(root.join("stash")).map_err(|error| io_error(&root, error))?;
        Ok(Self { root })
    }

    /// 落盘一个悬空变更集 ✓（`<root>/stash/<id>.json` ✓）。
    pub fn write_stash(&self, stash: &crate::service::Stash) -> Result<()> {
        let path = self.root.join("stash").join(format!("{}.json", stash.id));
        let encoded = serde_json::to_vec_pretty(stash).map_err(|error| {
            io_error(
                &path,
                std::io::Error::new(std::io::ErrorKind::InvalidData, error),
            )
        })?;
        // **先写临时文件再改名** ✓（与本项目其它落盘一致 ✓）：中途失败不会留下半个文件 ✗。
        let temporary = path.with_extension("json.tmp");
        fs::write(&temporary, &encoded).map_err(|error| io_error(&temporary, error))?;
        fs::rename(&temporary, &path).map_err(|error| io_error(&path, error))
    }

    /// 删除一个悬空变更集的文件 ✓（重放成功或用户丢弃 ✓）。
    ///
    /// **注意它不删 blob** ✓：§899 说 Stash 里的 blob 归**历史级保留、不被 GC** ✓ ——
    /// 而"丢弃这个 Stash"只是不再重放它 ✓，不是"把历史抹掉" ✗（本项目也没有 GC ✓）。
    pub fn remove_stash(&self, stash_id: &str) -> Result<()> {
        let path = self.root.join("stash").join(format!("{stash_id}.json"));
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(io_error(&path, error)),
        }
    }

    /// 根目录。
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// CAS（`<root>/blobs`）。
    ///
    /// **★ 必须带 `RenderCodec`** ✓（第 1236 轮查清的根因）
    ///   **问题**：原来它只是 `FsBlobStore::open(...)` ⇒ 用**默认的 `PlainCodec`**（identity）
    ///   ⇒ 而 `service.rs` 里**真正用于读写文档的那个 store** 是 `with_codec(RenderCodec)`
    ///   ⇒ **同一份 blob 数据被两个 store 用不同的 codec 解释** ✗
    ///     · `PlainCodec::decode` 恒返回 `Some(bytes)` ⇒ **裸 PNG 也「能解码」**
    ///     · `RenderCodec::decode` 认 `[0x00][TAG]` 头 ⇒ **裸 PNG 返回 `None`**
    ///   ⇒ ⇒ 于是**凡是走本函数的路径**（`put_blob` / `blob_decodable` / GC 统计）
    ///        都看到「能解码」⇒ **与渲染路径相反** ✗
    ///   ⇒ 这正是我 1202／1234／1235 三轮互相矛盾的**真正原因**（三个不同的观察点，
    ///     而它们背后是**两个不同的 store 实例**）
    ///
    ///   **修法**：本函数也带 `RenderCodec` ⇒ 全仓只有**一种** blob 解释 ✓
    pub fn blob_store(&self) -> Result<FsBlobStore> {
        Ok(FsBlobStore::open(self.root.join("blobs"))?
            .with_codec(std::sync::Arc::new(crate::blob_codec::RenderCodec)))
    }

    /// **校验 `doc_id` 是否可以安全当目录名** ✓（2026-10-20 第三轮复审 **C-02**）。
    ///
    /// **为什么必须有** ✗：`doc_dir` 原来是**裸 `join`** ——
    /// `self.root.join("docs").join(doc_id)` ⇒ 若 `doc_id` 含 `../`
    /// ⇒ **创建/导入/打开三条路径都能写到任意目录** ✗（路径穿越）。
    ///
    /// **规则**（白名单，不是黑名单 —— 黑名单总会漏）：
    /// · 非空、长度 ≤ 128
    /// · 只许 `[A-Za-z0-9._-]`
    /// · 不许 `.` / `..`（虽然白名单已挡 `/`，但 `.`/`..` 本身也是病态 id）
    /// · 不许以 `-`/`.` 开头（避免掩盖文件）
    pub fn validate_doc_id(doc_id: &str) -> Result<()> {
        let bad = |why: &str| {
            Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("doc_id {doc_id:?} 不合法：{why}")),
            ))
        };
        if doc_id.is_empty() {
            return bad("不能为空");
        }
        if doc_id.len() > 128 {
            return bad("长度超过 128");
        }
        if doc_id == "." || doc_id == ".." {
            return bad("不能是 . 或 ..");
        }
        if doc_id.starts_with('.') || doc_id.starts_with('-') {
            return bad("不能以 . 或 - 开头");
        }
        if let Some(ch) = doc_id
            .chars()
            .find(|c| !(c.is_ascii_alphanumeric() || *c == '.' || *c == '_' || *c == '-'))
        {
            return bad(&format!("含非法字符 {ch:?}（只许字母/数字/._-）"));
        }
        Ok(())
    }

    /// 文档目录。
    ///
    /// **⚠ 调用方必须先 `validate_doc_id`**（见上）。这里再兜一层：
    /// 若 id 不合法，返回一个**不可能被写到仓库外**的路径（`docs/_invalid`），
    /// 而不是让 `join` 穿越 —— 但**真正的拒绝在 `validate_doc_id`**，
    /// 这层只是「即使调用方忘了也不会穿越」（不是静默降级：入口会报错）。
    pub fn doc_dir(&self, doc_id: &str) -> PathBuf {
        let safe = if Self::validate_doc_id(doc_id).is_ok() {
            doc_id
        } else {
            debug_assert!(false, "doc_dir 收到未校验的 doc_id：{doc_id:?}");
            "_invalid"
        };
        self.root.join("docs").join(safe)
    }

    fn atoms_path(&self, doc_id: &str) -> PathBuf {
        self.doc_dir(doc_id).join("atoms.jsonl")
    }

    fn meta_path(&self, doc_id: &str) -> PathBuf {
        self.doc_dir(doc_id).join("meta.json")
    }

    /// **★ 找这份文档"最新可用的快照"（待办 ① ✓，2026-10-15 ✓）★**
    ///
    /// **为什么返回 `Option` 而不是 `Result`** ✓：调用处（`open_document_inner` ✓）要的语义是
    /// "**有就用、没有／坏了就算了**" ✓ —— 快照只是**加速** ✓。
    /// 若返回 `Result` ✗，调用处很容易写成 `?` ✗ ⇒ **一个坏快照就能让文档打不开** ✗（那是倒退 ✗）。
    ///
    /// **语义** ✓：列 `docs/<doc>/` ✓ ⇒ 挑 `snapshot-<seq>.json` 里 **`seq` 最大**的那个 ✓
    /// ⇒ 反序列化 ✓ ⇒ **`verify()`**（CRC ✓）⇒ 任一步失败一律 **`None`** ✓（**不报错、不 panic** ✓）。
    pub fn load_latest_snapshot(&self, doc_id: &str) -> Option<yanshi_core::Snapshot> {
        let dir = self.doc_dir(doc_id);
        let entries = fs::read_dir(&dir).ok()?;
        let mut best: Option<(Seq, std::path::PathBuf)> = None;
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_str()?;
            let Some(rest) = name.strip_prefix("snapshot-") else {
                continue;
            };
            let Some(digits) = rest.strip_suffix(".json") else {
                continue;
            };
            let Ok(seq) = digits.parse::<Seq>() else {
                continue;
            };
            if best.as_ref().is_none_or(|(current, _)| seq > *current) {
                best = Some((seq, entry.path()));
            }
        }
        let (_, path) = best?;
        let bytes = fs::read(&path).ok()?;
        let snapshot: yanshi_core::Snapshot = serde_json::from_slice(&bytes).ok()?;
        // **必须校验** ✓：坏快照 ⇒ 当没有 ✓ ⇒ 回退全量读 ✓（绝不让打开失败 ✗）。
        if !snapshot.verify() {
            return None;
        }
        Some(snapshot)
    }

    /// **★ 快照文件路径（待办 ① ✓）：`docs/<doc>/snapshot-<seq_at>.json` ★**
    ///
    /// **为什么把 `seq_at` 放进文件名** ✓：这样"**文件是否存在**"就等于"**写没写过**" ✓
    /// ⇒ 调用处**不需要任何新状态** ✓（见 `docs/design/vermeer-acceptance.md` §49 之补九 ✓）。
    fn snapshot_path(&self, doc_id: &str, seq_at: Seq) -> PathBuf {
        self.doc_dir(doc_id).join(format!("snapshot-{seq_at}.json"))
    }

    /// **★ 把一份快照写到盘上（待办 ① ✓）：幂等 ⇒ 已存在就返回 `false` ★**
    ///
    /// **语义** ✓：写 `docs/<doc>/snapshot-<seq_at>.json` ✓（`serde_json::to_vec` ✓，
    /// `Snapshot` 本身就 `derive(Serialize)` ✓）；
    /// **幂等** ✓：同一 `seq_at` 的文件已存在 ⇒ **不重写**、直接 `Ok(false)` ✓；
    /// 真写了 ⇒ `Ok(true)` ✓（返回 `bool` 是给调用处记账／打日志用的 ✓）。
    /// **失败怎么办** ✓：**由调用处决定** ✗ —— 落盘失败**不该让提交失败** ✓
    /// （快照只是**加速**用的 ✓）⇒ 调用处记 stderr ＋ 继续 ✓。
    pub fn save_snapshot(&self, doc_id: &str, snapshot: &yanshi_core::Snapshot) -> Result<bool> {
        let dir = self.doc_dir(doc_id);
        fs::create_dir_all(&dir).map_err(|error| io_error(&dir, error))?;
        let path = self.snapshot_path(doc_id, snapshot.seq_at);
        if path.exists() {
            return Ok(false); // **幂等** ✓：同一 `seq_at` 只写一次 ✓
        }
        let bytes = serde_json::to_vec(snapshot).map_err(|error| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("快照序列化失败: {error}")),
            )
        })?;
        fs::write(&path, bytes).map_err(|error| io_error(&path, error))?;
        Ok(true)
    }

    /// 追加一个原子（JSONL）。
    pub fn append_atom(&self, doc_id: &str, atom: &Atom) -> Result<()> {
        let dir = self.doc_dir(doc_id);
        fs::create_dir_all(&dir).map_err(|error| io_error(&dir, error))?;
        let path = self.atoms_path(doc_id);
        // **写前必须看盘上已到几号** ✓（第 749 轮；台账三百三十的**根因修复**）
        //   **⇒ 缺陷**：追加时**从不看盘** ✗ ⇒ 另一个进程（有自己的 `next_seq` ✓）占掉的号
        //     会被**照写一遍** ✓ ⇒ 盘上出现**重复 `seq`** ✓ ⇒ 下次打开要靠
        //     `repair_duplicate_seqs` 重编号才打得开 ✓（止血有效，但**写侧一直在制造脏数据** ✓）。
        //   **⇒ 修法**：追加前读盘上最后一条的 `seq` ✓；若本次 `seq` **不大于**它 ⇒ **响亮报错** ✓
        //     （**不重编号**：重编号会破坏 `revert`／`supersede` 里对旧 `seq` 的引用 ✓）。
        //   **⇒ 判据**：`two_writers_seqs` 里"另一个进程占掉号"之后，本进程的下一笔
        //     **必须失败**且盘上**保持唯一** ✓（变异：去掉这段 ⇒ 又出现重复 ⇒ 红 ✓）。
        if let Some(last) = self.last_seq_on_disk(doc_id)? {
            if atom.seq <= last {
                return Err(YanshiError::new(
                    ErrorCode::PreconditionFailed,
                    ErrorContext::detail(format!(
                        "文档 {doc_id} 的日志已经写到 seq {last}，而这一条是 seq {} ⇒ \
                         拒绝写入（**另一个进程正在写同一份文档**？）\
                         ⇒ 请只保留一个写者，或等它退出后重开（盘上保持唯一比丢一条更重要）",
                        atom.seq
                    )),
                ));
            }
        }
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|error| io_error(&path, error))?;
        let line = serde_json::to_string(atom).map_err(|error| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("原子序列化失败: {error}")),
            )
            .with_atom(atom.id.clone())
        })?;
        writeln!(file, "{line}").map_err(|error| io_error(&path, error))?;
        file.flush().map_err(|error| io_error(&path, error))?;
        Ok(())
    }

    /// 读取全部原子（忽略空行与损坏的尾行）。
    ///
    /// **★ 流式读取（2026-10-15 ✓）★**：原来是 `fs::read_to_string` 把**整个文件**读成 `String`
    /// ⇒ 大日志（几十～上百 MB）下峰值内存 ≈ **2× 文件大小** ✗（整文本 ＋ `Vec<Atom>` 同时活着 ✓）。
    /// 现在逐行读 ✓ ⇒ 峰值 ≈ **1× 原子 ＋ 一行** ✓。
    ///
    /// **盘上最后一条原子的 `seq`** ✓（第 749 轮）：只读**尾部 8 KiB** 并解析最后一整行 ✓
    /// （`atoms.jsonl` 可能几十 MB ✓ ⇒ 不整份读 ✓）。没有文件／空文件 ⇒ `None` ✓。
    fn last_seq_on_disk(&self, doc_id: &str) -> Result<Option<u64>> {
        use std::io::{Read as _, Seek as _, SeekFrom};
        let path = self.atoms_path(doc_id);
        let mut file = match fs::File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io_error(&path, error)),
        };
        let len = file
            .metadata()
            .map_err(|error| io_error(&path, error))?
            .len();
        if len == 0 {
            return Ok(None);
        }
        let window = len.min(8 * 1024);
        file.seek(SeekFrom::Start(len - window))
            .map_err(|error| io_error(&path, error))?;
        let mut tail = String::new();
        file.read_to_string(&mut tail)
            .map_err(|error| io_error(&path, error))?;
        // **最后一条完整行** ✓（文件末尾有换行 ⇒ 去掉末尾空串 ✓）
        for line in tail.lines().rev() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            match serde_json::from_str::<Atom>(trimmed) {
                Ok(atom) => return Ok(Some(atom.seq)),
                // **尾部窗口切断了第一行** ⇒ 它不是完整 JSON ✓ ⇒ 继续往前 ✓
                Err(_) => continue,
            }
        }
        Ok(None)
    }

    /// **语义逐条保持** ✓（不许悄悄改行为 ✗）：
    ///   * 文件不存在 ⇒ 空列表 ✓；
    ///   * 跳过空行 ✓；
    ///   * **只有最后一行**损坏才容忍 ✓ —— 判法改用"**它后面还有没有内容**" ✓
    ///     （原来多跑一趟 `text.lines().count()` ✗，那在大文件上是一整遍额外扫描 ✓）；
    ///   * 中间任何一行损坏 ⇒ 报错并指出**行号** ✓。
    pub fn load_atoms(&self, doc_id: &str) -> Result<Vec<Atom>> {
        let path = self.atoms_path(doc_id);
        let file = match fs::File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(io_error(&path, error)),
        };
        let mut atoms = Vec::new();
        let mut reader = std::io::BufReader::new(file);
        let mut line = String::new();
        let mut index: usize = 0;
        let mut broken: Option<(usize, String)> = None;
        loop {
            line.clear();
            let read = reader
                .read_line(&mut line)
                .map_err(|error| io_error(&path, error))?;
            if read == 0 {
                break;
            }
            index += 1;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some((broken_index, message)) = &broken {
                // 已经遇到坏行 ⇒ 只要**后面还有内容**，它就是"中间损坏" ⇒ 报错 ✓
                //（与原来的"坏行不是最后一行就报错"**逐字等价** ✓）。
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!("原子日志第 {broken_index} 行损坏: {message}")),
                ));
            }
            match serde_json::from_str::<Atom>(trimmed) {
                Ok(atom) => atoms.push(atom),
                Err(error) => {
                    // 先候选"尾部半行"✓ —— 到底算不算，要看**后面还有没有行** ✓（见上面的检查 ✓）。
                    broken = Some((index, error.to_string()));
                }
            }
        }
        Ok(atoms)
    }

    /// **★ 只读"某一 seq 之后"的原子（待办 ① 的"快照 ＋ 尾巴"要用 ✓，2026-10-15 ✓）★**
    ///
    /// **为什么** ✗：`load_atoms` 会把**整份日志**读进来 ✓；有了**快照**之后 ✓，
    /// 快照已经覆盖到某个 `seq_at` ✓ ⇒ 前面那一段**不必再读** ✓
    /// ⇒ ⇒ 本函数只返回 `seq > since` 的那些 ✓ ⇒ **打开时读入的原子数自然变小** ✓
    /// （这正是 `Document::atoms_read_on_open` 观测的量 ✓，判据 `tests/open_readout.rs` 断言它 ✓）。
    ///
    /// **语义与 `load_atoms` 逐条一致** ✓（只有一处不同：多了一道 `seq > since` 的门 ✓）：
    ///   * 文件不存在 ⇒ 空列表 ✓；跳过空行 ✓；**只有最后一行**损坏才容忍 ✓（判法＝"后面还有没有内容" ✓）；
    ///   * 中间任何一行损坏 ⇒ 报错并指出**行号** ✓；
    ///   * **流式**逐行 ✓（峰值 ≈ 1× 原子 ＋ 一行 ✓）。
    ///
    /// **注意** ✓：`since = 0` 时它与 `load_atoms` **等价** ✓ —— 保留两个入口是为了不打断既有调用 ✓；
    /// 将来可让 `load_atoms` 直接委托给本函数（那时只剩一份实现 ✓）。
    pub fn load_atoms_since(&self, doc_id: &str, since: Seq) -> Result<Vec<Atom>> {
        let path = self.atoms_path(doc_id);
        let file = match fs::File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(io_error(&path, error)),
        };
        let mut atoms = Vec::new();
        let mut reader = std::io::BufReader::new(file);
        let mut line = String::new();
        let mut index: usize = 0;
        let mut broken: Option<(usize, String)> = None;
        loop {
            line.clear();
            let read = reader
                .read_line(&mut line)
                .map_err(|error| io_error(&path, error))?;
            if read == 0 {
                break;
            }
            index += 1;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some((broken_index, message)) = &broken {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!("原子日志第 {broken_index} 行损坏: {message}")),
                ));
            }
            match serde_json::from_str::<Atom>(trimmed) {
                Ok(atom) => {
                    // **唯一的差别** ✓：只收 `seq > since` 的 ✓（快照已覆盖的那一段跳过 ✓）。
                    if atom.seq > since {
                        atoms.push(atom);
                    }
                }
                Err(error) => broken = Some((index, error.to_string())),
            }
        }
        Ok(atoms)
    }

    /// 写入文档元数据。
    pub fn save_meta(&self, meta: &DocumentMeta) -> Result<()> {
        let dir = self.doc_dir(&meta.doc_id);
        fs::create_dir_all(&dir).map_err(|error| io_error(&dir, error))?;
        let path = self.meta_path(&meta.doc_id);
        let text = serde_json::to_string_pretty(meta).map_err(|error| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("元数据序列化失败: {error}")),
            )
        })?;
        fs::write(&path, text).map_err(|error| io_error(&path, error))?;
        Ok(())
    }

    /// 读取文档元数据（不存在返回 `None`）。
    pub fn load_meta(&self, doc_id: &str) -> Result<Option<DocumentMeta>> {
        let path = self.meta_path(doc_id);
        match fs::read_to_string(&path) {
            Ok(text) => {
                let meta = serde_json::from_str(&text).map_err(|error| {
                    YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail(format!("元数据解析失败: {error}")),
                    )
                })?;
                Ok(Some(meta))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(io_error(&path, error)),
        }
    }

    /// 记录一个令牌。
    ///
    /// **★ 尺寸必须由**调用方**给 ✗ ★**（**2026-10-16 ✓**）：`meta.json` **缺失**时，
    /// 旧代码会**凭空写一份 `width: 0, height: 0`** ✗ —— 那是一个**谎** ✓：
    /// 它把「**我不知道尺寸**」写成了「**尺寸是 0×0**」✗ ⇒ 之后**所有**读 `meta.json` 的地方
    /// （例如 `list_documents` 的**未打开**分支 ✓）都跟着报 **0×0** ✗
    /// —— 而这正是真实用户报告里 `yanshi-mcp` 报 0×0、`yanshi-serve` 报对的那条分叉 ✓。
    ///
    /// **∴ 为什么让调用方给 ✗**：调用方 [`crate::service::Workspace::issue_token`] **手里正拿着那份文档** ✓
    /// ⇒ 尺寸是**已知的** ✓ ⇒ 传进来 ✓，**不要让持久层去猜** ✗（猜出来的 0 会被写进盘 ✓）。
    pub fn record_token(
        &self,
        doc_id: &str,
        width: u32,
        height: u32,
        token: &CapabilityToken,
        actor: &str,
        role: Role,
    ) -> Result<()> {
        let mut meta = self.load_meta(doc_id)?.unwrap_or(DocumentMeta {
            doc_id: doc_id.to_owned(),
            width,
            height,
            created_at: yanshi_core::now_ms(),
            tokens: Vec::new(),
        });
        meta.tokens.retain(|record| record.token != token.as_str());
        meta.tokens.push(TokenRecord {
            token: token.as_str().to_owned(),
            actor: actor.to_owned(),
            role,
        });
        self.save_meta(&meta)
    }

    /// 保存渲染缓存（`render.png` + `render.seq`）。
    pub fn save_render(&self, doc_id: &str, seq: Seq, png: &[u8]) -> Result<PathBuf> {
        self.save_image_cache(doc_id, "render", seq, png)
    }

    /// 读取渲染缓存。
    pub fn load_render(&self, doc_id: &str) -> Result<Option<(Seq, Vec<u8>)>> {
        self.load_image_cache(doc_id, "render")
    }

    /// **保存文档预览缓存**（`preview.png` + `preview.seq`）✓（冷启动复用专题）。
    ///
    /// **为什么必须与 `render.png` 分开两个文件** ✗：两者的用途与代价完全不同 ✓ ——
    /// `render.png` 是**整幅**导出缓存 ✓（4K 实测 6.7MB ✓，解码 **153.7s** ✓），
    /// 只在"整幅区域渲染与 HEAD 一致"时才值得用 ✓；
    /// `preview.png` 是**小的**文档预览 ✓（256² ✓，解码 ~1s ✓），是**增量预览的像素基座** ✓。
    /// 若共用一份文件 ✗：写预览（256²）会**顶掉**整幅缓存 ✓、写整幅会**顶掉**预览基座 ✓
    /// ⇒ 两条复用路径**互相破坏** ✗（实测正是如此 ✓）。
    pub fn save_preview(&self, doc_id: &str, seq: Seq, png: &[u8]) -> Result<PathBuf> {
        self.save_image_cache(doc_id, "preview", seq, png)
    }

    /// 读取文档预览缓存。
    pub fn load_preview(&self, doc_id: &str) -> Result<Option<(Seq, Vec<u8>)>> {
        self.load_image_cache(doc_id, "preview")
    }

    /// 图像缓存写入（`<name>.png` + `<name>.seq`；先写临时文件再改名 ✓）。
    fn save_image_cache(&self, doc_id: &str, name: &str, seq: Seq, png: &[u8]) -> Result<PathBuf> {
        let dir = self.doc_dir(doc_id);
        fs::create_dir_all(&dir).map_err(|error| io_error(&dir, error))?;
        let image_path = dir.join(format!("{name}.png"));
        let seq_path = dir.join(format!("{name}.seq"));
        // 先写临时文件再 rename，避免崩溃留下半张图（与 CAS 同一策略）。
        let tmp = dir.join(format!("{name}.png.tmp"));
        fs::write(&tmp, png).map_err(|error| io_error(&tmp, error))?;
        fs::rename(&tmp, &image_path).map_err(|error| io_error(&image_path, error))?;
        fs::write(&seq_path, seq.to_string()).map_err(|error| io_error(&seq_path, error))?;
        Ok(image_path)
    }

    /// 图像缓存读取。
    fn load_image_cache(&self, doc_id: &str, name: &str) -> Result<Option<(Seq, Vec<u8>)>> {
        let dir = self.doc_dir(doc_id);
        let image_path = dir.join(format!("{name}.png"));
        let seq_path = dir.join(format!("{name}.seq"));
        let png = match fs::read(&image_path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io_error(&image_path, error)),
        };
        let seq = fs::read_to_string(&seq_path)
            .ok()
            .and_then(|text| text.trim().parse::<Seq>().ok())
            .unwrap_or(0);
        Ok(Some((seq, png)))
    }

    /// 列出已有文档 id（按字典序）。
    pub fn list_documents(&self) -> Result<Vec<String>> {
        let docs = self.root.join("docs");
        let mut ids = Vec::new();
        let entries = match fs::read_dir(&docs) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(ids),
            Err(error) => return Err(io_error(&docs, error)),
        };
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    ids.push(name.to_owned());
                }
            }
        }
        ids.sort();
        Ok(ids)
    }
}

fn io_error(path: &Path, error: std::io::Error) -> YanshiError {
    YanshiError::new(
        ErrorCode::ResourceExhausted,
        ErrorContext::detail(format!("持久化 IO 失败 {:?}: {error}", path)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_root(tag: &str) -> PathBuf {
        let mut root = std::env::temp_dir();
        root.push(format!("yanshi-store-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        root
    }

    fn atom(id: &str, seq: Seq) -> Atom {
        let mut atom = Atom::new(
            yanshi_core::AtomKind::CreateLayer,
            "human:1",
            "session:a",
            json!({"layer_id": "layer_1"}),
        )
        .with_id(id);
        atom.seq = seq;
        atom
    }

    #[test]
    fn atoms_round_trip_as_jsonl() {
        let root = temp_root("atoms");
        let store = FileStore::open(&root).unwrap();
        for index in 1..=5 {
            store
                .append_atom("doc_1", &atom(&format!("a{index}"), index))
                .unwrap();
        }
        let atoms = store.load_atoms("doc_1").unwrap();
        assert_eq!(atoms.len(), 5);
        assert_eq!(atoms[4].seq, 5);
        assert_eq!(atoms[0].id, "a1");
        assert!(store.load_atoms("doc_missing").unwrap().is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn truncated_tail_line_is_tolerated() {
        let root = temp_root("tail");
        let store = FileStore::open(&root).unwrap();
        store.append_atom("doc_1", &atom("a1", 1)).unwrap();
        store.append_atom("doc_1", &atom("a2", 2)).unwrap();
        // 模拟崩溃：追加半行。
        let path = store.doc_dir("doc_1").join("atoms.jsonl");
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        write!(file, "{{\"id\":\"a3\",\"seq\":3").unwrap();
        drop(file);
        let atoms = store.load_atoms("doc_1").unwrap();
        assert_eq!(atoms.len(), 2, "半行被忽略");
        let _ = fs::remove_dir_all(&root);
    }

    /// **★ 中间损坏 ⇒ 必须报错，并指出行号（2026-10-15 补 ✓）★**
    ///
    /// **为什么补它** ✗：原来只测了"**半行被容忍**"那一半 ✓ ⇒ 另一半
    /// （「只有**最后一行**可以坏；**中间**坏了必须报错」✓）**没有测试** ✓ ——
    /// 而流式改写（`fs::read_to_string` ⇒ `BufReader` 逐行 ✓）动的正是**这条判定** ✓
    /// ⇒ 必须有东西钉住它 ✗（否则哪天悄悄变成"什么都容忍"也没人知道 ✓）。
    #[test]
    fn corrupt_middle_line_fails_with_the_line_number() {
        let root = temp_root("mid");
        let store = FileStore::open(&root).unwrap();
        store.append_atom("doc_1", &atom("a1", 1)).unwrap();
        let path = store.doc_dir("doc_1").join("atoms.jsonl");
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(file, "not json").unwrap(); // 第 2 行坏
        drop(file);
        // **后面还有内容** ⇒ 它不是"崩溃残留的半行" ⇒ 必须报错 ✓
        store.append_atom("doc_1", &atom("a2", 2)).unwrap();
        let error = store.load_atoms("doc_1").unwrap_err();
        let text = format!("{error:?} {error}");
        assert!(text.contains("第 2 行"), "报错应当指出行号，实测：{text}");
    }

    /// **★ 末尾**完整的一行**坏（带换行）也被容忍 ✓ —— 流式改写的**等价性边界** ✓**
    ///
    /// 旧实现按"**是不是最后一行**"判 ✓；新实现按"**后面还有没有内容**"判 ✓
    /// ⇒ ⇒ 两者在"末尾整行坏（带 `\n`）"这一格上**必须一致** ✓（都容忍 ✓）。
    /// **这一格最容易在改写时走偏** ✗（新实现若只看"行尾有没有 `\n`"就会**报错** ✗）⇒ 钉住它 ✓。
    #[test]
    fn corrupt_final_line_with_newline_is_tolerated() {
        let root = temp_root("fin");
        let store = FileStore::open(&root).unwrap();
        store.append_atom("doc_1", &atom("a1", 1)).unwrap();
        let path = store.doc_dir("doc_1").join("atoms.jsonl");
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(file, "not json").unwrap(); // 完整一行（带 \n）且是**最后**一行
        drop(file);
        let atoms = store.load_atoms("doc_1").unwrap();
        assert_eq!(atoms.len(), 1, "末尾坏行应当被容忍（与旧实现一致）");
    }

    #[test]
    fn meta_and_tokens_round_trip() {
        let root = temp_root("meta");
        let store = FileStore::open(&root).unwrap();
        assert!(store.load_meta("doc_1").unwrap().is_none());
        store
            .save_meta(&DocumentMeta {
                doc_id: "doc_1".to_owned(),
                width: 64,
                height: 32,
                created_at: 123,
                tokens: Vec::new(),
            })
            .unwrap();
        let token = CapabilityToken::generate();
        // **meta 已存在 ⇒ 传进来的尺寸不参与** ✓（真正的尺寸从 `meta.json` 读 ✓）。
        store
            .record_token("doc_1", 64, 32, &token, "human:1", Role::Editor)
            .unwrap();
        let meta = store.load_meta("doc_1").unwrap().unwrap();
        assert_eq!((meta.width, meta.height), (64, 32));
        assert_eq!(meta.tokens.len(), 1);
        assert_eq!(meta.tokens[0].actor, "human:1");
        assert_eq!(meta.tokens[0].role, Role::Editor);

        // 重复记录同一 token 不产生重复条目。
        store
            .record_token("doc_1", 64, 32, &token, "human:1", Role::Owner)
            .unwrap();
        let meta = store.load_meta("doc_1").unwrap().unwrap();
        assert_eq!(meta.tokens.len(), 1);
        assert_eq!(meta.tokens[0].role, Role::Owner);
        // **★ `meta.json` **缺失**时不许凭空写 `0×0` ✗ ★**（2026-10-16 ✓）：
        //   **∴ 旧行为 ✗**：`unwrap_or(DocumentMeta{ width: 0, height: 0, … })` ✓
        //     ⇒ **∴ 于是**它把「我不知道尺寸」写成「尺寸是 0×0」✗ ⇒
        //       **之后所有读 `meta.json` 的地方都跟着报 0×0** ✗
        //       （真实用户报告：`yanshi-mcp` 报 0×0 而 `yanshi-serve` 报对 ✓）。
        //   **∴ 判据 ✗**：删掉 meta 再记令牌 ⇒ 盘上写的必须是**调用方给的**真实尺寸 ✓。
        let root_missing = temp_root("meta-missing");
        let store_missing = FileStore::open(&root_missing).unwrap();
        let token_missing = CapabilityToken::generate();
        store_missing
            .record_token("doc_2", 200, 150, &token_missing, "human:2", Role::Owner)
            .unwrap();
        let meta_missing = store_missing.load_meta("doc_2").unwrap().unwrap();
        assert_eq!(
            (meta_missing.width, meta_missing.height),
            (200, 150),
            "meta 缺失时不许写 0×0（那会让 list_documents 永久报 0×0 ✗）"
        );
        let _ = fs::remove_dir_all(&root_missing);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn render_cache_round_trip_and_document_listing() {
        let root = temp_root("render");
        let store = FileStore::open(&root).unwrap();
        assert!(store.load_render("doc_1").unwrap().is_none());
        store.save_render("doc_1", 42, b"png-bytes").unwrap();
        let (seq, bytes) = store.load_render("doc_1").unwrap().unwrap();
        assert_eq!(seq, 42);
        assert_eq!(bytes, b"png-bytes");
        assert_eq!(store.list_documents().unwrap(), vec!["doc_1"]);
        store.append_atom("doc_2", &atom("a1", 1)).unwrap();
        assert_eq!(store.list_documents().unwrap(), vec!["doc_1", "doc_2"]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn blob_store_is_content_addressed_under_root() {
        use yanshi_core::blob::BlobStore;
        let root = temp_root("blobs");
        let store = FileStore::open(&root).unwrap();
        let blobs = store.blob_store().unwrap();
        let hash = blobs.put(b"bitmap").unwrap();
        assert!(blobs.exists(&hash));
        assert_eq!(blobs.get(&hash).unwrap(), b"bitmap");
        assert!(store.root().join("blobs").join("sha256").is_dir());
        let _ = fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod doc_id_validation_tests {
    //! **C-02 判据**（第三轮复审）：`doc_id` 必须不能穿越目录。
    //!
    //! 判据不是「`validate_doc_id` 返回 Ok」而是**逐类拒绝**：
    //! 只要有一类被放过，路径就穿越了 ⇒ 所以每类单独一条断言。

    use super::FileStore;

    fn ok(id: &str) -> bool {
        FileStore::validate_doc_id(id).is_ok()
    }

    #[test]
    fn rejects_path_traversal_and_separators() {
        // **这是 C-02 的核心**：`../` 与分隔符必须拒绝
        for bad in [
            "../etc",
            "..",
            ".",
            "a/b",
            "/abs",
            "a\\b",
            "..\\win",
            "docs/../../x",
        ] {
            assert!(!ok(bad), "必须拒绝 {bad:?}");
        }
    }

    #[test]
    fn rejects_empty_long_and_hidden() {
        assert!(!ok(""));
        assert!(!ok(&"a".repeat(129)), "129 字符必须拒绝");
        assert!(!ok(".hidden"), "以 . 开头必须拒绝");
        assert!(!ok("-flag"), "以 - 开头必须拒绝");
    }

    #[test]
    fn rejects_non_ascii_and_spaces() {
        for bad in ["文档", "a b", "a:b", "a*b", "a?b", "a\u{0}b"] {
            assert!(!ok(bad), "必须拒绝 {bad:?}");
        }
    }

    #[test]
    fn accepts_normal_ids() {
        // **正向**：正常 id 不能被误拒（否则功能坏了）
        for good in [
            "_b",
            "doc1",
            "a.b",
            "a-b",
            "a_b",
            "A1",
            "x".repeat(128).as_str(),
        ] {
            assert!(ok(good), "不该拒绝 {good:?}");
        }
    }
}
