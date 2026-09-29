//! 逻辑快照与清理策略（设计文档 4.5 / 6.5）。
//!
//! **快照不是原子**：它是 L1 旁路缓存，不进入日志、不参与折叠。快照通过
//! `seq_at` 锚定 `state@seq_n`（5.5 公式求值），带 CRC 自校验与活跃 blob Manifest。
//!
//! 清理例外（6.5）：用户显式 `checkpoint` 关联的快照不清理；最近 N 个
//! `declare_head` 的 base 状态快照保留。

use crate::atom::{AtomId, BlobHash};
use crate::error::{ErrorCode, ErrorContext, Result, YanshiError};
use crate::ids::{now_ms, Seq, SnapshotId, Ulid};
use crate::log::AtomLog;
use crate::seq::{state_at, StateAtCache};
use crate::state::DocumentState;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::OnceLock;

/// 快照触发阈值：落后原子数（设计文档 6.5）。
pub const SNAPSHOT_LAG_ATOMS: Seq = 1000;
/// 快照触发阈值：有新增原子时的最大空闲秒数（设计文档 6.5）。
pub const SNAPSHOT_IDLE_SECONDS: i64 = 30;

/// 快照的父引用（`base_ref: {type: declare_head | snapshot, id}`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "id", rename_all = "snake_case")]
pub enum SnapshotBase {
    /// 以上一次求值起点跳变（`declare_head`）为基。
    DeclareHead(AtomId),
    /// 以前一个快照为基（增量存储）。
    Snapshot(SnapshotId),
}

/// 逻辑快照（设计文档 4.5）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// 快照 id。
    pub id: SnapshotId,
    /// 父引用。
    pub base_ref: SnapshotBase,
    /// 锚定 seq（按 5.5 公式求值）。
    pub seq_at: Seq,
    /// 逻辑状态。
    pub state: DocumentState,
    /// 活跃 blob 清单：活跃集标记与冷热迁移依据，**不是 GC 根集**。
    pub manifest: Vec<BlobHash>,
    /// CRC 校验串。
    pub crc: String,
}

#[derive(Serialize)]
struct CrcPayload<'a> {
    base_ref: &'a SnapshotBase,
    seq_at: Seq,
    state: &'a DocumentState,
    manifest: &'a [BlobHash],
}

impl Snapshot {
    /// 按 5.5 公式求值 `state@n` 并生成快照。
    pub fn create(
        log: &AtomLog,
        n: Seq,
        base_ref: SnapshotBase,
        cache: &mut StateAtCache,
    ) -> Result<Self> {
        let evaluated = state_at(log, n, cache)?;
        let manifest: Vec<BlobHash> = evaluated.state.active_blob_manifest().into_iter().collect();
        let mut snapshot = Self {
            id: Ulid::new().encode(),
            base_ref,
            seq_at: evaluated.seq,
            state: evaluated.state,
            manifest,
            crc: String::new(),
        };
        snapshot.crc = snapshot.compute_crc()?;
        Ok(snapshot)
    }

    /// 由已经求得的状态生成快照（服务端已有 HEAD 状态时避免重复折叠）。
    ///
    /// `seq_at` 必须是该状态对应的序列位置（即 `state.head_seq` 或调用方确认的位置）。
    pub fn from_state(seq_at: Seq, base_ref: SnapshotBase, state: DocumentState) -> Result<Self> {
        let manifest: Vec<BlobHash> = state.active_blob_manifest().into_iter().collect();
        let mut snapshot = Self {
            id: Ulid::new().encode(),
            base_ref,
            seq_at,
            state,
            manifest,
            crc: String::new(),
        };
        snapshot.crc = snapshot.compute_crc()?;
        Ok(snapshot)
    }

    /// 以某个 `declare_head` 原子为 base_ref 生成快照。
    pub fn create_at_head(
        log: &AtomLog,
        n: Seq,
        declare_head_atom: impl Into<AtomId>,
        cache: &mut StateAtCache,
    ) -> Result<Self> {
        Self::create(
            log,
            n,
            SnapshotBase::DeclareHead(declare_head_atom.into()),
            cache,
        )
    }

    /// 重新计算 CRC。
    pub fn compute_crc(&self) -> Result<String> {
        let payload = CrcPayload {
            base_ref: &self.base_ref,
            seq_at: self.seq_at,
            state: &self.state,
            manifest: &self.manifest,
        };
        let bytes = serde_json::to_vec(&payload).map_err(|error| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("快照序列化失败: {error}")),
            )
        })?;
        Ok(format!("crc32:{:08x}", crc32(&bytes)))
    }

    /// CRC 自校验。
    pub fn verify(&self) -> bool {
        self.compute_crc()
            .map(|crc| crc == self.crc)
            .unwrap_or(false)
    }

    /// 恢复逻辑状态（时间旅行）。
    pub fn restore(&self) -> DocumentState {
        self.state.clone()
    }
}

/// IEEE CRC-32（快照自校验，无需额外依赖）。
pub fn crc32(bytes: &[u8]) -> u32 {
    static TABLE: OnceLock<[u32; 256]> = OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut table = [0u32; 256];
        for (index, slot) in table.iter_mut().enumerate() {
            let mut value = index as u32;
            for _ in 0..8 {
                value = if value & 1 == 1 {
                    0xEDB8_8320 ^ (value >> 1)
                } else {
                    value >> 1
                };
            }
            *slot = value;
        }
        table
    });
    let mut crc = 0xFFFF_FFFFu32;
    for byte in bytes {
        let index = ((crc ^ u32::from(*byte)) & 0xFF) as usize;
        crc = table[index] ^ (crc >> 8);
    }
    crc ^ 0xFFFF_FFFF
}

/// 快照触发原因（设计文档 6.5）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotTrigger {
    /// 无触发条件。
    None,
    /// 落后 ≥ 1000 原子。
    LaggingAtoms,
    /// ≥ 30s 且有新原子。
    IdleWithNewAtoms,
    /// 提交了重型原子。
    HeavyAtom,
}

/// 快照策略判定结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotDecision {
    /// 是否需要生成快照。
    pub needed: bool,
    /// 触发原因。
    pub trigger: SnapshotTrigger,
    /// 落后的原子数。
    pub lag_atoms: Seq,
}

/// 判定是否需要生成快照（设计文档 6.5）。
///
/// `all_declare_heads_snapshotted` 由调用方保证：重型原子中的 `declare_head`
/// 必须强制快照（本函数只识别 lag / idle / heavy 三类）。
pub fn snapshot_decision(
    log: &AtomLog,
    last_snapshot_seq: Seq,
    last_snapshot_at_ms: i64,
    now: i64,
) -> SnapshotDecision {
    let head = log.head_seq();
    let lag_atoms = head.saturating_sub(last_snapshot_seq);
    if lag_atoms >= SNAPSHOT_LAG_ATOMS {
        return SnapshotDecision {
            needed: true,
            trigger: SnapshotTrigger::LaggingAtoms,
            lag_atoms,
        };
    }
    if lag_atoms > 0 {
        if log
            .range_exclusive_inclusive(last_snapshot_seq, head)
            .iter()
            .any(|atom| atom.is_heavy())
        {
            return SnapshotDecision {
                needed: true,
                trigger: SnapshotTrigger::HeavyAtom,
                lag_atoms,
            };
        }
        if last_snapshot_at_ms > 0 && (now - last_snapshot_at_ms) / 1000 >= SNAPSHOT_IDLE_SECONDS {
            return SnapshotDecision {
                needed: true,
                trigger: SnapshotTrigger::IdleWithNewAtoms,
                lag_atoms,
            };
        }
    }
    SnapshotDecision {
        needed: false,
        trigger: SnapshotTrigger::None,
        lag_atoms,
    }
}

/// 快照缓存：可 LRU 清理，但用户检查点关联的快照与最近 N 个 `declare_head`
/// 的 base 快照不清理（设计文档 6.5）。
#[derive(Debug)]
pub struct SnapshotStore {
    snapshots: BTreeMap<SnapshotId, Snapshot>,
    lru: VecDeque<SnapshotId>,
    pinned: BTreeSet<SnapshotId>,
    max_entries: usize,
    evicted: Vec<SnapshotId>,
}

impl SnapshotStore {
    /// 以容量上限构造。
    pub fn new(max_entries: usize) -> Self {
        Self {
            snapshots: BTreeMap::new(),
            lru: VecDeque::new(),
            pinned: BTreeSet::new(),
            max_entries,
            evicted: Vec::new(),
        }
    }

    /// 条目数。
    pub fn len(&self) -> usize {
        self.snapshots.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }

    /// 写入快照并执行清理。
    pub fn insert(&mut self, snapshot: Snapshot) -> Vec<SnapshotId> {
        self.lru.push_back(snapshot.id.clone());
        self.snapshots.insert(snapshot.id.clone(), snapshot);
        self.evict_to_fit()
    }

    /// 读取快照。
    pub fn get(&self, id: &str) -> Option<&Snapshot> {
        self.snapshots.get(id)
    }

    /// 最新且 CRC 可用的快照（CRC 失败则回退更早快照，设计文档 6.5）。
    pub fn latest_usable(&self) -> Option<&Snapshot> {
        self.snapshots
            .values()
            .filter(|snapshot| snapshot.verify())
            .max_by_key(|snapshot| snapshot.seq_at)
    }

    /// 锚定 seq 不晚于 `seq` 的最新可用快照。
    pub fn nearest_usable_at_or_before(&self, seq: Seq) -> Option<&Snapshot> {
        self.snapshots
            .values()
            .filter(|snapshot| snapshot.seq_at <= seq && snapshot.verify())
            .max_by_key(|snapshot| snapshot.seq_at)
    }

    /// 固定（用户检查点关联、declare_head base）：不参与 LRU 清理。
    pub fn pin(&mut self, id: impl Into<SnapshotId>) {
        self.pinned.insert(id.into());
    }

    /// 取消固定。
    pub fn unpin(&mut self, id: &str) {
        self.pinned.remove(id);
    }

    /// 是否被固定。
    pub fn is_pinned(&self, id: &str) -> bool {
        self.pinned.contains(id)
    }

    /// 已清理的快照 id。
    pub fn evicted(&self) -> &[SnapshotId] {
        &self.evicted
    }

    /// 超过容量时按 LRU 清理，跳过被固定者。
    pub fn evict_to_fit(&mut self) -> Vec<SnapshotId> {
        let mut evicted = Vec::new();
        let mut pinned_streak = 0usize;
        while self.snapshots.len() > self.max_entries {
            let Some(candidate) = self.lru.pop_front() else {
                break;
            };
            if self.pinned.contains(&candidate) {
                self.lru.push_back(candidate);
                pinned_streak += 1;
                if pinned_streak >= self.lru.len() {
                    break; // 全部被固定：无法继续清理。
                }
                continue;
            }
            pinned_streak = 0;
            if self.snapshots.remove(&candidate).is_some() {
                evicted.push(candidate);
            }
        }
        self.evicted.extend(evicted.iter().cloned());
        evicted
    }

    /// 保留最近 `count` 个 `declare_head` base 快照并固定它们。
    pub fn pin_recent_declare_head_bases(
        &mut self,
        log: &AtomLog,
        count: usize,
    ) -> Vec<SnapshotId> {
        let heads: Vec<&crate::atom::Atom> = log.declare_heads().collect();
        let mut pinned = Vec::new();
        for head in heads.iter().rev().take(count) {
            let matching: Option<SnapshotId> = self
                .snapshots
                .values()
                .filter(|snapshot| snapshot.base_ref == SnapshotBase::DeclareHead(head.id.clone()))
                .max_by_key(|snapshot| snapshot.seq_at)
                .map(|snapshot| snapshot.id.clone());
            if let Some(id) = matching {
                self.pin(id.clone());
                pinned.push(id);
            }
        }
        pinned
    }

    /// 依据策略生成快照并写入（无触发条件时返回 None）。
    pub fn maybe_snapshot(
        &mut self,
        log: &AtomLog,
        last_snapshot_seq: Seq,
        last_snapshot_at_ms: i64,
        now: i64,
        cache: &mut StateAtCache,
    ) -> Result<Option<Snapshot>> {
        let decision = snapshot_decision(log, last_snapshot_seq, last_snapshot_at_ms, now);
        if !decision.needed {
            return Ok(None);
        }
        let base_ref = match log.last_declare_head_upto(log.head_seq()) {
            Some(head) => SnapshotBase::DeclareHead(head.id.clone()),
            None => SnapshotBase::Snapshot("root".to_owned()),
        };
        let snapshot = Snapshot::create(log, log.head_seq(), base_ref, cache)?;
        self.insert(snapshot.clone());
        Ok(Some(snapshot))
    }
}

/// 便捷：当前时刻（毫秒）。
pub fn now() -> i64 {
    now_ms()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atom::{Atom, AtomKind};
    use serde_json::{json, Value};

    fn atom(kind: AtomKind, id: &str, payload: Value) -> Atom {
        Atom::new(kind, "human:1", "session:a", payload).with_id(id)
    }

    fn log_with_edits(edits: usize) -> AtomLog {
        let mut log = AtomLog::new();
        log.append(atom(
            AtomKind::CreateDocument,
            "a_doc",
            json!({"doc_id": "doc_1", "width": 32, "height": 32}),
        ))
        .unwrap();
        log.append(atom(
            AtomKind::CreateLayer,
            "a_layer",
            json!({"layer_id": "layer_1"}),
        ))
        .unwrap();
        log.append(atom(
            AtomKind::CreateObject,
            "a_obj",
            json!({"object_id": "obj_1", "layer_id": "layer_1", "type": "stroke"}),
        ))
        .unwrap();
        for index in 0..edits {
            log.append(atom(
                AtomKind::Supersede,
                &format!("a_super_{index}"),
                json!({"object_id": "obj_1", "data": {"v": index}}),
            ))
            .unwrap();
        }
        log
    }

    #[test]
    fn crc32_matches_known_vector() {
        // 标准测试向量："123456789" → 0xCBF43926
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn snapshot_anchors_state_at_seq_and_verifies_crc() {
        let log = log_with_edits(3);
        let mut cache = StateAtCache::new();
        let snapshot = Snapshot::create(
            &log,
            5,
            SnapshotBase::DeclareHead("root".into()),
            &mut cache,
        )
        .unwrap();
        assert_eq!(snapshot.seq_at, 5);
        assert_eq!(
            snapshot.state.objects["obj_1"].versions.len(),
            3,
            "seq 5 = a_doc + a_layer + a_obj + 2 次 supersede"
        );
        assert!(snapshot.verify());

        let mut tampered = snapshot.clone();
        tampered.seq_at = 4;
        assert!(!tampered.verify(), "篡改后 CRC 必须失败");
    }

    #[test]
    fn snapshot_manifest_is_active_blob_set() {
        let mut log = log_with_edits(0);
        let blob = BlobHash::from_bytes(b"raster");
        log.append(atom(
            AtomKind::CreateObject,
            "a_patch",
            json!({
                "object_id": "obj_patch",
                "layer_id": "layer_1",
                "type": "raster_patch",
                "bitmap": {"blob_hash": blob, "size": 6, "mime_type": "image/webp"},
            }),
        ))
        .unwrap();
        let mut cache = StateAtCache::new();
        let snapshot = Snapshot::create(
            &log,
            log.head_seq(),
            SnapshotBase::DeclareHead("root".into()),
            &mut cache,
        )
        .unwrap();
        assert_eq!(snapshot.manifest, vec![blob]);
        assert!(snapshot.verify());
    }

    #[test]
    fn snapshot_decision_follows_document_thresholds() {
        let log = log_with_edits(0);
        let decision = snapshot_decision(&log, 3, 0, 0);
        assert!(!decision.needed);

        // ≥ 30s 且有新原子。
        let decision = snapshot_decision(&log, 2, 1_000, 1_000 + 31_000);
        assert!(decision.needed);
        assert_eq!(decision.trigger, SnapshotTrigger::IdleWithNewAtoms);

        // 未到阈值：只有 3 个原子且刚拍过快照。
        let decision = snapshot_decision(&log, 3, 1_000, 1_500);
        assert!(!decision.needed, "只有 3 个原子且未到 idle 阈值");

        // 重型原子强制快照。
        let mut log = log;
        log.append(atom(
            AtomKind::Liquify,
            "a_liquify",
            json!({"object_id": "obj_1"}),
        ))
        .unwrap();
        let decision = snapshot_decision(&log, 3, 1_000, 1_500);
        assert!(decision.needed);
        assert_eq!(decision.trigger, SnapshotTrigger::HeavyAtom);
    }

    #[test]
    fn heavy_atom_triggers_and_lag_triggers() {
        let mut log = AtomLog::new();
        for index in 0..1001 {
            log.append(atom(
                AtomKind::CreateLayer,
                &format!("a_layer_{index}"),
                json!({"layer_id": format!("layer_{index}")}),
            ))
            .unwrap();
        }
        let decision = snapshot_decision(&log, 0, 0, 0);
        assert!(decision.needed);
        assert_eq!(decision.trigger, SnapshotTrigger::LaggingAtoms);
        assert_eq!(decision.lag_atoms, 1001);
    }

    #[test]
    fn store_keeps_pinned_snapshots_and_evicts_the_rest() {
        let log = log_with_edits(2);
        let mut cache = StateAtCache::new();
        let mut store = SnapshotStore::new(2);
        let first = Snapshot::create(
            &log,
            3,
            SnapshotBase::DeclareHead("root".into()),
            &mut cache,
        )
        .unwrap();
        store.pin(first.id.clone());
        store.insert(first.clone());
        for seq in 4..=5 {
            let snapshot = Snapshot::create(
                &log,
                seq,
                SnapshotBase::DeclareHead("root".into()),
                &mut cache,
            )
            .unwrap();
            store.insert(snapshot);
        }
        assert_eq!(store.len(), 2, "容量上限生效");
        assert!(store.get(&first.id).is_some(), "被固定的快照不清理");
        assert_eq!(store.evicted().len(), 1);
        assert!(store.latest_usable().is_some());
    }

    #[test]
    fn nearest_usable_skips_corrupt_snapshots() {
        let log = log_with_edits(2);
        let mut cache = StateAtCache::new();
        let mut store = SnapshotStore::new(8);
        let good = Snapshot::create(
            &log,
            3,
            SnapshotBase::DeclareHead("root".into()),
            &mut cache,
        )
        .unwrap();
        let mut corrupt = Snapshot::create(
            &log,
            5,
            SnapshotBase::DeclareHead("root".into()),
            &mut cache,
        )
        .unwrap();
        corrupt.crc = "crc32:deadbeef".to_owned();
        store.insert(good.clone());
        store.insert(corrupt);
        assert_eq!(store.latest_usable().unwrap().id, good.id);
        assert_eq!(store.nearest_usable_at_or_before(5).unwrap().id, good.id);
    }

    #[test]
    fn pin_recent_declare_head_bases_keeps_them() {
        let mut log = log_with_edits(0);
        log.append(atom(
            AtomKind::DeclareHead,
            "a_head1",
            json!({"base": {"type": "atom", "id": "a_layer"}}),
        ))
        .unwrap();
        log.append(atom(
            AtomKind::CreateLayer,
            "a_layer2",
            json!({"layer_id": "layer_2"}),
        ))
        .unwrap();
        log.append(atom(
            AtomKind::DeclareHead,
            "a_head2",
            json!({"base": {"type": "atom", "id": "a_obj"}}),
        ))
        .unwrap();

        let mut cache = StateAtCache::new();
        let mut store = SnapshotStore::new(1);
        let snap1 = Snapshot::create_at_head(&log, 4, "a_head1", &mut cache).unwrap();
        let snap2 = Snapshot::create_at_head(&log, 6, "a_head2", &mut cache).unwrap();
        // 用户显式检查点与 declare_head base 的快照先固定，再写入。
        store.pin(snap1.id.clone());
        store.insert(snap1.clone());
        store.pin(snap2.id.clone());
        store.insert(snap2.clone());
        let pinned = store.pin_recent_declare_head_bases(&log, 2);
        assert_eq!(pinned.len(), 2);
        assert_eq!(store.len(), 2, "被固定的快照在容量不足时仍保留");

        // 未固定的快照仍会被 LRU 清理。
        let mut store = SnapshotStore::new(1);
        store.insert(snap1.clone());
        store.insert(snap2.clone());
        assert_eq!(store.len(), 1);
        assert_eq!(store.evicted().len(), 1);
    }
}
